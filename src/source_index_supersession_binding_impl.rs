impl SourceCompactIndex {
    #[cfg(test)]
    pub(crate) fn test_supersession_compaction_fault(&mut self, phase: &str) {
        self.journal.test_fault(phase, false);
    }
    #[cfg(test)]
    pub(crate) fn test_supersession_directory_record(&mut self) {
        // Real metadata-only fixture growth; content remains the same.
        let value = self.journal.get(DIRECTORY).unwrap().unwrap().to_vec();
        self.journal
            .commit(vec![Mutation::Put {
                key: DIRECTORY.into(),
                value,
            }])
            .unwrap();
    }
    pub(crate) fn require_actual_native_supersession(
        &self,
        original: &NativePreparedValidityWriteV1,
        observation: &NativePreparedValidityRecoveryObservationV1,
        replacement: &NativePreparedValidityWriteV1,
        mapping: &NativePreparedValiditySupersessionV1,
    ) -> Result<V> {
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
            || mapping.original_before != *original.put.append.before()
            || mapping.original_after != *original.put.append.after()
            || mapping.reopened != observation.snapshot
            || mapping.replacement_before != *replacement.put.append.before()
            || mapping.replacement_after != *replacement.put.append.after()
            || replacement.query != original.query
            || replacement.arguments != original.arguments
            || replacement.context != original.context
            || replacement.before_state != original.before_state
            || replacement.after_state != original.after_state
            || replacement.read.pin() != original.read.pin()
            || replacement.read.directory().encode()? != original.read.directory().encode()?
            || prepared_metadata_state_entries(replacement.read.raw_entries(), &[])?
                != original.before_state
            || replacement.put.directory.encode()? != original.put.directory.encode()?
            || replacement.put.append.mutations() != original.put.append.mutations()
            || replacement.put.append.configuration().target_log_bytes
                != original.put.append.configuration().target_log_bytes
            || replacement.put.append.configuration().retained_ancestors
                != original.put.append.configuration().retained_ancestors
            || replacement.recovery_before != original.verified_reopen_pin(observation.candidate)?
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.require_snapshot(&observation.snapshot)?;
        self.require_snapshot(&replacement.put.expected)?;
        self.journal
            .require_superseding_append_owner(&replacement.put.append)?;
        let pin = self.journal.verified_prefix_pin()?;
        Ok(prepared_obj(vec![
            ("profile", prepared_text(PREPARED_VALIDITY_REOPEN_PROFILE)),
            (
                "matched",
                prepared_text(match observation.candidate {
                    NativePreparedValidityCandidateV1::Before => "Before",
                    NativePreparedValidityCandidateV1::Intermediate => "Intermediate",
                    NativePreparedValidityCandidateV1::Post => {
                        return Err(CompactIndexError::WrongContext)
                    }
                }),
            ),
            ("identity", prepared_text(&pin.persistent_identity)),
            (
                "catalog",
                prepared_text(pin.canonical_catalog.to_string_lossy()),
            ),
            (
                "namespace",
                prepared_text(pin.namespace.path(Path::new("")).to_string_lossy()),
            ),
            ("snapshot", prepared_snapshot_value(&observation.snapshot)),
            ("completeState", prepared_state_value(&observation.content)),
            (
                "discardedBytes",
                prepared_text(observation.recovery.discarded_bytes.to_string()),
            ),
        ]))
    }
}
