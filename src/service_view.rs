//! Immutable, bounded projections of the sole economic owner. This facade is
//! not an RPC validator, indexer, wallet, consensus-hash store or signer.
use crate::client::RpcValue;
use crate::contract_orderbook::{Order, Side};
use crate::contract_registry::{ContractId, ContractSeries};
use crate::decimal::{Decimal, Rounding};
use crate::economic::{EconomicState, FundingEpoch, MarkEvent, MarkEventKind};
use crate::economic_chain::{DurableEconomicChain, EconomicChain};
use crate::economic_reporting::EconomicHistory;
use crate::economic_spot_prices::{
    EconomicSpotPrices, ExecutedSpotPrice, SpotPriceError, PRICE_POLICY,
};
use crate::ledger::{AccountId, PropertyId, PropertyKind, TxId};
use crate::mark_basis::BasisAction;
use crate::registry::MAX_ACTIVATION_TYPE;
use crate::service::{ServiceBackend, ServiceCall, ServiceError, MAX_SERVICE_RESPONSE};
use crate::spot::SpotPair;
use crate::trade_history::ContractEventOrigin;

pub const READ_POLICY: &str = "ImmutableEconomicReadV1";
pub const AUDIT_POLICY: &str = "LosslessJsonAuditV1";
pub const MAX_AUDIT_ROWS: usize = 65_536;
type Result<T> = core::result::Result<T, ServiceError>;

