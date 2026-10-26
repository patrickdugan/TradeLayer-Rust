// Derive the complete pool-cash stream from typed economic operations. Merely
// finding an existing internal transfer ID does not prove its pool endpoints.
// The caller separately verifies sequence identities and actual final custody.

#[derive(Default)]
struct AmmCashPnlReplay {
    ious: IouStore,
    unpaid: BTreeMap<AccountId, u128>,
    loss_pool: u128,
}

struct AmmCashHistory {
    cash: u128,
    legs: Vec<(bool, AccountId, u128)>,
}
impl AmmCashHistory {
    fn push(&mut self, from_pool: bool, counterparty: AccountId, atoms: u128) -> Result<()> {
        if atoms == 0 {
            return Ok(());
        }
        if self.legs.len() >= MAX_AMM_RECORDS {
            return Err(EconomicError::Capacity);
        }
        self.cash = if from_pool {
            self.cash
                .checked_sub(atoms)
                .ok_or(EconomicError::Invariant)?
        } else {
            add(self.cash, atoms)?
        };
        self.legs.push((from_pool, counterparty, atoms));
        Ok(())
    }
    fn verify_legs(&self, actual: &[AmmCashLeg]) -> Result<()> {
        if self.legs.len() != actual.len()
            || self
                .legs
                .iter()
                .zip(actual)
                .any(|((from_pool, counterparty, atoms), leg)| {
                    *from_pool != leg.from_pool
                        || *counterparty != leg.counterparty
                        || *atoms != leg.atoms
                })
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}

impl EconomicState {
    fn verify_amm_cash_history(&self) -> Result<()> {
        if self.config.amm.is_none() {
            return if self.amm == FundedAmmState::default() {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        }
        let pool = self.amm_owner()?;
        let mut cash = AmmCashHistory {
            cash: 0,
            legs: Vec::new(),
        };
        let mut pnl = AmmCashPnlReplay::default();
        let mut marks = self.marking.events.iter().peekable();
        let mut funding = self.funding.epochs.iter().peekable();
        let mut liquidations = self.liquidation.events.iter().peekable();
        for (ordinal, frame) in self.amm.frames.iter().enumerate() {
            if frame.height != ordinal as u64 + 1 {
                return Err(EconomicError::Invariant);
            }
            cash.legs.clear();
            for quote in &frame.released_quotes {
                cash.push(
                    false,
                    self.amm_quote_escrow(quote.side)?,
                    quote.reserve_atoms,
                )?;
            }
            for receipt in &frame.capital_receipts {
                if receipt.cash_before_atoms != cash.cash {
                    return Err(EconomicError::Invariant);
                }
                cash.push(
                    receipt.kind == AmmCapitalKind::Redeem,
                    receipt.owner.clone(),
                    receipt.collateral_atoms,
                )?;
            }
            // Normal matches and the fresh mark settle before funding and
            // quote creation. Pool claims may be paid by another owner's trade.
            while marks.peek().is_some_and(|event| {
                event.height == frame.height
                    && matches!(
                        event.kind,
                        MarkEventKind::Trade { .. } | MarkEventKind::Mark { .. }
                    )
            }) {
                let event = marks.next().ok_or(EconomicError::Invariant)?;
                if matches!(event.kind, MarkEventKind::Trade { .. })
                    && event.entries.iter().any(|entry| entry.owner == pool)
                {
                    return Err(EconomicError::Invariant);
                }
                self.replay_amm_cash_pnl(event, &pool, &mut pnl, &mut cash)?;
            }
            if funding
                .peek()
                .is_some_and(|epoch| epoch.height == frame.height)
            {
                let epoch = funding.next().ok_or(EconomicError::Invariant)?;
                if epoch
                    .positions
                    .iter()
                    .find(|position| position.owner == pool)
                    .is_some_and(|position| position.available_atoms != cash.cash)
                {
                    return Err(EconomicError::Invariant);
                }
                if let Some(plan) = &epoch.plan {
                    let escrow = self.escrow(b"funding-epoch", &epoch.id.as_bytes())?;
                    for debit in &plan.debits {
                        if debit.owner == pool {
                            cash.push(true, escrow.clone(), debit.atoms)?;
                        }
                    }
                    for credit in &plan.credits {
                        if credit.owner == pool {
                            cash.push(false, escrow.clone(), credit.atoms)?;
                        }
                    }
                }
            }
            if let Some(epoch) = &frame.quote_epoch {
                if epoch.cash_before_atoms != cash.cash
                    || epoch.cash_leg_count_before as usize != cash.legs.len()
                {
                    return Err(EconomicError::Invariant);
                }
                for quote in &epoch.quotes {
                    cash.push(
                        true,
                        self.amm_quote_escrow(quote.side)?,
                        quote.reserve_atoms,
                    )?;
                }
            }
            while liquidations
                .peek()
                .is_some_and(|event| event.height == frame.height)
            {
                let event = liquidations.next().ok_or(EconomicError::Invariant)?;
                let pool_target = event.owner == pool;
                if pool_target {
                    if event.available_before_atoms != cash.cash {
                        return Err(EconomicError::Invariant);
                    }
                    for quote in &event.pool_cancelled_quotes {
                        cash.push(
                            false,
                            self.amm_quote_escrow(quote.side)?,
                            quote.reserve_atoms,
                        )?;
                    }
                    cash.push(true, self.margin_escrow(&pool)?, event.top_up_atoms)?;
                }
                let mut pool_target_margin = if pool_target {
                    Some(add(event.margin_before_atoms, event.top_up_atoms)?)
                } else {
                    None
                };
                for (index, receipt) in event.amm_fills.iter().enumerate() {
                    if pool_target || receipt.maker_available_before_atoms != cash.cash {
                        return Err(EconomicError::Invariant);
                    }
                    let forced = marks.next().ok_or(EconomicError::Invariant)?;
                    if forced.height != frame.height
                        || forced.kind
                            != (MarkEventKind::AmmForced {
                                epoch_id: event.id,
                                fill_index: index as u32,
                                pool_id: receipt.quote_before.pool_id,
                                distressed_owner: event.owner.clone(),
                            })
                        || forced.entries.len() != 2
                        || forced.entries[1].owner != pool
                    {
                        return Err(EconomicError::Invariant);
                    }
                    let remaining = receipt
                        .quote_before
                        .remaining
                        .checked_sub(&receipt.quantity)?;
                    let used = receipt
                        .quote_before
                        .reserve_atoms
                        .checked_sub(
                            self.required_margin_at(&remaining, &receipt.quote_before.price)?,
                        )
                        .ok_or(EconomicError::Invariant)?;
                    let before = add(receipt.maker_margin_before_atoms, used)?;
                    let target = forced.entries[1].margin_atoms;
                    if target > before {
                        cash.push(true, self.margin_escrow(&pool)?, target - before)?;
                    } else {
                        cash.push(false, self.margin_escrow(&pool)?, before - target)?;
                    }
                    cash.push(
                        false,
                        self.escrow(b"fees", &[])?,
                        receipt.maker_rebate_atoms,
                    )?;
                    self.replay_amm_cash_pnl(forced, &pool, &mut pnl, &mut cash)?;
                }
                for (index, receipt) in event.fills.iter().enumerate() {
                    let forced = marks.next().ok_or(EconomicError::Invariant)?;
                    if forced.height != frame.height
                        || forced.kind
                            != (MarkEventKind::Forced {
                                epoch_id: event.id,
                                fill_index: index as u32,
                                maker_txid: receipt.maker_before.order.txid,
                                distressed_owner: event.owner.clone(),
                            })
                    {
                        return Err(EconomicError::Invariant);
                    }
                    if pool_target {
                        let before = pool_target_margin.ok_or(EconomicError::Invariant)?;
                        let released = before
                            .checked_sub(receipt.distressed_margin_before_fee_atoms)
                            .ok_or(EconomicError::Invariant)?;
                        cash.push(false, self.margin_escrow(&pool)?, released)?;
                        if receipt.distressed_available_before_fee_atoms != cash.cash {
                            return Err(EconomicError::Invariant);
                        }
                        let available_fee = receipt.nominal_fee_atoms.min(cash.cash);
                        cash.push(true, self.escrow(b"fees", &[])?, available_fee)?;
                        pool_target_margin = Some(receipt.distressed_margin_after_atoms);
                    } else if forced.entries.iter().any(|entry| entry.owner == pool) {
                        return Err(EconomicError::Invariant);
                    }
                    self.replay_amm_cash_pnl(forced, &pool, &mut pnl, &mut cash)?;
                }
            }
            if marks
                .peek()
                .is_some_and(|event| event.height <= frame.height)
                || funding
                    .peek()
                    .is_some_and(|epoch| epoch.height <= frame.height)
                || liquidations
                    .peek()
                    .is_some_and(|event| event.height <= frame.height)
            {
                return Err(EconomicError::Invariant);
            }
            cash.verify_legs(&frame.cash_legs)?;
        }
        if marks.next().is_some()
            || funding.next().is_some()
            || liquidations.next().is_some()
            || cash.cash != self.amm.cash_atoms
            || pnl.ious != self.ious
            || pnl.unpaid != self.marking.unpaid_losses
            || pnl.loss_pool != self.loss_pool_atoms
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }

    fn replay_amm_cash_pnl(
        &self,
        event: &MarkEvent,
        pool: &AccountId,
        replay: &mut AmmCashPnlReplay,
        cash: &mut AmmCashHistory,
    ) -> Result<()> {
        let key = self.iou_key();
        let mut obligations = Vec::with_capacity(event.entries.len());
        for entry in &event.entries {
            let mut expected = entry.clone();
            let obligation = mark_apply_offsets(
                &mut replay.ious,
                &mut replay.unpaid,
                key,
                &mut expected,
                event.height,
            )?;
            if expected.claim_offset_atoms != entry.claim_offset_atoms
                || expected.loss_offset_atoms != entry.loss_offset_atoms
            {
                return Err(EconomicError::Invariant);
            }
            obligations.push(obligation);
        }
        let mut collected = 0;
        let mut missing = 0;
        for (entry, (debt, _)) in event.entries.iter().zip(&obligations) {
            if &entry.owner == pool {
                if entry.available_atoms != cash.cash {
                    return Err(EconomicError::Invariant);
                }
                cash.push(
                    true,
                    self.escrow(b"loss-pool", &[])?,
                    entry.collected_atoms.min(entry.available_atoms),
                )?;
            }
            let unpaid = debt
                .checked_sub(entry.collected_atoms)
                .ok_or(EconomicError::Invariant)?;
            mark_add_unpaid(&mut replay.unpaid, &entry.owner, unpaid)?;
            missing = add(missing, unpaid)?;
            collected = add(collected, entry.collected_atoms)?;
        }
        if collected != event.collected_loss_atoms || missing != event.unfunded_loss_atoms {
            return Err(EconomicError::Invariant);
        }
        replay.loss_pool = add(replay.loss_pool, collected)?;
        if collected != 0 {
            replay
                .ious
                .add_loss(key, &tokens(collected)?, event.height)?;
        }
        for (entry, (_, profit)) in event.entries.iter().zip(obligations) {
            mark_replay_claim(&mut replay.ious, key, &entry.owner, profit, event.height)?;
        }
        let before = replay
            .ious
            .claim_map(key)
            .and_then(|claims| claims.claim(pool))
            .map(atoms_exact)
            .transpose()?
            .unwrap_or(0);
        let paid = mark_replay_payout(&mut replay.ious, key, &mut replay.loss_pool, event.height)?;
        let after = replay
            .ious
            .claim_map(key)
            .and_then(|claims| claims.claim(pool))
            .map(atoms_exact)
            .transpose()?
            .unwrap_or(0);
        if paid != event.paid_claim_atoms {
            return Err(EconomicError::Invariant);
        }
        let pool_paid = before.checked_sub(after).ok_or(EconomicError::Invariant)?;
        cash.push(false, self.escrow(b"loss-pool", &[])?, pool_paid)?;
        Ok(())
    }
}

#[cfg(test)]
mod amm_cash_history_tests {
    use super::*;
    fn account(text: &str) -> AccountId {
        AccountId::try_from(text).unwrap()
    }
    fn leg(sequence: u64, from_pool: bool, counterparty: &str, atoms: u128) -> AmmCashLeg {
        AmmCashLeg {
            sequence,
            from_pool,
            counterparty: account(counterparty),
            atoms,
        }
    }
    #[test]
    fn balanced_fictional_legs_are_rejected_even_with_identical_final_cash() {
        let mut expected = AmmCashHistory {
            cash: 0,
            legs: Vec::new(),
        };
        expected.push(false, account("alice"), 100).unwrap();
        let legitimate = vec![leg(0, false, "alice", 100)];
        expected.verify_legs(&legitimate).unwrap();
        let mut forged = legitimate;
        forged.push(leg(2, false, "victim", 1));
        forged.push(leg(3, true, "victim", 1));
        assert_eq!(amm_cash_before_leg(0, &forged).unwrap(), expected.cash);
        assert_eq!(expected.verify_legs(&forged), Err(EconomicError::Invariant));
    }
    #[test]
    fn same_net_cash_cannot_substitute_endpoints_amounts_or_order() {
        let mut expected = AmmCashHistory {
            cash: 100,
            legs: Vec::new(),
        };
        expected.push(true, account("~econ:quote"), 20).unwrap();
        expected.push(false, account("~econ:fees"), 10).unwrap();
        let legitimate = vec![
            leg(4, true, "~econ:quote", 20),
            leg(9, false, "~econ:fees", 10),
        ];
        expected.verify_legs(&legitimate).unwrap();
        for change in 0..3 {
            let mut forged = legitimate.clone();
            match change {
                0 => forged[0].counterparty = account("~econ:fees"),
                1 => {
                    forged[0].atoms = 21;
                    forged[1].atoms = 11;
                }
                _ => forged.reverse(),
            }
            assert_eq!(expected.verify_legs(&forged), Err(EconomicError::Invariant));
        }
        assert_eq!(expected.cash, 90);
    }

