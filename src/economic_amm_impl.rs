// Funded contract AMM metadata and execution in the sole economic Ledger.
pub const MAX_AMM_LPS: usize = 4096;
pub const MAX_AMM_FRAMES: usize = 4096;
pub const MAX_AMM_RECORDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundedAmmPolicy {
    ConservingFundedContractAmmV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundedAmmConfig {
    pub pool_id: u32,
    pub spread_bps: u32,
    pub max_quote_quantity: Decimal,
    pub max_abs_inventory: Decimal,
    pub max_collateral_atoms: u128,
    pub policy: FundedAmmPolicy,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmmCapitalInput {
    Deposit {
        txid: TxId,
        owner: AccountId,
        collateral_atoms: u128,
        min_share_units: u128,
    },
    Redeem {
        txid: TxId,
        owner: AccountId,
        share_units: u128,
        min_collateral_atoms: u128,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmmCapitalKind {
    Deposit,
    Redeem,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmCapitalReceipt {
    pub txid: TxId,
    pub owner: AccountId,
    pub kind: AmmCapitalKind,
    pub collateral_atoms: u128,
    pub share_units: u128,
    pub cash_before_atoms: u128,
    pub total_units_before: u128,
    pub owner_units_before: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundedAmmQuote {
    pub epoch_id: TxId,
    pub pool_id: u32,
    pub generation: u64,
    pub height: u64,
    pub mark_id: TxId,
    pub side: Side,
    pub price: Decimal,
    pub remaining: Decimal,
    pub reserve_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmQuoteEpoch {
    pub id: TxId,
    pub height: u64,
    pub mark_id: TxId,
    pub generation: u64,
    pub position_before: Decimal,
    pub cash_before_atoms: u128,
    pub margin_before_atoms: u128,
    pub unpaid_loss_before_atoms: u128,
    /// Exact boundary within this height's pool-cash leg journal.
    pub cash_leg_count_before: u32,
    pub quotes: Vec<FundedAmmQuote>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmCashLeg {
    pub sequence: u64,
    pub from_pool: bool,
    pub counterparty: AccountId,
    pub atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmFrame {
    pub height: u64,
    pub released_quotes: Vec<FundedAmmQuote>,
    pub capital_inputs: Vec<AmmCapitalInput>,
    pub capital_receipts: Vec<AmmCapitalReceipt>,
    pub quote_epoch: Option<AmmQuoteEpoch>,
    pub cash_legs: Vec<AmmCashLeg>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FundedAmmState {
    pub total_units: u128,
    pub shares: BTreeMap<AccountId, u128>,
    pub cash_atoms: u128,
    pub generation: u64,
    pub quotes: Vec<FundedAmmQuote>,
    pub frames: Vec<AmmFrame>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmForcedCloseReceipt {
    pub quote_before: FundedAmmQuote,
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

impl FundedAmmConfig {
    fn validate(&self) -> Result<()> {
        bounded(&self.max_quote_quantity)?;
        bounded(&self.max_abs_inventory)?;
        if self.pool_id == 0
            || !(1..=9999).contains(&self.spread_bps)
            || self.max_quote_quantity <= Decimal::zero()
            || self.max_abs_inventory <= Decimal::zero()
            || self.max_collateral_atoms == 0
        {
            return Err(EconomicError::InvalidConfig);
        }
        Ok(())
    }
}

impl EconomicState {
    fn external_joint_owner_boundary(tx: &UnverifiedJointTx) -> Result<()> {
        use crate::joint::JointOperation;
        use crate::registry::RegistryAction;
        normal_owner(&tx.actor)?;
        match &tx.operation {
            JointOperation::Send { recipient, .. } | JointOperation::Grant { recipient, .. } => {
                normal_owner(recipient)?
            }
            JointOperation::Registry(action) => match action {
                RegistryAction::CreateWhitelist { backup, .. }
                | RegistryAction::CreateOracle { backup, .. } => normal_owner(backup)?,
                RegistryAction::UpdateAdmin { new_address, .. } => normal_owner(new_address)?,
                RegistryAction::Attest { subject, .. }
                | RegistryAction::RevokeAttestation { subject, .. } => normal_owner(subject)?,
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }
    pub fn amm_state(&self) -> &FundedAmmState {
        &self.amm
    }
    pub fn amm_owner(&self) -> Result<AccountId> {
        let config = self
            .config
            .amm
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        self.escrow(b"amm-pool", &config.pool_id.to_be_bytes())
    }
    fn is_amm_owner(&self, owner: &AccountId) -> bool {
        self.config.amm.is_some() && self.amm_owner().is_ok_and(|pool| pool == *owner)
    }
    fn participant_owner(&self, owner: &AccountId) -> Result<()> {
        if self.is_amm_owner(owner) {
            Ok(())
        } else {
            normal_owner(owner)
        }
    }
    fn amm_quote_escrow(&self, side: Side) -> Result<AccountId> {
        let config = self
            .config
            .amm
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let mut key = config.pool_id.to_be_bytes().to_vec();
        key.push(if side == Side::Buy { 0 } else { 1 });
        self.escrow(b"amm-quote", &key)
    }
    fn amm_epoch_id(&self, height: u64, mark_id: TxId, generation: u64) -> Result<TxId> {
        let config = self
            .config
            .amm
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let mut bytes = self.domain(b"amm-quote-epoch-v1");
        bytes.extend_from_slice(&config.pool_id.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&mark_id.as_bytes());
        bytes.extend_from_slice(&generation.to_be_bytes());
        Ok(TxId::from_bytes(double_sha256(&bytes)))
    }
    fn begin_amm_frame(&mut self, height: u64) -> Result<()> {
        if self.config.amm.is_none() {
            return Ok(());
        }
        if self.amm.frames.len() >= MAX_AMM_FRAMES {
            return Err(EconomicError::Capacity);
        }
        self.amm.frames.push(AmmFrame {
            height,
            released_quotes: Vec::new(),
            capital_inputs: Vec::new(),
            capital_receipts: Vec::new(),
            quote_epoch: None,
            cash_legs: Vec::new(),
        });
        // Establish the staged height before an internal quote refund. This is
        // an epoch capability, never an external type18 actor.
        let release = self.amm.quotes.clone();
        if !release.is_empty() {
            let mut domain = self.domain(b"amm-quote-invalidation-v1");
            domain.extend_from_slice(&height.to_be_bytes());
            self.joint
                .note_economic_epoch_unverified(TxId::from_bytes(double_sha256(&domain)), height)?;
        }
        let released = self.cancel_amm_quotes()?;
        self.amm
            .frames
            .last_mut()
            .ok_or(EconomicError::Invariant)?
            .released_quotes = released;
        Ok(())
    }
    fn amm_invalidation_id(&self, height: u64) -> TxId {
        let mut domain = self.domain(b"amm-quote-invalidation-v1");
        domain.extend_from_slice(&height.to_be_bytes());
        TxId::from_bytes(double_sha256(&domain))
    }
    fn cancel_amm_quotes(&mut self) -> Result<Vec<FundedAmmQuote>> {
        if self.config.amm.is_none() {
            return Ok(Vec::new());
        }
        let quotes = core::mem::take(&mut self.amm.quotes);
        let pool = self.amm_owner()?;
        for quote in &quotes {
            self.transfer(
                self.amm_quote_escrow(quote.side)?,
                pool.clone(),
                quote.reserve_atoms,
            )?;
        }
        Ok(quotes)
    }
    fn record_amm_cash_leg(
        &mut self,
        sequence: u64,
        from: &AccountId,
        to: &AccountId,
        atoms: u128,
    ) -> Result<()> {
        if self.config.amm.is_none() {
            return Ok(());
        }
        let pool = self.amm_owner()?;
        if *from != pool && *to != pool {
            return Ok(());
        }
        if *from == *to || atoms == 0 {
            return Err(EconomicError::Invariant);
        }
        let from_pool = *from == pool;
        self.amm.cash_atoms = if from_pool {
            self.amm
                .cash_atoms
                .checked_sub(atoms)
                .ok_or(EconomicError::CustodyMismatch)?
        } else {
            add(self.amm.cash_atoms, atoms)?
        };
        self.amm
            .frames
            .last_mut()
            .ok_or(EconomicError::Invariant)?
            .cash_legs
            .push(AmmCashLeg {
                sequence,
                from_pool,
                counterparty: if from_pool { to.clone() } else { from.clone() },
                atoms,
            });
        Ok(())
    }
    fn amm_quiescent(&self) -> Result<()> {
        let pool = self.amm_owner()?;
        if !self.position(&pool).contracts.is_zero()
            || !self.amm.quotes.is_empty()
            || self.margin_atoms(&pool) != 0
            || self.marking.unpaid_losses.get(&pool).copied().unwrap_or(0) != 0
            || self
                .ious
                .claim_map(self.iou_key())
                .and_then(|c| c.claim(&pool))
                .is_some()
        {
            return Err(EconomicError::InvalidInput);
        }
        // The exact subatom carry remains attached to this pool identity.
        Ok(())
    }
    fn apply_amm_capital(&mut self, input: AmmCapitalInput, height: u64) -> Result<()> {
        let config = self
            .config
            .amm
            .as_ref()
            .ok_or(EconomicError::InvalidInput)?
            .clone();
        self.amm_quiescent()?;
        let (txid, owner) = amm_capital_identity(&input);
        normal_owner(owner)?;
        self.joint
            .note_economic_input_unverified(txid, height, 10)?;
        let cash = self.amm.cash_atoms;
        let total = self.amm.total_units;
        let old = self.amm.shares.get(owner).copied().unwrap_or(0);
        let (kind, atoms, units) = amm_capital_plan(&input, cash, total, old)?;
        let pool = self.amm_owner()?;
        match kind {
            AmmCapitalKind::Deposit => {
                if add(cash, atoms)? > config.max_collateral_atoms {
                    return Err(EconomicError::Capacity);
                }
                if old == 0 && self.amm.shares.len() >= MAX_AMM_LPS {
                    return Err(EconomicError::Capacity);
                }
                self.transfer(owner.clone(), pool, atoms)?;
                self.amm.total_units = add(total, units)?;
                self.amm.shares.insert(owner.clone(), add(old, units)?);
            }
            AmmCapitalKind::Redeem => {
                self.transfer(pool, owner.clone(), atoms)?;
                self.amm.total_units = total.checked_sub(units).ok_or(EconomicError::Invariant)?;
                let remaining = old.checked_sub(units).ok_or(EconomicError::Invariant)?;
                if remaining == 0 {
                    self.amm.shares.remove(owner);
                } else {
                    self.amm.shares.insert(owner.clone(), remaining);
                }
            }
        }
        let receipt = AmmCapitalReceipt {
            txid,
            owner: owner.clone(),
            kind,
            collateral_atoms: atoms,
            share_units: units,
            cash_before_atoms: cash,
            total_units_before: total,
            owner_units_before: old,
        };
        let frame = self.amm.frames.last_mut().ok_or(EconomicError::Invariant)?;
        frame.capital_inputs.push(input);
        frame.capital_receipts.push(receipt);
        self.amm_record_count()?;
        Ok(())
    }
    fn refresh_amm_quotes(&mut self, height: u64) -> Result<()> {
        if self.config.amm.is_none() {
            return Ok(());
        }
        let Some((mark_id, price)) = self.marking.events.iter().rev().find_map(|event| {
            if event.height == height {
                if let MarkEventKind::Mark { id, observation } = &event.kind {
                    return Some((*id, observation.price.clone()));
                }
            }
            None
        }) else {
            return Ok(());
        };
        if !self.amm.quotes.is_empty() {
            return Err(EconomicError::Invariant);
        }
        let pool = self.amm_owner()?;
        let generation = self
            .amm
            .generation
            .checked_add(1)
            .ok_or(EconomicError::Arithmetic)?;
        let mut epoch = AmmQuoteEpoch {
            id: self.amm_epoch_id(height, mark_id, generation)?,
            height,
            mark_id,
            generation,
            position_before: self.position(&pool).contracts,
            cash_before_atoms: self.amm.cash_atoms,
            margin_before_atoms: self.margin_atoms(&pool),
            unpaid_loss_before_atoms: self.marking.unpaid_losses.get(&pool).copied().unwrap_or(0),
            cash_leg_count_before: u32::try_from(
                self.amm
                    .frames
                    .last()
                    .ok_or(EconomicError::Invariant)?
                    .cash_legs
                    .len(),
            )
            .map_err(|_| EconomicError::Capacity)?,
            quotes: Vec::new(),
        };
        epoch.quotes = self.plan_amm_quotes(&epoch, &price)?;
        self.joint
            .note_economic_epoch_unverified(epoch.id, height)?;
        for quote in &epoch.quotes {
            self.transfer(
                pool.clone(),
                self.amm_quote_escrow(quote.side)?,
                quote.reserve_atoms,
            )?;
        }
        self.amm.generation = generation;
        self.amm.quotes = epoch.quotes.clone();
        self.amm
            .frames
            .last_mut()
            .ok_or(EconomicError::Invariant)?
            .quote_epoch = Some(epoch);
        self.amm_record_count()?;
        Ok(())
    }
    fn plan_amm_quotes(
        &self,
        epoch: &AmmQuoteEpoch,
        mark: &Decimal,
    ) -> Result<Vec<FundedAmmQuote>> {
        let config = self
            .config
            .amm
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        if epoch.unpaid_loss_before_atoms != 0
            || epoch.margin_before_atoms < self.required_margin(&epoch.position_before.abs())?
            || epoch.position_before.abs() > config.max_abs_inventory
        {
            return Ok(Vec::new());
        }
        let price: f64 = mark
            .to_string()
            .parse()
            .map_err(|_| EconomicError::Arithmetic)?;
        let source_pool = crate::amm::AmmPool::new(1.0, 1.0, 1.0, self.config.contract.get())
            .map_err(|_| EconomicError::InvalidConfig)?;
        let prices = source_pool
            .quote_contract_orders(price, epoch.height, config.spread_bps as f64)
            .map_err(|_| EconomicError::Arithmetic)?;
        let budget = epoch.cash_before_atoms / 2;
        let affordable = tokens(budget)?.checked_div(
            &self.config.initial_margin_per_contract,
            20,
            Rounding::Down,
        )?;
        let mut quotes = Vec::new();
        for source in prices {
            if source.validate_executable().is_err() {
                continue;
            }
            let side = if source.side == crate::amm::Side::Buy {
                Side::Buy
            } else {
                Side::Sell
            };
            let capacity = if side == Side::Buy {
                config
                    .max_abs_inventory
                    .checked_sub(&epoch.position_before)?
            } else {
                config
                    .max_abs_inventory
                    .checked_add(&epoch.position_before)?
            };
            let quantity = config
                .max_quote_quantity
                .clone()
                .min(capacity)
                .min(affordable.clone());
            if quantity <= Decimal::zero() {
                continue;
            }
            let price: Decimal = source.price.to_string().parse()?;
            bounded(&price)?;
            let reserve_atoms = self.required_margin(&quantity)?;
            if reserve_atoms > budget {
                return Err(EconomicError::Invariant);
            }
            quotes.push(FundedAmmQuote {
                epoch_id: epoch.id,
                pool_id: config.pool_id,
                generation: epoch.generation,
                height: epoch.height,
                mark_id: epoch.mark_id,
                side,
                price,
                remaining: quantity,
                reserve_atoms,
            });
        }
        Ok(quotes)
    }
    fn amm_quote_fill_quantity(
        &self,
        quote: &FundedAmmQuote,
        distressed_quantity: &Decimal,
        pool_quantity: &Decimal,
        guard_price: &Decimal,
    ) -> Result<Decimal> {
        let config = self
            .config
            .amm
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let desired = if distressed_quantity.is_negative() {
            Side::Sell
        } else {
            Side::Buy
        };
        if distressed_quantity.is_zero()
            || quote.side != desired
            || quote.price <= Decimal::zero()
            || quote.remaining <= Decimal::zero()
            || (desired == Side::Buy && quote.price < *guard_price)
            || (desired == Side::Sell && quote.price > *guard_price)
        {
            return Ok(Decimal::zero());
        }
        let capacity = if quote.side == Side::Buy {
            config.max_abs_inventory.checked_sub(pool_quantity)?
        } else {
            config.max_abs_inventory.checked_add(pool_quantity)?
        };
        Ok(quote
            .remaining
            .clone()
            .min(distressed_quantity.abs())
            .min(capacity.max(Decimal::zero())))
    }
    fn next_funded_amm_candidate(
        &self,
        event: &LiquidationEvent,
        quantity: &Decimal,
    ) -> Result<Option<(FundedAmmQuote, Decimal)>> {
        if self.config.amm.is_none() || self.is_amm_owner(&event.owner) {
            return Ok(None);
        }
        let pool = self.amm_owner()?;
        for quote in &self.amm.quotes {
            if quote.height != event.height || quote.mark_id != event.mark_id {
                return Err(EconomicError::Invariant);
            }
            let fill = self.amm_quote_fill_quantity(
                quote,
                quantity,
                &self.position(&pool).contracts,
                &event.guard_price,
            )?;
            if !fill.is_zero() {
                return Ok(Some((quote.clone(), fill)));
            }
        }
        Ok(None)
    }
}

fn amm_capital_identity(input: &AmmCapitalInput) -> (TxId, &AccountId) {
    match input {
        AmmCapitalInput::Deposit { txid, owner, .. }
        | AmmCapitalInput::Redeem { txid, owner, .. } => (*txid, owner),
    }
}
fn amm_floor_ratio(first: u128, second: u128, divisor: u128) -> Result<u128> {
    if divisor == 0 {
        return Err(EconomicError::Arithmetic);
    }
    let first: Decimal = first.to_string().parse()?;
    let second: Decimal = second.to_string().parse()?;
    let divisor: Decimal = divisor.to_string().parse()?;
    first
        .checked_mul(&second)?
        .checked_div(&divisor, 0, Rounding::Down)?
        .to_string()
        .parse()
        .map_err(|_| EconomicError::Arithmetic)
}
fn amm_capital_plan(
    input: &AmmCapitalInput,
    cash: u128,
    total: u128,
    owned: u128,
) -> Result<(AmmCapitalKind, u128, u128)> {
    match input {
        AmmCapitalInput::Deposit {
            collateral_atoms,
            min_share_units,
            ..
        } => {
            if *collateral_atoms == 0 || (total == 0 && cash != 0) || (total != 0 && cash == 0) {
                return Err(EconomicError::InvalidInput);
            }
            let units = if total == 0 {
                *collateral_atoms
            } else {
                amm_floor_ratio(*collateral_atoms, total, cash)?
            };
            if units == 0 || units < *min_share_units {
                return Err(EconomicError::InvalidInput);
            }
            Ok((AmmCapitalKind::Deposit, *collateral_atoms, units))
        }
        AmmCapitalInput::Redeem {
            share_units,
            min_collateral_atoms,
            ..
        } => {
            if *share_units == 0 || *share_units > owned || *share_units > total {
                return Err(EconomicError::InvalidInput);
            }
            let atoms = if *share_units == total {
                cash
            } else {
                amm_floor_ratio(*share_units, cash, total)?
            };
            if atoms == 0 || atoms < *min_collateral_atoms {
                return Err(EconomicError::InvalidInput);
            }
            Ok((AmmCapitalKind::Redeem, atoms, *share_units))
        }
    }
}

impl EconomicState {
    fn settle_forced_amm_candidate(
        &mut self,
        event: &LiquidationEvent,
        quote: &FundedAmmQuote,
        quantity: &Decimal,
    ) -> Result<AmmForcedCloseReceipt> {
        let owner = &event.owner;
        let pool = self.amm_owner()?;
        let old = self.position(owner);
        let maker_old = self.position(&pool);
        let fresh = self
            .next_funded_amm_candidate(event, &old.contracts)?
            .ok_or(EconomicError::Invariant)?;
        if fresh.0 != *quote || fresh.1 != *quantity {
            return Err(EconomicError::Invariant);
        }
        let maker_available_before_atoms = self.amm.cash_atoms;
        let maker_margin_before_atoms = self.margin_atoms(&pool);
        let side = if old.contracts.is_negative() {
            Side::Buy
        } else {
            Side::Sell
        };
        let remaining = quote.remaining.checked_sub(quantity)?;
        let reserve_after = self.required_margin(&remaining)?;
        let reserve_used = quote
            .reserve_atoms
            .checked_sub(reserve_after)
            .ok_or(EconomicError::Invariant)?;
        let index = self
            .amm
            .quotes
            .iter()
            .position(|q| q == quote)
            .ok_or(EconomicError::Invariant)?;
        if remaining.is_zero() {
            self.amm.quotes.remove(index);
        } else {
            self.amm.quotes[index].remaining = remaining;
            self.amm.quotes[index].reserve_atoms = reserve_after;
        }
        self.trade_position(&old, quantity, &quote.price, side)?;
        self.trade_position(&maker_old, quantity, &quote.price, quote.side)?;
        self.transfer(
            self.amm_quote_escrow(quote.side)?,
            self.margin_escrow(&pool)?,
            reserve_used,
        )?;
        self.set_margin(pool.clone(), add(maker_margin_before_atoms, reserve_used)?);
        self.reconcile_margin(&pool)?;
        let target = self.required_margin(&self.position(owner).contracts.abs())?;
        let margin = self.margin_atoms(owner);
        if margin > target {
            self.transfer(self.margin_escrow(owner)?, owner.clone(), margin - target)?;
            self.set_margin(owner.clone(), target);
        }
        let distressed_available_before_fee_atoms =
            self.ledger().available_atoms(self.config.collateral, owner);
        let distressed_margin_before_fee_atoms = self.margin_atoms(owner);
        let nominal_fee_atoms = self.forced_nominal_fee(quantity, &quote.price, side)?;
        let available_fee = nominal_fee_atoms.min(distressed_available_before_fee_atoms);
        let margin_fee =
            (nominal_fee_atoms - available_fee).min(distressed_margin_before_fee_atoms);
        let charged_fee_atoms = add(available_fee, margin_fee)?;
        let maker_rebate_atoms = charged_fee_atoms / 2;
        let retained_fee_atoms = charged_fee_atoms - maker_rebate_atoms;
        let fee_escrow = self.escrow(b"fees", &[])?;
        self.transfer(owner.clone(), fee_escrow.clone(), available_fee)?;
        self.transfer(self.margin_escrow(owner)?, fee_escrow.clone(), margin_fee)?;
        self.set_margin(
            owner.clone(),
            distressed_margin_before_fee_atoms - margin_fee,
        );
        self.transfer(fee_escrow.clone(), pool.clone(), maker_rebate_atoms)?;
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
        let delta = if side == Side::Buy {
            quantity.clone()
        } else {
            quantity.negated()
        };
        let mut entries = vec![
            self.mark_entry(
                owner,
                BasisAction::Trade {
                    delta: delta.clone(),
                    price: quote.price.clone(),
                },
            )?,
            self.mark_entry(
                &pool,
                BasisAction::Trade {
                    delta: delta.negated(),
                    price: quote.price.clone(),
                },
            )?,
        ];
        let distressed_pnl = entries[0].atoms.to_decimal();
        let maker_pnl = entries[1].atoms.to_decimal();
        let (collected_loss_atoms, unfunded_loss_atoms, paid_claim_atoms) =
            self.settle_mark_entries(&mut entries, event.height)?;
        self.record_mark_event(MarkEvent {
            height: event.height,
            kind: MarkEventKind::AmmForced {
                epoch_id: event.id,
                fill_index: event.amm_fills.len() as u32,
                pool_id: quote.pool_id,
                distressed_owner: owner.clone(),
            },
            entries,
            collected_loss_atoms,
            paid_claim_atoms,
            unfunded_loss_atoms,
        })?;
        Ok(AmmForcedCloseReceipt {
            quote_before: quote.clone(),
            quantity: quantity.clone(),
            price: quote.price.clone(),
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
    fn amm_forced_mark_event(&self, id: TxId, index: u32) -> Result<&MarkEvent> {
        let mut found = self.marking.events.iter().filter(|event| matches!(&event.kind,
            MarkEventKind::AmmForced {epoch_id,fill_index,..} if *epoch_id == id && *fill_index == index));
        let event = found.next().ok_or(EconomicError::Invariant)?;
        if found.next().is_some() {
            return Err(EconomicError::Invariant);
        }
        Ok(event)
    }
    fn verify_amm_forced_mark_entry(
        &self,
        event: &MarkEvent,
        bases: &BTreeMap<AccountId, BasisPosition>,
    ) -> Result<()> {
        let MarkEventKind::AmmForced {
            epoch_id,
            fill_index,
            pool_id,
            distressed_owner,
        } = &event.kind
        else {
            return Err(EconomicError::Invariant);
        };
        let config = self.config.amm.as_ref().ok_or(EconomicError::Invariant)?;
        let parent = self
            .liquidation
            .events
            .iter()
            .find(|e| e.id == *epoch_id)
            .ok_or(EconomicError::Invariant)?;
        let receipt = self.forced_amm_receipt(*epoch_id, *fill_index)?;
        let pool = self.amm_owner()?;
        if *pool_id != config.pool_id
            || event.height != parent.height
            || *distressed_owner != parent.owner
            || parent.owner == pool
            || event.entries.len() != 2
            || event.entries[0].owner != parent.owner
            || event.entries[1].owner != pool
            || event.collected_loss_atoms != receipt.collected_loss_atoms
            || event.paid_claim_atoms != receipt.paid_claim_atoms
            || event.unfunded_loss_atoms != receipt.unfunded_loss_atoms
            || receipt.quote_before.pool_id != *pool_id
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
    fn amm_record_count(&self) -> Result<usize> {
        if self.amm.shares.len() > MAX_AMM_LPS
            || self.amm.frames.len() > MAX_AMM_FRAMES
            || self.amm.quotes.len() > 2
        {
            return Err(EconomicError::Capacity);
        }
        let mut count = self.amm.shares.len() + self.amm.quotes.len();
        for frame in &self.amm.frames {
            if frame.released_quotes.len() > 2
                || frame.capital_inputs.len() > MAX_ECONOMIC_INPUTS
                || frame.capital_receipts.len() > MAX_ECONOMIC_INPUTS
                || frame
                    .quote_epoch
                    .as_ref()
                    .is_some_and(|e| e.quotes.len() > 2)
            {
                return Err(EconomicError::Capacity);
            }
            for addend in [
                frame.released_quotes.len(),
                frame.capital_inputs.len(),
                frame.capital_receipts.len(),
                usize::from(frame.quote_epoch.is_some()),
                frame.quote_epoch.as_ref().map_or(0, |e| e.quotes.len()),
                frame.cash_legs.len(),
            ] {
                count = count.checked_add(addend).ok_or(EconomicError::Capacity)?;
                if count > MAX_AMM_RECORDS {
                    return Err(EconomicError::Capacity);
                }
            }
        }
        Ok(count)
    }
    fn verify_amm(&self) -> Result<()> {
        if self.config.amm.is_none() {
            return if self.amm == FundedAmmState::default() {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        }
        self.amm_record_count()?;
        self.verify_amm_cash_history()?;
        if self.amm.frames.len() as u64 != self.finalized_height().unwrap_or(0) {
            return Err(EconomicError::Invariant);
        }
        let mut shares = BTreeMap::<AccountId, u128>::new();
        let mut total = 0;
        let mut cash = 0u128;
        let mut generation = 0u64;
        let mut last_sequence = None;
        let pool = self.amm_owner()?;
        let mut ids = BTreeSet::new();
        for (index, frame) in self.amm.frames.iter().enumerate() {
            if frame.height != index as u64 + 1
                || frame.capital_inputs.len() != frame.capital_receipts.len()
                || (!frame.capital_inputs.is_empty()
                    && self
                        .joint
                        .registry()
                        .activation(10)
                        .is_none_or(|activation| activation.height >= frame.height))
            {
                return Err(EconomicError::Invariant);
            }
            // Cash is a projection from the closed pool's actual internal-leg
            // journal. Captured external wallet authority remains unverified.
            let frame_cash_before = cash;
            if frame.released_quotes.len() > frame.cash_legs.len() {
                return Err(EconomicError::Invariant);
            }
            for (quote, leg) in frame.released_quotes.iter().zip(&frame.cash_legs) {
                if leg.from_pool
                    || leg.counterparty != self.amm_quote_escrow(quote.side)?
                    || leg.atoms != quote.reserve_atoms
                {
                    return Err(EconomicError::Invariant);
                }
            }
            let mut capital_cursor = 0usize;
            for (input, receipt) in frame.capital_inputs.iter().zip(&frame.capital_receipts) {
                let (id, owner) = amm_capital_identity(input);
                normal_owner(owner)?;
                let owned = shares.get(owner).copied().unwrap_or(0);
                if id != receipt.txid
                    || *owner != receipt.owner
                    || !ids.insert(id)
                    || !self.joint.economic_input_ids_unverified().contains(&id)
                    || receipt.total_units_before != total
                    || receipt.owner_units_before != owned
                {
                    return Err(EconomicError::Invariant);
                }
                // Find the unique corresponding physical LP cash leg; quote
                // refunds can precede it in the same block.
                while capital_cursor < frame.cash_legs.len() {
                    let leg = &frame.cash_legs[capital_cursor];
                    if leg.counterparty == *owner {
                        break;
                    }
                    capital_cursor += 1;
                }
                let leg = frame
                    .cash_legs
                    .get(capital_cursor)
                    .ok_or(EconomicError::Invariant)?;
                let before = amm_cash_before_leg(cash, &frame.cash_legs[..capital_cursor])?;
                let (kind, atoms, units) = amm_capital_plan(input, before, total, owned)?;
                if receipt.cash_before_atoms != before
                    || receipt.kind != kind
                    || receipt.collateral_atoms != atoms
                    || receipt.share_units != units
                    || leg.atoms != atoms
                    || leg.from_pool != (kind == AmmCapitalKind::Redeem)
                {
                    return Err(EconomicError::Invariant);
                }
                if kind == AmmCapitalKind::Deposit {
                    if add(before, atoms)?
                        > self
                            .config
                            .amm
                            .as_ref()
                            .ok_or(EconomicError::Invariant)?
                            .max_collateral_atoms
                    {
                        return Err(EconomicError::Capacity);
                    }
                    total = add(total, units)?;
                    shares.insert(owner.clone(), add(owned, units)?);
                } else {
                    total = total.checked_sub(units).ok_or(EconomicError::Invariant)?;
                    let left = owned.checked_sub(units).ok_or(EconomicError::Invariant)?;
                    if left == 0 {
                        shares.remove(owner);
                    } else {
                        shares.insert(owner.clone(), left);
                    }
                }
                capital_cursor += 1;
            }
            if frame
                .quote_epoch
                .as_ref()
                .is_some_and(|epoch| capital_cursor > epoch.cash_leg_count_before as usize)
            {
                return Err(EconomicError::Invariant);
            }
            for leg in &frame.cash_legs {
                if leg.atoms == 0
                    || leg.counterparty == pool
                    || leg.sequence >= self.internal_sequence
                    || last_sequence.is_some_and(|previous| leg.sequence <= previous)
                    || !self.ledger().is_processed(self.leg_id(leg.sequence))
                {
                    return Err(EconomicError::Invariant);
                }
                last_sequence = Some(leg.sequence);
                cash = amm_apply_cash(cash, leg)?;
            }
            let mark = self.marking.events.iter().find_map(|event| {
                if event.height == frame.height {
                    if let MarkEventKind::Mark { id, observation } = &event.kind {
                        return Some((*id, &observation.price));
                    }
                }
                None
            });
            match (mark, &frame.quote_epoch) {
                (None, None) => {}
                (Some((mark_id, price)), Some(epoch)) => {
                    generation = generation.checked_add(1).ok_or(EconomicError::Arithmetic)?;
                    if epoch.height != frame.height
                        || epoch.mark_id != mark_id
                        || epoch.generation != generation
                        || epoch.id != self.amm_epoch_id(frame.height, mark_id, generation)?
                        || epoch.quotes != self.plan_amm_quotes(epoch, price)?
                        || epoch.cash_leg_count_before as usize > frame.cash_legs.len()
                        || epoch.cash_before_atoms
                            != amm_cash_before_leg(
                                frame_cash_before,
                                &frame.cash_legs[..epoch.cash_leg_count_before as usize],
                            )?
                    {
                        return Err(EconomicError::Invariant);
                    }
                }
                _ => return Err(EconomicError::Invariant),
            }
            if let Some(epoch) = &frame.quote_epoch {
                for (offset, quote) in epoch.quotes.iter().enumerate() {
                    let leg = frame
                        .cash_legs
                        .get(epoch.cash_leg_count_before as usize + offset)
                        .ok_or(EconomicError::Invariant)?;
                    if !leg.from_pool
                        || leg.counterparty != self.amm_quote_escrow(quote.side)?
                        || leg.atoms != quote.reserve_atoms
                    {
                        return Err(EconomicError::Invariant);
                    }
                }
            }
        }
        if shares != self.amm.shares
            || total != self.amm.total_units
            || cash != self.amm.cash_atoms
            || generation != self.amm.generation
            || cash != self.ledger().available_atoms(self.config.collateral, &pool)
        {
            return Err(EconomicError::CustodyMismatch);
        }
        Ok(())
    }
    fn verify_amm_capital_boundary(
        &self,
        frame: &AmmFrame,
        bases: &BTreeMap<AccountId, BasisPosition>,
        ious: &IouStore,
        unpaid_losses: &BTreeMap<AccountId, u128>,
    ) -> Result<()> {
        if frame.capital_inputs.is_empty() {
            return Ok(());
        }
        let pool = self.amm_owner()?;
        if bases
            .get(&pool)
            .is_some_and(|basis| !basis.quantity.is_zero())
            || unpaid_losses.get(&pool).copied().unwrap_or(0) != 0
            || ious
                .claim_map(self.iou_key())
                .and_then(|claims| claims.claim(&pool))
                .is_some()
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}

fn amm_apply_cash(cash: u128, leg: &AmmCashLeg) -> Result<u128> {
    if leg.from_pool {
        cash.checked_sub(leg.atoms).ok_or(EconomicError::Invariant)
    } else {
        add(cash, leg.atoms)
    }
}
fn amm_cash_before_leg(mut cash: u128, legs: &[AmmCashLeg]) -> Result<u128> {
    for leg in legs {
        cash = amm_apply_cash(cash, leg)?;
    }
    Ok(cash)
}
