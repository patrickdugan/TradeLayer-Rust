//! One owned joint protocol state and one atomically synced local journal.
//!
//! Block inputs remain unverified caller assertions. CRCs and checkpoints are
//! not chain authentication. A write error can mean an on-disk commit occurred;
//! the journal then denies cached state until independently checked reopening.

use crate::chain::{BlockHeader, BlockId, Tip, MAX_REORG_HISTORY};
use crate::codec::Cursor;
use crate::joint_chain::{
    JointChainError, JointChainState, JointCheckpointError, UnverifiedJointBlock,
};
use crate::ledger::{AccountId, PropertyId};
use crate::network_profile::TradeLayerMainnet;
use crate::storage::{Journal, Mutation, Namespace, Recovery, RecoveryReport, StorageError};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "628c5b3cfd919fd518aedc2b757cdc256f7ccc36c447090f30aebd8a677ce2e1";
pub const MAX_HEADER_RETENTION: usize = 2048;
const CONFIG_KEY: &str = "joint/config";
const STATE_KEY: &str = "joint/state";
const HEADER_PREFIX: &str = "joint/header/";
const CONFIG_MAGIC: &[u8; 5] = b"TLPM\x01";

/// Source cadence on the typed nonnegative-integer domain. Zero disables it.
/// The durable owner above persists every accepted block regardless of cadence.
pub fn source_checkpoint_due(height: u64, interval: u64) -> bool {
    interval != 0 && height.is_multiple_of(interval)
}

/// The source deletes headers strictly below checkpoint height minus 1,000.
pub fn source_cleanup_before(checkpoint_height: u64) -> i128 {
    i128::from(checkpoint_height) - 1000
}