    #[test]
    fn maintenance_only_pool_cannot_lock_cash_needed_for_existing_initial_margin() {
        let decimal = |text: &str| text.parse::<Decimal>().unwrap();
        let collateral = PropertyId::try_from(7).unwrap();
        let mut ledger = Ledger::new();
        ledger
            .apply(
                TxId::from_bytes([0xa1; 32]),
                crate::ledger::Operation::CreateProperty {
                    id: collateral,
                    issuer: account("alice"),
                    kind: crate::ledger::PropertyKind::Fixed,
                    initial_atoms: 100 * ATOMS_PER_TOKEN,
                },
            )
            .unwrap();
        let config = EconomicConfig {
            source_consensus: None,
            network: TradeLayerMainnet::Bitcoin,
            contract: ContractId::try_from(1).unwrap(),
            collateral,
            inverse: false,
            notional: decimal("1"),
            initial_margin_per_contract: decimal("10"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            }),
            liquidation: Some(LiquidationConfig {
                max_price_deviation_bps: 100,
                max_fills_per_owner: 64,
                policy: LiquidationPolicy::ConservingLiquidationV1,
            }),
            amm: Some(FundedAmmConfig {
                pool_id: 7,
                spread_bps: 40,
                max_quote_quantity: decimal("1"),
                max_abs_inventory: decimal("3"),
                max_collateral_atoms: 100 * ATOMS_PER_TOKEN,
                policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
            }),
            contract_binding: None,
        };
        let joint = JointProtocolState::from_bootstrap_unverified(
            config.network,
            account("admin"),
            ledger,
            BTreeSet::from([collateral]),
        )
        .unwrap();
        let owner = EconomicState::from_joint_unverified(config, joint).unwrap();
        // This pure proposed epoch represents an existing position whose
        // earlier loss dented margin, then a later gain restored cash only.
        let mut epoch = AmmQuoteEpoch {
            id: TxId::from_bytes([0xa2; 32]),
            height: 2,
            mark_id: TxId::from_bytes([0xa3; 32]),
            generation: 1,
            position_before: decimal("1"),
            average_price_before: None,
            cash_before_atoms: 20 * ATOMS_PER_TOKEN,
            margin_before_atoms: 5 * ATOMS_PER_TOKEN,
            unpaid_loss_before_atoms: 0,
            cash_leg_count_before: 0,
            quotes: Vec::new(),
        };
        assert_eq!(
            owner.maintenance_at(&epoch.position_before, None).unwrap(),
            epoch.margin_before_atoms
        );
        assert_eq!(
            owner.required_margin(&epoch.position_before.abs()).unwrap(),
            10 * ATOMS_PER_TOKEN
        );
        assert!(owner
            .plan_amm_quotes(&epoch, &decimal("100"))
            .unwrap()
            .is_empty());

        epoch.margin_before_atoms = 10 * ATOMS_PER_TOKEN;
        let quotes = owner.plan_amm_quotes(&epoch, &decimal("100")).unwrap();
        assert_eq!(quotes.len(), 2);
        assert_eq!(
            quotes.iter().map(|quote| quote.side).collect::<Vec<_>>(),
            vec![Side::Buy, Side::Sell]
        );
        for quote in &quotes {
            assert_eq!(quote.remaining, decimal("1"));
            assert_eq!(quote.reserve_atoms, 10 * ATOMS_PER_TOKEN);
        }
        assert_eq!(
            quotes.iter().map(|quote| quote.reserve_atoms).sum::<u128>(),
            epoch.cash_before_atoms
        );
    }
}
