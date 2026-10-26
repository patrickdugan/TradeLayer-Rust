// Exact immutable contract reporting. Source defects remain oracle observations;
// CanonicalPositionHistoryV1 repairs shapes, aliases, lots and event identity.
use crate::decimal::{Decimal, Rounding};
use core::str::FromStr;
use std::collections::BTreeSet;

pub const MAX_HISTORY_DECIMAL_DIGITS: usize = 40;
pub const MAX_HISTORY_DECIMAL_SCALE: u32 = 20;
pub const POSITION_HISTORY_POLICY: &str = "CanonicalPositionHistoryV1";
const MAX_HISTORY_POSITION_DIGITS: usize = 64;
const EXACT_MAGIC: &[u8; 5] = b"TLTH\x02";
type HistoryResult<T> = Result<T, TradeHistoryError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractEventOrigin {
    Orders {
        buy_txid: TxId,
        sell_txid: TxId,
    },
    ForcedBook {
        epoch: TxId,
        fill_index: u32,
        maker_txid: TxId,
    },
    ForcedAmm {
        epoch: TxId,
        fill_index: u32,
        pool_id: u32,
    },
}
impl ContractEventOrigin {
    pub fn is_liquidation(&self) -> bool {
        !matches!(self, Self::Orders { .. })
    }
}
/// Booked settlement facts, never spendable balances. Actual payouts may satisfy
/// other, older claimants and therefore are retained only at event level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionPnl {
    pub booked: Decimal,
    pub collected_atoms: u128,
    pub claim_offset_atoms: u128,
    pub loss_offset_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractExecution {
    pub id: TradeId,
    pub block_height: u64,
    pub event_index: u32,
    pub contract_id: u32,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub quantity: Decimal,
    pub price: Decimal,
    pub origin: ContractEventOrigin,
    pub buyer_pnl: ExecutionPnl,
    pub seller_pnl: ExecutionPnl,
    pub retained_fee_atoms: u128,
    pub collected_loss_atoms: u128,
    pub paid_claim_atoms: u128,
    pub unfunded_loss_atoms: u128,
}

