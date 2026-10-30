impl NativeHistoricalValidityPlaybackV1<'_> {
    pub(crate) fn install_actual_fact_reopen_view(
        &mut self,
        capability: &NativeColdIndexExecutionV1,
        expected: &SourceIndexHistoryPinV1,
        view: VerifiedOriginalMetadataViewV1,
    ) -> Result<()> {
        if self.index.cold_execution != Some(capability.scope)
            || self.active.is_some()
            || self.source_history_pin() != expected
        {
            return Err(CompactIndexError::ColdWriteRefused);
        }
        let target = view.original();
        if target.storage.namespace != expected.storage.namespace
            || target.storage.persistent_identity != expected.storage.persistent_identity
            || target.storage.canonical_catalog != expected.storage.canonical_catalog
            || target.metadata.sequence != expected.metadata.sequence
            || target.rows != expected.rows
            || !self.current.raw_entries().eq(view.raw_entries())
        {
            return Err(CompactIndexError::WrongContext);
        }
        // Actual verified same-KV administrative reopen only. No append, new
        // financial owner, imported KV values or logical-write advancement.
        if self
            .verified_views
            .as_ref()
            .is_none_or(|views| self.next >= views.len())
        {
            return Err(CompactIndexError::WrongContext);
        }
        let pin = target.clone();
        let current = MainIndexReadViewV1::from_verified(view);
        self.boundaries[self.next] = pin;
        self.current = current.clone();
        let views = self
            .verified_views
            .as_mut()
            .expect("prevalidated verified view lane");
        views[self.next] = current;
        Ok(())
    }
}
