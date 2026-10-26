//! Exact prices and volume derived from executed spot amounts in the sole
//! economic owner. Caller-supplied chain and actor claims remain unverified.
use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::economic::{EconomicError, EconomicState};
use crate::ledger::{PropertyId, TxId};
use crate::network_profile::TradeLayerMainnet;
use crate::spot::SpotFill;

pub const PRICE_POLICY: &str = "ExecutedSpotAmountsV1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpotPriceError {
    Economic(EconomicError),
    Decimal(DecimalError),
    SameAsset,
    UnknownProperty,
    InvalidRange,
    BeyondFinalizedHeight,
    InvalidExecution,
    Overflow,
}
impl core::fmt::Display for SpotPriceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SpotPriceError {}
impl From<EconomicError> for SpotPriceError {
    fn from(value: EconomicError) -> Self {
        Self::Economic(value)
    }
}
impl From<DecimalError> for SpotPriceError {
    fn from(value: DecimalError) -> Self {
        Self::Decimal(value)
    }
}
type Result<T> = core::result::Result<T, SpotPriceError>;

/// The direction is the caller's requested quote asset per base asset.
/// Atomic amounts are exact and remain available if a decimal price rounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedSpotPrice {
    pub network: TradeLayerMainnet,
    pub base: PropertyId,
    pub quote: PropertyId,
    pub from_height: u64,
    pub through_height: u64,
    pub first_fill_index: usize,
    pub last_fill_index: usize,
    pub sample_count: usize,
    pub base_atoms: u128,
    pub quote_atoms: u128,
}
impl ExecutedSpotPrice {
    /// Both assets use the existing Ledger's eight-decimal atomic units, so
    /// equal scales cancel. No f64 or rounded per-fill price is accumulated.
    pub fn price(&self, places: u32, rounding: Rounding) -> Result<Decimal> {
        if self.base_atoms == 0 || self.quote_atoms == 0 {
            return Err(SpotPriceError::InvalidExecution);
        }
        Ok(Decimal::from(self.quote_atoms).checked_div(
            &Decimal::from(self.base_atoms),
            places,
            rounding,
        )?)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedSpotSample {
    pub fill_index: usize,
    pub height: u64,
    pub sell_txid: TxId,
    pub buy_txid: TxId,
    pub trigger_txid: TxId,
    pub base_atoms: u64,
    pub quote_atoms: u64,
}
/// Borrows the actual owner; no imported price rows or mutable Ledger clone.
pub struct EconomicSpotPrices<'a> {
    source: &'a EconomicState,
}
impl<'a> EconomicSpotPrices<'a> {
    pub fn from_state(source: &'a EconomicState) -> Result<Self> {
        source.verify()?;
        Ok(Self::from_verified_state(source))
    }
    /// Only immutable views whose constructor already verified this same owner
    /// may avoid a second full replay verification for a query.
    pub(crate) fn from_verified_state(source: &'a EconomicState) -> Self {
        Self { source }
    }
    pub fn network(&self) -> TradeLayerMainnet {
        self.source.config().network
    }
    pub fn finalized_height(&self) -> Option<u64> {
        self.source.finalized_height()
    }
    fn validate(&self, base: PropertyId, quote: PropertyId, from: u64, to: u64) -> Result<()> {
        if base == quote {
            return Err(SpotPriceError::SameAsset);
        }
        if self.source.ledger().property(base).is_none()
            || self.source.ledger().property(quote).is_none()
        {
            return Err(SpotPriceError::UnknownProperty);
        }
        if from > to {
            return Err(SpotPriceError::InvalidRange);
        }
        if to > self.finalized_height().unwrap_or(0) {
            return Err(SpotPriceError::BeyondFinalizedHeight);
        }
        Ok(())
    }
    /// Global retained fill order breaks ties at the same block height.
    /// An absent trade returns None; it never manufactures a fallback price.
    pub fn last_price(
        &self,
        base: PropertyId,
        quote: PropertyId,
        through_height: u64,
    ) -> Result<Option<ExecutedSpotPrice>> {
        self.validate(base, quote, 0, through_height)?;
        let fills = self.source.joint().spot().fills();
        if let Some(index) = last_index(fills, base, quote, through_height) {
            let fill = &fills[index];
            if let Some((base_atoms, quote_atoms)) = orient(fill, base, quote)? {
                return Ok(Some(ExecutedSpotPrice {
                    network: self.network(),
                    base,
                    quote,
                    from_height: fill.at_height,
                    through_height: fill.at_height,
                    first_fill_index: index,
                    last_fill_index: index,
                    sample_count: 1,
                    base_atoms: u128::from(base_atoms),
                    quote_atoms: u128::from(quote_atoms),
                }));
            }
        }
        Ok(None)
    }
    /// Inclusive block range, weighted by actual executed base amounts. For
    /// reverse requests both atomic sums are swapped before division.
    pub fn vwap(
        &self,
        base: PropertyId,
        quote: PropertyId,
        from_height: u64,
        through_height: u64,
    ) -> Result<Option<ExecutedSpotPrice>> {
        self.validate(base, quote, from_height, through_height)?;
        let mut result: Option<ExecutedSpotPrice> = None;
        for (index, fill) in self.source.joint().spot().fills().iter().enumerate() {
            if fill.at_height < from_height || fill.at_height > through_height {
                continue;
            }
            if let Some((base_atoms, quote_atoms)) = orient(fill, base, quote)? {
                let row = result.get_or_insert_with(|| ExecutedSpotPrice {
                    network: self.network(),
                    base,
                    quote,
                    from_height,
                    through_height,
                    first_fill_index: index,
                    last_fill_index: index,
                    sample_count: 0,
                    base_atoms: 0,
                    quote_atoms: 0,
                });
                row.last_fill_index = index;
                row.sample_count = row
                    .sample_count
                    .checked_add(1)
                    .ok_or(SpotPriceError::Overflow)?;
                row.base_atoms = row
                    .base_atoms
                    .checked_add(u128::from(base_atoms))
                    .ok_or(SpotPriceError::Overflow)?;
                row.quote_atoms = row
                    .quote_atoms
                    .checked_add(u128::from(quote_atoms))
                    .ok_or(SpotPriceError::Overflow)?;
            }
        }
        Ok(result)
    }
    pub fn samples(
        &self,
        base: PropertyId,
        quote: PropertyId,
        from_height: u64,
        through_height: u64,
    ) -> Result<Vec<ExecutedSpotSample>> {
        self.validate(base, quote, from_height, through_height)?;
        let mut rows = Vec::new();
        for (index, fill) in self.source.joint().spot().fills().iter().enumerate() {
            if fill.at_height < from_height || fill.at_height > through_height {
                continue;
            }
            if let Some((base_atoms, quote_atoms)) = orient(fill, base, quote)? {
                rows.push(ExecutedSpotSample {
                    fill_index: index,
                    height: fill.at_height,
                    sell_txid: fill.sell_txid,
                    buy_txid: fill.buy_txid,
                    trigger_txid: fill.trigger_txid,
                    base_atoms,
                    quote_atoms,
                });
            }
        }
        Ok(rows)
    }
}
// Older restored owner formats establish valid fill heights but need not
// establish a monotone retained array. Height wins; retained position breaks
// same-height ties, including histories restored from those formats.
fn last_index(
    fills: &[SpotFill],
    base: PropertyId,
    quote: PropertyId,
    through: u64,
) -> Option<usize> {
    fills
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            f.at_height <= through
                && f.pair.base == base.min(quote)
                && f.pair.quote == base.max(quote)
        })
        .max_by_key(|(index, f)| (f.at_height, *index))
        .map(|(index, _)| index)
}
fn orient(fill: &SpotFill, base: PropertyId, quote: PropertyId) -> Result<Option<(u64, u64)>> {
    let amounts = if fill.pair.base == base && fill.pair.quote == quote {
        Some((fill.base_atoms, fill.quote_atoms))
    } else if fill.pair.base == quote && fill.pair.quote == base {
        Some((fill.quote_atoms, fill.base_atoms))
    } else {
        None
    };
    if amounts.is_some_and(|(base, quote)| base == 0 || quote == 0) {
        return Err(SpotPriceError::InvalidExecution);
    }
    Ok(amounts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_retained_order_cannot_replace_a_later_height_price() {
        let base = 5.try_into().unwrap();
        let quote = 6.try_into().unwrap();
        let fill = |height, tx| SpotFill {
            sell_txid: TxId::from_bytes([tx; 32]),
            buy_txid: TxId::from_bytes([tx + 1; 32]),
            pair: crate::spot::SpotPair::new(base, quote).unwrap(),
            base_atoms: 1,
            quote_atoms: 2,
            trigger_txid: TxId::from_bytes([tx; 32]),
            at_height: height,
        };
        let fills = [fill(2, 1), fill(1, 3), fill(2, 5), fill(1, 7)];
        assert_eq!(last_index(&fills, base, quote, 2), Some(2));
        assert_eq!(last_index(&fills, quote, base, 1), Some(3));
        assert_eq!(last_index(&fills, base, quote, 0), None);
    }
}
