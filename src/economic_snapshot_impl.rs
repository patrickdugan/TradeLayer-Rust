// TLEC v1: complete combined economic metadata plus the sole Joint-owned
// Ledger. Canonical structure and CRC are not authentication of state/history.
use crate::codec::{verify_crc, with_crc, Cursor, MAX_CHECKPOINT_BYTES};
use crate::contract_orderbook::{
    BookSnapshot, RestingOrder, MAX_CONTRACT_ORDERS, MAX_CONTRACT_ORDER_IDS,
};
use crate::tally::{
    FeeAccrualPlan, FeeBookSnapshot, FeeDrawPlan, FeeJournalEntry, FeeKey, FeeQueuePlan, FeeRow,
    QueuedFee, MAX_FEE_JOURNAL, MAX_FEE_ROWS, MAX_QUEUED_FEES,
};

const ECONOMIC_STATE_MAGIC: &[u8; 5] = b"TLEC\x01";
const ECONOMIC_FUNDING_MAGIC: &[u8; 5] = b"TLEC\x02";
const MAX_ECONOMIC_DECIMAL_TEXT: usize = 128;

fn econ_snapshot_bad() -> EconomicError {
    EconomicError::InvalidSnapshot
}

struct EconomicWriter(Vec<u8>);
impl EconomicWriter {
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_CHECKPOINT_BYTES - 4)
        {
            return Err(EconomicError::Capacity);
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn byte(&mut self, value: u8) -> Result<()> {
        self.put(&[value])
    }
    fn u32(&mut self, value: u32) -> Result<()> {
        self.put(&value.to_be_bytes())
    }
    fn u64(&mut self, value: u64) -> Result<()> {
        self.put(&value.to_be_bytes())
    }
    fn atoms(&mut self, value: u128) -> Result<()> {
        self.put(&value.to_be_bytes())
    }
    fn count(&mut self, value: usize, limit: usize) -> Result<()> {
        if value > limit {
            return Err(EconomicError::Capacity);
        }
        self.u32(u32::try_from(value).map_err(|_| EconomicError::Capacity)?)
    }
    fn embedded(&mut self, bytes: &[u8]) -> Result<()> {
        self.count(bytes.len(), MAX_CHECKPOINT_BYTES)?;
        self.put(bytes)
    }
    fn decimal(&mut self, value: &Decimal) -> Result<()> {
        bounded(value)?;
        let text = value.to_string();
        if text.len() > MAX_ECONOMIC_DECIMAL_TEXT {
            return Err(EconomicError::Capacity);
        }
        self.byte(text.len() as u8)?;
        self.put(text.as_bytes())
    }
    fn optional_decimal(&mut self, value: &Option<Decimal>) -> Result<()> {
        match value {
            None => self.byte(0),
            Some(value) => {
                self.byte(1)?;
                self.decimal(value)
            }
        }
    }
    fn height(&mut self, value: Option<u64>) -> Result<()> {
        match value {
            None => self.byte(0),
            Some(value) => {
                self.byte(1)?;
                self.u64(value)
            }
        }
    }
    fn account(&mut self, value: &AccountId) -> Result<()> {
        self.byte(value.as_str().len() as u8)?;
        self.put(value.as_str().as_bytes())
    }
    fn txid(&mut self, value: TxId) -> Result<()> {
        self.put(&value.as_bytes())
    }
    fn route(&mut self, value: FeeRoute) -> Result<()> {
        match value {
            FeeRoute::Spot => self.byte(0),
            FeeRoute::Native(id) => {
                self.byte(1)?;
                self.u32(id)
            }
            FeeRoute::Oracle(id) => {
                self.byte(2)?;
                self.u32(id)
            }
        }
    }
    fn fee_key(&mut self, value: FeeKey) -> Result<()> {
        self.u32(value.property.get())?;
        self.u32(value.contract)
    }
}

