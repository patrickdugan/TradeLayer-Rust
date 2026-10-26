//! Bounded, deterministic offline trade-history storage and queries.
//!
//! Records are caller assertions, not authenticated chain events. This module
//! never executes trades. Exact contract events support derived position, lot
//! and PnL reports without creating balances. It mirrors the useful
//! query predicates in pinned `tradeHistoryManager.js` while deliberately using
//! stable IDs and canonical ordering instead of random database document IDs.
//! Checkpoints use CRC32 for accidental corruption detection, not authentication.

use crate::codec::{verify_crc, with_crc, CodecError, Cursor};
use crate::ledger::{AccountId, PropertyId, TxId, MAX_ACCOUNT_BYTES};
use std::collections::BTreeMap;

pub const MAX_HISTORY_RECORDS: usize = 65_536;
const MAGIC: &[u8; 5] = b"TLTH\x01";

/// Stable caller-assigned identity of a fill, which may differ from either
/// transaction ID when one order produces multiple fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TradeId(pub [u8; 32]);

/// Exact integer economics. Token pair order is retained in storage, while pair
/// queries accept either order. Contract price is an unreduced rational in
/// caller-selected units; this module does not select a price/collateral scale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeDetails {
    Token {
        property1: PropertyId,
        property2: PropertyId,
        amount1_atoms: u128,
        amount2_atoms: u128,
    },
    Contract {
        contract_id: u32,
        amount_contracts: u64,
        price_numerator: u128,
        price_denominator: u128,
        liquidation: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeRecord {
    pub id: TradeId,
    pub block_height: u64,
    /// Stable, unique event position within a block, supplied by the caller.
    pub event_index: u32,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub buyer_tx: TxId,
    pub seller_tx: TxId,
    pub details: TradeDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    AlreadyPresent,
}

/// The legacy static queries compare stored addresses to the original query
/// OR its lowercase form. This is asymmetric, not case-insensitive matching.
/// AccountId restricts these inputs to printable ASCII.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMatch {
    Exact,
    LegacyOriginalOrLowercase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeHistoryError {
    InvalidRecord(&'static str),
    ConflictingId(TradeId),
    ConflictingPosition { block_height: u64, event_index: u32 },
    CapacityExceeded,
    ReversedBlockRange,
    InvalidCheckpoint(&'static str),
    InvalidExecution(&'static str),
    Arithmetic,
}

impl core::fmt::Display for TradeHistoryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TradeHistoryError {}

type Position = (u64, u32);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeHistory {
    records: BTreeMap<Position, TradeRecord>,
    executions: BTreeMap<Position, ContractExecution>,
    ids: BTreeMap<TradeId, Position>,
}

impl TradeRecord {
    fn position(&self) -> Position {
        (self.block_height, self.event_index)
    }

    fn validate(&self) -> Result<(), TradeHistoryError> {
        use TradeHistoryError::InvalidRecord;
        match &self.details {
            TradeDetails::Token {
                property1,
                property2,
                amount1_atoms,
                amount2_atoms,
            } => {
                if property1 == property2 {
                    return Err(InvalidRecord("same token pair"));
                }
                if *amount1_atoms == 0 || *amount2_atoms == 0 {
                    return Err(InvalidRecord("zero token amount"));
                }
            }
            TradeDetails::Contract {
                contract_id,
                amount_contracts,
                price_numerator,
                price_denominator,
                ..
            } => {
                if *contract_id == 0 {
                    return Err(InvalidRecord("zero contract id"));
                }
                if *amount_contracts == 0 {
                    return Err(InvalidRecord("zero contract amount"));
                }
                if *price_numerator == 0 || *price_denominator == 0 {
                    return Err(InvalidRecord("zero contract price component"));
                }
            }
        }
        Ok(())
    }

    fn is_contract(&self, expected: u32) -> bool {
        matches!(self.details, TradeDetails::Contract { contract_id, .. } if contract_id == expected)
    }
}

impl TradeHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.records.len() + self.executions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.executions.is_empty()
    }

    /// Original integer/token observation rows, chronological by event position.
    /// Exact contract execution rows are exposed separately by `executions`.
    pub fn records(&self) -> impl Iterator<Item = &TradeRecord> {
        self.records.values()
    }

    pub fn get(&self, id: TradeId) -> Option<&TradeRecord> {
        self.ids.get(&id).and_then(|key| self.records.get(key))
    }

    /// Exact retransmission is idempotent. Reusing an ID for different data or
    /// assigning two IDs to one event position fails without changing history.
    pub fn insert(&mut self, record: TradeRecord) -> Result<InsertOutcome, TradeHistoryError> {
        record.validate()?;
        if let Some(position) = self.ids.get(&record.id) {
            return if self.records.get(position) == Some(&record) {
                Ok(InsertOutcome::AlreadyPresent)
            } else {
                Err(TradeHistoryError::ConflictingId(record.id))
            };
        }
        let position = record.position();
        if self.records.contains_key(&position) || self.executions.contains_key(&position) {
            return Err(TradeHistoryError::ConflictingPosition {
                block_height: record.block_height,
                event_index: record.event_index,
            });
        }
        if self.len() >= MAX_HISTORY_RECORDS {
            return Err(TradeHistoryError::CapacityExceeded);
        }
        self.ids.insert(record.id, position);
        self.records.insert(position, record);
        Ok(InsertOutcome::Inserted)
    }

    /// Apply a batch atomically, returning the number of new rows. The bounded
    /// slice avoids consuming an unbounded iterator or allocating before checks.
    pub fn insert_batch(&mut self, records: &[TradeRecord]) -> Result<usize, TradeHistoryError> {
        if records.len() > MAX_HISTORY_RECORDS {
            return Err(TradeHistoryError::CapacityExceeded);
        }
        let mut staged = self.clone();
        let mut inserted = 0;
        for record in records {
            inserted += usize::from(staged.insert(record.clone())? == InsertOutcome::Inserted);
        }
        *self = staged;
        Ok(inserted)
    }

    /// Remove records strictly above the retained height, releasing their IDs
    /// and positions for replacement-chain replay. Zero retains block zero.
    /// This does not decide which chain or retained height is authoritative.
    pub fn rollback_after(&mut self, retained_height: u64) -> usize {
        let Some(next_height) = retained_height.checked_add(1) else {
            return 0;
        };
        let removed = self.records.split_off(&(next_height, 0));
        for record in removed.values() {
            self.ids.remove(&record.id);
        }
        let removed_executions = self.executions.split_off(&(next_height, 0));
        for record in removed_executions.values() {
            self.ids.remove(&record.id);
        }
        removed.len() + removed_executions.len()
    }

    pub fn contract_trades(&self, contract_id: u32) -> Vec<&TradeRecord> {
        self.records()
            .filter(|r| r.is_contract(contract_id))
            .collect()
    }

    pub fn contract_trades_for_address(
        &self,
        contract_id: u32,
        address: &AccountId,
        mode: AddressMatch,
    ) -> Vec<&TradeRecord> {
        let lowercase = address.as_str().to_ascii_lowercase();
        self.records()
            .filter(|r| r.is_contract(contract_id) && matches_address(r, address, &lowercase, mode))
            .collect()
    }

    pub fn token_trades_for_address(
        &self,
        property1: PropertyId,
        property2: PropertyId,
        address: &AccountId,
        mode: AddressMatch,
    ) -> Vec<&TradeRecord> {
        let lowercase = address.as_str().to_ascii_lowercase();
        self.records()
            .filter(|r| {
                matches!(r.details, TradeDetails::Token { property1: a, property2: b, .. }
                    if (a == property1 && b == property2) || (a == property2 && b == property1))
                    && matches_address(r, address, &lowercase, mode)
            })
            .collect()
    }

    /// Both block endpoints are included, matching the legacy range query.
    pub fn contract_trades_between_blocks(
        &self,
        contract_id: u32,
        start_block: u64,
        end_block: u64,
    ) -> Result<Vec<&TradeRecord>, TradeHistoryError> {
        if start_block > end_block {
            return Err(TradeHistoryError::ReversedBlockRange);
        }
        Ok(self
            .records
            .range((start_block, 0)..=(end_block, u32::MAX))
            .filter_map(|(_, r)| r.is_contract(contract_id).then_some(r))
            .collect())
    }

    pub fn liquidation_trades_at_block(
        &self,
        contract_id: u32,
        block_height: u64,
    ) -> Vec<&TradeRecord> {
        self.records
            .range((block_height, 0)..=(block_height, u32::MAX))
            .filter_map(|(_, r)| {
                matches!(r.details, TradeDetails::Contract { contract_id: id, liquidation: true, .. }
                    if id == contract_id)
                .then_some(r)
            })
            .collect()
    }

    pub fn first_trade_block(&self, contract_id: u32) -> Option<u64> {
        let legacy = self
            .records()
            .find(|r| r.is_contract(contract_id))
            .map(|r| r.block_height);
        let exact = self
            .executions()
            .find(|r| r.contract_id == contract_id)
            .map(|r| r.block_height);
        legacy.into_iter().chain(exact).min()
    }

    /// Canonical bounded checkpoint, independent of insertion order. The
    /// caller owns filesystem durability and must persist/reorg it alongside
    /// the corresponding ledger, chain identity and execution checkpoint.
    pub fn encode_checkpoint(&self) -> Result<Vec<u8>, TradeHistoryError> {
        if !self.executions.is_empty() {
            return self.encode_exact_checkpoint();
        }
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(self.len() as u32).to_be_bytes());
        for row in self.records() {
            bytes.extend_from_slice(&row.id.0);
            bytes.extend_from_slice(&row.block_height.to_be_bytes());
            bytes.extend_from_slice(&row.event_index.to_be_bytes());
            for account in [&row.buyer, &row.seller] {
                bytes.push(account.as_str().len() as u8);
                bytes.extend_from_slice(account.as_str().as_bytes());
            }
            bytes.extend_from_slice(&row.buyer_tx.as_bytes());
            bytes.extend_from_slice(&row.seller_tx.as_bytes());
            match row.details {
                TradeDetails::Token {
                    property1,
                    property2,
                    amount1_atoms,
                    amount2_atoms,
                } => {
                    bytes.push(0);
                    bytes.extend_from_slice(&property1.get().to_be_bytes());
                    bytes.extend_from_slice(&property2.get().to_be_bytes());
                    bytes.extend_from_slice(&amount1_atoms.to_be_bytes());
                    bytes.extend_from_slice(&amount2_atoms.to_be_bytes());
                }
                TradeDetails::Contract {
                    contract_id,
                    amount_contracts,
                    price_numerator,
                    price_denominator,
                    liquidation,
                } => {
                    bytes.push(1);
                    bytes.extend_from_slice(&contract_id.to_be_bytes());
                    bytes.extend_from_slice(&amount_contracts.to_be_bytes());
                    bytes.extend_from_slice(&price_numerator.to_be_bytes());
                    bytes.extend_from_slice(&price_denominator.to_be_bytes());
                    bytes.push(u8::from(liquidation));
                }
            }
        }
        with_crc(bytes).map_err(checkpoint_codec_error)
    }

    pub fn decode_checkpoint(bytes: &[u8]) -> Result<Self, TradeHistoryError> {
        let payload = verify_crc(bytes).map_err(checkpoint_codec_error)?;
        if payload.starts_with(b"TLTH\x02") {
            return Self::decode_exact_checkpoint(payload);
        }
        let mut reader = Cursor::new(payload);
        if reader.take(MAGIC.len()).map_err(checkpoint_codec_error)? != MAGIC {
            return Err(checkpoint_invalid("magic/version"));
        }
        let count = reader.u32().map_err(checkpoint_codec_error)? as usize;
        if count > MAX_HISTORY_RECORDS {
            return Err(TradeHistoryError::CapacityExceeded);
        }
        let mut history = Self::new();
        let mut previous = None;
        for _ in 0..count {
            let id = TradeId(reader.fixed().map_err(checkpoint_codec_error)?);
            let block_height = reader.u64().map_err(checkpoint_codec_error)?;
            let event_index = reader.u32().map_err(checkpoint_codec_error)?;
            let buyer = read_account(&mut reader)?;
            let seller = read_account(&mut reader)?;
            let buyer_tx = TxId::from_bytes(reader.fixed().map_err(checkpoint_codec_error)?);
            let seller_tx = TxId::from_bytes(reader.fixed().map_err(checkpoint_codec_error)?);
            let details = match reader.u8().map_err(checkpoint_codec_error)? {
                0 => TradeDetails::Token {
                    property1: read_property(&mut reader)?,
                    property2: read_property(&mut reader)?,
                    amount1_atoms: reader.u128().map_err(checkpoint_codec_error)?,
                    amount2_atoms: reader.u128().map_err(checkpoint_codec_error)?,
                },
                1 => TradeDetails::Contract {
                    contract_id: reader.u32().map_err(checkpoint_codec_error)?,
                    amount_contracts: reader.u64().map_err(checkpoint_codec_error)?,
                    price_numerator: reader.u128().map_err(checkpoint_codec_error)?,
                    price_denominator: reader.u128().map_err(checkpoint_codec_error)?,
                    liquidation: match reader.u8().map_err(checkpoint_codec_error)? {
                        0 => false,
                        1 => true,
                        _ => return Err(checkpoint_invalid("noncanonical liquidation flag")),
                    },
                },
                _ => return Err(checkpoint_invalid("unknown trade kind")),
            };
            let row = TradeRecord {
                id,
                block_height,
                event_index,
                buyer,
                seller,
                buyer_tx,
                seller_tx,
                details,
            };
            if previous.is_some_and(|position| position >= row.position()) {
                return Err(checkpoint_invalid("noncanonical record order"));
            }
            previous = Some(row.position());
            if history.insert(row)? != InsertOutcome::Inserted {
                return Err(checkpoint_invalid("duplicate record"));
            }
        }
        reader.end().map_err(checkpoint_codec_error)?;
        Ok(history)
    }
}

include!("trade_history_position_impl.rs");

fn matches_address(
    row: &TradeRecord,
    address: &AccountId,
    lowercase: &str,
    mode: AddressMatch,
) -> bool {
    [&row.buyer, &row.seller].iter().any(|stored| {
        *stored == address
            || (mode == AddressMatch::LegacyOriginalOrLowercase && stored.as_str() == lowercase)
    })
}

fn checkpoint_invalid(reason: &'static str) -> TradeHistoryError {
    TradeHistoryError::InvalidCheckpoint(reason)
}

fn checkpoint_codec_error(error: CodecError) -> TradeHistoryError {
    checkpoint_invalid(match error {
        CodecError::Truncated => "truncated",
        CodecError::TooLarge => "too large",
        CodecError::BadChecksum => "checksum",
        CodecError::TrailingBytes => "trailing bytes",
    })
}

fn read_account(reader: &mut Cursor<'_>) -> Result<AccountId, TradeHistoryError> {
    let length = reader.u8().map_err(checkpoint_codec_error)? as usize;
    if length == 0 || length > MAX_ACCOUNT_BYTES {
        return Err(checkpoint_invalid("account length"));
    }
    let bytes = reader.take(length).map_err(checkpoint_codec_error)?;
    let text = core::str::from_utf8(bytes).map_err(|_| checkpoint_invalid("account utf8"))?;
    AccountId::try_from(text).map_err(|_| checkpoint_invalid("account characters"))
}

fn read_property(reader: &mut Cursor<'_>) -> Result<PropertyId, TradeHistoryError> {
    PropertyId::try_from(reader.u32().map_err(checkpoint_codec_error)?)
        .map_err(|_| checkpoint_invalid("zero property id"))
}