/// The lifetime prevents changing the owner while a read view is in use. Only
/// bounded response metadata is allocated; no mutable Ledger is cloned.
pub struct EconomicReadBackend<'a> {
    chain: &'a EconomicChain,
    durable_sequence: Option<u64>,
}
impl<'a> EconomicReadBackend<'a> {
    pub fn from_chain(chain: &'a EconomicChain) -> Result<Self> {
        chain
            .state()
            .verify()
            .map_err(|_| ServiceError::Backend("economic_invariants"))?;
        Ok(Self {
            chain,
            durable_sequence: None,
        })
    }
    pub fn from_durable(owner: &'a DurableEconomicChain) -> Result<Self> {
        let chain = owner
            .chain()
            .map_err(|_| ServiceError::Backend("durable_health"))?;
        Self::from_chain(chain)
    }
    pub fn chain(&self) -> &'a EconomicChain {
        self.chain
    }
    fn state(&self) -> &'a EconomicState {
        self.chain.state()
    }
    fn selected(&self, id: u32) -> Result<()> {
        if id == self.state().config().contract.get() {
            Ok(())
        } else {
            Err(ServiceError::Unavailable("contract_runtime"))
        }
    }
    fn balances(&self, owner: &AccountId) -> Result<RpcValue> {
        if private(owner) {
            return Err(ServiceError::Unavailable("private_custody_account"));
        }
        array(
            self.state()
                .ledger()
                .property_rows()
                .map(|(id, _)| self.balance_row(id, owner)),
        )
    }
    fn balance_row(&self, id: PropertyId, owner: &AccountId) -> Result<RpcValue> {
        let s = self.state();
        let available = s.ledger().available_atoms(id, owner);
        let margin = if id == s.config().collateral {
            s.margin_atoms(owner)
        } else {
            0
        };
        let contract_reserve = checked_sum(
            s.book()
                .resting()
                .iter()
                .filter(|r| &r.order.owner == owner && id == s.config().collateral)
                .map(|r| s.reserve_atoms(r.order.txid)),
        )?;
        let spot_reserve = checked_sum(
            s.joint()
                .spot()
                .open_order_rows()
                .filter(|r| &r.owner == owner && r.offered == id)
                .map(|r| u128::from(r.remaining_offered)),
        )?;
        let channel = checked_sum(
            s.ledger()
                .channel_balance_rows()
                .filter(|(p, a, _, _, _)| *p == id && *a == owner)
                .map(|(_, _, _, _, n)| n),
        )?;
        let pending = s.ledger().pending_withdrawal_atoms(id, owner);
        let total = checked_sum([
            available,
            margin,
            contract_reserve,
            spot_reserve,
            channel,
            pending,
        ])?;
        object(vec![
            ("propertyId", num(id.get())),
            ("address", string(owner.as_str())?),
            ("availableAtoms", atoms(available)?),
            ("available", dec_atoms(available)?),
            ("contractMarginAtoms", atoms(margin)?),
            ("contractReserveAtoms", atoms(contract_reserve)?),
            ("spotReserveAtoms", atoms(spot_reserve)?),
            ("channelAtoms", atoms(channel)?),
            ("pendingWithdrawalAtoms", atoms(pending)?),
            ("ownedAtoms", atoms(total)?),
        ])
    }
    fn public_pairs(&self) -> Result<std::collections::BTreeSet<(PropertyId, AccountId)>> {
        let s = self.state();
        let mut pairs = std::collections::BTreeSet::new();
        let mut insert = |id, owner: &AccountId| -> Result<()> {
            if !private(owner) {
                pairs.insert((id, owner.clone()));
                if pairs.len() > MAX_AUDIT_ROWS {
                    return Err(ServiceError::Capacity);
                }
            }
            Ok(())
        };
        for (id, a, _) in s.ledger().available_rows() {
            insert(id, a)?;
        }
        for p in s.positions().positions() {
            if s.margin_atoms(&p.address) > 0 {
                insert(s.config().collateral, &p.address)?;
            }
        }
        for r in s.book().resting() {
            if s.reserve_atoms(r.order.txid) > 0 {
                insert(s.config().collateral, &r.order.owner)?;
            }
        }
        for r in s.joint().spot().open_order_rows() {
            if r.remaining_offered > 0 {
                insert(r.offered, &r.owner)?;
            }
        }
        for (id, a, _, _, n) in s.ledger().channel_balance_rows() {
            if n > 0 {
                insert(id, a)?;
            }
        }
        for (id, a, n) in s.ledger().pending_withdrawal_rows() {
            if n > 0 {
                insert(id, a)?;
            }
        }
        Ok(pairs)
    }
    fn property(&self, id: PropertyId) -> Result<RpcValue> {
        let p = self
            .state()
            .ledger()
            .property(id)
            .ok_or(ServiceError::NotFound)?;
        object(vec![
            ("propertyId", num(id.get())),
            ("issuer", string(p.issuer.as_str())?),
            (
                "kind",
                string(match p.kind {
                    PropertyKind::Fixed => "fixed",
                    PropertyKind::Managed => "managed",
                })?,
            ),
            ("totalSupplyAtoms", atoms(p.total_supply_atoms)?),
            ("totalSupply", dec_atoms(p.total_supply_atoms)?),
        ])
    }
    fn position(&self, owner: &AccountId) -> Result<RpcValue> {
        let Some(p) = self.state().positions().get(owner) else {
            return Ok(RpcValue::null());
        };
        object(vec![
            ("address", string(owner.as_str())?),
            ("contractId", num(self.state().config().contract.get())),
            ("contracts", decimal(&p.contracts)?),
            ("averagePrice", option_decimal(p.avg_price.as_ref())?),
            ("marginAtoms", atoms(self.state().margin_atoms(owner))?),
            ("lastMark", option_decimal(p.last_mark.as_ref())?),
            ("realizedPnl", decimal(&p.realized_pnl)?),
            ("unrealizedPnl", decimal(&p.unrealized_pnl)?),
            (
                "settledBasis",
                self.state().mark_state().bases.get(owner).map_or_else(
                    || Ok(RpcValue::null()),
                    |b| {
                        object(vec![
                            ("quantity", decimal(&b.quantity)?),
                            ("entryBasis", ratio(&b.entry_basis)?),
                            ("carry", ratio(&b.carry)?),
                        ])
                    },
                )?,
            ),
            (
                "unpaidLossAtoms",
                atoms(
                    self.state()
                        .mark_state()
                        .unpaid_losses
                        .get(owner)
                        .copied()
                        .unwrap_or(0),
                )?,
            ),
        ])
    }
    fn contract_history(&self, owner: Option<&AccountId>) -> Result<RpcValue> {
        let projection = EconomicHistory::from_state(self.state())
            .map_err(|_| ServiceError::Unavailable("contract_owner_history"))?;
        array(
            projection
                .history()
                .executions()
                .filter(|e| owner.is_none_or(|a| &e.buyer == a || &e.seller == a))
                .map(|e| {
                    let origin = match e.origin {
                        ContractEventOrigin::Orders {
                            buy_txid,
                            sell_txid,
                        } => object(vec![
                            ("kind", string("orders")?),
                            ("buyTxid", txid(buy_txid)?),
                            ("sellTxid", txid(sell_txid)?),
                        ])?,
                        ContractEventOrigin::ForcedBook {
                            epoch,
                            fill_index,
                            maker_txid,
                        } => object(vec![
                            ("kind", string("forced_book")?),
                            ("epoch", txid(epoch)?),
                            ("fillIndex", num(fill_index)),
                            ("makerTxid", txid(maker_txid)?),
                        ])?,
                        ContractEventOrigin::ForcedAmm {
                            epoch,
                            fill_index,
                            pool_id,
                        } => object(vec![
                            ("kind", string("forced_amm")?),
                            ("epoch", txid(epoch)?),
                            ("fillIndex", num(fill_index)),
                            ("poolId", num(pool_id)),
                        ])?,
                    };
                    object(vec![
                        ("id", string(&hex(&e.id.0))?),
                        ("height", num(e.block_height)),
                        ("eventIndex", num(e.event_index)),
                        ("contractId", num(e.contract_id)),
                        ("buyer", string(e.buyer.as_str())?),
                        ("seller", string(e.seller.as_str())?),
                        ("quantity", decimal(&e.quantity)?),
                        ("price", decimal(&e.price)?),
                        ("origin", origin),
                        ("buyerBookedPnl", decimal(&e.buyer_pnl.booked)?),
                        ("sellerBookedPnl", decimal(&e.seller_pnl.booked)?),
                        ("retainedFeeAtoms", atoms(e.retained_fee_atoms)?),
                        ("collectedLossAtoms", atoms(e.collected_loss_atoms)?),
                        ("paidClaimAtoms", atoms(e.paid_claim_atoms)?),
                        ("unfundedLossAtoms", atoms(e.unfunded_loss_atoms)?),
                    ])
                }),
        )
    }
    fn clearing(&self, height: Option<u64>) -> Result<RpcValue> {
        if self.state().config().marking.is_none() {
            return Err(ServiceError::Unavailable("mark_history"));
        }
        array(
            self.state()
                .mark_state()
                .events
                .iter()
                .filter(|e| height.is_none_or(|h| h == e.height))
                .map(mark_event),
        )
    }
}
impl ServiceBackend for EconomicReadBackend<'_> {
    fn call(&mut self, call: ServiceCall) -> Result<RpcValue> {
        let args = Args(&call.arguments);
        let s = self.state();
        let registry = s.joint().registry();
        match call.target {
            "runtime.status" => {
                args.count(0)?;
                object(vec![
                    ("policy", string(READ_POLICY)?),
                    (
                        "network",
                        string(match s.config().network {
                            crate::network_profile::TradeLayerMainnet::Bitcoin => "bitcoin",
                            crate::network_profile::TradeLayerMainnet::Litecoin => "litecoin",
                            crate::network_profile::TradeLayerMainnet::Dogecoin => "dogecoin",
                        })?,
                    ),
                    ("height", num(self.chain.tip().height)),
                    ("blockId", string(&hex(&self.chain.tip().id.0))?),
                    ("rollbackFloor", num(self.chain.rollback_floor().height)),
                    ("durableSequence", opt_num(self.durable_sequence)),
                    ("authenticated", RpcValue::boolean(false)),
                ])
            }
            "runtime.heights" => {
                args.count(0)?;
                object(vec![
                    ("maxProcessedHeight", opt_num(s.finalized_height())),
                    ("maxParsedHeight", RpcValue::null()),
                    ("trackHeight", RpcValue::null()),
                ])
            }
            "ledger.balances" | "wallet.balances" => {
                args.count(1)?;
                self.balances(&args.owner(0)?)
            }
            "ledger.properties" => {
                args.count(0)?;
                array(s.ledger().property_rows().map(|(id, _)| self.property(id)))
            }
            "ledger.property" => {
                args.count(1)?;
                self.property(args.property(0)?)
            }
            "ledger.total_tokens" => {
                args.count(1)?;
                let p = s
                    .ledger()
                    .property(args.property(0)?)
                    .ok_or(ServiceError::NotFound)?;
                object(vec![
                    ("atoms", atoms(p.total_supply_atoms)?),
                    ("tokens", dec_atoms(p.total_supply_atoms)?),
                ])
            }
            "ledger.addresses_with_property" => {
                args.count(1)?;
                let id = args.property(0)?;
                if s.ledger().property(id).is_none() {
                    return Err(ServiceError::NotFound);
                }
                array(
                    self.public_pairs()?
                        .iter()
                        .filter(|(p, _)| *p == id)
                        .map(|(p, a)| self.balance_row(*p, a)),
                )
            }
            "ledger.audit" | "ledger.all_balances" => {
                args.count(0)?;
                array(
                    self.public_pairs()?
                        .iter()
                        .map(|(p, a)| self.balance_row(*p, a)),
                )
            }
            "ledger.custody" => {
                args.count(0)?;
                object(vec![
                    ("collateralPropertyId", num(s.config().collateral.get())),
                    ("lossPoolAtoms", atoms(s.loss_pool_atoms())?),
                    ("insuranceCustodyAtoms", atoms(s.insurance_custody_atoms())?),
                    ("grossUnfundedLossAtoms", atoms(s.unfunded_loss_atoms())?),
                    (
                        "privateAvailable",
                        array(
                            s.ledger()
                                .available_rows()
                                .filter(|(_, a, _)| private(a))
                                .map(|(p, a, n)| {
                                    object(vec![
                                        ("propertyId", num(p.get())),
                                        ("account", string(a.as_str())?),
                                        ("atoms", atoms(n)?),
                                    ])
                                }),
                        )?,
                    ),
                ])
            }
            "registry.activations" => {
                args.count(0)?;
                array(
                    (0..=MAX_ACTIVATION_TYPE)
                        .filter_map(|id| registry.activation(id).map(|a| (id, a)))
                        .map(|(id, a)| {
                            object(vec![
                                ("type", num(id)),
                                ("height", num(a.height)),
                                ("codeHash", string(&hex(&a.code_hash))?),
                            ])
                        }),
                )
            }
            "registry.is_active" | "registry.feature_status" => {
                args.count(1)?;
                let id = args.u32(0)?;
                let active = u8::try_from(id)
                    .ok()
                    .and_then(|i| registry.activation(i))
                    .is_some();
                if call.target == "registry.is_active" {
                    Ok(RpcValue::boolean(active))
                } else if active {
                    object(vec![("status", string("active")?)])
                } else {
                    object(vec![
                        ("status", string("inactive")?),
                        (
                            "message",
                            string(&format!("Feature ID {id} not found or not active."))?,
                        ),
                    ])
                }
            }
            "registry.oracles" => {
                args.count(0)?;
                array(registry.oracle_rows().map(|(id, o)| {
                    let latest = if let Some(q) = &o.latest {
                        object(vec![
                            ("height", num(q.height)),
                            (
                                "price",
                                decimal(
                                    &Decimal::from(q.price_1e4)
                                        .checked_div(&Decimal::from(10_000_u32), 4, Rounding::Down)
                                        .map_err(|_| ServiceError::Backend("oracle_price"))?,
                                )?,
                            ),
                            ("price1e4", atoms(q.price_1e4)?),
                            ("high1e4", opt_atoms(q.high_1e4)?),
                            ("low1e4", opt_atoms(q.low_1e4)?),
                            ("close1e4", opt_atoms(q.close_1e4)?),
                        ])?
                    } else {
                        RpcValue::null()
                    };
                    object(vec![
                        ("id", num(id)),
                        ("admin", string(o.admin.as_str())?),
                        ("backup", string(o.backup.as_str())?),
                        ("ticker", string(&o.ticker)?),
                        ("url", string(&o.url)?),
                        ("lag", num(o.lag)),
                        ("closed", RpcValue::boolean(o.closed)),
                        ("latest", latest),
                    ])
                }))
            }
            "registry.clear_lists" => {
                args.count(0)?;
                array(registry.whitelist_rows().map(|(id, w)| list_row(id, w)))
            }
            "registry.clear_list" => {
                args.count(1)?;
                let id = args.u32(0)?;
                list_row(id, registry.whitelist(id).ok_or(ServiceError::NotFound)?)
            }
            "registry.attestations" => {
                args.count(2)?;
                let owner = args.owner(0)?;
                let id = args.u32(1)?;
                array(
                    registry
                        .attestation_rows()
                        .filter(|(i, a, _)| *i == id && **a == owner)
                        .map(|(i, a, t)| {
                            object(vec![
                                ("listId", num(i)),
                                ("address", string(a.as_str())?),
                                ("metadata", string(&t.metadata)?),
                                ("issuedHeight", num(t.issued_height)),
                                ("updatedHeight", num(t.updated_height)),
                                ("active", RpcValue::boolean(t.active)),
                            ])
                        }),
                )
            }
            "contract.list" => {
                args.count(0)?;
                if s.config().contract_binding.is_none() {
                    return Err(ServiceError::Unavailable("accepted_contract_catalog"));
                }
                array(
                    s.contract_binding_state()
                        .contracts
                        .iter()
                        .map(|c| contract_row(&c.definition)),
                )
            }
            "contract.info" => {
                args.count(1)?;
                let id = args.contract(0)?;
                if s.config().contract_binding.is_none() {
                    return Err(ServiceError::Unavailable("accepted_contract_definition"));
                }
                contract_row(&s.contract(id).ok_or(ServiceError::NotFound)?.definition)
            }
            "contract.runtime_config" => {
                args.count(0)?;
                object(vec![
                    ("contractId", num(s.config().contract.get())),
                    ("collateralPropertyId", num(s.config().collateral.get())),
                    ("inverse", RpcValue::boolean(s.config().inverse)),
                    ("configuredNotional", decimal(&s.config().notional)?),
                    (
                        "legacyInitialMarginPerContract",
                        decimal(&s.config().initial_margin_per_contract)?,
                    ),
                    (
                        "definitionBound",
                        RpcValue::boolean(s.config().contract_binding.is_some()),
                    ),
                ])
            }
            "contract.initial_margin" => {
                args.count(2)?;
                let id = args.contract(0)?;
                self.selected(id.get())?;
                let price = args.decimal(1)?;
                if price <= Decimal::zero() {
                    return Err(ServiceError::InvalidRequest);
                }
                if s.config().contract_binding.is_none() {
                    decimal(&s.config().initial_margin_per_contract)
                } else {
                    let catalog = s
                        .accepted_contract_catalog()
                        .map_err(|_| ServiceError::Backend("catalog"))?;
                    decimal(
                        &catalog
                            .initial_margin(id, &price)
                            .map_err(|_| ServiceError::InvalidRequest)?,
                    )
                }
            }
            "contract.book" => {
                args.count(1)?;
                self.selected(args.u32(0)?)?;
                object(vec![
                    ("contractId", num(s.config().contract.get())),
                    ("processedThrough", opt_num(s.book().processed_through())),
                    (
                        "resting",
                        array(s.book().resting().iter().map(|r| {
                            object(vec![
                                ("order", order_row(&r.order)?),
                                ("remaining", decimal(&r.remaining)?),
                                ("reserveQuantity", decimal(&r.reserve_quantity)?),
                                ("reserveAtoms", atoms(s.reserve_atoms(r.order.txid))?),
                                ("sequence", num(r.sequence)),
                            ])
                        }))?,
                    ),
                    ("queued", array(s.book().queued().iter().map(order_row))?),
                ])
            }
            "contract.amm" => {
                args.count(1)?;
                self.selected(args.u32(0)?)?;
                let cfg = s
                    .config()
                    .amm
                    .as_ref()
                    .ok_or(ServiceError::Unavailable("funded_contract_amm"))?;
                let pool = s
                    .amm_owner()
                    .map_err(|_| ServiceError::Backend("pool_owner"))?;
                let amm = s.amm_state();
                object(vec![
                    ("scope", string("economic_funded_pool")?),
                    ("poolId", num(cfg.pool_id)),
                    ("owner", string(pool.as_str())?),
                    ("cashAtoms", atoms(amm.cash_atoms)?),
                    ("marginAtoms", atoms(s.margin_atoms(&pool))?),
                    ("shareUnits", atoms(amm.total_units)?),
                    ("generation", num(amm.generation)),
                    (
                        "shares",
                        array(amm.shares.iter().map(|(a, n)| {
                            object(vec![("owner", string(a.as_str())?), ("units", atoms(*n)?)])
                        }))?,
                    ),
                    (
                        "quotes",
                        array(amm.quotes.iter().map(|q| {
                            object(vec![
                                ("epoch", txid(q.epoch_id)?),
                                ("height", num(q.height)),
                                (
                                    "side",
                                    string(if q.side == Side::Buy { "buy" } else { "sell" })?,
                                ),
                                ("price", decimal(&q.price)?),
                                ("remaining", decimal(&q.remaining)?),
                                ("reserveAtoms", atoms(q.reserve_atoms)?),
                            ])
                        }))?,
                    ),
                ])
            }
            "contract.position" | "wallet.position" => {
                args.count(2)?;
                self.selected(args.u32(1)?)?;
                self.position(&args.owner(0)?)
            }
            "wallet.positions" => {
                args.count(1)?;
                let owner = args.owner(0)?;
                match s.positions().get(&owner) {
                    Some(_) => array([self.position(&owner)]),
                    None => array(std::iter::empty()),
                }
            }
            "spot.book" => {
                args.count(2)?;
                let pair = SpotPair::new(args.property(0)?, args.property(1)?)
                    .map_err(|_| ServiceError::InvalidRequest)?;
                array(s.joint().spot().open_orders(pair).iter().map(|r| {
                    object(vec![
                        ("txid", txid(r.txid)?),
                        ("owner", string(r.owner.as_str())?),
                        ("height", num(r.height)),
                        ("offeredPropertyId", num(r.offered.get())),
                        ("desiredPropertyId", num(r.desired.get())),
                        (
                            "remainingOfferedAtoms",
                            atoms(u128::from(r.remaining_offered))?,
                        ),
                        (
                            "remainingDesiredAtoms",
                            atoms(u128::from(r.remaining_desired))?,
                        ),
                    ])
                }))
            }
            "history.spot" => {
                args.count(2)?;
                let pair = SpotPair::new(args.property(0)?, args.property(1)?)
                    .map_err(|_| ServiceError::InvalidRequest)?;
                array(
                    s.joint()
                        .spot()
                        .fills()
                        .iter()
                        .filter(|f| f.pair == pair)
                        .map(|f| {
                            object(vec![
                                ("buyTxid", txid(f.buy_txid)?),
                                ("sellTxid", txid(f.sell_txid)?),
                                ("height", num(f.at_height)),
                                ("baseAtoms", atoms(u128::from(f.base_atoms))?),
                                ("quoteAtoms", atoms(u128::from(f.quote_atoms))?),
                            ])
                        }),
                )
            }
            "spot.last_price" | "spot.vwap" => {
                args.count(if call.target == "spot.last_price" {
                    3
                } else {
                    4
                })?;
                let base = args.property(0)?;
                let quote = args.property(1)?;
                let view = EconomicSpotPrices::from_verified_state(s);
                let value = if call.target == "spot.last_price" {
                    view.last_price(
                        base,
                        quote,
                        args.optional_height(2)?.unwrap_or(self.chain.tip().height),
                    )
                } else {
                    view.vwap(
                        base,
                        quote,
                        args.optional_height(2)?.unwrap_or(0),
                        args.optional_height(3)?.unwrap_or(self.chain.tip().height),
                    )
                }
                .map_err(spot_price_error)?;
                object(vec![
                    ("policy", string(PRICE_POLICY)?),
                    (
                        "network",
                        string(match s.config().network {
                            crate::network_profile::TradeLayerMainnet::Bitcoin => "bitcoin",
                            crate::network_profile::TradeLayerMainnet::Litecoin => "litecoin",
                            crate::network_profile::TradeLayerMainnet::Dogecoin => "dogecoin",
                        })?,
                    ),
                    ("authenticatedAdmission", RpcValue::boolean(false)),
                    ("ownerHeight", num(self.chain.tip().height)),
                    (
                        "ownerTip",
                        string(
                            &self
                                .chain
                                .tip()
                                .id
                                .0
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<String>(),
                        )?,
                    ),
                    (
                        "price",
                        value
                            .as_ref()
                            .map_or_else(|| Ok(RpcValue::null()), spot_price_row)?,
                    ),
                ])
            }
            "history.contract" => {
                args.count(1)?;
                self.selected(args.u32(0)?)?;
                self.contract_history(None)
            }
            "history.contract_for_address" => {
                args.count(2)?;
                self.selected(args.u32(0)?)?;
                self.contract_history(Some(&args.owner(1)?))
            }
            "history.address" => {
                args.count(1)?;
                if !s.joint().spot().fills().is_empty() {
                    return Err(ServiceError::Unavailable("spot_fill_owner_history"));
                }
                self.contract_history(Some(&args.owner(0)?))
            }
            "history.funding" => {
                args.count(3)?;
                self.selected(args.u32(0)?)?;
                if s.config().funding.is_none() {
                    return Err(ServiceError::Unavailable("funding_history"));
                }
                let first = args.optional_height(1)?.unwrap_or(0);
                let last = args.optional_height(2)?.unwrap_or(u64::MAX);
                if first > last {
                    return Err(ServiceError::InvalidRequest);
                }
                array(
                    s.funding_state()
                        .epochs
                        .iter()
                        .filter(|e| e.height >= first && e.height <= last)
                        .map(funding_row),
                )
            }
            "history.oracle" => {
                args.count(1)?;
                self.selected(args.u32(0)?)?;
                if s.config().funding.is_none() && s.config().marking.is_none() {
                    return Err(ServiceError::Unavailable("retained_oracle_observations"));
                }
                let mut observations = std::collections::BTreeMap::new();
                for o in s.funding_state().observations.iter().chain(
                    s.funding_state()
                        .epochs
                        .iter()
                        .flat_map(|e| e.window.iter()),
                ) {
                    observations.insert((o.height, o.txid), o);
                }
                for e in &s.mark_state().events {
                    if let MarkEventKind::Mark { observation, .. } = &e.kind {
                        observations.insert((observation.height, observation.txid), observation);
                    }
                }
                array(observations.values().map(|o| {
                    object(vec![
                        ("txid", txid(o.txid)?),
                        ("height", num(o.height)),
                        ("price", decimal(&o.price)?),
                    ])
                }))
            }
            "history.clearing" => {
                args.count(1)?;
                self.selected(args.u32(0)?)?;
                self.clearing(None)
            }
            "interface.audit_data" => {
                args.count(2)?;
                self.selected(args.u32(1)?)?;
                self.clearing(Some(args.height(0)?))
            }
            "fees.all" | "fees.property" => {
                let filter = if call.target == "fees.property" {
                    args.count(1)?;
                    Some(args.property(0)?)
                } else {
                    args.count(0)?;
                    None
                };
                array(
                    s.fees()
                        .rows()
                        .filter(|(k, _)| filter.is_none_or(|p| p == k.property))
                        .map(|(k, r)| {
                            object(vec![
                                ("propertyId", num(k.property.get())),
                                ("contractId", num(k.contract)),
                                ("valueAtoms", atoms(r.value_atoms)?),
                                ("stashAtoms", atoms(r.stash_atoms)?),
                            ])
                        }),
                )
            }
            "iou.claims" => {
                args.count(0)?;
                array(
                    s.ious()
                        .claim_maps()
                        .flat_map(|m| m.claims().iter().map(move |c| (m, c)))
                        .map(|(m, c)| {
                            object(vec![
                                ("contractId", num(m.key.contract_id)),
                                ("propertyId", num(m.key.property_id)),
                                ("address", string(c.address.as_str())?),
                                ("claim", decimal(&c.amount)?),
                                ("lastBlock", num(m.last_block)),
                            ])
                        }),
                )
            }
            "iou.buckets" => {
                args.count(0)?;
                array(s.ious().buckets().map(|b| {
                    object(vec![
                        ("contractId", num(b.key.contract_id)),
                        ("propertyId", num(b.key.property_id)),
                        ("amount", decimal(&b.amount)?),
                        ("lastBlock", num(b.last_block)),
                        ("blockLosses", decimal(&b.block_losses)?),
                        ("blockProfits", decimal(&b.block_profits)?),
                    ])
                }))
            }
            "interface.insurance_balance" => {
                args.count(1)?;
                self.selected(args.u32(0)?)?;
                object(vec![
                    ("custodyAtoms", atoms(s.insurance_custody_atoms())?),
                    ("scope", string("economic_insurance_custody")?),
                ])
            }
            "history.spot_for_address" => Err(ServiceError::Unavailable("spot_fill_owner_history")),
            "transactions.get"
            | "transactions.address"
            | "transactions.block"
            | "interface.block_range" => {
                Err(ServiceError::Unavailable("authenticated_transaction_index"))
            }
            "runtime.initialize" | "runtime.pause" => {
                Err(ServiceError::Unavailable("runtime_control"))
            }
            "node.validate_address" | "wallet.load" | "wallet.add_op_return" => {
                Err(ServiceError::Unavailable("wallet_rpc"))
            }
            "channel.get" => {
                use crate::ledger::ChannelSide;
                args.count(1)?;
                let channel = args.owner(0)?;
                let owner_a = s.ledger().channel_owner(&channel, ChannelSide::A);
                let owner_b = s.ledger().channel_owner(&channel, ChannelSide::B);
                if owner_a.is_none() && owner_b.is_none() {
                    return Err(ServiceError::NotFound);
                }
                object(vec![
                    ("scope", string("ledger_channel")?),
                    ("channel", string(channel.as_str())?),
                    (
                        "ownerA",
                        owner_a.map_or_else(|| Ok(RpcValue::null()), |a| string(a.as_str()))?,
                    ),
                    (
                        "ownerB",
                        owner_b.map_or_else(|| Ok(RpcValue::null()), |a| string(a.as_str()))?,
                    ),
                    (
                        "balances",
                        array(
                            s.ledger()
                                .channel_balance_rows()
                                .filter(|(_, _, c, _, _)| *c == &channel)
                                .map(|(p, a, _, side, n)| {
                                    object(vec![
                                        ("propertyId", num(p.get())),
                                        ("owner", string(a.as_str())?),
                                        (
                                            "side",
                                            string(if side == ChannelSide::A { "A" } else { "B" })?,
                                        ),
                                        ("atoms", atoms(n)?),
                                    ])
                                }),
                        )?,
                    ),
                ])
            }
            "channel.balance_for_committer" => {
                args.count(2)?;
                let owner = args.owner(0)?;
                let id = args.property(1)?;
                if private(&owner) {
                    return Err(ServiceError::Unavailable("private_custody_account"));
                }
                if s.ledger().property(id).is_none() {
                    return Err(ServiceError::NotFound);
                }
                let amount = checked_sum(
                    s.ledger()
                        .channel_balance_rows()
                        .filter(|(p, a, _, _, _)| *p == id && *a == &owner)
                        .map(|(_, _, _, _, n)| n),
                )?;
                object(vec![
                    ("scope", string("ledger_channel")?),
                    ("channelAtoms", atoms(amount)?),
                ])
            }
            "channel.predict_column" => Err(ServiceError::Unavailable("channel_lifecycle_view")),
            "issuance.intents" | "issuance.prepare_commit" => {
                Err(ServiceError::Unavailable("issuance_intent_store"))
            }
            "procedural.bitvm_status" => Err(ServiceError::Unavailable("bitvm_capability")),
            "synthetic.max" | "interface.vaults" => {
                Err(ServiceError::Unavailable("synthetic_vault_store"))
            }
            "interface.options_chain" => Err(ServiceError::Unavailable("options_chain_store")),
            "interface.consensus_hash" => {
                Err(ServiceError::Unavailable("historical_consensus_hash"))
            }
            "interface.pending" => Err(ServiceError::Unavailable("mempool_index")),
            "interface.grants" | "interface.pay_to_token" => {
                Err(ServiceError::Unavailable("property_lifecycle_history"))
            }
            "interface.insurance_payouts" => {
                Err(ServiceError::Unavailable("insurance_payout_history"))
            }
            "interface.update_wallet_cache" => {
                Err(ServiceError::Unavailable("wallet_cache_mutation"))
            }
            _ => Err(ServiceError::NotFound),
        }
    }
}

