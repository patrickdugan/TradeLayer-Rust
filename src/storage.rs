//! Bounded local disk journal. CRCs detect damage, not malicious substitution.
//! Each batch is synced before it becomes visible. Cross-file transactions,
//! legacy NeDB migration, compaction and authenticated recovery are external.

use crate::codec::crc32;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MAX_KEY_BYTES: usize = 256;
pub const MAX_VALUE_BYTES: usize = crate::codec::MAX_CHECKPOINT_BYTES;
pub const MAX_BATCH_BYTES: usize = MAX_VALUE_BYTES + 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_BATCH_OPERATIONS: usize = 1024;
pub const MAX_STORE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_LOG_BYTES: u64 = 512 * 1024 * 1024;

/// The 32 names in the pinned db.js. These identify files, not query APIs.
pub const CATEGORIES: &[&str] = &[
    "ammRegistry",
    "ammState",
    "txIndex",
    "propertyList",
    "oracleList",
    "oracleData",
    "contractList",
    "tallyMap",
    "tallyMapDelta",
    "marginMapDelta",
    "marginMaps",
    "clearlists",
    "attestations",
    "clearing",
    "consensus",
    "persistence",
    "volumeIndex",
    "channels",
    "withdrawQueue",
    "activations",
    "insurance",
    "orderBooks",
    "feeCache",
    "tradeHistory",
    "fundingEvents",
    "vaults",
    "syntheticTokens",
    "liquidations",
    "scaling",
    "channelDelta",
    "iou",
    "procedural",
];

#[derive(Debug)]
pub enum StorageError {
    Io(std::io::Error),
    Locked,
    InvalidNamespace,
    WrongNamespace,
    InvalidKey,
    LimitExceeded,
    DuplicateKey,
    EmptyBatch,
    Corrupt(&'static str),
    IncompleteTail {
        offset: u64,
    },
    /// A write, truncate or sync failed. Reopen and inspect recovery before use.
    Poisoned,
    Snapshot(crate::iou_ledger::FundedIouSnapshotError),
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for StorageError {}
impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    chain: String,
    test: bool,
    category: &'static str,
}
impl Namespace {
    pub fn for_mainnet(
        network: crate::network_profile::TradeLayerMainnet,
        category: &str,
    ) -> Result<Self, StorageError> {
        use crate::network_profile::TradeLayerMainnet;
        Self::new(
            match network {
                TradeLayerMainnet::Bitcoin => "BTC",
                TradeLayerMainnet::Litecoin => "LTC",
                TradeLayerMainnet::Dogecoin => "DOGE",
            },
            false,
            category,
        )
    }

    /// Bounded ASCII chain identifier; case folding matches db.js filenames.
    pub fn new(chain: &str, test: bool, category: &str) -> Result<Self, StorageError> {
        if chain.is_empty()
            || chain.len() > 32
            || !chain
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(StorageError::InvalidNamespace);
        }
        let category = CATEGORIES
            .iter()
            .copied()
            .find(|name| *name == category)
            .ok_or(StorageError::InvalidNamespace)?;
        Ok(Self {
            chain: chain.to_ascii_lowercase(),
            test,
            category,
        })
    }

    pub fn path(&self, root: &Path) -> PathBuf {
        root.join(format!(
            "{}-{}",
            self.chain,
            if self.test { "test" } else { "main" }
        ))
        .join(format!("{}.tldb", self.category))
    }

    fn header(&self) -> Vec<u8> {
        let mut bytes = b"TLDB\x01".to_vec();
        bytes.push(self.chain.len() as u8);
        bytes.extend_from_slice(self.chain.as_bytes());
        bytes.push(u8::from(self.test));
        bytes.push(self.category.len() as u8);
        bytes.extend_from_slice(self.category.as_bytes());
        bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    RejectIncompleteTail,
    /// Explicitly discard a final short header or short payload/footer only.
    DiscardIncompleteTail,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub discarded_bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}
impl Mutation {
    fn key(&self) -> &str {
        match self {
            Self::Put { key, .. } | Self::Delete { key } => key,
        }
    }
}

/// Exclusive OS lock is held by `file` until this value is dropped.
#[derive(Debug)]
pub struct Journal {
    file: File,
    namespace: Namespace,
    path: PathBuf,
    values: BTreeMap<String, Arc<[u8]>>,
    sequence: u64,
    previous_crc: u32,
    end: u64,
    poisoned: bool,
    #[cfg(test)]
    test_fault: Option<TestCommitFault>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
enum TestCommitFault {
    PartialWrite,
    AfterSync,
}

impl Journal {
    pub fn open(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode(root, namespace, recovery, true)
    }

