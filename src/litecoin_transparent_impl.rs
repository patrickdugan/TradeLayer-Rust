// Included under read_only::litecoin_transparent. This opt-in structural
// profile does not change the Bitcoin/default transaction or block parser.
use std::ops::Range;

pub const MAX_LITECOIN_TRANSPARENT_BLOCK_BYTES: usize = 4_000_256;
pub const MAX_LITECOIN_MWEB_STRUCTURE_BLOCK_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LitecoinTransparentProfile {
    /// Pinned Core beae01d serialization, transparent/HogEx transactions and
    /// an absent or present MWEB block with an explicitly empty body only.
    Core021EmptyMwebV1,
    /// Full bounded known-feature C++ MWEB framing, with distinct IDs.
    Core021MwebStructureV2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LitecoinTransparentError {
    Syntax(ScanError),
    OversizedBlock,
    InvalidTransactionCount,
    UnsupportedOptionalFlags(u8),
    UnsupportedMwebOnlyTransaction,
    UnsupportedNonNullMwebTransaction,
    UnsupportedNonemptyMwebBody,
    UnsupportedMwebFeatures { object: &'static str, features: u8 },
    InvalidMwebTransaction,
    NonCanonicalOptionalPresence(u8),
    InvalidMwebHeaderVarint,
    HogExNotFinal,
    TrailingBytes,
    RequestedBlockIdMismatch,
    InvalidTransparentMerkleTree,
    TransparentMerkleRootMismatch,
    TransparentWeightExceeded,
}
impl From<ScanError> for LitecoinTransparentError {
    fn from(value: ScanError) -> Self {
        Self::Syntax(value)
    }
}
impl fmt::Display for LitecoinTransparentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LitecoinTransparentError {}
type Result<T> = std::result::Result<T, LitecoinTransparentError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LitecoinTransparentTrustError {
    ChainScriptsUtxosAndMwebUnverified,
}

#[derive(Debug, Clone)]
pub struct UnverifiedLitecoinTransparentTransaction<'a> {
    raw: &'a [u8],
    layout: TransactionLayout,
    witness_end: usize,
    hogex: bool,
    mweb: Option<UnverifiedMwebTransaction>,
}
impl<'a> UnverifiedLitecoinTransparentTransaction<'a> {
    /// Exact standalone transaction; mandatory explicit serialization profile.
    pub fn parse(raw: &'a [u8], profile: LitecoinTransparentProfile) -> Result<Self> {
        if raw.len() > MAX_RAW_TRANSACTION {
            return Err(ScanError::TooLong.into());
        }
        let transaction = litecoin_transaction_prefix(raw, profile)?;
        if transaction.raw.len() != raw.len() {
            return Err(LitecoinTransparentError::TrailingBytes);
        }
        Ok(transaction)
    }
    pub fn raw_bytes(&self) -> &'a [u8] {
        self.raw
    }
    pub const fn is_hogex(&self) -> bool {
        self.hogex
    }
    pub const fn stripped_bytes(&self) -> usize {
        self.layout.stripped_bytes
    }
    /// SERIALIZE_NO_MWEB length, retaining genuine witness data only.
    pub fn transparent_bytes(&self) -> usize {
        if self.layout.has_witness {
            10 + self.witness_end - self.layout.input_start
        } else {
            self.stripped_bytes()
        }
    }
    pub fn inspect(&self) -> TransactionObservation {
        // Core ComputeHash uses NO_WITNESS|NO_MWEB for transparent TXIDs.
        // ComputeWitnessHash excludes MWEB and returns TXID without witnesses.
        let txid = double_sha256_parts(&[
            &self.raw[..4],
            &self.raw[self.layout.input_start..self.layout.outputs_end],
            &self.raw[self.raw.len() - 4..],
        ]);
        let wtxid = if self.layout.has_witness {
            double_sha256_parts(&[
                &self.raw[..4],
                &[0, 1],
                &self.raw[self.layout.input_start..self.witness_end],
                &self.raw[self.raw.len() - 4..],
            ])
        } else {
            txid
        };
        TransactionObservation {
            txid,
            wtxid,
            has_witness: self.layout.has_witness,
            input_count: self.layout.input_count,
            output_count: self.layout.output_count,
            op_return_count: self.layout.op_return_count,
            carriers: self
                .layout
                .payloads
                .iter()
                .filter_map(|(vout, range)| classify(*vout, &self.raw[range.clone()]))
                .collect(),
        }
    }
}

