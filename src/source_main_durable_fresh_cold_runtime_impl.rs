impl NativeMainServiceContinuationV1 {
    // Read-only genuine source-derived historical call, not a live plan/permit.
    pub(crate) fn durable_fresh_historical_inline_source(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativeHistoricalInlineDatabaseV1> {
        self.block
            .as_ref()?
            .inline_index
            .as_ref()?
            .historical
            .as_ref()
            .map(|v| v.source())
    }
}
