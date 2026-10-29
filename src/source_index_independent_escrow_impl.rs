impl SourceIndexGenerationEscrowPinV1 {
    pub fn open_independently_pinned_history(
        &self,
        expected: &SourceIndexHistoryPinV1,
    ) -> Result<SourceIndexEscrowReadSnapshotV1> {
        if expected.profile != VERIFIED_HISTORY_READ_PROFILE
            || expected.metadata.profile != METADATA_PROFILE
            || expected.metadata.publication != expected.storage.publication
            || expected.metadata.sequence != expected.storage.logical_sequence
            || expected.rows.profile != STREAM_PROFILE
            || !expected.rows.complete
            || expected.rows.snapshot != expected.metadata
        {
            return Err(CompactIndexError::WrongContext);
        }
        let data = self
            .storage
            .open_independently_pinned_prefix(&expected.storage)?;
        let directory = Directory::decode(data.get(DIRECTORY).ok_or(CompactIndexError::Corrupt)?)?;
        if directory.context != expected.metadata.context
            || data.entries().count() != directory.pages.len() + 1
        {
            return Err(CompactIndexError::Corrupt);
        }
        let view = SourceIndexEscrowReadSnapshotV1 {
            expected: expected.clone(),
            directory,
            data,
        };
        if view.visit_rows(|_| Ok(true))? != expected.rows {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(view)
    }
}
impl SourceIndexEscrowReadSnapshotV1 {
    pub(crate) fn raw_get(&self, key: &str) -> Option<&[u8]> {
        self.data.get(key)
    }
    pub(crate) fn raw_entries(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.data.entries()
    }
}
