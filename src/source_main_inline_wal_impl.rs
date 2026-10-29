pub const NATIVE_MAIN_INLINE_ACTIVE_WAL_PROFILE: &str =
    "TL-MAIN-ACTIVE-SERVICE-INLINE-ESCROW-WAL-13-1";
impl SourceMainNativeDurableActiveServiceV1 {
    pub fn create_with_inline_service_escrow(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        escrow_root: PathBuf,
        durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_generation_escrow_internal(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            (escrow_root, durability, true),
        )
    }
    fn inline_service_enabled(&self) -> bool {
        self.generation_escrow
            .as_ref()
            .is_some_and(|c| c.inline_profile)
    }
    fn pending_native_action_parts(
        service: &SourceMainNativeActiveServiceV1,
    ) -> ActiveWalResult<(V, Option<&crate::compact_journal::NativeAppendPlan>)> {
        if let Some(write) = service.pending_inline_write() {
            if !service.inline_profile || service.pending_write().is_some() {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            Ok((write.durable_read_context(), write.append_plan()))
        } else {
            let write = service
                .pending_write()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            Ok((write.durable_read_context(), Some(write.append_plan())))
        }
    }
    fn verify_current_native_post(service: &SourceMainNativeActiveServiceV1) -> ActiveWalResult<V> {
        if let Some(write) = service.pending_inline_write() {
            service
                .owner
                .index
                .resolve_inline_database_post(write)
                .map_err(active_wal_backend)?;
            Ok(write.durable_read_context())
        } else {
            let write = service
                .pending_write()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            service
                .owner
                .index
                .resolve_prepared_validity_post(write)
                .map_err(active_wal_backend)?;
            Ok(write.durable_read_context())
        }
    }
}
