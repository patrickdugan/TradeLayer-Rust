//! Bounded replay journal around the sole literal source contract runtime.
//! Expected heads detect substituted history; neither hashes nor CRC authenticate L1.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::hash256::sha256;
use crate::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy, StorageFailure,
};
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceCall, SourceException, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

pub const MAX_COMMANDS: usize = 4096;
pub const MAX_EFFECTS: usize = 4096;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CHECKPOINT_BYTES: usize = 32 * 1024 * 1024;
const MAX_STATE_NODES: usize = 262_144;
const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
const MAGIC_V1: &[u8; 5] = b"TLCR\x01";
const MAGIC_V2: &[u8; 5] = b"TLCR\x02";
const MAGIC_V3: &[u8; 5] = b"TLCR\x03";
const MAGIC_V4: &[u8; 5] = b"TLCR\x04";
const MAGIC_V5: &[u8; 5] = b"TLCR\x05";
const MAGIC_V6: &[u8; 5] = b"TLCR\x06";
const MAGIC_V7: &[u8; 5] = b"TLCR\x07";
const MAGIC_V8: &[u8; 5] = b"TLCR\x08";
const SOURCE_PIN: &[u8] = b"dca0888b827c5bc28fa9189bc9e1d507f5c5670c";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceContractCommand {
    Import { collection: String, rows: Vec<V> },
    SetEntry(String),
    SetHeight(V),
    SetActivation(bool),
    FailStorage(StorageFailure),
    Invoke(DependencyCall),
    EvaluateQueue(SourceCall),
    ExecuteContract(Vec<V>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceContractCommandOutcome {
    Void,
    Dependency(DependencyReturn),
    Source(SourceOutcome),
    LocalError(SourceRuleError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceContractReplayHead {
    pub commands: u64,
    pub commitment: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContractReceipt {
    pub sequence: u64,
    pub previous_head: [u8; 32],
    pub outcome: SourceContractCommandOutcome,
    pub effects: Vec<V>,
    pub state_commitment: [u8; 32],
    pub effects_commitment: [u8; 32],
    pub head: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    command: SourceContractCommand,
    receipt: SourceContractReceipt,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceContractReplayError {
    Capacity,
    InvalidCommand,
    InvalidCheckpoint,
    WrongHead,
    ReplayMismatch,
    Runtime(SourceRuleError),
}
impl core::fmt::Display for SourceContractReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceContractReplayError {}
type Result<T> = core::result::Result<T, SourceContractReplayError>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContractReplay {
    runtime: SourceContractRuntime,
    initial: [u8; 32],
    head: SourceContractReplayHead,
    journal: Vec<Entry>,
    encoded_bytes: usize,
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn string(v: V) -> Result<String> {
    if let V::String(v) = v {
        Ok(v)
    } else {
        Err(SourceContractReplayError::InvalidCheckpoint)
    }
}
fn array(v: V) -> Result<Vec<V>> {
    if let V::Array(v) = v {
        Ok(v)
    } else {
        Err(SourceContractReplayError::InvalidCheckpoint)
    }
}
fn boolean(v: V) -> Result<bool> {
    if let V::Bool(v) = v {
        Ok(v)
    } else {
        Err(SourceContractReplayError::InvalidCheckpoint)
    }
}
fn rule_text(e: &SourceRuleError) -> &'static str {
    match e {
        SourceRuleError::Capacity => "Capacity",
        SourceRuleError::InvalidInput => "InvalidInput",
        SourceRuleError::UnknownMethod => "UnknownMethod",
        SourceRuleError::UnsupportedNumericDomain => "UnsupportedNumericDomain",
        SourceRuleError::UnknownDependency => "UnknownDependency",
        SourceRuleError::Backend => "Backend",
        SourceRuleError::UnauthenticatedInputAndState => "UnauthenticatedInputAndState",
    }
}
fn parse_rule(v: V) -> Result<SourceRuleError> {
    Ok(match string(v)?.as_str() {
        "Capacity" => SourceRuleError::Capacity,
        "InvalidInput" => SourceRuleError::InvalidInput,
        "UnknownMethod" => SourceRuleError::UnknownMethod,
        "UnsupportedNumericDomain" => SourceRuleError::UnsupportedNumericDomain,
        "UnknownDependency" => SourceRuleError::UnknownDependency,
        "Backend" => SourceRuleError::Backend,
        "UnauthenticatedInputAndState" => SourceRuleError::UnauthenticatedInputAndState,
        _ => return Err(SourceContractReplayError::InvalidCheckpoint),
    })
}
fn exception(e: &SourceException) -> V {
    obj(&[("name", s(&e.name)), ("message", s(&e.message))])
}
fn parse_exception(v: V) -> Result<SourceException> {
    let e = SourceException {
        name: string(v.get("name"))?,
        message: string(v.get("message"))?,
    };
    if exception(&e) != v {
        return Err(SourceContractReplayError::InvalidCheckpoint);
    }
    Ok(e)
}
impl SourceContractCommand {
    pub fn value(&self) -> V {
        match self {
            Self::Import { collection, rows } => obj(&[
                ("kind", s("Import")),
                ("collection", s(collection)),
                ("rows", V::Array(rows.clone())),
            ]),
            Self::SetEntry(v) => obj(&[("kind", s("SetEntry")), ("entry", s(v))]),
            Self::SetHeight(v) => obj(&[("kind", s("SetHeight")), ("height", v.clone())]),
            Self::SetActivation(v) => obj(&[("kind", s("SetActivation")), ("active", V::Bool(*v))]),
            Self::FailStorage(v) => obj(&[
                ("kind", s("FailStorage")),
                ("collection", s(&v.collection)),
                ("operation", s(&v.operation)),
                ("skip", s(&v.skip.to_string())),
                ("message", s(&v.message)),
            ]),
            Self::Invoke(v) => obj(&[
                ("kind", s("Invoke")),
                ("target", s(&v.target)),
                ("arguments", V::Array(v.arguments.clone())),
            ]),
            Self::EvaluateQueue(v) => obj(&[
                ("kind", s("EvaluateQueue")),
                ("method", s(&v.method)),
                ("arguments", V::Array(v.arguments.clone())),
            ]),
            Self::ExecuteContract(v) => obj(&[
                ("kind", s("ExecuteContract")),
                ("arguments", V::Array(v.clone())),
            ]),
        }
    }
    fn from_value(v: &V) -> Result<Self> {
        let command = match string(v.get("kind"))?.as_str() {
            "Import" => Self::Import {
                collection: string(v.get("collection"))?,
                rows: array(v.get("rows"))?,
            },
            "SetEntry" => Self::SetEntry(string(v.get("entry"))?),
            "SetHeight" => Self::SetHeight(v.get("height")),
            "SetActivation" => Self::SetActivation(boolean(v.get("active"))?),
            "FailStorage" => {
                let skip = string(v.get("skip"))?;
                let n = skip
                    .parse::<usize>()
                    .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
                if n.to_string() != skip {
                    return Err(SourceContractReplayError::InvalidCheckpoint);
                }
                Self::FailStorage(StorageFailure {
                    collection: string(v.get("collection"))?,
                    operation: string(v.get("operation"))?,
                    skip: n,
                    message: string(v.get("message"))?,
                })
            }
            "Invoke" => Self::Invoke(DependencyCall {
                target: string(v.get("target"))?,
                arguments: array(v.get("arguments"))?,
            }),
            "EvaluateQueue" => Self::EvaluateQueue(SourceCall {
                method: string(v.get("method"))?,
                arguments: array(v.get("arguments"))?,
            }),
            "ExecuteContract" => Self::ExecuteContract(array(v.get("arguments"))?),
            _ => return Err(SourceContractReplayError::InvalidCheckpoint),
        };
        if command.value() != *v {
            return Err(SourceContractReplayError::InvalidCheckpoint);
        }
        Ok(command)
    }
}
impl SourceContractCommandOutcome {
    pub fn value(&self) -> V {
        match self {
            Self::Void => obj(&[("kind", s("Void"))]),
            Self::LocalError(e) => obj(&[("kind", s("LocalError")), ("error", s(rule_text(e)))]),
            Self::Source(SourceOutcome::Returned(v)) => obj(&[
                ("kind", s("Source")),
                ("return", s("Returned")),
                ("value", v.clone()),
            ]),
            Self::Source(SourceOutcome::Thrown(e)) => obj(&[
                ("kind", s("Source")),
                ("return", s("Thrown")),
                ("error", exception(e)),
            ]),
            Self::Dependency(DependencyReturn::Immediate(v)) => obj(&[
                ("kind", s("Dependency")),
                ("return", s("Immediate")),
                ("value", v.clone()),
            ]),
            Self::Dependency(DependencyReturn::Pending(v)) => obj(&[
                ("kind", s("Dependency")),
                ("return", s("Pending")),
                ("value", v.clone()),
            ]),
            Self::Dependency(DependencyReturn::Throw(e)) => obj(&[
                ("kind", s("Dependency")),
                ("return", s("Throw")),
                ("error", exception(e)),
            ]),
            Self::Dependency(DependencyReturn::PendingThrow(e)) => obj(&[
                ("kind", s("Dependency")),
                ("return", s("PendingThrow")),
                ("error", exception(e)),
            ]),
        }
    }
    fn from_value(v: &V) -> Result<Self> {
        let result = match string(v.get("kind"))?.as_str() {
            "Void" => Self::Void,
            "LocalError" => Self::LocalError(parse_rule(v.get("error"))?),
            "Source" => Self::Source(match string(v.get("return"))?.as_str() {
                "Returned" => SourceOutcome::Returned(v.get("value")),
                "Thrown" => SourceOutcome::Thrown(parse_exception(v.get("error"))?),
                _ => return Err(SourceContractReplayError::InvalidCheckpoint),
            }),
            "Dependency" => Self::Dependency(match string(v.get("return"))?.as_str() {
                "Immediate" => DependencyReturn::Immediate(v.get("value")),
                "Pending" => DependencyReturn::Pending(v.get("value")),
                "Throw" => DependencyReturn::Throw(parse_exception(v.get("error"))?),
                "PendingThrow" => DependencyReturn::PendingThrow(parse_exception(v.get("error"))?),
                _ => return Err(SourceContractReplayError::InvalidCheckpoint),
            }),
            _ => return Err(SourceContractReplayError::InvalidCheckpoint),
        };
        if result.value() != *v {
            return Err(SourceContractReplayError::InvalidCheckpoint);
        }
        Ok(result)
    }
}
fn diagnostic(v: &V) -> Result<Vec<u8>> {
    let text = v
        .diagnostic_json()
        .map_err(|_| SourceContractReplayError::Capacity)?;
    if text.len() > MAX_FRAME_BYTES {
        return Err(SourceContractReplayError::Capacity);
    }
    let parsed = crate::property_catalog::parse_diagnostic_value(&text)
        .map_err(|_| SourceContractReplayError::InvalidCommand)?;
    if parsed != *v {
        return Err(SourceContractReplayError::InvalidCommand);
    }
    Ok(text.into_bytes())
}
fn put_frame(out: &mut Vec<u8>, frame: &[u8]) -> Result<()> {
    if frame.len() > MAX_FRAME_BYTES
        || out.len().saturating_add(4).saturating_add(frame.len()) > MAX_CHECKPOINT_BYTES
    {
        return Err(SourceContractReplayError::Capacity);
    }
    out.extend_from_slice(&(frame.len() as u32).to_be_bytes());
    out.extend_from_slice(frame);
    Ok(())
}
fn read_frame(c: &mut Cursor<'_>) -> Result<V> {
    let len = c
        .u32()
        .map_err(|_| SourceContractReplayError::InvalidCheckpoint)? as usize;
    if len > MAX_FRAME_BYTES {
        return Err(SourceContractReplayError::Capacity);
    }
    let bytes = c
        .take(len)
        .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
    let text =
        core::str::from_utf8(bytes).map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
    let v = crate::property_catalog::parse_diagnostic_value(text)
        .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
    if diagnostic(&v)? != bytes {
        return Err(SourceContractReplayError::InvalidCheckpoint);
    }
    Ok(v)
}
fn effect_commitment(effects: &[V]) -> Result<[u8; 32]> {
    if effects.len() > MAX_EFFECTS {
        return Err(SourceContractReplayError::Capacity);
    }
    let mut bytes = b"TradeLayer/SourceContractEffects/v1/".to_vec();
    bytes.extend_from_slice(&(effects.len() as u32).to_be_bytes());
    for effect in effects {
        put_frame(&mut bytes, &diagnostic(effect)?)?;
    }
    Ok(sha256(&bytes))
}
// State is computed by the actual owner, never decoded as supplied current
// state. Its canonical binary hash has independent aggregate bounds so many
// separately valid source rows do not exceed one diagnostic frame's node limit.
fn state_commitment(runtime: &SourceContractRuntime) -> Result<[u8; 32]> {
    let state = runtime
        .state_value()
        .map_err(SourceContractReplayError::Runtime)?;
    let mut bytes = b"TradeLayer/SourceContractState/v1/".to_vec();
    let mut nodes = 0;
    encode_state(&state, 0, &mut nodes, &mut bytes)?;
    Ok(sha256(&bytes))
}
fn state_string(value: &str, out: &mut Vec<u8>) -> Result<()> {
    if value.len() > MAX_STATE_BYTES
        || out.len().saturating_add(4).saturating_add(value.len()) > MAX_STATE_BYTES
    {
        return Err(SourceContractReplayError::Capacity);
    }
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}
fn encode_state(v: &V, depth: usize, nodes: &mut usize, out: &mut Vec<u8>) -> Result<()> {
    *nodes = nodes
        .checked_add(1)
        .ok_or(SourceContractReplayError::Capacity)?;
    if *nodes > MAX_STATE_NODES || depth > 64 || out.len() > MAX_STATE_BYTES {
        return Err(SourceContractReplayError::Capacity);
    }
    match v {
        V::Undefined => out.push(0),
        V::Null => out.push(1),
        V::Bool(false) => out.push(2),
        V::Bool(true) => out.push(3),
        V::Number(bits) => {
            out.push(4);
            out.extend_from_slice(&bits.to_be_bytes())
        }
        V::String(s) => {
            out.push(5);
            state_string(s, out)?
        }
        V::Array(a) => {
            if a.len() > 1024 {
                return Err(SourceContractReplayError::Capacity);
            }
            out.push(6);
            out.extend_from_slice(&(a.len() as u32).to_be_bytes());
            for v in a {
                encode_state(v, depth + 1, nodes, out)?
            }
        }
        V::Object(a) => {
            if a.len() > 1024 {
                return Err(SourceContractReplayError::Capacity);
            }
            out.push(7);
            out.extend_from_slice(&(a.len() as u32).to_be_bytes());
            for (k, v) in a {
                state_string(k, out)?;
                encode_state(v, depth + 1, nodes, out)?
            }
        }
        V::Promise => out.push(8),
        V::BigNumber(s) => {
            out.push(9);
            state_string(s, out)?
        }
    }
    if out.len() > MAX_STATE_BYTES {
        return Err(SourceContractReplayError::Capacity);
    }
    Ok(())
}
fn entry_head(
    previous: [u8; 32],
    sequence: u64,
    command: &[u8],
    outcome: &[u8],
    state: [u8; 32],
    effects: [u8; 32],
) -> Result<[u8; 32]> {
    let mut bytes = b"TradeLayer/SourceContractJournal/v1/".to_vec();
    bytes.extend_from_slice(&previous);
    bytes.extend_from_slice(&sequence.to_be_bytes());
    put_frame(&mut bytes, command)?;
    put_frame(&mut bytes, outcome)?;
    bytes.extend_from_slice(&state);
    bytes.extend_from_slice(&effects);
    Ok(sha256(&bytes))
}
impl SourceContractReplay {
    pub fn new() -> Result<Self> {
        Self::with_policy(SourceContractRuntimePolicy::GlobalClearingV2)
    }
    pub fn with_policy(policy: SourceContractRuntimePolicy) -> Result<Self> {
        let runtime = SourceContractRuntime::with_policy(policy);
        let initial = state_commitment(&runtime)?;
        let mut seed = match policy {
            SourceContractRuntimePolicy::OnchainV1 => {
                b"TradeLayer/SourceContractBootstrap/v1/".to_vec()
            }
            SourceContractRuntimePolicy::GlobalClearingV2 => {
                b"TradeLayer/SourceContractBootstrap/v2/".to_vec()
            }
            SourceContractRuntimePolicy::SourceLifecycleV3 => {
                b"TradeLayer/SourceContractBootstrap/v3/".to_vec()
            }
            SourceContractRuntimePolicy::OptionLifecycleV4 => {
                b"TradeLayer/SourceContractBootstrap/v4/".to_vec()
            }
            SourceContractRuntimePolicy::DistributionLifecycleV5 => {
                b"TradeLayer/SourceContractBootstrap/v5/".to_vec()
            }
            SourceContractRuntimePolicy::ProofLifecycleV6 => {
                b"TradeLayer/SourceContractBootstrap/v6/".to_vec()
            }
            SourceContractRuntimePolicy::LowerLifecycleV7 => {
                b"TradeLayer/SourceContractBootstrap/v7/".to_vec()
            }
            SourceContractRuntimePolicy::MainCallableV8 => {
                b"TradeLayer/SourceContractBootstrap/v8/".to_vec()
            }
        };
        seed.extend_from_slice(SOURCE_PIN);
        seed.extend_from_slice(&initial);
        Ok(Self {
            runtime,
            initial,
            head: SourceContractReplayHead {
                commands: 0,
                commitment: sha256(&seed),
            },
            journal: vec![],
            encoded_bytes: 77,
        })
    }
    pub fn runtime(&self) -> &SourceContractRuntime {
        &self.runtime
    }
    pub fn head(&self) -> SourceContractReplayHead {
        self.head
    }
    pub fn receipts(&self) -> impl Iterator<Item = &SourceContractReceipt> {
        self.journal.iter().map(|e| &e.receipt)
    }
    pub fn commands(&self) -> impl Iterator<Item = &SourceContractCommand> {
        self.journal.iter().map(|e| &e.command)
    }
    pub fn apply(&mut self, command: SourceContractCommand) -> Result<SourceContractReceipt> {
        if self.journal.len() >= MAX_COMMANDS {
            return Err(SourceContractReplayError::Capacity);
        }
        let command_frame = diagnostic(&command.value())?;
        if SourceContractCommand::from_value(&command.value())? != command {
            return Err(SourceContractReplayError::InvalidCommand);
        }
        let before = self.runtime.clone();
        let actual = match &command {
            SourceContractCommand::Import { collection, rows } => self
                .runtime
                .import_collection(collection, rows.clone())
                .map(|()| SourceContractCommandOutcome::Void),
            SourceContractCommand::SetEntry(v) => self
                .runtime
                .set_entry(v)
                .map(|()| SourceContractCommandOutcome::Void),
            SourceContractCommand::SetHeight(v) => self
                .runtime
                .set_height(v.clone())
                .map(|()| SourceContractCommandOutcome::Void),
            SourceContractCommand::SetActivation(v) => {
                self.runtime.set_activation_answer(*v);
                Ok(SourceContractCommandOutcome::Void)
            }
            SourceContractCommand::FailStorage(v) => self
                .runtime
                .fail_storage(v.clone())
                .map(|()| SourceContractCommandOutcome::Void),
            SourceContractCommand::Invoke(v) => self
                .runtime
                .invoke_dependency(v)
                .map(SourceContractCommandOutcome::Dependency),
            SourceContractCommand::EvaluateQueue(v) => self
                .runtime
                .evaluate_queue(v)
                .map(SourceContractCommandOutcome::Source),
            SourceContractCommand::ExecuteContract(v) => self
                .runtime
                .execute_contract_unverified(v)
                .map(SourceContractCommandOutcome::Source),
        };
        let outcome = match actual {
            Ok(v) => v,
            Err(e) => {
                self.runtime = before.clone();
                SourceContractCommandOutcome::LocalError(e)
            }
        };
        let effects = self.runtime.take_effects();
        let prepared = (|| {
            let outcome_frame = diagnostic(&outcome.value())?;
            let effects_commitment = effect_commitment(&effects)?;
            let state_commitment = state_commitment(&self.runtime)?;
            let sequence = self.journal.len() as u64;
            let head = entry_head(
                self.head.commitment,
                sequence,
                &command_frame,
                &outcome_frame,
                state_commitment,
                effects_commitment,
            )?;
            let receipt = SourceContractReceipt {
                sequence,
                previous_head: self.head.commitment,
                outcome,
                effects,
                state_commitment,
                effects_commitment,
                head,
            };
            let size = encoded_entry_size(&command_frame, &outcome_frame, &receipt.effects)?;
            if self.encoded_bytes.saturating_add(size) > MAX_CHECKPOINT_BYTES {
                return Err(SourceContractReplayError::Capacity);
            }
            Ok((receipt, size))
        })();
        match prepared {
            Ok((receipt, size)) => {
                self.head = SourceContractReplayHead {
                    commands: receipt.sequence + 1,
                    commitment: receipt.head,
                };
                self.encoded_bytes += size;
                self.journal.push(Entry {
                    command,
                    receipt: receipt.clone(),
                });
                Ok(receipt)
            }
            Err(e) => {
                self.runtime = before;
                Err(e)
            }
        }
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>> {
        let mut bytes = match self.runtime.policy() {
            SourceContractRuntimePolicy::OnchainV1 => MAGIC_V1.to_vec(),
            SourceContractRuntimePolicy::GlobalClearingV2 => MAGIC_V2.to_vec(),
            SourceContractRuntimePolicy::SourceLifecycleV3 => MAGIC_V3.to_vec(),
            SourceContractRuntimePolicy::OptionLifecycleV4 => MAGIC_V4.to_vec(),
            SourceContractRuntimePolicy::DistributionLifecycleV5 => MAGIC_V5.to_vec(),
            SourceContractRuntimePolicy::ProofLifecycleV6 => MAGIC_V6.to_vec(),
            SourceContractRuntimePolicy::LowerLifecycleV7 => MAGIC_V7.to_vec(),
            SourceContractRuntimePolicy::MainCallableV8 => MAGIC_V8.to_vec(),
        };
        bytes.extend_from_slice(&self.initial);
        bytes.extend_from_slice(&(self.journal.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.head.commitment);
        for entry in &self.journal {
            let r = &entry.receipt;
            bytes.extend_from_slice(&r.sequence.to_be_bytes());
            bytes.extend_from_slice(&r.previous_head);
            put_frame(&mut bytes, &diagnostic(&entry.command.value())?)?;
            put_frame(&mut bytes, &diagnostic(&r.outcome.value())?)?;
            bytes.extend_from_slice(&(r.effects.len() as u32).to_be_bytes());
            for effect in &r.effects {
                put_frame(&mut bytes, &diagnostic(effect)?)?;
            }
            bytes.extend_from_slice(&r.state_commitment);
            bytes.extend_from_slice(&r.effects_commitment);
            bytes.extend_from_slice(&r.head);
        }
        if bytes.len().saturating_add(4) > MAX_CHECKPOINT_BYTES {
            return Err(SourceContractReplayError::Capacity);
        }
        let bytes = with_crc(bytes).map_err(|_| SourceContractReplayError::Capacity)?;
        if bytes.len() != self.encoded_bytes {
            return Err(SourceContractReplayError::InvalidCheckpoint);
        }
        Ok(bytes)
    }
    /// expected_head must come from an independently retained journal/CAS pin,
    /// never from parsing the checkpoint being admitted.
    pub fn from_checkpoint(bytes: &[u8], expected_head: SourceContractReplayHead) -> Result<Self> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(SourceContractReplayError::Capacity);
        }
        let payload =
            verify_crc(bytes).map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
        let mut c = Cursor::new(payload);
        let policy = match c
            .take(5)
            .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?
        {
            magic if magic == MAGIC_V1 => SourceContractRuntimePolicy::OnchainV1,
            magic if magic == MAGIC_V2 => SourceContractRuntimePolicy::GlobalClearingV2,
            magic if magic == MAGIC_V3 => SourceContractRuntimePolicy::SourceLifecycleV3,
            magic if magic == MAGIC_V4 => SourceContractRuntimePolicy::OptionLifecycleV4,
            magic if magic == MAGIC_V5 => SourceContractRuntimePolicy::DistributionLifecycleV5,
            magic if magic == MAGIC_V6 => SourceContractRuntimePolicy::ProofLifecycleV6,
            magic if magic == MAGIC_V7 => SourceContractRuntimePolicy::LowerLifecycleV7,
            magic if magic == MAGIC_V8 => SourceContractRuntimePolicy::MainCallableV8,
            _ => return Err(SourceContractReplayError::InvalidCheckpoint),
        };
        let initial = c
            .fixed::<32>()
            .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
        let count =
            c.u32()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)? as usize;
        if count > MAX_COMMANDS {
            return Err(SourceContractReplayError::Capacity);
        }
        let head = c
            .fixed::<32>()
            .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
        if expected_head.commands != count as u64 || expected_head.commitment != head {
            return Err(SourceContractReplayError::WrongHead);
        }
        let mut replay = Self::with_policy(policy)?;
        if replay.initial != initial {
            return Err(SourceContractReplayError::InvalidCheckpoint);
        }
        for index in 0..count {
            let sequence = c
                .u64()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
            let previous = c
                .fixed::<32>()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
            if sequence != index as u64 || previous != replay.head.commitment {
                return Err(SourceContractReplayError::InvalidCheckpoint);
            }
            let command = SourceContractCommand::from_value(&read_frame(&mut c)?)?;
            let outcome = SourceContractCommandOutcome::from_value(&read_frame(&mut c)?)?;
            let effects_len = c
                .u32()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?
                as usize;
            if effects_len > MAX_EFFECTS {
                return Err(SourceContractReplayError::Capacity);
            }
            let mut effects = vec![];
            for _ in 0..effects_len {
                effects.push(read_frame(&mut c)?)
            }
            let state_commitment = c
                .fixed::<32>()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
            let effects_commitment = c
                .fixed::<32>()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
            let head = c
                .fixed::<32>()
                .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
            let expected = SourceContractReceipt {
                sequence,
                previous_head: previous,
                outcome,
                effects,
                state_commitment,
                effects_commitment,
                head,
            };
            let actual = replay.apply(command)?;
            if actual != expected {
                return Err(SourceContractReplayError::ReplayMismatch);
            }
        }
        c.end()
            .map_err(|_| SourceContractReplayError::InvalidCheckpoint)?;
        if replay.head != expected_head || replay.checkpoint()? != bytes {
            return Err(SourceContractReplayError::ReplayMismatch);
        }
        Ok(replay)
    }
}
fn encoded_entry_size(command: &[u8], outcome: &[u8], effects: &[V]) -> Result<usize> {
    let mut size = 8 + 32 + 4 + command.len() + 4 + outcome.len() + 4 + 96;
    for effect in effects {
        size = size
            .checked_add(4 + diagnostic(effect)?.len())
            .ok_or(SourceContractReplayError::Capacity)?;
    }
    Ok(size)
}

include!("source_transition_witness_impl.rs");
include!("source_lifecycle_transition_witness_impl.rs");
