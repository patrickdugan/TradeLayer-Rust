//! In-protocol ZK verification, phase 1 (`ZkVerifyV1`): transaction type 36 and the Registry
//! action `RegisterZkProgram` (`docs/ZK_VERIFIER.md`).
//!
//! This module holds the pure parts: the wire frame, program identity, the statement binding
//! element, the per-block budget and the narrow [`ZkVerifier`] boundary. State lives in the
//! registry (`registry_zk.rs`) and the fee and block loop live in the joint owner. The Nova
//! verifier (`zk_nova`) plugs in at [`ZkNovaAdapter`].

use std::sync::OnceLock;

use crate::hash256::sha256;
use crate::ledger::AccountId;
use crate::zk_nova::{self, field::Fq, GeneratorCache, NovaVerifyError, RegisteredNovaKey};

/// Transaction type 36, `ZK Verify`. The legacy ASCII frame cannot carry it (its type selector is
/// one base-36 digit), so type 36 has its own binary frame: `"tl"`, the raw byte 0x24, the body.
pub const ZK_TX_TYPE: u8 = 36;
pub const ZK_FRAME_TYPE_BYTE: u8 = 0x24;
/// Admin-only binary frame for `RegisterZkProgram`: `"tl"`, the raw byte 0x25, the body.
pub const ZK_REGISTER_FRAME_BYTE: u8 = 0x25;
/// Admin-only binary frame that activates type 36: `"tl"`, the raw byte 0x26, the code hash. The
/// source activation rule (`txTypesToActivate`) rejects any type above 35, so it cannot name 36.
pub const ZK_ACTIVATE_FRAME_BYTE: u8 = 0x26;
/// Prefix of the source-runner row that carries a ZK frame: `"~zk:"` then the frame in lowercase
/// hex. `~` is no base-36 digit, so no source type uses it.
pub const ZK_ROW_PREFIX: &str = "~zk:";
/// Largest frame the node path carries. The source runner bounds a retained row to 64 KiB and the
/// row holds the frame in hex.
pub const MAX_ZK_NODE_FRAME_BYTES: usize = 30_000;

/// At most this many type-36 transactions are charged against one block.
pub const MAX_ZK_VERIFY_PER_BLOCK: usize = 8;
/// Sum of proof bytes of all type-36 transactions that reach the budget check in one block.
pub const MAX_ZK_PROOF_BYTES: usize = 256 * 1024;
/// Largest single proof (including its 4-byte step-count envelope).
pub const MAX_ZK_TX_PROOF_BYTES: usize = 64 * 1024;
/// Sum of `max_verify_ms` of the programs verified in one block.
pub const MAX_ZK_BLOCK_VERIFY_MS: u64 = 4_000;
pub const MAX_ZK_PUBLIC_INPUTS: usize = 64;
pub const MAX_ZK_KEY_BYTES: usize = 4096;
pub const MAX_ZK_PROGRAMS: usize = 4096;
pub const MAX_ZK_RECEIPTS: usize = 100_000;
/// Hard decode ceiling for a whole frame.
pub const MAX_ZK_FRAME_BYTES: usize = 128 * 1024;

pub const MOCK_PROOF_MAGIC: &[u8] = b"TL-MOCK-ZK-PROOF/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZkSystem {
    /// Reserved; registration and use are rejected.
    Stark = 1,
    /// Nova over Pallas/Vesta (`docs/ZK_VERIFIER_NOVA.md`).
    Nova = 2,
}
impl ZkSystem {
    pub const fn code(self) -> u8 {
        self as u8
    }
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Stark),
            2 => Some(Self::Nova),
            _ => None,
        }
    }
}

/// Phase 1 has one effect. Custody-moving effects are phase 2 and are not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZkEffect {
    StatementReceipt = 1,
}
impl ZkEffect {
    pub const fn code(self) -> u8 {
        self as u8
    }
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::StatementReceipt),
            _ => None,
        }
    }
}

