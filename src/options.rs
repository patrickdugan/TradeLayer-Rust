//! Functional calculator port of all nine helpers in pinned `options.js`.
//!
//! Decimal operations follow BigNumber's default 20-place half-up division.
//! Pricing deliberately retains the source's binary64 days/volatility/square
//! root bridge and its unusual, input-order-dependent portfolio offsets. These
//! helpers do not create contracts, accept trades, debit collateral or assign
//! options: those responsibilities do not exist in the source helper either.

use crate::decimal::{Decimal, Rounding};
use core::str::FromStr;
use std::collections::BTreeMap;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_LF_SHA256: &str =
    "02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsError {
    /// A finite-decimal operation exceeded the Decimal implementation's bounds.
    Arithmetic,
    /// BigNumber would receive a nonfinite number from the pricing bridge.
    NonFinitePricingInput,
}

impl core::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for OptionsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionType {
    Call,
    Put,
}

impl OptionType {
    pub const fn as_source_name(self) -> &'static str {
        match self {
            Self::Call => "Call",
            Self::Put => "Put",
        }
    }
}

/// `parseTicker` is intentionally more permissive than `isOptionTicker`.
/// Expiry may be infinite and strike may be NaN, as in the JavaScript parser.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTicker {
    pub series_id: String,
    pub expiry_block: f64,
    pub option_type: OptionType,
    pub strike: Option<f64>,
    pub raw: String,
}

