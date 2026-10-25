//! Typed contract-series catalog and caller-driven collateral calculations.
//!
//! Numerical operation order follows the archived `contractRegistry.js` where
//! it has concrete behavior. This store neither authenticates an issuer nor
//! writes the sole ledger, AMM, channel or margin-map state. Collateral plans
//! are atomic local projections; source failure paths and repairs are described
//! in `docs/CONTRACT_REGISTRY.md`.

use crate::amm::{AmmError, AmmPool};
use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::ledger::{AccountId, PropertyId};
use core::fmt;
use std::collections::BTreeMap;

pub const MAX_CONTRACT_SERIES: usize = 65_536;
pub const MAX_EXPIRIES_PER_SERIES: usize = 4096;
pub const MAX_CONTRACT_PAIRS: usize = 64;
pub const MAX_CONTRACT_TICKER_BYTES: usize = 256;
pub const MAX_FUNDING_EVENTS: usize = 65_536;
pub const MAX_JS_SAFE_HEIGHT: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractId(u32);

impl ContractId {
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for ContractId {
    type Error = ContractError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(ContractError::InvalidIdentifier)
        } else {
            Ok(Self(value))
        }
    }
}

/// Source property zero means the base chain; ledger PropertyId stays nonzero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractAsset {
    BaseChain,
    Property(PropertyId),
}

impl ContractAsset {
    pub const fn get(self) -> u32 {
        match self {
            Self::BaseChain => 0,
            Self::Property(id) => id.get(),
        }
    }
}