/// A registered statement program. Immutable once recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkProgram {
    pub system: ZkSystem,
    pub program_id: [u8; 32],
    pub key_bytes: Vec<u8>,
    /// Count of 32-byte field elements, including the binding element at index 0.
    pub public_input_layout: u32,
    pub effect: ZkEffect,
    pub verifier_version: u32,
    pub max_verify_ms: u32,
}

/// `program_id` = SHA-256 over the canonical encoding of every other field.
pub fn program_id_of(
    system: ZkSystem,
    key_bytes: &[u8],
    public_input_layout: u32,
    effect: ZkEffect,
    verifier_version: u32,
    max_verify_ms: u32,
) -> [u8; 32] {
    let mut out = b"TradeLayer/ZkProgram/v1/".to_vec();
    out.push(system.code());
    out.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(key_bytes);
    out.extend_from_slice(&public_input_layout.to_be_bytes());
    out.push(effect.code());
    out.extend_from_slice(&verifier_version.to_be_bytes());
    out.extend_from_slice(&max_verify_ms.to_be_bytes());
    sha256(&out)
}

/// What a verified type 36 records. No balance moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementReceipt {
    pub program_id: [u8; 32],
    pub public_inputs_hash: [u8; 32],
    pub sender: AccountId,
    pub height: u64,
    pub txid: crate::ledger::TxId,
}

/// Decoded type-36 payload. `proof` carries a 4-byte big-endian step count followed by the proof
/// bytes proper (see [`split_proof_envelope`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkVerifyPayload {
    pub system: u8,
    pub program_id: [u8; 32],
    pub public_inputs: Vec<[u8; 32]>,
    pub proof: Vec<u8>,
    pub expiry: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkWireError {
    NotZkFrame,
    Truncated,
    TooLong,
    TooManyInputs,
    TrailingBytes,
}
impl core::fmt::Display for ZkWireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ZkWireError {}

impl ZkVerifyPayload {
    /// `"tl" 0x24 | system u8 | program_id 32 | expiry u64 | n u32 | n*32 | proof_len u32 | proof`
    /// (all integers big-endian).
    pub fn encode_frame(&self) -> Vec<u8> {
        let mut out = vec![b't', b'l', ZK_FRAME_TYPE_BYTE, self.system];
        out.extend_from_slice(&self.program_id);
        out.extend_from_slice(&self.expiry.to_be_bytes());
        out.extend_from_slice(&(self.public_inputs.len() as u32).to_be_bytes());
        for input in &self.public_inputs {
            out.extend_from_slice(input);
        }
        out.extend_from_slice(&(self.proof.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.proof);
        out
    }

    pub fn decode_frame(raw: &[u8]) -> Result<Self, ZkWireError> {
        if raw.len() > MAX_ZK_FRAME_BYTES {
            return Err(ZkWireError::TooLong);
        }
        if raw.len() < 3 || &raw[..2] != b"tl" || raw[2] != ZK_FRAME_TYPE_BYTE {
            return Err(ZkWireError::NotZkFrame);
        }
        let mut at = 3usize;
        let mut take = |n: usize| -> Result<&[u8], ZkWireError> {
            let end = at.checked_add(n).ok_or(ZkWireError::Truncated)?;
            let slice = raw.get(at..end).ok_or(ZkWireError::Truncated)?;
            at = end;
            Ok(slice)
        };
        let system = take(1)?[0];
        let program_id: [u8; 32] = take(32)?.try_into().map_err(|_| ZkWireError::Truncated)?;
        let expiry = u64::from_be_bytes(take(8)?.try_into().map_err(|_| ZkWireError::Truncated)?);
        let n = u32::from_be_bytes(take(4)?.try_into().map_err(|_| ZkWireError::Truncated)?);
        if n as usize > MAX_ZK_PUBLIC_INPUTS {
            return Err(ZkWireError::TooManyInputs);
        }
        let mut public_inputs = Vec::with_capacity(n as usize);
        for _ in 0..n {
            public_inputs.push(take(32)?.try_into().map_err(|_| ZkWireError::Truncated)?);
        }
        let proof_len =
            u32::from_be_bytes(take(4)?.try_into().map_err(|_| ZkWireError::Truncated)?);
        let proof = take(proof_len as usize)?.to_vec();
        if at != raw.len() {
            return Err(ZkWireError::TrailingBytes);
        }
        Ok(Self {
            system,
            program_id,
            public_inputs,
            proof,
            expiry,
        })
    }
}

/// `RegisterZkProgram` as it travels on the node path. The registry derives `program_id` from the
/// other fields and refuses a mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkRegisterPayload {
    pub system: u8,
    pub program_id: [u8; 32],
    pub key_bytes: Vec<u8>,
    pub public_input_layout: u32,
    pub effect: u8,
    pub verifier_version: u32,
    pub max_verify_ms: u32,
}

