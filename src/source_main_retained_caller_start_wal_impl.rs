// Bounded durable Start seam. This is not a completed Main command or a reader
// that imports a financial owner. Later caller actions are not yet admitted.
pub const RETAINED_CALLER_START_WAL_PROFILE: &str = "TL-MAIN-RETAINED-CALLER-START-WAL-13-1";
type CallerStartWal = crate::source_main_active_service_wal::ActiveServiceWalV1;
type CallerStartPlan = crate::source_main_active_service_wal::PreparedActiveServiceWalWriteV1;
type CallerStartBlob = crate::source_main_active_service_wal::ActiveServiceWalBlobV1;
type CallerStartPin = crate::source_main_active_service_wal::ActiveServiceWalPinV1;
#[derive(Debug)]
pub enum RetainedCallerStartWalErrorV1 {
    Wal(crate::source_main_active_service_wal::ActiveServiceWalErrorV1),
    Action(crate::source_main_retained_caller_action::RetainedMainCallerActionErrorV1),
    Backend(String),
    InvalidInput,
    NeedWalRecovery,
    AlreadyExecuted,
    Capacity,
}
impl From<crate::source_main_active_service_wal::ActiveServiceWalErrorV1>
    for RetainedCallerStartWalErrorV1
{
    fn from(e: crate::source_main_active_service_wal::ActiveServiceWalErrorV1) -> Self {
        Self::Wal(e)
    }
}
impl From<crate::source_main_retained_caller_action::RetainedMainCallerActionErrorV1>
    for RetainedCallerStartWalErrorV1
{
    fn from(e: crate::source_main_retained_caller_action::RetainedMainCallerActionErrorV1) -> Self {
        Self::Action(e)
    }
}
type CallerStartResult<T> = std::result::Result<T, RetainedCallerStartWalErrorV1>;
fn caller_start_backend(e: impl core::fmt::Debug) -> RetainedCallerStartWalErrorV1 {
    RetainedCallerStartWalErrorV1::Backend(format!("{e:?}"))
}
fn caller_start_bytes(v: &V) -> CallerStartResult<Vec<u8>> {
    v.diagnostic_json()
        .map(String::into_bytes)
        .map_err(caller_start_backend)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedCallerStartWalPhaseV1 {
    Intent,
    Execute,
    Observe,
    Accepted,
    Ack,
    StartAcknowledged,
}
enum RetainedCallerStartOwnerV1 {
    Ready(Box<SourceMainNativeSessionV13>),
    Caller(Box<SourceMainNativeRetainedCallerV1>),
}
pub struct RejectedRetainedCallerStartWalV1 {
    pub error: RetainedCallerStartWalErrorV1,
    owner: SourceMainNativeSessionV13,
}
impl RejectedRetainedCallerStartWalV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedRetainedCallerStartWalV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedRetainedCallerStartWalV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
pub struct SourceMainNativeDurableCallerStartV1 {
    owner: Option<RetainedCallerStartOwnerV1>,
    wal: CallerStartWal,
    origin: V,
    action: crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
    raw: Vec<u8>,
    archive: Vec<u8>,
    before: Vec<u8>,
    post: Option<Vec<u8>>,
    route: Option<NativeMainRetainedCallerRouteV1>,
    source_error: Option<String>,
    executed: bool,
    phase: RetainedCallerStartWalPhaseV1,
    plan: Option<CallerStartPlan>,
    observation_limit: usize,
    profile: &'static str,
    selected_entry: Option<Vec<u8>>,
    #[cfg(test)]
    barrier_fault: Option<(
        RetainedCallerStartWalPhaseV1,
        crate::source_main_active_service_wal::TestFault,
    )>,
}
impl SourceMainNativeDurableCallerStartV1 {
    pub fn create(
        owner: SourceMainNativeSessionV13,
        expected: &NativeMainSessionExpectation,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        action: crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedRetainedCallerStartWalV1>> {
        Self::create_profile(
            owner,
            expected,
            root,
            namespace,
            filesystem,
            action,
            (observation_limit, RETAINED_CALLER_START_WAL_PROFILE, None),
        )
    }
    fn create_profile(
        owner: SourceMainNativeSessionV13,
        expected: &NativeMainSessionExpectation,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        action: crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
        entry: (usize, &'static str, Option<V>),
    ) -> Result<Self, Box<RejectedRetainedCallerStartWalV1>> {
        let (observation_limit, profile, configuration) = entry;

        let checked = (|| -> CallerStartResult<_> {
            use crate::source_main_retained_caller_action::RetainedMainCallerActionV1 as Action;
            let actual_entry = owner.expectation().map_err(caller_start_backend)?;
            if owner.halted
                || actual_entry != *expected
                || observation_limit == 0
                || observation_limit > 64 * 1024 * 1024
            {
                return Err(RetainedCallerStartWalErrorV1::InvalidInput);
            }
            let Action::Start {
                caller,
                scope,
                generation: _,
                documents,
            } = &action
            else {
                return Err(RetainedCallerStartWalErrorV1::InvalidInput);
            };
            if *scope == 0 || *scope > u64::MAX - 4096 {
                return Err(RetainedCallerStartWalErrorV1::InvalidInput);
            }
            let command = Self::command(caller);
            owner
                .financial
                .runtime()
                .native_main_retained_caller_preflight(&command, documents)
                .map_err(caller_start_backend)?;
            let raw = action.canonical_action_bytes()?;
            if crate::source_main_retained_caller_action::RetainedMainCallerActionV1::from_canonical_action_bytes(&raw)?!=action{return Err(RetainedCallerStartWalErrorV1::InvalidInput)}
            let (archive, _) = owner.checkpoint().map_err(caller_start_backend)?;
            let before = Self::ready_observation(&owner, observation_limit)?;
            let mut origin = obj(&[
                ("profile", s(profile)),
                (
                    "requiredCallerSource",
                    s("TL-MAIN-RETAINED-CALLER-SOURCE-13-1"),
                ),
                (
                    "requiredConsensusSource",
                    s("TL-MAIN-RETAINED-CONSENSUS-SOURCE-13-1"),
                ),
                (
                    "requiredRawAction",
                    s(crate::source_main_retained_caller_action::PROFILE),
                ),
                (
                    "admittedActions",
                    s(if profile == RETAINED_CALLER_START_WAL_PROFILE {
                        "StartOnly"
                    } else {
                        "SelectedActions"
                    }),
                ),
                ("actualEntryHead", s(&hex(&owner.head.commitment))),
                ("actualEntryCommands", s(&owner.head.commands.to_string())),
                ("actualEntryBefore", physical_value(&actual_entry.physical)),
                ("actualNativeOrigin", physical_value(&owner.origin_physical)),
                ("actualFoundation", s(&hex(&sha256(&owner.foundation)))),
                ("actualArchive", CallerStartBlob::of(&archive)?.value()),
                ("actualBefore", CallerStartBlob::of(&before)?.value()),
                ("rawStart", CallerStartBlob::of(&raw)?.value()),
            ]);
            // Generic metadata Store commits this NEW immutable origin before
            // any load_module/header/body action on the retained source owner.
            if let Some(configuration) = &configuration {
                origin
                    .insert("selectedConfiguration", configuration.clone())
                    .map_err(caller_start_backend)?;
            }
            let wal = CallerStartWal::create(root.as_ref(), namespace, origin.clone(), filesystem)?;
            Ok((raw, archive, before, origin, wal))
        })();
        match checked {
            Ok((raw, archive, before, origin, wal)) => Ok(Self {
                owner: Some(RetainedCallerStartOwnerV1::Ready(Box::new(owner))),
                wal,
                origin,
                action,
                raw,
                archive,
                before,
                post: None,
                route: None,
                source_error: None,
                executed: false,
                phase: RetainedCallerStartWalPhaseV1::Intent,
                plan: None,
                observation_limit,
                profile,
                selected_entry: None,
                #[cfg(test)]
                barrier_fault: None,
            }),
            Err(error) => Err(Box::new(RejectedRetainedCallerStartWalV1 { error, owner })),
        }
    }
    fn command(
        start: &crate::source_main_retained_caller_action::RetainedMainCallerStartV1,
    ) -> NativeMainRetainedCallerV1 {
        match start {
            crate::source_main_retained_caller_action::RetainedMainCallerStartV1::Initialize => {
                NativeMainRetainedCallerV1::Initialize
            }
            crate::source_main_retained_caller_action::RetainedMainCallerStartV1::InitOrLoad => {
                NativeMainRetainedCallerV1::InitOrLoad
            }
            crate::source_main_retained_caller_action::RetainedMainCallerStartV1::SyncIndex {
                max,
            } => NativeMainRetainedCallerV1::SyncIndex { max: max.clone() },
        }
    }
    fn ready_observation(
        owner: &SourceMainNativeSessionV13,
        limit: usize,
    ) -> CallerStartResult<Vec<u8>> {
        let mut fields =
            NativeFieldCollector::new("Native13/RetainedCallerStartObservation/v1", true, limit);
        fields
            .emit("ownerKind", &s("NativeSession"))
            .map_err(caller_start_backend)?;
        fields
            .emit(
                "financial",
                &s(&hex(&owner
                    .financial
                    .native_main_financial_commitment()
                    .map_err(caller_start_backend)?)),
            )
            .map_err(caller_start_backend)?;
        native_step_value_fields(
            "physical",
            &physical_value(&physical(&owner.index).map_err(caller_start_backend)?),
            &mut fields,
        )
        .map_err(caller_start_backend)?;
        native_step_value_fields(
            "rows",
            &logical_read(&complete(&owner.index).map_err(caller_start_backend)?),
            &mut fields,
        )
        .map_err(caller_start_backend)?;
        Ok(fields.frames.expect("retained source observations"))
    }
    fn observation(&self) -> CallerStartResult<Vec<u8>> {
        match self
            .owner
            .as_ref()
            .ok_or(RetainedCallerStartWalErrorV1::InvalidInput)?
        {
            RetainedCallerStartOwnerV1::Ready(owner) => {
                Self::ready_observation(owner, self.observation_limit)
            }
            RetainedCallerStartOwnerV1::Caller(caller) => {
                let mut fields = NativeFieldCollector::new(
                    "Native13/RetainedCallerStartObservation/v1",
                    true,
                    self.observation_limit,
                );
                fields
                    .emit("ownerKind", &s("RetainedCaller"))
                    .map_err(caller_start_backend)?;
                caller
                    .visit_canonical_fields(&mut |p, v| fields.emit(p, v))
                    .map_err(caller_start_backend)?;
                fields
                    .emit("actualRoute/present", &V::Bool(self.route.is_some()))
                    .map_err(caller_start_backend)?;
                if let Some(route) = &self.route {
                    native_step_value_fields("actualRoute", route.value(), &mut fields)
                        .map_err(caller_start_backend)?;
                }
                Ok(fields.frames.expect("retained source observations"))
            }
        }
    }
    fn record(&self, stage: &str) -> CallerStartResult<V> {
        let mut record = obj(&[
            ("profile", s(self.profile)),
            ("stage", s(stage)),
            ("sequence", s("1")),
            ("rawAction", CallerStartBlob::of(&self.raw)?.value()),
            ("actualBefore", CallerStartBlob::of(&self.before)?.value()),
            ("actualArchive", CallerStartBlob::of(&self.archive)?.value()),
            (
                "actualAccepted",
                self.post
                    .as_ref()
                    .map(|raw| CallerStartBlob::of(raw).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            ("constructorDispatched", V::Bool(self.executed)),
            (
                "apiRejection",
                self.source_error.as_deref().map_or(V::Null, s),
            ),
        ]);
        if let Some(entry) = &self.selected_entry {
            record
                .insert("selectedStartEntry", CallerStartBlob::of(entry)?.value())
                .map_err(caller_start_backend)?;
        }
        Ok(record)
    }
    fn write_barrier(&mut self, stage: &str) -> CallerStartResult<()> {
        #[cfg(test)]
        if self
            .barrier_fault
            .as_ref()
            .is_some_and(|(phase, _)| *phase == self.phase)
        {
            let (_, fault) = self.barrier_fault.take().expect("matching actual barrier");
            self.wal.set_test_fault(fault);
        }
        if self.plan.is_none() {
            let mut blobs = vec![self.raw.clone(), self.before.clone(), self.archive.clone()];
            if let Some(post) = &self.post {
                blobs.push(post.clone());
            }
            if let Some(entry) = &self.selected_entry {
                blobs.push(entry.clone());
            }
            let refs = blobs
                .iter()
                .map(|b| CallerStartBlob::of(b))
                .collect::<Result<Vec<_>, _>>()?;
            self.plan =
                Some(
                    self.wal
                        .prepare(&self.wal.pin()?, self.record(stage)?, blobs, refs)?,
                );
        }
        let plan = self.plan.as_mut().expect("actual sealed WAL plan");
        if plan.attempted() {
            return Err(RetainedCallerStartWalErrorV1::NeedWalRecovery);
        }
        self.wal.attempt(plan)?;
        self.plan = None;
        Ok(())
    }
    fn execute_once(&mut self) -> CallerStartResult<()> {
        if self.executed || self.phase != RetainedCallerStartWalPhaseV1::Execute {
            return Err(RetainedCallerStartWalErrorV1::AlreadyExecuted);
        }
        let crate::source_main_retained_caller_action::RetainedMainCallerActionV1::Start {
            caller,
            scope,
            generation,
            documents,
        } = &self.action
        else {
            return Err(RetainedCallerStartWalErrorV1::InvalidInput);
        };
        let command = Self::command(caller);
        let scope = *scope;
        let generation = *generation;
        let documents = documents.clone();
        let owner = match self.owner.take() {
            Some(RetainedCallerStartOwnerV1::Ready(owner)) => owner,
            other => {
                self.owner = other;
                return Err(RetainedCallerStartWalErrorV1::AlreadyExecuted);
            }
        };
        let expected = match owner.expectation() {
            Ok(expected) => expected,
            Err(error) => {
                self.owner = Some(RetainedCallerStartOwnerV1::Ready(owner));
                return Err(caller_start_backend(error));
            }
        };
        self.executed = true; // irreversible marker BEFORE the real constructor.
        match owner.into_retained_main_caller(&expected, command, scope, generation, documents) {
            Ok(caller) => {
                match NativeMainRetainedCallerRouteV1::from_actual_caller(&caller) {
                    Ok(route) => self.route = Some(route),
                    Err(error) => {
                        self.source_error = Some(format!("actual route unavailable: {error:?}"))
                    }
                }
                self.owner = Some(RetainedCallerStartOwnerV1::Caller(Box::new(caller)));
            }
            Err(rejected) => {
                let RejectedNativeMainRetainedCallerV1 { error, owner } = *rejected;
                self.source_error = Some(format!("{error:?}"));
                self.owner = Some(RetainedCallerStartOwnerV1::Ready(Box::new(owner)));
            }
        }
        Ok(())
    }
    pub fn resume(&mut self) -> CallerStartResult<RetainedCallerStartWalPhaseV1> {
        loop {
            match self.phase {
                RetainedCallerStartWalPhaseV1::Intent => {
                    self.write_barrier("Intent")?;
                    self.phase = RetainedCallerStartWalPhaseV1::Execute;
                }
                RetainedCallerStartWalPhaseV1::Execute => {
                    self.execute_once()?;
                    self.phase = RetainedCallerStartWalPhaseV1::Observe;
                }
                RetainedCallerStartWalPhaseV1::Observe => {
                    let observed = self.observation()?;
                    let mut post = b"TLMCSP\x01".to_vec();
                    post.extend_from_slice(&(observed.len() as u64).to_be_bytes());
                    post.extend_from_slice(&observed);
                    let diagnostic = caller_start_bytes(&obj(&[
                        (
                            "apiRejection",
                            self.source_error.as_deref().map_or(V::Null, s),
                        ),
                        ("routeCaptured", V::Bool(self.route.is_some())),
                    ]))?;
                    post.extend_from_slice(&(diagnostic.len() as u64).to_be_bytes());
                    post.extend_from_slice(&diagnostic);
                    self.post = Some(post);
                    if self.profile == RETAINED_CALLER_SELECTED_WAL_PROFILE {
                        self.selected_entry = Some(caller_start_bytes(&obj(&[
                            ("profile", s(self.profile)),
                            ("sequence", s("1")),
                            ("rawAction", CallerStartBlob::of(&self.raw)?.value()),
                            ("before", CallerStartBlob::of(&self.before)?.value()),
                            (
                                "accepted",
                                CallerStartBlob::of(self.post.as_ref().expect("actual post"))?
                                    .value(),
                            ),
                            ("origin", self.origin.clone()),
                        ]))?);
                    }

                    self.phase = RetainedCallerStartWalPhaseV1::Accepted;
                }
                RetainedCallerStartWalPhaseV1::Accepted => {
                    self.write_barrier("Accepted")?;
                    self.phase = RetainedCallerStartWalPhaseV1::Ack;
                }
                RetainedCallerStartWalPhaseV1::Ack => {
                    self.write_barrier("Ack")?;
                    self.phase = RetainedCallerStartWalPhaseV1::StartAcknowledged;
                }
                RetainedCallerStartWalPhaseV1::StartAcknowledged => return Ok(self.phase),
            }
        }
    }
    pub fn recover_wal(
        &mut self,
    ) -> CallerStartResult<crate::source_main_active_service_wal::ActiveServiceWalResolvedV1> {
        let plan = self
            .plan
            .as_ref()
            .ok_or(RetainedCallerStartWalErrorV1::InvalidInput)?;
        let resolved = self.wal.resolve(plan)?;
        match resolved {
            crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Before => {
                self.plan = Some(self.wal.rearm_before(plan)?)
            }
            crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post => {
                self.plan = None;
                self.phase = match self.phase {
                    RetainedCallerStartWalPhaseV1::Intent => RetainedCallerStartWalPhaseV1::Execute,
                    RetainedCallerStartWalPhaseV1::Accepted => RetainedCallerStartWalPhaseV1::Ack,
                    RetainedCallerStartWalPhaseV1::Ack => {
                        RetainedCallerStartWalPhaseV1::StartAcknowledged
                    }
                    _ => return Err(RetainedCallerStartWalErrorV1::InvalidInput),
                };
            }
        }
        Ok(resolved)
    }
    pub fn phase(&self) -> RetainedCallerStartWalPhaseV1 {
        self.phase
    }
    pub fn constructor_dispatched(&self) -> bool {
        self.executed
    }
    pub fn wal_pin(&self) -> CallerStartResult<CallerStartPin> {
        Ok(self.wal.pin()?)
    }
    pub fn origin(&self) -> &V {
        &self.origin
    }
    pub fn actual_route(&self) -> Option<&V> {
        self.route
            .as_ref()
            .map(NativeMainRetainedCallerRouteV1::value)
    }
    pub fn effects(&self) -> &[V] {
        match self.owner.as_ref().expect("retained same source owner") {
            RetainedCallerStartOwnerV1::Ready(owner) => {
                owner.financial.runtime().native_main_row_effects()
            }
            RetainedCallerStartOwnerV1::Caller(caller) => caller.effects(),
        }
    }
}
