impl NativeMainServiceContinuationV1 {
    pub(crate) fn actual_historical_inline_fact_descriptor(&self) -> Option<V> {
        let block = self.block.as_ref()?;
        let inline = block.inline_index.as_ref()?;
        let pending = inline.historical.as_ref()?;
        let job = block.request.job.saturating_add(1);
        if job >= 1 << 63 {
            return None;
        }
        let role = if self.caller_result.is_some() {
            crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Detached
        } else {
            crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Awaited
        };
        Some(
            NativeMainRetainedConsensusPendingV1 {
                kind: NativeMainRetainedConsensusOperationKindV1::ServiceInline,
                caller: None,
                caller_scope: 0,
                source_job: (1 << 63) | job,
                method_scope: inline.scope,
                operation_ordinal: inline.source.pending_write_ordinal(),
                role,
                context: Some(pending.source().durable_read_context()),
                route: None,
            }
            .value(),
        )
    }
}
