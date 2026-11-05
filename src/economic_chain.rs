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
use std::sync::Arc;

/// Version 1 holds one complete before-state per rollback frame.
const MAGIC_V1: &[u8; 5] = b"TLEH\x01";
/// Version 2 holds the state at the rollback floor and the exact inputs of
/// every block in the window; the frame states are rebuilt from them.
const MAGIC: &[u8; 5] = b"TLEH\x02";
const FRAME_ECONOMIC: u8 = 1;
const FRAME_SOURCE: u8 = 2;
const FRAME_BEFORE: u8 = 4;
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
    /// The tip the first block extends; it lives in `economics` because the
    /// owner's snapshots and invariants bind it. Heights are absolute.
    pub fn anchor(&self) -> Tip {
        self.economics.anchor
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

/// What a retained block needs to be applied again.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Replay {
    /// The block's inputs in the TLEC input codec.
    Economic(Vec<u8>),
    /// The block's raw source frame, which the state itself retains.
    Source,
}

/// A retained block. A block without a replay record comes from a version 1
/// checkpoint; the block after it then keeps the state before it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    header: BlockHeader,
    replay: Option<Replay>,
    before: Option<Vec<u8>>,
}

/// The state at the rollback floor and its encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FloorState {
    state: EconomicState,
    bytes: Vec<u8>,
}
impl FloorState {
    fn new(state: EconomicState) -> Result<Self, EconomicError> {
        let bytes = state.snapshot_bytes()?;
        Ok(Self { state, bytes })
    }
}
impl FloorState {
    fn from_applied(state: EconomicState) -> Result<Self, EconomicError> {
        let bytes = state.snapshot_bytes_of_applied_block()?;
        Ok(Self { state, bytes })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicChain {
    config: EconomicChainConfig,
    state: EconomicState,
    tip: Tip,
    floor: Tip,
    floor_state: Arc<FloorState>,
    history: VecDeque<Frame>,
}
impl EconomicChain {
    pub fn new_unverified(
        config: EconomicChainConfig,
        ledger: Ledger,
    ) -> Result<Self, EconomicError> {
        config.validate()?;
        let joint = Self::pristine_joint(&config, ledger)?;
        let state = EconomicState::from_joint_unverified(config.economics.clone(), joint)?;
        Self::from_initial_state(config, state)
    }

    /// Enter the ordinary checkpoint/rollback/durable workflow with complete
    /// actual catalogs and both independently expected bootstrap commitments.
    /// Initial funds still come only from the supplied pristine sole Ledger.
    pub fn new_with_source_catalogs_unverified(
        config: EconomicChainConfig,
        ledger: Ledger,
        owners: crate::source_catalogs::SourceCatalogOwners,
    ) -> Result<Self, EconomicError> {
        config.validate()?;
        let joint = Self::pristine_joint(&config, ledger)?;
        let state = EconomicState::from_joint_with_source_catalogs_unverified(
            config.economics.clone(),
            joint,
            owners,
        )?;
        Self::from_initial_state(config, state)
    }

    fn pristine_joint(
        config: &EconomicChainConfig,
        ledger: Ledger,
    ) -> Result<JointProtocolState, EconomicError> {
        let joint = if let Some(policy) = &config.economics.native_entitlements {
            let mut native_ledger = ledger;
            native_ledger
                .adopt_native_entitlements_unverified(policy)
                .map_err(|_| EconomicError::InvalidConfig)?;
            JointProtocolState::from_native_bootstrap_unverified(
                config.economics.network,
                config.admin.clone(),
                native_ledger,
                config.permitted_spot_properties.clone(),
                policy,
            )?
        } else {
            JointProtocolState::from_bootstrap_unverified(
                config.economics.network,
                config.admin.clone(),
                ledger,
                config.permitted_spot_properties.clone(),
            )?
        };
        Ok(joint)
    }

    fn from_initial_state(
        config: EconomicChainConfig,
        state: EconomicState,
    ) -> Result<Self, EconomicError> {
        let anchor = config.anchor();
        let floor_state = Arc::new(FloorState::new(state.clone())?);
        let chain = Self {
            config,
            state,
            tip: anchor,
            floor: anchor,
            floor_state,
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

    fn preflight_block(&self, block: &UnverifiedEconomicBlock) -> Result<(), EconomicError> {
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
        self.state
            .preflight_contract_inputs(h.height, &block.inputs)
    }

    pub fn apply_unverified_block(
        &mut self,
        block: &UnverifiedEconomicBlock,
    ) -> Result<Tip, EconomicError> {
        self.preflight_block(block)?;
        let inputs = EconomicState::encode_window_inputs(&block.inputs)?;
        let mut state = self.state.clone();
        state.apply_block_unverified(block.header.height, block.inputs.clone())?;
        self.commit_block(state, block.header, Replay::Economic(inputs))?;
        Ok(self.tip)
    }

    /// Restore the state after the block `target`, which is the floor or the
    /// tip of a retained block below the current tip. The state is rebuilt from
    /// the nearest stored state by applying the recorded blocks again.
    pub fn rollback_to_tip(&mut self, target: Tip) -> Result<Tip, EconomicError> {
        if target.height >= self.tip.height || target.height < self.floor.height {
            return Err(EconomicError::WrongTip);
        }
        let retained = (target.height - self.floor.height) as usize;
        let retained_tip = match retained.checked_sub(1) {
            None => self.floor,
            Some(index) => {
                let frame = self.history.get(index).ok_or(EconomicError::Invariant)?;
                Tip {
                    height: frame.header.height,
                    id: frame.header.id,
                }
            }
        };
        if retained_tip != target {
            return Err(EconomicError::WrongTip);
        }
        let mut restored = self.state_at(target.height)?;
        restored.inherit_receipt_log(&self.state);
        let mut history: VecDeque<Frame> = self.history.iter().take(retained).cloned().collect();
        if let Some(oldest) = history.front_mut() {
            oldest.before = None;
        }
        let next = Self {
            config: self.config.clone(),
            state: restored,
            tip: target,
            floor: self.floor,
            floor_state: Arc::clone(&self.floor_state),
            history,
        };
        next.checkpoint_bytes()?;
        *self = next;
        Ok(self.tip)
    }

    pub fn rollback_tip(&mut self, expected_tip: Tip) -> Result<Tip, EconomicError> {
        if expected_tip != self.tip {
            return Err(EconomicError::WrongTip);
        }
        let frame = self.history.back().ok_or(EconomicError::WrongTip)?;
        if frame.header.height != self.tip.height || frame.header.id != self.tip.id {
            return Err(EconomicError::Invariant);
        }
        let target = Tip {
            height: frame.header.height - 1,
            id: frame.header.parent,
        };
        self.rollback_to_tip(target)
    }

    /// Publish `state` as the result of the block `header`: record the block,
    /// move the floor when the window is full, and validate the checkpoint.
    fn commit_block(
        &mut self,
        state: EconomicState,
        header: BlockHeader,
        replay: Replay,
    ) -> Result<(), EconomicError> {
        let parent = self.tip;
        Self::check_transition(&self.config, &self.state, &state, parent, header)?;
        // The block after one without a replay record keeps the state before it.
        let before = match self.history.back() {
            Some(frame) if frame.replay.is_none() => Some(self.state.snapshot_bytes()?),
            _ => None,
        };
        let mut history = self.history.clone();
        history.push_back(Frame {
            header,
            replay: Some(replay),
            before,
        });
        let mut floor = self.floor;
        let mut floor_state = Arc::clone(&self.floor_state);
        if history.len() > self.config.rollback_limit {
            let evicted = history.pop_front().ok_or(EconomicError::Invariant)?;
            let known = match history.front() {
                Some(next) => match &next.before {
                    Some(bytes) => Known::Bytes(bytes),
                    None => Known::Missing,
                },
                None => Known::Current,
            };
            let after = replay_block(
                &self.config,
                floor_state.state.clone(),
                &evicted,
                known,
                &state,
            )?;
            floor = Tip {
                height: evicted.header.height,
                id: evicted.header.id,
            };
            floor_state = Arc::new(FloorState::from_applied(after)?);
            if let Some(oldest) = history.front_mut() {
                oldest.before = None;
            }
        }
        let next = Self {
            config: self.config.clone(),
            state,
            tip: Tip {
                height: header.height,
                id: header.id,
            },
            floor,
            floor_state,
            history,
        };
        next.checkpoint_bytes()?;
        *self = next;
        Ok(())
    }

    /// The linkage a block must keep with the state before it: the source
    /// frames only grow, and the native and contract histories continue.
    fn check_transition(
        config: &EconomicChainConfig,
        prior: &EconomicState,
        next: &EconomicState,
        parent: Tip,
        header: BlockHeader,
    ) -> Result<(), EconomicError> {
        if (config.economics.source_consensus.is_some()
            && prior
                .source_parent_unverified()
                .map_err(|_| EconomicError::Invariant)?
                != parent)
            || prior.tip_height() != parent.height
        {
            return Err(EconomicError::Invariant);
        }
        if config.economics.source_consensus.is_some() {
            let current = next.source_consensus_state();
            let previous = prior.source_consensus_state();
            if previous.bootstrap != current.bootstrap
                || !current.frames.starts_with(&previous.frames)
            {
                return Err(EconomicError::Invariant);
            }
        }
        if config.economics.native_entitlements.is_some()
            || config.economics.contract_binding.is_some()
        {
            prior.native_history_transition(next, header.height)?;
            prior.contract_history_transition(next, header.height)?;
        }
        Ok(())
    }

    /// The state after the block at `height`, when it is stored.
    fn known_state(&self, height: u64) -> Result<Option<EconomicState>, EconomicError> {
        if height == self.tip.height {
            return Ok(Some(self.state.clone()));
        }
        if height == self.floor.height {
            return Ok(Some(self.floor_state.state.clone()));
        }
        let successor = height
            .checked_sub(self.floor.height)
            .and_then(|index| self.history.get(index as usize));
        match successor.and_then(|frame| frame.before.as_ref()) {
            Some(bytes) => Ok(Some(self.config.decode(bytes)?)),
            None => Ok(None),
        }
    }

    /// The state after the block at `target`, from the closest stored state.
    fn state_at(&self, target: u64) -> Result<EconomicState, EconomicError> {
        let mut start = target;
        let mut state = loop {
            if let Some(state) = self.known_state(start)? {
                break state;
            }
            if start <= self.floor.height {
                return Err(EconomicError::Invariant);
            }
            start -= 1;
        };
        for height in start + 1..=target {
            let index = (height - self.floor.height - 1) as usize;
            state = replay_block(
                &self.config,
                state,
                self.history.get(index).ok_or(EconomicError::Invariant)?,
                self.known_after(index),
                &self.state,
            )?;
        }
        Ok(state)
    }

    fn known_after(&self, index: usize) -> Known<'_> {
        match self.history.get(index + 1) {
            Some(next) => match &next.before {
                Some(bytes) => Known::Bytes(bytes),
                None => Known::Missing,
            },
            None => Known::Current,
        }
    }

    /// Rebuild every retained state from the floor and check that the replay
    /// reaches the current state. The checkpoint reader runs it once; the
    /// per-block checkpoint relies on the transition checks of each block.
    fn verify_window(&self) -> Result<(), EconomicError> {
        let floor = &self.floor_state.state;
        if floor.tip_height() != self.floor.height
            || (self.config.economics.source_consensus.is_some()
                && floor
                    .source_parent_unverified()
                    .map_err(|_| EconomicError::Invariant)?
                    != self.floor)
            || floor.snapshot_bytes()? != self.floor_state.bytes
        {
            return Err(EconomicError::Invariant);
        }
        let mut prior = floor.clone();
        let mut parent = self.floor;
        for (index, frame) in self.history.iter().enumerate() {
            let next = replay_block(
                &self.config,
                prior.clone(),
                frame,
                self.known_after(index),
                &self.state,
            )?;
            Self::check_transition(&self.config, &prior, &next, parent, frame.header)?;
            parent = Tip {
                height: frame.header.height,
                id: frame.header.id,
            };
            prior = next;
        }
        if parent != self.tip || prior != self.state {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }

    fn verified_state_bytes(&self) -> Result<Vec<u8>, EconomicError> {
        self.config.validate()?;
        if self.state.config() != &self.config.economics
            || self.history.len() > self.config.rollback_limit
            || self.floor.height < self.config.anchor().height
            || self.floor.height > self.tip.height
            || self.state.tip_height() != self.tip.height
            || (self.floor.height == self.config.anchor().height
                && self.floor != self.config.anchor())
            || (self.floor.height > self.config.anchor().height
                && self.floor.id == self.config.anchor().id)
            || self.tip.height.checked_sub(self.floor.height) != Some(self.history.len() as u64)
            || self.floor_state.state.config() != &self.config.economics
            || self.floor_state.state.tip_height() != self.floor.height
            || (self.history.is_empty() && self.floor_state.state != self.state)
        {
            return Err(EconomicError::Invariant);
        }
        if self.config.economics.source_consensus.is_some()
            && self
                .state
                .source_parent_unverified()
                .map_err(|_| EconomicError::Invariant)?
                != self.tip
        {
            return Err(EconomicError::Invariant);
        }
        // Recheck the actual administrator and permitted set in every snapshot.
        let state_bytes = self.state.snapshot_bytes()?;
        if self.config.decode(&state_bytes)? != self.state {
            return Err(EconomicError::Invariant);
        }
        let mut cursor = self.floor;
        let mut ids = BTreeSet::from([self.config.anchor().id, self.floor.id]);
        let mut previous_replays = true;
        for frame in &self.history {
            if frame.header.height != cursor.height + 1
                || frame.header.parent != cursor.id
                || !ids.insert(frame.header.id)
                || frame.before.is_some() == previous_replays
                || match &frame.replay {
                    Some(Replay::Economic(_)) => self.config.economics.source_consensus.is_some(),
                    Some(Replay::Source) => self.config.economics.source_consensus.is_none(),
                    None => false,
                }
            {
                return Err(EconomicError::Invariant);
            }
            previous_replays = frame.replay.is_some();
            cursor = Tip {
                height: frame.header.height,
                id: frame.header.id,
            };
        }
        if cursor != self.tip {
            return Err(EconomicError::Invariant);
        }
        Ok(state_bytes)
    }

    fn checkpoint_header(&self, magic: &[u8; 5]) -> Vec<u8> {
        let mut out = magic.to_vec();
        out.push(self.config.economics.network.tag());
        out.extend_from_slice(&self.config.economics.network.genesis().0);
        out.push(self.config.rollback_limit as u8);
        write_tip(&mut out, self.floor);
        write_tip(&mut out, self.tip);
        out
    }

    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>, EconomicError> {
        let state_bytes = self.verified_state_bytes()?;
        let mut out = self.checkpoint_header(MAGIC);
        embedded(&mut out, &state_bytes)?;
        out.push(self.history.len() as u8);
        for frame in &self.history {
            out.extend_from_slice(&frame.header.height.to_be_bytes());
            out.extend_from_slice(&frame.header.id.0);
            out.extend_from_slice(&frame.header.parent.0);
            let mut flags = match &frame.replay {
                Some(Replay::Economic(_)) => FRAME_ECONOMIC,
                Some(Replay::Source) => FRAME_SOURCE,
                None => 0,
            };
            if frame.before.is_some() {
                flags |= FRAME_BEFORE;
            }
            out.push(flags);
            if let Some(Replay::Economic(inputs)) = &frame.replay {
                embedded(&mut out, inputs)?;
            }
            if let Some(before) = &frame.before {
                embedded(&mut out, before)?;
            }
        }
        if !self.history.is_empty() {
            embedded(&mut out, &self.floor_state.bytes)?;
        }
        with_crc(out).map_err(|_| EconomicError::Capacity)
    }

    /// The version 1 encoding: one complete before-state per retained block,
    /// rebuilt from the floor state and the recorded blocks.
    pub fn checkpoint_bytes_v1(&self) -> Result<Vec<u8>, EconomicError> {
        let state_bytes = self.verified_state_bytes()?;
        let mut out = self.checkpoint_header(MAGIC_V1);
        embedded(&mut out, &state_bytes)?;
        out.push(self.history.len() as u8);
        let mut before = self.floor_state.state.clone();
        for (index, frame) in self.history.iter().enumerate() {
            out.extend_from_slice(&frame.header.height.to_be_bytes());
            out.extend_from_slice(&frame.header.id.0);
            out.extend_from_slice(&frame.header.parent.0);
            embedded(&mut out, &before.snapshot_bytes()?)?;
            before = replay_block(
                &self.config,
                before,
                frame,
                self.known_after(index),
                &self.state,
            )?;
        }
        with_crc(out).map_err(|_| EconomicError::Capacity)
    }

    /// The expected policy and exact tip must be supplied independently. The
    /// history proves internal linkage only, not that any block was valid.
    /// Version 1 and version 2 checkpoints load; the chain writes version 2.
    pub fn from_checkpoint_bytes(
        bytes: &[u8],
        config: EconomicChainConfig,
        expected_tip: Tip,
    ) -> Result<Self, EconomicError> {
        config.validate()?;
        let bad = |_| EconomicError::InvalidSnapshot;
        let raw = verify_crc(bytes).map_err(bad)?;
        let mut r = Cursor::new(raw);
        let magic = r.fixed::<5>().map_err(bad)?;
        let legacy = magic == *MAGIC_V1;
        if (!legacy && magic != *MAGIC)
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
        let mut floor_state = None;
        for index in 0..count {
            let header = BlockHeader {
                height: r.u64().map_err(bad)?,
                id: BlockId(r.fixed().map_err(bad)?),
                parent: BlockId(r.fixed().map_err(bad)?),
            };
            if legacy {
                // The oldest before-state is the floor state itself.
                let before = read_embedded(&mut r)?;
                if index == 0 {
                    floor_state = Some(FloorState {
                        state: config.decode(before)?,
                        bytes: before.to_vec(),
                    });
                }
                history.push_back(Frame {
                    header,
                    replay: None,
                    before: (index != 0).then(|| before.to_vec()),
                });
                continue;
            }
            let flags = r.u8().map_err(bad)?;
            if flags & !(FRAME_ECONOMIC | FRAME_SOURCE | FRAME_BEFORE) != 0
                || flags & (FRAME_ECONOMIC | FRAME_SOURCE) == (FRAME_ECONOMIC | FRAME_SOURCE)
            {
                return Err(EconomicError::InvalidSnapshot);
            }
            let replay = if flags & FRAME_ECONOMIC != 0 {
                let inputs = read_embedded(&mut r)?;
                EconomicState::decode_window_inputs(inputs)?;
                Some(Replay::Economic(inputs.to_vec()))
            } else if flags & FRAME_SOURCE != 0 {
                Some(Replay::Source)
            } else {
                None
            };
            let before = if flags & FRAME_BEFORE != 0 {
                Some(read_embedded(&mut r)?.to_vec())
            } else {
                None
            };
            history.push_back(Frame {
                header,
                replay,
                before,
            });
        }
        if !legacy && count > 0 {
            let floor_bytes = read_embedded(&mut r)?;
            floor_state = Some(FloorState {
                state: config.decode(floor_bytes)?,
                bytes: floor_bytes.to_vec(),
            });
        }
        r.end().map_err(bad)?;
        let floor_state = match floor_state {
            Some(floor_state) => floor_state,
            None => FloorState::new(state.clone())?,
        };
        let chain = Self {
            config,
            state,
            tip,
            floor,
            floor_state: Arc::new(floor_state),
            history,
        };
        chain.verified_state_bytes()?;
        chain.verify_window()?;
        let canonical = if legacy {
            chain.checkpoint_bytes_v1()?
        } else {
            chain.checkpoint_bytes()?
        };
        if canonical != bytes {
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
    #[cfg(test)]
    pub(crate) fn test_fail_next_commit_after_sync(&mut self) {
        self.journal.test_fail_after_sync();
    }

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
        self.chain.preflight_block(block)?;
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
        next.rollback_to_tip(target)?;
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

/// Where the state after a block without a replay record is found.
enum Known<'a> {
    Bytes(&'a [u8]),
    Current,
    Missing,
}

/// The state after `frame`, applied to the state before it. A block without a
/// replay record takes its result from the stored state that follows it.
fn replay_block(
    config: &EconomicChainConfig,
    mut prior: EconomicState,
    frame: &Frame,
    known: Known<'_>,
    current: &EconomicState,
) -> Result<EconomicState, EconomicError> {
    let height = frame.header.height;
    match &frame.replay {
        Some(Replay::Economic(bytes)) => {
            prior.apply_block_unverified(height, EconomicState::decode_window_inputs(bytes)?)?;
            Ok(prior)
        }
        Some(Replay::Source) => {
            // The tip state holds one source frame per block after the anchor.
            let source = height
                .checked_sub(current.config().anchor.height + 1)
                .and_then(|index| current.source_consensus_state().frames.get(index as usize))
                .filter(|source| source.header == frame.header)
                .ok_or(EconomicError::Invariant)?;
            prior
                .apply_source_block_unverified(source.header, source.mode, &source.transactions)
                .map_err(|_| EconomicError::Invariant)?;
            Ok(prior)
        }
        None => match known {
            Known::Bytes(bytes) => config.decode(bytes),
            Known::Current => Ok(current.clone()),
            Known::Missing => Err(EconomicError::Invariant),
        },
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

impl EconomicChain {
    pub fn apply_source_unverified_block(
        &mut self,
        block: &crate::consensus_source::UnverifiedSourceBlock,
    ) -> core::result::Result<
        crate::consensus_source::SourceBlockReport,
        crate::consensus_source::ConsensusSourceError,
    > {
        use crate::consensus_source::{preflight_transactions, ConsensusSourceError};
        preflight_transactions(&block.transactions)?;
        let h = block.header;
        if block.network != self.config.economics.network
            || h.height
                != self
                    .tip
                    .height
                    .checked_add(1)
                    .ok_or(ConsensusSourceError::WrongParent)?
            || h.parent != self.tip.id
            || h.id == self.config.anchor().id
            || h.id == self.floor.id
            || h.id == self.tip.id
            || self.history.iter().any(|f| f.header.id == h.id)
        {
            return Err(ConsensusSourceError::WrongParent);
        }
        self.state
            .preflight_source_frame(h, block.mode, &block.transactions)?;
        let mut state = self.state.clone();
        let report = state.apply_source_block_unverified(h, block.mode, &block.transactions)?;
        self.commit_block(state, h, Replay::Source)?;
        Ok(report)
    }
}
impl DurableEconomicChain {
    /// One existing synced Journal transaction publishes economic state and all
    /// accepted/rejected source evidence together. RPC observations remain a
    /// separate journal and cannot call this method as an admission capability.
    pub fn apply_source_unverified_block(
        &mut self,
        expected: EconomicHead,
        block: &crate::consensus_source::UnverifiedSourceBlock,
    ) -> Result<(EconomicHead, crate::consensus_source::SourceBlockReport), DurableEconomicError>
    {
        self.check_head(expected)?;
        crate::consensus_source::preflight_transactions(&block.transactions)
            .map_err(|_| EconomicError::Capacity)?;
        self.chain
            .state
            .preflight_source_frame(block.header, block.mode, &block.transactions)?;
        let mut next = self.chain.clone();
        let report = next
            .apply_source_unverified_block(block)
            .map_err(|error| match error {
                crate::consensus_source::ConsensusSourceError::Economic(error) => error,
                crate::consensus_source::ConsensusSourceError::Capacity => EconomicError::Capacity,
                crate::consensus_source::ConsensusSourceError::WrongParent => {
                    EconomicError::WrongTip
                }
                _ => EconomicError::InvalidInput,
            })?;
        let head = self.publish(next)?;
        Ok((head, report))
    }
}