/// Reproduces the source's UNSAFE snapshot choice for characterization only:
/// it uses local tip, ignores the common ancestor, and can choose a future
/// snapshot when every supplied candidate exceeds the tip. Never restores data.
pub fn characterize_source_snapshot_choice(local_tip: u64, listed_heights: &[u64]) -> Option<u64> {
    listed_heights
        .iter()
        .copied()
        .filter(|height| *height <= local_tip)
        .max()
        .or_else(|| listed_heights.first().copied())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceConfig {
    pub network: TradeLayerMainnet,
    pub anchor: Tip,
    pub admin: AccountId,
    pub permitted_spot_properties: BTreeSet<PropertyId>,
    pub rollback_limit: usize,
    pub header_retention: usize,
}

impl PersistenceConfig {
    fn bytes(&self) -> Result<Vec<u8>, PersistenceError> {
        if !(1..=MAX_REORG_HISTORY).contains(&self.rollback_limit)
            || !(self.rollback_limit..=MAX_HEADER_RETENTION).contains(&self.header_retention)
            || self.permitted_spot_properties.len() > crate::storage::MAX_ENTRIES
        {
            return Err(PersistenceError::InvalidConfig);
        }
        let mut bytes = CONFIG_MAGIC.to_vec();
        bytes.push(self.network.tag());
        bytes.extend_from_slice(&self.network.genesis().0);
        bytes.extend_from_slice(&self.anchor.height.to_be_bytes());
        bytes.extend_from_slice(&self.anchor.id.0);
        bytes.push(self.admin.as_str().len() as u8);
        bytes.extend_from_slice(self.admin.as_str().as_bytes());
        bytes.push(self.rollback_limit as u8);
        bytes.extend_from_slice(&(self.header_retention as u16).to_be_bytes());
        bytes.extend_from_slice(&(self.permitted_spot_properties.len() as u32).to_be_bytes());
        for property in &self.permitted_spot_properties {
            bytes.extend_from_slice(&property.get().to_be_bytes());
        }
        Ok(bytes)
    }

    fn decode_state(
        &self,
        bytes: &[u8],
        expected_tip: Tip,
    ) -> Result<JointChainState, PersistenceError> {
        let state = JointChainState::from_checkpoint_bytes(
            bytes,
            self.network,
            self.anchor,
            &self.admin,
            &self.permitted_spot_properties,
            self.rollback_limit,
        )?;
        if state.tip() != expected_tip {
            return Err(PersistenceError::UnexpectedTip {
                expected: expected_tip,
                stored: state.tip(),
            });
        }
        Ok(state)
    }
}

#[derive(Debug)]
pub enum PersistenceError {
    Storage(StorageError),
    Chain(JointChainError),
    Checkpoint(JointCheckpointError),
    InvalidConfig,
    AlreadyInitialized,
    Uninitialized,
    InvalidStore(&'static str),
    UnexpectedTip { expected: Tip, stored: Tip },
    NoChange,
}
impl core::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PersistenceError {}
impl From<StorageError> for PersistenceError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}
impl From<JointChainError> for PersistenceError {
    fn from(error: JointChainError) -> Self {
        Self::Chain(error)
    }
}
impl From<JointCheckpointError> for PersistenceError {
    fn from(error: JointCheckpointError) -> Self {
        Self::Checkpoint(error)
    }
}

/// Complete current chain, sole spendable ledger and retained replay/rollback
/// history have exactly one owner. No mutable state or journal escape hatch.
#[derive(Debug)]
pub struct PersistentJointChain {
    journal: Journal,
    config: PersistenceConfig,
    state: JointChainState,
    headers: BTreeMap<u64, BlockHeader>,
}

impl PersistentJointChain {
    /// Imports one complete caller-asserted chain into a previously empty store.
    /// The explicit expected tip is checked before any journal batch is written.
    pub fn initialize(
        root: &Path,
        config: PersistenceConfig,
        initial: JointChainState,
        expected_tip: Tip,
    ) -> Result<Self, PersistenceError> {
        let config_bytes = config.bytes()?;
        let bytes = initial.checkpoint_bytes()?;
        let initial = config.decode_state(&bytes, expected_tip)?;
        let headers = retained_headers(&initial)?;
        let namespace = Namespace::for_mainnet(config.network, "persistence")?;
        let (mut journal, _) = Journal::open(root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(PersistenceError::AlreadyInitialized);
        }
        let mut mutations = vec![
            Mutation::Put {
                key: CONFIG_KEY.into(),
                value: config_bytes,
            },
            Mutation::Put {
                key: STATE_KEY.into(),
                value: bytes,
            },
        ];
        for header in headers.values() {
            mutations.push(Mutation::Put {
                key: header_key(header.height),
                value: header_bytes(*header),
            });
        }
        journal.commit(mutations)?;
        Ok(Self {
            journal,
            config,
            state: initial,
            headers,
        })
    }

    /// All expected fields come from the caller, including the exact intended
    /// tip. An older valid journal cannot authenticate its own freshness.
    pub fn load(
        root: &Path,
        config: PersistenceConfig,
        expected_tip: Tip,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), PersistenceError> {
        let config_bytes = config.bytes()?;
        let namespace = Namespace::for_mainnet(config.network, "persistence")?;
        let (journal, report) = Journal::open_existing(root, namespace, recovery)?;
        let stored_config = journal
            .get(CONFIG_KEY)?
            .ok_or(PersistenceError::Uninitialized)?;
        if stored_config != config_bytes {
            return Err(PersistenceError::InvalidStore("configuration mismatch"));
        }
        let bytes = journal
            .get(STATE_KEY)?
            .ok_or(PersistenceError::InvalidStore("missing chain checkpoint"))?;
        let state = config.decode_state(bytes, expected_tip)?;
        let mut headers = BTreeMap::new();
        for (key, value) in journal.entries()? {
            if key == CONFIG_KEY || key == STATE_KEY {
                continue;
            }
            let header = read_header(value)?;
            if key != header_key(header.height) {
                return Err(PersistenceError::InvalidStore(
                    "unknown or noncanonical key",
                ));
            }
            if headers.insert(header.height, header).is_some() {
                return Err(PersistenceError::InvalidStore("duplicate header"));
            }
        }
        validate_headers(&state, &headers, config.header_retention)?;
        Ok((
            Self {
                journal,
                config,
                state,
                headers,
            },
            report,
        ))
    }

    /// Access checks the actual journal health before exposing cached state.
    pub fn state(&self) -> Result<&JointChainState, PersistenceError> {
        self.journal.sequence()?;
        Ok(&self.state)
    }
    pub fn tip(&self) -> Result<Tip, PersistenceError> {
        Ok(self.state()?.tip())
    }
    pub fn sequence(&self) -> Result<u64, PersistenceError> {
        Ok(self.journal.sequence()?)
    }
    pub fn config(&self) -> Result<&PersistenceConfig, PersistenceError> {
        self.journal.sequence()?;
        Ok(&self.config)
    }
    pub fn path(&self) -> PathBuf {
        self.journal.path().to_owned()
    }
    pub fn get_block_header(&self, height: u64) -> Result<Option<BlockHeader>, PersistenceError> {
        self.journal.sequence()?;
        Ok(self.headers.get(&height).copied())
    }
    pub fn headers(&self) -> Result<impl Iterator<Item = BlockHeader> + '_, PersistenceError> {
        self.journal.sequence()?;
        Ok(self.headers.values().copied())
    }

    /// Finalizes the entire mixed block in a clone, persists the full chain and
    /// its header changes in one synced transaction, then publishes the clone.
    pub fn apply_unverified_block(
        &mut self,
        block: &UnverifiedJointBlock,
    ) -> Result<Tip, PersistenceError> {
        self.journal.sequence()?;
        let mut candidate = self.state.clone();
        candidate.apply_unverified_block(block)?;
        let mut headers = self.headers.clone();
        headers.insert(block.header.height, block.header);
        while headers.len() > self.config.header_retention {
            headers.pop_first();
        }
        self.publish(candidate, headers)
    }

    pub fn rollback_tip(&mut self, expected_tip: Tip) -> Result<Tip, PersistenceError> {
        self.journal.sequence()?;
        let mut candidate = self.state.clone();
        candidate.rollback_tip(expected_tip)?;
        let mut headers = self.headers.clone();
        headers.retain(|height, _| *height <= candidate.tip().height);
        self.publish(candidate, headers)
    }

    /// Multiple retained blocks roll back in a single journal transaction.
    pub fn rollback_to(
        &mut self,
        expected_current: Tip,
        target: Tip,
    ) -> Result<Tip, PersistenceError> {
        let current = self.tip()?;
        if current != expected_current {
            return Err(PersistenceError::UnexpectedTip {
                expected: expected_current,
                stored: current,
            });
        }
        if current == target {
            return Err(PersistenceError::NoChange);
        }
        if target.height >= current.height {
            return Err(PersistenceError::UnexpectedTip {
                expected: target,
                stored: current,
            });
        }
        let mut candidate = self.state.clone();
        while candidate.tip().height > target.height {
            candidate.rollback_tip(candidate.tip())?;
        }
        if candidate.tip() != target {
            return Err(PersistenceError::UnexpectedTip {
                expected: target,
                stored: candidate.tip(),
            });
        }
        let mut headers = self.headers.clone();
        headers.retain(|height, _| *height <= target.height);
        self.publish(candidate, headers)
    }

    fn publish(
        &mut self,
        candidate: JointChainState,
        headers: BTreeMap<u64, BlockHeader>,
    ) -> Result<Tip, PersistenceError> {
        let bytes = candidate.checkpoint_bytes()?;
        // Every candidate must be reloadable under the independently configured
        // policy, including a valid explicit oldest-history floor checkpoint.
        self.config.decode_state(&bytes, candidate.tip())?;
        validate_headers(&candidate, &headers, self.config.header_retention)?;
        let mut changes = vec![Mutation::Put {
            key: STATE_KEY.into(),
            value: bytes,
        }];
        for (&height, old) in &self.headers {
            if !headers.contains_key(&height) {
                changes.push(Mutation::Delete {
                    key: header_key(height),
                });
            } else if headers.get(&height) != Some(old) {
                return Err(PersistenceError::InvalidStore(
                    "existing header changed without rollback",
                ));
            }
        }
        for (&height, header) in &headers {
            if !self.headers.contains_key(&height) {
                changes.push(Mutation::Put {
                    key: header_key(height),
                    value: header_bytes(*header),
                });
            }
        }
        self.journal.commit(changes)?;
        self.state = candidate;
        self.headers = headers;
        Ok(self.state.tip())
    }
}

