// Explicit active-command WAL. This is not Store2 Idle, a financial checkpoint,
// a native-index recovery implementation, or a completed-row owner release.
use crate::source_main_active_service_wal::{
    ActiveServiceWalBlobV1, ActiveServiceWalErrorV1, ActiveServiceWalPinV1,
    ActiveServiceWalResolvedV1, ActiveServiceWalV1, PreparedActiveServiceWalWriteV1,
};
pub const NATIVE_MAIN_DURABLE_ACTIVE_SERVICE_PROFILE: &str = "TL-MAIN-ACTIVE-SERVICE-WAL-13-1";
const ACTIVE_WAL_OBSERVATION_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Debug)]
pub enum NativeMainActiveWalErrorV1 {
    Wal(ActiveServiceWalErrorV1),
    Service(NativeMainActiveServiceErrorV1),
    Backend(String),
    Busy,
    NoAction,
    WrongAction,
    NeedWalRecovery,
    NativeRecoveryRequired,
    Capacity,
}
impl From<ActiveServiceWalErrorV1> for NativeMainActiveWalErrorV1 {
    fn from(e: ActiveServiceWalErrorV1) -> Self {
        Self::Wal(e)
    }
}
impl From<NativeMainActiveServiceErrorV1> for NativeMainActiveWalErrorV1 {
    fn from(e: NativeMainActiveServiceErrorV1) -> Self {
        Self::Service(e)
    }
}
impl core::fmt::Display for NativeMainActiveWalErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NativeMainActiveWalErrorV1 {}
type ActiveWalResult<T> = Result<T, NativeMainActiveWalErrorV1>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMainActiveWalPhaseV1 {
    Idle,
    Intent,
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
pub struct NativeMainActiveWalReceiptV1 {
    pub sequence: u64,
    pub action: ActiveServiceWalBlobV1,
    pub before: ActiveServiceWalBlobV1,
    pub native_candidates: Option<ActiveServiceWalBlobV1>,
    pub native_mutations: Option<ActiveServiceWalBlobV1>,
    pub accepted: ActiveServiceWalBlobV1,
    pub entry: ActiveServiceWalBlobV1,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
}
enum ActiveWalOwner {
    Ready(Box<SourceMainNativeSessionV13>),
    Active(Box<SourceMainNativeActiveServiceV1>),
}
struct ActiveWalPending {
    escrow_permit: Option<crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    escrow_chain: Option<Vec<u8>>,
    escrow_prior: Option<ActiveServiceWalBlobV1>,
    escrow_route: Option<ActiveEscrowRoute>,
    native_recovery: Option<ActiveWalNativeRecovery>,
    history_before: Option<Vec<u8>>,
    history_post: Option<Vec<u8>>,
    action: NativeMainActiveServiceActionV1,
    raw: Vec<u8>,
    before: Vec<u8>,
    candidates: Option<Vec<u8>>,
    mutations: Option<Vec<u8>>,
    append_context: Option<V>,
    source_error: Option<NativeMainActiveServiceErrorV1>,
    executed: bool,
    post: Option<Vec<u8>>,
    plan: Option<PreparedActiveServiceWalWriteV1>,
    actual_fact_slots: Option<ActualSourceActionFactSlotsV1>,
    mixed_ordinary_slots: Option<MixedOrdinaryActionFactsV1>,
    entry: Option<Vec<u8>>,
    receipt: Option<NativeMainActiveWalReceiptV1>,
}
pub struct RejectedNativeMainActiveWalV1 {
    pub error: NativeMainActiveWalErrorV1,
    owner: SourceMainNativeSessionV13,
}
impl RejectedNativeMainActiveWalV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActiveWalV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActiveWalV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
/// Owns the same moved financial/index/frame owner for its entire lifetime.
/// Deliberately no owner release, legacy-head escape or state importer.
pub struct SourceMainNativeDurableActiveServiceV1 {
    owner: Option<ActiveWalOwner>,
    wal: ActiveServiceWalV1,
    phase: NativeMainActiveWalPhaseV1,
    pending: Option<ActiveWalPending>,
    origin_archive: Vec<u8>,
    origin_pin: ActiveServiceWalBlobV1,
    origin_history: Option<Vec<u8>>,
    history: Vec<NativeMainActiveWalReceiptV1>,
    head: [u8; 32],
    acknowledged_append: Option<V>,
    observation_limit: usize,
    history_enabled: bool,
    generation_escrow: Option<ActiveServiceEscrowConfigV1>,
    actual_facts: Option<ActualSourceFactModeStateV1>,
    mixed_ordinary_facts: Option<MixedOrdinaryFactModeV1>,
    segmented_history: Option<ActiveHistoryLiveStateV1>,
    #[cfg(test)]
    barrier_fault: Option<(
        NativeMainActiveWalPhaseV1,
        crate::source_main_active_service_wal::TestFault,
    )>,
}
fn active_wal_backend(e: impl core::fmt::Display) -> NativeMainActiveWalErrorV1 {
    NativeMainActiveWalErrorV1::Backend(e.to_string())
}
fn active_wal_bytes(value: &V) -> ActiveWalResult<Vec<u8>> {
    value
        .diagnostic_json()
        .map(|s| s.into_bytes())
        .map_err(active_wal_backend)
}
fn active_wal_pin(pin: &ActiveServiceWalBlobV1) -> V {
    pin.value()
}
impl SourceMainNativeDurableActiveServiceV1 {
    fn create_internal(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        history_enabled: bool,
        generation_escrow: Option<ActiveServiceEscrowConfigV1>,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_internal_fact_entry(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            history_enabled,
            (generation_escrow, false),
        )
    }
    fn create_internal_fact_entry(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        history_enabled: bool,
        entry: (Option<ActiveServiceEscrowConfigV1>, bool),
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_internal_segmented_entry(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            history_enabled,
            (entry.0, entry.1, false),
        )
    }
    fn create_internal_segmented_entry(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        history_enabled: bool,
        entry: (Option<ActiveServiceEscrowConfigV1>, bool, bool),
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_internal_mixed_fact_entry(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            history_enabled,
            (entry.0, entry.1, entry.2, false),
        )
    }
    fn create_internal_mixed_fact_entry(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        history_enabled: bool,
        entry: (Option<ActiveServiceEscrowConfigV1>, bool, bool, bool),
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        let (generation_escrow, actual_fact_mode, segmented, mixed) = entry;
        let checked = (|| {
            if observation_limit == 0 || observation_limit > ACTIVE_WAL_OBSERVATION_LIMIT {
                return Err(NativeMainActiveWalErrorV1::Capacity);
            }
            let (archive, _) = owner
                .checkpoint()
                .map_err(|e| active_wal_backend(format!("{e:?}")))?;
            let origin_pin = ActiveServiceWalBlobV1::of(&archive)?;
            let mut observation = Self::ready_observation(&owner, observation_limit)?;
            if generation_escrow.as_ref().is_some_and(|c| c.inline_profile) {
                Self::append_escrow_scope_observation(
                    &mut observation,
                    owner.index.inline_escrow_scope_active(),
                )?;
            }
            if actual_fact_mode {
                if mixed {
                    Self::append_actual_facts_scope_observation_for(
                        &mut observation,
                        MIXED_SOURCE_FACT_SCOPE,
                    )?;
                } else {
                    Self::append_actual_facts_scope_observation(&mut observation)?;
                }
            }
            if mixed {
                Self::append_mixed_ordinary_scope_observation(&mut observation)?;
                Self::append_mixed_recorded_scope(
                    &mut observation,
                    owner.index.recorded_native_only_scope_active(),
                )?;
            }
            let mut origin = obj(&[
                ("profile", s(NATIVE_MAIN_DURABLE_ACTIVE_SERVICE_PROFILE)),
                ("archive", origin_pin.value()),
                (
                    "observation",
                    ActiveServiceWalBlobV1::of(&observation)?.value(),
                ),
            ]);
            let origin_history = if history_enabled {
                let raw = Self::history_pin_bytes(&owner.index)?;
                origin
                    .insert("profile", s("TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1"))
                    .map_err(active_wal_backend)?;
                origin
                    .insert(
                        "requiredVerifiedHistory",
                        ActiveServiceWalBlobV1::of(&raw)?.value(),
                    )
                    .map_err(active_wal_backend)?;
                Some(raw)
            } else {
                None
            };
            if let Some(config) = &generation_escrow {
                origin
                    .insert("profile", s(ACTIVE_ESCROW_WAL_PROFILE))
                    .map_err(active_wal_backend)?;
                origin
                    .insert(
                        "generationEscrow",
                        obj(&[
                            ("profile", s("TL-COMPACT-GENERATION-ESCROW-13-1")),
                            ("root", s(&config.root.to_string_lossy())),
                            ("durability", s(&format!("{:?}", config.durability))),
                        ]),
                    )
                    .map_err(active_wal_backend)?;
            }
            if generation_escrow.as_ref().is_some_and(|c| c.inline_profile) {
                origin
                    .insert("profile", s(NATIVE_MAIN_INLINE_ACTIVE_WAL_PROFILE))
                    .map_err(active_wal_backend)?;
                origin.insert("requiredInlineServiceProfile",s(crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE)).map_err(active_wal_backend)?;
            }
            if actual_fact_mode {
                if !history_enabled
                    || generation_escrow
                        .as_ref()
                        .is_none_or(|config| !config.inline_profile)
                {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
                origin
                    .insert("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE))
                    .map_err(active_wal_backend)?;
                origin
                    .insert(
                        "requiredNativeFactProfile",
                        s(crate::source_main_active_service_wal::ACTUAL_NATIVE_FACT_PROFILE),
                    )
                    .map_err(active_wal_backend)?;
                origin
                    .insert("actualNativeFactScope", s(ACTUAL_SOURCE_FACT_SCOPE))
                    .map_err(active_wal_backend)?;
            }
            if segmented {
                if !history_enabled || (!mixed && (generation_escrow.is_some() || actual_fact_mode))
                {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
                origin
                    .insert(
                        "underlyingSourceHistoryProfile",
                        s("TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1"),
                    )
                    .map_err(active_wal_backend)?;
                origin
                    .insert(
                        "profile",
                        s(crate::source_main_active_service_wal::SEGMENTED_ACTIVE_HISTORY_PROFILE),
                    )
                    .map_err(active_wal_backend)?;
                origin
                    .insert("acknowledgedWindowLimit", s("256"))
                    .map_err(active_wal_backend)?;
            }
            if mixed {
                if !actual_fact_mode
                    || !history_enabled
                    || !segmented
                    || generation_escrow.as_ref().is_none_or(|c| !c.inline_profile)
                {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
                origin
                    .insert("profile", s(MIXED_SOURCE_FACT_WAL_PROFILE))
                    .map_err(active_wal_backend)?;
                origin
                    .insert("actualNativeFactScope", s(MIXED_SOURCE_FACT_SCOPE))
                    .map_err(active_wal_backend)?;
                origin.insert("requiredOrdinaryNativeFactProfile",s(crate::source_main_active_service_wal::ACTUAL_VALIDITY_PLAN_FACT_PROFILE)).map_err(active_wal_backend)?;
                origin
                    .insert(
                        "ordinaryNativeFactScope",
                        s(crate::source_main_active_service_wal::ACTUAL_VALIDITY_FACT_SCOPE),
                    )
                    .map_err(active_wal_backend)?;
                origin
                    .insert(
                        "underlyingSourceHistoryProfile",
                        s(MIXED_SOURCE_FACT_WAL_PROFILE),
                    )
                    .map_err(active_wal_backend)?;
            }
            let head = sha256(&active_wal_bytes(&origin)?);
            let wal = ActiveServiceWalV1::create(root.as_ref(), namespace, origin, filesystem)?;
            Ok((archive, origin_pin, origin_history, head, wal))
        })();
        match checked {
            Ok((origin_archive, origin_pin, origin_history, head, wal)) => Ok(Self {
                owner: Some(ActiveWalOwner::Ready(Box::new(owner))),
                wal,
                phase: NativeMainActiveWalPhaseV1::Idle,
                pending: None,
                origin_archive,
                origin_pin,
                origin_history,
                history: vec![],
                head,
                acknowledged_append: None,
                observation_limit,
                history_enabled,
                generation_escrow,
                actual_facts: actual_fact_mode.then(ActualSourceFactModeStateV1::default),
                mixed_ordinary_facts: mixed.then(MixedOrdinaryFactModeV1::default),
                segmented_history: segmented.then(ActiveHistoryLiveStateV1::default),
                #[cfg(test)]
                barrier_fault: None,
            }),
            Err(error) => Err(Box::new(RejectedNativeMainActiveWalV1 { error, owner })),
        }
    }
    pub fn profile(&self) -> &'static str {
        if self.mixed_ordinary_facts.is_some() {
            return MIXED_SOURCE_FACT_WAL_PROFILE;
        }
        if self.segmented_history.is_some() {
            return crate::source_main_active_service_wal::SEGMENTED_ACTIVE_HISTORY_PROFILE;
        }
        if self.actual_facts.is_some() {
            return ACTUAL_SOURCE_FACT_WAL_PROFILE;
        }
        if self.inline_service_enabled() {
            NATIVE_MAIN_INLINE_ACTIVE_WAL_PROFILE
        } else if self.generation_escrow.is_some() {
            ACTIVE_ESCROW_WAL_PROFILE
        } else if self.history_enabled {
            "TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1"
        } else {
            NATIVE_MAIN_DURABLE_ACTIVE_SERVICE_PROFILE
        }
    }
    pub fn phase(&self) -> NativeMainActiveWalPhaseV1 {
        self.phase
    }
    pub fn history(&self) -> &[NativeMainActiveWalReceiptV1] {
        &self.history
    }
    pub fn head(&self) -> [u8; 32] {
        self.head
    }
    pub fn wal_pin(&self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        Ok(self.wal.pin()?)
    }
    pub fn observation(&self) -> ActiveWalResult<Vec<u8>> {
        self.owner_observation()
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        match self.owner.as_ref().expect("retained actual owner") {
            ActiveWalOwner::Ready(o) => o.financial_runtime(),
            ActiveWalOwner::Active(o) => o.financial_runtime(),
        }
    }
    pub fn index(&self) -> &SourceCompactIndex {
        match self.owner.as_ref().expect("retained actual owner") {
            ActiveWalOwner::Ready(o) => o.index(),
            ActiveWalOwner::Active(o) => o.index(),
        }
    }
    pub fn active_service(&self) -> Option<&SourceMainNativeActiveServiceV1> {
        match self.owner.as_ref()? {
            ActiveWalOwner::Active(o) => Some(o),
            _ => None,
        }
    }
    pub fn read_history_blob(&self, pin: &ActiveServiceWalBlobV1) -> ActiveWalResult<Vec<u8>> {
        Ok(self.wal.read_blob(pin)?)
    }
    fn ready_observation(
        owner: &SourceMainNativeSessionV13,
        limit: usize,
    ) -> ActiveWalResult<Vec<u8>> {
        let mut fields = NativeFieldCollector::new("Native13/ActiveWalObservation/v1", true, limit);
        fields
            .emit("ownerKind", &s("NativeSession"))
            .map_err(active_wal_backend)?;
        fields
            .emit("head/commands", &s(&owner.head.commands.to_string()))
            .map_err(active_wal_backend)?;
        fields
            .emit("head/commitment", &s(&hex(&owner.head.commitment)))
            .map_err(active_wal_backend)?;
        Self::finish_observation(owner, &mut fields)?;
        Ok(fields.frames.expect("retained"))
    }
    fn finish_observation(
        owner: &SourceMainNativeSessionV13,
        fields: &mut NativeFieldCollector,
    ) -> ActiveWalResult<()> {
        let financial = owner
            .financial
            .native_main_financial_commitment()
            .map_err(active_wal_backend)?;
        fields
            .emit("financial", &s(&hex(&financial)))
            .map_err(active_wal_backend)?;
        let rows = complete(&owner.index).map_err(|e| active_wal_backend(format!("{e:?}")))?;
        native_step_value_fields("rows", &logical_read(&rows), fields)
            .map_err(active_wal_backend)?;
        let binding = physical(&owner.index).map_err(|e| active_wal_backend(format!("{e:?}")))?;
        native_step_value_fields("physical", &physical_value(&binding), fields)
            .map_err(active_wal_backend)?;
        Ok(())
    }
    fn owner_observation_unscoped(&self) -> ActiveWalResult<Vec<u8>> {
        match self.owner.as_ref().expect("retained actual owner") {
            ActiveWalOwner::Ready(o) => Self::ready_observation(o, self.observation_limit),
            ActiveWalOwner::Active(o) => {
                let mut fields = NativeFieldCollector::new(
                    "Native13/ActiveWalObservation/v1",
                    true,
                    self.observation_limit,
                );
                fields
                    .emit("ownerKind", &s("ActiveService"))
                    .map_err(active_wal_backend)?;
                o.visit_canonical_fields(&mut |p, v| fields.emit(p, v))
                    .map_err(active_wal_backend)?;
                if let Some(receipt) = o.history().last() {
                    receipt
                        .result
                        .visit_fields(&mut |p, v| fields.emit(&format!("result/{p}"), v))
                        .map_err(active_wal_backend)?;
                }
                Self::finish_observation(&o.owner, &mut fields)?;
                Ok(fields.frames.expect("retained"))
            }
        }
    }
    /// Stages raw intent without executing source. The next advance syncs it.
    pub fn prepare_action(
        &mut self,
        action: NativeMainActiveServiceActionV1,
    ) -> ActiveWalResult<()> {
        if self.phase != NativeMainActiveWalPhaseV1::Idle || self.pending.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        if self.history.len() >= self.active_ack_window_limit() {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        self.history
            .try_reserve(1)
            .map_err(|_| NativeMainActiveWalErrorV1::Capacity)?;
        match (&self.owner, &action) {
            (Some(ActiveWalOwner::Ready(_)), NativeMainActiveServiceActionV1::Start { .. }) => (),
            (Some(ActiveWalOwner::Active(_)), NativeMainActiveServiceActionV1::Start { .. })
            | (Some(ActiveWalOwner::Ready(_)), _) => {
                return Err(NativeMainActiveWalErrorV1::WrongAction)
            }
            _ => (),
        }
        let raw = action.canonical_action_bytes()?;
        // Decode and exact re-encode now, before recording or source execution.
        if NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw)? != action {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let actual_alias = if self.actual_facts.is_some() {
            self.actual_fact_action_alias(&action)?
        } else {
            None
        };
        let actual_before = if self.actual_facts.is_some() {
            Some(self.capture_actual_inline_fact_input()?)
        } else {
            None
        };
        let mixed_before = self.capture_mixed_ordinary_plan()?;
        let mixed_alias = self.mixed_ordinary_action_alias(&action)?;
        let before = self.owner_observation()?;
        let history_before = if self.history_enabled {
            Some(self.capture_history_bytes()?)
        } else {
            None
        };
        let mut candidates = None;
        let mut mutations = None;
        let mut append_context = None;
        if matches!(
            action,
            NativeMainActiveServiceActionV1::AppendPending
                | NativeMainActiveServiceActionV1::ResolvePost
        ) {
            let service = self
                .active_service()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let (context, append) = Self::pending_native_action_parts(service)?;
            if matches!(action, NativeMainActiveServiceActionV1::ResolvePost)
                && self.acknowledged_append.as_ref() != Some(&context)
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            if matches!(action, NativeMainActiveServiceActionV1::AppendPending) {
                if let Some(append) = append {
                    let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(append,context.clone())
                    .map_err(|e|active_wal_backend(format!("{e:?}")))?;
                    candidates = Some(active_wal_bytes(actual.independently_retained_facts())?);
                    mutations = Some(actual.exact_prepared_mutations().to_vec());
                }
                append_context = Some(context);
            }
        }
        self.pending = Some(ActiveWalPending {
            escrow_permit: None,
            escrow_chain: None,
            escrow_prior: None,
            escrow_route: None,
            native_recovery: None,
            history_before,
            history_post: None,
            action,
            raw,
            before,
            candidates,
            mutations,
            append_context,
            source_error: None,
            mixed_ordinary_slots: self.mixed_ordinary_facts.as_ref().map(|_| {
                MixedOrdinaryActionFactsV1 {
                    before: mixed_before,
                    alias: mixed_alias,
                    ..Default::default()
                }
            }),
            actual_fact_slots: actual_before.map(|before| ActualSourceActionFactSlotsV1 {
                before,
                native_alias: actual_alias,
                ..Default::default()
            }),
            executed: false,
            post: None,
            plan: None,
            entry: None,
            receipt: None,
        });
        self.phase = NativeMainActiveWalPhaseV1::Intent;
        Ok(())
    }
    fn pins(&self) -> ActiveWalResult<Vec<ActiveServiceWalBlobV1>> {
        let p = self
            .pending
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let mut refs = vec![
            ActiveServiceWalBlobV1::of(&p.raw)?,
            ActiveServiceWalBlobV1::of(&p.before)?,
            self.origin_pin.clone(),
        ];
        for raw in [&p.candidates, &p.mutations, &p.post, &p.entry]
            .into_iter()
            .flatten()
        {
            refs.push(ActiveServiceWalBlobV1::of(raw)?)
        }
        for raw in [&p.history_before, &p.history_post].into_iter().flatten() {
            refs.push(ActiveServiceWalBlobV1::of(raw)?);
        }
        if let Some(recovery) = &p.native_recovery {
            refs.push(ActiveServiceWalBlobV1::of(&recovery.raw)?);
            if let Some(prior) = &recovery.prior {
                refs.push(prior.clone());
            }
        }
        if let Some(raw) = &self.origin_history {
            refs.push(ActiveServiceWalBlobV1::of(raw)?);
        }
        if let Some(raw) = &p.escrow_chain {
            refs.push(ActiveServiceWalBlobV1::of(raw)?);
        }
        if let Some(prior) = &p.escrow_prior {
            refs.push(prior.clone());
        }
        if let Some(previous) = self.history.last() {
            refs.push(previous.entry.clone())
        }
        if let Some(raw) = self.segmented_ready_origin_blob()? {
            refs.push(ActiveServiceWalBlobV1::of(&raw)?);
        }
        if self.actual_facts.is_some() {
            for raw in self.actual_fact_blobs()? {
                refs.push(ActiveServiceWalBlobV1::of(&raw)?);
            }
            if let Some(prior) = self
                .pending
                .as_ref()
                .and_then(|p| p.actual_fact_slots.as_ref())
                .and_then(|s| s.io_prior.as_ref())
            {
                refs.push(prior.clone());
            }
        }
        for raw in self.mixed_ordinary_blobs()? {
            refs.push(ActiveServiceWalBlobV1::of(&raw)?);
        }
        if let Some(prior) = self
            .pending
            .as_ref()
            .and_then(|p| p.mixed_ordinary_slots.as_ref())
            .and_then(|s| s.io_prior.as_ref())
        {
            refs.push(prior.clone());
        }
        Ok(refs)
    }
    fn record(&self, stage: &str) -> ActiveWalResult<V> {
        let p = self
            .pending
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let mut record = obj(&[
            ("profile", s(self.profile())),
            ("stage", s(stage)),
            ("sequence", s(&self.next_active_ack_sequence()?.to_string())),
            ("previousHead", s(&hex(&self.head))),
            (
                "head",
                p.receipt.as_ref().map_or(V::Null, |r| s(&hex(&r.head))),
            ),
            ("action", ActiveServiceWalBlobV1::of(&p.raw)?.value()),
            ("before", ActiveServiceWalBlobV1::of(&p.before)?.value()),
            ("origin", self.origin_pin.value()),
            (
                "nativeCandidates",
                p.candidates
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(v).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "nativeMutations",
                p.mutations
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(v).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "accepted",
                p.post
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(v).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "entry",
                p.entry
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(v).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
        ]);
        if let Some(recovery) = &p.native_recovery {
            record
                .insert(
                    "nativeRecovery",
                    ActiveServiceWalBlobV1::of(&recovery.raw)?.value(),
                )
                .map_err(active_wal_backend)?;
        }
        for (name, raw) in [
            ("verifiedHistoryBefore", &p.history_before),
            ("verifiedHistoryPost", &p.history_post),
        ] {
            if let Some(raw) = raw {
                record
                    .insert(name, ActiveServiceWalBlobV1::of(raw)?.value())
                    .map_err(active_wal_backend)?;
            }
        }
        if let Some(raw) = &p.escrow_chain {
            record
                .insert("generationEscrow", ActiveServiceWalBlobV1::of(raw)?.value())
                .map_err(active_wal_backend)?;
        }
        if self.actual_facts.is_some() {
            self.append_actual_fact_record_fields(&mut record)?;
        }
        self.append_mixed_ordinary_record(&mut record)?;
        self.append_segment_fields(&mut record)?;
        Ok(record)
    }
    fn write_barrier(&mut self, stage: &str) -> ActiveWalResult<()> {
        #[cfg(test)]
        if self
            .barrier_fault
            .as_ref()
            .is_some_and(|(phase, _)| *phase == self.phase)
        {
            let (_, fault) = self.barrier_fault.take().expect("matching test boundary");
            self.wal.set_test_fault(fault);
        }

        if self
            .pending
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?
            .plan
            .is_none()
        {
            let p = self.pending.as_ref().expect("pending");
            let mut blobs = vec![p.raw.clone(), p.before.clone(), self.origin_archive.clone()];
            if let Some(raw) = self.segmented_ready_origin_blob()? {
                blobs.push(raw);
            }
            if let Some(raw) = &p.escrow_chain {
                blobs.push(raw.clone());
            }
            if let Some(raw) = &self.origin_history {
                blobs.push(raw.clone());
            }
            if let Some(recovery) = &p.native_recovery {
                blobs.push(recovery.raw.clone());
            }
            for raw in [&p.history_before, &p.history_post].into_iter().flatten() {
                blobs.push(raw.clone());
            }
            for raw in [&p.candidates, &p.mutations, &p.post, &p.entry]
                .into_iter()
                .flatten()
            {
                blobs.push(raw.clone())
            }
            self.add_actual_fact_blobs(&mut blobs)?;
            let plan =
                self.wal
                    .prepare(&self.wal.pin()?, self.record(stage)?, blobs, self.pins()?)?;
            self.pending.as_mut().expect("pending").plan = Some(plan);
        }
        let plan = self
            .pending
            .as_mut()
            .expect("pending")
            .plan
            .as_mut()
            .expect("retained plan");
        if plan.attempted() {
            return Err(NativeMainActiveWalErrorV1::NeedWalRecovery);
        }
        self.wal.attempt(plan)?;
        self.pending.as_mut().expect("pending").plan = None;
        Ok(())
    }
    /// Drives WAL barriers and each actual source action once. A pending source
    /// Promise remains the same frame; this method never synchronously drains it.
    pub fn advance(&mut self) -> ActiveWalResult<NativeMainActiveWalPhaseV1> {
        loop {
            if self.actual_facts.as_ref().is_some_and(|f| f.needs_barrier) {
                return Ok(self.phase);
            }
            if self
                .pending
                .as_ref()
                .and_then(|p| p.mixed_ordinary_slots.as_ref())
                .is_some_and(|s| s.attempt.is_some() && !s.delivered)
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            match self.phase {
                NativeMainActiveWalPhaseV1::Idle => return Ok(self.phase),
                NativeMainActiveWalPhaseV1::NativeRecoveryRequired => {
                    return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
                }
                NativeMainActiveWalPhaseV1::NativeSupersessionIntent
                | NativeMainActiveWalPhaseV1::NativeSupersessionAppend => {
                    self.advance_native_recovery()?;
                }
                NativeMainActiveWalPhaseV1::Intent => {
                    self.write_barrier("Intent")?;
                    self.after_raw_action_intent()?;
                }
                NativeMainActiveWalPhaseV1::EscrowPrepare
                | NativeMainActiveWalPhaseV1::EscrowBoundIntent => {
                    self.advance_escrow_barrier()?;
                }
                NativeMainActiveWalPhaseV1::Observe => {
                    if let Some(ActiveWalOwner::Active(service)) = &mut self.owner {
                        if service.observation_pending() {
                            service.retry_observation()?;
                        }
                    }
                    let actual_fields = self.owner_observation()?;
                    let mut bytes = b"TLMAP\x01".to_vec();
                    bytes.extend_from_slice(&(actual_fields.len() as u64).to_be_bytes());
                    bytes.extend_from_slice(&actual_fields);
                    // Distinguish a pre-execution API rejection from a source result.
                    let p = self.pending.as_ref().expect("pending");
                    let rejected = p
                        .source_error
                        .as_ref()
                        .map_or(V::Null, |e| s(&format!("{e:?}")));
                    let mut diagnostic = obj(&[("apiRejection", rejected)]);
                    if let Some(recovery) = &p.native_recovery {
                        diagnostic
                            .insert(
                                "nativeRecovery",
                                ActiveServiceWalBlobV1::of(&recovery.raw)?.value(),
                            )
                            .map_err(active_wal_backend)?;
                    }
                    if let Some(raw) = &p.escrow_chain {
                        diagnostic
                            .insert("generationEscrow", ActiveServiceWalBlobV1::of(raw)?.value())
                            .map_err(active_wal_backend)?;
                    }
                    let suffix = active_wal_bytes(&diagnostic)?;
                    bytes.extend_from_slice(&(suffix.len() as u64).to_be_bytes());
                    bytes.extend_from_slice(&suffix);
                    let history_post = if self.history_enabled {
                        Some(self.capture_history_bytes()?)
                    } else {
                        None
                    };
                    self.pending.as_mut().expect("pending").history_post = history_post;
                    self.pending.as_mut().expect("pending").post = Some(bytes);
                    self.seal_entry()?;
                    self.phase = NativeMainActiveWalPhaseV1::Accepted;
                }
                NativeMainActiveWalPhaseV1::Accepted => {
                    self.write_barrier("Accepted")?;
                    self.phase = NativeMainActiveWalPhaseV1::Ack;
                }
                NativeMainActiveWalPhaseV1::Ack => {
                    self.write_barrier("Ack")?;
                    self.install_ack()?;
                    return Ok(self.phase);
                }
            }
        }
    }
    fn execute_once_unscoped(&mut self) -> ActiveWalResult<()> {
        if self.actual_facts.is_some() {
            return self.execute_actual_fact_once();
        }
        let pending = self
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        if pending.executed {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        // Irreversible execution marker is set before calling the real owner.
        pending.executed = true;
        let action = pending.action.clone();
        match self.owner.take().expect("retained actual owner") {
            ActiveWalOwner::Ready(owner) => {
                let NativeMainActiveServiceActionV1::Start {
                    start,
                    generation,
                    documents,
                } = action
                else {
                    unreachable!("checked constructor action")
                };
                let expected = owner
                    .expectation()
                    .map_err(|e| active_wal_backend(format!("{e:?}")));
                match expected {
                    Err(error) => {
                        self.pending.as_mut().expect("pending").source_error =
                            Some(NativeMainActiveServiceErrorV1::Backend(error.to_string()));
                        self.owner = Some(ActiveWalOwner::Ready(owner));
                    }
                    Ok(expected) => match owner.into_active_service_internal(
                        &expected,
                        start,
                        generation,
                        documents,
                        (self.observation_limit, self.inline_service_enabled()),
                    ) {
                        Ok(active) => self.owner = Some(ActiveWalOwner::Active(Box::new(active))),
                        Err(rejected) => {
                            let RejectedNativeMainActiveServiceV1 { error, owner } = *rejected;
                            self.pending.as_mut().expect("pending").source_error = Some(error);
                            self.owner = Some(ActiveWalOwner::Ready(Box::new(owner)));
                        }
                    },
                }
            }
            ActiveWalOwner::Active(mut owner) => {
                let result = owner.expectation().and_then(|expected| {
                    if self.generation_escrow.is_some()
                        && matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                    {
                        owner
                            .apply_escrow_append(
                                &expected,
                                self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
                            )
                            .map(|_| ())
                    } else {
                        owner.apply(&expected, action).map(|_| ())
                    }
                });
                if let Err(error) = result {
                    if !owner.observation_pending() {
                        self.pending.as_mut().expect("pending").source_error = Some(error);
                    }
                }
                self.owner = Some(ActiveWalOwner::Active(owner));
            }
        }
        if self
            .pending
            .as_ref()
            .expect("pending")
            .source_error
            .is_none()
            && matches!(
                self.pending.as_ref().expect("pending").action,
                NativeMainActiveServiceActionV1::AppendPending
            )
            && self.active_service().is_some_and(|s| {
                matches!(
                    s.pending_observation
                        .as_ref()
                        .map(|p| &p.result)
                        .or_else(|| s.history().last().map(|r| &r.result)),
                    Some(NativeMainActiveServiceActionResultV1::Stop(
                        NativeMainServiceStopV1::StorageStopped(_)
                    ))
                )
            })
        {
            self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
            return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
        }
        Ok(())
    }
    fn seal_entry(&mut self) -> ActiveWalResult<()> {
        let p = self.pending.as_ref().expect("pending");
        let action = ActiveServiceWalBlobV1::of(&p.raw)?;
        let before = ActiveServiceWalBlobV1::of(&p.before)?;
        let accepted =
            ActiveServiceWalBlobV1::of(p.post.as_deref().expect("actual observed post"))?;
        let native_candidates = p
            .candidates
            .as_ref()
            .map(|v| ActiveServiceWalBlobV1::of(v))
            .transpose()?;
        let native_mutations = p
            .mutations
            .as_ref()
            .map(|v| ActiveServiceWalBlobV1::of(v))
            .transpose()?;
        let sequence = self.next_active_ack_sequence()?;
        let previous = self.active_previous_entry();
        let mut entry_value = obj(&[
            ("profile", s(self.profile())),
            ("sequence", s(&sequence.to_string())),
            ("previousEntry", previous),
            ("previousHead", s(&hex(&self.head))),
            ("action", action.value()),
            ("before", before.value()),
            (
                "nativeCandidates",
                native_candidates.as_ref().map_or(V::Null, active_wal_pin),
            ),
            (
                "nativeMutations",
                native_mutations.as_ref().map_or(V::Null, active_wal_pin),
            ),
            ("accepted", accepted.value()),
        ]);
        if let Some(recovery) = &p.native_recovery {
            entry_value
                .insert(
                    "nativeRecovery",
                    ActiveServiceWalBlobV1::of(&recovery.raw)?.value(),
                )
                .map_err(active_wal_backend)?;
        }
        for (name, raw) in [
            ("verifiedHistoryBefore", &p.history_before),
            ("verifiedHistoryPost", &p.history_post),
        ] {
            if let Some(raw) = raw {
                entry_value
                    .insert(name, ActiveServiceWalBlobV1::of(raw)?.value())
                    .map_err(active_wal_backend)?;
            }
        }
        if let Some(raw) = &p.escrow_chain {
            entry_value
                .insert("generationEscrow", ActiveServiceWalBlobV1::of(raw)?.value())
                .map_err(active_wal_backend)?;
        }
        if self.actual_facts.is_some() {
            self.append_actual_fact_record_fields(&mut entry_value)?;
        }
        self.append_mixed_ordinary_record(&mut entry_value)?;
        self.append_segment_fields(&mut entry_value)?;
        let raw = active_wal_bytes(&entry_value)?;
        let entry = ActiveServiceWalBlobV1::of(&raw)?;
        let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
        chain.extend_from_slice(&self.head);
        chain.extend_from_slice(&sequence.to_be_bytes());
        chain.extend_from_slice(&entry.sha256);
        let receipt = NativeMainActiveWalReceiptV1 {
            sequence,
            action,
            before,
            native_candidates,
            native_mutations,
            accepted,
            entry,
            previous_head: self.head,
            head: sha256(&chain),
        };
        let p = self.pending.as_mut().expect("pending");
        p.entry = Some(raw);
        p.receipt = Some(receipt);
        Ok(())
    }
    fn install_ack(&mut self) -> ActiveWalResult<()> {
        let p = self
            .pending
            .take()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let receipt = p.receipt.expect("sealed before ACK");
        if matches!(p.action, NativeMainActiveServiceActionV1::AppendPending)
            && p.source_error.is_none()
        {
            self.acknowledged_append = p.append_context;
        }
        if matches!(p.action, NativeMainActiveServiceActionV1::ResolvePost)
            && p.source_error.is_none()
        {
            self.acknowledged_append = None;
        }
        self.head = receipt.head;
        self.history.push(receipt);
        self.phase = NativeMainActiveWalPhaseV1::Idle;
        Ok(())
    }
    /// Reconciles only actual control-store bytes. Source/action is never retried.
    pub fn resolve_control_write(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        let p = self
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let plan = p
            .plan
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let resolution = self.wal.resolve(plan)?;
        match resolution {
            ActiveServiceWalResolvedV1::Before => {
                p.plan = Some(self.wal.rearm_before(plan)?);
            }
            ActiveServiceWalResolvedV1::Post => {
                p.plan = None;
                match self.phase {
                    NativeMainActiveWalPhaseV1::NativeSupersessionIntent => {
                        self.after_native_supersession_intent()?;
                    }
                    NativeMainActiveWalPhaseV1::EscrowBoundIntent => {
                        self.execute_escrow_route()?;
                    }
                    NativeMainActiveWalPhaseV1::Intent => {
                        self.after_raw_action_intent()?;
                    }
                    NativeMainActiveWalPhaseV1::Accepted => {
                        self.phase = NativeMainActiveWalPhaseV1::Ack;
                    }
                    NativeMainActiveWalPhaseV1::Ack => self.install_ack()?,
                    _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
                }
            }
        }
        Ok(resolution)
    }
}
