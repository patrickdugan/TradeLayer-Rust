pub const VERIFIED_HISTORY_READ_PROFILE: &str = "TL-TXINDEX-VERIFIED-COLD-HISTORY-READ-13-1";
/// Independent integrity expectation captured from an actual healthy owner.
/// This contains metadata receipts only, never financial state/source answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceIndexHistoryPinV1 {
    pub profile: String,
    pub metadata: MetadataSnapshot,
    pub storage: crate::compact_journal::VerifiedCompactPrefixV1,
    pub rows: RowStreamReceipt,
}
impl SourceIndexHistoryPinV1 {
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor("profile", &prepared_text(&self.profile))?;
        native_row_binding_value(
            "metadata",
            &prepared_snapshot_value(&self.metadata),
            visitor,
        )?;
        visitor("storage/profile", &prepared_text(&self.storage.profile))?;
        visitor(
            "storage/namespace",
            &prepared_text(self.storage.namespace.path(Path::new("")).to_string_lossy()),
        )?;
        visitor(
            "storage/identity",
            &prepared_text(&self.storage.persistent_identity),
        )?;
        visitor(
            "storage/catalog",
            &prepared_text(self.storage.canonical_catalog.to_string_lossy()),
        )?;
        visitor(
            "storage/logicalSequence",
            &prepared_text(self.storage.logical_sequence.to_string()),
        )?;
        let publication = &self.storage.publication;
        for (name, value) in [
            (
                "generation",
                prepared_text(publication.generation.to_string()),
            ),
            (
                "parent",
                publication
                    .parent
                    .map_or(V::Null, |v| prepared_text(v.to_string())),
            ),
            (
                "physicalCreation",
                prepared_text(publication.physical_creation_sequence.to_string()),
            ),
            (
                "logicalCreation",
                prepared_text(publication.creation_sequence.to_string()),
            ),
            (
                "manifest",
                prepared_text(prepared_hex(&publication.manifest_sha256d)),
            ),
        ] {
            visitor(&format!("storage/publication/{name}"), &value)?;
        }
        let content = &self.storage.content;
        for (name, value) in [
            ("profile", prepared_text(&content.profile)),
            ("keys", prepared_text(content.keys.to_string())),
            ("bytes", prepared_text(content.bytes.to_string())),
            ("chain", prepared_text(prepared_hex(&content.chain_sha256d))),
            ("complete", V::Bool(content.complete)),
        ] {
            visitor(&format!("storage/content/{name}"), &value)?;
        }
        visitor("rows/profile", &prepared_text(&self.rows.profile))?;
        native_row_binding_value(
            "rows/snapshot",
            &prepared_snapshot_value(&self.rows.snapshot),
            visitor,
        )?;
        visitor("rows/count", &prepared_text(self.rows.rows.to_string()))?;
        visitor(
            "rows/chain",
            &prepared_text(prepared_hex(&self.rows.chain_sha256d)),
        )?;
        visitor("rows/complete", &V::Bool(self.rows.complete))
    }
}
impl SourceCompactIndex {
    /// Cold current owner from actual files and independently retained complete
    /// current facts. No creation, repair, candidate selection or state import.
    pub fn open_verified_history_owner(
        root: &Path,
        namespace: Namespace,
        expected_current: &SourceIndexHistoryPinV1,
    ) -> Result<Self> {
        require_namespace(root, &namespace)?;
        if expected_current.profile != VERIFIED_HISTORY_READ_PROFILE
            || expected_current.storage.namespace != namespace
        {
            return Err(CompactIndexError::WrongContext);
        }
        let (journal, _) = Journal::open_existing_verified_prefix(
            root,
            namespace,
            Recovery::RejectIncompleteTail,
            &expected_current.storage,
        )?;
        let directory =
            Directory::decode(journal.get(DIRECTORY)?.ok_or(CompactIndexError::Corrupt)?)?;
        let owner = Self {
            journal,
            directory,
            cold_execution: None,
            inline_escrow_guard: None,
        };
        owner.require_snapshot(&expected_current.metadata)?;
        if owner.capture_verified_history_pin()? != *expected_current {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(owner)
    }
    pub fn capture_verified_history_pin(&self) -> Result<SourceIndexHistoryPinV1> {
        self.audit()?;
        let metadata = self.snapshot()?;
        let mut cursor = self.read_generation(&metadata)?.cursor();
        while cursor.next_row()?.is_some() {}
        let rows = cursor.receipt();
        if !rows.complete {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(SourceIndexHistoryPinV1 {
            profile: VERIFIED_HISTORY_READ_PROFILE.into(),
            metadata,
            storage: self.journal.verified_prefix_pin()?,
            rows,
        })
    }
    /// Read only actual frames from the same current publication. Unavailable
    /// old generations are refused, not reconstructed from a newer row state.
    pub fn read_verified_history_generation(
        &mut self,
        expected: &SourceIndexHistoryPinV1,
    ) -> Result<SourceIndexReadSnapshot> {
        if expected.profile != VERIFIED_HISTORY_READ_PROFILE
            || expected.metadata.profile != METADATA_PROFILE
            || expected.metadata.context != self.directory.context
            || expected.metadata.publication != expected.storage.publication
            || expected.metadata.sequence != expected.storage.logical_sequence
            || expected.rows.snapshot != expected.metadata
            || expected.rows.profile != STREAM_PROFILE
            || !expected.rows.complete
        {
            return Err(CompactIndexError::WrongContext);
        }
        let data = self
            .journal
            .read_verified_history_prefix(&expected.storage)?;
        let directory = Directory::decode(data.get(DIRECTORY).ok_or(CompactIndexError::Corrupt)?)?;
        if directory.context != expected.metadata.context
            || data.entries().count() != directory.pages.len() + 1
        {
            return Err(CompactIndexError::Corrupt);
        }
        let snapshot = SourceIndexReadSnapshot {
            pin: expected.metadata.clone(),
            directory,
            data,
        };
        let mut cursor = snapshot.cursor();
        let mut last = None;
        while let Some(row) = cursor.next_row()? {
            let id = row_id(&row)?;
            if last.as_ref().is_some_and(|prior: &String| prior >= &id) {
                return Err(CompactIndexError::Corrupt);
            }
            last = Some(id);
        }
        if cursor.receipt() != expected.rows {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(snapshot)
    }
}
