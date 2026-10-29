// Execution capability on the SAME actual index. It grants no source verdict
// and creates no mutable historical index. Default live write paths stay live.
pub const COLD_INDEX_EXECUTION_PROFILE: &str = "TL-TXINDEX-COLD-EXECUTION-CAPABILITY-13-1";
static NEXT_COLD_INDEX_SCOPE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub struct NativeColdIndexExecutionV1 {
    scope: u64,
    current: SourceIndexHistoryPinV1,
}
impl std::fmt::Debug for NativeColdIndexExecutionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeColdIndexExecutionV1")
            .field("profile", &COLD_INDEX_EXECUTION_PROFILE)
            .field("current", &self.current.metadata)
            .finish_non_exhaustive()
    }
}
impl NativeColdIndexExecutionV1 {
    pub fn profile(&self) -> &'static str {
        COLD_INDEX_EXECUTION_PROFILE
    }
    pub fn original_current(&self) -> &SourceIndexHistoryPinV1 {
        &self.current
    }
}
impl SourceCompactIndex {
    /// Bind a unique cold execution scope to independently verified actual Post
    /// files under this owner's retained lock. This does not release the owner.
    pub fn begin_verified_cold_execution(
        &mut self,
        expected: &SourceIndexHistoryPinV1,
    ) -> Result<NativeColdIndexExecutionV1> {
        if self.cold_execution.is_some() {
            return Err(CompactIndexError::ColdWriteRefused);
        }
        if self.capture_verified_history_pin()? != *expected {
            return Err(CompactIndexError::WrongContext);
        }
        self.read_verified_history_generation(expected)?;
        let scope = NEXT_COLD_INDEX_SCOPE
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map_err(|_| CompactIndexError::Limit)?;
        self.cold_execution = Some(scope);
        Ok(NativeColdIndexExecutionV1 {
            scope,
            current: expected.clone(),
        })
    }
    /// Only the exact retained capability for this actual owner may leave the
    /// scope, after rechecking actual Post files. Error keeps the guard active
    /// and the same borrowed capability can be retained/retried by its owner.
    pub fn finish_verified_cold_execution(
        &mut self,
        capability: &NativeColdIndexExecutionV1,
    ) -> Result<()> {
        if self.cold_execution != Some(capability.scope) {
            return Err(CompactIndexError::WrongContext);
        }
        self.read_verified_history_generation(&capability.current)?;
        if self.capture_verified_history_pin()? != capability.current {
            return Err(CompactIndexError::WrongContext);
        }
        self.cold_execution = None;
        Ok(())
    }
    fn require_live_metadata_write(&self) -> Result<()> {
        if self.cold_execution.is_some() {
            Err(CompactIndexError::ColdWriteRefused)
        } else {
            Ok(())
        }
    }
}
