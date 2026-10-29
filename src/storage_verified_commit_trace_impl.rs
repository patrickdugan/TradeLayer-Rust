// Actual immutable commit observations, read under the same live Journal lock.
// No history values, repairs or writer capabilities are imported or returned.
pub const VERIFIED_COMMIT_TRACE_PROFILE: &str = "TL-JOURNAL-VERIFIED-COMMIT-TRACE-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalCommitTracePinV1 {
    pub profile: String,
    pub bytes: u64,
    pub commits: u64,
    pub chain_sha256d: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalObservedCommitV1 {
    pub sequence: u64,
    pub mutations: Vec<Mutation>,
}
pub struct VerifiedJournalCommitHistoryV1 {
    prefix: VerifiedJournalPrefixV1,
    trace: JournalCommitTracePinV1,
    commits: Vec<JournalObservedCommitV1>,
}
impl VerifiedJournalCommitHistoryV1 {
    pub fn prefix(&self) -> &VerifiedJournalPrefixV1 {
        &self.prefix
    }
    pub fn trace(&self) -> &JournalCommitTracePinV1 {
        &self.trace
    }
    pub fn commits(&self) -> &[JournalObservedCommitV1] {
        &self.commits
    }
}
impl Journal {
    pub fn read_verified_current_commits(
        &mut self,
        expected: &VerifiedJournalPrefixV1,
        max_commits: usize,
        max_mutation_bytes: usize,
    ) -> Result<VerifiedJournalCommitHistoryV1, StorageError> {
        if max_commits == 0
            || max_commits > 65536
            || max_mutation_bytes == 0
            || max_mutation_bytes > 256 * 1024 * 1024
        {
            return Err(StorageError::LimitExceeded);
        }
        if self.verified_prefix_pin()? != *expected {
            return Err(StorageError::Corrupt(
                "commit trace expected current prefix",
            ));
        }
        let cursor = self.file.stream_position()?;
        let mut trace = JournalCommitTracePinV1 {
            profile: VERIFIED_COMMIT_TRACE_PROFILE.into(),
            bytes: self.end,
            commits: self.sequence,
            chain_sha256d: crate::hash256::double_sha256_parts(&[
                VERIFIED_COMMIT_TRACE_PROFILE.as_bytes(),
                &self.namespace.header(),
            ]),
        };
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
            if exceeded {
                return;
            }
            for mutation in mutations {
                let bytes = match mutation {
                    Mutation::Put { key, value } => key.len().checked_add(value.len()),
                    Mutation::Delete { key } => Some(key.len()),
                };
                let next = bytes.and_then(|n| charged.checked_add(n));
                if next.is_none_or(|n| n > max_mutation_bytes) {
                    exceeded = true;
                    return;
                }
                charged = next.expect("checked bound");
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
            return Err(StorageError::Corrupt(
                "commit trace actual complete current KV",
            ));
        }
        if exceeded {
            return Err(StorageError::LimitExceeded);
        }
        if commits.len() as u64 != self.sequence {
            return Err(StorageError::Corrupt("commit trace count"));
        }
        Ok(VerifiedJournalCommitHistoryV1 {
            prefix: expected.clone(),
            trace,
            commits,
        })
    }
}
