//! Read-only BTC liquidity-reward quote for the TL + TLVEST policy.
//!
//! The source tradelayer.js has (1) a 0.001 base-coin/TL absent-VWAP price,
//! (2) a fee-linked bounded logarithmic reward share, and (3) a legacy
//! property-3 reward credit. This explicitly replaces the property-3
//! destination with the TL/TLVEST policy, retaining a separate fee-paying
//! funding requirement. It performs no issuance or ledger mutation.
//!
//! The logarithmic preview uses f64, as the JavaScript source does. Never use
//! these floating-point quotes as consensus or spend authority; that requires
//! an integer-certified schedule and authenticated BTC/TL VWAP inputs.

use crate::native_entitlement::{
    ATOMS_PER_TOKEN, BITCOIN_TLVEST_PROPERTY_ID, BITCOIN_TL_PROPERTY_ID,
};

/// 0.001 BTC/TL, mirroring the source's numeric 0.001 LTC/TL fallback
/// in the new Bitcoin-denominated version. One BTC has 100_000_000 atoms.
pub const FALLBACK_TL_PRICE_BTC_ATOMS: u128 = ATOMS_PER_TOKEN / 1_000;
pub const SHARE_SCALE: u128 = 1_000_000;
const SOURCE_MAX_SHARE: f64 = 0.35;
const SOURCE_MIN_SHARE: f64 = 0.02;
const SOURCE_PIVOT_BTC: f64 = 1_000.0;
const SOURCE_SLOPE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardError {
    InvalidInput,
    Arithmetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardPhase {
    Vesting,
    FullyVested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceOrigin {
    ProvidedVwap,
    LowPriceFallback,
}

/// Caller-supplied, independently verified fee and volume observations.
/// `tl_vwap_price_btc_atoms` is BTC atoms per whole TL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardObservation {
    pub trade_btc_atoms: u128,
    pub traded_tl_atoms: u128,
    pub net_fee_btc_atoms: u128,
    pub cumulative_btc_atoms: u128,
    /// Exact BTC volume at which TLVEST is fully vested. No implicit choice.
    pub vest_complete_btc_atoms: u128,
    pub tl_vwap_price_btc_atoms: Option<u128>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardQuote {
    pub phase: RewardPhase,
    pub reward_property_id: u32,
    pub price_origin: PriceOrigin,
    pub tl_price_btc_atoms: u128,
    /// Trade BTC value capped by the executed TL quantity valued at VWAP.
    pub eligible_trade_btc_atoms: u128,
    pub decade_halvings: u32,
    /// Share of the actually paid net fee, in 1/1_000_000 units.
    pub fee_share_ppm: u128,
    pub reward_value_btc_atoms: u128,
    /// Units of TLVEST before full vesting and TL afterwards.
    pub reward_token_atoms: u128,
}

/// Source `VolumeIndex.computeLiquidityRewardShare`, with BTC input replacing
/// its Litecoin volume, converted to parts per million by round-down.
/// Used for a non-authoritative quote; the f64 source curve is not consensus-safe.
pub fn source_fee_share_ppm(cumulative_btc_atoms: u128) -> u128 {
    let cumulative = (cumulative_btc_atoms as f64) / (ATOMS_PER_TOKEN as f64);
    let scaled = cumulative / SOURCE_PIVOT_BTC;
    let attenuation = SOURCE_MAX_SHARE / (1.0 + (1.0 + scaled).log10() * SOURCE_SLOPE);
    let share = attenuation
        .clamp(SOURCE_MIN_SHARE, SOURCE_MAX_SHARE)
        .min(0.99);
    (share * SHARE_SCALE as f64).floor() as u128
}

/// Exact absolute base-10 BTC-volume boundaries. When vesting ends at
/// 21 million BTC, rewards first halve at 100 million BTC, then 1 billion BTC.
/// Both inputs use the same BTC atomic units, so their decimal order differs
/// by exactly the same number as the order of their whole-BTC values.
pub fn decade_halvings(cumulative: u128, full_vest: u128) -> Result<u32, RewardError> {
    if full_vest == 0 {
        return Err(RewardError::InvalidInput);
    }
    if cumulative < full_vest {
        return Ok(0);
    }
    fn decimal_order(mut n: u128) -> u32 {
        let mut count = 0;
        while n >= 10 {
            n /= 10;
            count += 1;
        }
        count
    }
    Ok(decimal_order(cumulative).saturating_sub(decimal_order(full_vest)))
}

fn mul_div_floor(a: u128, b: u128, denominator: u128) -> Result<u128, RewardError> {
    if denominator == 0 {
        return Err(RewardError::InvalidInput);
    }
    let whole = a / denominator;
    let remainder = a % denominator;
    let first = whole.checked_mul(b).ok_or(RewardError::Arithmetic)?;
    let second = remainder.checked_mul(b).ok_or(RewardError::Arithmetic)? / denominator;
    first.checked_add(second).ok_or(RewardError::Arithmetic)
}

pub fn quote_reward(o: RewardObservation) -> Result<RewardQuote, RewardError> {
    if o.trade_btc_atoms == 0
        || o.traded_tl_atoms == 0
        || o.vest_complete_btc_atoms == 0
        || o.tl_vwap_price_btc_atoms == Some(0)
    {
        return Err(RewardError::InvalidInput);
    }
    let price = o
        .tl_vwap_price_btc_atoms
        .unwrap_or(FALLBACK_TL_PRICE_BTC_ATOMS);
    let origin = if o.tl_vwap_price_btc_atoms.is_some() {
        PriceOrigin::ProvidedVwap
    } else {
        PriceOrigin::LowPriceFallback
    };
    // Cap wash/pump trade value at the amount implied by TL's VWAP.
    let market_value = mul_div_floor(o.traded_tl_atoms, price, ATOMS_PER_TOKEN)?;
    let eligible = o.trade_btc_atoms.min(market_value);
    let phase = if o.cumulative_btc_atoms < o.vest_complete_btc_atoms {
        RewardPhase::Vesting
    } else {
        RewardPhase::FullyVested
    };
    let decade_count = if phase == RewardPhase::FullyVested {
        decade_halvings(o.cumulative_btc_atoms, o.vest_complete_btc_atoms)?
    } else {
        0
    };
    let reference_volume = if phase == RewardPhase::FullyVested {
        o.vest_complete_btc_atoms
    } else {
        o.cumulative_btc_atoms
    };
    let base_share = source_fee_share_ppm(reference_volume);
    let fee_share_ppm = if decade_count >= 128 {
        0
    } else {
        base_share >> decade_count
    };
    // The reward is based on *net* paid fees and further reduced if a trade
    // was executed above its TL VWAP. Integer truncation keeps it fee-bounded.
    let vwapped_fee = mul_div_floor(o.net_fee_btc_atoms, eligible, o.trade_btc_atoms)?;
    let award_value = mul_div_floor(vwapped_fee, fee_share_ppm, SHARE_SCALE)?;
    let award_tokens = mul_div_floor(award_value, ATOMS_PER_TOKEN, price)?;
    Ok(RewardQuote {
        phase,
        reward_property_id: if phase == RewardPhase::Vesting {
            BITCOIN_TLVEST_PROPERTY_ID
        } else {
            BITCOIN_TL_PROPERTY_ID
        },
        price_origin: origin,
        tl_price_btc_atoms: price,
        eligible_trade_btc_atoms: eligible,
        decade_halvings: decade_count,
        fee_share_ppm,
        reward_value_btc_atoms: award_value,
        reward_token_atoms: award_tokens,
    })
}
