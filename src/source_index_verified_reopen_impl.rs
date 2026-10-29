// Metadata-only recovery of a sealed actual validity preparation. A matching
// fingerprint is integrity relative to that preparation, never authentication.
pub const PREPARED_VALIDITY_REOPEN_PROFILE: &str = "TL-TXINDEX-PREPARED-VALIDITY-REOPEN-13-1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePreparedValidityCandidateV1 {
    Before,
    Intermediate,
    Post,
}

/// Constructible only after a checked open of an actual prepared candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePreparedValidityRecoveryObservationV1 {
    candidate: NativePreparedValidityCandidateV1,
    snapshot: MetadataSnapshot,
    content: NativeMetadataStateReceiptV1,
    recovery: crate::storage::RecoveryReport,
    original_before: crate::compact_journal::VerifiedCompactPrefixV1,
    original_after: crate::compact_journal::VerifiedCompactPrefixV1,
}
impl NativePreparedValidityRecoveryObservationV1 {
    pub fn profile(&self) -> &'static str {
        PREPARED_VALIDITY_REOPEN_PROFILE
    }
    pub fn candidate(&self) -> NativePreparedValidityCandidateV1 {
        self.candidate
    }
    pub fn snapshot(&self) -> &MetadataSnapshot {
        &self.snapshot
    }
    pub fn content(&self) -> &NativeMetadataStateReceiptV1 {
        &self.content
    }
    pub fn recovery_report(&self) -> &crate::storage::RecoveryReport {
        &self.recovery
    }
    pub fn require_authenticated_admission(&self) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}

fn prepared_storage_receipt(
    receipt: &NativeMetadataStateReceiptV1,
) -> crate::storage::JournalCompleteKvReceiptV1 {
    // Copy all fields; an invalid profile or incomplete flag must remain invalid.
    crate::storage::JournalCompleteKvReceiptV1 {
        profile: receipt.profile.clone(),
        keys: receipt.keys,
        bytes: receipt.bytes,
        chain_sha256d: receipt.chain_sha256d,
        complete: receipt.complete,
    }
}

impl NativePreparedValidityWriteV1 {
    fn verified_reopen_pin(
        &self,
        candidate: NativePreparedValidityCandidateV1,
    ) -> Result<crate::compact_journal::VerifiedCompactPrefixV1> {
        let mut pin = self.recovery_before.clone();
        let (identity, catalog) = self.put.append.catalog_context();
        if pin.profile != crate::compact_journal::VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || pin.persistent_identity != identity
            || pin.canonical_catalog != catalog
            || pin.publication != self.put.append.before().publication
            || pin.logical_sequence != self.put.append.before().logical_sequence
            || pin.content != prepared_storage_receipt(&self.before_state)
            || self.put.expected.profile != METADATA_PROFILE
            || self.put.expected.context != self.read.directory.context
            || self.put.expected.publication != pin.publication
            || self.put.expected.sequence != pin.logical_sequence
            || self.context.0 != identity
            || self.context.1 != catalog
        {
            return Err(CompactIndexError::WrongContext);
        }
        let (append, receipt) = match candidate {
            NativePreparedValidityCandidateV1::Before => {
                (self.put.append.before().clone(), &self.before_state)
            }
            NativePreparedValidityCandidateV1::Intermediate => (
                self.put
                    .append
                    .intermediate()
                    .ok_or(CompactIndexError::WrongContext)?,
                &self.before_state,
            ),
            NativePreparedValidityCandidateV1::Post => {
                (self.put.append.after().clone(), &self.after_state)
            }
        };
        pin.publication = append.publication;
        pin.logical_sequence = append.logical_sequence;
        pin.content = prepared_storage_receipt(receipt);
        // The storage profile derives physical sequence under the exclusive
        // writer lock from these independently prepared logical/publication facts.
        Ok(pin)
    }
}

