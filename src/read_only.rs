//! Offline, **untrusted** Bitcoin-style transaction inspection.
//!
//! This module decodes Bitcoin transaction serialization, fingerprints txids,
//! and identifies a single canonical push after OP_RETURN. It cannot prove
//! inclusion, best-chain status, signature validity, input ownership, or TradeLayer
//! authorization. Its observations MUST NOT implement `TrustedChainAdapter`.
//! Historical Litecoin/Dogecoin features beyond this common serialization are
//! not covered by the fixture suite.

use crate::hash256::{double_sha256, double_sha256_parts};
use crate::{wire, wire_v1};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_RAW_TRANSACTION: usize = 1_000_000;
pub const MAX_IO_COUNT: u64 = 100_000;
pub const MAX_CORPUS_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CORPUS_ROWS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    TooLong,
    Truncated,
    Invalid(String),
    NonCanonicalCompactSize,
    InvalidHex,
    TrailingBytes,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ScanError {}

struct Cursor<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn read(&mut self, length: usize) -> Result<&'a [u8], ScanError> {
        let end = self.offset.checked_add(length).ok_or(ScanError::TooLong)?;
        let bytes = self
            .input
            .get(self.offset..end)
            .ok_or(ScanError::Truncated)?;
        self.offset = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, ScanError> {
        Ok(self.read(1)?[0])
    }

    fn varint(&mut self) -> Result<u64, ScanError> {
        let prefix = self.byte()?;
        let number = match prefix {
            0..=252 => u64::from(prefix),
            253 => {
                let number =
                    u64::from(u16::from_le_bytes(self.read(2)?.try_into().expect("slice")));
                if number < 253 {
                    return Err(ScanError::NonCanonicalCompactSize);
                }
                number
            }
            254 => {
                let number =
                    u64::from(u32::from_le_bytes(self.read(4)?.try_into().expect("slice")));
                if number <= u16::MAX as u64 {
                    return Err(ScanError::NonCanonicalCompactSize);
                }
                number
            }
            255 => {
                let number = u64::from_le_bytes(self.read(8)?.try_into().expect("slice"));
                if number <= u32::MAX as u64 {
                    return Err(ScanError::NonCanonicalCompactSize);
                }
                number
            }
        };
        Ok(number)
    }

    fn limited_count(&mut self) -> Result<usize, ScanError> {
        let value = self.varint()?;
        if value == 0 || value > MAX_IO_COUNT {
            return Err(ScanError::Invalid(
                "zero/excessive input or output count".into(),
            ));
        }
        usize::try_from(value).map_err(|_| ScanError::TooLong)
    }

    fn variable_bytes(&mut self) -> Result<&'a [u8], ScanError> {
        let size = self.varint()?;
        if size > MAX_RAW_TRANSACTION as u64 {
            return Err(ScanError::TooLong);
        }
        self.read(usize::try_from(size).map_err(|_| ScanError::TooLong)?)
    }
}

/// Read a canonical Bitcoin CompactSize prefix from a borrowed byte stream.
/// Internal block-ingestion boundary: this is syntax, not consensus authority.
pub(crate) fn compact_size_prefix(raw: &[u8]) -> Result<(u64, usize), ScanError> {
    let mut cursor = Cursor::new(raw);
    let count = cursor.varint()?;
    Ok((count, cursor.offset))
}