    /// Restore only. Missing roots, namespaces and files are never created.
    pub fn open_existing(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode(root, namespace, recovery, false)
    }

    fn open_mode(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        create: bool,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode_checked(root, namespace, recovery, create, None)
    }
    fn open_mode_verified(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        create: bool,
        expected_sequence: Option<u64>,
        expected_prefix: Option<&VerifiedJournalPrefixV1>,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        let path = namespace.path(root);
        // Caller owns creation/durability of the root and its ancestors.
        if !root.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "storage root must exist",
            )
            .into());
        }
        let directory = path.parent().ok_or(StorageError::InvalidNamespace)?;
        if create {
            match std::fs::create_dir(directory) {
                Ok(()) => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::AlreadyExists && directory.is_dir() => {}
                Err(error) => return Err(error.into()),
            }
            sync_directory(root)?;
        }
        let (mut file, created) = if !create {
            (
                OpenOptions::new().read(true).write(true).open(&path)?,
                false,
            )
        } else {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => (file, true),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
                    OpenOptions::new().read(true).write(true).open(&path)?,
                    false,
                ),
                Err(error) => return Err(error.into()),
            }
        };
        if created {
            // A contender can briefly lock the new, still-empty file before
            // its creator. It will reject its short header and release it.
            // Waiting here keeps the creator from abandoning an empty store.
            file.lock()?;
        } else {
            match file.try_lock() {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => return Err(StorageError::Locked),
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        let expected = namespace.header();
        if created {
            file.write_all(&expected)?;
            file.sync_all()?;
        }
        sync_directory(directory)?;
        let length = file.metadata()?.len();
        if length > MAX_LOG_BYTES {
            return Err(StorageError::LimitExceeded);
        }
        if length < expected.len() as u64 {
            return Err(StorageError::Corrupt("short store header"));
        }
        file.seek(SeekFrom::Start(0))?;
        let mut header = vec![0; expected.len()];
        file.read_exact(&mut header)?;
        if header != expected {
            return Err(StorageError::WrongNamespace);
        }
        let mut journal = Self {
            file,
            namespace,
            path,
            values: BTreeMap::new(),
            sequence: 0,
            previous_crc: crc32(&header),
            end: header.len() as u64,
            poisoned: false,
            #[cfg(test)]
            test_fault: None,
        };
        // Inspect the entire committed prefix without truncation. A rejected
        // caller pin must never authorize recover_tail's filesystem mutation.
        let replay = journal.replay(length, Recovery::RejectIncompleteTail);
        let report = match replay {
            Ok(report) => {
                if expected_sequence.is_some_and(|n| n != journal.sequence) {
                    return Err(StorageError::Corrupt(
                        "unexpected independently pinned sequence",
                    ));
                }
                journal.require_verified_prefix(expected_prefix)?;
                report
            }
            Err(StorageError::IncompleteTail { .. }) => {
                if expected_sequence.is_some_and(|n| n != journal.sequence) {
                    return Err(StorageError::Corrupt(
                        "unexpected independently pinned sequence",
                    ));
                }
                journal.require_verified_prefix(expected_prefix)?;
                journal.recover_tail(length, recovery)?
            }
            Err(error) => return Err(error),
        };
        // A prior writer may have returned an uncertain sync error after
        // writing a complete frame. Reconcile and sync before serving it.
        journal.file.sync_all()?;
        Ok((journal, report))
    }

    fn replay(&mut self, length: u64, recovery: Recovery) -> Result<RecoveryReport, StorageError> {
        while self.end < length {
            let remaining = length - self.end;
            if remaining < 24 {
                return self.recover_tail(length, recovery);
            }
            let mut header = [0_u8; 24];
            self.file.read_exact(&mut header)?;
            if &header[..4] != b"TXN1" || crc32(&header[..20]) != u32_at(&header, 20) {
                return Err(StorageError::Corrupt("batch header"));
            }
            let payload_len = u32_at(&header, 4) as usize;
            if !(4..=MAX_BATCH_BYTES).contains(&payload_len) {
                return Err(StorageError::LimitExceeded);
            }
            let sequence = u64::from_be_bytes(header[8..16].try_into().unwrap());
            if self.sequence.checked_add(1) != Some(sequence)
                || u32_at(&header, 16) != self.previous_crc
            {
                return Err(StorageError::Corrupt("batch sequence or CRC chain"));
            }
            let frame_len = payload_len as u64 + 32;
            if remaining < frame_len {
                return self.recover_tail(length, recovery);
            }
            let mut bytes = header.to_vec();
            bytes.resize(24 + payload_len + 8, 0);
            self.file.read_exact(&mut bytes[24..])?;
            let checksum = u32_at(&bytes, 24 + payload_len);
            if &bytes[28 + payload_len..] != b"DONE"
                || crc32(&bytes[..24 + payload_len]) != checksum
            {
                return Err(StorageError::Corrupt("batch checksum or commit footer"));
            }
            let mutations = decode(&bytes[24..24 + payload_len])?;
            let next = apply(&self.values, &mutations)?;
            self.values = next;
            self.sequence = sequence;
            self.previous_crc = checksum;
            self.end += frame_len;
        }
        Ok(RecoveryReport::default())
    }

    fn recover_tail(
        &mut self,
        length: u64,
        recovery: Recovery,
    ) -> Result<RecoveryReport, StorageError> {
        if recovery == Recovery::RejectIncompleteTail {
            return Err(StorageError::IncompleteTail { offset: self.end });
        }
        self.file.set_len(self.end)?;
        self.file.sync_all()?;
        self.file.seek(SeekFrom::Start(self.end))?;
        Ok(RecoveryReport {
            discarded_bytes: length - self.end,
        })
    }

    /// Returns no data after an uncertain write. Reopen to learn committed state.
    pub fn get(&self, key: &str) -> Result<Option<&[u8]>, StorageError> {
        self.check_health()?;
        Ok(self.values.get(key).map(AsRef::as_ref))
    }
    pub fn sequence(&self) -> Result<u64, StorageError> {
        self.check_health()?;
        Ok(self.sequence)
    }
    pub fn entries(&self) -> Result<impl Iterator<Item = (&str, &[u8])>, StorageError> {
        self.check_health()?;
        Ok(self
            .values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_ref())))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    fn check_health(&self) -> Result<(), StorageError> {
        if self.poisoned {
            Err(StorageError::Poisoned)
        } else {
            Ok(())
        }
    }

    /// One file, one all-or-nothing batch; duplicate keys are rejected.
    pub fn commit(&mut self, mutations: Vec<Mutation>) -> Result<u64, StorageError> {
        self.check_health()?;
        let mut ordered = BTreeMap::new();
        if mutations.is_empty() {
            return Err(StorageError::EmptyBatch);
        }
        if mutations.len() > MAX_BATCH_OPERATIONS {
            return Err(StorageError::LimitExceeded);
        }
        for mutation in mutations {
            validate(
                mutation.key(),
                match &mutation {
                    Mutation::Put { value, .. } => value.len(),
                    Mutation::Delete { .. } => 0,
                },
            )?;
            if ordered
                .insert(mutation.key().to_owned(), mutation)
                .is_some()
            {
                return Err(StorageError::DuplicateKey);
            }
        }
        let mutations: Vec<_> = ordered.into_values().collect();
        let next = apply(&self.values, &mutations)?;
        let payload = encode(&mutations)?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(StorageError::LimitExceeded)?;
        let mut frame = b"TXN1".to_vec();
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&sequence.to_be_bytes());
        frame.extend_from_slice(&self.previous_crc.to_be_bytes());
        frame.extend_from_slice(&crc32(&frame).to_be_bytes());
        frame.extend_from_slice(&payload);
        let checksum = crc32(&frame);
        frame.extend_from_slice(&checksum.to_be_bytes());
        frame.extend_from_slice(b"DONE");
        let end = self
            .end
            .checked_add(frame.len() as u64)
            .ok_or(StorageError::LimitExceeded)?;
        if end > MAX_LOG_BYTES {
            return Err(StorageError::LimitExceeded);
        }
        // Set before any fallible I/O: an error may follow a successful write.
        self.poisoned = true;
        self.file.seek(SeekFrom::Start(self.end))?;
        #[cfg(test)]
        if matches!(self.test_fault, Some(TestCommitFault::PartialWrite)) {
            self.test_fault = None;
            self.file.write_all(&frame[..frame.len() / 2])?;
            self.file.sync_all()?;
            return Err(std::io::Error::other("injected error after real partial write").into());
        }
        self.file.write_all(&frame)?;
        self.file.sync_all()?;
        #[cfg(test)]
        if matches!(self.test_fault, Some(TestCommitFault::AfterSync)) {
            self.test_fault = None;
            return Err(std::io::Error::other("injected lost acknowledgement after sync").into());
        }
        self.values = next;
        self.sequence = sequence;
        self.previous_crc = checksum;
        self.end = end;
        self.poisoned = false;
        Ok(sequence)
    }

    #[cfg(test)]
    pub(crate) fn test_fail_after_sync(&mut self) {
        self.test_fault = Some(TestCommitFault::AfterSync);
    }
    #[cfg(test)]
    pub(crate) fn test_fail_after_partial_write(&mut self) {
        self.test_fault = Some(TestCommitFault::PartialWrite);
    }

    /// Stores a complete ledger+IOU+replay checkpoint as one journal value.
    pub fn save_funded_iou(
        &mut self,
        key: &str,
        state: &crate::iou_ledger::FundedIouLedger,
    ) -> Result<u64, StorageError> {
        self.check_funded_namespace(state.network())?;
        let bytes = state.snapshot_bytes().map_err(StorageError::Snapshot)?;
        self.commit(vec![Mutation::Put {
            key: key.to_owned(),
            value: bytes,
        }])
    }
    pub fn load_funded_iou(
        &self,
        key: &str,
        network: crate::network_profile::TradeLayerMainnet,
        policy: crate::iou_ledger::PayoutAtomPolicy,
    ) -> Result<Option<crate::iou_ledger::FundedIouLedger>, StorageError> {
        self.check_funded_namespace(network)?;
        self.get(key)?
            .map(|bytes| {
                crate::iou_ledger::FundedIouLedger::from_snapshot_bytes(bytes, network, policy)
                    .map_err(StorageError::Snapshot)
            })
            .transpose()
    }

    fn check_funded_namespace(
        &self,
        network: crate::network_profile::TradeLayerMainnet,
    ) -> Result<(), StorageError> {
        if self.namespace != Namespace::for_mainnet(network, "iou")? {
            return Err(StorageError::WrongNamespace);
        }
        Ok(())
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn validate(key: &str, value_len: usize) -> Result<(), StorageError> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(StorageError::InvalidKey);
    }
    if value_len > MAX_VALUE_BYTES {
        return Err(StorageError::LimitExceeded);
    }
    Ok(())
}
type Values = BTreeMap<String, Arc<[u8]>>;
fn apply(current: &Values, mutations: &[Mutation]) -> Result<Values, StorageError> {
    encoded_length(mutations)?;
    let mut count = current.len();
    let mut total: usize = current
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum();
    // Callers guarantee unique keys. Validate final bounds before cloning values.
    for mutation in mutations {
        let key = mutation.key();
        if let Some(old) = current.get(key) {
            count -= 1;
            total -= key.len() + old.len();
        }
        if let Mutation::Put { value, .. } = mutation {
            count += 1;
            total = total
                .checked_add(key.len())
                .and_then(|n| n.checked_add(value.len()))
                .ok_or(StorageError::LimitExceeded)?;
        }
    }
    if count > MAX_ENTRIES || total > MAX_STORE_BYTES {
        return Err(StorageError::LimitExceeded);
    }
    let mut values = current.clone();
    for mutation in mutations {
        match mutation {
            Mutation::Put { key, value } => {
                validate(key, value.len())?;
                values.insert(key.clone(), Arc::from(value.as_slice()));
            }
            Mutation::Delete { key } => {
                validate(key, 0)?;
                values.remove(key);
            }
        }
    }
    Ok(values)
}
fn encoded_length(mutations: &[Mutation]) -> Result<usize, StorageError> {
    let mut length = 4_usize;
    for mutation in mutations {
        let value_len = match mutation {
            Mutation::Put { value, .. } => value.len(),
            _ => 0,
        };
        validate(mutation.key(), value_len)?;
        let extra = if matches!(mutation, Mutation::Put { .. }) {
            4
        } else {
            0
        };
        length = length
            .checked_add(3 + mutation.key().len() + extra)
            .and_then(|n| n.checked_add(value_len))
            .ok_or(StorageError::LimitExceeded)?;
        if length > MAX_BATCH_BYTES {
            return Err(StorageError::LimitExceeded);
        }
    }
    Ok(length)
}
fn encode(mutations: &[Mutation]) -> Result<Vec<u8>, StorageError> {
    let mut bytes = Vec::with_capacity(encoded_length(mutations)?);
    bytes.extend_from_slice(&(mutations.len() as u32).to_be_bytes());
    for mutation in mutations {
        let key = mutation.key();
        bytes.push(u8::from(matches!(mutation, Mutation::Put { .. })));
        bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
        bytes.extend_from_slice(key.as_bytes());
        if let Mutation::Put { value, .. } = mutation {
            bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
            bytes.extend_from_slice(value);
        }
        if bytes.len() > MAX_BATCH_BYTES {
            return Err(StorageError::LimitExceeded);
        }
    }
    Ok(bytes)
}

