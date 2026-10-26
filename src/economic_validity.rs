//! Immutable source-rule dependency reads from the actual economic owner.
//! This named projection supplies facts, never chain or settlement authority.
use crate::chain::Tip;
use crate::contract_registry::ContractId;
use crate::decimal::{Decimal, Rounding};
use crate::economic::EconomicState;
use crate::economic_chain::{DurableEconomicChain, EconomicChain, EconomicHead};
use crate::ledger::{AccountId, PropertyId, TxId};
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

/// The borrow prevents economic mutation during evaluation. Catalog queries
/// run on a clone of the actual pending metadata owners; the sole Ledger stays
/// borrowed. The sequential owner can retain genuine query/cache effects.
pub struct EconomicSourceBackend<'a> {
    state: &'a EconomicState,
    context: ValidationContext,
    durable_head: Option<EconomicHead>,
    catalogs: Option<crate::source_catalogs::SourceCatalogOwners>,
}
impl<'a> EconomicSourceBackend<'a> {
    pub fn from_chain_unverified(
        chain: &'a EconomicChain,
        expected_parent: Tip,
    ) -> Result<Self, EconomicValidityError> {
        if chain.tip() != expected_parent {
            return Err(EconomicValidityError::WrongHead);
        }
        Self::from_state_unverified(chain.state(), expected_parent)
    }

