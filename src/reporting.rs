//! Network-scoped, atomic reporting of the existing offline spot fills.
//!
//! SpotFill contains no addresses or native-coin valuation. Callers must supply
//! that metadata explicitly; it is never inferred from transaction IDs. These
//! indexes report observations and cannot authorize or mutate a spot exchange.

use crate::ledger::AccountId;
use crate::network_profile::TradeLayerMainnet;
use crate::spot::SpotFill;
use crate::trade_history::{TradeDetails, TradeHistory, TradeHistoryError, TradeId, TradeRecord};
use crate::volume_index::{
    VolumeError, VolumeHistory, VolumeKind, VolumePair, VolumeRecord, WeightedPrice, ATOMS_PER_UNIT,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotFillObservation {
    pub network: TradeLayerMainnet,
    pub fill: SpotFill,
    pub trade_id: TradeId,
    pub event_index: u32,
    pub buyer: AccountId,
    pub seller: AccountId,
    /// Caller-supplied valuation in the pinned volume module's LTC units.
    /// No conversion, oracle lookup, or authentication is performed.
    pub ltc_volume_atoms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportingError {
    WrongNetwork,
    InvalidFill,
    DuplicateObservation,
    History(TradeHistoryError),
    Volume(VolumeError),
}

impl core::fmt::Display for ReportingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ReportingError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineSpotReports {
    network: TradeLayerMainnet,
    history: TradeHistory,
    volume: VolumeHistory,
}

impl OfflineSpotReports {
    pub fn new(network: TradeLayerMainnet, capacity: usize) -> Result<Self, ReportingError> {
        Ok(Self {
            network,
            history: TradeHistory::new(),
            volume: VolumeHistory::new(capacity).map_err(ReportingError::Volume)?,
        })
    }

    pub const fn network(&self) -> TradeLayerMainnet {
        self.network
    }

    pub fn history(&self) -> &TradeHistory {
        &self.history
    }

    pub fn volume(&self) -> &VolumeHistory {
        &self.volume
    }

    /// Insert both reports or neither. Retransmitted observations are rejected
    /// rather than incrementing totals twice. A replacement suffix must first
    /// be removed explicitly with rollback_after. Full-state staging favors
    /// auditability over throughput and is not a production indexer design.
    pub fn record_spot_fill(
        &mut self,
        observation: SpotFillObservation,
    ) -> Result<(), ReportingError> {
        if observation.network != self.network {
            return Err(ReportingError::WrongNetwork);
        }
        let fill = &observation.fill;
        if fill.pair.base >= fill.pair.quote || fill.base_atoms == 0 || fill.quote_atoms == 0 {
            return Err(ReportingError::InvalidFill);
        }
        if self.history.get(observation.trade_id).is_some() {
            return Err(ReportingError::DuplicateObservation);
        }
        // Preserve both exact traded amounts in history. VolumeIndex's price
        // field is a decimal with eight places, rounded HALF_UP explicitly.
        let price_atoms = WeightedPrice {
            numerator: u128::from(fill.quote_atoms) * u128::from(ATOMS_PER_UNIT),
            denominator: u128::from(fill.base_atoms),
        }
        .price_atoms_half_up()
        .map_err(ReportingError::Volume)?;
        let history = TradeRecord {
            id: observation.trade_id,
            block_height: fill.at_height,
            event_index: observation.event_index,
            buyer: observation.buyer,
            seller: observation.seller,
            buyer_tx: fill.buy_txid,
            seller_tx: fill.sell_txid,
            details: TradeDetails::Token {
                property1: fill.pair.base,
                property2: fill.pair.quote,
                amount1_atoms: u128::from(fill.base_atoms),
                amount2_atoms: u128::from(fill.quote_atoms),
            },
        };
        let volume = VolumeRecord {
            event_id: format!(
                "spot:{}:{}",
                self.network.tag(),
                crate::read_only::hex_bytes(&observation.trade_id.0)
            ),
            source_id: format!("{}-{}", fill.pair.base.get(), fill.pair.quote.get()),
            pair: VolumePair {
                first: fill.pair.base.get(),
                second: fill.pair.quote.get(),
            },
            block_height: fill.at_height,
            raw_volume_atoms: fill.base_atoms,
            ltc_volume_atoms: observation.ltc_volume_atoms,
            price_atoms,
            kind: VolumeKind::Token,
        };
        let mut candidate = self.clone();
        candidate
            .history
            .insert(history)
            .map_err(ReportingError::History)?;
        candidate
            .volume
            .insert(volume)
            .map_err(ReportingError::Volume)?;
        *self = candidate;
        Ok(())
    }

    /// Remove the same suffix from both indexes. The retained height is a
    /// caller assertion; this does not select a canonical chain or undo trades.
    pub fn rollback_after(&mut self, retained_height: u64) -> usize {
        let removed = self.history.rollback_after(retained_height);
        let volume_removed = self.volume.rollback_after(retained_height);
        debug_assert_eq!(removed, volume_removed);
        removed
    }
}
