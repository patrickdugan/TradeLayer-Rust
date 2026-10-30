// Process-local live-writer scope only. Neither bytes/hashes nor a caller value
// can construct this capability or rebind it to a reopened/copied Journal.
pub(crate) struct SourceQueuedLiveWriterV1 {
    token: u64,
    identity: String,
    catalog: PathBuf,
    namespace: crate::storage::Namespace,
    target_bytes: u64,
    retained_ancestors: usize,
}
impl CompactingJournal {
    pub(crate) fn capture_source_queued_live_writer(
        &self,
    ) -> Result<SourceQueuedLiveWriterV1, StorageError> {
        self.check()?;
        Ok(SourceQueuedLiveWriterV1 {
            token: self.owner_token,
            identity: self.catalog_identity.clone(),
            catalog: self.catalog.canonicalize()?,
            namespace: self.namespace().clone(),
            target_bytes: self.config.target_log_bytes,
            retained_ancestors: self.config.retained_ancestors,
        })
    }
    pub(crate) fn require_source_queued_live_writer(
        &self,
        scope: &SourceQueuedLiveWriterV1,
    ) -> Result<(), StorageError> {
        self.check()?;
        if self.owner_token != scope.token
            || self.catalog_identity != scope.identity
            || self.catalog.canonicalize()? != scope.catalog
            || self.namespace() != &scope.namespace
            || self.config.target_log_bytes != scope.target_bytes
            || self.config.retained_ancestors != scope.retained_ancestors
        {
            return Err(bad("foreign/reopened queued source writer"));
        }
        Ok(())
    }
}
