impl SourceMainNativeDurableFreshInputV1 {
    /// Retain independently before cold opening. This publishes neither a
    /// source result nor a native permit; both actual stores remain owned.
    pub fn capture_cold_pin(
        &mut self,
    ) -> DurableFreshResult<crate::source_main_active_service_wal::DurableFreshColdPinV1> {
        if self.control.phase != NativeMainDurableFreshPhaseV1::Idle
            || self.control.pending.is_some()
            || self.control.history.is_empty()
        {
            return Err(NativeMainDurableFreshErrorV1::Busy);
        }
        let namespace = self.source.inner.index().native_namespace().clone();
        let (source_entry, source) = if self.source.history().is_empty() {
            let point = self
                .source
                .inner
                .wal
                .capture_actual_durable_fresh_source_boundary()?;
            (
                crate::source_main_active_service_wal::DurableFreshSourceEntryV1::Created,
                crate::source_main_active_service_wal::ActualSourceFactColdPinV1 {
                    profile:
                        crate::source_main_active_service_wal::ACTUAL_SOURCE_FACT_COLD_PIN_PROFILE
                            .into(),
                    wal: point.source_pin().clone(),
                    metadata_namespace: namespace,
                    trace: point.source_trace().clone(),
                },
            )
        } else {
            (
                crate::source_main_active_service_wal::DurableFreshSourceEntryV1::Acknowledged,
                self.source
                    .inner
                    .wal
                    .capture_actual_source_fact_cold_pin(&namespace)?,
            )
        };
        let (outer, trace) = self.control.wal.capture_durable_fresh_outer_trace()?;
        Ok(
            crate::source_main_active_service_wal::DurableFreshColdPinV1 {
                profile: crate::source_main_active_service_wal::DURABLE_FRESH_COLD_PIN_PROFILE
                    .into(),
                outer,
                trace,
                source_entry,
                source,
            },
        )
    }
}
