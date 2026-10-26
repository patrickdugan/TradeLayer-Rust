// Bounded, funded reduce-only closeouts in the existing economic Ledger.
use crate::contract_orderbook::{
    ExecutionPriceGuard, LiquidationId, ReduceOnlyCandidate, ReduceOnlyRequest,
};

pub const MAX_LIQUIDATION_EVENTS: usize = 4096;
pub const MAX_LIQUIDATION_FILLS: u32 = 64;
pub const MAX_LIQUIDATION_RECORDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidationPolicy {
    ConservingLiquidationV1,
}

fn liquidation_cancel_book(
    book: &mut ContractOrderbook,
    owner: &AccountId,
    registered_pool: Option<&AccountId>,
) -> Result<crate::contract_orderbook::Cancellation> {
    if registered_pool == Some(owner) {
        if book.orders_for_owner(owner).next().is_some()
            || book
                .queued()
                .iter()
                .chain(book.drained())
                .any(|o| &o.owner == owner)
        {
            return Err(EconomicError::Invariant);
        }
        Ok(crate::contract_orderbook::Cancellation::default())
    } else {
        Ok(book.cancel(owner, CancelCriteria::All)?)
    }
}
fn liquidation_book_candidate(
    book: &ContractOrderbook,
    request: &ReduceOnlyRequest,
    quantity: &Decimal,
    registered_pool: Option<&AccountId>,
) -> Result<Option<ReduceOnlyCandidate>> {
    if registered_pool == Some(&request.owner) {
        Ok(book.next_reduce_only_pool_candidate(
            request,
            quantity,
            registered_pool.ok_or(EconomicError::Invariant)?,
        )?)
    } else {
        Ok(book.next_reduce_only_candidate(request, quantity)?)
    }
}
fn liquidation_consume_book_candidate(
    book: &mut ContractOrderbook,
    candidate: &ReduceOnlyCandidate,
    quantity: &Decimal,
    amount: &Decimal,
    registered_pool: Option<&AccountId>,
) -> Result<crate::contract_orderbook::ReduceOnlyFill> {
    if registered_pool == Some(&candidate.request.owner) {
        Ok(book.consume_reduce_only_pool_candidate(
            candidate,
            quantity,
            amount,
            registered_pool.ok_or(EconomicError::Invariant)?,
        )?)
    } else {
        Ok(book.consume_reduce_only_candidate(candidate, quantity, amount)?)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationConfig {
    pub max_price_deviation_bps: u32,
    pub max_fills_per_owner: u32,
    pub policy: LiquidationPolicy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidationStatus {
    Cured,
    Closed,
    NoLiquidityWithinGuard,
    UnfundedBestMaker,
    FillLimit,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationCancellation {
    pub order: RestingOrder,
    pub reserve_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedLiquidationMaker {
    pub order: RestingOrder,
    pub available_atoms: u128,
    pub margin_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForcedCloseReceipt {
    pub maker_before: RestingOrder,
    pub quantity: Decimal,
    pub price: Decimal,
    pub distressed_pnl: Decimal,
    pub maker_pnl: Decimal,
    pub maker_available_before_atoms: u128,
    pub maker_margin_before_atoms: u128,
    pub distressed_available_before_fee_atoms: u128,
    pub distressed_margin_before_fee_atoms: u128,
    pub nominal_fee_atoms: u128,
    pub charged_fee_atoms: u128,
    pub maker_rebate_atoms: u128,
    pub retained_fee_atoms: u128,
    pub collected_loss_atoms: u128,
    pub paid_claim_atoms: u128,
    pub unfunded_loss_atoms: u128,
    pub distressed_margin_after_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationEvent {
    pub height: u64,
    pub id: TxId,
    pub mark_id: TxId,
    pub owner: AccountId,
    pub quantity_before: Decimal,
    pub margin_before_atoms: u128,
    pub available_before_atoms: u128,
    pub required_before_atoms: u128,
    pub guard_price: Decimal,
    pub book_before: BookSnapshot,
    pub cancelled: Vec<LiquidationCancellation>,
    pub pool_cancelled_quotes: Vec<FundedAmmQuote>,
    pub top_up_atoms: u128,
    pub amm_fills: Vec<AmmForcedCloseReceipt>,
    pub fills: Vec<ForcedCloseReceipt>,
    pub blocked_maker: Option<BlockedLiquidationMaker>,
    pub status: LiquidationStatus,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiquidationState {
    pub events: Vec<LiquidationEvent>,
    pub book_frames: Vec<LiquidationBookFrame>,
}

impl EconomicState {
    pub fn liquidation_state(&self) -> &LiquidationState {
        &self.liquidation
    }
    fn liquidation_id(&self, height: u64, owner: &AccountId) -> TxId {
        let mut bytes = self.domain(b"liquidation-epoch-v1");
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(owner.as_str().as_bytes());
        TxId::from_bytes(double_sha256(&bytes))
    }
    fn maintenance_atoms(&self, quantity: &Decimal) -> Result<u128> {
        atoms_up(
            &quantity
                .abs()
                .checked_mul(&self.config.initial_margin_per_contract)?
                .checked_mul(&"0.5".parse::<Decimal>()?)?,
        )
    }
    fn liquidation_guard(&self, quantity: &Decimal, mark: &Decimal) -> Result<Decimal> {
        let config = self
            .config
            .liquidation
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let factor = if quantity.is_negative() {
            10_000 + config.max_price_deviation_bps
        } else {
            10_000 - config.max_price_deviation_bps
        };
        Ok(mark.checked_mul(&Decimal::from(factor))?.checked_div(
            &Decimal::from(10_000),
            20,
            Rounding::HalfUp,
        )?)
    }
    fn liquidation_request(
        &self,
        event: &LiquidationEvent,
        quantity: &Decimal,
    ) -> ReduceOnlyRequest {
        ReduceOnlyRequest {
            id: LiquidationId::new_unverified(event.id),
            height: event.height,
            owner: event.owner.clone(),
            side: if quantity.is_negative() {
                Side::Buy
            } else {
                Side::Sell
            },
            requested_quantity: quantity.abs(),
            guard: if quantity.is_negative() {
                ExecutionPriceGuard::BuyAtMost(event.guard_price.clone())
            } else {
                ExecutionPriceGuard::SellAtLeast(event.guard_price.clone())
            },
        }
    }
    fn run_liquidations(&mut self, height: u64) -> Result<()> {
        if self.config.liquidation.is_none() {
            return Ok(());
        }
        let Some((mark_id, observation)) = self.marking.events.iter().rev().find_map(|e| {
            if e.height == height {
                if let MarkEventKind::Mark { id, observation } = &e.kind {
                    return Some((*id, observation.clone()));
                }
            }
            None
        }) else {
            return Ok(());
        };
        for call in self.expected_maintenance()? {
            let quantity = self.position(&call.owner).contracts;
            if quantity.is_zero()
                || self.margin_atoms(&call.owner) >= self.maintenance_atoms(&quantity)?
            {
                continue;
            }
            if self.liquidation.events.len() >= MAX_LIQUIDATION_EVENTS {
                return Err(EconomicError::Capacity);
            }
            let mut event = LiquidationEvent {
                height,
                id: self.liquidation_id(height, &call.owner),
                mark_id,
                owner: call.owner.clone(),
                quantity_before: quantity.clone(),
                margin_before_atoms: self.margin_atoms(&call.owner),
                available_before_atoms: self
                    .ledger()
                    .available_atoms(self.config.collateral, &call.owner),
                required_before_atoms: self.maintenance_atoms(&quantity)?,
                guard_price: self.liquidation_guard(&quantity, &observation.price)?,
                book_before: self.book.snapshot(),
                cancelled: Vec::new(),
                pool_cancelled_quotes: Vec::new(),
                top_up_atoms: 0,
                amm_fills: Vec::new(),
                fills: Vec::new(),
                blocked_maker: None,
                status: LiquidationStatus::NoLiquidityWithinGuard,
            };
            self.joint
                .note_economic_epoch_unverified(event.id, height)?;
            let registered_pool = self.liquidation_registered_pool()?;
            if self.is_amm_owner(&event.owner) {
                event.pool_cancelled_quotes = self.cancel_amm_quotes()?;
            }
            let cancelled =
                liquidation_cancel_book(&mut self.book, &event.owner, registered_pool.as_ref())?;
            if !cancelled.queued.is_empty() {
                return Err(EconomicError::Invariant);
            }
            for order in cancelled.resting {
                let reserve_atoms = self.reserve_atoms(order.order.txid);
                self.release_reserve(order.order.txid)?;
                event.cancelled.push(LiquidationCancellation {
                    order,
                    reserve_atoms,
                });
            }
            let missing = self
                .required_margin(&quantity.abs())?
                .saturating_sub(self.margin_atoms(&event.owner));
            event.top_up_atoms = missing.min(
                self.ledger()
                    .available_atoms(self.config.collateral, &event.owner),
            );
            self.transfer(
                event.owner.clone(),
                self.margin_escrow(&event.owner)?,
                event.top_up_atoms,
            )?;
            self.set_margin(
                event.owner.clone(),
                add(self.margin_atoms(&event.owner), event.top_up_atoms)?,
            );
            loop {
                let quantity = self.position(&event.owner).contracts;
                if quantity.is_zero() {
                    event.status = LiquidationStatus::Closed;
                    break;
                }
                if self.margin_atoms(&event.owner) >= self.maintenance_atoms(&quantity)? {
                    event.status = LiquidationStatus::Cured;
                    break;
                }
                if event.fills.len() + event.amm_fills.len()
                    >= self
                        .config
                        .liquidation
                        .as_ref()
                        .ok_or(EconomicError::Invariant)?
                        .max_fills_per_owner as usize
                {
                    event.status = LiquidationStatus::FillLimit;
                    break;
                }
                if event.fills.is_empty() {
                    if let Some((quote, amount)) =
                        self.next_funded_amm_candidate(&event, &quantity)?
                    {
                        let receipt = self.settle_forced_amm_candidate(&event, &quote, &amount)?;
                        event.amm_fills.push(receipt);
                        continue;
                    }
                }
                let request = self.liquidation_request(&event, &quantity);
                let Some(candidate) = liquidation_book_candidate(
                    &self.book,
                    &request,
                    &quantity,
                    registered_pool.as_ref(),
                )?
                else {
                    event.status = LiquidationStatus::NoLiquidityWithinGuard;
                    break;
                };
                let maker = &candidate.maker_before.order.owner;
                let available = self.ledger().available_atoms(self.config.collateral, maker);
                let margin = self.margin_atoms(maker);
                let maker_quantity = self.position(maker).contracts;
                if available
                    < self.liquidation_maker_required(&candidate, &maker_quantity, margin)?
                {
                    event.blocked_maker = Some(BlockedLiquidationMaker {
                        order: candidate.maker_before,
                        available_atoms: available,
                        margin_atoms: margin,
                    });
                    event.status = LiquidationStatus::UnfundedBestMaker;
                    break;
                }
                let receipt = self.settle_forced_candidate(&event, &candidate)?;
                event.fills.push(receipt);
            }
            self.liquidation.events.push(event);
            self.liquidation_record_count()?;
        }
        Ok(())
    }
    fn liquidation_maker_required(
        &self,
        candidate: &ReduceOnlyCandidate,
        old: &Decimal,
        margin: u128,
    ) -> Result<u128> {
        let remaining = candidate
            .maker_before
            .remaining
            .checked_sub(&candidate.maximum_quantity)?;
        let after_reserve = candidate
            .maker_before
            .reserve_quantity
            .clone()
            .min(remaining);
        let used = self
            .required_margin(&candidate.maker_before.reserve_quantity)?
            .checked_sub(self.required_margin(&after_reserve)?)
            .ok_or(EconomicError::Invariant)?;
        let delta = if candidate.maker_before.order.side == Side::Buy {
            candidate.maximum_quantity.clone()
        } else {
            candidate.maximum_quantity.negated()
        };
        let target = self.required_margin(&old.checked_add(&delta)?.abs())?;
        Ok(target.saturating_sub(add(margin, used)?))
    }
    fn forced_nominal_fee(
        &self,
        quantity: &Decimal,
        price: &Decimal,
        distressed_side: Side,
    ) -> Result<u128> {
        let buy_maker = distressed_side == Side::Sell;
        let fee = calculate_match_fees(&MatchFeeInput {
            amount_buy: quantity.clone(),
            amount_sell: quantity.clone(),
            buy_maker: maker(buy_maker),
            sell_maker: maker(!buy_maker),
            inverse: self.config.inverse,
            last_mark: price.clone(),
            notional_value: self.config.notional.clone(),
            channel: false,
        })?;
        Ok(if distressed_side == Side::Buy {
            fee.buyer.debit_atoms()
        } else {
            fee.seller.debit_atoms()
        })
    }
    fn settle_forced_candidate(
        &mut self,
        event: &LiquidationEvent,
        candidate: &ReduceOnlyCandidate,
    ) -> Result<ForcedCloseReceipt> {
        let owner = &event.owner;
        let maker_owner = &candidate.maker_before.order.owner;
        let old = self.position(owner);
        let maker_old = self.position(maker_owner);
        let maker_available_before_atoms = self
            .ledger()
            .available_atoms(self.config.collateral, maker_owner);
        let maker_margin_before_atoms = self.margin_atoms(maker_owner);
        let registered_pool = self.liquidation_registered_pool()?;
        let fill = liquidation_consume_book_candidate(
            &mut self.book,
            candidate,
            &old.contracts,
            &candidate.maximum_quantity,
            registered_pool.as_ref(),
        )?;
        if fill.id.txid() != event.id {
            return Err(EconomicError::Invariant);
        }
        self.trade_position(&old, &fill.quantity, &fill.price, fill.distressed_side)?;
        self.trade_position(
            &maker_old,
            &fill.quantity,
            &fill.price,
            candidate.maker_before.order.side,
        )?;
        self.consume_reserve(fill.maker_txid, &fill.maker_reserve_used)?;
        self.reconcile_margin(maker_owner)?;
        // A forced reduction releases surplus custody, and never demands that
        // an already distressed owner first fund its old initial target.
        let target = self.required_margin(&self.position(owner).contracts.abs())?;
        let margin = self.margin_atoms(owner);
        if margin > target {
            self.transfer(self.margin_escrow(owner)?, owner.clone(), margin - target)?;
            self.set_margin(owner.clone(), target);
        }
        if fill.maker_remaining.is_zero() {
            self.release_reserve(fill.maker_txid)?;
        }
        let distressed_available_before_fee_atoms =
            self.ledger().available_atoms(self.config.collateral, owner);
        let distressed_margin_before_fee_atoms = self.margin_atoms(owner);
        let nominal_fee_atoms =
            self.forced_nominal_fee(&fill.quantity, &fill.price, fill.distressed_side)?;
        let fee_escrow = self.escrow(b"fees", &[])?;
        let available_fee = nominal_fee_atoms.min(distressed_available_before_fee_atoms);
        let margin_fee =
            (nominal_fee_atoms - available_fee).min(distressed_margin_before_fee_atoms);
        let charged_fee_atoms = add(available_fee, margin_fee)?;
        self.transfer(owner.clone(), fee_escrow.clone(), available_fee)?;
        self.transfer(self.margin_escrow(owner)?, fee_escrow.clone(), margin_fee)?;
        self.set_margin(
            owner.clone(),
            distressed_margin_before_fee_atoms - margin_fee,
        );
        let maker_rebate_atoms = charged_fee_atoms / 2;
        let retained_fee_atoms = charged_fee_atoms - maker_rebate_atoms;
        self.transfer(fee_escrow.clone(), maker_owner.clone(), maker_rebate_atoms)?;
        let accrued = self.fees.accrue_atoms(
            self.config.collateral,
            self.config.fee_route,
            retained_fee_atoms,
            event.height,
        )?;
        self.transfer(
            fee_escrow,
            self.escrow(b"insurance-pending", &[])?,
            accrued.insurance_atoms,
        )?;
        self.insurance_custody_atoms = add(self.insurance_custody_atoms, accrued.insurance_atoms)?;
        let delta = if fill.distressed_side == Side::Buy {
            fill.quantity.clone()
        } else {
            fill.quantity.negated()
        };
        let mut entries = vec![
            self.mark_entry(
                owner,
                BasisAction::Trade {
                    delta: delta.clone(),
                    price: fill.price.clone(),
                },
            )?,
            self.mark_entry(
                maker_owner,
                BasisAction::Trade {
                    delta: delta.negated(),
                    price: fill.price.clone(),
                },
            )?,
        ];
        let distressed_pnl = entries[0].atoms.to_decimal();
        let maker_pnl = entries[1].atoms.to_decimal();
        let (collected_loss_atoms, unfunded_loss_atoms, paid_claim_atoms) =
            self.settle_mark_entries(&mut entries, event.height)?;
        self.record_mark_event(MarkEvent {
            height: event.height,
            kind: MarkEventKind::Forced {
                epoch_id: event.id,
                fill_index: event.fills.len() as u32,
                maker_txid: fill.maker_txid,
                distressed_owner: owner.clone(),
            },
            entries,
            collected_loss_atoms,
            paid_claim_atoms,
            unfunded_loss_atoms,
        })?;
        Ok(ForcedCloseReceipt {
            maker_before: candidate.maker_before.clone(),
            quantity: fill.quantity,
            price: fill.price,
            distressed_pnl,
            maker_pnl,
            maker_available_before_atoms,
            maker_margin_before_atoms,
            distressed_available_before_fee_atoms,
            distressed_margin_before_fee_atoms,
            nominal_fee_atoms,
            charged_fee_atoms,
            maker_rebate_atoms,
            retained_fee_atoms,
            collected_loss_atoms,
            paid_claim_atoms,
            unfunded_loss_atoms,
            distressed_margin_after_atoms: self.margin_atoms(owner),
        })
    }
    fn liquidation_record_count(&self) -> Result<usize> {
        if self.liquidation.events.len() > MAX_LIQUIDATION_EVENTS {
            return Err(EconomicError::Capacity);
        }
        let mut count = 0usize;
        if self.liquidation.book_frames.len() > MAX_BOOK_FRAMES {
            return Err(EconomicError::Capacity);
        }
        for frame in &self.liquidation.book_frames {
            if frame.inputs.len() > MAX_ECONOMIC_INPUTS
                || frame.placements.len() > MAX_ECONOMIC_INPUTS
            {
                return Err(EconomicError::Capacity);
            }
            count = count
                .checked_add(frame.inputs.len())
                .and_then(|n| n.checked_add(frame.placements.len()))
                .ok_or(EconomicError::Capacity)?;
            if count > MAX_LIQUIDATION_RECORDS {
                return Err(EconomicError::Capacity);
            }
        }
        for event in &self.liquidation.events {
            if event
                .fills
                .len()
                .checked_add(event.amm_fills.len())
                .is_none_or(|n| n > MAX_LIQUIDATION_FILLS as usize)
            {
                return Err(EconomicError::Capacity);
            }
            for n in [
                event.book_before.queued.len(),
                event.book_before.drained.len(),
                event.book_before.resting.len(),
                event.book_before.seen.len(),
                event.cancelled.len(),
                event.pool_cancelled_quotes.len(),
                event.amm_fills.len(),
                event.fills.len(),
                usize::from(event.blocked_maker.is_some()),
            ] {
                count = count.checked_add(n).ok_or(EconomicError::Capacity)?;
                if count > MAX_LIQUIDATION_RECORDS {
                    return Err(EconomicError::Capacity);
                }
            }
        }
        Ok(count)
    }
    fn forced_receipt(&self, id: TxId, index: u32) -> Result<&ForcedCloseReceipt> {
        self.liquidation
            .events
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.fills.get(index as usize))
            .ok_or(EconomicError::Invariant)
    }
    fn forced_amm_receipt(&self, id: TxId, index: u32) -> Result<&AmmForcedCloseReceipt> {
        self.liquidation
            .events
            .iter()
            .find(|event| event.id == id)
            .and_then(|event| event.amm_fills.get(index as usize))
            .ok_or(EconomicError::Invariant)
    }
    fn liquidation_registered_pool(&self) -> Result<Option<AccountId>> {
        if self.config.amm.is_some() {
            Ok(Some(self.amm_owner()?))
        } else {
            Ok(None)
        }
    }
    fn forced_mark_event(&self, id: TxId, index: u32) -> Result<&MarkEvent> {
        let mut matches = self.marking.events.iter().filter(|e| matches!(&e.kind,
            MarkEventKind::Forced { epoch_id, fill_index, .. } if *epoch_id == id && *fill_index == index));
        let event = matches.next().ok_or(EconomicError::Invariant)?;
        if matches.next().is_some() {
            return Err(EconomicError::Invariant);
        }
        Ok(event)
    }
    fn expected_economic_fees(&self) -> Result<(FeeBook, u128)> {
        let mut entries = Vec::new();
        if self.config.liquidation.is_some() {
            let mut ordinary = self.receipts.iter();
            for event in &self.marking.events {
                let amount = match event.kind {
                    MarkEventKind::Trade { .. } => Some(
                        ordinary
                            .next()
                            .ok_or(EconomicError::Invariant)?
                            .retained_fee_atoms,
                    ),
                    MarkEventKind::Forced {
                        epoch_id,
                        fill_index,
                        ..
                    } => Some(
                        self.forced_receipt(epoch_id, fill_index)?
                            .retained_fee_atoms,
                    ),
                    MarkEventKind::AmmForced {
                        epoch_id,
                        fill_index,
                        ..
                    } => Some(
                        self.forced_amm_receipt(epoch_id, fill_index)?
                            .retained_fee_atoms,
                    ),
                    MarkEventKind::Mark { .. } => None,
                };
                if let Some(atoms) = amount {
                    entries.push((
                        self.config.collateral,
                        self.config.fee_route,
                        atoms,
                        event.height,
                    ));
                }
            }
            if ordinary.next().is_some() {
                return Err(EconomicError::Invariant);
            }
        } else {
            entries.extend(self.receipts.iter().map(|r| {
                (
                    self.config.collateral,
                    self.config.fee_route,
                    r.retained_fee_atoms,
                    r.height,
                )
            }));
        }
        Ok(FeeBook::from_accruals(entries)?)
    }
    fn verify_forced_mark_entry(
        &self,
        event: &MarkEvent,
        bases: &BTreeMap<AccountId, BasisPosition>,
    ) -> Result<()> {
        let MarkEventKind::Forced {
            epoch_id,
            fill_index,
            maker_txid,
            distressed_owner,
        } = &event.kind
        else {
            return Err(EconomicError::Invariant);
        };
        let parent = self
            .liquidation
            .events
            .iter()
            .find(|e| e.id == *epoch_id)
            .ok_or(EconomicError::Invariant)?;
        let receipt = self.forced_receipt(*epoch_id, *fill_index)?;
        if event.height != parent.height
            || *distressed_owner != parent.owner
            || *maker_txid != receipt.maker_before.order.txid
            || event.entries.len() != 2
            || event.entries[0].owner != parent.owner
            || event.entries[1].owner != receipt.maker_before.order.owner
            || event.entries[0].owner == event.entries[1].owner
            || event.collected_loss_atoms != receipt.collected_loss_atoms
            || event.paid_claim_atoms != receipt.paid_claim_atoms
            || event.unfunded_loss_atoms != receipt.unfunded_loss_atoms
        {
            return Err(EconomicError::Invariant);
        }
        let old = bases.get(&parent.owner).ok_or(EconomicError::Invariant)?;
        if old.quantity.is_zero()
            || old.quantity.is_negative() != parent.quantity_before.is_negative()
            || receipt.quantity > old.quantity.abs()
        {
            return Err(EconomicError::Invariant);
        }
        let delta = if old.quantity.is_negative() {
            receipt.quantity.clone()
        } else {
            receipt.quantity.negated()
        };
        for (entry, delta, pnl) in [
            (&event.entries[0], delta.clone(), &receipt.distressed_pnl),
            (&event.entries[1], delta.negated(), &receipt.maker_pnl),
        ] {
            if entry.action
                != (BasisAction::Trade {
                    delta,
                    price: receipt.price.clone(),
                })
                || entry.atoms.to_decimal() != *pnl
            {
                return Err(EconomicError::Invariant);
            }
        }
        Ok(())
    }
    fn liquidation_positions_before(&self, ordinal: usize) -> Result<BTreeMap<AccountId, Decimal>> {
        let event = self
            .liquidation
            .events
            .get(ordinal)
            .ok_or(EconomicError::Invariant)?;
        let mut positions = BTreeMap::<AccountId, Decimal>::new();
        let indices: BTreeMap<_, _> = self
            .liquidation
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id, i))
            .collect();
        for action in &self.marking.events {
            if action.height > event.height {
                break;
            }
            if action.height == event.height {
                if let MarkEventKind::Forced { epoch_id, .. }
                | MarkEventKind::AmmForced { epoch_id, .. } = action.kind
                {
                    if *indices.get(&epoch_id).ok_or(EconomicError::Invariant)? >= ordinal {
                        continue;
                    }
                }
            }
            for entry in &action.entries {
                if let BasisAction::Trade { delta, .. } = &entry.action {
                    let old = positions.get(&entry.owner).cloned().unwrap_or_default();
                    positions.insert(entry.owner.clone(), old.checked_add(delta)?);
                }
            }
        }
        Ok(positions)
    }
    fn verify_liquidations(&self) -> Result<()> {
        let Some(config) = &self.config.liquidation else {
            return if self.liquidation == LiquidationState::default() {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        };
        self.liquidation_record_count()?;
        self.verify_liquidation_book_history()?;
        let mut seen = BTreeSet::new();
        let mut last: Option<&LiquidationEvent> = None;
        let mut previous_book = None;
        let mut total_fills = 0usize;
        for (ordinal, event) in self.liquidation.events.iter().enumerate() {
            self.participant_owner(&event.owner)?;
            if event.height == 0
                || self.finalized_height().is_none_or(|h| event.height > h)
                || event.id != self.liquidation_id(event.height, &event.owner)
                || !seen.insert(event.id)
                || last.is_some_and(|p| {
                    event.height < p.height || (event.height == p.height && event.owner <= p.owner)
                })
                || !self.joint.system_epoch_ids_unverified().contains(&event.id)
                || event.quantity_before.is_zero()
                || event.required_before_atoms != self.maintenance_atoms(&event.quantity_before)?
                || event.margin_before_atoms >= event.required_before_atoms
                || event.fills.len() + event.amm_fills.len() > config.max_fills_per_owner as usize
            {
                return Err(EconomicError::Invariant);
            }
            let mark = self
                .marking
                .events
                .iter()
                .find_map(|e| {
                    if e.height == event.height {
                        if let MarkEventKind::Mark { id, observation } = &e.kind {
                            if *id == event.mark_id {
                                return Some(observation);
                            }
                        }
                    }
                    None
                })
                .ok_or(EconomicError::Invariant)?;
            if event.guard_price != self.liquidation_guard(&event.quantity_before, &mark.price)? {
                return Err(EconomicError::Invariant);
            }
            let mut positions = self.liquidation_positions_before(ordinal)?;
            if positions.get(&event.owner) != Some(&event.quantity_before) {
                return Err(EconomicError::Invariant);
            }
            let mut book = ContractOrderbook::from_snapshot(event.book_before.clone())?;
            if book.contract() != self.config.contract
                || book.processed_through() != Some(event.height)
                || !book.queued().is_empty()
                || !book.drained().is_empty()
                || event
                    .book_before
                    .seen
                    .iter()
                    .any(|id| !self.book.has_seen(*id))
                || (last.is_some_and(|p| p.height == event.height)
                    && previous_book.as_ref() != Some(&event.book_before))
            {
                return Err(EconomicError::Invariant);
            }
            let registered_pool = self.liquidation_registered_pool()?;
            let cancelled =
                liquidation_cancel_book(&mut book, &event.owner, registered_pool.as_ref())?;
            if cancelled.resting.len() != event.cancelled.len() || !cancelled.queued.is_empty() {
                return Err(EconomicError::Invariant);
            }
            let mut refunded = 0;
            if !self.is_amm_owner(&event.owner) && !event.pool_cancelled_quotes.is_empty() {
                return Err(EconomicError::Invariant);
            }
            for quote in &event.pool_cancelled_quotes {
                refunded = add(refunded, quote.reserve_atoms)?;
            }
            for (order, recorded) in cancelled.resting.iter().zip(&event.cancelled) {
                let atoms = self.required_margin(&order.reserve_quantity)?;
                if *order != recorded.order || atoms != recorded.reserve_atoms {
                    return Err(EconomicError::Invariant);
                }
                refunded = add(refunded, atoms)?;
            }
            let available = add(event.available_before_atoms, refunded)?;
            let topup = available.min(
                self.required_margin(&event.quantity_before.abs())?
                    .saturating_sub(event.margin_before_atoms),
            );
            if event.top_up_atoms != topup {
                return Err(EconomicError::Invariant);
            }
            let mut margin = add(event.margin_before_atoms, topup)?;
            let mut quantity = event.quantity_before.clone();
            let mut first_available = Some(available - topup);
            for (index, receipt) in event.amm_fills.iter().enumerate() {
                if quantity.is_zero()
                    || margin >= self.maintenance_atoms(&quantity)?
                    || self.is_amm_owner(&event.owner)
                {
                    return Err(EconomicError::Invariant);
                }
                let pool = registered_pool.as_ref().ok_or(EconomicError::Invariant)?;
                let pool_quantity = positions.get(pool).cloned().unwrap_or_default();
                if receipt.quantity <= Decimal::zero()
                    || self.amm_quote_fill_quantity(
                        &receipt.quote_before,
                        &quantity,
                        &pool_quantity,
                        &event.guard_price,
                    )? != receipt.quantity
                    || receipt.price != receipt.quote_before.price
                {
                    return Err(EconomicError::Invariant);
                }
                let delta = if quantity.is_negative() {
                    receipt.quantity.clone()
                } else {
                    receipt.quantity.negated()
                };
                quantity = quantity.checked_add(&delta)?;
                let pool_after = pool_quantity.checked_sub(&delta)?;
                positions.insert(event.owner.clone(), quantity.clone());
                positions.insert(pool.clone(), pool_after.clone());
                let forced = self.amm_forced_mark_event(event.id, index as u32)?;
                let target = self.required_margin(&quantity.abs())?;
                let before_fee_margin = margin.min(target);
                let released = margin - before_fee_margin;
                if let Some(a) = first_available.take() {
                    if receipt.distressed_available_before_fee_atoms != add(a, released)? {
                        return Err(EconomicError::Invariant);
                    }
                } else if receipt.distressed_available_before_fee_atoms < released {
                    return Err(EconomicError::Invariant);
                }
                let side = if delta.is_negative() {
                    Side::Sell
                } else {
                    Side::Buy
                };
                let nominal = self.forced_nominal_fee(&receipt.quantity, &receipt.price, side)?;
                let available_fee = nominal.min(receipt.distressed_available_before_fee_atoms);
                let margin_fee = (nominal - available_fee).min(before_fee_margin);
                let charged = add(available_fee, margin_fee)?;
                let quote_left = receipt
                    .quote_before
                    .remaining
                    .checked_sub(&receipt.quantity)?;
                let reserve_used = receipt
                    .quote_before
                    .reserve_atoms
                    .checked_sub(self.required_margin(&quote_left)?)
                    .ok_or(EconomicError::Invariant)?;
                let pool_custody = add(receipt.maker_margin_before_atoms, reserve_used)?;
                let pool_target = self.required_margin(&pool_after.abs())?;
                let pool_available = if pool_custody >= pool_target {
                    add(
                        receipt.maker_available_before_atoms,
                        pool_custody - pool_target,
                    )?
                } else {
                    receipt
                        .maker_available_before_atoms
                        .checked_sub(pool_target - pool_custody)
                        .ok_or(EconomicError::Invariant)?
                };
                if receipt.distressed_margin_before_fee_atoms != before_fee_margin
                    || receipt.nominal_fee_atoms != nominal
                    || receipt.charged_fee_atoms != charged
                    || receipt.maker_rebate_atoms != charged / 2
                    || receipt.retained_fee_atoms != charged - charged / 2
                    || forced.entries.len() != 2
                    || forced.entries[0].available_atoms
                        != receipt.distressed_available_before_fee_atoms - available_fee
                    || forced.entries[0].margin_atoms != before_fee_margin - margin_fee
                    || forced.entries[1].margin_atoms != pool_target
                    || forced.entries[1].available_atoms
                        != add(pool_available, receipt.maker_rebate_atoms)?
                {
                    return Err(EconomicError::Invariant);
                }
                let owner_entry = &forced.entries[0];
                let margin_loss = owner_entry.collected_atoms
                    - owner_entry.collected_atoms.min(owner_entry.available_atoms);
                margin = owner_entry
                    .margin_atoms
                    .checked_sub(margin_loss)
                    .ok_or(EconomicError::Invariant)?;
                if receipt.distressed_margin_after_atoms != margin {
                    return Err(EconomicError::Invariant);
                }
                total_fills = total_fills.checked_add(1).ok_or(EconomicError::Capacity)?;
            }
            for (index, receipt) in event.fills.iter().enumerate() {
                if quantity.is_zero() || margin >= self.maintenance_atoms(&quantity)? {
                    return Err(EconomicError::Invariant);
                }
                let request = self.liquidation_request(event, &quantity);
                let candidate = liquidation_book_candidate(
                    &book,
                    &request,
                    &quantity,
                    registered_pool.as_ref(),
                )?
                .ok_or(EconomicError::Invariant)?;
                if candidate.maker_before != receipt.maker_before
                    || candidate.maximum_quantity != receipt.quantity
                    || receipt.price != candidate.maker_before.order.price
                {
                    return Err(EconomicError::Invariant);
                }
                let maker_owner = &candidate.maker_before.order.owner;
                let maker_old = positions.get(maker_owner).cloned().unwrap_or_default();
                if receipt.maker_available_before_atoms
                    < self.liquidation_maker_required(
                        &candidate,
                        &maker_old,
                        receipt.maker_margin_before_atoms,
                    )?
                {
                    return Err(EconomicError::Invariant);
                }
                let fill = liquidation_consume_book_candidate(
                    &mut book,
                    &candidate,
                    &quantity,
                    &receipt.quantity,
                    registered_pool.as_ref(),
                )?;
                let delta = if quantity.is_negative() {
                    receipt.quantity.clone()
                } else {
                    receipt.quantity.negated()
                };
                quantity = quantity.checked_add(&delta)?;
                let maker_after = maker_old.checked_sub(&delta)?;
                positions.insert(event.owner.clone(), quantity.clone());
                positions.insert(maker_owner.clone(), maker_after.clone());
                let forced = self.forced_mark_event(event.id, index as u32)?;
                let target = self.required_margin(&quantity.abs())?;
                let before_fee_margin = margin.min(target);
                let released = margin - before_fee_margin;
                if let Some(a) = first_available.take() {
                    if receipt.distressed_available_before_fee_atoms != add(a, released)? {
                        return Err(EconomicError::Invariant);
                    }
                } else if receipt.distressed_available_before_fee_atoms < released {
                    return Err(EconomicError::Invariant);
                }
                let nominal = self.forced_nominal_fee(
                    &receipt.quantity,
                    &receipt.price,
                    fill.distressed_side,
                )?;
                let available_fee = nominal.min(receipt.distressed_available_before_fee_atoms);
                let margin_fee = (nominal - available_fee).min(before_fee_margin);
                let charged = add(available_fee, margin_fee)?;
                if receipt.distressed_margin_before_fee_atoms != before_fee_margin
                    || receipt.nominal_fee_atoms != nominal
                    || receipt.charged_fee_atoms != charged
                    || receipt.maker_rebate_atoms != charged / 2
                    || receipt.retained_fee_atoms != charged - charged / 2
                    || forced.entries[0].available_atoms
                        != receipt.distressed_available_before_fee_atoms - available_fee
                    || forced.entries[0].margin_atoms != before_fee_margin - margin_fee
                {
                    return Err(EconomicError::Invariant);
                }
                let reserve_used = self
                    .required_margin(&fill.maker_reserve_before)?
                    .checked_sub(self.required_margin(&fill.maker_reserve_after)?)
                    .ok_or(EconomicError::Invariant)?;
                let maker_custody = add(receipt.maker_margin_before_atoms, reserve_used)?;
                let maker_target = self.required_margin(&maker_after.abs())?;
                let maker_available = if maker_custody >= maker_target {
                    add(
                        receipt.maker_available_before_atoms,
                        maker_custody - maker_target,
                    )?
                } else {
                    receipt
                        .maker_available_before_atoms
                        .checked_sub(maker_target - maker_custody)
                        .ok_or(EconomicError::Invariant)?
                };
                if forced.entries[1].margin_atoms != maker_target
                    || forced.entries[1].available_atoms
                        != add(maker_available, receipt.maker_rebate_atoms)?
                {
                    return Err(EconomicError::Invariant);
                }
                let owner_entry = &forced.entries[0];
                let margin_loss = owner_entry.collected_atoms
                    - owner_entry.collected_atoms.min(owner_entry.available_atoms);
                margin = owner_entry
                    .margin_atoms
                    .checked_sub(margin_loss)
                    .ok_or(EconomicError::Invariant)?;
                if receipt.distressed_margin_after_atoms != margin {
                    return Err(EconomicError::Invariant);
                }
                total_fills = total_fills.checked_add(1).ok_or(EconomicError::Capacity)?;
            }
            let expected = if quantity.is_zero() {
                LiquidationStatus::Closed
            } else if margin >= self.maintenance_atoms(&quantity)? {
                LiquidationStatus::Cured
            } else if event.fills.len() + event.amm_fills.len()
                == config.max_fills_per_owner as usize
            {
                LiquidationStatus::FillLimit
            } else {
                let request = self.liquidation_request(event, &quantity);
                if let Some(candidate) = liquidation_book_candidate(
                    &book,
                    &request,
                    &quantity,
                    registered_pool.as_ref(),
                )? {
                    let blocked = event
                        .blocked_maker
                        .as_ref()
                        .ok_or(EconomicError::Invariant)?;
                    let maker_quantity = positions
                        .get(&blocked.order.order.owner)
                        .cloned()
                        .unwrap_or_default();
                    if blocked.order != candidate.maker_before
                        || blocked.available_atoms
                            >= self.liquidation_maker_required(
                                &candidate,
                                &maker_quantity,
                                blocked.margin_atoms,
                            )?
                    {
                        return Err(EconomicError::Invariant);
                    }
                    LiquidationStatus::UnfundedBestMaker
                } else {
                    LiquidationStatus::NoLiquidityWithinGuard
                }
            };
            if event.status != expected
                || (expected != LiquidationStatus::UnfundedBestMaker
                    && event.blocked_maker.is_some())
            {
                return Err(EconomicError::Invariant);
            }
            previous_book = Some(book.snapshot());
            last = Some(event);
        }
        if total_fills
            != self
                .marking
                .events
                .iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        MarkEventKind::Forced { .. } | MarkEventKind::AmmForced { .. }
                    )
                })
                .count()
        {
            return Err(EconomicError::Invariant);
        }
        let expected_order: Vec<_> = self
            .liquidation
            .events
            .iter()
            .flat_map(|e| {
                (0..e.amm_fills.len())
                    .map(move |i| (e.id, true, i as u32))
                    .chain((0..e.fills.len()).map(move |i| (e.id, false, i as u32)))
            })
            .collect();
        let actual_order: Vec<_> = self
            .marking
            .events
            .iter()
            .filter_map(|e| match e.kind {
                MarkEventKind::Forced {
                    epoch_id,
                    fill_index,
                    ..
                } => Some((epoch_id, false, fill_index)),
                MarkEventKind::AmmForced {
                    epoch_id,
                    fill_index,
                    ..
                } => Some((epoch_id, true, fill_index)),
                _ => None,
            })
            .collect();
        if actual_order != expected_order {
            return Err(EconomicError::Invariant);
        }
        if last.is_some_and(|e| Some(e.height) == self.finalized_height())
            && previous_book != Some(self.book.snapshot())
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}