// Directory entry durability is available through std on Unix. Windows uses
// file.sync_all; explicit directory flush/power-loss certification is external.
#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), StorageError> {
    File::open(path)?.sync_all()?;
    Ok(())
}
#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}
fn decode(bytes: &[u8]) -> Result<Vec<Mutation>, StorageError> {
    use crate::codec::Cursor;
    let bad = |_| StorageError::Corrupt("batch payload encoding");
    let mut reader = Cursor::new(bytes);
    let count = reader.u32().map_err(bad)? as usize;
    if count == 0 || count > MAX_BATCH_OPERATIONS {
        return Err(StorageError::Corrupt("batch count"));
    }
    let mut mutations = Vec::with_capacity(count);
    let mut previous = String::new();
    for _ in 0..count {
        let kind = reader.u8().map_err(bad)?;
        let key_len = u16::from_be_bytes(reader.fixed().map_err(bad)?) as usize;
        if key_len == 0 || key_len > MAX_KEY_BYTES {
            return Err(StorageError::InvalidKey);
        }
        let key = std::str::from_utf8(reader.take(key_len).map_err(bad)?)
            .map_err(|_| StorageError::Corrupt("key UTF-8"))?
            .to_owned();
        if key <= previous {
            return Err(StorageError::Corrupt("keys not strictly ordered"));
        }
        previous = key.clone();
        mutations.push(match kind {
            0 => Mutation::Delete { key },
            1 => {
                let len = reader.u32().map_err(bad)? as usize;
                if len > MAX_VALUE_BYTES {
                    return Err(StorageError::LimitExceeded);
                }
                Mutation::Put {
                    key,
                    value: reader.take(len).map_err(bad)?.to_vec(),
                }
            }
            _ => return Err(StorageError::Corrupt("mutation tag")),
        });
    }
    reader.end().map_err(bad)?;
    Ok(mutations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_write_error_poison_blocks_read_and_write_until_reopen() {
        let root = std::env::temp_dir().join(format!(
            "tl-storage-fault-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let namespace = Namespace::new("BTC", false, "persistence").unwrap();
        let (mut journal, _) =
            Journal::open(&root, namespace.clone(), Recovery::RejectIncompleteTail).unwrap();
        journal
            .commit(vec![Mutation::Put {
                key: "old".into(),
                value: b"kept".to_vec(),
            }])
            .unwrap();
        // Substitute a real read-only handle to exercise a real failed write,
        // rather than merely returning a synthetic error before the I/O path.
        journal.file = File::open(journal.path()).unwrap();
        assert!(matches!(
            journal.commit(vec![Mutation::Put {
                key: "new".into(),
                value: b"lost".to_vec()
            }]),
            Err(StorageError::Io(_))
        ));
        assert!(matches!(journal.get("old"), Err(StorageError::Poisoned)));
        assert!(matches!(journal.sequence(), Err(StorageError::Poisoned)));
        assert!(matches!(journal.entries(), Err(StorageError::Poisoned)));
        assert!(matches!(
            journal.commit(vec![]),
            Err(StorageError::Poisoned)
        ));
        drop(journal);
        let (journal, _) = Journal::open(&root, namespace, Recovery::RejectIncompleteTail).unwrap();
        assert_eq!(journal.get("old").unwrap(), Some(b"kept".as_slice()));
        assert_eq!(journal.get("new").unwrap(), None);
        assert_eq!(journal.sequence().unwrap(), 1);
        drop(journal);
        assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()));
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-storage-fault-"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

include!("storage_read_snapshot_impl.rs");
include!("storage_verified_prefix_impl.rs");
#[cfg(test)]
#[path = "storage_verified_prefix_tests.rs"]
mod verified_prefix_tests;
