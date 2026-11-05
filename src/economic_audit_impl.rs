// Included inside economic.rs: how often the owner audits its whole state.
//
// Every block already passes through the staged owner's checks. The audit is a
// separate pass over the finished state. `EveryBlock` runs the complete `verify`
// on the starting state and again on the finished state, which is the original
// behavior. `Periodic(K)` splits the audit by cost:
//
// * Live checks cost O(live state) and run on every block: configuration, book
//   against reserves, positions, margins, custody balances, IOU balances, and
//   the joint ledger's conservation and id sets (the joint audit runs inside
//   `finalize_block_for_economic_unverified`).
// * History checks cost O(history). A block runs them only on its own
//   additions: new input ids, new legs, new receipts and mark events, the fee
//   book replayed over the new receipts, and the new accepted contracts and
//   input frame. Heights that are multiples of K run the complete `verify`.
//
// A violation inside history that a block did not touch is therefore found at
// the next full-audit height, at most K-1 blocks late. A failed audit returns
// the error `verify` returns and publishes nothing, because the finisher fails
// before it hands the staged state back.
//
// The policy is runtime state. It is not serialized, not compared and not
// printed. A state skips its starting audit only when it is the output of a
// block finished under the same policy in this process; a loaded, decoded or
// constructed state runs the complete `verify` first.

/// Default full-audit interval of `AuditPolicy::Periodic`.
pub const DEFAULT_AUDIT_INTERVAL: u64 = 64;

/// How often the owner audits its complete state while applying blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditPolicy {
    /// Audit the starting and the finished state of every block completely.
    EveryBlock,
    /// Audit live state and the block's own additions every block, and the
    /// complete state at every height that is a multiple of the interval. An
    /// interval of 0 or 1 audits completely every block.
    Periodic(u64),
}

impl Default for AuditPolicy {
    fn default() -> Self {
        Self::Periodic(DEFAULT_AUDIT_INTERVAL)
    }
}

impl AuditPolicy {
    fn full_due(self, height: u64) -> bool {
        match self {
            Self::EveryBlock => true,
            Self::Periodic(interval) => interval <= 1 || height.is_multiple_of(interval),
        }
    }
}

/// Runtime-only audit state. Clones keep it; equality ignores it.
#[derive(Clone, Default)]
struct AuditRuntime {
    policy: AuditPolicy,
    /// Policy of the block that produced this state in this process, if any.
    audited: Option<AuditPolicy>,
    /// Accepted-contract mode: the state after re-executing every input frame
    /// from the committed bootstrap, so the next block replays one frame.
    binding_replay: Option<Arc<EconomicState>>,
}
impl PartialEq for AuditRuntime {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for AuditRuntime {}

/// Values of the audited starting state that the delta checks extend.
#[derive(Clone)]
struct AuditBefore {
    receipts: usize,
    mark_events: usize,
    contracts: usize,
    internal_sequence: u64,
    loss_pool_atoms: u128,
    unfunded_loss_atoms: u128,
    insurance_total_atoms: u128,
    fees: crate::tally::FeeBookMark,
}

struct AuditDelta<'a> {
    before: &'a AuditBefore,
    new_orders: &'a [TxId],
    new_cancels: &'a [TxId],
}

/// Per-block audit plan of one staged block.
#[derive(Clone)]
struct StageAudit {
    policy: AuditPolicy,
    /// Periodic policy on a configuration whose history checks can be split.
    periodic: bool,
    /// Present when only the block's additions are audited at finish.
    before: Option<AuditBefore>,
    new_orders: Vec<TxId>,
    new_cancels: Vec<TxId>,
}
impl StageAudit {
    fn note_input(&mut self, input: &EconomicInput) {
        match input {
            EconomicInput::Order(order) => self.new_orders.push(order.txid),
            EconomicInput::Cancel { txid, .. } => self.new_cancels.push(*txid),
            _ => {}
        }
    }
}

impl EconomicState {
    pub fn audit_policy(&self) -> AuditPolicy {
        self.audit.policy
    }

    /// Sets the audit cadence of later blocks. The setting is runtime only.
    pub fn set_audit_policy(&mut self, policy: AuditPolicy) {
        self.audit.policy = policy;
    }

    #[must_use]
    pub fn with_audit_policy(mut self, policy: AuditPolicy) -> Self {
        self.audit.policy = policy;
        self
    }

    /// The split audit covers the plain owner, accepted-contract mode, funding,
    /// marking and liquidation. Native entitlements, source consensus, the
    /// insurance fund and the funded AMM keep their complete audit on every
    /// block: their history checks (metadata id sets, receipt categories,
    /// replayed frames) have no delta form here. States replaying a history
    /// audit completely as well.
    fn audit_eligible(&self) -> bool {
        self.config.native_entitlements.is_none()
            && self.config.source_consensus.is_none()
            && self.config.insurance.is_none()
            && self.config.amm.is_none()
            && !self.replaying_contract_history
            && !self.replaying_native_history
            && !self.replaying_source_history
            && !self.catalog_inner_source_verified
            && self.catalog_inner_oracle.is_none()
            && self.catalog_inner_metadata_ids.is_empty()
    }