    /// Inspect a genuine owner directly, with invariant and exact-height
    /// checks. Source-enabled owners also retain and check the actual parent
    /// ID. A source-free state alone has no stored block ID; callers requiring
    /// that binding must use the chain-backed constructor.
    pub fn from_state_unverified(
        state: &'a EconomicState,
        expected_parent: Tip,
    ) -> Result<Self, EconomicValidityError> {
        if state.config().source_consensus.is_some()
            && state
                .source_parent_unverified()
                .map_err(|_| EconomicValidityError::WrongHead)?
                != expected_parent
        {
            return Err(EconomicValidityError::WrongHead);
        }
        state
            .verify()
            .map_err(|_| EconomicValidityError::Invariants)?;
        let context = ValidationContext {
            network: state.config().network,
            height: expected_parent
                .height
                .checked_add(1)
                .ok_or(EconomicValidityError::WrongContext)?,
            parent: expected_parent,
        };
        Self::for_staged_input(state, context)
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
            catalogs: state.source_catalog_owners().cloned(),
        })
    }
    pub fn context(&self) -> ValidationContext {
        self.context
    }
    pub fn durable_head(&self) -> Option<EconomicHead> {
        self.durable_head
    }
    pub(crate) fn into_catalogs(self) -> Option<crate::source_catalogs::SourceCatalogOwners> {
        self.catalogs
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
        let Some(property) = tally_property(&call.arguments[1])? else {
            return Ok(n(0.0));
        };
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

    fn address_balances(
        &mut self,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        arity(call, 1)?;
        // Source Map.has uses the unchanged address value. Non-string keys
        // cannot match any genuine Ledger account and return the empty list.
        let SourceValue::String(address) = &call.arguments[0] else {
            return Ok(DependencyReturn::Pending(SourceValue::Array(vec![])));
        };
        let Ok(owner) = AccountId::try_from(address.as_str()) else {
            return Ok(DependencyReturn::Pending(SourceValue::Array(vec![])));
        };
        // Conserving escrows are custody implementation accounts, never
        // source address-map owners with independently spendable balances.
        if address.starts_with('~') {
            return Ok(DependencyReturn::Pending(SourceValue::Array(vec![])));
        }
        let ids: Vec<_> = self
            .state
            .ledger()
            .property_rows()
            .map(|(id, _)| id)
            .collect();
        let mut rows = Vec::new();
        for id in ids {
            let Some(c) = self.custody(&owner, id)? else {
                continue;
            };
            // Object iteration yields string property IDs. This actual source
            // lookup also retains the catalog's genuine typed-key/cache effect.
            let property_id = text(&id.get().to_string());
            let info = match self
                .catalogs
                .as_mut()
                .ok_or(SourceRuleError::UnknownDependency)?
                .invoke_read(&DependencyCall {
                    target: "Property.getPropertyData".into(),
                    arguments: vec![property_id.clone()],
                })? {
                DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => value,
                other => return Ok(other),
            };
            if matches!(info, SourceValue::Null | SourceValue::Undefined) {
                // Pinned code computes a fallback ticker, then nevertheless
                // dereferences info.ticker in its result object.
                let kind = if info == SourceValue::Null {
                    "null"
                } else {
                    "undefined"
                };
                return Ok(DependencyReturn::PendingThrow(
                    crate::validity_source::SourceException {
                        name: "TypeError".into(),
                        message: format!("Cannot read properties of {kind} (reading 'ticker')"),
                    },
                ));
            }
            rows.push(object([
                ("propertyId", property_id),
                ("ticker", info.get("ticker")),
                ("amount", token_number(c.total)?),
                ("available", token_number(c.available)?),
                ("reserved", token_number(c.reserve)?),
                ("margin", token_number(c.margin)?),
                ("vesting", token_number(c.vesting)?),
                ("channel", token_number(c.channel)?),
            ]));
        }
        let result = SourceValue::Array(rows);
        result.validate()?;
        Ok(DependencyReturn::Pending(result))
    }

    fn sufficient(
        &self,
        call: &DependencyCall,
        channel: bool,
    ) -> Result<SourceValue, SourceRuleError> {
        arity(call, 3)?;
        let owner = account(&call.arguments[0])?;
        let property = tally_property(&call.arguments[1])?;
        let required = nonnegative_decimal(&call.arguments[2])?;
        // Source compares its bucket and requirement with JavaScript '<'
        // before BigNumber shortfall arithmetic. Exclude a string/BigNumber
        // requirement whose Number coercion loses value; an exact Decimal
        // comparison would otherwise silently introduce a different rule.
        decimal_number(&required)?;
        let custody = if let Some(property) = property {
            self.custody(&owner, property)?
        } else {
            None
        };
        let Some(c) = custody else {
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

    /// Source loads the literal database key. In particular, the address query
    /// does not normalize a reversed token pair and does not inspect the queue.
    /// The rows come only from settled, still-open orders of the sole owner.
    fn orderbook(&self, key: &str) -> Result<SourceValue, SourceRuleError> {
        let mut buy = Vec::new();
        let mut sell = Vec::new();
        if key == self.state.book().contract().get().to_string() {
            for row in self.state.book().resting() {
                let order = &row.order;
                let value = object([
                    ("contractId", n(f64::from(order.contract.get()))),
                    ("amount", decimal_number(&row.remaining)?),
                    ("price", decimal_number(&order.price)?),
                    ("blockTime", safe_integer(order.height)?),
                    (
                        "sell",
                        SourceValue::Bool(order.side == crate::contract_orderbook::Side::Sell),
                    ),
                    (
                        "initMargin",
                        token_number(self.state.reserve_atoms(order.txid))?,
                    ),
                    ("sender", text(order.owner.as_str())),
                    ("txid", text(&order_id_text(order.txid))),
                    ("post", SourceValue::Bool(order.post)),
                    ("initialReduce", SourceValue::Bool(row.initial_reduce)),
                ]);
                if order.side == crate::contract_orderbook::Side::Buy {
                    buy.push(value);
                } else {
                    sell.push(value);
                }
            }
        } else {
            for row in self.state.joint().spot().open_order_rows() {
                let pair = row.pair();
                if key != format!("{}-{}", pair.base.get(), pair.quote.get()) {
                    continue;
                }
                // Source calculatePrice keeps the initial offered/expected
                // ratio even as matching reduces the two amounts.
                let price = Decimal::from(row.offered_atoms)
                    .checked_div(&Decimal::from(row.expected_atoms), 8, Rounding::HalfUp)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let remaining_expected = if row.side() == crate::spot::SpotSide::SellBase {
                    // The conserving book only needs the seller's remaining
                    // offered atoms. Source also reduces amountExpected by
                    // each actual quote leg, so derive that field from fills.
                    let delivered = sum(self
                        .state
                        .joint()
                        .spot()
                        .fills()
                        .iter()
                        .filter(|fill| fill.sell_txid == row.txid)
                        .map(|fill| u128::from(fill.quote_atoms)))?;
                    u128::from(row.expected_atoms)
                        .checked_sub(delivered)
                        .ok_or(SourceRuleError::UnsupportedNumericDomain)?
                } else {
                    u128::from(row.remaining_desired)
                };
                let id = order_id_text(row.txid);
                let value = object([
                    ("offeredPropertyId", n(f64::from(row.offered.get()))),
                    ("desiredPropertyId", n(f64::from(row.desired.get()))),
                    (
                        "amountOffered",
                        token_number(u128::from(row.remaining_offered))?,
                    ),
                    ("amountExpected", token_number(remaining_expected)?),
                    ("blockTime", safe_integer(row.height)?),
                    ("sender", text(row.owner.as_str())),
                    ("stop", SourceValue::Bool(false)),
                    ("post", SourceValue::Bool(false)),
                    ("price", decimal_number(&price)?),
                    // The pinned JS truncates only this display field. The
                    // actual owner retains all 32 bytes for reserve/replay.
                    (
                        "txid",
                        text(&format!("{}{}", &id[..3], &id[id.len() - 4..])),
                    ),
                ]);
                if row.side() == crate::spot::SpotSide::BuyBase {
                    buy.push(value);
                } else {
                    sell.push(value);
                }
            }
        }
        Ok(object([
            ("buy", SourceValue::Array(buy)),
            ("sell", SourceValue::Array(sell)),
        ]))
    }

    fn orderbook_instance(&self, call: &DependencyCall) -> Result<SourceValue, SourceRuleError> {
        arity(call, 1)?;
        let key = orderbook_key(&call.arguments[0])?;
        Ok(object([
            ("tickSize", SourceValue::BigNumber("0.00000001".into())),
            ("orderBookKey", call.arguments[0].clone()),
            (
                "orderBooks",
                SourceValue::Object(vec![(key.clone(), self.orderbook(&key)?)]),
            ),
            ("block", n(1.0)),
        ]))
    }

    fn orders_for_address(&self, call: &DependencyCall) -> Result<SourceValue, SourceRuleError> {
        if call.arguments.len() != 2 && call.arguments.len() != 4 {
            return Err(SourceRuleError::InvalidInput);
        }
        let contract = &call.arguments[1];
        let key = if source_truth(contract) {
            orderbook_key(contract)?
        } else {
            format!(
                "{}-{}",
                orderbook_key(call.arguments.get(2).unwrap_or(&SourceValue::Undefined))?,
                orderbook_key(call.arguments.get(3).unwrap_or(&SourceValue::Undefined))?
            )
        };
        if !matches!(call.arguments[0], SourceValue::String(_)) {
            return Ok(SourceValue::Array(vec![]));
        }
        let book = self.orderbook(&key)?;
        let mut result = Vec::new();
        for side in ["buy", "sell"] {
            let SourceValue::Array(rows) = book.get(side) else {
                return Err(SourceRuleError::Backend);
            };
            result.extend(
                rows.into_iter()
                    .filter(|row| row.get("sender") == call.arguments[0]),
            );
        }
        Ok(SourceValue::Array(result))
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
        if call.target.starts_with("Property.") || call.target.starts_with("ClearList.") {
            return self
                .catalogs
                .as_mut()
                .ok_or(SourceRuleError::UnknownDependency)?
                .invoke_read(call);
        }
        if call.target == "Bech32.decode" {
            arity(call, 1)?;
            let SourceValue::String(address) = &call.arguments[0] else {
                return Err(SourceRuleError::InvalidInput);
            };
            return Ok(match decode_bech32(address) {
                Some((prefix, words, 1)) => DependencyReturn::Immediate(object([
                    ("prefix", text(&prefix)),
                    (
                        "words",
                        SourceValue::Array(words.into_iter().map(|w| n(f64::from(w))).collect()),
                    ),
                ])),
                _ => DependencyReturn::Throw(crate::validity_source::SourceException {
                    name: "Error".into(),
                    message: format!("Invalid checksum or encoding for {address}"),
                }),
            });
        }
        if call.target == "Tally.getAddressBalances" {
            return self.address_balances(call);
        }
        if let Some(method) = call.target.strip_prefix("Oracle.") {
            if let Some(owner) = self
                .catalogs
                .as_mut()
                .and_then(|owners| owners.oracles.as_mut())
            {
                if !matches!(
                    method,
                    "getOracleInfo"
                        | "getOraclePrice"
                        | "isAdmin"
                        | "verifyAdmin"
                        | "getAllOracles"
                        | "getTWAP"
                        | "getNextId"
                        | "loadOracleData"
                        | "getStake"
                ) {
                    return Err(SourceRuleError::UnknownDependency);
                }
                // Genuine owner reads retain cache/DB effects in the pending
                // catalogs. In particular getOraclePrice keeps source's 1
                // fallback; funded market reads use market_oracle_view instead.
                return owner.invoke_source(method, &call.arguments);
            }
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
            "Orderbook.getOrderbookInstance" => self.orderbook_instance(call)?,
            // Both source methods are async. Keeping Pending here is vital:
            // validateCancelOrder reads Promise.length without awaiting it.
            "Orderbook.getOrdersForAddress" => self.orders_for_address(call)?,
            "TxUtils.validateAddressWrapper" => {
                arity(call, 1)?;
                let SourceValue::String(address) = &call.arguments[0] else {
                    return Err(SourceRuleError::InvalidInput);
                };
                object([(
                    "isvalid",
                    SourceValue::Bool(address_format_valid(self.context.network, address)?),
                )])
            }
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
fn order_id_text(id: TxId) -> String {
    id.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn source_truth(value: &SourceValue) -> bool {
    match value {
        SourceValue::Undefined | SourceValue::Null => false,
        SourceValue::Bool(value) => *value,
        SourceValue::Number(bits) => {
            let value = f64::from_bits(*bits);
            value != 0.0 && !value.is_nan()
        }
        SourceValue::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn orderbook_key(value: &SourceValue) -> Result<String, SourceRuleError> {
    Ok(match value {
        SourceValue::Undefined => "undefined".into(),
        SourceValue::Null => "null".into(),
        SourceValue::Bool(value) => value.to_string(),
        SourceValue::String(value) | SourceValue::BigNumber(value) => value.clone(),
        SourceValue::Number(bits) => {
            let value = f64::from_bits(*bits);
            if value.is_nan() {
                "NaN".into()
            } else if value == f64::INFINITY {
                "Infinity".into()
            } else if value == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(value)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
            }
        }
        SourceValue::Array(values) => values
            .iter()
            .map(|value| {
                if matches!(value, SourceValue::Undefined | SourceValue::Null) {
                    Ok(String::new())
                } else {
                    orderbook_key(value)
                }
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(","),
        SourceValue::Object(fields) => {
            if fields.iter().any(|(name, _)| name == "toString") {
                return Err(SourceRuleError::UnsupportedNumericDomain);
            }
            "[object Object]".into()
        }
        SourceValue::Promise => "[object Promise]".into(),
    })
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
// Pinned tally.js getTally uses `addressObj[propertyId]`. In particular the
// source send validator passes its entire propertyIds array: [7] names key
// "7", while [7,8] names the absent key "7,8" and returns zero. Keep the
// established canonical domain for scalar queries; do not broaden those IDs.
fn tally_property(value: &SourceValue) -> Result<Option<PropertyId>, SourceRuleError> {
    if !matches!(value, SourceValue::Array(_)) {
        return property(value).map(Some);
    }
    fn key(value: &SourceValue) -> Result<String, SourceRuleError> {
        Ok(match value {
            SourceValue::Array(values) => values
                .iter()
                .map(key)
                .collect::<Result<Vec<_>, _>>()?
                .join(","),
            SourceValue::Undefined | SourceValue::Null => String::new(),
            SourceValue::String(text) => text.clone(),
            SourceValue::Bool(value) => value.to_string(),
            SourceValue::Number(bits) => {
                let number = f64::from_bits(*bits);
                if number.is_nan() {
                    "NaN".into()
                } else if number == f64::INFINITY {
                    "Infinity".into()
                } else if number == f64::NEG_INFINITY {
                    "-Infinity".into()
                } else {
                    crate::state_oracle::source_number_text(number)
                        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
                }
            }
            SourceValue::Object(fields) => {
                if fields.iter().any(|(name, _)| name == "toString") {
                    return Err(SourceRuleError::InvalidInput);
                }
                "[object Object]".into()
            }
            SourceValue::Promise => "[object Promise]".into(),
            SourceValue::BigNumber(_) => return Err(SourceRuleError::UnsupportedNumericDomain),
        })
    }
    let text = key(value)?;
    if matches!(
        text.as_str(),
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
        return Err(SourceRuleError::InvalidInput);
    }
    let Some(id) = text
        .parse::<u32>()
        .ok()
        .filter(|id| *id != 0 && id.to_string() == text)
    else {
        return Ok(None);
    };
    Ok(Some(
        PropertyId::try_from(id).map_err(|_| SourceRuleError::InvalidInput)?,
    ))
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

// txUtils.validateAddressWrapper delegates to a node RPC. This bounded native
// replacement supplies its address-format `isvalid` fact only. It does not
// fabricate wallet, script, public-key, ownership or transaction-output data.
// Network versions follow the mainnet chainparams; witness rules follow BIP350:
// https://github.com/bitcoin/bips/blob/master/bip-0350.mediawiki
// https://github.com/litecoin-project/litecoin/blob/v0.21.4/src/key_io.cpp
// https://github.com/dogecoin/dogecoin/blob/v1.14.9/src/chainparams.cpp
fn address_format_valid(
    network: TradeLayerMainnet,
    address: &str,
) -> Result<bool, SourceRuleError> {
    if network == TradeLayerMainnet::Litecoin
        && address.to_ascii_lowercase().starts_with("ltcmweb1")
    {
        return Err(SourceRuleError::UnknownDependency);
    }
    let versions: &[u8] = match network {
        TradeLayerMainnet::Bitcoin => &[0, 5],
        TradeLayerMainnet::Litecoin => &[48, 5, 50],
        TradeLayerMainnet::Dogecoin => &[30, 22],
    };
    if let Some(bytes) = decode_base58_address(address) {
        if versions.contains(&bytes[0])
            && crate::hash256::double_sha256(&bytes[..21])[..4] == bytes[21..]
        {
            return Ok(true);
        }
    }
    let Some((hrp, words, encoding)) = decode_bech32(address) else {
        return Ok(false);
    };
    let expected = match network {
        TradeLayerMainnet::Bitcoin => "bc",
        TradeLayerMainnet::Litecoin => "ltc",
        TradeLayerMainnet::Dogecoin => return Ok(false),
    };
    if hrp != expected || words.is_empty() || words[0] > 16 {
        return Ok(false);
    }
    let version = words[0];
    let mut accumulator = 0_u32;
    let mut bits = 0_u32;
    let mut length = 0;
    for word in &words[1..] {
        accumulator = ((accumulator << 5) | u32::from(*word)) & 0x7fff;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            length += 1;
        }
    }
    if bits >= 5 || ((accumulator << (8 - bits)) & 255) != 0 || !(2..=40).contains(&length) {
        return Ok(false);
    }
    Ok(if version == 0 {
        (length == 20 || length == 32) && encoding == 1
    } else {
        encoding == 0x2bc8_30a3
    })
}
fn decode_base58_address(address: &str) -> Option<[u8; 25]> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let address = address.trim_matches(|c: char| c.is_ascii_whitespace());
    if address.is_empty() || address.len() > 35 {
        return None;
    }
    let zeros = address.bytes().take_while(|b| *b == b'1').count();
    let mut bytes = vec![];
    for ch in address.bytes().skip(zeros) {
        let mut carry = ALPHABET.iter().position(|b| *b == ch)? as u32;
        for byte in bytes.iter_mut().rev() {
            carry += u32::from(*byte) * 58;
            *byte = (carry & 255) as u8;
            carry >>= 8;
        }
        while carry != 0 {
            bytes.insert(0, (carry & 255) as u8);
            carry >>= 8;
        }
        if zeros + bytes.len() > 25 {
            return None;
        }
    }
    if zeros + bytes.len() != 25 {
        return None;
    }
    let mut output = [0; 25];
    output[zeros..].copy_from_slice(&bytes);
    Some(output)
}
fn decode_bech32(address: &str) -> Option<(String, Vec<u8>, u32)> {
    if !(8..=90).contains(&address.len()) || !address.bytes().all(|b| (33..=126).contains(&b)) {
        return None;
    }
    if address.bytes().any(|b| b.is_ascii_lowercase())
        && address.bytes().any(|b| b.is_ascii_uppercase())
    {
        return None;
    }
    let lower = address.to_ascii_lowercase();
    let separator = lower.rfind('1')?;
    if separator == 0 || separator + 7 > lower.len() {
        return None;
    }
    let prefix = &lower[..separator];
    const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let words = lower.as_bytes()[separator + 1..]
        .iter()
        .map(|b| CHARSET.iter().position(|v| v == b).map(|i| i as u8))
        .collect::<Option<Vec<_>>>()?;
    let values = prefix
        .bytes()
        .map(|b| b >> 5)
        .chain(core::iter::once(0))
        .chain(prefix.bytes().map(|b| b & 31))
        .chain(words.iter().copied());
    let mut checksum = 1_u32;
    for value in values {
        let top = checksum >> 25;
        checksum = ((checksum & 0x1ff_ffff) << 5) ^ u32::from(value);
        for (bit, generator) in [
            0x3b6a_57b2,
            0x2650_8e6d,
            0x1ea1_19fa,
            0x3d42_33dd,
            0x2a14_62b3,
        ]
        .iter()
        .enumerate()
        {
            if top & (1 << bit) != 0 {
                checksum ^= generator;
            }
        }
    }
    if checksum != 1 && checksum != 0x2bc8_30a3 {
        return None;
    }
    Some((prefix.into(), words[..words.len() - 6].to_vec(), checksum))
}
