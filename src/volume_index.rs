//! Bounded, caller-supplied volume observations and exact VWAP arithmetic.
//!
//! Source characterization: tradelayer.js@dca0888b `src/volumeIndex.js`.
//! Amounts and prices use eight decimal places; all integer calculations are
//! checked. This is an offline reporting projection, never reward issuance,
//! ledger mutation, authenticated price discovery, or a production DB adapter.

use std::collections::{BTreeMap, BTreeSet};

pub const ATOMS_PER_UNIT: u64 = 100_000_000;
pub const MAX_VOLUME_RECORDS: usize = 65_536;
pub const MAX_IDENTIFIER_BYTES: usize = 256;
pub const PINNED_SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const PINNED_SOURCE_SHA256: &str =
    "d4d4329254125ce8e4217d22b0ba57a1581c6279df999af60281b0365dffb08c";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeError {
    InvalidCapacity,
    CapacityExceeded,
    InvalidIdentifier,
    SameAsset,
    DuplicateEvent,
    InvalidRange,
    ArithmeticOverflow,
    InvalidShare,
}

impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VolumeError {}

/// Pair orientation is preserved; constructing a pair does not authorize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VolumePair {
    pub first: u32,
    pub second: u32,
}

impl VolumePair {
    fn validate(self) -> Result<(), VolumeError> {
        if self.first == self.second {
            Err(VolumeError::SameAsset)
        } else {
            Ok(())
        }
    }

