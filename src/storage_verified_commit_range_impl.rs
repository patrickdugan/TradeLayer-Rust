// Actual bounded historical WAL range. Every current byte/KV is checked under
// the retained file lock; only [before,after] commits are retained.
pub struct VerifiedJournalCommitRangeV1 {
    current: VerifiedJournalPrefixV1,
    before: JournalCommitTracePinV1,
    after: JournalCommitTracePinV1,
    commits: Vec<JournalObservedCommitV1>,
}
impl VerifiedJournalCommitRangeV1 {
    pub fn current(&self) -> &VerifiedJournalPrefixV1 {
        &self.current
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
    pub fn read_verified_commit_range(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
        current_trace: &JournalCommitTracePinV1,
        before: &JournalCommitTracePinV1,
        after: &JournalCommitTracePinV1,
        budget: (usize, usize),
    ) -> Result<VerifiedJournalCommitRangeV1, StorageError> {
        self.check_health()?;
        let (max_commits, max_bytes) = budget;
        if max_commits == 0
            || max_commits > 8192
            || max_bytes == 0
            || max_bytes > 64 * 1024 * 1024
            || before.commits > after.commits
            || after.commits > self.sequence
            || after.commits - before.commits > max_commits as u64
        {
            return Err(StorageError::LimitExceeded);
        }
        if self.verified_prefix_pin()? != *expected {
            return Err(StorageError::Corrupt(
                "historical range expected current prefix",
            ));
        }
        if [current_trace, before, after]
            .iter()
            .any(|p| p.profile != VERIFIED_COMMIT_TRACE_PROFILE)
            || current_trace.commits != self.sequence
            || current_trace.bytes != self.end
        {
            return Err(StorageError::Corrupt("historical range trace shape"));
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
        let mut actual_before = (before.commits == 0).then(|| trace.clone());
        let mut actual_after = (after.commits == 0).then(|| trace.clone());
        let mut commits = Vec::new();
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
            if sequence == before.commits {
                actual_before = Some(trace.clone())
            }
            if sequence == after.commits {
                actual_after = Some(trace.clone())
            }
            if sequence <= before.commits || sequence > after.commits || exceeded {
                return;
            }
            for mutation in mutations {
                let bytes = match mutation {
                    Mutation::Put { key, value } => key.len().checked_add(value.len()),
                    Mutation::Delete { key } => Some(key.len()),
                };
                let next = bytes.and_then(|n| charged.checked_add(n));
                if next.is_none_or(|n| n > max_bytes) {
                    exceeded = true;
                    return;
                }
                charged = next.expect("bounded actual mutation bytes");
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
            Ok(s) => s,
            Err(e) => {
                self.poisoned = true;
                return Err(e);
            }
        };
        if snapshot.complete_kv_receipt()? != expected.content {
            self.poisoned = true;
            return Err(StorageError::Corrupt("historical range actual complete KV"));
        }
        if exceeded {
            return Err(StorageError::LimitExceeded);
        }
        if &trace != current_trace
            || actual_before.as_ref() != Some(before)
            || actual_after.as_ref() != Some(after)
            || commits.len() as u64 != after.commits - before.commits
        {
            return Err(StorageError::Corrupt(
                "historical range independent trace mismatch",
            ));
        }
        Ok(VerifiedJournalCommitRangeV1 {
            current: expected.clone(),
            before: before.clone(),
            after: after.clone(),
            commits,
        })
    }
}
impl Journal {
    pub fn verified_commit_trace_origin(&self) -> Result<JournalCommitTracePinV1, StorageError> {
        self.check_health()?;
        Ok(JournalCommitTracePinV1 {
            profile: VERIFIED_COMMIT_TRACE_PROFILE.into(),
            bytes: self.namespace.header().len() as u64,
            commits: 0,
            chain_sha256d: crate::hash256::double_sha256_parts(&[
                VERIFIED_COMMIT_TRACE_PROFILE.as_bytes(),
                &self.namespace.header(),
            ]),
        })
    }
}
