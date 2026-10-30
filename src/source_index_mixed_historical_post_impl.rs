impl NativeHistoricalValidityAwaitV1 {
    /// Metadata-only physical closure. No native append/source result is
    /// performed; actual complete KV/Directory/rows remain independently read.
    pub(crate) fn verify_independent_mixed_post(
        &self,
        post: &VerifiedOriginalMetadataViewV1,
        mutations: &[u8],
    ) -> Result<()> {
        if self.resolved()
            || post.original() != &self.post
            || self.raw_mutations != mutations
            || prepared_metadata_state_entries(post.raw_entries(), &[])? != self.after_state
            || post.raw_get(DIRECTORY) != Some(self.directory.encode()?.as_slice())
        {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(())
    }
    pub(crate) fn install_mixed_write_context(
        &mut self,
        before: &VerifiedOriginalMetadataViewV1,
        post: &VerifiedOriginalMetadataViewV1,
        mutations: &[u8],
    ) -> Result<()> {
        self.verify_independent_mixed_post(post, mutations)?;
        self.recomputed_write_context_at(before)?;
        if self.phase != HistoricalNativePhase::Prepared {
            return Err(CompactIndexError::WrongContext);
        }
        self.mixed_write_before = Some(before.original().metadata.clone());
        Ok(())
    }
    pub(crate) fn retain_mixed_recorded_error(&mut self, error: String) -> Result<()> {
        if !self.append_verified() || self.mixed_recorded_error.is_some() {
            return Err(CompactIndexError::WrongContext);
        }
        self.mixed_recorded_error = Some(error);
        Ok(())
    }
    pub(crate) fn matches_mixed_recorded_error(&self, error: &str) -> bool {
        self.append_verified() && self.mixed_recorded_error.as_deref() == Some(error)
    }
    pub(crate) fn take_mixed_recorded_error(&mut self, error: &str) -> bool {
        if !self.matches_mixed_recorded_error(error) {
            return false;
        }
        self.mixed_recorded_error = None;
        true
    }
    pub(crate) fn mixed_bound_write_before(&self) -> Option<&MetadataSnapshot> {
        self.mixed_write_before.as_ref()
    }
}