    fn audit_before(&self) -> AuditBefore {
        AuditBefore {
            receipts: self.receipts.len(),
            mark_events: self.marking.events.len(),
            contracts: self.contract_binding.contracts.len(),
            internal_sequence: self.internal_sequence,
            loss_pool_atoms: self.loss_pool_atoms,
            unfunded_loss_atoms: self.unfunded_loss_atoms,
            insurance_total_atoms: self
                .insurance_custody_atoms
                .saturating_add(self.insurance.adopted_fee_atoms),
            fees: self.fees.audit_mark(),
        }
    }

    /// New input ids are unique across the three id categories and are in the
    /// joint replay set. The category sizes must add up to the joint set's size,
    /// so an id that entered a category or the joint set without its counterpart
    /// is rejected. Ids admitted by earlier blocks were checked then.
    fn verify_input_ids_delta(&self, delta: &AuditDelta<'_>) -> Result<()> {
        let joint_ids = self.joint.economic_input_ids_unverified();
        let contracts = self
            .contract_binding
            .contracts
            .get(delta.before.contracts..)
            .ok_or(EconomicError::Invariant)?;
        if self
            .book
            .seen_count()
            .checked_add(self.contract_binding.contracts.len())
            .and_then(|n| n.checked_add(self.cancel_ids.len()))
            != Some(joint_ids.len())
            || delta
                .new_orders
                .iter()
                .any(|id| !self.book.has_seen(*id) || !joint_ids.contains(id))
            || delta
                .new_cancels
                .iter()
                .any(|id| !self.cancel_ids.contains(id) || !joint_ids.contains(id))
            || contracts
                .iter()
                .any(|c| !joint_ids.contains(&c.creation_txid))
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }

    /// Every leg created by this block is a processed ledger transfer.
    fn verify_legs_delta(&self, delta: &AuditDelta<'_>) -> Result<()> {
        if self.internal_sequence < delta.before.internal_sequence {
            return Err(EconomicError::Invariant);
        }
        for sequence in delta.before.internal_sequence..self.internal_sequence {
            if !self.ledger().is_processed(self.leg_id(sequence)) {
                return Err(EconomicError::Invariant);
            }
        }
        Ok(())
    }

