// Actual sole-Ledger oracle-perpetual backstop. This named repair policy is
// separate from legacy insurance.js and never imports its balance declarations.
pub const MAX_INSURANCE_RECEIPTS: usize = 4096;
pub const MAX_INSURANCE_RECORDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsurancePolicy {
    ConservingOracleBackstopV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceFundIdentity {
    pub network: TradeLayerMainnet,
    pub contract: ContractId,
    pub collateral: PropertyId,
    pub creation_txid: TxId,
    pub definition_hash: [u8; 32],
    pub oracle_id: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceConfig {
    pub fund: InsuranceFundIdentity,
    pub sponsor: AccountId,
    pub operator: AccountId,
    pub policy: InsurancePolicy,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsuranceAction {
    Deposit { atoms: u128 },
    AdoptFees { max_atoms: u128 },
    Backstop { max_atoms: u128 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedInsuranceInput {
    pub txid: TxId,
    pub actor: AccountId,
    pub action: InsuranceAction,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceAmount {
    pub owner: AccountId,
    pub atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceBackstopPlan {
    pub pool_before_atoms: u128,
    pub claims_before: Vec<InsuranceAmount>,
    pub unpaid_losses_before: Vec<InsuranceAmount>,
    pub existing_funded_payouts: Vec<InsuranceAmount>,
    pub cover_atoms: u128,
    pub debt_reductions: Vec<InsuranceAmount>,
    pub insurer_funded_payouts: Vec<InsuranceAmount>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsuranceReceipt {
    pub height: u64,
    pub input: UnverifiedInsuranceInput,
    pub fund_id: [u8; 32],
    pub reserve_before_atoms: u128,
    pub reserve_after_atoms: u128,
    pub pending_before_atoms: u128,
    pub pending_after_atoms: u128,
    pub internal_sequence_before: u64,
    pub internal_sequence_after: u64,
    pub plan: Option<InsuranceBackstopPlan>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InsuranceState {
    pub reserve_atoms: u128,
    pub deposited_atoms: u128,
    pub adopted_fee_atoms: u128,
    pub covered_atoms: u128,
    pub receipts: Vec<InsuranceReceipt>,
}

impl EconomicConfig {
    fn validate_insurance(&self) -> Result<()> {
        let Some(insurance) = &self.insurance else {
            return Ok(());
        };
        normal_owner(&insurance.sponsor)?;
        normal_owner(&insurance.operator)?;
        let binding = self
            .contract_binding
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let marking = self.marking.as_ref().ok_or(EconomicError::InvalidConfig)?;
        if self.liquidation.is_none()
            || self.source_consensus.is_some()
            || insurance.fund.network != self.network
            || insurance.fund.contract != self.contract
            || insurance.fund.collateral != self.collateral
            || insurance.fund.creation_txid != binding.creation_txid
            || insurance.fund.definition_hash != binding.definition_hash
            || insurance.fund.oracle_id != marking.oracle_id
            || self.fee_route != FeeRoute::Oracle(self.contract.get())
        {
            return Err(EconomicError::InvalidConfig);
        }
        Ok(())
    }
}
impl EconomicState {
    pub fn insurance_state(&self) -> &InsuranceState {
        &self.insurance
    }
    pub fn insurance_reserve_account(&self) -> Result<AccountId> {
        self.escrow(b"insurance-reserve", &self.insurance_fund_id()?)
    }
    fn insurance_fund_id(&self) -> Result<[u8; 32]> {
        let config = self
            .config
            .insurance
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let mut out = EconomicWriter(b"TradeLayer/OracleInsuranceFund/v1".to_vec());
        insurance_write_identity(&mut out, &config.fund)?;
        Ok(double_sha256(&out.0))
    }
    fn require_insurance_market(&self) -> Result<()> {
        self.config.validate_insurance()?;
        let fund = &self
            .config
            .insurance
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?
            .fund;
        let accepted = self.selected_contract()?;
        self.validate_selected_contract(accepted)?;
        if accepted.definition.parameters.native
            || accepted.definition.parameters.expiry_period != 0
            || accepted.creation_txid != fund.creation_txid
            || accepted.definition_hash != fund.definition_hash
            || accepted.definition.parameters.underlying_oracle_id != Some(fund.oracle_id)
        {
            return Err(EconomicError::InvalidInput);
        }
        Ok(())
    }
    fn require_insurance_input(&self, input: &UnverifiedInsuranceInput, height: u64) -> Result<()> {
        self.require_insurance_market()?;
        normal_owner(&input.actor)?;
        let config = self
            .config
            .insurance
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let (expected, amount) = match input.action {
            InsuranceAction::Deposit { atoms } => (&config.sponsor, atoms),
            InsuranceAction::AdoptFees { max_atoms } | InsuranceAction::Backstop { max_atoms } => {
                (&config.operator, max_atoms)
            }
        };
        if &input.actor != expected
            || amount == 0
            || self
                .joint
                .registry()
                .activation(18)
                .is_none_or(|a| a.height >= height)
        {
            return Err(EconomicError::InvalidInput);
        }
        Ok(())
    }
    fn apply_insurance_capital(
        &mut self,
        input: UnverifiedInsuranceInput,
        height: u64,
    ) -> Result<()> {
        let receipt = self.insurance_receipt_start(input, height)?;
        let reserve = self.insurance_reserve_account()?;
        match receipt.input.action {
            InsuranceAction::Deposit { atoms } => {
                self.transfer(receipt.input.actor.clone(), reserve, atoms)?;
                self.insurance.reserve_atoms = add(self.insurance.reserve_atoms, atoms)?;
                self.insurance.deposited_atoms = add(self.insurance.deposited_atoms, atoms)?;
            }
            InsuranceAction::AdoptFees { max_atoms } => {
                let atoms = max_atoms.min(self.insurance_custody_atoms);
                if atoms == 0 {
                    return Err(EconomicError::InvalidInput);
                }
                self.transfer(self.escrow(b"insurance-pending", &[])?, reserve, atoms)?;
                self.insurance_custody_atoms -= atoms;
                self.insurance.reserve_atoms = add(self.insurance.reserve_atoms, atoms)?;
                self.insurance.adopted_fee_atoms = add(self.insurance.adopted_fee_atoms, atoms)?;
            }
            InsuranceAction::Backstop { .. } => return Err(EconomicError::InvalidInput),
        }
        self.finish_insurance_receipt(receipt)
    }
    fn insurance_receipt_start(
        &self,
        input: UnverifiedInsuranceInput,
        height: u64,
    ) -> Result<InsuranceReceipt> {
        self.require_insurance_input(&input, height)?;
        self.insurance_record_count()?;
        if self.insurance.receipts.len() >= MAX_INSURANCE_RECEIPTS {
            return Err(EconomicError::Capacity);
        }
        Ok(InsuranceReceipt {
            height,
            input,
            fund_id: self.insurance_fund_id()?,
            reserve_before_atoms: self.insurance.reserve_atoms,
            reserve_after_atoms: 0,
            pending_before_atoms: self.insurance_custody_atoms,
            pending_after_atoms: 0,
            internal_sequence_before: self.internal_sequence,
            internal_sequence_after: 0,
            plan: None,
        })
    }
    fn finish_insurance_receipt(&mut self, mut receipt: InsuranceReceipt) -> Result<()> {
        receipt.reserve_after_atoms = self.insurance.reserve_atoms;
        receipt.pending_after_atoms = self.insurance_custody_atoms;
        receipt.internal_sequence_after = self.internal_sequence;
        self.insurance.receipts.push(receipt);
        self.insurance_record_count()?;
        Ok(())
    }
    fn run_insurance_backstop(
        &mut self,
        input: UnverifiedInsuranceInput,
        height: u64,
    ) -> Result<()> {
        let mut receipt = self.insurance_receipt_start(input, height)?;
        let InsuranceAction::Backstop { max_atoms } = receipt.input.action else {
            return Err(EconomicError::InvalidInput);
        };
        let plan = insurance_backstop_plan(
            &self.ious,
            self.iou_key(),
            &self.marking.unpaid_losses,
            self.loss_pool_atoms,
            self.insurance.reserve_atoms,
            max_atoms,
        )?;
        self.preflight_insurance_plan(&plan)?;
        let existing = self.pay_claims(height)?;
        if existing != insurance_total(&plan.existing_funded_payouts)? {
            return Err(EconomicError::Invariant);
        }
        self.transfer(
            self.insurance_reserve_account()?,
            self.escrow(b"loss-pool", &[])?,
            plan.cover_atoms,
        )?;
        self.insurance.reserve_atoms -= plan.cover_atoms;
        self.insurance.covered_atoms = add(self.insurance.covered_atoms, plan.cover_atoms)?;
        self.loss_pool_atoms = add(self.loss_pool_atoms, plan.cover_atoms)?;
        self.ious
            .add_loss(self.iou_key(), &tokens(plan.cover_atoms)?, height)?;
        if self.pay_claims(height)? != plan.cover_atoms {
            return Err(EconomicError::Invariant);
        }
        insurance_reduce_debts(&mut self.marking.unpaid_losses, &plan.debt_reductions)?;
        receipt.plan = Some(plan);
        self.finish_insurance_receipt(receipt)
    }
    fn preflight_insurance_plan(&self, plan: &InsuranceBackstopPlan) -> Result<()> {
        let extra = insurance_plan_records(plan)?;
        if self
            .insurance_record_count()?
            .checked_add(extra)
            .and_then(|n| n.checked_add(1))
            .is_none_or(|n| n > MAX_INSURANCE_RECORDS)
        {
            return Err(EconomicError::Capacity);
        }
        let legs = plan
            .existing_funded_payouts
            .len()
            .checked_add(plan.insurer_funded_payouts.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(EconomicError::Capacity)?;
        if self
            .internal_sequence
            .checked_add(legs as u64)
            .is_none_or(|n| n > MAX_ECONOMIC_LEGS)
        {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
    fn insurance_record_count(&self) -> Result<usize> {
        if self.insurance.receipts.len() > MAX_INSURANCE_RECEIPTS {
            return Err(EconomicError::Capacity);
        }
        let mut total = self.insurance.receipts.len();
        for receipt in &self.insurance.receipts {
            if let Some(plan) = &receipt.plan {
                total = total
                    .checked_add(insurance_plan_records(plan)?)
                    .ok_or(EconomicError::Capacity)?;
            }
            if total > MAX_INSURANCE_RECORDS {
                return Err(EconomicError::Capacity);
            }
        }
        Ok(total)
    }
    fn verify_insurance(&self) -> Result<()> {
        let Some(config) = &self.config.insurance else {
            return if self.insurance == InsuranceState::default() {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        };
        self.config.validate_insurance()?;
        self.insurance_record_count()?;
        let fund_id = self.insurance_fund_id()?;
        let mut reserve = 0;
        let mut deposits = 0;
        let mut adopted = 0;
        let mut covered = 0;
        let mut last_height = 0;
        let mut last_sequence = 0;
        let mut ids = BTreeSet::new();
        for receipt in &self.insurance.receipts {
            self.require_insurance_input(&receipt.input, receipt.height)?;
            if receipt.fund_id != fund_id
                || receipt.height < last_height
                || receipt.height > self.finalized_height().unwrap_or(0)
                || receipt.reserve_before_atoms != reserve
                || receipt.internal_sequence_before < last_sequence
                || receipt.internal_sequence_after > self.internal_sequence
                || !ids.insert(receipt.input.txid)
                || !self
                    .joint
                    .economic_input_ids_unverified()
                    .contains(&receipt.input.txid)
            {
                return Err(EconomicError::Invariant);
            }
            let legs = match receipt.input.action {
                InsuranceAction::Deposit { atoms } => {
                    if receipt.plan.is_some()
                        || receipt.input.actor != config.sponsor
                        || receipt.pending_before_atoms != receipt.pending_after_atoms
                    {
                        return Err(EconomicError::Invariant);
                    }
                    deposits = add(deposits, atoms)?;
                    reserve = add(reserve, atoms)?;
                    1
                }
                InsuranceAction::AdoptFees { max_atoms } => {
                    let atoms = max_atoms.min(receipt.pending_before_atoms);
                    if atoms == 0
                        || receipt.plan.is_some()
                        || receipt.input.actor != config.operator
                        || receipt.pending_before_atoms.checked_sub(atoms)
                            != Some(receipt.pending_after_atoms)
                    {
                        return Err(EconomicError::Invariant);
                    }
                    adopted = add(adopted, atoms)?;
                    reserve = add(reserve, atoms)?;
                    1
                }
                InsuranceAction::Backstop { max_atoms } => {
                    let plan = receipt.plan.as_ref().ok_or(EconomicError::Invariant)?;
                    if receipt.input.actor != config.operator
                        || receipt.pending_before_atoms != receipt.pending_after_atoms
                        || plan.cover_atoms == 0
                        || plan.cover_atoms > max_atoms
                        || plan.cover_atoms > reserve
                    {
                        return Err(EconomicError::Invariant);
                    }
                    reserve -= plan.cover_atoms;
                    covered = add(covered, plan.cover_atoms)?;
                    1 + plan.existing_funded_payouts.len() + plan.insurer_funded_payouts.len()
                }
            };
            if receipt.reserve_after_atoms != reserve
                || receipt.internal_sequence_before.checked_add(legs as u64)
                    != Some(receipt.internal_sequence_after)
            {
                return Err(EconomicError::Invariant);
            }
            last_height = receipt.height;
            last_sequence = receipt.internal_sequence_after;
        }
        if (reserve, deposits, adopted, covered)
            != (
                self.insurance.reserve_atoms,
                self.insurance.deposited_atoms,
                self.insurance.adopted_fee_atoms,
                self.insurance.covered_atoms,
            )
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}

fn insurance_plan_records(plan: &InsuranceBackstopPlan) -> Result<usize> {
    let mut total = 0usize;
    for list in [
        &plan.claims_before,
        &plan.unpaid_losses_before,
        &plan.existing_funded_payouts,
        &plan.debt_reductions,
        &plan.insurer_funded_payouts,
    ] {
        if list.len() > MAX_ECONOMIC_POSITIONS {
            return Err(EconomicError::Capacity);
        }
        total = total
            .checked_add(list.len())
            .ok_or(EconomicError::Capacity)?;
    }
    Ok(total)
}
fn insurance_total(rows: &[InsuranceAmount]) -> Result<u128> {
    rows.iter().try_fold(0, |n, row| add(n, row.atoms))
}
fn insurance_claims(ious: &IouStore, key: IouKey) -> Result<Vec<InsuranceAmount>> {
    let Some(map) = ious.claim_map(key) else {
        return Ok(Vec::new());
    };
    if map.claims().len() > MAX_ECONOMIC_POSITIONS {
        return Err(EconomicError::Capacity);
    }
    map.claims()
        .iter()
        .map(|claim| {
            let atoms = atoms_exact(&claim.amount)?;
            if atoms == 0 {
                return Err(EconomicError::Invariant);
            }
            Ok(InsuranceAmount {
                owner: claim.address.clone(),
                atoms,
            })
        })
        .collect()
}
fn insurance_allocate(rows: &mut Vec<InsuranceAmount>, mut atoms: u128) -> Vec<InsuranceAmount> {
    let mut paid = Vec::new();
    for row in rows.iter_mut() {
        let part = row.atoms.min(atoms);
        row.atoms -= part;
        atoms -= part;
        if part != 0 {
            paid.push(InsuranceAmount {
                owner: row.owner.clone(),
                atoms: part,
            });
        }
    }
    rows.retain(|row| row.atoms != 0);
    paid
}
fn insurance_backstop_plan(
    ious: &IouStore,
    key: IouKey,
    debts: &BTreeMap<AccountId, u128>,
    pool: u128,
    reserve: u128,
    max_atoms: u128,
) -> Result<InsuranceBackstopPlan> {
    if debts.len() > MAX_ECONOMIC_POSITIONS {
        return Err(EconomicError::Capacity);
    }
    let claims_before = insurance_claims(ious, key)?;
    let unpaid_losses_before: Vec<_> = debts
        .iter()
        .map(|(owner, atoms)| InsuranceAmount {
            owner: owner.clone(),
            atoms: *atoms,
        })
        .collect();
    let mut claims = claims_before.clone();
    let existing_funded_payouts = insurance_allocate(&mut claims, pool);
    let cover_atoms = max_atoms
        .min(reserve)
        .min(insurance_total(&claims)?)
        .min(insurance_total(&unpaid_losses_before)?);
    if cover_atoms == 0 {
        return Err(EconomicError::InvalidInput);
    }
    let mut debt_copy = unpaid_losses_before.clone();
    let debt_reductions = insurance_allocate(&mut debt_copy, cover_atoms);
    let insurer_funded_payouts = insurance_allocate(&mut claims, cover_atoms);
    let plan = InsuranceBackstopPlan {
        pool_before_atoms: pool,
        claims_before,
        unpaid_losses_before,
        existing_funded_payouts,
        cover_atoms,
        debt_reductions,
        insurer_funded_payouts,
    };
    insurance_plan_records(&plan)?;
    Ok(plan)
}
fn insurance_reduce_debts(
    debts: &mut BTreeMap<AccountId, u128>,
    reductions: &[InsuranceAmount],
) -> Result<()> {
    for row in reductions {
        let before = debts
            .get(&row.owner)
            .copied()
            .ok_or(EconomicError::Invariant)?;
        let after = before
            .checked_sub(row.atoms)
            .ok_or(EconomicError::Invariant)?;
        if after == 0 {
            debts.remove(&row.owner);
        } else {
            debts.insert(row.owner.clone(), after);
        }
    }
    Ok(())
}
fn insurance_replay_pnl(
    receipt: &InsuranceReceipt,
    key: IouKey,
    ious: &mut IouStore,
    debts: &mut BTreeMap<AccountId, u128>,
    pool: &mut u128,
) -> Result<()> {
    let InsuranceAction::Backstop { max_atoms } = receipt.input.action else {
        return Ok(());
    };
    let expected = insurance_backstop_plan(
        ious,
        key,
        debts,
        *pool,
        receipt.reserve_before_atoms,
        max_atoms,
    )?;
    if receipt.plan.as_ref() != Some(&expected) {
        return Err(EconomicError::Invariant);
    }
    let existing = mark_replay_payout(ious, key, pool, receipt.height)?;
    if existing != insurance_total(&expected.existing_funded_payouts)? {
        return Err(EconomicError::Invariant);
    }
    *pool = add(*pool, expected.cover_atoms)?;
    ious.add_loss(key, &tokens(expected.cover_atoms)?, receipt.height)?;
    if mark_replay_payout(ious, key, pool, receipt.height)? != expected.cover_atoms {
        return Err(EconomicError::Invariant);
    }
    insurance_reduce_debts(debts, &expected.debt_reductions)
}

fn insurance_replay_before_height(
    receipts: &mut core::iter::Peekable<core::slice::Iter<'_, InsuranceReceipt>>,
    height: u64,
    key: IouKey,
    ious: &mut IouStore,
    debts: &mut BTreeMap<AccountId, u128>,
    pool: &mut u128,
) -> Result<()> {
    while receipts
        .peek()
        .is_some_and(|receipt| receipt.height < height)
    {
        insurance_replay_pnl(
            receipts.next().ok_or(EconomicError::Invariant)?,
            key,
            ious,
            debts,
            pool,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod insurance_plan_tests {
    use super::*;
    #[test]
    fn existing_pool_is_separate_from_new_coverage_and_replays_exactly() {
        let owner = AccountId::try_from("alice").unwrap();
        let debtor = AccountId::try_from("bob").unwrap();
        let key = IouKey::new(1, 7);
        let mut ious = IouStore::new();
        mark_replay_claim(&mut ious, key, &owner, 10, 1).unwrap();
        ious.add_loss(key, &tokens(4).unwrap(), 1).unwrap();
        let mut debts = BTreeMap::from([(debtor.clone(), 6)]);
        let mut pool = 4;
        let plan = insurance_backstop_plan(&ious, key, &debts, pool, 3, 2).unwrap();
        assert_eq!(
            plan.existing_funded_payouts,
            vec![InsuranceAmount {
                owner: owner.clone(),
                atoms: 4
            }]
        );
        assert_eq!(
            plan.insurer_funded_payouts,
            vec![InsuranceAmount { owner, atoms: 2 }]
        );
        assert_eq!(plan.cover_atoms, 2);
        let receipt = InsuranceReceipt {
            height: 2,
            input: UnverifiedInsuranceInput {
                txid: TxId::from_bytes([1; 32]),
                actor: AccountId::try_from("operator").unwrap(),
                action: InsuranceAction::Backstop { max_atoms: 2 },
            },
            fund_id: [0; 32],
            reserve_before_atoms: 3,
            reserve_after_atoms: 1,
            pending_before_atoms: 0,
            pending_after_atoms: 0,
            internal_sequence_before: 0,
            internal_sequence_after: 3,
            plan: Some(plan),
        };
        insurance_replay_pnl(&receipt, key, &mut ious, &mut debts, &mut pool).unwrap();
        assert_eq!(pool, 0);
        assert_eq!(debts.get(&debtor), Some(&4));
        assert_eq!(
            insurance_total(&insurance_claims(&ious, key).unwrap()).unwrap(),
            4
        );
        assert_eq!(ious.get(key), tokens(4).unwrap().negated());
        // A replayed stale proof cannot pay the already-consumed claim again.
        assert!(insurance_replay_pnl(&receipt, key, &mut ious, &mut debts, &mut pool).is_err());
    }
}
