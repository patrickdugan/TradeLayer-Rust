// Strict expected-descriptor reader only. Never constructs a live writer permit.
const MAX_EXPECTED_ESCROW_DESCRIPTOR_BYTES: usize = 1024 * 1024;
struct ExpectedEscrowDescriptorReader<'a> {
    cursor: crate::codec::Cursor<'a>,
}
impl ExpectedEscrowDescriptorReader<'_> {
    fn bytes(&mut self, n: usize) -> Result<&[u8], StorageError> {
        self.cursor
            .take(n)
            .map_err(|_| bad("escrow descriptor truncated"))
    }
    fn uint(&mut self) -> Result<u64, StorageError> {
        self.cursor
            .u64()
            .map_err(|_| bad("escrow descriptor integer"))
    }
    fn text(&mut self) -> Result<String, StorageError> {
        let length = self
            .cursor
            .u32()
            .map_err(|_| bad("escrow descriptor length"))? as usize;
        if length > MAX_EXPECTED_ESCROW_DESCRIPTOR_BYTES {
            return Err(StorageError::LimitExceeded);
        }
        Ok(std::str::from_utf8(self.bytes(length)?)
            .map_err(|_| bad("escrow descriptor UTF-8"))?
            .to_owned())
    }
    fn hash(&mut self) -> Result<[u8; 32], StorageError> {
        self.cursor
            .fixed::<32>()
            .map_err(|_| bad("escrow descriptor hash"))
    }
    fn prefix(&mut self, namespace: &Namespace) -> Result<VerifiedCompactPrefixV1, StorageError> {
        let profile = self.text()?;
        if self.text()? != identity(namespace) {
            return Err(bad("escrow descriptor namespace"));
        }
        let persistent_identity = self.text()?;
        let canonical_catalog = PathBuf::from(self.text()?);
        expected_escrow_lexical_path(&canonical_catalog)?;
        let generation = self.uint()?;
        let parent = self.uint()?;
        let physical_creation_sequence = self.uint()?;
        let creation_sequence = self.uint()?;
        let logical_sequence = self.uint()?;
        let publication = CatalogPublication {
            generation,
            parent: if parent == u64::MAX {
                None
            } else {
                Some(parent)
            },
            physical_creation_sequence,
            creation_sequence,
            manifest_sha256d: self.hash()?,
        };
        let content = crate::storage::JournalCompleteKvReceiptV1 {
            profile: self.text()?,
            keys: self.uint()?,
            bytes: self.uint()?,
            chain_sha256d: self.hash()?,
            complete: match self.cursor.u8() {
                Ok(0) => false,
                Ok(1) => true,
                _ => return Err(bad("escrow descriptor boolean")),
            },
        };
        if profile != VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || persistent_identity.is_empty()
            || content.profile != crate::storage::COMPLETE_KV_RECEIPT_PROFILE
            || !content.complete
            || publication.parent.is_some_and(|p| p >= generation)
            || creation_sequence > logical_sequence
            || logical_sequence
                .checked_sub(creation_sequence)
                .and_then(|n| n.checked_add(physical_creation_sequence))
                .is_none()
        {
            return Err(bad("escrow descriptor prefix domain"));
        }
        Ok(VerifiedCompactPrefixV1 {
            profile,
            namespace: namespace.clone(),
            persistent_identity,
            canonical_catalog,
            publication,
            logical_sequence,
            content,
        })
    }
}
fn expected_escrow_lexical_path(path: &Path) -> Result<(), StorageError> {
    let rebuilt: PathBuf = path.components().collect();
    if !path.is_absolute()||path.as_os_str()!=rebuilt.as_os_str()
        ||path.components().any(|p|matches!(p,std::path::Component::CurDir|std::path::Component::ParentDir)
            ||matches!(p,std::path::Component::Normal(v)if v==std::ffi::OsStr::new(".")||v==std::ffi::OsStr::new(".."))){
        return Err(bad("noncanonical expected escrow path"));
    }
    Ok(())
}
impl GenerationEscrowPinV1 {
    /// Parse independently retained expected bytes without touching a file.
    /// Actual copy/manifest/history verification remains mandatory at open.
    pub fn from_canonical_descriptor_bytes(
        namespace: &Namespace,
        raw: &[u8],
        record_sha256d: [u8; 32],
    ) -> Result<Self, StorageError> {
        if raw.len() > MAX_EXPECTED_ESCROW_DESCRIPTOR_BYTES {
            return Err(StorageError::LimitExceeded);
        }
        if double_sha256(raw) != record_sha256d {
            return Err(bad("escrow descriptor expected hash"));
        }
        let mut r = ExpectedEscrowDescriptorReader {
            cursor: crate::codec::Cursor::new(raw),
        };
        if r.bytes(7)? != b"TLGESC\x01" {
            return Err(bad("escrow descriptor magic"));
        }
        let profile = r.text()?;
        if profile != GENERATION_ESCROW_PROFILE {
            return Err(bad("escrow descriptor profile"));
        }
        let copied_root = PathBuf::from(r.text()?);
        expected_escrow_lexical_path(&copied_root)?;
        let durability = match r.cursor.u8() {
            Ok(0) => GenerationEscrowDurabilityV1::FileSyncOnly,
            Ok(1) => GenerationEscrowDurabilityV1::FileAndDirectorySync,
            _ => return Err(bad("escrow descriptor durability")),
        };
        let original_current = r.prefix(namespace)?;
        if copied_root.starts_with(&original_current.canonical_catalog)
            || original_current.canonical_catalog.starts_with(&copied_root)
        {
            return Err(bad("escrow descriptor origin/copy overlap"));
        }
        let count = r
            .cursor
            .u32()
            .map_err(|_| bad("escrow descriptor point count"))? as usize;
        if count == 0 || count > MAX_ESCROW_POINTS {
            return Err(StorageError::LimitExceeded);
        }
        let mut requested = Vec::with_capacity(count);
        for _ in 0..count {
            let point = r.prefix(namespace)?;
            if point.persistent_identity != original_current.persistent_identity
                || point.canonical_catalog != original_current.canonical_catalog
                || point.publication != original_current.publication
                || point.logical_sequence > original_current.logical_sequence
            {
                return Err(bad("escrow descriptor point alignment"));
            }
            requested.push(point);
        }
        let log = GenerationFilePinV1 {
            bytes: r.uint()?,
            chunks: r.uint()?,
            chain_sha256d: r.hash()?,
        };
        let manifest_sha256d = r.hash()?;
        r.cursor
            .end()
            .map_err(|_| bad("escrow descriptor trailing bytes"))?;
        if log.bytes == 0
            || log.bytes > 512 * 1024 * 1024
            || log.chunks != log.bytes.div_ceil(64 * 1024)
        {
            return Err(bad("escrow descriptor file bound"));
        }
        let copied_generation = copied_root.join(format!("escrow-{}", escrow_hex(&record_sha256d)));
        let pin = Self {
            profile,
            original_current,
            requested,
            copied_root,
            copied_generation,
            log,
            manifest_sha256d,
            record_sha256d,
            durability,
        };
        if pin.canonical_descriptor_bytes()?.as_slice() != raw {
            return Err(bad("noncanonical escrow descriptor"));
        }
        Ok(pin)
    }
}
