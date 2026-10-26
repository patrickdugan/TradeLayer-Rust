//! Stateful, caller-driven margin positions. See `docs/MARGIN_MAP.md` for
//! source defects, finite-number boundaries, and explicitly repaired paths.

use crate::contract_registry::ContractId;
use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::ledger::AccountId;
use crate::options::{parse_ticker, OptionType};
use core::{fmt, str::FromStr};
use std::collections::BTreeMap;

pub const MAX_MARGIN_POSITIONS: usize = 65_536;
pub const MAX_POSITION_OPTIONS: usize = 4096;
pub const MAX_MARGIN_DELTAS: usize = 262_144;
pub const SOURCE_LF_SHA256: &str =
    "ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginError {
    Arithmetic,
    MissingPosition,
    InvalidInput,
    LimitExceeded,
    NonFinite,
    UnsupportedLegacyPath,
}
impl From<DecimalError> for MarginError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
impl fmt::Display for MarginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MarginError {}
type Result<T> = core::result::Result<T, MarginError>;

/// Every stored scalar is finite. Source `.toNumber()` boundaries remain
/// binary64 boundaries; callers must not interpret AccountId as authorization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionPosition {
    pub contracts: Decimal,
    pub avg_price: Decimal,
    pub margin: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub address: AccountId,
    pub contracts: Decimal,
    pub margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub avg_price: Option<Decimal>,
    pub liquidation_price: Option<Decimal>,
    pub bankruptcy_price: Option<Decimal>,
    pub iou_claim: Decimal,
    pub available: Decimal,
    pub last_mark: Option<Decimal>,
    pub old_mark: Option<Decimal>,
    pub new_position_this_block: Decimal,
    pub options: BTreeMap<String, OptionPosition>,
}
impl Position {
    pub fn new(address: AccountId) -> Self {
        Self {
            address,
            contracts: Decimal::zero(),
            margin: Decimal::zero(),
            unrealized_pnl: Decimal::zero(),
            realized_pnl: Decimal::zero(),
            avg_price: Some(Decimal::zero()),
            liquidation_price: None,
            bankruptcy_price: None,
            iou_claim: Decimal::zero(),
            available: Decimal::zero(),
            last_mark: None,
            old_mark: None,
            new_position_this_block: Decimal::zero(),
            options: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarginDelta {
    pub address: AccountId,
    pub contract: ContractId,
    pub option_ticker: Option<String>,
    pub total_position: Decimal,
    pub position: Decimal,
    pub margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub average_entry: Decimal,
    pub mode: &'static str,
    pub block: Option<u64>,
    pub last_price: Option<Decimal>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationPrices {
    pub bankruptcy_price: Option<Decimal>,
    pub liquidation_price: Option<Decimal>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractContext {
    pub inverse: bool,
    pub notional: Decimal,
    pub available: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionTrade {
    pub address: AccountId,
    pub amount: Decimal,
    pub price: Decimal,
    pub sell: bool,
    /// The legacy caller computes close/flip quantities; they affect only the
    /// average-price branch, not the signed contract change.
    pub close: Decimal,
    pub flip: Decimal,
    pub in_clearing: bool,
    pub initial: bool,
    pub block: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionTradeResult {
    pub position: OptionPosition,
    pub closed_quantity: Decimal,
    pub flip_quantity: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationOrder {
    pub address: AccountId,
    pub contract: ContractId,
    pub amount: Decimal,
    pub price: Decimal,
    pub sell: bool,
    pub block: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleverageMatch {
    pub address: AccountId,
    pub matched: Decimal,
    pub before: Decimal,
    pub after: Decimal,
    pub collateral_return: Decimal,
    pub pool_share: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleverageResult {
    pub attempted: Decimal,
    pub total_deleveraged: Decimal,
    pub counterparties: Vec<DeleverageMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintMove {
    pub vault_address: AccountId,
    pub contracts: Decimal,
    pub margin: Decimal,
    pub excess: Decimal,
}

/// Insertion order is preserved separately: source Map iteration and stable
/// ADL ties must not silently become lexicographic account order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarginMap {
    contract: ContractId,
    positions: BTreeMap<AccountId, Position>,
    order: Vec<AccountId>,
    deltas: Vec<MarginDelta>,
    expiry_index: Vec<(u64, Vec<String>)>,
    ticker_expiry: BTreeMap<String, u64>,
    option_oi: BTreeMap<String, u32>,
}
#[allow(clippy::too_many_arguments)]
impl MarginMap {
    pub fn new(contract: ContractId) -> Self {
        Self {
            contract,
            positions: BTreeMap::new(),
            order: Vec::new(),
            deltas: Vec::new(),
            expiry_index: Vec::new(),
            ticker_expiry: BTreeMap::new(),
            option_oi: BTreeMap::new(),
        }
    }
    pub const fn contract(&self) -> ContractId {
        self.contract
    }
    pub fn deltas(&self) -> &[MarginDelta] {
        &self.deltas
    }
    pub fn take_deltas(&mut self) -> Vec<MarginDelta> {
        core::mem::take(&mut self.deltas)
    }
    pub fn positions(&self) -> impl Iterator<Item = &Position> {
        self.order.iter().filter_map(|a| self.positions.get(a))
    }
    pub fn get(&self, address: &AccountId) -> Option<&Position> {
        self.positions.get(address)
    }
    /// Source exact match, then lowercase BTC/LTC Bech32-prefix fallback.
    /// Uppercase query prefixes do not activate the fallback.
    pub fn get_position_for_address(&self, address: &AccountId) -> Position {
        if let Some(p) = self.get(address) {
            return p.clone();
        }
        let a = address.as_str();
        if ["ltc1", "tltc1", "bc1", "tb1"]
            .iter()
            .any(|p| a.starts_with(p))
        {
            if let Some(p) = self
                .positions()
                .find(|p| p.address.as_str().eq_ignore_ascii_case(a))
            {
                return p.clone();
            }
        }
        Position::new(address.clone())
    }
    pub fn write_position(&mut self, position: Position) -> Result<()> {
        validate_position(&position)?;
        if !self.positions.contains_key(&position.address) {
            if self.order.len() >= MAX_MARGIN_POSITIONS {
                return Err(MarginError::LimitExceeded);
            }
            self.order.push(position.address.clone());
        }
        self.positions.insert(position.address.clone(), position);
        Ok(())
    }
    /// Local whole-map replacement. The source flatten flag drops even funded
    /// flat positions; this quirk is preserved. Options do not prevent removal.
    pub fn merge_positions(&mut self, positions: &[Position], flatten_zeroes: bool) -> Result<()> {
        let mut next = self.clone();
        next.positions.clear();
        next.order.clear();
        for p in positions {
            if flatten_zeroes && p.contracts.is_zero() && p.iou_claim.is_zero() {
                continue;
            }
            next.write_position(p.clone())?;
        }
        *self = next;
        Ok(())
    }
    pub fn cleanup_flat_positions(&mut self) -> usize {
        let before = self.positions.len();
        self.positions.retain(|_, p| {
            !(p.contracts.is_zero()
                && p.margin.is_zero()
                && p.unrealized_pnl.is_zero()
                && p.iou_claim.is_zero())
        });
        self.order.retain(|a| self.positions.contains_key(a));
        before - self.positions.len()
    }
    fn record(
        &mut self,
        p: &Position,
        mode: &'static str,
        position: Decimal,
        margin: Decimal,
        pnl: Decimal,
        average: Decimal,
        block: Option<u64>,
    ) -> Result<()> {
        if block.is_some_and(|b| b > 9_007_199_254_740_991) {
            return Err(MarginError::InvalidInput);
        }
        if self.deltas.len() >= MAX_MARGIN_DELTAS {
            return Err(MarginError::LimitExceeded);
        }
        self.deltas.push(MarginDelta {
            address: p.address.clone(),
            contract: self.contract,
            option_ticker: None,
            total_position: p.contracts.clone(),
            position,
            margin,
            unrealized_pnl: pnl,
            average_entry: average,
            mode,
            block,
            last_price: None,
        });
        Ok(())
    }
    fn transaction<T>(&mut self, op: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let mut next = self.clone();
        let out = op(&mut next)?;
        *self = next;
        Ok(out)
    }
    pub fn set_initial_margin(
        &mut self,
        address: &AccountId,
        amount: &Decimal,
        block: u64,
    ) -> Result<Position> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .unwrap_or_else(|| Position::new(address.clone()));
            p.margin = number8(&p.margin.checked_add(amount)?)?;
            m.record(
                &p,
                "initialMargin",
                Decimal::zero(),
                amount.clone(),
                Decimal::zero(),
                p.avg_price.clone().unwrap_or_default(),
                Some(block),
            )?;
            m.write_position(p.clone())?;
            Ok(p)
        })
    }
    /// Source adds newMargin; it does not replace the margin balance.
    /// Missing position is initialized with its address (source omits it).
    pub fn update_margin(
        &mut self,
        address: &AccountId,
        delta: &Decimal,
        block: u64,
    ) -> Result<Position> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .unwrap_or_else(|| Position::new(address.clone()));
            p.margin = number8(&p.margin.checked_add(delta)?)?;
            m.record(
                &p,
                "updateMargin",
                Decimal::zero(),
                p.margin.clone(),
                Decimal::zero(),
                Decimal::zero(),
                Some(block),
            )?;
            m.write_position(p.clone())?;
            Ok(p)
        })
    }
    pub fn update_contract_balances(
        &mut self,
        trade: &PositionTrade,
        context: &ContractContext,
    ) -> Result<Position> {
        self.transaction(|m| m.apply_trade(trade, context))
    }
    fn apply_trade(&mut self, t: &PositionTrade, c: &ContractContext) -> Result<Position> {
        if t.amount.is_negative() || c.notional <= Decimal::zero() || t.price <= Decimal::zero() {
            return Err(MarginError::InvalidInput);
        }
        let mut p = self
            .get(&t.address)
            .cloned()
            .ok_or(MarginError::MissingPosition)?;
        let old = p.contracts.clone();
        if t.flip > Decimal::zero() {
            p.avg_price = Some(t.price.clone());
        } else if t.close.is_zero() {
            if old.is_zero() {
                p.avg_price = Some(t.price.clone());
            } else if old.is_negative() == t.sell {
                p.avg_price = update_average_price(&p, &t.amount, &t.price, t.sell)?;
            }
        }
        p.contracts = number(&if t.sell {
            old.checked_sub(&t.amount)?
        } else {
            old.checked_add(&t.amount)?
        })?;
        if p.contracts.is_zero() {
            p.liquidation_price = Some(Decimal::zero());
            p.bankruptcy_price = Some(Decimal::zero());
            p.avg_price = None;
        } else {
            let prices = calculate_liquidation_price(
                &c.available,
                &p.margin,
                &p.contracts,
                &c.notional,
                c.inverse,
                !p.contracts.is_negative(),
                p.avg_price.as_ref().ok_or(MarginError::InvalidInput)?,
                &Decimal::zero(),
            )?;
            p.liquidation_price = prices.liquidation_price.filter(|v| !v.is_zero());
            p.bankruptcy_price = prices.bankruptcy_price;
        }
        let tag = if t.in_clearing {
            if t.initial {
                "initialLiq"
            } else {
                "liquidatingContract"
            }
        } else {
            "updateContractBalances"
        };
        self.record(
            &p,
            tag,
            t.amount.clone(),
            Decimal::zero(),
            Decimal::zero(),
            Decimal::zero(),
            Some(t.block),
        )?;
        self.deltas.last_mut().expect("just appended").last_price = p.bankruptcy_price.clone();
        self.write_position(p.clone())?;
        Ok(p)
    }
    pub fn update_contract_balances_with_match(
        &mut self,
        buyer: &PositionTrade,
        buyer_context: &ContractContext,
        seller: &PositionTrade,
        seller_context: &ContractContext,
    ) -> Result<(Position, Position)> {
        self.transaction(|m| {
            Ok((
                m.apply_trade(buyer, buyer_context)?,
                m.apply_trade(seller, seller_context)?,
            ))
        })
    }
    pub fn reduce_margin(
        &mut self,
        address: &AccountId,
        initial_per_contract: &Decimal,
        fee_debit: bool,
        fee: &Decimal,
        block: u64,
    ) -> Result<Decimal> {
        self.transaction(|m| {
            m.reduce_margin_inner(address, initial_per_contract, fee_debit, fee, block)
        })
    }
    fn reduce_margin_inner(
        &mut self,
        address: &AccountId,
        initial: &Decimal,
        fee_debit: bool,
        fee: &Decimal,
        block: u64,
    ) -> Result<Decimal> {
        let mut p = self
            .get(address)
            .cloned()
            .ok_or(MarginError::MissingPosition)?;
        let required = p.contracts.abs().checked_mul(initial)?;
        let mut excess = p.margin.checked_sub(&required)?;
        if excess.is_negative() {
            return Ok(Decimal::zero());
        }
        if fee_debit {
            excess = excess.checked_sub(fee)?;
            p.margin = p.margin.checked_sub(fee)?;
        }
        let reduction = excess.max(Decimal::zero());
        p.margin = number8(&p.margin.checked_sub(&reduction)?)?;
        let reduction = number8(&reduction)?;
        self.record(
            &p,
            "marginReduction",
            Decimal::zero(),
            reduction.negated(),
            Decimal::zero(),
            Decimal::zero(),
            None,
        )?;
        // Source marginReduction's totalPosition is zero, unlike most deltas.
        self.deltas
            .last_mut()
            .expect("just appended")
            .total_position = Decimal::zero();
        let _ = block;
        self.write_position(p)?;
        Ok(reduction)
    }
    pub fn fee_margin_reduce(
        &mut self,
        address: &AccountId,
        reduction: &Decimal,
        block: u64,
    ) -> Result<Position> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .ok_or(MarginError::MissingPosition)?;
            p.margin = number8(&p.margin.checked_sub(reduction)?)?;
            m.record(
                &p,
                "marginFeeReduction",
                Decimal::zero(),
                reduction.negated(),
                Decimal::zero(),
                Decimal::zero(),
                None,
            )?;
            m.deltas.last_mut().expect("just appended").total_position = Decimal::zero();
            let _ = block;
            m.write_position(p.clone())?;
            Ok(p)
        })
    }
    pub fn compute_maintenance_margin_requirement(
        &self,
        address: &AccountId,
        initial: &Decimal,
    ) -> Result<Decimal> {
        let Some(p) = self.get(address) else {
            return Ok(Decimal::zero());
        };
        number8(
            &number(&p.contracts.abs())?
                .checked_mul(initial)?
                .checked_mul(&dec("0.5"))?,
        )
    }
    pub fn is_below_maintenance_margin(
        &self,
        address: &AccountId,
        initial: &Decimal,
    ) -> Result<Option<bool>> {
        let Some(p) = self.get(address) else {
            return Ok(None);
        };
        Ok(Some(
            p.margin < self.compute_maintenance_margin_requirement(address, initial)?,
        ))
    }
    pub fn realize_pnl(
        &mut self,
        address: &AccountId,
        contracts: &Decimal,
        price: &Decimal,
        average: &Decimal,
        inverse: bool,
        notional: &Decimal,
        is_buy: bool,
    ) -> Result<Position> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .ok_or(MarginError::MissingPosition)?;
            let mut pnl = number8(&raw_price_delta(
                contracts, average, price, inverse, notional,
            )?)?;
            if is_buy {
                pnl = pnl.negated();
            }
            let sign = if p.unrealized_pnl > Decimal::zero() {
                Decimal::from(1)
            } else {
                Decimal::from(-1)
            };
            p.unrealized_pnl = number8(
                &p.unrealized_pnl
                    .abs()
                    .checked_sub(&pnl.abs())?
                    .checked_mul(&sign)?,
            )?;
            p.realized_pnl = pnl.clone();
            m.record(
                &p,
                "rPNL",
                Decimal::zero(),
                Decimal::zero(),
                pnl,
                Decimal::zero(),
                None,
            )?;
            m.deltas.last_mut().expect("just appended").total_position = Decimal::zero();
            m.write_position(p.clone())?;
            Ok(p)
        })
    }
    pub fn clear(
        &mut self,
        address: &AccountId,
        pnl: &Decimal,
        average: &Decimal,
        block: u64,
        mark: &Decimal,
        old_mark: &Decimal,
    ) -> Result<Position> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .ok_or(MarginError::MissingPosition)?;
            p.old_mark = Some(old_mark.clone());
            p.last_mark = Some(mark.clone());
            p.unrealized_pnl = number8(&p.unrealized_pnl.checked_add(pnl)?)?;
            m.record(
                &p,
                "markPrice",
                Decimal::zero(),
                Decimal::zero(),
                pnl.clone(),
                average.clone(),
                Some(block),
            )?;
            m.deltas.last_mut().expect("just appended").last_price = Some(mark.clone());
            m.write_position(p.clone())?;
            Ok(p)
        })
    }
    /// Source linear settlement uses binary64 multiplication and omits
    /// notional. Its unusual `uPNL -= round(uPNL-pnl)` expression is retained.
    pub fn settle_pnl(
        &mut self,
        address: &AccountId,
        contracts: &Decimal,
        price: &Decimal,
        last_mark: Option<&Decimal>,
        inverse: bool,
        notional: &Decimal,
        block: u64,
    ) -> Result<Decimal> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .ok_or(MarginError::MissingPosition)?;
            let mark = last_mark
                .or(p.avg_price.as_ref())
                .ok_or(MarginError::InvalidInput)?;
            let pnl = if inverse {
                raw_price_delta(contracts, mark, price, true, notional)?
            } else {
                from_f64((as_f64(price)? - as_f64(mark)?) * as_f64(contracts)?)?
            };
            let rounded = number8(&p.unrealized_pnl.checked_sub(&pnl)?)?;
            p.unrealized_pnl = from_f64(as_f64(&p.unrealized_pnl)? - as_f64(&rounded)?)?;
            let delta = from_f64(-as_f64(&pnl)?)?;
            m.record(
                &p,
                "settlementPNL",
                contracts.clone(),
                Decimal::zero(),
                delta,
                Decimal::zero(),
                Some(block),
            )?;
            m.write_position(p)?;
            number8(&pnl)
        })
    }
    /// The source method only mutates a discarded projection and persists
    /// nothing. This explicit repaired method updates actual positions.
    pub fn allocate_iou_claims(
        &mut self,
        buyer: &AccountId,
        seller: &AccountId,
        buyer_pnl: &Decimal,
        seller_pnl: &Decimal,
        imbalance: &Decimal,
    ) -> Result<Vec<(AccountId, Decimal)>> {
        self.transaction(|m| {
            let bp = buyer_pnl.clone().max(Decimal::zero());
            let sp = seller_pnl.clone().max(Decimal::zero());
            let total = bp.checked_add(&sp)?;
            if imbalance <= &Decimal::zero() || total.is_zero() {
                return Ok(Vec::new());
            }
            let mut allocations = Vec::new();
            let mut remaining = imbalance.quantize(8, Rounding::Down)?;
            for (a, profit) in [(buyer, bp), (seller, sp)] {
                if profit.is_zero() {
                    continue;
                }
                if let Some(mut p) = m.get(a).cloned() {
                    let share = div(&imbalance.checked_mul(&profit)?, &total)?
                        .quantize(8, Rounding::HalfUp)?
                        .min(remaining.clone());
                    remaining = remaining.checked_sub(&share)?;
                    // This explicitly repaired path keeps exact decimal claims:
                    // a binary64 bridge could itself round up the claim delta.
                    p.iou_claim = p.iou_claim.checked_add(&share)?;
                    m.write_position(p)?;
                    allocations.push((a.clone(), share));
                }
            }
            Ok(allocations)
        })
    }
    /// Literal behavior of applyIouClaimDelta: successful no-op on stored map.
    pub fn apply_legacy_iou_claim_delta(&mut self) {}
    pub fn apply_option_trade(
        &mut self,
        address: &AccountId,
        ticker: &str,
        signed_qty: &Decimal,
        trade_price: &Decimal,
        credit_margin: &Decimal,
        block: u64,
    ) -> Result<OptionTradeResult> {
        self.transaction(|m| {
            if ticker.is_empty() || ticker.len() > 256 {
                return Err(MarginError::InvalidInput);
            }
            let mut p = m
                .get(address)
                .cloned()
                .unwrap_or_else(|| Position::new(address.clone()));
            if !p.options.contains_key(ticker) && p.options.len() >= MAX_POSITION_OPTIONS {
                return Err(MarginError::LimitExceeded);
            }
            let mut o = p.options.get(ticker).cloned().unwrap_or_default();
            let before = as_f64(&o.contracts)?;
            let delta = as_f64(signed_qty)?;
            let px = as_f64(trade_price)?;
            let after = before + delta;
            let opposite = before != 0.0 && before.signum() != delta.signum();
            let flip = before != 0.0 && delta != 0.0 && opposite && delta.abs() > before.abs();
            let avg = if after == 0.0 {
                0.0
            } else if !flip && (before == 0.0 || before.signum() == after.signum()) {
                (before * as_f64(&o.avg_price)? + delta * px) / after
            } else {
                px
            };
            o.contracts = from_f64(after)?;
            o.avg_price = from_f64(avg)?;
            o.margin = from_f64(as_f64(&o.margin)? + as_f64(credit_margin)?)?;
            p.options.insert(ticker.to_owned(), o.clone());
            // The source passes an extra `this` to _touchExpiryIndex, so its
            // actual applyOptionTrade never updates the expiry/OI index.
            m.record(
                &p,
                "optionTrade",
                signed_qty.clone(),
                trade_price.clone(),
                Decimal::zero(),
                credit_margin.clone(),
                Some(block),
            )?;
            m.deltas.last_mut().expect("just appended").total_position = o.contracts.clone();
            m.deltas.last_mut().expect("just appended").option_ticker = Some(ticker.to_owned());
            m.write_position(p)?;
            Ok(OptionTradeResult {
                position: o,
                closed_quantity: from_f64(if opposite {
                    before.abs().min(delta.abs())
                } else {
                    0.0
                })?,
                flip_quantity: from_f64(if flip {
                    delta.abs() - before.abs()
                } else {
                    0.0
                })?,
            })
        })
    }
    /// Explicit correctly-argued source helper. Calling it is opt-in; source
    /// applyOptionTrade itself leaves the index untouched.
    pub fn touch_expiry_index(
        &mut self,
        before: &Decimal,
        after: &Decimal,
        ticker: &str,
    ) -> Result<()> {
        self.transaction(|m| {
            let Some(meta) = parse_ticker(ticker) else {
                return Ok(());
            };
            let expiry = meta.expiry_block;
            if expiry == 0.0 {
                return Ok(());
            }
            if !expiry.is_finite() || !(0.0..=9_007_199_254_740_991.0).contains(&expiry) {
                return Err(MarginError::InvalidInput);
            }
            let expiry = expiry as u64;
            if before.is_zero() && !after.is_zero() {
                let oi = m
                    .option_oi
                    .get(ticker)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(MarginError::LimitExceeded)?;
                m.option_oi.insert(ticker.to_owned(), oi);
                if !m.ticker_expiry.contains_key(ticker) {
                    if m.ticker_expiry.len() >= MAX_POSITION_OPTIONS {
                        return Err(MarginError::LimitExceeded);
                    }
                    m.ticker_expiry.insert(ticker.to_owned(), expiry);
                    if let Some((_, set)) = m.expiry_index.iter_mut().find(|(e, _)| *e == expiry) {
                        set.push(ticker.to_owned());
                    } else {
                        m.expiry_index.push((expiry, vec![ticker.to_owned()]));
                    }
                }
            } else if !before.is_zero() && after.is_zero() {
                let oi = m
                    .option_oi
                    .get(ticker)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(1);
                if oi == 0 {
                    m.option_oi.remove(ticker);
                    m.ticker_expiry.remove(ticker);
                    for (_, set) in &mut m.expiry_index {
                        set.retain(|t| t != ticker);
                    }
                    m.expiry_index.retain(|(_, set)| !set.is_empty());
                } else {
                    m.option_oi.insert(ticker.to_owned(), oi);
                }
            }
            Ok(())
        })
    }
    pub fn option_open_interest(&self, ticker: &str) -> u32 {
        self.option_oi.get(ticker).copied().unwrap_or(0)
    }
    pub fn get_expiring_tickers_up_to(&self, height: u64) -> Vec<String> {
        self.expiry_index
            .iter()
            .filter(|(e, _)| *e <= height)
            .flat_map(|(_, s)| s.iter().cloned())
            .collect()
    }
    pub fn cleanup_expired_tickers_up_to(&mut self, height: u64) {
        for ticker in self.get_expiring_tickers_up_to(height) {
            self.ticker_expiry.remove(&ticker);
            self.option_oi.remove(&ticker);
        }
        self.expiry_index.retain(|(e, _)| *e > height);
    }
    /// Expiry/current-block and option quantity coverage are intentionally
    /// ignored, matching the actual protective-strike helper.
    pub fn calc_liquidation_with_options(
        &self,
        address: &AccountId,
        base: &Decimal,
    ) -> Result<Decimal> {
        let Some(p) = self.get(address) else {
            return Ok(base.clone());
        };
        if p.contracts.is_zero() {
            return Ok(base.clone());
        }
        let mut strikes = Vec::new();
        for (ticker, o) in &p.options {
            if o.contracts <= Decimal::zero() {
                continue;
            }
            let Some(meta) = parse_ticker(ticker) else {
                continue;
            };
            let protective = if p.contracts.is_negative() {
                meta.option_type == OptionType::Call
            } else {
                meta.option_type == OptionType::Put
            };
            if protective {
                let strike = meta.strike.unwrap_or(0.0);
                strikes.push(from_f64(if strike.is_nan() { 0.0 } else { strike })?);
            }
        }
        if strikes.is_empty() {
            return Ok(base.clone());
        }
        if p.contracts.is_negative() {
            Ok(base
                .clone()
                .min(strikes.into_iter().min().expect("nonempty")))
        } else {
            Ok(base
                .clone()
                .max(strikes.into_iter().max().expect("nonempty")))
        }
    }
    pub fn generate_liquidation_order(
        &self,
        address: &AccountId,
        amount: &Decimal,
        bankruptcy_price: &Decimal,
        block: u64,
    ) -> Result<Option<LiquidationOrder>> {
        let p = self.get(address).ok_or(MarginError::MissingPosition)?;
        if amount <= &Decimal::zero()
            || p.contracts.is_zero()
            || p.margin.checked_add(&p.available)? <= Decimal::zero()
        {
            return Ok(None);
        }
        Ok(Some(LiquidationOrder {
            address: address.clone(),
            contract: self.contract,
            amount: number(&amount.abs())?,
            price: number(bankruptcy_price)?,
            sell: !p.contracts.is_negative(),
            block,
        }))
    }
    /// The first-vault branch has executable source behavior. A subsequent
    /// mint calls nonexistent `updatedAvgPrice` with undefined `amount`; that
    /// branch returns an error before any local changes are committed.
    pub fn move_margin_and_contracts_for_mint(
        &mut self,
        address: &AccountId,
        property: u32,
        contracts: &Decimal,
        requested_margin: &Decimal,
        block: u64,
    ) -> Result<MintMove> {
        self.transaction(|m| {
            let vault_address =
                AccountId::try_from(format!("s-{property}-{}", m.contract.get()).as_str())
                    .map_err(|_| MarginError::InvalidInput)?;
            if m.get(&vault_address).is_some() {
                return Err(MarginError::UnsupportedLegacyPath);
            }
            let mut p = m
                .get(address)
                .cloned()
                .ok_or(MarginError::MissingPosition)?;
            p.contracts = number(&p.contracts.checked_sub(contracts)?)?;
            let mut margin = requested_margin.clone();
            let mut excess = Decimal::zero();
            if margin > p.margin {
                if p.contracts.abs() > contracts.abs() {
                    excess = margin;
                    margin = Decimal::zero();
                } else {
                    excess = number8(&margin.checked_sub(&p.margin)?)?;
                    margin = p.margin.clone();
                }
            }
            p.margin = number8(&p.margin.checked_sub(&margin)?)?;
            let mut vault = Position::new(vault_address.clone());
            vault.contracts = contracts.clone();
            vault.margin = margin.clone();
            vault.avg_price = p.avg_price.clone();
            m.record(
                &vault,
                "mintMarginAndContractsToVault",
                contracts.clone(),
                margin.clone(),
                Decimal::zero(),
                vault.avg_price.clone().unwrap_or_default(),
                None,
            )?;
            // The source mistakenly uses propertyId as this delta's address.
            // Typed records identify the position actually changed.
            m.record(
                &p,
                "moveMarginAndContractsForMint",
                contracts.negated(),
                margin.negated(),
                Decimal::zero(),
                Decimal::zero(),
                None,
            )?;
            let _ = block;
            m.write_position(p)?;
            m.write_position(vault)?;
            Ok(MintMove {
                vault_address,
                contracts: contracts.clone(),
                margin,
                excess,
            })
        })
    }
    /// The original redemption path always reaches undefined `notCovered`,
    /// or fails earlier through undefined variables/methods. No value movement
    /// is authorized by a broken legacy path.
    pub fn move_margin_and_contracts_for_redeem(&mut self) -> Result<()> {
        Err(MarginError::UnsupportedLegacyPath)
    }
    /// Source helper's concrete signed contract update (no margin movement).
    pub fn execute_deleveraging(
        &mut self,
        address: &AccountId,
        size: &Decimal,
        side: bool,
    ) -> Result<Position> {
        self.transaction(|m| {
            let mut p = m
                .get(address)
                .cloned()
                .ok_or(MarginError::MissingPosition)?;
            p.contracts = number(&p.contracts.checked_add(&if side {
                size.clone()
            } else {
                size.negated()
            })?)?;
            if p.contracts.is_zero() {
                p.liquidation_price = None;
                p.bankruptcy_price = None;
            }
            m.write_position(p.clone())?;
            Ok(p)
        })
    }
    /// Shrink a counterparty and emit its requested collateral-return amount.
    /// The caller must apply the plan to the sole ledger atomically.
    pub fn adjust_deleveraging(
        &mut self,
        address: &AccountId,
        size: &Decimal,
        initial: &Decimal,
        ledger_shortfall: &Decimal,
        block: u64,
    ) -> Result<DeleverageMatch> {
        self.transaction(|m| {
            m.adjust_deleveraging_inner(address, size, initial, ledger_shortfall, block)
        })
    }
    fn adjust_deleveraging_inner(
        &mut self,
        address: &AccountId,
        size: &Decimal,
        initial: &Decimal,
        shortfall: &Decimal,
        block: u64,
    ) -> Result<DeleverageMatch> {
        let mut p = self
            .get(address)
            .cloned()
            .ok_or(MarginError::MissingPosition)?;
        let before = p.contracts.clone();
        let matched = if size <= &Decimal::zero() {
            Decimal::zero()
        } else {
            from_f64(as_f64(size)?.min(as_f64(&before)?.abs()))?
        };
        if matched.is_zero() {
            return Ok(DeleverageMatch {
                address: address.clone(),
                matched,
                before: before.clone(),
                after: before,
                collateral_return: Decimal::zero(),
                pool_share: Decimal::zero(),
            });
        }
        let change = if before.is_negative() {
            matched.clone()
        } else {
            matched.negated()
        };
        p.contracts = from_f64(as_f64(&before)? + as_f64(&change)?)?;
        self.write_position(p.clone())?;
        let reduction =
            self.reduce_margin_inner(address, initial, false, &Decimal::zero(), block)?;
        let returned = number8(&reduction.checked_sub(shortfall)?)?;
        p = self.get(address).expect("position retained").clone();
        if p.contracts.is_zero() {
            p.bankruptcy_price = None;
            p.unrealized_pnl = Decimal::zero();
        }
        // Source writes the deleverage delta twice. avgPrice remains unchanged
        // because its reset addresses a different, unused averagePrice field.
        for _ in 0..2 {
            self.record(
                &p,
                "deleverage",
                change.clone(),
                returned.negated(),
                Decimal::zero(),
                Decimal::zero(),
                None,
            )?;
        }
        self.write_position(p.clone())?;
        Ok(DeleverageMatch {
            address: address.clone(),
            matched,
            before,
            after: p.contracts,
            collateral_return: returned,
            pool_share: Decimal::zero(),
        })
    }
    pub fn simple_deleverage(&mut self, request: &DeleverageRequest) -> Result<DeleverageResult> {
        self.transaction(|m| {
            let mut candidates = Vec::new();
            for p in m.positions() {
                if p.address == request.liquidating_address
                    || p.contracts.is_zero()
                    || p.contracts.is_negative() != request.sell
                {
                    continue;
                }
                let mut pnl = Decimal::zero();
                if let Some(last) = &p.last_mark {
                    if last > &Decimal::zero() && request.mark > Decimal::zero() {
                        // Source inverse ADL ranking omits notional.
                        pnl = raw_price_delta(
                            &p.contracts,
                            last,
                            &request.mark,
                            request.inverse,
                            &if request.inverse {
                                Decimal::from(1)
                            } else {
                                request.notional.clone()
                            },
                        )?;
                    }
                }
                candidates.push((p.address.clone(), number(&p.contracts.abs())?, pnl));
            }
            if candidates.iter().any(|(_, _, p)| p > &Decimal::zero()) {
                candidates.retain(|(_, _, p)| p > &Decimal::zero());
            }
            candidates.sort_by(|a, b| b.2.abs().cmp(&a.2.abs()).then_with(|| b.1.cmp(&a.1)));
            let mut result = DeleverageResult {
                attempted: request.unfilled.clone(),
                total_deleveraged: Decimal::zero(),
                counterparties: Vec::new(),
            };
            let mut remaining = request.unfilled.clone();
            let mut weights = Vec::new();
            let mut denominator = Decimal::zero();
            for (address, abs, pnl) in candidates {
                if remaining <= Decimal::zero() {
                    break;
                }
                let matched = from_f64(as_f64(&abs)?.min(as_f64(&remaining)?))?;
                let shortfall = request
                    .ledger_shortfalls
                    .get(&address)
                    .cloned()
                    .unwrap_or_default();
                let cp = m.adjust_deleveraging_inner(
                    &address,
                    &matched,
                    &request.initial_per_contract,
                    &shortfall,
                    request.block,
                )?;
                remaining = remaining.checked_sub(&matched)?;
                result.total_deleveraged =
                    from_f64(as_f64(&result.total_deleveraged)? + as_f64(&matched)?)?;
                let weight = div(&pnl, &abs)?.checked_mul(&matched)?;
                denominator = denominator.checked_add(&weight)?;
                weights.push(weight);
                result.counterparties.push(cp);
            }
            if request.liquidation_pool > Decimal::zero() {
                for (cp, weight) in result.counterparties.iter_mut().zip(weights) {
                    let fraction = if denominator > Decimal::zero() {
                        div(&weight, &denominator)?
                    } else {
                        div(&cp.matched, &result.total_deleveraged)?
                    };
                    cp.pool_share = number8(&request.liquidation_pool.checked_mul(&fraction)?)?;
                }
            }
            Ok(result)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleverageRequest {
    pub liquidating_address: AccountId,
    pub unfilled: Decimal,
    pub sell: bool,
    pub inverse: bool,
    pub notional: Decimal,
    pub mark: Decimal,
    pub initial_per_contract: Decimal,
    pub liquidation_pool: Decimal,
    pub ledger_shortfalls: BTreeMap<AccountId, Decimal>,
    pub block: u64,
}

pub fn update_average_price(
    position: &Position,
    amount: &Decimal,
    price: &Decimal,
    sell: bool,
) -> Result<Option<Decimal>> {
    let size = if sell {
        position.contracts.checked_sub(amount)?
    } else {
        position.contracts.checked_add(amount)?
    };
    if size.is_zero() {
        return Ok(None);
    }
    let numerator = position
        .avg_price
        .clone()
        .unwrap_or_default()
        .checked_mul(&number(&position.contracts.abs())?)?
        .checked_add(&price.checked_mul(amount)?)?;
    Ok(Some(number8(&div(&numerator, &number(&size.abs())?)?)?))
}
pub fn calculate_margin_requirement(
    contracts: &Decimal,
    price: &Decimal,
    inverse: bool,
) -> Result<Decimal> {
    let notional = if inverse {
        div(contracts, price)?
    } else {
        contracts.checked_mul(price)?
    };
    number8(&notional.checked_mul(&dec("0.1"))?)
}
pub fn estimate_pnl(
    contracts: &Decimal,
    entry: &Decimal,
    exit: &Decimal,
    inverse: bool,
    notional: &Decimal,
) -> Result<Decimal> {
    // Linear estimate (unlike realized/calcPriceDelta) ignores notional.
    let n = if inverse {
        notional.clone()
    } else {
        Decimal::from(1)
    };
    number8(&raw_price_delta(contracts, entry, exit, inverse, &n)?)
}
pub fn price_delta(
    contracts: &Decimal,
    entry: &Decimal,
    exit: &Decimal,
    inverse: bool,
    notional: &Decimal,
    is_long: bool,
) -> Result<Decimal> {
    let d = raw_price_delta(contracts, entry, exit, inverse, notional)?;
    let d = if is_long { d } else { d.negated() };
    // calcPriceDelta returns BigNumber, so no binary64 bridge here.
    Ok(d.quantize(8, Rounding::HalfUp)?)
}
fn raw_price_delta(
    contracts: &Decimal,
    entry: &Decimal,
    exit: &Decimal,
    inverse: bool,
    notional: &Decimal,
) -> Result<Decimal> {
    let change = if inverse {
        div(&Decimal::from(1), entry)?.checked_sub(&div(&Decimal::from(1), exit)?)?
    } else {
        exit.checked_sub(entry)?
    };
    Ok(change.checked_mul(contracts)?.checked_mul(notional)?)
}
#[allow(clippy::too_many_arguments)]
pub fn calculate_liquidation_price(
    available: &Decimal,
    margin: &Decimal,
    contracts: &Decimal,
    notional: &Decimal,
    inverse: bool,
    is_long: bool,
    average: &Decimal,
    upnl: &Decimal,
) -> Result<LiquidationPrices> {
    if contracts.is_zero() || notional.is_zero() {
        return Err(MarginError::NonFinite);
    }
    let collateral = available
        .checked_add(margin)?
        .checked_sub(&upnl.clone().max(Decimal::zero()))?;
    let count = number(&contracts.abs())?;
    let position_notional = count.checked_mul(notional)?;
    let adjustment = div(&div(margin, &Decimal::from(2))?, &count)?;
    let (bankruptcy, liquidation) = if !inverse {
        if is_long && collateral >= position_notional.checked_mul(average)? {
            return Ok(LiquidationPrices {
                bankruptcy_price: None,
                liquidation_price: None,
            });
        }
        let contribution = div(&collateral, &position_notional)?;
        let b = if is_long {
            average
                .checked_sub(&contribution)?
                .checked_mul(&dec("1.005"))?
        } else {
            average
                .checked_add(&contribution)?
                .checked_mul(&dec("0.995"))?
        };
        let l = if is_long {
            b.checked_add(&adjustment)?
        } else {
            b.checked_sub(&adjustment)?
        };
        (b, l)
    } else {
        let term = div(&div(margin, &Decimal::from(2))?, &position_notional)?;
        let reciprocal = div(&Decimal::from(1), average)?;
        let liq = if is_long {
            reciprocal.checked_add(&term)?
        } else {
            reciprocal.checked_sub(&term)?
        };
        if !is_long && liq <= Decimal::zero() {
            return Ok(LiquidationPrices {
                bankruptcy_price: None,
                liquidation_price: None,
            });
        }
        let bterm = term.checked_mul(&dec("1.005"))?;
        let bank = if is_long {
            reciprocal.checked_add(&bterm)?
        } else {
            reciprocal.checked_sub(&bterm)?
        };
        (
            div(&Decimal::from(1), &bank)?,
            div(&Decimal::from(1), &liq)?,
        )
    };
    Ok(LiquidationPrices {
        bankruptcy_price: Some(number(&bankruptcy.quantize(4, Rounding::HalfUp)?)?.abs()),
        liquidation_price: Some(number(&liquidation.quantize(4, Rounding::HalfUp)?)?.abs()),
    })
}
fn validate_position(p: &Position) -> Result<()> {
    if p.options.len() > MAX_POSITION_OPTIONS {
        return Err(MarginError::LimitExceeded);
    }
    if p.options.keys().any(|t| t.is_empty() || t.len() > 256) {
        return Err(MarginError::InvalidInput);
    }
    Ok(())
}
fn div(a: &Decimal, b: &Decimal) -> Result<Decimal> {
    Ok(a.checked_div(b, 20, Rounding::HalfUp)?)
}
fn dec(value: &str) -> Decimal {
    Decimal::from_str(value).expect("constant finite decimal")
}
fn as_f64(value: &Decimal) -> Result<f64> {
    let n = value
        .to_string()
        .parse::<f64>()
        .map_err(|_| MarginError::NonFinite)?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(MarginError::NonFinite)
    }
}
fn from_f64(value: f64) -> Result<Decimal> {
    if !value.is_finite() {
        return Err(MarginError::NonFinite);
    }
    Decimal::from_str(&value.to_string()).map_err(Into::into)
}
fn number(value: &Decimal) -> Result<Decimal> {
    from_f64(as_f64(value)?)
}
fn number8(value: &Decimal) -> Result<Decimal> {
    number(&value.quantize(8, Rounding::HalfUp)?)
}
