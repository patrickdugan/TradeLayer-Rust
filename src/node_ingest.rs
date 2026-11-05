//! Admission-to-economy ingest: authenticated TradeLayer candidates from one
//! observed raw Bitcoin block become one `UnverifiedSourceBlock`, which the
//! durable economic chain applies; an observer fork rolls the chain back.
//!
//! `docs/NODE.md` states the scope. In short: a candidate reaches the source
//! runner only when `admission::admit_block` authenticated its sender (input
//! 0, P2PKH or P2WPKH, SIGHASH_ALL) and proved the transaction included in the
//! block. Refused candidates are omitted: the runner records the candidates it
//! is given, and a candidate without a proven sender has no sender to give.
//!
//! # Height invariant
//!
//! The economic chain starts at height 0 on the network's genesis tip and
//! extends one block at a time. `NodeBinding` maps it onto the observed chain:
//! economic height 0 is the observed `base` tip, and economic height `e` is
//! observed height `base.height + e`. With `base` at height 0 on the network
//! genesis the map is the identity. Before every apply the economic tip, seen
//! through the map, must be the observer's anchor or a block on the observer's
//! selected chain, and the block applied must be that tip's child on that
//! chain. Any other state is `IngestError::Misaligned` and nothing is applied.
//! A nonzero `base` shifts the heights the source rules see by `base.height`.

use crate::admission::{
    admit_block, sender_address, AdmissionContext, AdmissionError, AdmissionNetwork,
    CandidateOutcome, ParentOutpoint, ParentOutput, ParentOutputSource, ParentSourceError,
};
use crate::bitcoin_inclusion::BitcoinPowProfile;
use crate::bitcoin_standard_input::BitcoinInputTemplate;
use crate::bitcoin_witness_binding::{
    inspect_bitcoin_witness_binding, BitcoinWitnessRules, ExpectedWitnessBlock,
};
use crate::chain::{BlockHeader, BlockId, Tip};
use crate::consensus_source::{
    SourceBlockMode, SourceBlockReport, SourceTransaction, UnverifiedSourceBlock,
};
use crate::daemon::{DaemonError, RawObservation};
use crate::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicHead,
};
use crate::ledger::TxId;
use crate::network_profile::TradeLayerMainnet;
use crate::read_only::{hex_bytes, CarrierFormat};
use crate::validity_source::SourceValue;

/// The sender scope the node admits, reported by every status surface.
pub const ADMISSION_SCOPE: &str = "input0-p2pkh-p2wpkh-sighash-all";
/// Refusals listed individually in one block report; the rest are only counted.
pub const MAX_LISTED_REFUSALS: usize = 32;
const COIN: f64 = 100_000_000.0;

#[derive(Debug)]
pub enum IngestError {
    /// The economic tip, the observer's chain and the next block do not line up.
    Misaligned(&'static str),
    /// The block cannot be trusted as bytes: header, proof of work, parent,
    /// witness commitment or transaction boundaries.
    Admission(AdmissionError),
    /// The parent-output source failed; refusing the candidates instead would
    /// make the state depend on the outage.
    PrevoutsUnavailable(String),
    Economic(DurableEconomicError),
    Observer(DaemonError),
    /// The observer's fork evidence names no common ancestor to roll back to.
    ForkUnresolved(&'static str),
}
impl core::fmt::Display for IngestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Misaligned(why) => write!(f, "Misaligned({why})"),
            Self::Admission(e) => write!(f, "Admission({e})"),
            Self::PrevoutsUnavailable(why) => write!(f, "PrevoutsUnavailable({why})"),
            Self::Economic(e) => write!(f, "Economic({e})"),
            Self::Observer(e) => write!(f, "Observer({e})"),
            Self::ForkUnresolved(why) => write!(f, "ForkUnresolved({why})"),
        }
    }
}
impl std::error::Error for IngestError {}
impl From<DurableEconomicError> for IngestError {
    fn from(value: DurableEconomicError) -> Self {
        Self::Economic(value)
    }
}
impl From<DaemonError> for IngestError {
    fn from(value: DaemonError) -> Self {
        Self::Observer(value)
    }
}

