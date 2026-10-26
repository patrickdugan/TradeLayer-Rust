// Accepted contract definitions and source price-context risk in the sole owner.
use crate::contract_registry::{
    ContractAsset, ContractParameters, ContractRegistry, ContractSeries, TickerContext,
};

pub const MAX_BOUND_CONTRACTS: usize = 4096;
pub const MAX_CONTRACT_FRAMES: usize = 4096;
pub const MAX_CONTRACT_RECORDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractRiskPolicy {
    SourceContextsConservingV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBindingConfig {
    pub creation_txid: TxId,
    pub definition_hash: [u8; 32],
    pub bootstrap_commitment: [u8; 32],
    pub policy: ContractRiskPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateContractInput {
    pub txid: TxId,
    pub issuer: AccountId,
    /// Accepted fee metadata is exactly zero/one for the source boolean flag.
    pub parameters: ContractParameters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedContract {
    pub creation_txid: TxId,
    pub height: u64,
    pub definition_hash: [u8; 32],
    pub definition: ContractSeries,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInputFrame {
    pub height: u64,
    pub inputs: Vec<EconomicInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractBindingState {
    /// Complete pristine Joint bootstrap, independently committed in config.
    pub bootstrap: Vec<u8>,
    pub contracts: Vec<AcceptedContract>,
    /// Complete ordered external typed inputs, including Registry dependencies.
    pub frames: Vec<ContractInputFrame>,
}

/// Expected deployment identity binds the original typed creation, including
/// fields which the source catalog normalizes when deriving its ticker.
pub fn contract_definition_commitment(input: &CreateContractInput) -> Result<[u8; 32]> {
    let mut bytes = b"TradeLayer/AcceptedContractDefinition/v1".to_vec();
    bytes.extend_from_slice(&econ_contract_creation_bytes(input)?);
    Ok(double_sha256(&bytes))
}
fn contract_bootstrap_hash(bootstrap: &[u8]) -> [u8; 32] {
    let mut bytes = b"TradeLayer/ContractBootstrap/v1".to_vec();
    bytes.extend_from_slice(bootstrap);
    double_sha256(&bytes)
}

impl EconomicState {
    pub fn contract_binding_state(&self) -> &ContractBindingState {
        &self.contract_binding
    }
    pub fn contract(&self, id: ContractId) -> Option<&AcceptedContract> {
        self.contract_binding
            .contracts
            .iter()
            .find(|row| row.definition.id == id)
    }
    pub fn contract_bootstrap_commitment(joint: &JointProtocolState) -> Result<[u8; 32]> {
        joint.verify_components_unverified()?;
        if joint.finalized_height().is_some()
            || !joint.economic_input_ids_unverified().is_empty()
            || !joint.system_epoch_ids_unverified().is_empty()
        {
            return Err(EconomicError::InvalidConfig);
        }
        let bytes = joint
            .snapshot_for_economic_bytes()
            .map_err(|_| EconomicError::InvalidSnapshot)?;
        Ok(contract_bootstrap_hash(&bytes))
    }
    fn contract_registry(&self) -> Result<ContractRegistry> {
        let mut registry = ContractRegistry::new();
        for contract in &self.contract_binding.contracts {
            registry
                .insert_loaded(contract.definition.clone())
                .map_err(|_| EconomicError::Invariant)?;
        }
        Ok(registry)
    }
    fn selected_contract(&self) -> Result<&AcceptedContract> {
        self.contract(self.config.contract)
            .ok_or(EconomicError::InvalidInput)
    }
    fn selected_contract_exists(&self) -> bool {
        self.config.contract_binding.is_none() || self.contract(self.config.contract).is_some()
    }
    fn require_contract_admission(&self) -> Result<()> {
        if self.config.contract_binding.is_some() {
            let contract = self.selected_contract()?;
            self.validate_selected_contract(contract)?;
            let oracle = self
                .joint
                .registry()
                .oracle(
                    contract
                        .definition
                        .parameters
                        .underlying_oracle_id
                        .ok_or(EconomicError::InvalidInput)?,
                )
                .ok_or(EconomicError::InvalidInput)?;
            if oracle.closed {
                return Err(EconomicError::InvalidInput);
            }
        }
        Ok(())
    }
    fn require_contract_order_admission(&self, order: &Order) -> Result<()> {
        self.require_contract_admission()?;
        if self.config.contract_binding.is_none() {
            return Ok(());
        }
        let parameters = &self.selected_contract()?.definition.parameters;
        let reference = self
            .joint
            .registry()
            .oracle(
                parameters
                    .underlying_oracle_id
                    .ok_or(EconomicError::InvalidInput)?,
            )
            .and_then(|oracle| oracle.latest.as_ref())
            .ok_or(EconomicError::InvalidInput)?;
        let mark = Decimal::from(reference.price_1e4).checked_div(
            &Decimal::from(10_000_u32),
            4,
            Rounding::Down,
        )?;
        bounded(&order.price)?;
        if order.price <= Decimal::zero()
            || order.price > Decimal::from(9_007_199_254_740_991_u64)
            || mark <= Decimal::zero()
        {
            return Err(EconomicError::InvalidInput);
        }
        let deviation =
            order
                .price
                .checked_sub(&mark)?
                .abs()
                .checked_div(&mark, 20, Rounding::HalfUp)?;
        // Preserve both source BigNumber divisions, rather than collapsing
        // 1/leverage/2 and changing its final rounding at a strict boundary.
        let allowed = Decimal::from(1_u32)
            .checked_div(&parameters.leverage, 20, Rounding::HalfUp)?
            .checked_div(&Decimal::from(2_u32), 20, Rounding::HalfUp)?
            .checked_sub(&"0.0065".parse::<Decimal>()?)?;
        if allowed <= Decimal::zero() || deviation >= allowed {
            return Err(EconomicError::InvalidInput);
        }
        Ok(())
    }
    fn validate_selected_contract(&self, contract: &AcceptedContract) -> Result<()> {
        let binding = self
            .config
            .contract_binding
            .as_ref()
            .ok_or(EconomicError::InvalidConfig)?;
        let row = &contract.definition;
        let data = &row.parameters;
        if contract.creation_txid != binding.creation_txid
            || contract.definition_hash != binding.definition_hash
            || row.id != self.config.contract
            || data.native
            || data.expiry_period != 0
            || data.series != 1
            || data.whitelist.unwrap_or(0) != 0
            || data.inverse != self.config.inverse
            || data.notional_value != self.config.notional
            || data.collateral_property_id != ContractAsset::Property(self.config.collateral)
            || self.config.fee_route != FeeRoute::Oracle(row.id.get())
            || self.config.marking.as_ref().map(|m| m.oracle_id) != data.underlying_oracle_id
            || self
                .config
                .funding
                .as_ref()
                .is_some_and(|f| Some(f.oracle_id) != data.underlying_oracle_id)
        {
            return Err(EconomicError::InvalidConfig);
        }
        Ok(())
    }
    fn apply_contract_creation(&mut self, input: CreateContractInput, height: u64) -> Result<()> {
        if self.config.contract_binding.is_none() {
            return Err(EconomicError::InvalidInput);
        }
        normal_owner(&input.issuer)?;
        let parameters = &input.parameters;
        bounded(&parameters.notional_value)?;
        bounded(&parameters.leverage)?;
        // Native references and dated settlement are separate lifecycle gates.
        // This initial executable catalog admits source oracle perpetuals only.
        let oracle_id = parameters
            .underlying_oracle_id
            .filter(|id| *id != 0)
            .ok_or(EconomicError::InvalidInput)?;
        let ContractAsset::Property(collateral) = parameters.collateral_property_id else {
            return Err(EconomicError::InvalidInput);
        };
        if parameters.native
            || parameters.notional_value <= Decimal::zero()
            || parameters.leverage <= Decimal::zero()
            || parameters.expiry_period != 0
            || parameters.series != 1
            || parameters.whitelist.unwrap_or(0) != 0
            || !parameters.on_chain_data.is_empty()
            || (parameters.fee != Decimal::zero() && parameters.fee != Decimal::from(1_u32))
            || self.ledger().property(collateral).is_none()
            || (collateral.get() == 2 && parameters.notional_property_id.get() == 2)
            || collateral.get() == 3
        {
            return Err(EconomicError::InvalidInput);
        }
        let oracle = self
            .joint
            .registry()
            .oracle(oracle_id)
            .ok_or(EconomicError::InvalidInput)?;
        if oracle.closed {
            return Err(EconomicError::InvalidInput);
        }
        let mut registry = self.contract_registry()?;
        let id = registry
            .create_contract_series(
                input.issuer.clone(),
                parameters.clone(),
                TickerContext {
                    oracle_ticker: Some(&oracle.ticker),
                    ..TickerContext::default()
                },
                height,
            )
            .map_err(|_| EconomicError::InvalidInput)?;
        let row = AcceptedContract {
            creation_txid: input.txid,
            height,
            definition_hash: contract_definition_commitment(&input)?,
            definition: registry
                .get_contract_series(id)
                .ok_or(EconomicError::Invariant)?
                .clone(),
        };
        if id == self.config.contract {
            self.validate_selected_contract(&row)?;
        }
        // A selected creation cannot be accepted under a different allocated ID.
        if self
            .config
            .contract_binding
            .as_ref()
            .is_some_and(|c| c.creation_txid == input.txid)
            && id != self.config.contract
        {
            return Err(EconomicError::InvalidConfig);
        }
        self.joint
            .note_economic_input_unverified(input.txid, height, 16)?;
        if self.contract_binding.contracts.len() >= MAX_BOUND_CONTRACTS {
            return Err(EconomicError::Capacity);
        }
        self.contract_binding.contracts.push(row);
        Ok(())
    }
    fn initial_margin_at(&self, price: &Decimal) -> Result<Decimal> {
        if self.config.contract_binding.is_none() {
            return Ok(self.config.initial_margin_per_contract.clone());
        }
        bounded(price)?;
        if price <= &Decimal::zero() {
            return Err(EconomicError::InvalidInput);
        }
        self.selected_contract()?;
        let value = self
            .contract_registry()?
            .initial_margin(self.config.contract, price)
            .map_err(|_| EconomicError::Arithmetic)?;
        bounded(&value)?;
        if value <= Decimal::zero() {
            return Err(EconomicError::InvalidInput);
        }
        Ok(value)
    }
    fn required_margin_at(&self, quantity: &Decimal, price: &Decimal) -> Result<u128> {
        if quantity.is_zero() {
            return Ok(0);
        }
        atoms_up(
            &quantity
                .abs()
                .checked_mul(&self.initial_margin_at(price)?)?,
        )
    }
    fn reserve_basis_atoms(
        &self,
        quantity: &Decimal,
        per_contract: &Option<Decimal>,
    ) -> Result<u128> {
        match (self.config.contract_binding.is_some(), per_contract) {
            (true, Some(basis)) if basis > &Decimal::zero() => {
                atoms_up(&quantity.checked_mul(basis)?)
            }
            (false, None) => self.required_margin(quantity),
            _ => Err(EconomicError::Invariant),
        }
    }
    fn margin_target_after_trade(
        &self,
        before_q: &Decimal,
        after_q: &Decimal,
        before_m: u128,
        price: &Decimal,
    ) -> Result<u128> {
        if self.config.contract_binding.is_none() {
            return self.required_margin(&after_q.abs());
        }
        if after_q.is_zero() {
            return Ok(0);
        }
        if before_q.is_zero() || before_q.is_negative() != after_q.is_negative() {
            return self.required_margin_at(&after_q.abs(), price);
        }
        if after_q.abs() > before_q.abs() {
            add(
                before_m,
                self.required_margin_at(&after_q.abs().checked_sub(&before_q.abs())?, price)?,
            )
        } else {
            Ok(before_m.min(self.required_margin_at(&after_q.abs(), price)?))
        }
    }
    fn maintenance_at(&self, quantity: &Decimal, average: Option<&Decimal>) -> Result<u128> {
        if quantity.is_zero() {
            return Ok(0);
        }
        let basis = if self.config.contract_binding.is_some() {
            self.initial_margin_at(average.ok_or(EconomicError::Invariant)?)?
        } else {
            self.config.initial_margin_per_contract.clone()
        };
        let value = quantity
            .abs()
            .checked_mul(&basis)?
            .checked_mul(&"0.5".parse::<Decimal>()?)?;
        if self.config.contract_binding.is_some() {
            atoms_exact(&value.quantize(8, Rounding::HalfUp)?)
        } else {
            atoms_up(&value)
        }
    }
    fn fee_input_notional(&self, fill_price: &Decimal) -> Result<Decimal> {
        if self.config.contract_binding.is_none() {
            return Ok(self.config.notional.clone());
        }
        self.selected_contract()?;
        self.contract_registry()?
            .notional_value(self.config.contract, Some(fill_price), None)
            .map(|value| value.notional_value)
            .map_err(|_| EconomicError::Arithmetic)
    }
    fn contract_record_budget(&self) -> Result<()> {
        // Reuse the encoder's exact nested count and byte bounds. Never accept
        // a runtime journal which cannot be represented by its strict decoder.
        let mut out = EconomicWriter(Vec::new());
        econ_write_contract_state(&mut out, &self.contract_binding)
    }
    pub(crate) fn preflight_contract_inputs(
        &self,
        height: u64,
        inputs: &[EconomicInput],
    ) -> Result<()> {
        if self.config.contract_binding.is_none() {
            return Ok(());
        }
        if self.contract_binding.frames.len() >= MAX_CONTRACT_FRAMES {
            return Err(EconomicError::Capacity);
        }
        let mut out = EconomicWriter(Vec::new());
        let mut budget = ContractRecordBudget::default();
        econ_write_contract_state_budget(&mut out, &self.contract_binding, &mut budget)?;
        budget.take(1)?;
        budget.take(inputs.len())?;
        out.u64(height)?;
        out.count(inputs.len(), MAX_ECONOMIC_INPUTS)?;
        for input in inputs {
            econ_write_input(&mut out, input, &mut budget)?;
        }
        Ok(())
    }
    fn verify_contract_binding(&self) -> Result<()> {
        let Some(config) = &self.config.contract_binding else {
            return if self.contract_binding == ContractBindingState::default()
                && !self.replaying_contract_history
            {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        };
        self.contract_record_budget()?;
        if contract_bootstrap_hash(&self.contract_binding.bootstrap) != config.bootstrap_commitment
            || self.contract_binding.frames.len() as u64 != self.finalized_height().unwrap_or(0)
        {
            return Err(EconomicError::Invariant);
        }
        let mut ids = BTreeSet::new();
        for (index, contract) in self.contract_binding.contracts.iter().enumerate() {
            if contract.definition.id.get() as usize != index + 1
                || !ids.insert(contract.creation_txid)
                || contract.height == 0
                || contract.height > self.finalized_height().unwrap_or(0)
            {
                return Err(EconomicError::Invariant);
            }
            if contract.definition.id == self.config.contract {
                self.validate_selected_contract(contract)?;
            }
        }
        for (index, frame) in self.contract_binding.frames.iter().enumerate() {
            if frame.height != index as u64 + 1 || frame.inputs.len() > MAX_ECONOMIC_INPUTS {
                return Err(EconomicError::Invariant);
            }
        }
        if self.replaying_contract_history {
            return Ok(());
        }
        let bootstrap = JointProtocolState::from_economic_snapshot_bytes(
            &self.contract_binding.bootstrap,
            self.config.network,
            self.joint.registry().protocol_admin_unverified(),
            self.joint.spot().permitted_properties(),
        )
        .map_err(|_| EconomicError::InvalidSnapshot)?;
        if Self::contract_bootstrap_commitment(&bootstrap)? != config.bootstrap_commitment {
            return Err(EconomicError::Invariant);
        }
        if self.contract_binding.frames.is_empty() {
            if bootstrap != self.joint || !self.contract_binding.contracts.is_empty() {
                return Err(EconomicError::Invariant);
            }
            return Ok(());
        }
        let mut replay = Self::from_joint_unverified(self.config.clone(), bootstrap)?;
        replay.replaying_contract_history = true;
        for frame in &self.contract_binding.frames {
            replay.apply_block_unverified(frame.height, frame.inputs.clone())?;
        }
        replay.replaying_contract_history = false;
        if replay != *self {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
}
