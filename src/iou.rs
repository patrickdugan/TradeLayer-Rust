//! Stateful, offline port of pinned `iou.js` bucket and claim arithmetic.
//!
//! Inputs are caller supplied and unauthenticated. Payouts reduce claims and
//! return proposed allocations; they do not credit collateral or own tokens.
//! Source quirks are deliberate: profits reduce bucket amount, apply-to-losers
//! increases a negative deficit, and payouts never consume the block-loss pool.

use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::ledger::AccountId;
use std::collections::{BTreeMap, BTreeSet};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "7ad5e87e4d72cf57c640c8c15564a3da21dfb4f2e692097ac8aab8215018cf96";
pub const MAX_IOU_RECORDS: usize = 4096;
pub const MAX_IOU_CLAIMS: usize = 16_384;
pub const MAX_JS_SAFE_BLOCK: u64 = 9_007_199_254_740_991;
const JS_DIVISION_PLACES: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IouKey {
    pub contract_id: u32,
    pub property_id: u32,
}

impl IouKey {
    pub const fn new(contract_id: u32, property_id: u32) -> Self {
        Self {
            contract_id,
            property_id,
        }
    }

    pub fn bucket_key(self) -> String {
        format!("{}:{}", self.contract_id, self.property_id)
    }

    pub fn claim_key(self) -> String {
        format!("claims:{}:{}", self.contract_id, self.property_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    pub key: IouKey,
    pub amount: Decimal,
    pub last_block: u64,
    pub block_start_amount: Decimal,
    pub block_delta: Decimal,
    pub block_losses: Decimal,
    pub block_profits: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub address: AccountId,
    pub amount: Decimal,
}

/// Claim order follows JavaScript Object.entries: canonical array-index keys
/// ascend numerically; all remaining keys retain their insertion order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimMap {
    pub key: IouKey,
    pub last_block: u64,
    claims: Vec<Claim>,
}

impl ClaimMap {
    pub fn claims(&self) -> &[Claim] {
        &self.claims
    }

    pub fn claim(&self, address: &AccountId) -> Option<&Decimal> {
        self.claims
            .iter()
            .find(|claim| &claim.address == address)
            .map(|claim| &claim.amount)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    pub address: AccountId,
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IouError {
    Decimal(DecimalError),
    TooManyRecords,
    TooManyClaims,
    InvalidBlock,
    ReservedAddress,
    DuplicateClaimant,
}

impl core::fmt::Display for IouError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IouError {}
impl From<DecimalError> for IouError {
    fn from(error: DecimalError) -> Self {
        Self::Decimal(error)
    }
}

/// Bounded in-memory state. Every fallible operation computes a complete local
/// replacement before writing any state, including claim-count bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IouStore {
    buckets: BTreeMap<IouKey, Bucket>,
    claim_maps: BTreeMap<IouKey, ClaimMap>,
    claim_count: usize,
}

impl IouStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bucket(&self, key: IouKey) -> Option<&Bucket> {
        self.buckets.get(&key)
    }

    pub fn claim_map(&self, key: IouKey) -> Option<&ClaimMap> {
        self.claim_maps.get(&key)
    }

    pub fn buckets(&self) -> impl Iterator<Item = &Bucket> {
        self.buckets.values()
    }

    pub fn claim_maps(&self) -> impl Iterator<Item = &ClaimMap> {
        self.claim_maps.values()
    }

    pub fn record_count(&self) -> usize {
        self.buckets.len() + self.claim_maps.len()
    }

    pub fn claim_count(&self) -> usize {
        self.claim_count
    }

    /// Missing bucket is zero, matching source get(). No database-failure
    /// simulation is performed by this in-memory implementation.
    pub fn get(&self, key: IouKey) -> Decimal {
        self.buckets
            .get(&key)
            .map_or_else(zero, |bucket| bucket.amount.clone())
    }

    pub fn add_loss(
        &mut self,
        key: IouKey,
        amount: &Decimal,
        block: u64,
    ) -> Result<Option<Bucket>, IouError> {
        if amount <= &zero() {
            return Ok(None);
        }
        self.change_bucket(key, amount, block, false).map(Some)
    }