fn header_key(height: u64) -> String {
    format!("{HEADER_PREFIX}{height:020}")
}
fn header_bytes(header: BlockHeader) -> Vec<u8> {
    let mut bytes = header.height.to_be_bytes().to_vec();
    bytes.extend_from_slice(&header.id.0);
    bytes.extend_from_slice(&header.parent.0);
    bytes
}
fn read_header(bytes: &[u8]) -> Result<BlockHeader, PersistenceError> {
    let bad = |_| PersistenceError::InvalidStore("header encoding");
    let mut reader = Cursor::new(bytes);
    let header = BlockHeader {
        height: reader.u64().map_err(bad)?,
        id: BlockId(reader.fixed().map_err(bad)?),
        parent: BlockId(reader.fixed().map_err(bad)?),
    };
    reader.end().map_err(bad)?;
    Ok(header)
}

fn retained_headers(
    state: &JointChainState,
) -> Result<BTreeMap<u64, BlockHeader>, PersistenceError> {
    let mut result = BTreeMap::new();
    let mut prior = state.clone();
    while prior.rollback_depth() != 0 {
        let tip = prior.tip();
        prior.rollback_tip(tip)?;
        result.insert(
            tip.height,
            BlockHeader {
                height: tip.height,
                id: tip.id,
                parent: prior.tip().id,
            },
        );
    }
    Ok(result)
}

