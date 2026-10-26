//! Atomic, caller-driven contract matching against JointProtocolState's sole
//! Ledger. ConservingV1 is an explicit repair policy, not historical acceptance.
//! Actor, contract configuration, block and price claims remain unverified.

use crate::contract_orderbook::{
    BookError, BookTransition, CancelCriteria, ContractOrderbook, Match, Order, Side,
};
use crate::contract_registry::ContractId;
use crate::decimal::{Decimal, DecimalError, Rounding};
use crate::hash256::double_sha256;
use crate::iou::{Claim, IouError, IouKey, IouStore};
use crate::joint::{JointError, JointProtocolState, UnverifiedJointTx};
use crate::ledger::{AccountId, Ledger, PropertyId, TxId};
use crate::margin_map::{price_delta, MarginError, MarginMap, Position};
use crate::network_profile::TradeLayerMainnet;
use crate::tally::{calculate_match_fees, FeeBook, FeeRoute, MakerFlag, MatchFeeInput, TallyError};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_ECONOMIC_INPUTS: usize = 4096;
pub const MAX_ECONOMIC_RECEIPTS: usize = 8192;
pub const MAX_ECONOMIC_POSITIONS: usize = 4096;
pub const MAX_ECONOMIC_LEGS: u64 = 65_536;
pub const ATOMS_PER_TOKEN: u128 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementPolicy {
    ConservingV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicConfig {
    pub network: TradeLayerMainnet,
    pub contract: ContractId,
    pub collateral: PropertyId,
    pub inverse: bool,
    pub notional: Decimal,
    pub initial_margin_per_contract: Decimal,
    pub fee_route: FeeRoute,
    pub policy: SettlementPolicy,
    pub funding: Option<FundingConfig>,
    pub marking: Option<MarkConfig>,
    pub liquidation: Option<LiquidationConfig>,
}
impl EconomicConfig {
    pub fn validate(&self) -> Result<(), EconomicError> {
        bounded(&self.notional)?;
        bounded(&self.initial_margin_per_contract)?;
        if self.notional <= Decimal::zero()
            || self.initial_margin_per_contract <= Decimal::zero()
            || self.fee_route == FeeRoute::Spot
            || self.fee_route.contract() != self.contract.get()
        {
            return Err(EconomicError::InvalidConfig);
        }
        if let Some(funding) = &self.funding {
            funding.validate()?;
            if !matches!(self.fee_route, FeeRoute::Oracle(_)) {
                return Err(EconomicError::InvalidConfig);
            }
        }
        if let Some(marking) = &self.marking {
            if marking.oracle_id == 0 || !matches!(self.fee_route, FeeRoute::Oracle(_)) {
                return Err(EconomicError::InvalidConfig);
            }
        }
        if let Some(liquidation) = &self.liquidation {
            if self.marking.is_none()
                || liquidation.max_price_deviation_bps > 9999
                || liquidation.max_fills_per_owner == 0
                || liquidation.max_fills_per_owner > MAX_LIQUIDATION_FILLS
            {
                return Err(EconomicError::InvalidConfig);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomicInput {
    Joint(UnverifiedJointTx),
    Order(Order),
    Cancel {
        txid: TxId,
        owner: AccountId,
        criteria: CancelCriteria,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderReserve {
    pub owner: AccountId,
    pub quantity: Decimal,
    pub atoms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeReceipt {
    pub height: u64,
    pub buy_txid: TxId,
    pub sell_txid: TxId,
    pub quantity: Decimal,
    pub price: Decimal,
    pub buyer_pnl: Decimal,
    pub seller_pnl: Decimal,
    pub retained_fee_atoms: u128,
    pub collected_loss_atoms: u128,
    pub paid_claim_atoms: u128,
    pub unfunded_loss_atoms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomicError {
    InvalidConfig,
    InvalidHeight,
    InvalidInput,
    Capacity,
    Arithmetic,
    Invariant,
    CustodyMismatch,
    InvalidSnapshot,
    WrongTip,
    Joint(JointError),
    Book(BookError),
    Margin(MarginError),
    Iou(IouError),
    Tally(TallyError),
    Funding(crate::funding::FundingError),
    Basis(crate::mark_basis::BasisError),
}
impl core::fmt::Display for EconomicError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EconomicError {}
impl From<DecimalError> for EconomicError {
    fn from(_: DecimalError) -> Self {
        Self::Arithmetic
    }
}
impl From<JointError> for EconomicError {
    fn from(e: JointError) -> Self {
        Self::Joint(e)
    }
}
impl From<BookError> for EconomicError {
    fn from(e: BookError) -> Self {
        Self::Book(e)
    }
}
impl From<MarginError> for EconomicError {
    fn from(e: MarginError) -> Self {
        Self::Margin(e)
    }
}
impl From<IouError> for EconomicError {
    fn from(e: IouError) -> Self {
        Self::Iou(e)
    }
}
impl From<TallyError> for EconomicError {
    fn from(e: TallyError) -> Self {
        Self::Tally(e)
    }
}
impl From<crate::funding::FundingError> for EconomicError {
    fn from(e: crate::funding::FundingError) -> Self {
        Self::Funding(e)
    }
}
impl From<crate::mark_basis::BasisError> for EconomicError {
    fn from(e: crate::mark_basis::BasisError) -> Self {
        Self::Basis(e)
    }
}
type Result<T, E = EconomicError> = core::result::Result<T, E>;

/// Metadata never owns a second Ledger. Every escrow balance below is a
/// projection reconciled to `joint.ledger()` before publishing a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicState {
    config: EconomicConfig,
    joint: JointProtocolState,
    book: ContractOrderbook,
    positions: MarginMap,
    ious: IouStore,
    fees: FeeBook,
    reserves: BTreeMap<TxId, OrderReserve>,
    margins: BTreeMap<AccountId, u128>,
    cancel_ids: BTreeSet<TxId>,
    loss_pool_atoms: u128,
    insurance_custody_atoms: u128,
    unfunded_loss_atoms: u128,
    receipts: Vec<TradeReceipt>,
    internal_sequence: u64,
    funding: FundingState,
    marking: MarkState,
    liquidation: LiquidationState,
}
impl EconomicState {
    pub fn from_joint_unverified(
        config: EconomicConfig,
        joint: JointProtocolState,
    ) -> Result<Self> {
        config.validate()?;
        joint.verify_components_unverified()?;
        if config.network != joint.network()
            || joint.ledger().property(config.collateral).is_none()
            || joint.finalized_height().is_some()
        {
            return Err(EconomicError::InvalidConfig);
        }
        let state = Self {
            book: ContractOrderbook::new(config.contract),
            positions: MarginMap::new(config.contract),
            config,
            joint,
            ious: IouStore::new(),
            fees: FeeBook::new(),
            reserves: BTreeMap::new(),
            margins: BTreeMap::new(),
            cancel_ids: BTreeSet::new(),
            loss_pool_atoms: 0,
            insurance_custody_atoms: 0,
            unfunded_loss_atoms: 0,
            receipts: Vec::new(),
            internal_sequence: 0,
            funding: FundingState::default(),
            marking: MarkState::default(),
            liquidation: LiquidationState::default(),
        };
        state.verify()?;
        Ok(state)
    }
    pub fn config(&self) -> &EconomicConfig {
        &self.config
    }
    pub fn joint(&self) -> &JointProtocolState {
        &self.joint
    }
    pub fn ledger(&self) -> &Ledger {
        self.joint.ledger()
    }
    pub fn book(&self) -> &ContractOrderbook {
        &self.book
    }
    pub fn positions(&self) -> &MarginMap {
        &self.positions
    }
    pub fn ious(&self) -> &IouStore {
        &self.ious
    }
    pub fn fees(&self) -> &FeeBook {
        &self.fees
    }
    pub fn receipts(&self) -> &[TradeReceipt] {
        &self.receipts
    }
    pub fn margin_atoms(&self, owner: &AccountId) -> u128 {
        self.margins.get(owner).copied().unwrap_or(0)
    }
    pub fn loss_pool_atoms(&self) -> u128 {
        self.loss_pool_atoms
    }
    pub fn insurance_custody_atoms(&self) -> u128 {
        self.insurance_custody_atoms
    }
    pub fn unfunded_loss_atoms(&self) -> u128 {
        self.unfunded_loss_atoms
    }
    pub fn reserve_atoms(&self, txid: TxId) -> u128 {
        self.reserves.get(&txid).map_or(0, |r| r.atoms)
    }
    pub fn finalized_height(&self) -> Option<u64> {
        self.joint.finalized_height()
    }

    /// Source queue order is preserved. Each admitted order is funded and all
    /// its matches settle before the next order reads the resulting position.
    /// A late failure discards ordinary commands, replay markers and every leg.
    pub fn apply_block_unverified(
        &mut self,
        height: u64,
        inputs: Vec<EconomicInput>,
    ) -> Result<usize> {
        if height == 0
            || height > crate::contract_orderbook::MAX_ORDER_HEIGHT
            || height != self.finalized_height().unwrap_or(0) + 1
        {
            return Err(EconomicError::InvalidHeight);
        }
        if inputs.len() > MAX_ECONOMIC_INPUTS {
            return Err(EconomicError::Capacity);
        }
        self.verify()?;
        let mut next = self.clone();
        let before = next.receipts.len();
        let mut book_frame = LiquidationBookFrame {
            height,
            inputs: Vec::new(),
            placements: Vec::new(),
        };
        let mut mark_publication = None;
        for input in inputs {
            match input {
                EconomicInput::Joint(tx) => {
                    let observed = next.funding_publication(&tx);
                    let mark_id = next.mark_publication(&tx);
                    next.joint.apply_one_for_economic_unverified(height, tx)?;
                    if let Some((txid, closed)) = observed {
                        next.capture_funding_publication(height, txid, closed)?;
                    }
                    if let Some(id) = mark_id {
                        mark_publication = Some(id);
                    }
                }
                EconomicInput::Order(order) => {
                    if order.height != height {
                        return Err(EconomicError::InvalidHeight);
                    }
                    next.joint
                        .note_economic_input_unverified(order.txid, height, 18)?;
                    if next.config.liquidation.is_some() {
                        book_frame
                            .inputs
                            .push(LiquidationBookInput::Order(order.clone()));
                    }
                    next.book.queue(order)?;
                }
                EconomicInput::Cancel {
                    txid,
                    owner,
                    criteria,
                } => {
                    normal_owner(&owner)?;
                    next.joint.note_economic_input_unverified(txid, height, 6)?;
                    if !next.cancel_ids.insert(txid) {
                        return Err(EconomicError::InvalidInput);
                    }
                    if next.config.liquidation.is_some() {
                        book_frame.inputs.push(LiquidationBookInput::Cancel {
                            txid,
                            owner: owner.clone(),
                            criteria: criteria.clone(),
                        });
                    }
                    let removed = next.book.cancel(&owner, criteria)?;
                    for order in removed.resting {
                        next.release_reserve(order.order.txid)?;
                    }
                }
            }
        }
        let orders = next.book.drain_queued_through(height)?;
        for order in orders {
            let position = next
                .positions
                .get(&order.owner)
                .map_or_else(Decimal::zero, |p| p.contracts.clone());
            if next.config.liquidation.is_some() {
                book_frame.placements.push(LiquidationBookPlacement {
                    order: order.clone(),
                    position: position.clone(),
                });
            }
            let transition = next.book.place(order, &position)?;
            next.apply_transition(transition, height)?;
        }
        next.book.finish_height(height)?;
        if next.config.liquidation.is_some() {
            next.liquidation.book_frames.push(book_frame);
            next.liquidation_record_count()?;
        }
        next.run_mark_epoch(height, mark_publication)?;
        next.run_funding_epoch(height)?;
        next.prune_funding_observations(height);
        next.refresh_positions()?;
        next.refresh_maintenance()?;
        next.run_liquidations(height)?;
        next.refresh_positions()?;
        next.refresh_maintenance()?;
        next.joint.finalize_block_for_economic_unverified(height)?;
        next.verify()?;
        // Refuse a successful mutation that cannot be checkpointed in full.
        next.snapshot_bytes()?;
        let count = next.receipts.len() - before;
        *self = next;
        Ok(count)
    }

    fn apply_transition(&mut self, transition: BookTransition, height: u64) -> Result<()> {
        for intent in transition.admitted {
            let atoms = self.required_margin(&intent.quantity)?;
            let escrow = self.escrow(b"order", &intent.txid.as_bytes())?;
            self.transfer(intent.owner.clone(), escrow, atoms)?;
            if self
                .reserves
                .insert(
                    intent.txid,
                    OrderReserve {
                        owner: intent.owner,
                        quantity: intent.quantity,
                        atoms,
                    },
                )
                .is_some()
            {
                return Err(EconomicError::Invariant);
            }
        }
        for matched in transition.matches {
            self.settle_match(matched, height)?;
        }
        for removed in transition.self_cross_removed {
            self.release_reserve(removed.order.txid)?;
        }
        // Zero-quantity orders and market residuals cannot strand reserve atoms.
        let live: BTreeSet<_> = self.book.resting().iter().map(|o| o.order.txid).collect();
        let removed: Vec<_> = self
            .reserves
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        for id in removed {
            self.release_reserve(id)?;
        }
        Ok(())
    }

    fn settle_match(&mut self, m: Match, height: u64) -> Result<()> {
        if self.receipts.len() >= MAX_ECONOMIC_RECEIPTS {
            return Err(EconomicError::Capacity);
        }
        let buyer_before = self.position(&m.buyer);
        let seller_before = self.position(&m.seller);
        let buyer_pnl = self.trade_position(&buyer_before, &m.quantity, &m.price, Side::Buy)?;
        let seller_pnl = self.trade_position(&seller_before, &m.quantity, &m.price, Side::Sell)?;
        self.consume_reserve(m.buy_txid, &m.buy_reserve_used)?;
        self.consume_reserve(m.sell_txid, &m.sell_reserve_used)?;
        self.reconcile_margin(&m.buyer)?;
        self.reconcile_margin(&m.seller)?;
        let fee = calculate_match_fees(&MatchFeeInput {
            amount_buy: m.quantity.clone(),
            amount_sell: m.quantity.clone(),
            buy_maker: maker(m.buy_maker),
            sell_maker: maker(m.sell_maker),
            inverse: self.config.inverse,
            last_mark: m.price.clone(),
            notional_value: self.config.notional.clone(),
            channel: false,
        })?;
        let fee_escrow = self.escrow(b"fees", &[])?;
        // Collect both charges before paying a rebate or crediting any cache.
        for (owner, charge) in [(&m.buyer, fee.buyer), (&m.seller, fee.seller)] {
            self.transfer(owner.clone(), fee_escrow.clone(), charge.debit_atoms())?;
        }
        for (owner, charge) in [(&m.buyer, fee.buyer), (&m.seller, fee.seller)] {
            self.transfer(fee_escrow.clone(), owner.clone(), charge.credit_atoms())?;
        }
        let accrued = self.fees.accrue_atoms(
            self.config.collateral,
            self.config.fee_route,
            fee.retained_atoms,
            height,
        )?;
        let insurance = self.escrow(b"insurance-pending", &[])?;
        self.transfer(fee_escrow, insurance, accrued.insurance_atoms)?;
        self.insurance_custody_atoms = add(self.insurance_custody_atoms, accrued.insurance_atoms)?;

        let mark_entries = if self.config.marking.is_some() {
            Some(vec![
                self.mark_entry(
                    &m.buyer,
                    crate::mark_basis::BasisAction::Trade {
                        delta: m.quantity.clone(),
                        price: m.price.clone(),
                    },
                )?,
                self.mark_entry(
                    &m.seller,
                    crate::mark_basis::BasisAction::Trade {
                        delta: m.quantity.negated(),
                        price: m.price.clone(),
                    },
                )?,
            ])
        } else {
            None
        };
        let buyer_pnl = mark_entries
            .as_ref()
            .map_or(buyer_pnl, |e| e[0].atoms.to_decimal());
        let seller_pnl = mark_entries
            .as_ref()
            .map_or(seller_pnl, |e| e[1].atoms.to_decimal());
        let (collected, bad_debt, paid) = if let Some(mut entries) = mark_entries {
            let totals = self.settle_mark_entries(&mut entries, height)?;
            self.record_mark_event(MarkEvent {
                height,
                kind: MarkEventKind::Trade {
                    buy_txid: m.buy_txid,
                    sell_txid: m.sell_txid,
                },
                entries,
                collected_loss_atoms: totals.0,
                paid_claim_atoms: totals.2,
                unfunded_loss_atoms: totals.1,
            })?;
            totals
        } else {
            let mut collected = 0;
            let mut bad_debt = 0;
            for (owner, pnl) in [(&m.buyer, &buyer_pnl), (&m.seller, &seller_pnl)] {
                if pnl.is_negative() {
                    let debt = atoms_down(&pnl.abs())?;
                    let paid = self.collect_loss(owner, debt)?;
                    collected = add(collected, paid)?;
                    bad_debt = add(bad_debt, debt - paid)?;
                }
            }
            self.unfunded_loss_atoms = add(self.unfunded_loss_atoms, bad_debt)?;
            if collected != 0 {
                self.ious
                    .add_loss(self.iou_key(), &tokens(collected)?, height)?;
            }
            for (owner, pnl) in [(&m.buyer, &buyer_pnl), (&m.seller, &seller_pnl)] {
                if !pnl.is_negative() && !pnl.is_zero() {
                    self.add_claim(owner, atoms_down(pnl)?, height)?;
                }
            }
            (collected, bad_debt, self.pay_claims(height)?)
        };
        self.receipts.push(TradeReceipt {
            height,
            buy_txid: m.buy_txid,
            sell_txid: m.sell_txid,
            quantity: m.quantity,
            price: m.price,
            buyer_pnl,
            seller_pnl,
            retained_fee_atoms: fee.retained_atoms,
            collected_loss_atoms: collected,
            paid_claim_atoms: paid,
            unfunded_loss_atoms: bad_debt,
        });
        Ok(())
    }

    fn position(&self, owner: &AccountId) -> Position {
        self.positions
            .get(owner)
            .cloned()
            .unwrap_or_else(|| Position::new(owner.clone()))
    }
    fn trade_position(
        &mut self,
        old: &Position,
        quantity: &Decimal,
        price: &Decimal,
        side: Side,
    ) -> Result<Decimal> {
        let signed = if side == Side::Sell {
            quantity.negated()
        } else {
            quantity.clone()
        };
        let opposite =
            !old.contracts.is_zero() && old.contracts.is_negative() != signed.is_negative();
        let close = if opposite {
            old.contracts.abs().min(quantity.clone())
        } else {
            Decimal::zero()
        };
        let after = old.contracts.checked_add(&signed)?;
        let average = if after.is_zero() {
            None
        } else if old.contracts.is_zero() || (opposite && quantity > &old.contracts.abs()) {
            Some(price.clone())
        } else if opposite {
            old.avg_price.clone()
        } else {
            Some(
                old.avg_price
                    .clone()
                    .unwrap_or_default()
                    .checked_mul(&old.contracts.abs())?
                    .checked_add(&price.checked_mul(quantity)?)?
                    .checked_div(&after.abs(), 20, Rounding::HalfUp)?,
            )
        };
        let pnl = if close.is_zero() || self.config.marking.is_some() {
            Decimal::zero()
        } else {
            price_delta(
                &close,
                old.avg_price.as_ref().ok_or(EconomicError::Invariant)?,
                price,
                self.config.inverse,
                &self.config.notional,
                !old.contracts.is_negative(),
            )?
        };
        let mut position = old.clone();
        position.contracts = after;
        position.avg_price = average;
        if self.config.marking.is_none() {
            position.realized_pnl = position.realized_pnl.checked_add(&pnl)?;
        }
        position.last_mark = Some(price.clone());
        self.positions.write_position(position)?;
        Ok(pnl)
    }

    fn consume_reserve(&mut self, id: TxId, used: &Decimal) -> Result<()> {
        let mut r = self
            .reserves
            .get(&id)
            .cloned()
            .ok_or(EconomicError::Invariant)?;
        if used.is_negative() || used > &r.quantity {
            return Err(EconomicError::Invariant);
        }
        r.quantity = r.quantity.checked_sub(used)?;
        let left = self.required_margin(&r.quantity)?;
        let taken = r.atoms.checked_sub(left).ok_or(EconomicError::Invariant)?;
        self.transfer(
            self.escrow(b"order", &id.as_bytes())?,
            self.margin_escrow(&r.owner)?,
            taken,
        )?;
        self.set_margin(r.owner.clone(), add(self.margin_atoms(&r.owner), taken)?);
        r.atoms = left;
        self.reserves.insert(id, r);
        Ok(())
    }
    fn release_reserve(&mut self, id: TxId) -> Result<()> {
        let r = self.reserves.remove(&id).ok_or(EconomicError::Invariant)?;
        self.transfer(self.escrow(b"order", &id.as_bytes())?, r.owner, r.atoms)
    }
    fn reconcile_margin(&mut self, owner: &AccountId) -> Result<()> {
        let target = self.required_margin(&self.position(owner).contracts.abs())?;
        let current = self.margin_atoms(owner);
        let escrow = self.margin_escrow(owner)?;
        if current < target {
            self.transfer(owner.clone(), escrow, target - current)?;
        } else {
            self.transfer(escrow, owner.clone(), current - target)?;
        }
        self.set_margin(owner.clone(), target);
        Ok(())
    }
    fn collect_loss(&mut self, owner: &AccountId, debt: u128) -> Result<u128> {
        let pool = self.escrow(b"loss-pool", &[])?;
        let available = self
            .ledger()
            .available_atoms(self.config.collateral, owner)
            .min(debt);
        self.transfer(owner.clone(), pool.clone(), available)?;
        let margin = self.margin_atoms(owner).min(debt - available);
        self.transfer(self.margin_escrow(owner)?, pool, margin)?;
        self.set_margin(owner.clone(), self.margin_atoms(owner) - margin);
        let paid = add(available, margin)?;
        self.loss_pool_atoms = add(self.loss_pool_atoms, paid)?;
        Ok(paid)
    }
    fn add_claim(&mut self, owner: &AccountId, atoms: u128, height: u64) -> Result<()> {
        if atoms == 0 {
            return Ok(());
        }
        let key = self.iou_key();
        let mut claims = self
            .ious
            .claim_map(key)
            .map_or_else(Vec::new, |c| c.claims().to_vec());
        let amount = tokens(atoms)?;
        match claims.iter_mut().find(|c| &c.address == owner) {
            Some(c) => c.amount = c.amount.checked_add(&amount)?,
            None => claims.push(Claim {
                address: owner.clone(),
                amount: amount.clone(),
            }),
        }
        self.ious.save_claim_map(key, height, claims)?;
        self.ious.add_profit(key, &amount, height)?;
        Ok(())
    }
    /// ConservingV1 pays the source's canonical claimant order from the actual
    /// remaining pool. The legacy same-block reusable pool is never custody.
    fn pay_claims(&mut self, height: u64) -> Result<u128> {
        let key = self.iou_key();
        let mut claims = self
            .ious
            .claim_map(key)
            .map_or_else(Vec::new, |c| c.claims().to_vec());
        let pool = self.escrow(b"loss-pool", &[])?;
        let mut total = 0;
        for claim in &mut claims {
            let due = atoms_exact(&claim.amount)?;
            let paid = due.min(self.loss_pool_atoms);
            self.transfer(pool.clone(), claim.address.clone(), paid)?;
            self.loss_pool_atoms -= paid;
            total = add(total, paid)?;
            claim.amount = tokens(due - paid)?;
        }
        claims.retain(|c| !c.amount.is_zero());
        if self.ious.claim_map(key).is_some() {
            self.ious.save_claim_map(key, height, claims)?;
        }
        Ok(total)
    }
    fn refresh_positions(&mut self) -> Result<()> {
        let mut positions: Vec<_> = self.positions.positions().cloned().collect();
        for p in &mut positions {
            p.margin = tokens(self.margin_atoms(&p.address))?;
            // Available is deliberately a query, not a stale copied balance.
            p.available = Decimal::zero();
            p.iou_claim = self
                .ious
                .claim_map(self.iou_key())
                .and_then(|c| c.claim(&p.address))
                .cloned()
                .unwrap_or_default();
        }
        self.positions.merge_positions(&positions, false)?;
        Ok(())
    }
    fn required_margin(&self, quantity: &Decimal) -> Result<u128> {
        atoms_up(&quantity.checked_mul(&self.config.initial_margin_per_contract)?)
    }
    fn set_margin(&mut self, owner: AccountId, atoms: u128) {
        if atoms == 0 {
            self.margins.remove(&owner);
        } else {
            self.margins.insert(owner, atoms);
        }
    }
    fn iou_key(&self) -> IouKey {
        IouKey::new(self.config.contract.get(), self.config.collateral.get())
    }
    fn margin_escrow(&self, owner: &AccountId) -> Result<AccountId> {
        self.escrow(b"margin", owner.as_str().as_bytes())
    }
    fn escrow(&self, role: &[u8], key: &[u8]) -> Result<AccountId> {
        let mut bytes = self.domain(b"escrow");
        bytes.extend_from_slice(&(role.len() as u32).to_be_bytes());
        bytes.extend_from_slice(role);
        bytes.extend_from_slice(&(key.len() as u32).to_be_bytes());
        bytes.extend_from_slice(key);
        let hash = double_sha256(&bytes);
        let mut name = String::from("~econ:");
        use core::fmt::Write;
        for b in hash {
            write!(&mut name, "{b:02x}").map_err(|_| EconomicError::Arithmetic)?;
        }
        AccountId::try_from(name.as_str()).map_err(|_| EconomicError::InvalidInput)
    }
    fn domain(&self, label: &[u8]) -> Vec<u8> {
        let mut out = b"TradeLayer/ConservingV1/".to_vec();
        out.extend_from_slice(label);
        out.push(self.config.network.tag());
        out.extend_from_slice(&self.config.network.genesis().0);
        out.extend_from_slice(&self.config.contract.get().to_be_bytes());
        out.extend_from_slice(&self.config.collateral.get().to_be_bytes());
        out
    }
    fn leg_id(&self, sequence: u64) -> TxId {
        let mut bytes = self.domain(b"leg");
        bytes.extend_from_slice(&sequence.to_be_bytes());
        TxId::from_bytes(double_sha256(&bytes))
    }
    fn transfer(&mut self, from: AccountId, to: AccountId, atoms: u128) -> Result<()> {
        if atoms == 0 {
            return Ok(());
        }
        if self.internal_sequence >= MAX_ECONOMIC_LEGS {
            return Err(EconomicError::Capacity);
        }
        self.joint.apply_economic_transfer_unverified(
            self.leg_id(self.internal_sequence),
            self.config.collateral,
            from,
            to,
            atoms,
        )?;
        self.internal_sequence += 1;
        Ok(())
    }

    pub fn verify(&self) -> Result<()> {
        self.config.validate()?;
        self.joint.verify_for_economic_owner_unverified()?;
        self.verify_funding()?;
        self.verify_marks()?;
        self.verify_liquidations()?;
        self.verify_system_epochs()?;
        if self.joint.network() != self.config.network
            || self.ledger().property(self.config.collateral).is_none()
            || self.book.contract() != self.config.contract
            || self.positions.contract() != self.config.contract
            || self.receipts.len() > MAX_ECONOMIC_RECEIPTS
            || self.positions.positions().count() > MAX_ECONOMIC_POSITIONS
            || self.internal_sequence > MAX_ECONOMIC_LEGS
            || !self.book.queued().is_empty()
            || !self.book.drained().is_empty()
            || self.book.processed_through() != self.finalized_height()
        {
            return Err(EconomicError::Invariant);
        }
        let book = ContractOrderbook::from_snapshot(self.book.snapshot())?;
        if book != self.book {
            return Err(EconomicError::Invariant);
        }
        let mut inputs: BTreeSet<_> = self.book.snapshot().seen.into_iter().collect();
        for id in &self.cancel_ids {
            if !inputs.insert(*id) {
                return Err(EconomicError::Invariant);
            }
        }
        if &inputs != self.joint.economic_input_ids_unverified() {
            return Err(EconomicError::Invariant);
        }
        for sequence in 0..self.internal_sequence {
            if !self.ledger().is_processed(self.leg_id(sequence)) {
                return Err(EconomicError::Invariant);
            }
        }
        let mut expected = BTreeMap::new();
        let mut track = |account: AccountId, atoms: u128| -> Result<()> {
            if atoms != 0
                && expected
                    .insert((self.config.collateral, account), atoms)
                    .is_some()
            {
                return Err(EconomicError::Invariant);
            }
            Ok(())
        };
        if self.reserves.len() != self.book.resting().len() {
            return Err(EconomicError::Invariant);
        }
        for order in self.book.resting() {
            let r = self
                .reserves
                .get(&order.order.txid)
                .ok_or(EconomicError::Invariant)?;
            if r.owner != order.order.owner
                || r.quantity != order.reserve_quantity
                || r.atoms != self.required_margin(&r.quantity)?
            {
                return Err(EconomicError::Invariant);
            }
            track(
                self.escrow(b"order", &order.order.txid.as_bytes())?,
                r.atoms,
            )?;
        }
        let mut net_contracts = Decimal::zero();
        for p in self.positions.positions() {
            normal_owner(&p.address)?;
            bounded(&p.contracts)?;
            let claims = self
                .ious
                .claim_map(self.iou_key())
                .and_then(|c| c.claim(&p.address))
                .cloned()
                .unwrap_or_default();
            if p.margin != tokens(self.margin_atoms(&p.address))?
                || p.iou_claim != claims
                || p.available != Decimal::zero()
                || p.unrealized_pnl != Decimal::zero()
                || p.liquidation_price.is_some()
                || p.bankruptcy_price.is_some()
                || p.old_mark.is_some()
                || p.new_position_this_block != Decimal::zero()
                || !p.options.is_empty()
                || (p.contracts.is_zero() && p.avg_price.is_some())
                || (!p.contracts.is_zero()
                    && p.avg_price.as_ref().is_none_or(|v| v <= &Decimal::zero()))
            {
                return Err(EconomicError::Invariant);
            }
            net_contracts = net_contracts.checked_add(&p.contracts)?;
        }
        if !net_contracts.is_zero() || !self.positions.deltas().is_empty() {
            return Err(EconomicError::Invariant);
        }
        for (owner, atoms) in &self.margins {
            if *atoms == 0 || self.positions.get(owner).is_none() {
                return Err(EconomicError::Invariant);
            }
            track(self.margin_escrow(owner)?, *atoms)?;
        }
        let key = self.config.fee_route.key(self.config.collateral)?;
        let fees = self.fees.expected_custody()?;
        if fees.iter().any(|(k, _)| *k != key) || !self.fees.queued().collect::<Vec<_>>().is_empty()
        {
            return Err(EconomicError::Invariant);
        }
        FeeBook::from_snapshot(self.fees.snapshot())?;
        track(
            self.escrow(b"fees", &[])?,
            self.fees.expected_custody_atoms(key)?,
        )?;
        track(
            self.escrow(b"insurance-pending", &[])?,
            self.insurance_custody_atoms,
        )?;
        track(self.escrow(b"loss-pool", &[])?, self.loss_pool_atoms)?;
        let actual: BTreeMap<_, _> = self
            .ledger()
            .balances_with_account_prefix("~econ:")
            .into_iter()
            .map(|(p, a, n)| ((p, a), n))
            .collect();
        if actual != expected {
            return Err(EconomicError::CustodyMismatch);
        }
        if self.ious.buckets().any(|b| b.key != self.iou_key())
            || self.ious.claim_maps().any(|c| c.key != self.iou_key())
        {
            return Err(EconomicError::Invariant);
        }
        let mut total_claims = Decimal::zero();
        if let Some(claims) = self.ious.claim_map(self.iou_key()) {
            for claim in claims.claims() {
                if claim.amount <= Decimal::zero() || self.positions.get(&claim.address).is_none() {
                    return Err(EconomicError::Invariant);
                }
                atoms_exact(&claim.amount)?;
                total_claims = total_claims.checked_add(&claim.amount)?;
            }
        }
        if self.ious.get(self.iou_key())
            != tokens(self.loss_pool_atoms)?.checked_sub(&total_claims)?
        {
            return Err(EconomicError::Invariant);
        }
        self.ious
            .snapshot_bytes()
            .map_err(|_| EconomicError::InvalidSnapshot)?;
        let mut last = 0;
        let mut collected = 0;
        let mut paid = 0;
        let mut bad_debt = 0;
        for event in &self.marking.events {
            if !matches!(event.kind, MarkEventKind::Trade { .. }) {
                collected = add(collected, event.collected_loss_atoms)?;
                paid = add(paid, event.paid_claim_atoms)?;
                bad_debt = add(bad_debt, event.unfunded_loss_atoms)?;
            }
        }
        let (expected_fees, expected_insurance) = self.expected_economic_fees()?;
        for receipt in &self.receipts {
            if receipt.height < last
                || receipt.height == 0
                || self.finalized_height().is_none_or(|h| receipt.height > h)
                || !self.book.has_seen(receipt.buy_txid)
                || !self.book.has_seen(receipt.sell_txid)
                || receipt.buy_txid == receipt.sell_txid
                || receipt.quantity <= Decimal::zero()
                || receipt.price <= Decimal::zero()
            {
                return Err(EconomicError::Invariant);
            }
            bounded(&receipt.quantity)?;
            bounded(&receipt.price)?;
            collected = add(collected, receipt.collected_loss_atoms)?;
            paid = add(paid, receipt.paid_claim_atoms)?;
            bad_debt = add(bad_debt, receipt.unfunded_loss_atoms)?;
            last = receipt.height;
        }
        if collected.checked_sub(paid) != Some(self.loss_pool_atoms)
            || bad_debt != self.unfunded_loss_atoms
            || expected_fees != self.fees
            || expected_insurance != self.insurance_custody_atoms
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}

fn normal_owner(owner: &AccountId) -> Result<()> {
    if owner.as_str().starts_with('~') {
        return Err(EconomicError::InvalidInput);
    }
    Ok(())
}
fn maker(value: bool) -> MakerFlag {
    if value {
        MakerFlag::True
    } else {
        MakerFlag::False
    }
}
fn add(a: u128, b: u128) -> Result<u128> {
    a.checked_add(b).ok_or(EconomicError::Arithmetic)
}
fn bounded(value: &Decimal) -> Result<()> {
    if value.coefficient_digits() > 40 || value.decimal_places() > 20 {
        return Err(EconomicError::Capacity);
    }
    Ok(())
}
fn tokens(atoms: u128) -> Result<Decimal> {
    Ok(Decimal::from(atoms).checked_div(&Decimal::from(ATOMS_PER_TOKEN), 8, Rounding::Down)?)
}
fn atoms_with(value: &Decimal, rounding: Rounding) -> Result<u128> {
    if value.is_negative() {
        return Err(EconomicError::Arithmetic);
    }
    value
        .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))?
        .quantize(0, rounding)?
        .to_string()
        .parse()
        .map_err(|_| EconomicError::Arithmetic)
}
fn atoms_up(value: &Decimal) -> Result<u128> {
    let down = atoms_down(value)?;
    if tokens(down)? == *value {
        Ok(down)
    } else {
        add(down, 1)
    }
}
fn atoms_down(value: &Decimal) -> Result<u128> {
    atoms_with(value, Rounding::Down)
}
fn atoms_exact(value: &Decimal) -> Result<u128> {
    let atoms = atoms_down(value)?;
    if tokens(atoms)? != *value {
        return Err(EconomicError::Arithmetic);
    }
    Ok(atoms)
}

include!("economic_snapshot_impl.rs");
include!("economic_funding_impl.rs");
include!("economic_mark_impl.rs");
include!("economic_liquidation_impl.rs");
include!("economic_liquidation_replay_impl.rs");
