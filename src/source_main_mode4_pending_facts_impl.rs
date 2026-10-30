// Independently checked metadata predictions for one existing single-inline
// Mode4 await. No writer token, source result or financial state is imported.
pub(crate) struct VerifiedMode4InlinePendingV1 {
    source: NativeHistoricalInlineDatabaseV1,
    projection: Option<crate::source_compact_index_v2::HistoricalInlinePhysicalProjectionV1>,
    append_sequence: Option<u64>,
}
impl VerifiedMode4InlinePendingV1 {
    pub(crate) fn source(&self) -> &NativeHistoricalInlineDatabaseV1 {
        &self.source
    }
    pub(crate) fn source_mut(&mut self) -> &mut NativeHistoricalInlineDatabaseV1 {
        &mut self.source
    }
    pub(crate) fn append_sequence(&self) -> Option<u64> {
        self.append_sequence
    }
    pub(crate) fn visit_live_fields(
        &self,
        visitor: &mut dyn FnMut(&str, &V),
    ) -> std::result::Result<(), crate::source_compact_index_v2::CompactIndexError> {
        self.source
            .visit_live_projection(self.projection.as_ref(), visitor)
    }
}
impl VerifiedInlineActiveServiceAckHistoryV1 {
    pub(crate) fn prepare_pending_inline(
        &self,
        index: &mut SourceCompactIndex,
        after_sequence: u64,
        source: NativeHistoricalInlineDatabaseV1,
    ) -> Result<VerifiedMode4InlinePendingV1> {
        if source.append_verified() || source.resolved() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        if !source.requires_native_write() {
            return Ok(VerifiedMode4InlinePendingV1 {
                source,
                projection: None,
                append_sequence: None,
            });
        }
        // Full read-context/TLNM equality, not first method/name matching. The
        // old format needs an actually recorded successful future append; a
        // never-appended pending plan needs a separate future fact profile.
        let context = source.durable_read_context();
        let mut found = None;
        for entry in self
            .entries()
            .iter()
            .filter(|e| e.sequence > after_sequence)
        {
            let Some(raw) = entry.native_candidates() else {
                continue;
            };
            let candidate = parse(raw)?;
            if candidate.get("readContext") != context {
                continue;
            }
            if found.replace((entry, candidate)).is_some() {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
        }
        let (entry, raw) = found.ok_or(ActiveServiceWalErrorV1::WrongContext)?;
        let value = self.entry_value(entry)?;
        if value.get("nativeRecovery") != V::Undefined && value.get("nativeRecovery") != V::Null {
            // No independent raw native-IO-stop fact exists in old Mode4.
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let before = self.mode4_pin(entry.verified_history_before())?;
        if &before != source.original_read()
            || entry.native_mutations() != Some(source.exact_prepared_mutations())
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let candidates = Mode4Candidates::parse(raw, &before)?;
        let view = self.checked_original_view(index, &before)?;
        let mut independently_recomputed = view
            .prepare_historical_inline_database(source.method(), source.arguments().to_vec())
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        self.verify_inline_operation(index, entry.sequence, &mut independently_recomputed)?;
        if independently_recomputed.durable_read_context() != context
            || independently_recomputed.exact_prepared_mutations()
                != source.exact_prepared_mutations()
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let snapshot = |point: Mode4AppendPoint| {
            let mut pin = before.metadata.clone();
            pin.sequence = point.sequence;
            pin.publication = point.publication;
            pin
        };
        let projection = crate::source_compact_index_v2::HistoricalInlinePhysicalProjectionV1 {
            target_log_bytes: history_count(&candidates.raw.get("targetLogBytes"))?,
            retained_ancestors: usize::try_from(history_count(
                &candidates.raw.get("retainedAncestors"),
            )?)
            .map_err(|_| ActiveServiceWalErrorV1::Capacity)?,
            after: snapshot(candidates.post),
            intermediate: candidates.intermediate.map(snapshot),
        };
        Ok(VerifiedMode4InlinePendingV1 {
            source,
            projection: Some(projection),
            append_sequence: Some(entry.sequence),
        })
    }
}
