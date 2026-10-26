//! Pinned vesting calculations and explicit genesis dependency calls.
//!
//! Initialization executes an ordered caller-owned backend; it does not mint
//! ledger balances or authorize chain state. In particular, the source's
//! static/instance parameter mismatch and chain-dependent supply discrepancy
//! are preserved in the calls rather than silently repaired.

use crate::decimal::{Decimal, DecimalError};
use crate::ledger::{AccountId, PropertyId};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_LF_SHA256: &str =
    "4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b";
pub const EPS: f64 = 1e-18;
pub const MAX_CHAIN_ATTEMPTS: usize = 10;
pub const CHAIN_RETRY_DELAY_MS: u64 = 2000;
pub const MAX_JS_SAFE_BLOCK: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VestingError {
    Arithmetic(DecimalError),
    MissingParameters,
    InvalidBlock,
    UnexpectedReply,
    Backend(String),
    /// Source performBuyback receives undefined from its empty orderbook stub.
    UnimplementedOrderbook,
    /// Source returns an undefined shorthand, projectedYearVolumeBTC.
    UndefinedProjectedYearVolume,
}

impl From<DecimalError> for VestingError {
    fn from(value: DecimalError) -> Self {
        Self::Arithmetic(value)
    }
}
impl core::fmt::Display for VestingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VestingError {}

#[derive(Debug, Clone, PartialEq)]
pub struct ChainParameters {
    pub base_volume: f64,
    pub min_rebate: f64,
    pub max_rebate: f64,
    pub initial_token_amount: u64,
    pub ticker: &'static str,
    pub hedge_leverage: u32,
    pub expiry_interval: u64,
    pub sponsor: AccountId,
    pub free_portion: Decimal,
    pub insurance_portion: Decimal,
    pub vesting_portion: Decimal,
    pub sale_portion: Decimal,
}

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).expect("pinned source address is a bounded ASCII identifier")
}
fn tenth(numerator: u8) -> Decimal {
    format!("0.{numerator}")
        .parse()
        .expect("pinned decimal constant")
}

impl ChainParameters {
    /// Every chain other than exact uppercase BTC or DOGE gets LTC parameters.
    pub fn for_chain(chain: Option<&str>) -> Self {
        match chain {
            Some("BTC") => Self {
                base_volume: 100.0,
                min_rebate: 0.00000625,
                max_rebate: 0.0001,
                initial_token_amount: 100_000,
                ticker: "TL",
                hedge_leverage: 5,
                expiry_interval: 12_960,
                sponsor: account("bc1qhc2cj60auf67e0pa3dfd46cvg0fehchx56vw0f"),
                free_portion: tenth(1),
                insurance_portion: tenth(1),
                vesting_portion: tenth(5),
                sale_portion: Decimal::zero(),
            },
            Some("DOGE") => Self {
                base_volume: 2_000_000.0,
                min_rebate: 0.00000125,
                max_rebate: 0.0005,
                initial_token_amount: 200_000_000,
                ticker: "TD",
                hedge_leverage: 10,
                expiry_interval: 129_600,
                sponsor: account("D8HA73pAhxK7eNXSUVhQrWpUkrszUDGs7Z"),
                free_portion: "0.25".parse().expect("constant"),
                insurance_portion: "0.25".parse().expect("constant"),
                vesting_portion: tenth(5),
                sale_portion: Decimal::zero(),
            },
            _ => Self {
                base_volume: 1000.0,
                min_rebate: 0.000003125,
                max_rebate: 0.0001,
                initial_token_amount: 500_000,
                ticker: "TLITE",
                hedge_leverage: 5,
                expiry_interval: 51_840,
                sponsor: account("MWip91xMhaEmDn5oUW5NDNbWSDyG5dSK9Q"),
                free_portion: tenth(1),
                insurance_portion: tenth(3),
                vesting_portion: tenth(5),
                sale_portion: tenth(1),
            },
        }
    }
}

