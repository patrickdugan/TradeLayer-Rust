/// Explicit mixed fact/segmented entry. The real source owner is private for
/// its whole lifetime, including ACK, native/WAL recovery and checkpoints.
pub struct SourceMainNativeMixedFactServiceV1 {
    inner: Option<SourceMainNativeActualFactServiceV1>,
    checkpoint: Option<ActiveHistoryCheckpointPendingV1>,
}
impl SourceMainNativeMixedFactServiceV1 {
    pub fn create(
        owner: SourceMainNativeSessionV13,
        wal_root: impl AsRef<Path>,
        namespace: Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        escrow_root: PathBuf,
        durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        // Same actual-directory/config checks as the existing escrow entry.
        // No source invocation, financial clone or speculative Kernel is made.
        let checked = (|| -> ActiveWalResult<ActiveServiceEscrowConfigV1> {
            let metadata = std::fs::symlink_metadata(&escrow_root).map_err(active_wal_backend)?;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || !escrow_root.is_absolute()
                || escrow_root.canonicalize().map_err(active_wal_backend)? != escrow_root
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
            }
            let (_, catalog) = owner
                .index()
                .native_store_context()
                .map_err(active_wal_backend)?;
            if catalog.starts_with(&escrow_root) || escrow_root.starts_with(&catalog) {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            #[cfg(not(unix))]
            if durability
                == crate::compact_journal::GenerationEscrowDurabilityV1::FileAndDirectorySync
            {
                return Err(NativeMainActiveWalErrorV1::Backend(
                    "directory sync unavailable for explicit escrow mode".into(),
                ));
            }
            Ok(ActiveServiceEscrowConfigV1 {
                root: escrow_root,
                durability,
                inline_profile: true,
            })
        })();
        let config = match checked {
            Ok(config) => config,
            Err(error) => return Err(Box::new(RejectedNativeMainActiveWalV1 { error, owner })),
        };
        SourceMainNativeDurableActiveServiceV1::create_internal_mixed_fact_entry(
            owner,
            wal_root,
            namespace,
            filesystem,
            observation_limit,
            true,
            (Some(config), true, true, true),
        )
        .map(|inner| Self {
            inner: Some(SourceMainNativeActualFactServiceV1 {
                inner,
                fact_barrier: false,
                deferred_error: None,
            }),
            checkpoint: None,
        })
    }
    fn actual(&self) -> &SourceMainNativeActualFactServiceV1 {
        self.inner.as_ref().expect("same whole actual fact owner")
    }
    fn actual_mut(&mut self) -> &mut SourceMainNativeActualFactServiceV1 {
        self.inner.as_mut().expect("same whole actual fact owner")
    }
    fn require_action_route(&self) -> ActiveWalResult<()> {
        if self.checkpoint.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        Ok(())
    }
    pub fn profile(&self) -> &'static str {
        MIXED_SOURCE_FACT_WAL_PROFILE
    }
    pub fn phase(&self) -> NativeMainActiveWalPhaseV1 {
        self.actual().phase()
    }
    pub fn fact_barrier_pending(&self) -> bool {
        self.actual().fact_barrier_pending()
    }
    pub fn checkpoint_pending(&self) -> bool {
        self.checkpoint.is_some()
    }
    pub fn head(&self) -> [u8; 32] {
        self.actual().head()
    }
    pub fn acknowledged_actions(&self) -> u64 {
        let c = &self.actual().inner;
        c.archived_active_actions() + c.history.len() as u64
    }
    pub fn history_window(&self) -> &[NativeMainActiveWalReceiptV1] {
        self.actual().history()
    }
    pub fn archived(&self) -> Option<&ActiveHistorySegmentCursorV1> {
        self.actual()
            .inner
            .segmented_history
            .as_ref()
            .and_then(|s| s.archived.as_ref())
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.actual().inner.financial_runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.actual().inner.index()
    }
    pub fn active_service(&self) -> Option<&SourceMainNativeActiveServiceV1> {
        self.actual().inner.active_service()
    }
    pub fn wal_pin(&self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        self.actual().wal_pin()
    }
    pub fn read_history_blob(&self, pin: &ActiveServiceWalBlobV1) -> ActiveWalResult<Vec<u8>> {
        self.actual().read_history_blob(pin)
    }
    pub fn prepare_action(
        &mut self,
        action: NativeMainActiveServiceActionV1,
    ) -> ActiveWalResult<()> {
        self.require_action_route()?;
        self.actual_mut().prepare_action(action)
    }
    pub fn advance(&mut self) -> ActiveWalResult<NativeMainActiveWalPhaseV1> {
        self.require_action_route()?;
        self.actual_mut().advance()
    }
    pub fn resolve_control_write(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        self.require_action_route()?;
        self.actual_mut().resolve_control_write()
    }
    fn with_segmented<T>(
        &mut self,
        operation: impl FnOnce(&mut SourceMainNativeSegmentedActiveServiceV1) -> ActiveWalResult<T>,
    ) -> ActiveWalResult<T> {
        if self.fact_barrier_pending() || self.actual().deferred_error.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let SourceMainNativeActualFactServiceV1 {
            inner,
            fact_barrier,
            deferred_error,
        } = self.inner.take().expect("same opaque actual owner");
        let mut segment = SourceMainNativeSegmentedActiveServiceV1 {
            controller: inner,
            checkpoint: self.checkpoint.take(),
        };
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(&mut segment)));
        self.checkpoint = segment.checkpoint;
        self.inner = Some(SourceMainNativeActualFactServiceV1 {
            inner: segment.controller,
            fact_barrier,
            deferred_error,
        });
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
    pub fn prepare_checkpoint(&mut self) -> ActiveWalResult<()> {
        self.with_segmented(|segment| segment.prepare_checkpoint())
    }
    pub fn attempt_checkpoint(&mut self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        self.with_segmented(|segment| segment.attempt_checkpoint())
    }
    pub fn resolve_checkpoint(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        self.with_segmented(|segment| segment.resolve_checkpoint())
    }
    pub fn finish_checkpoint(&mut self) -> ActiveWalResult<ActiveHistorySegmentCursorV1> {
        self.with_segmented(|segment| segment.finish_checkpoint().cloned())
    }
}
