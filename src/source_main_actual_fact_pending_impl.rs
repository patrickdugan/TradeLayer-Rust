impl VerifiedActualSourceFactTraceV1 {
    pub(crate) fn prepare_actual_pending_inline(
        &self,
        index: &mut SourceCompactIndex,
        sequence: u64,
        after: bool,
        source: NativeHistoricalInlineDatabaseV1,
    ) -> Result<VerifiedMode4InlinePendingV1> {
        if source.append_verified() || source.resolved() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let input = self
            .source_pending_input(sequence, after)?
            .ok_or(ActiveServiceWalErrorV1::WrongContext)?;
        self.prepare_actual_pending_from_metadata(index, source, input.metadata())
    }
    pub(crate) fn prepare_actual_pending_from_metadata(
        &self,
        index: &mut SourceCompactIndex,
        source: NativeHistoricalInlineDatabaseV1,
        metadata: &ActualNativePlanFactExpectationV1,
    ) -> Result<VerifiedMode4InlinePendingV1> {
        metadata.verify_recomputed_plan(&source)?;
        // Re-read only actual verified ORIGINAL/COPIED directory/page bytes.
        // This is metadata recomputation, never a native writer or source result.
        let view = self.checked_original_view(index, source.original_read())?;
        let computed = view
            .prepare_historical_inline_database(source.method(), source.arguments().to_vec())
            .map_err(|_| ActiveServiceWalErrorV1::WrongPlan)?;
        if computed.durable_read_context() != source.durable_read_context()
            || computed.exact_prepared_mutations() != source.exact_prepared_mutations()
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let projection = if source.requires_native_write() {
            let candidates =
                Mode4Candidates::parse(metadata.native_candidates(), source.original_read())?;
            if candidates.read_context != source.durable_read_context()
                || candidates.before != Mode4AppendPoint::of(source.original_read())
            {
                return Err(ActiveServiceWalErrorV1::WrongPlan);
            }
            let snapshot = |point: Mode4AppendPoint| {
                let mut metadata = source.original_read().metadata.clone();
                metadata.sequence = point.sequence;
                metadata.publication = point.publication;
                metadata
            };
            Some(
                crate::source_compact_index_v2::HistoricalInlinePhysicalProjectionV1 {
                    target_log_bytes: history_count(&candidates.raw.get("targetLogBytes"))?,
                    retained_ancestors: usize::try_from(history_count(
                        &candidates.raw.get("retainedAncestors"),
                    )?)
                    .map_err(|_| ActiveServiceWalErrorV1::Capacity)?,
                    after: snapshot(candidates.post),
                    intermediate: candidates.intermediate.map(snapshot),
                },
            )
        } else {
            None
        };
        // Internal reusable readonly LIVE SCALAR shape only. It is not an old
        // Mode4 reader/proof/append capability. Unlike old format this needs no
        // future successful append and may remain held through Pause/Shutdown.
        Ok(VerifiedMode4InlinePendingV1 {
            source,
            projection,
            append_sequence: None,
        })
    }
}
