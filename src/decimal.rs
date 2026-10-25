//! Finite decimal arithmetic for legacy economic calculations.
//!
//! Coefficients are decimal integers rather than floating point or fixed-size
//! machine integers. Division/quantization require explicit decimal places and
//! rounding. Resource limits are local bounds, not historical protocol rules.

use core::{cmp::Ordering, fmt, str::FromStr};

pub const MAX_DECIMAL_DIGITS: usize = 4096;
pub const MAX_DECIMAL_SCALE: u32 = 4096;
pub const MAX_DECIMAL_INPUT_BYTES: usize = 8192;
const MAX_WORK_DIGITS: usize = MAX_DECIMAL_DIGITS * 2 + MAX_DECIMAL_SCALE as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalError {
    InvalidSyntax,
    LimitExceeded,
    DivisionByZero,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DecimalError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    /// Truncate toward zero (BigNumber ROUND_DOWN).
    Down,
    /// Nearest, with ties away from zero (BigNumber ROUND_HALF_UP).
    HalfUp,
}

/// Canonical sign, little-endian base-ten coefficient, and fractional scale.
/// Zero has an empty coefficient, no sign, and scale zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    scale: u32,
}

impl Decimal {
    pub fn zero() -> Self {
        Self::default()
    }

    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.negative
    }

    pub fn abs(&self) -> Self {
        let mut out = self.clone();
        out.negative = false;
        out
    }

    pub fn negated(&self) -> Self {
        let mut out = self.clone();
        if !out.is_zero() {
            out.negative = !out.negative;
        }
        out
    }

    pub fn decimal_places(&self) -> u32 {
        self.scale
    }

    pub fn coefficient_digits(&self) -> usize {
        self.digits.len()
    }

    fn canonical(
        negative: bool,
        mut digits: Vec<u8>,
        mut scale: u32,
    ) -> Result<Self, DecimalError> {
        trim(&mut digits);
        if digits.is_empty() {
            return Ok(Self::zero());
        }
        let removable = digits
            .iter()
            .take_while(|&&n| n == 0)
            .count()
            .min(scale as usize);
        digits.drain(..removable);
        scale -= removable as u32;
        if digits.len() > MAX_DECIMAL_DIGITS || scale > MAX_DECIMAL_SCALE {
            return Err(DecimalError::LimitExceeded);
        }
        Ok(Self {
            negative,
            digits,
            scale,
        })
    }

    pub fn checked_add(&self, rhs: &Self) -> Result<Self, DecimalError> {
        let scale = self.scale.max(rhs.scale);
        let left = shifted(&self.digits, (scale - self.scale) as usize)?;
        let right = shifted(&rhs.digits, (scale - rhs.scale) as usize)?;
        let (negative, digits) = if self.negative == rhs.negative {
            (self.negative, add(&left, &right))
        } else {
            match compare(&left, &right) {
                Ordering::Greater => (self.negative, subtract(&left, &right)),
                Ordering::Less => (rhs.negative, subtract(&right, &left)),
                Ordering::Equal => return Ok(Self::zero()),
            }
        };
        Self::canonical(negative, digits, scale)
    }

    pub fn checked_sub(&self, rhs: &Self) -> Result<Self, DecimalError> {
        self.checked_add(&rhs.negated())
    }

    pub fn checked_mul(&self, rhs: &Self) -> Result<Self, DecimalError> {
        if self.is_zero() || rhs.is_zero() {
            return Ok(Self::zero());
        }
        let mut digits = vec![0; self.digits.len() + rhs.digits.len()];
        for (i, &left) in self.digits.iter().enumerate() {
            let mut carry = 0u16;
            for (j, &right) in rhs.digits.iter().enumerate() {
                let value = u16::from(digits[i + j]) + u16::from(left) * u16::from(right) + carry;
                digits[i + j] = (value % 10) as u8;
                carry = value / 10;
            }
            digits[i + rhs.digits.len()] = carry as u8;
        }
        Self::canonical(
            self.negative != rhs.negative,
            digits,
            self.scale + rhs.scale,
        )
    }

    pub fn checked_div(
        &self,
        rhs: &Self,
        places: u32,
        rounding: Rounding,
    ) -> Result<Self, DecimalError> {
        if rhs.is_zero() {
            return Err(DecimalError::DivisionByZero);
        }
        if places > MAX_DECIMAL_SCALE {
            return Err(DecimalError::LimitExceeded);
        }
        if self.is_zero() {
            return Ok(Self::zero());
        }
        let shift = i64::from(rhs.scale) + i64::from(places) - i64::from(self.scale);
        let numerator = shifted(&self.digits, shift.max(0) as usize)?;
        let denominator = shifted(&rhs.digits, (-shift).max(0) as usize)?;
        let (mut quotient, remainder) = divide(&numerator, &denominator);
        if rounding == Rounding::HalfUp
            && compare(&multiply_digit(&remainder, 2), &denominator) != Ordering::Less
        {
            quotient = add(&quotient, &[1]);
        }
        Self::canonical(self.negative != rhs.negative, quotient, places)
    }

    pub fn quantize(&self, places: u32, rounding: Rounding) -> Result<Self, DecimalError> {
        if places > MAX_DECIMAL_SCALE {
            return Err(DecimalError::LimitExceeded);
        }
        if self.scale <= places {
            return Ok(self.clone());
        }
        let discarded = (self.scale - places) as usize;
        let mut digits = self.digits.get(discarded..).unwrap_or_default().to_vec();
        // The most significant removed digit decides decimal HALF_UP.
        if rounding == Rounding::HalfUp && self.digits.get(discarded - 1).is_some_and(|&n| n >= 5) {
            digits = add(&digits, &[1]);
        }
        Self::canonical(self.negative, digits, places)
    }

    fn magnitude_cmp(&self, rhs: &Self) -> Ordering {
        if self.is_zero() || rhs.is_zero() {
            return self.digits.len().cmp(&rhs.digits.len());
        }
        let scale = self.scale.max(rhs.scale);
        let left_pad = (scale - self.scale) as usize;
        let right_pad = (scale - rhs.scale) as usize;
        let left_len = self.digits.len() + left_pad;
        let right_len = rhs.digits.len() + right_pad;
        let length_order = left_len.cmp(&right_len);
        if length_order != Ordering::Equal {
            return length_order;
        }
        for i in (0..left_len).rev() {
            let left = i
                .checked_sub(left_pad)
                .and_then(|n| self.digits.get(n))
                .copied()
                .unwrap_or(0);
            let right = i
                .checked_sub(right_pad)
                .and_then(|n| rhs.digits.get(n))
                .copied()
                .unwrap_or(0);
            match left.cmp(&right) {
                Ordering::Equal => {}
                other => return other,
            }
        }
        Ordering::Equal
    }
}

