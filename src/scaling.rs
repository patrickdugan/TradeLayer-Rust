//! Bounded channel-settlement metadata and explicit, unfunded custody plans.
//! No signature, UTXO, actor or settlement-root authentication is implied.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::decimal::Decimal;
use crate::ledger::{AccountId, ChannelSide, Ledger, PropertyId, TxId};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "ea2103b70dd051d39e44c9201a1eed2107b8110ac9803a0b825b92942799ca61";
pub const MAX_CHANNELS: usize = 128;
pub const MAX_ACTIONS: usize = 4096;
pub const MAX_RECORDS: usize = 16_384;
pub const MAX_CHECKPOINT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SAFE_HEIGHT: u64 = 9_007_199_254_740_991;
const MAGIC: &[u8; 5] = b"TLSL\x01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalingError {
    Capacity,
    InvalidHeight,
    InvalidTime,
    NonFinite,
    InvalidInput,
    UnknownChannel,
    UnknownTrade,
    WrongChannel,
    Replay,
    Expired,
    InvalidChain,
    InsufficientCollateral,
    NonAtomicAmount,
    InvalidRoot,
    EmptyRange,
    InvalidSnapshot,
    Validation(String),
}
impl core::fmt::Display for ScalingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ScalingError {}
type Result<T> = core::result::Result<T, ScalingError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleType {
    KeepAlive,
    ClosePosition,
    NetSettle,
    KingSettle,
}
impl SettleType {
    pub const fn code(self) -> u8 {
        match self {
            Self::KeepAlive => 0,
            Self::ClosePosition => 1,
            Self::NetSettle => 2,
            Self::KingSettle => 3,
        }
    }
    fn read(code: u8) -> Result<Self> {
        match code {
            0 => Ok(Self::KeepAlive),
            1 => Ok(Self::ClosePosition),
            2 => Ok(Self::NetSettle),
            3 => Ok(Self::KingSettle),
            _ => Err(ScalingError::InvalidInput),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleStatus {
    Pending,
    Live,
    Neutralized,
    Expired,
    Swept,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    AtoB,
    BtoA,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScalingConfig {
    pub activation_23: Option<u64>,
    pub activation_31: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDescriptor {
    pub address: AccountId,
    pub party_a: AccountId,
    pub party_b: AccountId,
    pub root: TxId,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TradeInput {
    pub txid: TxId,
    pub price: f64,
    pub contracts: f64,
    pub expiry_block: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TradeRecord {
    pub input: TradeInput,
    pub block: u64,
    pub status: SettleStatus,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SettleRequest {
    pub settle_type: u8,
    pub reference_1: Option<TxId>,
    pub reference_2: Option<TxId>,
    pub mark_price: Option<f64>,
    pub net_amount: Option<f64>,
    pub column_a_is_seller: Option<bool>,
    pub expiry_block: Option<u64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct KingRequest {
    pub block_start: u64,
    pub block_end: u64,
    pub property_id: u32,
    pub net_amount: Option<f64>,
    pub a_pays_b: Option<bool>,
    pub channel_root: Option<TxId>,
    pub total_contracts: f64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    pub valid: bool,
    pub reason: String,
    pub neutralized_count: Option<u32>,
}
impl Validation {
    fn good() -> Self {
        Self {
            valid: true,
            reason: String::new(),
            neutralized_count: None,
        }
    }
    fn reject(&mut self, reason: &str) {
        self.valid = false;
        self.reason.push_str(reason);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationContext {
    pub active_23: bool,
    pub active_31: bool,
    pub channel_exists: bool,
    pub commit_a_present: bool,
    pub commit_b_present: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Pnl {
    pub amount: f64,
    pub direction: Direction,
}
#[derive(Debug, Clone, PartialEq)]
pub struct OffsetPlan {
    pub trade_txid: TxId,
    pub keep_alive_txid: TxId,
    pub settle_txid: TxId,
    pub entry_price: f64,
    pub mark_price: f64,
    pub contracts: f64,
    pub pnl: Pnl,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAliveRecord {
    pub trade_txid: TxId,
    pub settle_txid: TxId,
    pub block: u64,
    pub status: Option<SettleStatus>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CloseRecord {
    pub offset: OffsetPlan,
    pub block: u64,
    pub status: Option<SettleStatus>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NetRecord {
    pub txid: TxId,
    pub reference_1: Option<TxId>,
    pub reference_2: Option<TxId>,
    pub amount: f64,
    pub column_a_is_seller: bool,
    pub block: u64,
    pub status: SettleStatus,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementRecord {
    pub txid: TxId,
    pub settle_type: SettleType,
    pub block: u64,
    pub status: SettleStatus,
    pub timestamp_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustodyWitness {
    pub owner_a: AccountId,
    pub owner_b: AccountId,
    pub atoms_a: u128,
    pub atoms_b: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KingTransferPlan {
    pub channel: AccountId,
    pub property: PropertyId,
    pub direction: Direction,
    pub amount_atoms: u128,
    pub before_a: u128,
    pub before_b: u128,
    pub after_a: u128,
    pub after_b: u128,
}
#[derive(Debug, Clone, PartialEq)]
pub struct KingRecord {
    pub txid: TxId,
    pub request: KingRequest,
    pub block: u64,
    pub timestamp_ms: u64,
    pub neutralized_count: u32,
    pub plan: KingTransferPlan,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelRecord {
    pub descriptor: ChannelDescriptor,
    pub trades: Vec<TradeRecord>,
    pub keep_alives: Vec<KeepAliveRecord>,
    pub closes: Vec<CloseRecord>,
    pub net_settles: Vec<NetRecord>,
    pub settlements: Vec<SettlementRecord>,
    pub kings: Vec<KingRecord>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    Settlement,
    Trade,
    Net,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingReference {
    pub txid: TxId,
    pub kind: PendingKind,
    pub block: u64,
    pub status: SettleStatus,
}
#[derive(Debug, Clone, PartialEq)]
enum Action {
    Register {
        channel: ChannelDescriptor,
        block: u64,
    },
    Trade {
        channel: AccountId,
        input: TradeInput,
        block: u64,
    },
    Settle {
        channel: AccountId,
        request: SettleRequest,
        block: u64,
        txid: TxId,
        millis: u64,
    },
    King {
        channel: AccountId,
        request: KingRequest,
        block: u64,
        txid: TxId,
        millis: u64,
        custody: CustodyWitness,
    },
    Advance {
        block: u64,
    },
    Neutralize {
        channel: AccountId,
        txid: TxId,
        block: u64,
    },
    Sweep {
        channel: AccountId,
        start: u64,
        end: u64,
        block: u64,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct ScalingState {
    config: ScalingConfig,
    channels: Vec<ChannelRecord>,
    actions: Vec<Action>,
    last_height: u64,
}

pub fn calculate_pnl(
    entry_price: f64,
    contracts: f64,
    mark_price: f64,
    column_a_is_seller: bool,
) -> Result<Pnl> {
    finite(entry_price)?;
    finite(contracts)?;
    finite(mark_price)?;
    let difference = mark_price - entry_price;
    let amount = (difference * contracts).abs();
    finite(difference)?;
    finite(amount)?;
    Ok(Pnl {
        amount,
        direction: if if column_a_is_seller {
            difference < 0.0
        } else {
            difference > 0.0
        } {
            Direction::BtoA
        } else {
            Direction::AtoB
        },
    })
}
impl ScalingState {
    pub fn new(config: ScalingConfig) -> Result<Self> {
        for h in [config.activation_23, config.activation_31]
            .into_iter()
            .flatten()
        {
            height(h)?;
        }
        Ok(Self {
            config,
            channels: vec![],
            actions: vec![],
            last_height: 0,
        })
    }
    pub fn config(&self) -> &ScalingConfig {
        &self.config
    }
    pub fn last_height(&self) -> u64 {
        self.last_height
    }
    pub fn channels(&self) -> &[ChannelRecord] {
        &self.channels
    }
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }
    pub fn channel(&self, address: &AccountId) -> Option<&ChannelRecord> {
        self.channels
            .iter()
            .find(|c| &c.descriptor.address == address)
    }
    fn channel_mut(&mut self, address: &AccountId) -> Result<&mut ChannelRecord> {
        self.channels
            .iter_mut()
            .find(|c| &c.descriptor.address == address)
            .ok_or(ScalingError::UnknownChannel)
    }
    pub fn register_channel_unverified(
        &mut self,
        channel: ChannelDescriptor,
        block: u64,
    ) -> Result<()> {
        self.apply(Action::Register { channel, block })
    }
    pub fn register_channel_from_ledger_unverified(
        &mut self,
        ledger: &Ledger,
        address: AccountId,
        root: TxId,
        block: u64,
    ) -> Result<()> {
        let party_a = ledger
            .channel_owner(&address, ChannelSide::A)
            .ok_or(ScalingError::UnknownChannel)?
            .clone();
        let party_b = ledger
            .channel_owner(&address, ChannelSide::B)
            .ok_or(ScalingError::UnknownChannel)?
            .clone();
        self.register_channel_unverified(
            ChannelDescriptor {
                address,
                party_a,
                party_b,
                root,
            },
            block,
        )
    }
    pub fn register_trade_unverified(
        &mut self,
        channel: &AccountId,
        input: TradeInput,
        block: u64,
    ) -> Result<()> {
        self.apply(Action::Trade {
            channel: channel.clone(),
            input,
            block,
        })
    }
    pub fn advance_height(&mut self, block: u64) -> Result<()> {
        self.apply(Action::Advance { block })
    }
    pub fn get_trade_by_txid(&self, txid: TxId) -> Option<&TradeRecord> {
        self.channels
            .iter()
            .flat_map(|c| &c.trades)
            .find(|t| t.input.txid == txid)
    }
    pub fn get_trade_status(&self, txid: TxId) -> Option<SettleStatus> {
        self.get_trade_by_txid(txid).map(|t| t.status)
    }
    pub fn validate_keep_alive_chain(&self, trade: TxId, keep_alive: TxId) -> bool {
        self.channels
            .iter()
            .flat_map(|c| &c.keep_alives)
            .any(|k| k.trade_txid == trade && k.settle_txid == keep_alive)
    }
    pub fn is_settlement_neutralized(&self, channel: &AccountId, txid: TxId) -> bool {
        self.channel(channel)
            .and_then(|c| c.settlements.iter().find(|s| s.txid == txid))
            .is_some_and(|s| s.status == SettleStatus::Neutralized)
    }
    pub fn get_pending_settlements_in_range(
        &self,
        channel: &AccountId,
        start: u64,
        end: u64,
    ) -> Vec<PendingReference> {
        let Some(c) = self.channel(channel) else {
            return vec![];
        };
        let pending = |status: SettleStatus| {
            !matches!(status, SettleStatus::Neutralized | SettleStatus::Swept)
        };
        let mut rows = vec![];
        for s in &c.settlements {
            if (start..=end).contains(&s.block) && pending(s.status) {
                rows.push(PendingReference {
                    txid: s.txid,
                    kind: PendingKind::Settlement,
                    block: s.block,
                    status: s.status,
                });
            }
        }
        for t in &c.trades {
            if (start..=end).contains(&t.block) && pending(t.status) {
                rows.push(PendingReference {
                    txid: t.input.txid,
                    kind: PendingKind::Trade,
                    block: t.block,
                    status: t.status,
                });
            }
        }
        for n in &c.net_settles {
            if (start..=end).contains(&n.block) && pending(n.status) {
                rows.push(PendingReference {
                    txid: n.txid,
                    kind: PendingKind::Net,
                    block: n.block,
                    status: n.status,
                });
            }
        }
        rows
    }
    fn context(&self, channel: &AccountId, block: u64) -> ValidationContext {
        ValidationContext {
            active_23: self.config.activation_23.is_some_and(|h| h < block),
            active_31: self.config.activation_31.is_some_and(|h| h < block),
            channel_exists: self.channel(channel).is_some(),
            commit_a_present: self.channel(channel).is_some(),
            commit_b_present: self.channel(channel).is_some(),
        }
    }
    /// Source validation for the finite typed field domain; context is an
    /// explicit claim. In particular unpublished trade keep-alive can validate.
    pub fn validate_settle_channel_pnl(
        &self,
        channel: &AccountId,
        p: &SettleRequest,
        block: u64,
        context: ValidationContext,
    ) -> Validation {
        let mut v = Validation::good();
        if !context.active_23 {
            v.reject("Tx type not yet activated; ");
            return v;
        }
        if p.settle_type > 2 {
            v.reject(&format!("Invalid settleType: {}; ", p.settle_type));
            return v;
        }
        if !context.channel_exists {
            v.reject("Channel not found for sender; ");
            return v;
        }
        if !context.commit_a_present && !context.commit_b_present {
            v.reject("Sender is not a channel participant; ");
            return v;
        }
        let Some(reference) = p.reference_1 else {
            v.reject("Missing txidNeutralized1 (trade reference); ");
            return v;
        };
        if self.is_settlement_neutralized(channel, reference) {
            v.reject("Settlement already superseded by later settlement; ");
            return v;
        }
        match p.settle_type {
            0 => {
                if self.get_trade_status(reference) == Some(SettleStatus::Expired) {
                    v.reject("Cannot keep-alive an expired trade; ");
                }
            }
            1 => {
                if p.mark_price.is_none_or(|m| m <= 0.0 || m.is_nan()) {
                    v.reject("Invalid mark price for close; ");
                }
                if p.reference_2.is_none() {
                    v.reject("Missing txidNeutralized2 (keep-alive reference); ");
                }
                if !p
                    .reference_2
                    .is_some_and(|ka| self.validate_keep_alive_chain(reference, ka))
                {
                    v.reject("Keep-alive chain invalid; ");
                }
            }
            2 => {
                if p.net_amount.is_none() {
                    v.reject("Missing netAmount for NET_SETTLE; ");
                }
                if p.column_a_is_seller.is_none() {
                    v.reject("Missing columnAIsSeller direction flag; ");
                }
            }
            _ => unreachable!(),
        }
        if p.expiry_block.is_some_and(|e| e != 0 && block > e) {
            v.reject(&format!(
                "Settlement expired at block {}, current: {block}; ",
                p.expiry_block.unwrap()
            ));
        }
        v
    }
    pub fn validate_king_settle(
        &self,
        channel: &AccountId,
        p: &KingRequest,
        block: u64,
        context: ValidationContext,
    ) -> Validation {
        let mut v = Validation::good();
        if !context.active_31 {
            v.reject("Tx type not yet activated; ");
            return v;
        }
        if p.block_start == 0 || p.block_end == 0 {
            v.reject("Missing block range (blockStart/blockEnd); ");
            return v;
        }
        if p.block_start > p.block_end {
            v.reject("Invalid block range: start > end; ");
            return v;
        }
        if block < p.block_start {
            v.reject("Cannot settle future block range; ");
            return v;
        }
        if !context.channel_exists {
            v.reject("Channel not found; ");
            return v;
        }
        if p.property_id == 0 {
            v.reject("Invalid propertyId; ");
            return v;
        }
        if p.net_amount.is_none() {
            v.reject("Missing netAmount; ");
            return v;
        }
        if p.a_pays_b.is_none() {
            v.reject("Missing payment direction (aPaysBDirection); ");
            return v;
        }
        if p.channel_root.is_none() {
            v.reject("Missing channelRoot (founding UTXO reference); ");
            return v;
        }
        v.neutralized_count = Some(
            self.get_pending_settlements_in_range(channel, p.block_start, p.block_end)
                .len() as u32,
        );
        v
    }
    pub fn settle_channel_pnl_unverified(
        &mut self,
        channel: &AccountId,
        request: SettleRequest,
        block: u64,
        txid: TxId,
        timestamp_ms: u64,
    ) -> Result<Option<OffsetPlan>> {
        self.apply(Action::Settle {
            channel: channel.clone(),
            request,
            block,
            txid,
            millis: timestamp_ms,
        })?;
        Ok(self
            .channel(channel)
            .and_then(|c| c.closes.iter().find(|c| c.offset.settle_txid == txid))
            .map(|c| c.offset.clone()))
    }
    pub fn process_king_settle_unverified(
        &mut self,
        ledger: &Ledger,
        channel: &AccountId,
        request: KingRequest,
        block: u64,
        txid: TxId,
        timestamp_ms: u64,
    ) -> Result<KingTransferPlan> {
        let property =
            PropertyId::try_from(request.property_id).map_err(|_| ScalingError::InvalidInput)?;
        if ledger.property(property).is_none() {
            return Err(ScalingError::InvalidInput);
        }
        let custody = CustodyWitness {
            owner_a: ledger
                .channel_owner(channel, ChannelSide::A)
                .ok_or(ScalingError::UnknownChannel)?
                .clone(),
            owner_b: ledger
                .channel_owner(channel, ChannelSide::B)
                .ok_or(ScalingError::UnknownChannel)?
                .clone(),
            atoms_a: ledger.channel_atoms(property, channel, ChannelSide::A),
            atoms_b: ledger.channel_atoms(property, channel, ChannelSide::B),
        };
        self.apply(Action::King {
            channel: channel.clone(),
            request,
            block,
            txid,
            millis: timestamp_ms,
            custody,
        })?;
        Ok(self
            .channel(channel)
            .expect("applied king channel")
            .kings
            .last()
            .expect("applied king record")
            .plan
            .clone())
    }
    pub fn neutralize_settlement_unverified(
        &mut self,
        channel: &AccountId,
        txid: TxId,
        block: u64,
    ) -> Result<()> {
        self.apply(Action::Neutralize {
            channel: channel.clone(),
            txid,
            block,
        })
    }
    pub fn neutralize_settlements_in_range_unverified(
        &mut self,
        channel: &AccountId,
        start: u64,
        end: u64,
        block: u64,
    ) -> Result<()> {
        self.apply(Action::Sweep {
            channel: channel.clone(),
            start,
            end,
            block,
        })
    }
    /// The source helper only logs; it deletes or archives no state.
    pub fn clear_l2_state(&self, channel: &AccountId, start: u64, end: u64) -> Result<()> {
        if self.channel(channel).is_none() {
            return Err(ScalingError::UnknownChannel);
        }
        range(start, end)?;
        Ok(())
    }
    fn seen(&self, id: TxId) -> bool {
        self.channels.iter().any(|c| {
            c.trades.iter().any(|t| t.input.txid == id)
                || c.settlements.iter().any(|s| s.txid == id)
                || c.kings.iter().any(|k| k.txid == id)
        })
    }
    fn expire_pending(&mut self, block: u64) {
        for c in &mut self.channels {
            for t in &mut c.trades {
                if t.status == SettleStatus::Pending && block > t.input.expiry_block {
                    t.status = SettleStatus::Expired;
                }
            }
        }
    }
    fn record_count(&self) -> usize {
        self.actions.len()
            + self
                .channels
                .iter()
                .map(|c| {
                    1 + c.trades.len()
                        + c.keep_alives.len()
                        + c.closes.len()
                        + c.net_settles.len()
                        + c.settlements.len()
                        + c.kings.len()
                })
                .sum::<usize>()
    }
    fn apply(&mut self, action: Action) -> Result<()> {
        validate_action(&action)?;
        if self.actions.len() >= MAX_ACTIONS || self.record_count() + 5 > MAX_RECORDS {
            return Err(ScalingError::Capacity);
        }
        let block = action_block(&action);
        if block < self.last_height {
            return Err(ScalingError::InvalidHeight);
        }
        let mut next = self.clone();
        next.expire_pending(block);
        next.execute(&action)?;
        next.last_height = block;
        next.actions.push(action);
        next.encode_checkpoint()?;
        *self = next;
        Ok(())
    }
    fn execute(&mut self, action: &Action) -> Result<()> {
        match action {
            Action::Register { channel, .. } => {
                if channel.party_a == channel.party_b || self.channel(&channel.address).is_some() {
                    return Err(ScalingError::InvalidInput);
                }
                if self.channels.len() >= MAX_CHANNELS {
                    return Err(ScalingError::Capacity);
                }
                self.channels.push(ChannelRecord {
                    descriptor: channel.clone(),
                    trades: vec![],
                    keep_alives: vec![],
                    closes: vec![],
                    net_settles: vec![],
                    settlements: vec![],
                    kings: vec![],
                });
            }
            Action::Trade {
                channel,
                input,
                block,
            } => {
                if self.seen(input.txid) {
                    return Err(ScalingError::Replay);
                }
                if input.price <= 0.0 || input.contracts <= 0.0 || input.expiry_block < *block {
                    return Err(ScalingError::InvalidInput);
                }
                self.channel_mut(channel)?.trades.push(TradeRecord {
                    input: input.clone(),
                    block: *block,
                    status: SettleStatus::Pending,
                });
            }
            Action::Settle {
                channel,
                request,
                block,
                txid,
                millis,
            } => {
                if self.seen(*txid) {
                    return Err(ScalingError::Replay);
                }
                let v = self.validate_settle_channel_pnl(
                    channel,
                    request,
                    *block,
                    self.context(channel, *block),
                );
                if !v.valid {
                    return Err(ScalingError::Validation(v.reason));
                }
                let reference = request.reference_1.ok_or(ScalingError::InvalidInput)?;
                match SettleType::read(request.settle_type)? {
                    SettleType::KeepAlive => {
                        self.process_keep_alive(channel, reference, *block, *txid)?
                    }
                    SettleType::ClosePosition => {
                        self.process_close_position(channel, request, *block, *txid)?
                    }
                    SettleType::NetSettle => {
                        self.process_net_settle(channel, request, *block, *txid)?
                    }
                    SettleType::KingSettle => return Err(ScalingError::InvalidInput),
                }
                self.record_settlement(
                    channel,
                    *txid,
                    SettleType::read(request.settle_type)?,
                    *block,
                    *millis,
                )?;
            }
            Action::King {
                channel,
                request,
                block,
                txid,
                millis,
                custody,
            } => {
                if self.seen(*txid) {
                    return Err(ScalingError::Replay);
                }
                let v = self.validate_king_settle(
                    channel,
                    request,
                    *block,
                    self.context(channel, *block),
                );
                if !v.valid {
                    return Err(ScalingError::Validation(v.reason));
                }
                if request.block_end > *block {
                    return Err(ScalingError::InvalidHeight);
                }
                let count = v.neutralized_count.unwrap_or(0);
                if count == 0 {
                    return Err(ScalingError::EmptyRange);
                }
                let descriptor = &self
                    .channel(channel)
                    .ok_or(ScalingError::UnknownChannel)?
                    .descriptor;
                if request.channel_root != Some(descriptor.root) {
                    return Err(ScalingError::InvalidRoot);
                }
                let plan = king_plan(descriptor, request, custody)?;
                self.sweep_inner(channel, request.block_start, request.block_end)?;
                self.record_king_settlement(
                    channel,
                    KingRecord {
                        txid: *txid,
                        request: request.clone(),
                        block: *block,
                        timestamp_ms: *millis,
                        neutralized_count: count,
                        plan,
                    },
                )?;
                self.clear_l2_state(channel, request.block_start, request.block_end)?;
            }
            Action::Advance { .. } => {}
            Action::Neutralize { channel, txid, .. } => {
                self.neutralize_inner(channel, *txid)?;
            }
            Action::Sweep {
                channel,
                start,
                end,
                ..
            } => {
                self.sweep_inner(channel, *start, *end)?;
            }
        }
        Ok(())
    }
    fn process_keep_alive(
        &mut self,
        channel: &AccountId,
        trade: TxId,
        block: u64,
        settle: TxId,
    ) -> Result<()> {
        let c = self.channel_mut(channel)?;
        let t = c
            .trades
            .iter_mut()
            .find(|t| t.input.txid == trade)
            .ok_or(ScalingError::UnknownTrade)?;
        if matches!(
            t.status,
            SettleStatus::Expired | SettleStatus::Neutralized | SettleStatus::Swept
        ) {
            return Err(ScalingError::Expired);
        }
        t.status = SettleStatus::Live;
        c.keep_alives.push(KeepAliveRecord {
            trade_txid: trade,
            settle_txid: settle,
            block,
            status: None,
        });
        Ok(())
    }
    fn process_close_position(
        &mut self,
        channel: &AccountId,
        p: &SettleRequest,
        block: u64,
        settle: TxId,
    ) -> Result<()> {
        let trade = p.reference_1.ok_or(ScalingError::InvalidInput)?;
        let keep = p.reference_2.ok_or(ScalingError::InvalidChain)?;
        let mark = p.mark_price.ok_or(ScalingError::InvalidInput)?;
        let seller = p.column_a_is_seller.unwrap_or(false);
        let c = self.channel_mut(channel)?;
        let t = c
            .trades
            .iter()
            .find(|t| t.input.txid == trade)
            .ok_or(ScalingError::UnknownTrade)?;
        if t.status != SettleStatus::Live
            || !c
                .keep_alives
                .iter()
                .any(|k| k.trade_txid == trade && k.settle_txid == keep && k.status.is_none())
            || !c
                .settlements
                .iter()
                .any(|s| s.txid == keep && s.status == SettleStatus::Live)
        {
            return Err(ScalingError::InvalidChain);
        }
        let offset = OffsetPlan {
            trade_txid: trade,
            keep_alive_txid: keep,
            settle_txid: settle,
            entry_price: t.input.price,
            mark_price: mark,
            contracts: t.input.contracts,
            pnl: calculate_pnl(t.input.price, t.input.contracts, mark, seller)?,
        };
        c.trades
            .iter_mut()
            .find(|t| t.input.txid == trade)
            .expect("found trade")
            .status = SettleStatus::Neutralized;
        for k in &mut c.keep_alives {
            if k.settle_txid == keep {
                k.status = Some(SettleStatus::Neutralized);
            }
        }
        self.neutralize_inner(channel, trade)?;
        self.neutralize_inner(channel, keep)?;
        self.channel_mut(channel)?.closes.push(CloseRecord {
            offset,
            block,
            status: None,
        });
        Ok(())
    }
    fn process_net_settle(
        &mut self,
        channel: &AccountId,
        p: &SettleRequest,
        block: u64,
        txid: TxId,
    ) -> Result<()> {
        for id in [p.reference_1, p.reference_2].into_iter().flatten() {
            let c = self.channel(channel).ok_or(ScalingError::UnknownChannel)?;
            let active = c
                .settlements
                .iter()
                .any(|s| s.txid == id && s.status == SettleStatus::Live)
                || c.trades.iter().any(|t| {
                    t.input.txid == id
                        && matches!(t.status, SettleStatus::Pending | SettleStatus::Live)
                });
            if !active {
                return Err(ScalingError::InvalidChain);
            }
            self.neutralize_inner(channel, id)?;
            for t in &mut self.channel_mut(channel)?.trades {
                if t.input.txid == id {
                    t.status = SettleStatus::Neutralized;
                }
            }
        }
        self.channel_mut(channel)?.net_settles.push(NetRecord {
            txid,
            reference_1: p.reference_1,
            reference_2: p.reference_2,
            amount: p.net_amount.ok_or(ScalingError::InvalidInput)?,
            column_a_is_seller: p.column_a_is_seller.ok_or(ScalingError::InvalidInput)?,
            block,
            status: SettleStatus::Pending,
        });
        Ok(())
    }
    fn record_settlement(
        &mut self,
        channel: &AccountId,
        txid: TxId,
        settle_type: SettleType,
        block: u64,
        timestamp_ms: u64,
    ) -> Result<()> {
        self.channel_mut(channel)?
            .settlements
            .push(SettlementRecord {
                txid,
                settle_type,
                block,
                status: SettleStatus::Live,
                timestamp_ms,
            });
        Ok(())
    }
    fn record_king_settlement(&mut self, channel: &AccountId, record: KingRecord) -> Result<()> {
        self.channel_mut(channel)?.kings.push(record);
        Ok(())
    }
    fn neutralize_inner(&mut self, channel: &AccountId, txid: TxId) -> Result<()> {
        for s in &mut self.channel_mut(channel)?.settlements {
            if s.txid == txid {
                s.status = SettleStatus::Neutralized;
            }
        }
        Ok(())
    }
    fn sweep_inner(&mut self, channel: &AccountId, start: u64, end: u64) -> Result<()> {
        range(start, end)?;
        let c = self.channel_mut(channel)?;
        for s in &mut c.settlements {
            if (start..=end).contains(&s.block) {
                s.status = SettleStatus::Swept;
            }
        }
        for t in &mut c.trades {
            if (start..=end).contains(&t.block) {
                t.status = SettleStatus::Swept;
            }
        }
        for k in &mut c.keep_alives {
            if (start..=end).contains(&k.block) {
                k.status = Some(SettleStatus::Swept);
            }
        }
        for close in &mut c.closes {
            if (start..=end).contains(&close.block) {
                close.status = Some(SettleStatus::Swept);
            }
        }
        for n in &mut c.net_settles {
            if (start..=end).contains(&n.block) {
                n.status = SettleStatus::Swept;
            }
        }
        Ok(())
    }
}
fn finite(n: f64) -> Result<()> {
    if n.is_finite() {
        Ok(())
    } else {
        Err(ScalingError::NonFinite)
    }
}
fn height(h: u64) -> Result<()> {
    if h <= MAX_SAFE_HEIGHT {
        Ok(())
    } else {
        Err(ScalingError::InvalidHeight)
    }
}
fn range(start: u64, end: u64) -> Result<()> {
    height(start)?;
    height(end)?;
    if start == 0 || start > end {
        Err(ScalingError::InvalidHeight)
    } else {
        Ok(())
    }
}
fn action_block(a: &Action) -> u64 {
    match a {
        Action::Register { block, .. }
        | Action::Trade { block, .. }
        | Action::Settle { block, .. }
        | Action::King { block, .. }
        | Action::Advance { block }
        | Action::Neutralize { block, .. }
        | Action::Sweep { block, .. } => *block,
    }
}
fn validate_action(a: &Action) -> Result<()> {
    height(action_block(a))?;
    match a {
        Action::Trade { input, .. } => {
            finite(input.price)?;
            finite(input.contracts)?;
            height(input.expiry_block)?;
        }
        Action::Settle {
            request, millis, ..
        } => {
            for n in [request.mark_price, request.net_amount]
                .into_iter()
                .flatten()
            {
                finite(n)?;
            }
            if let Some(h) = request.expiry_block {
                height(h)?;
            }
            if *millis > MAX_SAFE_HEIGHT {
                return Err(ScalingError::InvalidTime);
            }
        }
        Action::King {
            request, millis, ..
        } => {
            finite(request.total_contracts)?;
            if let Some(n) = request.net_amount {
                finite(n)?;
            }
            height(request.block_start)?;
            height(request.block_end)?;
            if *millis > MAX_SAFE_HEIGHT {
                return Err(ScalingError::InvalidTime);
            }
        }
        Action::Sweep { start, end, .. } => range(*start, *end)?,
        _ => {}
    }
    Ok(())
}
fn amount_atoms(value: f64) -> Result<u128> {
    finite(value)?;
    let text = crate::state_oracle::source_number_text(value.abs())
        .map_err(|_| ScalingError::NonFinite)?;
    let value: Decimal = text.parse().map_err(|_| ScalingError::NonAtomicAmount)?;
    let scaled = value
        .checked_mul(&Decimal::from(100_000_000u32))
        .map_err(|_| ScalingError::NonAtomicAmount)?;
    if scaled.decimal_places() != 0 {
        return Err(ScalingError::NonAtomicAmount);
    }
    scaled
        .to_string()
        .parse()
        .map_err(|_| ScalingError::NonAtomicAmount)
}
fn king_plan(
    c: &ChannelDescriptor,
    p: &KingRequest,
    w: &CustodyWitness,
) -> Result<KingTransferPlan> {
    if w.owner_a != c.party_a || w.owner_b != c.party_b {
        return Err(ScalingError::WrongChannel);
    }
    let property = PropertyId::try_from(p.property_id).map_err(|_| ScalingError::InvalidInput)?;
    let amount = amount_atoms(p.net_amount.ok_or(ScalingError::InvalidInput)?)?;
    let direction = if p.a_pays_b.ok_or(ScalingError::InvalidInput)? {
        Direction::AtoB
    } else {
        Direction::BtoA
    };
    let (a, b) = match direction {
        Direction::AtoB => (w.atoms_a.checked_sub(amount), w.atoms_b.checked_add(amount)),
        Direction::BtoA => (w.atoms_a.checked_add(amount), w.atoms_b.checked_sub(amount)),
    };
    let (after_a, after_b) = (
        a.ok_or(ScalingError::InsufficientCollateral)?,
        b.ok_or(ScalingError::InsufficientCollateral)?,
    );
    Ok(KingTransferPlan {
        channel: c.address.clone(),
        property,
        direction,
        amount_atoms: amount,
        before_a: w.atoms_a,
        before_b: w.atoms_b,
        after_a,
        after_b,
    })
}

impl ScalingState {
    /// Replays every admitted action and checks the complete derived projection.
    /// Custody witnesses are historical claims; restore does not authenticate them.
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        let mut replay = Self::new(self.config.clone())?;
        for action in &self.actions {
            replay.apply(action.clone())?;
        }
        if replay.projection_bytes()? != self.projection_bytes()? {
            return Err(ScalingError::InvalidSnapshot);
        }
        self.encode_checkpoint()
    }
    pub fn from_checkpoint_bytes(bytes: &[u8], expected_config: &ScalingConfig) -> Result<Self> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(ScalingError::Capacity);
        }
        let payload = verify_crc(bytes).map_err(|_| ScalingError::InvalidSnapshot)?;
        let mut reader = Reader(Cursor::new(payload));
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(ScalingError::InvalidSnapshot);
        }
        let config = ScalingConfig {
            activation_23: reader.option_height()?,
            activation_31: reader.option_height()?,
        };
        if &config != expected_config {
            return Err(ScalingError::InvalidSnapshot);
        }
        let mut state = Self::new(config)?;
        let count = reader.count(MAX_ACTIONS)?;
        for _ in 0..count {
            let action = reader.action()?;
            state.apply(action)?;
        }
        let length = reader.count(MAX_CHECKPOINT_BYTES)?;
        let projection = reader.take(length)?;
        if projection != state.projection_bytes()? {
            return Err(ScalingError::InvalidSnapshot);
        }
        reader.0.end().map_err(|_| ScalingError::InvalidSnapshot)?;
        if state.encode_checkpoint()? != bytes {
            return Err(ScalingError::InvalidSnapshot);
        }
        Ok(state)
    }
    fn encode_checkpoint(&self) -> Result<Vec<u8>> {
        if self.actions.len() > MAX_ACTIONS
            || self.channels.len() > MAX_CHANNELS
            || self.record_count() > MAX_RECORDS
        {
            return Err(ScalingError::Capacity);
        }
        let mut w = Writer(vec![]);
        w.bytes(MAGIC)?;
        w.option_height(self.config.activation_23)?;
        w.option_height(self.config.activation_31)?;
        w.count(self.actions.len())?;
        for action in &self.actions {
            w.action(action)?;
        }
        let projection = self.projection_bytes()?;
        w.count(projection.len())?;
        w.bytes(&projection)?;
        with_crc(w.0).map_err(|_| ScalingError::Capacity)
    }
    fn projection_bytes(&self) -> Result<Vec<u8>> {
        let mut w = Writer(vec![]);
        w.u64(self.last_height)?;
        w.count(self.channels.len())?;
        for c in &self.channels {
            w.channel(&c.descriptor)?;
            w.count(c.trades.len())?;
            for t in &c.trades {
                w.trade(&t.input)?;
                w.u64(t.block)?;
                w.status(t.status)?;
            }
            w.count(c.keep_alives.len())?;
            for k in &c.keep_alives {
                w.id(k.trade_txid)?;
                w.id(k.settle_txid)?;
                w.u64(k.block)?;
                w.option_status(k.status)?;
            }
            w.count(c.closes.len())?;
            for c in &c.closes {
                w.offset(&c.offset)?;
                w.u64(c.block)?;
                w.option_status(c.status)?;
            }
            w.count(c.net_settles.len())?;
            for n in &c.net_settles {
                w.id(n.txid)?;
                w.option_id(n.reference_1)?;
                w.option_id(n.reference_2)?;
                w.number(n.amount)?;
                w.u8(u8::from(n.column_a_is_seller))?;
                w.u64(n.block)?;
                w.status(n.status)?;
            }
            w.count(c.settlements.len())?;
            for s in &c.settlements {
                w.id(s.txid)?;
                w.u8(s.settle_type.code())?;
                w.u64(s.block)?;
                w.status(s.status)?;
                w.u64(s.timestamp_ms)?;
            }
            w.count(c.kings.len())?;
            for k in &c.kings {
                w.id(k.txid)?;
                w.king(&k.request)?;
                w.u64(k.block)?;
                w.u64(k.timestamp_ms)?;
                w.u32(k.neutralized_count)?;
                w.plan(&k.plan)?;
            }
        }
        Ok(w.0)
    }
}
struct Writer(Vec<u8>);
impl Writer {
    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_CHECKPOINT_BYTES - 4)
        {
            return Err(ScalingError::Capacity);
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn u8(&mut self, n: u8) -> Result<()> {
        self.bytes(&[n])
    }
    fn u32(&mut self, n: u32) -> Result<()> {
        self.bytes(&n.to_be_bytes())
    }
    fn u64(&mut self, n: u64) -> Result<()> {
        self.bytes(&n.to_be_bytes())
    }
    fn u128(&mut self, n: u128) -> Result<()> {
        self.bytes(&n.to_be_bytes())
    }
    fn count(&mut self, n: usize) -> Result<()> {
        self.u32(u32::try_from(n).map_err(|_| ScalingError::Capacity)?)
    }
    fn id(&mut self, id: TxId) -> Result<()> {
        self.bytes(&id.as_bytes())
    }
    fn account(&mut self, a: &AccountId) -> Result<()> {
        self.u8(a.as_str().len() as u8)?;
        self.bytes(a.as_str().as_bytes())
    }
    fn number(&mut self, n: f64) -> Result<()> {
        finite(n)?;
        self.u64(n.to_bits())
    }
    fn option_height(&mut self, n: Option<u64>) -> Result<()> {
        self.u8(u8::from(n.is_some()))?;
        if let Some(n) = n {
            self.u64(n)?;
        }
        Ok(())
    }
    fn option_id(&mut self, n: Option<TxId>) -> Result<()> {
        self.u8(u8::from(n.is_some()))?;
        if let Some(n) = n {
            self.id(n)?;
        }
        Ok(())
    }
    fn option_number(&mut self, n: Option<f64>) -> Result<()> {
        self.u8(u8::from(n.is_some()))?;
        if let Some(n) = n {
            self.number(n)?;
        }
        Ok(())
    }
    fn option_bool(&mut self, n: Option<bool>) -> Result<()> {
        self.u8(match n {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        })
    }
    fn status(&mut self, s: SettleStatus) -> Result<()> {
        self.u8(match s {
            SettleStatus::Pending => 0,
            SettleStatus::Live => 1,
            SettleStatus::Neutralized => 2,
            SettleStatus::Expired => 3,
            SettleStatus::Swept => 4,
        })
    }
    fn option_status(&mut self, s: Option<SettleStatus>) -> Result<()> {
        self.u8(u8::from(s.is_some()))?;
        if let Some(s) = s {
            self.status(s)?;
        }
        Ok(())
    }
    fn channel(&mut self, c: &ChannelDescriptor) -> Result<()> {
        self.account(&c.address)?;
        self.account(&c.party_a)?;
        self.account(&c.party_b)?;
        self.id(c.root)
    }
    fn trade(&mut self, t: &TradeInput) -> Result<()> {
        self.id(t.txid)?;
        self.number(t.price)?;
        self.number(t.contracts)?;
        self.u64(t.expiry_block)
    }
    fn settle(&mut self, p: &SettleRequest) -> Result<()> {
        self.u8(p.settle_type)?;
        self.option_id(p.reference_1)?;
        self.option_id(p.reference_2)?;
        self.option_number(p.mark_price)?;
        self.option_number(p.net_amount)?;
        self.option_bool(p.column_a_is_seller)?;
        self.option_height(p.expiry_block)
    }
    fn king(&mut self, p: &KingRequest) -> Result<()> {
        self.u64(p.block_start)?;
        self.u64(p.block_end)?;
        self.u32(p.property_id)?;
        self.option_number(p.net_amount)?;
        self.option_bool(p.a_pays_b)?;
        self.option_id(p.channel_root)?;
        self.number(p.total_contracts)
    }
    fn custody(&mut self, c: &CustodyWitness) -> Result<()> {
        self.account(&c.owner_a)?;
        self.account(&c.owner_b)?;
        self.u128(c.atoms_a)?;
        self.u128(c.atoms_b)
    }
    fn offset(&mut self, p: &OffsetPlan) -> Result<()> {
        self.id(p.trade_txid)?;
        self.id(p.keep_alive_txid)?;
        self.id(p.settle_txid)?;
        self.number(p.entry_price)?;
        self.number(p.mark_price)?;
        self.number(p.contracts)?;
        self.number(p.pnl.amount)?;
        self.u8(u8::from(p.pnl.direction == Direction::BtoA))
    }
    fn plan(&mut self, p: &KingTransferPlan) -> Result<()> {
        self.account(&p.channel)?;
        self.u32(p.property.get())?;
        self.u8(u8::from(p.direction == Direction::BtoA))?;
        for n in [p.amount_atoms, p.before_a, p.before_b, p.after_a, p.after_b] {
            self.u128(n)?;
        }
        Ok(())
    }
    fn action(&mut self, a: &Action) -> Result<()> {
        match a {
            Action::Register { channel, block } => {
                self.u8(0)?;
                self.channel(channel)?;
                self.u64(*block)
            }
            Action::Trade {
                channel,
                input,
                block,
            } => {
                self.u8(1)?;
                self.account(channel)?;
                self.trade(input)?;
                self.u64(*block)
            }
            Action::Settle {
                channel,
                request,
                block,
                txid,
                millis,
            } => {
                self.u8(2)?;
                self.account(channel)?;
                self.settle(request)?;
                self.u64(*block)?;
                self.id(*txid)?;
                self.u64(*millis)
            }
            Action::King {
                channel,
                request,
                block,
                txid,
                millis,
                custody,
            } => {
                self.u8(3)?;
                self.account(channel)?;
                self.king(request)?;
                self.u64(*block)?;
                self.id(*txid)?;
                self.u64(*millis)?;
                self.custody(custody)
            }
            Action::Advance { block } => {
                self.u8(4)?;
                self.u64(*block)
            }
            Action::Neutralize {
                channel,
                txid,
                block,
            } => {
                self.u8(5)?;
                self.account(channel)?;
                self.id(*txid)?;
                self.u64(*block)
            }
            Action::Sweep {
                channel,
                start,
                end,
                block,
            } => {
                self.u8(6)?;
                self.account(channel)?;
                self.u64(*start)?;
                self.u64(*end)?;
                self.u64(*block)
            }
        }
    }
}
struct Reader<'a>(Cursor<'a>);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        self.0.take(n).map_err(|_| ScalingError::InvalidSnapshot)
    }
    fn u8(&mut self) -> Result<u8> {
        self.0.u8().map_err(|_| ScalingError::InvalidSnapshot)
    }
    fn u32(&mut self) -> Result<u32> {
        self.0.u32().map_err(|_| ScalingError::InvalidSnapshot)
    }
    fn u64(&mut self) -> Result<u64> {
        self.0.u64().map_err(|_| ScalingError::InvalidSnapshot)
    }
    fn u128(&mut self) -> Result<u128> {
        self.0.u128().map_err(|_| ScalingError::InvalidSnapshot)
    }
    fn flag(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ScalingError::InvalidSnapshot),
        }
    }
    fn count(&mut self, max: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n > max {
            Err(ScalingError::Capacity)
        } else {
            Ok(n)
        }
    }
    fn id(&mut self) -> Result<TxId> {
        Ok(TxId::from_bytes(
            self.0.fixed().map_err(|_| ScalingError::InvalidSnapshot)?,
        ))
    }
    fn account(&mut self) -> Result<AccountId> {
        let n = self.u8()? as usize;
        if n > crate::ledger::MAX_ACCOUNT_BYTES {
            return Err(ScalingError::InvalidSnapshot);
        }
        let text =
            core::str::from_utf8(self.take(n)?).map_err(|_| ScalingError::InvalidSnapshot)?;
        AccountId::try_from(text).map_err(|_| ScalingError::InvalidSnapshot)
    }
    fn number(&mut self) -> Result<f64> {
        let n = f64::from_bits(self.u64()?);
        finite(n)?;
        Ok(n)
    }
    fn option_height(&mut self) -> Result<Option<u64>> {
        if self.flag()? {
            Ok(Some(self.u64()?))
        } else {
            Ok(None)
        }
    }
    fn option_id(&mut self) -> Result<Option<TxId>> {
        if self.flag()? {
            Ok(Some(self.id()?))
        } else {
            Ok(None)
        }
    }
    fn option_number(&mut self) -> Result<Option<f64>> {
        if self.flag()? {
            Ok(Some(self.number()?))
        } else {
            Ok(None)
        }
    }
    fn option_bool(&mut self) -> Result<Option<bool>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(false)),
            2 => Ok(Some(true)),
            _ => Err(ScalingError::InvalidSnapshot),
        }
    }
    fn channel(&mut self) -> Result<ChannelDescriptor> {
        Ok(ChannelDescriptor {
            address: self.account()?,
            party_a: self.account()?,
            party_b: self.account()?,
            root: self.id()?,
        })
    }
    fn trade(&mut self) -> Result<TradeInput> {
        Ok(TradeInput {
            txid: self.id()?,
            price: self.number()?,
            contracts: self.number()?,
            expiry_block: self.u64()?,
        })
    }
    fn settle(&mut self) -> Result<SettleRequest> {
        Ok(SettleRequest {
            settle_type: self.u8()?,
            reference_1: self.option_id()?,
            reference_2: self.option_id()?,
            mark_price: self.option_number()?,
            net_amount: self.option_number()?,
            column_a_is_seller: self.option_bool()?,
            expiry_block: self.option_height()?,
        })
    }
    fn king(&mut self) -> Result<KingRequest> {
        Ok(KingRequest {
            block_start: self.u64()?,
            block_end: self.u64()?,
            property_id: self.u32()?,
            net_amount: self.option_number()?,
            a_pays_b: self.option_bool()?,
            channel_root: self.option_id()?,
            total_contracts: self.number()?,
        })
    }
    fn custody(&mut self) -> Result<CustodyWitness> {
        Ok(CustodyWitness {
            owner_a: self.account()?,
            owner_b: self.account()?,
            atoms_a: self.u128()?,
            atoms_b: self.u128()?,
        })
    }
    fn action(&mut self) -> Result<Action> {
        match self.u8()? {
            0 => Ok(Action::Register {
                channel: self.channel()?,
                block: self.u64()?,
            }),
            1 => Ok(Action::Trade {
                channel: self.account()?,
                input: self.trade()?,
                block: self.u64()?,
            }),
            2 => Ok(Action::Settle {
                channel: self.account()?,
                request: self.settle()?,
                block: self.u64()?,
                txid: self.id()?,
                millis: self.u64()?,
            }),
            3 => Ok(Action::King {
                channel: self.account()?,
                request: self.king()?,
                block: self.u64()?,
                txid: self.id()?,
                millis: self.u64()?,
                custody: self.custody()?,
            }),
            4 => Ok(Action::Advance { block: self.u64()? }),
            5 => Ok(Action::Neutralize {
                channel: self.account()?,
                txid: self.id()?,
                block: self.u64()?,
            }),
            6 => Ok(Action::Sweep {
                channel: self.account()?,
                start: self.u64()?,
                end: self.u64()?,
                block: self.u64()?,
            }),
            _ => Err(ScalingError::InvalidSnapshot),
        }
    }
}
