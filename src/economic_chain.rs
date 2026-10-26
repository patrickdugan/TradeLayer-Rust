//! Full economic rollback and synced journal ownership. All chain inputs are
//! caller assertions; checkpoints and their digests provide no authentication.

use crate::chain::{BlockHeader, BlockId, Tip, MAX_REORG_HISTORY};
use crate::codec::{verify_crc, with_crc, Cursor, MAX_CHECKPOINT_BYTES};
use crate::economic::{EconomicConfig, EconomicError, EconomicInput, EconomicState};
use crate::joint::JointProtocolState;
use crate::ledger::{AccountId, Ledger, PropertyId};
use crate::storage::{Journal, Mutation, Namespace, Recovery, RecoveryReport, StorageError};
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 5] = b"TLEH\x01";
const STATE_KEY: &str = "economic/full-chain-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicChainConfig {
    pub economics: EconomicConfig,
    pub admin: AccountId,
    pub permitted_spot_properties: BTreeSet<PropertyId>,
    pub rollback_limit: usize,
}
impl EconomicChainConfig {
    pub fn validate(&self) -> Result<(), EconomicError> {
        self.economics.validate()?;
        if self.admin.as_str().starts_with('~')
            || !(1..=MAX_REORG_HISTORY).contains(&self.rollback_limit)
            || self.permitted_spot_properties.len() > 4096
        {
            return Err(EconomicError::InvalidConfig);
        }
        Ok(())
    }
    pub fn anchor(&self) -> Tip {
        Tip {
            height: 0,
            id: self.economics.network.genesis(),
        }
    }
    fn decode(&self, bytes: &[u8]) -> Result<EconomicState, EconomicError> {
        EconomicState::from_snapshot_bytes(
            bytes,
            &self.economics,
            &self.admin,
            &self.permitted_spot_properties,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedEconomicBlock {
    pub network: crate::network_profile::TradeLayerMainnet,
    pub header: BlockHeader,
    pub inputs: Vec<EconomicInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    header: BlockHeader,
    before: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicChain {
    config: EconomicChainConfig,
    state: EconomicState,
    tip: Tip,
    floor: Tip,
    history: VecDeque<Frame>,
}
impl EconomicChain {
    pub fn new_unverified(
        config: EconomicChainConfig,
        ledger: Ledger,
    ) -> Result<Self, EconomicError> {
        config.validate()?;
        let joint = JointProtocolState::from_bootstrap_unverified(
            config.economics.network,
            config.admin.clone(),
            ledger,
            config.permitted_spot_properties.clone(),
        )?;
        let state = EconomicState::from_joint_unverified(config.economics.clone(), joint)?;
        let anchor = config.anchor();
        let chain = Self {
            config,
            state,
            tip: anchor,
            floor: anchor,
            history: VecDeque::new(),
        };
        chain.checkpoint_bytes()?;
        Ok(chain)
    }
    pub fn config(&self) -> &EconomicChainConfig {
        &self.config
    }
    pub fn state(&self) -> &EconomicState {
        &self.state
    }
    pub fn tip(&self) -> Tip {
        self.tip
    }
    pub fn rollback_floor(&self) -> Tip {
        self.floor
    }
    pub fn rollback_depth(&self) -> usize {
        self.history.len()
    }

    pub fn apply_unverified_block(
        &mut self,
        block: &UnverifiedEconomicBlock,
    ) -> Result<Tip, EconomicError> {
        if block.inputs.len() > crate::economic::MAX_ECONOMIC_INPUTS {
            return Err(EconomicError::Capacity);
        }
        let h = block.header;
        if block.network != self.config.economics.network
            || h.height
                != self
                    .tip
                    .height
                    .checked_add(1)
                    .ok_or(EconomicError::InvalidHeight)?
            || h.parent != self.tip.id
            || h.id == self.config.anchor().id
            || h.id == self.floor.id
            || h.id == self.tip.id
            || self.history.iter().any(|f| f.header.id == h.id)
        {
            return Err(EconomicError::WrongTip);
        }
        let mut next = self.clone();
        let before = next.state.snapshot_bytes()?;
        next.state
            .apply_block_unverified(h.height, block.inputs.clone())?;
        next.history.push_back(Frame { header: h, before });
        if next.history.len() > next.config.rollback_limit {
            let evicted = next.history.pop_front().ok_or(EconomicError::Invariant)?;
            next.floor = Tip {
                height: evicted.header.height,
                id: evicted.header.id,
            };
        }
        next.tip = Tip {
            height: h.height,
            id: h.id,
        };
        next.checkpoint_bytes()?;
        *self = next;
        Ok(self.tip)
    }

    pub fn rollback_tip(&mut self, expected_tip: Tip) -> Result<Tip, EconomicError> {
        if expected_tip != self.tip {
            return Err(EconomicError::WrongTip);
        }
        let mut next = self.clone();
        let frame = next.history.pop_back().ok_or(EconomicError::WrongTip)?;
        if frame.header.height != next.tip.height || frame.header.id != next.tip.id {
            return Err(EconomicError::Invariant);
        }
        next.state = next.config.decode(&frame.before)?;
        next.tip = Tip {
            height: frame.header.height - 1,
            id: frame.header.parent,
        };
        next.checkpoint_bytes()?;
        *self = next;
        Ok(self.tip)
    }

    fn verify(&self) -> Result<(), EconomicError> {
        self.config.validate()?;
        self.state.verify()?;
        if self.state.config() != &self.config.economics
            || self.history.len() > self.config.rollback_limit
            || self.floor.height > self.tip.height
            || self.state.finalized_height().unwrap_or(0) != self.tip.height
            || (self.floor.height == 0 && self.floor != self.config.anchor())
            || (self.floor.height > 0 && self.floor.id == self.config.anchor().id)
            || self.tip.height.checked_sub(self.floor.height) != Some(self.history.len() as u64)
        {
            return Err(EconomicError::Invariant);
        }
        // Recheck the actual administrator and permitted set in every snapshot.
        if self.config.decode(&self.state.snapshot_bytes()?)? != self.state {
            return Err(EconomicError::Invariant);
        }
        let mut cursor = self.floor;
        let mut ids = BTreeSet::from([self.config.anchor().id, self.floor.id]);
        for frame in &self.history {
            if frame.header.height != cursor.height + 1
                || frame.header.parent != cursor.id
                || !ids.insert(frame.header.id)
            {
                return Err(EconomicError::Invariant);
            }
            let before = self.config.decode(&frame.before)?;
            if before.finalized_height().unwrap_or(0) != cursor.height {
                return Err(EconomicError::Invariant);
            }
            cursor = Tip {
                height: frame.header.height,
                id: frame.header.id,
            };
        }
        if cursor != self.tip {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }

    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>, EconomicError> {
        self.verify()?;
        let mut out = MAGIC.to_vec();
        out.push(self.config.economics.network.tag());
        out.extend_from_slice(&self.config.economics.network.genesis().0);
        out.push(self.config.rollback_limit as u8);
        write_tip(&mut out, self.floor);
        write_tip(&mut out, self.tip);
        embedded(&mut out, &self.state.snapshot_bytes()?)?;
        out.push(self.history.len() as u8);
        for frame in &self.history {
            out.extend_from_slice(&frame.header.height.to_be_bytes());
            out.extend_from_slice(&frame.header.id.0);
            out.extend_from_slice(&frame.header.parent.0);
            embedded(&mut out, &frame.before)?;
        }
        with_crc(out).map_err(|_| EconomicError::Capacity)
    }

    /// The expected policy and exact tip must be supplied independently. The
    /// history proves internal linkage only, not that any block was valid.
    pub fn from_checkpoint_bytes(
        bytes: &[u8],
        config: EconomicChainConfig,
        expected_tip: Tip,
    ) -> Result<Self, EconomicError> {
        config.validate()?;
        let bad = |_| EconomicError::InvalidSnapshot;
        let raw = verify_crc(bytes).map_err(bad)?;
        let mut r = Cursor::new(raw);
        if r.fixed::<5>().map_err(bad)? != *MAGIC
            || r.u8().map_err(bad)? != config.economics.network.tag()
            || r.fixed::<32>().map_err(bad)? != config.economics.network.genesis().0
            || r.u8().map_err(bad)? as usize != config.rollback_limit
        {
            return Err(EconomicError::InvalidSnapshot);
        }
        let floor = read_tip(&mut r)?;
        let tip = read_tip(&mut r)?;
        if tip != expected_tip {
            return Err(EconomicError::WrongTip);
        }
        let state = config.decode(read_embedded(&mut r)?)?;
        let count = r.u8().map_err(bad)? as usize;
        if count > config.rollback_limit {
            return Err(EconomicError::Capacity);
        }
        let mut history = VecDeque::new();
        for _ in 0..count {
            let header = BlockHeader {
                height: r.u64().map_err(bad)?,
                id: BlockId(r.fixed().map_err(bad)?),
                parent: BlockId(r.fixed().map_err(bad)?),
            };
            let before = read_embedded(&mut r)?.to_vec();
            history.push_back(Frame { header, before });
        }
        r.end().map_err(bad)?;
        let chain = Self {
            config,
            state,
            tip,
            floor,
            history,
        };
        chain.verify()?;
        if chain.checkpoint_bytes()? != bytes {
            return Err(EconomicError::InvalidSnapshot);
        }
        Ok(chain)
    }
}

/// Exact local freshness token. Its sequence changes even when a rollback
/// returns to the same block; commitment covers every retained economic frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EconomicHead {
    pub sequence: u64,
    pub tip: Tip,
    pub commitment: [u8; 32],
}

#[derive(Debug)]
pub enum DurableEconomicError {
    Economic(EconomicError),
    Storage(StorageError),
    StaleHead,
    InvalidStore,
}
impl core::fmt::Display for DurableEconomicError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DurableEconomicError {}
impl From<EconomicError> for DurableEconomicError {
    fn from(e: EconomicError) -> Self {
        Self::Economic(e)
    }
}
impl From<StorageError> for DurableEconomicError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

#[derive(Debug)]
pub struct DurableEconomicChain {
    journal: Journal,
    chain: EconomicChain,
}
impl DurableEconomicChain {
    pub fn initialize(
        root: &Path,
        chain: EconomicChain,
        expected_tip: Tip,
    ) -> Result<Self, DurableEconomicError> {
        let bytes = chain.checkpoint_bytes()?;
        let chain =
            EconomicChain::from_checkpoint_bytes(&bytes, chain.config.clone(), expected_tip)?;
        let namespace = Namespace::for_mainnet(chain.config.economics.network, "clearing")?;
        let (mut journal, _) = Journal::open(root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(DurableEconomicError::InvalidStore);
        }
        journal.commit(vec![Mutation::Put {
            key: STATE_KEY.into(),
            value: bytes,
        }])?;
        Ok(Self { journal, chain })
    }

    pub fn load(
        root: &Path,
        config: EconomicChainConfig,
        expected: EconomicHead,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), DurableEconomicError> {
        config.validate()?;
        let namespace = Namespace::for_mainnet(config.economics.network, "clearing")?;
        let (journal, report) = Journal::open_existing(root, namespace, recovery)?;
        if journal.entries()?.count() != 1 {
            return Err(DurableEconomicError::InvalidStore);
        }
        let bytes = journal
            .get(STATE_KEY)?
            .ok_or(DurableEconomicError::InvalidStore)?;
        if journal.sequence()? != expected.sequence || commitment(bytes) != expected.commitment {
            return Err(DurableEconomicError::StaleHead);
        }
        let chain = EconomicChain::from_checkpoint_bytes(bytes, config, expected.tip)?;
        Ok((Self { journal, chain }, report))
    }
    pub fn chain(&self) -> Result<&EconomicChain, DurableEconomicError> {
        self.journal.sequence()?;
        Ok(&self.chain)
    }
    pub fn head(&self) -> Result<EconomicHead, DurableEconomicError> {
        let sequence = self.journal.sequence()?;
        let bytes = self
            .journal
            .get(STATE_KEY)?
            .ok_or(DurableEconomicError::InvalidStore)?;
        Ok(EconomicHead {
            sequence,
            tip: self.chain.tip(),
            commitment: commitment(bytes),
        })
    }
    pub fn path(&self) -> PathBuf {
        self.journal.path().to_owned()
    }
    pub fn apply_unverified_block(
        &mut self,
        expected: EconomicHead,
        block: &UnverifiedEconomicBlock,
    ) -> Result<EconomicHead, DurableEconomicError> {
        self.check_head(expected)?;
        let mut next = self.chain.clone();
        next.apply_unverified_block(block)?;
        self.publish(next)
    }
    pub fn rollback_to(
        &mut self,
        expected: EconomicHead,
        target: Tip,
    ) -> Result<EconomicHead, DurableEconomicError> {
        self.check_head(expected)?;
        if target.height >= self.chain.tip().height {
            return Err(DurableEconomicError::Economic(EconomicError::WrongTip));
        }
        let mut next = self.chain.clone();
        while next.tip().height > target.height {
            next.rollback_tip(next.tip())?;
        }
        if next.tip() != target {
            return Err(DurableEconomicError::Economic(EconomicError::WrongTip));
        }
        self.publish(next)
    }
    fn check_head(&self, expected: EconomicHead) -> Result<(), DurableEconomicError> {
        if self.head()? != expected {
            return Err(DurableEconomicError::StaleHead);
        }
        Ok(())
    }
    fn publish(&mut self, next: EconomicChain) -> Result<EconomicHead, DurableEconomicError> {
        let bytes = next.checkpoint_bytes()?;
        // The OS-synced frame is the commit point. A failure leaves cached state
        // unpublished; Journal poisons uncertain writes and denies cached reads.
        self.journal.commit(vec![Mutation::Put {
            key: STATE_KEY.into(),
            value: bytes,
        }])?;
        self.chain = next;
        self.head()
    }
}

fn write_tip(out: &mut Vec<u8>, tip: Tip) {
    out.extend_from_slice(&tip.height.to_be_bytes());
    out.extend_from_slice(&tip.id.0);
}
fn read_tip(r: &mut Cursor<'_>) -> Result<Tip, EconomicError> {
    Ok(Tip {
        height: r.u64().map_err(|_| EconomicError::InvalidSnapshot)?,
        id: BlockId(r.fixed().map_err(|_| EconomicError::InvalidSnapshot)?),
    })
}
fn embedded(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EconomicError> {
    if out
        .len()
        .saturating_add(4)
        .saturating_add(bytes.len())
        .saturating_add(4)
        > MAX_CHECKPOINT_BYTES
    {
        return Err(EconomicError::Capacity);
    }
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn read_embedded<'a>(r: &mut Cursor<'a>) -> Result<&'a [u8], EconomicError> {
    let size = r.u32().map_err(|_| EconomicError::InvalidSnapshot)? as usize;
    if size > MAX_CHECKPOINT_BYTES {
        return Err(EconomicError::Capacity);
    }
    r.take(size).map_err(|_| EconomicError::InvalidSnapshot)
}
fn commitment(bytes: &[u8]) -> [u8; 32] {
    crate::hash256::double_sha256_parts(&[b"TradeLayer/DurableEconomicChain/v1/", bytes])
}