fn econ_read_count(reader: &mut Cursor<'_>, limit: usize) -> Result<usize> {
    let count = reader.u32().map_err(|_| econ_snapshot_bad())? as usize;
    if count > limit {
        return Err(EconomicError::Capacity);
    }
    Ok(count)
}
fn econ_read_bool(reader: &mut Cursor<'_>) -> Result<bool> {
    match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(econ_snapshot_bad()),
    }
}
fn econ_read_decimal(reader: &mut Cursor<'_>) -> Result<Decimal> {
    let length = reader.u8().map_err(|_| econ_snapshot_bad())? as usize;
    if length == 0 || length > MAX_ECONOMIC_DECIMAL_TEXT {
        return Err(econ_snapshot_bad());
    }
    let text = core::str::from_utf8(reader.take(length).map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    let value: Decimal = text.parse().map_err(|_| econ_snapshot_bad())?;
    bounded(&value)?;
    if value.to_string() != text {
        return Err(econ_snapshot_bad());
    }
    Ok(value)
}
fn econ_read_optional_decimal(reader: &mut Cursor<'_>) -> Result<Option<Decimal>> {
    if econ_read_bool(reader)? {
        Ok(Some(econ_read_decimal(reader)?))
    } else {
        Ok(None)
    }
}
fn econ_read_height(reader: &mut Cursor<'_>) -> Result<Option<u64>> {
    if econ_read_bool(reader)? {
        Ok(Some(reader.u64().map_err(|_| econ_snapshot_bad())?))
    } else {
        Ok(None)
    }
}
fn econ_read_account(reader: &mut Cursor<'_>) -> Result<AccountId> {
    let length = reader.u8().map_err(|_| econ_snapshot_bad())? as usize;
    if length == 0 || length > crate::ledger::MAX_ACCOUNT_BYTES {
        return Err(econ_snapshot_bad());
    }
    let text = core::str::from_utf8(reader.take(length).map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    text.try_into().map_err(|_| econ_snapshot_bad())
}
fn econ_read_txid(reader: &mut Cursor<'_>) -> Result<TxId> {
    Ok(TxId::from_bytes(
        reader.fixed::<32>().map_err(|_| econ_snapshot_bad())?,
    ))
}
fn econ_read_atoms(reader: &mut Cursor<'_>) -> Result<u128> {
    reader.u128().map_err(|_| econ_snapshot_bad())
}
fn econ_read_u64(reader: &mut Cursor<'_>) -> Result<u64> {
    reader.u64().map_err(|_| econ_snapshot_bad())
}
fn econ_read_property(reader: &mut Cursor<'_>) -> Result<PropertyId> {
    reader
        .u32()
        .map_err(|_| econ_snapshot_bad())?
        .try_into()
        .map_err(|_| econ_snapshot_bad())
}
fn econ_read_contract(reader: &mut Cursor<'_>) -> Result<ContractId> {
    reader
        .u32()
        .map_err(|_| econ_snapshot_bad())?
        .try_into()
        .map_err(|_| econ_snapshot_bad())
}
fn econ_read_route(reader: &mut Cursor<'_>) -> Result<FeeRoute> {
    match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => Ok(FeeRoute::Spot),
        1 => Ok(FeeRoute::Native(
            reader.u32().map_err(|_| econ_snapshot_bad())?,
        )),
        2 => Ok(FeeRoute::Oracle(
            reader.u32().map_err(|_| econ_snapshot_bad())?,
        )),
        _ => Err(econ_snapshot_bad()),
    }
}
fn econ_read_fee_key(reader: &mut Cursor<'_>) -> Result<FeeKey> {
    Ok(FeeKey {
        property: econ_read_property(reader)?,
        contract: reader.u32().map_err(|_| econ_snapshot_bad())?,
    })
}
fn econ_read_embedded<'a>(reader: &mut Cursor<'a>) -> Result<&'a [u8]> {
    let length = econ_read_count(reader, MAX_CHECKPOINT_BYTES)?;
    if length == 0 {
        return Err(econ_snapshot_bad());
    }
    reader.take(length).map_err(|_| econ_snapshot_bad())
}

fn econ_write_config(out: &mut EconomicWriter, config: &EconomicConfig) -> Result<()> {
    out.byte(config.network.tag())?;
    out.put(&config.network.genesis().0)?;
    out.u32(config.contract.get())?;
    out.u32(config.collateral.get())?;
    out.byte(u8::from(config.inverse))?;
    out.decimal(&config.notional)?;
    out.decimal(&config.initial_margin_per_contract)?;
    out.route(config.fee_route)?;
    match config.policy {
        SettlementPolicy::ConservingV1 => out.byte(1)?,
    }
    if let Some(funding) = &config.funding {
        out.u32(funding.oracle_id)?;
        out.u64(funding.max_staleness_blocks)?;
        match funding.policy {
            FundingPolicy::ConservingFundingV1 => out.byte(1)?,
        }
    }
    Ok(())
}
fn econ_read_config(reader: &mut Cursor<'_>, has_funding: bool) -> Result<EconomicConfig> {
    let network = TradeLayerMainnet::from_tag(reader.u8().map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    if reader.fixed::<32>().map_err(|_| econ_snapshot_bad())? != network.genesis().0 {
        return Err(econ_snapshot_bad());
    }
    let config = EconomicConfig {
        network,
        contract: econ_read_contract(reader)?,
        collateral: econ_read_property(reader)?,
        inverse: econ_read_bool(reader)?,
        notional: econ_read_decimal(reader)?,
        initial_margin_per_contract: econ_read_decimal(reader)?,
        fee_route: econ_read_route(reader)?,
        policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
            1 => SettlementPolicy::ConservingV1,
            _ => return Err(econ_snapshot_bad()),
        },
        funding: if has_funding {
            Some(FundingConfig {
                oracle_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
                max_staleness_blocks: econ_read_u64(reader)?,
                policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                    1 => FundingPolicy::ConservingFundingV1,
                    _ => return Err(econ_snapshot_bad()),
                },
            })
        } else {
            None
        },
    };
    config.validate()?;
    Ok(config)
}

