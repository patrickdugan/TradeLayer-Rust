// Versioned TLEC: complete combined economic metadata plus the sole Joint-owned
// Ledger. Canonical structure and CRC are not authentication of state/history.
use crate::codec::{verify_crc, with_crc, Cursor, MAX_CHECKPOINT_BYTES};
use crate::contract_orderbook::{
    BookSnapshot, RestingOrder, MAX_CONTRACT_ORDERS, MAX_CONTRACT_ORDER_IDS,
};
use crate::joint::JointOperation;
use crate::mark_basis::{ExactRatio, MAX_BASIS_RATIO_DIGITS};
use crate::registry::{AdminTarget, RegistryAction};
use crate::spot::{CancelSelector, SpotIntent};
use crate::tally::{
    FeeAccrualPlan, FeeBookSnapshot, FeeDrawPlan, FeeJournalEntry, FeeKey, FeeQueuePlan, FeeRow,
    QueuedFee, MAX_FEE_JOURNAL, MAX_FEE_ROWS, MAX_QUEUED_FEES,
};

const ECONOMIC_STATE_MAGIC: &[u8; 5] = b"TLEC\x01";
const ECONOMIC_FUNDING_MAGIC: &[u8; 5] = b"TLEC\x02";
const ECONOMIC_MARK_MAGIC: &[u8; 5] = b"TLEC\x03";
const ECONOMIC_LIQUIDATION_MAGIC: &[u8; 5] = b"TLEC\x04";
const ECONOMIC_AMM_MAGIC: &[u8; 5] = b"TLEC\x05";
const ECONOMIC_CONTRACT_MAGIC: &[u8; 5] = b"TLEC\x06";
const MAX_FUNDED_AMM_QUOTES: usize = 2;
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
    if config.contract_binding.is_some() {
        out.byte(
            u8::from(config.funding.is_some())
                | (u8::from(config.liquidation.is_some()) << 1)
                | (u8::from(config.amm.is_some()) << 2),
        )?;
    }
    if config.marking.is_some() {
        out.byte(u8::from(config.funding.is_some()))?;
    }
    if let Some(funding) = &config.funding {
        out.u32(funding.oracle_id)?;
        out.u64(funding.max_staleness_blocks)?;
        match funding.policy {
            FundingPolicy::ConservingFundingV1 => out.byte(1)?,
        }
    }
    if let Some(marking) = &config.marking {
        out.u32(marking.oracle_id)?;
        match marking.policy {
            MarkPolicy::ConservingMarkBasisV1 => out.byte(1)?,
        }
    }
    if let Some(liquidation) = &config.liquidation {
        out.u32(liquidation.max_price_deviation_bps)?;
        out.u32(liquidation.max_fills_per_owner)?;
        match liquidation.policy {
            LiquidationPolicy::ConservingLiquidationV1 => out.byte(1)?,
        }
    }
    if let Some(amm) = &config.amm {
        out.u32(amm.pool_id)?;
        out.u32(amm.spread_bps)?;
        out.decimal(&amm.max_quote_quantity)?;
        out.decimal(&amm.max_abs_inventory)?;
        out.atoms(amm.max_collateral_atoms)?;
        match amm.policy {
            FundedAmmPolicy::ConservingFundedContractAmmV1 => out.byte(1)?,
        }
    }
    if let Some(binding) = &config.contract_binding {
        out.txid(binding.creation_txid)?;
        out.put(&binding.definition_hash)?;
        out.put(&binding.bootstrap_commitment)?;
        match binding.policy {
            ContractRiskPolicy::SourceContextsConservingV1 => out.byte(1)?,
        }
    }
    Ok(())
}
fn econ_read_config(
    reader: &mut Cursor<'_>,
    has_funding: bool,
    has_marking: bool,
    has_liquidation: bool,
    has_amm: bool,
    has_binding: bool,
) -> Result<EconomicConfig> {
    let network = TradeLayerMainnet::from_tag(reader.u8().map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    if reader.fixed::<32>().map_err(|_| econ_snapshot_bad())? != network.genesis().0 {
        return Err(econ_snapshot_bad());
    }
    let mut config = EconomicConfig {
        insurance: None,
        source_consensus: None,
        native_entitlements: None,
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
        funding: None,
        marking: None,
        liquidation: None,
        amm: None,
        contract_binding: None,
    };
    let features = if has_binding {
        let flags = reader.u8().map_err(|_| econ_snapshot_bad())?;
        if flags & !7 != 0 {
            return Err(econ_snapshot_bad());
        }
        Some(flags)
    } else {
        None
    };
    let has_liquidation = features.map_or(has_liquidation, |f| f & 2 != 0);
    let has_amm = features.map_or(has_amm, |f| f & 4 != 0);
    let has_funding = if has_marking {
        econ_read_bool(reader)?
    } else {
        has_funding
    };
    if features.is_some_and(|f| (f & 1 != 0) != has_funding) {
        return Err(econ_snapshot_bad());
    }
    config.funding = if has_funding {
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
    };
    config.marking = if has_marking {
        Some(MarkConfig {
            oracle_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
            policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                1 => MarkPolicy::ConservingMarkBasisV1,
                _ => return Err(econ_snapshot_bad()),
            },
        })
    } else {
        None
    };
    config.liquidation = if has_liquidation {
        Some(LiquidationConfig {
            max_price_deviation_bps: reader.u32().map_err(|_| econ_snapshot_bad())?,
            max_fills_per_owner: reader.u32().map_err(|_| econ_snapshot_bad())?,
            policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                1 => LiquidationPolicy::ConservingLiquidationV1,
                _ => return Err(econ_snapshot_bad()),
            },
        })
    } else {
        None
    };
    config.amm = if has_amm {
        Some(FundedAmmConfig {
            pool_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
            spread_bps: reader.u32().map_err(|_| econ_snapshot_bad())?,
            max_quote_quantity: econ_read_decimal(reader)?,
            max_abs_inventory: econ_read_decimal(reader)?,
            max_collateral_atoms: econ_read_atoms(reader)?,
            policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                1 => FundedAmmPolicy::ConservingFundedContractAmmV1,
                _ => return Err(econ_snapshot_bad()),
            },
        })
    } else {
        None
    };
    config.contract_binding = if has_binding {
        Some(ContractBindingConfig {
            creation_txid: econ_read_txid(reader)?,
            definition_hash: reader.fixed().map_err(|_| econ_snapshot_bad())?,
            bootstrap_commitment: reader.fixed().map_err(|_| econ_snapshot_bad())?,
            policy: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                1 => ContractRiskPolicy::SourceContextsConservingV1,
                _ => return Err(econ_snapshot_bad()),
            },
        })
    } else {
        None
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
fn econ_write_resting(out: &mut EconomicWriter, resting: &RestingOrder) -> Result<()> {
    econ_write_order(out, &resting.order)?;
    out.decimal(&resting.remaining)?;
    out.decimal(&resting.reserve_quantity)?;
    out.byte(u8::from(resting.initial_reduce))?;
    out.u64(resting.sequence)
}
fn econ_read_resting(reader: &mut Cursor<'_>) -> Result<RestingOrder> {
    Ok(RestingOrder {
        order: econ_read_order(reader)?,
        remaining: econ_read_decimal(reader)?,
        reserve_quantity: econ_read_decimal(reader)?,
        initial_reduce: econ_read_bool(reader)?,
        sequence: econ_read_u64(reader)?,
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
        econ_write_resting(out, resting)?;
    }
    out.count(snapshot.seen.len(), MAX_CONTRACT_ORDER_IDS)?;
    for id in snapshot.seen {
        out.txid(id)?;
    }
    out.u64(snapshot.next_sequence)?;
    out.height(snapshot.processed_through)
}
fn econ_read_book(reader: &mut Cursor<'_>) -> Result<ContractOrderbook> {
    econ_read_book_budgeted(reader, None)
}
fn econ_read_book_budgeted(
    reader: &mut Cursor<'_>,
    mut budget: Option<&mut LiquidationRecordBudget>,
) -> Result<ContractOrderbook> {
    let contract = econ_read_contract(reader)?;
    let mut queued = Vec::new();
    let mut drained = Vec::new();
    for orders in [&mut queued, &mut drained] {
        let count = econ_read_count(reader, MAX_CONTRACT_ORDERS)?;
        if let Some(budget) = budget.as_deref_mut() {
            budget.take(count)?;
        }
        for _ in 0..count {
            orders.push(econ_read_order(reader)?);
        }
    }
    let count = econ_read_count(reader, MAX_CONTRACT_ORDERS)?;
    if let Some(budget) = budget.as_deref_mut() {
        budget.take(count)?;
    }
    let mut resting = Vec::new();
    for _ in 0..count {
        resting.push(econ_read_resting(reader)?);
    }
    let count = econ_read_count(reader, MAX_CONTRACT_ORDER_IDS)?;
    if let Some(budget) = budget {
        budget.take(count)?;
    }
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

#[derive(Default)]
struct MarkRecordBudget {
    used: usize,
}
impl MarkRecordBudget {
    fn take(&mut self, count: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or(EconomicError::Capacity)?;
        if self.used > MAX_MARK_RECORDS {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
}

fn econ_write_ratio_integer(out: &mut EconomicWriter, value: &Decimal) -> Result<()> {
    if value.decimal_places() != 0 || value.coefficient_digits() > MAX_BASIS_RATIO_DIGITS {
        return Err(econ_snapshot_bad());
    }
    let text = value.to_string();
    if text.len() > MAX_BASIS_RATIO_DIGITS + 1 {
        return Err(EconomicError::Capacity);
    }
    out.put(&(text.len() as u16).to_be_bytes())?;
    out.put(text.as_bytes())
}
fn econ_read_ratio_integer(reader: &mut Cursor<'_>) -> Result<Decimal> {
    let length = u16::from_be_bytes(reader.fixed::<2>().map_err(|_| econ_snapshot_bad())?) as usize;
    if length == 0 || length > MAX_BASIS_RATIO_DIGITS + 1 {
        return Err(EconomicError::Capacity);
    }
    let text = core::str::from_utf8(reader.take(length).map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    let value: Decimal = text.parse().map_err(|_| econ_snapshot_bad())?;
    if value.decimal_places() != 0
        || value.coefficient_digits() > MAX_BASIS_RATIO_DIGITS
        || value.to_string() != text
    {
        return Err(econ_snapshot_bad());
    }
    Ok(value)
}
fn econ_write_ratio(out: &mut EconomicWriter, value: &ExactRatio) -> Result<()> {
    econ_write_ratio_integer(out, value.numerator())?;
    econ_write_ratio_integer(out, value.denominator())
}
fn econ_read_ratio(reader: &mut Cursor<'_>) -> Result<ExactRatio> {
    Ok(ExactRatio::from_parts(
        econ_read_ratio_integer(reader)?,
        econ_read_ratio_integer(reader)?,
    )?)
}
fn econ_write_basis_action(out: &mut EconomicWriter, action: &BasisAction) -> Result<()> {
    match action {
        BasisAction::Trade { delta, price } => {
            out.byte(0)?;
            out.decimal(delta)?;
            out.decimal(price)
        }
        BasisAction::Mark { price } => {
            out.byte(1)?;
            out.decimal(price)
        }
    }
}
fn econ_read_basis_action(reader: &mut Cursor<'_>) -> Result<BasisAction> {
    match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => Ok(BasisAction::Trade {
            delta: econ_read_decimal(reader)?,
            price: econ_read_decimal(reader)?,
        }),
        1 => Ok(BasisAction::Mark {
            price: econ_read_decimal(reader)?,
        }),
        _ => Err(econ_snapshot_bad()),
    }
}
fn econ_write_marks(
    out: &mut EconomicWriter,
    marks: &MarkState,
    allow_forced: bool,
    allow_amm: bool,
) -> Result<()> {
    let mut budget = MarkRecordBudget::default();
    budget.take(marks.bases.len())?;
    out.count(marks.bases.len(), MAX_ECONOMIC_POSITIONS)?;
    for (owner, basis) in &marks.bases {
        out.account(owner)?;
        out.decimal(&basis.quantity)?;
        econ_write_ratio(out, &basis.entry_basis)?;
        econ_write_ratio(out, &basis.carry)?;
    }
    budget.take(marks.unpaid_losses.len())?;
    out.count(marks.unpaid_losses.len(), MAX_ECONOMIC_POSITIONS)?;
    for (owner, atoms) in &marks.unpaid_losses {
        if *atoms == 0 {
            return Err(econ_snapshot_bad());
        }
        out.account(owner)?;
        out.atoms(*atoms)?;
    }
    out.count(marks.events.len(), MAX_MARK_EVENTS)?;
    let mut epochs = 0;
    for event in &marks.events {
        out.u64(event.height)?;
        match &event.kind {
            MarkEventKind::Trade {
                buy_txid,
                sell_txid,
            } => {
                out.byte(0)?;
                out.txid(*buy_txid)?;
                out.txid(*sell_txid)?;
            }
            MarkEventKind::Mark { id, observation } => {
                epochs += 1;
                if epochs > MAX_MARK_EPOCHS {
                    return Err(EconomicError::Capacity);
                }
                out.byte(1)?;
                out.txid(*id)?;
                econ_write_funding_observation(out, observation)?;
            }
            MarkEventKind::Forced {
                epoch_id,
                fill_index,
                maker_txid,
                distressed_owner,
            } => {
                if !allow_forced {
                    return Err(econ_snapshot_bad());
                }
                out.byte(2)?;
                out.txid(*epoch_id)?;
                out.u32(*fill_index)?;
                out.txid(*maker_txid)?;
                out.account(distressed_owner)?;
            }
            MarkEventKind::AmmForced {
                epoch_id,
                fill_index,
                pool_id,
                distressed_owner,
            } => {
                if !allow_amm {
                    return Err(econ_snapshot_bad());
                }
                out.byte(3)?;
                out.txid(*epoch_id)?;
                out.u32(*fill_index)?;
                out.u32(*pool_id)?;
                out.account(distressed_owner)?;
            }
        }
        budget.take(event.entries.len())?;
        out.count(event.entries.len(), MAX_ECONOMIC_POSITIONS)?;
        for entry in &event.entries {
            out.account(&entry.owner)?;
            econ_write_basis_action(out, &entry.action)?;
            if entry.atoms.negative && entry.atoms.atoms == 0 {
                return Err(econ_snapshot_bad());
            }
            out.byte(u8::from(entry.atoms.negative))?;
            out.atoms(entry.atoms.atoms)?;
            out.atoms(entry.available_atoms)?;
            out.atoms(entry.margin_atoms)?;
            out.atoms(entry.collected_atoms)?;
            out.atoms(entry.claim_offset_atoms)?;
            out.atoms(entry.loss_offset_atoms)?;
        }
        out.atoms(event.collected_loss_atoms)?;
        out.atoms(event.paid_claim_atoms)?;
        out.atoms(event.unfunded_loss_atoms)?;
    }
    budget.take(marks.maintenance.len())?;
    out.count(marks.maintenance.len(), MAX_ECONOMIC_POSITIONS)?;
    for call in &marks.maintenance {
        out.account(&call.owner)?;
        out.decimal(&call.contracts)?;
        out.atoms(call.required_atoms)?;
        out.atoms(call.margin_atoms)?;
    }
    Ok(())
}
fn econ_read_marks(
    reader: &mut Cursor<'_>,
    allow_forced: bool,
    allow_amm: bool,
) -> Result<MarkState> {
    let mut budget = MarkRecordBudget::default();
    let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
    budget.take(count)?;
    let mut bases = BTreeMap::new();
    let mut last = None;
    for _ in 0..count {
        let owner = econ_read_account(reader)?;
        if last.as_ref().is_some_and(|previous| &owner <= previous) {
            return Err(econ_snapshot_bad());
        }
        last = Some(owner.clone());
        bases.insert(
            owner,
            BasisPosition {
                quantity: econ_read_decimal(reader)?,
                entry_basis: econ_read_ratio(reader)?,
                carry: econ_read_ratio(reader)?,
            },
        );
    }
    let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
    budget.take(count)?;
    let mut unpaid_losses = BTreeMap::new();
    let mut last = None;
    for _ in 0..count {
        let owner = econ_read_account(reader)?;
        if last.as_ref().is_some_and(|previous| &owner <= previous) {
            return Err(econ_snapshot_bad());
        }
        last = Some(owner.clone());
        let atoms = econ_read_atoms(reader)?;
        if atoms == 0 {
            return Err(econ_snapshot_bad());
        }
        unpaid_losses.insert(owner, atoms);
    }
    let count = econ_read_count(reader, MAX_MARK_EVENTS)?;
    let mut events = Vec::new();
    let mut epochs = 0;
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let kind = match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => MarkEventKind::Trade {
                buy_txid: econ_read_txid(reader)?,
                sell_txid: econ_read_txid(reader)?,
            },
            1 => {
                epochs += 1;
                if epochs > MAX_MARK_EPOCHS {
                    return Err(EconomicError::Capacity);
                }
                MarkEventKind::Mark {
                    id: econ_read_txid(reader)?,
                    observation: econ_read_funding_observation(reader)?,
                }
            }
            2 if allow_forced => MarkEventKind::Forced {
                epoch_id: econ_read_txid(reader)?,
                fill_index: reader.u32().map_err(|_| econ_snapshot_bad())?,
                maker_txid: econ_read_txid(reader)?,
                distressed_owner: econ_read_account(reader)?,
            },
            3 if allow_amm => MarkEventKind::AmmForced {
                epoch_id: econ_read_txid(reader)?,
                fill_index: reader.u32().map_err(|_| econ_snapshot_bad())?,
                pool_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
                distressed_owner: econ_read_account(reader)?,
            },
            _ => return Err(econ_snapshot_bad()),
        };
        let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
        budget.take(count)?;
        let mut entries = Vec::new();
        for _ in 0..count {
            let owner = econ_read_account(reader)?;
            let action = econ_read_basis_action(reader)?;
            let atoms = SignedAtoms {
                negative: econ_read_bool(reader)?,
                atoms: econ_read_atoms(reader)?,
            };
            if atoms.negative && atoms.atoms == 0 {
                return Err(econ_snapshot_bad());
            }
            entries.push(MarkEventEntry {
                owner,
                action,
                atoms,
                available_atoms: econ_read_atoms(reader)?,
                margin_atoms: econ_read_atoms(reader)?,
                collected_atoms: econ_read_atoms(reader)?,
                claim_offset_atoms: econ_read_atoms(reader)?,
                loss_offset_atoms: econ_read_atoms(reader)?,
            });
        }
        events.push(MarkEvent {
            height,
            kind,
            entries,
            collected_loss_atoms: econ_read_atoms(reader)?,
            paid_claim_atoms: econ_read_atoms(reader)?,
            unfunded_loss_atoms: econ_read_atoms(reader)?,
        });
    }
    let count = econ_read_count(reader, MAX_ECONOMIC_POSITIONS)?;
    budget.take(count)?;
    let mut maintenance = Vec::new();
    let mut last = None;
    for _ in 0..count {
        let owner = econ_read_account(reader)?;
        if last.as_ref().is_some_and(|previous| &owner <= previous) {
            return Err(econ_snapshot_bad());
        }
        last = Some(owner.clone());
        maintenance.push(MaintenanceCall {
            owner,
            contracts: econ_read_decimal(reader)?,
            required_atoms: econ_read_atoms(reader)?,
            margin_atoms: econ_read_atoms(reader)?,
        });
    }
    Ok(MarkState {
        bases,
        unpaid_losses,
        events,
        maintenance,
    })
}

#[derive(Default)]
struct LiquidationRecordBudget {
    used: usize,
}
impl LiquidationRecordBudget {
    fn take(&mut self, count: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or(EconomicError::Capacity)?;
        if self.used > MAX_LIQUIDATION_RECORDS {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
}
fn econ_write_forced_receipt(out: &mut EconomicWriter, fill: &ForcedCloseReceipt) -> Result<()> {
    econ_write_resting(out, &fill.maker_before)?;
    for value in [
        &fill.quantity,
        &fill.price,
        &fill.distressed_pnl,
        &fill.maker_pnl,
    ] {
        out.decimal(value)?;
    }
    for value in [
        fill.maker_available_before_atoms,
        fill.maker_margin_before_atoms,
        fill.distressed_available_before_fee_atoms,
        fill.distressed_margin_before_fee_atoms,
        fill.nominal_fee_atoms,
        fill.charged_fee_atoms,
        fill.maker_rebate_atoms,
        fill.retained_fee_atoms,
        fill.collected_loss_atoms,
        fill.paid_claim_atoms,
        fill.unfunded_loss_atoms,
        fill.distressed_margin_after_atoms,
    ] {
        out.atoms(value)?;
    }
    Ok(())
}
fn econ_read_forced_receipt(reader: &mut Cursor<'_>) -> Result<ForcedCloseReceipt> {
    Ok(ForcedCloseReceipt {
        maker_before: econ_read_resting(reader)?,
        quantity: econ_read_decimal(reader)?,
        price: econ_read_decimal(reader)?,
        distressed_pnl: econ_read_decimal(reader)?,
        maker_pnl: econ_read_decimal(reader)?,
        maker_available_before_atoms: econ_read_atoms(reader)?,
        maker_margin_before_atoms: econ_read_atoms(reader)?,
        distressed_available_before_fee_atoms: econ_read_atoms(reader)?,
        distressed_margin_before_fee_atoms: econ_read_atoms(reader)?,
        nominal_fee_atoms: econ_read_atoms(reader)?,
        charged_fee_atoms: econ_read_atoms(reader)?,
        maker_rebate_atoms: econ_read_atoms(reader)?,
        retained_fee_atoms: econ_read_atoms(reader)?,
        collected_loss_atoms: econ_read_atoms(reader)?,
        paid_claim_atoms: econ_read_atoms(reader)?,
        unfunded_loss_atoms: econ_read_atoms(reader)?,
        distressed_margin_after_atoms: econ_read_atoms(reader)?,
    })
}
fn econ_write_cancel_criteria(out: &mut EconomicWriter, criteria: &CancelCriteria) -> Result<()> {
    match criteria {
        CancelCriteria::All => out.byte(0),
        CancelCriteria::Txid(id) => {
            out.byte(1)?;
            out.txid(*id)
        }
        CancelCriteria::Price { side, price } => {
            out.byte(2)?;
            out.byte(match side {
                Side::Buy => 0,
                Side::Sell => 1,
            })?;
            out.decimal(price)
        }
    }
}
fn econ_read_cancel_criteria(reader: &mut Cursor<'_>) -> Result<CancelCriteria> {
    match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => Ok(CancelCriteria::All),
        1 => Ok(CancelCriteria::Txid(econ_read_txid(reader)?)),
        2 => Ok(CancelCriteria::Price {
            side: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                0 => Side::Buy,
                1 => Side::Sell,
                _ => return Err(econ_snapshot_bad()),
            },
            price: econ_read_decimal(reader)?,
        }),
        _ => Err(econ_snapshot_bad()),
    }
}
fn econ_write_liquidation(
    out: &mut EconomicWriter,
    state: &LiquidationState,
    allow_amm: bool,
    allow_contexts: bool,
) -> Result<()> {
    let mut budget = LiquidationRecordBudget::default();
    out.count(state.events.len(), MAX_LIQUIDATION_EVENTS)?;
    for event in &state.events {
        for count in [
            event.book_before.queued.len(),
            event.book_before.drained.len(),
            event.book_before.resting.len(),
            event.book_before.seen.len(),
            event.cancelled.len(),
            event.fills.len(),
            usize::from(event.blocked_maker.is_some()),
            event.amm_fills.len(),
            event.pool_cancelled_quotes.len(),
        ] {
            budget.take(count)?;
        }
        if event
            .fills
            .len()
            .checked_add(event.amm_fills.len())
            .is_none_or(|n| n > MAX_LIQUIDATION_FILLS as usize)
        {
            return Err(EconomicError::Capacity);
        }
        if !allow_amm && (!event.amm_fills.is_empty() || !event.pool_cancelled_quotes.is_empty()) {
            return Err(econ_snapshot_bad());
        }
        out.u64(event.height)?;
        out.txid(event.id)?;
        out.txid(event.mark_id)?;
        out.account(&event.owner)?;
        out.decimal(&event.quantity_before)?;
        if allow_contexts {
            out.optional_decimal(&event.average_price_before)?;
        } else if event.average_price_before.is_some() {
            return Err(econ_snapshot_bad());
        }
        for value in [
            event.margin_before_atoms,
            event.available_before_atoms,
            event.required_before_atoms,
        ] {
            out.atoms(value)?;
        }
        out.decimal(&event.guard_price)?;
        econ_write_book(
            out,
            &ContractOrderbook::from_snapshot(event.book_before.clone())?,
        )?;
        out.count(event.cancelled.len(), MAX_CONTRACT_ORDERS)?;
        for cancellation in &event.cancelled {
            econ_write_resting(out, &cancellation.order)?;
            out.atoms(cancellation.reserve_atoms)?;
        }
        out.atoms(event.top_up_atoms)?;
        out.count(event.fills.len(), MAX_LIQUIDATION_FILLS as usize)?;
        for fill in &event.fills {
            econ_write_forced_receipt(out, fill)?;
        }
        match &event.blocked_maker {
            None => out.byte(0)?,
            Some(blocked) => {
                out.byte(1)?;
                econ_write_resting(out, &blocked.order)?;
                out.atoms(blocked.available_atoms)?;
                out.atoms(blocked.margin_atoms)?;
            }
        }
        out.byte(match event.status {
            LiquidationStatus::Cured => 0,
            LiquidationStatus::Closed => 1,
            LiquidationStatus::NoLiquidityWithinGuard => 2,
            LiquidationStatus::UnfundedBestMaker => 3,
            LiquidationStatus::FillLimit => 4,
        })?;
        if allow_amm {
            out.count(event.amm_fills.len(), MAX_LIQUIDATION_FILLS as usize)?;
            for fill in &event.amm_fills {
                econ_write_amm_forced(out, fill)?;
            }
            out.count(event.pool_cancelled_quotes.len(), MAX_FUNDED_AMM_QUOTES)?;
            for quote in &event.pool_cancelled_quotes {
                econ_write_amm_quote(out, quote)?;
            }
        }
    }
    out.count(state.book_frames.len(), MAX_BOOK_FRAMES)?;
    for frame in &state.book_frames {
        budget.take(frame.inputs.len())?;
        budget.take(frame.placements.len())?;
        out.u64(frame.height)?;
        out.count(frame.inputs.len(), MAX_ECONOMIC_INPUTS)?;
        for input in &frame.inputs {
            match input {
                LiquidationBookInput::Order(order) => {
                    out.byte(0)?;
                    econ_write_order(out, order)?;
                }
                LiquidationBookInput::Cancel {
                    txid,
                    owner,
                    criteria,
                } => {
                    out.byte(1)?;
                    out.txid(*txid)?;
                    out.account(owner)?;
                    econ_write_cancel_criteria(out, criteria)?;
                }
            }
        }
        out.count(frame.placements.len(), MAX_ECONOMIC_INPUTS)?;
        for placement in &frame.placements {
            econ_write_order(out, &placement.order)?;
            out.decimal(&placement.position)?;
        }
    }
    Ok(())
}
fn econ_read_liquidation(
    reader: &mut Cursor<'_>,
    allow_amm: bool,
    allow_contexts: bool,
) -> Result<LiquidationState> {
    let mut budget = LiquidationRecordBudget::default();
    let count = econ_read_count(reader, MAX_LIQUIDATION_EVENTS)?;
    let mut events = Vec::new();
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let id = econ_read_txid(reader)?;
        let mark_id = econ_read_txid(reader)?;
        let owner = econ_read_account(reader)?;
        let quantity_before = econ_read_decimal(reader)?;
        let average_price_before = if allow_contexts {
            econ_read_optional_decimal(reader)?
        } else {
            None
        };
        let margin_before_atoms = econ_read_atoms(reader)?;
        let available_before_atoms = econ_read_atoms(reader)?;
        let required_before_atoms = econ_read_atoms(reader)?;
        let guard_price = econ_read_decimal(reader)?;
        let book_before = econ_read_book_budgeted(reader, Some(&mut budget))?.snapshot();
        let count = econ_read_count(reader, MAX_CONTRACT_ORDERS)?;
        budget.take(count)?;
        let mut cancelled = Vec::new();
        for _ in 0..count {
            cancelled.push(LiquidationCancellation {
                order: econ_read_resting(reader)?,
                reserve_atoms: econ_read_atoms(reader)?,
            });
        }
        let top_up_atoms = econ_read_atoms(reader)?;
        let count = econ_read_count(reader, MAX_LIQUIDATION_FILLS as usize)?;
        budget.take(count)?;
        let mut fills = Vec::new();
        for _ in 0..count {
            fills.push(econ_read_forced_receipt(reader)?);
        }
        let blocked_maker = if econ_read_bool(reader)? {
            budget.take(1)?;
            Some(BlockedLiquidationMaker {
                order: econ_read_resting(reader)?,
                available_atoms: econ_read_atoms(reader)?,
                margin_atoms: econ_read_atoms(reader)?,
            })
        } else {
            None
        };
        let status = match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => LiquidationStatus::Cured,
            1 => LiquidationStatus::Closed,
            2 => LiquidationStatus::NoLiquidityWithinGuard,
            3 => LiquidationStatus::UnfundedBestMaker,
            4 => LiquidationStatus::FillLimit,
            _ => return Err(econ_snapshot_bad()),
        };
        let mut amm_fills = Vec::new();
        let mut pool_cancelled_quotes = Vec::new();
        if allow_amm {
            let count = econ_read_count(reader, MAX_LIQUIDATION_FILLS as usize)?;
            budget.take(count)?;
            if fills.len() + count > MAX_LIQUIDATION_FILLS as usize {
                return Err(EconomicError::Capacity);
            }
            for _ in 0..count {
                amm_fills.push(econ_read_amm_forced(reader)?);
            }
            let count = econ_read_count(reader, MAX_FUNDED_AMM_QUOTES)?;
            budget.take(count)?;
            for _ in 0..count {
                pool_cancelled_quotes.push(econ_read_amm_quote(reader)?);
            }
        }
        events.push(LiquidationEvent {
            height,
            id,
            mark_id,
            owner,
            quantity_before,
            average_price_before,
            margin_before_atoms,
            available_before_atoms,
            required_before_atoms,
            guard_price,
            book_before,
            cancelled,
            top_up_atoms,
            fills,
            blocked_maker,
            status,
            amm_fills,
            pool_cancelled_quotes,
        });
    }
    let count = econ_read_count(reader, MAX_BOOK_FRAMES)?;
    let mut book_frames = Vec::new();
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(count)?;
        let mut inputs = Vec::new();
        for _ in 0..count {
            inputs.push(match reader.u8().map_err(|_| econ_snapshot_bad())? {
                0 => LiquidationBookInput::Order(econ_read_order(reader)?),
                1 => LiquidationBookInput::Cancel {
                    txid: econ_read_txid(reader)?,
                    owner: econ_read_account(reader)?,
                    criteria: econ_read_cancel_criteria(reader)?,
                },
                _ => return Err(econ_snapshot_bad()),
            });
        }
        let count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(count)?;
        let mut placements = Vec::new();
        for _ in 0..count {
            placements.push(LiquidationBookPlacement {
                order: econ_read_order(reader)?,
                position: econ_read_decimal(reader)?,
            });
        }
        book_frames.push(LiquidationBookFrame {
            height,
            inputs,
            placements,
        });
    }
    Ok(LiquidationState {
        events,
        book_frames,
    })
}

