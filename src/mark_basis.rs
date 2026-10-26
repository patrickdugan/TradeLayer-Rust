//! Exact settlement basis and signed subatom carry (ConservingMarkBasisV1).
//!
//! This is a named repair, separate from the source's q8 PnL helpers. It owns
//! no Ledger, trade average, mark selection, scheduler or settlement authority.

use crate::decimal::{Decimal, DecimalError, Rounding};
use core::fmt;

pub const MAX_BASIS_INPUT_DIGITS: usize = 40;
pub const MAX_BASIS_INPUT_SCALE: u32 = 20;
pub const MAX_BASIS_RATIO_DIGITS: usize = 512;
pub const ATOMS_PER_TOKEN: u128 = 100_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasisError {
    InvalidNumber,
    InvalidPrice,
    InvalidNotional,
    InvalidQuantity,
    InvalidRatio,
    InvalidPosition,
    Capacity,
    AmountOverflow,
    Arithmetic,
}
impl fmt::Display for BasisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BasisError {}
impl From<DecimalError> for BasisError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
type Result<T> = core::result::Result<T, BasisError>;

/// Canonical exact fraction: integer numerator, positive integer denominator,
/// greatest common divisor one; zero is always 0/1. No rounded basis division.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactRatio {
    numerator: Decimal,
    denominator: Decimal,
}
impl Default for ExactRatio {
    fn default() -> Self {
        Self {
            numerator: Decimal::zero(),
            denominator: Decimal::from(1),
        }
    }
}
impl ExactRatio {
    /// Restore a canonical fraction. Noncanonical inputs are rejected, allowing
    /// checkpoint decoders to reject alternate encodings of the same value.
    pub fn from_parts(numerator: Decimal, denominator: Decimal) -> Result<Self> {
        check_parts(&numerator, &denominator)?;
        if gcd(numerator.abs(), denominator.clone())? != Decimal::from(1) {
            return Err(BasisError::InvalidRatio);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
    pub fn from_decimal(value: &Decimal) -> Result<Self> {
        if value.coefficient_digits() > MAX_BASIS_RATIO_DIGITS
            || value.decimal_places() as usize >= MAX_BASIS_RATIO_DIGITS
        {
            return Err(BasisError::Capacity);
        }
        let denominator = power_ten(value.decimal_places())?;
        Self::reduced(value.checked_mul(&denominator)?, denominator)
    }
    pub fn numerator(&self) -> &Decimal {
        &self.numerator
    }
    pub fn denominator(&self) -> &Decimal {
        &self.denominator
    }
    pub fn is_zero(&self) -> bool {
        self.numerator.is_zero()
    }
    pub fn is_negative(&self) -> bool {
        self.numerator.is_negative()
    }
    pub fn negated(&self) -> Self {
        Self {
            numerator: self.numerator.negated(),
            denominator: self.denominator.clone(),
        }
    }
    pub fn checked_add(&self, rhs: &Self) -> Result<Self> {
        Self::reduced(
            self.numerator
                .checked_mul(&rhs.denominator)?
                .checked_add(&rhs.numerator.checked_mul(&self.denominator)?)?,
            self.denominator.checked_mul(&rhs.denominator)?,
        )
    }
    pub fn checked_sub(&self, rhs: &Self) -> Result<Self> {
        self.checked_add(&rhs.negated())
    }
    pub fn checked_mul(&self, rhs: &Self) -> Result<Self> {
        Self::reduced(
            self.numerator.checked_mul(&rhs.numerator)?,
            self.denominator.checked_mul(&rhs.denominator)?,
        )
    }
    pub fn checked_div(&self, rhs: &Self) -> Result<Self> {
        if rhs.is_zero() {
            return Err(BasisError::InvalidRatio);
        }
        Self::reduced(
            self.numerator.checked_mul(&rhs.denominator)?,
            self.denominator.checked_mul(&rhs.numerator)?,
        )
    }
    /// Explicitly rounded display/projection only; transitions do not call it.
    pub fn to_decimal(&self, places: u32, rounding: Rounding) -> Result<Decimal> {
        Ok(self
            .numerator
            .checked_div(&self.denominator, places, rounding)?)
    }
    fn reduced(mut numerator: Decimal, mut denominator: Decimal) -> Result<Self> {
        if denominator.is_zero() {
            return Err(BasisError::InvalidRatio);
        }
        if numerator.is_zero() {
            return Ok(Self::default());
        }
        if denominator.is_negative() {
            numerator = numerator.negated();
            denominator = denominator.negated();
        }
        let common = gcd(numerator.abs(), denominator.clone())?;
        numerator = numerator.checked_div(&common, 0, Rounding::Down)?;
        denominator = denominator.checked_div(&common, 0, Rounding::Down)?;
        check_parts(&numerator, &denominator)?;
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasisConfig {
    pub inverse: bool,
    pub notional: Decimal,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignedAtoms {
    pub negative: bool,
    pub atoms: u128,
}
impl SignedAtoms {
    pub fn to_decimal(&self) -> Decimal {
        let value = Decimal::from(self.atoms)
            .checked_div(&Decimal::from(ATOMS_PER_TOKEN), 8, Rounding::Down)
            .expect("u128 atoms divided by 1e8 always fits Decimal");
        if self.negative {
            value.negated()
        } else {
            value
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BasisPosition {
    pub quantity: Decimal,
    pub entry_basis: ExactRatio,
    /// Signed residual token PnL, strictly smaller than one atom in magnitude.
    /// Retained even when quantity becomes zero.
    pub carry: ExactRatio,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasisAction {
    Trade { delta: Decimal, price: Decimal },
    Mark { price: Decimal },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasisTransition {
    pub position: BasisPosition,
    /// Exact incremental token PnL, excluding the previous carry.
    pub pnl: ExactRatio,
    pub atoms: SignedAtoms,
}
impl BasisPosition {
    pub fn validate(&self, config: &BasisConfig) -> Result<()> {
        bounded(&config.notional)?;
        if config.notional <= Decimal::zero() {
            return Err(BasisError::InvalidNotional);
        }
        bounded(&self.quantity)?;
        if (self.quantity.is_zero() && !self.entry_basis.is_zero())
            || (!config.inverse && !self.quantity.is_zero() && self.entry_basis.is_zero())
            || (!self.quantity.is_zero()
                && !self.entry_basis.is_zero()
                && self.entry_basis.is_negative()
                    != (self.quantity.is_negative() != config.inverse))
            || self
                .carry
                .numerator
                .abs()
                .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?
                >= self.carry.denominator
        {
            return Err(BasisError::InvalidPosition);
        }
        Ok(())
    }

    /// Pure staged transition. The caller applies atom movements and this
    /// returned position together, using an immutable contract configuration.
    pub fn transition(
        &self,
        config: &BasisConfig,
        action: &BasisAction,
    ) -> Result<BasisTransition> {
        self.validate(config)?;
        let mut position = self.clone();
        let pnl = match action {
            BasisAction::Mark { price } => {
                let target = ExactRatio::from_decimal(&self.quantity)?
                    .checked_mul(&coordinate(price, config.inverse)?)?;
                let change = target
                    .checked_sub(&self.entry_basis)?
                    .checked_mul(&ExactRatio::from_decimal(&config.notional)?)?;
                position.entry_basis = target;
                change
            }
            BasisAction::Trade { delta, price } => {
                bounded(delta)?;
                if delta.is_zero() {
                    return Err(BasisError::InvalidQuantity);
                }
                let unit = coordinate(price, config.inverse)?;
                let after = self.quantity.checked_add(delta)?;
                bounded(&after)?;
                position.quantity = after.clone();
                if self.quantity.is_zero() || self.quantity.is_negative() == delta.is_negative() {
                    position.entry_basis = self
                        .entry_basis
                        .checked_add(&ExactRatio::from_decimal(delta)?.checked_mul(&unit)?)?;
                    ExactRatio::default()
                } else {
                    let close = self.quantity.abs().min(delta.abs());
                    let fraction = ExactRatio::from_decimal(&close)?
                        .checked_div(&ExactRatio::from_decimal(&self.quantity.abs())?)?;
                    let removed = self.entry_basis.checked_mul(&fraction)?;
                    let signed_close = if self.quantity.is_negative() {
                        close.negated()
                    } else {
                        close
                    };
                    let change = ExactRatio::from_decimal(&signed_close)?
                        .checked_mul(&unit)?
                        .checked_sub(&removed)?
                        .checked_mul(&ExactRatio::from_decimal(&config.notional)?)?;
                    position.entry_basis = if after.is_zero() {
                        ExactRatio::default()
                    } else if after.is_negative() != self.quantity.is_negative() {
                        ExactRatio::from_decimal(&after)?.checked_mul(&unit)?
                    } else {
                        self.entry_basis.checked_sub(&removed)?
                    };
                    change
                }
            }
        };
        let total = self.carry.checked_add(&pnl)?;
        let scaled = total
            .numerator
            .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?
            .checked_div(&total.denominator, 0, Rounding::Down)?;
        let atoms = scaled
            .abs()
            .to_string()
            .parse::<u128>()
            .map_err(|_| BasisError::AmountOverflow)?;
        let atoms = SignedAtoms {
            negative: atoms != 0 && scaled.is_negative(),
            atoms,
        };
        position.carry = total.checked_sub(&ExactRatio::from_decimal(&atoms.to_decimal())?)?;
        position.validate(config)?;
        Ok(BasisTransition {
            position,
            pnl,
            atoms,
        })
    }
}

/// Source default BigNumber reciprocal precision is 20 places, HalfUp.
/// The inverse coordinate is its negative, so signed q*(U(to)-U(from))
/// matches source (1/from-1/to)*q without averaging reciprocal prices.
pub fn coordinate(price: &Decimal, inverse: bool) -> Result<ExactRatio> {
    bounded(price)?;
    if price <= &Decimal::zero() {
        return Err(BasisError::InvalidPrice);
    }
    let unit = if inverse {
        Decimal::from(1)
            .checked_div(price, 20, Rounding::HalfUp)?
            .negated()
    } else {
        price.clone()
    };
    ExactRatio::from_decimal(&unit)
}

/// Positive bounded-domain equivalent of active calculateClearingPNL. Returns
/// raw exact Decimal PnL, before q8/Number helpers; zero quantity is permitted.
pub fn source_clearing_pnl(
    quantity: &Decimal,
    from: &Decimal,
    to: &Decimal,
    inverse: bool,
    notional: &Decimal,
) -> Result<Decimal> {
    bounded(quantity)?;
    bounded(notional)?;
    if notional <= &Decimal::zero() {
        return Err(BasisError::InvalidNotional);
    }
    coordinate(to, inverse)?
        .checked_sub(&coordinate(from, inverse)?)?
        .checked_mul(&ExactRatio::from_decimal(quantity)?)?
        .checked_mul(&ExactRatio::from_decimal(notional)?)?
        .to_decimal(100, Rounding::Down)
}

fn bounded(value: &Decimal) -> Result<()> {
    if value.coefficient_digits() > MAX_BASIS_INPUT_DIGITS
        || value.decimal_places() > MAX_BASIS_INPUT_SCALE
    {
        Err(BasisError::Capacity)
    } else {
        Ok(())
    }
}
fn check_parts(numerator: &Decimal, denominator: &Decimal) -> Result<()> {
    if numerator.decimal_places() != 0
        || denominator.decimal_places() != 0
        || denominator <= &Decimal::zero()
    {
        return Err(BasisError::InvalidRatio);
    }
    if numerator.coefficient_digits() > MAX_BASIS_RATIO_DIGITS
        || denominator.coefficient_digits() > MAX_BASIS_RATIO_DIGITS
    {
        return Err(BasisError::Capacity);
    }
    Ok(())
}
fn power_ten(places: u32) -> Result<Decimal> {
    format!("1{}", "0".repeat(places as usize))
        .parse()
        .map_err(|_| BasisError::Arithmetic)
}
fn gcd(mut left: Decimal, mut right: Decimal) -> Result<Decimal> {
    // Transient products of two stored ratios have at most 1024 digits.
    // Euclid takes fewer than five iterations per decimal digit.
    let mut steps = 0usize;
    while !right.is_zero() {
        if steps >= MAX_BASIS_RATIO_DIGITS * 10 {
            return Err(BasisError::Capacity);
        }
        let quotient = left.checked_div(&right, 0, Rounding::Down)?;
        let remainder = left.checked_sub(&quotient.checked_mul(&right)?)?;
        left = right;
        right = remainder;
        steps += 1;
    }
    Ok(left)
}
