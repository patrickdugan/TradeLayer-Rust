//! Derived activation origins from the actual verified v7 economic owner.
//! No state store, Ledger, checkpoint format, or authenticated capability is added.
use crate::chain::{BlockHeader, Tip};
use crate::consensus_source::{
    SourceBlockMode, SourceDisposition, MAX_SOURCE_FRAMES, MAX_SOURCE_RECORDS,
};
use crate::economic_chain::{DurableEconomicChain, EconomicChain, EconomicHead};
use crate::ledger::{AccountId, TxId};
use crate::legacy_txdecoder_audit::{source_decode_historical_method_unverified, HistoricalValue};
use crate::network_profile::TradeLayerMainnet;
use crate::registry::{Activation, MAX_ACTIVATION_TYPE};
use crate::validity_source::SourceValue;

pub const MAX_ACTIVATION_ORIGINS: usize = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationHistoryError {
    MissingOriginHistory,
    IncompleteOriginHistory,
    InvalidOwner,
    WrongTip,
    WrongHead,
    UnhealthyOwner,
    Capacity,
    InvalidType(u8),
    HeightOutsideHistory,
    InvalidAppliedActivation,
    DuplicateOrigin(u8),
    RegistryOriginMismatch(u8),
    ClaimedHistoryMismatch,
    UnauthenticatedInputAndState,
}
impl core::fmt::Display for ActivationHistoryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ActivationHistoryError {}
type Result<T> = core::result::Result<T, ActivationHistoryError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedActivationOrigin {
    pub txid: TxId,
    /// This is the source candidate's claimed sender, not an authenticated actor.
    pub claimed_actor: AccountId,
    pub header: BlockHeader,
    pub position: u32,
    /// Index within this block's actual source execution receipts.
    pub execution_index: u32,
    pub mode: SourceBlockMode,
    /// Original accepted target order, not an independently sorted substitute.
    pub types: Vec<u8>,
    pub code_hash: [u8; 32],
}
impl AcceptedActivationOrigin {
    /// The current owner records target0 metadata; it does not execute funded genesis.
    pub fn requires_native_genesis_policy(&self) -> bool {
        self.types.contains(&0)
    }
    pub const fn require_authenticated_admission(&self) -> Result<()> {
        Err(ActivationHistoryError::UnauthenticatedInputAndState)
    }
}