type Result<T> = core::result::Result<T, IngestError>;

/// The map between the observed chain and the economic chain; see the module
/// documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeBinding {
    pub network: TradeLayerMainnet,
    /// The observed tip that is economic height 0.
    pub base: Tip,
}
impl NodeBinding {
    /// The economic header of an observed block. The first economic block's
    /// parent is the network genesis tip, which stands for `base`.
    pub fn economic_header(&self, observed: BlockHeader) -> Result<BlockHeader> {
        let height = observed
            .height
            .checked_sub(self.base.height)
            .filter(|h| *h > 0)
            .ok_or(IngestError::Misaligned("block is not above the base tip"))?;
        let parent = if height == 1 {
            if observed.parent != self.base.id {
                return Err(IngestError::Misaligned(
                    "first block does not extend the base",
                ));
            }
            self.network.genesis()
        } else {
            observed.parent
        };
        Ok(BlockHeader {
            height,
            id: observed.id,
            parent,
        })
    }
    /// The observed tip an economic tip stands for.
    pub fn observed_tip(&self, economic: Tip) -> Tip {
        if economic.height == 0 {
            self.base
        } else {
            Tip {
                height: economic.height.saturating_add(self.base.height),
                id: economic.id,
            }
        }
    }
    /// The economic tip an observed tip stands for; `None` below the base or
    /// when the base height holds another block.
    pub fn economic_tip(&self, observed: Tip) -> Option<Tip> {
        if observed.height == self.base.height {
            return (observed.id == self.base.id).then_some(Tip {
                height: 0,
                id: self.network.genesis(),
            });
        }
        Some(Tip {
            height: observed.height.checked_sub(self.base.height)?,
            id: observed.id,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NodeIngestConfig {
    pub binding: NodeBinding,
    pub admission_network: AdmissionNetwork,
    pub pow: BitcoinPowProfile,
    pub rules: BitcoinWitnessRules,
    pub mode: SourceBlockMode,
}

/// Supplies the parent outputs of one block, fetched once and only when a
/// candidate needs them. The answers are claims; admission verifies them.
pub trait PrevoutProvider {
    fn parent_source(
        &mut self,
        block: BlockId,
    ) -> core::result::Result<Box<dyn ParentOutputSource>, String>;
}

struct LazyParents<'a, P: PrevoutProvider + ?Sized> {
    provider: &'a mut P,
    block: BlockId,
    source: Option<Box<dyn ParentOutputSource>>,
    failure: Option<String>,
}
impl<P: PrevoutProvider + ?Sized> ParentOutputSource for LazyParents<'_, P> {
    fn parent_output(
        &mut self,
        outpoint: &ParentOutpoint,
    ) -> core::result::Result<Option<ParentOutput>, ParentSourceError> {
        if self.failure.is_some() {
            return Err(ParentSourceError::Unavailable);
        }
        if self.source.is_none() {
            match self.provider.parent_source(self.block) {
                Ok(source) => self.source = Some(source),
                Err(why) => {
                    self.failure = Some(why);
                    return Err(ParentSourceError::Unavailable);
                }
            }
        }
        self.source
            .as_mut()
            .expect("source loaded above")
            .parent_output(outpoint)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedCandidate {
    /// Display-order txid, hex.
    pub txid: String,
    pub position: u32,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct BlockReport {
    pub observed: Tip,
    pub economic_height: u64,
    /// Candidates the admission pass examined.
    pub candidates: usize,
    pub authenticated: usize,
    /// Refused candidates, the first `MAX_LISTED_REFUSALS` of them.
    pub refused: Vec<RefusedCandidate>,
    pub refused_total: usize,
    /// Authenticated candidates the source runner has no input form for: the
    /// binary `TLR` carrier format and non-ASCII legacy payloads.
    pub unrepresentable: usize,
    pub submitted: usize,
    pub source: SourceBlockReport,
}

/// One admitted block, before it is applied.
#[derive(Debug, Clone)]
pub struct AdmittedBlock {
    pub block: UnverifiedSourceBlock,
    pub report: BlockReport,
}

fn number(value: f64) -> SourceValue {
    SourceValue::number(value)
}

/// `(satoshis, script)` of each output, or `None` when the bytes do not parse.
fn transaction_outputs(raw: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    fn varint(raw: &[u8], at: &mut usize) -> Option<u64> {
        let first = *raw.get(*at)?;
        *at += 1;
        let width = match first {
            0..=0xfc => return Some(u64::from(first)),
            0xfd => 2,
            0xfe => 4,
            _ => 8,
        };
        let bytes = raw.get(*at..at.checked_add(width)?)?;
        *at += width;
        Some(
            bytes
                .iter()
                .rev()
                .fold(0u64, |value, byte| (value << 8) | u64::from(*byte)),
        )
    }
    let mut at = 4usize;
    if raw.get(4) == Some(&0) && raw.get(5) == Some(&1) {
        at = 6;
    }
    let inputs = varint(raw, &mut at)?;
    for _ in 0..inputs {
        at = at.checked_add(36)?;
        let script = usize::try_from(varint(raw, &mut at)?).ok()?;
        at = at.checked_add(script)?.checked_add(4)?;
    }
    let count = varint(raw, &mut at)?;
    let mut outputs = Vec::new();
    for _ in 0..count {
        let value = u64::from_le_bytes(raw.get(at..at.checked_add(8)?)?.try_into().ok()?);
        at += 8;
        let length = usize::try_from(varint(raw, &mut at)?).ok()?;
        let script = raw.get(at..at.checked_add(length)?)?;
        at += length;
        outputs.push((value, script));
    }
    Some(outputs)
}

fn output_address(network: AdmissionNetwork, script: &[u8]) -> Option<String> {
    if script.len() == 25 && script[..3] == [0x76, 0xa9, 20] && script[23..] == [0x88, 0xac] {
        return Some(sender_address(
            network,
            BitcoinInputTemplate::P2pkh,
            script[3..23].try_into().ok()?,
        ));
    }
    if script.len() == 22 && script[..2] == [0, 20] {
        return Some(sender_address(
            network,
            BitcoinInputTemplate::P2wpkh,
            script[2..].try_into().ok()?,
        ));
    }
    None
}

/// The source-runner row of one authenticated candidate, or `None` when the
/// runner has no input form for its carrier.
///
/// - `txid`, `position`: the display-order txid and block position.
/// - `payload`: the carrier text after the `tl` marker (type character first).
/// - `marker`: `"tl"`.
/// - `sender`: the authenticated address; `sender_amount`: the claimed parent
///   output value in coin units (signed only for P2WPKH).
/// - `reference`: one `{vout, address, satoshis}` object per P2PKH or P2WPKH
///   output other than the carrier, in output order; `reference_amount`: the
///   first such output's value in coin units, 0 when there is none. These
///   bytes belong to the included transaction.
fn source_transaction(
    network: AdmissionNetwork,
    outcome: &CandidateOutcome,
    outputs: &[(u64, &[u8])],
) -> Option<SourceTransaction> {
    let CandidateOutcome::Authenticated {
        txid,
        position,
        sender,
        payload,
        ..
    } = outcome
    else {
        return None;
    };
    if payload.format != CarrierFormat::LegacyAscii || !payload.payload.starts_with(b"tl") {
        return None;
    }
    let text = core::str::from_utf8(&payload.payload[2..]).ok()?;
    if !text.is_ascii() || text.is_empty() {
        return None;
    }
    let mut reference = Vec::new();
    let mut first_amount = None;
    for (vout, (satoshis, script)) in outputs.iter().enumerate() {
        if vout as u32 == payload.vout {
            continue;
        }
        if let Some(address) = output_address(network, script) {
            first_amount.get_or_insert(*satoshis);
            reference.push(SourceValue::Object(vec![
                ("vout".to_owned(), number(vout as f64)),
                ("address".to_owned(), SourceValue::String(address)),
                ("satoshis".to_owned(), number(*satoshis as f64)),
            ]));
        }
    }
    Some(SourceTransaction {
        position: *position,
        txid: TxId::from_bytes(*txid),
        payload: text.to_owned(),
        marker: SourceValue::String("tl".into()),
        sender: SourceValue::String(sender.address.clone()),
        reference: SourceValue::Array(reference),
        sender_amount: number(sender.parent_amount_satoshis as f64 / COIN),
        reference_amount: number(first_amount.unwrap_or(0) as f64 / COIN),
    })
}

/// See `NodeIngest::locate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locate {
    Apply(usize),
    Ahead,
    Fork(Tip),
}

#[derive(Debug, Clone, Copy)]
pub struct NodeIngest {
    config: NodeIngestConfig,
}
impl NodeIngest {
    pub const fn new(config: NodeIngestConfig) -> Self {
        Self { config }
    }
    pub const fn config(&self) -> &NodeIngestConfig {
        &self.config
    }
    pub const fn binding(&self) -> &NodeBinding {
        &self.config.binding
    }

    /// Where the economic chain stands against the observer's view: `anchor`
    /// and the selected chain `canonical` above it.
    ///
    /// - `Apply(i)`: the economic tip is the anchor or `canonical[i - 1]`;
    ///   `canonical[i]`, when it exists, is the next block to apply.
    /// - `Ahead`: the economic tip is above the observer's tip and the
    ///   observer's tip is on the economic chain. A rotated observer
    ///   re-fetches history below the economic tip; nothing is applied until
    ///   it passes the tip.
    /// - `Fork(block)`: the economic tip is not on the observer's chain but
    ///   `block`, the highest block both chains hold, is. The economic chain
    ///   must go back to it.
    ///
    /// Anything else is `Misaligned`: the economic chain shares no block with
    /// the observer's view, or ends below its anchor.
    pub fn locate(
        &self,
        chain: &EconomicChain,
        anchor: Tip,
        canonical: &[BlockHeader],
    ) -> Result<Locate> {
        let observed = self.config.binding.observed_tip(chain.tip());
        if observed == anchor {
            return Ok(Locate::Apply(0));
        }
        if let Some(index) = canonical
            .iter()
            .position(|h| h.height == observed.height && h.id == observed.id)
        {
            return Ok(Locate::Apply(index + 1));
        }
        if observed.height < anchor.height {
            return Err(IngestError::Misaligned(
                "economic tip is below the observer's anchor",
            ));
        }
        // The economic chain's block ids by observed height.
        let frames = &chain.state().source_consensus_state().frames;
        let binding = &self.config.binding;
        let holds = |tip: Tip| -> bool {
            match tip.height.checked_sub(binding.base.height) {
                Some(0) => tip.id == binding.base.id,
                Some(e) => usize::try_from(e - 1)
                    .ok()
                    .and_then(|index| frames.get(index))
                    .is_some_and(|f| f.header.height == e && f.header.id == tip.id),
                None => false,
            }
        };
        let view = || {
            canonical
                .iter()
                .map(|h| Tip {
                    height: h.height,
                    id: h.id,
                })
                .rev()
                .chain(core::iter::once(anchor))
        };
        let Some(shared) = view().find(|tip| holds(*tip)) else {
            return Err(IngestError::Misaligned(
                "economic chain and observer share no block",
            ));
        };
        let observer_tip = view().next().expect("the view holds the anchor");
        if shared == observer_tip && observed.height > observer_tip.height {
            return Ok(Locate::Ahead);
        }
        Ok(Locate::Fork(shared))
    }

    /// Admits `record` and builds the block the source runner will execute.
    /// Nothing is applied. The prevout provider is asked only when the block
    /// holds a candidate.
    pub fn admit<P: PrevoutProvider + ?Sized>(
        &self,
        record: &RawObservation,
        provider: &mut P,
    ) -> Result<AdmittedBlock> {
        let header = record.header();
        let economic = self.config.binding.economic_header(header)?;
        let context = AdmissionContext {
            height: header.height,
            block: header.id,
            parent: header.parent,
            pow: self.config.pow,
            rules: self.config.rules,
            network: self.config.admission_network,
        };
        let mut parents = LazyParents {
            provider,
            block: header.id,
            source: None,
            failure: None,
        };
        let outcomes =
            admit_block(record.raw(), &context, &mut parents).map_err(IngestError::Admission)?;
        if let Some(why) = parents.failure {
            return Err(IngestError::PrevoutsUnavailable(why));
        }
        let authenticated = outcomes.iter().filter(|o| o.is_authenticated()).count();
        let mut refused = Vec::new();
        let mut refused_total = 0;
        for outcome in &outcomes {
            if let CandidateOutcome::Refused {
                txid,
                position,
                reason,
            } = outcome
            {
                refused_total += 1;
                if refused.len() < MAX_LISTED_REFUSALS {
                    refused.push(RefusedCandidate {
                        txid: hex_bytes(txid),
                        position: *position,
                        reason: format!("{reason:?}"),
                    });
                }
            }
        }
        let mut transactions = Vec::new();
        if authenticated > 0 {
            let relation = inspect_bitcoin_witness_binding(
                record.raw(),
                ExpectedWitnessBlock {
                    tip: Tip {
                        height: header.height,
                        id: header.id,
                    },
                    parent: header.parent,
                    rules: self.config.rules,
                },
            )
            .map_err(|e| IngestError::Admission(AdmissionError::Witness(e)))?;
            for outcome in &outcomes {
                if !outcome.is_authenticated() {
                    continue;
                }
                let raw = relation
                    .raw_transaction(outcome.position() as usize)
                    .ok_or(IngestError::Misaligned(
                        "candidate position is outside the block",
                    ))?;
                let outputs = transaction_outputs(raw)
                    .ok_or(IngestError::Misaligned("candidate outputs do not parse"))?;
                if let Some(row) =
                    source_transaction(self.config.admission_network, outcome, &outputs)
                {
                    transactions.push(row);
                }
            }
        }
        let unrepresentable = authenticated - transactions.len();
        let submitted = transactions.len();
        let block = UnverifiedSourceBlock {
            network: self.config.binding.network,
            header: economic,
            mode: self.config.mode,
            transactions,
        };
        Ok(AdmittedBlock {
            report: BlockReport {
                observed: Tip {
                    height: header.height,
                    id: header.id,
                },
                economic_height: economic.height,
                candidates: outcomes.len(),
                authenticated,
                refused,
                refused_total,
                unrepresentable,
                submitted,
                source: SourceBlockReport {
                    header: economic,
                    accepted: 0,
                    rejected: 0,
                    already_processed: 0,
                    trade_receipts: 0,
                    receipts: Vec::new(),
                },
            },
            block,
        })
    }

    /// Admits `record` and applies it to `durable`, which must sit at the
    /// block's parent. Returns the committed head and the report.
    pub fn apply<P: PrevoutProvider + ?Sized>(
        &self,
        durable: &mut DurableEconomicChain,
        record: &RawObservation,
        provider: &mut P,
    ) -> Result<(EconomicHead, BlockReport)> {
        let header = record.header();
        let expected_parent = self.config.binding.observed_tip(durable.chain()?.tip());
        if expected_parent.height.checked_add(1) != Some(header.height)
            || expected_parent.id != header.parent
        {
            return Err(IngestError::Misaligned(
                "block does not extend the economic tip",
            ));
        }
        let mut admitted = self.admit(record, provider)?;
        let head = durable.head()?;
        let (head, source) = durable.apply_source_unverified_block(head, &admitted.block)?;
        admitted.report.source = source;
        Ok((head, admitted.report))
    }

    /// Rolls `durable` back so its tip stands for `target` on the observed
    /// chain. A chain already at or below `target` is left alone.
    pub fn rollback_to(
        &self,
        durable: &mut DurableEconomicChain,
        target: Tip,
    ) -> Result<EconomicHead> {
        let economic = self
            .config
            .binding
            .economic_tip(target)
            .ok_or(IngestError::Misaligned(
                "rollback target is below the base tip",
            ))?;
        let tip = durable.chain()?.tip();
        let head = durable.head()?;
        if tip.height <= economic.height {
            return Ok(head);
        }
        Ok(durable.rollback_to(head, economic)?)
    }
}
