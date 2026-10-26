// Canonical ordinary-book and margin history for the liquidation owner.
// This binds stored liquidation evidence to prior computed transitions. It is
// not authentication of actors, oracle claims or historical available wallets.

pub const MAX_BOOK_FRAMES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiquidationBookInput {
    Order(Order),
    Cancel {
        txid: TxId,
        owner: AccountId,
        criteria: CancelCriteria,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationBookPlacement {
    pub order: Order,
    pub position: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiquidationBookFrame {
    pub height: u64,
    pub inputs: Vec<LiquidationBookInput>,
    pub placements: Vec<LiquidationBookPlacement>,
}

impl EconomicState {
    fn verify_liquidation_book_history(&self) -> Result<()> {
        if self.config.liquidation.is_none() {
            return if self.liquidation.book_frames.is_empty() {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        }
        let finalized = self.finalized_height().unwrap_or(0);
        if self.liquidation.book_frames.len() > MAX_BOOK_FRAMES {
            return Err(EconomicError::Capacity);
        }
        if self.liquidation.book_frames.len() as u64 != finalized {
            return Err(EconomicError::Invariant);
        }
        let mut records = 0usize;
        for frame in &self.liquidation.book_frames {
            if frame.inputs.len() > MAX_ECONOMIC_INPUTS
                || frame.placements.len() > MAX_ECONOMIC_INPUTS
            {
                return Err(EconomicError::Capacity);
            }
            records = records
                .checked_add(frame.inputs.len())
                .and_then(|n| n.checked_add(frame.placements.len()))
                .ok_or(EconomicError::Capacity)?;
            if records > MAX_LIQUIDATION_RECORDS {
                return Err(EconomicError::Capacity);
            }
        }
        let mut book = ContractOrderbook::new(self.config.contract);
        let mut positions = BTreeMap::<AccountId, Decimal>::new();
        let mut averages = BTreeMap::<AccountId, Decimal>::new();
        let mut margins = BTreeMap::<AccountId, u128>::new();
        let mut cancellations = BTreeSet::new();
        let mut marks = self.marking.events.iter().peekable();
        let mut ordinary_receipts = self.receipts.iter();
        let mut liquidations = self.liquidation.events.iter().peekable();
        let registered_pool = self.liquidation_registered_pool()?;
        let mut amm_quotes = Vec::<FundedAmmQuote>::new();
        for (index, frame) in self.liquidation.book_frames.iter().enumerate() {
            if frame.height != index as u64 + 1 {
                return Err(EconomicError::Invariant);
            }
            let amm_frame = if registered_pool.is_some() {
                let captured = self.amm.frames.get(index).ok_or(EconomicError::Invariant)?;
                if captured.height != frame.height || captured.released_quotes != amm_quotes {
                    return Err(EconomicError::Invariant);
                }
                amm_quotes.clear();
                Some(captured)
            } else {
                None
            };
            for input in &frame.inputs {
                match input {
                    LiquidationBookInput::Order(order) => {
                        if order.height != frame.height || cancellations.contains(&order.txid) {
                            return Err(EconomicError::Invariant);
                        }
                        book.queue(order.clone())?;
                    }
                    LiquidationBookInput::Cancel {
                        txid,
                        owner,
                        criteria,
                    } => {
                        if book.has_seen(*txid) || !cancellations.insert(*txid) {
                            return Err(EconomicError::Invariant);
                        }
                        normal_owner(owner)?;
                        book.cancel(owner, criteria.clone())?;
                    }
                }
            }
            let drained = book.drain_queued_through(frame.height)?;
            if drained.len() != frame.placements.len() {
                return Err(EconomicError::Invariant);
            }
            for (order, placement) in drained.into_iter().zip(&frame.placements) {
                let position = positions.get(&order.owner).cloned().unwrap_or_default();
                if placement.order != order || placement.position != position {
                    return Err(EconomicError::Invariant);
                }
                let transition = book.place(order, &position)?;
                for matched in transition.matches {
                    let receipt = ordinary_receipts.next().ok_or(EconomicError::Invariant)?;
                    let fee = calculate_match_fees(&MatchFeeInput {
                        amount_buy: matched.quantity.clone(),
                        amount_sell: matched.quantity.clone(),
                        buy_maker: maker(matched.buy_maker),
                        sell_maker: maker(matched.sell_maker),
                        inverse: self.config.inverse,
                        last_mark: matched.price.clone(),
                        notional_value: self.fee_input_notional(&matched.price)?,
                        channel: false,
                    })?;
                    if receipt.height != frame.height
                        || receipt.buy_txid != matched.buy_txid
                        || receipt.sell_txid != matched.sell_txid
                        || receipt.quantity != matched.quantity
                        || receipt.price != matched.price
                        || receipt.retained_fee_atoms != fee.retained_atoms
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let event = marks.next().ok_or(EconomicError::Invariant)?;
                    if event.height != frame.height
                        || event.kind
                            != (MarkEventKind::Trade {
                                buy_txid: matched.buy_txid,
                                sell_txid: matched.sell_txid,
                            })
                        || event.entries.len() != 2
                        || event.entries[0].owner != matched.buyer
                        || event.entries[1].owner != matched.seller
                    {
                        return Err(EconomicError::Invariant);
                    }
                    for (entry, delta) in [
                        (&event.entries[0], matched.quantity.clone()),
                        (&event.entries[1], matched.quantity.negated()),
                    ] {
                        if entry.action
                            != (BasisAction::Trade {
                                delta: delta.clone(),
                                price: matched.price.clone(),
                            })
                        {
                            return Err(EconomicError::Invariant);
                        }
                        let before = positions.get(&entry.owner).cloned().unwrap_or_default();
                        let after = before.checked_add(&delta)?;
                        let target = self.margin_target_after_trade(
                            &before,
                            &after,
                            liquidation_replay_margin(&margins, &entry.owner),
                            &matched.price,
                        )?;
                        liquidation_replay_trade(
                            &mut positions,
                            &mut averages,
                            &entry.owner,
                            &delta,
                            &matched.price,
                        )?;
                        liquidation_replay_set_margin(&mut margins, &entry.owner, target);
                    }
                    liquidation_replay_margin_loss(&mut margins, event)?;
                }
            }
            book.finish_height(frame.height)?;
            let mut mark_id = None;
            if marks.peek().is_some_and(|event| {
                event.height == frame.height && matches!(event.kind, MarkEventKind::Mark { .. })
            }) {
                let event = marks.next().ok_or(EconomicError::Invariant)?;
                if let MarkEventKind::Mark { id, .. } = event.kind {
                    mark_id = Some(id);
                }
                liquidation_replay_margin_loss(&mut margins, event)?;
            }
            // Funding is deliberately between Mark and Forced. It moves only
            // available balances, so it does not alter either replayed map.
            if let Some(captured) = amm_frame {
                match (&captured.quote_epoch, mark_id) {
                    (Some(epoch), Some(id)) => {
                        let pool = registered_pool.as_ref().ok_or(EconomicError::Invariant)?;
                        if epoch.mark_id != id
                            || epoch.height != frame.height
                            || epoch.position_before
                                != positions.get(pool).cloned().unwrap_or_default()
                            || epoch.margin_before_atoms
                                != liquidation_replay_margin(&margins, pool)
                            || epoch.average_price_before
                                != if self.config.contract_binding.is_some() {
                                    averages.get(pool).cloned()
                                } else {
                                    None
                                }
                        {
                            return Err(EconomicError::Invariant);
                        }
                        amm_quotes = epoch.quotes.clone();
                    }
                    (None, None) => {}
                    _ => return Err(EconomicError::Invariant),
                }
            }
            // Runtime snapshots the initial maintenance set once, in sorted
            // owner order. Earlier forced fills can then cure/flatten a later
            // target; newly distressed makers are left for the next sweep.
            let mut targets = Vec::new();
            if mark_id.is_some() {
                for (owner, quantity) in &positions {
                    if !quantity.is_zero()
                        && liquidation_replay_margin(&margins, owner)
                            < self.maintenance_at(quantity, averages.get(owner))?
                    {
                        targets.push(owner.clone());
                    }
                }
            }
            for target in targets {
                let quantity = positions.get(&target).ok_or(EconomicError::Invariant)?;
                if quantity.is_zero()
                    || liquidation_replay_margin(&margins, &target)
                        >= self.maintenance_at(quantity, averages.get(&target))?
                {
                    continue;
                }
                let event = liquidations.next().ok_or(EconomicError::Invariant)?;
                if event.height != frame.height
                    || event.owner != target
                    || Some(event.mark_id) != mark_id
                    || book.snapshot() != event.book_before
                    || positions.get(&event.owner) != Some(&event.quantity_before)
                    || liquidation_replay_margin(&margins, &event.owner)
                        != event.margin_before_atoms
                    || event.average_price_before
                        != if self.config.contract_binding.is_some() {
                            averages.get(&event.owner).cloned()
                        } else {
                            None
                        }
                {
                    return Err(EconomicError::Invariant);
                }
                if registered_pool.as_ref() == Some(&event.owner) {
                    if event.pool_cancelled_quotes != amm_quotes {
                        return Err(EconomicError::Invariant);
                    }
                    amm_quotes.clear();
                } else if !event.pool_cancelled_quotes.is_empty() {
                    return Err(EconomicError::Invariant);
                }
                let cancelled =
                    liquidation_cancel_book(&mut book, &event.owner, registered_pool.as_ref())?;
                if !cancelled.queued.is_empty() || cancelled.resting.len() != event.cancelled.len()
                {
                    return Err(EconomicError::Invariant);
                }
                for (order, capture) in cancelled.resting.iter().zip(&event.cancelled) {
                    if order != &capture.order
                        || self.required_margin_at(&order.reserve_quantity, &order.order.price)?
                            != capture.reserve_atoms
                    {
                        return Err(EconomicError::Invariant);
                    }
                }
                let margin = add(event.margin_before_atoms, event.top_up_atoms)?;
                liquidation_replay_set_margin(&mut margins, &event.owner, margin);
                for (fill_index, receipt) in event.amm_fills.iter().enumerate() {
                    let old_quantity = positions
                        .get(&event.owner)
                        .cloned()
                        .ok_or(EconomicError::Invariant)?;
                    if old_quantity.is_zero()
                        || liquidation_replay_margin(&margins, &event.owner)
                            >= self.maintenance_at(&old_quantity, averages.get(&event.owner))?
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let pool = registered_pool.as_ref().ok_or(EconomicError::Invariant)?;
                    let pool_quantity = positions.get(pool).cloned().unwrap_or_default();
                    let (quote_index, amount) = self
                        .liquidation_replay_amm_candidate(
                            &amm_quotes,
                            event,
                            &old_quantity,
                            &pool_quantity,
                        )?
                        .ok_or(EconomicError::Invariant)?;
                    let quote = amm_quotes
                        .get(quote_index)
                        .ok_or(EconomicError::Invariant)?
                        .clone();
                    if quote != receipt.quote_before
                        || receipt.quantity != amount
                        || receipt.price != quote.price
                        || receipt.maker_margin_before_atoms
                            != liquidation_replay_margin(&margins, pool)
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let delta = if old_quantity.is_negative() {
                        amount.clone()
                    } else {
                        amount.negated()
                    };
                    let after = old_quantity.checked_add(&delta)?;
                    let pool_after = pool_quantity.checked_sub(&delta)?;
                    let pool_target = self.margin_target_after_trade(
                        &pool_quantity,
                        &pool_after,
                        liquidation_replay_margin(&margins, pool),
                        &receipt.price,
                    )?;
                    liquidation_replay_set_margin(&mut margins, pool, pool_target);
                    let distressed_before = liquidation_replay_margin(&margins, &event.owner);
                    let before_fee = distressed_before.min(self.margin_target_after_trade(
                        &old_quantity,
                        &after,
                        distressed_before,
                        &receipt.price,
                    )?);
                    let available_fee = receipt
                        .nominal_fee_atoms
                        .min(receipt.distressed_available_before_fee_atoms);
                    let margin_fee = (receipt.nominal_fee_atoms - available_fee).min(before_fee);
                    if receipt.distressed_margin_before_fee_atoms != before_fee
                        || receipt.charged_fee_atoms != add(available_fee, margin_fee)?
                    {
                        return Err(EconomicError::Invariant);
                    }
                    liquidation_replay_set_margin(
                        &mut margins,
                        &event.owner,
                        before_fee - margin_fee,
                    );
                    liquidation_replay_trade(
                        &mut positions,
                        &mut averages,
                        &event.owner,
                        &delta,
                        &receipt.price,
                    )?;
                    liquidation_replay_trade(
                        &mut positions,
                        &mut averages,
                        pool,
                        &delta.negated(),
                        &receipt.price,
                    )?;
                    let forced = marks.next().ok_or(EconomicError::Invariant)?;
                    if forced.height != frame.height
                        || forced.kind
                            != (MarkEventKind::AmmForced {
                                epoch_id: event.id,
                                fill_index: fill_index as u32,
                                pool_id: quote.pool_id,
                                distressed_owner: event.owner.clone(),
                            })
                        || forced.entries.len() != 2
                        || forced.entries[0].owner != event.owner
                        || forced.entries[1].owner != *pool
                    {
                        return Err(EconomicError::Invariant);
                    }
                    liquidation_replay_margin_loss(&mut margins, forced)?;
                    if receipt.distressed_margin_after_atoms
                        != liquidation_replay_margin(&margins, &event.owner)
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let remaining = quote.remaining.checked_sub(&amount)?;
                    if remaining.is_zero() {
                        amm_quotes.remove(quote_index);
                    } else {
                        amm_quotes[quote_index].reserve_atoms =
                            self.required_margin_at(&remaining, &quote.price)?;
                        amm_quotes[quote_index].remaining = remaining;
                    }
                }
                for (fill_index, receipt) in event.fills.iter().enumerate() {
                    let old_quantity = positions
                        .get(&event.owner)
                        .cloned()
                        .ok_or(EconomicError::Invariant)?;
                    let request = self.liquidation_request(event, &old_quantity);
                    let pool_quantity = registered_pool
                        .as_ref()
                        .and_then(|pool| positions.get(pool))
                        .cloned()
                        .unwrap_or_default();
                    if self
                        .liquidation_replay_amm_candidate(
                            &amm_quotes,
                            event,
                            &old_quantity,
                            &pool_quantity,
                        )?
                        .is_some()
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let candidate = liquidation_book_candidate(
                        &book,
                        &request,
                        &old_quantity,
                        registered_pool.as_ref(),
                    )?
                    .ok_or(EconomicError::Invariant)?;
                    if candidate.maker_before != receipt.maker_before
                        || candidate.maximum_quantity != receipt.quantity
                        || receipt.maker_margin_before_atoms
                            != liquidation_replay_margin(
                                &margins,
                                &candidate.maker_before.order.owner,
                            )
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let fill = liquidation_consume_book_candidate(
                        &mut book,
                        &candidate,
                        &old_quantity,
                        &receipt.quantity,
                        registered_pool.as_ref(),
                    )?;
                    let delta = if fill.distressed_side == Side::Buy {
                        fill.quantity.clone()
                    } else {
                        fill.quantity.negated()
                    };
                    let after = old_quantity.checked_add(&delta)?;
                    let maker_old = positions
                        .get(&fill.maker_owner)
                        .cloned()
                        .unwrap_or_default();
                    let maker_after = maker_old.checked_sub(&delta)?;
                    let maker_target = self.margin_target_after_trade(
                        &maker_old,
                        &maker_after,
                        liquidation_replay_margin(&margins, &fill.maker_owner),
                        &receipt.price,
                    )?;
                    liquidation_replay_set_margin(&mut margins, &fill.maker_owner, maker_target);
                    let distressed_before = liquidation_replay_margin(&margins, &event.owner);
                    let before_fee = distressed_before.min(self.margin_target_after_trade(
                        &old_quantity,
                        &after,
                        distressed_before,
                        &receipt.price,
                    )?);
                    let available_fee = receipt
                        .nominal_fee_atoms
                        .min(receipt.distressed_available_before_fee_atoms);
                    let margin_fee = (receipt.nominal_fee_atoms - available_fee).min(before_fee);
                    if receipt.distressed_margin_before_fee_atoms != before_fee
                        || receipt.charged_fee_atoms != add(available_fee, margin_fee)?
                    {
                        return Err(EconomicError::Invariant);
                    }
                    liquidation_replay_set_margin(
                        &mut margins,
                        &event.owner,
                        before_fee - margin_fee,
                    );
                    liquidation_replay_trade(
                        &mut positions,
                        &mut averages,
                        &event.owner,
                        &delta,
                        &receipt.price,
                    )?;
                    liquidation_replay_trade(
                        &mut positions,
                        &mut averages,
                        &fill.maker_owner,
                        &delta.negated(),
                        &receipt.price,
                    )?;
                    let forced = marks.next().ok_or(EconomicError::Invariant)?;
                    if forced.height != frame.height
                        || forced.kind
                            != (MarkEventKind::Forced {
                                epoch_id: event.id,
                                fill_index: fill_index as u32,
                                maker_txid: fill.maker_txid,
                                distressed_owner: event.owner.clone(),
                            })
                        || forced.entries.len() != 2
                        || forced.entries[0].owner != event.owner
                        || forced.entries[1].owner != fill.maker_owner
                    {
                        return Err(EconomicError::Invariant);
                    }
                    liquidation_replay_margin_loss(&mut margins, forced)?;
                    if receipt.distressed_margin_after_atoms
                        != liquidation_replay_margin(&margins, &event.owner)
                    {
                        return Err(EconomicError::Invariant);
                    }
                }
                let quantity = positions
                    .get(&event.owner)
                    .ok_or(EconomicError::Invariant)?;
                let pool_quantity = registered_pool
                    .as_ref()
                    .and_then(|pool| positions.get(pool))
                    .cloned()
                    .unwrap_or_default();
                if !quantity.is_zero()
                    && liquidation_replay_margin(&margins, &event.owner)
                        < self.maintenance_at(quantity, averages.get(&event.owner))?
                    && event.amm_fills.len() + event.fills.len()
                        < self
                            .config
                            .liquidation
                            .as_ref()
                            .ok_or(EconomicError::Invariant)?
                            .max_fills_per_owner as usize
                    && self
                        .liquidation_replay_amm_candidate(
                            &amm_quotes,
                            event,
                            quantity,
                            &pool_quantity,
                        )?
                        .is_some()
                {
                    return Err(EconomicError::Invariant);
                }
                if event.blocked_maker.as_ref().is_some_and(|blocked| {
                    blocked.margin_atoms
                        != liquidation_replay_margin(&margins, &blocked.order.order.owner)
                }) {
                    return Err(EconomicError::Invariant);
                }
            }
            if marks
                .peek()
                .is_some_and(|event| event.height <= frame.height)
                || liquidations
                    .peek()
                    .is_some_and(|event| event.height <= frame.height)
            {
                return Err(EconomicError::Invariant);
            }
        }
        let actual_positions: BTreeMap<_, _> = self
            .positions
            .positions()
            .map(|position| (position.address.clone(), position.contracts.clone()))
            .collect();
        let actual_averages: BTreeMap<_, _> = self
            .positions
            .positions()
            .filter(|position| !position.contracts.is_zero())
            .map(|position| {
                Ok((
                    position.address.clone(),
                    position.avg_price.clone().ok_or(EconomicError::Invariant)?,
                ))
            })
            .collect::<Result<_>>()?;
        if marks.next().is_some()
            || ordinary_receipts.next().is_some()
            || liquidations.next().is_some()
            || book != self.book
            || positions != actual_positions
            || averages != actual_averages
            || margins != self.margins
            || cancellations != self.cancel_ids
            || amm_quotes != self.amm.quotes
            || (registered_pool.is_some()
                && self.amm.frames.len() != self.liquidation.book_frames.len())
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
    fn liquidation_replay_amm_candidate(
        &self,
        quotes: &[FundedAmmQuote],
        event: &LiquidationEvent,
        quantity: &Decimal,
        pool_quantity: &Decimal,
    ) -> Result<Option<(usize, Decimal)>> {
        if self.config.amm.is_none() || self.is_amm_owner(&event.owner) {
            return Ok(None);
        }
        for (index, quote) in quotes.iter().enumerate() {
            let amount =
                self.amm_quote_fill_quantity(quote, quantity, pool_quantity, &event.guard_price)?;
            if !amount.is_zero() {
                return Ok(Some((index, amount)));
            }
        }
        Ok(None)
    }
}

fn liquidation_replay_trade(
    positions: &mut BTreeMap<AccountId, Decimal>,
    averages: &mut BTreeMap<AccountId, Decimal>,
    owner: &AccountId,
    delta: &Decimal,
    price: &Decimal,
) -> Result<()> {
    let before = positions.get(owner).cloned().unwrap_or_default();
    let average = mark_trade_average(&before, averages.get(owner), delta, price)?;
    positions.insert(owner.clone(), before.checked_add(delta)?);
    if let Some(average) = average {
        averages.insert(owner.clone(), average);
    } else {
        averages.remove(owner);
    }
    Ok(())
}

fn liquidation_replay_margin(margins: &BTreeMap<AccountId, u128>, owner: &AccountId) -> u128 {
    margins.get(owner).copied().unwrap_or(0)
}
fn liquidation_replay_set_margin(
    margins: &mut BTreeMap<AccountId, u128>,
    owner: &AccountId,
    atoms: u128,
) {
    if atoms == 0 {
        margins.remove(owner);
    } else {
        margins.insert(owner.clone(), atoms);
    }
}
fn liquidation_replay_margin_loss(
    margins: &mut BTreeMap<AccountId, u128>,
    event: &MarkEvent,
) -> Result<()> {
    for entry in &event.entries {
        let before = liquidation_replay_margin(margins, &entry.owner);
        if entry.margin_atoms != before {
            return Err(EconomicError::Invariant);
        }
        let from_margin = entry
            .collected_atoms
            .checked_sub(entry.collected_atoms.min(entry.available_atoms))
            .ok_or(EconomicError::Invariant)?;
        let after = before
            .checked_sub(from_margin)
            .ok_or(EconomicError::Invariant)?;
        liquidation_replay_set_margin(margins, &entry.owner, after);
    }
    Ok(())
}

#[cfg(test)]
mod liquidation_average_replay_tests {
    use super::*;

    #[test]
    fn fractional_history_preserves_reduction_average_and_resets_on_flip_and_close() {
        let owner = AccountId::try_from("alice").unwrap();
        let decimal = |text: &str| text.parse::<Decimal>().unwrap();
        let mut quantities = BTreeMap::new();
        let mut averages = BTreeMap::new();
        for (delta, price, quantity, average) in [
            ("0.5", "100", "0.5", Some("100")),
            ("0.5", "120", "1", Some("110")),
            ("-0.25", "80", "0.75", Some("110")),
            ("-1", "90", "-0.25", Some("90")),
            ("0.25", "200", "0", None),
        ] {
            liquidation_replay_trade(
                &mut quantities,
                &mut averages,
                &owner,
                &decimal(delta),
                &decimal(price),
            )
            .unwrap();
            assert_eq!(quantities.get(&owner), Some(&decimal(quantity)));
            assert_eq!(averages.get(&owner).cloned(), average.map(decimal));
        }
    }
}
