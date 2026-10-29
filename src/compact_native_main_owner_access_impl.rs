// Read-only persistent owner/location receipt. A digest is not authority.
impl CompactingJournal {
    pub fn native_main_store_context(&self) -> Result<(String, PathBuf), StorageError> {
        self.check()?;
        Ok((self.catalog_identity.clone(), self.catalog.canonicalize()?))
    }
}
