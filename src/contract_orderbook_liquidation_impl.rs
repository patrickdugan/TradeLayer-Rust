// A named exact, reduce-only system API, separate from ordinary source matching.
// Only the complete economic owner registers protected IDs and funds/settles
// these candidates. No external order, Ledger or system replay store lives here.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LiquidationId(TxId);
impl LiquidationId {
    /// Metadata identity supplied by the complete owner's protected system
    /// epoch path, not an external input or authorization claim.
    pub(crate) fn new_unverified(id: TxId) -> Self {
        Self(id)
    }
    pub(crate) fn txid(self) -> TxId {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutionPriceGuard {
    SellAtLeast(Decimal),
    BuyAtMost(Decimal),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReduceOnlyRequest {
    pub id: LiquidationId,
    pub height: u64,
    pub owner: AccountId,
    pub side: Side,
    pub requested_quantity: Decimal,
    pub guard: ExecutionPriceGuard,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReduceOnlyCandidate {
    pub request: ReduceOnlyRequest,
    pub maker_before: RestingOrder,
    pub maximum_quantity: Decimal,
    /// Exact position freshness token, even if the requested cap is smaller.
    pub position_before: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReduceOnlyFill {
    pub id: LiquidationId,
    pub height: u64,
    pub contract: ContractId,
    pub distressed_owner: AccountId,
    pub distressed_side: Side,
    pub maker_owner: AccountId,
    pub maker_txid: TxId,
    pub maker_sequence: u64,
    pub quantity: Decimal,
    pub price: Decimal,
    pub maker_remaining: Decimal,
    pub maker_reserve_before: Decimal,
    pub maker_reserve_after: Decimal,
    pub maker_reserve_used: Decimal,
}

impl ContractOrderbook {
    /// Select one real, opposite firm-limit maker after ordinary processing.
    /// Existing price/height/sequence priority applies. Self and market orders
    /// are excluded without cancelling them. An outside-guard best price gives
    /// None: no inferior price can meet the same monotone inclusive guard.
    pub(crate) fn next_reduce_only_candidate(
        &self,
        request: &ReduceOnlyRequest,
        current_distressed_position: &Decimal,
    ) -> Result<Option<ReduceOnlyCandidate>> {
        self.validate_reduce_only_request(request, current_distressed_position)?;
        let maker_side = match request.side {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        };
        let maker = self
            .resting
            .iter()
            .filter(|maker| {
                maker.order.side == maker_side
                    && maker.order.owner != request.owner
                    && !maker.order.is_market
                    && maker.order.height <= request.height
            })
            .min_by(|left, right| priority(left, right, maker_side));
        let Some(maker) = maker else {
            return Ok(None);
        };
        let guarded = match &request.guard {
            ExecutionPriceGuard::SellAtLeast(minimum) => maker.order.price >= *minimum,
            ExecutionPriceGuard::BuyAtMost(maximum) => maker.order.price <= *maximum,
        };
        if !guarded {
            return Ok(None);
        }
        let maximum_quantity = request
            .requested_quantity
            .clone()
            .min(current_distressed_position.abs())
            .min(maker.remaining.clone());
        positive(&maximum_quantity)?;
        Ok(Some(ReduceOnlyCandidate {
            request: request.clone(),
            maker_before: maker.clone(),
            maximum_quantity,
            position_before: current_distressed_position.clone(),
        }))
    }

    /// Consume a freshly reselected candidate only after the outer owner has
    /// staged this exact funded quantity. Actual reserve atom movement is
    /// old_atoms-ceil(new_reserve_quantity*IM), performed by that owner.
    /// This call never inserts an order, admission intent, external ID, sequence
    /// or residual system order. The outer owner bounds the full fill loop.
    pub(crate) fn consume_reduce_only_candidate(
        &mut self,
        candidate: &ReduceOnlyCandidate,
        current_distressed_position: &Decimal,
        funded_quantity: &Decimal,
    ) -> Result<ReduceOnlyFill> {
        if &candidate.position_before != current_distressed_position {
            return Err(BookError::StaleLiquidationCandidate);
        }
        positive(funded_quantity)?;
        let fresh =
            self.next_reduce_only_candidate(&candidate.request, current_distressed_position)?;
        if fresh.as_ref() != Some(candidate) {
            return Err(BookError::StaleLiquidationCandidate);
        }
        if funded_quantity > &candidate.maximum_quantity
            || funded_quantity > &current_distressed_position.abs()
        {
            return Err(BookError::InvalidReduceOnly);
        }
        let maker = &candidate.maker_before;
        let remaining = maker.remaining.checked_sub(funded_quantity)?;
        nonnegative(&remaining)?;
        let reserve_after = maker.reserve_quantity.clone().min(remaining.clone());
        let reserve_used = maker.reserve_quantity.checked_sub(&reserve_after)?;
        nonnegative(&reserve_after)?;
        nonnegative(&reserve_used)?;
        let fill = ReduceOnlyFill {
            id: candidate.request.id,
            height: candidate.request.height,
            contract: self.contract,
            distressed_owner: candidate.request.owner.clone(),
            distressed_side: candidate.request.side,
            maker_owner: maker.order.owner.clone(),
            maker_txid: maker.order.txid,
            maker_sequence: maker.sequence,
            quantity: funded_quantity.clone(),
            price: maker.order.price.clone(),
            maker_remaining: remaining.clone(),
            maker_reserve_before: maker.reserve_quantity.clone(),
            maker_reserve_after: reserve_after.clone(),
            maker_reserve_used: reserve_used,
        };
        // Everything fallible completed above. A full clone also protects this
        // operation if a future invariant adds a fallible mutation step.
        let mut staged = self.clone();
        let index = staged
            .resting
            .iter()
            .position(|entry| entry == maker)
            .ok_or(BookError::StaleLiquidationCandidate)?;
        if remaining.is_zero() {
            staged.resting.remove(index);
        } else {
            staged.resting[index].remaining = remaining;
            staged.resting[index].reserve_quantity = reserve_after;
        }
        *self = staged;
        Ok(fill)
    }

    fn validate_reduce_only_request(
        &self,
        request: &ReduceOnlyRequest,
        current_position: &Decimal,
    ) -> Result<()> {
        validate_owner(&request.owner)?;
        validate_height(request.height)?;
        if request.height == 0
            || self.processed_through != Some(request.height)
            || !self.queued.is_empty()
            || !self.drained.is_empty()
        {
            return Err(BookError::InvalidHeight);
        }
        if self.seen.contains(&request.id.txid()) {
            return Err(BookError::DuplicateInput);
        }
        finite(current_position)?;
        positive(&request.requested_quantity)?;
        if current_position.is_zero()
            || request.requested_quantity > current_position.abs()
            || (request.side == Side::Buy) != current_position.is_negative()
        {
            return Err(BookError::InvalidReduceOnly);
        }
        match (&request.guard, request.side) {
            (ExecutionPriceGuard::BuyAtMost(value), Side::Buy)
            | (ExecutionPriceGuard::SellAtLeast(value), Side::Sell) => positive(value),
            _ => Err(BookError::InvalidReduceOnly),
        }
    }
}

#[cfg(test)]
mod liquidation_book_tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        text.parse().unwrap()
    }
    fn account(text: &str) -> AccountId {
        AccountId::try_from(text).unwrap()
    }
    fn id(number: u64) -> TxId {
        let mut bytes = [0; 32];
        bytes[24..].copy_from_slice(&number.to_be_bytes());
        TxId::from_bytes(bytes)
    }
    #[allow(clippy::too_many_arguments)]
    fn order(
        number: u64,
        owner: &str,
        side: Side,
        quantity: &str,
        price: &str,
        height: u64,
        market: bool,
    ) -> Order {
        Order {
            txid: id(number),
            owner: account(owner),
            contract: ContractId::try_from(1).unwrap(),
            height,
            side,
            quantity: d(quantity),
            price: d(price),
            post: false,
            is_market: market,
        }
    }
    fn request(side: Side, quantity: &str, guard: &str) -> ReduceOnlyRequest {
        ReduceOnlyRequest {
            id: LiquidationId::new_unverified(id(999)),
            height: 10,
            owner: account("distressed"),
            side,
            requested_quantity: d(quantity),
            guard: if side == Side::Buy {
                ExecutionPriceGuard::BuyAtMost(d(guard))
            } else {
                ExecutionPriceGuard::SellAtLeast(d(guard))
            },
        }
    }
    fn book(orders: &[Order]) -> ContractOrderbook {
        let mut book = ContractOrderbook::new(ContractId::try_from(1).unwrap());
        for order in orders {
            book.place(order.clone(), &d("0")).unwrap();
        }
        book.finish_height(10).unwrap();
        book
    }
    fn candidate(
        book: &ContractOrderbook,
        request: &ReduceOnlyRequest,
        position: &str,
    ) -> ReduceOnlyCandidate {
        book.next_reduce_only_candidate(request, &d(position))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn firm_makers_follow_price_height_sequence_and_inclusive_guards() {
        for side in [Side::Buy, Side::Sell] {
            let maker_side = if side == Side::Buy {
                Side::Sell
            } else {
                Side::Buy
            };
            let price = if side == Side::Buy { "99" } else { "101" };
            let worse = "100";
            let mut book = book(&[
                order(1, "worse", maker_side, "1", worse, 1, false),
                order(2, "laterheight", maker_side, "1", price, 3, false),
                order(3, "firstsequence", maker_side, "1", price, 2, false),
                order(4, "secondsequence", maker_side, "1", price, 2, false),
            ]);
            let position = if side == Side::Buy { "-3" } else { "3" };
            let request = request(side, "1", price);
            for expected in [3, 4, 2] {
                let candidate = candidate(&book, &request, position);
                assert_eq!(candidate.maker_before.order.txid, id(expected));
                let fill = book
                    .consume_reduce_only_candidate(&candidate, &d(position), &d("1"))
                    .unwrap();
                assert_eq!(fill.price, d(price));
                assert_eq!(fill.quantity, d("1"));
            }
            assert!(book
                .next_reduce_only_candidate(&request, &d(position))
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn self_market_and_future_orders_cannot_supply_forced_liquidity() {
        let mut book = book(&[
            order(1, "distressed", Side::Buy, "1", "999", 1, false),
            order(2, "market", Side::Buy, "1", "1000", 1, true),
            order(3, "firm", Side::Buy, "1", "100", 1, false),
            order(4, "future", Side::Buy, "1", "1001", 11, false),
        ]);
        let request = request(Side::Sell, "1", "100");
        let candidate = candidate(&book, &request, "1");
        assert_eq!(candidate.maker_before.order.txid, id(3));
        book.consume_reduce_only_candidate(&candidate, &d("1"), &d("1"))
            .unwrap();
        assert_eq!(book.resting.len(), 3);
        assert!(book
            .next_reduce_only_candidate(&request, &d("1"))
            .unwrap()
            .is_none());
        assert!(book
            .resting
            .iter()
            .any(|o| o.order.owner == account("distressed")));
        assert!(book.resting.iter().any(|o| o.order.is_market));
    }

    #[test]
    fn candidate_is_reselected_and_full_position_and_maker_freshness_are_checked() {
        let mut book = book(&[order(1, "maker", Side::Buy, "2", "100", 1, false)]);
        let request = request(Side::Sell, "0.5", "100");
        let old = candidate(&book, &request, "2");
        let saved = book.clone();
        assert_eq!(
            book.consume_reduce_only_candidate(&old, &d("1.5"), &d("0.5")),
            Err(BookError::StaleLiquidationCandidate)
        );
        assert_eq!(book, saved);
        for mutate in 0..3 {
            let mut changed = old.clone();
            match mutate {
                0 => changed.maximum_quantity = d("1"),
                1 => changed.maker_before.reserve_quantity = d("0"),
                _ => changed.maker_before.sequence += 1,
            }
            assert_eq!(
                book.consume_reduce_only_candidate(&changed, &d("2"), &d("0.5")),
                Err(BookError::StaleLiquidationCandidate)
            );
            assert_eq!(book, saved);
        }
        // Reconstitute a changed completed phase: another ordinary maker was
        // admitted before completion, after this candidate was projected.
        book.processed_through = Some(9);
        book.place(
            order(2, "better", Side::Buy, "1", "101", 10, false),
            &d("0"),
        )
        .unwrap();
        book.finish_height(10).unwrap();
        let changed_book = book.clone();
        assert_eq!(
            book.consume_reduce_only_candidate(&old, &d("2"), &d("0.5")),
            Err(BookError::StaleLiquidationCandidate)
        );
        assert_eq!(book, changed_book);
        let next = candidate(&book, &request, "2");
        book.consume_reduce_only_candidate(&next, &d("2"), &d("0.5"))
            .unwrap();
        let changed_book = book.clone();
        assert_eq!(
            book.consume_reduce_only_candidate(&next, &d("2"), &d("0.5")),
            Err(BookError::StaleLiquidationCandidate)
        );
        assert_eq!(book, changed_book);
    }

    #[test]
    fn exact_fractional_reserve_consumption_preserves_reducing_prefix_and_identity() {
        let mut book = ContractOrderbook::new(ContractId::try_from(1).unwrap());
        book.place(
            order(1, "maker", Side::Buy, "1", "100", 1, false),
            &d("-0.6"),
        )
        .unwrap();
        book.finish_height(10).unwrap();
        let sequence = book.next_sequence;
        let seen = book.seen.clone();
        let original = book.resting[0].clone();
        assert_eq!(original.reserve_quantity, d("0.4"));
        let mut position = d("1");
        for (quantity, left, reserve, used) in [
            ("0.3", "0.7", "0.4", "0"),
            ("0.5", "0.2", "0.2", "0.2"),
            ("0.2", "0", "0", "0.2"),
        ] {
            let request = request(Side::Sell, &position.to_string(), "100");
            let candidate = book
                .next_reduce_only_candidate(&request, &position)
                .unwrap()
                .unwrap();
            let fill = book
                .consume_reduce_only_candidate(&candidate, &position, &d(quantity))
                .unwrap();
            assert_eq!(fill.maker_remaining, d(left));
            assert_eq!(fill.maker_reserve_after, d(reserve));
            assert_eq!(fill.maker_reserve_used, d(used));
            assert_eq!(fill.maker_sequence, original.sequence);
            position = position.checked_sub(&d(quantity)).unwrap();
            if !position.is_zero() {
                assert_eq!(book.resting[0].order, original.order);
                assert_eq!(book.resting[0].initial_reduce, original.initial_reduce);
            }
            assert_eq!(book.seen, seen);
            assert_eq!(book.next_sequence, sequence);
            assert_eq!(book.processed_through, Some(10));
            assert!(!book.has_seen(id(999)));
            assert_eq!(
                ContractOrderbook::from_snapshot(book.snapshot()).unwrap(),
                book
            );
        }
        assert!(book.resting.is_empty());
        assert!(position.is_zero());
    }

    #[test]
    fn exact_fractional_fill_avoids_number_rounding_and_never_flips_distressed_side() {
        let mut book = book(&[order(
            1,
            "maker",
            Side::Sell,
            "0.30000000000000000001",
            "7",
            1,
            false,
        )]);
        let request = request(Side::Buy, "0.10000000000000000001", "7");
        let candidate = candidate(&book, &request, "-0.10000000000000000001");
        let saved = book.clone();
        assert_eq!(
            book.consume_reduce_only_candidate(
                &candidate,
                &candidate.position_before,
                &d("0.10000000000000000002")
            ),
            Err(BookError::InvalidReduceOnly)
        );
        assert_eq!(book, saved);
        let fill = book
            .consume_reduce_only_candidate(
                &candidate,
                &candidate.position_before,
                &d("0.10000000000000000001"),
            )
            .unwrap();
        assert_eq!(fill.quantity, d("0.10000000000000000001"));
        assert_eq!(fill.maker_remaining, d("0.2"));
        assert_eq!(
            candidate
                .position_before
                .checked_add(&fill.quantity)
                .unwrap(),
            d("0")
        );
        assert_eq!(book.resting[0].remaining, d("0.2"));
    }

    #[test]
    fn invalid_side_guard_phase_height_quantity_and_external_id_are_atomic() {
        let mut book = book(&[order(1, "maker", Side::Buy, "1", "100", 1, false)]);
        let valid = request(Side::Sell, "1", "100");
        let saved = book.clone();
        for kind in 0..9 {
            let mut changed = valid.clone();
            let mut position = d("1");
            match kind {
                0 => changed.side = Side::Buy,
                1 => changed.guard = ExecutionPriceGuard::BuyAtMost(d("100")),
                2 => changed.requested_quantity = d("1.00000000000000000001"),
                3 => changed.requested_quantity = d("0"),
                4 => changed.guard = ExecutionPriceGuard::SellAtLeast(d("0")),
                5 => changed.height = 0,
                6 => changed.height = 9,
                7 => changed.id = LiquidationId::new_unverified(id(1)),
                _ => position = d("0"),
            }
            assert!(book
                .next_reduce_only_candidate(&changed, &position)
                .is_err());
            assert_eq!(book, saved);
        }
        let candidate = candidate(&book, &valid, "1");
        for quantity in ["0", "-1", "0.000000000000000000001"] {
            assert!(book
                .consume_reduce_only_candidate(&candidate, &d("1"), &d(quantity))
                .is_err());
            assert_eq!(book, saved);
        }
        book.queue(order(2, "queued", Side::Buy, "1", "100", 11, false))
            .unwrap();
        assert_eq!(
            book.next_reduce_only_candidate(&valid, &d("1")),
            Err(BookError::InvalidHeight)
        );
        let mut pending = saved;
        pending.processed_through = Some(9);
        pending
            .queue(order(3, "drained", Side::Buy, "1", "100", 10, false))
            .unwrap();
        pending.drain_queued_through(10).unwrap();
        pending.processed_through = Some(10);
        assert_eq!(
            pending.next_reduce_only_candidate(&valid, &d("1")),
            Err(BookError::InvalidHeight)
        );
    }
}
