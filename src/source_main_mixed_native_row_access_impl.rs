impl NativeMainServiceContinuationV1 {
    pub(crate) fn mixed_historical_validity(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativeHistoricalValidityAwaitV1> {
        let row = if let Some(block) = &self.block {
            block.row.as_ref()
        } else {
            self.catchup.row.as_ref()
        }?;
        row.pending_historical_write()
    }
    pub(crate) fn mixed_historical_validity_descriptor(&self) -> Option<V> {
        let row = if let Some(block) = &self.block {
            block.row.as_ref()
        } else {
            self.catchup.row.as_ref()
        }?;
        let write = row.pending_historical_write()?;
        let job = self
            .block
            .as_ref()
            .map_or(0, |b| b.request.job.saturating_add(1));
        if job >= 1 << 63 {
            return None;
        }
        let completed = self
            .block
            .as_ref()
            .map_or(self.catchup.validity.len(), |b| b.validity.len());
        Some(
            NativeMainRetainedConsensusPendingV1 {
                kind: NativeMainRetainedConsensusOperationKindV1::ServiceValidity,
                caller: None,
                caller_scope: 0,
                source_job: (1 << 63) | job,
                method_scope: 0,
                operation_ordinal: u64::try_from(completed.checked_add(row.validity.len())?)
                    .ok()?,
                role: if self.caller_result.is_some() {
                    crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Detached
                } else {
                    crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Awaited
                },
                context: Some(write.durable_read_context()),
                route: None,
            }
            .value(),
        )
    }
}
include!("source_main_mixed_native_ordinary_result_cold_impl.rs");
