//! Immutable reports rebuilt from the verified complete economic owner.
//! Reports preserve execution origins; they do not settle PnL or create money.
use crate::contract_registry::ContractId;
use crate::decimal::Decimal;
use crate::economic::{
    EconomicError, EconomicState, FundingEpoch, MarkEvent, MarkEventEntry, MarkEventKind,
};
use crate::hash256::double_sha256;
use crate::mark_basis::BasisAction;
use crate::network_profile::TradeLayerMainnet;
use crate::trade_history::{
    ContractEventOrigin, ContractExecution, ExecutionPnl, TradeHistory, TradeHistoryError, TradeId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomicReportingError {
    Economic(EconomicError),
    History(TradeHistoryError),
    MissingOwnerEvidence,
    MissingExecutionEvidence,
    Capacity,
}
impl core::fmt::Display for EconomicReportingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EconomicReportingError {}
impl From<EconomicError> for EconomicReportingError {
    fn from(e: EconomicError) -> Self {
        Self::Economic(e)
    }
}
impl From<TradeHistoryError> for EconomicReportingError {
    fn from(e: TradeHistoryError) -> Self {
        Self::History(e)
    }
}
type Result<T> = core::result::Result<T, EconomicReportingError>;

/// The borrow prevents publication of another owner state while this report is
/// in use. Only trade metadata is materialized; mark/funding facts stay borrowed.
pub struct EconomicHistory<'a> {
    source: &'a EconomicState,
    history: TradeHistory,
}
impl<'a> EconomicHistory<'a> {
    pub fn from_state(source: &'a EconomicState) -> Result<Self> {
        source.verify()?;
        if source.config().marking.is_none() && !source.receipts().is_empty() {
            return Err(EconomicReportingError::MissingOwnerEvidence);
        }
        let mut history = TradeHistory::new();
        if source.mark_state().events.len() > crate::trade_history::MAX_HISTORY_RECORDS {
            return Err(EconomicReportingError::Capacity);
        }
        let mut executions = Vec::with_capacity(source.mark_state().events.len());
        let mut ordinary = source.receipts().iter();
        for (index, event) in source.mark_state().events.iter().enumerate() {
            let retained_fee_atoms;
            let origin = match &event.kind {
                MarkEventKind::Mark { .. } => continue,
                MarkEventKind::Trade {
                    buy_txid,
                    sell_txid,
                } => {
                    let receipt = ordinary
                        .next()
                        .ok_or(EconomicReportingError::MissingExecutionEvidence)?;
                    if receipt.buy_txid != *buy_txid
                        || receipt.sell_txid != *sell_txid
                        || receipt.height != event.height
                    {
                        return Err(EconomicReportingError::MissingExecutionEvidence);
                    }
                    retained_fee_atoms = receipt.retained_fee_atoms;
                    ContractEventOrigin::Orders {
                        buy_txid: *buy_txid,
                        sell_txid: *sell_txid,
                    }
                }
                MarkEventKind::Forced {
                    epoch_id,
                    fill_index,
                    maker_txid,
                    ..
                } => {
                    let liquidation = source
                        .liquidation_state()
                        .events
                        .iter()
                        .find(|e| e.id == *epoch_id)
                        .ok_or(EconomicReportingError::MissingExecutionEvidence)?;
                    let receipt = liquidation
                        .fills
                        .get(*fill_index as usize)
                        .ok_or(EconomicReportingError::MissingExecutionEvidence)?;
                    if receipt.maker_before.order.txid != *maker_txid {
                        return Err(EconomicReportingError::MissingExecutionEvidence);
                    }
                    retained_fee_atoms = receipt.retained_fee_atoms;
                    ContractEventOrigin::ForcedBook {
                        epoch: *epoch_id,
                        fill_index: *fill_index,
                        maker_txid: *maker_txid,
                    }
                }
                MarkEventKind::AmmForced {
                    epoch_id,
                    fill_index,
                    pool_id,
                    ..
                } => {
                    let liquidation = source
                        .liquidation_state()
                        .events
                        .iter()
                        .find(|e| e.id == *epoch_id)
                        .ok_or(EconomicReportingError::MissingExecutionEvidence)?;
                    let receipt = liquidation
                        .amm_fills
                        .get(*fill_index as usize)
                        .ok_or(EconomicReportingError::MissingExecutionEvidence)?;
                    if receipt.quote_before.pool_id != *pool_id {
                        return Err(EconomicReportingError::MissingExecutionEvidence);
                    }
                    retained_fee_atoms = receipt.retained_fee_atoms;
                    ContractEventOrigin::ForcedAmm {
                        epoch: *epoch_id,
                        fill_index: *fill_index,
                        pool_id: *pool_id,
                    }
                }
            };
            let event_index = u32::try_from(index).map_err(|_| EconomicReportingError::Capacity)?;
            let (buyer, seller, quantity, price) = orient(event)?;
            executions.push(ContractExecution {
                id: event_identity(source, event, event_index, &origin),
                block_height: event.height,
                event_index,
                contract_id: source.config().contract.get(),
                buyer: buyer.owner.clone(),
                seller: seller.owner.clone(),
                quantity,
                price,
                origin,
                buyer_pnl: execution_pnl(buyer),
                seller_pnl: execution_pnl(seller),
                retained_fee_atoms,
                collected_loss_atoms: event.collected_loss_atoms,
                paid_claim_atoms: event.paid_claim_atoms,
                unfunded_loss_atoms: event.unfunded_loss_atoms,
            });
        }
        if ordinary.next().is_some() {
            return Err(EconomicReportingError::MissingExecutionEvidence);
        }
        history.insert_execution_batch(&executions)?;
        Ok(Self { source, history })
    }
    pub fn network(&self) -> TradeLayerMainnet {
        self.source.config().network
    }
    pub fn contract(&self) -> ContractId {
        self.source.config().contract
    }
    pub fn finalized_height(&self) -> Option<u64> {
        self.source.finalized_height()
    }
    pub fn history(&self) -> &TradeHistory {
        &self.history
    }
    pub fn marks(&self) -> impl Iterator<Item = &MarkEvent> {
        self.source
            .mark_state()
            .events
            .iter()
            .filter(|event| matches!(event.kind, MarkEventKind::Mark { .. }))
    }
    pub fn funding(&self) -> &[FundingEpoch] {
        &self.source.funding_state().epochs
    }
}
fn execution_pnl(entry: &MarkEventEntry) -> ExecutionPnl {
    ExecutionPnl {
        booked: entry.atoms.to_decimal(),
        collected_atoms: entry.collected_atoms,
        claim_offset_atoms: entry.claim_offset_atoms,
        loss_offset_atoms: entry.loss_offset_atoms,
    }
}
fn orient(event: &MarkEvent) -> Result<(&MarkEventEntry, &MarkEventEntry, Decimal, Decimal)> {
    let [first, second] = event.entries.as_slice() else {
        return Err(EconomicReportingError::MissingExecutionEvidence);
    };
    let (
        BasisAction::Trade {
            delta: first_delta,
            price,
        },
        BasisAction::Trade {
            delta: second_delta,
            price: second_price,
        },
    ) = (&first.action, &second.action)
    else {
        return Err(EconomicReportingError::MissingExecutionEvidence);
    };
    if first_delta.is_zero() || first_delta.negated() != *second_delta || price != second_price {
        return Err(EconomicReportingError::MissingExecutionEvidence);
    }
    Ok(if first_delta.is_negative() {
        (second, first, first_delta.abs(), price.clone())
    } else {
        (first, second, first_delta.clone(), price.clone())
    })
}
fn event_identity(
    source: &EconomicState,
    event: &MarkEvent,
    event_index: u32,
    origin: &ContractEventOrigin,
) -> TradeId {
    let mut bytes = b"TradeLayer/EconomicReportExecution/v1".to_vec();
    bytes.push(source.config().network.tag());
    bytes.extend_from_slice(&source.config().network.genesis().0);
    bytes.extend_from_slice(&source.config().contract.get().to_be_bytes());
    bytes.extend_from_slice(&event.height.to_be_bytes());
    bytes.extend_from_slice(&event_index.to_be_bytes());
    match origin {
        ContractEventOrigin::Orders {
            buy_txid,
            sell_txid,
        } => {
            bytes.push(0);
            bytes.extend_from_slice(&buy_txid.as_bytes());
            bytes.extend_from_slice(&sell_txid.as_bytes());
        }
        ContractEventOrigin::ForcedBook {
            epoch,
            fill_index,
            maker_txid,
        } => {
            bytes.push(1);
            bytes.extend_from_slice(&epoch.as_bytes());
            bytes.extend_from_slice(&fill_index.to_be_bytes());
            bytes.extend_from_slice(&maker_txid.as_bytes());
        }
        ContractEventOrigin::ForcedAmm {
            epoch,
            fill_index,
            pool_id,
        } => {
            bytes.push(2);
            bytes.extend_from_slice(&epoch.as_bytes());
            bytes.extend_from_slice(&fill_index.to_be_bytes());
            bytes.extend_from_slice(&pool_id.to_be_bytes());
        }
    }
    TradeId(double_sha256(&bytes))
}