/// A single canonical parse over raw TX syntax, storing only the structural
/// offsets needed for later hashing and source-carrier classification.
/// Ordinary non-TL transactions allocate no per-TX payload vectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionLayout {
    total_bytes: usize,
    stripped_bytes: usize,
    input_start: usize,
    first_outpoint_start: usize,
    outputs_end: usize,
    has_witness: bool,
    input_count: u32,
    output_count: u32,
    op_return_count: u32,
    payloads: Vec<(u32, std::ops::Range<usize>)>,
}
impl TransactionLayout {
    pub(crate) const fn total_bytes(&self) -> usize {
        self.total_bytes
    }
    pub(crate) const fn stripped_bytes(&self) -> usize {
        self.stripped_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RawOutpoint {
    pub(crate) txid_raw: [u8; 32],
    pub(crate) vout: u32,
}

/// Extract first-input parent references only when an offline dependency
/// report is requested. This does not prove that external UTXOs exist.
pub(crate) fn source_prevouts_from_layout(
    raw: &[u8],
    layout: &TransactionLayout,
) -> Result<Vec<RawOutpoint>, ScanError> {
    if raw.len() != layout.total_bytes {
        return Err(ScanError::TrailingBytes);
    }
    let mut cursor = Cursor::new(raw);
    cursor.offset = layout.first_outpoint_start;
    let mut out = Vec::with_capacity(layout.input_count as usize);
    for _ in 0..layout.input_count {
        let txid_raw = cursor.read(32)?.try_into().expect("read 32-byte parent");
        let vout = u32::from_le_bytes(cursor.read(4)?.try_into().expect("read u32"));
        out.push(RawOutpoint { txid_raw, vout });
        cursor.variable_bytes()?;
        cursor.read(4)?;
    }
    Ok(out)
}

/// Locate exactly one transaction at the start of a possibly larger block
/// suffix. This strictly parses BIP144 marker, vin/vout, witness and locktime
/// without computing a TXID or copying serialized bytes.
pub(crate) fn prepare_transaction_prefix(raw: &[u8]) -> Result<TransactionLayout, ScanError> {
    let mut reader = Cursor::new(raw);
    reader.read(4)?; // version
    let has_witness = raw.get(4) == Some(&0);
    if has_witness {
        let marker = reader.byte()?;
        let flag = reader.byte()?;
        if marker != 0 || flag != 1 {
            return Err(ScanError::Invalid("unsupported SegWit marker/flag".into()));
        }
    }
    let input_start = reader.offset;
    let input_count = reader.limited_count()?;
    let first_outpoint_start = reader.offset;
    for _ in 0..input_count {
        reader.read(36)?;
        reader.variable_bytes()?;
        reader.read(4)?;
    }
    let output_count = reader.limited_count()?;
    let mut op_return_count = 0u32;
    let mut payloads = Vec::new();
    for n in 0..output_count {
        reader.read(8)?;
        let script = reader.variable_bytes()?;
        if script.first() == Some(&0x6a) {
            op_return_count += 1;
            if let Some(payload) = one_op_return_push(script) {
                if payload.starts_with(b"tl") || payload.starts_with(b"TLR") {
                    let script_start = reader.offset - script.len();
                    let start = script_start + script.len() - payload.len();
                    payloads.push((n as u32, start..reader.offset));
                }
            }
        }
    }
    let outputs_end = reader.offset;
    if has_witness {
        let mut nonempty = false;
        for _ in 0..input_count {
            let count = reader.varint()?;
            if count > MAX_IO_COUNT {
                return Err(ScanError::TooLong);
            }
            // Bitcoin CScriptWitness::IsNull tests whether the stack is
            // empty, not whether any stack item contains nonempty bytes.
            // A one-item stack containing an empty vector is still witness.
            nonempty |= count != 0;
            for _ in 0..count {
                reader.variable_bytes()?;
            }
        }
        if !nonempty {
            return Err(ScanError::Invalid(
                "superfluous SegWit witness marker".into(),
            ));
        }
    }
    reader.read(4)?;
    let total_bytes = reader.offset;
    if total_bytes > MAX_RAW_TRANSACTION {
        return Err(ScanError::TooLong);
    }
    let stripped_bytes = if has_witness {
        8 + outputs_end - input_start
    } else {
        total_bytes
    };
    Ok(TransactionLayout {
        total_bytes,
        stripped_bytes,
        input_start,
        first_outpoint_start,
        outputs_end,
        has_witness,
        input_count: input_count as u32,
        output_count: output_count as u32,
        op_return_count,
        payloads,
    })
}

/// Inspect one actual output directly from a complete serialized Bitcoin
/// transaction; return its borrowed script and amount in satoshis.
///
/// Strict prefix parsing has already walked the entire transaction, including
/// SegWit witness stacks. We separately walk the canonical vin/vout region,
/// checking every output amount and total against Bitcoin MoneyRange. This
/// provides byte-origin consistency for a *claimed* prevout, not UTXO existence,
/// coinbase history, funding inclusion, signature validity or spend authority.
pub fn select_unverified_transaction_output(
    raw: &[u8],
    requested_vout: u32,
) -> Result<Option<(u64, &[u8])>, ScanError> {
    let layout = prepare_transaction_prefix(raw)?;
    if raw.len() != layout.total_bytes {
        return Err(ScanError::TrailingBytes);
    }
    let mut reader = Cursor::new(raw);
    reader.offset = layout.first_outpoint_start;
    for _ in 0..layout.input_count {
        reader.read(36)?;
        reader.variable_bytes()?;
        reader.read(4)?;
    }
    let count = reader.limited_count()?;
    if count != layout.output_count as usize {
        return Err(ScanError::Invalid(
            "output count differs from layout".into(),
        ));
    }
    let mut amount_total = 0u64;
    let mut selected = None;
    for index in 0..count {
        let amount = u64::from_le_bytes(reader.read(8)?.try_into().expect("8-byte amount"));
        amount_total = amount_total
            .checked_add(amount)
            .filter(|total| *total <= 21_000_000 * 100_000_000)
            .ok_or_else(|| ScanError::Invalid("Bitcoin output MoneyRange exceeded".into()))?;
        let script = reader.variable_bytes()?;
        if index == requested_vout as usize {
            selected = Some((amount, script));
        }
    }
    Ok(selected)
}

/// Hash in display order, the reverse of SHA256d digest bytes.
pub fn id_hex(digest: &[u8; 32]) -> String {
    digest
        .iter()
        .rev()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Encode raw bytes for fixture and log inspection.
pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn decode_hex(hex: &str) -> Result<Vec<u8>, ScanError> {
    fn nibble(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    if !hex.len().is_multiple_of(2) || hex.len() > 2 * MAX_RAW_TRANSACTION {
        return Err(ScanError::InvalidHex);
    }
    let mut result = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let high = nibble(pair[0]).ok_or(ScanError::InvalidHex)?;
        let low = nibble(pair[1]).ok_or(ScanError::InvalidHex)?;
        result.push((high << 4) | low);
    }
    Ok(result)
}

/// A single `OP_RETURN <canonical push>` carrying one of the TradeLayer markers.
/// The marker is only a syntactic observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierFormat {
    LegacyAscii,
    ProposedBinaryV1,
}
impl CarrierFormat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::LegacyAscii => "legacy-tl",
            Self::ProposedBinaryV1 => "proposed-TLR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carrier {
    pub vout: u32,
    pub format: CarrierFormat,
    pub payload: Vec<u8>,
    pub transaction_type: Option<u8>,
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionObservation {
    /// Raw digest bytes in hashing order (not explorer display order).
    pub txid: [u8; 32],
    pub wtxid: [u8; 32],
    pub has_witness: bool,
    pub input_count: u32,
    pub output_count: u32,
    pub op_return_count: u32,
    pub carriers: Vec<Carrier>,
}

/// Litecoin MWEB extension data carried under serialization flag 0x08
/// (Litecoin Core src/primitives/transaction.h at ec1b6489).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MwebData {
    None,
    /// Null MWEB::Tx (a single 0x00 byte): the block's integrating HogEx.
    HogEx,
    /// Non-null MWEB payload, kept opaque; only canonical fields are hashed.
    Opaque,
}

/// A spent outpoint; txid in raw hashing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutPoint {
    pub txid: [u8; 32],
    pub vout: u32,
}

