// Exact actual past trace under the SAME current Journal lock. No repair,
// imported KV, append capability or financial state is constructed.
impl Journal {
    pub(crate) fn read_verified_fresh_trace_point(
        &mut self,
        current: &VerifiedJournalPrefixV1,
        past: &VerifiedJournalPrefixV1,
    ) -> Result<JournalCommitTracePinV1, StorageError> {
        self.check_health()?;
        if self.verified_prefix_pin()? != *current
            || past.profile != current.profile
            || past.namespace != current.namespace
            || past.canonical_file != current.canonical_file
            || past.physical_sequence == 0
            || past.physical_sequence > current.physical_sequence
        {
            return Err(StorageError::Corrupt(
                "fresh source past trace point binding",
            ));
        }
        if self.end > 256 * 1024 * 1024 {
            return Err(StorageError::LimitExceeded);
        }
        let cursor = self.file.stream_position()?;
        let mut trace = JournalCommitTracePinV1 {
            profile: VERIFIED_COMMIT_TRACE_PROFILE.into(),
            bytes: self.namespace.header().len() as u64,
            commits: 0,
            chain_sha256d: crate::hash256::double_sha256_parts(&[
                VERIFIED_COMMIT_TRACE_PROFILE.as_bytes(),
                &self.namespace.header(),
            ]),
        };
        let mut entries = std::collections::BTreeMap::new();
        let scanned = self.read_history_frames_with_visit(current, |sequence, raw, mutations| {
            if sequence > past.physical_sequence {
                return;
            }
            trace.chain_sha256d = crate::hash256::double_sha256_parts(&[
                &trace.chain_sha256d,
                &sequence.to_be_bytes(),
                &(raw.len() as u64).to_be_bytes(),
                raw,
            ]);
            trace.bytes += raw.len() as u64;
            trace.commits = sequence;
            for mutation in mutations {
                match mutation {
                    Mutation::Put { key, value } => {
                        entries.insert(key.clone(), value.clone());
                    }
                    Mutation::Delete { key } => {
                        entries.remove(key);
                    }
                }
            }
        });
        if let Err(error) = self.file.seek(SeekFrom::Start(cursor)) {
            self.poisoned = true;
            return Err(error.into());
        }
        let snapshot = match scanned {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.poisoned = true;
                return Err(error);
            }
        };
        if snapshot.complete_kv_receipt()? != current.content {
            self.poisoned = true;
            return Err(StorageError::Corrupt("fresh source current full KV"));
        }
        // Derive the receipt from real past entries only; expected contents are
        // compared afterwards and never restored or supplied to a writer.
        let receipt = complete_kv_receipt(
            entries
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_slice())),
        )?;
        if trace.commits != past.physical_sequence || receipt != past.content {
            return Err(StorageError::Corrupt("fresh source actual past full KV"));
        }
        Ok(trace)
    }
}

#[cfg(test)]
#[path = "storage_durable_fresh_trace_point_tests.rs"]
mod durable_fresh_trace_point_tests;
