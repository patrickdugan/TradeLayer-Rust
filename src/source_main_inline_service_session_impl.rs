impl SourceMainNativeSessionV13 {
    /// Explicit new source-service component. Existing into_active_service and
    /// its old observations/heads keep the original coarse inline semantics.
    /// This entry alone retains each native processBlockData DB await.
    pub fn into_inline_active_service(
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
            (observation_limit, true),
        )
    }
}
impl SourceMainNativeActiveServiceV1 {
    pub fn pending_inline_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.frame.as_ref()?.inline_pending_write()
    }
}
