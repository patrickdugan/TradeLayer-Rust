// Persisted settled basis, actual funded marks and maintenance projections.
// These caller-configured conserving rules do not authenticate oracle inputs.
use crate::mark_basis::{BasisAction, BasisConfig, BasisPosition, SignedAtoms};

pub const MAX_MARK_EVENTS: usize = 8192;
pub const MAX_MARK_EPOCHS: usize = 4096;
pub const MAX_MARK_RECORDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkPolicy {
    ConservingMarkBasisV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkConfig {
    pub oracle_id: u32,
    pub policy: MarkPolicy,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkEventKind {
    Trade {
        buy_txid: TxId,
        sell_txid: TxId,
    },
    Mark {
        id: TxId,
        observation: FundingObservation,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkEventEntry {
    pub owner: AccountId,
    pub action: BasisAction,
    pub atoms: SignedAtoms,
    pub available_atoms: u128,
    pub margin_atoms: u128,
    pub collected_atoms: u128,
    pub claim_offset_atoms: u128,
    pub loss_offset_atoms: u128,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkEvent {
    pub height: u64,
    pub kind: MarkEventKind,
    pub entries: Vec<MarkEventEntry>,
    pub collected_loss_atoms: u128,
    pub paid_claim_atoms: u128,
    pub unfunded_loss_atoms: u128,
}
/// A queryable deficit in actual margin custody, never a second balance store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceCall {
    pub owner: AccountId,
    pub contracts: Decimal,
    pub required_atoms: u128,
    pub margin_atoms: u128,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkState {
    pub bases: BTreeMap<AccountId, BasisPosition>,
    pub unpaid_losses: BTreeMap<AccountId, u128>,
    pub events: Vec<MarkEvent>,
    pub maintenance: Vec<MaintenanceCall>,
}

impl EconomicState {
    pub fn mark_state(&self) -> &MarkState {
        &self.marking
    }
    fn basis_config(&self) -> BasisConfig {
        BasisConfig {
            inverse: self.config.inverse,
            notional: self.config.notional.clone(),
        }
    }
    fn mark_publication(&self, tx: &UnverifiedJointTx) -> Option<TxId> {
        use crate::joint::JointOperation;
        use crate::registry::RegistryAction;
        match (&self.config.marking, &tx.operation) {
            (
                Some(config),
                JointOperation::Registry(RegistryAction::PublishOracle { oracle_id, .. }),
            ) if *oracle_id == config.oracle_id => Some(tx.txid),
            _ => None,
        }
    }
    fn mark_epoch_id(&self, height: u64) -> TxId {
        let mut bytes = self.domain(b"mark-epoch-v1");
        bytes.extend_from_slice(&height.to_be_bytes());
        if let Some(config) = &self.config.marking {
            bytes.extend_from_slice(&config.oracle_id.to_be_bytes());
        }
        TxId::from_bytes(double_sha256(&bytes))
    }
    /// Apply only metadata. Both counterparties' exact amounts are determined
    /// before collecting any loss or paying any claim.
    fn mark_entry(&mut self, owner: &AccountId, action: BasisAction) -> Result<MarkEventEntry> {
        let old = self.marking.bases.get(owner).cloned().unwrap_or_default();
        let transition = old.transition(&self.basis_config(), &action)?;
        if transition.position.quantity != self.position(owner).contracts {
            return Err(EconomicError::Invariant);
        }
        let mut position = self.position(owner);
        position.realized_pnl = position
            .realized_pnl
            .checked_add(&transition.atoms.to_decimal())?;
        position.last_mark = Some(match &action {
            BasisAction::Trade { price, .. } | BasisAction::Mark { price } => price.clone(),
        });
        self.positions.write_position(position)?;
        self.marking
            .bases
            .insert(owner.clone(), transition.position);
        Ok(MarkEventEntry {
            owner: owner.clone(),
            action,
            atoms: transition.atoms,
            available_atoms: self.ledger().available_atoms(self.config.collateral, owner),
            margin_atoms: self.margin_atoms(owner),
            collected_atoms: 0,
            claim_offset_atoms: 0,
            loss_offset_atoms: 0,
        })
    }
    fn record_mark_event(&mut self, event: MarkEvent) -> Result<()> {
        if self.marking.events.len() >= MAX_MARK_EVENTS {
            return Err(EconomicError::Capacity);
        }
        self.marking.events.push(event);
        self.mark_record_count()?;
        Ok(())
    }
    fn run_mark_epoch(&mut self, height: u64, publication: Option<TxId>) -> Result<()> {
        let (Some(config), Some(txid)) = (self.config.marking.clone(), publication) else {
            return Ok(());
        };
        if self
            .joint
            .registry()
            .activation(18)
            .is_none_or(|a| a.height >= height)
        {
            return Ok(());
        }
        let oracle = self
            .joint
            .registry()
            .oracle(config.oracle_id)
            .ok_or(EconomicError::Invariant)?;
        if oracle.closed {
            return Ok(());
        }
        let quote = oracle.latest.as_ref().ok_or(EconomicError::Invariant)?;
        if quote.height != height {
            return Err(EconomicError::Invariant);
        }
        if self
            .marking
            .events
            .iter()
            .filter(|e| matches!(e.kind, MarkEventKind::Mark { .. }))
            .count()
            >= MAX_MARK_EPOCHS
        {
            return Err(EconomicError::Capacity);
        }
        let observation = FundingObservation {
            txid,
            height,
            price: Decimal::from(quote.price_1e4).checked_div(
                &Decimal::from(10_000),
                4,
                Rounding::Down,
            )?,
        };
        let id = self.mark_epoch_id(height);
        self.joint.note_economic_epoch_unverified(id, height)?;
        let owners: Vec<_> = self
            .marking
            .bases
            .iter()
            .filter(|(_, p)| !p.quantity.is_zero())
            .map(|(a, _)| a.clone())
            .collect();
        let mut entries = Vec::with_capacity(owners.len());
        for owner in owners {
            entries.push(self.mark_entry(
                &owner,
                BasisAction::Mark {
                    price: observation.price.clone(),
                },
            )?);
        }
        let (collected, bad_debt, paid) = self.settle_mark_entries(&mut entries, height)?;
        self.record_mark_event(MarkEvent {
            height,
            kind: MarkEventKind::Mark { id, observation },
            entries,
            collected_loss_atoms: collected,
            paid_claim_atoms: paid,
            unfunded_loss_atoms: bad_debt,
        })
    }
    fn settle_mark_entries(
        &mut self,
        entries: &mut [MarkEventEntry],
        height: u64,
    ) -> Result<(u128, u128, u128)> {
        let mut obligations = Vec::with_capacity(entries.len());
        let key = self.iou_key();
        // Cancel nominal obligations before collecting any new cash. Otherwise
        // a reversal could collect cash to pay the very stale claim it reverses.
        for entry in entries.iter_mut() {
            obligations.push(mark_apply_offsets(
                &mut self.ious,
                &mut self.marking.unpaid_losses,
                key,
                entry,
                height,
            )?);
        }
        let mut collected = 0;
        let mut bad_debt = 0;
        for (entry, (debt, _)) in entries.iter_mut().zip(&obligations) {
            entry.collected_atoms = self.collect_loss(&entry.owner, *debt)?;
            collected = add(collected, entry.collected_atoms)?;
            let unpaid = debt - entry.collected_atoms;
            bad_debt = add(bad_debt, unpaid)?;
            mark_add_unpaid(&mut self.marking.unpaid_losses, &entry.owner, unpaid)?;
        }
        self.unfunded_loss_atoms = add(self.unfunded_loss_atoms, bad_debt)?;
        if collected != 0 {
            self.ious.add_loss(key, &tokens(collected)?, height)?;
        }
        for (entry, (_, profit)) in entries.iter().zip(&obligations) {
            self.add_claim(&entry.owner, *profit, height)?;
        }
        Ok((collected, bad_debt, self.pay_claims(height)?))
    }
    fn expected_maintenance(&self) -> Result<Vec<MaintenanceCall>> {
        if self.config.marking.is_none() {
            return Ok(Vec::new());
        }
        let half: Decimal = "0.5".parse().map_err(|_| EconomicError::Arithmetic)?;
        let mut calls = Vec::new();
        for (owner, basis) in &self.marking.bases {
            if basis.quantity.is_zero() {
                continue;
            }
            let required_atoms = atoms_up(
                &basis
                    .quantity
                    .abs()
                    .checked_mul(&self.config.initial_margin_per_contract)?
                    .checked_mul(&half)?,
            )?;
            let margin_atoms = self.margin_atoms(owner);
            if margin_atoms < required_atoms {
                calls.push(MaintenanceCall {
                    owner: owner.clone(),
                    contracts: basis.quantity.clone(),
                    required_atoms,
                    margin_atoms,
                });
            }
        }
        Ok(calls)
    }
    fn refresh_maintenance(&mut self) -> Result<()> {
        self.marking.maintenance = self.expected_maintenance()?;
        Ok(())
    }
    fn mark_record_count(&self) -> Result<usize> {
        if self.marking.events.len() > MAX_MARK_EVENTS
            || self.marking.bases.len() > MAX_ECONOMIC_POSITIONS
            || self.marking.unpaid_losses.len() > MAX_ECONOMIC_POSITIONS
            || self.marking.maintenance.len() > MAX_ECONOMIC_POSITIONS
        {
            return Err(EconomicError::Capacity);
        }
        let mut count = self
            .marking
            .bases
            .len()
            .checked_add(self.marking.unpaid_losses.len())
            .and_then(|n| n.checked_add(self.marking.maintenance.len()))
            .ok_or(EconomicError::Capacity)?;
        for event in &self.marking.events {
            if event.entries.len() > MAX_ECONOMIC_POSITIONS {
                return Err(EconomicError::Capacity);
            }
            count = count
                .checked_add(event.entries.len())
                .ok_or(EconomicError::Capacity)?;
            if count > MAX_MARK_RECORDS {
                return Err(EconomicError::Capacity);
            }
        }
        Ok(count)
    }
    fn verify_system_epochs(&self) -> Result<()> {
        let mut expected = BTreeSet::new();
        for id in
            self.funding
                .epochs
                .iter()
                .map(|e| e.id)
                .chain(self.marking.events.iter().filter_map(|e| {
                    if let MarkEventKind::Mark { id, .. } = e.kind {
                        Some(id)
                    } else {
                        None
                    }
                }))
        {
            if !expected.insert(id) {
                return Err(EconomicError::Invariant);
            }
        }
        if &expected != self.joint.system_epoch_ids_unverified() {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }
    fn verify_mark_observation(
        &self,
        observation: &FundingObservation,
        oracle_id: u32,
    ) -> Result<()> {
        if let Some(funding) = &self.config.funding {
            let same = funding.oracle_id == oracle_id;
            for known in self.funding.observations.iter().chain(
                self.funding
                    .epochs
                    .iter()
                    .flat_map(|e| e.window.iter().chain(e.index.iter())),
            ) {
                if (same && known.height == observation.height && known != observation)
                    || (known.txid == observation.txid && (!same || known != observation))
                {
                    return Err(EconomicError::Invariant);
                }
            }
            if self.funding.closed_at.as_ref().is_some_and(|c| {
                c.txid == observation.txid || (same && observation.height >= c.height)
            }) {
                return Err(EconomicError::Invariant);
            }
        }
        Ok(())
    }
    fn verify_marks(&self) -> Result<()> {
        let Some(config) = &self.config.marking else {
            return if self.marking == MarkState::default() {
                Ok(())
            } else {
                Err(EconomicError::Invariant)
            };
        };
        self.mark_record_count()?;
        let basis_config = self.basis_config();
        let mut bases = BTreeMap::<AccountId, BasisPosition>::new();
        let mut realized = BTreeMap::<AccountId, Decimal>::new();
        let mut averages = BTreeMap::<AccountId, Option<Decimal>>::new();
        let mut last_prices = BTreeMap::new();
        let mut receipts = self.receipts.iter();
        let mut ious = IouStore::new();
        let mut unpaid_losses = BTreeMap::new();
        let mut funding_epochs = self.funding.epochs.iter().peekable();
        let mut publications = BTreeSet::new();
        let mut pool = 0;
        let mut bad_debt = 0;
        let mut last_height = 0;
        let mut mark_height = 0;
        let mut mark_count = 0;
        let finalized = self.finalized_height().unwrap_or(0);
        let supply = self
            .ledger()
            .property(self.config.collateral)
            .ok_or(EconomicError::Invariant)?
            .total_supply_atoms;
        for event in &self.marking.events {
            while funding_epochs
                .peek()
                .is_some_and(|e| e.height < event.height)
            {
                mark_verify_funding_positions(
                    funding_epochs.next().ok_or(EconomicError::Invariant)?,
                    &bases,
                )?;
            }
            if event.height == 0
                || event.height < last_height
                || event.height > finalized
                || (event.height == mark_height)
            {
                return Err(EconomicError::Invariant);
            }
            match &event.kind {
                MarkEventKind::Trade {
                    buy_txid,
                    sell_txid,
                } => {
                    let receipt = receipts.next().ok_or(EconomicError::Invariant)?;
                    if event.height != receipt.height
                        || *buy_txid != receipt.buy_txid
                        || *sell_txid != receipt.sell_txid
                        || event.entries.len() != 2
                        || event.entries[0].owner == event.entries[1].owner
                        || event.collected_loss_atoms != receipt.collected_loss_atoms
                        || event.paid_claim_atoms != receipt.paid_claim_atoms
                        || event.unfunded_loss_atoms != receipt.unfunded_loss_atoms
                    {
                        return Err(EconomicError::Invariant);
                    }
                    for (entry, delta, pnl) in [
                        (
                            &event.entries[0],
                            receipt.quantity.clone(),
                            &receipt.buyer_pnl,
                        ),
                        (
                            &event.entries[1],
                            receipt.quantity.negated(),
                            &receipt.seller_pnl,
                        ),
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
                }
                MarkEventKind::Mark { id, observation } => {
                    mark_count += 1;
                    if mark_count > MAX_MARK_EPOCHS
                        || event.height <= mark_height
                        || observation.height != event.height
                        || *id != self.mark_epoch_id(event.height)
                        || !self.joint.system_epoch_ids_unverified().contains(id)
                        || self
                            .joint
                            .registry()
                            .activation(18)
                            .is_none_or(|a| a.height >= event.height)
                    {
                        return Err(EconomicError::Invariant);
                    }
                    self.verify_funding_observations(
                        core::slice::from_ref(observation),
                        event.height,
                    )?;
                    if !publications.insert(observation.txid) {
                        return Err(EconomicError::Invariant);
                    }
                    self.verify_mark_observation(observation, config.oracle_id)?;
                    let owners: Vec<_> = bases
                        .iter()
                        .filter(|(_, p)| !p.quantity.is_zero())
                        .map(|(o, _)| o)
                        .collect();
                    if owners != event.entries.iter().map(|e| &e.owner).collect::<Vec<_>>() {
                        return Err(EconomicError::Invariant);
                    }
                    for entry in &event.entries {
                        if entry.action
                            != (BasisAction::Mark {
                                price: observation.price.clone(),
                            })
                        {
                            return Err(EconomicError::Invariant);
                        }
                    }
                    mark_height = event.height;
                }
            }
            let mut collected = 0;
            let mut unpaid = 0;
            let mut owners = BTreeSet::new();
            let mut obligations = Vec::with_capacity(event.entries.len());
            for entry in &event.entries {
                let mut expected = entry.clone();
                let obligation = mark_apply_offsets(
                    &mut ious,
                    &mut unpaid_losses,
                    self.iou_key(),
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
            for (entry, (debt, _)) in event.entries.iter().zip(&obligations) {
                normal_owner(&entry.owner)?;
                if !owners.insert(&entry.owner)
                    || add(entry.available_atoms, entry.margin_atoms)? > supply
                {
                    return Err(EconomicError::Invariant);
                }
                let old = bases.get(&entry.owner).cloned().unwrap_or_default();
                let transition = old.transition(&basis_config, &entry.action)?;
                if transition.atoms != entry.atoms {
                    return Err(EconomicError::Invariant);
                }
                if entry.collected_atoms
                    != (*debt).min(add(entry.available_atoms, entry.margin_atoms)?)
                {
                    return Err(EconomicError::Invariant);
                }
                collected = add(collected, entry.collected_atoms)?;
                let missing = debt - entry.collected_atoms;
                unpaid = add(unpaid, missing)?;
                mark_add_unpaid(&mut unpaid_losses, &entry.owner, missing)?;
                let previous = realized.get(&entry.owner).cloned().unwrap_or_default();
                realized.insert(
                    entry.owner.clone(),
                    previous.checked_add(&entry.atoms.to_decimal())?,
                );
                match &entry.action {
                    BasisAction::Trade { delta, price } => {
                        let average = mark_trade_average(
                            &old.quantity,
                            averages.get(&entry.owner).and_then(|p| p.as_ref()),
                            delta,
                            price,
                        )?;
                        averages.insert(entry.owner.clone(), average);
                        last_prices.insert(entry.owner.clone(), price.clone());
                    }
                    BasisAction::Mark { price } => {
                        last_prices.insert(entry.owner.clone(), price.clone());
                    }
                }
                bases.insert(entry.owner.clone(), transition.position);
            }
            if collected != event.collected_loss_atoms || unpaid != event.unfunded_loss_atoms {
                return Err(EconomicError::Invariant);
            }
            pool = add(pool, collected)?;
            bad_debt = add(bad_debt, unpaid)?;
            if collected != 0 {
                ious.add_loss(self.iou_key(), &tokens(collected)?, event.height)?;
            }
            for (entry, (_, profit)) in event.entries.iter().zip(&obligations) {
                mark_replay_claim(
                    &mut ious,
                    self.iou_key(),
                    &entry.owner,
                    *profit,
                    event.height,
                )?;
            }
            let paid = mark_replay_payout(&mut ious, self.iou_key(), &mut pool, event.height)?;
            if paid != event.paid_claim_atoms {
                return Err(EconomicError::Invariant);
            }
            last_height = event.height;
        }
        for epoch in funding_epochs {
            mark_verify_funding_positions(epoch, &bases)?;
        }
        if receipts.next().is_some()
            || bases != self.marking.bases
            || unpaid_losses != self.marking.unpaid_losses
            || ious != self.ious
            || pool != self.loss_pool_atoms
            || bad_debt != self.unfunded_loss_atoms
            || self.positions.positions().count() != bases.len()
            || self.expected_maintenance()? != self.marking.maintenance
        {
            return Err(EconomicError::Invariant);
        }
        for (owner, basis) in &bases {
            basis.validate(&basis_config)?;
            let position = self.positions.get(owner).ok_or(EconomicError::Invariant)?;
            if position.contracts != basis.quantity
                || position.avg_price != averages.get(owner).cloned().unwrap_or_default()
                || position.realized_pnl != realized.get(owner).cloned().unwrap_or_default()
                || position.last_mark != last_prices.get(owner).cloned()
            {
                return Err(EconomicError::Invariant);
            }
        }
        if let Some(oracle) = self.joint.registry().oracle(config.oracle_id) {
            if !oracle.closed
                && self
                    .joint
                    .registry()
                    .activation(18)
                    .is_some_and(|a| a.height < finalized)
            {
                if let Some(quote) = &oracle.latest {
                    if quote.height == finalized {
                        let Some(MarkEvent {
                            kind: MarkEventKind::Mark { observation, .. },
                            ..
                        }) = self.marking.events.last()
                        else {
                            return Err(EconomicError::Invariant);
                        };
                        if observation.height != quote.height
                            || observation.price.checked_mul(&Decimal::from(10_000))?
                                != Decimal::from(quote.price_1e4)
                        {
                            return Err(EconomicError::Invariant);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn mark_verify_funding_positions(
    epoch: &FundingEpoch,
    bases: &BTreeMap<AccountId, BasisPosition>,
) -> Result<()> {
    let expected: BTreeMap<_, _> = bases
        .iter()
        .filter(|(_, p)| !p.quantity.is_zero())
        .map(|(owner, p)| (owner, &p.quantity))
        .collect();
    let recorded: BTreeMap<_, _> = epoch
        .positions
        .iter()
        .map(|p| (&p.owner, &p.contracts))
        .collect();
    if expected != recorded {
        return Err(EconomicError::Invariant);
    }
    Ok(())
}
fn mark_add_unpaid(
    losses: &mut BTreeMap<AccountId, u128>,
    owner: &AccountId,
    atoms: u128,
) -> Result<()> {
    if atoms != 0 {
        losses.insert(
            owner.clone(),
            add(losses.get(owner).copied().unwrap_or(0), atoms)?,
        );
    }
    Ok(())
}
/// Nominal offsets move no Ledger atoms. Cancelling an unpaid positive claim
/// adjusts the IOU bucket by the same amount to keep pool-minus-claims exact.
fn mark_apply_offsets(
    ious: &mut IouStore,
    losses: &mut BTreeMap<AccountId, u128>,
    key: IouKey,
    entry: &mut MarkEventEntry,
    height: u64,
) -> Result<(u128, u128)> {
    entry.claim_offset_atoms = 0;
    entry.loss_offset_atoms = 0;
    if entry.atoms.negative {
        let mut claims = ious
            .claim_map(key)
            .map_or_else(Vec::new, |c| c.claims().to_vec());
        if let Some(claim) = claims.iter_mut().find(|c| c.address == entry.owner) {
            let due = atoms_exact(&claim.amount)?;
            entry.claim_offset_atoms = due.min(entry.atoms.atoms);
            if entry.claim_offset_atoms != 0 {
                claim.amount = tokens(due - entry.claim_offset_atoms)?;
                claims.retain(|c| !c.amount.is_zero());
                ious.save_claim_map(key, height, claims)?;
                ious.add_loss(key, &tokens(entry.claim_offset_atoms)?, height)?;
            }
        }
        Ok((entry.atoms.atoms - entry.claim_offset_atoms, 0))
    } else {
        let debt = losses.get(&entry.owner).copied().unwrap_or(0);
        entry.loss_offset_atoms = debt.min(entry.atoms.atoms);
        let remaining = debt - entry.loss_offset_atoms;
        if remaining == 0 {
            losses.remove(&entry.owner);
        } else {
            losses.insert(entry.owner.clone(), remaining);
        }
        Ok((0, entry.atoms.atoms - entry.loss_offset_atoms))
    }
}
fn mark_trade_average(
    old: &Decimal,
    average: Option<&Decimal>,
    delta: &Decimal,
    price: &Decimal,
) -> Result<Option<Decimal>> {
    let after = old.checked_add(delta)?;
    let opposite = !old.is_zero() && old.is_negative() != delta.is_negative();
    if after.is_zero() {
        Ok(None)
    } else if old.is_zero() || (opposite && delta.abs() > old.abs()) {
        Ok(Some(price.clone()))
    } else if opposite {
        Ok(average.cloned())
    } else {
        Ok(Some(
            average
                .ok_or(EconomicError::Invariant)?
                .checked_mul(&old.abs())?
                .checked_add(&price.checked_mul(&delta.abs())?)?
                .checked_div(&after.abs(), 20, Rounding::HalfUp)?,
        ))
    }
}
fn mark_replay_claim(
    ious: &mut IouStore,
    key: IouKey,
    owner: &AccountId,
    atoms: u128,
    height: u64,
) -> Result<()> {
    if atoms == 0 {
        return Ok(());
    }
    let mut claims = ious
        .claim_map(key)
        .map_or_else(Vec::new, |c| c.claims().to_vec());
    let amount = tokens(atoms)?;
    match claims.iter_mut().find(|c| &c.address == owner) {
        Some(c) => c.amount = c.amount.checked_add(&amount)?,
        None => claims.push(Claim {
            address: owner.clone(),
            amount: amount.clone(),
        }),
    }
    ious.save_claim_map(key, height, claims)?;
    ious.add_profit(key, &amount, height)?;
    Ok(())
}
fn mark_replay_payout(
    ious: &mut IouStore,
    key: IouKey,
    pool: &mut u128,
    height: u64,
) -> Result<u128> {
    let mut claims = ious
        .claim_map(key)
        .map_or_else(Vec::new, |c| c.claims().to_vec());
    let mut total = 0;
    for claim in &mut claims {
        let due = atoms_exact(&claim.amount)?;
        let paid = due.min(*pool);
        *pool -= paid;
        total = add(total, paid)?;
        claim.amount = tokens(due - paid)?;
    }
    claims.retain(|c| !c.amount.is_zero());
    if ious.claim_map(key).is_some() {
        ious.save_claim_map(key, height, claims)?;
    }
    Ok(total)
}
