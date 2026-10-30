// New explicit ordinary-HISTORY actor. Old finite/source/13-2 readers are not
// upgraded. No financial/source action is repeated by a segment checkpoint.
use crate::source_main_active_service_wal::{
    ActiveHistorySegmentCursorV1, PreparedActiveHistoryCheckpointV1,
    SyncedActiveHistoryCheckpointV1, SEGMENTED_ACTIVE_HISTORY_PROFILE,
};
const ACTIVE_ACK_SEGMENT_WINDOW: usize = 256;
#[derive(Default)]
struct ActiveHistoryLiveStateV1 {
    archived: Option<ActiveHistorySegmentCursorV1>,
}
struct ActiveHistoryCheckpointPendingV1 {
    plan: PreparedActiveHistoryCheckpointV1,
    actual_owner_before: Vec<u8>,
    source_head: V,
    wal_head: [u8; 32],
    window_count: usize,
}
/// Owns the one actual running owner and its WAL for the whole history. There
/// is no conversion to the finite controller, owner release or mutable accessor.
pub struct SourceMainNativeSegmentedActiveServiceV1 {
    controller: SourceMainNativeDurableActiveServiceV1,
    checkpoint: Option<ActiveHistoryCheckpointPendingV1>,
}
impl SourceMainNativeDurableActiveServiceV1 {
    fn active_ack_window_limit(&self) -> usize {
        if self.segmented_history.is_some() {
            ACTIVE_ACK_SEGMENT_WINDOW
        } else {
            MAX_SERVICE_ACTIONS
        }
    }
    fn archived_active_actions(&self) -> u64 {
        self.segmented_history
            .as_ref()
            .and_then(|s| s.archived.as_ref())
            .map_or(0, |s| s.actions)
    }
    fn next_active_ack_sequence(&self) -> ActiveWalResult<u64> {
        self.archived_active_actions()
            .checked_add(self.history.len() as u64)
            .and_then(|n| n.checked_add(1))
            .ok_or(NativeMainActiveWalErrorV1::Capacity)
    }
    fn active_previous_entry(&self) -> V {
        self.history
            .last()
            .map(|r| r.entry.value())
            .or_else(|| {
                self.segmented_history
                    .as_ref()
                    .and_then(|s| s.archived.as_ref())
                    .map(|s| s.last_entry.value())
            })
            .unwrap_or(V::Null)
    }
    fn actual_active_source_head(&self) -> V {
        self.active_service().map_or(V::Null, |service| {
            obj(&[
                ("actions", s(&service.head.actions.to_string())),
                ("commitment", s(&hex(&service.head.commitment))),
            ])
        })
    }
    fn append_segment_fields(&self, value: &mut V) -> ActiveWalResult<()> {
        if let Some(state) = &self.segmented_history {
            value
                .insert("sourceHead", self.actual_active_source_head())
                .map_err(active_wal_backend)?;
            value
                .insert(
                    "historySegment",
                    state
                        .archived
                        .as_ref()
                        .map_or(V::Null, |s| s.segment.value()),
                )
                .map_err(active_wal_backend)?;
        }
        Ok(())
    }
    fn append_segment_observation(&self, raw: &mut Vec<u8>) -> ActiveWalResult<()> {
        let Some(state) = &self.segmented_history else {
            return Ok(());
        };
        let mut fields =
            NativeFieldCollector::new("Native13/SegmentedActiveHistory/v1", true, 4096);
        fields
            .emit("profile", &s(SEGMENTED_ACTIVE_HISTORY_PROFILE))
            .map_err(active_wal_backend)?;
        fields
            .emit(
                "archivedActions",
                &s(&self.archived_active_actions().to_string()),
            )
            .map_err(active_wal_backend)?;
        fields
            .emit("acknowledgedWindow", &s(&self.history.len().to_string()))
            .map_err(active_wal_backend)?;
        native_step_value_fields("sourceHead", &self.actual_active_source_head(), &mut fields)
            .map_err(active_wal_backend)?;
        native_step_value_fields(
            "previousSegment",
            &state
                .archived
                .as_ref()
                .map_or(V::Null, |s| s.segment.value()),
            &mut fields,
        )
        .map_err(active_wal_backend)?;
        raw.extend_from_slice(fields.frames.as_deref().expect("retained actual fields"));
        if raw.len() > self.observation_limit {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        Ok(())
    }
}
impl SourceMainNativeSegmentedActiveServiceV1 {
    /// New origin is durable before Start. The underlying source is the ordinary
    /// verified-history entry; single-inline/13-2 fact producers are unadmitted.
    pub fn create(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        SourceMainNativeDurableActiveServiceV1::create_internal_segmented_entry(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            true,
            (None, false, true),
        )
        .map(|controller| Self {
            controller,
            checkpoint: None,
        })
    }
    pub fn profile(&self) -> &'static str {
        SEGMENTED_ACTIVE_HISTORY_PROFILE
    }
    pub fn phase(&self) -> NativeMainActiveWalPhaseV1 {
        self.controller.phase()
    }
    pub fn checkpoint_pending(&self) -> bool {
        self.checkpoint.is_some()
    }
    pub fn archived(&self) -> Option<&ActiveHistorySegmentCursorV1> {
        self.controller
            .segmented_history
            .as_ref()
            .and_then(|s| s.archived.as_ref())
    }
    pub fn history_window(&self) -> &[NativeMainActiveWalReceiptV1] {
        self.controller.history()
    }
    pub fn acknowledged_actions(&self) -> u64 {
        self.controller.archived_active_actions() + self.controller.history.len() as u64
    }
    pub fn head(&self) -> [u8; 32] {
        self.controller.head()
    }
    pub fn wal_pin(&self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        self.controller.wal_pin()
    }
    pub fn active_service(&self) -> Option<&SourceMainNativeActiveServiceV1> {
        self.controller.active_service()
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.controller.financial_runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.controller.index()
    }
    pub fn observation(&self) -> ActiveWalResult<Vec<u8>> {
        self.controller.observation()
    }
    pub fn read_history_blob(&self, pin: &ActiveServiceWalBlobV1) -> ActiveWalResult<Vec<u8>> {
        self.controller.read_history_blob(pin)
    }
    fn require_action_route(&self) -> ActiveWalResult<()> {
        if self.checkpoint.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        Ok(())
    }
    pub fn prepare_action(
        &mut self,
        action: NativeMainActiveServiceActionV1,
    ) -> ActiveWalResult<()> {
        self.require_action_route()?;
        self.controller.prepare_action(action)
    }
    pub fn advance(&mut self) -> ActiveWalResult<NativeMainActiveWalPhaseV1> {
        self.require_action_route()?;
        self.controller.advance()
    }
    pub fn resolve_control_write(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        self.require_action_route()?;
        self.controller.resolve_control_write()
    }
    /// Pure prepare. The actual source frame, native plan/token/index and queue
    /// remain held. Source cannot execute until this metadata operation resolves.
    pub fn prepare_checkpoint(&mut self) -> ActiveWalResult<()> {
        self.require_action_route()?;
        let c = &mut self.controller;
        if c.phase != NativeMainActiveWalPhaseV1::Idle
            || c.pending.is_some()
            || c.history.is_empty()
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        if c.active_service().is_some_and(|s| s.observation_pending()) {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let actual_owner_before = c.owner_observation()?;
        let source_head = c.actual_active_source_head();
        let entries = c
            .history
            .iter()
            .map(|r| r.entry.clone())
            .collect::<Vec<_>>();
        let previous = c
            .segmented_history
            .as_ref()
            .and_then(|s| s.archived.as_ref());
        let expected = c.wal.pin()?;
        let plan = c
            .wal
            .prepare_acknowledged_history_checkpoint(&expected, previous, &entries)?;
        self.checkpoint = Some(ActiveHistoryCheckpointPendingV1 {
            plan,
            actual_owner_before,
            source_head,
            wal_head: c.head,
            window_count: c.history.len(),
        });
        Ok(())
    }
    fn require_checkpoint_source_unchanged(&self) -> ActiveWalResult<()> {
        let pending = self
            .checkpoint
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let c = &self.controller;
        if c.phase != NativeMainActiveWalPhaseV1::Idle
            || c.pending.is_some()
            || c.head != pending.wal_head
            || c.history.len() != pending.window_count
            || c.actual_active_source_head() != pending.source_head
            || c.owner_observation()? != pending.actual_owner_before
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        Ok(())
    }
    pub fn attempt_checkpoint(&mut self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        self.require_checkpoint_source_unchanged()?;
        let p = self.checkpoint.as_mut().expect("checked pending");
        Ok(self
            .controller
            .wal
            .attempt_acknowledged_history_checkpoint(&mut p.plan)?)
    }
    pub fn resolve_checkpoint(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        self.require_checkpoint_source_unchanged()?;
        let p = self.checkpoint.as_mut().expect("checked pending");
        let resolution = self
            .controller
            .wal
            .resolve_acknowledged_history_checkpoint(&p.plan)?;
        if resolution == ActiveServiceWalResolvedV1::Before {
            p.plan = self
                .controller
                .wal
                .rearm_acknowledged_history_checkpoint(&p.plan)?;
        }
        Ok(resolution)
    }
    /// Exact Post only. The proof comes from actual same-WAL ACK+trace checks,
    /// not a cursor supplied by the caller. Receipt memory changes last.
    pub fn finish_checkpoint(&mut self) -> ActiveWalResult<&ActiveHistorySegmentCursorV1> {
        self.require_checkpoint_source_unchanged()?;
        let p = self.checkpoint.as_mut().expect("checked pending");
        let proof = self
            .controller
            .wal
            .finish_acknowledged_history_checkpoint(&mut p.plan)?;
        self.install_checkpoint(proof)?;
        self.checkpoint = None;
        Ok(self.archived().expect("installed actual cursor"))
    }
    fn install_checkpoint(
        &mut self,
        proof: SyncedActiveHistoryCheckpointV1,
    ) -> ActiveWalResult<()> {
        self.controller
            .wal
            .require_synced_history_checkpoint(&proof)?;
        let cursor = proof.cursor();
        let c = &mut self.controller;
        if cursor.actions != c.archived_active_actions() + c.history.len() as u64
            || cursor.wal_head != c.head
            || cursor.source_head != c.actual_active_source_head()
            || c.history.last().map(|r| &r.entry) != Some(&cursor.last_entry)
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        // This changes receipt RAM only, after every check and ACK. Source head,
        // financial Core/effects, Kernel, native frame/plan and index are untouched.
        if let Some(ActiveWalOwner::Active(service)) = c.owner.as_mut() {
            service.history.clear();
        }
        c.history.clear();
        c.segmented_history
            .as_mut()
            .expect("new origin only")
            .archived = Some(cursor.clone());
        Ok(())
    }
}
/// Typed native recovery keeps the whole segmented controller/cursor and the
/// same financial/source frame. It cannot expose the finite controller.
pub struct SourceMainNativeSegmentedServiceNoIndexV1 {
    owner: SourceMainNativeDurableServiceNoIndexV1,
}
pub struct RejectedSegmentedNoIndexV1 {
    pub cause: String,
    owner: SourceMainNativeSegmentedActiveServiceV1,
}
impl RejectedSegmentedNoIndexV1 {
    pub fn into_owner(self) -> SourceMainNativeSegmentedActiveServiceV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedSegmentedNoIndexV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedSegmentedNoIndexV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedSegmentedReopenV1 {
    pub cause: String,
    owner: SourceMainNativeSegmentedServiceNoIndexV1,
}
impl RejectedSegmentedReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeSegmentedServiceNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedSegmentedReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedSegmentedReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeSegmentedActiveServiceV1 {
    pub fn into_no_native_index(
        self,
    ) -> Result<SourceMainNativeSegmentedServiceNoIndexV1, Box<RejectedSegmentedNoIndexV1>> {
        if self.checkpoint.is_some() {
            return Err(Box::new(RejectedSegmentedNoIndexV1 {
                cause: "checkpoint must resolve before native recovery".into(),
                owner: self,
            }));
        }
        match self.controller.into_no_native_index() {
            Ok(owner) => Ok(SourceMainNativeSegmentedServiceNoIndexV1 { owner }),
            Err(rejected) => {
                let cause = rejected.cause.clone();
                let controller = rejected.into_controller();
                Err(Box::new(RejectedSegmentedNoIndexV1 {
                    cause,
                    owner: Self {
                        controller,
                        checkpoint: None,
                    },
                }))
            }
        }
    }
}
impl SourceMainNativeSegmentedServiceNoIndexV1 {
    pub fn profile(&self) -> &'static str {
        SEGMENTED_ACTIVE_HISTORY_PROFILE
    }
    pub fn archived(&self) -> Option<&ActiveHistorySegmentCursorV1> {
        self.owner
            .controller
            .segmented_history
            .as_ref()
            .and_then(|s| s.archived.as_ref())
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial_runtime()
    }
    pub fn effects(&self) -> &[V] {
        self.owner.effects()
    }
    pub fn pending_write(&self) -> &crate::source_compact_index_v2::NativePreparedValidityWriteV1 {
        self.owner.pending_write()
    }
    pub fn try_reopen(
        self,
        root: &std::path::Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<SourceMainNativeSegmentedActiveServiceV1, Box<RejectedSegmentedReopenV1>> {
        match self.owner.try_reopen(root, candidate, recovery) {
            Ok(controller) => Ok(SourceMainNativeSegmentedActiveServiceV1 {
                controller,
                checkpoint: None,
            }),
            Err(rejected) => {
                let cause = rejected.cause.clone();
                let owner = rejected.into_owner();
                Err(Box::new(RejectedSegmentedReopenV1 {
                    cause,
                    owner: Self { owner },
                }))
            }
        }
    }
}