    pub fn add_profit(
        &mut self,
        key: IouKey,
        amount: &Decimal,
        block: u64,
    ) -> Result<Option<Bucket>, IouError> {
        if amount <= &zero() {
            return Ok(None);
        }
        self.change_bucket(key, amount, block, true).map(Some)
    }

    pub fn add_delta(
        &mut self,
        key: IouKey,
        delta: &Decimal,
        block: u64,
    ) -> Result<Option<Bucket>, IouError> {
        if delta.is_zero() {
            Ok(None)
        } else if delta.is_negative() {
            self.add_profit(key, &delta.abs(), block)
        } else {
            self.add_loss(key, delta, block)
        }
    }

    fn change_bucket(
        &mut self,
        key: IouKey,
        amount: &Decimal,
        block: u64,
        profit: bool,
    ) -> Result<Bucket, IouError> {
        check_block(block)?;
        let mut next = match self.buckets.get(&key) {
            Some(bucket) => bucket.clone(),
            None => {
                self.check_new_record()?;
                Bucket {
                    key,
                    amount: zero(),
                    last_block: block,
                    block_start_amount: zero(),
                    block_delta: zero(),
                    block_losses: zero(),
                    block_profits: zero(),
                }
            }
        };
        // The source compares block equality, not monotonicity. An older
        // block resets counters too; no reorg authority is inferred here.
        if next.last_block != block {
            next.block_start_amount = next.amount.clone();
            next.block_delta = zero();
            next.block_losses = zero();
            next.block_profits = zero();
        }
        if profit {
            next.amount = next.amount.checked_sub(amount)?;
            next.block_delta = next.block_delta.checked_sub(amount)?;
            next.block_profits = next.block_profits.checked_add(amount)?;
        } else {
            next.amount = next.amount.checked_add(amount)?;
            next.block_delta = next.block_delta.checked_add(amount)?;
            next.block_losses = next.block_losses.checked_add(amount)?;
        }
        // BigNumber.toString(10), used by every source bucket field write,
        // rounds to DECIMAL_PLACES even though plain toString() would not.
        next.amount = persist_decimal(&next.amount)?;
        next.block_start_amount = persist_decimal(&next.block_start_amount)?;
        next.block_delta = persist_decimal(&next.block_delta)?;
        next.block_losses = persist_decimal(&next.block_losses)?;
        next.block_profits = persist_decimal(&next.block_profits)?;
        next.last_block = block;
        self.buckets.insert(key, next.clone());
        Ok(next)
    }

    /// Source-exact sign: calls addDelta(-min(markDelta, abs(deficit))).
    /// A negative bucket therefore becomes more negative. This preserves an
    /// observed source behavior and does not endorse its economic intent.
    pub fn apply_to_losers(
        &mut self,
        key: IouKey,
        mark_delta: &Decimal,
        block: u64,
    ) -> Result<Option<Bucket>, IouError> {
        let deficit = self.get(key);
        if !deficit.is_negative() || mark_delta <= &zero() {
            return Ok(None);
        }
        let payout = mark_delta.clone().min(deficit.abs());
        self.add_delta(key, &payout.negated(), block)
    }

    /// Positive delta is split by the positive buyer/seller PnL fractions.
    /// Division occurs BEFORE multiplication, with BigNumber's default 20
    /// places / ROUND_HALF_UP. The two independently rounded shares need not
    /// sum to delta. If addresses match, the buyer update precedes seller.
    #[allow(clippy::too_many_arguments)]
    pub fn add_iou_claims(
        &mut self,
        key: IouKey,
        block: u64,
        buyer: &AccountId,
        seller: &AccountId,
        buyer_pnl: &Decimal,
        seller_pnl: &Decimal,
        delta: &Decimal,
    ) -> Result<Option<ClaimMap>, IouError> {
        if delta <= &zero() {
            return Ok(None);
        }
        let positive_buyer = buyer_pnl.clone().max(zero());
        let positive_seller = seller_pnl.clone().max(zero());
        let sum = positive_buyer.checked_add(&positive_seller)?;
        if sum.is_zero() {
            return Ok(None);
        }
        check_block(block)?;
        let buyer_share = accrual_share(delta, &positive_buyer, &sum)?;
        let seller_share = accrual_share(delta, &positive_seller, &sum)?;
        let mut next = self.claim_maps.get(&key).cloned().unwrap_or(ClaimMap {
            key,
            last_block: block,
            claims: Vec::new(),
        });
        add_claim(&mut next.claims, buyer, buyer_share)?;
        add_claim(&mut next.claims, seller, seller_share)?;
        order_claims(&mut next.claims);
        next.last_block = block;
        self.install_claim_map(next.clone())?;
        Ok(Some(next))
    }