impl SourceCompactIndex {
    fn native_capture_validity_recovery(
        &self,
        append: &crate::compact_journal::NativeAppendPlan,
        before: &NativeMetadataStateReceiptV1,
    ) -> Result<crate::compact_journal::VerifiedCompactPrefixV1> {
        let pin = self.journal.verified_prefix_pin()?;
        let (identity, catalog) = append.catalog_context();
        if pin.content != prepared_storage_receipt(before)
            || pin.persistent_identity != identity
            || pin.canonical_catalog != catalog
            || pin.publication != append.before().publication
            || pin.logical_sequence != append.before().logical_sequence
        {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(pin)
    }

    /// Drop the previous mutable index first. Old immutable readers may remain.
    /// The caller chooses an independently expected actual plan candidate; this
    /// function never discovers a new expectation from unknown recovery bytes.
    pub fn reopen_prepared_validity(
        root: &Path,
        namespace: Namespace,
        plan: &NativePreparedValidityWriteV1,
        candidate: NativePreparedValidityCandidateV1,
        recovery: Recovery,
    ) -> Result<(Self, NativePreparedValidityRecoveryObservationV1)> {
        require_namespace(root, &namespace)?;
        let expected = plan.verified_reopen_pin(candidate)?;
        if namespace != expected.namespace {
            return Err(CompactIndexError::WrongContext);
        }
        let (mut journal, report) =
            Journal::open_existing_verified_prefix(root, namespace, recovery, &expected)?;
        journal.configure(plan.put.append.configuration().clone())?;
        let directory =
            Directory::decode(journal.get(DIRECTORY)?.ok_or(CompactIndexError::Corrupt)?)?;
        let expected_directory = if candidate == NativePreparedValidityCandidateV1::Post {
            &plan.put.directory
        } else {
            &plan.read.directory
        };
        if directory.context != plan.put.expected.context
            || directory.encode()? != expected_directory.encode()?
        {
            return Err(CompactIndexError::WrongContext);
        }
        let index = Self {
            journal,
            directory,
            cold_execution: None,
        };
        index.audit()?;
        let snapshot = index.snapshot()?;
        let content = index.native_complete_metadata_state()?;
        if snapshot.profile != plan.put.expected.profile
            || snapshot.publication != expected.publication
            || snapshot.sequence != expected.logical_sequence
            || prepared_storage_receipt(&content) != expected.content
        {
            return Err(CompactIndexError::WrongContext);
        }
        let observation = NativePreparedValidityRecoveryObservationV1 {
            candidate,
            snapshot,
            content,
            recovery: report,
            original_before: plan.verified_reopen_pin(NativePreparedValidityCandidateV1::Before)?,
            original_after: plan.verified_reopen_pin(NativePreparedValidityCandidateV1::Post)?,
        };
        Ok((index, observation))
    }
}

/// Metadata mapping only. It neither acknowledges nor executes the source await.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePreparedValiditySupersessionV1 {
    original_before: crate::compact_journal::NativeAppendPin,
    original_after: crate::compact_journal::NativeAppendPin,
    reopened: MetadataSnapshot,
    replacement_before: crate::compact_journal::NativeAppendPin,
    replacement_after: crate::compact_journal::NativeAppendPin,
}
impl NativePreparedValiditySupersessionV1 {
    pub fn original_before(&self) -> &crate::compact_journal::NativeAppendPin {
        &self.original_before
    }
    pub fn original_after(&self) -> &crate::compact_journal::NativeAppendPin {
        &self.original_after
    }
    pub fn reopened(&self) -> &MetadataSnapshot {
        &self.reopened
    }
    pub fn replacement_before(&self) -> &crate::compact_journal::NativeAppendPin {
        &self.replacement_before
    }
    pub fn replacement_after(&self) -> &crate::compact_journal::NativeAppendPin {
        &self.replacement_after
    }
}
impl SourceCompactIndex {
    /// Reprepare only the retained actual native mutation on a checked pre/post-
    /// compaction BEFORE state. No source query, failure check, or suffix
    /// selection is rerun; a new durable intent is still required before commit.
    pub fn supersede_prepared_validity(
        &self,
        original: &NativePreparedValidityWriteV1,
        observation: &NativePreparedValidityRecoveryObservationV1,
    ) -> Result<(
        NativePreparedValidityWriteV1,
        NativePreparedValiditySupersessionV1,
    )> {
        if observation.candidate == NativePreparedValidityCandidateV1::Post
            || observation.original_before
                != original.verified_reopen_pin(NativePreparedValidityCandidateV1::Before)?
            || observation.original_after
                != original.verified_reopen_pin(NativePreparedValidityCandidateV1::Post)?
            || observation.content != original.before_state
            || self.native_store_context()? != original.context
            || self.native_complete_metadata_state()? != original.before_state
            || self.journal.verified_prefix_pin()?
                != original.verified_reopen_pin(observation.candidate)?
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.require_snapshot(&observation.snapshot)?;
        let append = self
            .journal
            .prepare_native_append(original.put.append.mutations().to_vec())?;
        let recovery_before =
            self.native_capture_validity_recovery(&append, &original.before_state)?;
        let mapping = NativePreparedValiditySupersessionV1 {
            original_before: original.put.append.before().clone(),
            original_after: original.put.append.after().clone(),
            reopened: observation.snapshot.clone(),
            replacement_before: append.before().clone(),
            replacement_after: append.after().clone(),
        };
        let replacement = NativePreparedValidityWriteV1 {
            read: original.read.clone(),
            query: original.query.clone(),
            arguments: original.arguments.clone(),
            context: original.context.clone(),
            put: NativeMetadataPutPlanV1 {
                expected: self.snapshot()?,
                directory: original.put.directory.clone(),
                append,
            },
            before_state: original.before_state.clone(),
            after_state: original.after_state.clone(),
            recovery_before,
        };
        Ok((replacement, mapping))
    }
}
