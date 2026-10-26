//! Sequential source dispatch against the pending sole economic owner.
//!
//! This is an explicitly repaired, UNVERIFIED block runner. Source Types and
//! validity are recomputed over actual owner reads; neither raw actor claims,
//! source `valid` flags nor persisted hashes authenticate an L1 transaction.

use crate::chain::BlockHeader;
use crate::economic::{EconomicError, EconomicState, MAX_ECONOMIC_INPUTS};
use crate::economic_validity::{EconomicSourceBackend, ValidationContext};
use crate::hash256::double_sha256_parts;
use crate::ledger::TxId;
use crate::types_economic::map_economic_input_unverified;
use crate::types_source::{decode_payload_unverified, TypesOutcome};
use crate::validity_source::SourceValue;
use std::collections::BTreeSet;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "af4c2fe7c37680be018dcb0df9f2cf8b348baac8aec1fe2335a220ce27dc0230";
pub const MAX_SOURCE_FRAMES: usize = 4096;
pub const MAX_SOURCE_RECORDS: usize = 65_536;
pub const MAX_SOURCE_HISTORY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SOURCE_TRANSACTION_BYTES: usize = 65_536;
pub const MAX_SOURCE_REASON_BYTES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceConsensusPolicy {
    /// Source phase order with candidate rollback, exact owner custody,
    /// previously activated operations and atomic end-of-block publication.
    ConservingSequentialV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceConsensusConfig {
    pub bootstrap_commitment: [u8; 32],
    pub policy: SourceConsensusPolicy,
    /// Independent pin for complete actual property/clearlist bootstrap state.
    pub catalog_bootstrap_commitment: Option<[u8; 32]>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceBlockMode {
    /// main.constructConsensusFromIndex prioritizes source types 4 and 20.
    IndexedReplay,
    /// main.blockHandlerBegin prioritizes payload prefixes '4' and 'm' (22).
    Realtime,
}
impl SourceBlockMode {
    pub fn is_funding(self, transaction_type: Option<u8>) -> bool {
        match self {
            Self::IndexedReplay => matches!(transaction_type, Some(4 | 20)),
            Self::Realtime => matches!(transaction_type, Some(4 | 22)),
        }
    }
    pub fn is_funding_payload(self, payload: &str) -> bool {
        if self == Self::Realtime {
            // blockHandlerBegin uses literal startsWith, not parsed base36:
            // uppercase M decodes to type22 but is NOT prioritized live.
            payload.starts_with('4') || payload.starts_with('m')
        } else {
            self.is_funding(payload.as_bytes().first().and_then(|byte| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'z' => Some(byte - b'a' + 10),
                b'A'..=b'Z' => Some(byte - b'A' + 10),
                _ => None,
            }))
        }
    }
}

/// Original index/RPC metadata, deliberately unverified. Source amounts are
/// the exact values passed into Types (reference amount already divided by
/// COIN by the caller's extraction layer); they are not trusted UTXO evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTransaction {
    pub position: u32,
    pub txid: TxId,
    pub payload: String,
    pub marker: SourceValue,
    pub sender: SourceValue,
    pub reference: SourceValue,
    pub sender_amount: SourceValue,
    pub reference_amount: SourceValue,
}
impl SourceTransaction {
    pub fn transaction_type(&self) -> Option<u8> {
        self.payload.as_bytes().first().and_then(|byte| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'z' => Some(byte - b'a' + 10),
            b'A'..=b'Z' => Some(byte - b'A' + 10),
            _ => None,
        })
    }
    pub fn arguments(&self, height: u64, mode: SourceBlockMode) -> Vec<SourceValue> {
        let ty = self.transaction_type().map_or(f64::NAN, f64::from);
        // Source removes one UTF-16 code unit. The supported payload is ASCII,
        // so byte slicing and code-unit slicing agree exactly.
        let body = self.payload.get(1..).unwrap_or("");
        vec![
            SourceValue::String(txid_text(self.txid)),
            SourceValue::number(ty),
            if mode == SourceBlockMode::Realtime {
                SourceValue::String("tl".into())
            } else {
                self.marker.clone()
            },
            SourceValue::String(body.into()),
            self.sender.clone(),
            self.reference.clone(),
            self.sender_amount.clone(),
            self.reference_amount.clone(),
            SourceValue::number(height as f64),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDisposition {
    Applied,
    Invalid,
    ReturnedError,
    Thrown,
    UnsupportedMapping,
    EconomicRejected,
    AlreadyProcessed,
    DependencyUnavailable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReceipt {
    pub position: u32,
    pub txid: TxId,
    pub disposition: SourceDisposition,
    /// Commits the actual Types diagnostic or local unavailable error. Stored
    /// diagnostics/dependency answers are never supplied back to validation.
    pub evaluation_commitment: [u8; 32],
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBlockFrame {
    pub header: BlockHeader,
    pub mode: SourceBlockMode,
    /// Original L1/index order, including rejected and duplicate candidates.
    pub transactions: Vec<SourceTransaction>,
    /// Source execution order: stable funding partition, then remaining rows.
    pub receipts: Vec<SourceReceipt>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceConsensusState {
    pub bootstrap: Vec<u8>,
    pub frames: Vec<SourceBlockFrame>,
    pub catalog_bootstrap: Vec<u8>,
    pub catalogs: Option<crate::source_catalogs::SourceCatalogOwners>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedSourceBlock {
    pub network: crate::network_profile::TradeLayerMainnet,
    pub header: BlockHeader,
    pub mode: SourceBlockMode,
    pub transactions: Vec<SourceTransaction>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBlockReport {
    pub header: BlockHeader,
    pub accepted: usize,
    pub rejected: usize,
    pub already_processed: usize,
    pub trade_receipts: usize,
    pub receipts: Vec<SourceReceipt>,
}

#[derive(Debug)]
pub enum ConsensusSourceError {
    Disabled,
    InvalidInput,
    Capacity,
    WrongParent,
    UnauthenticatedInputAndState,
    Economic(EconomicError),
}
impl core::fmt::Display for ConsensusSourceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ConsensusSourceError {}
impl From<EconomicError> for ConsensusSourceError {
    fn from(value: EconomicError) -> Self {
        Self::Economic(value)
    }
}
type Result<T> = core::result::Result<T, ConsensusSourceError>;

pub fn require_authenticated_admission() -> Result<()> {
    Err(ConsensusSourceError::UnauthenticatedInputAndState)
}

pub(crate) fn bootstrap_commitment(bytes: &[u8]) -> [u8; 32] {
    double_sha256_parts(&[b"TradeLayer/SourceConsensusBootstrap/v1/", bytes])
}

/// Used by the owner stage and checkpoint preflight before any candidate
/// clones. Exact history byte/node budgets are enforced by the binary writer.
pub(crate) fn preflight_transactions(transactions: &[SourceTransaction]) -> Result<()> {
    if transactions.len() > MAX_ECONOMIC_INPUTS {
        return Err(ConsensusSourceError::Capacity);
    }
    let mut previous = None;
    let mut total = 0usize;
    for tx in transactions {
        if previous.is_some_and(|p| p >= tx.position) || !tx.payload.is_ascii() {
            return Err(ConsensusSourceError::InvalidInput);
        }
        previous = Some(tx.position);
        let mut size = tx
            .payload
            .len()
            .checked_add(40)
            .ok_or(ConsensusSourceError::Capacity)?;
        for value in [
            &tx.marker,
            &tx.sender,
            &tx.reference,
            &tx.sender_amount,
            &tx.reference_amount,
        ] {
            value
                .validate()
                .map_err(|_| ConsensusSourceError::Capacity)?;
            size = size
                .checked_add(raw_value_size(value)?)
                .ok_or(ConsensusSourceError::Capacity)?;
        }
        if size > MAX_SOURCE_TRANSACTION_BYTES {
            return Err(ConsensusSourceError::Capacity);
        }
        total = total
            .checked_add(size)
            .ok_or(ConsensusSourceError::Capacity)?;
        if total > MAX_SOURCE_HISTORY_BYTES {
            return Err(ConsensusSourceError::Capacity);
        }
    }
    Ok(())
}

fn raw_value_size(value: &SourceValue) -> Result<usize> {
    let add = |a: usize, b: usize| a.checked_add(b).ok_or(ConsensusSourceError::Capacity);
    match value {
        SourceValue::Undefined | SourceValue::Null | SourceValue::Bool(_) => Ok(1),
        SourceValue::Number(bits) if f64::from_bits(*bits).is_finite() => Ok(9),
        SourceValue::String(text) => add(5, text.len()),
        SourceValue::Array(values) => values.iter().try_fold(5, |n, v| add(n, raw_value_size(v)?)),
        SourceValue::Object(fields) => fields.iter().try_fold(5, |n, (k, v)| {
            add(add(n, add(4, k.len())?)?, raw_value_size(v)?)
        }),
        // Raw index/RPC metadata supports JSON-safe finite values and explicit
        // absence. Runtime promises, BigNumber instances and nonfinite caller
        // numbers are outside this producer-facing input domain.
        _ => Err(ConsensusSourceError::InvalidInput),
    }
}

/// Executes source decode/validation afresh over each pending owner view.
/// Candidate failures retain evidence and continue; end-phase failures abort
/// the entire block. There is no mutable caller backend or supplied verdict.
pub(crate) fn execute_block_unverified(
    state: &EconomicState,
    header: BlockHeader,
    mode: SourceBlockMode,
    transactions: &[SourceTransaction],
) -> Result<(EconomicState, SourceBlockReport)> {
    state
        .config()
        .source_consensus
        .as_ref()
        .ok_or(ConsensusSourceError::Disabled)?;
    preflight_transactions(transactions)?;
    let parent = state.source_parent_unverified()?;
    if header.height
        != parent
            .height
            .checked_add(1)
            .ok_or(ConsensusSourceError::WrongParent)?
        || header.parent != parent.id
        || header.id == state.config().network.genesis()
        || state
            .source_consensus_state()
            .frames
            .iter()
            .any(|f| f.header.id == header.id)
    {
        return Err(ConsensusSourceError::WrongParent);
    }
    state.preflight_source_frame(header, mode, transactions)?;
    let context = ValidationContext {
        network: state.config().network,
        height: header.height,
        parent,
    };
    let mut stage = state.begin_block_stage_unverified(header.height)?;
    let mut processed: BTreeSet<TxId> = state
        .source_consensus_state()
        .frames
        .iter()
        .flat_map(|f| f.transactions.iter().map(|tx| tx.txid))
        .collect();
    let mut receipts = Vec::with_capacity(transactions.len());
    for funding in [true, false] {
        for tx in transactions
            .iter()
            .filter(|tx| mode.is_funding_payload(&tx.payload) == funding)
        {
            let mut receipt = SourceReceipt {
                position: tx.position,
                txid: tx.txid,
                disposition: SourceDisposition::AlreadyProcessed,
                evaluation_commitment: [0; 32],
                reason: String::new(),
            };
            if !processed.insert(tx.txid) {
                receipts.push(receipt);
                continue;
            }
            let arguments = tx.arguments(header.height, mode);
            let mut backend = EconomicSourceBackend::for_staged_input(stage.state(), context)
                .map_err(|_| ConsensusSourceError::InvalidInput)?;
            let evaluation = decode_payload_unverified(&arguments, &mut backend);
            // Genuine source queries can load caches or record caught DB
            // failures even when validation rejects. Retain those owner reads.
            let catalogs = backend.into_catalogs();
            stage.retain_source_catalog_reads(catalogs)?;
            match evaluation {
                Err(error) => {
                    receipt.disposition = SourceDisposition::DependencyUnavailable;
                    let diagnostic = format!("{error:?}");
                    receipt.evaluation_commitment = evaluation_commitment(diagnostic.as_bytes());
                    receipt.reason = bounded_reason(&diagnostic);
                }
                Ok(evaluation) => {
                    receipt.evaluation_commitment = full_evaluation_commitment(&evaluation)?;
                    match &evaluation.outcome {
                        TypesOutcome::Returned(params)
                            if params.get("valid") == SourceValue::Bool(true) =>
                        {
                            if let Some(result) = stage
                                .try_apply_source_send_unverified(&arguments, &evaluation)
                                .or_else(|| {
                                    stage
                                        .try_apply_source_oracle_unverified(&arguments, &evaluation)
                                })
                                .or_else(|| {
                                    stage.try_apply_source_catalog_unverified(
                                        &arguments,
                                        &evaluation,
                                    )
                                })
                            {
                                use crate::economic::SourceCatalogExecutionError;
                                match result {
                                    Ok(()) => receipt.disposition = SourceDisposition::Applied,
                                    Err(SourceCatalogExecutionError::Mapping(reason)) => {
                                        receipt.disposition = SourceDisposition::UnsupportedMapping;
                                        receipt.reason = bounded_reason(&reason);
                                    }
                                    Err(SourceCatalogExecutionError::Rule(error)) => {
                                        receipt.disposition =
                                            SourceDisposition::DependencyUnavailable;
                                        receipt.reason = bounded_reason(&format!("{error:?}"));
                                    }
                                    Err(SourceCatalogExecutionError::Thrown(error)) => {
                                        receipt.disposition = SourceDisposition::Thrown;
                                        receipt.reason = bounded_reason(&format!(
                                            "{}: {}",
                                            error.name, error.message
                                        ));
                                    }
                                    Err(SourceCatalogExecutionError::Economic(error)) => {
                                        receipt.disposition = SourceDisposition::EconomicRejected;
                                        receipt.reason = bounded_reason(&format!("{error:?}"));
                                    }
                                }
                                receipts.push(receipt);
                                continue;
                            }
                            match map_economic_input_unverified(
                                &arguments,
                                &evaluation,
                                header.height,
                                stage.state().config().contract,
                            ) {
                                Err(error) => {
                                    receipt.disposition = SourceDisposition::UnsupportedMapping;
                                    receipt.reason = bounded_reason(&format!("{error:?}"));
                                }
                                Ok(input) => match stage.try_apply_unverified(&input) {
                                    Ok(()) => receipt.disposition = SourceDisposition::Applied,
                                    Err(error) => {
                                        receipt.disposition = SourceDisposition::EconomicRejected;
                                        receipt.reason = bounded_reason(&format!("{error:?}"));
                                    }
                                },
                            }
                        }
                        TypesOutcome::Returned(params) => {
                            receipt.disposition = SourceDisposition::Invalid;
                            receipt.reason = bounded_reason(
                                &params
                                    .get("reason")
                                    .diagnostic_json()
                                    .map_err(|_| ConsensusSourceError::Capacity)?,
                            );
                        }
                        TypesOutcome::ReturnedError(error) => {
                            receipt.disposition = SourceDisposition::ReturnedError;
                            receipt.reason =
                                bounded_reason(&format!("{}: {}", error.name, error.message));
                        }
                        TypesOutcome::Rejected(error) => {
                            receipt.disposition = SourceDisposition::Thrown;
                            receipt.reason =
                                bounded_reason(&format!("{}: {}", error.name, error.message));
                        }
                    }
                }
            }
            receipts.push(receipt);
        }
    }
    let frame = SourceBlockFrame {
        header,
        mode,
        transactions: transactions.to_vec(),
        receipts: receipts.clone(),
    };
    let (next, trade_receipts) = stage.finish_source_unverified(frame)?;
    let accepted = receipts
        .iter()
        .filter(|r| r.disposition == SourceDisposition::Applied)
        .count();
    let already_processed = receipts
        .iter()
        .filter(|r| r.disposition == SourceDisposition::AlreadyProcessed)
        .count();
    let report = SourceBlockReport {
        header,
        accepted,
        rejected: receipts.len() - accepted - already_processed,
        already_processed,
        trade_receipts,
        receipts,
    };
    Ok((next, report))
}

fn evaluation_commitment(bytes: &[u8]) -> [u8; 32] {
    double_sha256_parts(&[b"TradeLayer/SourceConsensusEvaluation/v1/", bytes])
}
fn full_evaluation_commitment(
    evaluation: &crate::types_source::TypesEvaluation,
) -> Result<[u8; 32]> {
    use crate::validity_source::DependencyReturn;
    fn field(out: &mut Vec<u8>, text: &str) -> Result<()> {
        if out
            .len()
            .checked_add(4)
            .and_then(|n| n.checked_add(text.len()))
            .is_none_or(|n| n > crate::types_source::MAX_TRACE_BYTES)
        {
            return Err(ConsensusSourceError::Capacity);
        }
        out.extend_from_slice(&(text.len() as u32).to_be_bytes());
        out.extend_from_slice(text.as_bytes());
        Ok(())
    }
    let mut out = Vec::new();
    field(
        &mut out,
        &evaluation
            .diagnostic_json()
            .map_err(|_| ConsensusSourceError::Capacity)?,
    )?;
    if let Some(validity) = &evaluation.validity {
        let source = &validity.settlement.evaluation;
        field(&mut out, &source.dependencies.len().to_string())?;
        for dependency in &source.dependencies {
            field(&mut out, &dependency.call.target)?;
            field(&mut out, &dependency.call.arguments.len().to_string())?;
            for value in &dependency.call.arguments {
                field(
                    &mut out,
                    &value
                        .diagnostic_json()
                        .map_err(|_| ConsensusSourceError::Capacity)?,
                )?;
            }
            match &dependency.returned {
                DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => {
                    field(
                        &mut out,
                        if matches!(dependency.returned, DependencyReturn::Immediate(_)) {
                            "immediate"
                        } else {
                            "pending"
                        },
                    )?;
                    field(
                        &mut out,
                        &value
                            .diagnostic_json()
                            .map_err(|_| ConsensusSourceError::Capacity)?,
                    )?;
                }
                DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                    field(
                        &mut out,
                        if matches!(dependency.returned, DependencyReturn::Throw(_)) {
                            "throw"
                        } else {
                            "pending-throw"
                        },
                    )?;
                    field(&mut out, &error.name)?;
                    field(&mut out, &error.message)?;
                }
            }
        }
        field(&mut out, &source.deferred_rejections.len().to_string())?;
        for error in &source.deferred_rejections {
            field(&mut out, &error.name)?;
            field(&mut out, &error.message)?;
        }
    }
    Ok(evaluation_commitment(&out))
}
fn bounded_reason(reason: &str) -> String {
    let mut end = reason.len().min(MAX_SOURCE_REASON_BYTES);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_owned()
}
fn txid_text(txid: TxId) -> String {
    let mut output = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in txid.as_bytes() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}

/// Literal source DB projection hashing. This is a distinct SHA256 namespace
/// from TLEC/TLEH/TLSE: database row order and object field order are significant.
pub fn source_state_consensus_hash(
    rows: &[(String, Vec<crate::issuance_intent::JsonValue>)],
) -> Result<(String, [u8; 32])> {
    use crate::issuance_intent::{JsonObject, JsonValue};
    const FIELDS: [(&str, &str); 16] = [
        ("tallyMap", "latestTally"),
        ("activations", "latestActivation"),
        ("channels", "channels"),
        ("clearlists", "clearlists"),
        ("contractList", "contractList"),
        ("feeCache", "feeCache"),
        ("insurance", "insurance"),
        ("marginMaps", "marginMaps"),
        ("oracleData", "oracleData"),
        ("oracleList", "oracleList"),
        ("orderBooks", "orderBooks"),
        ("propertyList", "propertyList"),
        ("syntheticTokens", "syntheticTokens"),
        ("vaults", "vaults"),
        ("volumeIndex", "volumeIndex"),
        ("withdrawQueue", "withdrawQueue"),
    ];
    if rows.len() != FIELDS.len() {
        return Err(ConsensusSourceError::InvalidInput);
    }
    let mut json_nodes = 1;
    let mut json_bytes = 2;
    for (_, values) in rows {
        for value in values {
            source_json_budget(value, &mut json_nodes, &mut json_bytes)?;
        }
    }
    let mut object = JsonObject::new();
    let mut count = 0usize;
    for (index, (database, field)) in FIELDS.into_iter().enumerate() {
        let mut matches = rows.iter().filter(|(name, _)| name == database);
        let (_, values) = matches.next().ok_or(ConsensusSourceError::InvalidInput)?;
        if matches.next().is_some() {
            return Err(ConsensusSourceError::InvalidInput);
        }
        count = count
            .checked_add(values.len())
            .ok_or(ConsensusSourceError::Capacity)?;
        if count > MAX_SOURCE_RECORDS {
            return Err(ConsensusSourceError::Capacity);
        }
        // to_json performs bounded borrowed validation before cloning rows.
        for value in values {
            value
                .to_json()
                .map_err(|_| ConsensusSourceError::Capacity)?;
        }
        let value = if index < 2 {
            values.last().cloned().unwrap_or(JsonValue::Null)
        } else {
            JsonValue::Array(values.clone())
        };
        object
            .insert(field.into(), value)
            .map_err(|_| ConsensusSourceError::Capacity)?;
    }
    let json = object
        .to_json()
        .map_err(|_| ConsensusSourceError::Capacity)?;
    let digest = crate::hash256::sha256(json.as_bytes());
    Ok((json, digest))
}

pub fn source_tx_index_hash(rows: &[crate::issuance_intent::JsonValue]) -> Result<[u8; 32]> {
    use crate::issuance_intent::JsonValue;
    if rows.len() > MAX_SOURCE_RECORDS {
        return Err(ConsensusSourceError::Capacity);
    }
    let mut nodes = 1;
    let mut bytes = 2;
    for row in rows {
        source_json_budget(row, &mut nodes, &mut bytes)?;
    }
    let mut selected = Vec::new();
    for row in rows {
        let JsonValue::Object(object) = row else {
            return Err(ConsensusSourceError::InvalidInput);
        };
        let Some(JsonValue::String(id)) = object.get("_id") else {
            return Err(ConsensusSourceError::InvalidInput);
        };
        if id.starts_with("tx") {
            selected.push(row.clone());
        }
    }
    let json = JsonValue::Array(selected)
        .to_json()
        .map_err(|_| ConsensusSourceError::Capacity)?;
    Ok(crate::hash256::sha256(json.as_bytes()))
}

fn source_json_budget(
    value: &crate::issuance_intent::JsonValue,
    nodes: &mut usize,
    bytes: &mut usize,
) -> Result<()> {
    use crate::issuance_intent::JsonValue;
    fn count(value: &JsonValue, nodes: &mut usize, depth: usize) -> Result<()> {
        if depth > crate::issuance_intent::MAX_JSON_DEPTH {
            return Err(ConsensusSourceError::Capacity);
        }
        *nodes = nodes.checked_add(1).ok_or(ConsensusSourceError::Capacity)?;
        if *nodes > crate::issuance_intent::MAX_JSON_NODES {
            return Err(ConsensusSourceError::Capacity);
        }
        match value {
            JsonValue::Array(values) => {
                for value in values {
                    count(value, nodes, depth + 1)?;
                }
            }
            JsonValue::Object(object) => {
                for (_, value) in object.iter() {
                    count(value, nodes, depth + 1)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    count(value, nodes, 0)?;
    *bytes = bytes
        .checked_add(
            value
                .to_json()
                .map_err(|_| ConsensusSourceError::Capacity)?
                .len()
                + 1,
        )
        .ok_or(ConsensusSourceError::Capacity)?;
    if *bytes > crate::issuance_intent::MAX_JSON_BYTES {
        return Err(ConsensusSourceError::Capacity);
    }
    Ok(())
}