impl ZkRegisterPayload {
    /// `"tl" 0x25 | system u8 | program_id 32 | layout u32 | effect u8 | verifier_version u32 |
    /// max_verify_ms u32 | key_len u32 | key` (all integers big-endian).
    pub fn encode_frame(&self) -> Vec<u8> {
        let mut out = vec![b't', b'l', ZK_REGISTER_FRAME_BYTE, self.system];
        out.extend_from_slice(&self.program_id);
        out.extend_from_slice(&self.public_input_layout.to_be_bytes());
        out.push(self.effect);
        out.extend_from_slice(&self.verifier_version.to_be_bytes());
        out.extend_from_slice(&self.max_verify_ms.to_be_bytes());
        out.extend_from_slice(&(self.key_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.key_bytes);
        out
    }

    pub fn decode_frame(raw: &[u8]) -> Result<Self, ZkWireError> {
        if raw.len() > MAX_ZK_FRAME_BYTES {
            return Err(ZkWireError::TooLong);
        }
        if raw.len() < 3 || &raw[..2] != b"tl" || raw[2] != ZK_REGISTER_FRAME_BYTE {
            return Err(ZkWireError::NotZkFrame);
        }
        let mut at = 3usize;
        let mut take = |n: usize| -> Result<&[u8], ZkWireError> {
            let end = at.checked_add(n).ok_or(ZkWireError::Truncated)?;
            let slice = raw.get(at..end).ok_or(ZkWireError::Truncated)?;
            at = end;
            Ok(slice)
        };
        let be32 = |b: &[u8]| -> Result<u32, ZkWireError> {
            Ok(u32::from_be_bytes(
                b.try_into().map_err(|_| ZkWireError::Truncated)?,
            ))
        };
        let system = take(1)?[0];
        let program_id: [u8; 32] = take(32)?.try_into().map_err(|_| ZkWireError::Truncated)?;
        let public_input_layout = be32(take(4)?)?;
        let effect = take(1)?[0];
        let verifier_version = be32(take(4)?)?;
        let max_verify_ms = be32(take(4)?)?;
        let key_len = be32(take(4)?)? as usize;
        if key_len > MAX_ZK_KEY_BYTES {
            return Err(ZkWireError::TooLong);
        }
        let key_bytes = take(key_len)?.to_vec();
        if at != raw.len() {
            return Err(ZkWireError::TrailingBytes);
        }
        Ok(Self {
            system,
            program_id,
            key_bytes,
            public_input_layout,
            effect,
            verifier_version,
            max_verify_ms,
        })
    }

    /// The registry action, or `None` when `system` or `effect` is no known code.
    pub fn into_action(self) -> Option<crate::registry::RegistryAction> {
        Some(crate::registry::RegistryAction::RegisterZkProgram {
            system: ZkSystem::from_code(self.system)?,
            program_id: self.program_id,
            key_bytes: self.key_bytes,
            public_input_layout: self.public_input_layout,
            effect: ZkEffect::from_code(self.effect)?,
            verifier_version: self.verifier_version,
            max_verify_ms: self.max_verify_ms,
        })
    }
}

/// Activation of type 36 as it travels on the node path: `"tl" 0x26 | code_hash 32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZkActivatePayload {
    pub code_hash: [u8; 32],
}

