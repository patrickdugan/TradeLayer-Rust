/// Same-process source-call scope only, never reconstructed from a WAL pin.
pub(crate) struct NativeRecordedOnlyScopeV1 {
    token: u64,
}
impl SourceCompactIndex {
    pub(crate) fn begin_recorded_native_only_scope(&mut self) -> Result<NativeRecordedOnlyScopeV1> {
        self.require_live_metadata_write()?;
        if self.recorded_native_only_scope.is_some() {
            return Err(CompactIndexError::WrongContext);
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let token = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map_err(|_| CompactIndexError::Limit)?;
        self.recorded_native_only_scope = Some(token);
        Ok(NativeRecordedOnlyScopeV1 { token })
    }
    pub(crate) fn finish_recorded_native_only_scope(
        &mut self,
        scope: NativeRecordedOnlyScopeV1,
    ) -> Result<()> {
        if self.recorded_native_only_scope != Some(scope.token) {
            return Err(CompactIndexError::WrongContext);
        }
        self.recorded_native_only_scope = None;
        Ok(())
    }
    pub(crate) fn recorded_native_only_scope_active(&self) -> bool {
        self.recorded_native_only_scope.is_some()
    }
    fn require_unretained_metadata_write(&self) -> Result<()> {
        if self.recorded_native_only_scope.is_some() {
            return Err(CompactIndexError::Source(
                SourceRuleError::UnknownDependency,
            ));
        }
        Ok(())
    }
}
