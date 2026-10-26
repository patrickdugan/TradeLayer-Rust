//! Immutable source-rule dependency reads from the actual economic owner.
//! This named projection supplies facts, never chain or settlement authority.
use crate::chain::Tip;
use crate::contract_registry::ContractId;
use crate::decimal::{Decimal, Rounding};
use crate::economic::EconomicState;
use crate::economic_chain::{DurableEconomicChain, EconomicChain, EconomicHead};
use crate::ledger::{AccountId, PropertyId};
use crate::network_profile::TradeLayerMainnet;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceRuleError, SourceValue,
};
use core::str::FromStr;

pub const READ_POLICY: &str = "EconomicSourceReadV1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationContext {
    pub network: TradeLayerMainnet,
    /// Candidate block, whose parent is the immutable pre-block state.
    pub height: u64,
    pub parent: Tip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EconomicValidityError {
    UnhealthyOwner,
    WrongHead,
    WrongContext,
    Invariants,
}
impl core::fmt::Display for EconomicValidityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EconomicValidityError {}

/// The borrow prevents economic mutation during evaluation. No Ledger,
/// property catalog, price database or balance map is retained or fabricated.
pub struct EconomicSourceBackend<'a> {
    state: &'a EconomicState,
    context: ValidationContext,
    durable_head: Option<EconomicHead>,
}
impl<'a> EconomicSourceBackend<'a> {
    pub fn from_chain_unverified(
        chain: &'a EconomicChain,
        expected_parent: Tip,
    ) -> Result<Self, EconomicValidityError> {
        if chain.tip() != expected_parent {
            return Err(EconomicValidityError::WrongHead);
        }
        chain
            .state()
            .verify()
            .map_err(|_| EconomicValidityError::Invariants)?;
        let context = ValidationContext {
            network: chain.config().economics.network,
            height: expected_parent
                .height
                .checked_add(1)
                .ok_or(EconomicValidityError::WrongContext)?,
            parent: expected_parent,
        };
        Self::for_staged_input(chain.state(), context)
    }

    pub fn from_durable(
        owner: &'a DurableEconomicChain,
        expected_head: EconomicHead,
    ) -> Result<Self, EconomicValidityError> {
        if owner
            .head()
            .map_err(|_| EconomicValidityError::UnhealthyOwner)?
            != expected_head
        {
            return Err(EconomicValidityError::WrongHead);
        }
        let chain = owner
            .chain()
            .map_err(|_| EconomicValidityError::UnhealthyOwner)?;
        let mut result = Self::from_chain_unverified(chain, expected_head.tip)?;
        result.durable_head = Some(expected_head);
        Ok(result)
    }