fn econ_write_order(out: &mut EconomicWriter, order: &Order) -> Result<()> {
    out.txid(order.txid)?;
    out.account(&order.owner)?;
    out.u32(order.contract.get())?;
    out.u64(order.height)?;
    out.byte(match order.side {
        Side::Buy => 0,
        Side::Sell => 1,
    })?;
    out.decimal(&order.quantity)?;
    out.decimal(&order.price)?;
    out.byte(u8::from(order.post))?;
    out.byte(u8::from(order.is_market))
}
fn econ_read_order(reader: &mut Cursor<'_>) -> Result<Order> {
    Ok(Order {
        txid: econ_read_txid(reader)?,
        owner: econ_read_account(reader)?,
        contract: econ_read_contract(reader)?,
        height: econ_read_u64(reader)?,
        side: match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => Side::Buy,
            1 => Side::Sell,
            _ => return Err(econ_snapshot_bad()),
        },
        quantity: econ_read_decimal(reader)?,
        price: econ_read_decimal(reader)?,
        post: econ_read_bool(reader)?,
        is_market: econ_read_bool(reader)?,
    })
}
fn econ_write_book(out: &mut EconomicWriter, book: &ContractOrderbook) -> Result<()> {
    let snapshot = book.snapshot();
    out.u32(snapshot.contract.get())?;
    for orders in [&snapshot.queued, &snapshot.drained] {
        out.count(orders.len(), MAX_CONTRACT_ORDERS)?;
        for order in orders {
            econ_write_order(out, order)?;
        }
    }
    out.count(snapshot.resting.len(), MAX_CONTRACT_ORDERS)?;
    for resting in &snapshot.resting {
        econ_write_order(out, &resting.order)?;
        out.decimal(&resting.remaining)?;
        out.decimal(&resting.reserve_quantity)?;
        out.byte(u8::from(resting.initial_reduce))?;
        out.u64(resting.sequence)?;
    }
    out.count(snapshot.seen.len(), MAX_CONTRACT_ORDER_IDS)?;
    for id in snapshot.seen {
        out.txid(id)?;
    }
    out.u64(snapshot.next_sequence)?;
    out.height(snapshot.processed_through)
}
fn econ_read_book(reader: &mut Cursor<'_>) -> Result<ContractOrderbook> {
    let contract = econ_read_contract(reader)?;
    let mut queued = Vec::new();
    let mut drained = Vec::new();
    for orders in [&mut queued, &mut drained] {
        let count = econ_read_count(reader, MAX_CONTRACT_ORDERS)?;
        for _ in 0..count {
            orders.push(econ_read_order(reader)?);
        }
    }
    let count = econ_read_count(reader, MAX_CONTRACT_ORDERS)?;
    let mut resting = Vec::new();
    for _ in 0..count {
        resting.push(RestingOrder {
            order: econ_read_order(reader)?,
            remaining: econ_read_decimal(reader)?,
            reserve_quantity: econ_read_decimal(reader)?,
            initial_reduce: econ_read_bool(reader)?,
            sequence: econ_read_u64(reader)?,
        });
    }
    let count = econ_read_count(reader, MAX_CONTRACT_ORDER_IDS)?;
    let mut seen = Vec::new();
    for _ in 0..count {
        seen.push(econ_read_txid(reader)?);
    }
    Ok(ContractOrderbook::from_snapshot(BookSnapshot {
        contract,
        queued,
        drained,
        resting,
        seen,
        next_sequence: econ_read_u64(reader)?,
        processed_through: econ_read_height(reader)?,
    })?)
}

