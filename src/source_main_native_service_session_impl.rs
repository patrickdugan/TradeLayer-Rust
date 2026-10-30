// A running same-owner component, separate from Completed-row publication.
// There is deliberately no archive importer, recovery constructor, owner
// release, legacy checkpoint, or mutable financial/index accessor here.
use crate::source_contract_runtime::{NativeMainServiceContinuationV1, NativeMainServiceStopV1};
pub const NATIVE_ACTIVE_SERVICE_SESSION_PROFILE: &str =
    "TL-MAIN-NATIVE-ACTIVE-SERVICE-SESSION-13-1";
const MAX_SERVICE_ACTIONS: usize = 4096;
const MAX_SERVICE_OBSERVATION_BYTES: usize = 256 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainActiveServiceHeadV1 {
    pub actions: u64,
    pub commitment: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainActiveServiceExpectationV1 {
    pub head: NativeMainActiveServiceHeadV1,
    pub physical: NativePhysicalBinding,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainActiveServiceActionV1 {
    Start {
        start: V,
        generation: u64,
        documents: V,
    },
    Advance {
        limit: usize,
    },
    Pause,
    AppendPending,
    ResolvePost,
    FireTimer {
        id: u64,
        generation: u64,
    },
    ClearTimer {
        id: u64,
        generation: u64,
    },
    Shutdown(Vec<V>),
}
impl NativeMainActiveServiceActionV1 {
    fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        use crate::source_contract_runtime::native_main_row_visit_value as visit;
        let kind = match self {
            Self::Start { .. } => "Start",
            Self::Advance { .. } => "Advance",
            Self::Pause => "Pause",
            Self::AppendPending => "AppendPending",
            Self::ResolvePost => "ResolvePost",
            Self::FireTimer { .. } => "FireTimer",
            Self::ClearTimer { .. } => "ClearTimer",
            Self::Shutdown(_) => "Shutdown",
        };
        visitor("kind", &s(kind))?;
        match self {
            Self::Start {
                start,
                generation,
                documents,
            } => {
                visit("start", start, visitor)?;
                visitor("generation", &s(&generation.to_string()))?;
                visit("documents", documents, visitor)?;
            }
            Self::Advance { limit } => visitor("limit", &s(&limit.to_string()))?,
            Self::FireTimer { id, generation } | Self::ClearTimer { id, generation } => {
                visitor("id", &s(&id.to_string()))?;
                visitor("generation", &s(&generation.to_string()))?;
            }
            Self::Shutdown(arguments) => visit("arguments", &V::Array(arguments.clone()), visitor)?,
            _ => (),
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainActiveServiceActionResultV1 {
    Started(Result<(), SourceRuleError>),
    Stop(NativeMainServiceStopV1),
    Pause(Result<DependencyReturn, SourceRuleError>),
    Control(Result<(), MainStepError>),
}
impl NativeMainActiveServiceActionResultV1 {
    fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        let (kind, diagnostic) = match self {
            Self::Started(result) => ("Started", format!("{result:?}")),
            Self::Stop(stop) => ("Stop", format!("{stop:?}")),
            Self::Control(result) => ("Control", format!("{result:?}")),
            Self::Pause(result) => {
                visitor("kind", &s("Pause"))?;
                return match result {
                    Ok(returned) => crate::source_contract_runtime::native_main_row_visit_value(
                        "returned",
                        &SourceContractCommandOutcome::Dependency(returned.clone()).value(),
                        visitor,
                    ),
                    Err(error) => visitor("localError", &s(&format!("{error:?}"))),
                };
            }
        };
        visitor("kind", &s(kind))?;
        visitor("value", &s(&diagnostic))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainActiveServiceReceiptV1 {
    pub sequence: u64,
    pub action: NativeMainActiveServiceActionV1,
    pub result: NativeMainActiveServiceActionResultV1,
    pub fields: NativeCanonicalFieldsV1,
    pub financial_commitment: [u8; 32],
    pub rows: RowStreamReceipt,
    pub before: NativePhysicalBinding,
    pub after: NativePhysicalBinding,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainActiveServiceErrorV1 {
    WrongExpectation,
    AlreadyExited(u8),
    InvalidInput,
    Capacity,
    ObservationPending,
    NoObservationPending,
    NoFrame,
    WriteAlreadyAttempted,
    NoAttemptedWrite,
    Backend(String),
}
pub struct RejectedNativeMainActiveServiceV1 {
    pub error: NativeMainActiveServiceErrorV1,
    owner: SourceMainNativeSessionV13,
}
impl RejectedNativeMainActiveServiceV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActiveServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActiveServiceV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
struct NativeMainActiveServicePendingV1 {
    action: NativeMainActiveServiceActionV1,
    result: NativeMainActiveServiceActionResultV1,
    before: NativePhysicalBinding,
}
pub struct SourceMainNativeActiveServiceV1 {
    owner: SourceMainNativeSessionV13,
    frame: Option<Box<NativeMainServiceContinuationV1>>,
    source_start_error: Option<SourceRuleError>,
    origin: NativeMainSessionExpectation,
    head: NativeMainActiveServiceHeadV1,
    history: Vec<NativeMainActiveServiceReceiptV1>,
    pending_observation: Option<NativeMainActiveServicePendingV1>,
    attempted_write: Option<V>,
    observation_limit: usize,
    inline_profile: bool,
}
impl core::fmt::Debug for SourceMainNativeActiveServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SourceMainNativeActiveServiceV1")
            .field("profile", &NATIVE_ACTIVE_SERVICE_SESSION_PROFILE)
            .field("head", &self.head)
            .field("pending_observation", &self.pending_observation.is_some())
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeSessionV13 {
    pub fn into_active_service(
        self,
        expected: &NativeMainSessionExpectation,
        start: V,
        generation: u64,
        documents: V,
        observation_limit: usize,
    ) -> Result<SourceMainNativeActiveServiceV1, Box<RejectedNativeMainActiveServiceV1>> {
        self.into_active_service_internal(
            expected,
            start,
            generation,
            documents,
            (observation_limit, false),
        )
    }
    fn into_active_service_internal(
        self,
        expected: &NativeMainSessionExpectation,
        start: V,
        generation: u64,
        documents: V,
        entry: (usize, bool),
    ) -> Result<SourceMainNativeActiveServiceV1, Box<RejectedNativeMainActiveServiceV1>> {
        self.into_active_service_profile(
            expected,
            start,
            generation,
            documents,
            (entry.0, entry.1, false),
        )
    }
    fn into_active_service_profile(
        self,
        expected: &NativeMainSessionExpectation,
        start: V,
        generation: u64,
        documents: V,
        entry: (usize, bool, bool),
    ) -> Result<SourceMainNativeActiveServiceV1, Box<RejectedNativeMainActiveServiceV1>> {
        self.into_active_service_profile_deferred(
            expected, start, generation, documents, entry, false,
        )
    }

    fn into_active_service_fact_entry(
        self,
        expected: &NativeMainSessionExpectation,
        start: V,
        generation: u64,
        documents: V,
        entry: (usize, bool),
        defer_observation: bool,
    ) -> Result<SourceMainNativeActiveServiceV1, Box<RejectedNativeMainActiveServiceV1>> {
        self.into_active_service_profile_deferred(
            expected,
            start,
            generation,
            documents,
            (entry.0, entry.1, false),
            defer_observation,
        )
    }
    fn into_active_service_profile_deferred(
        mut self,
        expected: &NativeMainSessionExpectation,
        start: V,
        generation: u64,
        documents: V,
        entry: (usize, bool, bool),
        defer_observation: bool,
    ) -> Result<SourceMainNativeActiveServiceV1, Box<RejectedNativeMainActiveServiceV1>> {
        let (observation_limit, inline_profile, automatic_files) = entry;
        let checked = (|| {
            if self.halted
                || self
                    .expectation()
                    .map_err(|e| NativeMainActiveServiceErrorV1::Backend(format!("{e:?}")))?
                    != *expected
            {
                return Err(NativeMainActiveServiceErrorV1::WrongExpectation);
            }
            if observation_limit == 0 || observation_limit > MAX_SERVICE_OBSERVATION_BYTES {
                return Err(NativeMainActiveServiceErrorV1::InvalidInput);
            }
            start
                .validate()
                .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
            (if automatic_files {
                self.financial
                    .runtime()
                    .native_main_automatic_file_preflight(&documents)
            } else {
                self.financial
                    .runtime()
                    .native_main_service_preflight(&documents)
            })
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
            let mut history = Vec::new();
            history
                .try_reserve(1)
                .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
            Ok(history)
        })();
        let history = match checked {
            Ok(history) => history,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainActiveServiceV1 {
                    error,
                    owner: self,
                }))
            }
        };
        let mut origin_bytes = (if automatic_files {
            crate::source_contract_runtime::NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE
        } else if inline_profile {
            crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE
        } else {
            NATIVE_ACTIVE_SERVICE_SESSION_PROFILE
        })
        .as_bytes()
        .to_vec();
        origin_bytes.extend_from_slice(&expected.session.commands.to_be_bytes());
        origin_bytes.extend_from_slice(&expected.session.commitment);
        let action = NativeMainActiveServiceActionV1::Start {
            start: start.clone(),
            generation,
            documents: documents.clone(),
        };
        let runtime = self.financial.native_main_runtime_mut();
        let begun = if automatic_files {
            runtime.native_main_automatic_file_begin(&mut self.index, start, generation, documents)
        } else {
            runtime.native_main_service_begin(&mut self.index, start, generation, documents)
        };
        let (frame, source_start_error, result) = match begun {
            Ok(mut frame) => {
                if inline_profile {
                    frame.enable_inline_profile();
                }
                (
                    Some(Box::new(frame)),
                    None,
                    NativeMainActiveServiceActionResultV1::Started(Ok(())),
                )
            }
            Err(error) => (
                None,
                Some(error.clone()),
                NativeMainActiveServiceActionResultV1::Started(Err(error)),
            ),
        };
        let mut service = SourceMainNativeActiveServiceV1 {
            owner: self,
            frame,
            source_start_error,
            origin: expected.clone(),
            head: NativeMainActiveServiceHeadV1 {
                actions: 0,
                commitment: sha256(&origin_bytes),
            },
            history,
            pending_observation: Some(NativeMainActiveServicePendingV1 {
                action,
                result,
                before: expected.physical.clone(),
            }),
            attempted_write: None,
            observation_limit,
            inline_profile,
        };
        // A post-execution observation refusal retains the actual changed
        // owner/result. Only observation may retry, never this source entry.
        if !defer_observation {
            let _ = service.retry_observation();
        }
        Ok(service)
    }
}
impl SourceMainNativeActiveServiceV1 {
    pub fn profile(&self) -> &'static str {
        if self
            .frame
            .as_ref()
            .is_some_and(|f| f.automatic_file_profile())
        {
            return crate::source_contract_runtime::NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE;
        }
        if self.inline_profile {
            crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE
        } else {
            NATIVE_ACTIVE_SERVICE_SESSION_PROFILE
        }
    }
    pub fn head(&self) -> NativeMainActiveServiceHeadV1 {
        self.head.clone()
    }
    pub fn history(&self) -> &[NativeMainActiveServiceReceiptV1] {
        &self.history
    }
    pub fn observation_pending(&self) -> bool {
        self.pending_observation.is_some()
    }
    pub fn source_start_error(&self) -> Option<&SourceRuleError> {
        self.source_start_error.as_ref()
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.owner.index
    }
    pub fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    pub fn bootstrap_effects(&self) -> &[V] {
        self.frame
            .as_ref()
            .map_or(&[], |frame| &self.effects()[frame.bootstrap_range()])
    }
    pub fn invocation_effects(&self) -> &[V] {
        self.frame.as_ref().map_or(&[], |frame| {
            &self.effects()[frame.invocation_effect_start()..]
        })
    }
    pub fn caller_result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        self.frame.as_ref()?.caller_result()
    }
    pub fn pending_native_context(&self) -> Option<V> {
        self.frame.as_ref()?.pending_native_context()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedValidityWriteV1> {
        self.frame.as_ref()?.pending_write()
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.frame.as_ref()?.next_timer()
    }
    pub fn kernel_value(&self) -> Option<V> {
        self.frame.as_ref().map(|frame| frame.kernel_value())
    }
    pub fn expectation(
        &self,
    ) -> Result<NativeMainActiveServiceExpectationV1, NativeMainActiveServiceErrorV1> {
        if self.observation_pending() {
            return Err(NativeMainActiveServiceErrorV1::ObservationPending);
        }
        Ok(NativeMainActiveServiceExpectationV1 {
            head: self.head(),
            physical: physical(&self.owner.index)
                .map_err(|e| NativeMainActiveServiceErrorV1::Backend(format!("{e:?}")))?,
        })
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        use crate::source_contract_runtime::native_main_row_visit_value as visit;
        visitor("profile", &s(self.profile()))?;
        visitor(
            "activeClientScope",
            &self
                .owner
                .financial
                .runtime()
                .native_main_service_client_scope_value(),
        )?;
        visitor("history/count", &s(&self.history.len().to_string()))?;
        visitor("previousHead/actions", &s(&self.head.actions.to_string()))?;
        visitor("previousHead/commitment", &s(&hex(&self.head.commitment)))?;
        visitor(
            "origin/commands",
            &s(&self.origin.session.commands.to_string()),
        )?;
        visitor("origin/head", &s(&hex(&self.origin.session.commitment)))?;
        visit(
            "origin/physical",
            &physical_value(&self.origin.physical),
            visitor,
        )?;
        visitor("frame/present", &V::Bool(self.frame.is_some()))?;
        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("frame/{path}"), value)
            })?;
        }
        visitor(
            "sourceStartError",
            &self
                .source_start_error
                .as_ref()
                .map_or(V::Null, |e| s(&format!("{e:?}"))),
        )?;
        visitor(
            "attemptedWrite/present",
            &V::Bool(self.attempted_write.is_some()),
        )?;
        if let Some(write) = &self.attempted_write {
            visit("attemptedWrite", write, visitor)?;
        }
        visitor(
            "pendingObservation/present",
            &V::Bool(self.pending_observation.is_some()),
        )?;
        if let Some(pending) = &self.pending_observation {
            pending.action.visit_fields(&mut |path, value| {
                visitor(&format!("pendingObservation/action/{path}"), value)
            })?;
            pending.result.visit_fields(&mut |path, value| {
                visitor(&format!("pendingObservation/result/{path}"), value)
            })?;
            visit(
                "pendingObservation/before",
                &physical_value(&pending.before),
                visitor,
            )?;
        }
        visitor("observationLimit", &s(&self.observation_limit.to_string()))?;
        visitor("effects/count", &s(&self.effects().len().to_string()))?;
        for (at, effect) in self.effects().iter().enumerate() {
            visit(&format!("effects/{at}"), effect, visitor)?;
        }
        Ok(())
    }
    pub fn retry_observation(
        &mut self,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        if self.pending_observation.is_none() {
            return Err(NativeMainActiveServiceErrorV1::NoObservationPending);
        }
        let financial = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        let rows = complete(&self.owner.index)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(format!("{e:?}")))?;
        let after = physical(&self.owner.index)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(format!("{e:?}")))?;
        let mut fields = NativeFieldCollector::new(
            "Native13/ActiveServiceObservation/v1",
            true,
            self.observation_limit,
        );
        self.visit_canonical_fields(&mut |path, value| fields.emit(path, value))
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        fields
            .emit("financial", &s(&hex(&financial)))
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        native_step_value_fields("afterRows", &logical_read(&rows), &mut fields)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        native_step_value_fields("afterPhysical", &physical_value(&after), &mut fields)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        let sequence = self
            .head
            .actions
            .checked_add(1)
            .ok_or(NativeMainActiveServiceErrorV1::Capacity)?;
        let mut bytes = NATIVE_ACTIVE_SERVICE_SESSION_PROFILE.as_bytes().to_vec();
        bytes.extend_from_slice(&self.head.commitment);
        bytes.extend_from_slice(&sequence.to_be_bytes());
        native_step_field_summary(&mut bytes, &fields.summary);
        let head = sha256(&bytes);
        let pending = self
            .pending_observation
            .take()
            .expect("checked actual pending observation");
        self.history.push(NativeMainActiveServiceReceiptV1 {
            sequence,
            action: pending.action,
            result: pending.result,
            fields: fields.summary,
            financial_commitment: financial,
            rows,
            before: pending.before,
            after,
            previous_head: self.head.commitment,
            head,
        });
        self.head = NativeMainActiveServiceHeadV1 {
            actions: sequence,
            commitment: head,
        };
        Ok(self.history.last().expect("published observation"))
    }
    pub fn apply(
        &mut self,
        expected: &NativeMainActiveServiceExpectationV1,
        action: NativeMainActiveServiceActionV1,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        self.stage_action(expected, action)?;
        self.retry_observation()
    }
    fn stage_action(
        &mut self,
        expected: &NativeMainActiveServiceExpectationV1,
        action: NativeMainActiveServiceActionV1,
    ) -> Result<(), NativeMainActiveServiceErrorV1> {
        if self.expectation()? != *expected {
            return Err(NativeMainActiveServiceErrorV1::WrongExpectation);
        }
        if self.history.len() >= MAX_SERVICE_ACTIONS {
            return Err(NativeMainActiveServiceErrorV1::Capacity);
        }
        self.history
            .try_reserve(1)
            .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
        let frame = self
            .frame
            .as_ref()
            .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
        if let Some(code) = frame.terminal_code() {
            return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code));
        }
        match &action {
            NativeMainActiveServiceActionV1::Start { .. } => {
                return Err(NativeMainActiveServiceErrorV1::InvalidInput)
            }
            NativeMainActiveServiceActionV1::Advance { limit } if *limit == 0 || *limit > 4096 => {
                return Err(NativeMainActiveServiceErrorV1::InvalidInput)
            }
            NativeMainActiveServiceActionV1::Shutdown(args) => {
                if args.len() > 8 || args.iter().any(|v| v.validate().is_err()) {
                    return Err(NativeMainActiveServiceErrorV1::InvalidInput);
                }
            }
            NativeMainActiveServiceActionV1::AppendPending => {
                let context = frame
                    .pending_native_context()
                    .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                if self.attempted_write.as_ref() == Some(&context) {
                    return Err(NativeMainActiveServiceErrorV1::WriteAlreadyAttempted);
                }
            }
            NativeMainActiveServiceActionV1::ResolvePost => {
                let context = frame
                    .pending_native_context()
                    .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                if self.attempted_write.as_ref() != Some(&context) {
                    return Err(NativeMainActiveServiceErrorV1::NoAttemptedWrite);
                }
            }
            _ => (),
        }
        let frame = self.frame.as_mut().expect("checked same frame");
        let runtime = self.owner.financial.native_main_runtime_mut();
        let result = match &action {
            NativeMainActiveServiceActionV1::Advance { limit } => {
                NativeMainActiveServiceActionResultV1::Stop(runtime.native_main_service_advance(
                    &mut self.owner.index,
                    frame,
                    *limit,
                ))
            }
            NativeMainActiveServiceActionV1::Pause => NativeMainActiveServiceActionResultV1::Pause(
                runtime.native_main_service_pause(frame),
            ),
            NativeMainActiveServiceActionV1::AppendPending => {
                self.attempted_write = Some(
                    frame
                        .pending_native_context()
                        .expect("checked pending write"),
                );
                NativeMainActiveServiceActionResultV1::Stop(
                    runtime.native_main_service_append(&mut self.owner.index, frame),
                )
            }
            NativeMainActiveServiceActionV1::ResolvePost => {
                NativeMainActiveServiceActionResultV1::Stop(
                    runtime.native_main_service_resolve_post(&mut self.owner.index, frame),
                )
            }
            NativeMainActiveServiceActionV1::FireTimer { id, generation } => {
                NativeMainActiveServiceActionResultV1::Control(runtime.native_main_service_control(
                    &mut self.owner.index,
                    frame,
                    MainStepCommandV1::FireTimer {
                        id: *id,
                        generation: *generation,
                    },
                ))
            }
            NativeMainActiveServiceActionV1::ClearTimer { id, generation } => {
                NativeMainActiveServiceActionResultV1::Control(runtime.native_main_service_control(
                    &mut self.owner.index,
                    frame,
                    MainStepCommandV1::ClearTimer {
                        id: *id,
                        generation: *generation,
                    },
                ))
            }
            NativeMainActiveServiceActionV1::Shutdown(args) => {
                NativeMainActiveServiceActionResultV1::Control(runtime.native_main_service_control(
                    &mut self.owner.index,
                    frame,
                    MainStepCommandV1::Shutdown(args.clone()),
                ))
            }
            NativeMainActiveServiceActionV1::Start { .. } => {
                unreachable!("Start is constructor only")
            }
        };
        if frame.pending_native_context().as_ref() != self.attempted_write.as_ref() {
            self.attempted_write = None;
        }
        self.pending_observation = Some(NativeMainActiveServicePendingV1 {
            action,
            result,
            before: expected.physical.clone(),
        });
        Ok(())
    }
}
