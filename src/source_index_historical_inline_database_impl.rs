// Source-computed historical metadata only. This type cannot append, install a
// writer token, or advance a financial/source PC.
pub const HISTORICAL_INLINE_DATABASE_PROFILE: &str = "TL-TXINDEX-HISTORICAL-INLINE-DATABASE-13-1";
fn historical_inline_visit_bytes(
    path: &str,
    bytes: &[u8],
    visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
) -> std::result::Result<(), SourceRuleError> {
    const CHUNK: usize = 16 * 1024;
    visitor(
        &format!("{path}/bytes"),
        &prepared_text(bytes.len().to_string()),
    )?;
    visitor(
        &format!("{path}/chunks"),
        &prepared_text(bytes.len().div_ceil(CHUNK).to_string()),
    )?;
    for (at, chunk) in bytes.chunks(CHUNK).enumerate() {
        visitor(
            &format!("{path}/chunk/{at}"),
            &prepared_text(prepared_hex(chunk)),
        )?;
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoricalInlineDatabasePhaseV1 {
    Prepared,
    PostVerified,
    Resolved,
}
pub struct NativeHistoricalInlineDatabaseV1 {
    original: SourceIndexHistoryPinV1,
    copied: Option<crate::compact_journal::GenerationEscrowPinV1>,
    method: String,
    arguments: Vec<V>,
    before_state: NativeMetadataStateReceiptV1,
    after_state: NativeMetadataStateReceiptV1,
    directory: Directory,
    mutations: Vec<Mutation>,
    raw_mutations: Vec<u8>,
    returned: DependencyReturn,
    phase: HistoricalInlineDatabasePhaseV1,
    verified_post: Option<SourceIndexHistoryPinV1>,
}
impl VerifiedOriginalMetadataViewV1 {
    pub fn prepare_historical_inline_database(
        &self,
        method: &str,
        arguments: Vec<V>,
    ) -> Result<NativeHistoricalInlineDatabaseV1> {
        if arguments.len() > 3 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        for value in &arguments {
            value.validate()?;
        }
        let before_state = prepared_metadata_state_entries(self.raw_entries(), &[])?;
        if prepared_storage_receipt(&before_state) != self.original().storage.content {
            return Err(CompactIndexError::WrongContext);
        }
        let (row, returned) =
            inline_database_source_outcome(method, &arguments, |id| self.get(id))?;
        let (directory, mutations) = if let Some(row) = row {
            prepared_metadata_mutations(self.directory(), row, |_, page| {
                decode_page(
                    self.raw_get(&page_key(page.number))
                        .ok_or(CompactIndexError::Corrupt)?,
                )
            })?
        } else {
            (self.directory().clone(), vec![])
        };
        let after_state = prepared_metadata_state_entries(self.raw_entries(), &mutations)?;
        let raw_mutations =
            crate::source_main_native_intent_store::native_prepared_mutation_bytes(&mutations)
                .map_err(|_| CompactIndexError::Limit)?;
        Ok(NativeHistoricalInlineDatabaseV1 {
            original: self.original().clone(),
            copied: self.copied_generation().cloned(),
            method: method.into(),
            arguments,
            before_state,
            after_state,
            directory,
            mutations,
            raw_mutations,
            returned,
            phase: HistoricalInlineDatabasePhaseV1::Prepared,
            verified_post: None,
        })
    }
}
impl NativeHistoricalInlineDatabaseV1 {
    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn arguments(&self) -> &[V] {
        &self.arguments
    }
    pub fn original_read(&self) -> &SourceIndexHistoryPinV1 {
        &self.original
    }
    pub fn before_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.before_state
    }
    pub fn after_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.after_state
    }
    pub fn requires_native_write(&self) -> bool {
        !self.mutations.is_empty()
    }
    pub fn exact_prepared_mutations(&self) -> &[u8] {
        &self.raw_mutations
    }
    pub fn durable_read_context(&self) -> V {
        // This is the existing live read-context shape, derived from ORIGINAL
        // history and actual source arguments, never the copied file identity.
        prepared_obj(vec![
            ("profile", prepared_text(INLINE_DATABASE_PROFILE)),
            (
                "context",
                prepared_text(&self.original.storage.persistent_identity),
            ),
            (
                "canonicalFile",
                prepared_text(self.original.storage.canonical_catalog.to_string_lossy()),
            ),
            ("expected", prepared_snapshot_value(&self.original.metadata)),
            ("method", prepared_text(&self.method)),
            ("arguments", V::Array(self.arguments.clone())),
            ("before", prepared_state_value(&self.before_state)),
            ("after", prepared_state_value(&self.after_state)),
        ])
    }
    pub fn append_verified(&self) -> bool {
        self.phase == HistoricalInlineDatabasePhaseV1::PostVerified
    }
    pub fn resolved(&self) -> bool {
        self.phase == HistoricalInlineDatabasePhaseV1::Resolved
    }
    /// Check an independently bound actual Post and exact retained TLNM bytes.
    /// This establishes a source metadata transition, not the native candidate
    /// lineage, action selector, Accepted/ACK chain or append authorization.
    pub fn verify_recorded_native_append(
        &mut self,
        post: &VerifiedOriginalMetadataViewV1,
        expected_raw_mutations: &[u8],
    ) -> Result<()> {
        let actual = post.original();
        if self.phase != HistoricalInlineDatabasePhaseV1::Prepared
            || !self.requires_native_write()
            || expected_raw_mutations != self.raw_mutations
            || actual.storage.namespace != self.original.storage.namespace
            || actual.storage.persistent_identity != self.original.storage.persistent_identity
            || actual.storage.canonical_catalog != self.original.storage.canonical_catalog
            || actual.metadata.profile != self.original.metadata.profile
            || actual.metadata.context != self.original.metadata.context
            || self.original.metadata.sequence.checked_add(1) != Some(actual.metadata.sequence)
            || prepared_metadata_state_entries(post.raw_entries(), &[])? != self.after_state
            || post.directory().encode()? != self.directory.encode()?
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.verified_post = Some(actual.clone());
        self.phase = HistoricalInlineDatabasePhaseV1::PostVerified;
        Ok(())
    }
    /// Return the computed DB outcome once, separately from Post verification.
    /// This method has no source-owner, native writer, or financial access.
    pub fn resolve_recorded_post(&mut self) -> Result<DependencyReturn> {
        let required = if self.requires_native_write() {
            HistoricalInlineDatabasePhaseV1::PostVerified
        } else {
            HistoricalInlineDatabasePhaseV1::Prepared
        };
        if self.phase != required {
            return Err(CompactIndexError::WrongContext);
        }
        self.phase = HistoricalInlineDatabasePhaseV1::Resolved;
        Ok(self.returned.clone())
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor(
            "profile",
            &prepared_text(HISTORICAL_INLINE_DATABASE_PROFILE),
        )?;
        visitor(
            "phase",
            &prepared_text(match self.phase {
                HistoricalInlineDatabasePhaseV1::Prepared => "Prepared",
                HistoricalInlineDatabasePhaseV1::PostVerified => "PostVerified",
                HistoricalInlineDatabasePhaseV1::Resolved => "Resolved",
            }),
        )?;
        native_row_binding_value("sourceReadContext", &self.durable_read_context(), visitor)?;
        self.original.visit_canonical_fields(&mut |path, value| {
            visitor(&format!("originalRead/{path}"), value)
        })?;
        visitor("copiedGeneration", &V::Bool(self.copied.is_some()))?;
        if let Some(copy) = &self.copied {
            let raw = copy
                .canonical_descriptor_bytes()
                .map_err(|_| SourceRuleError::Backend)?;
            historical_inline_visit_bytes("copiedDescriptor", &raw, visitor)?;
            visitor(
                "copiedDescriptorSha256d",
                &prepared_text(prepared_hex(&copy.record_sha256d)),
            )?;
        }
        visitor("write", &V::Bool(self.requires_native_write()))?;
        historical_inline_visit_bytes(
            "directory",
            &self
                .directory
                .encode()
                .map_err(|_| SourceRuleError::Backend)?,
            visitor,
        )?;
        historical_inline_visit_bytes("mutations", &self.raw_mutations, visitor)?;
        visitor("post", &V::Bool(self.verified_post.is_some()))?;
        if let Some(post) = &self.verified_post {
            post.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("originalPost/{path}"), value)
            })?;
        }
        let (kind, value, error) = match &self.returned {
            DependencyReturn::Immediate(value) => ("Immediate", Some(value), None),
            DependencyReturn::Pending(value) => ("Pending", Some(value), None),
            DependencyReturn::Throw(error) => ("Throw", None, Some(error)),
            DependencyReturn::PendingThrow(error) => ("PendingThrow", None, Some(error)),
        };
        visitor("returned/kind", &prepared_text(kind))?;
        if let Some(value) = value {
            native_row_binding_value("returned/value", value, visitor)?;
        }
        if let Some(error) = error {
            visitor("returned/name", &prepared_text(&error.name))?;
            visitor("returned/message", &prepared_text(&error.message))?;
        }
        Ok(())
    }
}