fn litecoin_envelope_prefix(
    raw: &[u8],
    profile: LitecoinTransparentProfile,
) -> Result<UnverifiedLitecoinTransparentTransaction<'_>> {
    let full = profile == LitecoinTransparentProfile::Core021MwebStructureV2;
    let mut reader = Cursor::new(raw);
    reader.read(4)?;
    let flags = if raw.get(4) == Some(&0) {
        reader.byte()?;
        let flags = reader.byte()?;
        if !matches!(flags, 1 | 8 | 9) {
            return Err(LitecoinTransparentError::UnsupportedOptionalFlags(flags));
        }
        flags
    } else {
        0
    };
    let input_start = reader.offset;
    // Pure MWEB-only objects have a different kernel-derived identifier.
    // Never issue a SHA256d transparent TXID for that unsupported object.
    let input_count = reader.varint()?;
    if input_count == 0 && !full {
        return Err(LitecoinTransparentError::UnsupportedMwebOnlyTransaction);
    }
    if input_count > MAX_IO_COUNT {
        return Err(ScanError::TooLong.into());
    }
    let first_outpoint_start = reader.offset;
    for _ in 0..input_count {
        reader.read(36)?;
        reader.variable_bytes()?;
        reader.read(4)?;
    }
    let output_count = if full {
        mweb_count(&mut reader)?
    } else {
        reader.limited_count()?
    };
    let mut payloads = vec![];
    let mut op_return_count = 0;
    for n in 0..output_count {
        reader.read(8)?;
        let script = reader.variable_bytes()?;
        if script.first() == Some(&0x6a) {
            op_return_count += 1;
            if let Some(payload) = one_op_return_push(script) {
                if payload.starts_with(b"tl") || payload.starts_with(b"TLR") {
                    let start = reader.offset - payload.len();
                    payloads.push((n as u32, start..reader.offset));
                }
            }
        }
    }
    let outputs_end = reader.offset;
    let has_witness = flags & 1 != 0;
    if has_witness {
        let mut nonempty = false;
        for _ in 0..input_count {
            let count = reader.varint()?;
            if count > MAX_IO_COUNT {
                return Err(ScanError::TooLong.into());
            }
            nonempty |= count != 0;
            for _ in 0..count {
                reader.variable_bytes()?;
            }
        }
        if !nonempty {
            return Err(ScanError::Invalid("superfluous SegWit witness marker".into()).into());
        }
    }
    let witness_end = reader.offset;
    let mut hogex = false;
    let mut mweb = None;
    if flags & 8 != 0 {
        match reader.byte()? {
            0 if output_count != 0 => hogex = true,
            0 => return Err(LitecoinTransparentError::InvalidMwebTransaction),
            1 if full => mweb = Some(mweb_transaction(&mut reader)?),
            1 => return Err(LitecoinTransparentError::UnsupportedNonNullMwebTransaction),
            v => return Err(LitecoinTransparentError::NonCanonicalOptionalPresence(v)),
        }
    }
    reader.read(4)?;
    let total_bytes = reader.offset;
    if total_bytes > MAX_RAW_TRANSACTION {
        return Err(ScanError::TooLong.into());
    }
    Ok(UnverifiedLitecoinTransparentTransaction {
        raw: &raw[..total_bytes],
        layout: TransactionLayout {
            total_bytes,
            stripped_bytes: 8 + outputs_end - input_start,
            input_start,
            first_outpoint_start,
            outputs_end,
            has_witness,
            input_count: input_count as u32,
            output_count: output_count as u32,
            op_return_count,
            payloads,
        },
        witness_end,
        hogex,
        mweb,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedEmptyMwebHeader {
    pub height: u32,
    pub output_root: [u8; 32],
    pub kernel_root: [u8; 32],
    pub leafset_root: [u8; 32],
    pub kernel_offset: [u8; 32],
    pub stealth_offset: [u8; 32],
    pub output_mmr_size: u64,
    pub kernel_mmr_size: u64,
}
// Core VARINT is MSB base128 with continuation +1, NOT CompactSize/LEB128.
fn litecoin_header_varint(reader: &mut Cursor<'_>, max: u64) -> Result<u64> {
    let mut n = 0u64;
    loop {
        let b = reader.byte()?;
        if n > max >> 7 {
            return Err(LitecoinTransparentError::InvalidMwebHeaderVarint);
        }
        n = (n << 7) | u64::from(b & 127);
        if n > max {
            return Err(LitecoinTransparentError::InvalidMwebHeaderVarint);
        }
        if b & 128 == 0 {
            return Ok(n);
        }
        n = n
            .checked_add(1)
            .filter(|n| *n <= max)
            .ok_or(LitecoinTransparentError::InvalidMwebHeaderVarint)?;
    }
}
fn litecoin_mweb(
    reader: &mut Cursor<'_>,
    profile: LitecoinTransparentProfile,
) -> Result<(
    Option<UnverifiedEmptyMwebHeader>,
    Option<UnverifiedMwebBody>,
)> {
    match reader.byte()? {
        0 => return Ok((None, None)),
        1 => {}
        v => return Err(LitecoinTransparentError::NonCanonicalOptionalPresence(v)),
    }
    let height = litecoin_header_varint(reader, i32::MAX as u64)? as u32;
    let mut fixed =
        || -> Result<[u8; 32]> { Ok(reader.read(32)?.try_into().expect("32-byte field")) };
    let (output_root, kernel_root, leafset_root, kernel_offset, stealth_offset) =
        (fixed()?, fixed()?, fixed()?, fixed()?, fixed()?);
    let output_mmr_size = litecoin_header_varint(reader, u64::MAX)?;
    let kernel_mmr_size = litecoin_header_varint(reader, u64::MAX)?;
    let body = if profile == LitecoinTransparentProfile::Core021MwebStructureV2 {
        Some(mweb_body(reader)?)
    } else {
        for _ in 0..3 {
            if reader.varint()? != 0 {
                return Err(LitecoinTransparentError::UnsupportedNonemptyMwebBody);
            }
        }
        None
    };
    Ok((
        Some(UnverifiedEmptyMwebHeader {
            height,
            output_root,
            kernel_root,
            leafset_root,
            kernel_offset,
            stealth_offset,
            output_mmr_size,
            kernel_mmr_size,
        }),
        body,
    ))
}

#[derive(Debug, Clone)]
pub struct UnverifiedLitecoinTransparentBlock<'a> {
    raw: &'a [u8],
    profile: LitecoinTransparentProfile,
    block_id: [u8; 32],
    merkle_root: [u8; 32],
    transaction_ranges: Vec<Range<usize>>,
    transactions: Vec<UnverifiedLitecoinTransparentTransaction<'a>>,
    extension_range: Option<Range<usize>>,
    mweb_header: Option<UnverifiedEmptyMwebHeader>,
    mweb_body: Option<UnverifiedMwebBody>,
    transparent_weight: u64,
}
impl<'a> UnverifiedLitecoinTransparentBlock<'a> {
    pub fn parse(
        raw: &'a [u8],
        profile: LitecoinTransparentProfile,
        expected_block_id_raw: Option<[u8; 32]>,
    ) -> Result<Self> {
        let limit = if profile == LitecoinTransparentProfile::Core021MwebStructureV2 {
            MAX_LITECOIN_MWEB_STRUCTURE_BLOCK_BYTES
        } else {
            MAX_LITECOIN_TRANSPARENT_BLOCK_BYTES
        };
        if raw.len() > limit {
            return Err(LitecoinTransparentError::OversizedBlock);
        }
        let mut reader = Cursor::new(raw);
        let header = reader.read(80)?;
        let block_id = double_sha256(header);
        if expected_block_id_raw.is_some_and(|v| v != block_id) {
            return Err(LitecoinTransparentError::RequestedBlockIdMismatch);
        }
        let merkle_root = header[36..68].try_into().expect("header checked");
        let count = reader.varint()?;
        if count == 0 || count > crate::parallel_txindex::MAX_BLOCK_TRANSACTIONS as u64 {
            return Err(LitecoinTransparentError::InvalidTransactionCount);
        }
        let mut base = reader.offset;
        let mut transparent = reader.offset;
        let mut transactions = Vec::with_capacity(count as usize);
        let mut transaction_ranges = Vec::with_capacity(count as usize);
        let mut txids = Vec::with_capacity(count as usize);
        let mut distinct_txids = BTreeSet::new();
        for index in 0..count {
            let start = reader.offset;
            let tx = litecoin_transaction_prefix(&raw[start..], profile)?;
            if tx.hogex && (index + 1 != count || count < 2) {
                return Err(LitecoinTransparentError::HogExNotFinal);
            }
            reader.read(tx.raw.len())?;
            transaction_ranges.push(start..reader.offset);
            base = base
                .checked_add(tx.stripped_bytes())
                .ok_or(ScanError::TooLong)?;
            transparent = transparent
                .checked_add(tx.transparent_bytes())
                .ok_or(ScanError::TooLong)?;
            let txid = tx.inspect().txid;
            if !distinct_txids.insert(txid) {
                return Err(LitecoinTransparentError::InvalidTransparentMerkleTree);
            }
            txids.push(txid);
            transactions.push(tx);
        }
        let extension_start = reader.offset;
        let (extension_range, mweb_header, mweb_body) =
            if transactions.last().is_some_and(|tx| tx.hogex) {
                let (header, body) = litecoin_mweb(&mut reader, profile)?;
                (Some(extension_start..reader.offset), header, body)
            } else {
                (None, None, None)
            };
        if reader.offset != raw.len() {
            return Err(LitecoinTransparentError::TrailingBytes);
        }
        let actual = crate::raw_block_ingest::merkle_root_unverified(&txids)
            .map_err(|_| LitecoinTransparentError::InvalidTransparentMerkleTree)?;
        if actual != merkle_root {
            return Err(LitecoinTransparentError::TransparentMerkleRootMismatch);
        }
        let transparent_weight = (base as u64)
            .checked_mul(3)
            .and_then(|v| v.checked_add(transparent as u64))
            .ok_or(LitecoinTransparentError::TransparentWeightExceeded)?;
        if transparent_weight > 4_000_000 {
            return Err(LitecoinTransparentError::TransparentWeightExceeded);
        }
        Ok(Self {
            raw,
            profile,
            block_id,
            merkle_root,
            transaction_ranges,
            transactions,
            extension_range,
            mweb_header,
            mweb_body,
            transparent_weight,
        })
    }
    pub const fn profile(&self) -> LitecoinTransparentProfile {
        self.profile
    }
    pub fn raw_bytes(&self) -> &'a [u8] {
        self.raw
    }
    /// Litecoin's GetHash/block identifier is SHA256d, separate from Scrypt PoW.
    pub const fn block_id_raw(&self) -> [u8; 32] {
        self.block_id
    }
    pub const fn transparent_merkle_root_raw(&self) -> [u8; 32] {
        self.merkle_root
    }
    pub fn transaction_ranges(&self) -> &[Range<usize>] {
        &self.transaction_ranges
    }
    pub fn transactions(&self) -> &[UnverifiedLitecoinTransparentTransaction<'a>] {
        &self.transactions
    }
    pub fn extension_range(&self) -> Option<Range<usize>> {
        self.extension_range.clone()
    }
    pub fn mweb_header(&self) -> Option<&UnverifiedEmptyMwebHeader> {
        self.mweb_header.as_ref()
    }
    pub const fn transparent_weight(&self) -> u64 {
        self.transparent_weight
    }
    pub const fn require_full_litecoin_consensus(
        &self,
    ) -> std::result::Result<(), LitecoinTransparentTrustError> {
        Err(LitecoinTransparentTrustError::ChainScriptsUtxosAndMwebUnverified)
    }
}