impl Ord for Decimal {
    fn cmp(&self, rhs: &Self) -> Ordering {
        match (self.negative, rhs.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => self.magnitude_cmp(rhs).reverse(),
            (false, false) => self.magnitude_cmp(rhs),
        }
    }
}
impl PartialOrd for Decimal {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("0");
        }
        if self.negative {
            f.write_str("-")?;
        }
        let text: String = self
            .digits
            .iter()
            .rev()
            .map(|&d| char::from(b'0' + d))
            .collect();
        let scale = self.scale as usize;
        if scale == 0 {
            return f.write_str(&text);
        }
        if text.len() > scale {
            let split = text.len() - scale;
            write!(f, "{}.{}", &text[..split], &text[split..])
        } else {
            f.write_str("0.")?;
            for _ in text.len()..scale {
                f.write_str("0")?;
            }
            f.write_str(&text)
        }
    }
}

impl FromStr for Decimal {
    type Err = DecimalError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw.len() > MAX_DECIMAL_INPUT_BYTES {
            return Err(DecimalError::LimitExceeded);
        }
        let text = raw.trim_matches(js_space);
        let (negative, text) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        if text.is_empty() {
            return Err(DecimalError::InvalidSyntax);
        }
        let (mantissa, exponent) = if let Some(index) = text.find(['e', 'E']) {
            let exponent = &text[index + 1..];
            let unsigned = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            if unsigned.is_empty() || !unsigned.bytes().all(|b| b.is_ascii_digit()) {
                return Err(DecimalError::InvalidSyntax);
            }
            let exponent = exponent
                .parse::<i64>()
                .map_err(|_| DecimalError::LimitExceeded)?;
            if exponent.unsigned_abs() > MAX_WORK_DIGITS as u64 {
                return Err(DecimalError::LimitExceeded);
            }
            (&text[..index], exponent)
        } else {
            (text, 0)
        };
        let mut point = false;
        let mut scale = 0i64;
        let mut digits = Vec::new();
        for byte in mantissa.bytes() {
            if byte == b'.' && !point {
                point = true;
            } else if byte.is_ascii_digit() {
                digits.push(byte - b'0');
                scale += i64::from(point);
            } else {
                return Err(DecimalError::InvalidSyntax);
            }
        }
        if digits.is_empty() {
            return Err(DecimalError::InvalidSyntax);
        }
        digits.reverse();
        trim(&mut digits);
        if digits.is_empty() {
            return Ok(Self::zero());
        }
        let scale = scale - exponent;
        if scale < 0 {
            Self::canonical(negative, shifted(&digits, (-scale) as usize)?, 0)
        } else {
            Self::canonical(negative, digits, scale as u32)
        }
    }
}