impl From<u32> for ContractAsset {
    fn from(value: u32) -> Self {
        PropertyId::try_from(value).map_or(Self::BaseChain, Self::Property)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedContract {
    pub id: String,
    pub expiration_block: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractParameters {
    pub native: bool,
    pub underlying_oracle_id: Option<u32>,
    pub on_chain_data: Vec<(ContractAsset, ContractAsset)>,
    pub notional_property_id: ContractAsset,
    pub notional_value: Decimal,
    pub collateral_property_id: ContractAsset,
    pub leverage: Decimal,
    pub expiry_period: u64,
    pub series: u32,
    pub inverse: bool,
    pub fee: Decimal,
    pub whitelist: Option<u32>,
    pub ticker: Option<String>,
}

/// Values returned by the separate property/oracle registries. Native series
/// require the two property tickers (base-chain notional is always `LTC`). An
/// oracle series whose id differs from numeric zero (including null) requires
/// that oracle provider's nested ticker.
#[derive(Debug, Clone, Copy, Default)]
pub struct TickerContext<'a> {
    pub notional_ticker: Option<&'a str>,
    pub collateral_ticker: Option<&'a str>,
    pub oracle_ticker: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmConfiguration {
    pub position: Decimal,
    pub max_position: Decimal,
    pub max_quote_size: Decimal,
    pub contract_type: ContractId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSeries {
    pub id: ContractId,
    pub issuer: AccountId,
    pub ticker: String,
    pub parameters: ContractParameters,
    pub expired: Vec<GeneratedContract>,
    pub unexpired: Vec<GeneratedContract>,
    /// Source constructor's default values. Capital/orders belong to `amm`.
    pub amm: AmmConfiguration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InverseNativeMatch {
    pub contract_id: ContractId,
    pub series_id: ContractId,
    pub symbol: String,
    pub notional_property_id: ContractAsset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotionalValue {
    pub notional_value: Decimal,
    pub notional_per_contract: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractType {
    Native,
    Oracle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractError {
    InvalidIdentifier,
    NotFound,
    DuplicateIdentifier,
    MissingTicker,
    InvalidTicker,
    HeightOutOfRange,
    LimitExceeded,
    Arithmetic(DecimalError),
    Amm(AmmError),
    NonFiniteNumber,
    NegativeCollateral,
    InsufficientCollateral,
    MissingChannelBalance,
    UnsupportedSourcePath(&'static str),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ContractError {}
impl From<DecimalError> for ContractError {
    fn from(error: DecimalError) -> Self {
        Self::Arithmetic(error)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContractRegistry {
    series: BTreeMap<ContractId, ContractSeries>,
    insertion_order: Vec<ContractId>,
    funding_events: Vec<FundingEvent>,
    mod_flag: bool,
}

impl ContractRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_mod_flag(&mut self, flag: bool) {
        self.mod_flag = flag;
    }

    pub const fn mod_flag(&self) -> bool {
        self.mod_flag
    }

    /// Numeric typed IDs; source accepts additional parseInt string keys.
    pub fn next_id(&self) -> Result<ContractId, ContractError> {
        let next = self
            .series
            .last_key_value()
            .map_or(Some(1), |(id, _)| id.get().checked_add(1))
            .ok_or(ContractError::InvalidIdentifier)?;
        ContractId::try_from(next)
    }

    pub fn create_contract_series(
        &mut self,
        sender: AccountId,
        mut parameters: ContractParameters,
        tickers: TickerContext<'_>,
        block: u64,
    ) -> Result<ContractId, ContractError> {
        let id = self.next_id()?;
        let ticker = if parameters.native {
            let notional = if parameters.notional_property_id == ContractAsset::BaseChain {
                "LTC"
            } else {
                tickers
                    .notional_ticker
                    .ok_or(ContractError::MissingTicker)?
            };
            format!(
                "{}/{}-PERP",
                notional,
                tickers
                    .collateral_ticker
                    .ok_or(ContractError::MissingTicker)?
            )
        } else if parameters.underlying_oracle_id != Some(0) {
            format!(
                "{}-OPERP{}-{}",
                tickers.oracle_ticker.ok_or(ContractError::MissingTicker)?,
                parameters
                    .underlying_oracle_id
                    .map_or_else(|| "null".to_owned(), |oracle| oracle.to_string()),
                id.get()
            )
        } else {
            parameters
                .ticker
                .clone()
                .filter(|ticker| !ticker.is_empty())
                .unwrap_or_else(|| format!("{}-PERP", id.get()))
        };
        parameters.whitelist = Some(parameters.whitelist.unwrap_or(0));
        parameters.ticker = Some(ticker.clone());
        let unexpired = generate_contracts(parameters.expiry_period, parameters.series, id, block)?;
        self.insert_loaded(ContractSeries {
            id,
            issuer: sender,
            ticker,
            parameters,
            expired: Vec::new(),
            unexpired,
            amm: AmmConfiguration {
                position: Decimal::zero(),
                max_position: Decimal::from(1_u32),
                max_quote_size: Decimal::from(10_u32),
                contract_type: id,
            },
        })?;
        Ok(id)
    }

    /// Explicit typed restoration, independent of the source's inconsistent
    /// static/instance caches. Insertion order remains the query order.
    pub fn insert_loaded(&mut self, series: ContractSeries) -> Result<(), ContractError> {
        if self.series.contains_key(&series.id) {
            return Err(ContractError::DuplicateIdentifier);
        }
        if self.series.len() >= MAX_CONTRACT_SERIES
            || series.parameters.on_chain_data.len() > MAX_CONTRACT_PAIRS
            || series.expired.len() + series.unexpired.len() > MAX_EXPIRIES_PER_SERIES
        {
            return Err(ContractError::LimitExceeded);
        }
        if series.ticker.is_empty()
            || series.ticker.len() > MAX_CONTRACT_TICKER_BYTES
            || series.ticker.chars().any(char::is_control)
        {
            return Err(ContractError::InvalidTicker);
        }
        if series.parameters.expiry_period > MAX_JS_SAFE_HEIGHT
            || series
                .expired
                .iter()
                .chain(series.unexpired.iter())
                .any(|contract| contract.expiration_block > MAX_JS_SAFE_HEIGHT)
        {
            return Err(ContractError::HeightOutOfRange);
        }
        self.insertion_order.push(series.id);
        self.series.insert(series.id, series);
        Ok(())
    }

    pub fn get_contract_series(&self, id: ContractId) -> Option<&ContractSeries> {
        self.series.get(&id)
    }

    pub fn get_contract_info(&self, id_text: &str) -> Option<&ContractSeries> {
        parse_source_contract_id(id_text).and_then(|id| self.series.get(&id))
    }

    pub fn is_valid_series_id(&self, id: ContractId) -> bool {
        self.series.contains_key(&id)
    }

    pub fn all_contracts(&self) -> impl Iterator<Item = &ContractSeries> {
        self.insertion_order.iter().map(|id| &self.series[id])
    }

    pub fn all_perpetual_contracts(&self) -> Vec<ContractId> {
        self.all_contracts()
            .filter(|series| series.parameters.expiry_period == 0 || series.ticker.contains("PERP"))
            .map(|series| series.id)
            .collect()
    }

    /// The source's address argument is unused.
    pub fn contracts_for_collateral(&self, collateral: ContractAsset) -> Vec<ContractId> {
        self.all_contracts()
            .filter(|series| series.parameters.collateral_property_id == collateral)
            .map(|series| series.id)
            .collect()
    }

    /// Retains source operator precedence: a forward pair ignores notional;
    /// a reversed pair requires matching notional property.
    pub fn is_duplicate_native_contract(
        &self,
        collateral: ContractAsset,
        pair: (ContractAsset, ContractAsset),
        notional: ContractAsset,
    ) -> bool {
        self.all_contracts().any(|series| {
            let data = &series.parameters;
            data.native
                && data.collateral_property_id == collateral
                && data.on_chain_data.iter().any(|&existing| {
                    existing == pair
                        || (existing == (pair.1, pair.0) && data.notional_property_id == notional)
                })
        })
    }

    /// Despite the source method's name this filters collateralPropertyId.
    pub fn lookup_inverse_native_by_notional_pid(
        &self,
        base_pid: ContractAsset,
    ) -> Vec<InverseNativeMatch> {
        self.all_contracts()
            .filter(|series| {
                series.parameters.native
                    && series.parameters.inverse
                    && series.parameters.collateral_property_id == base_pid
            })
            .map(|series| InverseNativeMatch {
                contract_id: series.id,
                series_id: series.id,
                symbol: series.ticker.clone(),
                notional_property_id: series.parameters.notional_property_id,
            })
            .collect()
    }

    pub fn contract_type(&self, id: ContractId) -> Result<ContractType, ContractError> {
        Ok(if self.required(id)?.parameters.native {
            ContractType::Native
        } else {
            ContractType::Oracle
        })
    }

    pub fn is_native_contract(&self, id: ContractId) -> bool {
        self.series
            .get(&id)
            .is_some_and(|series| series.parameters.native)
    }

    pub fn is_oracle_contract(&self, id: ContractId) -> bool {
        self.series
            .get(&id)
            .is_some_and(|series| !series.parameters.native)
    }

    pub fn is_inverse(&self, id: ContractId) -> bool {
        self.series
            .get(&id)
            .is_some_and(|series| series.parameters.inverse)
    }

    pub fn oracle_id(&self, id: ContractId) -> Result<Option<u32>, ContractError> {
        Ok(self.required(id)?.parameters.underlying_oracle_id)
    }

    pub fn collateral_id(&self, id: ContractId) -> Result<ContractAsset, ContractError> {
        Ok(self.required(id)?.parameters.collateral_property_id)
    }

    pub fn amm_configuration(&self, id: ContractId) -> Result<&AmmConfiguration, ContractError> {
        Ok(&self.required(id)?.amm)
    }

    /// Instantiate source creation/default metadata in the typed AMM module.
    /// Imported LP shares and orders require a separate explicit AMM snapshot.
    pub fn instantiate_amm_pool(&self, id: ContractId) -> Result<AmmPool, ContractError> {
        let configuration = self.amm_configuration(id)?;
        let max_position = if configuration.max_position.is_zero() {
            Decimal::from(1_u32)
        } else {
            configuration.max_position.clone()
        };
        let max_quote_size = if configuration.max_quote_size.is_zero() {
            Decimal::from(10_u32)
        } else {
            configuration.max_quote_size.clone()
        };
        AmmPool::new(
            finite_binary64(&configuration.position)?,
            finite_binary64(&max_position)?,
            finite_binary64(&max_quote_size)?,
            configuration.contract_type.get(),
        )
        .map_err(ContractError::Amm)
    }

    /// Oracle fallback is explicit caller data. The source invokes its oracle
    /// provider even for native series with a non-null oracle id (including 0).
    pub fn notional_value(
        &self,
        id: ContractId,
        mark: Option<&Decimal>,
        oracle_fallback: Option<&Decimal>,
    ) -> Result<NotionalValue, ContractError> {
        let Some(series) = self.series.get(&id) else {
            return Ok(NotionalValue {
                notional_value: Decimal::zero(),
                notional_per_contract: Decimal::zero(),
            });
        };
        let data = &series.parameters;
        let zero = Decimal::zero();
        let mut mark = mark.unwrap_or(&zero);
        if *mark <= zero && data.underlying_oracle_id.is_some() {
            mark = oracle_fallback.unwrap_or(&zero);
        }
        // Only the native-inverse invalid-mark early return coerces this
        // metadata field through Number; the other branches return it intact.
        let number_notional_metadata = data.native && data.inverse && *mark <= zero;
        let value = if data.inverse {
            if *mark <= zero {
                zero
            } else {
                // Source divides first at 20 places, then multiplies.
                Decimal::from(1_u32)
                    .checked_div(mark, 20, Rounding::HalfUp)?
                    .checked_mul(&data.notional_value)?
                    .quantize(8, Rounding::HalfUp)?
            }
        } else if !data.native && *mark > zero {
            data.notional_value
                .checked_mul(mark)?
                .quantize(8, Rounding::HalfUp)?
        } else {
            data.notional_value.clone()
        };
        Ok(NotionalValue {
            notional_value: source_number(&value)?,
            notional_per_contract: if number_notional_metadata {
                source_number(&data.notional_value)?
            } else {
                data.notional_value.clone()
            },
        })
    }

    pub fn initial_margin(
        &self,
        id: ContractId,
        price: &Decimal,
    ) -> Result<Decimal, ContractError> {
        let data = &self.required(id)?.parameters;
        let leverage = if data.leverage.is_zero() {
            Decimal::from(10_u32)
        } else {
            data.leverage.clone()
        };
        let margin = if data.inverse {
            let divided = data
                .notional_value
                .checked_div(price, 20, Rounding::HalfUp)?
                .checked_div(&leverage, 20, Rounding::HalfUp)?;
            ceil_eight(&divided)?
        } else {
            data.notional_value
                .checked_mul(price)?
                .checked_div(&leverage, 20, Rounding::HalfUp)?
                .quantize(8, Rounding::HalfUp)?
        };
        source_number(&margin)
    }

    pub fn collateral_value(
        &self,
        id: ContractId,
        property_value: Option<&Decimal>,
        oracle_data: Option<&Decimal>,
    ) -> Result<Decimal, ContractError> {
        let data = &self.required(id)?.parameters;
        Ok(if data.collateral_property_id != ContractAsset::BaseChain {
            property_value.cloned().unwrap_or_default()
        } else if data.underlying_oracle_id.is_some_and(|oracle| oracle != 0) {
            oracle_data.cloned().unwrap_or_default()
        } else {
            Decimal::zero()
        })
    }

    /// The oracle source cannot calculate its own trade average; it throws.
    pub fn calculate_funding_rate(&self, id: ContractId) -> Result<f64, ContractError> {
        if self.is_oracle_contract(id) {
            Err(ContractError::UnsupportedSourcePath(
                "calculateFundingRate: undefined avgTradePrice / missing ./Oracles",
            ))
        } else {
            Ok(0.0)
        }
    }

    pub fn save_funding_event(&mut self, event: FundingEvent) -> Result<(), ContractError> {
        if self.funding_events.len() >= MAX_FUNDING_EVENTS {
            return Err(ContractError::LimitExceeded);
        }
        if event.block_height > MAX_JS_SAFE_HEIGHT {
            return Err(ContractError::HeightOutOfRange);
        }
        if !event.funding_rate.is_finite() {
            return Err(ContractError::NonFiniteNumber);
        }
        self.funding_events.push(event);
        Ok(())
    }

    pub fn funding_events(&self, id: ContractId) -> impl Iterator<Item = &FundingEvent> {
        self.funding_events
            .iter()
            .filter(move |event| event.contract_id == id)
    }

    fn required(&self, id: ContractId) -> Result<&ContractSeries, ContractError> {
        self.series.get(&id).ok_or(ContractError::NotFound)
    }
}

pub fn generate_contracts(
    expiry_period: u64,
    series_count: u32,
    id: ContractId,
    block: u64,
) -> Result<Vec<GeneratedContract>, ContractError> {
    if series_count as usize > MAX_EXPIRIES_PER_SERIES {
        return Err(ContractError::LimitExceeded);
    }
    if expiry_period > MAX_JS_SAFE_HEIGHT || block > MAX_JS_SAFE_HEIGHT {
        return Err(ContractError::HeightOutOfRange);
    }
    let mut contracts = Vec::with_capacity(series_count as usize);
    for index in 1..=u64::from(series_count) {
        let expiration_block = expiry_period
            .checked_mul(index)
            .and_then(|offset| block.checked_add(offset))
            .filter(|height| *height <= MAX_JS_SAFE_HEIGHT)
            .ok_or(ContractError::HeightOutOfRange)?;
        contracts.push(GeneratedContract {
            id: format!("{}-{expiration_block}", id.get()),
            expiration_block,
        });
    }
    Ok(contracts)
}

/// Source getContractInfo uses parseInt(text, 10), so expiry identifiers and
/// decimal suffixes resolve to their numeric series prefix.
pub fn parse_source_contract_id(text: &str) -> Option<ContractId> {
    let trimmed = text.trim_start_matches(js_whitespace);
    let unsigned = trimmed.strip_prefix('+').unwrap_or(trimmed);
    let digits: String = unsigned.chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse::<u32>()
        .ok()
        .and_then(|value| ContractId::try_from(value).ok())
}

/// Source getNextIdFromMap ignores nonnumeric keys and examines a decimal
/// prefix; unlike its DB reduce counterpart, one bad key does not poison max.
pub fn next_id_from_map_keys<'a>(
    keys: impl IntoIterator<Item = &'a str>,
) -> Result<ContractId, ContractError> {
    let mut maximum = 0;
    for key in keys {
        let trimmed = key.trim_start_matches(js_whitespace);
        let positive = trimmed.strip_prefix('+').unwrap_or(trimmed);
        if positive.starts_with('-') {
            continue;
        }
        // This source method omits the radix, unlike getContractInfo.
        let (digits, radix): (String, u32) = if let Some(hex) = positive
            .strip_prefix("0x")
            .or_else(|| positive.strip_prefix("0X"))
        {
            (
                hex.chars().take_while(char::is_ascii_hexdigit).collect(),
                16,
            )
        } else {
            (
                positive.chars().take_while(char::is_ascii_digit).collect(),
                10,
            )
        };
        if digits.is_empty() {
            continue;
        }
        let value =
            u32::from_str_radix(&digits, radix).map_err(|_| ContractError::InvalidIdentifier)?;
        maximum = maximum.max(value);
    }
    ContractId::try_from(
        maximum
            .checked_add(1)
            .ok_or(ContractError::InvalidIdentifier)?,
    )
}

fn js_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn source_number(value: &Decimal) -> Result<Decimal, ContractError> {
    finite_binary64(value)?
        .to_string()
        .parse()
        .map_err(ContractError::Arithmetic)
}

fn finite_binary64(value: &Decimal) -> Result<f64, ContractError> {
    let number: f64 = value
        .to_string()
        .parse()
        .map_err(|_| ContractError::NonFiniteNumber)?;
    if !number.is_finite() {
        return Err(ContractError::NonFiniteNumber);
    }
    Ok(number)
}

fn ceil_eight(value: &Decimal) -> Result<Decimal, ContractError> {
    let down = value.quantize(8, Rounding::Down)?;
    if !value.is_negative() && down != *value {
        Ok(down.checked_add(&"0.00000001".parse::<Decimal>()?)?)
    } else {
        Ok(down)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FundingEvent {
    pub contract_id: ContractId,
    pub funding_rate: f64,
    pub block_height: u64,
}

/// Source numeric prices use JavaScript's left-to-right binary64 addition.
pub fn calculate_average_price(prices: &[f64]) -> f64 {
    if prices.is_empty() {
        0.0
    } else {
        prices.iter().fold(0.0, |total, price| total + price) / prices.len() as f64
    }
}

pub fn calculate_funding_amount(contract_size: f64, funding_rate: f64) -> f64 {
    contract_size * funding_rate
}

/// Source `getLatestOracleData` returns the first matching document, without
/// sorting all records. The provider's order is therefore significant.
pub fn first_oracle_record<T>(records: &[(u32, T)], oracle_id: u32) -> Option<&T> {
    records
        .iter()
        .find(|(id, _)| *id == oracle_id)
        .map(|(_, value)| value)
}

/// Native price-at-block delegates only the first configured pair.
pub fn first_price_pair(
    series: &ContractSeries,
) -> Result<(ContractAsset, ContractAsset), ContractError> {
    if !series.parameters.native {
        return Err(ContractError::UnsupportedSourcePath(
            "getPriceAtBlock: oracle records indexed as arrays",
        ));
    }
    series
        .parameters
        .on_chain_data
        .first()
        .copied()
        .ok_or(ContractError::InvalidIdentifier)
}

/// Source's intended scan checks positive contracts only. Its actual instance
/// method throws first because MarginMap is undefined; this explicit-data
/// helper preserves the predicate without claiming the broken method works.
pub fn has_positive_open_position(contracts: &[Decimal]) -> bool {
    contracts
        .iter()
        .any(|contracts| *contracts > Decimal::zero())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollateralBalance {
    pub available: Decimal,
    pub reserved: Decimal,
    pub margin: Decimal,
}

impl CollateralBalance {
    fn validate(&self) -> Result<(), ContractError> {
        if self.available.is_negative() || self.reserved.is_negative() || self.margin.is_negative()
        {
            Err(ContractError::NegativeCollateral)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollateralMovement {
    pub available: Decimal,
    pub reserved: Decimal,
    pub margin: Decimal,
    pub channel: Decimal,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollateralPlan {
    pub after: CollateralBalance,
    pub channel_after: Option<Decimal>,
    pub movements: Vec<CollateralMovement>,
    /// Exact input of source marginMap.setInitialMargin, not a balance delta.
    pub position_initial_margin: Decimal,
    /// Amount passed to Channels.debitInitMarginFromChannel.
    pub channel_initial_margin_debit: Decimal,
}

#[derive(Debug, Clone)]
pub struct ReserveRequest {
    pub contract_id: ContractId,
    pub amount: Decimal,
    pub price: Decimal,
    pub in_process: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FeeInformation {
    pub buy_fee_from_reserve: bool,
    pub sell_fee_from_reserve: bool,
    pub buyer_fee: Decimal,
    pub seller_fee: Decimal,
}

#[derive(Debug, Clone)]
pub struct MarginRequest {
    pub sender: AccountId,
    pub contract_id: ContractId,
    pub amount: Decimal,
    pub price: Decimal,
    pub order_price: Decimal,
    pub buy_side: bool,
    /// Source call sites provide a total, not a per-contract amount.
    pub initial_margin: Decimal,
    pub maker: bool,
    pub channel: bool,
    pub fee_information: FeeInformation,
}

impl ContractRegistry {
    /// Local simulation of moveCollateralToReserve. None means insufficient
    /// available balance, exactly as the source's null return. No writes occur.
    pub fn plan_collateral_to_reserve(
        &self,
        balance: &CollateralBalance,
        request: &ReserveRequest,
    ) -> Result<Option<CollateralPlan>, ContractError> {
        balance.validate()?;
        // Math.abs first coerces source amount through binary64.
        let amount = source_number(&request.amount)?.abs();
        let initial = self.initial_margin(request.contract_id, &request.price)?;
        let total = round_eight_number(&initial.checked_mul(&amount)?)?;
        if total.is_negative() {
            return Err(ContractError::NegativeCollateral);
        }
        if balance.available < total {
            return Ok(None);
        }
        let mut plan = CollateralPlan::new(balance, None);
        plan.movement(CollateralMovement {
            available: total.negated(),
            reserved: total.clone(),
            margin: Decimal::zero(),
            channel: Decimal::zero(),
            reason: if request.in_process {
                "contractReserveFromMatchProcess"
            } else {
                "contractReserveInitMargin"
            },
        })?;
        plan.position_initial_margin = total;
        Ok(Some(plan))
    }

    /// Computes ordered tally/channel effects against explicit balances. Every
    /// arithmetic/insufficiency error discards this private plan. Integrators
    /// must commit it atomically with the real ledger and margin-map update.
    pub fn plan_collateral_to_margin(
        &self,
        balance: &CollateralBalance,
        channel_balance: Option<&Decimal>,
        request: &MarginRequest,
    ) -> Result<CollateralPlan, ContractError> {
        balance.validate()?;
        if channel_balance.is_some_and(Decimal::is_negative) {
            return Err(ContractError::NegativeCollateral);
        }
        // The source evaluates both even though the first result is unused.
        self.initial_margin(request.contract_id, &request.price)?;
        let compared_per_contract =
            self.initial_margin(request.contract_id, &request.order_price)?;
        let compared = round_eight_number(&compared_per_contract.checked_mul(&request.amount)?)?;
        let mut total = round_eight_number(&request.initial_margin)?;
        let mut plan = CollateralPlan::new(balance, channel_balance);

        if !request.channel && !request.maker {
            let diff = round_eight_number(&compared.checked_sub(&total)?)?;
            if !diff.is_negative() && !diff.is_zero() {
                plan.movement(CollateralMovement {
                    available: diff.clone(),
                    reserved: diff.negated(),
                    margin: Decimal::zero(),
                    channel: Decimal::zero(),
                    reason: "returnExcessMargin",
                })?;
            } else if diff.is_negative() {
                let top_up = round_eight_number(&diff.abs())?;
                if plan.after.available >= top_up {
                    plan.movement(CollateralMovement {
                        available: top_up.negated(),
                        reserved: top_up,
                        margin: Decimal::zero(),
                        channel: Decimal::zero(),
                        reason: "topUpReserveToFillPriceMargin",
                    })?;
                }
            }
        }

        let fees = &request.fee_information;
        if fees.buy_fee_from_reserve && request.buy_side {
            total = round_eight_number(&total.checked_sub(&fees.buyer_fee)?)?;
        } else if fees.sell_fee_from_reserve && !request.buy_side {
            total = round_eight_number(&total.checked_sub(&fees.seller_fee)?)?;
        }
        if total.is_negative() {
            return Err(ContractError::NegativeCollateral);
        }
        plan.position_initial_margin = total.clone();

        let virtual_amm =
            request.sender.as_str() == "amm" || request.sender.as_str().starts_with("amm:");
        if !request.channel && virtual_amm {
            return Ok(plan);
        }

        if request.channel {
            let channel_available = plan
                .channel_after
                .clone()
                .ok_or(ContractError::MissingChannelBalance)?;
            if channel_available >= total {
                plan.movement(CollateralMovement {
                    available: Decimal::zero(),
                    reserved: Decimal::zero(),
                    margin: Decimal::zero(),
                    channel: total.negated(),
                    reason: "debitChannelContractTradeInitMargin",
                })?;
                plan.movement(CollateralMovement {
                    available: Decimal::zero(),
                    reserved: Decimal::zero(),
                    margin: total.clone(),
                    channel: Decimal::zero(),
                    reason: "creditChannelContractTradeInitMargin",
                })?;
                plan.channel_initial_margin_debit = total;
            } else {
                // Typed caller supplies a real channel balance. The source
                // provider's ordinary shortfall branch uses exactly this gap.
                let shortfall = total.checked_sub(&channel_available)?;
                let debit = round_eight_number(&total.checked_sub(&shortfall)?)?;
                plan.movement(CollateralMovement {
                    available: Decimal::zero(),
                    reserved: Decimal::zero(),
                    margin: Decimal::zero(),
                    channel: debit.negated(),
                    reason: "contractTradeInitMargin",
                })?;
                plan.movement(CollateralMovement {
                    available: source_number(&shortfall)?.negated(),
                    reserved: Decimal::zero(),
                    margin: total,
                    channel: Decimal::zero(),
                    reason: "contractTradeInitMargin",
                })?;
                plan.channel_initial_margin_debit = debit;
            }
            return Ok(plan);
        }

        let reserve = plan.after.reserved.clone().min(total.clone());
        let reserve_debit = round_eight_number(&reserve)?;
        let remaining = round_eight_number(&total.checked_sub(&reserve)?)?;
        if remaining <= Decimal::zero() {
            plan.reserve_into_margin(&reserve_debit)?;
        } else if plan.after.available >= remaining {
            plan.movement(CollateralMovement {
                available: remaining.negated(),
                reserved: reserve_debit.negated(),
                margin: total,
                channel: Decimal::zero(),
                reason: "contractTradeInitMargin",
            })?;
        } else if plan.after.margin >= remaining {
            plan.reserve_into_margin(&reserve_debit)?;
        } else if plan.after.available.checked_add(&plan.after.margin)? >= remaining {
            let from_available = round_eight_number(&plan.after.available)?;
            plan.reserve_into_margin(&reserve_debit)?;
            if from_available > Decimal::zero() {
                plan.movement(CollateralMovement {
                    available: from_available.negated(),
                    reserved: Decimal::zero(),
                    margin: from_available,
                    channel: Decimal::zero(),
                    reason: "contractTradeInitMargin",
                })?;
            }
        } else {
            return Err(ContractError::InsufficientCollateral);
        }
        Ok(plan)
    }
}

impl CollateralPlan {
    fn new(balance: &CollateralBalance, channel: Option<&Decimal>) -> Self {
        Self {
            after: balance.clone(),
            channel_after: channel.cloned(),
            movements: Vec::new(),
            position_initial_margin: Decimal::zero(),
            channel_initial_margin_debit: Decimal::zero(),
        }
    }

    fn movement(&mut self, movement: CollateralMovement) -> Result<(), ContractError> {
        self.after.available = self.after.available.checked_add(&movement.available)?;
        self.after.reserved = self.after.reserved.checked_add(&movement.reserved)?;
        self.after.margin = self.after.margin.checked_add(&movement.margin)?;
        self.after.validate()?;
        if !movement.channel.is_zero() {
            let channel = self
                .channel_after
                .as_ref()
                .ok_or(ContractError::MissingChannelBalance)?
                .checked_add(&movement.channel)?;
            if channel.is_negative() {
                return Err(ContractError::InsufficientCollateral);
            }
            self.channel_after = Some(channel);
        }
        self.movements.push(movement);
        Ok(())
    }

    fn reserve_into_margin(&mut self, reserve: &Decimal) -> Result<(), ContractError> {
        if *reserve > Decimal::zero() {
            self.movement(CollateralMovement {
                available: Decimal::zero(),
                reserved: reserve.negated(),
                margin: reserve.clone(),
                channel: Decimal::zero(),
                reason: "contractTradeInitMargin",
            })?;
        }
        Ok(())
    }
}

fn round_eight_number(value: &Decimal) -> Result<Decimal, ContractError> {
    source_number(&value.quantize(8, Rounding::HalfUp)?)
}
