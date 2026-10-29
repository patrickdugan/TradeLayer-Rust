impl CompactingJournal {
    /// Cold history within the currently verified publication only. Earlier
    /// generation discovery/retention is an explicit separate prerequisite.
    pub fn read_verified_history_prefix(
        &mut self,
        expected: &VerifiedCompactPrefixV1,
    ) -> Result<CompactReadSnapshot, StorageError> {
        self.check()?;
        if expected.profile != VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || expected.namespace != self.namespace
            || expected.persistent_identity != self.catalog_identity
            || expected.canonical_catalog != self.catalog.canonicalize()?
            || expected.publication != self.publication
            || expected.logical_sequence < self.publication.creation_sequence
            || expected.logical_sequence > self.sequence()?
        {
            return Err(bad("unsupported or mismatched historical publication"));
        }
        let physical_sequence = expected
            .logical_sequence
            .checked_sub(self.publication.creation_sequence)
            .and_then(|n| n.checked_add(self.publication.physical_creation_sequence))
            .ok_or(StorageError::LimitExceeded)?;
        let actual_file = self.inner.path().canonicalize()?;
        let pin = crate::storage::VerifiedJournalPrefixV1 {
            profile: crate::storage::VERIFIED_PREFIX_OPEN_PROFILE.into(),
            namespace: self.namespace.clone(),
            canonical_file: actual_file,
            physical_sequence,
            content: expected.content.clone(),
        };
        let lease = self.read_snapshot()?._lease;
        let data = self.inner.read_verified_history_prefix(&pin)?;
        Ok(CompactReadSnapshot {
            data,
            logical_sequence: expected.logical_sequence,
            publication: expected.publication.clone(),
            _lease: lease,
        })
    }
}
