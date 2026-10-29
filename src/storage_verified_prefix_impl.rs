/// Complete KV integrity relative to independent expectations, not authority.
pub const COMPLETE_KV_RECEIPT_PROFILE: &str = "TL-NATIVE-COMPLETE-KV-PREPARED-13-1";
pub const VERIFIED_PREFIX_OPEN_PROFILE: &str = "TL-NATIVE-VERIFIED-PREFIX-OPEN-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalCompleteKvReceiptV1 {
    pub profile: String,
    pub keys: u64,
    pub bytes: u64,
    pub chain_sha256d: [u8; 32],
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedJournalPrefixV1 {
    pub profile: String,
    pub namespace: Namespace,
    pub canonical_file: PathBuf,
    pub physical_sequence: u64,
    pub content: JournalCompleteKvReceiptV1,
}
fn complete_kv_receipt<'a>(
    entries: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Result<JournalCompleteKvReceiptV1, StorageError> {
    let mut receipt = JournalCompleteKvReceiptV1 {
        profile: COMPLETE_KV_RECEIPT_PROFILE.into(),
        keys: 0,
        bytes: 0,
        chain_sha256d: crate::hash256::double_sha256(COMPLETE_KV_RECEIPT_PROFILE.as_bytes()),
        complete: false,
    };
    for (key, value) in entries {
        let ordinal = receipt.keys.to_be_bytes();
        let key_len = u32::try_from(key.len())
            .map_err(|_| StorageError::LimitExceeded)?
            .to_be_bytes();
        let value_len = u64::try_from(value.len())
            .map_err(|_| StorageError::LimitExceeded)?
            .to_be_bytes();
        receipt.chain_sha256d = crate::hash256::double_sha256_parts(&[
            &receipt.chain_sha256d,
            &ordinal,
            &key_len,
            key.as_bytes(),
            &value_len,
            value,
        ]);
        receipt.keys = receipt
            .keys
            .checked_add(1)
            .ok_or(StorageError::LimitExceeded)?;
        receipt.bytes = receipt
            .bytes
            .checked_add(key.len() as u64)
            .and_then(|n| n.checked_add(value.len() as u64))
            .ok_or(StorageError::LimitExceeded)?;
    }
    receipt.complete = true;
    Ok(receipt)
}
impl JournalReadSnapshot {
    pub fn complete_kv_receipt(&self) -> Result<JournalCompleteKvReceiptV1, StorageError> {
        complete_kv_receipt(self.entries())
    }
}
impl Journal {
    // Existing private callers retain their exact sequence-only behavior.
    fn open_mode_checked(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        create: bool,
        expected_sequence: Option<u64>,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode_verified(root, namespace, recovery, create, expected_sequence, None)
    }
    pub fn complete_kv_receipt(&self) -> Result<JournalCompleteKvReceiptV1, StorageError> {
        complete_kv_receipt(self.entries()?)
    }
    pub fn verified_prefix_pin(&self) -> Result<VerifiedJournalPrefixV1, StorageError> {
        Ok(VerifiedJournalPrefixV1 {
            profile: VERIFIED_PREFIX_OPEN_PROFILE.into(),
            namespace: self.namespace.clone(),
            canonical_file: self.path.canonicalize()?,
            physical_sequence: self.sequence()?,
            content: self.complete_kv_receipt()?,
        })
    }
    /// Complete prefix check under the SAME exclusive file lock before repair.
    /// Missing files/namespaces are never created by this explicit profile.
    pub fn open_existing_verified_prefix(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        expected: &VerifiedJournalPrefixV1,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        if expected.profile != VERIFIED_PREFIX_OPEN_PROFILE
            || expected.namespace != namespace
            || expected.content.profile != COMPLETE_KV_RECEIPT_PROFILE
            || !expected.content.complete
        {
            return Err(StorageError::Corrupt(
                "invalid independently pinned recovery profile",
            ));
        }
        Self::open_mode_verified(
            root,
            namespace,
            recovery,
            false,
            Some(expected.physical_sequence),
            Some(expected),
        )
    }
    fn require_verified_prefix(
        &self,
        expected: Option<&VerifiedJournalPrefixV1>,
    ) -> Result<(), StorageError> {
        let Some(expected) = expected else {
            return Ok(());
        };
        if self.namespace != expected.namespace
            || self.path.canonicalize()? != expected.canonical_file
        {
            return Err(StorageError::Corrupt(
                "unexpected independently pinned recovery location",
            ));
        }
        if self.complete_kv_receipt()? != expected.content {
            return Err(StorageError::Corrupt(
                "unexpected independently pinned complete KV prefix",
            ));
        }
        #[cfg(test)]
        verified_prefix_test_hook(self);
        Ok(())
    }
}
#[cfg(test)]
type VerifiedPrefixTestHook = Box<dyn FnOnce(&Journal)>;
#[cfg(test)]
std::thread_local! {
    static VERIFIED_PREFIX_TEST_HOOK: std::cell::RefCell<Option<VerifiedPrefixTestHook>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn verified_prefix_test_hook(journal: &Journal) {
    let hook = VERIFIED_PREFIX_TEST_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook(journal);
    }
}
