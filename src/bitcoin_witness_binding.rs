//! Read-only BIP141 witness-byte binding for one complete Bitcoin-style block.
//!
//! Expected header/parent/rule context is supplied independently of candidate
//! bytes. It remains a caller-held claim, not authenticated chainstate. Neither
//! this relation nor a matching hash authorizes a transaction or TradeLayer.

use crate::chain::{BlockId, Tip};
use crate::channel_witness_audit::Cursor;
use crate::hash256::double_sha256;
use crate::raw_block_ingest::{
    merkle_root_unverified, RawBlockIngestError, UnverifiedRawBlockView,
};
use crate::read_only::{ScanError, MAX_IO_COUNT};

const COMMITMENT_PREFIX: [u8; 6] = [0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinWitnessRules {
    /// Corresponds to Core's expect_witness_commitment=false. Commitment-like
    /// outputs have no special meaning, and all witness data is rejected.
    NoWitness,
    /// A declared active BIP141 context; this function cannot prove activation.
    SegwitActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedWitnessBlock {
    pub tip: Tip,
    pub parent: BlockId,
    pub rules: BitcoinWitnessRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessBindingError {
    RawBlock(RawBlockIngestError),
    Transaction(ScanError),
    WrongParent,
    MissingFirstCoinbase,
    AdditionalCoinbase { index: usize },
    InvalidReservedValue,
    UnexpectedWitness,
    CommitmentMismatch,
}
impl From<RawBlockIngestError> for WitnessBindingError {
    fn from(value: RawBlockIngestError) -> Self {
        Self::RawBlock(value)
    }
}
impl From<ScanError> for WitnessBindingError {
    fn from(value: ScanError) -> Self {
        Self::Transaction(value)
    }
}
impl core::fmt::Display for WitnessBindingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WitnessBindingError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessBindingTrustError {
    ActiveChainAndRuleContextUnauthenticated,
    ParentUtxoStateUnavailable,
    FullTransactionScriptsUnverified,
    ProtocolAndActorAuthorityUnavailable,
}

/// An immutable relation over the exact borrowed block, never an admission
/// capability. There is no supplied preindex or signature/UTXO verdict.
#[derive(Debug, Clone)]
pub struct BitcoinWitnessRelation<'a> {
    view: UnverifiedRawBlockView<'a>,
    context: ExpectedWitnessBlock,
    selected_commitment_vout: Option<u32>,
    witness_root_raw: Option<[u8; 32]>,
    reserved_value: Option<[u8; 32]>,
}
impl<'a> BitcoinWitnessRelation<'a> {
    pub const fn claimed_context(&self) -> ExpectedWitnessBlock {
        self.context
    }
    pub fn transaction_count(&self) -> usize {
        self.view.transaction_count()
    }
    pub fn raw_block(&self) -> &'a [u8] {
        self.view.source_block_bytes()
    }
    pub fn raw_transaction(&self, index: usize) -> Option<&'a [u8]> {
        self.view.raw_transaction(index)
    }
    pub const fn commitment_vout(&self) -> Option<u32> {
        self.selected_commitment_vout
    }
    pub const fn witness_root_raw(&self) -> Option<[u8; 32]> {
        self.witness_root_raw
    }
    pub const fn reserved_value(&self) -> Option<[u8; 32]> {
        self.reserved_value
    }
    pub const fn require_authenticated_chain(&self) -> Result<(), WitnessBindingTrustError> {
        Err(WitnessBindingTrustError::ActiveChainAndRuleContextUnauthenticated)
    }
    pub const fn require_authenticated_parent_utxo(&self) -> Result<(), WitnessBindingTrustError> {
        Err(WitnessBindingTrustError::ParentUtxoStateUnavailable)
    }
    pub const fn require_verified_scripts(&self) -> Result<(), WitnessBindingTrustError> {
        Err(WitnessBindingTrustError::FullTransactionScriptsUnverified)
    }
    pub const fn require_protocol_actor_admission(&self) -> Result<(), WitnessBindingTrustError> {
        Err(WitnessBindingTrustError::ProtocolAndActorAuthorityUnavailable)
    }
}

