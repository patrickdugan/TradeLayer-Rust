// Explicit origin-selected action WAL. Native ambiguity is retained separately
// and can never repeat caller/source dispatch. Recovery continuation follows.
pub const RETAINED_CALLER_SELECTED_WAL_PROFILE: &str = "TL-MAIN-RETAINED-CALLER-SELECTED-WAL-13-1";
#[derive(Debug)]
pub enum RetainedCallerSelectedWalErrorV1 {
    Start(RetainedCallerStartWalErrorV1),
    Wal(crate::source_main_active_service_wal::ActiveServiceWalErrorV1),
    Action(crate::source_main_retained_caller_action::RetainedMainCallerActionErrorV1),
    Backend(String),
    Busy,
    NotStarted,
    WrongAction,
    WrongSelector,
    NeedWalRecovery,
    NativeRecoveryRequired,
    Capacity,
}
impl From<RetainedCallerStartWalErrorV1> for RetainedCallerSelectedWalErrorV1 {
    fn from(e: RetainedCallerStartWalErrorV1) -> Self {
        Self::Start(e)
    }
}
impl From<crate::source_main_active_service_wal::ActiveServiceWalErrorV1>
    for RetainedCallerSelectedWalErrorV1
{
    fn from(e: crate::source_main_active_service_wal::ActiveServiceWalErrorV1) -> Self {
        Self::Wal(e)
    }
}
impl From<crate::source_main_retained_caller_action::RetainedMainCallerActionErrorV1>
    for RetainedCallerSelectedWalErrorV1
{
    fn from(e: crate::source_main_retained_caller_action::RetainedMainCallerActionErrorV1) -> Self {
        Self::Action(e)
    }
}
type CallerSelectedResult<T> = std::result::Result<T, RetainedCallerSelectedWalErrorV1>;
fn caller_selected_backend(e: impl core::fmt::Debug) -> RetainedCallerSelectedWalErrorV1 {
    RetainedCallerSelectedWalErrorV1::Backend(format!("{e:?}"))
}
fn caller_selected_bytes(v: &V) -> CallerSelectedResult<Vec<u8>> {
    v.diagnostic_json()
        .map(String::into_bytes)
        .map_err(caller_selected_backend)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedCallerSelectedWalPhaseV1 {
    Start,
    Idle,
    Intent,
    Execute,
    Observe,
    Accepted,
    Ack,
    NativeRecoveryRequired,
    NativeSupersessionIntent,
    NativeSupersessionAppend,
    EscrowPrepare,
    EscrowBoundIntent,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedCallerSelectedWalReceiptV1 {
    pub sequence: u64,
    pub raw_action: CallerStartBlob,
    pub before: CallerStartBlob,
    pub accepted: CallerStartBlob,
    pub entry: CallerStartBlob,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
}
struct RetainedCallerSelectedPendingV1 {
    action: crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
    raw: Vec<u8>,
    before: Vec<u8>,
    operation: Option<NativeMainRetainedConsensusPendingV1>,
    candidates: Option<Vec<u8>>,
    mutations: Option<Vec<u8>>,
    native_recovery: Option<Vec<u8>>,
    recovery_prior: Option<CallerStartBlob>,
    replacement: Option<RetainedCallerSelectedReplacementV1>,
    replacement_mutations: Option<Vec<u8>>,
    escrow_permit: Option<crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    escrow_chain: Option<Vec<u8>>,
    escrow_prior: Option<CallerStartBlob>,
    escrow_replacement: bool,
    executed: bool,
    result: Option<V>,
    post: Option<Vec<u8>>,
    entry: Option<Vec<u8>>,
    receipt: Option<RetainedCallerSelectedWalReceiptV1>,
    plan: Option<CallerStartPlan>,
}
pub struct RejectedRetainedCallerSelectedWalV1 {
    pub error: RetainedCallerStartWalErrorV1,
    owner: SourceMainNativeSessionV13,
}
impl RejectedRetainedCallerSelectedWalV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedRetainedCallerSelectedWalV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedRetainedCallerSelectedWalV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
pub struct SourceMainNativeDurableSelectedCallerV1 {
    start: Option<SourceMainNativeDurableCallerStartV1>,
    owner: Option<RetainedCallerSelectedOwnerV1>,
    wal: Option<CallerStartWal>,
    route: Option<NativeMainRetainedCallerRouteV1>,
    origin: V,
    origin_archive: Vec<u8>,
    phase: RetainedCallerSelectedWalPhaseV1,
    pending: Option<RetainedCallerSelectedPendingV1>,
    history: Vec<RetainedCallerSelectedWalReceiptV1>,
    head: [u8; 32],
    acknowledged_append: Option<V>,
    observation_limit: usize,
    escrow_root: PathBuf,
    escrow_durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    #[cfg(test)]
    barrier_fault: Option<(
        RetainedCallerSelectedWalPhaseV1,
        crate::source_main_active_service_wal::TestFault,
    )>,
}
impl SourceMainNativeDurableSelectedCallerV1 {
    pub fn create(
        owner: SourceMainNativeSessionV13,
        expected: &NativeMainSessionExpectation,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        start: crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
        entry: (
            usize,
            PathBuf,
            crate::compact_journal::GenerationEscrowDurabilityV1,
        ),
    ) -> Result<Self, Box<RejectedRetainedCallerSelectedWalV1>> {
        let (observation_limit, escrow_root, escrow_durability) = entry;
        let checked = std::fs::symlink_metadata(&escrow_root).and_then(|metadata| {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(std::io::Error::other("actual escrow directory required"));
            }
            escrow_root.canonicalize()
        });
        let escrow_root = match checked {
            Ok(root) => root,
            Err(error) => {
                return Err(Box::new(RejectedRetainedCallerSelectedWalV1 {
                    error: caller_start_backend(error),
                    owner,
                }))
            }
        };
        let configuration = obj(&[
            ("generationEscrowRoot", s(&escrow_root.to_string_lossy())),
            (
                "generationEscrowDurability",
                s(&format!("{escrow_durability:?}")),
            ),
        ]);
        match SourceMainNativeDurableCallerStartV1::create_profile(
            owner,
            expected,
            root,
            namespace,
            filesystem,
            start,
            (
                observation_limit,
                RETAINED_CALLER_SELECTED_WAL_PROFILE,
                Some(configuration),
            ),
        ) {
            Ok(start) => Ok(Self {
                origin: start.origin.clone(),
                origin_archive: start.archive.clone(),
                start: Some(start),
                owner: None,
                wal: None,
                route: None,
                phase: RetainedCallerSelectedWalPhaseV1::Start,
                pending: None,
                history: vec![],
                head: [0; 32],
                acknowledged_append: None,
                observation_limit,
                escrow_root,
                escrow_durability,
                #[cfg(test)]
                barrier_fault: None,
            }),
            Err(rejected) => {
                let RejectedRetainedCallerStartWalV1 { error, owner } = *rejected;
                Err(Box::new(RejectedRetainedCallerSelectedWalV1 {
                    error,
                    owner,
                }))
            }
        }
    }
    fn finish_start(&mut self) -> CallerSelectedResult<()> {
        let start = self
            .start
            .as_mut()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        start.resume()?;
        if start.phase != RetainedCallerStartWalPhaseV1::StartAcknowledged
            || start.route.is_none()
            || !matches!(start.owner, Some(RetainedCallerStartOwnerV1::Caller(_)))
        {
            return Err(RetainedCallerSelectedWalErrorV1::NotStarted);
        }
        self.history
            .try_reserve(1)
            .map_err(|_| RetainedCallerSelectedWalErrorV1::Capacity)?;
        // All fallible pin/blob verification is BEFORE moving the only owner.
        let entry = start
            .selected_entry
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        let entry_pin = CallerStartBlob::of(entry)?;
        if start.wal.record()?.get("selectedStartEntry") != entry_pin.value()
            || start.wal.read_blob(&entry_pin)?.as_slice() != entry.as_slice()
        {
            return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
        }
        let head = sha256(entry);
        let receipt = RetainedCallerSelectedWalReceiptV1 {
            sequence: 1,
            raw_action: CallerStartBlob::of(&start.raw)?,
            before: CallerStartBlob::of(&start.before)?,
            accepted: CallerStartBlob::of(
                start
                    .post
                    .as_ref()
                    .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?,
            )?,
            entry: entry_pin,
            previous_head: [0; 32],
            head,
        };
        let SourceMainNativeDurableCallerStartV1 {
            owner, wal, route, ..
        } = self.start.take().expect("checked actual started owner");
        let Some(RetainedCallerStartOwnerV1::Caller(caller)) = owner else {
            unreachable!("checked actual caller")
        };
        self.owner = Some(RetainedCallerSelectedOwnerV1::Caller(caller));
        self.wal = Some(wal);
        self.route = route;
        self.head = head;
        self.history.push(receipt);
        self.phase = RetainedCallerSelectedWalPhaseV1::Idle;
        Ok(())
    }
    fn observe(&self) -> CallerSelectedResult<Vec<u8>> {
        let owner = self
            .owner
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        let route = self
            .route
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        let mut fields = NativeFieldCollector::new(
            "Native13/RetainedCallerSelectedObservation/v1",
            true,
            self.observation_limit,
        );
        owner.observe(&mut fields)?;
        native_step_value_fields("actualOriginRoute", route.value(), &mut fields)
            .map_err(caller_selected_backend)?;
        let actual = owner.session();
        let rows = complete(&actual.index).map_err(caller_selected_backend)?;
        native_step_value_fields("completeNativeRows", &logical_read(&rows), &mut fields)
            .map_err(caller_selected_backend)?;
        Ok(fields.frames.expect("retained actual observations"))
    }
    pub fn begin(
        &mut self,
        action: crate::source_main_retained_caller_action::RetainedMainCallerActionV1,
    ) -> CallerSelectedResult<()> {
        use crate::source_main_retained_caller_action::RetainedMainCallerActionV1 as Action;
        if self.phase != RetainedCallerSelectedWalPhaseV1::Idle || self.pending.is_some() {
            return Err(RetainedCallerSelectedWalErrorV1::Busy);
        }
        if matches!(action, Action::Start { .. }) {
            return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
        }
        if self.history.len() >= 4096 {
            return Err(RetainedCallerSelectedWalErrorV1::Capacity);
        }
        self.history
            .try_reserve(1)
            .map_err(|_| RetainedCallerSelectedWalErrorV1::Capacity)?;
        let route = self
            .route
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        let owner = self
            .owner
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        if !owner.route_matches(route) {
            return Err(RetainedCallerSelectedWalErrorV1::WrongSelector);
        }
        let raw = action.canonical_action_bytes()?;
        if crate::source_main_retained_caller_action::RetainedMainCallerActionV1::from_canonical_action_bytes(&raw)?!=action{return Err(RetainedCallerSelectedWalErrorV1::WrongAction)}
        let operation = caller_selected_action_selector(&action)
            .map(|raw| owner.select(route, raw))
            .transpose()?;
        let mut candidates = None;
        let mut mutations = None;
        if matches!(
            action,
            Action::AppendSelected { .. } | Action::ResolveSelectedPost { .. }
        ) {
            if matches!(action, Action::AppendSelected { .. }) && self.acknowledged_append.is_some()
            {
                return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
            }
            let op = operation
                .as_ref()
                .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?;
            let actual = owner.native(op)?;
            if matches!(action, Action::ResolveSelectedPost { .. })
                && self.acknowledged_append.as_ref() != Some(&op.value())
            {
                return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
            }
            if matches!(action, Action::AppendSelected { .. }) {
                if let Some(plan) = actual.append() {
                    let binding=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(plan,actual.context()).map_err(caller_selected_backend)?;
                    candidates = Some(caller_selected_bytes(
                        binding.independently_retained_facts(),
                    )?);
                    mutations = Some(binding.exact_prepared_mutations().to_vec());
                }
            }
        }
        self.pending = Some(RetainedCallerSelectedPendingV1 {
            action,
            raw,
            before: self.observe()?,
            operation,
            candidates,
            mutations,
            native_recovery: None,
            recovery_prior: None,
            replacement: None,
            replacement_mutations: None,
            escrow_permit: None,
            escrow_chain: None,
            escrow_prior: None,
            escrow_replacement: false,
            executed: false,
            result: None,
            post: None,
            entry: None,
            receipt: None,
            plan: None,
        });
        self.phase = RetainedCallerSelectedWalPhaseV1::Intent;
        Ok(())
    }
    fn record(&self, stage: &str) -> CallerSelectedResult<V> {
        let p = self
            .pending
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        let mut record = obj(&[
            ("profile", s(RETAINED_CALLER_SELECTED_WAL_PROFILE)),
            ("stage", s(stage)),
            ("sequence", s(&(self.history.len() + 1).to_string())),
            ("previousHead", s(&hex(&self.head))),
            ("rawAction", CallerStartBlob::of(&p.raw)?.value()),
            ("before", CallerStartBlob::of(&p.before)?.value()),
            (
                "nativeCandidates",
                p.candidates
                    .as_ref()
                    .map(|r| CallerStartBlob::of(r).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "nativeMutations",
                p.mutations
                    .as_ref()
                    .map(|r| CallerStartBlob::of(r).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "accepted",
                p.post
                    .as_ref()
                    .map(|r| CallerStartBlob::of(r).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "entry",
                p.entry
                    .as_ref()
                    .map(|r| CallerStartBlob::of(r).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
        ]);
        if let Some(raw) = &p.native_recovery {
            record
                .insert("nativeRecovery", CallerStartBlob::of(raw)?.value())
                .map_err(caller_selected_backend)?;
        }
        if let Some(raw) = &p.escrow_chain {
            record
                .insert("generationEscrow", CallerStartBlob::of(raw)?.value())
                .map_err(caller_selected_backend)?;
            record
                .insert("escrowReplacement", V::Bool(p.escrow_replacement))
                .map_err(caller_selected_backend)?;
        }
        Ok(record)
    }
    fn write_barrier(&mut self, stage: &str) -> CallerSelectedResult<()> {
        #[cfg(test)]
        if self
            .barrier_fault
            .as_ref()
            .is_some_and(|(phase, _)| *phase == self.phase)
        {
            let (_, fault) = self.barrier_fault.take().expect("matching real barrier");
            self.wal
                .as_mut()
                .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?
                .set_test_fault(fault);
        }
        if self.pending.as_ref().is_none_or(|p| p.plan.is_none()) {
            let p = self
                .pending
                .as_ref()
                .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
            let mut blobs = vec![p.raw.clone(), p.before.clone(), self.origin_archive.clone()];
            for raw in [
                &p.candidates,
                &p.mutations,
                &p.native_recovery,
                &p.replacement_mutations,
                &p.escrow_chain,
                &p.post,
                &p.entry,
            ]
            .into_iter()
            .flatten()
            {
                blobs.push(raw.clone());
            }
            let mut refs = blobs
                .iter()
                .map(|r| CallerStartBlob::of(r))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(previous) = self.history.last() {
                refs.push(previous.entry.clone());
            }
            if let Some(prior) = &p.recovery_prior {
                refs.push(prior.clone());
            }
            if let Some(prior) = &p.escrow_prior {
                refs.push(prior.clone());
            }
            let wal = self
                .wal
                .as_ref()
                .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
            let plan = wal.prepare(&wal.pin()?, self.record(stage)?, blobs, refs)?;
            self.pending.as_mut().expect("pending").plan = Some(plan);
        }
        let p = self.pending.as_mut().expect("pending");
        let plan = p.plan.as_mut().expect("sealed actual plan");
        if plan.attempted() {
            return Err(RetainedCallerSelectedWalErrorV1::NeedWalRecovery);
        }
        self.wal.as_mut().expect("actual WAL").attempt(plan)?;
        p.plan = None;
        Ok(())
    }
    fn execute_once(&mut self) -> CallerSelectedResult<()> {
        use crate::source_main_retained_caller_action::RetainedMainCallerActionV1 as Action;
        let p = self
            .pending
            .as_mut()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        if p.executed {
            return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
        }
        let action = p.action.clone();
        let mut actual = self
            .owner
            .take()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        let scope = match actual.session_mut().index.begin_inline_escrow_guard() {
            Ok(scope) => scope,
            Err(error) => {
                self.owner = Some(actual);
                return Err(caller_selected_backend(error));
            }
        };
        self.pending.as_mut().expect("pending").executed = true; // immediately before actual source/control dispatch
        let (owner, result) = caller_selected_execute(
            actual,
            self.route.as_ref().expect("sealed actual route"),
            &action,
            self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
        );
        self.owner = Some(owner);
        self.owner
            .as_mut()
            .expect("same actual owner")
            .session_mut()
            .index
            .finish_inline_escrow_guard(scope)
            .map_err(caller_selected_backend)?;
        self.pending.as_mut().expect("pending").result = Some(result);
        if matches!(action, Action::AppendSelected { .. }) {
            let p = self.pending.as_ref().expect("pending");
            let actual = self.owner.as_ref().expect("retained actual owner");
            if actual
                .native(p.operation.as_ref().expect("checked actual op"))
                .and_then(|w| {
                    w.verify_post(&actual.session().index)
                        .map_err(RetainedCallerSelectedWalErrorV1::Backend)
                })
                .is_err()
            {
                self.phase = RetainedCallerSelectedWalPhaseV1::NativeRecoveryRequired;
                return Err(RetainedCallerSelectedWalErrorV1::NativeRecoveryRequired);
            }
        }
        self.phase = RetainedCallerSelectedWalPhaseV1::Observe;
        Ok(())
    }
    fn seal_post(&mut self) -> CallerSelectedResult<()> {
        let actual = self.observe()?;
        let p = self
            .pending
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        let result = caller_selected_bytes(
            p.result
                .as_ref()
                .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?,
        )?;
        let mut post = b"TLMCSA\x01".to_vec();
        post.extend_from_slice(&(actual.len() as u64).to_be_bytes());
        post.extend_from_slice(&actual);
        post.extend_from_slice(&(result.len() as u64).to_be_bytes());
        post.extend_from_slice(&result);
        let mut entry = obj(&[
            ("profile", s(RETAINED_CALLER_SELECTED_WAL_PROFILE)),
            ("sequence", s(&(self.history.len() + 1).to_string())),
            ("previousHead", s(&hex(&self.head))),
            ("rawAction", CallerStartBlob::of(&p.raw)?.value()),
            ("before", CallerStartBlob::of(&p.before)?.value()),
            ("accepted", CallerStartBlob::of(&post)?.value()),
            ("actualOrigin", self.origin.clone()),
        ]);
        for (name, raw) in [
            ("nativeCandidates", &p.candidates),
            ("nativeMutations", &p.mutations),
            ("nativeRecovery", &p.native_recovery),
            ("generationEscrow", &p.escrow_chain),
        ] {
            entry
                .insert(
                    name,
                    raw.as_ref()
                        .map(|r| CallerStartBlob::of(r).map(|p| p.value()))
                        .transpose()?
                        .unwrap_or(V::Null),
                )
                .map_err(caller_selected_backend)?;
        }
        let entry = caller_selected_bytes(&entry)?;
        let mut seed = self.head.to_vec();
        seed.extend_from_slice(&sha256(&entry));
        let head = sha256(&seed);
        let receipt = RetainedCallerSelectedWalReceiptV1 {
            sequence: (self.history.len() + 1) as u64,
            raw_action: CallerStartBlob::of(&p.raw)?,
            before: CallerStartBlob::of(&p.before)?,
            accepted: CallerStartBlob::of(&post)?,
            entry: CallerStartBlob::of(&entry)?,
            previous_head: self.head,
            head,
        };
        let p = self.pending.as_mut().expect("pending");
        p.post = Some(post);
        p.entry = Some(entry);
        p.receipt = Some(receipt);
        self.phase = RetainedCallerSelectedWalPhaseV1::Accepted;
        Ok(())
    }
    fn install_ack(&mut self) -> CallerSelectedResult<()> {
        use crate::source_main_retained_caller_action::RetainedMainCallerActionV1 as Action;
        let p = self
            .pending
            .take()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        if matches!(p.action, Action::AppendSelected { .. }) {
            self.acknowledged_append = p.operation.as_ref().map(|p| p.value());
        } else if matches!(p.action, Action::ResolveSelectedPost { .. }) {
            let remains =
                self.owner
                    .as_ref()
                    .zip(self.route.as_ref())
                    .is_some_and(|(owner, route)| {
                        p.operation
                            .as_ref()
                            .is_some_and(|op| owner.operations(route).contains(op))
                    });
            if !remains {
                self.acknowledged_append = None;
            }
        }
        let receipt = p.receipt.expect("actual Accepted receipt");
        self.head = receipt.head;
        self.history.push(receipt);
        self.phase = RetainedCallerSelectedWalPhaseV1::Idle;
        Ok(())
    }
    pub fn resume(&mut self) -> CallerSelectedResult<RetainedCallerSelectedWalPhaseV1> {
        loop {
            match self.phase {
                RetainedCallerSelectedWalPhaseV1::Start => self.finish_start()?,
                RetainedCallerSelectedWalPhaseV1::Idle => return Ok(self.phase),
                RetainedCallerSelectedWalPhaseV1::Intent => {
                    self.write_barrier("Intent")?;
                    if matches!(self.pending.as_ref().expect("pending").action,crate::source_main_retained_caller_action::RetainedMainCallerActionV1::AppendSelected{..}){self.pending.as_mut().expect("pending").escrow_replacement=false;self.phase=RetainedCallerSelectedWalPhaseV1::EscrowPrepare;}else{self.phase = RetainedCallerSelectedWalPhaseV1::Execute;}
                }
                RetainedCallerSelectedWalPhaseV1::Execute => self.execute_once()?,
                RetainedCallerSelectedWalPhaseV1::Observe => self.seal_post()?,
                RetainedCallerSelectedWalPhaseV1::Accepted => {
                    self.write_barrier("Accepted")?;
                    self.phase = RetainedCallerSelectedWalPhaseV1::Ack;
                }
                RetainedCallerSelectedWalPhaseV1::Ack => {
                    self.write_barrier("Ack")?;
                    self.install_ack()?;
                }
                RetainedCallerSelectedWalPhaseV1::NativeRecoveryRequired => {
                    return Err(RetainedCallerSelectedWalErrorV1::NativeRecoveryRequired)
                }
                RetainedCallerSelectedWalPhaseV1::NativeSupersessionIntent => {
                    self.write_barrier("NativeSupersessionIntent")?;
                    self.pending.as_mut().expect("pending").escrow_replacement = true;
                    self.phase = RetainedCallerSelectedWalPhaseV1::EscrowPrepare;
                }
                RetainedCallerSelectedWalPhaseV1::NativeSupersessionAppend => {
                    self.append_replacement_native_only()?
                }
                RetainedCallerSelectedWalPhaseV1::EscrowPrepare => {
                    if self.prepare_selected_generation_escrow()? {
                        self.phase = RetainedCallerSelectedWalPhaseV1::EscrowBoundIntent;
                    } else {
                        self.execute_selected_escrow_route()?;
                    }
                }
                RetainedCallerSelectedWalPhaseV1::EscrowBoundIntent => {
                    self.write_barrier("EscrowBoundIntent")?;
                    self.execute_selected_escrow_route()?;
                }
            }
        }
    }
    pub fn recover_wal(
        &mut self,
    ) -> CallerSelectedResult<crate::source_main_active_service_wal::ActiveServiceWalResolvedV1>
    {
        if self.phase == RetainedCallerSelectedWalPhaseV1::Start {
            return Ok(self
                .start
                .as_mut()
                .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?
                .recover_wal()?);
        }
        let p = self
            .pending
            .as_mut()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        let plan = p
            .plan
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongAction)?;
        let wal = self
            .wal
            .as_mut()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        let state = wal.resolve(plan)?;
        match state {
            crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Before => {
                p.plan = Some(wal.rearm_before(plan)?)
            }
            crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post => {
                p.plan = None;
                self.phase = match self.phase {
                    RetainedCallerSelectedWalPhaseV1::Intent => {
                        if matches!(p.action,crate::source_main_retained_caller_action::RetainedMainCallerActionV1::AppendSelected{..}){p.escrow_replacement=false;RetainedCallerSelectedWalPhaseV1::EscrowPrepare}else{RetainedCallerSelectedWalPhaseV1::Execute}
                    }
                    RetainedCallerSelectedWalPhaseV1::Accepted => {
                        RetainedCallerSelectedWalPhaseV1::Ack
                    }
                    RetainedCallerSelectedWalPhaseV1::NativeSupersessionIntent => {
                        p.escrow_replacement=true;RetainedCallerSelectedWalPhaseV1::EscrowPrepare
                    }
                    RetainedCallerSelectedWalPhaseV1::EscrowBoundIntent=>if p.escrow_replacement{RetainedCallerSelectedWalPhaseV1::NativeSupersessionAppend}else{RetainedCallerSelectedWalPhaseV1::Execute},
                    RetainedCallerSelectedWalPhaseV1::Ack => {
                        self.install_ack()?;
                        RetainedCallerSelectedWalPhaseV1::Idle
                    }
                    _ => return Err(RetainedCallerSelectedWalErrorV1::WrongAction),
                }
            }
        }
        Ok(state)
    }
    pub fn phase(&self) -> RetainedCallerSelectedWalPhaseV1 {
        self.phase
    }
    pub fn history(&self) -> &[RetainedCallerSelectedWalReceiptV1] {
        &self.history
    }
    pub fn origin(&self) -> &V {
        &self.origin
    }
    pub fn head(&self) -> [u8; 32] {
        self.head
    }
    pub fn wal_pin(&self) -> CallerSelectedResult<CallerStartPin> {
        match (&self.start, &self.wal) {
            (Some(start), None) => Ok(start.wal_pin()?),
            (None, Some(wal)) => Ok(wal.pin()?),
            _ => Err(RetainedCallerSelectedWalErrorV1::NotStarted),
        }
    }
    /// Read only bytes named by an actual retained acknowledged receipt. This
    /// does not admit this profile into any cold financial replay reader.
    pub fn read_acknowledged_entry(
        &self,
        receipt: &RetainedCallerSelectedWalReceiptV1,
    ) -> CallerSelectedResult<Vec<u8>> {
        if !self.history.contains(receipt) {
            return Err(RetainedCallerSelectedWalErrorV1::WrongAction);
        }
        Ok(self
            .wal
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?
            .read_blob(&receipt.entry)?)
    }
    pub fn pending_operations(&self) -> Vec<NativeMainRetainedConsensusPendingV1> {
        match (&self.owner, &self.route) {
            (Some(owner), Some(route)) => owner.operations(route),
            _ => vec![],
        }
    }
}