    /// Typed counterpart to source saveClaimMap. Zero/negative stored claims
    /// are allowed because the payout source filters them, but duplicate keys
    /// and inherited Object.prototype names are outside this typed domain.
    pub fn save_claim_map(
        &mut self,
        key: IouKey,
        last_block: u64,
        mut claims: Vec<Claim>,
    ) -> Result<(), IouError> {
        check_block(last_block)?;
        if claims.len() > MAX_IOU_CLAIMS {
            return Err(IouError::TooManyClaims);
        }
        let mut seen = BTreeSet::new();
        for claim in &claims {
            check_address(&claim.address)?;
            if !seen.insert(claim.address.clone()) {
                return Err(IouError::DuplicateClaimant);
            }
        }
        order_claims(&mut claims);
        self.install_claim_map(ClaimMap {
            key,
            last_block,
            claims,
        })
    }

    fn install_claim_map(&mut self, mut next: ClaimMap) -> Result<(), IouError> {
        let previous_count = match self.claim_maps.get(&next.key) {
            Some(previous) => previous.claims.len(),
            None => {
                self.check_new_record()?;
                0
            }
        };
        let total = self.claim_count - previous_count + next.claims.len();
        if total > MAX_IOU_CLAIMS {
            return Err(IouError::TooManyClaims);
        }
        order_claims(&mut next.claims);
        self.claim_maps.insert(next.key, next);
        self.claim_count = total;
        Ok(())
    }

    /// Returns proposed payments and atomically reduces claim records.
    /// The pool is min(markDelta, THIS block's losses, positive claims).
    /// Payout rounding multiplies BEFORE 20-place division, unlike accrual.
    /// Source does not subtract the payout from amount/blockLosses, so repeated
    /// same-block calls reuse the pool while positive claims remain.
    pub fn pay_outstanding_ious(
        &mut self,
        key: IouKey,
        mark_delta: &Decimal,
        block: u64,
    ) -> Result<Vec<Allocation>, IouError> {
        if mark_delta <= &zero() {
            return Ok(Vec::new());
        }
        check_block(block)?;
        let bucket = match self.buckets.get(&key) {
            Some(bucket) if bucket.last_block == block && bucket.block_losses > zero() => bucket,
            _ => return Ok(Vec::new()),
        };
        let mut next = match self.claim_maps.get(&key) {
            Some(claims) => claims.clone(),
            None => return Ok(Vec::new()),
        };
        let mut total = zero();
        for claim in &next.claims {
            if claim.amount > zero() {
                total = total.checked_add(&claim.amount)?;
            }
        }
        if total.is_zero() {
            return Ok(Vec::new());
        }
        let payout = mark_delta
            .clone()
            .min(bucket.block_losses.clone())
            .min(total.clone());
        let mut allocations = Vec::new();
        let mut removed = BTreeSet::new();
        for claim in &mut next.claims {
            if claim.amount > zero() {
                let share = payout.checked_mul(&claim.amount)?.checked_div(
                    &total,
                    JS_DIVISION_PLACES,
                    Rounding::HalfUp,
                )?;
                let remaining = claim.amount.checked_sub(&share)?;
                if remaining <= zero() {
                    removed.insert(claim.address.clone());
                } else {
                    claim.amount = persist_decimal(&remaining)?;
                }
                allocations.push(Allocation {
                    address: claim.address.clone(),
                    amount: share,
                });
            }
        }
        // Source leaves nonpositive imported claims alone; delete only claims
        // that participated and whose remaining value is nonpositive.
        next.claims
            .retain(|claim| !removed.contains(&claim.address));
        next.last_block = block;
        self.install_claim_map(next)?;
        Ok(allocations)
    }