#[derive(Default)]
struct AmmRecordBudget {
    used: usize,
}
impl AmmRecordBudget {
    fn take(&mut self, count: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or(EconomicError::Capacity)?;
        if self.used > MAX_AMM_RECORDS {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
}
fn econ_write_amm_quote(out: &mut EconomicWriter, q: &FundedAmmQuote) -> Result<()> {
    out.txid(q.epoch_id)?;
    out.u32(q.pool_id)?;
    out.u64(q.generation)?;
    out.u64(q.height)?;
    out.txid(q.mark_id)?;
    out.byte(match q.side {
        Side::Buy => 0,
        Side::Sell => 1,
    })?;
    out.decimal(&q.price)?;
    out.decimal(&q.remaining)?;
    out.atoms(q.reserve_atoms)
}
fn econ_read_amm_quote(reader: &mut Cursor<'_>) -> Result<FundedAmmQuote> {
    Ok(FundedAmmQuote {
        epoch_id: econ_read_txid(reader)?,
        pool_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
        generation: econ_read_u64(reader)?,
        height: econ_read_u64(reader)?,
        mark_id: econ_read_txid(reader)?,
        side: match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => Side::Buy,
            1 => Side::Sell,
            _ => return Err(econ_snapshot_bad()),
        },
        price: econ_read_decimal(reader)?,
        remaining: econ_read_decimal(reader)?,
        reserve_atoms: econ_read_atoms(reader)?,
    })
}
fn econ_write_amm_quotes(
    out: &mut EconomicWriter,
    quotes: &[FundedAmmQuote],
    budget: &mut AmmRecordBudget,
) -> Result<()> {
    budget.take(quotes.len())?;
    out.count(quotes.len(), MAX_FUNDED_AMM_QUOTES)?;
    for q in quotes {
        econ_write_amm_quote(out, q)?;
    }
    Ok(())
}
fn econ_read_amm_quotes(
    reader: &mut Cursor<'_>,
    budget: &mut AmmRecordBudget,
) -> Result<Vec<FundedAmmQuote>> {
    let count = econ_read_count(reader, MAX_FUNDED_AMM_QUOTES)?;
    budget.take(count)?;
    let mut quotes = Vec::new();
    for _ in 0..count {
        quotes.push(econ_read_amm_quote(reader)?);
    }
    Ok(quotes)
}
fn econ_write_amm_input(out: &mut EconomicWriter, input: &AmmCapitalInput) -> Result<()> {
    let (tag, txid, owner, amount, min) = match input {
        AmmCapitalInput::Deposit {
            txid,
            owner,
            collateral_atoms,
            min_share_units,
        } => (0, *txid, owner, *collateral_atoms, *min_share_units),
        AmmCapitalInput::Redeem {
            txid,
            owner,
            share_units,
            min_collateral_atoms,
        } => (1, *txid, owner, *share_units, *min_collateral_atoms),
    };
    out.byte(tag)?;
    out.txid(txid)?;
    out.account(owner)?;
    out.atoms(amount)?;
    out.atoms(min)
}
fn econ_read_amm_input(reader: &mut Cursor<'_>) -> Result<AmmCapitalInput> {
    let tag = reader.u8().map_err(|_| econ_snapshot_bad())?;
    let txid = econ_read_txid(reader)?;
    let owner = econ_read_account(reader)?;
    let amount = econ_read_atoms(reader)?;
    let min = econ_read_atoms(reader)?;
    match tag {
        0 => Ok(AmmCapitalInput::Deposit {
            txid,
            owner,
            collateral_atoms: amount,
            min_share_units: min,
        }),
        1 => Ok(AmmCapitalInput::Redeem {
            txid,
            owner,
            share_units: amount,
            min_collateral_atoms: min,
        }),
        _ => Err(econ_snapshot_bad()),
    }
}
fn econ_write_amm_receipt(out: &mut EconomicWriter, r: &AmmCapitalReceipt) -> Result<()> {
    out.txid(r.txid)?;
    out.account(&r.owner)?;
    out.byte(match r.kind {
        AmmCapitalKind::Deposit => 0,
        AmmCapitalKind::Redeem => 1,
    })?;
    for value in [
        r.collateral_atoms,
        r.share_units,
        r.cash_before_atoms,
        r.total_units_before,
        r.owner_units_before,
    ] {
        out.atoms(value)?;
    }
    Ok(())
}
fn econ_read_amm_receipt(reader: &mut Cursor<'_>) -> Result<AmmCapitalReceipt> {
    Ok(AmmCapitalReceipt {
        txid: econ_read_txid(reader)?,
        owner: econ_read_account(reader)?,
        kind: match reader.u8().map_err(|_| econ_snapshot_bad())? {
            0 => AmmCapitalKind::Deposit,
            1 => AmmCapitalKind::Redeem,
            _ => return Err(econ_snapshot_bad()),
        },
        collateral_atoms: econ_read_atoms(reader)?,
        share_units: econ_read_atoms(reader)?,
        cash_before_atoms: econ_read_atoms(reader)?,
        total_units_before: econ_read_atoms(reader)?,
        owner_units_before: econ_read_atoms(reader)?,
    })
}
fn econ_write_amm_epoch(
    out: &mut EconomicWriter,
    e: &AmmQuoteEpoch,
    budget: &mut AmmRecordBudget,
    allow_contexts: bool,
) -> Result<()> {
    budget.take(1)?;
    out.txid(e.id)?;
    out.u64(e.height)?;
    out.txid(e.mark_id)?;
    out.u64(e.generation)?;
    out.decimal(&e.position_before)?;
    if allow_contexts {
        out.optional_decimal(&e.average_price_before)?;
    } else if e.average_price_before.is_some() {
        return Err(econ_snapshot_bad());
    }
    for value in [
        e.cash_before_atoms,
        e.margin_before_atoms,
        e.unpaid_loss_before_atoms,
    ] {
        out.atoms(value)?;
    }
    out.u32(e.cash_leg_count_before)?;
    econ_write_amm_quotes(out, &e.quotes, budget)
}
fn econ_read_amm_epoch(
    reader: &mut Cursor<'_>,
    budget: &mut AmmRecordBudget,
    allow_contexts: bool,
) -> Result<AmmQuoteEpoch> {
    budget.take(1)?;
    Ok(AmmQuoteEpoch {
        id: econ_read_txid(reader)?,
        height: econ_read_u64(reader)?,
        mark_id: econ_read_txid(reader)?,
        generation: econ_read_u64(reader)?,
        position_before: econ_read_decimal(reader)?,
        average_price_before: if allow_contexts {
            econ_read_optional_decimal(reader)?
        } else {
            None
        },
        cash_before_atoms: econ_read_atoms(reader)?,
        margin_before_atoms: econ_read_atoms(reader)?,
        unpaid_loss_before_atoms: econ_read_atoms(reader)?,
        cash_leg_count_before: reader.u32().map_err(|_| econ_snapshot_bad())?,
        quotes: econ_read_amm_quotes(reader, budget)?,
    })
}
fn econ_write_amm(
    out: &mut EconomicWriter,
    amm: &FundedAmmState,
    allow_contexts: bool,
) -> Result<()> {
    let mut budget = AmmRecordBudget::default();
    out.atoms(amm.total_units)?;
    budget.take(amm.shares.len())?;
    out.count(amm.shares.len(), MAX_AMM_LPS)?;
    for (owner, units) in &amm.shares {
        if *units == 0 {
            return Err(econ_snapshot_bad());
        }
        out.account(owner)?;
        out.atoms(*units)?;
    }
    out.atoms(amm.cash_atoms)?;
    out.u64(amm.generation)?;
    econ_write_amm_quotes(out, &amm.quotes, &mut budget)?;
    out.count(amm.frames.len(), MAX_AMM_FRAMES)?;
    for frame in &amm.frames {
        out.u64(frame.height)?;
        econ_write_amm_quotes(out, &frame.released_quotes, &mut budget)?;
        budget.take(frame.capital_inputs.len())?;
        out.count(frame.capital_inputs.len(), MAX_ECONOMIC_INPUTS)?;
        for input in &frame.capital_inputs {
            econ_write_amm_input(out, input)?;
        }
        budget.take(frame.capital_receipts.len())?;
        out.count(frame.capital_receipts.len(), MAX_ECONOMIC_INPUTS)?;
        for receipt in &frame.capital_receipts {
            econ_write_amm_receipt(out, receipt)?;
        }
        match &frame.quote_epoch {
            None => out.byte(0)?,
            Some(epoch) => {
                out.byte(1)?;
                econ_write_amm_epoch(out, epoch, &mut budget, allow_contexts)?;
            }
        }
        budget.take(frame.cash_legs.len())?;
        out.count(frame.cash_legs.len(), MAX_AMM_RECORDS)?;
        for leg in &frame.cash_legs {
            out.u64(leg.sequence)?;
            out.byte(u8::from(leg.from_pool))?;
            out.account(&leg.counterparty)?;
            out.atoms(leg.atoms)?;
        }
    }
    Ok(())
}
fn econ_read_amm(reader: &mut Cursor<'_>, allow_contexts: bool) -> Result<FundedAmmState> {
    let mut budget = AmmRecordBudget::default();
    let total_units = econ_read_atoms(reader)?;
    let count = econ_read_count(reader, MAX_AMM_LPS)?;
    budget.take(count)?;
    let mut shares = BTreeMap::new();
    let mut last = None;
    for _ in 0..count {
        let owner = econ_read_account(reader)?;
        if last.as_ref().is_some_and(|p| &owner <= p) {
            return Err(econ_snapshot_bad());
        }
        last = Some(owner.clone());
        let units = econ_read_atoms(reader)?;
        if units == 0 {
            return Err(econ_snapshot_bad());
        }
        shares.insert(owner, units);
    }
    let cash_atoms = econ_read_atoms(reader)?;
    let generation = econ_read_u64(reader)?;
    let quotes = econ_read_amm_quotes(reader, &mut budget)?;
    let count = econ_read_count(reader, MAX_AMM_FRAMES)?;
    let mut frames = Vec::new();
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let released_quotes = econ_read_amm_quotes(reader, &mut budget)?;
        let count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(count)?;
        let mut capital_inputs = Vec::new();
        for _ in 0..count {
            capital_inputs.push(econ_read_amm_input(reader)?);
        }
        let count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(count)?;
        let mut capital_receipts = Vec::new();
        for _ in 0..count {
            capital_receipts.push(econ_read_amm_receipt(reader)?);
        }
        let quote_epoch = if econ_read_bool(reader)? {
            Some(econ_read_amm_epoch(reader, &mut budget, allow_contexts)?)
        } else {
            None
        };
        let count = econ_read_count(reader, MAX_AMM_RECORDS)?;
        budget.take(count)?;
        let mut cash_legs = Vec::new();
        for _ in 0..count {
            cash_legs.push(AmmCashLeg {
                sequence: econ_read_u64(reader)?,
                from_pool: econ_read_bool(reader)?,
                counterparty: econ_read_account(reader)?,
                atoms: econ_read_atoms(reader)?,
            });
        }
        frames.push(AmmFrame {
            height,
            released_quotes,
            capital_inputs,
            capital_receipts,
            quote_epoch,
            cash_legs,
        });
    }
    Ok(FundedAmmState {
        total_units,
        shares,
        cash_atoms,
        generation,
        quotes,
        frames,
    })
}
fn econ_write_amm_forced(out: &mut EconomicWriter, f: &AmmForcedCloseReceipt) -> Result<()> {
    econ_write_amm_quote(out, &f.quote_before)?;
    for value in [&f.quantity, &f.price, &f.distressed_pnl, &f.maker_pnl] {
        out.decimal(value)?;
    }
    for value in [
        f.maker_available_before_atoms,
        f.maker_margin_before_atoms,
        f.distressed_available_before_fee_atoms,
        f.distressed_margin_before_fee_atoms,
        f.nominal_fee_atoms,
        f.charged_fee_atoms,
        f.maker_rebate_atoms,
        f.retained_fee_atoms,
        f.collected_loss_atoms,
        f.paid_claim_atoms,
        f.unfunded_loss_atoms,
        f.distressed_margin_after_atoms,
    ] {
        out.atoms(value)?;
    }
    Ok(())
}
fn econ_read_amm_forced(reader: &mut Cursor<'_>) -> Result<AmmForcedCloseReceipt> {
    Ok(AmmForcedCloseReceipt {
        quote_before: econ_read_amm_quote(reader)?,
        quantity: econ_read_decimal(reader)?,
        price: econ_read_decimal(reader)?,
        distressed_pnl: econ_read_decimal(reader)?,
        maker_pnl: econ_read_decimal(reader)?,
        maker_available_before_atoms: econ_read_atoms(reader)?,
        maker_margin_before_atoms: econ_read_atoms(reader)?,
        distressed_available_before_fee_atoms: econ_read_atoms(reader)?,
        distressed_margin_before_fee_atoms: econ_read_atoms(reader)?,
        nominal_fee_atoms: econ_read_atoms(reader)?,
        charged_fee_atoms: econ_read_atoms(reader)?,
        maker_rebate_atoms: econ_read_atoms(reader)?,
        retained_fee_atoms: econ_read_atoms(reader)?,
        collected_loss_atoms: econ_read_atoms(reader)?,
        paid_claim_atoms: econ_read_atoms(reader)?,
        unfunded_loss_atoms: econ_read_atoms(reader)?,
        distressed_margin_after_atoms: econ_read_atoms(reader)?,
    })
}