impl ZkActivatePayload {
    pub fn encode_frame(&self) -> Vec<u8> {
        let mut out = vec![b't', b'l', ZK_ACTIVATE_FRAME_BYTE];
        out.extend_from_slice(&self.code_hash);
        out
    }

    pub fn decode_frame(raw: &[u8]) -> Result<Self, ZkWireError> {
        if raw.len() < 3 || &raw[..2] != b"tl" || raw[2] != ZK_ACTIVATE_FRAME_BYTE {
            return Err(ZkWireError::NotZkFrame);
        }
        let body = &raw[3..];
        let code_hash: [u8; 32] = body
            .get(..32)
            .ok_or(ZkWireError::Truncated)?
            .try_into()
            .map_err(|_| ZkWireError::Truncated)?;
        if body.len() != 32 {
            return Err(ZkWireError::TrailingBytes);
        }
        Ok(Self { code_hash })
    }

    pub fn into_action(self) -> crate::registry::RegistryAction {
        crate::registry::RegistryAction::Activate {
            types: vec![ZK_TX_TYPE],
            code_hash: self.code_hash,
        }
    }
}

/// Any frame of the type-36 family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkNodeFrame {
    Verify(ZkVerifyPayload),
    Register(ZkRegisterPayload),
    Activate(ZkActivatePayload),
}

impl ZkNodeFrame {
    /// True when `raw` starts with `"tl"` and one of the three frame bytes.
    pub fn is_zk_frame(raw: &[u8]) -> bool {
        raw.len() >= 3
            && &raw[..2] == b"tl"
            && matches!(
                raw[2],
                ZK_FRAME_TYPE_BYTE | ZK_REGISTER_FRAME_BYTE | ZK_ACTIVATE_FRAME_BYTE
            )
    }

    pub fn decode(raw: &[u8]) -> Result<Self, ZkWireError> {
        if raw.len() < 3 || &raw[..2] != b"tl" {
            return Err(ZkWireError::NotZkFrame);
        }
        match raw[2] {
            ZK_FRAME_TYPE_BYTE => ZkVerifyPayload::decode_frame(raw).map(Self::Verify),
            ZK_REGISTER_FRAME_BYTE => ZkRegisterPayload::decode_frame(raw).map(Self::Register),
            ZK_ACTIVATE_FRAME_BYTE => ZkActivatePayload::decode_frame(raw).map(Self::Activate),
            _ => Err(ZkWireError::NotZkFrame),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Verify(p) => p.encode_frame(),
            Self::Register(p) => p.encode_frame(),
            Self::Activate(p) => p.encode_frame(),
        }
    }
}

/// The source-runner payload of a frame: [`ZK_ROW_PREFIX`] and the frame in lowercase hex.
pub fn zk_row_payload(frame: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(ZK_ROW_PREFIX.len() + frame.len() * 2);
    out.push_str(ZK_ROW_PREFIX);
    for byte in frame {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 15)] as char);
    }
    out
}

/// The frame bytes of a row payload, or `None` when it lacks the prefix or the hex is not
/// canonical lowercase.
pub fn parse_zk_row_payload(payload: &str) -> Option<Vec<u8>> {
    let hex = payload.strip_prefix(ZK_ROW_PREFIX)?.as_bytes();
    if hex.len() % 2 != 0 {
        return None;
    }
    let nibble = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    hex.chunks(2)
        .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

/// Splits the payload's `proof` into the Nova step count and the proof proper.
pub fn split_proof_envelope(proof: &[u8]) -> Option<(u32, &[u8])> {
    let head: [u8; 4] = proof.get(..4)?.try_into().ok()?;
    let steps = u32::from_be_bytes(head);
    (steps >= 1).then(|| (steps, &proof[4..]))
}

/// Hash of the public inputs as recorded in receipts.
pub fn public_inputs_hash(public_inputs: &[[u8; 32]]) -> [u8; 32] {
    let mut out = b"TradeLayer/ZkPublicInputs/v1/".to_vec();
    out.extend_from_slice(&(public_inputs.len() as u32).to_be_bytes());
    for input in public_inputs {
        out.extend_from_slice(input);
    }
    sha256(&out)
}

/// The values the node derives itself. The sender is the authenticated actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZkBindingContext {
    pub genesis: [u8; 32],
    pub anchor: [u8; 32],
}