    /// The private block stage validates its candidate atomically. A partially
    /// staged state is not a finalized checkpoint and must not undergo the
    /// finalized-state verifier after every dependency read.
    pub(crate) fn for_staged_input(
        state: &'a EconomicState,
        context: ValidationContext,
    ) -> Result<Self, EconomicValidityError> {
        if state.config().network != context.network
            || context.parent.height.checked_add(1) != Some(context.height)
            || state.finalized_height().unwrap_or(0) != context.parent.height
            || (context.parent.height == 0 && context.parent.id != context.network.genesis())
        {
            return Err(EconomicValidityError::WrongContext);
        }
        Ok(Self {
            state,
            context,
            durable_head: None,
        })
    }
    pub fn context(&self) -> ValidationContext {
        self.context
    }
    pub fn durable_head(&self) -> Option<EconomicHead> {
        self.durable_head
    }
    pub const fn require_authenticated_admission(&self) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }

    fn custody(
        &self,
        owner: &AccountId,
        property: PropertyId,
    ) -> Result<Option<Custody>, SourceRuleError> {
        let s = self.state;
        if s.ledger().property(property).is_none() {
            return Ok(None);
        }
        let available = s.ledger().available_atoms(property, owner);
        let margin = if property == s.config().collateral {
            s.margin_atoms(owner)
        } else {
            0
        };
        let reserve = sum(s
            .book()
            .resting()
            .iter()
            .filter(|row| &row.order.owner == owner && property == s.config().collateral)
            .map(|row| s.reserve_atoms(row.order.txid))
            .chain(
                s.joint()
                    .spot()
                    .open_order_rows()
                    .filter(|row| &row.owner == owner && row.offered == property)
                    .map(|row| u128::from(row.remaining_offered)),
            ))?;
        // Source Tally.channel is keyed by the channel address, not by the
        // two participants. It must not be added to a participant's available.
        let channel = sum(s
            .ledger()
            .channel_balance_rows()
            .filter(|(id, _, channel, _, _)| *id == property && *channel == owner)
            .map(|(_, _, _, _, atoms)| atoms))?;
        // The existing source queue leaves custody in channel until maturity;
        // the local conserving Ledger reserves withdrawals separately. Such
        // a state needs its own named projection, rather than inventing an
        // interchangeable source bucket here.
        if s.ledger().pending_withdrawal_count() != 0 {
            return Err(SourceRuleError::Backend);
        }
        let total = sum([available, margin, reserve, channel])?;
        let vesting = s.ledger().native_remaining_atoms(property, owner);
        Ok((total != 0 || vesting != 0).then_some(Custody {
            available,
            margin,
            reserve,
            channel,
            total,
            vesting,
        }))
    }

    fn tally(&self, call: &DependencyCall) -> Result<SourceValue, SourceRuleError> {
        arity(call, 2)?;
        let owner = account(&call.arguments[0])?;
        let property = property(&call.arguments[1])?;
        let Some(c) = self.custody(&owner, property)? else {
            return Ok(n(0.0));
        };
        Ok(object([
            ("amount", token_number(c.total)?),
            ("available", token_number(c.available)?),
            ("reserved", token_number(c.reserve)?),
            ("margin", token_number(c.margin)?),
            ("vesting", token_number(c.vesting)?),
            ("channel", token_number(c.channel)?),
        ]))
    }

    fn sufficient(
        &self,
        call: &DependencyCall,
        channel: bool,
    ) -> Result<SourceValue, SourceRuleError> {
        arity(call, 3)?;
        let owner = account(&call.arguments[0])?;
        let property = property(&call.arguments[1])?;
        let required = nonnegative_decimal(&call.arguments[2])?;
        // Source compares its bucket and requirement with JavaScript '<'
        // before BigNumber shortfall arithmetic. Exclude a string/BigNumber
        // requirement whose Number coercion loses value; an exact Decimal
        // comparison would otherwise silently introduce a different rule.
        decimal_number(&required)?;
        let Some(c) = self.custody(&owner, property)? else {
            return Ok(if channel {
                object([
                    ("hasSufficient", SourceValue::Bool(false)),
                    ("reason", text("undefined")),
                ])
            } else {
                object([
                    ("hasSufficient", SourceValue::Bool(false)),
                    ("reason", text("undefined")),
                    ("shortfall", call.arguments[2].clone()),
                ])
            });
        };
        let available = if channel { c.channel } else { c.available };
        // Expose only amounts that survive the exact source Number boundary.
        let available_number = token_number(available)?;
        let available_decimal = nonnegative_decimal(&available_number)?;
        if available_decimal >= required {
            return Ok(object([
                ("hasSufficient", SourceValue::Bool(true)),
                ("reason", text("")),
            ]));
        }
        let mut shortfall = required
            .checked_sub(&available_decimal)
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
        if !channel {
            shortfall = shortfall
                .quantize(8, Rounding::HalfUp)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
        }
        let mut fields = vec![
            ("hasSufficient".into(), SourceValue::Bool(false)),
            ("reason".into(), text("Insufficient available balance")),
            ("shortfall".into(), decimal_number(&shortfall)?),
        ];
        if !channel {
            fields.push(("available".into(), available_number));
        }
        Ok(SourceValue::Object(fields))
    }

    fn oracle_info(&self, call: &DependencyCall) -> Result<SourceValue, SourceRuleError> {
        arity(call, 1)?;
        let id = identifier(&call.arguments[0])?;
        let Some(oracle) = self.state.joint().registry().oracle(id) else {
            return Ok(SourceValue::Null);
        };
        let data = match &oracle.latest {
            Some(quote) => object([
                ("price", scaled_number(quote.price_1e4, 4)?),
                (
                    "high",
                    quote
                        .high_1e4
                        .map(|x| scaled_number(x, 4))
                        .transpose()?
                        .unwrap_or(SourceValue::Undefined),
                ),
                (
                    "low",
                    quote
                        .low_1e4
                        .map(|x| scaled_number(x, 4))
                        .transpose()?
                        .unwrap_or(SourceValue::Undefined),
                ),
                (
                    "close",
                    quote
                        .close_1e4
                        .map(|x| scaled_number(x, 4))
                        .transpose()?
                        .unwrap_or(SourceValue::Undefined),
                ),
            ]),
            None => SourceValue::Undefined,
        };
        Ok(object([
            ("id", n(f64::from(id))),
            ("adminAddress", text(oracle.admin.as_str())),
            ("backupAddress", text(oracle.backup.as_str())),
            ("ticker", text(&oracle.ticker)),
            ("url", text(&oracle.url)),
            ("lag", n(f64::from(oracle.lag))),
            ("closed", SourceValue::Bool(oracle.closed)),
            ("data", data),
        ]))
    }

    fn contract_info(&self, call: &DependencyCall) -> Result<SourceValue, SourceRuleError> {
        arity(call, 1)?;
        let id = ContractId::try_from(identifier(&call.arguments[0])?)
            .map_err(|_| SourceRuleError::InvalidInput)?;
        let Some(contract) = self.state.contract(id) else {
            return Ok(SourceValue::Null);
        };
        let p = &contract.definition.parameters;
        Ok(object([
            ("id", n(f64::from(id.get()))),
            ("ticker", text(&contract.definition.ticker)),
            ("native", SourceValue::Bool(p.native)),
            (
                "underlyingOracleId",
                p.underlying_oracle_id
                    .map(|x| n(f64::from(x)))
                    .unwrap_or(SourceValue::Null),
            ),
            (
                "onChainData",
                SourceValue::Array(
                    p.on_chain_data
                        .iter()
                        .map(|(a, b)| {
                            SourceValue::Array(vec![n(f64::from(a.get())), n(f64::from(b.get()))])
                        })
                        .collect(),
                ),
            ),
            (
                "notionalPropertyId",
                n(f64::from(p.notional_property_id.get())),
            ),
            ("notionalValue", decimal_number(&p.notional_value)?),
            (
                "collateralPropertyId",
                n(f64::from(p.collateral_property_id.get())),
            ),
            ("leverage", decimal_number(&p.leverage)?),
            ("expiryPeriod", safe_integer(p.expiry_period)?),
            ("series", n(f64::from(p.series))),
            ("inverse", SourceValue::Bool(p.inverse)),
            ("fee", decimal_number(&p.fee)?),
            (
                "whitelist",
                p.whitelist
                    .map(|x| n(f64::from(x)))
                    .unwrap_or(SourceValue::Null),
            ),
        ]))
    }
}
impl SourceBackend for EconomicSourceBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.arguments.len() > crate::validity_source::MAX_ARGUMENTS {
            return Err(SourceRuleError::Capacity);
        }
        for arg in &call.arguments {
            arg.validate()?;
        }
        let registry = self.state.joint().registry();
        let value = match call.target.as_str() {
            "Activation.getAdmin" => {
                arity(call, 0)?;
                return Ok(DependencyReturn::Immediate(text(
                    registry.protocol_admin_unverified().as_str(),
                )));
            }
            "Activation.isTxTypeActive" => {
                arity(call, 1)?;
                let id = integer(&call.arguments[0])?;
                SourceValue::Bool(
                    u8::try_from(id)
                        .ok()
                        .and_then(|id| registry.activation(id))
                        .is_some(),
                )
            }
            "Activation.areActivationsAboveThreshold" => {
                arity(call, 0)?;
                let count = (0..=crate::registry::MAX_ACTIVATION_TYPE)
                    .filter(|ty| registry.activation(*ty).is_some())
                    .count();
                SourceValue::Bool(count * 10 > 36 * 9)
            }
            "Tally.getTally" => self.tally(call)?,
            "Tally.hasSufficientBalance" => self.sufficient(call, false)?,
            "Tally.hasSufficientChannel" => self.sufficient(call, true)?,
            "Oracle.isAdmin" => {
                arity(call, 2)?;
                let actor = account(&call.arguments[0])?;
                let id = identifier(&call.arguments[1])?;
                SourceValue::Bool(
                    registry
                        .oracle(id)
                        .is_some_and(|o| actor == o.admin || actor == o.backup),
                )
            }
            "Oracle.getOracleInfo" => self.oracle_info(call)?,
            "Oracle.getOraclePrice" => {
                arity(call, 1)?;
                let id = identifier(&call.arguments[0])?;
                let quote = registry
                    .oracle(id)
                    .and_then(|o| o.latest.as_ref())
                    .ok_or(SourceRuleError::Backend)?;
                // There is no source default-1 price in an owner-backed market read.
                scaled_number(quote.price_1e4, 4)?
            }
            "Contract.getContractInfo" => self.contract_info(call)?,
            _ => return Err(SourceRuleError::UnknownDependency),
        };
        value.validate()?;
        Ok(DependencyReturn::Pending(value))
    }
}

