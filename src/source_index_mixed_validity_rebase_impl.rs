impl NativeHistoricalValidityAwaitV1 {
    /// Pure metadata comparison for an independently verified reopened/copy
    /// view. It grants no append token, playback advance or source result.
    /// Original query/read arguments stay fixed; only actual Write-Before is
    /// substituted after the full reopened KV has independently matched.
    pub fn recomputed_write_context_at(
        &self,
        actual: &VerifiedOriginalMetadataViewV1,
    ) -> Result<V> {
        if self.resolved()
            || actual.original().metadata.context != self.query.snapshot.context
            || prepared_metadata_state_entries(actual.raw_entries(), &[])? != self.before_state
        {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(prepared_durable_read_context(
            &self.query,
            &self.arguments,
            &self.before_state,
            &self.after_state,
            &actual.original().metadata,
        ))
    }
    pub fn recomputed_before_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.before_state
    }
    pub fn recomputed_after_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.after_state
    }
}
