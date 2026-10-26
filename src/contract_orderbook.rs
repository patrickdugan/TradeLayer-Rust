//! Bounded contract-order metadata, with no ledger or settlement authority.
//!
//! Matching follows the archived orderbook.js price/time, market, same-block
//! post and self-cross rules. Queue replay, cancellation ownership, bounded
//! iteration and reserve-quantity accounting are explicit typed repairs. A
//! coordinator must stage this component with its sole ledger and positions.

use crate::contract_registry::ContractId;
use crate::decimal::{Decimal, DecimalError};
use crate::ledger::{AccountId, TxId};
use core::{cmp::Ordering, fmt};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CONTRACT_ORDERS: usize = 4096;
pub const MAX_CONTRACT_ORDER_IDS: usize = 65_536;
pub const MAX_ORDER_DECIMAL_DIGITS: usize = 40;
pub const MAX_ORDER_DECIMAL_SCALE: u32 = 20;
pub const MAX_ORDER_HEIGHT: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookError {
    WrongContract,
    InvalidNumber,
    InvalidOwner,
    InvalidHeight,
    DuplicateInput,
    Capacity,
    InvalidSnapshot,
    Arithmetic,
}
impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BookError {}
impl From<DecimalError> for BookError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
type Result<T> = core::result::Result<T, BookError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Buy,
    Sell,
}

