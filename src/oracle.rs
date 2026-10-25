//! Stateful finite-domain port of pinned `oracle.js`.
//!
//! Logical administrator checks do not authenticate a blockchain sender. Stake,
//! fraud and closure records describe source metadata and proposed external
//! effects; this store neither escrows tokens nor verifies fraud or DLC proofs.

use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::hash256::sha256;
use crate::ledger::AccountId;
use std::collections::BTreeMap;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d";
pub const MAX_ORACLES: usize = 4096;
pub const MAX_ORACLE_RECORDS: usize = 65_536;
pub const MAX_ORACLE_TEXT_BYTES: usize = 4096;
pub const MAX_RELAY_BLOB_BYTES: usize = 65_536;
pub const MAX_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleError {
    Decimal(DecimalError),
    UnknownOracle(u64),
    DuplicateOracle(u64),
    Unauthorized,
    InvalidId,
    InvalidBlock,
    InvalidField(&'static str),
    NonFiniteNumber,
    Capacity,
    ReplayConflict,
}

impl core::fmt::Display for OracleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OracleError {}
impl From<DecimalError> for OracleError {
    fn from(value: DecimalError) -> Self {
        Self::Decimal(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleConfig {
    pub name: String,
    pub ticker: String,
    pub url: String,
    pub admin: AccountId,
    pub backup: Option<AccountId>,
    pub clearlists: Vec<u32>,
    /// Source accepts finite Number lag, including negative/fractional values.
    pub lag: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleData {
    pub price: Decimal,
    pub high: Option<Decimal>,
    pub low: Option<Decimal>,
    pub close: Option<Decimal>,
}

impl OracleData {
    pub fn price(price: Decimal) -> Self {
        Self {
            price,
            high: None,
            low: None,
            close: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oracle {
    pub id: u64,
    pub config: OracleConfig,
    pub closed: bool,
    pub data: Option<OracleData>,
    pub last_published_block: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceRecord {
    pub oracle_id: u64,
    pub block: u64,
    pub data: OracleData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeRecord {
    pub oracle_id: u64,
    pub staker: AccountId,
    pub property_id: u32,
    pub amount: Decimal,
    pub block: u64,
}

/// This receipt is an unfunded metadata change, not proof of deposited tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeChange {
    pub previous: Option<StakeRecord>,
    pub current: StakeRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FraudRecord {
    pub oracle_id: u64,
    pub accused: AccountId,
    pub challenger: AccountId,
    pub slash_amount: Decimal,
    pub evidence_hash: String,
    pub block: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FraudClaim {
    pub oracle_id: u64,
    pub accused: AccountId,
    pub challenger: AccountId,
    pub slash_amount: Decimal,
    pub evidence_hash: String,
    pub block: u64,
}

/// Parsed bridge fields must come from the caller's bounded bridge adapter.
/// Neither possession nor reuse of these strings proves an attestation valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedRelayFields {
    pub signature_hex: String,
    pub state_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRecord {
    pub oracle_id: u64,
    pub sender: AccountId,
    pub relay_type: String,
    pub state_hash: String,
    pub dlc_ref: String,
    pub blob: String,
    pub block: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureUse {
    pub oracle_id: u64,
    pub signature_hex: String,
    pub state_hash: String,
    pub dlc_ref: String,
    pub first_seen_block: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleRecord {
    Price(PriceRecord),
    Stake(StakeRecord),
    Fraud(FraudRecord),
    Relay(RelayRecord),
    SignatureUse(SignatureUse),
}

impl OracleRecord {
    pub fn oracle_id(&self) -> u64 {
        match self {
            Self::Price(row) => row.oracle_id,
            Self::Stake(row) => row.oracle_id,
            Self::Fraud(row) => row.oracle_id,
            Self::Relay(row) => row.oracle_id,
            Self::SignatureUse(row) => row.oracle_id,
        }
    }

    pub fn block(&self) -> Option<u64> {
        match self {
            Self::Price(row) => Some(row.block),
            Self::Stake(row) => Some(row.block),
            Self::Fraud(row) => Some(row.block),
            Self::Relay(row) => Some(row.block),
            Self::SignatureUse(_) => None,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Price(row) => format!("oracle-{}-{}", row.oracle_id, row.block),
            Self::Stake(row) => {
                format!("oracle-stake-{}-{}", row.oracle_id, row.staker.as_str())
            }
            Self::Fraud(row) => format!(
                "oracle-fraud-{}-{}-{}",
                row.oracle_id,
                row.block,
                // JavaScript slice counts UTF-16 code units. Evidence keys in
                // this finite domain are ASCII so that key truncation is exact.
                row.evidence_hash.chars().take(24).collect::<String>()
            ),
            Self::Relay(row) => format!(
                "oracle-relay-{}-{}-{}",
                row.oracle_id, row.relay_type, row.block
            ),
            Self::SignatureUse(row) => {
                let digest = sha256(row.signature_hex.as_bytes());
                let hex = digest
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                format!("oracle-relay-sig-{}-{hex}", row.oracle_id)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Publication {
    Published(PriceRecord),
    Skipped { last_update_block: u64 },
}

/// The source closes metadata before requesting this external operation.
/// A caller must execute and fund it separately; constructing it pays nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceLiquidationRequest {
    pub oracle_id: u64,
    pub admin: AccountId,
    pub oracle_closure: bool,
}

/// Records retain insertion order, as in the source NeDB adapter fixtures.
/// This matters to latest-price ties and the unsorted history return value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OracleStore {
    oracles: Vec<Oracle>,
    persisted_oracles: Vec<Oracle>,
    records: Vec<OracleRecord>,
    last_update_block: BTreeMap<u64, u64>,
}

impl OracleStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn oracles(&self) -> &[Oracle] {
        &self.oracles
    }

    /// Source load merges persisted metadata into the cache, discarding cached
    /// publication fields because publishData does not persist that metadata.
    pub fn load_metadata(&mut self) {
        for persisted in &self.persisted_oracles {
            if let Some(cached) = self.oracles.iter_mut().find(|row| row.id == persisted.id) {
                *cached = persisted.clone();
            } else {
                self.oracles.push(persisted.clone());
            }
        }
    }

    pub fn get_all(&mut self) -> &[Oracle] {
        self.load_metadata();
        &self.oracles
    }

    /// Source lastOracleUpdateBlock is process-local and is lost on restart.
    /// Stored rows still determine the circuit-breaker baseline afterward.
    pub fn reset_publication_watermarks(&mut self) {
        self.last_update_block.clear();
    }

    pub fn records(&self) -> &[OracleRecord] {
        &self.records
    }

    pub fn get(&self, id: u64) -> Option<&Oracle> {
        self.oracles.iter().find(|oracle| oracle.id == id)
    }

    pub fn last_update_block(&self, id: u64) -> Option<u64> {
        self.last_update_block.get(&id).copied()
    }

    /// Source max-ID allocation; imported metadata may leave holes.
    pub fn next_id(&self) -> Result<u64, OracleError> {
        self.oracles
            .iter()
            .map(|oracle| oracle.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|&id| id <= MAX_JS_SAFE_INTEGER)
            .ok_or(OracleError::InvalidId)
    }

    /// Metadata creation only. The caller must enforce transaction activation,
    /// authorization, clearlist and bond requirements before treating it as live.
    pub fn create(&mut self, mut config: OracleConfig) -> Result<u64, OracleError> {
        let id = self.next_id()?;
        if config.name.is_empty() {
            config.name = if config.ticker.is_empty() {
                format!("oracle-{id}")
            } else {
                config.ticker.clone()
            };
        }
        if config.ticker.is_empty() {
            config.ticker = config.name.clone();
        }
        self.add(Oracle {
            id,
            config,
            closed: false,
            data: None,
            last_published_block: None,
        })?;
        Ok(id)
    }

    /// Typed import/add. Unlike source addOracle, duplicates cannot overwrite
    /// memory before an insert error; malformed inputs leave this store intact.
    pub fn add(&mut self, mut oracle: Oracle) -> Result<(), OracleError> {
        valid_id(oracle.id)?;
        valid_config(&oracle.config)?;
        if let Some(block) = oracle.last_published_block {
            valid_block(block)?;
        }
        oracle.config.lag = js_number(&oracle.config.lag)?;
        if self.get(oracle.id).is_some() {
            return Err(OracleError::DuplicateOracle(oracle.id));
        }
        if self.oracles.len() >= MAX_ORACLES {
            return Err(OracleError::Capacity);
        }
        self.persisted_oracles.push(oracle.clone());
        self.oracles.push(oracle);
        Ok(())
    }

    pub fn is_admin(&self, actor: &AccountId, id: u64) -> bool {
        self.get(id).is_some_and(|oracle| {
            &oracle.config.admin == actor || oracle.config.backup.as_ref() == Some(actor)
        })
    }

    /// Source verifyAdmin recognizes only the primary, unlike isAdmin.
    pub fn verify_admin(&self, id: u64, actor: &AccountId) -> bool {
        self.get(id)
            .is_some_and(|oracle| &oracle.config.admin == actor)
    }

    pub fn update_admin(
        &mut self,
        actor: &AccountId,
        id: u64,
        address: AccountId,
        backup: bool,
    ) -> Result<(), OracleError> {
        self.require_admin(actor, id)?;
        let oracle = self
            .persisted_oracles
            .iter_mut()
            .find(|row| row.id == id)
            .unwrap();
        if backup {
            oracle.config.backup = Some(address);
        } else {
            oracle.config.admin = address;
        }
        let refreshed = oracle.clone();
        *self.oracles.iter_mut().find(|row| row.id == id).unwrap() = refreshed;
        Ok(())
    }

    /// Latest valid typed price, default 1. Source does not filter closed feeds.
    pub fn price(&self, id: u64) -> Decimal {
        let mut latest: Option<&PriceRecord> = None;
        for record in &self.records {
            if let OracleRecord::Price(row) = record {
                if row.oracle_id == id && latest.is_none_or(|old| row.block > old.block) {
                    latest = Some(row);
                }
            }
        }
        latest
            .map(|row| row.data.price.clone())
            .unwrap_or_else(|| Decimal::from(1))
    }

    pub fn publish(
        &mut self,
        actor: &AccountId,
        id: u64,
        data: OracleData,
        block: u64,
    ) -> Result<Publication, OracleError> {
        self.require_admin(actor, id)?;
        self.publish_metadata(id, data, block)
    }

    /// Source storage operation, without authentication. Unknown IDs are
    /// rejected instead of creating incomplete implicit oracle metadata.
    /// Watermarks are committed only with a successful local state update,
    /// repairing source's failed-write poison. Closed feeds remain publishable,
    /// preserving source behavior; live admission must reject them separately.
    pub fn publish_metadata(
        &mut self,
        id: u64,
        mut data: OracleData,
        block: u64,
    ) -> Result<Publication, OracleError> {
        valid_block(block)?;
        if self.get(id).is_none() {
            return Err(OracleError::UnknownOracle(id));
        }
        if let Some(last) = self.last_update_block(id) {
            if last >= block {
                return Ok(Publication::Skipped {
                    last_update_block: last,
                });
            }
        }
        let last = self.price(id);
        // Source computes limits even for its sentinel price 1.
        let up = js_number(
            &last
                .checked_mul(&"1.05".parse()?)?
                .quantize(4, Rounding::HalfUp)?,
        )?;
        let down = js_number(
            &last
                .checked_mul(&"0.95".parse()?)?
                .quantize(4, Rounding::HalfUp)?,
        )?;
        if js_number(&last)? != Decimal::from(1) {
            // Relational comparisons convert strings/Numbers to Number.
            let number_price = js_number(&data.price)?;
            if number_price > up {
                data.price = up;
            } else if number_price < down {
                data.price = down;
            }
        }
        let row = PriceRecord {
            oracle_id: id,
            block,
            data: data.clone(),
        };
        self.upsert(OracleRecord::Price(row.clone()))?;
        let oracle = self.oracles.iter_mut().find(|row| row.id == id).unwrap();
        oracle.data = Some(data);
        oracle.last_published_block = Some(block);
        self.last_update_block.insert(id, block);
        Ok(Publication::Published(row))
    }

    /// Source saveOracleData bypasses the watermark and metadata refresh.
    pub fn save_price_record(&mut self, row: PriceRecord) -> Result<(), OracleError> {
        valid_id(row.oracle_id)?;
        valid_block(row.block)?;
        self.upsert(OracleRecord::Price(row))
    }

    pub fn history(&self, id: u64, start: u64, end: u64) -> Vec<&PriceRecord> {
        self.records
            .iter()
            .filter_map(|row| match row {
                OracleRecord::Price(row)
                    if row.oracle_id == id && row.block >= start && row.block <= end =>
                {
                    Some(row)
                }
                _ => None,
            })
            .collect()
    }

    /// Despite its name this is an equally weighted arithmetic mean of rows,
    /// not time weighting. Source omits a record-type filter: an in-range
    /// stake/fraud/relay row makes it return null. Signature-use rows lack a
    /// blockHeight and are excluded by the query.
    pub fn twap(
        &self,
        id: u64,
        block: u64,
        trailing_blocks: u64,
    ) -> Result<Option<Decimal>, OracleError> {
        valid_block(block)?;
        valid_block(trailing_blocks)?;
        let start = i128::from(block) - i128::from(trailing_blocks);
        let mut sum = Decimal::zero();
        let mut count = 0u64;
        for record in &self.records {
            if record.oracle_id() != id
                || record
                    .block()
                    .is_none_or(|height| i128::from(height) < start || height > block)
            {
                continue;
            }
            let OracleRecord::Price(row) = record else {
                return Ok(None);
            };
            sum = sum.checked_add(&row.data.price)?;
            count += 1;
        }
        if count == 0 {
            return Ok(None);
        }
        Ok(Some(js_number(
            &sum.checked_div(&Decimal::from(count), 20, Rounding::HalfUp)?
                .quantize(8, Rounding::HalfUp)?,
        )?))
    }

    /// Upsert unfunded metadata exactly as recordStake: each addition truncates
    /// to 8 places then crosses the JavaScript Number bridge. Signed amounts
    /// and property replacement are source behavior, not deposit validation.
    pub fn record_stake_metadata(
        &mut self,
        id: u64,
        staker: AccountId,
        property_id: u32,
        amount: &Decimal,
        block: u64,
    ) -> Result<StakeChange, OracleError> {
        valid_id(id)?;
        valid_block(block)?;
        let previous = self.stake(id, &staker).cloned();
        let old = previous
            .as_ref()
            .map(|row| row.amount.clone())
            .unwrap_or_default();
        let current = StakeRecord {
            oracle_id: id,
            staker,
            property_id,
            amount: js_number(&old.checked_add(amount)?.quantize(8, Rounding::Down)?)?,
            block,
        };
        self.upsert(OracleRecord::Stake(current.clone()))?;
        Ok(StakeChange { previous, current })
    }

    pub fn stake(&self, id: u64, staker: &AccountId) -> Option<&StakeRecord> {
        self.records.iter().find_map(|row| match row {
            OracleRecord::Stake(row) if row.oracle_id == id && &row.staker == staker => Some(row),
            _ => None,
        })
    }

    /// Records an unverified fraud allegation and proposed slash. Source has
    /// no evidence validation, recipient credit or bond debit here. Negative
    /// slashes increase recorded stake and are deliberately visible, never
    /// interpreted as a funded reward. Prefix collisions replace fraud rows.
    pub fn apply_fraud_metadata(&mut self, claim: FraudClaim) -> Result<Decimal, OracleError> {
        valid_id(claim.oracle_id)?;
        valid_block(claim.block)?;
        valid_text(&claim.evidence_hash, "evidence_hash")?;
        if !claim.evidence_hash.is_ascii() {
            return Err(OracleError::InvalidField("evidence_hash_ascii"));
        }
        let Some(mut stake) = self.stake(claim.oracle_id, &claim.accused).cloned() else {
            return Ok(Decimal::zero());
        };
        let slash = stake.amount.clone().min(js_number(&claim.slash_amount)?);
        stake.amount = js_number(
            &stake
                .amount
                .checked_sub(&slash)?
                .quantize(8, Rounding::Down)?,
        )?;
        stake.block = claim.block;
        let fraud = FraudRecord {
            oracle_id: claim.oracle_id,
            accused: claim.accused,
            challenger: claim.challenger,
            slash_amount: slash.clone(),
            evidence_hash: claim.evidence_hash,
            block: claim.block,
        };
        let mut replacement = self.clone();
        replacement.upsert(OracleRecord::Stake(stake))?;
        replacement.upsert(OracleRecord::Fraud(fraud))?;
        *self = replacement;
        Ok(slash)
    }

    /// Source replay bookkeeping only, with parser output supplied explicitly.
    /// Identical signature/state/DLC reuse is allowed. Relay rows at the same
    /// oracle/type/block overwrite; case and surrounding signature whitespace
    /// normalize, but hexadecimal validity is not checked by oracle.js.
    pub fn relay_metadata(
        &mut self,
        row: RelayRecord,
        parsed: ParsedRelayFields,
    ) -> Result<RelayRecord, OracleError> {
        valid_id(row.oracle_id)?;
        valid_block(row.block)?;
        for (value, label) in [
            (&row.relay_type, "relay_type"),
            (&row.state_hash, "state_hash"),
            (&row.dlc_ref, "dlc_ref"),
            (&parsed.signature_hex, "signature_hex"),
            (&parsed.state_hash, "parsed_state_hash"),
        ] {
            valid_text(value, label)?;
        }
        if row.blob.len() > MAX_RELAY_BLOB_BYTES {
            return Err(OracleError::InvalidField("relay_blob"));
        }
        if !parsed.signature_hex.is_ascii() {
            return Err(OracleError::InvalidField("signature_hex_ascii"));
        }
        let signature_hex = parsed.signature_hex.trim().to_ascii_lowercase();
        let state_hash = if row.state_hash.is_empty() {
            parsed.state_hash
        } else {
            row.state_hash.clone()
        };
        let mut replacement = self.clone();
        if !signature_hex.is_empty() {
            let signature = SignatureUse {
                oracle_id: row.oracle_id,
                signature_hex,
                state_hash,
                dlc_ref: row.dlc_ref.clone(),
                first_seen_block: row.block,
            };
            let key = OracleRecord::SignatureUse(signature.clone()).key();
            if let Some(OracleRecord::SignatureUse(previous)) =
                self.records.iter().find(|item| item.key() == key)
            {
                if previous.state_hash != signature.state_hash
                    || previous.dlc_ref != signature.dlc_ref
                {
                    return Err(OracleError::ReplayConflict);
                }
            } else {
                replacement.upsert(OracleRecord::SignatureUse(signature))?;
            }
        }
        replacement.upsert(OracleRecord::Relay(row.clone()))?;
        *self = replacement;
        Ok(row)
    }

    pub fn close(
        &mut self,
        actor: &AccountId,
        id: u64,
    ) -> Result<InsuranceLiquidationRequest, OracleError> {
        self.require_admin(actor, id)?;
        let oracle = self
            .persisted_oracles
            .iter_mut()
            .find(|row| row.id == id)
            .unwrap();
        oracle.closed = true;
        let request = InsuranceLiquidationRequest {
            oracle_id: id,
            admin: oracle.config.admin.clone(),
            oracle_closure: true,
        };
        let refreshed = oracle.clone();
        *self.oracles.iter_mut().find(|row| row.id == id).unwrap() = refreshed;
        Ok(request)
    }

    fn require_admin(&self, actor: &AccountId, id: u64) -> Result<(), OracleError> {
        if self.get(id).is_none() {
            Err(OracleError::UnknownOracle(id))
        } else if !self.is_admin(actor, id) {
            Err(OracleError::Unauthorized)
        } else {
            Ok(())
        }
    }

    fn upsert(&mut self, row: OracleRecord) -> Result<(), OracleError> {
        let key = row.key();
        if let Some(index) = self.records.iter().position(|item| item.key() == key) {
            self.records[index] = row;
        } else {
            if self.records.len() >= MAX_ORACLE_RECORDS {
                return Err(OracleError::Capacity);
            }
            self.records.push(row);
        }
        Ok(())
    }
}

fn valid_id(id: u64) -> Result<(), OracleError> {
    if id == 0 || id > MAX_JS_SAFE_INTEGER {
        Err(OracleError::InvalidId)
    } else {
        Ok(())
    }
}

fn valid_block(block: u64) -> Result<(), OracleError> {
    if block > MAX_JS_SAFE_INTEGER {
        Err(OracleError::InvalidBlock)
    } else {
        Ok(())
    }
}

fn valid_text(value: &str, label: &'static str) -> Result<(), OracleError> {
    if value.len() > MAX_ORACLE_TEXT_BYTES {
        Err(OracleError::InvalidField(label))
    } else {
        Ok(())
    }
}

fn valid_config(config: &OracleConfig) -> Result<(), OracleError> {
    for (value, label) in [
        (&config.name, "name"),
        (&config.ticker, "ticker"),
        (&config.url, "url"),
    ] {
        valid_text(value, label)?;
    }
    if config.clearlists.len() > MAX_ORACLES {
        return Err(OracleError::Capacity);
    }
    Ok(())
}

/// The source explicitly calls Number/toNumber at these boundaries. This
/// round-trip retains finite IEEE-754 rounding using Rust's shortest decimal
/// representation; arbitrary finite BigNumber inputs elsewhere stay decimal.
fn js_number(value: &Decimal) -> Result<Decimal, OracleError> {
    let number: f64 = value
        .to_string()
        .parse()
        .map_err(|_| OracleError::NonFiniteNumber)?;
    if !number.is_finite() {
        return Err(OracleError::NonFiniteNumber);
    }
    Ok(number.to_string().parse()?)
}