/// One canonical ordered query over both supported record forms. This keeps
/// legacy integer rows lossless instead of rounding exact events into them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRecordRef<'a> {
    Legacy(&'a TradeRecord),
    Execution(&'a ContractExecution),
}
impl<'a> HistoryRecordRef<'a> {
    pub fn id(self) -> TradeId {
        match self {
            Self::Legacy(row) => row.id,
            Self::Execution(row) => row.id,
        }
    }
    pub fn position(self) -> (u64, u32) {
        match self {
            Self::Legacy(row) => row.position(),
            Self::Execution(row) => row.position(),
        }
    }
    pub fn buyer(self) -> &'a AccountId {
        match self {
            Self::Legacy(row) => &row.buyer,
            Self::Execution(row) => &row.buyer,
        }
    }
    pub fn seller(self) -> &'a AccountId {
        match self {
            Self::Legacy(row) => &row.seller,
            Self::Execution(row) => &row.seller,
        }
    }
    pub fn contract_id(self) -> Option<u32> {
        match self {
            Self::Legacy(row) => match row.details {
                TradeDetails::Contract { contract_id, .. } => Some(contract_id),
                _ => None,
            },
            Self::Execution(row) => Some(row.contract_id),
        }
    }
    pub fn is_liquidation(self) -> bool {
        match self {
            Self::Legacy(row) => matches!(
                row.details,
                TradeDetails::Contract {
                    liquidation: true,
                    ..
                }
            ),
            Self::Execution(row) => row.origin.is_liquidation(),
        }
    }
}
impl ContractExecution {
    fn position(&self) -> Position {
        (self.block_height, self.event_index)
    }
    fn validate(&self) -> HistoryResult<()> {
        if self.contract_id == 0 || self.buyer == self.seller {
            return Err(TradeHistoryError::InvalidExecution(
                "contract or identical parties",
            ));
        }
        for value in [&self.quantity, &self.price] {
            history_decimal(value)?;
            if value.is_zero() || value.is_negative() {
                return Err(TradeHistoryError::InvalidExecution(
                    "nonpositive quantity/price",
                ));
            }
        }
        if matches!(
            self.origin,
            ContractEventOrigin::ForcedAmm { pool_id: 0, .. }
        ) {
            return Err(TradeHistoryError::InvalidExecution("zero pool id"));
        }
        let buyer_debt = validate_execution_pnl(&self.buyer_pnl)?;
        let seller_debt = validate_execution_pnl(&self.seller_pnl)?;
        if self
            .buyer_pnl
            .collected_atoms
            .checked_add(self.seller_pnl.collected_atoms)
            != Some(self.collected_loss_atoms)
            || buyer_debt.checked_add(seller_debt) != Some(self.unfunded_loss_atoms)
        {
            return Err(TradeHistoryError::InvalidExecution("loss totals"));
        }
        Ok(())
    }
}
fn history_decimal(value: &Decimal) -> HistoryResult<()> {
    if value.coefficient_digits() > MAX_HISTORY_DECIMAL_DIGITS
        || value.decimal_places() > MAX_HISTORY_DECIMAL_SCALE
    {
        Err(TradeHistoryError::CapacityExceeded)
    } else {
        Ok(())
    }
}
fn history_atoms(value: &Decimal) -> HistoryResult<u128> {
    value
        .abs()
        .checked_mul(&Decimal::from(100_000_000_u64))
        .and_then(|value| value.quantize(0, Rounding::Down))
        .map_err(|_| TradeHistoryError::Arithmetic)?
        .to_string()
        .parse()
        .map_err(|_| TradeHistoryError::Arithmetic)
}
fn validate_execution_pnl(pnl: &ExecutionPnl) -> HistoryResult<u128> {
    history_decimal(&pnl.booked)?;
    let atoms = history_atoms(&pnl.booked)?;
    if pnl.booked.is_negative() {
        if pnl.loss_offset_atoms != 0 {
            return Err(TradeHistoryError::InvalidExecution("negative loss offset"));
        }
        atoms
            .checked_sub(pnl.claim_offset_atoms)
            .and_then(|n| n.checked_sub(pnl.collected_atoms))
            .ok_or(TradeHistoryError::InvalidExecution(
                "negative PnL allocations",
            ))
    } else {
        if pnl.collected_atoms != 0 || pnl.claim_offset_atoms != 0 || pnl.loss_offset_atoms > atoms
        {
            return Err(TradeHistoryError::InvalidExecution(
                "positive PnL allocations",
            ));
        }
        Ok(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractDirection {
    Long,
    Short,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionEffect {
    pub before: Decimal,
    pub delta: Decimal,
    pub after: Decimal,
    pub closed: Decimal,
    pub opened: Decimal,
    pub flip: Decimal,
    pub fully_closed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractPositionSnapshot {
    pub trade_id: TradeId,
    pub block_height: u64,
    pub event_index: u32,
    pub price: Decimal,
    pub effect: PositionEffect,
    /// Reporting trade average. Marks do not change it or quantity.
    pub average_entry_price: Option<Decimal>,
    pub origin: ContractEventOrigin,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategorizedTrade {
    pub trade_id: TradeId,
    pub block_height: u64,
    pub event_index: u32,
    /// Signed in the execution direction, matching the source convention.
    pub amount: Decimal,
    pub price: Decimal,
    pub fully_closes_position: bool,
    pub origin: ContractEventOrigin,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategorizedTrades {
    pub open_trades: Vec<CategorizedTrade>,
    pub close_trades: Vec<CategorizedTrade>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionEpisode {
    pub direction: ContractDirection,
    pub open_trades: Vec<CategorizedTrade>,
    pub close_trades: Vec<CategorizedTrade>,
    pub remaining: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractTransactionReference {
    pub trade_id: TradeId,
    pub block_height: u64,
    pub event_index: u32,
    pub origin: ContractEventOrigin,
    /// Present only for an ordinary order's corresponding participant. A
    /// forced epoch/pool identifier is never presented as an external order.
    pub participant_order_txid: Option<TxId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifoLotUse {
    pub trade_id: TradeId,
    pub block_height: u64,
    pub event_index: u32,
    pub quantity: Decimal,
    pub price: Decimal,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LifoEntry {
    pub requested: Decimal,
    pub covered: Decimal,
    pub uncovered: Decimal,
    pub total_cost: Decimal,
    pub lots: Vec<LifoLotUse>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractPnlRecord {
    pub trade_id: TradeId,
    pub block_height: u64,
    pub event_index: u32,
    pub origin: ContractEventOrigin,
    pub participant: ExecutionPnl,
    /// Event-wide evidence; not attributed to this participant.
    pub event_paid_claim_atoms: u128,
    pub event_retained_fee_atoms: u128,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractPnlSummary {
    pub booked: Decimal,
    pub collected_atoms: u128,
    pub claim_offset_atoms: u128,
    pub loss_offset_atoms: u128,
}

#[derive(Clone)]
struct HistoryFill {
    id: TradeId,
    position: Position,
    buyer: AccountId,
    seller: AccountId,
    quantity: Decimal,
    price: Decimal,
    origin: ContractEventOrigin,
}
fn history_add(left: &Decimal, right: &Decimal) -> HistoryResult<Decimal> {
    left.checked_add(right)
        .map_err(|_| TradeHistoryError::Arithmetic)
}
fn history_sub(left: &Decimal, right: &Decimal) -> HistoryResult<Decimal> {
    left.checked_sub(right)
        .map_err(|_| TradeHistoryError::Arithmetic)
}
fn history_mul(left: &Decimal, right: &Decimal) -> HistoryResult<Decimal> {
    left.checked_mul(right)
        .map_err(|_| TradeHistoryError::Arithmetic)
}
fn history_position_effect(before: &Decimal, delta: &Decimal) -> HistoryResult<PositionEffect> {
    let after = history_add(before, delta)?;
    if after.coefficient_digits() > MAX_HISTORY_POSITION_DIGITS
        || after.decimal_places() > MAX_HISTORY_DECIMAL_SCALE
    {
        return Err(TradeHistoryError::CapacityExceeded);
    }
    let reducing =
        !before.is_zero() && !delta.is_zero() && before.is_negative() != delta.is_negative();
    let closed = if reducing {
        before.abs().min(delta.abs())
    } else {
        Decimal::zero()
    };
    let opened = history_sub(&delta.abs(), &closed)?;
    let flipped = reducing && !after.is_zero() && before.is_negative() != after.is_negative();
    Ok(PositionEffect {
        before: before.clone(),
        delta: delta.clone(),
        fully_closed: !before.is_zero() && after.is_zero(),
        after,
        closed,
        flip: if flipped {
            opened.clone()
        } else {
            Decimal::zero()
        },
        opened,
    })
}
fn history_trade_average(
    previous: Option<&Decimal>,
    effect: &PositionEffect,
    price: &Decimal,
) -> HistoryResult<Option<Decimal>> {
    if effect.after.is_zero() {
        return Ok(None);
    }
    if effect.before.is_zero() || !effect.flip.is_zero() {
        return Ok(Some(price.clone()));
    }
    let old = previous.ok_or(TradeHistoryError::InvalidExecution("missing average"))?;
    if effect.opened.is_zero() {
        return Ok(Some(old.clone()));
    }
    let cost = history_add(
        &history_mul(&effect.before.abs(), old)?,
        &history_mul(&effect.opened, price)?,
    )?;
    cost.checked_div(&effect.after.abs(), 20, Rounding::HalfUp)
        .map(Some)
        .map_err(|_| TradeHistoryError::Arithmetic)
}
fn category_leg(
    snapshot: &ContractPositionSnapshot,
    quantity: &Decimal,
    fully_closes_position: bool,
) -> CategorizedTrade {
    CategorizedTrade {
        trade_id: snapshot.trade_id,
        block_height: snapshot.block_height,
        event_index: snapshot.event_index,
        amount: if snapshot.effect.delta.is_negative() {
            quantity.negated()
        } else {
            quantity.clone()
        },
        price: snapshot.price.clone(),
        fully_closes_position,
        origin: snapshot.origin.clone(),
    }
}

impl TradeHistory {
    pub fn entries(&self) -> Vec<HistoryRecordRef<'_>> {
        let mut rows: Vec<_> = self
            .records()
            .map(HistoryRecordRef::Legacy)
            .chain(self.executions().map(HistoryRecordRef::Execution))
            .collect();
        rows.sort_by_key(|row| row.position());
        rows
    }
    pub fn contract_entries(&self, contract: u32) -> Vec<HistoryRecordRef<'_>> {
        self.entries()
            .into_iter()
            .filter(|row| row.contract_id() == Some(contract))
            .collect()
    }
    pub fn contract_entries_for_address(
        &self,
        contract: u32,
        owner: &AccountId,
        mode: AddressMatch,
    ) -> Vec<HistoryRecordRef<'_>> {
        let lower = owner.as_str().to_ascii_lowercase();
        self.contract_entries(contract)
            .into_iter()
            .filter(|row| {
                [row.buyer(), row.seller()].iter().any(|party| {
                    *party == owner
                        || (mode == AddressMatch::LegacyOriginalOrLowercase
                            && party.as_str() == lower)
                })
            })
            .collect()
    }
    pub fn contract_entries_between_blocks(
        &self,
        contract: u32,
        start: u64,
        end: u64,
    ) -> HistoryResult<Vec<HistoryRecordRef<'_>>> {
        if start > end {
            return Err(TradeHistoryError::ReversedBlockRange);
        }
        Ok(self
            .contract_entries(contract)
            .into_iter()
            .filter(|row| (start..=end).contains(&row.position().0))
            .collect())
    }
    pub fn liquidation_entries_at_block(
        &self,
        contract: u32,
        height: u64,
    ) -> Vec<HistoryRecordRef<'_>> {
        self.contract_entries(contract)
            .into_iter()
            .filter(|row| row.position().0 == height && row.is_liquidation())
            .collect()
    }
    pub fn executions(&self) -> impl Iterator<Item = &ContractExecution> {
        self.executions.values()
    }
    pub fn execution(&self, id: TradeId) -> Option<&ContractExecution> {
        self.ids.get(&id).and_then(|key| self.executions.get(key))
    }
    pub fn insert_execution(&mut self, event: ContractExecution) -> HistoryResult<InsertOutcome> {
        if self.execution_preflight(&event)? == InsertOutcome::AlreadyPresent {
            return Ok(InsertOutcome::AlreadyPresent);
        }
        let mut candidate = self.clone();
        let result = candidate.insert_execution_inner(event)?;
        candidate.verify()?;
        *self = candidate;
        Ok(result)
    }
    pub fn insert_execution_batch(&mut self, events: &[ContractExecution]) -> HistoryResult<usize> {
        if events.len() > MAX_HISTORY_RECORDS {
            return Err(TradeHistoryError::CapacityExceeded);
        }
        for event in events {
            event.validate()?;
        }
        let mut candidate = self.clone();
        let mut added = 0;
        for event in events {
            added += usize::from(
                candidate.insert_execution_inner(event.clone())? == InsertOutcome::Inserted,
            );
        }
        candidate.verify()?;
        *self = candidate;
        Ok(added)
    }
    fn insert_execution_inner(&mut self, event: ContractExecution) -> HistoryResult<InsertOutcome> {
        if self.execution_preflight(&event)? == InsertOutcome::AlreadyPresent {
            return Ok(InsertOutcome::AlreadyPresent);
        }
        let position = event.position();
        self.ids.insert(event.id, position);
        self.executions.insert(position, event);
        Ok(InsertOutcome::Inserted)
    }
    fn execution_preflight(&self, event: &ContractExecution) -> HistoryResult<InsertOutcome> {
        event.validate()?;
        let position = event.position();
        if let Some(existing) = self.ids.get(&event.id) {
            return if self.executions.get(existing) == Some(event) {
                Ok(InsertOutcome::AlreadyPresent)
            } else {
                Err(TradeHistoryError::ConflictingId(event.id))
            };
        }
        if self.records.contains_key(&position) || self.executions.contains_key(&position) {
            return Err(TradeHistoryError::ConflictingPosition {
                block_height: position.0,
                event_index: position.1,
            });
        }
        if self.len() >= MAX_HISTORY_RECORDS {
            return Err(TradeHistoryError::CapacityExceeded);
        }
        Ok(InsertOutcome::Inserted)
    }
    pub fn contract_executions(&self, contract_id: u32) -> Vec<&ContractExecution> {
        self.executions()
            .filter(|event| event.contract_id == contract_id)
            .collect()
    }
    pub fn executions_for_address(
        &self,
        contract_id: u32,
        owner: &AccountId,
        mode: AddressMatch,
    ) -> Vec<&ContractExecution> {
        let lower = owner.as_str().to_ascii_lowercase();
        self.executions()
            .filter(|event| {
                event.contract_id == contract_id
                    && [&event.buyer, &event.seller].iter().any(|party| {
                        *party == owner
                            || (mode == AddressMatch::LegacyOriginalOrLowercase
                                && party.as_str() == lower)
                    })
            })
            .collect()
    }
    pub fn executions_between_blocks(
        &self,
        contract_id: u32,
        start: u64,
        end: u64,
    ) -> HistoryResult<Vec<&ContractExecution>> {
        if start > end {
            return Err(TradeHistoryError::ReversedBlockRange);
        }
        Ok(self
            .executions
            .range((start, 0)..=(end, u32::MAX))
            .filter_map(|(_, event)| (event.contract_id == contract_id).then_some(event))
            .collect())
    }
    pub fn liquidation_executions_at_block(
        &self,
        contract_id: u32,
        height: u64,
    ) -> Vec<&ContractExecution> {
        self.executions
            .range((height, 0)..=(height, u32::MAX))
            .filter_map(|(_, event)| {
                (event.contract_id == contract_id && event.origin.is_liquidation()).then_some(event)
            })
            .collect()
    }
    fn contract_fills(
        &self,
        contract: u32,
        owner: &AccountId,
        through: Option<Position>,
    ) -> HistoryResult<Vec<HistoryFill>> {
        let mut fills = Vec::new();
        for row in self.contract_trades(contract) {
            if (&row.buyer != owner && &row.seller != owner)
                || through.is_some_and(|end| row.position() > end)
            {
                continue;
            }
            if let TradeDetails::Contract {
                amount_contracts,
                price_numerator,
                price_denominator,
                ..
            } = row.details
            {
                fills.push(HistoryFill {
                    id: row.id,
                    position: row.position(),
                    buyer: row.buyer.clone(),
                    seller: row.seller.clone(),
                    quantity: Decimal::from(amount_contracts),
                    price: Decimal::from(price_numerator)
                        .checked_div(&Decimal::from(price_denominator), 20, Rounding::HalfUp)
                        .map_err(|_| TradeHistoryError::Arithmetic)?,
                    origin: ContractEventOrigin::Orders {
                        buy_txid: row.buyer_tx,
                        sell_txid: row.seller_tx,
                    },
                });
            }
        }
        for event in self.contract_executions(contract) {
            if (&event.buyer != owner && &event.seller != owner)
                || through.is_some_and(|end| event.position() > end)
            {
                continue;
            }
            fills.push(HistoryFill {
                id: event.id,
                position: event.position(),
                buyer: event.buyer.clone(),
                seller: event.seller.clone(),
                quantity: event.quantity.clone(),
                price: event.price.clone(),
                origin: event.origin.clone(),
            });
        }
        fills.sort_by_key(|fill| fill.position);
        Ok(fills)
    }
    pub fn position_history(
        &self,
        owner: &AccountId,
        contract: u32,
    ) -> HistoryResult<Vec<ContractPositionSnapshot>> {
        self.position_history_through(owner, contract, None)
    }
    pub fn position_history_through(
        &self,
        owner: &AccountId,
        contract: u32,
        through: Option<(u64, u32)>,
    ) -> HistoryResult<Vec<ContractPositionSnapshot>> {
        let mut quantity = Decimal::zero();
        let mut average = None;
        let mut snapshots = Vec::new();
        for fill in self.contract_fills(contract, owner, through)? {
            let delta = if fill.buyer == fill.seller {
                Decimal::zero()
            } else if &fill.buyer == owner {
                fill.quantity.clone()
            } else {
                fill.quantity.negated()
            };
            let effect = history_position_effect(&quantity, &delta)?;
            average = history_trade_average(average.as_ref(), &effect, &fill.price)?;
            quantity = effect.after.clone();
            snapshots.push(ContractPositionSnapshot {
                trade_id: fill.id,
                block_height: fill.position.0,
                event_index: fill.position.1,
                price: fill.price,
                effect,
                average_entry_price: average.clone(),
                origin: fill.origin,
            });
        }
        Ok(snapshots)
    }
    pub fn categorized_trades(
        &self,
        owner: &AccountId,
        contract: u32,
    ) -> HistoryResult<CategorizedTrades> {
        let mut result = CategorizedTrades::default();
        for snapshot in self.position_history(owner, contract)? {
            if !snapshot.effect.closed.is_zero() {
                result.close_trades.push(category_leg(
                    &snapshot,
                    &snapshot.effect.closed,
                    snapshot.effect.fully_closed || !snapshot.effect.flip.is_zero(),
                ));
            }
            if !snapshot.effect.opened.is_zero() {
                result
                    .open_trades
                    .push(category_leg(&snapshot, &snapshot.effect.opened, false));
            }
        }
        Ok(result)
    }
    pub fn position_episodes(
        &self,
        owner: &AccountId,
        contract: u32,
    ) -> HistoryResult<Vec<PositionEpisode>> {
        let mut episodes = Vec::new();
        let mut current: Option<PositionEpisode> = None;
        for snapshot in self.position_history(owner, contract)? {
            if !snapshot.effect.closed.is_zero() {
                let episode = current
                    .as_mut()
                    .ok_or(TradeHistoryError::InvalidExecution("missing episode"))?;
                let complete = snapshot.effect.fully_closed || !snapshot.effect.flip.is_zero();
                episode.close_trades.push(category_leg(
                    &snapshot,
                    &snapshot.effect.closed,
                    complete,
                ));
                episode.remaining = history_sub(&episode.remaining, &snapshot.effect.closed)?;
                if complete {
                    episodes.push(current.take().expect("episode exists"));
                }
            }
            if !snapshot.effect.opened.is_zero() {
                let episode = current.get_or_insert_with(|| PositionEpisode {
                    direction: if snapshot.effect.delta.is_negative() {
                        ContractDirection::Short
                    } else {
                        ContractDirection::Long
                    },
                    open_trades: Vec::new(),
                    close_trades: Vec::new(),
                    remaining: Decimal::zero(),
                });
                episode
                    .open_trades
                    .push(category_leg(&snapshot, &snapshot.effect.opened, false));
                episode.remaining = history_add(&episode.remaining, &snapshot.effect.opened)?;
            }
        }
        if let Some(episode) = current {
            episodes.push(episode);
        }
        Ok(episodes)
    }
    pub fn transaction_references(
        &self,
        owner: &AccountId,
        contract: u32,
    ) -> HistoryResult<Vec<ContractTransactionReference>> {
        self.contract_fills(contract, owner, None)?
            .into_iter()
            .map(|fill| {
                let participant_order_txid = match fill.origin {
                    ContractEventOrigin::Orders {
                        buy_txid,
                        sell_txid,
                    } => Some(if &fill.buyer == owner {
                        buy_txid
                    } else {
                        sell_txid
                    }),
                    _ => None,
                };
                Ok(ContractTransactionReference {
                    trade_id: fill.id,
                    block_height: fill.position.0,
                    event_index: fill.position.1,
                    origin: fill.origin,
                    participant_order_txid,
                })
            })
            .collect()
    }
    /// Repaired LIFO over outstanding lots after all earlier closes. Cost is a
    /// quote-price reporting quantity, not collateral PnL or a wallet credit.
    pub fn lifo_entry(
        &self,
        owner: &AccountId,
        contract: u32,
        requested: &Decimal,
        through: Option<(u64, u32)>,
    ) -> HistoryResult<LifoEntry> {
        history_decimal(requested)?;
        let mut lots: Vec<LifoLotUse> = Vec::new();
        for snapshot in self.position_history_through(owner, contract, through)? {
            let mut closing = snapshot.effect.closed;
            while !closing.is_zero() {
                let lot = lots
                    .last_mut()
                    .ok_or(TradeHistoryError::InvalidExecution("missing LIFO lot"))?;
                let consumed = lot.quantity.clone().min(closing.clone());
                lot.quantity = history_sub(&lot.quantity, &consumed)?;
                closing = history_sub(&closing, &consumed)?;
                if lot.quantity.is_zero() {
                    lots.pop();
                }
            }
            if !snapshot.effect.opened.is_zero() {
                lots.push(LifoLotUse {
                    trade_id: snapshot.trade_id,
                    block_height: snapshot.block_height,
                    event_index: snapshot.event_index,
                    quantity: snapshot.effect.opened,
                    price: snapshot.price,
                });
            }
        }
        let mut result = LifoEntry {
            requested: requested.abs(),
            uncovered: requested.abs(),
            ..LifoEntry::default()
        };
        for mut lot in lots.into_iter().rev() {
            if result.uncovered.is_zero() {
                break;
            }
            lot.quantity = lot.quantity.min(result.uncovered.clone());
            result.uncovered = history_sub(&result.uncovered, &lot.quantity)?;
            result.covered = history_add(&result.covered, &lot.quantity)?;
            result.total_cost =
                history_add(&result.total_cost, &history_mul(&lot.quantity, &lot.price)?)?;
            result.lots.push(lot);
        }
        Ok(result)
    }
    pub fn pnl_history(&self, owner: &AccountId, contract: u32) -> Vec<ContractPnlRecord> {
        self.executions_for_address(contract, owner, AddressMatch::Exact)
            .into_iter()
            .map(|event| ContractPnlRecord {
                trade_id: event.id,
                block_height: event.block_height,
                event_index: event.event_index,
                origin: event.origin.clone(),
                participant: if &event.buyer == owner {
                    event.buyer_pnl.clone()
                } else {
                    event.seller_pnl.clone()
                },
                event_paid_claim_atoms: event.paid_claim_atoms,
                event_retained_fee_atoms: event.retained_fee_atoms,
            })
            .collect()
    }
    pub fn pnl_summary(
        &self,
        owner: &AccountId,
        contract: u32,
    ) -> HistoryResult<ContractPnlSummary> {
        let mut result = ContractPnlSummary::default();
        for row in self.pnl_history(owner, contract) {
            result.booked = history_add(&result.booked, &row.participant.booked)?;
            result.collected_atoms = result
                .collected_atoms
                .checked_add(row.participant.collected_atoms)
                .ok_or(TradeHistoryError::Arithmetic)?;
            result.claim_offset_atoms = result
                .claim_offset_atoms
                .checked_add(row.participant.claim_offset_atoms)
                .ok_or(TradeHistoryError::Arithmetic)?;
            result.loss_offset_atoms = result
                .loss_offset_atoms
                .checked_add(row.participant.loss_offset_atoms)
                .ok_or(TradeHistoryError::Arithmetic)?;
        }
        Ok(result)
    }
    fn position_totals(&self) -> HistoryResult<BTreeMap<(u32, AccountId), Decimal>> {
        let mut totals = BTreeMap::new();
        let mut apply = |contract: u32,
                         buyer: &AccountId,
                         seller: &AccountId,
                         quantity: &Decimal|
         -> HistoryResult<()> {
            for (owner, delta) in [(buyer, quantity.clone()), (seller, quantity.negated())] {
                let key = (contract, owner.clone());
                let next = history_add(totals.get(&key).unwrap_or(&Decimal::zero()), &delta)?;
                if next.coefficient_digits() > MAX_HISTORY_POSITION_DIGITS
                    || next.decimal_places() > MAX_HISTORY_DECIMAL_SCALE
                {
                    return Err(TradeHistoryError::CapacityExceeded);
                }
                if next.is_zero() {
                    totals.remove(&key);
                } else {
                    totals.insert(key, next);
                }
            }
            Ok(())
        };
        for row in self.records() {
            if let TradeDetails::Contract {
                contract_id,
                amount_contracts,
                ..
            } = row.details
            {
                apply(
                    contract_id,
                    &row.buyer,
                    &row.seller,
                    &Decimal::from(amount_contracts),
                )?;
            }
        }
        for event in self.executions() {
            apply(
                event.contract_id,
                &event.buyer,
                &event.seller,
                &event.quantity,
            )?;
        }
        Ok(totals)
    }
    pub fn verify(&self) -> HistoryResult<()> {
        if self.len() > MAX_HISTORY_RECORDS || self.ids.len() != self.len() {
            return Err(TradeHistoryError::CapacityExceeded);
        }
        let mut positions = BTreeSet::new();
        for (position, row) in &self.records {
            row.validate()?;
            if *position != row.position()
                || self.ids.get(&row.id) != Some(position)
                || !positions.insert(*position)
            {
                return Err(checkpoint_invalid("legacy indexes"));
            }
        }
        for (position, row) in &self.executions {
            row.validate()?;
            if *position != row.position()
                || self.ids.get(&row.id) != Some(position)
                || !positions.insert(*position)
            {
                return Err(checkpoint_invalid("execution indexes"));
            }
        }
        self.position_totals()?;
        Ok(())
    }
}

/// Exact source computeAverageEntryPrices: absolute weights, 20dp HalfUp
/// division, then binary64 return. Negative prices are source-valid here.
pub fn source_weighted_mean(entries: &[(Decimal, Decimal)]) -> HistoryResult<f64> {
    if entries.len() > MAX_HISTORY_RECORDS {
        return Err(TradeHistoryError::CapacityExceeded);
    }
    let mut cost = Decimal::zero();
    let mut quantity = Decimal::zero();
    for (price, amount) in entries {
        history_decimal(price)?;
        history_decimal(amount)?;
        cost = history_add(&cost, &history_mul(price, &amount.abs())?)?;
        quantity = history_add(&quantity, &amount.abs())?;
    }
    if quantity.is_zero() {
        return Ok(0.0);
    }
    let mean = cost
        .checked_div(&quantity, 20, Rounding::HalfUp)
        .map_err(|_| TradeHistoryError::Arithmetic)?;
    if mean.is_zero() && cost.is_negative() {
        return Ok(-0.0);
    }
    let number: f64 = mean
        .to_string()
        .parse()
        .map_err(|_| TradeHistoryError::Arithmetic)?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(TradeHistoryError::Arithmetic)
    }
}

/// Source breakUpOpeningTrades returns references to the original rows. Index
/// groups preserve those aliases without mutable shared objects. None models
/// a missing blockHeight. It can return an empty first group and always returns
/// one empty group for empty input. It does not sort or inspect close flags.
pub fn source_break_up_opening_indices(
    open_heights: &[Option<u64>],
    close_heights: &[Option<u64>],
) -> HistoryResult<Vec<Vec<usize>>> {
    if open_heights.len() > MAX_HISTORY_RECORDS
        || close_heights.len() > MAX_HISTORY_RECORDS
        || open_heights
            .iter()
            .chain(close_heights)
            .flatten()
            .any(|height| *height > 9_007_199_254_740_991)
    {
        return Err(TradeHistoryError::CapacityExceeded);
    }
    let mut separated = Vec::new();
    let mut current = Vec::new();
    let mut previous = None;
    let closes: BTreeSet<_> = close_heights.iter().flatten().copied().collect();
    for (index, height) in open_heights.iter().enumerate() {
        if previous != Some(*height)
            && height
                .and_then(|h| h.checked_sub(1))
                .is_some_and(|prior| closes.contains(&prior))
        {
            separated.push(current);
            current = Vec::new();
        }
        current.push(index);
        previous = Some(*height);
    }
    separated.push(current);
    Ok(separated)
}

fn history_write_decimal(bytes: &mut Vec<u8>, value: &Decimal) -> HistoryResult<()> {
    let text = value.to_string();
    if text.len() > 128 {
        return Err(TradeHistoryError::CapacityExceeded);
    }
    bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
    Ok(())
}
fn history_read_decimal(reader: &mut Cursor<'_>) -> HistoryResult<Decimal> {
    let count = reader.u32().map_err(checkpoint_codec_error)? as usize;
    if count == 0 || count > 128 {
        return Err(checkpoint_invalid("decimal length"));
    }
    let text = core::str::from_utf8(reader.take(count).map_err(checkpoint_codec_error)?)
        .map_err(|_| checkpoint_invalid("decimal UTF8"))?;
    let value = Decimal::from_str(text).map_err(|_| checkpoint_invalid("decimal syntax"))?;
    if value.to_string() != text {
        return Err(checkpoint_invalid("noncanonical decimal"));
    }
    Ok(value)
}
fn history_write_account(bytes: &mut Vec<u8>, value: &AccountId) {
    bytes.push(value.as_str().len() as u8);
    bytes.extend_from_slice(value.as_str().as_bytes());
}
fn history_write_pnl(bytes: &mut Vec<u8>, pnl: &ExecutionPnl) -> HistoryResult<()> {
    history_write_decimal(bytes, &pnl.booked)?;
    for atoms in [
        pnl.collected_atoms,
        pnl.claim_offset_atoms,
        pnl.loss_offset_atoms,
    ] {
        bytes.extend_from_slice(&atoms.to_be_bytes());
    }
    Ok(())
}
fn history_read_pnl(reader: &mut Cursor<'_>) -> HistoryResult<ExecutionPnl> {
    Ok(ExecutionPnl {
        booked: history_read_decimal(reader)?,
        collected_atoms: reader.u128().map_err(checkpoint_codec_error)?,
        claim_offset_atoms: reader.u128().map_err(checkpoint_codec_error)?,
        loss_offset_atoms: reader.u128().map_err(checkpoint_codec_error)?,
    })
}
fn history_write_execution(bytes: &mut Vec<u8>, event: &ContractExecution) -> HistoryResult<()> {
    bytes.extend_from_slice(&event.id.0);
    bytes.extend_from_slice(&event.block_height.to_be_bytes());
    bytes.extend_from_slice(&event.event_index.to_be_bytes());
    bytes.extend_from_slice(&event.contract_id.to_be_bytes());
    history_write_account(bytes, &event.buyer);
    history_write_account(bytes, &event.seller);
    history_write_decimal(bytes, &event.quantity)?;
    history_write_decimal(bytes, &event.price)?;
    match event.origin {
        ContractEventOrigin::Orders {
            buy_txid,
            sell_txid,
        } => {
            bytes.push(0);
            bytes.extend_from_slice(&buy_txid.as_bytes());
            bytes.extend_from_slice(&sell_txid.as_bytes());
        }
        ContractEventOrigin::ForcedBook {
            epoch,
            fill_index,
            maker_txid,
        } => {
            bytes.push(1);
            bytes.extend_from_slice(&epoch.as_bytes());
            bytes.extend_from_slice(&fill_index.to_be_bytes());
            bytes.extend_from_slice(&maker_txid.as_bytes());
        }
        ContractEventOrigin::ForcedAmm {
            epoch,
            fill_index,
            pool_id,
        } => {
            bytes.push(2);
            bytes.extend_from_slice(&epoch.as_bytes());
            bytes.extend_from_slice(&fill_index.to_be_bytes());
            bytes.extend_from_slice(&pool_id.to_be_bytes());
        }
    }
    history_write_pnl(bytes, &event.buyer_pnl)?;
    history_write_pnl(bytes, &event.seller_pnl)?;
    for atoms in [
        event.retained_fee_atoms,
        event.collected_loss_atoms,
        event.paid_claim_atoms,
        event.unfunded_loss_atoms,
    ] {
        bytes.extend_from_slice(&atoms.to_be_bytes());
    }
    Ok(())
}
fn history_read_execution(reader: &mut Cursor<'_>) -> HistoryResult<ContractExecution> {
    let id = TradeId(reader.fixed().map_err(checkpoint_codec_error)?);
    let block_height = reader.u64().map_err(checkpoint_codec_error)?;
    let event_index = reader.u32().map_err(checkpoint_codec_error)?;
    let contract_id = reader.u32().map_err(checkpoint_codec_error)?;
    let buyer = read_account(reader)?;
    let seller = read_account(reader)?;
    let quantity = history_read_decimal(reader)?;
    let price = history_read_decimal(reader)?;
    let tx = |reader: &mut Cursor<'_>| {
        reader
            .fixed()
            .map(TxId::from_bytes)
            .map_err(checkpoint_codec_error)
    };
    let origin = match reader.u8().map_err(checkpoint_codec_error)? {
        0 => ContractEventOrigin::Orders {
            buy_txid: tx(reader)?,
            sell_txid: tx(reader)?,
        },
        1 => ContractEventOrigin::ForcedBook {
            epoch: tx(reader)?,
            fill_index: reader.u32().map_err(checkpoint_codec_error)?,
            maker_txid: tx(reader)?,
        },
        2 => ContractEventOrigin::ForcedAmm {
            epoch: tx(reader)?,
            fill_index: reader.u32().map_err(checkpoint_codec_error)?,
            pool_id: reader.u32().map_err(checkpoint_codec_error)?,
        },
        _ => return Err(checkpoint_invalid("origin tag")),
    };
    Ok(ContractExecution {
        id,
        block_height,
        event_index,
        contract_id,
        buyer,
        seller,
        quantity,
        price,
        origin,
        buyer_pnl: history_read_pnl(reader)?,
        seller_pnl: history_read_pnl(reader)?,
        retained_fee_atoms: reader.u128().map_err(checkpoint_codec_error)?,
        collected_loss_atoms: reader.u128().map_err(checkpoint_codec_error)?,
        paid_claim_atoms: reader.u128().map_err(checkpoint_codec_error)?,
        unfunded_loss_atoms: reader.u128().map_err(checkpoint_codec_error)?,
    })
}
impl TradeHistory {
    fn encode_exact_checkpoint(&self) -> HistoryResult<Vec<u8>> {
        self.verify()?;
        let legacy = Self {
            records: self.records.clone(),
            executions: BTreeMap::new(),
            ids: self
                .records
                .values()
                .map(|row| (row.id, row.position()))
                .collect(),
        }
        .encode_checkpoint()?;
        let mut bytes = EXACT_MAGIC.to_vec();
        bytes.extend_from_slice(&(legacy.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&legacy);
        bytes.extend_from_slice(&(self.executions.len() as u32).to_be_bytes());
        for event in self.executions() {
            history_write_execution(&mut bytes, event)?;
        }
        // Redundant derived position capture is checked against a full event
        // replay at restore. It carries no separate balance or trust authority.
        let totals = self.position_totals()?;
        bytes.extend_from_slice(&(totals.len() as u32).to_be_bytes());
        for ((contract, owner), quantity) in totals {
            bytes.extend_from_slice(&contract.to_be_bytes());
            history_write_account(&mut bytes, &owner);
            history_write_decimal(&mut bytes, &quantity)?;
        }
        with_crc(bytes).map_err(checkpoint_codec_error)
    }
    fn decode_exact_checkpoint(payload: &[u8]) -> HistoryResult<Self> {
        let mut reader = Cursor::new(payload);
        if reader.take(5).map_err(checkpoint_codec_error)? != EXACT_MAGIC {
            return Err(checkpoint_invalid("exact magic"));
        }
        let length = reader.u32().map_err(checkpoint_codec_error)? as usize;
        let legacy = reader.take(length).map_err(checkpoint_codec_error)?;
        if !legacy.starts_with(MAGIC) {
            return Err(checkpoint_invalid("nested legacy version"));
        }
        let mut history = Self::decode_checkpoint(legacy)?;
        let count = reader.u32().map_err(checkpoint_codec_error)? as usize;
        if count == 0 || count > MAX_HISTORY_RECORDS.saturating_sub(history.len()) {
            return Err(checkpoint_invalid("exact count/version selection"));
        }
        let mut previous = None;
        for _ in 0..count {
            let event = history_read_execution(&mut reader)?;
            if previous.is_some_and(|position| position >= event.position()) {
                return Err(checkpoint_invalid("exact event order"));
            }
            previous = Some(event.position());
            if history.insert_execution_inner(event)? != InsertOutcome::Inserted {
                return Err(checkpoint_invalid("duplicate exact event"));
            }
        }
        let expected = history.position_totals()?;
        let count = reader.u32().map_err(checkpoint_codec_error)? as usize;
        if count != expected.len() {
            return Err(checkpoint_invalid("position count"));
        }
        let mut totals = BTreeMap::new();
        let mut previous = None;
        for _ in 0..count {
            let contract = reader.u32().map_err(checkpoint_codec_error)?;
            let owner = read_account(&mut reader)?;
            let quantity = history_read_decimal(&mut reader)?;
            let key = (contract, owner);
            if quantity.is_zero() || previous.as_ref().is_some_and(|prior| prior >= &key) {
                return Err(checkpoint_invalid("position order/zero"));
            }
            previous = Some(key.clone());
            totals.insert(key, quantity);
        }
        reader.end().map_err(checkpoint_codec_error)?;
        if totals != expected {
            return Err(checkpoint_invalid("replayed positions"));
        }
        history.verify()?;
        Ok(history)
    }
}
