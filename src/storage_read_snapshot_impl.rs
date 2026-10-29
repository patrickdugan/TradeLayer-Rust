// Included inside storage.rs. No mutable owner or disk authority is duplicated.
#[derive(Clone, Debug)]
pub struct JournalReadSnapshot {
    namespace: Namespace,
    sequence: u64,
    values: BTreeMap<String, Arc<[u8]>>,
}
impl JournalReadSnapshot {
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.values.get(key).map(AsRef::as_ref)
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_ref()))
    }
}
impl Journal {
    pub fn open_existing_at_sequence(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        expected_sequence: u64,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode_checked(root, namespace, recovery, false, Some(expected_sequence))
    }
    /// The same existing mutation rules, checked before optional maintenance.
    pub fn validate_commit(&self, mutations: &[Mutation]) -> Result<(), StorageError> {
        self.check_health()?;
        if mutations.is_empty() {
            return Err(StorageError::EmptyBatch);
        }
        if mutations.len() > MAX_BATCH_OPERATIONS {
            return Err(StorageError::LimitExceeded);
        }
        let mut keys = std::collections::BTreeSet::new();
        for m in mutations {
            validate(
                m.key(),
                match m {
                    Mutation::Put { value, .. } => value.len(),
                    Mutation::Delete { .. } => 0,
                },
            )?;
            if !keys.insert(m.key()) {
                return Err(StorageError::DuplicateKey);
            }
        }
        apply(&self.values, mutations)?;
        encode(mutations)?;
        Ok(())
    }
    /// Immutable, cheap page-byte snapshot captured only from a healthy owner.
    /// Future writes replace Arcs; old read generations retain the original
    /// values even after source validity updates or file compaction.
    pub fn read_snapshot(&self) -> Result<JournalReadSnapshot, StorageError> {
        self.check_health()?;
        Ok(JournalReadSnapshot {
            namespace: self.namespace.clone(),
            sequence: self.sequence,
            values: self.values.clone(),
        })
    }
}