/// Exact string-language equivalent of the source's option-ticker regex.
/// Rust's string argument statically excludes JavaScript non-string inputs.
pub fn is_option_ticker(ticker: &str) -> bool {
    if ticker
        .chars()
        .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
    {
        return false;
    }
    ticker.char_indices().any(|(index, character)| {
        if character != '-' || index == 0 {
            return false;
        }
        let rest = &ticker[index + 1..];
        let Some((expiry, tail)) = rest.split_once('-') else {
            return false;
        };
        if expiry.is_empty() || !expiry.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let Some(kind) = tail.as_bytes().first() else {
            return false;
        };
        if *kind != b'C' && *kind != b'P' {
            return false;
        }
        let suffix = &tail[1..];
        if suffix.is_empty() {
            return true;
        }
        let Some(strike) = suffix.strip_prefix('-') else {
            return false;
        };
        let (integer, fractional) = match strike.split_once('.') {
            Some((integer, fractional)) => (integer, Some(fractional)),
            None => (strike, None),
        };
        !integer.is_empty()
            && integer.bytes().all(|b| b.is_ascii_digit())
            && fractional.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Source `split('-')`, radix-10 `parseInt`, and prefix `parseFloat` semantics.
/// Extra segments are ignored; an invalid strike is retained as `Some(NaN)`.
pub fn parse_ticker(ticker: &str) -> Option<ParsedTicker> {
    let parts: Vec<_> = ticker.split('-').collect();
    if parts.len() < 3 || parts[0].is_empty() {
        return None;
    }
    let expiry_block = parse_int_prefix(parts[1]);
    let option_type = match parts[2] {
        "C" => OptionType::Call,
        "P" => OptionType::Put,
        _ => return None,
    };
    if expiry_block.is_nan() {
        return None;
    }
    let strike = parts
        .get(3)
        .filter(|strike| !strike.is_empty())
        .map(|strike| parse_float_prefix(strike));
    Some(ParsedTicker {
        series_id: parts[0].to_owned(),
        expiry_block,
        option_type,
        strike,
        raw: ticker.to_owned(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSide {
    Long,
    Short,
}

impl PositionSide {
    pub const fn as_source_name(self) -> &'static str {
        match self {
            Self::Long => "LONG",
            Self::Short => "SHORT",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReduceFlip {
    pub existing: f64,
    pub delta: f64,
    pub after: f64,
    pub existing_side: Option<PositionSide>,
    pub delta_side: Option<PositionSide>,
    pub after_side: Option<PositionSide>,
    pub closed_qty: f64,
    pub flip_qty: f64,
}

/// The source uses JavaScript `Number` arithmetic here, rather than BigNumber.
/// Inputs are already numeric; NaN and either zero follow `Number(value) || 0`.
/// Infinities are retained, including the resulting NaN for opposite infinities.
pub fn compute_reduce_flip(existing_qty: f64, delta_qty: f64) -> ReduceFlip {
    let existing = number_or(existing_qty, 0.0);
    let delta = number_or(delta_qty, 0.0);
    let after = existing + delta;
    let mut closed_qty = 0.0;
    let mut flip_qty = 0.0;
    if existing != 0.0 && delta != 0.0 && existing.signum() != delta.signum() {
        closed_qty = existing.abs().min(delta.abs());
        if delta.abs() > existing.abs() {
            flip_qty = delta.abs() - existing.abs();
        }
    }
    ReduceFlip {
        existing,
        delta,
        after,
        existing_side: side(existing),
        delta_side: side(delta),
        after_side: side(after),
        closed_qty,
        flip_qty,
    }
}

/// The source treats any nonempty side other than `LONG` as short.
/// Negative close quantities are retained; this helper is not a validator.
pub fn rpnl_for_close(
    existing_side: Option<&str>,
    closed_qty: &Decimal,
    trade_price: &Decimal,
    avg_price: &Decimal,
) -> Result<Decimal, OptionsError> {
    let Some(existing_side) = existing_side.filter(|side| !side.is_empty()) else {
        return Ok(zero());
    };
    if closed_qty.is_zero() {
        return Ok(zero());
    }
    let difference = if existing_side == "LONG" {
        trade_price.checked_sub(avg_price)
    } else {
        avg_price.checked_sub(trade_price)
    }
    .map_err(|_| OptionsError::Arithmetic)?;
    difference
        .checked_mul(closed_qty)
        .map_err(|_| OptionsError::Arithmetic)
}

/// Every type other than exactly `Call` takes the source's put branch.
pub fn intrinsic(
    option_type: &str,
    strike: &Decimal,
    spot: &Decimal,
) -> Result<Decimal, OptionsError> {
    let difference = if option_type == "Call" {
        spot.checked_sub(strike)
    } else {
        strike.checked_sub(spot)
    }
    .map_err(|_| OptionsError::Arithmetic)?;
    Ok(difference.max(zero()))
}

/// Source EU approximation adds spot times sigma times sqrt(days/365) times
/// 0.3989 to intrinsic and floors at intrinsic. Multiplication remains decimal
/// at every step; there is no invented final 8-place rounding. The f64 bridges use their
/// shortest decimal representations, just as BigNumber's numeric inputs do.
pub fn price_eu_approx(
    option_type: &str,
    spot: &Decimal,
    strike: &Decimal,
    vol_annual: f64,
    days_to_expiry: f64,
) -> Result<Decimal, OptionsError> {
    let iv = intrinsic(option_type, strike, spot)?;
    let years = number_or(days_to_expiry, 0.0).max(0.0) / 365.0;
    if !number_truthy(vol_annual) || !number_truthy(years) {
        return Ok(iv);
    }
    let sigma = finite_number_decimal(vol_annual)?;
    let root_years = finite_number_decimal(years.sqrt())?;
    let factor = Decimal::from_str("0.3989").map_err(|_| OptionsError::Arithmetic)?;
    let noise = spot
        .checked_mul(&sigma)
        .and_then(|v| v.checked_mul(&root_years))
        .and_then(|v| v.checked_mul(&factor))
        .map_err(|_| OptionsError::Arithmetic)?;
    let with_noise = iv
        .checked_add(&noise)
        .map_err(|_| OptionsError::Arithmetic)?;
    Ok(iv.max(with_noise))
}

pub fn naked_maintenance(
    option_type: &str,
    strike: &Decimal,
    spot: &Decimal,
) -> Result<Decimal, OptionsError> {
    let base = if option_type == "Call" { spot } else { strike };
    base.checked_div(&Decimal::from(10i128), 20, Rounding::HalfUp)
        .map_err(|_| OptionsError::Arithmetic)
}

/// The numeric fields are already converted from JavaScript input values.
/// Unknown and empty type names are retained to match the source branches.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionPosition {
    pub option_type: String,
    pub expiry_block: f64,
    pub strike: Decimal,
    pub qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtmExposure {
    pub premium: Decimal,
    pub intrinsic: Decimal,
}

/// Signed net premium and intrinsic, not absolute exposure. Numeric zero/NaN
/// blocks-per-day select the source default 144; negative values clamp to one.
pub fn mtm_exposure(
    positions: &[OptionPosition],
    spot: &Decimal,
    vol_annual: f64,
    blocks_to_expiry: f64,
    blocks_per_day: f64,
) -> Result<MtmExposure, OptionsError> {
    let bpd = number_or(blocks_per_day, 144.0).max(1.0);
    let days = number_or(blocks_to_expiry, 0.0) / bpd;
    let days = if days.is_nan() {
        f64::NAN
    } else {
        days.max(0.0)
    };
    let mut result = MtmExposure {
        premium: zero(),
        intrinsic: zero(),
    };
    for position in positions {
        let premium = price_eu_approx(
            &position.option_type,
            spot,
            &position.strike,
            vol_annual,
            days,
        )?
        .checked_mul(&position.qty)
        .map_err(|_| OptionsError::Arithmetic)?;
        let iv = intrinsic(&position.option_type, &position.strike, spot)?
            .checked_mul(&position.qty)
            .map_err(|_| OptionsError::Arithmetic)?;
        result.premium = result
            .premium
            .checked_add(&premium)
            .map_err(|_| OptionsError::Arithmetic)?;
        result.intrinsic = result
            .intrinsic
            .checked_add(&iv)
            .map_err(|_| OptionsError::Arithmetic)?;
    }
    Ok(result)
}

/// Reproduces source grouping and greedy offsets in original input order.
/// This intentionally subtracts `min(width / 10, naked margin) * covered qty`
/// from naked maintenance. It does not replace that source rule with a spread
/// maximum-loss formula, sort legs, or reuse already consumed long quantities.
pub fn portfolio_maintenance(
    positions: &[OptionPosition],
    spot: &Decimal,
) -> Result<Decimal, OptionsError> {
    let mut groups: Vec<Vec<&OptionPosition>> = Vec::new();
    let mut by_key = BTreeMap::new();
    for position in positions {
        if position.option_type.is_empty() {
            continue;
        }
        let expiry = expiry_group(position.expiry_block);
        let group = *by_key
            .entry((position.option_type.as_str(), expiry))
            .or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
        groups[group].push(position);
    }
    let mut total = zero();
    for legs in groups {
        let mut shorts: Vec<_> = legs
            .iter()
            .filter(|p| p.qty.is_negative())
            .map(|p| (*p, p.qty.abs()))
            .collect();
        let mut longs: Vec<_> = legs
            .iter()
            .filter(|p| !p.qty.is_zero() && !p.qty.is_negative())
            .map(|p| (*p, p.qty.clone()))
            .collect();
        for (short, remaining) in &shorts {
            let naked = naked_maintenance(&short.option_type, &short.strike, spot)?
                .checked_mul(remaining)
                .map_err(|_| OptionsError::Arithmetic)?;
            total = total
                .checked_add(&naked)
                .map_err(|_| OptionsError::Arithmetic)?;
        }
        for (short, short_remaining) in &mut shorts {
            for (long, long_remaining) in &mut longs {
                if short_remaining.is_zero() || long_remaining.is_zero() {
                    continue;
                }
                let protective = (short.option_type == "Call" && long.strike > short.strike)
                    || (short.option_type == "Put" && long.strike < short.strike);
                if !protective {
                    continue;
                }
                let covered_qty = short_remaining.clone().min(long_remaining.clone());
                let width = long
                    .strike
                    .checked_sub(&short.strike)
                    .map_err(|_| OptionsError::Arithmetic)?
                    .abs();
                let offset = width
                    .checked_div(&Decimal::from(10i128), 20, Rounding::HalfUp)
                    .and_then(|v| v.checked_mul(&covered_qty))
                    .map_err(|_| OptionsError::Arithmetic)?;
                let max_offset = naked_maintenance(&short.option_type, &short.strike, spot)?
                    .checked_mul(&covered_qty)
                    .map_err(|_| OptionsError::Arithmetic)?;
                total = total
                    .checked_sub(&offset.min(max_offset))
                    .map_err(|_| OptionsError::Arithmetic)?;
                *short_remaining = short_remaining
                    .checked_sub(&covered_qty)
                    .map_err(|_| OptionsError::Arithmetic)?;
                *long_remaining = long_remaining
                    .checked_sub(&covered_qty)
                    .map_err(|_| OptionsError::Arithmetic)?;
            }
        }
    }
    Ok(total.max(zero()))
}

fn zero() -> Decimal {
    Decimal::from(0i128)
}

fn number_truthy(number: f64) -> bool {
    number != 0.0 && !number.is_nan()
}

fn number_or(number: f64, fallback: f64) -> f64 {
    if number_truthy(number) {
        number
    } else {
        fallback
    }
}

fn side(qty: f64) -> Option<PositionSide> {
    if qty > 0.0 {
        Some(PositionSide::Long)
    } else if qty < 0.0 {
        Some(PositionSide::Short)
    } else {
        None
    }
}

fn finite_number_decimal(number: f64) -> Result<Decimal, OptionsError> {
    if !number.is_finite() {
        return Err(OptionsError::NonFinitePricingInput);
    }
    Decimal::from_str(&number.to_string()).map_err(|_| OptionsError::Arithmetic)
}

fn expiry_group(expiry: f64) -> u64 {
    // JavaScript `${Number(expiry || 0)}` maps numeric NaN and -0 to "0".
    number_or(expiry, 0.0).to_bits()
}

fn js_whitespace(character: char) -> bool {
    matches!(character,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn parse_int_prefix(text: &str) -> f64 {
    let text = text.trim_start_matches(js_whitespace);
    let bytes = text.as_bytes();
    let start = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == start {
        f64::NAN
    } else {
        text[..end].parse().unwrap_or(f64::NAN)
    }
}

fn parse_float_prefix(text: &str) -> f64 {
    let text = text.trim_start_matches(js_whitespace);
    let bytes = text.as_bytes();
    let start = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    if text.get(start..).is_some_and(|t| t.starts_with("Infinity")) {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let mut digits = end - start;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let fractional = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        digits += end - fractional;
    }
    if digits == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(end), Some(b'e') | Some(b'E')) {
        let mut exponent_end = end + 1;
        if matches!(bytes.get(exponent_end), Some(b'+') | Some(b'-')) {
            exponent_end += 1;
        }
        let exponent_start = exponent_end;
        while bytes.get(exponent_end).is_some_and(u8::is_ascii_digit) {
            exponent_end += 1;
        }
        if exponent_end > exponent_start {
            end = exponent_end;
        }
    }
    text[..end].parse().unwrap_or(f64::NAN)
}
