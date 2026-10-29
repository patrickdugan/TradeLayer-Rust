// Included in source_main_native_session: actual same-owner production bridge.
use crate::source_main_step_kernel::{
    MainStepCommandV1, MainStepError, MainStepEvaluationV1, MainStepKernelV1, MainStepStop,
};
pub const NATIVE_STEP_PROFILE: &str = "TL-MAIN-NATIVE-STEP-CONTROL-13-1";
pub const NATIVE_FILE_STEP_PROFILE: &str = "TL-MAIN-NATIVE-STEP-FILE-RETRY-CONTROL-13-1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeStepProfileV1 {
    Immediate,
    FileRetries,
}
impl NativeStepProfileV1 {
    pub fn identifier(self) -> &'static str {
        match self {
            Self::Immediate => NATIVE_STEP_PROFILE,
            Self::FileRetries => NATIVE_FILE_STEP_PROFILE,
        }
    }
}
include!("source_main_native_step_decode_impl.rs");
include!("source_main_native_step_checkpoint_impl.rs");
const MAX_STEP_RECORDS: usize = 4096;
const MAX_STEP_INPUT_BYTES: usize = 64 * 1024 * 1024;
fn native_step_validate_input(command: &MainStepCommandV1) -> Result<(), SourceRuleError> {
    match command {
        MainStepCommandV1::Start { method, arguments } => {
            s(method).validate()?;
            if arguments.len() > 64 {
                return Err(SourceRuleError::Capacity);
            }
            for value in arguments {
                value.validate()?;
            }
        }
        MainStepCommandV1::Shutdown(arguments) => {
            if arguments.len() > 8 {
                return Err(SourceRuleError::Capacity);
            }
            for value in arguments {
                value.validate()?;
            }
        }
        MainStepCommandV1::UpdateRecordedIo(value) => value.validate()?,
        _ => (),
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepHeadV1 {
    pub commands: u64,
    pub commitment: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepExpectationV1 {
    pub head: NativeStepHeadV1,
    pub physical: NativePhysicalBinding,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCanonicalFieldsV1 {
    pub fields: u64,
    pub bytes: u64,
    pub commitment: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStepReceiptV1 {
    pub sequence: u64,
    pub command: MainStepCommandV1,
    pub evaluation: Result<MainStepEvaluationV1, MainStepError>,
    pub command_fields: NativeCanonicalFieldsV1,
    pub result_fields: NativeCanonicalFieldsV1,
    pub kernel_fields: NativeCanonicalFieldsV1,
    pub native_fields: NativeCanonicalFieldsV1,
    pub financial_commitment: [u8; 32],
    pub validity: Vec<StreamedValidityEvaluation>,
    pub consumed_rows: u64,
    pub before: NativePhysicalBinding,
    pub after: Option<NativePhysicalBinding>,
    pub after_rows: Option<RowStreamReceipt>,
    pub storage_stop: Option<String>,
    pub trailing_effects: Vec<V>,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
    pub result_frames: Vec<u8>,
    pub kernel_frames: Vec<u8>,
    pub native_frames: Vec<u8>,
}
impl SourceMainNativeStepSessionV1 {
    pub fn publish_pending(&mut self) -> Result<&NativeStepReceiptV1, NativeStepSessionError> {
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| NativeStepSessionError::PublicationBlocked("no pending step".into()))?;
        let blocked =
            |error: SourceRuleError| NativeStepSessionError::PublicationBlocked(error.to_string());
        let mut result =
            NativeFieldCollector::new("Native13/StepResult/v1", true, self.publication_limit);
        match &pending.evaluation {
            Ok(evaluation) => {
                result.emit("kind", &s("Evaluation")).map_err(blocked)?;
                evaluation
                    .visit_canonical_fields(&mut |path, value| {
                        result.emit(&format!("evaluation/{path}"), value)
                    })
                    .map_err(blocked)?;
            }
            Err(error) => {
                result
                    .emit("kind", &s("ControllerRefused"))
                    .map_err(blocked)?;
                result
                    .emit("error", &s(&format!("{error:?}")))
                    .map_err(blocked)?;
            }
        }
        let mut kernel =
            NativeFieldCollector::new("Native13/StepKernel/v1", true, self.publication_limit);
        self.kernel
            .visit_canonical_fields(&mut |path, value| kernel.emit(path, value))
            .map_err(blocked)?;
        let financial_commitment = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|error| NativeStepSessionError::PublicationBlocked(error.to_string()))?;
        let after = physical(&self.owner.index).ok();
        let after_rows = complete(&self.owner.index).ok();
        let mut native =
            NativeFieldCollector::new("Native13/StepNative/v1", true, self.publication_limit);
        native_step_native_fields(pending, &after_rows, &mut native).map_err(blocked)?;
        native
            .emit("physicalKnown", &V::Bool(after.is_some()))
            .map_err(blocked)?;
        let mut bytes = self.profile.identifier().as_bytes().to_vec();
        bytes.extend_from_slice(&self.head.commitment);
        bytes.extend_from_slice(&self.head.commands.to_be_bytes());
        for summary in [
            &pending.command_fields,
            &result.summary,
            &kernel.summary,
            &native.summary,
        ] {
            native_step_field_summary(&mut bytes, summary);
        }
        bytes.extend_from_slice(&financial_commitment);
        let head = sha256(&bytes);
        let pending = self.pending.take().expect("retained while publishing");
        self.recovery = pending.storage_stop.is_some() || after.is_none() || after_rows.is_none();
        if let Some(after) = &after {
            self.last_physical = after.clone();
        }
        self.input_bytes += pending.command_frames.len();
        let receipt = NativeStepReceiptV1 {
            sequence: self.head.commands,
            command: pending.command,
            evaluation: pending.evaluation,
            command_fields: pending.command_fields,
            result_fields: result.summary,
            kernel_fields: kernel.summary,
            native_fields: native.summary,
            financial_commitment,
            validity: pending.validity,
            consumed_rows: pending.consumed_rows,
            before: pending.before,
            after,
            after_rows,
            storage_stop: pending.storage_stop,
            trailing_effects: pending.trailing_effects,
            previous_head: self.head.commitment,
            head,
            result_frames: result.frames.take().expect("retained result"),
            kernel_frames: kernel.frames.take().expect("retained kernel"),
            native_frames: native.frames.take().expect("retained native"),
        };
        self.head = NativeStepHeadV1 {
            commands: self.head.commands + 1,
            commitment: head,
        };
        self.records.push(NativeStepRecordV1 {
            command_frames: pending.command_frames,
            receipt,
        });
        Ok(&self.records.last().expect("published step").receipt)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeStepSessionError {
    WrongHead,
    WrongPhysicalBinding,
    Capacity,
    Exited,
    RecoveryRequired,
    PublicationBlocked(String),
    Input(SourceRuleError),
    Backend(String),
}
pub struct RejectedNativeStepSessionV1 {
    pub error: NativeStepSessionError,
    owner: SourceMainNativeSessionV13,
}
impl core::fmt::Debug for RejectedNativeStepSessionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeStepSessionV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeStepSessionV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
struct NativeStepRecordV1 {
    command_frames: Vec<u8>,
    receipt: NativeStepReceiptV1,
}
struct NativeStepPendingV1 {
    command: MainStepCommandV1,
    command_frames: Vec<u8>,
    command_fields: NativeCanonicalFieldsV1,
    evaluation: Result<MainStepEvaluationV1, MainStepError>,
    validity: Vec<StreamedValidityEvaluation>,
    consumed_rows: u64,
    storage_stop: Option<String>,
    trailing_effects: Vec<V>,
    before: NativePhysicalBinding,
}
/// This moves the existing native session; no second financial owner exists.
/// Its old archive/head become foundation data only after actual steps.
pub struct SourceMainNativeStepSessionV1 {
    owner: SourceMainNativeSessionV13,
    profile: NativeStepProfileV1,
    kernel: MainStepKernelV1,
    foundation: Vec<u8>,
    foundation_pin: NativeMainArchivePin,
    initial_generation: u64,
    head: NativeStepHeadV1,
    records: Vec<NativeStepRecordV1>,
    input_bytes: usize,
    last_physical: NativePhysicalBinding,
    exited: bool,
    recovery: bool,
    pending: Option<NativeStepPendingV1>,
    publication_limit: usize,
}
impl core::fmt::Debug for SourceMainNativeStepSessionV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SourceMainNativeStepSessionV1")
            .field("profile", &self.profile)
            .field("head", &self.head)
            .field("exited", &self.exited)
            .field("recovery", &self.recovery)
            .finish_non_exhaustive()
    }
}
struct NativeFieldCollector {
    summary: NativeCanonicalFieldsV1,
    frames: Option<Vec<u8>>,
    limit: usize,
}
impl NativeFieldCollector {
    fn new(domain: &str, retain: bool, limit: usize) -> Self {
        Self {
            summary: NativeCanonicalFieldsV1 {
                fields: 0,
                bytes: 0,
                commitment: sha256(domain.as_bytes()),
            },
            frames: retain.then(Vec::new),
            limit,
        }
    }
    fn emit(&mut self, path: &str, value: &V) -> Result<(), SourceRuleError> {
        if matches!(value, V::Array(_) | V::Object(_)) {
            return Err(SourceRuleError::InvalidInput);
        }
        value.validate()?;
        let body = value.diagnostic_json()?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            &u32::try_from(path.len())
                .map_err(|_| SourceRuleError::Capacity)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(path.as_bytes());
        bytes.extend_from_slice(
            &u32::try_from(body.len())
                .map_err(|_| SourceRuleError::Capacity)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(body.as_bytes());
        let total = self
            .summary
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(SourceRuleError::Capacity)?;
        if let Some(frames) = &mut self.frames {
            if total > self.limit as u64 {
                return Err(SourceRuleError::Capacity);
            }
            frames
                .try_reserve(bytes.len())
                .map_err(|_| SourceRuleError::Capacity)?;
            frames.extend_from_slice(&bytes);
        }
        let mut chain = self.summary.commitment.to_vec();
        chain.extend_from_slice(&self.summary.fields.to_be_bytes());
        chain.extend_from_slice(&bytes);
        self.summary.commitment = sha256(&chain);
        self.summary.fields = self
            .summary
            .fields
            .checked_add(1)
            .ok_or(SourceRuleError::Capacity)?;
        self.summary.bytes = total;
        Ok(())
    }
}
fn native_step_field_summary(bytes: &mut Vec<u8>, summary: &NativeCanonicalFieldsV1) {
    bytes.extend_from_slice(&summary.fields.to_be_bytes());
    bytes.extend_from_slice(&summary.bytes.to_be_bytes());
    bytes.extend_from_slice(&summary.commitment);
}
fn native_step_value_fields(
    path: &str,
    value: &V,
    collector: &mut NativeFieldCollector,
) -> Result<(), SourceRuleError> {
    match value {
        V::Array(values) => {
            collector.emit(&format!("{path}/kind"), &s("Array"))?;
            collector.emit(&format!("{path}/length"), &s(&values.len().to_string()))?;
            for (at, value) in values.iter().enumerate() {
                native_step_value_fields(&format!("{path}/items/{at}"), value, collector)?;
            }
        }
        V::Object(fields) => {
            collector.emit(&format!("{path}/kind"), &s("Object"))?;
            collector.emit(&format!("{path}/length"), &s(&fields.len().to_string()))?;
            for (at, (key, value)) in fields.iter().enumerate() {
                collector.emit(&format!("{path}/fields/{at}/key"), &s(key))?;
                native_step_value_fields(&format!("{path}/fields/{at}/value"), value, collector)?;
            }
        }
        _ => collector.emit(&format!("{path}/atom"), value)?,
    }
    Ok(())
}
fn native_step_native_fields(
    pending: &NativeStepPendingV1,
    after_rows: &Option<RowStreamReceipt>,
    collector: &mut NativeFieldCollector,
) -> Result<(), SourceRuleError> {
    collector.emit("validity/length", &s(&pending.validity.len().to_string()))?;
    for (at, evaluation) in pending.validity.iter().enumerate() {
        let prefix = format!("validity/{at}");
        let query = &evaluation.query;
        for (name, value) in [
            ("profile", s(&evaluation.profile)),
            ("predicate", s(&query.predicate)),
            ("visited", s(&query.visited_rows.to_string())),
            ("matched", s(&query.matched_rows.to_string())),
            ("chain", s(&hex(&query.chain_sha256d))),
            ("complete", V::Bool(query.complete)),
        ] {
            collector.emit(&format!("{prefix}/{name}"), &value)?;
        }
        let update = evaluation.update.as_ref().map_or(V::Null, |observation| {
            obj(&[
                ("target", s(&observation.call.target)),
                ("arguments", V::Array(observation.call.arguments.clone())),
                (
                    "returned",
                    SourceContractCommandOutcome::Dependency(observation.returned.clone()).value(),
                ),
            ])
        });
        native_step_value_fields(&format!("{prefix}/update"), &update, collector)?;
        let outcome = match &evaluation.outcome {
            crate::validity_source::SourceOutcome::Returned(value) => {
                obj(&[("returned", value.clone())])
            }
            crate::validity_source::SourceOutcome::Thrown(error) => {
                obj(&[("name", s(&error.name)), ("message", s(&error.message))])
            }
        };
        native_step_value_fields(&format!("{prefix}/outcome"), &outcome, collector)?;
    }
    collector.emit("consumedRows", &s(&pending.consumed_rows.to_string()))?;
    collector.emit(
        "trailingEffects/length",
        &s(&pending.trailing_effects.len().to_string()),
    )?;
    for (at, effect) in pending.trailing_effects.iter().enumerate() {
        native_step_value_fields(&format!("trailingEffects/{at}"), effect, collector)?;
    }
    native_step_value_fields(
        "afterRows",
        &after_rows.as_ref().map_or(V::Null, logical_read),
        collector,
    )?;
    native_step_value_fields(
        "storageStop",
        &pending
            .storage_stop
            .as_ref()
            .map_or(V::Null, |value| s(value)),
        collector,
    )
}
impl SourceMainNativeStepSessionV1 {
    pub fn adopt(
        owner: SourceMainNativeSessionV13,
        expected: &NativeMainSessionExpectation,
        generation: u64,
    ) -> Result<Self, Box<RejectedNativeStepSessionV1>> {
        Self::adopt_profile(owner, expected, generation, NativeStepProfileV1::Immediate)
    }
    pub fn adopt_with_file_retries(
        owner: SourceMainNativeSessionV13,
        expected: &NativeMainSessionExpectation,
        generation: u64,
    ) -> Result<Self, Box<RejectedNativeStepSessionV1>> {
        Self::adopt_profile(
            owner,
            expected,
            generation,
            NativeStepProfileV1::FileRetries,
        )
    }
    fn adopt_profile(
        owner: SourceMainNativeSessionV13,
        expected: &NativeMainSessionExpectation,
        generation: u64,
        profile: NativeStepProfileV1,
    ) -> Result<Self, Box<RejectedNativeStepSessionV1>> {
        let prepared = (|| {
            if owner.halted {
                return Err(NativeStepSessionError::RecoveryRequired);
            }
            let actual = owner
                .expectation()
                .map_err(|error| NativeStepSessionError::Backend(format!("{error:?}")))?;
            if actual.session != expected.session {
                return Err(NativeStepSessionError::WrongHead);
            }
            if actual.physical != expected.physical {
                return Err(NativeStepSessionError::WrongPhysicalBinding);
            }
            let (foundation, foundation_pin) = owner
                .checkpoint()
                .map_err(|error| NativeStepSessionError::Backend(format!("{error:?}")))?;
            let kernel = match profile {
                NativeStepProfileV1::Immediate => MainStepKernelV1::new(generation),
                NativeStepProfileV1::FileRetries => MainStepKernelV1::with_file_retries(generation),
            };
            let mut fields = NativeFieldCollector::new("Native13/StepKernel/v1", false, 0);
            kernel
                .visit_canonical_fields(&mut |path, value| fields.emit(path, value))
                .map_err(NativeStepSessionError::Input)?;
            let mut seed = profile.identifier().as_bytes().to_vec();
            seed.extend_from_slice(&foundation_pin.head.commands.to_be_bytes());
            seed.extend_from_slice(&foundation_pin.head.commitment);
            seed.extend_from_slice(
                &owner
                    .financial
                    .native_main_financial_commitment()
                    .map_err(|error| NativeStepSessionError::Backend(error.to_string()))?,
            );
            native_step_field_summary(&mut seed, &fields.summary);
            Ok((
                foundation,
                foundation_pin,
                kernel,
                sha256(&seed),
                actual.physical,
            ))
        })();
        let (foundation, foundation_pin, kernel, commitment, last_physical) = match prepared {
            Ok(value) => value,
            Err(error) => return Err(Box::new(RejectedNativeStepSessionV1 { error, owner })),
        };
        Ok(Self {
            owner,
            profile,
            kernel,
            foundation,
            foundation_pin,
            initial_generation: generation,
            head: NativeStepHeadV1 {
                commands: 0,
                commitment,
            },
            records: vec![],
            input_bytes: 0,
            last_physical,
            exited: false,
            recovery: false,
            pending: None,
            publication_limit: 64 * 1024 * 1024,
        })
    }
    pub fn head(&self) -> Result<NativeStepHeadV1, NativeStepSessionError> {
        if self.pending.is_some() {
            Err(NativeStepSessionError::PublicationBlocked(
                "actual accepted step awaits complete publication".into(),
            ))
        } else {
            Ok(self.head.clone())
        }
    }
    pub fn expectation(&self) -> Result<NativeStepExpectationV1, NativeStepSessionError> {
        Ok(NativeStepExpectationV1 {
            head: self.head()?,
            physical: physical(&self.owner.index).unwrap_or_else(|_| self.last_physical.clone()),
        })
    }
    pub fn foundation_checkpoint(&self) -> &[u8] {
        &self.foundation
    }
    pub fn foundation_pin(&self) -> &NativeMainArchivePin {
        &self.foundation_pin
    }
    pub fn kernel(&self) -> &MainStepKernelV1 {
        &self.kernel
    }
    pub fn profile(&self) -> NativeStepProfileV1 {
        self.profile
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.owner.index
    }
    pub fn requires_recovery(&self) -> bool {
        self.recovery
    }
    pub fn require_authenticated_admission(&self) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
    pub fn receipts(&self) -> impl Iterator<Item = &NativeStepReceiptV1> {
        self.records.iter().map(|record| &record.receipt)
    }
    pub fn initial_generation(&self) -> u64 {
        self.initial_generation
    }
    fn native_history_release_proof(&mut self) {
        self.records.clear();
        self.input_bytes = 0;
    }
    fn native_history_is_exited(&self) -> bool {
        self.exited
    }
    fn native_history_release_prefix_proof(
        &mut self,
        count: usize,
    ) -> Result<(), NativeStepSessionError> {
        if count > self.records.len() {
            return Err(NativeStepSessionError::Capacity);
        }
        let released = self
            .records
            .iter()
            .take(count)
            .try_fold(0usize, |sum, record| {
                sum.checked_add(record.command_frames.len())
                    .ok_or(NativeStepSessionError::Capacity)
            })?;
        let remaining = self
            .input_bytes
            .checked_sub(released)
            .ok_or(NativeStepSessionError::Capacity)?;
        self.records.drain(..count);
        self.input_bytes = remaining;
        Ok(())
    }
    pub fn command_field_frames(&self, sequence: usize) -> Option<&[u8]> {
        let first = usize::try_from(self.records.first()?.receipt.sequence).ok()?;
        self.records
            .get(sequence.checked_sub(first)?)
            .map(|record| record.command_frames.as_slice())
    }
    pub fn raise_publication_limit(&mut self, bytes: usize) -> Result<(), NativeStepSessionError> {
        if bytes < self.publication_limit || bytes > 256 * 1024 * 1024 {
            return Err(NativeStepSessionError::Capacity);
        }
        self.publication_limit = bytes;
        Ok(())
    }
    pub fn apply_step(
        &mut self,
        expected: &NativeStepExpectationV1,
        command: MainStepCommandV1,
    ) -> Result<&NativeStepReceiptV1, NativeStepSessionError> {
        if self.exited {
            return Err(NativeStepSessionError::Exited);
        }
        if self.pending.is_some() {
            return Err(NativeStepSessionError::PublicationBlocked(
                "publish retained prefix first".into(),
            ));
        }
        if expected.head != self.head {
            return Err(NativeStepSessionError::WrongHead);
        }
        let shutdown = matches!(&command, MainStepCommandV1::Shutdown(_));
        if self.recovery && !shutdown {
            return Err(NativeStepSessionError::RecoveryRequired);
        }
        let before = match physical(&self.owner.index) {
            Ok(binding) => binding,
            Err(_) if shutdown => self.last_physical.clone(),
            Err(_) => return Err(NativeStepSessionError::RecoveryRequired),
        };
        if before != expected.physical {
            return Err(NativeStepSessionError::WrongPhysicalBinding);
        }
        if self.records.len() >= MAX_STEP_RECORDS && !shutdown {
            return Err(NativeStepSessionError::Capacity);
        }
        self.head
            .commands
            .checked_add(1)
            .ok_or(NativeStepSessionError::Capacity)?;
        native_step_validate_input(&command).map_err(NativeStepSessionError::Input)?;
        let mut fields =
            NativeFieldCollector::new("Native13/StepCommand/v1", true, MAX_STEP_INPUT_BYTES);
        command
            .visit_canonical_fields(&mut |path, value| fields.emit(path, value))
            .map_err(NativeStepSessionError::Input)?;
        let command_frames = fields.frames.take().expect("retained command fields");
        if self.input_bytes.saturating_add(command_frames.len()) > MAX_STEP_INPUT_BYTES && !shutdown
        {
            return Err(NativeStepSessionError::Capacity);
        }
        self.records
            .try_reserve(1)
            .map_err(|_| NativeStepSessionError::Capacity)?;
        // Real production same-owner bridge; no fake backend or Replay escape.
        let actual = self
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_step_apply(&mut self.owner.index, &mut self.kernel, command.clone());
        let trailing_effects = self
            .owner
            .financial
            .native_main_runtime_mut()
            .take_effects();
        self.exited = matches!(&actual.evaluation,Ok(evaluation)if matches!(evaluation.stop,MainStepStop::ProcessExited(_)));
        self.pending = Some(NativeStepPendingV1 {
            command,
            command_frames,
            command_fields: fields.summary,
            evaluation: actual.evaluation,
            validity: actual.validity,
            consumed_rows: actual.consumed_rows,
            storage_stop: actual.storage_stop,
            trailing_effects,
            before,
        });
        self.publish_pending()
    }
}
