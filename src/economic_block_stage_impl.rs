// Included inside economic.rs: private pre-finalization access, never a second
// committed Ledger. A failed candidate restores its entire staged owner.
pub(crate) struct EconomicBlockStage {
    state: EconomicState,
    height: u64,
    receipt_count_before: usize,
    inputs: Vec<EconomicInput>,
    book_frame: LiquidationBookFrame,
    mark_publication: Option<TxId>,
}

impl EconomicState {
    pub(crate) fn begin_block_stage_unverified(&self, height: u64) -> Result<EconomicBlockStage> {
        if height == 0
            || height > crate::contract_orderbook::MAX_ORDER_HEIGHT
            || self.finalized_height().unwrap_or(0).checked_add(1) != Some(height)
        {
            return Err(EconomicError::InvalidHeight);
        }
        self.verify()?;
        let mut state = self.clone();
        state.begin_amm_frame(height)?;
        Ok(EconomicBlockStage {
            receipt_count_before: state.receipts.len(),
            state,
            height,
            inputs: Vec::new(),
            book_frame: LiquidationBookFrame {
                height,
                inputs: Vec::new(),
                placements: Vec::new(),
            },
            mark_publication: None,
        })
    }
}

impl EconomicBlockStage {
    /// Pending reads reflect preceding admitted commands. Queued contract
    /// orders still settle in the once-per-block end phase, as before.
    pub(crate) fn state(&self) -> &EconomicState {
        &self.state
    }

    /// Borrowed aggregate preflight precedes the candidate/state copies.
    /// No caller validity flag can make this method bypass owner checks.
    pub(crate) fn try_apply_unverified(&mut self, input: &EconomicInput) -> Result<()> {
        if self.inputs.len() >= MAX_ECONOMIC_INPUTS {
            return Err(EconomicError::Capacity);
        }
        self.state
            .preflight_contract_candidate(self.height, &self.inputs, input)?;
        let mut pending = self.state.clone();
        let mut book_frame = self.book_frame.clone();
        let mut mark_publication = self.mark_publication;
        Self::apply_to_pending(
            &mut pending,
            &mut book_frame,
            &mut mark_publication,
            self.height,
            input.clone(),
        )?;
        self.state = pending;
        self.book_frame = book_frame;
        self.mark_publication = mark_publication;
        self.inputs.push(input.clone());
        Ok(())
    }

    /// Existing typed whole-block application already preflights its entire
    /// borrowed batch and discards this stage on any error. Preserve that
    /// path's single staging clone instead of cloning the owner per command.
    pub(crate) fn apply_all_preflighted_unverified(
        &mut self,
        inputs: Vec<EconomicInput>,
    ) -> Result<()> {
        for input in inputs {
            if self.state.config.contract_binding.is_some() {
                self.inputs.push(input.clone());
            }
            Self::apply_to_pending(
                &mut self.state,
                &mut self.book_frame,
                &mut self.mark_publication,
                self.height,
                input,
            )?;
        }
        Ok(())
    }
    fn apply_to_pending(
        pending: &mut EconomicState,
        book_frame: &mut LiquidationBookFrame,
        mark_publication: &mut Option<TxId>,
        height: u64,
        input: EconomicInput,
    ) -> Result<()> {
        match input {
            EconomicInput::CreateContract(input) => {
                pending.apply_contract_creation(input, height)?;
            }
            EconomicInput::Amm(input) => pending.apply_amm_capital(input, height)?,
            EconomicInput::Joint(tx) => {
                EconomicState::external_joint_owner_boundary(&tx)?;
                let observed = pending.funding_publication(&tx);
                let mark_id = pending.mark_publication(&tx);
                pending
                    .joint
                    .apply_one_for_economic_unverified(height, tx)?;
                if let Some((txid, closed)) = observed {
                    pending.capture_funding_publication(height, txid, closed)?;
                }
                if let Some(id) = mark_id {
                    *mark_publication = Some(id);
                }
            }
            EconomicInput::Order(order) => {
                pending.require_contract_order_admission(&order)?;
                if order.height != height {
                    return Err(EconomicError::InvalidHeight);
                }
                pending
                    .joint
                    .note_economic_input_unverified(order.txid, height, 18)?;
                if pending.config.liquidation.is_some() {
                    book_frame
                        .inputs
                        .push(LiquidationBookInput::Order(order.clone()));
                }
                pending.book.queue(order)?;
            }
            EconomicInput::Cancel {
                txid,
                owner,
                criteria,
            } => {
                normal_owner(&owner)?;
                pending
                    .joint
                    .note_economic_input_unverified(txid, height, 6)?;
                if !pending.cancel_ids.insert(txid) {
                    return Err(EconomicError::InvalidInput);
                }
                if pending.config.liquidation.is_some() {
                    book_frame.inputs.push(LiquidationBookInput::Cancel {
                        txid,
                        owner: owner.clone(),
                        criteria: criteria.clone(),
                    });
                }
                let removed = pending.book.cancel(&owner, criteria)?;
                for order in removed.resting {
                    pending.release_reserve(order.order.txid)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn finish_unverified(self) -> Result<(EconomicState, usize)> {
        if self.state.config.source_consensus.is_some() {
            return Err(EconomicError::InvalidInput);
        }
        self.finish_internal(None)
    }
    pub(crate) fn finish_source_unverified(
        self,
        frame: crate::consensus_source::SourceBlockFrame,
    ) -> Result<(EconomicState, usize)> {
        if self.state.config.source_consensus.is_none() || frame.header.height != self.height {
            return Err(EconomicError::InvalidInput);
        }
        self.finish_internal(Some(frame))
    }
    fn finish_internal(
        mut self,
        source_frame: Option<crate::consensus_source::SourceBlockFrame>,
    ) -> Result<(EconomicState, usize)> {
        if self.state.config.contract_binding.is_some() {
            self.state.contract_binding.frames.push(ContractInputFrame {
                height: self.height,
                inputs: self.inputs,
            });
            self.state.contract_record_budget()?;
        }
        let orders = self.state.book.drain_queued_through(self.height)?;
        for order in orders {
            let position = self
                .state
                .positions
                .get(&order.owner)
                .map_or_else(Decimal::zero, |p| p.contracts.clone());
            if self.state.config.liquidation.is_some() {
                self.book_frame.placements.push(LiquidationBookPlacement {
                    order: order.clone(),
                    position: position.clone(),
                });
            }
            let admission_price = order.price.clone();
            let transition = self.state.book.place(order, &position)?;
            self.state
                .apply_transition(transition, self.height, &admission_price)?;
        }
        self.state.book.finish_height(self.height)?;
        if self.state.config.liquidation.is_some() {
            self.state.liquidation.book_frames.push(self.book_frame);
            self.state.liquidation_record_count()?;
        }
        self.state
            .run_mark_epoch(self.height, self.mark_publication)?;
        self.state.run_funding_epoch(self.height)?;
        self.state.prune_funding_observations(self.height);
        self.state.refresh_positions()?;
        self.state.refresh_amm_quotes(self.height)?;
        self.state.refresh_maintenance()?;
        self.state.run_liquidations(self.height)?;
        self.state.refresh_positions()?;
        self.state.refresh_maintenance()?;
        self.state
            .joint
            .finalize_block_for_economic_unverified(self.height)?;
        if let Some(frame) = source_frame {
            self.state.source_consensus.frames.push(frame);
        }
        self.state.snapshot_bytes()?;
        let receipt_count = self.state.receipts.len() - self.receipt_count_before;
        Ok((self.state, receipt_count))
    }
}