macro_rules! from_signed {
    ($($kind:ty),*) => {$ (
        impl From<$kind> for Decimal {
            fn from(value: $kind) -> Self {
                let value = value as i128;
                Self { negative: value < 0, digits: integer_digits(value.unsigned_abs()), scale: 0 }
            }
        }
    )*};
}
macro_rules! from_unsigned {
    ($($kind:ty),*) => {$ (
        impl From<$kind> for Decimal {
            fn from(value: $kind) -> Self {
                Self { negative: false, digits: integer_digits(value as u128), scale: 0 }
            }
        }
    )*};
}
from_signed!(i8, i16, i32, i64, i128, isize);
from_unsigned!(u8, u16, u32, u64, u128, usize);

fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn integer_digits(mut value: u128) -> Vec<u8> {
    let mut digits = Vec::new();
    while value != 0 {
        digits.push((value % 10) as u8);
        value /= 10;
    }
    digits
}
fn trim(digits: &mut Vec<u8>) {
    while digits.last() == Some(&0) {
        digits.pop();
    }
}
fn shifted(digits: &[u8], zeros: usize) -> Result<Vec<u8>, DecimalError> {
    if digits.is_empty() {
        return Ok(Vec::new());
    }
    if zeros.saturating_add(digits.len()) > MAX_WORK_DIGITS {
        return Err(DecimalError::LimitExceeded);
    }
    let mut out = vec![0; zeros];
    out.extend_from_slice(digits);
    Ok(out)
}
fn compare(left: &[u8], right: &[u8]) -> Ordering {
    left.len()
        .cmp(&right.len())
        .then_with(|| left.iter().rev().cmp(right.iter().rev()))
}
fn add(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0;
    for i in 0..left.len().max(right.len()) {
        let value = left.get(i).copied().unwrap_or(0) + right.get(i).copied().unwrap_or(0) + carry;
        out.push(value % 10);
        carry = value / 10;
    }
    if carry != 0 {
        out.push(carry);
    }
    out
}
fn subtract(left: &[u8], right: &[u8]) -> Vec<u8> {
    debug_assert!(compare(left, right) != Ordering::Less);
    let mut out = Vec::with_capacity(left.len());
    let mut borrow = 0i16;
    for (i, &digit) in left.iter().enumerate() {
        let value = i16::from(digit) - i16::from(right.get(i).copied().unwrap_or(0)) - borrow;
        borrow = i16::from(value < 0);
        out.push((if value < 0 { value + 10 } else { value }) as u8);
    }
    trim(&mut out);
    out
}
fn multiply_digit(digits: &[u8], digit: u8) -> Vec<u8> {
    if digit == 0 || digits.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(digits.len() + 1);
    let mut carry = 0u16;
    for &n in digits {
        let value = u16::from(n) * u16::from(digit) + carry;
        out.push((value % 10) as u8);
        carry = value / 10;
    }
    if carry != 0 {
        out.push(carry as u8);
    }
    out
}
fn divide(numerator: &[u8], denominator: &[u8]) -> (Vec<u8>, Vec<u8>) {
    debug_assert!(!denominator.is_empty());
    if compare(numerator, denominator) == Ordering::Less {
        return (Vec::new(), numerator.to_vec());
    }
    let multiples: Vec<_> = (0..=9)
        .map(|digit| multiply_digit(denominator, digit))
        .collect();
    let mut quotient = Vec::with_capacity(numerator.len());
    let mut remainder = Vec::new();
    for &digit in numerator.iter().rev() {
        if !remainder.is_empty() || digit != 0 {
            remainder.insert(0, digit);
        }
        let next = (1..=9)
            .rev()
            .find(|&d| compare(&multiples[d], &remainder) != Ordering::Greater)
            .unwrap_or(0);
        quotient.push(next as u8);
        if next != 0 {
            remainder = subtract(&remainder, &multiples[next]);
        }
    }
    quotient.reverse();
    trim(&mut quotient);
    (quotient, remainder)
}