fn econ_write_positions(out: &mut EconomicWriter, positions: &MarginMap) -> Result<()> {
    let mut runtime = MarginMap::new(positions.contract());
    out.u32(positions.contract().get())?;
    out.count(positions.positions().count(), MAX_ECONOMIC_POSITIONS)?;
    for p in positions.positions() {
        runtime.write_position(p.clone())?;
        if !p.options.is_empty() {
            return Err(econ_snapshot_bad());
        }
        out.account(&p.address)?;
        out.decimal(&p.contracts)?;
        out.optional_decimal(&p.avg_price)?;
        out.decimal(&p.realized_pnl)?;
        out.optional_decimal(&p.last_mark)?;
        out.decimal(&p.margin)?;
        out.decimal(&p.iou_claim)?;
    }
    // No hidden deltas, expiry indexes or option OI may disappear on restart.
    if runtime != *positions {
        return Err(econ_snapshot_bad());
    }
    Ok(())
}
fn econ_read_positions(reader: &mut Cursor<'_>) -> Result<MarginMap> {
    let mut positions = MarginMap::new(econ_read_contract(reader)?);
    let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
    for _ in 0..count {
        let mut p = Position::new(econ_read_account(reader)?);
        if positions.get(&p.address).is_some() {
            return Err(econ_snapshot_bad());
        }
        p.contracts = econ_read_decimal(reader)?;
        p.avg_price = econ_read_optional_decimal(reader)?;
        p.realized_pnl = econ_read_decimal(reader)?;
        p.last_mark = econ_read_optional_decimal(reader)?;
        p.margin = econ_read_decimal(reader)?;
        p.iou_claim = econ_read_decimal(reader)?;
        positions.write_position(p)?;
    }
    Ok(positions)
}

fn econ_write_queued_fee(out: &mut EconomicWriter, q: QueuedFee) -> Result<()> {
    out.u64(q.block)?;
    out.fee_key(q.key)?;
    out.route(q.route)?;
    out.atoms(q.atoms)
}
fn econ_read_queued_fee(reader: &mut Cursor<'_>) -> Result<QueuedFee> {
    Ok(QueuedFee {
        block: econ_read_u64(reader)?,
        key: econ_read_fee_key(reader)?,
        route: econ_read_route(reader)?,
        atoms: econ_read_atoms(reader)?,
    })
}
fn econ_write_fees(out: &mut EconomicWriter, fees: &FeeBook) -> Result<()> {
    let snapshot = fees.snapshot();
    out.count(snapshot.rows.len(), MAX_FEE_ROWS)?;
    for (key, row) in snapshot.rows {
        out.fee_key(key)?;
        out.atoms(row.value_atoms)?;
        out.atoms(row.stash_atoms)?;
    }
    out.count(snapshot.queued.len(), MAX_QUEUED_FEES)?;
    for q in snapshot.queued {
        econ_write_queued_fee(out, q)?;
    }
    out.count(snapshot.journal.len(), MAX_FEE_JOURNAL)?;
    for entry in snapshot.journal {
        match entry {
            FeeJournalEntry::Accrued(p) => {
                out.byte(0)?;
                out.u64(p.block)?;
                out.fee_key(p.key)?;
                out.route(p.route)?;
                for atoms in [
                    p.amount_atoms,
                    p.collection_atoms,
                    p.value_added_atoms,
                    p.stash_added_atoms,
                    p.insurance_atoms,
                    p.custody_before_atoms,
                    p.custody_after_atoms,
                ] {
                    out.atoms(atoms)?;
                }
            }
            FeeJournalEntry::Queued(p) => {
                out.byte(1)?;
                econ_write_queued_fee(out, p.entry)?;
                out.atoms(p.custody_before_atoms)?;
                out.atoms(p.custody_after_atoms)?;
            }
            FeeJournalEntry::Drawn(p) => {
                out.byte(2)?;
                out.u64(p.block)?;
                out.fee_key(p.key)?;
                for atoms in [
                    p.value_atoms,
                    p.stash_atoms,
                    p.custody_before_atoms,
                    p.custody_after_atoms,
                ] {
                    out.atoms(atoms)?;
                }
            }
        }
    }
    Ok(())
}
fn econ_read_fees(reader: &mut Cursor<'_>) -> Result<FeeBook> {
    let count = econ_read_count(reader, MAX_FEE_ROWS)?;
    let mut rows = Vec::new();
    for _ in 0..count {
        rows.push((
            econ_read_fee_key(reader)?,
            FeeRow {
                value_atoms: econ_read_atoms(reader)?,
                stash_atoms: econ_read_atoms(reader)?,
            },
        ));
    }
    let count = econ_read_count(reader, MAX_QUEUED_FEES)?;
    let mut queued = Vec::new();
    for _ in 0..count {
        queued.push(econ_read_queued_fee(reader)?);
    }
    let count = econ_read_count(reader, MAX_FEE_JOURNAL)?;
    let mut journal = Vec::new();
    for _ in 0..count {
        journal.push(match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => FeeJournalEntry::Accrued(FeeAccrualPlan {
                block: econ_read_u64(reader)?,
                key: econ_read_fee_key(reader)?,
                route: econ_read_route(reader)?,
                amount_atoms: econ_read_atoms(reader)?,
                collection_atoms: econ_read_atoms(reader)?,
                value_added_atoms: econ_read_atoms(reader)?,
                stash_added_atoms: econ_read_atoms(reader)?,
                insurance_atoms: econ_read_atoms(reader)?,
                custody_before_atoms: econ_read_atoms(reader)?,
                custody_after_atoms: econ_read_atoms(reader)?,
            }),
            1 => FeeJournalEntry::Queued(FeeQueuePlan {
                entry: econ_read_queued_fee(reader)?,
                custody_before_atoms: econ_read_atoms(reader)?,
                custody_after_atoms: econ_read_atoms(reader)?,
            }),
            2 => FeeJournalEntry::Drawn(FeeDrawPlan {
                block: econ_read_u64(reader)?,
                key: econ_read_fee_key(reader)?,
                value_atoms: econ_read_atoms(reader)?,
                stash_atoms: econ_read_atoms(reader)?,
                custody_before_atoms: econ_read_atoms(reader)?,
                custody_after_atoms: econ_read_atoms(reader)?,
            }),
            _ => return Err(econ_snapshot_bad()),
        });
    }
    Ok(FeeBook::from_snapshot(FeeBookSnapshot {
        rows,
        queued,
        journal,
    })?)
}

