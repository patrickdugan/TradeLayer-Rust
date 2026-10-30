impl NativeHistoricalValidityPlaybackV1<'_> {
    pub(crate) fn install_verified_inline_post(
        &mut self,
        source: &NativeHistoricalInlineDatabaseV1,
    ) -> Result<()> {
        if self.active.is_some() || source.original_read() != self.source_history_pin() {
            return Err(CompactIndexError::WrongContext);
        }
        if !source.requires_native_write() {
            return Ok(());
        }
        if !source.append_verified() {
            return Err(CompactIndexError::WrongContext);
        }
        let next = self.next.checked_add(1).ok_or(CompactIndexError::Limit)?;
        let pin = self
            .boundaries
            .get(next)
            .ok_or(CompactIndexError::WrongContext)?;
        if source.verified_post.as_ref() != Some(pin) {
            return Err(CompactIndexError::WrongContext);
        }
        let view = self
            .verified_views
            .as_ref()
            .and_then(|v| v.get(next))
            .ok_or(CompactIndexError::WrongContext)?
            .clone();
        if view.original_history() != Some(pin)
            || prepared_metadata_state_entries(view.raw_entries(), &[])? != *source.after_state()
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.current = view;
        self.next = next;
        Ok(())
    }
}