struct Custody {
    available: u128,
    reserve: u128,
    margin: u128,
    channel: u128,
    total: u128,
    vesting: u128,
}
fn n(value: f64) -> SourceValue {
    SourceValue::number(value)
}
fn text(value: &str) -> SourceValue {
    SourceValue::String(value.into())
}
fn object<const N: usize>(fields: [(&str, SourceValue); N]) -> SourceValue {
    SourceValue::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn arity(call: &DependencyCall, count: usize) -> Result<(), SourceRuleError> {
    if call.arguments.len() == count {
        Ok(())
    } else {
        Err(SourceRuleError::InvalidInput)
    }
}
fn sum(values: impl IntoIterator<Item = u128>) -> Result<u128, SourceRuleError> {
    values.into_iter().try_fold(0u128, |total, value| {
        total.checked_add(value).ok_or(SourceRuleError::Backend)
    })
}
fn integer(value: &SourceValue) -> Result<u64, SourceRuleError> {
    match value {
        SourceValue::Number(bits) => {
            let n = f64::from_bits(*bits);
            if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
                return Err(SourceRuleError::UnsupportedNumericDomain);
            }
            Ok(n as u64)
        }
        SourceValue::String(text) => {
            let n: u64 = text.parse().map_err(|_| SourceRuleError::InvalidInput)?;
            if n.to_string() != *text || n > 9_007_199_254_740_991 {
                return Err(SourceRuleError::InvalidInput);
            }
            Ok(n)
        }
        _ => Err(SourceRuleError::InvalidInput),
    }
}
fn identifier(value: &SourceValue) -> Result<u32, SourceRuleError> {
    let n = u32::try_from(integer(value)?).map_err(|_| SourceRuleError::InvalidInput)?;
    if n == 0 {
        Err(SourceRuleError::InvalidInput)
    } else {
        Ok(n)
    }
}
fn property(value: &SourceValue) -> Result<PropertyId, SourceRuleError> {
    PropertyId::try_from(identifier(value)?).map_err(|_| SourceRuleError::InvalidInput)
}
fn account(value: &SourceValue) -> Result<AccountId, SourceRuleError> {
    let SourceValue::String(text) = value else {
        return Err(SourceRuleError::InvalidInput);
    };
    if text.starts_with('~') {
        return Err(SourceRuleError::InvalidInput);
    }
    AccountId::try_from(text.as_str()).map_err(|_| SourceRuleError::InvalidInput)
}
fn safe_integer(value: u64) -> Result<SourceValue, SourceRuleError> {
    if value > 9_007_199_254_740_991 {
        return Err(SourceRuleError::UnsupportedNumericDomain);
    }
    Ok(n(value as f64))
}
fn nonnegative_decimal(value: &SourceValue) -> Result<Decimal, SourceRuleError> {
    let text = match value {
        SourceValue::Number(bits) => crate::state_oracle::source_number_text(f64::from_bits(*bits))
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
        SourceValue::BigNumber(text) | SourceValue::String(text) => text.clone(),
        _ => return Err(SourceRuleError::InvalidInput),
    };
    let value = Decimal::from_str(&text).map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    if value.is_negative() {
        Err(SourceRuleError::UnsupportedNumericDomain)
    } else {
        Ok(value)
    }
}
fn decimal_number(value: &Decimal) -> Result<SourceValue, SourceRuleError> {
    let number: f64 = value
        .to_string()
        .parse()
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    let result = n(number);
    if nonnegative_decimal(&result)? != *value {
        return Err(SourceRuleError::UnsupportedNumericDomain);
    }
    Ok(result)
}
fn scaled_number(value: u128, scale: u32) -> Result<SourceValue, SourceRuleError> {
    let divisor = 10u128
        .checked_pow(scale)
        .ok_or(SourceRuleError::UnsupportedNumericDomain)?;
    let decimal = Decimal::from_str(&format!(
        "{}.{:0width$}",
        value / divisor,
        value % divisor,
        width = scale as usize
    ))
    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    decimal_number(&decimal)
}
fn token_number(value: u128) -> Result<SourceValue, SourceRuleError> {
    scaled_number(value, 8)
}
