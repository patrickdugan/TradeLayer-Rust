//! Isolated AMM capital/share accounting and native-number pricing helpers.
//!
//! The pinned JavaScript uses binary64 throughout this module. Source quirks
//! remain in its formulas; explicit receivers, bounded state, validated imports
//! and atomic capital changes are deliberate typed repairs. Balance movements
//! are a journal for a caller, never proof of an authorized ledger transfer.

use crate::decimal::{Decimal, Rounding};
use core::str::FromStr;
use std::collections::BTreeMap;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_LF_SHA256: &str =
    "bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1";
pub const MAX_LPS: usize = 4096;
pub const MAX_AMM_ORDERS: usize = 4096;
pub const MAX_AMMS: usize = 4096;
pub const MAX_METADATA_BYTES: usize = 8192;
pub const MAX_SAFE_BLOCK: u64 = (1u64 << 53) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmmError {
    NonFinite,
    InvalidLimits,
    InvalidAmount,
    InvalidSecondAmount,
    InsufficientShares,
    MaximumPosition,
    CapacityExceeded,
    InvalidIdentifier,
    InvalidBlock,
    Arithmetic,
    InvalidSnapshot,
    InvalidContractType,
    NonExecutableQuote,
    NotFound,
}
impl core::fmt::Display for AmmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AmmError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
impl Side {
    pub const fn as_source_name(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LpShare {
    pub address: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiquidityOrder {
    pub size: f64,
    pub side: Side,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmmSnapshot {
    pub position: f64,
    pub max_position: f64,
    pub max_quote_size: f64,
    pub contract_id: u32,
    /// Insertion order is preserved; address queries use JS Object.keys order.
    pub lp_addresses: Vec<LpShare>,
    pub amm_orders: Vec<LiquidityOrder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapitalAsset {
    Contract(String),
    TokenPair { first: String, second: String },
}
impl CapitalAsset {
    fn ids(&self) -> Result<(&str, Option<&str>), AmmError> {
        let (first, second) = match self {
            Self::Contract(first) => (first.as_str(), None),
            Self::TokenPair { first, second } => (first.as_str(), Some(second.as_str())),
        };
        identifier(first)?;
        if let Some(second) = second {
            identifier(second)?;
        }
        Ok((first, second))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementKind {
    Pledge,
    Issue,
    Burn,
    Redeem,
}
impl MovementKind {
    pub const fn as_source_name(self) -> &'static str {
        match self {
            Self::Pledge => "AMMPledge",
            Self::Issue => "LPIssue",
            Self::Burn => "LPBurn",
            Self::Redeem => "AMMRedeem",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceMovement {
    pub address: String,
    pub property: String,
    pub amount: f64,
    pub kind: MovementKind,
    pub block: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmmPool {
    state: AmmSnapshot,
}

impl AmmPool {
    /// Numeric contract IDs follow the actual contractRegistry constructor path.
    pub fn new(
        position: f64,
        max_position: f64,
        max_quote_size: f64,
        contract_id: u32,
    ) -> Result<Self, AmmError> {
        Self::from_snapshot(AmmSnapshot {
            position,
            max_position,
            max_quote_size,
            contract_id,
            lp_addresses: Vec::new(),
            amm_orders: Vec::new(),
        })
    }

    /// Source constructors/hydration do not perform these validation checks.
    /// Inventory is signed and need not equal capital shares, as in the source.
    pub fn from_snapshot(state: AmmSnapshot) -> Result<Self, AmmError> {
        finite(state.position)?;
        finite(state.max_position)?;
        finite(state.max_quote_size)?;
        if state.max_position < 0.0 || state.max_quote_size < 0.0 {
            return Err(AmmError::InvalidLimits);
        }
        if state.lp_addresses.len() > MAX_LPS || state.amm_orders.len() > MAX_AMM_ORDERS {
            return Err(AmmError::CapacityExceeded);
        }
        let mut seen = BTreeMap::new();
        for share in &state.lp_addresses {
            address(&share.address)?;
            if !share.amount.is_finite() || share.amount <= 0.0 {
                return Err(AmmError::InvalidSnapshot);
            }
            if seen.insert(&share.address, ()).is_some() {
                return Err(AmmError::InvalidSnapshot);
            }
        }
        for order in &state.amm_orders {
            finite(order.size)?;
        }
        Ok(Self { state })
    }

    pub fn snapshot(&self) -> &AmmSnapshot {
        &self.state
    }
    pub fn position(&self) -> f64 {
        self.state.position
    }
    pub fn max_position(&self) -> f64 {
        self.state.max_position
    }
    pub fn max_quote_size(&self) -> f64 {
        self.state.max_quote_size
    }
    pub fn contract_id(&self) -> u32 {
        self.state.contract_id
    }
    pub fn share(&self, owner: &str) -> f64 {
        self.state
            .lp_addresses
            .iter()
            .find(|share| share.address == owner)
            .map_or(0.0, |share| share.amount)
    }
    pub fn lp_addresses(&self) -> Vec<&str> {
        source_object_order(&self.state.lp_addresses)
            .into_iter()
            .map(|share| share.address.as_str())
            .collect()
    }

    /// Mirrors successful instance addCapital journal order. Both amounts and
    /// the complete next state are checked before any mutation (a source repair).
    pub fn add_capital(
        &mut self,
        owner: &str,
        asset: &CapitalAsset,
        amount: f64,
        second_amount: f64,
        block: u64,
    ) -> Result<Vec<BalanceMovement>, AmmError> {
        self.change_capital(owner, asset, amount, second_amount, block, false)
    }

    pub fn redeem_capital(
        &mut self,
        owner: &str,
        asset: &CapitalAsset,
        amount: f64,
        second_amount: f64,
        block: u64,
    ) -> Result<Vec<BalanceMovement>, AmmError> {
        self.change_capital(owner, asset, amount, second_amount, block, true)
    }

    fn change_capital(
        &mut self,
        owner: &str,
        asset: &CapitalAsset,
        amount: f64,
        second_amount: f64,
        block: u64,
        redeem: bool,
    ) -> Result<Vec<BalanceMovement>, AmmError> {
        address(owner)?;
        valid_block(block)?;
        let (first, second) = asset.ids()?;
        if !amount.is_finite() || amount <= 0.0 {
            return Err(AmmError::InvalidAmount);
        }
        if second.is_some() && (!second_amount.is_finite() || second_amount <= 0.0) {
            return Err(AmmError::InvalidSecondAmount);
        }
        let current = self.share(owner);
        if redeem && current < amount {
            return Err(AmmError::InsufficientShares);
        }
        let position = finite(if redeem {
            self.state.position - amount
        } else {
            self.state.position + amount
        })?;
        if !redeem && position > self.state.max_position {
            return Err(AmmError::MaximumPosition);
        }
        let share = finite(if redeem {
            current - amount
        } else {
            current + amount
        })?;
        if current == 0.0 && !redeem && self.state.lp_addresses.len() == MAX_LPS {
            return Err(AmmError::CapacityExceeded);
        }
        let mut movements = Vec::with_capacity(if second.is_some() { 4 } else { 2 });
        let lp_first = second.map_or_else(|| format!("{first}-LP"), |s| format!("{first}-{s}-LP"));
        append_movements(
            &mut movements,
            owner,
            first,
            &lp_first,
            amount,
            block,
            redeem,
        );
        if let Some(second) = second {
            append_movements(
                &mut movements,
                owner,
                second,
                &format!("{second}-{first}-LP"),
                second_amount,
                block,
                redeem,
            );
        }
        self.state.position = position;
        if let Some(index) = self
            .state
            .lp_addresses
            .iter()
            .position(|s| s.address == owner)
        {
            if share <= 0.0 {
                self.state.lp_addresses.remove(index);
            } else {
                self.state.lp_addresses[index].amount = share;
            }
        } else {
            self.state.lp_addresses.push(LpShare {
                address: owner.to_owned(),
                amount: share,
            });
        }
        Ok(movements)
    }

    /// Native Number formulas, including source minimum spread and zero->40
    /// default. Negative/zero bid prices are retained for parity; callers must
    /// explicitly validate a quote before proposing book execution.
    pub fn quote_contract_orders(
        &self,
        mark_price: f64,
        block: u64,
        spread_bps: f64,
    ) -> Result<Vec<ContractQuote>, AmmError> {
        valid_block(block)?;
        if !mark_price.is_finite() || mark_price <= 0.0 {
            return Ok(Vec::new());
        }
        let size = self.state.max_quote_size.min(self.state.position).max(0.0);
        if size <= 0.0 {
            return Ok(Vec::new());
        }
        let spread = number_or(spread_bps, 40.0).max(1.0) / 10000.0;
        let bid = to_fixed_eight(mark_price * (1.0 - spread))?;
        let ask = to_fixed_eight(mark_price * (1.0 + spread))?;
        Ok([Side::Buy, Side::Sell]
            .into_iter()
            .map(|side| ContractQuote {
                contract_id: self.state.contract_id,
                amount: size,
                price: if side == Side::Buy { bid } else { ask },
                block,
                side,
                txid: format!(
                    "amm-{}-{}-{block}",
                    if side == Side::Buy { "bid" } else { "ask" },
                    self.state.contract_id
                ),
            })
            .collect())
    }

    /// Source gate is position+size for both sides; even reducing sells can be
    /// rejected. Signed sizes and f64 accumulation follow the source formula.
    pub fn request_liquidity(
        &mut self,
        order: LiquidityOrder,
    ) -> Result<Option<LiquidityOrder>, AmmError> {
        finite(order.size)?;
        if self.state.position + order.size > self.state.max_position {
            return Ok(None);
        }
        if self.state.amm_orders.len() == MAX_AMM_ORDERS {
            return Err(AmmError::CapacityExceeded);
        }
        let position = finite(
            self.state.position + order.size * if order.side == Side::Buy { 1.0 } else { -1.0 },
        )?;
        self.state.position = position;
        self.state.amm_orders.push(order.clone());
        Ok(Some(order))
    }

    /// Unlike the source class's uninitialized lpShares static field, the typed
    /// receiver deliberately reads the instance's actual capital-share table.
    pub fn total_position_percent(&self, owner: Option<&str>) -> f64 {
        if let Some(owner) = owner {
            let share = self.share(owner);
            return if share == 0.0 {
                0.0
            } else {
                share / self.state.max_position * 100.0
            };
        }
        let shares: f64 = source_object_order(&self.state.lp_addresses)
            .iter()
            // Source reduce starts at positive zero; f64 Sum uses -0.0.
            .fold(0.0, |total, share| total + share.amount);
        shares / self.state.max_position * 100.0
    }

    pub fn generate_contract_order(
        &mut self,
        contract_type: ContractType,
        oracle_price: f64,
        price_distance: f64,
        total_orders: f64,
    ) -> Result<CurveOrder, AmmError> {
        finite(oracle_price)?;
        finite(price_distance)?;
        finite(total_orders)?;
        let distance = if contract_type == ContractType::Inverse {
            0.001
        } else {
            0.01
        };
        let order_size = calculate_order_size(distance, price_distance, total_orders);
        let size = finite(order_size.min(self.state.position.min(self.state.max_quote_size)))?;
        let position = finite(self.state.position + size)?;
        let price = finite(oracle_price + distance)?;
        let side = if contract_type == ContractType::Inverse || position <= 0.0 {
            Side::Sell
        } else {
            Side::Buy
        };
        self.state.position = position;
        Ok(CurveOrder { price, size, side })
    }

    /// Repairs absent static receiver state; strategy selection is a typed
    /// value rather than reparsing an arbitrary JSON blob inside consensus.
    pub fn build_option_strategy(
        &self,
        strategy: OptionStrategy,
        last_price: f64,
        block: u64,
    ) -> Result<Vec<OptionOrder>, AmmError> {
        finite(last_price)?;
        let horizon = match strategy {
            OptionStrategy::None => 0,
            OptionStrategy::Calendar => 10,
            _ => 1,
        };
        valid_block(block.checked_add(horizon).ok_or(AmmError::InvalidBlock)?)?;
        let make = |side, option_type, strike, expiry| OptionOrder {
            side,
            option_type,
            strike,
            expiry,
            size: self.state.max_quote_size,
        };
        Ok(match strategy {
            OptionStrategy::Straddle => vec![
                make(Side::Buy, OptionKind::Call, last_price, block + 1),
                make(Side::Buy, OptionKind::Put, last_price, block + 1),
            ],
            OptionStrategy::IronFly => vec![
                make(Side::Sell, OptionKind::Call, last_price, block + 1),
                make(Side::Sell, OptionKind::Put, last_price, block + 1),
                make(
                    Side::Buy,
                    OptionKind::Call,
                    finite(last_price * 1.02)?,
                    block + 1,
                ),
                make(
                    Side::Buy,
                    OptionKind::Put,
                    finite(last_price * 0.98)?,
                    block + 1,
                ),
            ],
            OptionStrategy::Calendar => vec![
                make(Side::Sell, OptionKind::Call, last_price, block + 1),
                make(Side::Buy, OptionKind::Call, last_price, block + 10),
            ],
            OptionStrategy::None => Vec::new(),
        })
    }
}

fn append_movements(
    movements: &mut Vec<BalanceMovement>,
    owner: &str,
    property: &str,
    lp_property: &str,
    amount: f64,
    block: u64,
    redeem: bool,
) {
    let legs = if redeem {
        [
            (lp_property, -amount, MovementKind::Burn),
            (property, amount, MovementKind::Redeem),
        ]
    } else {
        [
            (property, -amount, MovementKind::Pledge),
            (lp_property, amount, MovementKind::Issue),
        ]
    };
    for (property, amount, kind) in legs {
        movements.push(BalanceMovement {
            address: owner.to_owned(),
            property: property.to_owned(),
            amount,
            kind,
            block,
        });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContractQuote {
    pub contract_id: u32,
    pub amount: f64,
    pub price: f64,
    pub block: u64,
    pub side: Side,
    pub txid: String,
}
impl ContractQuote {
    pub fn sender(&self) -> &'static str {
        "amm"
    }
    /// All source quote flags and initial margin are zero/false.
    pub fn initial_margin(&self) -> f64 {
        0.0
    }
    pub fn validate_executable(&self) -> Result<(), AmmError> {
        if !self.amount.is_finite()
            || self.amount <= 0.0
            || !self.price.is_finite()
            || self.price <= 0.0
        {
            return Err(AmmError::NonExecutableQuote);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractType {
    Inverse,
    Linear,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CurveOrder {
    pub price: f64,
    pub size: f64,
    pub side: Side,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionStrategy {
    Straddle,
    IronFly,
    Calendar,
    None,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Call,
    Put,
}
impl OptionKind {
    pub const fn as_source_name(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Put => "put",
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct OptionOrder {
    pub side: Side,
    pub option_type: OptionKind,
    pub strike: f64,
    pub expiry: u64,
    pub size: f64,
}
impl OptionOrder {
    /// Explicit adapter into the existing options calculators. This does not
    /// open a contract or reserve maintenance collateral. The binary64 source
    /// numbers enter Decimal using their shortest decimal representations.
    pub fn as_option_position(&self) -> Result<crate::options::OptionPosition, AmmError> {
        valid_block(self.expiry)?;
        finite(self.strike)?;
        finite(self.size)?;
        let strike =
            Decimal::from_str(&self.strike.to_string()).map_err(|_| AmmError::Arithmetic)?;
        let qty = Decimal::from_str(&self.size.to_string()).map_err(|_| AmmError::Arithmetic)?;
        Ok(crate::options::OptionPosition {
            option_type: match self.option_type {
                OptionKind::Call => "Call".to_owned(),
                OptionKind::Put => "Put".to_owned(),
            },
            expiry_block: self.expiry as f64,
            strike,
            qty: if self.side == Side::Buy {
                qty
            } else {
                qty.negated()
            },
        })
    }
}

/// Retains source branch boundaries, including NaN ratio falling through to 5%.
pub fn calculate_order_size(distance: f64, price_distance: f64, total_orders: f64) -> f64 {
    let ratio = distance / (0.2 * price_distance);
    total_orders
        * if ratio <= 0.05 {
            0.35
        } else if ratio <= 0.1 {
            0.25
        } else if ratio <= 0.15 {
            0.15
        } else {
            0.05
        }
}

/// This is the source helper's literal formula: collateral_id is multiplied as
/// an amount despite its name. Zero tokens can produce Infinity or NaN.
pub fn calculate_redemption_value(
    amount: f64,
    is_contract: bool,
    tokens: f64,
    collateral_id: f64,
    pool_price: f64,
    last_price: f64,
) -> f64 {
    if is_contract {
        (amount / tokens) * (tokens + collateral_id * pool_price)
    } else {
        amount * last_price
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenCurveOrder {
    pub offered_property: String,
    pub desired_property: String,
    pub amount_offered: f64,
    pub amount_expected: f64,
    pub price: f64,
}

/// Computes the source's token curve intent. Actual generateTokenOrders catches
/// an undeclared `txid` before calling addTokenOrder, so this API does not place
/// orders. Negative source amountExpected values are intentionally retained.
pub fn token_curve_intents(
    token_x: &str,
    token_y: &str,
    total_liquidity: f64,
    total_orders: usize,
    last_price: f64,
) -> Result<Vec<TokenCurveOrder>, AmmError> {
    identifier(token_x)?;
    identifier(token_y)?;
    finite(total_liquidity)?;
    finite(last_price)?;
    if total_orders > MAX_AMM_ORDERS / 2 {
        return Err(AmmError::CapacityExceeded);
    }
    if total_orders == 0 {
        return Ok(Vec::new());
    }
    let distance = 0.30 * last_price;
    let increment = distance / total_orders as f64;
    let initial_x = (total_liquidity * (1.0 - distance / (2.0 * last_price))).sqrt();
    let initial_y = (total_liquidity * (1.0 + distance / (2.0 * last_price))).sqrt();
    let mut orders = Vec::with_capacity(total_orders * 2);
    for i in 1..=total_orders {
        let price_x = last_price - i as f64 * increment;
        let price_y = last_price + i as f64 * increment;
        let x_supply = initial_x * (last_price / price_x);
        let y_supply = total_liquidity / x_supply;
        let x = finite(x_supply - initial_x)?;
        let y = finite(y_supply - initial_y)?;
        orders.push(TokenCurveOrder {
            offered_property: token_x.to_owned(),
            desired_property: token_y.to_owned(),
            amount_offered: x,
            amount_expected: y,
            price: finite(price_x)?,
        });
        orders.push(TokenCurveOrder {
            offered_property: token_y.to_owned(),
            desired_property: token_x.to_owned(),
            amount_offered: y,
            amount_expected: x,
            price: finite(price_y)?,
        });
    }
    Ok(orders)
}

/// ECMAScript Number.toFixed(8), then Number: quantize the exact binary64
/// rational, avoiding shortest-decimal and ties-to-even formatting mistakes.
pub fn to_fixed_eight(value: f64) -> Result<f64, AmmError> {
    if !value.is_finite() || value.abs() >= 1e21 {
        return Ok(value);
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, power) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), exponent - 1023 - 52)
    };
    let mut coefficient = Decimal::from(mantissa);
    let factor = Decimal::from(if power < 0 { 5u8 } else { 2u8 });
    for _ in 0..power.unsigned_abs() {
        coefficient = coefficient
            .checked_mul(&factor)
            .map_err(|_| AmmError::Arithmetic)?;
    }
    let mut exact = if power < 0 {
        Decimal::from_str(&format!("{coefficient}e{power}")).map_err(|_| AmmError::Arithmetic)?
    } else {
        coefficient
    };
    if value.is_sign_negative() {
        exact = exact.negated();
    }
    let rounded = exact
        .quantize(8, Rounding::HalfUp)
        .map_err(|_| AmmError::Arithmetic)?;
    let out = rounded
        .to_string()
        .parse::<f64>()
        .map_err(|_| AmmError::Arithmetic)?;
    // Number('-0.00000000') retains -0; exact -0 itself formats as positive 0.
    Ok(if out == 0.0 && value < 0.0 { -0.0 } else { out })
}

fn finite(value: f64) -> Result<f64, AmmError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AmmError::NonFinite)
    }
}
fn number_or(value: f64, fallback: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        fallback
    } else {
        value
    }
}
fn valid_block(block: u64) -> Result<(), AmmError> {
    if block > MAX_SAFE_BLOCK {
        Err(AmmError::InvalidBlock)
    } else {
        Ok(())
    }
}
fn identifier(value: &str) -> Result<(), AmmError> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        Err(AmmError::InvalidIdentifier)
    } else {
        Ok(())
    }
}
fn address(value: &str) -> Result<(), AmmError> {
    identifier(value)?;
    if matches!(
        value,
        "__proto__"
            | "constructor"
            | "toString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "toLocaleString"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    ) {
        Err(AmmError::InvalidIdentifier)
    } else {
        Ok(())
    }
}
fn array_index(value: &str) -> Option<u32> {
    let parsed: u32 = value.parse().ok()?;
    (parsed != u32::MAX && parsed.to_string() == value).then_some(parsed)
}
fn source_object_order(shares: &[LpShare]) -> Vec<&LpShare> {
    let mut out: Vec<_> = shares.iter().collect();
    // Stable sort keeps ordinary string insertion order, as Object.values.
    out.sort_by_key(|share| array_index(&share.address).map_or((1, 0), |i| (0, i)));
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmDefinition {
    pub amm_id: u32,
    pub contract_id: Option<u32>,
    pub property_id: Option<u32>,
    pub options_maker: Option<String>,
    pub options_taker: Option<String>,
    pub strategy_blob: Option<String>,
    pub created_at: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AmmMutableState {
    pub lp_shares: Vec<LpShare>,
    pub position: f64,
    pub orders: Vec<LiquidityOrder>,
    pub pnl: f64,
    pub updated_at: u64,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AmmRegistry {
    definitions: BTreeMap<u32, AmmDefinition>,
    states: BTreeMap<u32, AmmMutableState>,
}
impl AmmRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    /// Deterministic local replacement for createAMM's two nontransactional DB
    /// writes. Timestamp is supplied by the caller, rather than Date.now().
    pub fn create(
        &mut self,
        contract_id: Option<u32>,
        property_id: Option<u32>,
        options_maker: Option<String>,
        options_taker: Option<String>,
        strategy_blob: Option<String>,
        now: u64,
    ) -> Result<AmmDefinition, AmmError> {
        if self.definitions.len() == MAX_AMMS {
            return Err(AmmError::CapacityExceeded);
        }
        for field in [&options_maker, &options_taker, &strategy_blob]
            .into_iter()
            .flatten()
        {
            if field.len() > MAX_METADATA_BYTES {
                return Err(AmmError::CapacityExceeded);
            }
        }
        let amm_id = self
            .definitions
            .last_key_value()
            .map_or(Some(1), |(id, _)| id.checked_add(1))
            .ok_or(AmmError::CapacityExceeded)?;
        let definition = AmmDefinition {
            amm_id,
            contract_id,
            property_id,
            options_maker: options_maker.filter(|v| !v.is_empty()),
            options_taker: options_taker.filter(|v| !v.is_empty()),
            strategy_blob: strategy_blob.filter(|v| !v.is_empty()),
            created_at: now,
        };
        self.states.insert(
            amm_id,
            AmmMutableState {
                lp_shares: Vec::new(),
                position: 0.0,
                orders: Vec::new(),
                pnl: 0.0,
                updated_at: now,
            },
        );
        self.definitions.insert(amm_id, definition.clone());
        Ok(definition)
    }
    pub fn load_registry(&self, amm_id: u32) -> Option<&AmmDefinition> {
        self.definitions.get(&amm_id)
    }
    pub fn load_state(&self, amm_id: u32) -> Option<&AmmMutableState> {
        self.states.get(&amm_id)
    }
    /// Typed full state replacement; source arbitrary-field upsert is excluded.
    pub fn save_state(
        &mut self,
        amm_id: u32,
        mut state: AmmMutableState,
        now: u64,
    ) -> Result<(), AmmError> {
        if !self.definitions.contains_key(&amm_id) {
            return Err(AmmError::NotFound);
        }
        finite(state.pnl)?;
        AmmPool::from_snapshot(AmmSnapshot {
            position: state.position,
            max_position: 0.0,
            max_quote_size: 0.0,
            contract_id: 0,
            lp_addresses: state.lp_shares.clone(),
            amm_orders: state.orders.clone(),
        })?;
        state.updated_at = now;
        self.states.insert(amm_id, state);
        Ok(())
    }
}