/// Leading public input of every program: genesis hash, anchor, sender, expiry and program id,
/// compressed to one field element (SHA-256 with the top three bits of the last byte cleared, so
/// the little-endian value is below 2^253 and canonical in Pallas and Vesta).
pub fn binding_element(
    ctx: &ZkBindingContext,
    sender: &AccountId,
    expiry: u64,
    program_id: &[u8; 32],
) -> [u8; 32] {
    let mut out = b"TradeLayer/ZkBinding/v1/".to_vec();
    out.extend_from_slice(&ctx.genesis);
    out.extend_from_slice(&ctx.anchor);
    out.extend_from_slice(&(sender.as_str().len() as u32).to_be_bytes());
    out.extend_from_slice(sender.as_str().as_bytes());
    out.extend_from_slice(&expiry.to_be_bytes());
    out.extend_from_slice(program_id);
    let mut element = sha256(&out);
    element[31] &= 0x1f;
    element
}

/// Verifier verdicts. Typed, never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZkReject {
    UnsupportedSystem,
    MalformedKey,
    MalformedProof,
    InvalidProof,
}

/// What a verifier attests it checked. Consensus requires `public_inputs_hash` to equal the hash
/// it computed itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedStatement {
    pub public_inputs_hash: [u8; 32],
}

/// The only boundary between consensus and a proof system. Implementations are pure functions of
/// their arguments.
pub trait ZkVerifier {
    fn verify(
        &self,
        system: ZkSystem,
        key_bytes: &[u8],
        num_steps: u32,
        public_inputs: &[[u8; 32]],
        proof: &[u8],
    ) -> Result<VerifiedStatement, ZkReject>;
}

/// Deterministic test verifier: accepts exactly [`MOCK_PROOF_MAGIC`] under Nova with a nonempty
/// key. It counts calls so tests can show a rejected transaction never reached it.
#[derive(Debug, Default)]
pub struct MockZkVerifier {
    calls: std::sync::atomic::AtomicUsize,
}
impl MockZkVerifier {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}
impl ZkVerifier for MockZkVerifier {
    fn verify(
        &self,
        system: ZkSystem,
        key_bytes: &[u8],
        num_steps: u32,
        public_inputs: &[[u8; 32]],
        proof: &[u8],
    ) -> Result<VerifiedStatement, ZkReject> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if system != ZkSystem::Nova {
            return Err(ZkReject::UnsupportedSystem);
        }
        if key_bytes.is_empty() {
            return Err(ZkReject::MalformedKey);
        }
        if num_steps == 0 || public_inputs.is_empty() {
            return Err(ZkReject::MalformedProof);
        }
        if proof != MOCK_PROOF_MAGIC {
            return Err(ZkReject::InvalidProof);
        }
        Ok(VerifiedStatement {
            public_inputs_hash: public_inputs_hash(public_inputs),
        })
    }
}

/// Receipt reason of a verified type 36 on the node path.
pub const ZK_REASON_VERIFIED: &str = "zk:verified";

/// Receipt reason of a type 36 that was consumed and rejected (the fee is kept).
pub fn zk_reason_rejected(reason: &ZkRejectReason) -> String {
    format!("zk:rejected:{reason:?}")
}