/// Typed useful calls for every query in the source facade, including methods
/// whose original dependencies are undeclared. Generic backends can supply
/// those capabilities without changing the immutable economic adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceQuery {
    AuditTally,
    ConsensusHash(u64),
    FeatureStatus(u32),
    Balances(AccountId),
    TotalTokens(PropertyId),
    AllBalances,
    IsActive(u32),
    ActiveTypes,
    Addresses(PropertyId),
    Transaction(String),
    Property(PropertyId),
    Properties,
    Grants(PropertyId),
    PayToToken(PropertyId),
    BlockTransactions(u64),
    BlockRange(u64, u64),
    Pending(String),
    UpdateWalletCache,
    Oracles,
    Whitelists,
    Vaults(PropertyId),
    OptionsChain(u32),
    InsuranceBalance(u32),
    InsurancePayouts {
        contract: u32,
        start: u64,
        end: Option<u64>,
    },
    AuditData {
        height: u64,
        contract: u32,
    },
}
impl InterfaceQuery {
    pub fn service_call(self) -> Result<ServiceCall> {
        use InterfaceQuery::*;
        let (target, arguments) = match self {
            AuditTally => ("ledger.audit", vec![]),
            ConsensusHash(h) => ("interface.consensus_hash", vec![num(h)]),
            FeatureStatus(i) => ("registry.feature_status", vec![num(i)]),
            Balances(a) => ("ledger.balances", vec![string(a.as_str())?]),
            TotalTokens(p) => ("ledger.total_tokens", vec![num(p.get())]),
            AllBalances => ("ledger.all_balances", vec![]),
            IsActive(i) => ("registry.is_active", vec![num(i)]),
            ActiveTypes => ("registry.activations", vec![]),
            Addresses(p) => ("ledger.addresses_with_property", vec![num(p.get())]),
            Transaction(id) => ("transactions.get", vec![string(&id)?]),
            Property(p) => ("ledger.property", vec![num(p.get())]),
            Properties => ("ledger.properties", vec![]),
            Grants(p) => ("interface.grants", vec![num(p.get())]),
            PayToToken(p) => ("interface.pay_to_token", vec![num(p.get())]),
            BlockTransactions(h) => ("transactions.block", vec![num(h)]),
            BlockRange(a, b) => {
                if a > b {
                    return Err(ServiceError::InvalidRequest);
                }
                ("interface.block_range", vec![num(a), num(b)])
            }
            Pending(a) => ("interface.pending", vec![string(&a)?]),
            UpdateWalletCache => ("interface.update_wallet_cache", vec![]),
            Oracles => ("registry.oracles", vec![]),
            Whitelists => ("registry.clear_lists", vec![]),
            Vaults(p) => ("interface.vaults", vec![num(p.get())]),
            OptionsChain(i) => ("interface.options_chain", vec![num(i)]),
            InsuranceBalance(i) => ("interface.insurance_balance", vec![num(i)]),
            InsurancePayouts {
                contract,
                start,
                end,
            } => {
                if end.is_some_and(|e| e < start) {
                    return Err(ServiceError::InvalidRequest);
                }
                (
                    "interface.insurance_payouts",
                    vec![num(contract), num(start), opt_num(end)],
                )
            }
            AuditData { height, contract } => {
                ("interface.audit_data", vec![num(height), num(contract)])
            }
        };
        Ok(ServiceCall { target, arguments })
    }
}
pub struct InterfaceFacade<'a, B: ServiceBackend> {
    backend: &'a mut B,
}
impl<'a, B: ServiceBackend> InterfaceFacade<'a, B> {
    pub fn new(backend: &'a mut B) -> Self {
        Self { backend }
    }
    pub fn query(&mut self, query: InterfaceQuery) -> Result<RpcValue> {
        self.backend.call(query.service_call()?)
    }
    pub fn tally_audit_bytes(&mut self) -> Result<Vec<u8>> {
        let rows = self
            .query(InterfaceQuery::AuditTally)?
            .as_array()
            .map_err(|_| ServiceError::Backend("audit_rows"))?;
        audit_lines(&rows)
    }
}

