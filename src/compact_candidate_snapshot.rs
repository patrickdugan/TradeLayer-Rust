//! Read-only native candidate snapshots. Repeated raw-block discovery proves
//! only the complete structural carrier set for these supplied bytes. It
//! grants no active-chain, script, actor or TradeLayer authority.
use crate::hash256::double_sha256;
use crate::raw_block_ingest::UnverifiedRawBlockView;
use crate::read_only::{decode_hex, CarrierFormat};
use std::fs::{self, File};
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SEGMENT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_MANIFEST_BLOCKS: usize = 200_000;
pub const MAX_CARRIERS_PER_BLOCK: usize = 20_000;
pub const MAX_RAW_BLOCK_BYTES: usize = 4_000_000;
pub const MAX_RAW_TRANSACTION_BYTES: usize = 1_000_000;
const MANIFEST: &str = "TLCAND-MANIFEST-1";
const SEGMENT: &str = "TLCAND-SEGMENT-1";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactCandidateProfile {
    NativeV1BitcoinCanonicalAllOutputs,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateNetwork {
    Main,
    Test,
    Testnet4,
    Signet,
    Regtest,
}
impl CandidateNetwork {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Test => "test",
            Self::Testnet4 => "testnet4",
            Self::Signet => "signet",
            Self::Regtest => "regtest",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSnapshotExpectation {
    pub profile: CompactCandidateProfile,
    pub network: CandidateNetwork,
    pub anchor_height: i64,
    /// Hash bytes in displayed hexadecimal order.
    pub anchor_hash_display: [u8; 32],
    pub generation: u64,
    /// Raw SHA256d bytes of exact CURRENT contents, independently retained.
    pub manifest_sha256d: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSnapshotToken {
    pub expectation: CandidateSnapshotExpectation,
    pub observed_tip_height: i64,
    pub observed_tip_hash_display: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateBlockIdentity {
    pub height: u32,
    pub hash_display: [u8; 32],
    pub parent_display: [u8; 32],
    pub transaction_count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    identity: CandidateBlockIdentity,
    segment_name: String,
    digest: [u8; 32],
}
#[derive(Debug, Clone)]
pub struct CompactCandidateSnapshot {
    root: PathBuf,
    token: CandidateSnapshotToken,
    blocks: Vec<Block>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCandidateOutput {
    pub transaction_position: u32,
    pub txid_display: [u8; 32],
    pub vout: u32,
    pub format: CarrierFormat,
    pub transaction_type: Option<u8>,
    pub syntax_status: &'static str,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructurallyCompleteCandidateBlock {
    snapshot: CandidateSnapshotToken,
    identity: CandidateBlockIdentity,
    raw_block_sha256d: [u8; 32],
    outputs: Vec<ObservedCandidateOutput>,
    raw_block: Vec<u8>,
    transaction_ranges: Vec<Range<usize>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateTrustError {
    ActiveChainAndConsensusUnverified,
    InputScriptsAndActorEvidenceRequired,
    TradeLayerStateUnverified,
}
impl StructurallyCompleteCandidateBlock {
    pub fn snapshot(&self) -> &CandidateSnapshotToken {
        &self.snapshot
    }
    pub fn identity(&self) -> &CandidateBlockIdentity {
        &self.identity
    }
    pub const fn raw_block_sha256d(&self) -> [u8; 32] {
        self.raw_block_sha256d
    }
    pub fn outputs(&self) -> &[ObservedCandidateOutput] {
        &self.outputs
    }
    pub fn raw_transaction(&self, position: u32) -> Option<&[u8]> {
        self.transaction_ranges
            .get(position as usize)
            .and_then(|r| self.raw_block.get(r.clone()))
    }
    pub const fn require_authenticated_chain(
        &self,
    ) -> std::result::Result<(), CandidateTrustError> {
        Err(CandidateTrustError::ActiveChainAndConsensusUnverified)
    }
    pub const fn require_authenticated_actor(
        &self,
    ) -> std::result::Result<(), CandidateTrustError> {
        Err(CandidateTrustError::InputScriptsAndActorEvidenceRequired)
    }
    pub const fn require_tradelayer_transition(
        &self,
    ) -> std::result::Result<(), CandidateTrustError> {
        Err(CandidateTrustError::TradeLayerStateUnverified)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSnapshotError {
    Io,
    UnsafePath,
    Limit,
    InvalidManifest,
    WrongContext,
    StaleSnapshot,
    MissingHeight,
    InvalidSegment,
    SegmentDigestMismatch,
    RawBlockMismatch,
    CandidateSetMismatch,
    RawTransactionMismatch,
    ClassificationMismatch,
}
impl std::fmt::Display for CandidateSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CandidateSnapshotError {}
type Result<T> = std::result::Result<T, CandidateSnapshotError>;
fn fields<const N: usize>(line: &str) -> Result<[&str; N]> {
    let mut parts = line.split('\t');
    let values = (0..N)
        .map(|_| parts.next().ok_or(CandidateSnapshotError::InvalidManifest))
        .collect::<Result<Vec<_>>>()?;
    if parts.next().is_some() {
        return Err(CandidateSnapshotError::InvalidManifest);
    }
    values
        .try_into()
        .map_err(|_| CandidateSnapshotError::InvalidManifest)
}
fn number(v: &str, max: u64) -> Result<u64> {
    if v.is_empty()
        || v.len() > 20
        || !v.bytes().all(|b| b.is_ascii_digit())
        || (v.len() > 1 && v.starts_with('0'))
    {
        return Err(CandidateSnapshotError::InvalidManifest);
    }
    v.parse::<u64>()
        .ok()
        .filter(|n| *n <= max)
        .ok_or(CandidateSnapshotError::InvalidManifest)
}
fn hash(v: &str) -> Result<[u8; 32]> {
    if v.len() != 64
        || !v
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(CandidateSnapshotError::InvalidManifest);
    }
    decode_hex(v)
        .map_err(|_| CandidateSnapshotError::InvalidManifest)?
        .try_into()
        .map_err(|_| CandidateSnapshotError::InvalidManifest)
}
fn hex(v: &[u8; 32]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
fn canonical_text(v: &[u8]) -> Result<&str> {
    if v.is_empty()
        || v.last() != Some(&b'\n')
        || !v.iter().all(|b| matches!(b, b'\n' | b'\t' | b' '..=b'~'))
    {
        return Err(CandidateSnapshotError::InvalidManifest);
    }
    std::str::from_utf8(v).map_err(|_| CandidateSnapshotError::InvalidManifest)
}
fn ordinary_directory(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(|_| CandidateSnapshotError::Io)?;
    if m.file_type().is_symlink() || !m.is_dir() {
        return Err(CandidateSnapshotError::UnsafePath);
    }
    Ok(())
}
fn read_file(path: &Path, parent: &Path, limit: usize) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).map_err(|_| CandidateSnapshotError::Io)?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(CandidateSnapshotError::UnsafePath);
    }
    let resolved = fs::canonicalize(path).map_err(|_| CandidateSnapshotError::Io)?;
    if resolved.parent() != Some(parent) {
        return Err(CandidateSnapshotError::UnsafePath);
    }
    let file = File::open(path).map_err(|_| CandidateSnapshotError::Io)?;
    let m = file.metadata().map_err(|_| CandidateSnapshotError::Io)?;
    if !m.is_file() || m.len() > limit as u64 {
        return Err(CandidateSnapshotError::Limit);
    }
    let mut v = Vec::with_capacity(m.len() as usize);
    file.take(limit as u64 + 1)
        .read_to_end(&mut v)
        .map_err(|_| CandidateSnapshotError::Io)?;
    if v.len() > limit {
        return Err(CandidateSnapshotError::Limit);
    }
    let after = fs::symlink_metadata(path).map_err(|_| CandidateSnapshotError::Io)?;
    if after.file_type().is_symlink() || !after.is_file() {
        return Err(CandidateSnapshotError::UnsafePath);
    }
    Ok(v)
}
impl CompactCandidateSnapshot {
    pub fn open(root: &Path, expected: CandidateSnapshotExpectation) -> Result<Self> {
        match expected.profile {
            CompactCandidateProfile::NativeV1BitcoinCanonicalAllOutputs => (),
        }
        ordinary_directory(root)?;
        let root = fs::canonicalize(root).map_err(|_| CandidateSnapshotError::Io)?;
        ordinary_directory(&root.join("segments"))?;
        let dir =
            fs::canonicalize(root.join("segments")).map_err(|_| CandidateSnapshotError::Io)?;
        if dir.parent() != Some(root.as_path()) {
            return Err(CandidateSnapshotError::UnsafePath);
        }
        let raw = read_file(&root.join("CURRENT"), &root, MAX_MANIFEST_BYTES)?;
        if double_sha256(&raw) != expected.manifest_sha256d {
            return Err(CandidateSnapshotError::StaleSnapshot);
        }
        let mut lines = canonical_text(&raw)?.lines();
        if lines.next() != Some(MANIFEST) {
            return Err(CandidateSnapshotError::InvalidManifest);
        }
        let network = fields::<2>(
            lines
                .next()
                .ok_or(CandidateSnapshotError::InvalidManifest)?,
        )?;
        let anchor = fields::<3>(
            lines
                .next()
                .ok_or(CandidateSnapshotError::InvalidManifest)?,
        )?;
        let generation = fields::<2>(
            lines
                .next()
                .ok_or(CandidateSnapshotError::InvalidManifest)?,
        )?;
        if network[0] != "NETWORK" || anchor[0] != "ANCHOR" || generation[0] != "GEN" {
            return Err(CandidateSnapshotError::InvalidManifest);
        }
        let anchor_height = if anchor[1] == "-1" {
            -1
        } else {
            number(anchor[1], u32::MAX as u64)? as i64
        };
        let anchor_hash = hash(anchor[2])?;
        if anchor_height == -1 && anchor_hash != [0; 32] {
            return Err(CandidateSnapshotError::InvalidManifest);
        }
        if network[1] != expected.network.label()
            || anchor_height != expected.anchor_height
            || anchor_hash != expected.anchor_hash_display
            || number(generation[1], u64::MAX)? != expected.generation
        {
            return Err(CandidateSnapshotError::WrongContext);
        }
        let mut blocks = vec![];
        let (mut previous_height, mut previous_hash) = (anchor_height, anchor_hash);
        let mut ended = false;
        for line in lines {
            if ended || line.len() > 1024 {
                return Err(CandidateSnapshotError::InvalidManifest);
            }
            if line.starts_with("END\t") {
                let f = fields::<2>(line)?;
                if number(f[1], MAX_MANIFEST_BLOCKS as u64)? != blocks.len() as u64 {
                    return Err(CandidateSnapshotError::InvalidManifest);
                }
                ended = true;
                continue;
            }
            if blocks.len() >= MAX_MANIFEST_BLOCKS {
                return Err(CandidateSnapshotError::Limit);
            }
            let f = fields::<7>(line)?;
            if f[0] != "B" {
                return Err(CandidateSnapshotError::InvalidManifest);
            }
            let height = number(f[1], u32::MAX as u64)? as u32;
            let blockhash = hash(f[2])?;
            let parent = hash(f[3])?;
            let count =
                number(f[4], crate::parallel_txindex::MAX_BLOCK_TRANSACTIONS as u64)? as u32;
            let digest = hash(f[6])?;
            let name = format!("seg-{height:010}-{}.dat", hex(&digest));
            if i64::from(height) != previous_height + 1
                || parent != previous_hash
                || count == 0
                || f[5] != name
            {
                return Err(CandidateSnapshotError::InvalidManifest);
            }
            blocks.push(Block {
                identity: CandidateBlockIdentity {
                    height,
                    hash_display: blockhash,
                    parent_display: parent,
                    transaction_count: count,
                },
                segment_name: name,
                digest,
            });
            previous_height = i64::from(height);
            previous_hash = blockhash;
        }
        if !ended {
            return Err(CandidateSnapshotError::InvalidManifest);
        }
        Ok(Self {
            root,
            token: CandidateSnapshotToken {
                expectation: expected,
                observed_tip_height: previous_height,
                observed_tip_hash_display: previous_hash,
            },
            blocks,
        })
    }
    pub fn token(&self) -> &CandidateSnapshotToken {
        &self.token
    }
    pub fn block_identities(&self) -> impl Iterator<Item = &CandidateBlockIdentity> {
        self.blocks.iter().map(|b| &b.identity)
    }
    /// Detect changed publication, even an A-B-A tip when GEN has advanced.
    pub fn ensure_current(&self) -> Result<()> {
        let bytes = read_file(&self.root.join("CURRENT"), &self.root, MAX_MANIFEST_BYTES)?;
        if double_sha256(&bytes) != self.token.expectation.manifest_sha256d {
            return Err(CandidateSnapshotError::StaleSnapshot);
        }
        Ok(())
    }
    /// Raw block bytes are mandatory, including an allegedly empty segment.
    /// Reads are checked against the same publication before and after discovery.
    pub fn read_block(
        &self,
        height: u32,
        raw_block: &[u8],
    ) -> Result<StructurallyCompleteCandidateBlock> {
        self.ensure_current()?;
        if raw_block.len() > MAX_RAW_BLOCK_BYTES {
            return Err(CandidateSnapshotError::Limit);
        }
        let i = self
            .blocks
            .binary_search_by_key(&height, |b| b.identity.height)
            .map_err(|_| CandidateSnapshotError::MissingHeight)?;
        let block = &self.blocks[i];
        let mut raw_hash = block.identity.hash_display;
        raw_hash.reverse();
        let view = UnverifiedRawBlockView::parse(raw_block, Some(raw_hash))
            .map_err(|_| CandidateSnapshotError::RawBlockMismatch)?;
        let mut parent: [u8; 32] = raw_block[4..36]
            .try_into()
            .map_err(|_| CandidateSnapshotError::RawBlockMismatch)?;
        parent.reverse();
        if parent != block.identity.parent_display
            || view.transaction_count() != block.identity.transaction_count as usize
        {
            return Err(CandidateSnapshotError::RawBlockMismatch);
        }
        let inspected = view
            .inspect_serial()
            .map_err(|_| CandidateSnapshotError::RawBlockMismatch)?;
        let mut outputs = vec![];
        for tx in inspected.ordered_preindex().candidates_in_source_order() {
            let mut display = tx.observation.txid;
            display.reverse();
            for c in &tx.observation.carriers {
                if outputs.len() >= MAX_CARRIERS_PER_BLOCK {
                    return Err(CandidateSnapshotError::Limit);
                }
                outputs.push(ObservedCandidateOutput {
                    transaction_position: tx.position as u32,
                    txid_display: display,
                    vout: c.vout,
                    format: c.format,
                    transaction_type: c.transaction_type,
                    syntax_status: c.status,
                });
            }
        }
        let dir = self.root.join("segments");
        ordinary_directory(&dir)?;
        let dir = fs::canonicalize(dir).map_err(|_| CandidateSnapshotError::Io)?;
        if dir.parent() != Some(self.root.as_path()) {
            return Err(CandidateSnapshotError::UnsafePath);
        }
        let bytes = read_file(&dir.join(&block.segment_name), &dir, MAX_SEGMENT_BYTES)?;
        if double_sha256(&bytes) != block.digest {
            return Err(CandidateSnapshotError::SegmentDigestMismatch);
        }
        let text = canonical_text(&bytes).map_err(|_| CandidateSnapshotError::InvalidSegment)?;
        let mut lines = text.lines();
        let header = format!(
            "{SEGMENT}\t{}\t{}\t{}\t{}",
            height,
            hex(&block.identity.hash_display),
            hex(&parent),
            block.identity.transaction_count
        );
        if lines.next() != Some(header.as_str()) {
            return Err(CandidateSnapshotError::InvalidSegment);
        }
        let (mut seen, mut ended) = (0, false);
        for line in lines {
            if ended || line.len() > 2 * MAX_RAW_TRANSACTION_BYTES + 512 {
                return Err(CandidateSnapshotError::InvalidSegment);
            }
            if line.starts_with("END\t") {
                let f = fields::<2>(line).map_err(|_| CandidateSnapshotError::InvalidSegment)?;
                if number(f[1], MAX_CARRIERS_PER_BLOCK as u64)? != seen as u64
                    || seen != outputs.len()
                {
                    return Err(CandidateSnapshotError::CandidateSetMismatch);
                }
                ended = true;
                continue;
            }
            let f = fields::<11>(line).map_err(|_| CandidateSnapshotError::InvalidSegment)?;
            let c = outputs
                .get(seen)
                .ok_or(CandidateSnapshotError::CandidateSetMismatch)?;
            if f[0] != "C"
                || f[1] != "BLOCK"
                || number(f[2], u32::MAX as u64)? != u64::from(height)
                || hash(f[3])? != block.identity.hash_display
                || number(f[4], u32::MAX as u64)? != u64::from(c.transaction_position)
                || hash(f[5])? != c.txid_display
                || number(f[6], u32::MAX as u64)? != u64::from(c.vout)
            {
                return Err(CandidateSnapshotError::CandidateSetMismatch);
            }
            if f[7] != c.format.label()
                || f[8]
                    != c.transaction_type
                        .map_or_else(|| "-".into(), |v| v.to_string())
                || f[9] != c.syntax_status
            {
                return Err(CandidateSnapshotError::ClassificationMismatch);
            }
            if f[10].len() > 2 * MAX_RAW_TRANSACTION_BYTES
                || !f[10]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(CandidateSnapshotError::InvalidSegment);
            }
            let raw = decode_hex(f[10]).map_err(|_| CandidateSnapshotError::InvalidSegment)?;
            if view.raw_transaction(c.transaction_position as usize) != Some(raw.as_slice()) {
                return Err(CandidateSnapshotError::RawTransactionMismatch);
            }
            seen += 1;
        }
        if !ended {
            return Err(CandidateSnapshotError::InvalidSegment);
        }
        self.ensure_current()?;
        Ok(StructurallyCompleteCandidateBlock {
            snapshot: self.token.clone(),
            identity: block.identity.clone(),
            raw_block_sha256d: double_sha256(raw_block),
            outputs,
            transaction_ranges: view.transaction_ranges().to_vec(),
            raw_block: raw_block.to_vec(),
        })
    }
}
