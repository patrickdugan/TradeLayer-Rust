// Immutable EXTERNAL byte escrow. No old catalog format or pruning rule changes.
pub const GENERATION_ESCROW_PROFILE: &str = "TL-COMPACT-GENERATION-ESCROW-13-1";
const MAX_ESCROW_POINTS: usize = 128;
#[cfg(test)]
thread_local! {static ESCROW_FAIL_BEFORE_PUBLISH:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationEscrowDurabilityV1 {
    FileSyncOnly,
    FileAndDirectorySync,
}
pub type GenerationFilePinV1 = crate::storage::ActualJournalFilePinV1;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationEscrowPinV1 {
    pub profile: String,
    pub original_current: VerifiedCompactPrefixV1,
    pub requested: Vec<VerifiedCompactPrefixV1>,
    pub copied_root: PathBuf,
    pub copied_generation: PathBuf,
    pub log: GenerationFilePinV1,
    pub manifest_sha256d: [u8; 32],
    pub record_sha256d: [u8; 32],
    pub durability: GenerationEscrowDurabilityV1,
}
/// Constructed only after actual copy/data/record/publication sync+verification.
/// A cold pin/digest or reader does not construct this live mutation permit.
pub struct DurableGenerationEscrowPermitV1 {
    owner_token: u64,
    pin: GenerationEscrowPinV1,
}
impl DurableGenerationEscrowPermitV1 {
    pub fn independently_retained_pin(&self) -> &GenerationEscrowPinV1 {
        &self.pin
    }
}
pub struct GenerationEscrowSnapshotV1 {
    pub original: VerifiedCompactPrefixV1,
    pub copied_file: PathBuf,
    pub escrow: GenerationEscrowPinV1,
    data: JournalReadSnapshot,
}
impl GenerationEscrowSnapshotV1 {
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.data.get(key)
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.data.entries()
    }
    pub fn complete_kv_receipt(
        &self,
    ) -> Result<crate::storage::JournalCompleteKvReceiptV1, StorageError> {
        self.data.complete_kv_receipt()
    }
}
fn escrow_sync(path: &Path, mode: GenerationEscrowDurabilityV1) -> Result<(), StorageError> {
    if mode == GenerationEscrowDurabilityV1::FileAndDirectorySync {
        #[cfg(unix)]
        {
            File::open(path)?.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            return Err(bad("directory sync unavailable for escrow profile"));
        }
    }
    Ok(())
}
fn escrow_directory(path: &Path) -> Result<PathBuf, StorageError> {
    real_dir(path)?;
    let actual = path.canonicalize()?;
    if !path.is_absolute() || actual != path {
        return Err(bad("noncanonical escrow directory"));
    }
    Ok(actual)
}
fn escrow_part(out: &mut Vec<u8>, part: &[u8]) -> Result<(), StorageError> {
    let length = u32::try_from(part.len()).map_err(|_| StorageError::LimitExceeded)?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(part);
    if out.len() > 1024 * 1024 {
        return Err(StorageError::LimitExceeded);
    }
    Ok(())
}
fn escrow_prefix(out: &mut Vec<u8>, pin: &VerifiedCompactPrefixV1) -> Result<(), StorageError> {
    let namespace = identity(&pin.namespace);
    let catalog = pin.canonical_catalog.to_string_lossy();
    for text in [
        pin.profile.as_str(),
        namespace.as_str(),
        pin.persistent_identity.as_str(),
        catalog.as_ref(),
    ] {
        escrow_part(out, text.as_bytes())?;
    }
    for n in [
        pin.publication.generation,
        pin.publication.parent.unwrap_or(u64::MAX),
        pin.publication.physical_creation_sequence,
        pin.publication.creation_sequence,
        pin.logical_sequence,
    ] {
        out.extend_from_slice(&n.to_be_bytes());
    }
    out.extend_from_slice(&pin.publication.manifest_sha256d);
    escrow_part(out, pin.content.profile.as_bytes())?;
    out.extend_from_slice(&pin.content.keys.to_be_bytes());
    out.extend_from_slice(&pin.content.bytes.to_be_bytes());
    out.extend_from_slice(&pin.content.chain_sha256d);
    out.push(u8::from(pin.content.complete));
    Ok(())
}
fn escrow_record(pin: &GenerationEscrowPinV1) -> Result<Vec<u8>, StorageError> {
    if pin.profile != GENERATION_ESCROW_PROFILE
        || pin.requested.is_empty()
        || pin.requested.len() > MAX_ESCROW_POINTS
    {
        return Err(bad("escrow point/profile domain"));
    }
    let mut out = b"TLGESC\x01".to_vec();
    escrow_part(&mut out, pin.profile.as_bytes())?;
    escrow_part(&mut out, pin.copied_root.to_string_lossy().as_bytes())?;
    out.push(match pin.durability {
        GenerationEscrowDurabilityV1::FileSyncOnly => 0,
        GenerationEscrowDurabilityV1::FileAndDirectorySync => 1,
    });
    escrow_prefix(&mut out, &pin.original_current)?;
    out.extend_from_slice(&(pin.requested.len() as u32).to_be_bytes());
    for point in &pin.requested {
        escrow_prefix(&mut out, point)?;
    }
    out.extend_from_slice(&pin.log.bytes.to_be_bytes());
    out.extend_from_slice(&pin.log.chunks.to_be_bytes());
    out.extend_from_slice(&pin.log.chain_sha256d);
    out.extend_from_slice(&pin.manifest_sha256d);
    Ok(out)
}
fn escrow_hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
fn escrow_validate(pin: &GenerationEscrowPinV1) -> Result<(), StorageError> {
    let root = escrow_directory(&pin.copied_root)?;
    let record = escrow_record(pin)?;
    if double_sha256(&record) != pin.record_sha256d
        || pin.copied_generation != root.join(format!("escrow-{}", escrow_hex(&pin.record_sha256d)))
    {
        return Err(bad("escrow record/path binding"));
    }
    real_dir(&pin.copied_generation)?;
    let record_file = pin.copied_generation.join("RECORD");
    real_file(&record_file)?;
    if fs::read(&record_file)? != record {
        return Err(bad("escrow record bytes"));
    }
    let manifest = pin.copied_generation.join("GENINFO");
    real_file(&manifest)?;
    let raw = fs::read(manifest)?;
    if raw.len() > 4096
        || double_sha256(&raw) != pin.manifest_sha256d
        || parse_manifest(&raw, &pin.original_current.persistent_identity)?
            != pin.original_current.publication
    {
        return Err(bad("escrow original manifest"));
    }
    let copied = pin.original_current.namespace.path(&pin.copied_generation);
    let sub = copied
        .parent()
        .ok_or_else(|| bad("escrow copied namespace"))?;
    real_dir(sub)?;
    if sub.canonicalize()? != sub || sub.parent() != Some(pin.copied_generation.as_path()) {
        return Err(bad("escrow namespace escaped immutable directory"));
    }
    real_file(&copied)?;
    Ok(())
}
fn escrow_journal_pin(
    origin: &VerifiedCompactPrefixV1,
    copied_file: PathBuf,
) -> Result<crate::storage::VerifiedJournalPrefixV1, StorageError> {
    let sequence = origin
        .logical_sequence
        .checked_sub(origin.publication.creation_sequence)
        .and_then(|n| n.checked_add(origin.publication.physical_creation_sequence))
        .ok_or(StorageError::LimitExceeded)?;
    Ok(crate::storage::VerifiedJournalPrefixV1 {
        profile: crate::storage::VERIFIED_PREFIX_OPEN_PROFILE.into(),
        namespace: origin.namespace.clone(),
        canonical_file: copied_file,
        physical_sequence: sequence,
        content: origin.content.clone(),
    })
}
fn escrow_open_current(pin: &GenerationEscrowPinV1) -> Result<Journal, StorageError> {
    escrow_validate(pin)?;
    let file = pin
        .original_current
        .namespace
        .path(&pin.copied_generation)
        .canonicalize()?;
    let current = escrow_journal_pin(&pin.original_current, file)?;
    let (mut journal, _) = Journal::open_existing_verified_prefix(
        &pin.copied_generation,
        pin.original_current.namespace.clone(),
        Recovery::RejectIncompleteTail,
        &current,
    )?;
    // Digest through the very same locked copied-file owner, closing the gap
    // between an unlocked filesystem fingerprint and actual history replay.
    if journal.copy_verified_current_file(&current, None)? != pin.log {
        return Err(bad("locked copied generation fingerprint"));
    }
    Ok(journal)
}
impl GenerationEscrowPinV1 {
    /// Raw expected descriptor for durable external records, never a writer permit.
    pub fn canonical_descriptor_bytes(&self) -> Result<Vec<u8>, StorageError> {
        let raw = escrow_record(self)?;
        if double_sha256(&raw) != self.record_sha256d
            || self.copied_generation
                != self
                    .copied_root
                    .join(format!("escrow-{}", escrow_hex(&self.record_sha256d)))
        {
            return Err(bad("noncanonical escrow pin"));
        }
        Ok(raw)
    }
    /// Explicit original/copy mapping. Never returns an original native writer.
    pub fn open_historical(
        &self,
        expected: &VerifiedCompactPrefixV1,
    ) -> Result<GenerationEscrowSnapshotV1, StorageError> {
        if !self.requested.contains(expected) {
            return Err(bad("unretained independent historical point"));
        }
        let file = self
            .original_current
            .namespace
            .path(&self.copied_generation)
            .canonicalize()?;
        let mut journal = escrow_open_current(self)?;
        let selected = escrow_journal_pin(expected, file.clone())?;
        let data = journal.read_verified_history_prefix(&selected)?;
        Ok(GenerationEscrowSnapshotV1 {
            original: expected.clone(),
            copied_file: file,
            escrow: self.clone(),
            data,
        })
    }
}
impl CompactingJournal {
    /// Call once at an actual publication boundary, not on ordinary row writes.
    pub fn retain_verified_generation(
        &mut self,
        escrow_root: &Path,
        requested: &[VerifiedCompactPrefixV1],
        durability: GenerationEscrowDurabilityV1,
    ) -> Result<DurableGenerationEscrowPermitV1, StorageError> {
        self.check()?;
        let root = escrow_directory(escrow_root)?;
        let catalog = self.catalog.canonicalize()?;
        if root.starts_with(&catalog) || catalog.starts_with(&root) {
            return Err(bad("escrow must be outside original catalog"));
        }
        escrow_sync(&root, durability)?;
        if requested.is_empty() || requested.len() > MAX_ESCROW_POINTS {
            return Err(StorageError::LimitExceeded);
        }
        let current = self.verified_prefix_pin()?;
        for point in requested {
            self.read_verified_history_prefix(point)?;
        }
        // Validate the actual complete current suffix before copying.
        self.read_verified_history_prefix(&current)?;
        let source_info = gen_path(&self.catalog, self.publication.generation).join("GENINFO");
        real_file(&source_info)?;
        let manifest = fs::read(source_info)?;
        if manifest.len() > 4096
            || parse_manifest(&manifest, &self.catalog_identity)? != self.publication
        {
            return Err(bad("escrow source publication"));
        }
        let source_pin = self.inner.verified_prefix_pin()?;
        let fingerprint = self.inner.copy_verified_current_file(&source_pin, None)?;
        let mut pin = GenerationEscrowPinV1 {
            profile: GENERATION_ESCROW_PROFILE.into(),
            original_current: current.clone(),
            requested: requested.to_vec(),
            copied_root: root.clone(),
            copied_generation: PathBuf::new(),
            log: fingerprint,
            manifest_sha256d: double_sha256(&manifest),
            record_sha256d: [0; 32],
            durability,
        };
        let record = escrow_record(&pin)?;
        pin.record_sha256d = double_sha256(&record);
        pin.copied_generation = root.join(format!("escrow-{}", escrow_hex(&pin.record_sha256d)));
        if !pin.copied_generation.exists() {
            let mut entries = 0;
            for entry in fs::read_dir(&root)? {
                let _ = entry?;
                entries += 1;
                if entries > 1024 {
                    return Err(StorageError::LimitExceeded);
                }
            }
            let pending = root.join(format!(
                ".pending-{}-{}",
                std::process::id(),
                next_owner_token()?
            ));
            fs::create_dir(&pending)?;
            let file = self.namespace.path(&pending);
            fs::create_dir(file.parent().ok_or_else(|| bad("escrow namespace"))?)?;
            let copied = self
                .inner
                .copy_verified_current_file(&source_pin, Some(&file))?;
            if copied != pin.log {
                return Err(bad("source changed during generation copy"));
            }
            write_new(&pending.join("GENINFO"), &manifest)?;
            write_new(&pending.join("RECORD"), &record)?;
            escrow_sync(
                file.parent().ok_or_else(|| bad("escrow namespace"))?,
                durability,
            )?;
            escrow_sync(&pending, durability)?;
            // Recheck disk/current owner before publication; no source mutation.
            self.read_verified_history_prefix(&current)?;
            #[cfg(test)]
            if ESCROW_FAIL_BEFORE_PUBLISH.with(|flag| flag.replace(false)) {
                return Err(std::io::Error::other("actual escrow before publication").into());
            }
            fs::rename(&pending, &pin.copied_generation)?;
            escrow_sync(&root, durability)?;
        }
        escrow_validate(&pin)?;
        pin.open_historical(&requested[0])?;
        Ok(DurableGenerationEscrowPermitV1 {
            owner_token: self.owner_token,
            pin,
        })
    }
    fn require_generation_escrow(
        &self,
        permit: &DurableGenerationEscrowPermitV1,
    ) -> Result<Journal, StorageError> {
        self.check()?;
        if permit.owner_token != self.owner_token
            || self.verified_prefix_pin()? != permit.pin.original_current
        {
            return Err(bad("foreign or stale durable escrow permit"));
        }
        escrow_open_current(&permit.pin)
    }
    pub fn publish_compaction_with_escrow(
        &mut self,
        plan: &CompactionPlan,
        permit: &DurableGenerationEscrowPermitV1,
    ) -> Result<CompactionReport, StorageError> {
        let _locked_copy = self.require_generation_escrow(permit)?;
        self.publish_compaction(plan)
    }
    pub fn commit_prepared_native_append_with_escrow(
        &mut self,
        plan: &NativeAppendPlan,
        permit: Option<&DurableGenerationEscrowPermitV1>,
    ) -> Result<NativeAppendPin, StorageError> {
        let _locked_copy = if plan.compaction.is_some() {
            Some(self.require_generation_escrow(
                permit.ok_or_else(|| bad("cross-publication append requires synced escrow"))?,
            )?)
        } else {
            if permit.is_some() {
                return Err(bad("ordinary append must not request a new escrow copy"));
            }
            None
        };
        self.commit_prepared_native_append(plan)
    }
}

include!("compact_generation_independent_prefix_impl.rs");