/// Four-space JSON plus LF framing. LosslessJsonAuditV1 preserves exact JSON
/// number lexemes and key order; it does not coerce them through JS Number or
/// reorder integer-like keys. The JSON-safe source corpus agrees byte for byte.
/// Undefined, cyclic, bigint and nonfinite JS inputs are source observations.
pub fn audit_lines(rows: &[RpcValue]) -> Result<Vec<u8>> {
    if rows.len() > MAX_AUDIT_ROWS {
        return Err(ServiceError::Capacity);
    }
    let mut out = String::new();
    for row in rows {
        let pretty = pretty_json(row.as_json())?;
        if out
            .len()
            .checked_add(pretty.len() + 1)
            .is_none_or(|n| n > MAX_SERVICE_RESPONSE)
        {
            return Err(ServiceError::Capacity);
        }
        out.push_str(&pretty);
        out.push('\n');
    }
    Ok(out.into_bytes())
}
fn pretty_json(text: &str) -> Result<String> {
    let mut compact = Vec::with_capacity(text.len());
    let mut in_string = false;
    let mut was_escape = false;
    for b in text.bytes() {
        if in_string || !b.is_ascii_whitespace() {
            compact.push(b);
        }
        if in_string {
            if was_escape {
                was_escape = false;
            } else if b == b'\\' {
                was_escape = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
        }
    }
    let bytes = compact.as_slice();
    let mut out = String::new();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if quoted {
            out.push(char::from(b));
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
            continue;
        }
        match b {
            b'"' => {
                quoted = true;
                out.push('"');
            }
            b'{' | b'[' => {
                out.push(char::from(b));
                if bytes.get(i + 1).is_some_and(|v| *v != b'}' && *v != b']') {
                    depth += 1;
                    out.push('\n');
                    out.push_str(&" ".repeat(depth * 4));
                }
            }
            b'}' | b']' => {
                if i > 0 && bytes[i - 1] != b'{' && bytes[i - 1] != b'[' {
                    depth = depth.checked_sub(1).ok_or(ServiceError::InvalidRequest)?;
                    out.push('\n');
                    out.push_str(&" ".repeat(depth * 4));
                }
                out.push(char::from(b));
            }
            b',' => {
                out.push_str(",\n");
                out.push_str(&" ".repeat(depth * 4));
            }
            b':' => out.push_str(": "),
            b if b.is_ascii_whitespace() => {}
            _ => out.push(char::from(b)),
        }
        if out.len() > MAX_SERVICE_RESPONSE {
            return Err(ServiceError::Capacity);
        }
    }
    // UTF-8 is copied as bytes above; restore it without changing multibyte data.
    let original: Vec<u8> = out.chars().map(|c| c as u8).collect();
    String::from_utf8(original).map_err(|_| ServiceError::InvalidRequest)
}
struct Args<'a>(&'a [RpcValue]);
fn spot_price_error(error: SpotPriceError) -> ServiceError {
    match error {
        SpotPriceError::UnknownProperty => ServiceError::NotFound,
        SpotPriceError::SameAsset
        | SpotPriceError::InvalidRange
        | SpotPriceError::BeyondFinalizedHeight => ServiceError::InvalidRequest,
        _ => ServiceError::Backend("executed_spot_price"),
    }
}
fn spot_price_row(row: &ExecutedSpotPrice) -> Result<RpcValue> {
    object(vec![
        ("basePropertyId", num(row.base.get())),
        ("quotePropertyId", num(row.quote.get())),
        ("fromHeight", num(row.from_height)),
        ("throughHeight", num(row.through_height)),
        (
            "firstFillIndex",
            num(u64::try_from(row.first_fill_index).map_err(|_| ServiceError::Capacity)?),
        ),
        (
            "lastFillIndex",
            num(u64::try_from(row.last_fill_index).map_err(|_| ServiceError::Capacity)?),
        ),
        (
            "sampleCount",
            num(u64::try_from(row.sample_count).map_err(|_| ServiceError::Capacity)?),
        ),
        ("baseAtoms", atoms(row.base_atoms)?),
        ("quoteAtoms", atoms(row.quote_atoms)?),
        (
            "quotePerBase",
            decimal(&row.price(20, Rounding::HalfUp).map_err(spot_price_error)?)?,
        ),
    ])
}
impl Args<'_> {
    fn count(&self, n: usize) -> Result<()> {
        if self.0.len() == n {
            Ok(())
        } else {
            Err(ServiceError::InvalidRequest)
        }
    }
    fn value(&self, n: usize) -> Result<&RpcValue> {
        self.0.get(n).ok_or(ServiceError::InvalidRequest)
    }
    fn height(&self, n: usize) -> Result<u64> {
        self.value(n)?
            .as_u64()
            .map_err(|_| ServiceError::InvalidRequest)
    }
    fn u32(&self, n: usize) -> Result<u32> {
        u32::try_from(self.height(n)?).map_err(|_| ServiceError::InvalidRequest)
    }
    fn owner(&self, n: usize) -> Result<AccountId> {
        self.value(n)?
            .as_string()
            .map_err(|_| ServiceError::InvalidRequest)?
            .as_str()
            .try_into()
            .map_err(|_| ServiceError::InvalidRequest)
    }
    fn property(&self, n: usize) -> Result<PropertyId> {
        self.u32(n)?
            .try_into()
            .map_err(|_| ServiceError::InvalidRequest)
    }
    fn contract(&self, n: usize) -> Result<ContractId> {
        self.u32(n)?
            .try_into()
            .map_err(|_| ServiceError::InvalidRequest)
    }
    fn optional_height(&self, n: usize) -> Result<Option<u64>> {
        if self.value(n)?.as_json() == "null" {
            Ok(None)
        } else {
            self.height(n).map(Some)
        }
    }
    fn decimal(&self, n: usize) -> Result<Decimal> {
        let text = self
            .value(n)?
            .as_string()
            .map_err(|_| ServiceError::InvalidRequest)?;
        if text.len() > 64 {
            return Err(ServiceError::Capacity);
        }
        text.parse().map_err(|_| ServiceError::InvalidRequest)
    }
}
fn private(a: &AccountId) -> bool {
    a.as_str().starts_with('~')
}
fn num(n: impl Into<u64>) -> RpcValue {
    RpcValue::unsigned(n.into())
}
fn checked_sum(values: impl IntoIterator<Item = u128>) -> Result<u128> {
    values.into_iter().try_fold(0_u128, |sum, v| {
        sum.checked_add(v).ok_or(ServiceError::Capacity)
    })
}
fn opt_num(n: Option<u64>) -> RpcValue {
    n.map_or_else(RpcValue::null, num)
}
fn string(s: &str) -> Result<RpcValue> {
    RpcValue::string(s).map_err(|_| ServiceError::Capacity)
}
fn decimal(d: &Decimal) -> Result<RpcValue> {
    string(&d.to_string())
}
fn ratio(r: &crate::mark_basis::ExactRatio) -> Result<RpcValue> {
    object(vec![
        ("numerator", decimal(r.numerator())?),
        ("denominator", decimal(r.denominator())?),
    ])
}
fn option_decimal(d: Option<&Decimal>) -> Result<RpcValue> {
    d.map_or_else(|| Ok(RpcValue::null()), decimal)
}
fn atoms(n: u128) -> Result<RpcValue> {
    string(&n.to_string())
}
fn opt_atoms(n: Option<u128>) -> Result<RpcValue> {
    n.map_or_else(|| Ok(RpcValue::null()), atoms)
}
fn dec_atoms(n: u128) -> Result<RpcValue> {
    decimal(
        &Decimal::from(n)
            .checked_div(&Decimal::from(100_000_000_u32), 8, Rounding::Down)
            .map_err(|_| ServiceError::Backend("amount"))?,
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn txid(id: TxId) -> Result<RpcValue> {
    string(&hex(&id.as_bytes()))
}
fn parse(s: &str) -> Result<RpcValue> {
    RpcValue::parse(s).map_err(|_| ServiceError::Capacity)
}
fn array(items: impl IntoIterator<Item = Result<RpcValue>>) -> Result<RpcValue> {
    let mut out = String::from("[");
    for item in items {
        let item = item?;
        if out
            .len()
            .checked_add(item.as_json().len() + 2)
            .is_none_or(|n| n > MAX_SERVICE_RESPONSE)
        {
            return Err(ServiceError::Capacity);
        }
        if out.len() > 1 {
            out.push(',');
        }
        out.push_str(item.as_json());
    }
    out.push(']');
    parse(&out)
}
fn object(fields: Vec<(&str, RpcValue)>) -> Result<RpcValue> {
    let mut out = String::from("{");
    for (k, v) in fields {
        if out.len() > 1 {
            out.push(',');
        }
        out.push_str(string(k)?.as_json());
        out.push(':');
        out.push_str(v.as_json());
        if out.len() > MAX_SERVICE_RESPONSE {
            return Err(ServiceError::Capacity);
        }
    }
    out.push('}');
    parse(&out)
}
fn list_row(id: u32, w: &crate::registry::Whitelist) -> Result<RpcValue> {
    object(vec![
        ("id", num(id)),
        ("admin", string(w.admin.as_str())?),
        ("backup", string(w.backup.as_str())?),
        ("name", string(&w.name)?),
        ("url", string(&w.url)?),
        ("description", string(&w.description)?),
    ])
}
fn contract_row(c: &ContractSeries) -> Result<RpcValue> {
    let p = &c.parameters;
    object(vec![
        ("id", num(c.id.get())),
        ("issuer", string(c.issuer.as_str())?),
        ("ticker", string(&c.ticker)?),
        ("native", RpcValue::boolean(p.native)),
        (
            "oracleId",
            p.underlying_oracle_id.map_or_else(RpcValue::null, num),
        ),
        ("inverse", RpcValue::boolean(p.inverse)),
        ("notionalValue", decimal(&p.notional_value)?),
        ("notionalPropertyId", num(p.notional_property_id.get())),
        ("collateralPropertyId", num(p.collateral_property_id.get())),
        ("leverage", decimal(&p.leverage)?),
        ("expiryPeriod", num(p.expiry_period)),
        ("series", num(p.series)),
        ("fee", decimal(&p.fee)?),
        ("whitelist", p.whitelist.map_or_else(RpcValue::null, num)),
        (
            "onChainData",
            array(
                p.on_chain_data
                    .iter()
                    .map(|(a, b)| array([Ok(num(a.get())), Ok(num(b.get()))])),
            )?,
        ),
        (
            "ammConfiguration",
            object(vec![
                ("position", decimal(&c.amm.position)?),
                ("maxPosition", decimal(&c.amm.max_position)?),
                ("maxQuoteSize", decimal(&c.amm.max_quote_size)?),
                ("contractId", num(c.amm.contract_type.get())),
            ])?,
        ),
        (
            "unexpired",
            array(c.unexpired.iter().map(|e| {
                object(vec![
                    ("id", string(&e.id)?),
                    ("expirationBlock", num(e.expiration_block)),
                ])
            }))?,
        ),
        (
            "expired",
            array(c.expired.iter().map(|e| {
                object(vec![
                    ("id", string(&e.id)?),
                    ("expirationBlock", num(e.expiration_block)),
                ])
            }))?,
        ),
    ])
}
fn order_row(o: &Order) -> Result<RpcValue> {
    object(vec![
        ("txid", txid(o.txid)?),
        ("owner", string(o.owner.as_str())?),
        ("contractId", num(o.contract.get())),
        ("height", num(o.height)),
        (
            "side",
            string(match o.side {
                Side::Buy => "buy",
                Side::Sell => "sell",
            })?,
        ),
        ("quantity", decimal(&o.quantity)?),
        ("price", decimal(&o.price)?),
        ("post", RpcValue::boolean(o.post)),
        ("market", RpcValue::boolean(o.is_market)),
    ])
}
fn funding_row(e: &FundingEpoch) -> Result<RpcValue> {
    let plan = if let Some(p) = &e.plan {
        object(vec![
            ("rateBps", decimal(&p.rate_bps)?),
            ("collateralNotional", decimal(&p.collateral_notional)?),
            ("totalAtoms", atoms(p.total_atoms)?),
            (
                "debits",
                array(p.debits.iter().map(|v| {
                    object(vec![
                        ("owner", string(v.owner.as_str())?),
                        ("atoms", atoms(v.atoms)?),
                    ])
                }))?,
            ),
            (
                "credits",
                array(p.credits.iter().map(|v| {
                    object(vec![
                        ("owner", string(v.owner.as_str())?),
                        ("atoms", atoms(v.atoms)?),
                    ])
                }))?,
            ),
        ])?
    } else {
        RpcValue::null()
    };
    object(vec![
        ("height", num(e.height)),
        ("id", txid(e.id)?),
        ("status", string(&format!("{:?}", e.status))?),
        ("plan", plan),
        (
            "window",
            array(e.window.iter().map(|o| {
                object(vec![
                    ("txid", txid(o.txid)?),
                    ("height", num(o.height)),
                    ("price", decimal(&o.price)?),
                ])
            }))?,
        ),
        (
            "positions",
            array(e.positions.iter().map(|p| {
                object(vec![
                    ("owner", string(p.owner.as_str())?),
                    ("contracts", decimal(&p.contracts)?),
                    ("availableBeforeAtoms", atoms(p.available_atoms)?),
                ])
            }))?,
        ),
        (
            "index",
            e.index.as_ref().map_or_else(
                || Ok(RpcValue::null()),
                |o| {
                    object(vec![
                        ("height", num(o.height)),
                        ("txid", txid(o.txid)?),
                        ("price", decimal(&o.price)?),
                    ])
                },
            )?,
        ),
    ])
}
fn mark_event(e: &MarkEvent) -> Result<RpcValue> {
    let kind = match &e.kind {
        MarkEventKind::Trade {
            buy_txid,
            sell_txid,
        } => object(vec![
            ("kind", string("trade")?),
            ("buyTxid", txid(*buy_txid)?),
            ("sellTxid", txid(*sell_txid)?),
        ])?,
        MarkEventKind::Mark { id, observation } => object(vec![
            ("kind", string("mark")?),
            ("id", txid(*id)?),
            ("observationTxid", txid(observation.txid)?),
            ("price", decimal(&observation.price)?),
        ])?,
        MarkEventKind::Forced {
            epoch_id,
            fill_index,
            maker_txid,
            distressed_owner,
        } => object(vec![
            ("kind", string("forced_book")?),
            ("epoch", txid(*epoch_id)?),
            ("fillIndex", num(*fill_index)),
            ("makerTxid", txid(*maker_txid)?),
            ("owner", string(distressed_owner.as_str())?),
        ])?,
        MarkEventKind::AmmForced {
            epoch_id,
            fill_index,
            pool_id,
            distressed_owner,
        } => object(vec![
            ("kind", string("forced_amm")?),
            ("epoch", txid(*epoch_id)?),
            ("fillIndex", num(*fill_index)),
            ("poolId", num(*pool_id)),
            ("owner", string(distressed_owner.as_str())?),
        ])?,
    };
    object(vec![
        ("height", num(e.height)),
        ("event", kind),
        (
            "entries",
            array(e.entries.iter().map(|v| {
                let (kind, price, delta) = match &v.action {
                    BasisAction::Trade { delta, price } => ("trade", price, Some(delta)),
                    BasisAction::Mark { price } => ("mark", price, None),
                };
                object(vec![
                    ("owner", string(v.owner.as_str())?),
                    ("action", string(kind)?),
                    ("price", decimal(price)?),
                    ("delta", option_decimal(delta)?),
                    ("bookedPnl", decimal(&v.atoms.to_decimal())?),
                    ("availableBeforeAtoms", atoms(v.available_atoms)?),
                    ("marginBeforeAtoms", atoms(v.margin_atoms)?),
                    ("collectedAtoms", atoms(v.collected_atoms)?),
                    ("claimOffsetAtoms", atoms(v.claim_offset_atoms)?),
                    ("lossOffsetAtoms", atoms(v.loss_offset_atoms)?),
                ])
            }))?,
        ),
        ("collectedLossAtoms", atoms(e.collected_loss_atoms)?),
        ("paidClaimAtoms", atoms(e.paid_claim_atoms)?),
        ("unfundedLossAtoms", atoms(e.unfunded_loss_atoms)?),
    ])
}