    /// New receipts and mark events: ordering, bounds and the loss-pool, bad
    /// debt, fee and insurance totals, each extended from the audited start.
    fn verify_receipts_delta(&self, delta: &AuditDelta<'_>) -> Result<()> {
        let before = delta.before;
        let new_receipts = self
            .receipts
            .get(before.receipts..)
            .ok_or(EconomicError::Invariant)?;
        let new_events = self
            .marking
            .events
            .get(before.mark_events..)
            .ok_or(EconomicError::Invariant)?;
        let mut collected = 0;
        let mut paid = 0;
        let mut bad_debt = 0;
        for event in new_events {
            if !matches!(event.kind, MarkEventKind::Trade { .. }) {
                collected = add(collected, event.collected_loss_atoms)?;
                paid = add(paid, event.paid_claim_atoms)?;
                bad_debt = add(bad_debt, event.unfunded_loss_atoms)?;
            }
        }
        let mut last = before
            .receipts
            .checked_sub(1)
            .map_or(self.receipt_archive.last_height, |index| {
                self.receipts[index].height
            });
        for receipt in new_receipts {
            if receipt.height < last
                || receipt.height <= self.config.anchor.height
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
        // The audited start satisfied collected - paid == pool and bad debt ==
        // unfunded, so the new totals hold exactly when the increments agree.
        if add(before.loss_pool_atoms, collected)? != add(self.loss_pool_atoms, paid)?
            || add(before.unfunded_loss_atoms, bad_debt)? != self.unfunded_loss_atoms
        {
            return Err(EconomicError::Invariant);
        }
        if self.config.liquidation.is_none() {
            let (same, insurance) = self.fees.matches_accruals_after(
                &before.fees,
                new_receipts.iter().map(|r| {
                    (
                        self.config.collateral,
                        self.config.fee_route,
                        r.retained_fee_atoms,
                        r.height,
                    )
                }),
            )?;
            if !same
                || add(before.insurance_total_atoms, insurance)?
                    != add(
                        self.insurance_custody_atoms,
                        self.insurance.adopted_fee_atoms,
                    )?
            {
                return Err(EconomicError::Invariant);
            }
        } else {
            // Forced fills and AMM fills interleave with receipts in the fee
            // derivation, so liquidation configurations keep the complete rebuild.
            let (expected_fees, expected_insurance) = self.expected_economic_fees()?;
            if expected_fees != self.fees
                || expected_insurance
                    != add(
                        self.insurance_custody_atoms,
                        self.insurance.adopted_fee_atoms,
                    )?
            {
                return Err(EconomicError::Invariant);
            }
        }
        Ok(())
    }

    /// Accepted-contract binding. The complete audit re-executes every input
    /// frame from the committed bootstrap and returns the replayed state. The
    /// delta audit extends the previous replay by the block's frame.
    fn verify_contract_binding_audit(
        &self,
        delta: Option<&AuditDelta<'_>>,
    ) -> Result<Option<Box<EconomicState>>> {
        let Some(delta) = delta else {
            return self.verify_contract_binding_full();
        };
        let Some(config) = &self.config.contract_binding else {
            return self.verify_contract_binding_full();
        };
        // The journal's record budget was checked on this exact journal when
        // the finisher appended the frame, so it is not encoded a second time.
        if contract_bootstrap_hash(&self.contract_binding.bootstrap) != config.bootstrap_commitment
            || self.contract_binding.frames.len() as u64 != self.applied_blocks()
        {
            return Err(EconomicError::Invariant);
        }
        for (index, contract) in self
            .contract_binding
            .contracts
            .iter()
            .enumerate()
            .skip(delta.before.contracts)
        {
            if contract.definition.id.get() as usize != index + 1
                || contract.height <= self.config.anchor.height
                || contract.height > self.tip_height()
            {
                return Err(EconomicError::Invariant);
            }
            if contract.definition.id == self.config.contract {
                self.validate_selected_contract(contract)?;
            }
        }
        let Some(frame) = self.contract_binding.frames.last() else {
            return self.verify_contract_binding_full();
        };
        if frame.height != self.tip_height() || frame.inputs.len() > MAX_ECONOMIC_INPUTS {
            return Err(EconomicError::Invariant);
        }
        let Some(previous) = &self.audit.binding_replay else {
            return self.verify_contract_binding_full();
        };
        if previous.tip_height() + 1 != frame.height {
            return self.verify_contract_binding_full();
        }
        let mut replay = (**previous).clone();
        replay.replaying_contract_history = true;
        replay.apply_block_unverified(frame.height, frame.inputs.clone())?;
        replay.replaying_contract_history = false;
        if replay != *self {
            return Err(EconomicError::Invariant);
        }
        Ok(Some(Box::new(replay)))
    }

    /// Test hook: changes a receipt that no later block touches, so only a
    /// complete audit can see it.
    #[cfg(test)]
    pub(crate) fn corrupt_oldest_receipt_for_audit_test(&mut self) {
        self.receipts[0].retained_fee_atoms += 1;
    }

    #[cfg(test)]
    pub(crate) fn has_audit_marker_for_test(&self) -> bool {
        self.audit.audited.is_some()
    }

    /// Test hook: the state of a loaded or constructed owner, which no block of
    /// this process produced.
    #[cfg(test)]
    pub(crate) fn clear_audit_marker_for_test(&mut self) {
        self.audit.audited = None;
        self.audit.binding_replay = None;
    }
}

impl core::fmt::Debug for EconomicState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EconomicState")
            .field("config", &self.config)
            .field("joint", &self.joint)
            .field("book", &self.book)
            .field("positions", &self.positions)
            .field("ious", &self.ious)
            .field("fees", &self.fees)
            .field("reserves", &self.reserves)
            .field("margins", &self.margins)
            .field("cancel_ids", &self.cancel_ids)
            .field("loss_pool_atoms", &self.loss_pool_atoms)
            .field("insurance_custody_atoms", &self.insurance_custody_atoms)
            .field("unfunded_loss_atoms", &self.unfunded_loss_atoms)
            .field("receipts", &self.receipts)
            .field("receipt_archive", &self.receipt_archive)
            .field("internal_sequence", &self.internal_sequence)
            .field("funding", &self.funding)
            .field("marking", &self.marking)
            .field("liquidation", &self.liquidation)
            .field("amm", &self.amm)
            .field("contract_binding", &self.contract_binding)
            .field(
                "replaying_contract_history",
                &self.replaying_contract_history,
            )
            .field("source_consensus", &self.source_consensus)
            .field("replaying_source_history", &self.replaying_source_history)
            .field(
                "catalog_inner_metadata_ids",
                &self.catalog_inner_metadata_ids,
            )
            .field("catalog_inner_oracle", &self.catalog_inner_oracle)
            .field(
                "catalog_inner_source_verified",
                &self.catalog_inner_source_verified,
            )
            .field("native_entitlements", &self.native_entitlements)
            .field("replaying_native_history", &self.replaying_native_history)
            .field("insurance", &self.insurance)
            .finish()
    }
}
