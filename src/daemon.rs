//! Owned, durable observations of a configured node. No observation authorizes
//! a protocol input, economic transition, best-chain choice or rollback.
use crate::bootstrap::{BootstrapControl, ClaimedNodeIdentity};
use crate::chain::{BlockHeader, BlockId, Tip};
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::network_profile::TradeLayerMainnet;
use crate::parallel_txindex::ParallelIndexConfig;
use crate::raw_block_ingest::{UnverifiedRawBlockView, MAX_UNVERIFIED_RAW_BLOCK_BYTES};
use crate::storage::{Journal, Mutation, Namespace, Recovery, RecoveryReport, StorageError};
use std::path::{Path, PathBuf};

pub const SOURCE_SHA256: &str = "006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009";
pub const MAX_OBSERVED_BLOCKS: u32 = 64;
pub const MAX_SPOOL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_STATE_BYTES: usize = MAX_SPOOL_BYTES as usize + 128 * 1024;
const KEY: &str = "observer/full-v1";
const MAGIC: &[u8; 5] = b"TLOB\x01";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationRpcError {
    Transient,
    Unavailable,
    InvalidResponse,
    WrongIdentity,
}
pub trait ObservationRpc {
    /// May perform up to three controlled RPCs for family/mainnet/genesis.
    fn verify_identity(&mut self) -> Result<ClaimedNodeIdentity, ObservationRpcError>;
    fn reported_tip(&mut self) -> Result<Tip, ObservationRpcError>;
    fn block_hash(&mut self, height: u64) -> Result<BlockId, ObservationRpcError>;
    fn raw_block(&mut self, hash: BlockId) -> Result<Vec<u8>, ObservationRpcError>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserverConfig {
    pub network: TradeLayerMainnet,
    pub anchor: Tip,
    pub poll_interval_ms: u64,
    pub retry_base_ms: u64,
    pub retry_max_ms: u64,
    pub max_retries: u8,
    pub max_blocks: u32,
    pub max_raw_bytes: u64,
    pub scan: ParallelIndexConfig,
}
impl ObserverConfig {
    pub fn new(network: TradeLayerMainnet, anchor: Tip) -> Result<Self, DaemonError> {
        let config = Self {
            network,
            anchor,
            poll_interval_ms: 10_000,
            retry_base_ms: 1_000,
            retry_max_ms: 60_000,
            max_retries: 3,
            max_blocks: MAX_OBSERVED_BLOCKS,
            max_raw_bytes: MAX_SPOOL_BYTES,
            scan: ParallelIndexConfig::default(),
        };
        config.validate()?;
        Ok(config)
    }
    fn validate(&self) -> Result<(), DaemonError> {
        if self.anchor.height == 0 && self.anchor.id != self.network.genesis()
            || self.anchor.height == u64::MAX
            || self.poll_interval_ms == 0
            || self.poll_interval_ms > 300_000
            || self.retry_base_ms == 0
            || self.retry_base_ms > self.retry_max_ms
            || self.retry_max_ms > 300_000
            || self.max_retries > 16
            || self.max_blocks == 0
            || self.max_blocks > MAX_OBSERVED_BLOCKS
            || self.max_raw_bytes == 0
            || self.max_raw_bytes > MAX_SPOOL_BYTES
            || self.scan.validate().is_err()
        {
            return Err(DaemonError::InvalidConfig);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub enum DaemonError {
    Storage(StorageError),
    InvalidConfig,
    InvalidSnapshot,
    StaleHead,
    InvalidTransition,
    ClockOverflow,
}
impl From<StorageError> for DaemonError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}
impl core::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DaemonError {}
type Result<T, E = DaemonError> = core::result::Result<T, E>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineReason {
    WrongIdentity,
    InvalidRpcResponse,
    HistoricalBlockUnavailable,
    UnsupportedDogecoinLayout,
    InvalidRawBlock,
    HashMismatch,
    ParentMismatch,
    ForkDetected,
    CursorAboveTip,
    SpoolFull,
    RetryExhausted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverMode {
    Observing,
    Paused,
    Stopped,
    Quarantined(QuarantineReason),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSummary {
    pub transaction_count: u32,
    pub candidate_count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawObservation {
    header: BlockHeader,
    raw: Vec<u8>,
    summary: Option<BlockSummary>,
}
impl RawObservation {
    pub const fn header(&self) -> BlockHeader {
        self.header
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub const fn summary(&self) -> Option<BlockSummary> {
        self.summary
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForkEvidence {
    pub old_tip: Tip,
    pub reported_tip: Tip,
    pub mismatch_height: u64,
    pub expected: BlockId,
    pub reported: Option<BlockId>,
    pub common_ancestor: Option<Tip>,
    pub history_exhausted: bool,
    pub probe_failed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationHead {
    pub sequence: u64,
    pub tip: Tip,
    pub commitment: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    config: ObserverConfig,
    records: Vec<RawObservation>,
    canonical: Vec<u32>,
    mode: ObserverMode,
    fork: Option<ForkEvidence>,
}
impl State {
    fn tip(&self) -> Tip {
        self.canonical.last().map_or(self.config.anchor, |index| {
            let h = self.records[*index as usize].header;
            Tip {
                height: h.height,
                id: h.id,
            }
        })
    }
    fn tip_at(&self, height: u64) -> Option<Tip> {
        if height == self.config.anchor.height {
            return Some(self.config.anchor);
        }
        self.canonical
            .iter()
            .map(|i| self.records[*i as usize].header)
            .find(|h| h.height == height)
            .map(|h| Tip {
                height: h.height,
                id: h.id,
            })
    }
    fn raw_bytes(&self) -> u64 {
        self.records.iter().map(|r| r.raw.len() as u64).sum()
    }
    fn verify(&self, full: bool) -> Result<()> {
        self.config.validate()?;
        if self.records.len() > self.config.max_blocks as usize
            || self.canonical.len() > self.records.len()
            || self.raw_bytes() > self.config.max_raw_bytes
        {
            return Err(DaemonError::InvalidSnapshot);
        }
        for (i, r) in self.records.iter().enumerate() {
            if self.records[..i]
                .iter()
                .any(|old| old.header.id == r.header.id)
            {
                return Err(DaemonError::InvalidSnapshot);
            }
            let parent = if r.header.parent == self.config.anchor.id {
                Some(self.config.anchor.height)
            } else {
                self.records[..i]
                    .iter()
                    .find(|old| old.header.id == r.header.parent)
                    .map(|old| old.header.height)
            };
            if parent.and_then(|h| h.checked_add(1)) != Some(r.header.height)
                || full && inspect(&self.config, r.header, &r.raw).ok() != Some(r.summary)
            {
                return Err(DaemonError::InvalidSnapshot);
            }
        }
        let mut previous = self.config.anchor;
        for index in &self.canonical {
            let r = self
                .records
                .get(*index as usize)
                .ok_or(DaemonError::InvalidSnapshot)?;
            if r.summary.is_none()
                || r.header.parent != previous.id
                || previous.height.checked_add(1) != Some(r.header.height)
            {
                return Err(DaemonError::InvalidSnapshot);
            }
            previous = Tip {
                height: r.header.height,
                id: r.header.id,
            };
        }
        if let Some(f) = self.fork {
            if f.old_tip != self.tip()
                || f.mismatch_height > f.old_tip.height
                || f.mismatch_height < self.config.anchor.height
                || self
                    .tip_at(f.mismatch_height)
                    .is_none_or(|t| t.id != f.expected)
                || f.common_ancestor.is_some_and(|t| {
                    self.tip_at(t.height) != Some(t)
                        || t.height > f.reported_tip.height
                        || t.height > f.mismatch_height
                })
                || (f.history_exhausted || f.probe_failed) && f.common_ancestor.is_some()
            {
                return Err(DaemonError::InvalidSnapshot);
            }
        }
        Ok(())
    }
}
/// Sole owner of the consensus-namespace observer journal. It contains no
/// Ledger, accepted economic head, or mutation capability for another owner.
pub struct ObservationStore {
    journal: Journal,
    state: State,
}
impl ObservationStore {
    pub fn initialize(root: &Path, config: ObserverConfig) -> Result<Self> {
        config.validate()?;
        let namespace = Namespace::for_mainnet(config.network, "consensus")?;
        let (mut journal, _) = Journal::open(root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(DaemonError::InvalidSnapshot);
        }
        let state = State {
            config,
            records: vec![],
            canonical: vec![],
            mode: ObserverMode::Observing,
            fork: None,
        };
        journal.commit(vec![Mutation::Put {
            key: KEY.into(),
            value: encode(&state)?,
        }])?;
        Ok(Self { journal, state })
    }
    pub fn load(
        root: &Path,
        config: ObserverConfig,
        expected: ObservationHead,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport)> {
        config.validate()?;
        let (journal, report) = Journal::open_existing(
            root,
            Namespace::for_mainnet(config.network, "consensus")?,
            recovery,
        )?;
        if journal.entries()?.count() != 1 {
            return Err(DaemonError::InvalidSnapshot);
        }
        let bytes = journal.get(KEY)?.ok_or(DaemonError::InvalidSnapshot)?;
        if journal.sequence()? != expected.sequence || commitment(bytes) != expected.commitment {
            return Err(DaemonError::StaleHead);
        }
        let state = decode(bytes, &config)?;
        if state.tip() != expected.tip {
            return Err(DaemonError::StaleHead);
        }
        Ok((Self { journal, state }, report))
    }
    fn check(&self) -> Result<()> {
        self.journal.sequence()?;
        Ok(())
    }
    pub fn head(&self) -> Result<ObservationHead> {
        let sequence = self.journal.sequence()?;
        let bytes = self.journal.get(KEY)?.ok_or(DaemonError::InvalidSnapshot)?;
        Ok(ObservationHead {
            sequence,
            tip: self.state.tip(),
            commitment: commitment(bytes),
        })
    }
    pub fn config(&self) -> Result<&ObserverConfig> {
        self.check()?;
        Ok(&self.state.config)
    }
    pub fn records(&self) -> Result<&[RawObservation]> {
        self.check()?;
        Ok(&self.state.records)
    }
    pub fn canonical_headers(&self) -> Result<Vec<BlockHeader>> {
        self.check()?;
        Ok(self
            .state
            .canonical
            .iter()
            .map(|i| self.state.records[*i as usize].header)
            .collect())
    }
    pub fn mode(&self) -> Result<ObserverMode> {
        self.check()?;
        Ok(self.state.mode)
    }
    pub fn fork_evidence(&self) -> Result<Option<ForkEvidence>> {
        self.check()?;
        Ok(self.state.fork)
    }
    pub fn path(&self) -> PathBuf {
        self.journal.path().to_owned()
    }
    fn expected(&self, expected: ObservationHead) -> Result<()> {
        if self.head()? != expected {
            return Err(DaemonError::StaleHead);
        }
        Ok(())
    }
    fn publish(&mut self, next: State) -> Result<ObservationHead> {
        let bytes = encode(&next)?;
        self.journal.commit(vec![Mutation::Put {
            key: KEY.into(),
            value: bytes,
        }])?;
        self.state = next;
        self.head()
    }
    fn mode_change(&mut self, mode: ObserverMode) -> Result<ObservationHead> {
        self.check()?;
        let mut next = self.state.clone();
        next.mode = mode;
        self.publish(next)
    }
    fn quarantine(
        &mut self,
        reason: QuarantineReason,
        fork: Option<ForkEvidence>,
    ) -> Result<ObservationHead> {
        self.check()?;
        let mut next = self.state.clone();
        next.mode = ObserverMode::Quarantined(reason);
        next.fork = fork;
        self.publish(next)
    }
    fn append(&mut self, record: RawObservation, canonical: bool) -> Result<ObservationHead> {
        self.check()?;
        let mut next = self.state.clone();
        let index = if let Some(i) = next
            .records
            .iter()
            .position(|r| r.header.id == record.header.id)
        {
            if next.records[i] != record {
                return Err(DaemonError::InvalidSnapshot);
            }
            i
        } else {
            if next.records.len() >= next.config.max_blocks as usize
                || next
                    .raw_bytes()
                    .checked_add(record.raw.len() as u64)
                    .is_none_or(|n| n > next.config.max_raw_bytes)
            {
                return Err(DaemonError::InvalidTransition);
            }
            let i = next.records.len();
            next.records.push(record);
            i
        };
        if canonical {
            next.canonical.push(index as u32);
        } else {
            next.mode = ObserverMode::Quarantined(QuarantineReason::UnsupportedDogecoinLayout);
        }
        next.verify(false)?;
        self.publish(next)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverPhase {
    Identity,
    Tip,
    Cursor,
    NextHash,
    Fetch,
    Scan,
    Recheck,
    ForkSearch,
    Waiting,
    Idle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaemonStatus {
    pub mode: ObserverMode,
    pub phase: ObserverPhase,
    pub head: ObservationHead,
    pub reported_tip: Option<Tip>,
    pub stored_blocks: u32,
    pub stored_raw_bytes: u64,
    pub identity_checked: bool,
    pub retry_attempt: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Progress,
    Waiting { until_ms: u64 },
    Recorded { head: ObservationHead },
    Quarantined { reason: QuarantineReason },
    Paused,
    Stopped,
}
enum Task {
    Identity,
    Tip,
    Cursor {
        tip: Tip,
    },
    NextHash {
        tip: Tip,
    },
    Fetch {
        tip: Tip,
        header: BlockHeader,
    },
    Scan {
        tip: Tip,
        header: BlockHeader,
        raw: Vec<u8>,
    },
    Recheck {
        tip: Tip,
        record: RawObservation,
    },
    Search {
        evidence: ForkEvidence,
        height: u64,
    },
    Wait {
        until: u64,
        identity: bool,
    },
    Idle,
}
pub struct Daemon<R> {
    source: R,
    store: ObservationStore,
    task: Task,
    reported_tip: Option<Tip>,
    identity_checked: bool,
    retry_attempt: u8,
    last_clock: Option<u64>,
}
impl<R: ObservationRpc> Daemon<R> {
    pub fn new(source: R, store: ObservationStore) -> Self {
        Self {
            source,
            store,
            task: Task::Identity,
            reported_tip: None,
            identity_checked: false,
            retry_attempt: 0,
            last_clock: None,
        }
    }
    pub fn head(&self) -> Result<ObservationHead> {
        self.store.head()
    }
    pub fn store(&self) -> &ObservationStore {
        &self.store
    }
    pub fn into_parts(self) -> (R, ObservationStore) {
        (self.source, self.store)
    }
    pub fn status(&self) -> Result<DaemonStatus> {
        self.store.check()?;
        let phase = match self.task {
            Task::Identity => ObserverPhase::Identity,
            Task::Tip => ObserverPhase::Tip,
            Task::Cursor { .. } => ObserverPhase::Cursor,
            Task::NextHash { .. } => ObserverPhase::NextHash,
            Task::Fetch { .. } => ObserverPhase::Fetch,
            Task::Scan { .. } => ObserverPhase::Scan,
            Task::Recheck { .. } => ObserverPhase::Recheck,
            Task::Search { .. } => ObserverPhase::ForkSearch,
            Task::Wait { .. } => ObserverPhase::Waiting,
            Task::Idle => ObserverPhase::Idle,
        };
        Ok(DaemonStatus {
            mode: self.store.state.mode,
            phase,
            head: self.store.head()?,
            reported_tip: self.reported_tip,
            stored_blocks: self.store.state.records.len() as u32,
            stored_raw_bytes: self.store.state.raw_bytes(),
            identity_checked: self.identity_checked,
            retry_attempt: self.retry_attempt,
        })
    }
    pub fn pause(&mut self, expected: ObservationHead) -> Result<ObservationHead> {
        self.store.expected(expected)?;
        if !matches!(self.store.mode()?, ObserverMode::Observing) {
            return Err(DaemonError::InvalidTransition);
        }
        let head = self.store.mode_change(ObserverMode::Paused)?;
        self.task = Task::Idle;
        Ok(head)
    }
    pub fn resume(&mut self, expected: ObservationHead) -> Result<ObservationHead> {
        self.store.expected(expected)?;
        if self.store.state.fork.is_some()
            || matches!(
                self.store.state.mode,
                ObserverMode::Observing
                    | ObserverMode::Quarantined(
                        QuarantineReason::ForkDetected
                            | QuarantineReason::CursorAboveTip
                            | QuarantineReason::UnsupportedDogecoinLayout
                            | QuarantineReason::SpoolFull
                    )
            )
        {
            return Err(DaemonError::InvalidTransition);
        }
        let head = self.store.mode_change(ObserverMode::Observing)?;
        self.task = Task::Identity;
        self.identity_checked = false;
        self.retry_attempt = 0;
        Ok(head)
    }
    /// Rewinds only the observed canonical selection. Every raw branch remains
    /// retained. The supplied comparison token and retained target must match.
    pub fn reconcile(&mut self, expected: ObservationHead, target: Tip) -> Result<ObservationHead> {
        self.store.expected(expected)?;
        if !(matches!(
            self.store.state.mode,
            ObserverMode::Quarantined(
                QuarantineReason::ForkDetected | QuarantineReason::CursorAboveTip
            )
        ) || self.store.state.mode == ObserverMode::Stopped && self.store.state.fork.is_some())
            || self.store.state.tip_at(target.height) != Some(target)
            || target.height > self.store.state.tip().height
        {
            return Err(DaemonError::InvalidTransition);
        }
        let mut next = self.store.state.clone();
        next.canonical
            .retain(|i| next.records[*i as usize].header.height <= target.height);
        next.mode = ObserverMode::Observing;
        next.fork = None;
        let head = self.store.publish(next)?;
        self.task = Task::Identity;
        self.identity_checked = false;
        self.retry_attempt = 0;
        Ok(head)
    }
    pub fn stop(&mut self, expected: ObservationHead) -> Result<ObservationHead> {
        self.store.expected(expected)?;
        let head = self.store.mode_change(ObserverMode::Stopped)?;
        self.task = Task::Idle;
        Ok(head)
    }
    fn quarantine(&mut self, reason: QuarantineReason) -> Result<StepOutcome> {
        self.store.quarantine(reason, None)?;
        self.task = Task::Idle;
        Ok(StepOutcome::Quarantined { reason })
    }
    fn rpc_failure(
        &mut self,
        error: ObservationRpcError,
        now: u64,
        control: &BootstrapControl,
    ) -> Result<StepOutcome> {
        if let Some(outcome) = self.stop_if_cancelled(control)? {
            return Ok(outcome);
        }
        match error {
            ObservationRpcError::WrongIdentity => {
                self.identity_checked = false;
                self.quarantine(QuarantineReason::WrongIdentity)
            }
            ObservationRpcError::Unavailable => {
                self.quarantine(QuarantineReason::HistoricalBlockUnavailable)
            }
            ObservationRpcError::InvalidResponse => {
                self.quarantine(QuarantineReason::InvalidRpcResponse)
            }
            ObservationRpcError::Transient => {
                self.identity_checked = false;
                if self.retry_attempt >= self.store.state.config.max_retries {
                    return self.quarantine(QuarantineReason::RetryExhausted);
                }
                let factor = 1u64 << self.retry_attempt;
                self.retry_attempt += 1;
                let delay = self
                    .store
                    .state
                    .config
                    .retry_base_ms
                    .saturating_mul(factor)
                    .min(self.store.state.config.retry_max_ms);
                let until = now.checked_add(delay).ok_or(DaemonError::ClockOverflow)?;
                self.task = Task::Wait {
                    until,
                    identity: true,
                };
                Ok(StepOutcome::Waiting { until_ms: until })
            }
        }
    }
    fn stop_if_cancelled(&mut self, control: &BootstrapControl) -> Result<Option<StepOutcome>> {
        if control.check().is_ok() {
            return Ok(None);
        }
        if self.store.state.mode != ObserverMode::Stopped {
            let head = self.head()?;
            self.stop(head)?;
        }
        Ok(Some(StepOutcome::Stopped))
    }
    fn start_fork(
        &mut self,
        tip: Tip,
        reported: Option<BlockId>,
        reason: QuarantineReason,
    ) -> Result<StepOutcome> {
        let old = self.store.state.tip();
        let evidence = ForkEvidence {
            old_tip: old,
            reported_tip: tip,
            mismatch_height: old.height,
            expected: old.id,
            reported,
            common_ancestor: None,
            history_exhausted: false,
            probe_failed: false,
        };
        self.store.quarantine(reason, Some(evidence))?;
        let height = old.height.min(tip.height);
        if height < self.store.state.config.anchor.height {
            let mut evidence = evidence;
            evidence.history_exhausted = true;
            self.store.quarantine(reason, Some(evidence))?;
            self.task = Task::Idle;
        } else {
            self.task = Task::Search { evidence, height };
        }
        Ok(StepOutcome::Quarantined { reason })
    }
    /// One controlled identity verification (at most three RPC calls), one
    /// subsequent RPC, or one bounded scan. No sleeps or economic writes occur.
    pub fn step(&mut self, now_ms: u64, control: &BootstrapControl) -> Result<StepOutcome> {
        self.store.check()?;
        if self.last_clock.is_some_and(|last| now_ms < last) {
            return Err(DaemonError::InvalidTransition);
        }
        self.last_clock = Some(now_ms);
        if let Some(outcome) = self.stop_if_cancelled(control)? {
            return Ok(outcome);
        }
        match self.store.state.mode {
            ObserverMode::Paused => return Ok(StepOutcome::Paused),
            ObserverMode::Stopped => return Ok(StepOutcome::Stopped),
            ObserverMode::Quarantined(reason) if !matches!(self.task, Task::Search { .. }) => {
                return Ok(StepOutcome::Quarantined { reason })
            }
            _ => {}
        }
        let task = std::mem::replace(&mut self.task, Task::Idle);
        match task {
            Task::Identity => match self.source.verify_identity() {
                Ok(identity) => {
                    if let Some(outcome) = self.stop_if_cancelled(control)? {
                        return Ok(outcome);
                    }
                    let expected = self.store.state.config.network;
                    if identity.network != expected
                        || identity.test
                        || identity.genesis != expected.genesis()
                    {
                        return self.quarantine(QuarantineReason::WrongIdentity);
                    }
                    self.identity_checked = true;
                    self.task = Task::Tip;
                    Ok(StepOutcome::Progress)
                }
                Err(e) => self.rpc_failure(e, now_ms, control),
            },
            Task::Tip => match self.source.reported_tip() {
                Ok(tip) => {
                    if let Some(outcome) = self.stop_if_cancelled(control)? {
                        return Ok(outcome);
                    }
                    self.reported_tip = Some(tip);
                    if tip.height < self.store.state.tip().height {
                        return self.start_fork(tip, None, QuarantineReason::CursorAboveTip);
                    }
                    self.task = Task::Cursor { tip };
                    Ok(StepOutcome::Progress)
                }
                Err(e) => self.rpc_failure(e, now_ms, control),
            },
            Task::Cursor { tip } => {
                let cursor = self.store.state.tip();
                match self.source.block_hash(cursor.height) {
                    Ok(hash) => {
                        if let Some(outcome) = self.stop_if_cancelled(control)? {
                            return Ok(outcome);
                        }
                        if hash != cursor.id {
                            return self.start_fork(
                                tip,
                                Some(hash),
                                QuarantineReason::ForkDetected,
                            );
                        }
                        if tip.height == cursor.height {
                            if tip.id != hash {
                                return self.start_fork(
                                    tip,
                                    Some(hash),
                                    QuarantineReason::ForkDetected,
                                );
                            }
                            self.retry_attempt = 0;
                            let until = now_ms
                                .checked_add(self.store.state.config.poll_interval_ms)
                                .ok_or(DaemonError::ClockOverflow)?;
                            self.task = Task::Wait {
                                until,
                                identity: false,
                            };
                            Ok(StepOutcome::Waiting { until_ms: until })
                        } else {
                            self.task = Task::NextHash { tip };
                            Ok(StepOutcome::Progress)
                        }
                    }
                    Err(e) => self.rpc_failure(e, now_ms, control),
                }
            }
            Task::NextHash { tip } => {
                if self.store.state.records.len() >= self.store.state.config.max_blocks as usize
                    || self.store.state.raw_bytes() >= self.store.state.config.max_raw_bytes
                {
                    return self.quarantine(QuarantineReason::SpoolFull);
                }
                let cursor = self.store.state.tip();
                let height = cursor
                    .height
                    .checked_add(1)
                    .ok_or(DaemonError::InvalidSnapshot)?;
                match self.source.block_hash(height) {
                    Ok(hash) => {
                        if let Some(outcome) = self.stop_if_cancelled(control)? {
                            return Ok(outcome);
                        }
                        self.task = Task::Fetch {
                            tip,
                            header: BlockHeader {
                                height,
                                id: hash,
                                parent: cursor.id,
                            },
                        };
                        Ok(StepOutcome::Progress)
                    }
                    Err(e) => self.rpc_failure(e, now_ms, control),
                }
            }
            Task::Fetch { tip, header } => match self.source.raw_block(header.id) {
                Ok(raw) => {
                    if let Some(outcome) = self.stop_if_cancelled(control)? {
                        return Ok(outcome);
                    }
                    if raw.len() > MAX_UNVERIFIED_RAW_BLOCK_BYTES {
                        return self.quarantine(QuarantineReason::InvalidRawBlock);
                    }
                    if self
                        .store
                        .state
                        .raw_bytes()
                        .checked_add(raw.len() as u64)
                        .is_none_or(|n| n > self.store.state.config.max_raw_bytes)
                    {
                        return self.quarantine(QuarantineReason::SpoolFull);
                    }
                    self.task = Task::Scan { tip, header, raw };
                    Ok(StepOutcome::Progress)
                }
                Err(e) => self.rpc_failure(e, now_ms, control),
            },
            Task::Scan { tip, header, raw } => {
                match inspect(&self.store.state.config, header, &raw) {
                    Ok(summary) => {
                        self.task = Task::Recheck {
                            tip,
                            record: RawObservation {
                                header,
                                raw,
                                summary,
                            },
                        };
                        Ok(StepOutcome::Progress)
                    }
                    Err(reason) => self.quarantine(reason),
                }
            }
            Task::Recheck { tip, record } => match self.source.block_hash(record.header.height) {
                Ok(hash) => {
                    if let Some(outcome) = self.stop_if_cancelled(control)? {
                        return Ok(outcome);
                    }
                    if hash != record.header.id {
                        return self.quarantine(QuarantineReason::ForkDetected);
                    }
                    let opaque = record.summary.is_none();
                    let head = self.store.append(record, !opaque)?;
                    self.retry_attempt = 0;
                    if opaque {
                        self.task = Task::Idle;
                        Ok(StepOutcome::Quarantined {
                            reason: QuarantineReason::UnsupportedDogecoinLayout,
                        })
                    } else {
                        self.task = Task::Tip;
                        self.reported_tip = Some(tip);
                        Ok(StepOutcome::Recorded { head })
                    }
                }
                Err(e) => self.rpc_failure(e, now_ms, control),
            },
            Task::Search {
                mut evidence,
                height,
            } => {
                let Some(expected) = self.store.state.tip_at(height) else {
                    evidence.history_exhausted = true;
                    let reason = match self.store.state.mode {
                        ObserverMode::Quarantined(r) => r,
                        _ => QuarantineReason::ForkDetected,
                    };
                    self.store.quarantine(reason, Some(evidence))?;
                    return Ok(StepOutcome::Quarantined { reason });
                };
                match self.source.block_hash(height) {
                    Ok(hash) => {
                        if let Some(outcome) = self.stop_if_cancelled(control)? {
                            return Ok(outcome);
                        }
                        if hash == expected.id {
                            evidence.common_ancestor = Some(expected);
                        } else if height == self.store.state.config.anchor.height {
                            evidence.history_exhausted = true;
                        } else {
                            self.task = Task::Search {
                                evidence,
                                height: height - 1,
                            };
                            return Ok(StepOutcome::Progress);
                        }
                        let reason = match self.store.state.mode {
                            ObserverMode::Quarantined(r) => r,
                            _ => QuarantineReason::ForkDetected,
                        };
                        self.store.quarantine(reason, Some(evidence))?;
                        Ok(StepOutcome::Quarantined { reason })
                    }
                    Err(_) => {
                        if let Some(outcome) = self.stop_if_cancelled(control)? {
                            return Ok(outcome);
                        }
                        evidence.probe_failed = true;
                        let reason = match self.store.state.mode {
                            ObserverMode::Quarantined(r) => r,
                            _ => QuarantineReason::ForkDetected,
                        };
                        self.store.quarantine(reason, Some(evidence))?;
                        Ok(StepOutcome::Quarantined { reason })
                    }
                }
            }
            Task::Wait { until, identity } => {
                if now_ms < until {
                    self.task = Task::Wait { until, identity };
                    Ok(StepOutcome::Waiting { until_ms: until })
                } else {
                    self.task = if identity { Task::Identity } else { Task::Tip };
                    Ok(StepOutcome::Progress)
                }
            }
            Task::Idle => Ok(StepOutcome::Progress),
        }
    }
}
fn inspect(
    config: &ObserverConfig,
    header: BlockHeader,
    raw: &[u8],
) -> core::result::Result<Option<BlockSummary>, QuarantineReason> {
    if raw.len() < 80 || raw.len() > MAX_UNVERIFIED_RAW_BLOCK_BYTES {
        return Err(QuarantineReason::InvalidRawBlock);
    }
    let mut hash = crate::hash256::double_sha256(&raw[..80]);
    hash.reverse();
    if BlockId(hash) != header.id {
        return Err(QuarantineReason::HashMismatch);
    }
    let mut parent: [u8; 32] = raw[4..36].try_into().expect("header length checked");
    parent.reverse();
    if BlockId(parent) != header.parent {
        return Err(QuarantineReason::ParentMismatch);
    }
    if config.network == TradeLayerMainnet::Dogecoin {
        return Ok(None);
    }
    let mut expected = header.id.0;
    expected.reverse();
    let view = UnverifiedRawBlockView::parse(raw, Some(expected))
        .map_err(|_| QuarantineReason::InvalidRawBlock)?;
    let report = view
        .inspect_parallel(config.scan)
        .map_err(|_| QuarantineReason::InvalidRawBlock)?;
    Ok(Some(BlockSummary {
        transaction_count: report.ordered_preindex().transaction_count() as u32,
        candidate_count: report.ordered_preindex().candidate_positions().len() as u32,
    }))
}
fn commitment(bytes: &[u8]) -> [u8; 32] {
    crate::hash256::double_sha256_parts(&[b"TradeLayer/Observer/v1/", bytes])
}
fn reason_code(r: QuarantineReason) -> u8 {
    match r {
        QuarantineReason::WrongIdentity => 0,
        QuarantineReason::InvalidRpcResponse => 1,
        QuarantineReason::HistoricalBlockUnavailable => 2,
        QuarantineReason::UnsupportedDogecoinLayout => 3,
        QuarantineReason::InvalidRawBlock => 4,
        QuarantineReason::HashMismatch => 5,
        QuarantineReason::ParentMismatch => 6,
        QuarantineReason::ForkDetected => 7,
        QuarantineReason::CursorAboveTip => 8,
        QuarantineReason::SpoolFull => 9,
        QuarantineReason::RetryExhausted => 10,
    }
}
fn read_reason(r: u8) -> Result<QuarantineReason> {
    Ok(match r {
        0 => QuarantineReason::WrongIdentity,
        1 => QuarantineReason::InvalidRpcResponse,
        2 => QuarantineReason::HistoricalBlockUnavailable,
        3 => QuarantineReason::UnsupportedDogecoinLayout,
        4 => QuarantineReason::InvalidRawBlock,
        5 => QuarantineReason::HashMismatch,
        6 => QuarantineReason::ParentMismatch,
        7 => QuarantineReason::ForkDetected,
        8 => QuarantineReason::CursorAboveTip,
        9 => QuarantineReason::SpoolFull,
        10 => QuarantineReason::RetryExhausted,
        _ => return Err(DaemonError::InvalidSnapshot),
    })
}
struct Writer(Vec<u8>);
impl Writer {
    fn bytes(&mut self, v: &[u8]) -> Result<()> {
        if self
            .0
            .len()
            .checked_add(v.len())
            .is_none_or(|n| n > MAX_STATE_BYTES - 4)
        {
            return Err(DaemonError::InvalidSnapshot);
        }
        self.0.extend_from_slice(v);
        Ok(())
    }
    fn u8(&mut self, v: u8) -> Result<()> {
        self.bytes(&[v])
    }
    fn u32(&mut self, v: u32) -> Result<()> {
        self.bytes(&v.to_be_bytes())
    }
    fn u64(&mut self, v: u64) -> Result<()> {
        self.bytes(&v.to_be_bytes())
    }
    fn tip(&mut self, t: Tip) -> Result<()> {
        self.u64(t.height)?;
        self.bytes(&t.id.0)
    }
    fn optional_tip(&mut self, t: Option<Tip>) -> Result<()> {
        self.u8(u8::from(t.is_some()))?;
        if let Some(t) = t {
            self.tip(t)?;
        }
        Ok(())
    }
    fn config(&mut self, c: &ObserverConfig) -> Result<()> {
        self.u8(c.network.tag())?;
        self.tip(c.anchor)?;
        for n in [c.poll_interval_ms, c.retry_base_ms, c.retry_max_ms] {
            self.u64(n)?;
        }
        self.u8(c.max_retries)?;
        self.u32(c.max_blocks)?;
        self.u64(c.max_raw_bytes)?;
        self.u32(c.scan.workers as u32)?;
        self.u32(c.scan.window_transactions as u32)
    }
}
fn encode(state: &State) -> Result<Vec<u8>> {
    state.verify(false)?;
    let mut w = Writer(vec![]);
    w.bytes(MAGIC)?;
    w.config(&state.config)?;
    match state.mode {
        ObserverMode::Observing => w.u8(0)?,
        ObserverMode::Paused => w.u8(1)?,
        ObserverMode::Stopped => w.u8(2)?,
        ObserverMode::Quarantined(r) => {
            w.u8(3)?;
            w.u8(reason_code(r))?;
        }
    }
    w.u8(u8::from(state.fork.is_some()))?;
    if let Some(f) = state.fork {
        w.tip(f.old_tip)?;
        w.tip(f.reported_tip)?;
        w.u64(f.mismatch_height)?;
        w.bytes(&f.expected.0)?;
        w.u8(u8::from(f.reported.is_some()))?;
        if let Some(h) = f.reported {
            w.bytes(&h.0)?;
        }
        w.optional_tip(f.common_ancestor)?;
        w.u8(u8::from(f.history_exhausted))?;
        w.u8(u8::from(f.probe_failed))?;
    }
    w.u32(state.records.len() as u32)?;
    for r in &state.records {
        w.tip(Tip {
            height: r.header.height,
            id: r.header.id,
        })?;
        w.bytes(&r.header.parent.0)?;
        w.u8(u8::from(r.summary.is_some()))?;
        if let Some(s) = r.summary {
            w.u32(s.transaction_count)?;
            w.u32(s.candidate_count)?;
        }
        w.u32(r.raw.len() as u32)?;
        w.bytes(&r.raw)?;
    }
    w.u32(state.canonical.len() as u32)?;
    for i in &state.canonical {
        w.u32(*i)?;
    }
    with_crc(w.0).map_err(|_| DaemonError::InvalidSnapshot)
}
struct Reader<'a>(Cursor<'a>);
impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8> {
        self.0.u8().map_err(|_| DaemonError::InvalidSnapshot)
    }
    fn u32(&mut self) -> Result<u32> {
        self.0.u32().map_err(|_| DaemonError::InvalidSnapshot)
    }
    fn u64(&mut self) -> Result<u64> {
        self.0.u64().map_err(|_| DaemonError::InvalidSnapshot)
    }
    fn hash(&mut self) -> Result<BlockId> {
        Ok(BlockId(
            self.0.fixed().map_err(|_| DaemonError::InvalidSnapshot)?,
        ))
    }
    fn flag(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DaemonError::InvalidSnapshot),
        }
    }
    fn tip(&mut self) -> Result<Tip> {
        Ok(Tip {
            height: self.u64()?,
            id: self.hash()?,
        })
    }
    fn optional_tip(&mut self) -> Result<Option<Tip>> {
        if self.flag()? {
            Ok(Some(self.tip()?))
        } else {
            Ok(None)
        }
    }
    fn config(&mut self) -> Result<ObserverConfig> {
        Ok(ObserverConfig {
            network: TradeLayerMainnet::from_tag(self.u8()?)
                .map_err(|_| DaemonError::InvalidSnapshot)?,
            anchor: self.tip()?,
            poll_interval_ms: self.u64()?,
            retry_base_ms: self.u64()?,
            retry_max_ms: self.u64()?,
            max_retries: self.u8()?,
            max_blocks: self.u32()?,
            max_raw_bytes: self.u64()?,
            scan: ParallelIndexConfig {
                workers: self.u32()? as usize,
                window_transactions: self.u32()? as usize,
            },
        })
    }
}
fn decode(bytes: &[u8], expected: &ObserverConfig) -> Result<State> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(DaemonError::InvalidSnapshot);
    }
    let body = verify_crc(bytes).map_err(|_| DaemonError::InvalidSnapshot)?;
    let mut r = Reader(Cursor::new(body));
    if r.0
        .take(MAGIC.len())
        .map_err(|_| DaemonError::InvalidSnapshot)?
        != MAGIC
    {
        return Err(DaemonError::InvalidSnapshot);
    }
    let config = r.config()?;
    if &config != expected {
        return Err(DaemonError::InvalidSnapshot);
    }
    config.validate()?;
    let mode = match r.u8()? {
        0 => ObserverMode::Observing,
        1 => ObserverMode::Paused,
        2 => ObserverMode::Stopped,
        3 => ObserverMode::Quarantined(read_reason(r.u8()?)?),
        _ => return Err(DaemonError::InvalidSnapshot),
    };
    let fork = if r.flag()? {
        Some(ForkEvidence {
            old_tip: r.tip()?,
            reported_tip: r.tip()?,
            mismatch_height: r.u64()?,
            expected: r.hash()?,
            reported: if r.flag()? { Some(r.hash()?) } else { None },
            common_ancestor: r.optional_tip()?,
            history_exhausted: r.flag()?,
            probe_failed: r.flag()?,
        })
    } else {
        None
    };
    let count = r.u32()?;
    if count > config.max_blocks {
        return Err(DaemonError::InvalidSnapshot);
    }
    let mut records = Vec::with_capacity(count as usize);
    let mut raw_bytes = 0u64;
    for _ in 0..count {
        let t = r.tip()?;
        let header = BlockHeader {
            height: t.height,
            id: t.id,
            parent: r.hash()?,
        };
        let summary = if r.flag()? {
            Some(BlockSummary {
                transaction_count: r.u32()?,
                candidate_count: r.u32()?,
            })
        } else {
            None
        };
        let length = r.u32()? as usize;
        raw_bytes = raw_bytes
            .checked_add(length as u64)
            .ok_or(DaemonError::InvalidSnapshot)?;
        if length > MAX_UNVERIFIED_RAW_BLOCK_BYTES || raw_bytes > config.max_raw_bytes {
            return Err(DaemonError::InvalidSnapshot);
        }
        let raw =
            r.0.take(length)
                .map_err(|_| DaemonError::InvalidSnapshot)?
                .to_vec();
        records.push(RawObservation {
            header,
            summary,
            raw,
        });
    }
    let count = r.u32()?;
    if count > config.max_blocks {
        return Err(DaemonError::InvalidSnapshot);
    }
    let mut canonical = Vec::with_capacity(count as usize);
    for _ in 0..count {
        canonical.push(r.u32()?);
    }
    r.0.end().map_err(|_| DaemonError::InvalidSnapshot)?;
    let state = State {
        config,
        records,
        canonical,
        mode,
        fork,
    };
    state.verify(true)?;
    if encode(&state)? != bytes {
        return Err(DaemonError::InvalidSnapshot);
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "tl-observer-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn config() -> ObserverConfig {
        let n = TradeLayerMainnet::Bitcoin;
        let mut c = ObserverConfig::new(
            n,
            Tip {
                height: 0,
                id: n.genesis(),
            },
        )
        .unwrap();
        c.scan.workers = 1;
        c
    }
    fn record(c: &ObserverConfig) -> RawObservation {
        let mut raw = crate::read_only::decode_hex(
            include_str!("../fixtures/raw_block_ingest_synthetic.hex").trim(),
        )
        .unwrap();
        let mut parent = c.anchor.id.0;
        parent.reverse();
        raw[4..36].copy_from_slice(&parent);
        let mut id = crate::hash256::double_sha256(&raw[..80]);
        id.reverse();
        let header = BlockHeader {
            height: 1,
            id: BlockId(id),
            parent: c.anchor.id,
        };
        let summary = inspect(c, header, &raw).unwrap();
        RawObservation {
            header,
            raw,
            summary,
        }
    }
    #[test]
    fn canonical_restore_recomputes_raw_hash_merkle_and_projection_not_just_crc() {
        let c = config();
        let mut s = State {
            config: c.clone(),
            records: vec![record(&c)],
            canonical: vec![0],
            mode: ObserverMode::Observing,
            fork: None,
        };
        let good = encode(&s).unwrap();
        assert_eq!(decode(&good, &c).unwrap(), s);
        s.records[0].summary.as_mut().unwrap().candidate_count += 1;
        let forged = encode(&s).unwrap();
        assert!(decode(&forged, &c).is_err());
        s.records[0].summary.as_mut().unwrap().candidate_count -= 1;
        let last = s.records[0].raw.len() - 1;
        s.records[0].raw[last] ^= 1;
        let forged = encode(&s).unwrap();
        assert!(decode(&forged, &c).is_err());
        for len in [0, 5, 50, good.len() - 1] {
            assert!(decode(&good[..len], &c).is_err());
        }
        let mut changed = good;
        changed.push(0);
        assert!(decode(&changed, &c).is_err());
    }
    #[test]
    fn decoder_checks_counts_and_lengths_before_allocating_nested_raw() {
        let c = config();
        let s = State {
            config: c.clone(),
            records: vec![record(&c)],
            canonical: vec![0],
            mode: ObserverMode::Observing,
            fork: None,
        };
        let bytes = encode(&s).unwrap();
        for offset in [93usize, 178usize] {
            let mut bad = bytes[..bytes.len() - 4].to_vec();
            bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            let bad = with_crc(bad).unwrap();
            assert!(decode(&bad, &c).is_err());
        }
    }
    #[test]
    fn lost_ack_poison_denies_cached_reads_and_reopen_requires_the_new_head() {
        let dir = Directory::new();
        let c = config();
        let mut store = ObservationStore::initialize(&dir.0, c.clone()).unwrap();
        let old = store.head().unwrap();
        let mut next = store.state.clone();
        next.mode = ObserverMode::Paused;
        let expected = ObservationHead {
            sequence: old.sequence + 1,
            tip: old.tip,
            commitment: commitment(&encode(&next).unwrap()),
        };
        store.journal.test_fail_after_sync();
        assert!(store.mode_change(ObserverMode::Paused).is_err());
        assert!(store.head().is_err());
        assert!(store.records().is_err());
        assert!(store.config().is_err());
        assert!(store.mode().is_err());
        assert!(store.canonical_headers().is_err());
        assert!(store.fork_evidence().is_err());
        drop(store);
        assert!(matches!(
            ObservationStore::load(&dir.0, c.clone(), old, Recovery::RejectIncompleteTail),
            Err(DaemonError::StaleHead)
        ));
        let (restored, _) =
            ObservationStore::load(&dir.0, c, expected, Recovery::RejectIncompleteTail).unwrap();
        assert_eq!(restored.mode().unwrap(), ObserverMode::Paused);
    }
    #[test]
    fn partial_write_requires_explicit_recovery_and_restores_only_old_cursor() {
        let dir = Directory::new();
        let c = config();
        let mut store = ObservationStore::initialize(&dir.0, c.clone()).unwrap();
        let old = store.head().unwrap();
        store.journal.test_fail_after_partial_write();
        assert!(store.mode_change(ObserverMode::Paused).is_err());
        assert!(store.head().is_err());
        drop(store);
        assert!(
            ObservationStore::load(&dir.0, c.clone(), old, Recovery::RejectIncompleteTail).is_err()
        );
        let (restored, report) =
            ObservationStore::load(&dir.0, c, old, Recovery::DiscardIncompleteTail).unwrap();
        assert!(report.discarded_bytes > 0);
        assert_eq!(restored.head().unwrap(), old);
        assert_eq!(restored.mode().unwrap(), ObserverMode::Observing);
    }
}