/// Replays the verifier's recorded verdict. The node verifies a proof once, when it first applies
/// the block, and writes the verdict into the receipt's reason. Rebuilding a state (checkpoint
/// verification, rollback) runs every other rule again and takes only the proof math from this
/// record, so no replay repeats a multi-second verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedZkVerifier {
    verdict: RecordedVerdict,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordedVerdict {
    Accept,
    Reject(ZkReject),
    /// The recorded outcome was `StatementMismatch`: the verifier answered with another hash.
    WrongStatement,
    /// The record shows the verifier was not reached; a call now contradicts it.
    NotReached,
}
impl RecordedZkVerifier {
    pub fn from_reason(reason: &str) -> Self {
        let verdict = if reason == ZK_REASON_VERIFIED {
            RecordedVerdict::Accept
        } else if reason == zk_reason_rejected(&ZkRejectReason::StatementMismatch) {
            RecordedVerdict::WrongStatement
        } else {
            [
                ZkReject::UnsupportedSystem,
                ZkReject::MalformedKey,
                ZkReject::MalformedProof,
                ZkReject::InvalidProof,
            ]
            .into_iter()
            .find(|r| reason == zk_reason_rejected(&ZkRejectReason::Verifier(*r)))
            .map_or(RecordedVerdict::NotReached, RecordedVerdict::Reject)
        };
        Self { verdict }
    }
}
impl ZkVerifier for RecordedZkVerifier {
    fn verify(
        &self,
        _system: ZkSystem,
        _key_bytes: &[u8],
        _num_steps: u32,
        public_inputs: &[[u8; 32]],
        _proof: &[u8],
    ) -> Result<VerifiedStatement, ZkReject> {
        match self.verdict {
            RecordedVerdict::Accept => Ok(VerifiedStatement {
                public_inputs_hash: public_inputs_hash(public_inputs),
            }),
            RecordedVerdict::Reject(reject) => Err(reject),
            RecordedVerdict::WrongStatement => Ok(VerifiedStatement {
                public_inputs_hash: [0; 32],
            }),
            RecordedVerdict::NotReached => Err(ZkReject::MalformedProof),
        }
    }
}

/// Registration check of a Nova program key, run by `RegisterZkProgram`: `key_bytes` decodes as
/// a [`RegisteredNovaKey`] (every shape nova can produce, nothing else), each curve's generator
/// count equals its `N` (as nova's setup emits), and the statement layout is `z0 || zn`, so
/// `public_input_layout = 2 * F_arity`. Pure and cheap: no generator work.
pub fn check_nova_key(key_bytes: &[u8], public_input_layout: u32) -> Result<(), ZkReject> {
    let key = RegisteredNovaKey::decode(key_bytes).map_err(|_| ZkReject::MalformedKey)?;
    if key.primary.ck_len != key.primary.spark.n || key.secondary.ck_len != key.secondary.spark.n {
        return Err(ZkReject::MalformedKey);
    }
    if u64::from(public_input_layout) != key.f_arity.saturating_mul(2) {
        return Err(ZkReject::MalformedKey);
    }
    Ok(())
}

/// The Nova verifier (`zk_nova`, nova-snark 0.76 `CompressedSNARK` with ppsnark and IPA).
///
/// Statement layout of a Nova program: `public_inputs = z0 || zn`, `F_arity` canonical `Fq`
/// elements each (32-byte little-endian), with the binding element as `z0[0]`. The proof must take
/// exactly this `z0` to exactly this `zn` in `num_steps` steps. Generator tables are derived once
/// per process and shared by every program.
#[derive(Debug, Default, Clone, Copy)]
pub struct ZkNovaAdapter;

impl ZkNovaAdapter {
    fn generators() -> &'static GeneratorCache {
        static CACHE: OnceLock<GeneratorCache> = OnceLock::new();
        CACHE.get_or_init(GeneratorCache::new)
    }
}

