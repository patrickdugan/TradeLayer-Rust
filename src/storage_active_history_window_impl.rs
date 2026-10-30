// Actual-file bounded suffix under the SAME live Journal lock. Full old bytes
// are scanned and hashed; only the selected commit suffix is retained in RAM.
pub struct VerifiedJournalCommitWindowV1 {
    prefix: VerifiedJournalPrefixV1,
    before: JournalCommitTracePinV1,
    after: JournalCommitTracePinV1,
    commits: Vec<JournalObservedCommitV1>,
}
impl VerifiedJournalCommitWindowV1 {
    pub fn prefix(&self) -> &VerifiedJournalPrefixV1 {
        &self.prefix
    }
    pub fn before(&self) -> &JournalCommitTracePinV1 {
        &self.before
    }
    pub fn after(&self) -> &JournalCommitTracePinV1 {
        &self.after
    }
    pub fn commits(&self) -> &[JournalObservedCommitV1] {
        &self.commits
    }
}
impl Journal {
    pub fn read_verified_current_commit_window(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
        from_exclusive: u64,
        retained_before: Option<&JournalCommitTracePinV1>,
        max_commits: usize,
        max_mutation_bytes: usize,
    ) -> Result<VerifiedJournalCommitWindowV1, StorageError> {
        self.check_health()?;
        if max_commits == 0
            || max_commits > 8192
            || max_mutation_bytes == 0
            || max_mutation_bytes > 64 * 1024 * 1024
            || from_exclusive > self.sequence
            || self.sequence - from_exclusive > max_commits as u64
            || (from_exclusive > 0 && from_exclusive != self.sequence && retained_before.is_none())
        {
            return Err(StorageError::LimitExceeded);
        }
        if self.verified_prefix_pin()? != *expected {
            return Err(StorageError::Corrupt(
                "commit window expected current prefix",
            ));
        }
        if retained_before.is_some_and(|pin| {
            pin.profile != VERIFIED_COMMIT_TRACE_PROFILE || pin.commits != from_exclusive
        }) {
            return Err(StorageError::Corrupt("commit window earlier trace shape"));
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
        let mut before = (from_exclusive == 0).then(|| trace.clone());
        let mut commits = vec![];
        let mut charged = 0usize;
        let mut exceeded = false;
        let scanned = self.read_history_frames_with_visit(expected, |sequence, raw, mutations| {
            trace.chain_sha256d = crate::hash256::double_sha256_parts(&[
                &trace.chain_sha256d,
                &sequence.to_be_bytes(),
                &(raw.len() as u64).to_be_bytes(),
                raw,
            ]);
            trace.bytes += raw.len() as u64;
            trace.commits = sequence;
            if sequence == from_exclusive {
                before = Some(trace.clone());
            }
            if sequence <= from_exclusive || exceeded {
                return;
            }
            for mutation in mutations {
                let amount = match mutation {
                    Mutation::Put { key, value } => key.len().checked_add(value.len()),
                    Mutation::Delete { key } => Some(key.len()),
                };
                let next = amount.and_then(|amount| charged.checked_add(amount));
                if next.is_none_or(|next| next > max_mutation_bytes) {
                    exceeded = true;
                    return;
                }
                charged = next.expect("bounded actual bytes");
            }
            if commits.len() >= max_commits {
                exceeded = true;
                return;
            }
            commits.push(JournalObservedCommitV1 {
                sequence,
                mutations: mutations.to_vec(),
            });
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
        if snapshot.complete_kv_receipt()? != expected.content {
            self.poisoned = true;
            return Err(StorageError::Corrupt("commit window actual complete KV"));
        }
        if exceeded {
            return Err(StorageError::LimitExceeded);
        }
        let before = before.ok_or(StorageError::Corrupt("commit window missing earlier trace"))?;
        if retained_before.is_some_and(|expected| expected != &before) {
            return Err(StorageError::Corrupt(
                "commit window earlier trace mismatch",
            ));
        }
        if trace.commits != self.sequence
            || trace.bytes != self.end
            || commits.len() as u64 != self.sequence - from_exclusive
        {
            return Err(StorageError::Corrupt("commit window actual range"));
        }
        Ok(VerifiedJournalCommitWindowV1 {
            prefix: expected.clone(),
            before,
            after: trace,
            commits,
        })
    }
}
