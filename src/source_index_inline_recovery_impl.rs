// Actual inline DB-operation recovery. Reopens only a previously sealed native
// candidate, retains source-method PC separately, and never retries source code.
pub const INLINE_DATABASE_REOPEN_PROFILE: &str = "TL-NATIVE-INLINE-DATABASE-REOPEN-13-1";
pub struct NativeInlineRecoveryObservationV1 {
    candidate: NativePreparedValidityCandidateV1,
    snapshot: MetadataSnapshot,
    content: NativeMetadataStateReceiptV1,
    recovery: crate::storage::RecoveryReport,
    original_before: crate::compact_journal::VerifiedCompactPrefixV1,
    original_after: crate::compact_journal::VerifiedCompactPrefixV1,
}
impl NativeInlineRecoveryObservationV1 {
    pub fn candidate(&self) -> NativePreparedValidityCandidateV1 {
        self.candidate
    }
    pub fn snapshot(&self) -> &MetadataSnapshot {
        &self.snapshot
    }
    pub fn recovery_report(&self) -> &crate::storage::RecoveryReport {
        &self.recovery
    }
}
pub struct NativeInlineSupersessionV1 {
    candidate: NativePreparedValidityCandidateV1,
    original_before: crate::compact_journal::NativeAppendPin,
    original_after: crate::compact_journal::NativeAppendPin,
    reopened: MetadataSnapshot,
    replacement_before: crate::compact_journal::NativeAppendPin,
    replacement_after: crate::compact_journal::NativeAppendPin,
}
impl NativePreparedInlineDatabaseV1 {
    pub fn verified_reopen_pin(
        &self,
        candidate: NativePreparedValidityCandidateV1,
    ) -> Result<crate::compact_journal::VerifiedCompactPrefixV1> {
        let put = self.put.as_ref().ok_or(CompactIndexError::WrongContext)?;
        let mut pin = self.recovery_before.clone();
        let (identity, catalog) = put.append.catalog_context();
        if pin.profile != crate::compact_journal::VERIFIED_COMPACT_PREFIX_OPEN_PROFILE
            || pin.persistent_identity != identity
            || pin.canonical_catalog != catalog
            || pin.publication != put.append.before().publication
            || pin.logical_sequence != put.append.before().logical_sequence
            || pin.content != prepared_storage_receipt(&self.before_state)
            || self.expected.profile != METADATA_PROFILE
            || self.expected.context != self.before_directory.context
            || self.expected.publication != pin.publication
            || self.expected.sequence != pin.logical_sequence
            || self.context.0 != identity
            || self.context.1 != catalog
        {
            return Err(CompactIndexError::WrongContext);
        }
        let (append, content) = match candidate {
            NativePreparedValidityCandidateV1::Before => {
                (put.append.before().clone(), &self.before_state)
            }
            NativePreparedValidityCandidateV1::Intermediate => (
                put.append
                    .intermediate()
                    .ok_or(CompactIndexError::WrongContext)?,
                &self.before_state,
            ),
            NativePreparedValidityCandidateV1::Post => {
                (put.append.after().clone(), &self.after_state)
            }
        };
        pin.publication = append.publication;
        pin.logical_sequence = append.logical_sequence;
        pin.content = prepared_storage_receipt(content);
        Ok(pin)
    }
}
impl SourceCompactIndex {
    pub fn reopen_inline_database(
        root: &Path,
        namespace: Namespace,
        plan: &NativePreparedInlineDatabaseV1,
        candidate: NativePreparedValidityCandidateV1,
        recovery: Recovery,
    ) -> Result<(Self, NativeInlineRecoveryObservationV1)> {
        require_namespace(root, &namespace)?;
        let expected = plan.verified_reopen_pin(candidate)?;
        if namespace != expected.namespace {
            return Err(CompactIndexError::WrongContext);
        }
        let put = plan.put.as_ref().ok_or(CompactIndexError::WrongContext)?;
        let (mut journal, report) =
            Journal::open_existing_verified_prefix(root, namespace, recovery, &expected)?;
        journal.configure(put.append.configuration().clone())?;
        let directory =
            Directory::decode(journal.get(DIRECTORY)?.ok_or(CompactIndexError::Corrupt)?)?;
        let expected_directory = if candidate == NativePreparedValidityCandidateV1::Post {
            &put.directory
        } else {
            &plan.before_directory
        };
        if directory.context != plan.expected.context
            || directory.encode()? != expected_directory.encode()?
        {
            return Err(CompactIndexError::WrongContext);
        }
        let index = Self {
            journal,
            directory,
            cold_execution: None,
            inline_escrow_guard: None,
        };
        index.audit()?;
        let snapshot = index.snapshot()?;
        let content = index.native_complete_metadata_state()?;
        if snapshot.profile != plan.expected.profile
            || snapshot.publication != expected.publication
            || snapshot.sequence != expected.logical_sequence
            || prepared_storage_receipt(&content) != expected.content
        {
            return Err(CompactIndexError::WrongContext);
        }
        let observed = NativeInlineRecoveryObservationV1 {
            candidate,
            snapshot,
            content,
            recovery: report,
            original_before: plan.verified_reopen_pin(NativePreparedValidityCandidateV1::Before)?,
            original_after: plan.verified_reopen_pin(NativePreparedValidityCandidateV1::Post)?,
        };
        Ok((index, observed))
    }
    /// Reprepare only the exact retained native page/directory mutations. The
    /// original source query, RPC/decoder, failure hook and result are not rerun.
    pub fn supersede_inline_database(
        &self,
        original: &NativePreparedInlineDatabaseV1,
        observation: &NativeInlineRecoveryObservationV1,
    ) -> Result<(NativePreparedInlineDatabaseV1, NativeInlineSupersessionV1)> {
        if observation.candidate == NativePreparedValidityCandidateV1::Post
            || observation.original_before
                != original.verified_reopen_pin(NativePreparedValidityCandidateV1::Before)?
            || observation.original_after
                != original.verified_reopen_pin(NativePreparedValidityCandidateV1::Post)?
            || observation.content != original.before_state
            || self.native_complete_metadata_state()? != original.before_state
            || self.native_store_context()? != original.context
            || self.journal.verified_prefix_pin()?
                != original.verified_reopen_pin(observation.candidate)?
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.require_snapshot(&observation.snapshot)?;
        let put = original
            .put
            .as_ref()
            .ok_or(CompactIndexError::WrongContext)?;
        let append = self
            .journal
            .prepare_native_append(put.append.mutations().to_vec())?;
        let recovery_before =
            self.native_capture_validity_recovery(&append, &original.before_state)?;
        let mapping = NativeInlineSupersessionV1 {
            candidate: observation.candidate,
            original_before: put.append.before().clone(),
            original_after: put.append.after().clone(),
            reopened: observation.snapshot.clone(),
            replacement_before: append.before().clone(),
            replacement_after: append.after().clone(),
        };
        let replacement = NativePreparedInlineDatabaseV1 {
            context: original.context.clone(),
            expected: self.snapshot()?,
            method: original.method.clone(),
            arguments: original.arguments.clone(),
            before_state: original.before_state.clone(),
            after_state: original.after_state.clone(),
            before_directory: original.before_directory.clone(),
            recovery_before,
            put: Some(NativeMetadataPutPlanV1 {
                expected: self.snapshot()?,
                directory: put.directory.clone(),
                append,
            }),
            returned: original.returned.clone(),
        };
        self.require_inline_native_supersession(original, observation, &replacement, &mapping)?;
        Ok((replacement, mapping))
    }
    pub fn require_inline_native_supersession(
        &self,
        original: &NativePreparedInlineDatabaseV1,
        observation: &NativeInlineRecoveryObservationV1,
        replacement: &NativePreparedInlineDatabaseV1,
        mapping: &NativeInlineSupersessionV1,
    ) -> Result<V> {
        let old = original
            .put
            .as_ref()
            .ok_or(CompactIndexError::WrongContext)?;
        let new = replacement
            .put
            .as_ref()
            .ok_or(CompactIndexError::WrongContext)?;
        if observation.candidate == NativePreparedValidityCandidateV1::Post
            || mapping.candidate != observation.candidate
            || observation.original_before
                != original.verified_reopen_pin(NativePreparedValidityCandidateV1::Before)?
            || observation.original_after
                != original.verified_reopen_pin(NativePreparedValidityCandidateV1::Post)?
            || observation.content != original.before_state
            || self.native_complete_metadata_state()? != original.before_state
            || self.native_store_context()? != original.context
            || self.journal.verified_prefix_pin()?
                != original.verified_reopen_pin(observation.candidate)?
            || mapping.original_before != *old.append.before()
            || mapping.original_after != *old.append.after()
            || mapping.reopened != observation.snapshot
            || mapping.replacement_before != *new.append.before()
            || mapping.replacement_after != *new.append.after()
            || replacement.method != original.method
            || replacement.arguments != original.arguments
            || replacement.returned != original.returned
            || replacement.context != original.context
            || replacement.before_state != original.before_state
            || replacement.after_state != original.after_state
            || replacement.before_directory.encode()? != original.before_directory.encode()?
            || new.directory.encode()? != old.directory.encode()?
            || new.append.mutations() != old.append.mutations()
            || new.append.configuration().target_log_bytes
                != old.append.configuration().target_log_bytes
            || new.append.configuration().retained_ancestors
                != old.append.configuration().retained_ancestors
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.require_snapshot(&observation.snapshot)?;
        self.require_snapshot(&replacement.expected)?;
        self.require_snapshot(&new.expected)?;
        self.journal.require_superseding_append_owner(&new.append)?;
        Ok(prepared_obj(vec![
            ("profile", prepared_text(INLINE_DATABASE_REOPEN_PROFILE)),
            ("method", prepared_text(&original.method)),
            ("arguments", V::Array(original.arguments.clone())),
            ("snapshot", prepared_snapshot_value(&observation.snapshot)),
            ("completeState", prepared_state_value(&observation.content)),
            (
                "discardedBytes",
                prepared_text(observation.recovery.discarded_bytes.to_string()),
            ),
        ]))
    }
    pub(crate) fn require_inline_installation(
        &self,
        original: &NativePreparedInlineDatabaseV1,
        replacement: &NativePreparedInlineDatabaseV1,
        mapping: &NativeInlineSupersessionV1,
    ) -> Result<()> {
        let old = original
            .put
            .as_ref()
            .ok_or(CompactIndexError::WrongContext)?;
        let new = replacement
            .put
            .as_ref()
            .ok_or(CompactIndexError::WrongContext)?;
        if mapping.candidate == NativePreparedValidityCandidateV1::Post
            || self.journal.verified_prefix_pin()?
                != original.verified_reopen_pin(mapping.candidate)?
            || self.native_store_context()? != original.context
            || self.native_complete_metadata_state()? != original.before_state
            || mapping.original_before != *old.append.before()
            || mapping.original_after != *old.append.after()
            || mapping.replacement_before != *new.append.before()
            || mapping.replacement_after != *new.append.after()
            || replacement.method != original.method
            || replacement.arguments != original.arguments
            || replacement.returned != original.returned
            || replacement.context != original.context
            || replacement.before_state != original.before_state
            || replacement.after_state != original.after_state
            || replacement.before_directory.encode()? != original.before_directory.encode()?
            || new.directory.encode()? != old.directory.encode()?
            || new.append.mutations() != old.append.mutations()
            || new.append.configuration().target_log_bytes
                != old.append.configuration().target_log_bytes
            || new.append.configuration().retained_ancestors
                != old.append.configuration().retained_ancestors
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.require_snapshot(&mapping.reopened)?;
        self.require_snapshot(&replacement.expected)?;
        self.require_snapshot(&new.expected)?;
        self.journal.require_superseding_append_owner(&new.append)?;
        Ok(())
    }
}