    /// Sums signed bucket amounts then clamps the final sum at zero. A claim
    /// map has no source amount, so contributes zero to this query.
    pub fn get_total_for_property(&self, property_id: u32) -> Result<Decimal, IouError> {
        let mut total = zero();
        for bucket in self.buckets.values() {
            if bucket.key.property_id == property_id {
                total = total.checked_add(&bucket.amount)?;
            }
        }
        Ok(total.max(zero()))
    }

    /// Source delete removes the bucket only, leaving its claim map intact.
    pub fn delete(&mut self, key: IouKey) -> bool {
        self.buckets.remove(&key).is_some()
    }

    /// Removes all bucket and claim-map records for a property. Source also
    /// deletes matching audit documents; this store owns no nondeterministic
    /// audit DB records.
    pub fn delete_for_property(&mut self, property_id: u32) -> usize {
        let before = self.record_count();
        self.buckets.retain(|key, _| key.property_id != property_id);
        self.claim_maps.retain(|key, claims| {
            if key.property_id == property_id {
                self.claim_count -= claims.claims.len();
                false
            } else {
                true
            }
        });
        before - self.record_count()
    }

    fn check_new_record(&self) -> Result<(), IouError> {
        if self.record_count() >= MAX_IOU_RECORDS {
            Err(IouError::TooManyRecords)
        } else {
            Ok(())
        }
    }
}

/// max(abs(start) - abs(end), 0), including the source's crossing-zero case.
pub fn block_reduction_toward_zero(bucket: Option<&Bucket>) -> Result<Decimal, IouError> {
    match bucket {
        Some(bucket) => Ok(bucket
            .block_start_amount
            .abs()
            .checked_sub(&bucket.amount.abs())?
            .max(zero())),
        None => Ok(zero()),
    }
}

fn zero() -> Decimal {
    Decimal::from(0_u64)
}

fn accrual_share(
    delta: &Decimal,
    positive: &Decimal,
    total: &Decimal,
) -> Result<Decimal, IouError> {
    if positive.is_zero() {
        return Ok(zero());
    }
    Ok(delta.checked_mul(&positive.checked_div(total, JS_DIVISION_PLACES, Rounding::HalfUp)?)?)
}

fn add_claim(claims: &mut Vec<Claim>, address: &AccountId, share: Decimal) -> Result<(), IouError> {
    if share <= zero() {
        return Ok(());
    }
    check_address(address)?;
    if let Some(claim) = claims.iter_mut().find(|claim| &claim.address == address) {
        claim.amount = persist_decimal(&claim.amount.checked_add(&share)?)?;
    } else {
        if claims.len() >= MAX_IOU_CLAIMS {
            return Err(IouError::TooManyClaims);
        }
        claims.push(Claim {
            address: address.clone(),
            amount: persist_decimal(&share)?,
        });
    }
    Ok(())
}

fn check_block(block: u64) -> Result<(), IouError> {
    if block > MAX_JS_SAFE_BLOCK {
        Err(IouError::InvalidBlock)
    } else {
        Ok(())
    }
}

fn persist_decimal(value: &Decimal) -> Result<Decimal, IouError> {
    Ok(value.quantize(JS_DIVISION_PLACES, Rounding::HalfUp)?)
}

fn check_address(address: &AccountId) -> Result<(), IouError> {
    if matches!(
        address.as_str(),
        "__proto__"
            | "constructor"
            | "__defineGetter__"
            | "__defineSetter__"
            | "hasOwnProperty"
            | "__lookupGetter__"
            | "__lookupSetter__"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "toString"
            | "valueOf"
            | "toLocaleString"
    ) {
        Err(IouError::ReservedAddress)
    } else {
        Ok(())
    }
}

fn array_index(address: &AccountId) -> Option<u32> {
    let value = address.as_str().parse::<u32>().ok()?;
    (value != u32::MAX && value.to_string() == address.as_str()).then_some(value)
}

fn order_claims(claims: &mut [Claim]) {
    // Stable sort keeps insertion order among ordinary string keys.
    claims.sort_by_key(|claim| {
        (
            array_index(&claim.address).is_none(),
            array_index(&claim.address),
        )
    });
}
