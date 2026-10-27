//! Conditional, read-only completeness checking for PR142 B/E + C/END evidence.
//!
//! A caller independently supplies the expected snapshot descriptor. Matching
//! its token or hashes is never authentication. Every covered raw block is
//! rescanned with the existing parser/Merkle/carrier implementation, including
//! blocks with no carriers. No database, chain selection or execution is owned.

use crate::hash256::sha256;
use crate::raw_block_ingest::{RawBlockIngestError, UnverifiedRawBlockView};
use crate::read_only::Carrier;

pub const PROFILE_ID: &str = "PR142_BLOCKS_CANONICAL_CORE_CARRIER_V1";
pub const MAX_TRANSCRIPT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 8_000_512;
pub const MAX_BLOCKS: usize = 256;
pub const MAX_TRANSACTIONS: usize = 1_000_000;
pub const MAX_CARRIERS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateNetwork {
    Main,
    Test,
    Testnet4,
    Signet,
    Regtest,
}

/// The immediately preceding covered-window checkpoint, not chain authority.
/// Height -1 is allowed only with a zero hash; the first covered height is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateAnchor {
    pub height: i64,
    pub hash_raw: [u8; 32],
}

/// Source/store namespace and the *independently expected* transcript binding.
/// Hashes use SHA256 digest order; header hashes use internal SHA256d order.
/// The opaque publication token must come from the caller's snapshot contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateSnapshotDescriptor {
    pub network: CandidateNetwork,
    pub profile: String,
    pub publication_token: [u8; 32],
    pub anchor: CandidateAnchor,
    pub first_height: u32,
    pub last_height: u32,
    pub last_hash_raw: [u8; 32],
    pub block_transcript_sha256: [u8; 32],
    pub candidate_transcript_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateField {
    Height,
    BlockHash,
    TransactionPosition,
    Txid,
    Vout,
    Format,
    TransactionType,
    SyntaxStatus,
    RawTransaction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateEvidenceError {
    DescriptorMismatch,
    UnsupportedProfile,
    InvalidAnchorOrRange,
    Capacity,
    TranscriptDigestMismatch,
    NoncanonicalFrame,
    NoncanonicalNumber,
    NoncanonicalHex,
    UnexpectedBlockFrame,
    UnexpectedCandidateFrame,
    MissingBlockCompletion,
    MissingCandidateCompletion,
    TrailingFrame,
    BlockRangeMismatch,
    BlockParentMismatch,
    BlockTipMismatch,
    BlockCompletionMismatch,
    CandidateCompletionMismatch,
    MissingCandidate,
    ExtraCandidate,
    CandidateMismatch {
        row: usize,
        field: CandidateField,
    },
    RawBlock {
        height: u32,
        error: RawBlockIngestError,
    },
}

type Result<T> = std::result::Result<T, CandidateEvidenceError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateTransactionEvidence {
    position: u32,
    txid_raw: [u8; 32],
    raw: Vec<u8>,
    carriers: Vec<Carrier>,
}
impl CandidateTransactionEvidence {
    pub const fn position(&self) -> u32 {
        self.position
    }
    pub const fn txid_raw(&self) -> [u8; 32] {
        self.txid_raw
    }
    pub fn raw_transaction(&self) -> &[u8] {
        &self.raw
    }
    pub fn carriers(&self) -> &[Carrier] {
        &self.carriers
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateBlockEvidence {
    height: u32,
    hash_raw: [u8; 32],
    parent_raw: [u8; 32],
    transaction_count: u32,
    candidate_transactions: Vec<CandidateTransactionEvidence>,
}
impl CandidateBlockEvidence {
    pub const fn height(&self) -> u32 {
        self.height
    }
    pub const fn hash_raw(&self) -> [u8; 32] {
        self.hash_raw
    }
    pub const fn parent_raw(&self) -> [u8; 32] {
        self.parent_raw
    }
    pub const fn transaction_count(&self) -> u32 {
        self.transaction_count
    }
    /// Only transactions containing a scanner carrier retain their raw bytes.
    pub fn candidate_transactions(&self) -> &[CandidateTransactionEvidence] {
        &self.candidate_transactions
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionalCandidateRow<'a> {
    pub height: u32,
    pub block_hash_raw: [u8; 32],
    pub transaction_position: u32,
    pub txid_raw: [u8; 32],
    pub raw_transaction: &'a [u8],
    pub carrier: &'a Carrier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateEvidenceTrustError {
    ChainworkAndActiveChainUnverified,
    InputScriptsAndUtxoUnverified,
    TradeLayerStateAndActorUnverified,
}

/// Immutable evidence of equality/completeness *conditional on supplied bytes*
/// and an independently expected descriptor. It grants no execution capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateEvidenceReader {
    descriptor: CandidateSnapshotDescriptor,
    blocks: Vec<CandidateBlockEvidence>,
    transaction_count: usize,
    carrier_count: usize,
}
impl CandidateEvidenceReader {
    pub fn verify(
        expected: &CandidateSnapshotDescriptor,
        claimed: &CandidateSnapshotDescriptor,
        block_frames: &[u8],
        candidate_frames: &[u8],
    ) -> Result<Self> {
        if expected != claimed {
            return Err(CandidateEvidenceError::DescriptorMismatch);
        }
        validate_descriptor(expected)?;
        if fingerprint_transcript_unverified(block_frames)? != expected.block_transcript_sha256
            || fingerprint_transcript_unverified(candidate_frames)?
                != expected.candidate_transcript_sha256
        {
            return Err(CandidateEvidenceError::TranscriptDigestMismatch);
        }
        let reader = scan_blocks(expected, block_frames)?;
        reader.check_candidates(candidate_frames)?;
        Ok(reader)
    }
    pub fn descriptor(&self) -> &CandidateSnapshotDescriptor {
        &self.descriptor
    }
    pub fn blocks(&self) -> &[CandidateBlockEvidence] {
        &self.blocks
    }
    pub fn block(&self, height: u32) -> Option<&CandidateBlockEvidence> {
        self.blocks
            .get(height.checked_sub(self.descriptor.first_height)? as usize)
    }
    pub const fn transaction_count(&self) -> usize {
        self.transaction_count
    }
    pub const fn carrier_count(&self) -> usize {
        self.carrier_count
    }
    pub fn rows(&self) -> impl Iterator<Item = ConditionalCandidateRow<'_>> {
        self.blocks.iter().flat_map(|block| {
            block.candidate_transactions.iter().flat_map(move |tx| {
                tx.carriers
                    .iter()
                    .map(move |carrier| ConditionalCandidateRow {
                        height: block.height,
                        block_hash_raw: block.hash_raw,
                        transaction_position: tx.position,
                        txid_raw: tx.txid_raw,
                        raw_transaction: &tx.raw,
                        carrier,
                    })
            })
        })
    }
    pub const fn require_authenticated_chain(
        &self,
    ) -> std::result::Result<(), CandidateEvidenceTrustError> {
        Err(CandidateEvidenceTrustError::ChainworkAndActiveChainUnverified)
    }
    pub const fn require_authenticated_utxos(
        &self,
    ) -> std::result::Result<(), CandidateEvidenceTrustError> {
        Err(CandidateEvidenceTrustError::InputScriptsAndUtxoUnverified)
    }
    pub const fn require_execution_authority(
        &self,
    ) -> std::result::Result<(), CandidateEvidenceTrustError> {
        Err(CandidateEvidenceTrustError::TradeLayerStateAndActorUnverified)
    }

    fn check_candidates(&self, transcript: &[u8]) -> Result<()> {
        let mut frames = Frames::new(transcript);
        let mut wanted = self.rows();
        let mut rows = 0usize;
        while let Some(frame) = frames.next()? {
            let fields = frame.splitn(12, '\t').collect::<Vec<_>>();
            match fields.as_slice() {
                ["C", "BLOCK", height, blockhash, position, txid, vout, format, typ, status, raw] =>
                {
                    let expected = wanted
                        .next()
                        .ok_or(CandidateEvidenceError::ExtraCandidate)?;
                    let mismatch =
                        |field| CandidateEvidenceError::CandidateMismatch { row: rows, field };
                    if unsigned(height)? != u64::from(expected.height) {
                        return Err(mismatch(CandidateField::Height));
                    }
                    if display_hash(blockhash)? != expected.block_hash_raw {
                        return Err(mismatch(CandidateField::BlockHash));
                    }
                    if unsigned(position)? != u64::from(expected.transaction_position) {
                        return Err(mismatch(CandidateField::TransactionPosition));
                    }
                    if display_hash(txid)? != expected.txid_raw {
                        return Err(mismatch(CandidateField::Txid));
                    }
                    if unsigned(vout)? != u64::from(expected.carrier.vout) {
                        return Err(mismatch(CandidateField::Vout));
                    }
                    if *format != expected.carrier.format.label() {
                        return Err(mismatch(CandidateField::Format));
                    }
                    let typ = if *typ == "-" {
                        None
                    } else {
                        Some(unsigned(typ)?)
                    };
                    if typ != expected.carrier.transaction_type.map(u64::from) {
                        return Err(mismatch(CandidateField::TransactionType));
                    }
                    if *status != expected.carrier.status {
                        return Err(mismatch(CandidateField::SyntaxStatus));
                    }
                    if !matches_hex(raw, expected.raw_transaction)? {
                        return Err(mismatch(CandidateField::RawTransaction));
                    }
                    rows += 1;
                }
                ["END", "COMPLETE_UNVERIFIED", "BLOCKS", blocks, txs, carriers] => {
                    if unsigned(blocks)? != self.blocks.len() as u64
                        || unsigned(txs)? != self.transaction_count as u64
                        || unsigned(carriers)? != rows as u64
                    {
                        return Err(CandidateEvidenceError::CandidateCompletionMismatch);
                    }
                    if wanted.next().is_some() {
                        return Err(CandidateEvidenceError::MissingCandidate);
                    }
                    if frames.next()?.is_some() {
                        return Err(CandidateEvidenceError::TrailingFrame);
                    }
                    return Ok(());
                }
                _ => return Err(CandidateEvidenceError::UnexpectedCandidateFrame),
            }
        }
        Err(CandidateEvidenceError::MissingCandidateCompletion)
    }
}

/// Fingerprinting helper only. Choosing the expected value from untrusted input
/// does not authenticate it; completeness still requires the full rescan.
pub fn fingerprint_transcript_unverified(bytes: &[u8]) -> Result<[u8; 32]> {
    if bytes.len() > MAX_TRANSCRIPT_BYTES {
        return Err(CandidateEvidenceError::Capacity);
    }
    Ok(sha256(bytes))
}

fn validate_descriptor(descriptor: &CandidateSnapshotDescriptor) -> Result<()> {
    if descriptor.profile != PROFILE_ID {
        return Err(CandidateEvidenceError::UnsupportedProfile);
    }
    let anchor = descriptor.anchor;
    if anchor.height < -1
        || anchor.height >= i64::from(u32::MAX)
        || (anchor.height == -1 && anchor.hash_raw != [0; 32])
        || i64::from(descriptor.first_height) != anchor.height + 1
        || descriptor.last_height < descriptor.first_height
    {
        return Err(CandidateEvidenceError::InvalidAnchorOrRange);
    }
    let count = u64::from(descriptor.last_height) - u64::from(descriptor.first_height) + 1;
    if count > MAX_BLOCKS as u64 {
        return Err(CandidateEvidenceError::Capacity);
    }
    Ok(())
}

fn scan_blocks(
    descriptor: &CandidateSnapshotDescriptor,
    transcript: &[u8],
) -> Result<CandidateEvidenceReader> {
    let mut reader = CandidateEvidenceReader {
        descriptor: descriptor.clone(),
        blocks: Vec::new(),
        transaction_count: 0,
        carrier_count: 0,
    };
    let mut frames = Frames::new(transcript);
    let mut previous_height = descriptor.anchor.height;
    let mut previous_hash = descriptor.anchor.hash_raw;
    while let Some(frame) = frames.next()? {
        let fields = frame.splitn(5, '\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["B", height, advertised_hash, raw] => {
                if reader.blocks.len() >= MAX_BLOCKS {
                    return Err(CandidateEvidenceError::Capacity);
                }
                let height = u32::try_from(unsigned(height)?)
                    .map_err(|_| CandidateEvidenceError::NoncanonicalNumber)?;
                if i64::from(height) != previous_height + 1 || height > descriptor.last_height {
                    return Err(CandidateEvidenceError::BlockRangeMismatch);
                }
                let advertised_hash = display_hash(advertised_hash)?;
                let raw = decode_block_hex(raw)?;
                let view = UnverifiedRawBlockView::parse(&raw, Some(advertised_hash))
                    .map_err(|error| CandidateEvidenceError::RawBlock { height, error })?;
                let parent: [u8; 32] = raw[4..36].try_into().expect("checked raw header");
                if parent != previous_hash {
                    return Err(CandidateEvidenceError::BlockParentMismatch);
                }
                let report = view
                    .inspect_serial()
                    .map_err(|error| CandidateEvidenceError::RawBlock { height, error })?;
                reader.transaction_count = reader
                    .transaction_count
                    .checked_add(view.transaction_count())
                    .ok_or(CandidateEvidenceError::Capacity)?;
                if reader.transaction_count > MAX_TRANSACTIONS {
                    return Err(CandidateEvidenceError::Capacity);
                }
                let mut candidates = Vec::new();
                for tx in report.ordered_preindex().candidates_in_source_order() {
                    reader.carrier_count = reader
                        .carrier_count
                        .checked_add(tx.observation.carriers.len())
                        .ok_or(CandidateEvidenceError::Capacity)?;
                    if reader.carrier_count > MAX_CARRIERS {
                        return Err(CandidateEvidenceError::Capacity);
                    }
                    candidates.push(CandidateTransactionEvidence {
                        position: tx.position as u32,
                        txid_raw: tx.observation.txid,
                        raw: view
                            .raw_transaction(tx.position)
                            .expect("shared preindex position")
                            .to_vec(),
                        carriers: tx.observation.carriers.clone(),
                    });
                }
                reader.blocks.push(CandidateBlockEvidence {
                    height,
                    hash_raw: advertised_hash,
                    parent_raw: parent,
                    transaction_count: view.transaction_count() as u32,
                    candidate_transactions: candidates,
                });
                previous_height = i64::from(height);
                previous_hash = advertised_hash;
            }
            ["E", "BLOCKS", count] => {
                if reader.blocks.is_empty() || unsigned(count)? != reader.blocks.len() as u64 {
                    return Err(CandidateEvidenceError::BlockCompletionMismatch);
                }
                if previous_height != i64::from(descriptor.last_height) {
                    return Err(CandidateEvidenceError::BlockRangeMismatch);
                }
                if previous_hash != descriptor.last_hash_raw {
                    return Err(CandidateEvidenceError::BlockTipMismatch);
                }
                if frames.next()?.is_some() {
                    return Err(CandidateEvidenceError::TrailingFrame);
                }
                return Ok(reader);
            }
            _ => return Err(CandidateEvidenceError::UnexpectedBlockFrame),
        }
    }
    Err(CandidateEvidenceError::MissingBlockCompletion)
}

struct Frames<'a> {
    remaining: &'a [u8],
}
impl<'a> Frames<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }
    fn next(&mut self) -> Result<Option<&'a str>> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        // Search only a bounded prefix, before allocation or field splitting.
        let prefix = &self.remaining[..self.remaining.len().min(MAX_FRAME_BYTES + 1)];
        let newline = prefix.iter().position(|b| *b == b'\n').ok_or(
            if self.remaining.len() > MAX_FRAME_BYTES {
                CandidateEvidenceError::Capacity
            } else {
                CandidateEvidenceError::NoncanonicalFrame
            },
        )?;
        if newline + 1 > MAX_FRAME_BYTES {
            return Err(CandidateEvidenceError::Capacity);
        }
        let bytes = &self.remaining[..newline];
        if bytes.is_empty() || !bytes.is_ascii() || bytes.contains(&b'\r') || bytes.contains(&0) {
            return Err(CandidateEvidenceError::NoncanonicalFrame);
        }
        self.remaining = &self.remaining[newline + 1..];
        Ok(Some(std::str::from_utf8(bytes).expect("ASCII checked")))
    }
}
fn unsigned(value: &str) -> Result<u64> {
    if value.is_empty()
        || value.len() > 20
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CandidateEvidenceError::NoncanonicalNumber);
    }
    value
        .parse()
        .map_err(|_| CandidateEvidenceError::NoncanonicalNumber)
}
fn nibble(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(CandidateEvidenceError::NoncanonicalHex),
    }
}
fn display_hash(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        return Err(CandidateEvidenceError::NoncanonicalHex);
    }
    let mut hash = [0u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        hash[31 - index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(hash)
}
fn decode_block_hex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err(CandidateEvidenceError::NoncanonicalHex);
    }
    if value.len() / 2 > crate::raw_block_ingest::MAX_UNVERIFIED_RAW_BLOCK_BYTES {
        return Err(CandidateEvidenceError::Capacity);
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}
fn matches_hex(value: &str, expected: &[u8]) -> Result<bool> {
    if value.len() != expected.len() * 2 {
        return Ok(false);
    }
    for (pair, wanted) in value.as_bytes().as_chunks::<2>().0.iter().zip(expected) {
        if ((nibble(pair[0])? << 4) | nibble(pair[1])?) != *wanted {
            return Ok(false);
        }
    }
    Ok(true)
}