/// Caller-supplied metadata; no accepted-wire, identity or funding assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub txid: TxId,
    pub owner: AccountId,
    pub contract: ContractId,
    pub height: u64,
    pub side: Side,
    pub quantity: Decimal,
    pub price: Decimal,
    pub post: bool,
    pub is_market: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestingOrder {
    pub order: Order,
    pub remaining: Decimal,
    /// Opening quantity still represented by this order, never token atoms.
    /// Reducing quantity is consumed first. Custody must reconcile actual funds.
    pub reserve_quantity: Decimal,
    pub initial_reduce: bool,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveIntent {
    pub txid: TxId,
    pub owner: AccountId,
    pub quantity: Decimal,
    pub initial_reduce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub contract: ContractId,
    pub buy_txid: TxId,
    pub sell_txid: TxId,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub quantity: Decimal,
    pub price: Decimal,
    pub buy_maker: bool,
    pub sell_maker: bool,
    pub buy_remaining: Decimal,
    pub sell_remaining: Decimal,
    pub buy_reserve_used: Decimal,
    pub sell_reserve_used: Decimal,
    pub buy_initial_reduce: bool,
    pub sell_initial_reduce: bool,
    /// Source selects the sell ID if sell is maker; otherwise the buy ID.
    pub source_txid: TxId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookTransition {
    pub admitted: Vec<ReserveIntent>,
    pub matches: Vec<Match>,
    /// Source silently drops a resting self-cross order. Return it so the
    /// coordinator can explicitly release its actual, owned reserve.
    pub self_cross_removed: Vec<RestingOrder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelCriteria {
    All,
    Txid(TxId),
    /// Buy orders at/below this price, sell orders at/above this price.
    Price {
        side: Side,
        price: Decimal,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cancellation {
    pub resting: Vec<RestingOrder>,
    pub queued: Vec<Order>,
    pub reserve_quantity: Decimal,
}

/// Complete metadata export. Import is unverified, bounded and structural;
/// it proves neither balances, historical provenance nor correct positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSnapshot {
    pub contract: ContractId,
    pub queued: Vec<Order>,
    pub drained: Vec<Order>,
    pub resting: Vec<RestingOrder>,
    pub seen: Vec<TxId>,
    pub next_sequence: u64,
    pub processed_through: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractOrderbook {
    contract: ContractId,
    queued: Vec<Order>,
    drained: Vec<Order>,
    resting: Vec<RestingOrder>,
    seen: BTreeSet<TxId>,
    next_sequence: u64,
    processed_through: Option<u64>,
}

impl ContractOrderbook {
    pub fn new(contract: ContractId) -> Self {
        Self {
            contract,
            queued: Vec::new(),
            drained: Vec::new(),
            resting: Vec::new(),
            seen: BTreeSet::new(),
            next_sequence: 0,
            processed_through: None,
        }
    }
    pub fn contract(&self) -> ContractId {
        self.contract
    }
    pub fn queued(&self) -> &[Order] {
        &self.queued
    }
    pub fn resting(&self) -> &[RestingOrder] {
        &self.resting
    }
    pub fn drained(&self) -> &[Order] {
        &self.drained
    }
    pub fn processed_through(&self) -> Option<u64> {
        self.processed_through
    }
    pub fn has_seen(&self, txid: TxId) -> bool {
        self.seen.contains(&txid)
    }
    pub fn orders_for_owner<'a>(
        &'a self,
        owner: &'a AccountId,
    ) -> impl Iterator<Item = &'a RestingOrder> {
        self.resting.iter().filter(move |o| &o.order.owner == owner)
    }
    pub fn reserve_for_owner(&self, owner: &AccountId) -> Result<Decimal> {
        self.orders_for_owner(owner)
            .try_fold(Decimal::zero(), |sum, o| {
                Ok(sum.checked_add(&o.reserve_quantity)?)
            })
    }
    /// Stable price/time priority, including original insertion sequence ties.
    /// Market orders receive no additional sort priority in the actual source.
    pub fn side(&self, side: Side) -> Vec<&RestingOrder> {
        let mut orders: Vec<_> = self
            .resting
            .iter()
            .filter(|o| o.order.side == side)
            .collect();
        orders.sort_by(|a, b| priority(a, b, side));
        orders
    }
    pub fn snapshot(&self) -> BookSnapshot {
        BookSnapshot {
            contract: self.contract,
            queued: self.queued.clone(),
            drained: self.drained.clone(),
            resting: self.resting.clone(),
            seen: self.seen.iter().copied().collect(),
            next_sequence: self.next_sequence,
            processed_through: self.processed_through,
        }
    }
    pub fn from_snapshot(snapshot: BookSnapshot) -> Result<Self> {
        if snapshot.queued.len() + snapshot.drained.len() + snapshot.resting.len()
            > MAX_CONTRACT_ORDERS
            || snapshot.seen.len() > MAX_CONTRACT_ORDER_IDS
            || snapshot
                .processed_through
                .is_some_and(|h| h > MAX_ORDER_HEIGHT)
            || snapshot.seen.windows(2).any(|w| w[0] >= w[1])
            || snapshot
                .resting
                .windows(2)
                .any(|w| w[0].sequence >= w[1].sequence)
        {
            return Err(BookError::InvalidSnapshot);
        }
        let seen: BTreeSet<_> = snapshot.seen.iter().copied().collect();
        let mut live = BTreeSet::new();
        for order in snapshot
            .queued
            .iter()
            .chain(&snapshot.drained)
            .chain(snapshot.resting.iter().map(|o| &o.order))
        {
            validate_order(order, snapshot.contract)?;
            if !seen.contains(&order.txid) || !live.insert(order.txid) {
                return Err(BookError::InvalidSnapshot);
            }
        }
        if snapshot
            .queued
            .iter()
            .chain(&snapshot.drained)
            .any(|o| snapshot.processed_through.is_some_and(|h| o.height <= h))
        {
            return Err(BookError::InvalidSnapshot);
        }
        for o in &snapshot.resting {
            positive(&o.remaining)?;
            nonnegative(&o.reserve_quantity)?;
            if o.sequence >= snapshot.next_sequence
                || o.reserve_quantity > o.remaining
                || o.remaining > number(&o.order.quantity)?.max(o.order.quantity.clone())
            {
                return Err(BookError::InvalidSnapshot);
            }
        }
        Ok(Self {
            contract: snapshot.contract,
            queued: snapshot.queued,
            drained: snapshot.drained,
            resting: snapshot.resting,
            seen,
            next_sequence: snapshot.next_sequence,
            processed_through: snapshot.processed_through,
        })
    }
    fn validate_new(&self, order: &Order) -> Result<()> {
        validate_order(order, self.contract)?;
        if self.processed_through.is_some_and(|h| order.height <= h) {
            return Err(BookError::InvalidHeight);
        }
        if self.seen.contains(&order.txid) {
            return Err(BookError::DuplicateInput);
        }
        if self.queued.len() + self.drained.len() + self.resting.len() >= MAX_CONTRACT_ORDERS
            || self.seen.len() >= MAX_CONTRACT_ORDER_IDS
        {
            return Err(BookError::Capacity);
        }
        Ok(())
    }
    /// Queue admission reserves no collateral. The outer coordinator funds
    /// each ReserveIntent when processing, in the returned source order.
    pub fn queue(&mut self, order: Order) -> Result<()> {
        self.validate_new(&order)?;
        self.seen.insert(order.txid);
        self.queued.push(order);
        Ok(())
    }
    pub fn ready_orders(&self, height: u64) -> Result<Vec<Order>> {
        validate_height(height)?;
        let mut ready: Vec<_> = self
            .queued
            .iter()
            .filter(|o| o.height <= height)
            .cloned()
            .collect();
        ready.sort_by(queue_priority);
        Ok(ready)
    }
    /// Move ready orders into a pending-admission list in exact source order.
    /// `place` accepts each identical drained order once, retaining replay IDs.
    /// The complete-state coordinator must stage drain + custody + settlement.
    pub fn drain_queued_through(&mut self, height: u64) -> Result<Vec<Order>> {
        validate_height(height)?;
        if self.processed_through.is_some_and(|h| height <= h) || !self.drained.is_empty() {
            return Err(BookError::InvalidHeight);
        }
        let ready = self.ready_orders(height)?;
        self.queued.retain(|o| o.height > height);
        self.drained = ready.clone();
        Ok(ready)
    }
    /// Complete a coordinator-driven height after all ready admissions settled.
    pub fn finish_height(&mut self, height: u64) -> Result<()> {
        validate_height(height)?;
        if self.processed_through.is_some_and(|h| height <= h)
            || self
                .queued
                .iter()
                .chain(&self.drained)
                .any(|o| o.height <= height)
        {
            return Err(BookError::InvalidHeight);
        }
        self.processed_through = Some(height);
        Ok(())
    }
    /// Process one already queued order. Useful when the coordinator must
    /// settle each match before obtaining the next owner's current position.
    pub fn admit_queued(
        &mut self,
        txid: TxId,
        current_position: &Decimal,
    ) -> Result<BookTransition> {
        let mut staged = self.clone();
        let index = staged
            .queued
            .iter()
            .position(|o| o.txid == txid)
            .ok_or(BookError::InvalidSnapshot)?;
        let order = staged.queued.remove(index);
        let result = staged.insert_and_match(order, current_position)?;
        *self = staged;
        Ok(result)
    }
    pub fn place(&mut self, order: Order, current_position: &Decimal) -> Result<BookTransition> {
        let mut staged = self.clone();
        if let Some(index) = staged.drained.iter().position(|o| o.txid == order.txid) {
            if index != 0 || staged.drained[index] != order {
                return Err(BookError::DuplicateInput);
            }
            staged.drained.remove(index);
        } else {
            if !staged.drained.is_empty() {
                return Err(BookError::InvalidSnapshot);
            }
            staged.validate_new(&order)?;
            staged.seen.insert(order.txid);
        }
        let result = staged.insert_and_match(order, current_position)?;
        *self = staged;
        Ok(result)
    }
    /// Local all-or-nothing convenience projection. Fetch each initial signed
    /// position once; apply returned fills to the projected positions for later
    /// admission. The callback must be read-only: external writes cannot be
    /// rolled back by this metadata component. For custody use ready_orders +
    /// admit_queued inside the coordinator's complete-state transaction.
    pub fn process_through(
        &mut self,
        height: u64,
        mut position_provider: impl FnMut(&AccountId) -> Result<Decimal>,
    ) -> Result<BookTransition> {
        validate_height(height)?;
        if self.processed_through.is_some_and(|h| height <= h) {
            return Err(BookError::InvalidHeight);
        }
        let ready = self.ready_orders(height)?;
        let mut staged = self.clone();
        let mut positions = BTreeMap::<AccountId, Decimal>::new();
        let mut result = BookTransition::default();
        for order in ready {
            let position =
                projected_position(&mut positions, &order.owner, &mut position_provider)?;
            let part = staged.admit_queued(order.txid, &position)?;
            for fill in &part.matches {
                for (owner, change) in [
                    (&fill.buyer, fill.quantity.clone()),
                    (&fill.seller, fill.quantity.negated()),
                ] {
                    let before = projected_position(&mut positions, owner, &mut position_provider)?;
                    positions.insert(owner.clone(), number(&before.checked_add(&change)?)?);
                }
            }
            result.admitted.extend(part.admitted);
            result.matches.extend(part.matches);
            result.self_cross_removed.extend(part.self_cross_removed);
        }
        staged.processed_through = Some(height);
        *self = staged;
        Ok(result)
    }
    fn insert_and_match(
        &mut self,
        order: Order,
        current_position: &Decimal,
    ) -> Result<BookTransition> {
        let intent = reserve_quantity(&order, current_position)?;
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1).ok_or(BookError::Capacity)?;
        self.resting.push(RestingOrder {
            remaining: order.quantity.clone(),
            reserve_quantity: intent.quantity.clone(),
            initial_reduce: intent.initial_reduce,
            order,
            sequence,
        });
        let mut result = self.match_inner()?;
        result.admitted.push(intent);
        Ok(result)
    }
    pub fn match_resting(&mut self) -> Result<BookTransition> {
        let mut staged = self.clone();
        let result = staged.match_inner()?;
        *self = staged;
        Ok(result)
    }
    fn match_inner(&mut self) -> Result<BookTransition> {
        let mut result = BookTransition::default();
        // Every iteration removes at least one order, including self-crosses.
        // Unlike source recursion this is one complete-operation bound.
        for _ in 0..MAX_CONTRACT_ORDERS {
            let buy = self.best_index(Side::Buy);
            let sell = self.best_index(Side::Sell);
            let (Some(bi), Some(si)) = (buy, sell) else {
                break;
            };
            let buy = self.resting[bi].clone();
            let sell = self.resting[si].clone();
            let bm = buy.order.is_market;
            let sm = sell.order.is_market;
            if !bm && !sm && buy.order.price < sell.order.price {
                break;
            }
            let (mut buy_maker, mut sell_maker) = if bm && !sm {
                (false, true)
            } else if sm && !bm {
                (true, false)
            } else {
                (
                    buy.order.height < sell.order.height,
                    sell.order.height < buy.order.height,
                )
            };
            // This happens before same-block post flags in the source.
            if buy.order.owner == sell.order.owner {
                result
                    .self_cross_removed
                    .push(self.resting.remove(if sell_maker { si } else { bi }));
                continue;
            }
            let mut price = if bm && sm {
                buy.order.price.clone()
            } else if bm && !sm {
                sell.order.price.clone()
            } else if sm && !bm {
                buy.order.price.clone()
            } else if sell.order.height < buy.order.height {
                sell.order.price.clone()
            } else {
                buy.order.price.clone()
            };
            if !bm && !sm && buy.order.height == sell.order.height {
                if sell.order.post {
                    price = sell.order.price.clone();
                    sell_maker = true;
                    buy_maker = false;
                } else if buy.order.post {
                    price = buy.order.price.clone();
                    buy_maker = true;
                    sell_maker = false;
                } else {
                    buy_maker = false;
                    sell_maker = false;
                }
            }
            let raw_quantity = buy.remaining.clone().min(sell.remaining.clone());
            let quantity = number(&raw_quantity)?;
            let buy_remaining = number(&buy.remaining.checked_sub(&raw_quantity)?)?;
            let sell_remaining = number(&sell.remaining.checked_sub(&raw_quantity)?)?;
            let buy_reserve = buy.reserve_quantity.clone().min(buy_remaining.clone());
            let sell_reserve = sell.reserve_quantity.clone().min(sell_remaining.clone());
            result.matches.push(Match {
                contract: self.contract,
                buy_txid: buy.order.txid,
                sell_txid: sell.order.txid,
                buyer: buy.order.owner.clone(),
                seller: sell.order.owner.clone(),
                quantity,
                price,
                buy_maker,
                sell_maker,
                buy_remaining: buy_remaining.clone(),
                sell_remaining: sell_remaining.clone(),
                buy_reserve_used: buy.reserve_quantity.checked_sub(&buy_reserve)?,
                sell_reserve_used: sell.reserve_quantity.checked_sub(&sell_reserve)?,
                buy_initial_reduce: buy.initial_reduce,
                sell_initial_reduce: sell.initial_reduce,
                source_txid: if sell_maker {
                    sell.order.txid
                } else {
                    buy.order.txid
                },
            });
            self.resting[bi].remaining = buy_remaining;
            self.resting[bi].reserve_quantity = buy_reserve;
            self.resting[si].remaining = sell_remaining;
            self.resting[si].reserve_quantity = sell_reserve;
            self.resting.retain(|o| !o.remaining.is_zero());
        }
        Ok(result)
    }
    fn best_index(&self, side: Side) -> Option<usize> {
        self.resting
            .iter()
            .enumerate()
            .filter(|(_, o)| o.order.side == side)
            .min_by(|(_, a), (_, b)| priority(a, b, side))
            .map(|(i, _)| i)
    }
    /// Ownership is always checked, including txid/price criteria. Returns
    /// reserve quantity only; it cannot refund any balance. Seen IDs persist.
    pub fn cancel(&mut self, owner: &AccountId, criteria: CancelCriteria) -> Result<Cancellation> {
        validate_owner(owner)?;
        if let CancelCriteria::Price { price, .. } = &criteria {
            positive(price)?;
        }
        let mut staged = self.clone();
        let mut out = Cancellation::default();
        staged.resting.retain(|o| {
            if &o.order.owner == owner && matches_criteria(&o.order, &criteria) {
                out.resting.push(o.clone());
                false
            } else {
                true
            }
        });
        staged.queued.retain(|o| {
            if &o.owner == owner && matches_criteria(o, &criteria) {
                out.queued.push(o.clone());
                false
            } else {
                true
            }
        });
        staged.drained.retain(|o| {
            if &o.owner == owner && matches_criteria(o, &criteria) {
                out.queued.push(o.clone());
                false
            } else {
                true
            }
        });
        for o in &out.resting {
            out.reserve_quantity = out.reserve_quantity.checked_add(&o.reserve_quantity)?;
        }
        *self = staged;
        Ok(out)
    }
}

fn projected_position(
    positions: &mut BTreeMap<AccountId, Decimal>,
    owner: &AccountId,
    provider: &mut impl FnMut(&AccountId) -> Result<Decimal>,
) -> Result<Decimal> {
    if let Some(value) = positions.get(owner) {
        return Ok(value.clone());
    }
    let value = provider(owner)?;
    finite(&value)?;
    positions.insert(owner.clone(), value.clone());
    Ok(value)
}

/// Source admission reserves the full quantity for new/increasing exposure;
/// for an opposite order it reserves only the native-Number flip overshoot.
pub fn reserve_quantity(order: &Order, current_position: &Decimal) -> Result<ReserveIntent> {
    validate_order(order, order.contract)?;
    finite(current_position)?;
    let position = as_number(current_position)?;
    let reducing =
        (position < 0.0 && order.side == Side::Buy) || (position > 0.0 && order.side == Side::Sell);
    let quantity = if reducing {
        from_number((as_number(&order.quantity)? - position.abs()).max(0.0))?
    } else {
        order.quantity.clone()
    };
    Ok(ReserveIntent {
        txid: order.txid,
        owner: order.owner.clone(),
        quantity,
        initial_reduce: reducing,
    })
}

fn priority(a: &RestingOrder, b: &RestingOrder, side: Side) -> Ordering {
    let prices = if side == Side::Buy {
        b.order.price.cmp(&a.order.price)
    } else {
        a.order.price.cmp(&b.order.price)
    };
    prices
        .then(a.order.height.cmp(&b.order.height))
        .then(a.sequence.cmp(&b.sequence))
}
fn queue_priority(a: &Order, b: &Order) -> Ordering {
    a.height
        .cmp(&b.height)
        .then_with(|| compare_sender_addresses(&a.owner, &b.owner, a.txid, b.txid))
}

/// The typed owner domain is ASCII alphanumeric. The full-address fallback
/// matches the source's ordinary Latin locale ordering (primary lowercase
/// string, then lowercase-before-uppercase case weights); no Unicode/locale
/// behavior is claimed outside this explicitly bounded domain.
pub fn compare_sender_addresses(
    a: &AccountId,
    b: &AccountId,
    txid_a: TxId,
    txid_b: TxId,
) -> Ordering {
    let aa = a.as_str().as_bytes();
    let bb = b.as_str().as_bytes();
    let rank = |c: Option<&u8>| match c {
        None => (2, 0),
        Some(c) => (u8::from(c.is_ascii_digit()), c.to_ascii_lowercase()),
    };
    for index in 1..=3 {
        let order = rank(aa.len().checked_sub(index).and_then(|i| aa.get(i)))
            .cmp(&rank(bb.len().checked_sub(index).and_then(|i| bb.get(i))));
        if order != Ordering::Equal {
            return order;
        }
    }
    let lower_a = a.as_str().to_ascii_lowercase();
    let lower_b = b.as_str().to_ascii_lowercase();
    lower_a
        .cmp(&lower_b)
        .then_with(|| {
            aa.iter()
                .map(|c| c.is_ascii_uppercase())
                .cmp(bb.iter().map(|c| c.is_ascii_uppercase()))
        })
        .then(txid_a.cmp(&txid_b))
}
fn matches_criteria(order: &Order, criteria: &CancelCriteria) -> bool {
    match criteria {
        CancelCriteria::All => true,
        CancelCriteria::Txid(txid) => order.txid == *txid,
        CancelCriteria::Price { side, price } => {
            order.side == *side
                && match side {
                    Side::Buy => order.price <= *price,
                    Side::Sell => order.price >= *price,
                }
        }
    }
}
fn validate_order(order: &Order, contract: ContractId) -> Result<()> {
    if order.contract != contract {
        return Err(BookError::WrongContract);
    }
    validate_owner(&order.owner)?;
    validate_height(order.height)?;
    positive(&order.quantity)?;
    positive(&order.price)
}
fn validate_owner(owner: &AccountId) -> Result<()> {
    if !owner.as_str().bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err(BookError::InvalidOwner);
    }
    Ok(())
}
fn validate_height(height: u64) -> Result<()> {
    if height > MAX_ORDER_HEIGHT {
        Err(BookError::InvalidHeight)
    } else {
        Ok(())
    }
}
fn finite(value: &Decimal) -> Result<()> {
    if value.coefficient_digits() > MAX_ORDER_DECIMAL_DIGITS
        || value.decimal_places() > MAX_ORDER_DECIMAL_SCALE
        || !as_number(value)?.is_finite()
    {
        return Err(BookError::InvalidNumber);
    }
    Ok(())
}
fn positive(value: &Decimal) -> Result<()> {
    finite(value)?;
    if value <= &Decimal::zero() {
        Err(BookError::InvalidNumber)
    } else {
        Ok(())
    }
}
fn nonnegative(value: &Decimal) -> Result<()> {
    finite(value)?;
    if value.is_negative() {
        Err(BookError::InvalidNumber)
    } else {
        Ok(())
    }
}
fn as_number(value: &Decimal) -> Result<f64> {
    let value: f64 = value
        .to_string()
        .parse()
        .map_err(|_| BookError::InvalidNumber)?;
    if !value.is_finite() {
        return Err(BookError::InvalidNumber);
    }
    Ok(value)
}
fn from_number(value: f64) -> Result<Decimal> {
    if !value.is_finite() {
        return Err(BookError::InvalidNumber);
    }
    value.to_string().parse().map_err(Into::into)
}
fn number(value: &Decimal) -> Result<Decimal> {
    from_number(as_number(value)?)
}