impl OutPoint {
    /// The null outpoint that marks a coinbase input.
    pub fn is_coinbase(&self) -> bool {
        self.txid == [0; 32] && self.vout == u32::MAX
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOutput {
    pub value: u64,
    pub script: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitecoinTransactionObservation {
    /// Raw digest bytes in hashing order (not explorer display order).
    /// Excludes witness and MWEB data, as Litecoin Core's GetHash does.
    pub txid: [u8; 32],
    /// Excludes MWEB data, as Litecoin Core's GetWitnessHash does.
    pub wtxid: [u8; 32],
    pub has_witness: bool,
    pub mweb: MwebData,
    pub input_count: u32,
    pub output_count: u32,
    pub op_return_count: u32,
    pub carriers: Vec<Carrier>,
    /// Canonical inputs and outputs exactly as serialized.
    pub inputs: Vec<OutPoint>,
    pub outputs: Vec<TxOutput>,
}

/// Strictly extract one push following OP_RETURN. Multiple pushes, trailing
/// opcodes, and malformed/nonminimal encodings are ignored as TL candidates.
fn one_op_return_push(script: &[u8]) -> Option<&[u8]> {
    if script.first().copied() != Some(0x6a) {
        return None;
    }
    let opcode = *script.get(1)?;
    let (length, start): (usize, usize) = match opcode {
        1..=75 => (usize::from(opcode), 2),
        0x4c => {
            let size = usize::from(*script.get(2)?);
            if size <= 75 {
                return None;
            }
            (size, 3)
        }
        0x4d => {
            let bytes: [u8; 2] = script.get(2..4)?.try_into().ok()?;
            let size = usize::from(u16::from_le_bytes(bytes));
            if size <= u8::MAX as usize {
                return None;
            }
            (size, 4)
        }
        0x4e => {
            let bytes: [u8; 4] = script.get(2..6)?.try_into().ok()?;
            let size = usize::try_from(u32::from_le_bytes(bytes)).ok()?;
            if size <= u16::MAX as usize {
                return None;
            }
            (size, 6)
        }
        _ => return None,
    };
    let end = start.checked_add(length)?;
    if end != script.len() {
        return None;
    }
    script.get(start..end)
}

fn classify(vout: u32, payload: &[u8]) -> Option<Carrier> {
    let (format, transaction_type, status) = if payload.starts_with(b"tl") {
        let value = match std::str::from_utf8(payload) {
            Ok(value) => value,
            Err(_) => {
                return Some(Carrier {
                    vout,
                    format: CarrierFormat::LegacyAscii,
                    payload: payload.to_vec(),
                    transaction_type: None,
                    status: "invalid-ascii",
                })
            }
        };
        match wire::parse_frame(value) {
            Ok(frame) => {
                let status = match wire::parse_typed(value) {
                    Ok(_) => "typed-syntax",
                    Err(wire::WireError::UnsupportedType(_)) => "opaque-type",
                    Err(_) => "invalid-body",
                };
                (
                    CarrierFormat::LegacyAscii,
                    Some(frame.tx_type.as_u8()),
                    status,
                )
            }
            Err(_) => (CarrierFormat::LegacyAscii, None, "invalid-frame"),
        }
    } else if payload.starts_with(b"TLR") {
        match wire_v1::parse_frame(payload) {
            Ok(frame) => {
                let status = if frame.tx_type.as_u8() == 2 {
                    if wire_v1::parse_send(payload).is_ok() {
                        "typed-syntax"
                    } else {
                        "invalid-body"
                    }
                } else {
                    "opaque-type"
                };
                (
                    CarrierFormat::ProposedBinaryV1,
                    Some(frame.tx_type.as_u8()),
                    status,
                )
            }
            Err(_) => (CarrierFormat::ProposedBinaryV1, None, "invalid-frame"),
        }
    } else {
        return None;
    };
    Some(Carrier {
        vout,
        format,
        payload: payload.to_vec(),
        transaction_type,
        status,
    })
}

/// Hash and classify carriers using a previously validated structural scan.
/// No second walk through every vin, vout, scriptSig or witness stack.
pub(crate) fn inspect_prepared_transaction(
    raw: &[u8],
    layout: &TransactionLayout,
) -> Result<TransactionObservation, ScanError> {
    if raw.len() > MAX_RAW_TRANSACTION {
        return Err(ScanError::TooLong);
    }
    if raw.len() != layout.total_bytes {
        return Err(ScanError::TrailingBytes);
    }
    let wtxid = double_sha256(raw);
    let txid = if layout.has_witness {
        double_sha256_parts(&[
            &raw[..4],
            &raw[layout.input_start..layout.outputs_end],
            &raw[raw.len() - 4..],
        ])
    } else {
        wtxid
    };
    let carriers = layout
        .payloads
        .iter()
        .filter_map(|(vout, range)| classify(*vout, &raw[range.clone()]))
        .collect();
    Ok(TransactionObservation {
        txid,
        wtxid,
        has_witness: layout.has_witness,
        input_count: layout.input_count,
        output_count: layout.output_count,
        op_return_count: layout.op_return_count,
        carriers,
    })
}

/// Standalone TX inspection checks exact length; block preparse uses the
/// same structural layout against a larger borrowed suffix.
pub fn inspect_transaction(raw: &[u8]) -> Result<TransactionObservation, ScanError> {
    if raw.len() > MAX_RAW_TRANSACTION {
        return Err(ScanError::TooLong);
    }
    let layout = prepare_transaction_prefix(raw)?;
    inspect_prepared_transaction(raw, &layout)
}

/// Explicit Litecoin serialization including opaque MWEB; syntax only, never consensus.
pub fn inspect_litecoin_transaction(
    raw: &[u8],
) -> Result<LitecoinTransactionObservation, ScanError> {
    if raw.len() > MAX_RAW_TRANSACTION {
        return Err(ScanError::TooLong);
    }
    let mut reader = Cursor::new(raw);
    let version = reader.read(4)?;
    let extended = raw.get(4).copied() == Some(0);
    let mut flags = 0u8;
    if extended {
        let _marker = reader.byte()?;
        flags = reader.byte()?;
        // Only SegWit (0x01) and Litecoin MWEB (0x08) are defined.
        if flags == 0 || flags & !0x09 != 0 {
            return Err(ScanError::Invalid("unsupported serialization flags".into()));
        }
    }
    let has_witness = flags & 0x01 != 0;
    let has_mweb = flags & 0x08 != 0;
    let inputs_start = reader.offset;
    let inputs = reader.limited_count()?;
    let mut outpoints = Vec::with_capacity(inputs);
    for _ in 0..inputs {
        let outpoint = reader.read(36)?;
        let mut txid = [0u8; 32];
        txid.copy_from_slice(&outpoint[..32]);
        let mut vout = [0u8; 4];
        vout.copy_from_slice(&outpoint[32..]);
        outpoints.push(OutPoint {
            txid,
            vout: u32::from_le_bytes(vout),
        });
        reader.variable_bytes()?; // scriptSig
        reader.read(4)?; // sequence
    }
    let outputs = reader.limited_count()?;
    let mut carriers = Vec::new();
    let mut tx_outputs = Vec::with_capacity(outputs);
    let mut op_return_count = 0u32;
    for n in 0..outputs {
        let mut value = [0u8; 8];
        value.copy_from_slice(reader.read(8)?);
        let script = reader.variable_bytes()?;
        tx_outputs.push(TxOutput {
            value: u64::from_le_bytes(value),
            script: script.to_vec(),
        });
        if script.first() == Some(&0x6a) {
            op_return_count += 1;
            if let Some(payload) = one_op_return_push(script) {
                if let Some(carrier) = classify(n as u32, payload) {
                    carriers.push(carrier);
                }
            }
        }
    }
    let outputs_end = reader.offset;
    if has_witness {
        let mut nonempty_witness = false;
        for _ in 0..inputs {
            let n = reader.varint()?;
            if n > MAX_IO_COUNT {
                return Err(ScanError::TooLong);
            }
            nonempty_witness |= n != 0;
            for _ in 0..n {
                reader.variable_bytes()?;
            }
        }
        if !nonempty_witness {
            return Err(ScanError::Invalid(
                "superfluous SegWit witness marker".into(),
            ));
        }
    }
    let witness_end = reader.offset;
    let mut mweb = MwebData::None;
    if has_mweb {
        let remaining = raw
            .len()
            .checked_sub(reader.offset + 4)
            .filter(|n| *n > 0)
            .ok_or_else(|| ScanError::Invalid("truncated MWEB data".into()))?;
        let blob = reader.read(remaining)?;
        mweb = match blob[0] {
            0 if blob.len() == 1 => MwebData::HogEx,
            1 => MwebData::Opaque,
            _ => {
                return Err(ScanError::Invalid(
                    "invalid MWEB optional-pointer tag".into(),
                ))
            }
        };
    }
    let locktime = reader.read(4)?;
    if reader.offset != raw.len() {
        return Err(ScanError::TrailingBytes);
    }
    let txid = if extended {
        let mut stripped = Vec::with_capacity(raw.len());
        stripped.extend_from_slice(version);
        stripped.extend_from_slice(&raw[inputs_start..outputs_end]);
        stripped.extend_from_slice(locktime);
        double_sha256(&stripped)
    } else {
        double_sha256(raw)
    };
    let wtxid = if !has_witness {
        txid
    } else if has_mweb {
        let mut without_mweb = Vec::with_capacity(raw.len());
        without_mweb.extend_from_slice(version);
        without_mweb.extend_from_slice(&[0, 1]);
        without_mweb.extend_from_slice(&raw[inputs_start..witness_end]);
        without_mweb.extend_from_slice(locktime);
        double_sha256(&without_mweb)
    } else {
        double_sha256(raw)
    };
    Ok(LitecoinTransactionObservation {
        txid,
        wtxid,
        has_witness,
        mweb,
        input_count: u32::try_from(inputs).map_err(|_| ScanError::TooLong)?,
        output_count: u32::try_from(outputs).map_err(|_| ScanError::TooLong)?,
        op_return_count,
        carriers,
        inputs: outpoints,
        outputs: tx_outputs,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayItem {
    pub label: String,
    pub network_label: String,
    pub reported_height: Option<u64>,
    pub reported_block_hash: Option<String>,
    pub observation: TransactionObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError {
    pub line: usize,
    pub reason: String,
}
impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}
impl std::error::Error for ReplayError {}

/// Scan a TSV corpus in file order. Each line has exactly six columns:
/// label, network, reported_height_or_dash, reported_blockhash_or_dash,
/// expected_txid_display_hex, raw_transaction_hex.
///
/// The reported chain metadata is UNVERIFIED. Matching txids proves only that
/// the supplied bytes match their expected fingerprint; it does not prove
/// block inclusion, input signatures, ownership, or protocol acceptance.
pub fn replay_tsv(tsv: &str) -> Result<Vec<ReplayItem>, ReplayError> {
    if tsv.len() > MAX_CORPUS_BYTES {
        return Err(ReplayError {
            line: 0,
            reason: "fixture corpus too large".into(),
        });
    }
    let mut result = Vec::new();
    let mut observed: BTreeSet<(String, String)> = BTreeSet::new();
    let mut heights: BTreeMap<String, u64> = BTreeMap::new();
    for (index, line) in tsv.lines().enumerate() {
        let line_number = index + 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if result.len() == MAX_CORPUS_ROWS {
            return Err(ReplayError {
                line: line_number,
                reason: "too many rows".into(),
            });
        }
        let problem = |message: String| ReplayError {
            line: line_number,
            reason: message,
        };
        let columns: Vec<_> = line.split('\t').collect();
        let [label, network, height, blockhash, txid, raw_hex] = <[&str; 6]>::try_from(columns)
            .map_err(|_| problem("expected six TSV columns".into()))?;
        if label.is_empty()
            || network.is_empty()
            || !label.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
            || !network
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(problem("bad fixture label/network".into()));
        }
        let reported_height = if height == "-" {
            None
        } else {
            Some(
                height
                    .parse::<u64>()
                    .map_err(|_| problem("invalid height".into()))?,
            )
        };
        if let Some(height) = reported_height {
            if heights.get(network).is_some_and(|last| *last > height) {
                return Err(problem("height order regressed".into()));
            }
            heights.insert(network.to_owned(), height);
        }
        let reported_block_hash = if blockhash == "-" {
            None
        } else {
            if blockhash.len() != 64 || decode_hex(blockhash).is_err() {
                return Err(problem("invalid reported block hash".into()));
            }
            Some((*blockhash).to_owned())
        };
        if txid.len() != 64 || decode_hex(txid).is_err() {
            return Err(problem("bad expected txid".into()));
        }
        let raw = decode_hex(raw_hex).map_err(|e| problem(e.to_string()))?;
        let observation = inspect_transaction(&raw).map_err(|e| problem(e.to_string()))?;
        let derived = id_hex(&observation.txid);
        if !derived.eq_ignore_ascii_case(txid) {
            return Err(problem(format!(
                "txid mismatch: expected {txid}, derived {derived}"
            )));
        }
        if !observed.insert((network.to_owned(), derived)) {
            return Err(problem(
                "duplicate transaction on same named network".into(),
            ));
        }
        result.push(ReplayItem {
            label: (*label).to_owned(),
            network_label: network.to_owned(),
            reported_height,
            reported_block_hash,
            observation,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_push_extraction() {
        assert_eq!(one_op_return_push(b"\x6a\x02tl"), Some(&b"tl"[..]));
        assert_eq!(one_op_return_push(b"\x6a\x4c\x4c"), None); // truncated
        assert_eq!(one_op_return_push(b"\x6a\x4c\x02tl"), None); // nonminimal
        assert_eq!(one_op_return_push(b"\x6a\x02tl\x51"), None); // trailing opcode
        assert_eq!(one_op_return_push(b"\x00\x6a\x02tl"), None); // nonstandard prefix
    }

    #[test]
    fn hex_checks() {
        assert_eq!(decode_hex("AbC0"), Ok(vec![171, 192]));
        assert!(decode_hex("abc").is_err());
        assert!(decode_hex("0z").is_err());
        assert!(decode_hex(&"ff".repeat(MAX_RAW_TRANSACTION + 1)).is_err());
    }

    #[test]
    fn litecoin_mweb_flags_hash_only_canonical_fields() {
        // Synthetic: version 2, one input, outputs OP_RETURN "tl0" and P2WPKH.
        let body = decode_hex(concat!(
            "01",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "00000000",
            "00",
            "ffffffff",
            "02",
            "0000000000000000",
            "05",
            "6a03746c30",
            "2202000000000000",
            "16",
            "00141111111111111111111111111111111111111111"
        ))
        .unwrap();
        let version = [2u8, 0, 0, 0];
        let locktime = [0u8; 4];
        let legacy = [&version[..], &body, &locktime].concat();
        let legacy_txid = double_sha256(&legacy);
        let witness = [1u8, 1, 7];
        let cases: [(u8, &[u8], &[u8], MwebData); 3] = [
            (0x08, &[], &[0], MwebData::HogEx),
            (0x09, &witness, &[0], MwebData::HogEx),
            (0x08, &[], &[1, 0xee, 0xee, 0xee], MwebData::Opaque),
        ];
        for (flags, wit, blob, kind) in cases {
            let raw = [&version[..], &[0, flags], &body, wit, blob, &locktime].concat();
            assert!(
                inspect_transaction(&raw).is_err(),
                "Bitcoin must reject MWEB flags"
            );
            let seen = inspect_litecoin_transaction(&raw).unwrap();
            assert_eq!(seen.txid, legacy_txid);
            assert_eq!(seen.mweb, kind);
            assert_eq!(seen.carriers.len(), 1);
            if flags & 1 == 0 {
                assert_eq!(seen.wtxid, legacy_txid);
            } else {
                let segwit_only = [&version[..], &[0, 1], &body, wit, &locktime].concat();
                assert_eq!(seen.wtxid, double_sha256(&segwit_only));
            }
        }
        for (flags, blob) in [(0x08u8, &[2u8][..]), (0x08, &[]), (0x02, &[]), (0x18, &[0])] {
            let raw = [&version[..], &[0, flags], &body, blob, &locktime].concat();
            assert!(
                inspect_litecoin_transaction(&raw).is_err(),
                "flags {flags:#x}"
            );
        }
        // MWEB-only (no canonical inputs/outputs) has no canonical txid preimage.
        let mweb_only = [&version[..], &[0, 0x08, 0, 0, 1, 0xee], &locktime].concat();
        assert!(inspect_litecoin_transaction(&mweb_only).is_err());
        // An item may be empty while its witness stack is nonempty.
        let empty_item_witness = [&version[..], &[0, 1], &body, &[1, 0], &locktime].concat();
        assert_eq!(
            inspect_litecoin_transaction(&empty_item_witness)
                .unwrap()
                .txid,
            inspect_transaction(&empty_item_witness).unwrap().txid,
        );
    }

    #[test]
    fn malformed_transactions_fail_closed() {
        assert_eq!(inspect_transaction(b""), Err(ScanError::Truncated));
        let malformed = decode_hex("0100000000ff").unwrap();
        assert!(inspect_transaction(&malformed).is_err());
        let noncanonical = decode_hex("01000000fdfc00").unwrap();
        assert_eq!(
            inspect_transaction(&noncanonical),
            Err(ScanError::NonCanonicalCompactSize)
        );
    }
}

/// Explicit, unverified Litecoin transparent serialization profile.
pub mod litecoin_transparent {
    use super::*;
    include!("litecoin_transparent_impl.rs");
}