/// A chain attempt fetches chain first, then test. Errors belong to that
/// attempt. Retry waiting is delegated so deterministic tests never sleep.
pub trait ChainProbe {
    fn probe(&mut self) -> Result<(Option<String>, Option<bool>), String>;
    fn retry_delay(&mut self, milliseconds: u64);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainInitialization {
    pub attempts: usize,
    pub used_fallback: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VestingManager {
    pub admin: Option<AccountId>,
    chain: Option<String>,
    test: Option<bool>,
    parameters: Option<ChainParameters>,
}

impl VestingManager {
    pub fn new(admin: Option<AccountId>, chain: Option<String>, test: Option<bool>) -> Self {
        Self {
            admin,
            chain,
            test,
            parameters: None,
        }
    }

    pub fn get_chain(&self) -> Option<&str> {
        self.chain.as_deref()
    }

    pub const fn get_test(&self) -> Option<bool> {
        self.test
    }

    pub fn parameters(&self) -> Option<&ChainParameters> {
        self.parameters.as_ref()
    }

    /// Source ignores its argument and reads `this.chain` instead.
    pub fn set_chain_params(&mut self, _argument: Option<&str>) {
        self.parameters = Some(ChainParameters::for_chain(self.chain.as_deref()));
    }

    pub fn ensure_chain_initialized(&mut self, probe: &mut impl ChainProbe) -> ChainInitialization {
        if self.chain.as_ref().is_some_and(|chain| !chain.is_empty()) && self.test.is_some() {
            self.set_chain_params(None);
            return ChainInitialization {
                attempts: 0,
                used_fallback: false,
            };
        }
        for attempt in 1..=MAX_CHAIN_ATTEMPTS {
            if let Ok((Some(chain), Some(test))) = probe.probe() {
                if !chain.is_empty() {
                    self.chain = Some(chain);
                    self.test = Some(test);
                    self.set_chain_params(None);
                    return ChainInitialization {
                        attempts: attempt,
                        used_fallback: false,
                    };
                }
            }
            // The JS waits after the tenth unsuccessful attempt as well.
            probe.retry_delay(CHAIN_RETRY_DELAY_MS);
        }
        self.chain = Some("BTC".to_owned());
        self.set_chain_params(None);
        ChainInitialization {
            attempts: MAX_CHAIN_ATTEMPTS,
            used_fallback: true,
        }
    }

    fn fill_admin(&mut self) {
        if self.admin.is_some() {
            return;
        }
        let test = self.test.unwrap_or(false);
        self.admin = match self.chain.as_deref() {
            Some("BTC") => Some(account(if test {
                "tb1q8f84erfegxhaylmvpfll9m5rgwymqy4akjnnvq"
            } else {
                // The trailing comma exists in the pinned source.
                "bc1qktknrnx2jcchjht9anz0uy8ae02xryxq2vxeem,"
            })),
            Some("DOGE") => Some(account(if test {
                "nop27JQWbGr95ySHXZMzCg8XXxYzbCBZAW"
            } else {
                "DLSfu9qvEggkeXAgCAwBBw5BVLvMCtkewz"
            })),
            Some("LTC") => Some(account(if test {
                "tltc1qa0kd2d39nmeph3hvcx8ytv65ztcywg5sazhtw8"
            } else {
                "MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB"
            })),
            _ => None,
        };
    }

    /// Invoke the source dependency sequence. Backend failure preserves prior
    /// external calls, as in JS; only its own backend can offer transactions.
    /// The flag is written last. Nothing in this API grants mint authority.
    pub fn initialize_tokens(
        &mut self,
        block: u64,
        backend: &mut impl InitializationBackend,
    ) -> Result<Option<Vec<InitializationBalance>>, VestingError> {
        if block > MAX_JS_SAFE_BLOCK {
            return Err(VestingError::InvalidBlock);
        }
        let initialized = match backend.call(InitializationCall::CheckInitializationFlag)? {
            InitializationReply::Flag(value) => value,
            _ => return Err(VestingError::UnexpectedReply),
        };
        self.fill_admin();
        if initialized {
            return Ok(None);
        }
        let p = self
            .parameters
            .as_ref()
            .ok_or(VestingError::MissingParameters)?;
        let total = Decimal::from(p.initial_token_amount);
        let vest_total = total.checked_mul(&p.vesting_portion)?;
        let insurance = total.checked_mul(&p.insurance_portion)?;
        let liquidity = total.checked_mul(&p.sale_portion)?;
        let free = total.checked_mul(&p.free_portion)?;
        let income_vest = total.checked_mul(&Decimal::from(3u8))?;
        let income = income_vest.checked_add(&Decimal::from(1u8))?;
        expect_none(backend, InitializationCall::AcquirePropertyManager)?;
        let token = create_token(backend, p.ticker.to_owned(), total, InitialTokenKind::Fixed)?;
        let vest_token = create_token(
            backend,
            format!("{}VEST", p.ticker),
            vest_total.clone(),
            InitialTokenKind::Vesting,
        )?;
        let income_vest_token = create_token(
            backend,
            format!("{}I", p.ticker),
            income_vest,
            InitialTokenKind::Vesting,
        )?;
        let income_token = create_token(
            backend,
            format!("{}IVEST", p.ticker),
            income,
            InitialTokenKind::Native,
        )?;
        Self::initialize_contract_series(block, backend)?;
        expect_none(
            backend,
            InitializationCall::ConstructInsurance {
                contract_id: 1,
                initial_balance: Decimal::zero(),
                distribution_share: tenth(5),
                hedge: false,
            },
        )?;
        expect_none(
            backend,
            InitializationCall::LaunchInsuranceDeposit {
                property: token,
                amount: insurance,
                is_hedge: false,
            },
        )?;
        for (address, property, available, vesting) in [
            (self.admin.clone(), token, liquidity, Decimal::zero()),
            (
                self.admin.clone(),
                vest_token,
                vest_total.clone(),
                vest_total,
            ),
            (
                self.admin.clone(),
                income_vest_token,
                Decimal::from(1_500_000u64),
                Decimal::from(1_500_000u64),
            ),
            (
                self.admin.clone(),
                income_token,
                Decimal::from(1u8),
                Decimal::zero(),
            ),
            (Some(p.sponsor.clone()), token, free, Decimal::zero()),
        ] {
            expect_none(
                backend,
                InitializationCall::UpdateBalance(InitializationBalance {
                    address,
                    property,
                    available,
                    reserved: Decimal::zero(),
                    margin: Decimal::zero(),
                    vesting,
                }),
            )?;
        }
        let balances =
            match backend.call(InitializationCall::GetAddressBalances(self.admin.clone()))? {
                InitializationReply::Balances(balances) => balances,
                _ => return Err(VestingError::UnexpectedReply),
            };
        Self::initialize_clearlists(backend)?;
        expect_none(backend, InitializationCall::SetInitializationFlag)?;
        Ok(Some(balances))
    }

    /// Source calls this statically, so instance admin/expiry/leverage are absent.
    pub fn initialize_contract_series(
        block: u64,
        backend: &mut impl InitializationBackend,
    ) -> Result<u32, VestingError> {
        if block > MAX_JS_SAFE_BLOCK {
            return Err(VestingError::InvalidBlock);
        }
        match backend.call(InitializationCall::CreateContractSeries {
            block,
            request: NativeContractRequest::default(),
        })? {
            InitializationReply::ContractId(id) => Ok(id),
            _ => Err(VestingError::UnexpectedReply),
        }
    }

    /// These launches are not awaited in JS, even though the method is async.
    pub fn initialize_clearlists(
        backend: &mut impl InitializationBackend,
    ) -> Result<(), VestingError> {
        for (id, name, description) in [
            (
                1,
                "Issuer Clearlist",
                "Oracles and Tokens included in Liquidity Reward",
            ),
            (
                2,
                "Market Maker Clearlist",
                "Market Makers and active traders who do not wash trade.",
            ),
            (
                3,
                "Accredited Clearlist",
                "Exempt from restrictions on native tokens.",
            ),
        ] {
            expect_none(
                backend,
                InitializationCall::LaunchClearlist {
                    id,
                    admin: None,
                    name,
                    url: "",
                    description,
                    backup: None,
                },
            )?;
        }
        Ok(())
    }

    pub fn calculate_trade_rebates(&self, cumulative_volume: f64) -> Result<f64, VestingError> {
        let p = self
            .parameters
            .as_ref()
            .ok_or(VestingError::MissingParameters)?;
        if cumulative_volume < p.base_volume {
            return Ok(p.max_rebate);
        }
        let scale = (cumulative_volume / p.base_volume).ln() / (100_000_000.0 / p.base_volume).ln();
        Ok(js_max(
            p.max_rebate - scale * (p.max_rebate - p.min_rebate),
            p.min_rebate,
        ))
    }
}

/// An explicit owner replaces the process-global JS singleton. Like source
/// getInstance, only first-call admin/chain are used, and test is not an argument.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ManagerSlot(Option<VestingManager>);

impl ManagerSlot {
    pub fn get_instance(
        &mut self,
        admin: Option<AccountId>,
        chain: Option<String>,
        probe: &mut impl ChainProbe,
    ) -> &mut VestingManager {
        self.0.get_or_insert_with(|| {
            let mut manager = VestingManager::new(admin, chain, None);
            manager.ensure_chain_initialized(probe);
            manager
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialTokenKind {
    Fixed,
    Vesting,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationBalance {
    pub address: Option<AccountId>,
    pub property: PropertyId,
    pub available: Decimal,
    pub reserved: Decimal,
    pub margin: Decimal,
    pub vesting: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeContractRequest {
    pub admin: Option<AccountId>,
    pub initial_margin: Decimal,
    pub maintenance_margin: Decimal,
    pub expiry: &'static str,
    pub on_chain_data: [(u32, u32); 1],
    pub expiry_interval: Option<u64>,
    pub leverage: u32,
    /// This is source seriesLength, not registry's different `series` field.
    pub series_length: u32,
    pub native: bool,
    pub inverse: bool,
    pub fee: bool,
    pub notional_property_id: u32,
    pub notional_value: Decimal,
    pub collateral_property_id: u32,
}

impl Default for NativeContractRequest {
    fn default() -> Self {
        Self {
            admin: None,
            initial_margin: tenth(2),
            maintenance_margin: tenth(1),
            expiry: "perp",
            on_chain_data: [(1, 0)],
            expiry_interval: None,
            leverage: 5,
            series_length: 6,
            native: true,
            inverse: true,
            fee: false,
            notional_property_id: 0,
            notional_value: "0.001".parse().expect("constant"),
            collateral_property_id: 1,
        }
    }
}

/// Explicit requests; `Launch*` records initiation, never durable completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitializationCall {
    CheckInitializationFlag,
    AcquirePropertyManager,
    CreateToken {
        ticker: String,
        amount: Decimal,
        kind: InitialTokenKind,
        whitelist_id: u32,
        issuer: Option<AccountId>,
        backup: Option<AccountId>,
    },
    CreateContractSeries {
        block: u64,
        request: NativeContractRequest,
    },
    ConstructInsurance {
        contract_id: u32,
        initial_balance: Decimal,
        distribution_share: Decimal,
        hedge: bool,
    },
    LaunchInsuranceDeposit {
        property: PropertyId,
        amount: Decimal,
        is_hedge: bool,
    },
    UpdateBalance(InitializationBalance),
    GetAddressBalances(Option<AccountId>),
    LaunchClearlist {
        id: u32,
        admin: Option<AccountId>,
        name: &'static str,
        url: &'static str,
        description: &'static str,
        backup: Option<AccountId>,
    },
    SetInitializationFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitializationReply {
    None,
    Flag(bool),
    Property(PropertyId),
    ContractId(u32),
    Balances(Vec<InitializationBalance>),
}

pub trait InitializationBackend {
    fn call(&mut self, call: InitializationCall) -> Result<InitializationReply, VestingError>;
}

fn expect_none(
    backend: &mut impl InitializationBackend,
    call: InitializationCall,
) -> Result<(), VestingError> {
    match backend.call(call)? {
        InitializationReply::None => Ok(()),
        _ => Err(VestingError::UnexpectedReply),
    }
}

fn create_token(
    backend: &mut impl InitializationBackend,
    ticker: String,
    amount: Decimal,
    kind: InitialTokenKind,
) -> Result<PropertyId, VestingError> {
    match backend.call(InitializationCall::CreateToken {
        ticker,
        amount,
        kind,
        whitelist_id: 0,
        issuer: None,
        backup: None,
    })? {
        InitializationReply::Property(id) => Ok(id),
        _ => Err(VestingError::UnexpectedReply),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VestingAmounts {
    pub two: f64,
    pub three: f64,
}

/// This function returns cumulative targets, not an incremental vesting debit.
/// Source uses binary64 arithmetic, despite BigNumber elsewhere in the file.
pub fn update_vesting(
    cumulative_ltc: f64,
    block_ltc: f64,
    cumulative_global: f64,
    block_global: f64,
    property_two_supply: Option<f64>,
    property_three_supply: Option<f64>,
) -> Option<VestingAmounts> {
    let (Some(two), Some(three)) = (property_two_supply, property_three_supply) else {
        return None;
    };
    let ltc = cumulative_ltc + block_ltc;
    let global = cumulative_global + block_global;
    let fraction_one = if ltc >= 1000.0 {
        js_min((ltc - 1000.0) / (1_000_000_000.0 - 1000.0), 1.0)
    } else {
        0.0
    };
    let fraction_two = if global >= 10_000_000_000.0 {
        js_min(
            (global - 10_000_000_000.0) / (1_000_000_000_000.0 - 10_000_000_000.0),
            1.0,
        )
    } else {
        0.0
    };
    Some(VestingAmounts {
        two: fraction_one * two,
        three: fraction_two * three,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuybackOrder {
    pub price: f64,
    pub amount: f64,
}

pub fn calculate_buyback_amount(mut available_funds: f64, orderbook: &[BuybackOrder]) -> f64 {
    let mut total = 0.0;
    for order in orderbook {
        let cost = order.price * order.amount;
        if available_funds >= cost {
            total += order.amount;
            available_funds -= cost;
        } else {
            total += available_funds / order.price;
            break;
        }
        if available_funds <= 0.0 {
            break;
        }
    }
    total
}

/// Source stub has no fetch implementation. The empty input makes no calls;
/// any cache encounters undefined orderbook and cannot execute a buyback.
pub fn perform_buyback(cache_count: usize) -> Result<(), VestingError> {
    if cache_count == 0 {
        Ok(())
    } else {
        Err(VestingError::UnimplementedOrderbook)
    }
}

pub fn fetch_orderbook_for_token(_token_id: u32) -> Option<Vec<BuybackOrder>> {
    None
}

fn js_max(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.max(right)
    }
}
fn js_min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.min(right)
    }
}

pub fn vest_fraction(cumulative: f64, start: f64, full: f64, alpha: f64) -> f64 {
    if cumulative <= start {
        return 0.0;
    }
    let alpha = js_max(alpha, EPS);
    let numerator = (1.0 + (cumulative - start) / alpha).ln();
    let denominator = (1.0 + (full - start) / alpha).ln();
    js_max(0.0, js_min(1.0, numerator / js_max(denominator, EPS)))
}

pub fn project_next_year_volume_btc(monthly: f64, cmgr: f64) -> f64 {
    if cmgr <= EPS {
        return 12.0 * monthly;
    }
    let rate = 1.0 + cmgr;
    monthly * ((rate.powf(12.0) - 1.0) / cmgr)
}

pub fn months_to_reach(cumulative: f64, full: f64, monthly: f64, cmgr: f64) -> f64 {
    let remaining = js_max(0.0, full - cumulative);
    if remaining <= EPS {
        return 0.0;
    }
    if monthly <= EPS {
        return f64::INFINITY;
    }
    if cmgr <= EPS {
        return (remaining / monthly).ceil();
    }
    let rate = 1.0 + cmgr;
    let numerator = (remaining * (rate - 1.0)) / monthly + 1.0;
    if numerator <= 1.0 {
        return 0.0;
    }
    (numerator.ln() / rate.ln()).ceil()
}

pub fn annualize_from_cmgr(cmgr: f64) -> f64 {
    (1.0 + cmgr).powf(12.0) - 1.0
}

pub fn discount_factor_months(annual_rate: f64, months: f64) -> f64 {
    (-annual_rate * (months / 12.0)).exp()
}

pub fn cmgr_from_series(monthly: &[f64]) -> f64 {
    if monthly.len() < 2 {
        return 0.0;
    }
    let mut sum_log = 0.0;
    for pair in monthly.windows(2) {
        let previous = js_max(pair[0], EPS);
        let current = js_max(pair[1], EPS);
        sum_log += (current / previous).ln();
    }
    (sum_log / (monthly.len() - 1) as f64).exp() - 1.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeAwardParameters {
    pub fee_rate: f64,
    pub base_rebate_share: f64,
    pub vip_rebate_share: f64,
    pub dividend_share_of_fees: f64,
    pub discount_rate_annual: f64,
    pub price_tli_in_btc: f64,
    pub vest_to_tli_ratio: f64,
    pub cumulative_volume_btc: f64,
    pub monthly_volume_btc: f64,
    pub cmgr12: f64,
    pub tlvest_effective_supply: f64,
    pub vest_v0_btc: f64,
    pub vest_v_star_btc: f64,
    pub vest_alpha: f64,
}

impl Default for TradeAwardParameters {
    fn default() -> Self {
        Self {
            fee_rate: 0.000005,
            base_rebate_share: 0.40,
            vip_rebate_share: 0.20,
            dividend_share_of_fees: 0.50,
            discount_rate_annual: 0.08,
            price_tli_in_btc: 0.00001,
            vest_to_tli_ratio: 1.0,
            cumulative_volume_btc: 1_000_000.0,
            monthly_volume_btc: 50_000.0,
            cmgr12: 0.05,
            tlvest_effective_supply: 10_000_000.0,
            vest_v0_btc: 100_000.0,
            vest_v_star_btc: 21_000_000.0,
            vest_alpha: 500_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeAward {
    pub tokens: f64,
    pub award_value_btc: f64,
    pub pv_token_btc: f64,
    pub vest_fraction_now: f64,
    pub months_to_full_vest: f64,
    pub pv_dividend_btc: f64,
    pub pv_conversion_btc: f64,
    pub annual_growth: f64,
    pub projected_year_volume_btc: f64,
    pub per_token_dividend_btc: f64,
    pub fee_btc: f64,
}

/// The ordinary pinned source throws while constructing its return object:
/// `projectedYearVolumeBTC` is undefined (the local is `projectedYearVolBTC`).
pub fn compute_tokens_for_trade(
    _notional_btc: f64,
    _vip: bool,
    _parameters: &TradeAwardParameters,
) -> Result<TradeAward, VestingError> {
    Err(VestingError::UndefinedProjectedYearVolume)
}

/// Explicit repair of that single return-name defect. This is a numerical
/// quote only, has no issuance/mint effect, and is not historical acceptance.
pub fn quote_tokens_for_trade_repaired(
    notional_btc: f64,
    vip: bool,
    p: &TradeAwardParameters,
) -> TradeAward {
    let fee_btc = p.fee_rate * notional_btc;
    let award_value_btc =
        fee_btc * (p.base_rebate_share + if vip { p.vip_rebate_share } else { 0.0 });
    let annual_growth = annualize_from_cmgr(p.cmgr12);
    let rate = js_max(p.discount_rate_annual, 0.0001);
    let capped_growth = js_min(annual_growth, rate - 0.01);
    let projected_year_volume_btc = project_next_year_volume_btc(p.monthly_volume_btc, p.cmgr12);
    let yearly_fees = p.fee_rate * projected_year_volume_btc;
    let dividend_pool = p.dividend_share_of_fees * yearly_fees;
    let per_token_dividend_btc = dividend_pool / js_max(p.tlvest_effective_supply, 1.0);
    let pv_dividend_btc = per_token_dividend_btc / js_max(rate - capped_growth, 0.01);
    let months_to_full_vest = months_to_reach(
        p.cumulative_volume_btc,
        p.vest_v_star_btc,
        p.monthly_volume_btc,
        p.cmgr12,
    );
    let vest_fraction_now = vest_fraction(
        p.cumulative_volume_btc,
        p.vest_v0_btc,
        p.vest_v_star_btc,
        p.vest_alpha,
    );
    let discount = discount_factor_months(rate, js_max(0.0, months_to_full_vest));
    let pv_conversion_btc = p.price_tli_in_btc
        * p.vest_to_tli_ratio
        * (vest_fraction_now + (1.0 - vest_fraction_now) * discount);
    let pv_token_btc = js_max(js_max(pv_dividend_btc, pv_conversion_btc), EPS);
    TradeAward {
        tokens: award_value_btc / pv_token_btc,
        award_value_btc,
        pv_token_btc,
        vest_fraction_now,
        months_to_full_vest,
        pv_dividend_btc,
        pv_conversion_btc,
        annual_growth,
        projected_year_volume_btc,
        per_token_dividend_btc,
        fee_btc,
    }
}