fn validate_headers(
    state: &JointChainState,
    headers: &BTreeMap<u64, BlockHeader>,
    limit: usize,
) -> Result<(), PersistenceError> {
    if headers.len() > limit {
        return Err(PersistenceError::InvalidStore("header retention exceeded"));
    }
    let mut previous: Option<BlockHeader> = None;
    for (&height, header) in headers {
        if height != header.height || height <= state.anchor().height || height > state.tip().height
        {
            return Err(PersistenceError::InvalidStore(
                "header height outside chain",
            ));
        }
        if previous
            .is_some_and(|old| old.height.checked_add(1) != Some(height) || old.id != header.parent)
            || (height == state.anchor().height.saturating_add(1)
                && header.parent != state.anchor().id)
        {
            return Err(PersistenceError::InvalidStore("noncontiguous header chain"));
        }
        previous = Some(*header);
    }
    if state.tip() != state.anchor()
        && !(headers.is_empty() && state.rollback_depth() == 0)
        && previous.map(|header| Tip {
            height: header.height,
            id: header.id,
        }) != Some(state.tip())
    {
        return Err(PersistenceError::InvalidStore("header tip mismatch"));
    }
    for (height, expected) in retained_headers(state)? {
        if headers.get(&height) != Some(&expected) {
            return Err(PersistenceError::InvalidStore(
                "checkpoint/header disagreement",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod fault_tests {
    use super::*;
    use crate::ledger::Ledger;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "tl-persistence-fault-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let file = Namespace::for_mainnet(TradeLayerMainnet::Bitcoin, "persistence")
                .unwrap()
                .path(&self.0);
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_dir(file.parent().unwrap());
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    fn initial(directory: &Directory) -> (PersistentJointChain, PersistenceConfig) {
        let network = TradeLayerMainnet::Bitcoin;
        let anchor = Tip {
            height: 0,
            id: network.genesis(),
        };
        let config = PersistenceConfig {
            network,
            anchor,
            admin: "admin".try_into().unwrap(),
            permitted_spot_properties: BTreeSet::new(),
            rollback_limit: 4,
            header_retention: 4,
        };
        let state = JointChainState::new_unverified(
            network,
            anchor,
            config.admin.clone(),
            Ledger::new(),
            BTreeSet::new(),
            4,
        )
        .unwrap();
        (
            PersistentJointChain::initialize(&directory.0, config.clone(), state, anchor).unwrap(),
            config,
        )
    }
    fn next(owner: &PersistentJointChain) -> UnverifiedJointBlock {
        let tip = owner.tip().unwrap();
        UnverifiedJointBlock {
            network: owner.config.network,
            header: BlockHeader {
                height: tip.height + 1,
                id: BlockId([0x75; 32]),
                parent: tip.id,
            },
            transactions: vec![],
        }
    }
    fn assert_poisoned(owner: &mut PersistentJointChain, block: &UnverifiedJointBlock) {
        fn poisoned<T>(result: Result<T, PersistenceError>) {
            assert!(matches!(
                result,
                Err(PersistenceError::Storage(StorageError::Poisoned))
            ));
        }
        poisoned(owner.state());
        poisoned(owner.tip());
        poisoned(owner.sequence());
        poisoned(owner.config());
        poisoned(owner.get_block_header(1));
        poisoned(owner.headers());
        poisoned(owner.apply_unverified_block(block));
        poisoned(owner.rollback_tip(Tip {
            height: block.header.height,
            id: block.header.id,
        }));
    }

    #[test]
    fn acknowledged_error_after_sync_denies_cached_reads_but_reopen_finds_new_tip() {
        let directory = Directory::new();
        let (mut owner, config) = initial(&directory);
        let block = next(&owner);
        let old_tip = owner.tip().unwrap();
        let new_tip = Tip {
            height: block.header.height,
            id: block.header.id,
        };
        owner.journal.test_fail_after_sync();
        assert!(matches!(
            owner.apply_unverified_block(&block),
            Err(PersistenceError::Storage(StorageError::Io(_)))
        ));
        assert_poisoned(&mut owner, &block);
        drop(owner);
        assert!(matches!(
            PersistentJointChain::load(
                &directory.0,
                config.clone(),
                old_tip,
                Recovery::RejectIncompleteTail
            ),
            Err(PersistenceError::UnexpectedTip { .. })
        ));
        let (owner, report) = PersistentJointChain::load(
            &directory.0,
            config,
            new_tip,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(report.discarded_bytes, 0);
        assert_eq!(owner.tip().unwrap(), new_tip);
        assert_eq!(owner.sequence().unwrap(), 2);
        assert_eq!(owner.get_block_header(1).unwrap(), Some(block.header));
    }

    #[test]
    fn partial_synced_write_requires_explicit_recovery_and_restores_old_complete_state() {
        let directory = Directory::new();
        let (mut owner, config) = initial(&directory);
        let block = next(&owner);
        let old_tip = owner.tip().unwrap();
        owner.journal.test_fail_after_partial_write();
        assert!(matches!(
            owner.apply_unverified_block(&block),
            Err(PersistenceError::Storage(StorageError::Io(_)))
        ));
        assert_poisoned(&mut owner, &block);
        drop(owner);
        assert!(matches!(
            PersistentJointChain::load(
                &directory.0,
                config.clone(),
                old_tip,
                Recovery::RejectIncompleteTail
            ),
            Err(PersistenceError::Storage(
                StorageError::IncompleteTail { .. }
            ))
        ));
        let (mut owner, report) = PersistentJointChain::load(
            &directory.0,
            config,
            old_tip,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
        assert!(report.discarded_bytes > 0);
        assert_eq!(owner.tip().unwrap(), old_tip);
        assert_eq!(owner.get_block_header(1).unwrap(), None);
        assert_eq!(owner.sequence().unwrap(), 1);
        owner.apply_unverified_block(&block).unwrap();
        assert_eq!(owner.tip().unwrap().height, 1);
    }

    #[test]
    fn rollback_lost_acknowledgement_reopens_committed_ancestor_and_header_deletion() {
        let directory = Directory::new();
        let (mut owner, config) = initial(&directory);
        let block = next(&owner);
        let tip = owner.apply_unverified_block(&block).unwrap();
        owner.journal.test_fail_after_sync();
        assert!(matches!(
            owner.rollback_tip(tip),
            Err(PersistenceError::Storage(StorageError::Io(_)))
        ));
        assert_poisoned(&mut owner, &block);
        drop(owner);
        assert!(matches!(
            PersistentJointChain::load(
                &directory.0,
                config.clone(),
                tip,
                Recovery::RejectIncompleteTail
            ),
            Err(PersistenceError::UnexpectedTip { .. })
        ));
        let (owner, _) = PersistentJointChain::load(
            &directory.0,
            config.clone(),
            config.anchor,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        assert_eq!(owner.tip().unwrap(), config.anchor);
        assert_eq!(owner.sequence().unwrap(), 3);
        assert_eq!(owner.get_block_header(1).unwrap(), None);
    }

    #[test]
    fn torn_rollback_recovery_retains_both_complete_child_state_and_header() {
        let directory = Directory::new();
        let (mut owner, config) = initial(&directory);
        let block = next(&owner);
        let tip = owner.apply_unverified_block(&block).unwrap();
        owner.journal.test_fail_after_partial_write();
        assert!(matches!(
            owner.rollback_tip(tip),
            Err(PersistenceError::Storage(StorageError::Io(_)))
        ));
        assert_poisoned(&mut owner, &block);
        drop(owner);
        assert!(matches!(
            PersistentJointChain::load(
                &directory.0,
                config.clone(),
                tip,
                Recovery::RejectIncompleteTail
            ),
            Err(PersistenceError::Storage(
                StorageError::IncompleteTail { .. }
            ))
        ));
        let (owner, report) =
            PersistentJointChain::load(&directory.0, config, tip, Recovery::DiscardIncompleteTail)
                .unwrap();
        assert!(report.discarded_bytes > 0);
        assert_eq!(owner.tip().unwrap(), tip);
        assert_eq!(owner.sequence().unwrap(), 2);
        assert_eq!(owner.get_block_header(1).unwrap(), Some(block.header));
    }
}
