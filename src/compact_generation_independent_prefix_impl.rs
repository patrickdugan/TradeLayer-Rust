// Explicit readonly independent-prefix API. The old requested-point whitelist
// reader remains unchanged; no supplied point creates a live mutation permit.
impl GenerationEscrowPinV1 {
    pub fn open_independently_pinned_prefix(
        &self,
        expected: &VerifiedCompactPrefixV1,
    ) -> Result<GenerationEscrowSnapshotV1, StorageError> {
        let origin = &self.original_current;
        if expected.profile != VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || expected.namespace != origin.namespace
            || expected.persistent_identity != origin.persistent_identity
            || expected.canonical_catalog != origin.canonical_catalog
            || expected.publication != origin.publication
            || expected.logical_sequence < origin.publication.creation_sequence
            || expected.logical_sequence > origin.logical_sequence
            || !expected.content.complete
        {
            return Err(bad("independent escrow point outside original generation"));
        }
        let mut journal = escrow_open_current(self)?;
        let copied = self
            .original_current
            .namespace
            .path(&self.copied_generation)
            .canonicalize()?;
        let mapped = escrow_journal_pin(expected, copied.clone())?;
        let data = journal.read_verified_history_prefix(&mapped)?;
        Ok(GenerationEscrowSnapshotV1 {
            original: expected.clone(),
            copied_file: copied,
            escrow: self.clone(),
            data,
        })
    }
}
