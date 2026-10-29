// Strict reader for the EXISTING enhanced-WAL metadata-pin scalar frames.
// Metadata integrity expectations only: no financial/source state is decoded.
const MAX_HISTORY_PIN_BYTES: usize = 1024 * 1024;
impl SourceIndexHistoryPinV1 {
    pub fn canonical_scalar_bytes(&self) -> std::result::Result<Vec<u8>, SourceRuleError> {
        let mut bytes = vec![];
        self.visit_canonical_fields(&mut |path, value| {
            let value = value.diagnostic_json()?;
            let add = 8usize
                .checked_add(path.len())
                .and_then(|v| v.checked_add(value.len()))
                .ok_or(SourceRuleError::Capacity)?;
            if bytes
                .len()
                .checked_add(add)
                .is_none_or(|v| v > MAX_HISTORY_PIN_BYTES)
            {
                return Err(SourceRuleError::Capacity);
            }
            bytes.extend_from_slice(
                &u32::try_from(path.len())
                    .map_err(|_| SourceRuleError::Capacity)?
                    .to_be_bytes(),
            );
            bytes.extend_from_slice(path.as_bytes());
            bytes.extend_from_slice(
                &u32::try_from(value.len())
                    .map_err(|_| SourceRuleError::Capacity)?
                    .to_be_bytes(),
            );
            bytes.extend_from_slice(value.as_bytes());
            Ok(())
        })?;
        Ok(bytes)
    }
    pub fn from_canonical_scalar_bytes(namespace: &Namespace, raw: &[u8]) -> Result<Self> {
        if raw.len() > MAX_HISTORY_PIN_BYTES {
            return Err(CompactIndexError::Limit);
        }
        let mut cursor = crate::codec::Cursor::new(raw);
        let mut fields = vec![];
        while cursor.end().is_err() {
            if fields.len() >= 256 {
                return Err(CompactIndexError::Limit);
            }
            let path = history_pin_wire_text(&mut cursor, 4096)?;
            let body = history_pin_wire_text(&mut cursor, 64 * 1024)?;
            let value = V::from_diagnostic_json(&body)?;
            if matches!(value, V::Array(_) | V::Object(_)) || value.diagnostic_json()? != body {
                return Err(CompactIndexError::Corrupt);
            }
            fields.push((path, value));
        }
        let mut r = HistoryPinFieldReader { fields, next: 0 };
        let profile = r.text("profile")?;
        let metadata = r.snapshot("metadata")?;
        let storage_profile = r.text("storage/profile")?;
        if r.text("storage/namespace")? != namespace.path(Path::new("")).to_string_lossy() {
            return Err(CompactIndexError::WrongContext);
        }
        let persistent_identity = r.text("storage/identity")?;
        let canonical_catalog = std::path::PathBuf::from(r.text("storage/catalog")?);
        let logical_sequence = r.uint("storage/logicalSequence")?;
        let publication = r.publication("storage/publication")?;
        let content = crate::storage::JournalCompleteKvReceiptV1 {
            profile: r.text("storage/content/profile")?,
            keys: r.uint("storage/content/keys")?,
            bytes: r.uint("storage/content/bytes")?,
            chain_sha256d: r.hash("storage/content/chain")?,
            complete: r.boolean("storage/content/complete")?,
        };
        let rows = RowStreamReceipt {
            profile: r.text("rows/profile")?,
            snapshot: r.snapshot("rows/snapshot")?,
            rows: r.uint("rows/count")?,
            chain_sha256d: r.hash("rows/chain")?,
            complete: r.boolean("rows/complete")?,
        };
        let pin = Self {
            profile,
            metadata,
            storage: crate::compact_journal::VerifiedCompactPrefixV1 {
                profile: storage_profile,
                namespace: namespace.clone(),
                persistent_identity,
                canonical_catalog,
                publication,
                logical_sequence,
                content,
            },
            rows,
        };
        if r.next != r.fields.len()
            || pin.profile != VERIFIED_HISTORY_READ_PROFILE
            || pin.metadata.profile != METADATA_PROFILE
            || pin.storage.profile != crate::compact_journal::VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || pin.storage.content.profile != crate::storage::COMPLETE_KV_RECEIPT_PROFILE
            || !pin.storage.content.complete
            || pin.rows.profile != STREAM_PROFILE
            || !pin.rows.complete
            || pin.rows.snapshot != pin.metadata
            || pin.metadata.sequence != pin.storage.logical_sequence
            || pin.metadata.publication != pin.storage.publication
            || pin.metadata.context.is_empty()
            || pin.metadata.context.len() > 4096
            || pin.storage.persistent_identity.is_empty()
            || pin.storage.canonical_catalog.as_os_str().is_empty()
            || pin.canonical_scalar_bytes()?.as_slice() != raw
        {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(pin)
    }
}
fn history_pin_wire_text(cursor: &mut crate::codec::Cursor<'_>, limit: usize) -> Result<String> {
    let count = cursor.u32().map_err(|_| CompactIndexError::Corrupt)? as usize;
    if count > limit {
        return Err(CompactIndexError::Limit);
    }
    let raw = cursor.take(count).map_err(|_| CompactIndexError::Corrupt)?;
    Ok(std::str::from_utf8(raw)
        .map_err(|_| CompactIndexError::Corrupt)?
        .to_owned())
}
struct HistoryPinFieldReader {
    fields: Vec<(String, V)>,
    next: usize,
}
impl HistoryPinFieldReader {
    fn take(&mut self, path: &str) -> Result<V> {
        let (actual, value) = self
            .fields
            .get(self.next)
            .ok_or(CompactIndexError::Corrupt)?;
        if actual != path {
            return Err(CompactIndexError::Corrupt);
        }
        self.next += 1;
        Ok(value.clone())
    }
    fn text(&mut self, path: &str) -> Result<String> {
        let V::String(value) = self.take(path)? else {
            return Err(CompactIndexError::Corrupt);
        };
        Ok(value)
    }
    fn uint(&mut self, path: &str) -> Result<u64> {
        let raw = self.text(path)?;
        let value = raw.parse::<u64>().map_err(|_| CompactIndexError::Corrupt)?;
        if value.to_string() != raw {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(value)
    }
    fn boolean(&mut self, path: &str) -> Result<bool> {
        let V::Bool(value) = self.take(path)? else {
            return Err(CompactIndexError::Corrupt);
        };
        Ok(value)
    }
    fn parent(&mut self, path: &str) -> Result<Option<u64>> {
        match self.take(path)? {
            V::Null => Ok(None),
            V::String(raw) => {
                let value = raw.parse::<u64>().map_err(|_| CompactIndexError::Corrupt)?;
                if value.to_string() != raw {
                    return Err(CompactIndexError::Corrupt);
                }
                Ok(Some(value))
            }
            _ => Err(CompactIndexError::Corrupt),
        }
    }
    fn hash(&mut self, path: &str) -> Result<[u8; 32]> {
        let raw = self.text(path)?;
        if raw.len() != 64
            || raw
                .bytes()
                .any(|b| !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b))
        {
            return Err(CompactIndexError::Corrupt);
        }
        let mut hash = [0; 32];
        for (i, byte) in hash.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16)
                .map_err(|_| CompactIndexError::Corrupt)?;
        }
        Ok(hash)
    }
    fn publication(&mut self, path: &str) -> Result<CatalogPublication> {
        Ok(CatalogPublication {
            generation: self.uint(&format!("{path}/generation"))?,
            parent: self.parent(&format!("{path}/parent"))?,
            physical_creation_sequence: self.uint(&format!("{path}/physicalCreation"))?,
            creation_sequence: self.uint(&format!("{path}/logicalCreation"))?,
            manifest_sha256d: self.hash(&format!("{path}/manifest"))?,
        })
    }
    fn snapshot(&mut self, path: &str) -> Result<MetadataSnapshot> {
        if self.text(&format!("{path}/kind"))? != "Object"
            || self.uint(&format!("{path}/count"))? != 8
        {
            return Err(CompactIndexError::Corrupt);
        }
        let mut values = vec![];
        for (i, key) in [
            "profile",
            "context",
            "sequence",
            "generation",
            "parent",
            "physicalCreation",
            "logicalCreation",
            "manifest",
        ]
        .into_iter()
        .enumerate()
        {
            if self.text(&format!("{path}/{i}/key"))? != key {
                return Err(CompactIndexError::Corrupt);
            }
            let scalar = self.take(&format!("{path}/{i}/value"))?;
            values.push((key.to_owned(), scalar));
        }
        let mut nested = HistoryPinFieldReader {
            fields: values,
            next: 0,
        };
        let profile = nested.text("profile")?;
        let context = nested.text("context")?;
        let sequence = nested.uint("sequence")?;
        let publication = CatalogPublication {
            generation: nested.uint("generation")?,
            parent: nested.parent("parent")?,
            physical_creation_sequence: nested.uint("physicalCreation")?,
            creation_sequence: nested.uint("logicalCreation")?,
            manifest_sha256d: nested.hash("manifest")?,
        };
        Ok(MetadataSnapshot {
            profile,
            context,
            sequence,
            publication,
        })
    }
}