fn litecoin_transaction_prefix(
    raw: &[u8],
    profile: LitecoinTransparentProfile,
) -> Result<UnverifiedLitecoinTransparentTransaction<'_>> {
    let tx = litecoin_envelope_prefix(raw, profile)?;
    if tx.is_mweb_only() {
        return Err(LitecoinTransparentError::UnsupportedMwebOnlyTransaction);
    }
    Ok(tx)
}
impl UnverifiedLitecoinTransparentTransaction<'_> {
    fn is_mweb_only(&self) -> bool {
        self.layout.input_count == 0 && self.layout.output_count == 0 && self.mweb.is_some()
    }
    pub fn mweb_transaction(&self) -> Option<&UnverifiedMwebTransaction> {
        self.mweb.as_ref()
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let mut out = self.raw[..self.witness_end].to_vec();
        if self.hogex {
            out.push(0);
        } else if let Some(m) = &self.mweb {
            out.push(1);
            out.extend(m.canonical_bytes(self.raw)?);
        }
        out.extend(&self.raw[self.raw.len() - 4..]);
        Ok(out)
    }
}
impl UnverifiedLitecoinTransparentBlock<'_> {
    /// A source-compatible fingerprint, not proof of MWEB validity.
    pub fn mweb_header_id_raw(&self) -> Option<[u8; 32]> {
        let extension = self.extension_range.as_ref()?;
        self.mweb_header.as_ref()?;
        let end = self
            .mweb_body
            .as_ref()
            .map(|b| b.range.start)
            .unwrap_or(extension.end - 3);
        Some(crate::blake3::blake3_256(
            &self.raw[extension.start + 1..end],
        ))
    }
    pub fn mweb_body(&self) -> Option<&UnverifiedMwebBody> {
        self.mweb_body.as_ref()
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let mut out = self.raw[..80].to_vec();
        mweb_write_size(&mut out, self.transactions.len());
        for tx in &self.transactions {
            out.extend(tx.canonical_bytes()?);
        }
        if let Some(range) = &self.extension_range {
            if let Some(body) = &self.mweb_body {
                out.extend(&self.raw[range.start..body.range.start]);
                out.extend(body.canonical_bytes(self.raw)?);
            } else {
                out.extend(&self.raw[range.clone()]);
            }
        }
        Ok(out)
    }
}
include!("litecoin_mweb_structure_impl.rs");
