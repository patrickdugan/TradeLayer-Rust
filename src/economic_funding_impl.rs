// Complete funded funding epochs, staged with the sole economic Ledger.
use crate::funding::{
    collateral_notional, plan_funding, source_average, source_rate, FundingPlan, FundingPosition,
};

pub const FUNDING_INTERVAL: u64 = 24;
pub const FUNDING_WINDOW: u64 = 192;
pub const MAX_FUNDING_OBSERVATIONS: usize = 4096;
pub const MAX_FUNDING_EPOCHS: usize = 4096;
pub const MAX_FUNDING_RECORDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundingPolicy {
    ConservingFundingV1,
}

/// One explicitly configured oracle-backed perpetual. Registry observations
/// inherit the existing unverified actor boundary and 1e4 quote precision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingConfig {
    pub oracle_id: u32,
    pub max_staleness_blocks: u64,
    pub policy: FundingPolicy,
}
impl FundingConfig {
    pub fn validate(&self) -> Result<()> {
        if self.oracle_id == 0 || self.max_staleness_blocks > FUNDING_WINDOW {
            return Err(EconomicError::InvalidConfig);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingObservation {
    pub txid: TxId,
    pub height: u64,
    pub price: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingClosure {
    pub txid: TxId,
    pub height: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundingStatus {
    Paid,
    ZeroRate,
    NoPositions,
    MissingPrice,
    StalePrice,
    ClosedOracle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingEpoch {
    pub height: u64,
    pub id: TxId,
    pub status: FundingStatus,
    pub index: Option<FundingObservation>,
    pub window: Vec<FundingObservation>,
    pub positions: Vec<FundingPosition>,
    pub plan: Option<FundingPlan>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FundingState {
    pub observations: Vec<FundingObservation>,
    pub epochs: Vec<FundingEpoch>,
    pub closed_at: Option<FundingClosure>,
}
impl EconomicState {
    pub fn funding_state(&self) -> &FundingState {
        &self.funding
    }

    fn funding_publication(&self, tx: &UnverifiedJointTx) -> Option<(TxId, bool)> {
        use crate::joint::JointOperation;
        use crate::registry::RegistryAction;
        match (&self.config.funding, &tx.operation) {
            (
                Some(config),
                JointOperation::Registry(RegistryAction::PublishOracle { oracle_id, .. }),
            ) if *oracle_id == config.oracle_id => Some((tx.txid, false)),
            (Some(config), JointOperation::Registry(RegistryAction::CloseOracle { oracle_id }))
                if *oracle_id == config.oracle_id =>
            {
                Some((tx.txid, true))
            }
            _ => None,
        }
    }
    fn capture_funding_publication(&mut self, height: u64, txid: TxId, closed: bool) -> Result<()> {
        self.prune_funding_observations(height);
        let config = self
            .config
            .funding
            .as_ref()
            .ok_or(EconomicError::Invariant)?;
        if closed {
            if self.funding.closed_at.is_some()
                || !self
                    .market_oracle_view(config.oracle_id)?
                    .is_some_and(|o| o.closed)
            {
                return Err(EconomicError::Invariant);
            }
            self.funding.closed_at = Some(FundingClosure { txid, height });
            return Ok(());
        }
        let quote = self
            .market_oracle_view(config.oracle_id)?
            .and_then(|o| o.latest)
            .ok_or(EconomicError::Invariant)?;
        if quote.height != height {
            return Err(EconomicError::Invariant);
        }
        let price = quote.price;
        bounded(&price)?;
        let observation = FundingObservation {
            txid,
            height,
            price,
        };
        if self
            .funding
            .observations
            .last()
            .is_some_and(|o| o.height == height)
        {
            // The existing Joint registry accepts successive publications;
            // its final quote supplies the oracle/height upserted price row.
            *self
                .funding
                .observations
                .last_mut()
                .ok_or(EconomicError::Invariant)? = observation;
        } else {
            if self.funding.observations.len() >= MAX_FUNDING_OBSERVATIONS {
                return Err(EconomicError::Capacity);
            }
            self.funding.observations.push(observation);
        }
        Ok(())
    }
    fn prune_funding_observations(&mut self, height: u64) {
        let latest = self.funding.observations.last().map(|o| o.txid);
        let floor = height.saturating_sub(FUNDING_WINDOW);
        self.funding
            .observations
            .retain(|o| o.height >= floor || Some(o.txid) == latest);
    }
    fn funding_epoch_id(&self, height: u64) -> TxId {
        let mut bytes = self.domain(b"funding-epoch-v1");
        bytes.extend_from_slice(&height.to_be_bytes());
        TxId::from_bytes(double_sha256(&bytes))
    }
    fn run_funding_epoch(&mut self, height: u64) -> Result<()> {
        if !self.selected_contract_exists() {
            return Ok(());
        }
        let Some(config) = self.config.funding.clone() else {
            return Ok(());
        };
        if !height.is_multiple_of(FUNDING_INTERVAL)
            || self
                .joint
                .registry()
                .activation(18)
                .is_none_or(|a| a.height >= height)
        {
            return Ok(());
        }
        if self.funding.epochs.len() >= MAX_FUNDING_EPOCHS {
            return Err(EconomicError::Capacity);
        }
        let id = self.funding_epoch_id(height);
        self.joint.note_economic_epoch_unverified(id, height)?;
        let index = self.funding.observations.last().cloned();
        let floor = height.saturating_sub(FUNDING_WINDOW);
        let window: Vec<_> = self
            .funding
            .observations
            .iter()
            .filter(|o| (floor..=height).contains(&o.height))
            .cloned()
            .collect();
        let positions: Vec<_> = self
            .positions
            .positions()
            .filter(|p| !p.contracts.is_zero())
            .map(|p| FundingPosition {
                owner: p.address.clone(),
                contracts: p.contracts.clone(),
                available_atoms: self
                    .ledger()
                    .available_atoms(self.config.collateral, &p.address),
            })
            .collect();
        let closed = self
            .market_oracle_view(config.oracle_id)?
            .is_some_and(|o| o.closed);
        let (status, plan) = self.project_funding_epoch(
            height,
            &config,
            closed,
            index.as_ref(),
            &window,
            &positions,
        )?;
        if let Some(plan) = &plan {
            let escrow = self.escrow(b"funding-epoch", &id.as_bytes())?;
            // Both sides have been preflighted by the pure plan. No credits
            // occur before all payers have actually funded the transient pool.
            for debit in &plan.debits {
                self.transfer(debit.owner.clone(), escrow.clone(), debit.atoms)?;
            }
            for credit in &plan.credits {
                self.transfer(escrow.clone(), credit.owner.clone(), credit.atoms)?;
            }
            if self
                .ledger()
                .available_atoms(self.config.collateral, &escrow)
                != 0
            {
                return Err(EconomicError::CustodyMismatch);
            }
        }
        self.funding.epochs.push(FundingEpoch {
            height,
            id,
            status,
            index,
            window,
            positions,
            plan,
        });
        Ok(())
    }
    fn project_funding_epoch(
        &self,
        height: u64,
        config: &FundingConfig,
        closed: bool,
        index: Option<&FundingObservation>,
        window: &[FundingObservation],
        positions: &[FundingPosition],
    ) -> Result<(FundingStatus, Option<FundingPlan>)> {
        if closed {
            return Ok((FundingStatus::ClosedOracle, None));
        }
        let Some(index) = index else {
            return Ok((FundingStatus::MissingPrice, None));
        };
        if index.height > height {
            return Err(EconomicError::Invariant);
        }
        if height - index.height > config.max_staleness_blocks {
            return Ok((FundingStatus::StalePrice, None));
        }
        if window.is_empty() {
            return Ok((FundingStatus::MissingPrice, None));
        }
        let prices: Vec<_> = window
            .iter()
            .map(|observation| observation.price.clone())
            .collect();
        let average = source_average(&prices)?.ok_or(EconomicError::Invariant)?;
        let rate = source_rate(&index.price, &average)?;
        let notional = collateral_notional(
            &self.config.notional,
            &index.price,
            self.config.inverse,
            true,
        )?;
        let plan = plan_funding(positions, &notional, &rate)?;
        let status = if rate.is_zero() {
            FundingStatus::ZeroRate
        } else if positions.is_empty() {
            FundingStatus::NoPositions
        } else {
            FundingStatus::Paid
        };
        Ok((status, Some(plan)))
    }
    fn verify_funding(&self) -> Result<()> {
        let epochs = self.joint.system_epoch_ids_unverified();
        let Some(config) = &self.config.funding else {
            if self.funding != FundingState::default() {
                return Err(EconomicError::Invariant);
            }
            return Ok(());
        };
        config.validate()?;
        if self.funding.observations.len() > MAX_FUNDING_OBSERVATIONS
            || self.funding.epochs.len() > MAX_FUNDING_EPOCHS
        {
            return Err(EconomicError::Capacity);
        }
        let finalized = self.finalized_height().unwrap_or(0);
        let mut start = self
            .joint
            .registry()
            .activation(18)
            .map(|a| a.height / FUNDING_INTERVAL);
        if self.config.contract_binding.is_some() {
            start = self.contract(self.config.contract).and_then(|contract| {
                start.map(|a| a.max(contract.height.saturating_sub(1) / FUNDING_INTERVAL))
            });
        }
        let count = start.map_or(0, |a| (finalized / FUNDING_INTERVAL).saturating_sub(a));
        if count != self.funding.epochs.len() as u64 {
            return Err(EconomicError::Invariant);
        }
        self.verify_funding_observations(&self.funding.observations, finalized)?;
        let floor = finalized.saturating_sub(FUNDING_WINDOW);
        for (i, observation) in self.funding.observations.iter().enumerate() {
            if observation.height < floor && i + 1 != self.funding.observations.len() {
                return Err(EconomicError::Invariant);
            }
        }
        if let Some(oracle) = self.market_oracle_view(config.oracle_id)? {
            if oracle.closed != self.funding.closed_at.is_some() {
                return Err(EconomicError::Invariant);
            }
            match (&oracle.latest, self.funding.observations.last()) {
                (None, None) => (),
                (Some(quote), Some(observation))
                    if quote.height == observation.height && observation.price == quote.price => {}
                _ => return Err(EconomicError::Invariant),
            }
        } else if !self.funding.observations.is_empty() || self.funding.closed_at.is_some() {
            return Err(EconomicError::Invariant);
        }
        if let Some(closure) = &self.funding.closed_at {
            if closure.height == 0
                || closure.height > finalized
                || !self.joint.registry().is_processed(closure.txid)
                || self
                    .funding
                    .observations
                    .iter()
                    .any(|o| o.txid == closure.txid || o.height > closure.height)
            {
                return Err(EconomicError::Invariant);
            }
        }
        let mut records = self.funding.observations.len();
        for epoch in &self.funding.epochs {
            records = records
                .checked_add(epoch.window.len())
                .and_then(|n| n.checked_add(epoch.positions.len()))
                .ok_or(EconomicError::Capacity)?;
            if let Some(plan) = &epoch.plan {
                records = records
                    .checked_add(plan.debits.len())
                    .and_then(|n| n.checked_add(plan.credits.len()))
                    .ok_or(EconomicError::Capacity)?;
            }
            if records > MAX_FUNDING_RECORDS {
                return Err(EconomicError::Capacity);
            }
        }
        let mut known = BTreeMap::new();
        let mut known_ids = BTreeMap::new();
        for observation in self.funding.observations.iter().chain(
            self.funding
                .epochs
                .iter()
                .flat_map(|epoch| epoch.window.iter().chain(epoch.index.iter())),
        ) {
            if known
                .insert(observation.height, observation)
                .is_some_and(|prior| prior != observation)
                || known_ids
                    .insert(observation.txid, observation)
                    .is_some_and(|prior| prior != observation)
                || self
                    .funding
                    .closed_at
                    .as_ref()
                    .is_some_and(|c| c.txid == observation.txid || observation.height > c.height)
            {
                return Err(EconomicError::Invariant);
            }
        }
        let latest = known.last_key_value().map(|(h, _)| *h);
        let retained: Vec<_> = known
            .iter()
            .filter(|(h, _)| **h >= floor || Some(**h) == latest)
            .map(|(_, o)| (*o).clone())
            .collect();
        if retained != self.funding.observations {
            return Err(EconomicError::Invariant);
        }
        let mut seen_epochs = BTreeSet::new();
        for (i, epoch) in self.funding.epochs.iter().enumerate() {
            let expected = (start.ok_or(EconomicError::Invariant)? + 1 + i as u64)
                .checked_mul(FUNDING_INTERVAL)
                .ok_or(EconomicError::Arithmetic)?;
            if epoch.height != expected
                || epoch.id != self.funding_epoch_id(expected)
                || !epochs.contains(&epoch.id)
                || !seen_epochs.insert(epoch.id)
                || epoch.positions.len() > MAX_ECONOMIC_POSITIONS
                || epoch.window.len() > MAX_FUNDING_OBSERVATIONS
            {
                return Err(EconomicError::Invariant);
            }
            self.verify_funding_observations(&epoch.window, epoch.height)?;
            let floor = epoch.height.saturating_sub(FUNDING_WINDOW);
            if epoch.window.iter().any(|o| o.height < floor) {
                return Err(EconomicError::Invariant);
            }
            let complete: Vec<_> = known
                .range(floor..=epoch.height)
                .map(|(_, o)| (*o).clone())
                .collect();
            if complete != epoch.window {
                return Err(EconomicError::Invariant);
            }
            let as_of = known
                .range(..=epoch.height)
                .next_back()
                .map(|(_, o)| (*o).clone());
            if as_of != epoch.index {
                return Err(EconomicError::Invariant);
            }
            match (&epoch.index, epoch.window.last()) {
                (None, None) => (),
                (Some(index), Some(last)) if index == last => (),
                (Some(index), None) if index.height < floor => {
                    self.verify_funding_observations(core::slice::from_ref(index), epoch.height)?
                }
                _ => return Err(EconomicError::Invariant),
            }
            let mut owners = BTreeSet::new();
            let mut open_interest = Decimal::zero();
            for position in &epoch.positions {
                self.participant_owner(&position.owner)?;
                bounded(&position.contracts)?;
                if position.contracts.is_zero()
                    || !owners.insert(&position.owner)
                    || self.positions.get(&position.owner).is_none()
                {
                    return Err(EconomicError::Invariant);
                }
                open_interest = open_interest.checked_add(&position.contracts)?;
            }
            if !open_interest.is_zero() {
                return Err(EconomicError::Invariant);
            }
            if epoch.height == finalized && self.config.liquidation.is_none() {
                let captured: BTreeMap<_, _> = epoch
                    .positions
                    .iter()
                    .map(|p| (&p.owner, &p.contracts))
                    .collect();
                let current: BTreeMap<_, _> = self
                    .positions
                    .positions()
                    .filter(|p| !p.contracts.is_zero())
                    .map(|p| (&p.address, &p.contracts))
                    .collect();
                if captured != current {
                    return Err(EconomicError::Invariant);
                }
            }
            let closed = self
                .funding
                .closed_at
                .as_ref()
                .is_some_and(|c| c.height <= epoch.height);
            let (status, plan) = self.project_funding_epoch(
                epoch.height,
                config,
                closed,
                epoch.index.as_ref(),
                &epoch.window,
                &epoch.positions,
            )?;
            if status != epoch.status || plan != epoch.plan {
                return Err(EconomicError::Invariant);
            }
        }
        Ok(())
    }
    fn verify_funding_observations(
        &self,
        observations: &[FundingObservation],
        through: u64,
    ) -> Result<()> {
        let mut last = 0;
        let mut seen = BTreeSet::new();
        let actual_oracle = self
            .source_catalog_owners()
            .and_then(|owners| owners.oracles.as_ref())
            .or(self.catalog_inner_oracle.as_ref())
            .is_some();
        // Actual source publications retain their original SourceMetadata ID,
        // without inventing a second oracle in Registry. Full raw-source replay
        // binds the publication and observation; the private encoding context
        // is available only after that complete replay has succeeded.
        let source_ids = if actual_oracle {
            self.source_metadata_ids()?
        } else {
            BTreeSet::new()
        };
        for observation in observations {
            let recorded = if actual_oracle {
                source_ids.contains(&observation.txid)
            } else {
                self.joint.registry().is_processed(observation.txid)
            };
            if observation.height == 0
                || observation.height <= last
                || observation.height > through
                || observation.price <= Decimal::zero()
                || !recorded
                || !seen.insert(observation.txid)
            {
                return Err(EconomicError::Invariant);
            }
            bounded(&observation.price)?;
            if !actual_oracle {
                // The legacy typed Registry stores u128 fixed-four prices.
                // Actual source data can retain more precise decimal strings.
                let units = observation.price.checked_mul(&Decimal::from(10_000))?;
                if units.quantize(0, Rounding::Down)? != units {
                    return Err(EconomicError::Invariant);
                }
                units
                    .to_string()
                    .parse::<u128>()
                    .map_err(|_| EconomicError::Invariant)?;
            }
            last = observation.height;
        }
        Ok(())
    }
}