/// The borrow prevents the owner from changing while records are queried. Only
/// small derived origins are allocated; no Ledger/economic owner is cloned.
pub struct ActivationHistoryView<'a> {
    chain: &'a EconomicChain,
    records: Vec<AcceptedActivationOrigin>,
    indices: [Option<usize>; 36],
    durable_head: Option<EconomicHead>,
}
impl<'a> ActivationHistoryView<'a> {
    pub fn from_chain_unverified(chain: &'a EconomicChain, expected_tip: Tip) -> Result<Self> {
        if chain.tip() != expected_tip {
            return Err(ActivationHistoryError::WrongTip);
        }
        let state = chain.state();
        if state.config().source_consensus.is_none() {
            return Err(ActivationHistoryError::MissingOriginHistory);
        }
        if state.config() != &chain.config().economics
            || state.finalized_height().unwrap_or(0) != expected_tip.height
        {
            return Err(ActivationHistoryError::InvalidOwner);
        }
        let source = state.source_consensus_state();
        if source.bootstrap.is_empty() || source.frames.len() as u64 != expected_tip.height {
            return Err(ActivationHistoryError::IncompleteOriginHistory);
        }
        if source.frames.len() > MAX_SOURCE_FRAMES {
            return Err(ActivationHistoryError::Capacity);
        }
        let mut parent = Tip {
            height: 0,
            id: chain.config().economics.network.genesis(),
        };
        let mut total = 0usize;
        for frame in &source.frames {
            if frame.header.height
                != parent
                    .height
                    .checked_add(1)
                    .ok_or(ActivationHistoryError::Capacity)?
                || frame.header.parent != parent.id
                || frame.receipts.len() != frame.transactions.len()
            {
                return Err(ActivationHistoryError::IncompleteOriginHistory);
            }
            total = total
                .checked_add(1)
                .and_then(|n| n.checked_add(frame.transactions.len()))
                .and_then(|n| n.checked_add(frame.receipts.len()))
                .ok_or(ActivationHistoryError::Capacity)?;
            if total > MAX_SOURCE_RECORDS {
                return Err(ActivationHistoryError::Capacity);
            }
            parent = Tip {
                height: frame.header.height,
                id: frame.header.id,
            };
        }
        if parent != expected_tip {
            return Err(ActivationHistoryError::WrongTip);
        }
        // This recomputes actual raw Types/validity/mapping/candidate application
        // against the sole owner from its independently committed pristine
        // bootstrap. Stored dispositions and dependency digests are not trusted.
        state
            .verify()
            .map_err(|_| ActivationHistoryError::InvalidOwner)?;
        let mut result = Self {
            chain,
            records: Vec::new(),
            indices: [None; 36],
            durable_head: None,
        };
        for frame in &source.frames {
            for (execution_index, receipt) in frame.receipts.iter().enumerate() {
                if receipt.disposition != SourceDisposition::Applied {
                    continue;
                }
                let tx = frame
                    .transactions
                    .binary_search_by_key(&receipt.position, |tx| tx.position)
                    .ok()
                    .and_then(|at| frame.transactions.get(at))
                    .filter(|tx| tx.txid == receipt.txid)
                    .ok_or(ActivationHistoryError::InvalidAppliedActivation)?;
                if tx.transaction_type() != Some(0) {
                    continue;
                }
                if result.records.len() >= MAX_ACTIVATION_ORIGINS {
                    return Err(ActivationHistoryError::Capacity);
                }
                let SourceValue::String(actor) = &tx.sender else {
                    return Err(ActivationHistoryError::InvalidAppliedActivation);
                };
                let claimed_actor = AccountId::try_from(actor.as_str())
                    .map_err(|_| ActivationHistoryError::InvalidAppliedActivation)?;
                if &claimed_actor != state.joint().registry().protocol_admin() {
                    return Err(ActivationHistoryError::InvalidAppliedActivation);
                }
                let body = tx
                    .payload
                    .get(1..)
                    .ok_or(ActivationHistoryError::InvalidAppliedActivation)?;
                let decoded =
                    source_decode_historical_method_unverified("decodeActivateTradeLayer", body)
                        .map_err(|_| ActivationHistoryError::InvalidAppliedActivation)?;
                let HistoricalValue::Array(types) = decoded
                    .value()
                    .get("txTypesToActivate")
                    .ok_or(ActivationHistoryError::InvalidAppliedActivation)?
                else {
                    return Err(ActivationHistoryError::InvalidAppliedActivation);
                };
                if types.is_empty() || types.len() > 36 {
                    return Err(ActivationHistoryError::InvalidAppliedActivation);
                }
                let mut targets = Vec::with_capacity(types.len());
                for target in types {
                    let HistoricalValue::Number(n) = target else {
                        return Err(ActivationHistoryError::InvalidAppliedActivation);
                    };
                    let n = n.as_f64();
                    if !n.is_finite()
                        || n < 0.0
                        || n.fract() != 0.0
                        || n > f64::from(MAX_ACTIVATION_TYPE)
                    {
                        return Err(ActivationHistoryError::InvalidAppliedActivation);
                    }
                    targets.push(n as u8);
                }
                let HistoricalValue::String(hash) = decoded
                    .value()
                    .get("codeHash")
                    .ok_or(ActivationHistoryError::InvalidAppliedActivation)?
                else {
                    return Err(ActivationHistoryError::InvalidAppliedActivation);
                };
                let code_hash = parse_hash(hash)?;
                let index = result.records.len();
                for ty in &targets {
                    let slot = &mut result.indices[usize::from(*ty)];
                    if slot.replace(index).is_some() {
                        return Err(ActivationHistoryError::DuplicateOrigin(*ty));
                    }
                    if state.joint().registry().activation(*ty)
                        != Some(Activation {
                            height: frame.header.height,
                            code_hash,
                        })
                    {
                        return Err(ActivationHistoryError::RegistryOriginMismatch(*ty));
                    }
                }
                result.records.push(AcceptedActivationOrigin {
                    txid: tx.txid,
                    claimed_actor,
                    header: frame.header,
                    position: tx.position,
                    execution_index: u32::try_from(execution_index)
                        .map_err(|_| ActivationHistoryError::Capacity)?,
                    mode: frame.mode,
                    types: targets,
                    code_hash,
                });
            }
        }
        for ty in 0..=MAX_ACTIVATION_TYPE {
            if state.joint().registry().activation(ty).is_some()
                != result.indices[usize::from(ty)].is_some()
            {
                return Err(ActivationHistoryError::RegistryOriginMismatch(ty));
            }
        }
        Ok(result)
    }
    pub fn from_durable(
        owner: &'a DurableEconomicChain,
        expected_head: EconomicHead,
    ) -> Result<Self> {
        let actual = owner
            .head()
            .map_err(|_| ActivationHistoryError::UnhealthyOwner)?;
        if actual != expected_head {
            return Err(ActivationHistoryError::WrongHead);
        }
        let chain = owner
            .chain()
            .map_err(|_| ActivationHistoryError::UnhealthyOwner)?;
        let mut result = Self::from_chain_unverified(chain, expected_head.tip)?;
        result.durable_head = Some(expected_head);
        Ok(result)
    }
    pub fn records(&self) -> &[AcceptedActivationOrigin] {
        &self.records
    }
    pub fn tip(&self) -> Tip {
        self.chain.tip()
    }
    pub fn network(&self) -> TradeLayerMainnet {
        self.chain.config().economics.network
    }
    pub fn durable_head(&self) -> Option<EconomicHead> {
        self.durable_head
    }
    pub fn origin(&self, ty: u8) -> Result<Option<&AcceptedActivationOrigin>> {
        let at = self
            .indices
            .get(usize::from(ty))
            .ok_or(ActivationHistoryError::InvalidType(ty))?;
        Ok(at.and_then(|i| self.records.get(i)))
    }
    /// State at the END of a retained canonical historical height. There is no
    /// record before its originating block, including at genesis height zero.
    pub fn activation_at(&self, ty: u8, height: u64) -> Result<Option<Activation>> {
        if height > self.tip().height {
            return Err(ActivationHistoryError::HeightOutsideHistory);
        }
        Ok(self
            .origin(ty)?
            .filter(|r| r.header.height <= height)
            .map(|r| Activation {
                height: r.header.height,
                code_hash: r.code_hash,
            }))
    }
    /// Existing Registry/Joint prior-height rule; the next candidate height is
    /// queryable, but this does not authenticate or authorize that candidate.
    pub fn is_effective_at(&self, ty: u8, height: u64) -> Result<bool> {
        if height
            > self
                .tip()
                .height
                .checked_add(1)
                .ok_or(ActivationHistoryError::Capacity)?
        {
            return Err(ActivationHistoryError::HeightOutsideHistory);
        }
        Ok(self.origin(ty)?.is_some_and(|r| r.header.height < height))
    }
    /// Reject modified/exported origin assertions against this exact borrowed
    /// owner. Equality is evidence binding only, never a trust promotion.
    pub fn verify_claimed_history(&self, claimed: &[AcceptedActivationOrigin]) -> Result<()> {
        if claimed.len() > MAX_ACTIVATION_ORIGINS {
            return Err(ActivationHistoryError::Capacity);
        }
        if claimed != self.records {
            return Err(ActivationHistoryError::ClaimedHistoryMismatch);
        }
        Ok(())
    }
    pub const fn require_authenticated_admission(&self) -> Result<()> {
        Err(ActivationHistoryError::UnauthenticatedInputAndState)
    }
}
fn parse_hash(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ActivationHistoryError::InvalidAppliedActivation);
    }
    let mut out = [0; 32];
    for (i, b) in s.as_bytes().chunks_exact(2).enumerate() {
        out[i] = u8::from_str_radix(
            core::str::from_utf8(b)
                .map_err(|_| ActivationHistoryError::InvalidAppliedActivation)?,
            16,
        )
        .map_err(|_| ActivationHistoryError::InvalidAppliedActivation)?;
    }
    Ok(out)
}