    pub const fn reversed(self) -> Self {
        Self {
            first: self.second,
            second: self.first,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    Utxo,
    Token,
    Contract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeRecord {
    /// Unique observation key, independent of the source's overwritable `_id`.
    pub event_id: String,
    pub source_id: String,
    pub pair: VolumePair,
    pub block_height: u64,
    /// Explicit volume weight for VWAP, in eight-decimal units.
    pub raw_volume_atoms: u64,
    pub ltc_volume_atoms: u64,
    pub price_atoms: u64,
    pub kind: VolumeKind,
}

impl VolumeRecord {
    fn validate(&self) -> Result<(), VolumeError> {
        for id in [&self.event_id, &self.source_id] {
            if id.is_empty() || id.len() > MAX_IDENTIFIER_BYTES || id.chars().any(char::is_control)
            {
                return Err(VolumeError::InvalidIdentifier);
            }
        }
        self.pair.validate()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VolumeTotals {
    pub global_atoms: u128,
    /// Type-based subtotal, matching updateCumulativeVolumes's `type === UTXO`.
    pub utxo_atoms: u128,
    /// Explicit projection; the pinned updater never writes its contract total.
    pub contract_atoms: u128,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceBlockVolumes {
    pub global_atoms: u128,
    /// Exact source classification: `_id.toString().includes('0')`.
    pub ltc_pairs_atoms: u128,
}

/// Numerator is sum(volume_atoms * price_atoms), denominator is volume_atoms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeightedPrice {
    pub numerator: u128,
    pub denominator: u128,
}

impl WeightedPrice {
    /// Eight-decimal ROUND_HALF_UP, as in the pinned getVWAP BigNumber call.
    pub fn price_atoms_half_up(self) -> Result<u64, VolumeError> {
        if self.denominator == 0 {
            return Err(VolumeError::InvalidRange);
        }
        let whole = self.numerator / self.denominator;
        let remainder = self.numerator % self.denominator;
        let threshold = self.denominator / 2 + self.denominator % 2;
        let rounded = whole
            .checked_add(u128::from(remainder >= threshold))
            .ok_or(VolumeError::ArithmeticOverflow)?;
        u64::try_from(rounded).map_err(|_| VolumeError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VwapObservation {
    pub volume_atoms: u64,
    pub price_atoms: u64,
}

/// Typed counterpart of calculateVWAP: amount1/amount2 can be swapped by the
/// caller for the source's `contract` mode. Returns the unrounded exact ratio.
/// Unlike JS Numbers, this rejects overflow and cannot represent NaN/negative.
pub fn calculate_vwap(data: &[VwapObservation]) -> Result<Option<WeightedPrice>, VolumeError> {
    if data.len() > MAX_VOLUME_RECORDS {
        return Err(VolumeError::CapacityExceeded);
    }
    weighted_price(data.iter().copied())
}

fn weighted_price(
    data: impl IntoIterator<Item = VwapObservation>,
) -> Result<Option<WeightedPrice>, VolumeError> {
    let mut numerator = 0u128;
    let mut denominator = 0u128;
    for entry in data {
        let weight = u128::from(entry.volume_atoms);
        denominator = denominator
            .checked_add(weight)
            .ok_or(VolumeError::ArithmeticOverflow)?;
        numerator = numerator
            .checked_add(weight * u128::from(entry.price_atoms))
            .ok_or(VolumeError::ArithmeticOverflow)?;
    }
    Ok((denominator != 0).then_some(WeightedPrice {
        numerator,
        denominator,
    }))
}

/// Exact fee-linked ROUND_DOWN for an explicitly supplied rational share.
/// This does not evaluate the source's logarithmic attenuation or issue tokens.
pub fn fee_linked_reward_atoms(
    fee_atoms: u64,
    share_numerator: u64,
    share_denominator: u64,
) -> Result<u64, VolumeError> {
    if share_denominator == 0 || share_numerator > share_denominator {
        return Err(VolumeError::InvalidShare);
    }
    let result =
        u128::from(fee_atoms) * u128::from(share_numerator) / u128::from(share_denominator);
    u64::try_from(result).map_err(|_| VolumeError::ArithmeticOverflow)
}

/// Complete event observations rather than JS's mutable per-pair DB documents.
/// Duplicate IDs are rejected, out-of-order insertion is allowed, and queries
/// sort by (height, event_id). No caller input is treated as trusted chain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeHistory {
    capacity: usize,
    records: BTreeMap<String, VolumeRecord>,
}

impl VolumeHistory {
    pub fn new(capacity: usize) -> Result<Self, VolumeError> {
        if capacity == 0 || capacity > MAX_VOLUME_RECORDS {
            return Err(VolumeError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            records: BTreeMap::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn insert(&mut self, record: VolumeRecord) -> Result<(), VolumeError> {
        record.validate()?;
        if self.records.contains_key(&record.event_id) {
            return Err(VolumeError::DuplicateEvent);
        }
        if self.records.len() == self.capacity {
            return Err(VolumeError::CapacityExceeded);
        }
        self.records.insert(record.event_id.clone(), record);
        Ok(())
    }

    /// Validates every element before mutating, including duplicates in a batch.
    pub fn insert_batch(&mut self, records: &[VolumeRecord]) -> Result<(), VolumeError> {
        if records.len() > self.capacity - self.records.len() {
            return Err(VolumeError::CapacityExceeded);
        }
        let mut ids = BTreeSet::new();
        for record in records {
            record.validate()?;
            if self.records.contains_key(&record.event_id) || !ids.insert(&record.event_id) {
                return Err(VolumeError::DuplicateEvent);
            }
        }
        for record in records {
            self.records.insert(record.event_id.clone(), record.clone());
        }
        Ok(())
    }

    pub fn sample_range(&self, start: u64, end: u64) -> Result<Vec<&VolumeRecord>, VolumeError> {
        if start > end {
            return Err(VolumeError::InvalidRange);
        }
        let mut records: Vec<_> = self
            .records
            .values()
            .filter(|r| (start..=end).contains(&r.block_height))
            .collect();
        records.sort_by(|a, b| (a.block_height, &a.event_id).cmp(&(b.block_height, &b.event_id)));
        Ok(records)
    }

    /// Inclusive as-of projection. JS getCumulativeVolumes uses `< block` and
    /// overwrites its only cumulative document; its historical loss is not copied.
    pub fn totals_through(&self, height: u64) -> Result<VolumeTotals, VolumeError> {
        let mut totals = VolumeTotals::default();
        for record in self.records.values().filter(|r| r.block_height <= height) {
            let amount = u128::from(record.ltc_volume_atoms);
            totals.global_atoms = totals
                .global_atoms
                .checked_add(amount)
                .ok_or(VolumeError::ArithmeticOverflow)?;
            let subtotal = match record.kind {
                VolumeKind::Utxo => &mut totals.utxo_atoms,
                VolumeKind::Contract => &mut totals.contract_atoms,
                VolumeKind::Token => continue,
            };
            *subtotal = subtotal
                .checked_add(amount)
                .ok_or(VolumeError::ArithmeticOverflow)?;
        }
        Ok(totals)
    }

    pub fn source_block_volumes(&self, height: u64) -> Result<SourceBlockVolumes, VolumeError> {
        let mut totals = SourceBlockVolumes::default();
        for record in self.records.values().filter(|r| r.block_height == height) {
            let amount = u128::from(record.ltc_volume_atoms);
            totals.global_atoms = totals
                .global_atoms
                .checked_add(amount)
                .ok_or(VolumeError::ArithmeticOverflow)?;
            if record.source_id.contains('0') {
                totals.ltc_pairs_atoms = totals
                    .ltc_pairs_atoms
                    .checked_add(amount)
                    .ok_or(VolumeError::ArithmeticOverflow)?;
            }
        }
        Ok(totals)
    }

    /// Directional pair VWAP over inclusive blocks. Does not silently invert
    /// price units when the opposite pair orientation has observations.
    pub fn vwap(
        &self,
        pair: VolumePair,
        start: u64,
        end: u64,
    ) -> Result<Option<WeightedPrice>, VolumeError> {
        pair.validate()?;
        let records = self.sample_range(start, end)?;
        weighted_price(
            records
                .into_iter()
                .filter(|r| r.pair == pair)
                .map(|r| VwapObservation {
                    volume_atoms: r.raw_volume_atoms,
                    price_atoms: r.price_atoms,
                }),
        )
    }

    /// Source getVWAP's two-pair selection, including its lack of reciprocal
    /// conversion for reverse-pair prices. Kept explicit for characterization.
    pub fn source_vwap(
        &self,
        pair: VolumePair,
        start: u64,
        end: u64,
    ) -> Result<Option<WeightedPrice>, VolumeError> {
        pair.validate()?;
        let records = self.sample_range(start, end)?;
        weighted_price(
            records
                .into_iter()
                .filter(|r| r.pair == pair || r.pair == pair.reversed())
                .map(|r| VwapObservation {
                    volume_atoms: r.raw_volume_atoms,
                    price_atoms: r.price_atoms,
                }),
        )
    }

    /// Remove observations strictly above the retained height. Re-insertion of
    /// a replacement suffix is allowed; retained records remain byte-identical.
    pub fn rollback_after(&mut self, retain_through: u64) -> usize {
        let previous = self.records.len();
        self.records
            .retain(|_, record| record.block_height <= retain_through);
        previous - self.records.len()
    }
}

include!("volume_index_source_impl.rs");
