// Explicit paired-WAL coordinator. The inner actual financial owner never escapes.
pub use crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1;
pub const NATIVE_MAIN_DURABLE_FRESH_PROFILE: &str = "TL-MAIN-DURABLE-FRESH-FACTUAL-INPUT-13-1";
const MAX_DURABLE_FRESH_ACTIONS: usize = 4096;
const MAX_DURABLE_FRESH_RAW_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMainDurableFreshPhaseV1 {
    Idle,
    Intent,
    Execute,
    SourceInputsCaptured,
    Observe,
    Accepted,
    Ack,
    NativeRecoveryRequired,
}
#[derive(Debug)]
pub enum NativeMainDurableFreshErrorV1 {
    Wal(ActiveServiceWalErrorV1),
    Source(NativeMainActiveWalErrorV1),
    Service(NativeMainActiveServiceErrorV1),
    Busy,
    NoAction,
    NeedControlRecovery,
    NativeRecoveryRequired,
    WrongContext,
    Capacity,
}
impl core::fmt::Display for NativeMainDurableFreshErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NativeMainDurableFreshErrorV1 {}
impl From<ActiveServiceWalErrorV1> for NativeMainDurableFreshErrorV1 {
    fn from(e: ActiveServiceWalErrorV1) -> Self {
        Self::Wal(e)
    }
}
impl From<NativeMainActiveWalErrorV1> for NativeMainDurableFreshErrorV1 {
    fn from(e: NativeMainActiveWalErrorV1) -> Self {
        Self::Source(e)
    }
}
impl From<NativeMainActiveServiceErrorV1> for NativeMainDurableFreshErrorV1 {
    fn from(e: NativeMainActiveServiceErrorV1) -> Self {
        Self::Service(e)
    }
}
type DurableFreshResult<T> = Result<T, NativeMainDurableFreshErrorV1>;
pub struct RejectedNativeMainDurableFreshV1<T> {
    pub cause: String,
    owner: T,
}
impl<T> RejectedNativeMainDurableFreshV1<T> {
    pub fn into_owner(self) -> T {
        self.owner
    }
}
impl<T> core::fmt::Debug for RejectedNativeMainDurableFreshV1<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainDurableFreshV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainDurableFreshReceiptV1 {
    pub sequence: u64,
    pub action: ActiveServiceWalBlobV1,
    pub before: ActiveServiceWalBlobV1,
    pub after: ActiveServiceWalBlobV1,
    pub source_before: ActiveServiceWalBlobV1,
    pub source_after: ActiveServiceWalBlobV1,
    pub entry: ActiveServiceWalBlobV1,
    pub outcome: V,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
}
struct DurableFreshSourceBindingV1 {
    raw: Vec<u8>,
    source: Vec<u8>,
    metadata: Vec<u8>,
    pending: Option<Vec<u8>>,
}
impl DurableFreshSourceBindingV1 {
    fn blobs(&self) -> Vec<Vec<u8>> {
        let mut raw = vec![self.raw.clone(), self.source.clone(), self.metadata.clone()];
        if let Some(p) = &self.pending {
            raw.push(p.clone());
        }
        raw
    }
}
struct DurableFreshObservationV1 {
    raw: Vec<u8>,
    source: Vec<u8>,
}
struct DurableFreshPendingV1 {
    action: NativeMainDurableFreshActionV1,
    raw: Vec<u8>,
    before_binding: DurableFreshSourceBindingV1,
    before: DurableFreshObservationV1,
    after_binding: Option<DurableFreshSourceBindingV1>,
    after: Option<DurableFreshObservationV1>,
    executed: bool,
    source_prepared: bool,
    outcome: Option<V>,
    plan: Option<PreparedActiveServiceWalWriteV1>,
    entry: Option<Vec<u8>>,
    receipt: Option<NativeMainDurableFreshReceiptV1>,
}
struct DurableFreshCoordinatorV1 {
    wal: ActiveServiceWalV1,
    phase: NativeMainDurableFreshPhaseV1,
    pending: Option<DurableFreshPendingV1>,
    origin_binding: DurableFreshSourceBindingV1,
    origin_observation: DurableFreshObservationV1,
    origin_archive: Vec<u8>,
    history: Vec<NativeMainDurableFreshReceiptV1>,
    head: [u8; 32],
    feeds: Vec<NativeMainFreshInputReceiptV1>,
    feed_head: [u8; 32],
    raw_bytes: usize,
    observation_limit: usize,
    #[cfg(test)]
    barrier_fault: Option<(
        NativeMainDurableFreshPhaseV1,
        crate::source_main_active_service_wal::TestFault,
    )>,
}
pub struct SourceMainNativeDurableFreshInputV1 {
    source: SourceMainNativeActualFactServiceV1,
    control: DurableFreshCoordinatorV1,
}
fn durable_fresh_bytes(v: &V) -> DurableFreshResult<Vec<u8>> {
    v.diagnostic_json()
        .map(String::into_bytes)
        .map_err(|_| NativeMainDurableFreshErrorV1::WrongContext)
}
fn durable_fresh_binding(
    source: &mut SourceMainNativeActualFactServiceV1,
) -> DurableFreshResult<DurableFreshSourceBindingV1> {
    if source.phase() != NativeMainActiveWalPhaseV1::Idle
        || source.fact_barrier_pending()
        || source.deferred_error.is_some()
    {
        return Err(NativeMainDurableFreshErrorV1::Busy);
    }
    // Actual plan producer runs BEFORE source observations; no decoded fact is a permit.
    let input = source.inner.capture_actual_inline_fact_input()?;
    let actual = source
        .inner
        .wal
        .capture_actual_durable_fresh_source_boundary()?;
    let metadata = source
        .inner
        .index()
        .capture_verified_history_pin()
        .map_err(active_wal_backend)?
        .canonical_scalar_bytes()
        .map_err(active_wal_backend)?;
    let raw = durable_fresh_bytes(&obj(&[
        ("profile", s("TL-MAIN-DURABLE-FRESH-ACTUAL-RECEIVER-13-1")),
        (
            "sourcePrefix",
            ActiveServiceWalBlobV1::of(actual.canonical_boundary_bytes())?.value(),
        ),
        ("metadata", ActiveServiceWalBlobV1::of(&metadata)?.value()),
        (
            "pendingPlan",
            input
                .plan
                .as_ref()
                .map(|v| ActiveServiceWalBlobV1::of(v).map(|p| p.value()))
                .transpose()?
                .unwrap_or(V::Null),
        ),
        ("selection", input.selection.unwrap_or(V::Null)),
    ]))?;
    Ok(DurableFreshSourceBindingV1 {
        raw,
        source: actual.canonical_boundary_bytes().to_vec(),
        metadata,
        pending: input.plan,
    })
}
fn durable_fresh_observation(
    source: &SourceMainNativeActualFactServiceV1,
    feed_head: &[u8; 32],
    feeds: usize,
    limit: usize,
) -> DurableFreshResult<DurableFreshObservationV1> {
    let observed = source.inner.observation()?;
    let raw = durable_fresh_bytes(&obj(&[
        ("profile", s(NATIVE_MAIN_DURABLE_FRESH_PROFILE)),
        (
            "sourceObservation",
            ActiveServiceWalBlobV1::of(&observed)?.value(),
        ),
        ("feeds", s(&feeds.to_string())),
        ("feedHead", s(&hex(feed_head))),
    ]))?;
    if observed.len() > limit || raw.len() > limit {
        return Err(NativeMainDurableFreshErrorV1::Capacity);
    }
    Ok(DurableFreshObservationV1 {
        raw,
        source: observed,
    })
}
impl SourceMainNativeDurableFreshInputV1 {
    pub fn adopt(
        mut source: SourceMainNativeActualFactServiceV1,
        root: &std::path::Path,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainDurableFreshV1<SourceMainNativeActualFactServiceV1>>>
    {
        let checked = (|| {
            if observation_limit == 0 || observation_limit > ACTIVE_WAL_OBSERVATION_LIMIT {
                return Err(NativeMainDurableFreshErrorV1::Capacity);
            }
            let actual_root = root
                .canonicalize()
                .map_err(|e| NativeMainDurableFreshErrorV1::Wal(e.into()))?;
            if actual_root == source.wal_pin()?.root {
                return Err(NativeMainDurableFreshErrorV1::WrongContext);
            }
            let binding = durable_fresh_binding(&mut source)?;
            // Persist actual raw origin even if a Feed API call is rejected
            // before the child has written its first SourceAction intent.
            let origin_archive = source.inner.origin_archive.clone();
            ActiveServiceWalBlobV1::of(&origin_archive)?;
            let feed_head = sha256(NATIVE_MAIN_DURABLE_FRESH_PROFILE.as_bytes());
            let observation = durable_fresh_observation(&source, &feed_head, 0, observation_limit)?;
            let origin = obj(&[
                ("profile", s(NATIVE_MAIN_DURABLE_FRESH_PROFILE)),
                ("rawActionCodec", s("TLMDFI1")),
                ("sourceFactsProfile", s(source.profile())),
                (
                    "sourceArchive",
                    ActiveServiceWalBlobV1::of(&origin_archive)?.value(),
                ),
                (
                    "sourceBoundary",
                    ActiveServiceWalBlobV1::of(&binding.raw)?.value(),
                ),
                (
                    "sourceObservation",
                    ActiveServiceWalBlobV1::of(&observation.raw)?.value(),
                ),
                ("feedHead", s(&hex(&feed_head))),
            ]);
            let head = sha256(&durable_fresh_bytes(&origin)?);
            let wal = ActiveServiceWalV1::create(root, namespace, origin, filesystem)?;
            Ok(DurableFreshCoordinatorV1 {
                wal,
                phase: NativeMainDurableFreshPhaseV1::Idle,
                pending: None,
                origin_binding: binding,
                origin_observation: observation,
                origin_archive,
                history: vec![],
                head,
                feeds: vec![],
                feed_head,
                raw_bytes: 0,
                observation_limit,
                #[cfg(test)]
                barrier_fault: None,
            })
        })();
        match checked {
            Ok(control) => Ok(Self { source, control }),
            Err(error) => Err(Box::new(RejectedNativeMainDurableFreshV1 {
                cause: error.to_string(),
                owner: source,
            })),
        }
    }
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_DURABLE_FRESH_PROFILE
    }
    pub fn phase(&self) -> NativeMainDurableFreshPhaseV1 {
        self.control.phase
    }
    pub fn head(&self) -> [u8; 32] {
        self.control.head
    }
    pub fn history(&self) -> &[NativeMainDurableFreshReceiptV1] {
        &self.control.history
    }
    pub fn feeds(&self) -> &[NativeMainFreshInputReceiptV1] {
        &self.control.feeds
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.source.inner.financial_runtime().state_value()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.source.inner.index()
    }
    pub fn wal_pin(&self) -> DurableFreshResult<ActiveServiceWalPinV1> {
        Ok(self.control.wal.pin()?)
    }
    pub fn source_wal_pin(&self) -> DurableFreshResult<ActiveServiceWalPinV1> {
        Ok(self.source.wal_pin()?)
    }
    pub fn prepare_action(
        &mut self,
        action: NativeMainDurableFreshActionV1,
    ) -> DurableFreshResult<()> {
        if self.control.phase != NativeMainDurableFreshPhaseV1::Idle
            || self.control.pending.is_some()
        {
            return Err(NativeMainDurableFreshErrorV1::Busy);
        }
        if self.control.history.len() >= MAX_DURABLE_FRESH_ACTIONS {
            return Err(NativeMainDurableFreshErrorV1::Capacity);
        }
        let raw = action.canonical_action_bytes().map_err(|error| {
            if error == SourceRuleError::Capacity {
                NativeMainDurableFreshErrorV1::Capacity
            } else {
                NativeMainDurableFreshErrorV1::WrongContext
            }
        })?;
        if self
            .control
            .raw_bytes
            .checked_add(raw.len())
            .is_none_or(|v| v > MAX_DURABLE_FRESH_RAW_BYTES)
        {
            return Err(NativeMainDurableFreshErrorV1::Capacity);
        }
        if matches!(action, NativeMainDurableFreshActionV1::Feed(_)) {
            if self.control.feeds.len() >= MAX_FRESH_INPUT_FEEDS {
                return Err(NativeMainDurableFreshErrorV1::Capacity);
            }
            self.control
                .feeds
                .try_reserve(1)
                .map_err(|_| NativeMainDurableFreshErrorV1::Capacity)?;
        }
        self.control
            .history
            .try_reserve(1)
            .map_err(|_| NativeMainDurableFreshErrorV1::Capacity)?;
        let before_binding = durable_fresh_binding(&mut self.source)?;
        let before = durable_fresh_observation(
            &self.source,
            &self.control.feed_head,
            self.control.feeds.len(),
            self.control.observation_limit,
        )?;
        self.control.pending = Some(DurableFreshPendingV1 {
            action,
            raw,
            before_binding,
            before,
            after_binding: None,
            after: None,
            executed: false,
            source_prepared: false,
            outcome: None,
            plan: None,
            entry: None,
            receipt: None,
        });
        self.control.phase = NativeMainDurableFreshPhaseV1::Intent;
        Ok(())
    }
}