fn input_cursor(raw: &[u8]) -> Result<(Cursor<'_>, bool, usize), ScanError> {
    let mut cursor = Cursor::new(raw);
    cursor.take(4)?;
    let witness = raw.get(4) == Some(&0);
    if witness && cursor.take(2)? != [0, 1] {
        return Err(ScanError::Invalid("unsupported witness marker/flag".into()));
    }
    let inputs = cursor.count()?;
    Ok((cursor, witness, inputs))
}
fn is_coinbase(raw: &[u8]) -> Result<bool, ScanError> {
    let (mut cursor, _, inputs) = input_cursor(raw)?;
    if inputs != 1 {
        return Ok(false);
    }
    let previous = cursor.take(36)?;
    Ok(previous[..32] == [0; 32] && previous[32..] == [0xff; 4])
}
struct Coinbase {
    commitment: Option<(u32, [u8; 32])>,
    reserved: Option<[u8; 32]>,
}
fn coinbase_fields(raw: &[u8]) -> Result<Coinbase, WitnessBindingError> {
    let (mut cursor, witness, inputs) = input_cursor(raw)?;
    if inputs != 1 {
        return Err(WitnessBindingError::MissingFirstCoinbase);
    }
    let previous = cursor.take(36)?;
    if previous[..32] != [0; 32] || previous[32..] != [0xff; 4] {
        return Err(WitnessBindingError::MissingFirstCoinbase);
    }
    cursor.data()?;
    cursor.take(4)?;
    let outputs = cursor.count()?;
    let mut commitment = None;
    for index in 0..outputs {
        cursor.take(8)?;
        let script = cursor.data()?;
        if script.len() >= 38 && script[..6] == COMMITMENT_PREFIX {
            // Core selects the highest matching output, including scripts
            // with optional uninterpreted trailing bytes after byte38.
            commitment = Some((
                index as u32,
                script[6..38].try_into().expect("32-byte commitment"),
            ));
        }
    }
    let mut reserved = None;
    if witness {
        let items = cursor.compact()?;
        if items > MAX_IO_COUNT {
            return Err(ScanError::TooLong.into());
        }
        for _ in 0..items {
            let item = cursor.data()?;
            if items == 1 && item.len() == 32 {
                reserved = Some(item.try_into().expect("32-byte reserved value"));
            }
        }
    }
    cursor.take(4)?;
    if cursor.offset() != raw.len() {
        return Err(ScanError::TrailingBytes.into());
    }
    Ok(Coinbase {
        commitment,
        reserved,
    })
}

/// Recompute header/TXID Merkle and every actual wtxid from the complete
/// borrowed block, then apply BIP141's declared rule context. The caller must
/// independently retain its expected header and parent; height/activation,
/// best chain, parent spentness and Script execution remain unauthenticated.
pub fn inspect_bitcoin_witness_binding<'a>(
    raw_block: &'a [u8],
    expected: ExpectedWitnessBlock,
) -> Result<BitcoinWitnessRelation<'a>, WitnessBindingError> {
    let mut header_raw = expected.tip.id.0;
    header_raw.reverse();
    let view = UnverifiedRawBlockView::parse(raw_block, Some(header_raw))?;
    let mut parent_raw = expected.parent.0;
    parent_raw.reverse();
    if raw_block[4..36] != parent_raw {
        return Err(WitnessBindingError::WrongParent);
    }
    let fresh = view.inspect_serial()?;
    let first = view
        .raw_transaction(0)
        .ok_or(WitnessBindingError::MissingFirstCoinbase)?;
    let coinbase = coinbase_fields(first)?;
    for index in 1..view.transaction_count() {
        if is_coinbase(view.raw_transaction(index).expect("parsed transaction"))? {
            return Err(WitnessBindingError::AdditionalCoinbase { index });
        }
    }
    let ordered = fresh.ordered_preindex().ordered();
    let has_witness = ordered.iter().any(|tx| tx.observation.has_witness);
    let commitment = if expected.rules == BitcoinWitnessRules::SegwitActive {
        coinbase.commitment
    } else {
        None
    };
    let mut root = None;
    let mut reserved = None;
    if let Some((_, claimed)) = commitment {
        let nonce = coinbase
            .reserved
            .ok_or(WitnessBindingError::InvalidReservedValue)?;
        let mut wtxids = Vec::with_capacity(ordered.len());
        wtxids.push([0; 32]); // BIP141: the coinbase wtxid leaf is always zero.
        wtxids.extend(ordered.iter().skip(1).map(|tx| tx.observation.wtxid));
        let witness_root = merkle_root_unverified(&wtxids)?;
        let mut preimage = [0u8; 64];
        preimage[..32].copy_from_slice(&witness_root);
        preimage[32..].copy_from_slice(&nonce);
        if double_sha256(&preimage) != claimed {
            return Err(WitnessBindingError::CommitmentMismatch);
        }
        root = Some(witness_root);
        reserved = Some(nonce);
    } else if has_witness {
        return Err(WitnessBindingError::UnexpectedWitness);
    }
    Ok(BitcoinWitnessRelation {
        view,
        context: expected,
        selected_commitment_vout: commitment.map(|(index, _)| index),
        witness_root_raw: root,
        reserved_value: reserved,
    })
}