#[derive(Default)]
struct FundingRecordBudget {
    used: usize,
}
impl FundingRecordBudget {
    // The common budget covers retained observations, epoch window samples,
    // captured positions and both payment lists. Epoch/index/plan headers have
    // their independent 4096-epoch bound and cannot amplify vector counts.
    fn take(&mut self, count: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or(EconomicError::Capacity)?;
        if self.used > MAX_FUNDING_RECORDS {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
}

fn econ_write_funding_observation(
    out: &mut EconomicWriter,
    observation: &FundingObservation,
) -> Result<()> {
    out.txid(observation.txid)?;
    out.u64(observation.height)?;
    out.decimal(&observation.price)
}
fn econ_read_funding_observation(reader: &mut Cursor<'_>) -> Result<FundingObservation> {
    Ok(FundingObservation {
        txid: econ_read_txid(reader)?,
        height: econ_read_u64(reader)?,
        price: econ_read_decimal(reader)?,
    })
}
fn econ_write_funding_payment(
    out: &mut EconomicWriter,
    payment: &crate::funding::FundingPayment,
) -> Result<()> {
    out.account(&payment.owner)?;
    out.atoms(payment.atoms)
}
fn econ_read_funding_payment(reader: &mut Cursor<'_>) -> Result<crate::funding::FundingPayment> {
    Ok(crate::funding::FundingPayment {
        owner: econ_read_account(reader)?,
        atoms: econ_read_atoms(reader)?,
    })
}
fn econ_write_funding(out: &mut EconomicWriter, funding: &FundingState) -> Result<()> {
    let mut budget = FundingRecordBudget::default();
    budget.take(funding.observations.len())?;
    out.count(funding.observations.len(), MAX_FUNDING_OBSERVATIONS)?;
    for observation in &funding.observations {
        econ_write_funding_observation(out, observation)?;
    }
    match &funding.closed_at {
        None => out.byte(0)?,
        Some(closure) => {
            out.byte(1)?;
            out.txid(closure.txid)?;
            out.u64(closure.height)?;
        }
    }
    out.count(funding.epochs.len(), MAX_FUNDING_EPOCHS)?;
    for epoch in &funding.epochs {
        out.u64(epoch.height)?;
        out.txid(epoch.id)?;
        out.byte(match epoch.status {
            FundingStatus::Paid => 0,
            FundingStatus::ZeroRate => 1,
            FundingStatus::NoPositions => 2,
            FundingStatus::MissingPrice => 3,
            FundingStatus::StalePrice => 4,
            FundingStatus::ClosedOracle => 5,
        })?;
        match &epoch.index {
            None => out.byte(0)?,
            Some(index) => {
                out.byte(1)?;
                econ_write_funding_observation(out, index)?;
            }
        }
        budget.take(epoch.window.len())?;
        out.count(epoch.window.len(), MAX_FUNDING_OBSERVATIONS)?;
        for observation in &epoch.window {
            econ_write_funding_observation(out, observation)?;
        }
        budget.take(epoch.positions.len())?;
        out.count(epoch.positions.len(), MAX_ECONOMIC_POSITIONS)?;
        for position in &epoch.positions {
            out.account(&position.owner)?;
            out.decimal(&position.contracts)?;
            out.atoms(position.available_atoms)?;
        }
        match &epoch.plan {
            None => out.byte(0)?,
            Some(plan) => {
                out.byte(1)?;
                out.decimal(&plan.rate_bps)?;
                out.decimal(&plan.collateral_notional)?;
                out.atoms(plan.total_atoms)?;
                for payments in [&plan.debits, &plan.credits] {
                    budget.take(payments.len())?;
                    out.count(payments.len(), MAX_ECONOMIC_POSITIONS)?;
                    for payment in payments {
                        econ_write_funding_payment(out, payment)?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn econ_read_funding(reader: &mut Cursor<'_>) -> Result<FundingState> {
    let mut budget = FundingRecordBudget::default();
    let count = econ_read_count(reader, MAX_FUNDING_OBSERVATIONS)?;
    budget.take(count)?;
    let mut observations = Vec::new();
    for _ in 0..count {
        observations.push(econ_read_funding_observation(reader)?);
    }
    let closed_at = if econ_read_bool(reader)? {
        Some(FundingClosure {
            txid: econ_read_txid(reader)?,
            height: econ_read_u64(reader)?,
        })
    } else {
        None
    };
    let count = econ_read_count(reader, MAX_FUNDING_EPOCHS)?;
    let mut epochs = Vec::new();
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let id = econ_read_txid(reader)?;
        let status = match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => FundingStatus::Paid,
            1 => FundingStatus::ZeroRate,
            2 => FundingStatus::NoPositions,
            3 => FundingStatus::MissingPrice,
            4 => FundingStatus::StalePrice,
            5 => FundingStatus::ClosedOracle,
            _ => return Err(econ_snapshot_bad()),
        };
        let index = if econ_read_bool(reader)? {
            Some(econ_read_funding_observation(reader)?)
        } else {
            None
        };
        let count = econ_read_count(reader, MAX_FUNDING_OBSERVATIONS)?;
        budget.take(count)?;
        let mut window = Vec::new();
        for _ in 0..count {
            window.push(econ_read_funding_observation(reader)?);
        }
        let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
        budget.take(count)?;
        let mut positions = Vec::new();
        for _ in 0..count {
            positions.push(FundingPosition {
                owner: econ_read_account(reader)?,
                contracts: econ_read_decimal(reader)?,
                available_atoms: econ_read_atoms(reader)?,
            });
        }
        let plan = if econ_read_bool(reader)? {
            let rate_bps = econ_read_decimal(reader)?;
            let collateral_notional = econ_read_decimal(reader)?;
            let total_atoms = econ_read_atoms(reader)?;
            let mut debits = Vec::new();
            let mut credits = Vec::new();
            for payments in [&mut debits, &mut credits] {
                let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
                budget.take(count)?;
                for _ in 0..count {
                    payments.push(econ_read_funding_payment(reader)?);
                }
            }
            Some(FundingPlan {
                rate_bps,
                collateral_notional,
                total_atoms,
                debits,
                credits,
            })
        } else {
            None
        };
        epochs.push(FundingEpoch {
            height,
            id,
            status,
            index,
            window,
            positions,
            plan,
        });
    }
    Ok(FundingState {
        observations,
        closed_at,
        epochs,
    })
}

impl EconomicState {
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.verify()?;
        let mut out = EconomicWriter(if self.config.funding.is_some() {
            ECONOMIC_FUNDING_MAGIC.to_vec()
        } else {
            ECONOMIC_STATE_MAGIC.to_vec()
        });
        econ_write_config(&mut out, &self.config)?;
        out.embedded(
            &self
                .joint
                .snapshot_for_economic_bytes()
                .map_err(|_| econ_snapshot_bad())?,
        )?;
        econ_write_book(&mut out, &self.book)?;
        econ_write_positions(&mut out, &self.positions)?;
        out.embedded(
            &self
                .ious
                .snapshot_bytes()
                .map_err(|_| econ_snapshot_bad())?,
        )?;
        econ_write_fees(&mut out, &self.fees)?;
        out.count(self.reserves.len(), MAX_CONTRACT_ORDERS)?;
        for (id, reserve) in &self.reserves {
            out.txid(*id)?;
            out.account(&reserve.owner)?;
            out.decimal(&reserve.quantity)?;
            out.atoms(reserve.atoms)?;
        }
        out.count(self.margins.len(), MAX_ECONOMIC_POSITIONS)?;
        for (owner, atoms) in &self.margins {
            out.account(owner)?;
            out.atoms(*atoms)?;
        }
        out.count(
            self.cancel_ids.len(),
            crate::joint::MAX_JOINT_REPLAY_MARKERS,
        )?;
        for id in &self.cancel_ids {
            out.txid(*id)?;
        }
        for atoms in [
            self.loss_pool_atoms,
            self.insurance_custody_atoms,
            self.unfunded_loss_atoms,
        ] {
            out.atoms(atoms)?;
        }
        out.count(self.receipts.len(), MAX_ECONOMIC_RECEIPTS)?;
        for receipt in &self.receipts {
            out.u64(receipt.height)?;
            out.txid(receipt.buy_txid)?;
            out.txid(receipt.sell_txid)?;
            for value in [
                &receipt.quantity,
                &receipt.price,
                &receipt.buyer_pnl,
                &receipt.seller_pnl,
            ] {
                out.decimal(value)?;
            }
            for atoms in [
                receipt.retained_fee_atoms,
                receipt.collected_loss_atoms,
                receipt.paid_claim_atoms,
                receipt.unfunded_loss_atoms,
            ] {
                out.atoms(atoms)?;
            }
        }
        out.u64(self.internal_sequence)?;
        if self.config.funding.is_some() {
            econ_write_funding(&mut out, &self.funding)?;
        }
        with_crc(out.0).map_err(|_| EconomicError::Capacity)
    }

    /// Config, administrator and spot policy must come from independent
    /// deployment inputs; checkpoint bytes establish no authority of their own.
    pub fn from_snapshot_bytes(
        bytes: &[u8],
        expected_config: &EconomicConfig,
        admin: &AccountId,
        permitted: &BTreeSet<PropertyId>,
    ) -> Result<Self> {
        expected_config.validate()?;
        let raw = verify_crc(bytes).map_err(|_| econ_snapshot_bad())?;
        let mut reader = Cursor::new(raw);
        let version = reader.fixed::<5>().map_err(|_| econ_snapshot_bad())?;
        let has_funding = if version == *ECONOMIC_STATE_MAGIC {
            false
        } else if version == *ECONOMIC_FUNDING_MAGIC {
            true
        } else {
            return Err(econ_snapshot_bad());
        };
        let config = econ_read_config(&mut reader, has_funding)?;
        if &config != expected_config {
            return Err(EconomicError::InvalidConfig);
        }
        let joint = JointProtocolState::from_economic_snapshot_bytes(
            econ_read_embedded(&mut reader)?,
            config.network,
            admin,
            permitted,
        )
        .map_err(|_| econ_snapshot_bad())?;
        let book = econ_read_book(&mut reader)?;
        let positions = econ_read_positions(&mut reader)?;
        let ious = IouStore::from_snapshot_bytes(econ_read_embedded(&mut reader)?)
            .map_err(|_| econ_snapshot_bad())?;
        let fees = econ_read_fees(&mut reader)?;
        let mut reserves = BTreeMap::new();
        let mut last = None;
        let count = econ_read_count(&mut reader, MAX_CONTRACT_ORDERS)?;
        for _ in 0..count {
            let id = econ_read_txid(&mut reader)?;
            if last.is_some_and(|p| id <= p) {
                return Err(econ_snapshot_bad());
            }
            last = Some(id);
            reserves.insert(
                id,
                OrderReserve {
                    owner: econ_read_account(&mut reader)?,
                    quantity: econ_read_decimal(&mut reader)?,
                    atoms: econ_read_atoms(&mut reader)?,
                },
            );
        }
        let mut margins = BTreeMap::new();
        let mut last = None;
        let count = econ_read_count(&mut reader, MAX_ECONOMIC_POSITIONS)?;
        for _ in 0..count {
            let owner = econ_read_account(&mut reader)?;
            if last.as_ref().is_some_and(|p| &owner <= p) {
                return Err(econ_snapshot_bad());
            }
            last = Some(owner.clone());
            margins.insert(owner, econ_read_atoms(&mut reader)?);
        }
        let mut cancel_ids = BTreeSet::new();
        let mut last = None;
        let count = econ_read_count(&mut reader, crate::joint::MAX_JOINT_REPLAY_MARKERS)?;
        for _ in 0..count {
            let id = econ_read_txid(&mut reader)?;
            if last.is_some_and(|p| id <= p) {
                return Err(econ_snapshot_bad());
            }
            last = Some(id);
            cancel_ids.insert(id);
        }
        let loss_pool_atoms = econ_read_atoms(&mut reader)?;
        let insurance_custody_atoms = econ_read_atoms(&mut reader)?;
        let unfunded_loss_atoms = econ_read_atoms(&mut reader)?;
        let count = econ_read_count(&mut reader, MAX_ECONOMIC_RECEIPTS)?;
        let mut receipts = Vec::new();
        for _ in 0..count {
            receipts.push(TradeReceipt {
                height: econ_read_u64(&mut reader)?,
                buy_txid: econ_read_txid(&mut reader)?,
                sell_txid: econ_read_txid(&mut reader)?,
                quantity: econ_read_decimal(&mut reader)?,
                price: econ_read_decimal(&mut reader)?,
                buyer_pnl: econ_read_decimal(&mut reader)?,
                seller_pnl: econ_read_decimal(&mut reader)?,
                retained_fee_atoms: econ_read_atoms(&mut reader)?,
                collected_loss_atoms: econ_read_atoms(&mut reader)?,
                paid_claim_atoms: econ_read_atoms(&mut reader)?,
                unfunded_loss_atoms: econ_read_atoms(&mut reader)?,
            });
        }
        let internal_sequence = econ_read_u64(&mut reader)?;
        let funding = if has_funding {
            econ_read_funding(&mut reader)?
        } else {
            FundingState::default()
        };
        reader.end().map_err(|_| econ_snapshot_bad())?;
        let result = Self {
            config,
            joint,
            book,
            positions,
            ious,
            fees,
            reserves,
            margins,
            cancel_ids,
            loss_pool_atoms,
            insurance_custody_atoms,
            unfunded_loss_atoms,
            receipts,
            internal_sequence,
            funding,
        };
        result.verify()?;
        if result.snapshot_bytes()?.as_slice() != bytes {
            return Err(econ_snapshot_bad());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod funding_codec_compat_tests {
    use super::*;

    #[test]
    fn disabled_funding_preserves_genuine_pre_funding_tlec_v1_bytes() {
        let mut ledger = Ledger::new();
        ledger
            .apply(
                TxId::from_bytes([1; 32]),
                crate::ledger::Operation::CreateProperty {
                    id: 7.try_into().unwrap(),
                    issuer: "alice".try_into().unwrap(),
                    kind: crate::ledger::PropertyKind::Fixed,
                    initial_atoms: 1000,
                },
            )
            .unwrap();
        let joint = JointProtocolState::from_bootstrap_unverified(
            TradeLayerMainnet::Bitcoin,
            "admin".try_into().unwrap(),
            ledger,
            BTreeSet::from([7.try_into().unwrap()]),
        )
        .unwrap();
        let config = EconomicConfig {
            network: TradeLayerMainnet::Bitcoin,
            contract: 1.try_into().unwrap(),
            collateral: 7.try_into().unwrap(),
            inverse: false,
            notional: "1".parse().unwrap(),
            initial_margin_per_contract: "10".parse().unwrap(),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
        };
        let mut state = EconomicState::from_joint_unverified(config, joint).unwrap();
        state
            .apply_block_unverified(
                1,
                vec![EconomicInput::Joint(UnverifiedJointTx {
                    txid: TxId::from_bytes([10; 32]),
                    actor: "admin".try_into().unwrap(),
                    operation: crate::joint::JointOperation::Registry(
                        crate::registry::RegistryAction::Activate {
                            types: vec![1, 2, 5, 6, 18],
                            code_hash: [0x51; 32],
                        },
                    ),
                })],
            )
            .unwrap();
        let bytes = state.snapshot_bytes().unwrap();
        // Captured from the pre-funding compiled owner, not this encoder.
        assert_eq!(bytes.len(), 800);
        assert_eq!(
            crate::hash256::double_sha256(&bytes),
            [
                162, 132, 6, 167, 118, 174, 82, 22, 72, 182, 150, 144, 228, 90, 170, 1, 229, 76,
                147, 103, 41, 63, 157, 33, 143, 205, 144, 232, 244, 246, 14, 208,
            ]
        );
        let restored = EconomicState::from_snapshot_bytes(
            &bytes,
            state.config(),
            &"admin".try_into().unwrap(),
            &BTreeSet::from([7.try_into().unwrap()]),
        )
        .unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.funding, FundingState::default());
    }
}