impl ZkVerifier for ZkNovaAdapter {
    fn verify(
        &self,
        system: ZkSystem,
        key_bytes: &[u8],
        num_steps: u32,
        public_inputs: &[[u8; 32]],
        proof: &[u8],
    ) -> Result<VerifiedStatement, ZkReject> {
        if system != ZkSystem::Nova {
            return Err(ZkReject::UnsupportedSystem);
        }
        let key = RegisteredNovaKey::decode(key_bytes).map_err(|_| ZkReject::MalformedKey)?;
        let arity = usize::try_from(key.f_arity).map_err(|_| ZkReject::MalformedKey)?;
        if public_inputs.len() != arity.saturating_mul(2) {
            return Err(ZkReject::MalformedProof);
        }
        let mut elements = Vec::with_capacity(public_inputs.len());
        for input in public_inputs {
            elements.push(Fq::from_repr(input).map_err(|_| ZkReject::MalformedProof)?);
        }
        let (z0, zn) = elements.split_at(arity);
        let proven = zk_nova::verify(&key, Self::generators(), u64::from(num_steps), z0, proof)
            .map_err(|error| match error {
                NovaVerifyError::InvalidKey(_) | NovaVerifyError::Generators(_) => {
                    ZkReject::MalformedKey
                }
                NovaVerifyError::ProofTooLarge { .. }
                | NovaVerifyError::Decode(_)
                | NovaVerifyError::ZeroSteps
                | NovaVerifyError::ArityMismatch
                | NovaVerifyError::InstanceLength => ZkReject::MalformedProof,
                NovaVerifyError::HashBindingMismatch
                | NovaVerifyError::InvalidSumcheckProof
                | NovaVerifyError::InvalidInputLength
                | NovaVerifyError::ZeroChallenge
                | NovaVerifyError::InvalidPcs
                | NovaVerifyError::Internal(_) => ZkReject::InvalidProof,
            })?;
        if proven.as_slice() != zn {
            return Err(ZkReject::InvalidProof);
        }
        Ok(VerifiedStatement {
            public_inputs_hash: public_inputs_hash(public_inputs),
        })
    }
}

/// Per-block counters. One instance per block, cloned state and budget commit together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZkBlockBudget {
    pub count: usize,
    pub proof_bytes: usize,
    pub verify_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZkBudgetError {
    Count,
    ProofBytes,
    VerifyMs,
}
impl ZkBlockBudget {
    /// Charges one transaction, or leaves the counters unchanged and says which cap it hit.
    pub fn charge(&mut self, proof_bytes: usize, verify_ms: u32) -> Result<(), ZkBudgetError> {
        if self.count >= MAX_ZK_VERIFY_PER_BLOCK {
            return Err(ZkBudgetError::Count);
        }
        if self.proof_bytes.saturating_add(proof_bytes) > MAX_ZK_PROOF_BYTES {
            return Err(ZkBudgetError::ProofBytes);
        }
        if self.verify_ms.saturating_add(u64::from(verify_ms)) > MAX_ZK_BLOCK_VERIFY_MS {
            return Err(ZkBudgetError::VerifyMs);
        }
        self.count += 1;
        self.proof_bytes += proof_bytes;
        self.verify_ms += u64::from(verify_ms);
        Ok(())
    }
}

/// Reasons a type-36 transaction that passed activation and paid its fee is rejected. The fee is
/// kept for every one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZkRejectReason {
    Expired,
    UnknownProgram,
    SystemMismatch,
    OversizeProof,
    PublicInputCount,
    BudgetCount,
    BudgetProofBytes,
    BudgetVerifyMs,
    BindingMismatch,
    ProofEnvelope,
    ReceiptsExhausted,
    Verifier(ZkReject),
    StatementMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkOutcome {
    Verified(StatementReceipt),
    Rejected(ZkRejectReason),
}

/// Fee charged to the sender before verification, as a ledger transfer of one property to a
/// configured sink. Kept on failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZkFeePolicy {
    pub property: crate::ledger::PropertyId,
    pub amount_atoms: u128,
    pub sink: AccountId,
}
