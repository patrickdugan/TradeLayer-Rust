//! Caller-custody projections and bounded fee classification metadata.
//! No Ledger or spendable account map is owned here. Fee plans require the
//! economic owner to apply and verify their actual custody legs atomically.

use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::ledger::PropertyId;
use std::collections::{BTreeMap, BTreeSet};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef";
pub const MAX_FEE_ROWS: usize = 4096;
pub const MAX_QUEUED_FEES: usize = 4096;
pub const MAX_FEE_JOURNAL: usize = 16_384;
pub const MAX_SAFE_BLOCK: u64 = 9_007_199_254_740_991;
pub const ATOMS_PER_TOKEN: u128 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TallyError {
    Arithmetic,
    AmountOverflow,
    NonFinite,
    InvalidInput,
    InvalidSnapshot,
    Capacity,
    InsufficientMetadata,
    CustodyMismatch,
}
impl From<DecimalError> for TallyError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
impl core::fmt::Display for TallyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TallyError {}
type Result<T> = core::result::Result<T, TallyError>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustodyAtoms {
    pub available: u128,
    pub reserved: u128,
    pub margin: u128,
    pub channel: u128,
    /// Non-spendable entitlement; never added to owned token supply here.
    pub vesting: Decimal,
}
impl CustodyAtoms {
    pub fn owned_atoms(&self) -> Result<u128> {
        self.available
            .checked_add(self.reserved)
            .and_then(|n| n.checked_add(self.margin))
            .and_then(|n| n.checked_add(self.channel))
            .ok_or(TallyError::AmountOverflow)
    }
    pub fn sufficient(&self, bucket: BalanceBucket, required_atoms: u128) -> Sufficiency {
        let available = match bucket {
            BalanceBucket::Available => self.available,
            BalanceBucket::Reserved => self.reserved,
            BalanceBucket::Margin => self.margin,
            BalanceBucket::Channel => self.channel,
        };
        Sufficiency {
            has_sufficient: available >= required_atoms,
            shortfall_atoms: required_atoms.saturating_sub(available),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceBucket {
    Available,
    Reserved,
    Margin,
    Channel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sufficiency {
    pub has_sufficient: bool,
    pub shortfall_atoms: u128,
}

/// Aggregate a caller's actual custody projection without storing balances.
pub fn aggregate_custody<'a>(
    rows: impl IntoIterator<Item = &'a CustodyAtoms>,
) -> Result<CustodyAtoms> {
    let mut total = CustodyAtoms::default();
    for row in rows {
        if row.vesting.is_negative() {
            return Err(TallyError::InvalidInput);
        }
        row.owned_atoms()?;
        total.available = total
            .available
            .checked_add(row.available)
            .ok_or(TallyError::AmountOverflow)?;
        total.reserved = total
            .reserved
            .checked_add(row.reserved)
            .ok_or(TallyError::AmountOverflow)?;
        total.margin = total
            .margin
            .checked_add(row.margin)
            .ok_or(TallyError::AmountOverflow)?;
        total.channel = total
            .channel
            .checked_add(row.channel)
            .ok_or(TallyError::AmountOverflow)?;
        total.vesting = total.vesting.checked_add(&row.vesting)?;
    }
    total.owned_atoms()?;
    Ok(total)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyBuckets {
    pub available: Decimal,
    pub reserved: Decimal,
    pub margin: Decimal,
    pub vesting: Decimal,
    pub channel: Decimal,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyTally {
    pub amount: Decimal,
    pub buckets: LegacyBuckets,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyMutation {
    pub after: LegacyTally,
    pub rejected_bucket: Option<BalanceBucket>,
    pub vesting_rejected: bool,
    pub delta_emitted: bool,
}

pub fn source_calculate_total(b: &LegacyBuckets) -> Result<Decimal> {
    number(
        &q8(&b.available)?
            .checked_add(&q8(&b.reserved)?)?
            .checked_add(&q8(&b.margin)?)?
            .checked_add(&q8(&b.channel)?)?,
    )
}
pub fn source_total_for_property(rows: &[LegacyBuckets]) -> Result<Decimal> {
    let mut total = Decimal::zero();
    for b in rows {
        total = total
            .checked_add(&b.available)?
            .checked_add(&b.reserved)?
            .checked_add(&b.margin)?
            .checked_add(&b.channel)?;
    }
    Ok(total)
}
pub fn source_total_tally(rows: &[LegacyTally]) -> Result<LegacyTally> {
    let mut out = LegacyTally::default();
    for row in rows {
        out.amount = out.amount.checked_add(&row.amount)?;
        out.buckets.available = out.buckets.available.checked_add(&row.buckets.available)?;
        out.buckets.reserved = out.buckets.reserved.checked_add(&row.buckets.reserved)?;
        out.buckets.margin = out.buckets.margin.checked_add(&row.buckets.margin)?;
        out.buckets.vesting = out.buckets.vesting.checked_add(&row.buckets.vesting)?;
        out.buckets.channel = out.buckets.channel.checked_add(&row.buckets.channel)?;
    }
    out.amount = number(&q8(&out.amount)?)?;
    out.buckets.available = number(&q8(&out.buckets.available)?)?;
    out.buckets.reserved = number(&q8(&out.buckets.reserved)?)?;
    out.buckets.margin = number(&q8(&out.buckets.margin)?)?;
    out.buckets.vesting = number(&q8(&out.buckets.vesting)?)?;
    out.buckets.channel = number(&q8(&out.buckets.channel)?)?;
    Ok(out)
}
pub fn source_total_tokens(rows: &[LegacyBuckets]) -> Result<Decimal> {
    let mut total = 0.0;
    for row in rows {
        total += as_f64(&row.available)? + as_f64(&row.reserved)?;
    }
    from_f64(total)
}

/// Finite source projection, including partial writes before a later negative
/// bucket check. This returns an observation; it never changes caller custody.
pub fn source_update_projection(
    before: &LegacyTally,
    change: &LegacyBuckets,
) -> Result<LegacyMutation> {
    let mut after = before.clone();
    let changes = [
        number(&change.available)?,
        number(&change.reserved)?,
        number(&change.margin)?,
        number(&change.vesting)?,
    ];
    for (index, value) in changes.iter().enumerate() {
        let old = match index {
            0 => &mut after.buckets.available,
            1 => &mut after.buckets.reserved,
            2 => &mut after.buckets.margin,
            _ => &mut after.buckets.vesting,
        };
        let new = old.checked_add(value)?;
        if new.is_negative() {
            return Ok(LegacyMutation {
                after,
                rejected_bucket: match index {
                    0 => Some(BalanceBucket::Available),
                    1 => Some(BalanceBucket::Reserved),
                    2 => Some(BalanceBucket::Margin),
                    _ => None,
                },
                vesting_rejected: index == 3,
                delta_emitted: false,
            });
        }
        *old = number(&new)?;
    }
    after.amount = source_calculate_total(&after.buckets)?;
    Ok(LegacyMutation {
        after,
        rejected_bucket: None,
        vesting_rejected: false,
        delta_emitted: changes.iter().any(|n| !n.is_zero()),
    })
}

/// Source channel mutation for a finite numeric change. Unlike updateBalance,
/// the source does not first convert this change through Number. The returned
/// projection remains an observation, not a mutation of the economic ledger.
pub fn source_update_channel_projection(
    before: &LegacyTally,
    change: &Decimal,
) -> Result<LegacyMutation> {
    let mut after = before.clone();
    let value = before.buckets.channel.checked_add(change)?;
    if value.is_negative() {
        return Ok(LegacyMutation {
            after,
            rejected_bucket: Some(BalanceBucket::Channel),
            vesting_rejected: false,
            delta_emitted: false,
        });
    }
    after.buckets.channel = number(&value)?;
    after.amount = source_calculate_total(&after.buckets)?;
    Ok(LegacyMutation {
        after,
        rejected_bucket: None,
        vesting_rejected: false,
        delta_emitted: !change.is_zero(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MakerFlag {
    True,
    False,
    Missing,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchFeeInput {
    pub amount_buy: Decimal,
    pub amount_sell: Decimal,
    pub buy_maker: MakerFlag,
    pub sell_maker: MakerFlag,
    pub inverse: bool,
    pub last_mark: Decimal,
    pub notional_value: Decimal,
    pub channel: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignedAtoms {
    Debit(u128),
    Credit(u128),
}
impl SignedAtoms {
    pub const fn debit_atoms(self) -> u128 {
        match self {
            Self::Debit(n) => n,
            Self::Credit(_) => 0,
        }
    }
    pub const fn credit_atoms(self) -> u128 {
        match self {
            Self::Credit(n) => n,
            Self::Debit(_) => 0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchFeePlan {
    pub buyer: SignedAtoms,
    pub seller: SignedAtoms,
    pub retained_atoms: u128,
}

/// Source orderbook.calculateFee, including missing/both-maker flag behavior.
/// Comments in JS disagree with its concrete 5bps formula; code wins here.
pub fn calculate_match_fees(input: &MatchFeeInput) -> Result<MatchFeePlan> {
    let rate = if input.channel {
        d("0.00005")
    } else {
        d("0.0005")
    };
    let neither = input.buy_maker != MakerFlag::True && input.sell_maker != MakerFlag::True;
    let buyer_taker = input.buy_maker == MakerFlag::False;
    let amount = if neither || buyer_taker {
        &input.amount_buy
    } else {
        &input.amount_sell
    };
    let raw = if input.inverse {
        div(&rate.checked_mul(&input.notional_value)?, &input.last_mark)?.checked_mul(amount)?
    } else {
        div(&rate.checked_mul(&input.last_mark)?, &input.notional_value)?.checked_mul(amount)?
    }
    .abs();
    let mut atoms = integer(&raw.checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?)?;
    if atoms % 2 != 0 {
        atoms = atoms.checked_add(1).ok_or(TallyError::AmountOverflow)?;
    }
    let half = atoms / 2;
    if neither {
        return Ok(MatchFeePlan {
            buyer: SignedAtoms::Debit(half),
            seller: SignedAtoms::Debit(half),
            retained_atoms: atoms,
        });
    }
    let rebate = if half == 0 {
        SignedAtoms::Debit(0)
    } else {
        SignedAtoms::Credit(half)
    };
    Ok(MatchFeePlan {
        buyer: if buyer_taker {
            SignedAtoms::Debit(atoms)
        } else {
            rebate
        },
        seller: if buyer_taker {
            rebate
        } else {
            SignedAtoms::Debit(atoms)
        },
        retained_atoms: half,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeConversion {
    pub atoms: u128,
    pub fractional_sats: Decimal,
    pub source_discards_all: bool,
}
/// Source accrueFee's conversion. The source discards sub-one-sat inputs before
/// dust handling. Fractional observations are never additional funded atoms.
pub fn source_fee_conversion(amount: &Decimal) -> Result<FeeConversion> {
    let raw = amount.checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?;
    if raw <= Decimal::zero() {
        return Ok(FeeConversion {
            atoms: 0,
            fractional_sats: raw,
            source_discards_all: true,
        });
    }
    let whole = raw.quantize(0, Rounding::Down)?;
    let atoms = integer(&whole)?;
    Ok(FeeConversion {
        atoms,
        fractional_sats: raw.checked_sub(&whole)?,
        source_discards_all: atoms == 0,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FeeKey {
    pub property: PropertyId,
    pub contract: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeeRoute {
    Spot,
    Native(u32),
    Oracle(u32),
}
impl FeeRoute {
    pub const fn contract(self) -> u32 {
        match self {
            Self::Spot => 1,
            Self::Native(id) | Self::Oracle(id) => id,
        }
    }
    pub fn key(self, property: PropertyId) -> Result<FeeKey> {
        if self.contract() == 0 {
            return Err(TallyError::InvalidInput);
        }
        Ok(FeeKey {
            property,
            contract: self.contract(),
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeeRow {
    pub value_atoms: u128,
    pub stash_atoms: u128,
}
impl FeeRow {
    pub fn total(self) -> Result<u128> {
        self.value_atoms
            .checked_add(self.stash_atoms)
            .ok_or(TallyError::AmountOverflow)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedFee {
    pub block: u64,
    pub key: FeeKey,
    pub route: FeeRoute,
    pub atoms: u128,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeAccrualPlan {
    pub block: u64,
    pub key: FeeKey,
    pub route: FeeRoute,
    pub amount_atoms: u128,
    pub collection_atoms: u128,
    pub value_added_atoms: u128,
    pub stash_added_atoms: u128,
    pub insurance_atoms: u128,
    pub custody_before_atoms: u128,
    pub custody_after_atoms: u128,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeQueuePlan {
    pub entry: QueuedFee,
    pub custody_before_atoms: u128,
    pub custody_after_atoms: u128,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeDrawPlan {
    pub block: u64,
    pub key: FeeKey,
    pub value_atoms: u128,
    pub stash_atoms: u128,
    pub custody_before_atoms: u128,
    pub custody_after_atoms: u128,
}
impl FeeDrawPlan {
    pub fn spent_atoms(self) -> Result<u128> {
        self.value_atoms
            .checked_add(self.stash_atoms)
            .ok_or(TallyError::AmountOverflow)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeJournalEntry {
    Accrued(FeeAccrualPlan),
    Queued(FeeQueuePlan),
    Drawn(FeeDrawPlan),
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeeBookSnapshot {
    pub rows: Vec<(FeeKey, FeeRow)>,
    pub queued: Vec<QueuedFee>,
    pub journal: Vec<FeeJournalEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeeBook {
    rows: BTreeMap<FeeKey, FeeRow>,
    queued: BTreeMap<u64, Vec<QueuedFee>>,
    journal: Vec<FeeJournalEntry>,
}
impl FeeBook {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn rows(&self) -> impl Iterator<Item = (FeeKey, FeeRow)> + '_ {
        self.rows.iter().map(|(k, v)| (*k, *v))
    }
    pub fn queued(&self) -> impl Iterator<Item = &QueuedFee> {
        self.queued.values().flat_map(|rows| rows.iter())
    }
    pub fn journal(&self) -> &[FeeJournalEntry] {
        &self.journal
    }
    pub fn take_journal(&mut self) -> Vec<FeeJournalEntry> {
        core::mem::take(&mut self.journal)
    }
    pub fn row(&self, key: FeeKey) -> FeeRow {
        self.rows.get(&key).copied().unwrap_or_default()
    }
    pub fn expected_custody_atoms(&self, key: FeeKey) -> Result<u128> {
        self.queued()
            .filter(|q| q.key == key)
            .try_fold(self.row(key).total()?, |sum, q| {
                sum.checked_add(q.atoms).ok_or(TallyError::AmountOverflow)
            })
    }
    pub fn expected_custody(&self) -> Result<Vec<(FeeKey, u128)>> {
        let keys: BTreeSet<_> = self
            .rows
            .keys()
            .copied()
            .chain(self.queued().map(|q| q.key))
            .collect();
        keys.into_iter()
            .map(|key| Ok((key, self.expected_custody_atoms(key)?)))
            .collect()
    }
    pub fn verify_custody(&self, actual: impl IntoIterator<Item = (FeeKey, u128)>) -> Result<()> {
        let mut supplied = BTreeMap::new();
        for (key, atoms) in actual {
            if supplied.len() >= MAX_FEE_ROWS + MAX_QUEUED_FEES {
                return Err(TallyError::Capacity);
            }
            if key.contract == 0 || supplied.insert(key, atoms).is_some() {
                return Err(TallyError::CustodyMismatch);
            }
        }
        for (key, atoms) in self.expected_custody()? {
            if supplied.remove(&key).unwrap_or(0) != atoms {
                return Err(TallyError::CustodyMismatch);
            }
        }
        if supplied.values().any(|n| *n != 0) {
            return Err(TallyError::CustodyMismatch);
        }
        Ok(())
    }
    pub fn snapshot(&self) -> FeeBookSnapshot {
        FeeBookSnapshot {
            rows: self.rows().collect(),
            queued: self.queued().copied().collect(),
            journal: self.journal.clone(),
        }
    }
    pub fn from_snapshot(s: FeeBookSnapshot) -> Result<Self> {
        if s.rows.len() > MAX_FEE_ROWS
            || s.queued.len() > MAX_QUEUED_FEES
            || s.journal.len() > MAX_FEE_JOURNAL
        {
            return Err(TallyError::Capacity);
        }
        if s.rows.windows(2).any(|w| w[0].0 >= w[1].0)
            || s.queued.windows(2).any(|w| w[0].block > w[1].block)
        {
            return Err(TallyError::InvalidSnapshot);
        }
        let mut out = Self::new();
        for (key, row) in s.rows {
            valid_key(key)?;
            row.total()?;
            out.rows.insert(key, row);
        }
        let mut seen = BTreeSet::new();
        for q in s.queued {
            valid_block(q.block)?;
            if q.atoms == 0
                || q.route.key(q.key.property)? != q.key
                || !seen.insert((q.block, q.key, q.route))
            {
                return Err(TallyError::InvalidSnapshot);
            }
            out.queued.entry(q.block).or_default().push(q);
        }
        // Journal is informational, but imported records must be internally
        // arithmetic-consistent before the combined owner persists them.
        for entry in &s.journal {
            validate_journal(*entry)?;
        }
        out.journal = s.journal;
        out.expected_custody()?;
        Ok(out)
    }
    fn event(&mut self, event: FeeJournalEntry) -> Result<()> {
        if self.journal.len() >= MAX_FEE_JOURNAL {
            return Err(TallyError::Capacity);
        }
        self.journal.push(event);
        Ok(())
    }
    fn ensure_row(&mut self, key: FeeKey) -> Result<()> {
        valid_key(key)?;
        if !self.rows.contains_key(&key) && self.rows.len() >= MAX_FEE_ROWS {
            return Err(TallyError::Capacity);
        }
        self.rows.entry(key).or_default();
        Ok(())
    }
    pub fn accrue_atoms(
        &mut self,
        property: PropertyId,
        route: FeeRoute,
        atoms: u128,
        block: u64,
    ) -> Result<FeeAccrualPlan> {
        let mut next = self.clone();
        let plan = next.accrue_inner(property, route, atoms, block, atoms)?;
        *self = next;
        Ok(plan)
    }
    fn accrue_inner(
        &mut self,
        property: PropertyId,
        route: FeeRoute,
        atoms: u128,
        block: u64,
        collection_atoms: u128,
    ) -> Result<FeeAccrualPlan> {
        valid_block(block)?;
        let key = route.key(property)?;
        let before = self
            .expected_custody_atoms(key)?
            .checked_add(if collection_atoms == 0 { atoms } else { 0 })
            .ok_or(TallyError::AmountOverflow)?;
        let (value, stash, insurance) = split_fee(property, route, atoms);
        if atoms != 0 {
            self.ensure_row(key)?;
            let row = self.rows.get_mut(&key).expect("created");
            row.value_atoms = row
                .value_atoms
                .checked_add(value)
                .ok_or(TallyError::AmountOverflow)?;
            row.stash_atoms = row
                .stash_atoms
                .checked_add(stash)
                .ok_or(TallyError::AmountOverflow)?;
        }
        let after = self.expected_custody_atoms(key)?;
        if before
            .checked_add(collection_atoms)
            .and_then(|n| n.checked_sub(insurance))
            != Some(after)
        {
            return Err(TallyError::AmountOverflow);
        }
        let plan = FeeAccrualPlan {
            block,
            key,
            route,
            amount_atoms: atoms,
            collection_atoms,
            value_added_atoms: value,
            stash_added_atoms: stash,
            insurance_atoms: insurance,
            custody_before_atoms: before,
            custody_after_atoms: after,
        };
        if atoms != 0 {
            self.event(FeeJournalEntry::Accrued(plan))?;
        }
        Ok(plan)
    }
    /// Explicit repair: retain Spot versus Contract identity through buffering.
    /// The parent must fund the returned queued amount once, before commitment.
    pub fn queue_repaired(
        &mut self,
        property: PropertyId,
        route: FeeRoute,
        atoms: u128,
        block: u64,
    ) -> Result<FeeQueuePlan> {
        valid_block(block)?;
        let key = route.key(property)?;
        let mut next = self.clone();
        let before = next.expected_custody_atoms(key)?;
        if atoms != 0 {
            let existing = next
                .queued
                .get_mut(&block)
                .and_then(|q| q.iter_mut().find(|q| q.key == key && q.route == route));
            if let Some(q) = existing {
                q.atoms = q
                    .atoms
                    .checked_add(atoms)
                    .ok_or(TallyError::AmountOverflow)?;
            } else {
                if next.queued().count() >= MAX_QUEUED_FEES {
                    return Err(TallyError::Capacity);
                }
                next.queued.entry(block).or_default().push(QueuedFee {
                    block,
                    key,
                    route,
                    atoms,
                });
            }
        }
        let plan = FeeQueuePlan {
            entry: QueuedFee {
                block,
                key,
                route,
                atoms,
            },
            custody_before_atoms: before,
            custody_after_atoms: next.expected_custody_atoms(key)?,
        };
        if atoms != 0 {
            next.event(FeeJournalEntry::Queued(plan))?;
        }
        *self = next;
        Ok(plan)
    }
    /// All-or-nothing metadata flush. Plans have collection_atoms=0 because
    /// queue funding already occurred; only insurance legs leave fee custody.
    pub fn flush_repaired(&mut self, block: u64) -> Result<Vec<FeeAccrualPlan>> {
        valid_block(block)?;
        let mut next = self.clone();
        let entries = next.queued.get(&block).cloned().unwrap_or_default();
        let mut plans = Vec::with_capacity(entries.len());
        for q in entries {
            let pending = next.queued.get_mut(&block).expect("cloned pending block");
            pending.remove(0);
            if pending.is_empty() {
                next.queued.remove(&block);
            }
            plans.push(next.accrue_inner(q.key.property, q.route, q.atoms, block, 0)?);
        }
        *self = next;
        Ok(plans)
    }
    /// Literal source VALUE-first behavior: STASH is used only when VALUE is
    /// zero, rather than topping up a partial VALUE draw.
    pub fn draw_source_order(
        &mut self,
        key: FeeKey,
        max: Option<u128>,
        allow_stash: bool,
        block: u64,
    ) -> Result<FeeDrawPlan> {
        valid_key(key)?;
        valid_block(block)?;
        let mut next = self.clone();
        let before = next.expected_custody_atoms(key)?;
        let row = next.row(key);
        let (value, stash) = if row.value_atoms != 0 {
            (max.unwrap_or(row.value_atoms).min(row.value_atoms), 0)
        } else if allow_stash {
            (0, max.unwrap_or(row.stash_atoms).min(row.stash_atoms))
        } else {
            (0, 0)
        };
        if value != 0 || stash != 0 {
            let row = next.rows.get_mut(&key).expect("positive row");
            row.value_atoms -= value;
            row.stash_atoms -= stash;
        }
        let plan = FeeDrawPlan {
            block,
            key,
            value_atoms: value,
            stash_atoms: stash,
            custody_before_atoms: before,
            custody_after_atoms: next.expected_custody_atoms(key)?,
        };
        if value != 0 || stash != 0 {
            next.event(FeeJournalEntry::Drawn(plan))?;
        }
        *self = next;
        Ok(plan)
    }
}

fn validate_journal(entry: FeeJournalEntry) -> Result<()> {
    match entry {
        FeeJournalEntry::Queued(q) => {
            valid_block(q.entry.block)?;
            valid_key(q.entry.key)?;
            if q.entry.atoms == 0
                || q.entry.route.key(q.entry.key.property)? != q.entry.key
                || q.custody_before_atoms.checked_add(q.entry.atoms) != Some(q.custody_after_atoms)
            {
                return Err(TallyError::InvalidSnapshot);
            }
        }
        FeeJournalEntry::Accrued(p) => {
            valid_block(p.block)?;
            valid_key(p.key)?;
            let (value, stash, insurance) = split_fee(p.key.property, p.route, p.amount_atoms);
            if p.amount_atoms == 0
                || p.route.key(p.key.property)? != p.key
                || p.value_added_atoms
                    .checked_add(p.stash_added_atoms)
                    .and_then(|n| n.checked_add(p.insurance_atoms))
                    != Some(p.amount_atoms)
                || ![0, p.amount_atoms].contains(&p.collection_atoms)
                || (p.value_added_atoms, p.stash_added_atoms, p.insurance_atoms)
                    != (value, stash, insurance)
                || p.custody_before_atoms
                    .checked_add(p.collection_atoms)
                    .and_then(|n| n.checked_sub(p.insurance_atoms))
                    != Some(p.custody_after_atoms)
            {
                return Err(TallyError::InvalidSnapshot);
            }
        }
        FeeJournalEntry::Drawn(p) => {
            valid_block(p.block)?;
            valid_key(p.key)?;
            if p.custody_before_atoms.checked_sub(p.spent_atoms()?) != Some(p.custody_after_atoms) {
                return Err(TallyError::InvalidSnapshot);
            }
        }
    }
    Ok(())
}
fn split_fee(property: PropertyId, route: FeeRoute, atoms: u128) -> (u128, u128, u128) {
    match route {
        FeeRoute::Native(_) => (atoms, 0, 0),
        FeeRoute::Spot if property.get() != 1 => (atoms, 0, 0),
        _ => (0, atoms - atoms / 2, atoms / 2),
    }
}
fn valid_key(key: FeeKey) -> Result<()> {
    if key.contract == 0 {
        Err(TallyError::InvalidInput)
    } else {
        Ok(())
    }
}
fn valid_block(block: u64) -> Result<()> {
    if block > MAX_SAFE_BLOCK {
        Err(TallyError::InvalidInput)
    } else {
        Ok(())
    }
}
fn integer(value: &Decimal) -> Result<u128> {
    value
        .quantize(0, Rounding::Down)?
        .to_string()
        .parse()
        .map_err(|_| TallyError::AmountOverflow)
}
fn div(a: &Decimal, b: &Decimal) -> Result<Decimal> {
    Ok(a.checked_div(b, 20, Rounding::HalfUp)?)
}
fn q8(a: &Decimal) -> Result<Decimal> {
    Ok(a.quantize(8, Rounding::HalfUp)?)
}
fn d(s: &str) -> Decimal {
    s.parse().expect("finite source constant")
}
fn as_f64(a: &Decimal) -> Result<f64> {
    let n = a
        .to_string()
        .parse::<f64>()
        .map_err(|_| TallyError::NonFinite)?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(TallyError::NonFinite)
    }
}
fn from_f64(n: f64) -> Result<Decimal> {
    if !n.is_finite() {
        return Err(TallyError::NonFinite);
    }
    n.to_string().parse().map_err(Into::into)
}
fn number(a: &Decimal) -> Result<Decimal> {
    from_f64(as_f64(a)?)
}