#[derive(Default)]
struct ContractRecordBudget {
    used: usize,
}
impl ContractRecordBudget {
    fn take(&mut self, count: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or(EconomicError::Capacity)?;
        if self.used > MAX_CONTRACT_RECORDS {
            return Err(EconomicError::Capacity);
        }
        Ok(())
    }
}
fn econ_write_text(out: &mut EconomicWriter, text: &str, max: usize) -> Result<()> {
    if text.len() > max {
        return Err(EconomicError::Capacity);
    }
    let len = u16::try_from(text.len()).map_err(|_| EconomicError::Capacity)?;
    out.put(&len.to_be_bytes())?;
    out.put(text.as_bytes())
}
fn econ_read_text(reader: &mut Cursor<'_>, max: usize) -> Result<String> {
    let count = u16::from_be_bytes(reader.fixed().map_err(|_| econ_snapshot_bad())?) as usize;
    if count > max {
        return Err(EconomicError::Capacity);
    }
    let text = core::str::from_utf8(reader.take(count).map_err(|_| econ_snapshot_bad())?)
        .map_err(|_| econ_snapshot_bad())?;
    Ok(text.to_owned())
}
fn econ_write_optional_u32(out: &mut EconomicWriter, value: Option<u32>) -> Result<()> {
    match value {
        None => out.byte(0),
        Some(value) => {
            out.byte(1)?;
            out.u32(value)
        }
    }
}
fn econ_read_optional_u32(reader: &mut Cursor<'_>) -> Result<Option<u32>> {
    if econ_read_bool(reader)? {
        Ok(Some(reader.u32().map_err(|_| econ_snapshot_bad())?))
    } else {
        Ok(None)
    }
}
fn econ_write_optional_atoms(out: &mut EconomicWriter, value: Option<u128>) -> Result<()> {
    match value {
        None => out.byte(0),
        Some(value) => {
            out.byte(1)?;
            out.atoms(value)
        }
    }
}
fn econ_read_optional_atoms(reader: &mut Cursor<'_>) -> Result<Option<u128>> {
    if econ_read_bool(reader)? {
        Ok(Some(econ_read_atoms(reader)?))
    } else {
        Ok(None)
    }
}
fn econ_write_contract_parameters(
    out: &mut EconomicWriter,
    p: &ContractParameters,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    out.byte(u8::from(p.native))?;
    econ_write_optional_u32(out, p.underlying_oracle_id)?;
    budget.take(p.on_chain_data.len())?;
    out.count(
        p.on_chain_data.len(),
        crate::contract_registry::MAX_CONTRACT_PAIRS,
    )?;
    for (first, second) in &p.on_chain_data {
        out.u32(first.get())?;
        out.u32(second.get())?;
    }
    out.u32(p.notional_property_id.get())?;
    out.decimal(&p.notional_value)?;
    out.u32(p.collateral_property_id.get())?;
    out.decimal(&p.leverage)?;
    out.u64(p.expiry_period)?;
    out.u32(p.series)?;
    out.byte(u8::from(p.inverse))?;
    out.decimal(&p.fee)?;
    econ_write_optional_u32(out, p.whitelist)?;
    match &p.ticker {
        None => out.byte(0),
        Some(ticker) => {
            out.byte(1)?;
            econ_write_text(
                out,
                ticker,
                crate::contract_registry::MAX_CONTRACT_TICKER_BYTES,
            )
        }
    }
}
fn econ_read_contract_parameters(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<ContractParameters> {
    let native = econ_read_bool(reader)?;
    let underlying_oracle_id = econ_read_optional_u32(reader)?;
    let count = econ_read_count(reader, crate::contract_registry::MAX_CONTRACT_PAIRS)?;
    budget.take(count)?;
    let mut on_chain_data = Vec::new();
    for _ in 0..count {
        on_chain_data.push((
            ContractAsset::from(reader.u32().map_err(|_| econ_snapshot_bad())?),
            ContractAsset::from(reader.u32().map_err(|_| econ_snapshot_bad())?),
        ));
    }
    Ok(ContractParameters {
        native,
        underlying_oracle_id,
        on_chain_data,
        notional_property_id: ContractAsset::from(reader.u32().map_err(|_| econ_snapshot_bad())?),
        notional_value: econ_read_decimal(reader)?,
        collateral_property_id: ContractAsset::from(reader.u32().map_err(|_| econ_snapshot_bad())?),
        leverage: econ_read_decimal(reader)?,
        expiry_period: econ_read_u64(reader)?,
        series: reader.u32().map_err(|_| econ_snapshot_bad())?,
        inverse: econ_read_bool(reader)?,
        fee: econ_read_decimal(reader)?,
        whitelist: econ_read_optional_u32(reader)?,
        ticker: if econ_read_bool(reader)? {
            Some(econ_read_text(
                reader,
                crate::contract_registry::MAX_CONTRACT_TICKER_BYTES,
            )?)
        } else {
            None
        },
    })
}
fn econ_write_contract_creation(
    out: &mut EconomicWriter,
    input: &CreateContractInput,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    out.txid(input.txid)?;
    out.account(&input.issuer)?;
    econ_write_contract_parameters(out, &input.parameters, budget)
}
fn econ_read_contract_creation(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<CreateContractInput> {
    Ok(CreateContractInput {
        txid: econ_read_txid(reader)?,
        issuer: econ_read_account(reader)?,
        parameters: econ_read_contract_parameters(reader, budget)?,
    })
}
/// Canonical input identity for the independently expected contract definition.
/// Domain separation and the hash are applied by the binding owner.
fn econ_contract_creation_bytes(input: &CreateContractInput) -> Result<Vec<u8>> {
    let mut out = EconomicWriter(Vec::new());
    econ_write_contract_creation(&mut out, input, &mut ContractRecordBudget::default())?;
    Ok(out.0)
}
fn econ_write_contract_series(
    out: &mut EconomicWriter,
    series: &ContractSeries,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    out.u32(series.id.get())?;
    out.account(&series.issuer)?;
    econ_write_text(
        out,
        &series.ticker,
        crate::contract_registry::MAX_CONTRACT_TICKER_BYTES,
    )?;
    econ_write_contract_parameters(out, &series.parameters, budget)?;
    if series
        .expired
        .len()
        .checked_add(series.unexpired.len())
        .is_none_or(|n| n > crate::contract_registry::MAX_EXPIRIES_PER_SERIES)
    {
        return Err(EconomicError::Capacity);
    }
    for contracts in [&series.expired, &series.unexpired] {
        budget.take(contracts.len())?;
        out.count(
            contracts.len(),
            crate::contract_registry::MAX_EXPIRIES_PER_SERIES,
        )?;
        for contract in contracts {
            econ_write_text(
                out,
                &contract.id,
                crate::contract_registry::MAX_CONTRACT_TICKER_BYTES,
            )?;
            out.u64(contract.expiration_block)?;
        }
    }
    out.decimal(&series.amm.position)?;
    out.decimal(&series.amm.max_position)?;
    out.decimal(&series.amm.max_quote_size)?;
    out.u32(series.amm.contract_type.get())
}
fn econ_read_contract_series(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<ContractSeries> {
    let id = econ_read_contract(reader)?;
    let issuer = econ_read_account(reader)?;
    let ticker = econ_read_text(reader, crate::contract_registry::MAX_CONTRACT_TICKER_BYTES)?;
    let parameters = econ_read_contract_parameters(reader, budget)?;
    let mut generated = [Vec::new(), Vec::new()];
    let mut total = 0usize;
    for contracts in &mut generated {
        let count = econ_read_count(reader, crate::contract_registry::MAX_EXPIRIES_PER_SERIES)?;
        total = total.checked_add(count).ok_or(EconomicError::Capacity)?;
        if total > crate::contract_registry::MAX_EXPIRIES_PER_SERIES {
            return Err(EconomicError::Capacity);
        }
        budget.take(count)?;
        for _ in 0..count {
            contracts.push(crate::contract_registry::GeneratedContract {
                id: econ_read_text(reader, crate::contract_registry::MAX_CONTRACT_TICKER_BYTES)?,
                expiration_block: econ_read_u64(reader)?,
            });
        }
    }
    let [expired, unexpired] = generated;
    Ok(ContractSeries {
        id,
        issuer,
        ticker,
        parameters,
        expired,
        unexpired,
        amm: crate::contract_registry::AmmConfiguration {
            position: econ_read_decimal(reader)?,
            max_position: econ_read_decimal(reader)?,
            max_quote_size: econ_read_decimal(reader)?,
            contract_type: econ_read_contract(reader)?,
        },
    })
}
fn econ_write_registry_action(
    out: &mut EconomicWriter,
    action: &RegistryAction,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    use crate::registry::{MAX_METADATA_BYTES, MAX_NAME_BYTES, MAX_URL_BYTES};
    match action {
        RegistryAction::Activate { types, code_hash } => {
            out.byte(0)?;
            budget.take(types.len())?;
            out.count(types.len(), 36)?;
            for ty in types {
                out.byte(*ty)?;
            }
            out.put(code_hash)
        }
        RegistryAction::CreateWhitelist {
            name,
            url,
            description,
            backup,
        } => {
            out.byte(1)?;
            econ_write_text(out, name, MAX_NAME_BYTES)?;
            econ_write_text(out, url, MAX_URL_BYTES)?;
            econ_write_text(out, description, MAX_METADATA_BYTES)?;
            out.account(backup)
        }
        RegistryAction::UpdateAdmin {
            target,
            new_address,
            update_backup,
        } => {
            out.byte(2)?;
            let (tag, id) = match target {
                AdminTarget::Whitelist(id) => (0, *id),
                AdminTarget::Oracle(id) => (1, *id),
                AdminTarget::Token(id) => (2, *id),
            };
            out.byte(tag)?;
            out.u32(id)?;
            out.account(new_address)?;
            out.byte(u8::from(*update_backup))
        }
        RegistryAction::Attest {
            list_id,
            subject,
            metadata,
        } => {
            out.byte(3)?;
            out.u32(*list_id)?;
            out.account(subject)?;
            econ_write_text(out, metadata, MAX_METADATA_BYTES)
        }
        RegistryAction::RevokeAttestation { list_id, subject } => {
            out.byte(4)?;
            out.u32(*list_id)?;
            out.account(subject)
        }
        RegistryAction::CreateOracle {
            ticker,
            url,
            backup,
            lag,
        } => {
            out.byte(5)?;
            econ_write_text(out, ticker, MAX_NAME_BYTES)?;
            econ_write_text(out, url, MAX_URL_BYTES)?;
            out.account(backup)?;
            out.u32(*lag)
        }
        RegistryAction::PublishOracle {
            oracle_id,
            price_1e4,
            high_1e4,
            low_1e4,
            close_1e4,
        } => {
            out.byte(6)?;
            out.u32(*oracle_id)?;
            out.atoms(*price_1e4)?;
            for value in [high_1e4, low_1e4, close_1e4] {
                econ_write_optional_atoms(out, *value)?;
            }
            Ok(())
        }
        RegistryAction::CloseOracle { oracle_id } => {
            out.byte(7)?;
            out.u32(*oracle_id)
        }
    }
}
fn econ_read_registry_action(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<RegistryAction> {
    use crate::registry::{MAX_METADATA_BYTES, MAX_NAME_BYTES, MAX_URL_BYTES};
    Ok(match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => {
            let count = econ_read_count(reader, 36)?;
            budget.take(count)?;
            let mut types = Vec::new();
            for _ in 0..count {
                types.push(reader.u8().map_err(|_| econ_snapshot_bad())?);
            }
            RegistryAction::Activate {
                types,
                code_hash: reader.fixed().map_err(|_| econ_snapshot_bad())?,
            }
        }
        1 => RegistryAction::CreateWhitelist {
            name: econ_read_text(reader, MAX_NAME_BYTES)?,
            url: econ_read_text(reader, MAX_URL_BYTES)?,
            description: econ_read_text(reader, MAX_METADATA_BYTES)?,
            backup: econ_read_account(reader)?,
        },
        2 => {
            let tag = reader.u8().map_err(|_| econ_snapshot_bad())?;
            let id = reader.u32().map_err(|_| econ_snapshot_bad())?;
            let target = match tag {
                0 => AdminTarget::Whitelist(id),
                1 => AdminTarget::Oracle(id),
                2 => AdminTarget::Token(id),
                _ => return Err(econ_snapshot_bad()),
            };
            RegistryAction::UpdateAdmin {
                target,
                new_address: econ_read_account(reader)?,
                update_backup: econ_read_bool(reader)?,
            }
        }
        3 => RegistryAction::Attest {
            list_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
            subject: econ_read_account(reader)?,
            metadata: econ_read_text(reader, MAX_METADATA_BYTES)?,
        },
        4 => RegistryAction::RevokeAttestation {
            list_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
            subject: econ_read_account(reader)?,
        },
        5 => RegistryAction::CreateOracle {
            ticker: econ_read_text(reader, MAX_NAME_BYTES)?,
            url: econ_read_text(reader, MAX_URL_BYTES)?,
            backup: econ_read_account(reader)?,
            lag: reader.u32().map_err(|_| econ_snapshot_bad())?,
        },
        6 => RegistryAction::PublishOracle {
            oracle_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
            price_1e4: econ_read_atoms(reader)?,
            high_1e4: econ_read_optional_atoms(reader)?,
            low_1e4: econ_read_optional_atoms(reader)?,
            close_1e4: econ_read_optional_atoms(reader)?,
        },
        7 => RegistryAction::CloseOracle {
            oracle_id: reader.u32().map_err(|_| econ_snapshot_bad())?,
        },
        _ => return Err(econ_snapshot_bad()),
    })
}
fn econ_write_joint_input(
    out: &mut EconomicWriter,
    tx: &UnverifiedJointTx,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    out.txid(tx.txid)?;
    out.account(&tx.actor)?;
    match &tx.operation {
        JointOperation::Registry(action) => {
            out.byte(0)?;
            econ_write_registry_action(out, action, budget)
        }
        JointOperation::Issue {
            id,
            kind,
            initial_atoms,
        } => {
            out.byte(1)?;
            out.u32(id.get())?;
            out.byte(match kind {
                crate::ledger::PropertyKind::Fixed => 0,
                crate::ledger::PropertyKind::Managed => 1,
            })?;
            out.atoms(*initial_atoms)
        }
        JointOperation::Send {
            id,
            recipient,
            amount_atoms,
        }
        | JointOperation::Grant {
            id,
            recipient,
            amount_atoms,
        } => {
            out.byte(if matches!(tx.operation, JointOperation::Send { .. }) {
                2
            } else {
                3
            })?;
            out.u32(id.get())?;
            out.account(recipient)?;
            out.atoms(*amount_atoms)
        }
        JointOperation::Redeem { id, amount_atoms } => {
            out.byte(4)?;
            out.u32(id.get())?;
            out.atoms(*amount_atoms)
        }
        JointOperation::SpotPlace(intent) => {
            out.byte(5)?;
            out.u32(intent.offered.get())?;
            out.u32(intent.desired.get())?;
            out.u64(intent.offered_atoms)?;
            out.u64(intent.expected_atoms)?;
            out.byte(u8::from(intent.post))?;
            out.byte(u8::from(intent.stop))
        }
        JointOperation::SpotCancel {
            offered,
            desired,
            is_contract,
            selector,
        } => {
            out.byte(6)?;
            out.u32(offered.get())?;
            out.u32(desired.get())?;
            out.byte(u8::from(*is_contract))?;
            match selector {
                CancelSelector::All => out.byte(0),
                CancelSelector::ByOrderTxId(id) => {
                    out.byte(1)?;
                    out.txid(*id)
                }
            }
        }
        JointOperation::UtxoTradeType3 => out.byte(7),
        JointOperation::EconomicSettlement { ty } => {
            out.byte(8)?;
            out.byte(*ty)
        }
        JointOperation::Unsupported { ty } => {
            out.byte(9)?;
            out.byte(*ty)
        }
    }
}
fn econ_read_joint_input(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<UnverifiedJointTx> {
    let txid = econ_read_txid(reader)?;
    let actor = econ_read_account(reader)?;
    let operation = match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => JointOperation::Registry(econ_read_registry_action(reader, budget)?),
        1 => JointOperation::Issue {
            id: econ_read_property(reader)?,
            kind: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                0 => crate::ledger::PropertyKind::Fixed,
                1 => crate::ledger::PropertyKind::Managed,
                _ => return Err(econ_snapshot_bad()),
            },
            initial_atoms: econ_read_atoms(reader)?,
        },
        tag @ (2 | 3) => {
            let id = econ_read_property(reader)?;
            let recipient = econ_read_account(reader)?;
            let amount_atoms = econ_read_atoms(reader)?;
            if tag == 2 {
                JointOperation::Send {
                    id,
                    recipient,
                    amount_atoms,
                }
            } else {
                JointOperation::Grant {
                    id,
                    recipient,
                    amount_atoms,
                }
            }
        }
        4 => JointOperation::Redeem {
            id: econ_read_property(reader)?,
            amount_atoms: econ_read_atoms(reader)?,
        },
        5 => JointOperation::SpotPlace(SpotIntent {
            offered: econ_read_property(reader)?,
            desired: econ_read_property(reader)?,
            offered_atoms: econ_read_u64(reader)?,
            expected_atoms: econ_read_u64(reader)?,
            post: econ_read_bool(reader)?,
            stop: econ_read_bool(reader)?,
        }),
        6 => JointOperation::SpotCancel {
            offered: econ_read_property(reader)?,
            desired: econ_read_property(reader)?,
            is_contract: econ_read_bool(reader)?,
            selector: match reader.u8().map_err(|_| econ_snapshot_bad())? {
                0 => CancelSelector::All,
                1 => CancelSelector::ByOrderTxId(econ_read_txid(reader)?),
                _ => return Err(econ_snapshot_bad()),
            },
        },
        7 => JointOperation::UtxoTradeType3,
        8 => JointOperation::EconomicSettlement {
            ty: reader.u8().map_err(|_| econ_snapshot_bad())?,
        },
        9 => JointOperation::Unsupported {
            ty: reader.u8().map_err(|_| econ_snapshot_bad())?,
        },
        _ => return Err(econ_snapshot_bad()),
    };
    Ok(UnverifiedJointTx {
        txid,
        actor,
        operation,
    })
}
fn econ_write_input(
    out: &mut EconomicWriter,
    input: &EconomicInput,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    match input {
        EconomicInput::NativeTarget(request) => {
            out.byte(5)?;
            out.txid(request.txid)?;
            out.account(&request.claimed_actor)?;
            out.u32(request.coupon.get())?;
            out.atoms(request.requested_target_atoms)
        }
        EconomicInput::Insurance(input) => {
            out.byte(6)?;
            insurance_write_input(out, input)
        }
        EconomicInput::Joint(tx) => {
            out.byte(0)?;
            econ_write_joint_input(out, tx, budget)
        }
        EconomicInput::Order(order) => {
            out.byte(1)?;
            econ_write_order(out, order)
        }
        EconomicInput::Cancel {
            txid,
            owner,
            criteria,
        } => {
            out.byte(2)?;
            out.txid(*txid)?;
            out.account(owner)?;
            econ_write_cancel_criteria(out, criteria)
        }
        EconomicInput::Amm(input) => {
            out.byte(3)?;
            econ_write_amm_input(out, input)
        }
        EconomicInput::CreateContract(input) => {
            out.byte(4)?;
            econ_write_contract_creation(out, input, budget)
        }
    }
}
fn econ_read_input(
    reader: &mut Cursor<'_>,
    budget: &mut ContractRecordBudget,
) -> Result<EconomicInput> {
    Ok(match reader.u8().map_err(|_| econ_snapshot_bad())? {
        0 => EconomicInput::Joint(econ_read_joint_input(reader, budget)?),
        1 => EconomicInput::Order(econ_read_order(reader)?),
        2 => EconomicInput::Cancel {
            txid: econ_read_txid(reader)?,
            owner: econ_read_account(reader)?,
            criteria: econ_read_cancel_criteria(reader)?,
        },
        3 => EconomicInput::Amm(econ_read_amm_input(reader)?),
        4 => EconomicInput::CreateContract(econ_read_contract_creation(reader, budget)?),
        5 => EconomicInput::NativeTarget(crate::native_entitlement::UnverifiedNativeTarget {
            txid: econ_read_txid(reader)?,
            claimed_actor: econ_read_account(reader)?,
            coupon: econ_read_property(reader)?,
            requested_target_atoms: econ_read_atoms(reader)?,
        }),
        6 => EconomicInput::Insurance(insurance_read_input(reader)?),
        _ => return Err(econ_snapshot_bad()),
    })
}
fn econ_write_contract_state(out: &mut EconomicWriter, state: &ContractBindingState) -> Result<()> {
    econ_write_contract_state_budget(out, state, &mut ContractRecordBudget::default())
}
fn econ_write_contract_state_budget(
    out: &mut EconomicWriter,
    state: &ContractBindingState,
    budget: &mut ContractRecordBudget,
) -> Result<()> {
    if state.bootstrap.is_empty() {
        return Err(econ_snapshot_bad());
    }
    out.embedded(&state.bootstrap)?;
    budget.take(state.contracts.len())?;
    out.count(state.contracts.len(), MAX_BOUND_CONTRACTS)?;
    for contract in &state.contracts {
        out.txid(contract.creation_txid)?;
        out.u64(contract.height)?;
        out.put(&contract.definition_hash)?;
        econ_write_contract_series(out, &contract.definition, budget)?;
    }
    budget.take(state.frames.len())?;
    out.count(state.frames.len(), MAX_CONTRACT_FRAMES)?;
    for frame in &state.frames {
        out.u64(frame.height)?;
        budget.take(frame.inputs.len())?;
        out.count(frame.inputs.len(), MAX_ECONOMIC_INPUTS)?;
        for input in &frame.inputs {
            econ_write_input(out, input, budget)?;
        }
    }
    Ok(())
}
fn econ_read_contract_state(reader: &mut Cursor<'_>) -> Result<ContractBindingState> {
    let mut budget = ContractRecordBudget::default();
    let bootstrap = econ_read_embedded(reader)?.to_vec();
    let count = econ_read_count(reader, MAX_BOUND_CONTRACTS)?;
    budget.take(count)?;
    let mut contracts = Vec::new();
    for _ in 0..count {
        contracts.push(AcceptedContract {
            creation_txid: econ_read_txid(reader)?,
            height: econ_read_u64(reader)?,
            definition_hash: reader.fixed().map_err(|_| econ_snapshot_bad())?,
            definition: econ_read_contract_series(reader, &mut budget)?,
        });
    }
    let count = econ_read_count(reader, MAX_CONTRACT_FRAMES)?;
    budget.take(count)?;
    let mut frames = Vec::new();
    for _ in 0..count {
        let height = econ_read_u64(reader)?;
        let count = econ_read_count(reader, MAX_ECONOMIC_INPUTS)?;
        budget.take(count)?;
        let mut inputs = Vec::new();
        for _ in 0..count {
            inputs.push(econ_read_input(reader, &mut budget)?);
        }
        frames.push(ContractInputFrame { height, inputs });
    }
    Ok(ContractBindingState {
        bootstrap,
        contracts,
        frames,
    })
}

impl EconomicState {
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>> {
        if self.config.insurance.is_some() {
            return self.insurance_snapshot_bytes();
        }
        if self.config.native_entitlements.is_some() {
            return self.native_snapshot_bytes();
        }
        if self.config.source_consensus.is_some() {
            return self.source_snapshot_bytes();
        }
        self.verify()?;
        let has_binding = self.config.contract_binding.is_some();
        let mut out = EconomicWriter(if has_binding {
            ECONOMIC_CONTRACT_MAGIC.to_vec()
        } else if self.config.amm.is_some() {
            ECONOMIC_AMM_MAGIC.to_vec()
        } else if self.config.liquidation.is_some() {
            ECONOMIC_LIQUIDATION_MAGIC.to_vec()
        } else if self.config.marking.is_some() {
            ECONOMIC_MARK_MAGIC.to_vec()
        } else if self.config.funding.is_some() {
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
            if has_binding {
                out.optional_decimal(&reserve.margin_per_contract)?;
            } else if reserve.margin_per_contract.is_some() {
                return Err(econ_snapshot_bad());
            }
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
        if self.config.marking.is_some() {
            econ_write_marks(
                &mut out,
                &self.marking,
                self.config.liquidation.is_some(),
                self.config.amm.is_some(),
            )?;
        }
        if self.config.liquidation.is_some() {
            econ_write_liquidation(
                &mut out,
                &self.liquidation,
                self.config.amm.is_some(),
                has_binding,
            )?;
        }
        if self.config.amm.is_some() {
            econ_write_amm(&mut out, &self.amm, has_binding)?;
        }
        if has_binding {
            econ_write_contract_state(&mut out, &self.contract_binding)?;
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
        if bytes.starts_with(ECONOMIC_INSURANCE_MAGIC) {
            return Self::from_insurance_snapshot_bytes(bytes, expected_config, admin, permitted);
        }
        if expected_config.insurance.is_some() {
            return Err(EconomicError::InvalidConfig);
        }
        if bytes.starts_with(b"TLEC\x08") {
            return Self::from_native_snapshot_bytes(bytes, expected_config, admin, permitted);
        }
        if expected_config.native_entitlements.is_some() {
            return Err(EconomicError::InvalidSnapshot);
        }
        if bytes.starts_with(ECONOMIC_SOURCE_MAGIC) {
            return Self::from_source_snapshot_bytes(bytes, expected_config, admin, permitted);
        }
        if expected_config.source_consensus.is_some() {
            return Err(EconomicError::InvalidConfig);
        }
        let raw = verify_crc(bytes).map_err(|_| econ_snapshot_bad())?;
        let mut reader = Cursor::new(raw);
        let version = reader.fixed::<5>().map_err(|_| econ_snapshot_bad())?;
        let has_binding = version == *ECONOMIC_CONTRACT_MAGIC;
        let (has_funding, has_marking, has_liquidation, has_amm) =
            if version == *ECONOMIC_STATE_MAGIC {
                (false, false, false, false)
            } else if version == *ECONOMIC_FUNDING_MAGIC {
                (true, false, false, false)
            } else if version == *ECONOMIC_MARK_MAGIC {
                (false, true, false, false)
            } else if version == *ECONOMIC_LIQUIDATION_MAGIC {
                (false, true, true, false)
            } else if version == *ECONOMIC_AMM_MAGIC {
                (false, true, true, true)
            } else if has_binding {
                (false, true, false, false)
            } else {
                return Err(econ_snapshot_bad());
            };
        let config = econ_read_config(
            &mut reader,
            has_funding,
            has_marking,
            has_liquidation,
            has_amm,
            has_binding,
        )?;
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
                    margin_per_contract: if has_binding {
                        econ_read_optional_decimal(&mut reader)?
                    } else {
                        None
                    },
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
        let funding = if config.funding.is_some() {
            econ_read_funding(&mut reader)?
        } else {
            FundingState::default()
        };
        let marking = if has_marking {
            econ_read_marks(
                &mut reader,
                config.liquidation.is_some(),
                config.amm.is_some(),
            )?
        } else {
            MarkState::default()
        };
        let liquidation = if config.liquidation.is_some() {
            econ_read_liquidation(&mut reader, config.amm.is_some(), has_binding)?
        } else {
            LiquidationState::default()
        };
        let amm = if config.amm.is_some() {
            econ_read_amm(&mut reader, has_binding)?
        } else {
            FundedAmmState::default()
        };
        let contract_binding = if has_binding {
            econ_read_contract_state(&mut reader)?
        } else {
            ContractBindingState::default()
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
            insurance: InsuranceState::default(),
            unfunded_loss_atoms,
            receipts,
            internal_sequence,
            funding,
            marking,
            liquidation,
            amm,
            contract_binding,
            replaying_contract_history: false,
            source_consensus: SourceConsensusState::default(),
            replaying_source_history: false,
            native_entitlements: NativeEconomicState::default(),
            replaying_native_history: false,
        };
        result.verify()?;
        if result.snapshot_bytes()?.as_slice() != bytes {
            return Err(econ_snapshot_bad());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod contract_codec_tests {
    use super::*;

    fn owner() -> AccountId {
        "alice".try_into().unwrap()
    }
    fn id() -> TxId {
        TxId::from_bytes([8; 32])
    }
    fn property() -> PropertyId {
        7.try_into().unwrap()
    }
    fn joint(operation: JointOperation) -> EconomicInput {
        EconomicInput::Joint(UnverifiedJointTx {
            txid: id(),
            actor: owner(),
            operation,
        })
    }
    #[test]
    fn every_typed_frame_variant_roundtrips_without_implying_admission() {
        let mut inputs = vec![
            joint(JointOperation::Issue {
                id: property(),
                kind: crate::ledger::PropertyKind::Fixed,
                initial_atoms: 7,
            }),
            joint(JointOperation::Issue {
                id: property(),
                kind: crate::ledger::PropertyKind::Managed,
                initial_atoms: 0,
            }),
            joint(JointOperation::Send {
                id: property(),
                recipient: owner(),
                amount_atoms: 3,
            }),
            joint(JointOperation::Grant {
                id: property(),
                recipient: owner(),
                amount_atoms: 4,
            }),
            joint(JointOperation::Redeem {
                id: property(),
                amount_atoms: 2,
            }),
            joint(JointOperation::SpotPlace(SpotIntent {
                offered: property(),
                desired: 8.try_into().unwrap(),
                offered_atoms: 3,
                expected_atoms: 5,
                post: true,
                stop: false,
            })),
            joint(JointOperation::UtxoTradeType3),
            joint(JointOperation::EconomicSettlement { ty: 23 }),
            joint(JointOperation::Unsupported { ty: 35 }),
            EconomicInput::Amm(AmmCapitalInput::Deposit {
                txid: id(),
                owner: owner(),
                collateral_atoms: 4,
                min_share_units: 3,
            }),
            EconomicInput::Amm(AmmCapitalInput::Redeem {
                txid: id(),
                owner: owner(),
                share_units: 2,
                min_collateral_atoms: 1,
            }),
            EconomicInput::Order(Order {
                txid: id(),
                owner: owner(),
                contract: 1.try_into().unwrap(),
                height: 2,
                side: Side::Sell,
                quantity: Decimal::from(2),
                price: Decimal::from(3),
                post: false,
                is_market: true,
            }),
            EconomicInput::CreateContract(CreateContractInput {
                txid: id(),
                issuer: owner(),
                parameters: ContractParameters {
                    native: false,
                    underlying_oracle_id: Some(1),
                    on_chain_data: vec![(
                        ContractAsset::BaseChain,
                        ContractAsset::Property(property()),
                    )],
                    notional_property_id: ContractAsset::BaseChain,
                    notional_value: Decimal::from(1),
                    collateral_property_id: ContractAsset::Property(property()),
                    leverage: Decimal::from(10),
                    expiry_period: 0,
                    series: 1,
                    inverse: true,
                    fee: Decimal::from(1),
                    whitelist: None,
                    ticker: Some("INPUT".into()),
                },
            }),
        ];
        for selector in [CancelSelector::All, CancelSelector::ByOrderTxId(id())] {
            inputs.push(joint(JointOperation::SpotCancel {
                offered: property(),
                desired: 8.try_into().unwrap(),
                is_contract: false,
                selector,
            }));
        }
        for criteria in [
            CancelCriteria::All,
            CancelCriteria::Txid(id()),
            CancelCriteria::Price {
                side: Side::Buy,
                price: Decimal::from(3),
            },
        ] {
            inputs.push(EconomicInput::Cancel {
                txid: id(),
                owner: owner(),
                criteria,
            });
        }
        let mut actions = vec![
            RegistryAction::Activate {
                types: vec![1, 2, 16, 18],
                code_hash: [9; 32],
            },
            RegistryAction::CreateWhitelist {
                name: "LIST".into(),
                url: "url".into(),
                description: "desc".into(),
                backup: owner(),
            },
            RegistryAction::Attest {
                list_id: 1,
                subject: owner(),
                metadata: "metadata".into(),
            },
            RegistryAction::RevokeAttestation {
                list_id: 1,
                subject: owner(),
            },
            RegistryAction::CreateOracle {
                ticker: "ORACLE".into(),
                url: "url".into(),
                backup: owner(),
                lag: 2,
            },
            RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4: 7,
                high_1e4: Some(9),
                low_1e4: None,
                close_1e4: Some(8),
            },
            RegistryAction::CloseOracle { oracle_id: 1 },
        ];
        for target in [
            AdminTarget::Whitelist(1),
            AdminTarget::Oracle(2),
            AdminTarget::Token(3),
        ] {
            actions.push(RegistryAction::UpdateAdmin {
                target,
                new_address: owner(),
                update_backup: true,
            });
        }
        inputs.extend(
            actions
                .into_iter()
                .map(|action| joint(JointOperation::Registry(action))),
        );
        for input in inputs {
            let mut out = EconomicWriter(vec![]);
            econ_write_input(&mut out, &input, &mut ContractRecordBudget::default()).unwrap();
            let mut reader = Cursor::new(&out.0);
            assert_eq!(
                econ_read_input(&mut reader, &mut ContractRecordBudget::default()).unwrap(),
                input
            );
            reader.end().unwrap();
        }
    }
    #[test]
    fn nested_contract_counts_are_rejected_before_their_elements_are_read() {
        let mut raw = vec![0, 0];
        raw.extend_from_slice(&1u32.to_be_bytes());
        let mut budget = ContractRecordBudget {
            used: MAX_CONTRACT_RECORDS,
        };
        assert_eq!(
            econ_read_contract_parameters(&mut Cursor::new(&raw), &mut budget),
            Err(EconomicError::Capacity)
        );
        let mut raw = 1u32.to_be_bytes().to_vec();
        raw.push(0);
        raw.extend_from_slice(&0u32.to_be_bytes());
        raw.extend_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            econ_read_contract_state(&mut Cursor::new(&raw)),
            Err(EconomicError::Capacity)
        );
    }
    #[test]
    fn pool_average_context_is_v6_only_and_canonical() {
        let epoch = AmmQuoteEpoch {
            id: id(),
            height: 2,
            mark_id: id(),
            generation: 1,
            position_before: Decimal::from(1),
            average_price_before: Some(Decimal::from(100)),
            cash_before_atoms: 7,
            margin_before_atoms: 8,
            unpaid_loss_before_atoms: 0,
            cash_leg_count_before: 0,
            quotes: vec![],
        };
        assert!(econ_write_amm_epoch(
            &mut EconomicWriter(vec![]),
            &epoch,
            &mut AmmRecordBudget::default(),
            false
        )
        .is_err());
        let mut out = EconomicWriter(vec![]);
        econ_write_amm_epoch(&mut out, &epoch, &mut AmmRecordBudget::default(), true).unwrap();
        let mut reader = Cursor::new(&out.0);
        assert_eq!(
            econ_read_amm_epoch(&mut reader, &mut AmmRecordBudget::default(), true).unwrap(),
            epoch
        );
        reader.end().unwrap();
    }
}

#[cfg(test)]
mod funding_codec_compat_tests {
    use super::*;

    #[test]
    fn forced_mark_tags_are_explicitly_gated_by_format_version() {
        let marks = MarkState {
            events: vec![MarkEvent {
                height: 1,
                kind: MarkEventKind::Forced {
                    epoch_id: TxId::from_bytes([1; 32]),
                    fill_index: 0,
                    maker_txid: TxId::from_bytes([2; 32]),
                    distressed_owner: "alice".try_into().unwrap(),
                },
                entries: vec![],
                collected_loss_atoms: 0,
                paid_claim_atoms: 0,
                unfunded_loss_atoms: 0,
            }],
            ..MarkState::default()
        };
        assert!(econ_write_marks(&mut EconomicWriter(vec![]), &marks, false, false).is_err());
        let mut out = EconomicWriter(vec![]);
        econ_write_marks(&mut out, &marks, true, false).unwrap();
        assert!(econ_read_marks(&mut Cursor::new(&out.0), false, false).is_err());
        let mut reader = Cursor::new(&out.0);
        assert_eq!(econ_read_marks(&mut reader, true, false).unwrap(), marks);
        reader.end().unwrap();
    }

    #[test]
    fn nested_liquidation_book_budget_is_checked_before_reading_declared_ids() {
        let mut raw = vec![];
        for number in [1u32, 0, 0, 0, MAX_LIQUIDATION_RECORDS as u32] {
            raw.extend_from_slice(&number.to_be_bytes());
        }
        let mut budget = LiquidationRecordBudget { used: 1 };
        assert_eq!(
            econ_read_book_budgeted(&mut Cursor::new(&raw), Some(&mut budget)),
            Err(EconomicError::Capacity)
        );
    }

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
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            network: TradeLayerMainnet::Bitcoin,
            contract: 1.try_into().unwrap(),
            collateral: 7.try_into().unwrap(),
            inverse: false,
            notional: "1".parse().unwrap(),
            initial_margin_per_contract: "10".parse().unwrap(),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
            contract_binding: None,
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

        // Independently captured from the pre-mark, funding-enabled compiled
        // owner with this same activation/ledger and empty funding state.
        state.config.fee_route = FeeRoute::Oracle(1);
        state.config.funding = Some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: 96,
            policy: FundingPolicy::ConservingFundingV1,
        });
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x02");
        assert_eq!(bytes.len(), 822);
        assert_eq!(
            crate::hash256::double_sha256(&bytes),
            [
                99, 2, 107, 167, 228, 167, 249, 119, 218, 220, 159, 146, 23, 215, 170, 239, 207,
                125, 242, 70, 170, 104, 178, 232, 186, 1, 24, 145, 34, 176, 111, 1
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
        assert_eq!(restored.marking, MarkState::default());

        // Captured from the compiled pre-liquidation owner, with both funding
        // and marking configured and the same activation/bootstrap fixture.
        state.config.marking = Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        });
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..5], b"TLEC\x03");
        assert_eq!(bytes.len(), 844);
        assert_eq!(
            crate::hash256::double_sha256(&bytes),
            [
                106, 202, 8, 228, 68, 191, 224, 11, 18, 183, 235, 80, 104, 33, 189, 255, 217, 151,
                228, 71, 150, 178, 56, 237, 2, 86, 192, 193, 222, 222, 208, 89
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
        assert_eq!(restored.liquidation, LiquidationState::default());
    }
}
