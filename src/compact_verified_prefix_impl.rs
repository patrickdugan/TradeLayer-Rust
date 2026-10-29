pub const VERIFIED_COMPACT_PREFIX_OPEN_PROFILE: &str = "TL-COMPACT-VERIFIED-PREFIX-OPEN-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCompactPrefixV1 {
    pub profile: String,
    pub namespace: Namespace,
    pub persistent_identity: String,
    pub canonical_catalog: PathBuf,
    pub publication: CatalogPublication,
    pub logical_sequence: u64,
    pub content: crate::storage::JournalCompleteKvReceiptV1,
}
impl CompactingJournal {
    fn open_mode(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        create: bool,
        expected: Option<(&CatalogPublication, u64)>,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode_verified(root, namespace, recovery, create, expected, None)
    }
    pub fn complete_kv_receipt(
        &self,
    ) -> Result<crate::storage::JournalCompleteKvReceiptV1, StorageError> {
        self.check()?;
        self.inner.complete_kv_receipt()
    }
    /// Receipt only; no financial state or authority is constructed.
    pub fn verified_prefix_pin(&self) -> Result<VerifiedCompactPrefixV1, StorageError> {
        self.check()?;
        Ok(VerifiedCompactPrefixV1 {
            profile: VERIFIED_COMPACT_PREFIX_OPEN_PROFILE.into(),
            namespace: self.namespace.clone(),
            persistent_identity: self.catalog_identity.clone(),
            canonical_catalog: self.catalog.canonicalize()?,
            publication: self.publication.clone(),
            logical_sequence: self.sequence()?,
            content: self.inner.complete_kv_receipt()?,
        })
    }
    pub fn open_existing_verified_prefix(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        expected: &VerifiedCompactPrefixV1,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        if expected.profile != VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || expected.namespace != namespace
            || expected.content.profile != crate::storage::COMPLETE_KV_RECEIPT_PROFILE
            || !expected.content.complete
        {
            return Err(bad("invalid independently pinned compact recovery profile"));
        }
        Self::open_mode_verified(
            root,
            namespace,
            recovery,
            false,
            Some((&expected.publication, expected.logical_sequence)),
            Some(expected),
        )
    }
}
