// TLECv9 retains the full deterministic sole-owner replay, not a legacy state
// with feature metadata removed. Independent deployment config pins bootstrap.
const ECONOMIC_INSURANCE_MAGIC: &[u8; 5] = b"TLEC\x09";

fn insurance_write_identity(out: &mut EconomicWriter, fund: &InsuranceFundIdentity) -> Result<()> {
    out.byte(fund.network.tag())?;
    out.put(&fund.network.genesis().0)?;
    out.u32(fund.contract.get())?;
    out.u32(fund.collateral.get())?;
    out.txid(fund.creation_txid)?;
    out.put(&fund.definition_hash)?;
    out.u32(fund.oracle_id)
}
fn insurance_read_identity(reader: &mut Cursor<'_>) -> Result<InsuranceFundIdentity> {
    let network = TradeLayerMainnet::from_tag(reader.u8().map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    if reader.fixed::<32>().map_err(|_| econ_snapshot_bad())? != network.genesis().0 {
        return Err(econ_snapshot_bad());
    }
    Ok(InsuranceFundIdentity {
        network,
        contract: econ_read_contract(reader)?,
        collateral: econ_read_property(reader)?,
        creation_txid: econ_read_txid(reader)?,
        definition_hash: reader.fixed().map_err(|_| econ_snapshot_bad())?,
        oracle_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
    })
}
fn insurance_write_config(out: &mut EconomicWriter, config: &InsuranceConfig) -> Result<()> {
    insurance_write_identity(out, &config.fund)?;
    out.account(&config.sponsor)?;
    out.account(&config.operator)?;
    match config.policy {
        InsurancePolicy::ConservingOracleBackstopV1 => out.byte(1),
    }
}
fn insurance_read_config(reader: &mut Cursor<'_>) -> Result<InsuranceConfig> {
    Ok(InsuranceConfig {
        fund: insurance_read_identity(reader)?,
        sponsor: econ_read_account(reader)?,
        operator: econ_read_account(reader)?,
        policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
            1 => InsurancePolicy::ConservingOracleBackstopV1,
            _ => return Err(econ_snapshot_bad()),
        },
    })
}
fn insurance_write_input(out: &mut EconomicWriter, input: &UnverifiedInsuranceInput) -> Result<()> {
    out.txid(input.txid)?;
    out.account(&input.actor)?;
    match input.action {
        InsuranceAction::Deposit { atoms } => {
            out.byte(0)?;
            out.atoms(atoms)
        }
        InsuranceAction::AdoptFees { max_atoms } => {
            out.byte(1)?;
            out.atoms(max_atoms)
        }
        InsuranceAction::Backstop { max_atoms } => {
            out.byte(2)?;
            out.atoms(max_atoms)
        }
    }
}
fn insurance_read_input(reader: &mut Cursor<'_>) -> Result<UnverifiedInsuranceInput> {
    let txid = econ_read_txid(reader)?;
    let actor = econ_read_account(reader)?;
    let kind = reader.u8().map_err(|_| econ_snapshot_bad())?;
    let atoms = econ_read_atoms(reader)?;
    let action = match kind {
        0 => InsuranceAction::Deposit { atoms },
        1 => InsuranceAction::AdoptFees { max_atoms: atoms },
        2 => InsuranceAction::Backstop { max_atoms: atoms },
        _ => return Err(econ_snapshot_bad()),
    };
    Ok(UnverifiedInsuranceInput {
        txid,
        actor,
        action,
    })
}
fn insurance_write_amounts(
    out: &mut EconomicWriter,
    rows: &[InsuranceAmount],
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    budget.take(rows.len())?;
    out.count(rows.len(), MAX_ECONOMIC_POSITIONS)?;
    for row in rows {
        out.account(&row.owner)?;
        out.atoms(row.atoms)?;
    }
    Ok(())
}
fn insurance_read_amounts(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<Vec<InsuranceAmount>> {
    let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
    budget.take(count)?;
    let mut rows = Vec::new();
    for _ in 0..count {
        rows.push(InsuranceAmount {
            owner: econ_read_account(reader)?,
            atoms: econ_read_atoms(reader)?,
        });
    }
    Ok(rows)
}
fn insurance_write_state(
    out: &mut EconomicWriter,
    state: &InsuranceState,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    for atoms in [
        state.reserve_atoms,
        state.deposited_atoms,
        state.adopted_fee_atoms,
        state.covered_atoms,
    ] {
        out.atoms(atoms)?;
    }
    budget.take(state.receipts.len())?;
    out.count(state.receipts.len(), MAX_INSURANCE_RECEIPTS)?;
    for receipt in &state.receipts {
        out.u64(receipt.height)?;
        insurance_write_input(out, &receipt.input)?;
        out.put(&receipt.fund_id)?;
        for atoms in [
            receipt.reserve_before_atoms,
            receipt.reserve_after_atoms,
            receipt.pending_before_atoms,
            receipt.pending_after_atoms,
        ] {
            out.atoms(atoms)?;
        }
        out.u64(receipt.internal_sequence_before)?;
        out.u64(receipt.internal_sequence_after)?;
        out.byte(u8::from(receipt.plan.is_some()))?;
        if let Some(plan) = &receipt.plan {
            out.atoms(plan.pool_before_atoms)?;
            insurance_write_amounts(out, &plan.claims_before, budget)?;
            insurance_write_amounts(out, &plan.unpaid_losses_before, budget)?;
            insurance_write_amounts(out, &plan.existing_funded_payouts, budget)?;
            out.atoms(plan.cover_atoms)?;
            insurance_write_amounts(out, &plan.debt_reductions, budget)?;
            insurance_write_amounts(out, &plan.insurer_funded_payouts, budget)?;
        }
    }
    Ok(())
}
fn insurance_read_state(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<InsuranceState> {
    let reserve_atoms = econ_read_atoms(reader)?;
    let deposited_atoms = econ_read_atoms(reader)?;
    let adopted_fee_atoms = econ_read_atoms(reader)?;
    let covered_atoms = econ_read_atoms(reader)?;
    let count = econ_read_count(reader, MAX_INSURANCE_RECEIPTS)?;
    budget.take(count)?;
    let mut receipts = Vec::new();
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let input = insurance_read_input(reader)?;
        let fund_id = reader.fixed().map_err(|_| econ_snapshot_bad())?;
        let reserve_before_atoms = econ_read_atoms(reader)?;
        let reserve_after_atoms = econ_read_atoms(reader)?;
        let pending_before_atoms = econ_read_atoms(reader)?;
        let pending_after_atoms = econ_read_atoms(reader)?;
        let internal_sequence_before = econ_read_u64(reader)?;
        let internal_sequence_after = econ_read_u64(reader)?;
        let plan = if econ_read_bool(reader)? {
            Some(InsuranceBackstopPlan {
                pool_before_atoms: econ_read_atoms(reader)?,
                claims_before: insurance_read_amounts(reader, budget)?,
                unpaid_losses_before: insurance_read_amounts(reader, budget)?,
                existing_funded_payouts: insurance_read_amounts(reader, budget)?,
                cover_atoms: econ_read_atoms(reader)?,
                debt_reductions: insurance_read_amounts(reader, budget)?,
                insurer_funded_payouts: insurance_read_amounts(reader, budget)?,
            })
        } else {
            None
        };
        receipts.push(InsuranceReceipt {
            height,
            input,
            fund_id,
            reserve_before_atoms,
            reserve_after_atoms,
            pending_before_atoms,
            pending_after_atoms,
            internal_sequence_before,
            internal_sequence_after,
            plan,
        });
    }
    Ok(InsuranceState {
        reserve_atoms,
        deposited_atoms,
        adopted_fee_atoms,
        covered_atoms,
        receipts,
    })
}
impl EconomicState {
    fn insurance_snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        let mut out = EconomicWriter(ECONOMIC_INSURANCE_MAGIC.to_vec());
        econ_write_config(&mut out, &self.config)?;
        insurance_write_config(
            &mut out,
            self.config
                .insurance
                .as_ref()
                .ok_or(EconomicError::InvalidConfig)?,
        )?;
        out.byte(u8::from(self.config.native_entitlements.is_some()))?;
        if let Some(policy) = &self.config.native_entitlements {
            out.embedded(
                &crate::native_entitlement::canonical_policy_bytes(policy)
                    .map_err(|_| EconomicError::InvalidConfig)?,
            )?;
        }
        let mut budget = ContractRecordBudget::default();
        econ_write_contract_state_budget(&mut out, &self.contract_binding, &mut budget)?;
        insurance_write_state(&mut out, &self.insurance, &mut budget)?;
        out.u64(self.internal_sequence)?;
        with_crc(out.0).map_err(|_| EconomicError::Capacity)
    }
    fn from_insurance_snapshot_bytes(
        bytes: &[u8],
        expected: &EconomicConfig,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
    ) -> Result<Self> {
        expected.validate()?;
        expected
            .insurance
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let raw = verify_crc(bytes).map_err(|_| econ_snapshot_bad())?;
        let mut reader = Cursor::new(raw);
        if reader.fixed::<5>().map_err(|_| econ_snapshot_bad())? != *ECONOMIC_INSURANCE_MAGIC {
            return Err(econ_snapshot_bad());
        }
        let mut config = econ_read_config(&mut reader, false, true, false, false, true)?;
        config.insurance = Some(insurance_read_config(&mut reader)?);
        config.native_entitlements = if econ_read_bool(&mut reader)? {
            Some(
                crate::native_entitlement::parse_policy(econ_read_embedded(&mut reader)?)
                    .map_err(|_| econ_snapshot_bad())?,
            )
        } else {
            None
        };
        if &config != expected {
            return Err(EconomicError::InvalidConfig);
        }
        // Decode the full borrowed-bounded frame journal before any replay.
        let binding = econ_read_contract_state(&mut reader)?;
        let mut budget = ContractRecordBudget::default();
        let mut sizing = EconomicWriter(Vec::new());
        econ_write_contract_state_budget(&mut sizing, &binding, &mut budget)?;
        let insurance = insurance_read_state(&mut reader, &mut budget)?;
        let internal_sequence = econ_read_u64(&mut reader)?;
        reader.end().map_err(|_| econ_snapshot_bad())?;
        if contract_bootstrap_hash(&binding.bootstrap)
            != expected
                .contract_binding
                .as_ref()
                .ok_or(EconomicError::InvalidConfig)?
                .bootstrap_commitment
        {
            return Err(EconomicError::InvalidConfig);
        }
        let joint = match &config.native_entitlements {
            Some(policy) => JointProtocolState::from_economic_snapshot_bytes_with_native_policy(
                &binding.bootstrap,
                config.network,
                admin,
                permitted,
                policy,
            ),
            None => JointProtocolState::from_economic_snapshot_bytes(
                &binding.bootstrap,
                config.network,
                admin,
                permitted,
            ),
        }
        .map_err(|_| econ_snapshot_bad())?;
        let mut state = Self::from_joint_unverified(config, joint)?;
        state.replaying_contract_history = true;
        for frame in &binding.frames {
            state.apply_block_unverified(frame.height, frame.inputs.clone())?;
        }
        state.replaying_contract_history = false;
        if state.contract_binding != binding
            || state.insurance != insurance
            || state.internal_sequence != internal_sequence
        {
            return Err(econ_snapshot_bad());
        }
        state.verify()?;
        if state.insurance_snapshot_bytes()?.as_slice() != bytes {
            return Err(econ_snapshot_bad());
        }
        Ok(state)
    }
}
