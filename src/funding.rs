//! Fractional funding calculations and a conserving, caller-custody plan.
//!
//! Source rate/notional arithmetic is separate from ConservingFundingV1:
//! positive per-payer charges, exact Hamilton receiver allocation, and complete
//! available-balance preflight repair the source sign/rounding defects. No
//! Ledger, mark authority, scheduler or durable state is owned here.

use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::ledger::AccountId;
use core::fmt;
use std::collections::BTreeSet;

pub const MAX_FUNDING_POSITIONS: usize = 4096;
pub const MAX_FUNDING_DIGITS: usize = 40;
pub const MAX_FUNDING_SCALE: u32 = 20;
pub const ATOMS_PER_TOKEN: u128 = 100_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundingError {
    InvalidNumber,
    InvalidRate,
    InvalidNotional,
    Capacity,
    DuplicateOwner(AccountId),
    MissingCounterparty,
    InsufficientAvailable(AccountId),
    AmountOverflow(AccountId),
    Arithmetic,
}
impl fmt::Display for FundingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FundingError {}
impl From<DecimalError> for FundingError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
type Result<T> = core::result::Result<T, FundingError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingPosition {
    pub owner: AccountId,
    pub contracts: Decimal,
    pub available_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingPayment {
    pub owner: AccountId,
    pub atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingPlan {
    pub rate_bps: Decimal,
    pub collateral_notional: Decimal,
    pub total_atoms: u128,
    /// Positive charges, one individually rounded amount for each payer.
    /// Zero legs are omitted. Both lists use ascending ASCII owner order.
    pub debits: Vec<FundingPayment>,
    pub credits: Vec<FundingPayment>,
}

/// Actual oracle.getTWAP mean: equal row weights, 20-place division followed
/// by q8 HalfUp and a Number return. The owning driver selects the as-of rows.
pub fn source_average(prices: &[Decimal]) -> Result<Option<Decimal>> {
    if prices.len() > MAX_FUNDING_POSITIONS {
        return Err(FundingError::Capacity);
    }
    if prices.is_empty() {
        return Ok(None);
    }
    let mut sum = Decimal::zero();
    for price in prices {
        bounded(price)?;
        if price.is_negative() {
            return Err(FundingError::InvalidNumber);
        }
        sum = sum.checked_add(price)?;
    }
    let average = sum
        .checked_div(&Decimal::from(prices.len()), 20, Rounding::HalfUp)?
        .quantize(8, Rounding::HalfUp)?;
    Ok(Some(number(&average)?))
}

/// Actual clearing.calculateFundingRate operation order with BigNumber's
/// 20-place division, q2 bps, 5bps deadband, /8 q4 and +/-12.5bps cap.
/// Zero input observes the source's falsy missing-price return. Negative
/// prices and numbers outside the bounded finite domain are rejected.
pub fn source_rate(index: &Decimal, average: &Decimal) -> Result<Decimal> {
    bounded(index)?;
    bounded(average)?;
    if index.is_negative() || average.is_negative() {
        return Err(FundingError::InvalidNumber);
    }
    if index.is_zero() || average.is_zero() {
        return Ok(Decimal::zero());
    }
    let basis = index
        .checked_sub(average)?
        .checked_div(average, 20, Rounding::HalfUp)?
        .checked_mul(&Decimal::from(10_000_u32))?
        .quantize(2, Rounding::HalfUp)?;
    let reduced = if basis.abs() < Decimal::from(5_u32) {
        Decimal::zero()
    } else {
        basis.abs().checked_sub(&Decimal::from(5_u32))?
    };
    let hourly = reduced
        .checked_div(&Decimal::from(8_u32), 4, Rounding::HalfUp)?
        .min(d("12.5"));
    number(&if basis.is_negative() {
        hourly.negated()
    } else {
        hourly
    })
}

/// Actual getNotionalValue scalar selected by applyFundingToPositions.
/// Inverse source divides 1/index first at20dp, then multiplies by rawN.
/// Oracle-linear uses rawN*index q8, native-linear rawN; all return Number.
/// This helper requires a positive scoped index instead of invoking a fallback
/// oracle or accepting stale/future observations. Zero raw notional is allowed
/// as an observation but is not an executable nonzero-rate plan notional.
pub fn collateral_notional(
    raw: &Decimal,
    index: &Decimal,
    inverse: bool,
    oracle: bool,
) -> Result<Decimal> {
    bounded(raw)?;
    bounded(index)?;
    if raw.is_negative() || index <= &Decimal::zero() {
        return Err(FundingError::InvalidNotional);
    }
    let result = if inverse {
        Decimal::from(1_u32)
            .checked_div(index, 20, Rounding::HalfUp)?
            .checked_mul(raw)?
            .quantize(8, Rounding::HalfUp)?
    } else if oracle {
        raw.checked_mul(index)?.quantize(8, Rounding::HalfUp)?
    } else {
        raw.clone()
    };
    number(&result)
}

/// Pure ConservingFundingV1 plan. Fractional positions remain exact Decimals;
/// no conversion through source Math.abs(Number(contracts)) is performed.
/// Every payer is rounded once; receivers share the actual positive atom total
/// with exact rational Hamilton remainders and ASCII owner ties. Missing sides,
/// any insufficient payer and any receiver overflow reject the complete plan.
pub fn plan_funding(
    positions: &[FundingPosition],
    collateral_notional: &Decimal,
    rate_bps: &Decimal,
) -> Result<FundingPlan> {
    if positions.len() > MAX_FUNDING_POSITIONS {
        return Err(FundingError::Capacity);
    }
    bounded(collateral_notional)?;
    bounded(rate_bps)?;
    if collateral_notional.is_negative() {
        return Err(FundingError::InvalidNotional);
    }
    if rate_bps.abs() > d("12.5") || rate_bps.decimal_places() > 4 {
        return Err(FundingError::InvalidRate);
    }
    let mut seen = BTreeSet::new();
    let mut payers = Vec::new();
    let mut receivers = Vec::new();
    for position in positions {
        bounded(&position.contracts)?;
        if !seen.insert(position.owner.clone()) {
            return Err(FundingError::DuplicateOwner(position.owner.clone()));
        }
        if position.contracts.is_zero() {
            continue;
        }
        if position.contracts.is_negative() == rate_bps.is_negative() {
            payers.push(position);
        } else {
            receivers.push(position);
        }
    }
    let mut plan = FundingPlan {
        rate_bps: rate_bps.clone(),
        collateral_notional: collateral_notional.clone(),
        total_atoms: 0,
        debits: Vec::new(),
        credits: Vec::new(),
    };
    if rate_bps.is_zero() || (payers.is_empty() && receivers.is_empty()) {
        return Ok(plan);
    }
    if collateral_notional.is_zero() {
        return Err(FundingError::InvalidNotional);
    }
    if payers.is_empty() || receivers.is_empty() {
        return Err(FundingError::MissingCounterparty);
    }
    payers.sort_by(|a, b| a.owner.cmp(&b.owner));
    receivers.sort_by(|a, b| a.owner.cmp(&b.owner));
    for payer in payers {
        let amount = payer
            .contracts
            .abs()
            .checked_mul(collateral_notional)?
            .checked_mul(&rate_bps.abs())?
            .checked_div(&Decimal::from(10_000_u32), 8, Rounding::HalfUp)?;
        let atoms = atoms(&amount)?;
        if atoms > payer.available_atoms {
            return Err(FundingError::InsufficientAvailable(payer.owner.clone()));
        }
        plan.total_atoms = plan
            .total_atoms
            .checked_add(atoms)
            .ok_or(FundingError::Arithmetic)?;
        if atoms != 0 {
            plan.debits.push(FundingPayment {
                owner: payer.owner.clone(),
                atoms,
            });
        }
    }
    if plan.total_atoms == 0 {
        return Ok(plan);
    }
    let denominator = receivers.iter().try_fold(Decimal::zero(), |sum, p| {
        sum.checked_add(&p.contracts.abs())
    })?;
    let mut shares = Vec::with_capacity(receivers.len());
    let mut allocated = 0_u128;
    for receiver in &receivers {
        let numerator = Decimal::from(plan.total_atoms).checked_mul(&receiver.contracts.abs())?;
        let quotient = numerator.checked_div(&denominator, 0, Rounding::Down)?;
        let whole: u128 = quotient
            .to_string()
            .parse()
            .map_err(|_| FundingError::Arithmetic)?;
        let remainder = numerator.checked_sub(&quotient.checked_mul(&denominator)?)?;
        allocated = allocated
            .checked_add(whole)
            .ok_or(FundingError::Arithmetic)?;
        shares.push((receiver, whole, remainder));
    }
    let remaining: usize = plan
        .total_atoms
        .checked_sub(allocated)
        .ok_or(FundingError::Arithmetic)?
        .try_into()
        .map_err(|_| FundingError::Arithmetic)?;
    if remaining >= shares.len() {
        return Err(FundingError::Arithmetic);
    }
    let mut priority: Vec<_> = (0..shares.len()).collect();
    priority.sort_by(|&a, &b| {
        shares[b]
            .2
            .cmp(&shares[a].2)
            .then_with(|| shares[a].0.owner.cmp(&shares[b].0.owner))
    });
    for &i in priority.iter().take(remaining) {
        shares[i].1 = shares[i].1.checked_add(1).ok_or(FundingError::Arithmetic)?;
    }
    for (receiver, atoms, _) in shares {
        if receiver.available_atoms.checked_add(atoms).is_none() {
            return Err(FundingError::AmountOverflow(receiver.owner.clone()));
        }
        if atoms != 0 {
            plan.credits.push(FundingPayment {
                owner: receiver.owner.clone(),
                atoms,
            });
        }
    }
    let credited = plan.credits.iter().try_fold(0_u128, |sum, p| {
        sum.checked_add(p.atoms).ok_or(FundingError::Arithmetic)
    })?;
    if credited != plan.total_atoms {
        return Err(FundingError::Arithmetic);
    }
    Ok(plan)
}

fn bounded(value: &Decimal) -> Result<()> {
    if value.coefficient_digits() > MAX_FUNDING_DIGITS || value.decimal_places() > MAX_FUNDING_SCALE
    {
        return Err(FundingError::Capacity);
    }
    Ok(())
}
fn d(text: &str) -> Decimal {
    text.parse().expect("literal decimal")
}
fn atoms(value: &Decimal) -> Result<u128> {
    value
        .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?
        .to_string()
        .parse()
        .map_err(|_| FundingError::Arithmetic)
}
fn number(value: &Decimal) -> Result<Decimal> {
    let value: f64 = value
        .to_string()
        .parse()
        .map_err(|_| FundingError::InvalidNumber)?;
    if !value.is_finite() {
        return Err(FundingError::InvalidNumber);
    }
    value.to_string().parse().map_err(Into::into)
}
