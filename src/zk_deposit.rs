//! Parser and policy check for the M16 deposit attestation (`docs/ZK_DEPOSIT_PROOFS.md`).
//!
//! M16 is an SP1 guest whose committed public values are 1,292 bytes: guest A's NHC1 chain
//! statement (1,164 bytes) followed by a 128-byte `PAY1` payment record. This module reads and
//! checks those bytes. It does not verify the SP1 wrap proof: the caller does that with the
//! native verifier and passes the verified statement to [`check_wrap_statement`], which binds the
//! public values to the proof through the committed-values digest.
//!
//! Integers are little-endian. The txid and block hash are in Bitcoin internal byte order.
//! Zero external crates; no consensus wiring.

use crate::hash256::sha256;
use std::collections::BTreeMap;

pub const CHAIN_STATEMENT_LEN: usize = 1164;
pub const PAY1_LEN: usize = 128;
pub const PUBLIC_VALUES_LEN: usize = CHAIN_STATEMENT_LEN + PAY1_LEN;
/// Field elements in the NHC1 state (`z0` and `zn`), 32 bytes each.
pub const STATE_ELEMENTS: usize = 17;
pub const STATE_BYTES: usize = STATE_ELEMENTS * 32;
/// Bitcoin's consensus limit on a script (`MAX_SCRIPT_SIZE`).
pub const MAX_SCRIPT_LEN: u32 = 10_000;

pub const CHAIN_MAGIC: [u8; 4] = *b"NHC1";
pub const PAY1_MAGIC: [u8; 4] = *b"PAY1";

/// SP1 program key (verifying-key digest) of the M16 guest.
pub const M16_PROGRAM_KEY: [u8; 32] =
    hex32("dfc68a5d579b86244a60ba0baee7852348cf0d2a3bb13910ebb95a3e625fcb60");
/// SHA-256 of the compressed Nova verifying key for the mainnet, one-header-per-step circuit.
pub const MAINNET_NOVA_KEY_SHA256: [u8; 32] =
    hex32("6aa50d09397c51d0b89d78b3a918d3fe0962251225c8b6ca91e83199a4e6da1d");
/// SHA-256 of the 544 bytes of the Bitcoin mainnet genesis state `z0`.
pub const MAINNET_GENESIS_Z0_SHA256: [u8; 32] =
    hex32("b6eeaf96639fecb456f742df89c6fb02f54bf94f2484d42c92da21be978bf045");

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("hex digit"),
    }
}

const fn hex32(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    assert!(b.len() == 64);
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

/// What a consumer trusts. Build with [`DepositPolicy::bitcoin_mainnet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositPolicy {
    pub program_key: [u8; 32],
    pub nova_key_sha256: [u8; 32],
    pub genesis_z0_sha256: [u8; 32],
    pub min_confirmations: u32,
}

impl DepositPolicy {
    pub fn bitcoin_mainnet(min_confirmations: u32) -> Self {
        Self {
            program_key: M16_PROGRAM_KEY,
            nova_key_sha256: MAINNET_NOVA_KEY_SHA256,
            genesis_z0_sha256: MAINNET_GENESIS_Z0_SHA256,
            min_confirmations,
        }
    }
}

/// Typed reasons the public values are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepositRefusal {
    Length {
        got: usize,
    },
    ChainMagic,
    /// Headers per Nova step; only the one-header-per-step circuit is pinned.
    StepsPerFold {
        got: u32,
    },
    ZeroSteps,
    GenesisState,
    NovaKey,
    Pay1Magic,
    Pay1Segments,
    /// The payment block is above the proven tip.
    HeightAboveTip,
    /// `confirmations` is not `steps - height + 1`.
    ConfirmationsInconsistent,
    ConfirmationsBelowThreshold {
        got: u32,
        need: u32,
    },
    ScriptLength {
        got: u32,
    },
    ZeroValue,
    /// The wrap statement names a different SP1 program.
    ProgramKey,
    /// The public values do not hash to the digest the verified wrap proof committed.
    CommittedValuesDigest,
}

/// The fields a verified wrap proof exposes that bind the public values (`m16-statement.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapStatement {
    pub program_vk_digest: [u8; 32],
    pub committed_values_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Outpoint {
    /// Internal byte order.
    pub txid: [u8; 32],
    pub vout: u32,
}

/// A checked deposit attestation. It states that the proven header chain contains a transaction
/// output with this script hash and value. It does not state that the chain is the heaviest one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositAttestation {
    pub outpoint: Outpoint,
    pub value_sats: u64,
    pub script_len: u32,
    pub script_sha256: [u8; 32],
    pub block_height: u32,
    pub block_hash: [u8; 32],
    pub confirmations: u32,
    pub tip_height: u32,
    pub segments: u32,
    /// Tip state of the proven chain (height, hash and chainwork among other elements).
    pub tip_state: [u8; STATE_BYTES],
    pub proofs_digest: [u8; 32],
}

impl DepositAttestation {
    /// True when `script` is the output script the attestation commits to.
    pub fn pays_script(&self, script: &[u8]) -> bool {
        script.len() as u64 == u64::from(self.script_len) && sha256(script) == self.script_sha256
    }
}

fn u32_at(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(raw[at..at + 4].try_into().expect("4 bytes"))
}

fn arr32(raw: &[u8], at: usize) -> [u8; 32] {
    raw[at..at + 32].try_into().expect("32 bytes")
}

/// Parses and checks the 1,292 public values against `policy`.
pub fn parse_public_values(
    raw: &[u8],
    policy: &DepositPolicy,
) -> Result<DepositAttestation, DepositRefusal> {
    use DepositRefusal as R;
    if raw.len() != PUBLIC_VALUES_LEN {
        return Err(R::Length { got: raw.len() });
    }
    if raw[0..4] != CHAIN_MAGIC {
        return Err(R::ChainMagic);
    }
    let k = u32_at(raw, 4);
    if k != 1 {
        return Err(R::StepsPerFold { got: k });
    }
    let steps = u32_at(raw, 8);
    if steps == 0 {
        return Err(R::ZeroSteps);
    }
    let z0 = &raw[12..12 + STATE_BYTES];
    if sha256(z0) != policy.genesis_z0_sha256 {
        return Err(R::GenesisState);
    }
    let zn_at = 12 + STATE_BYTES;
    let key_at = zn_at + STATE_BYTES;
    if arr32(raw, key_at) != policy.nova_key_sha256 {
        return Err(R::NovaKey);
    }
    let proofs_digest = arr32(raw, key_at + 32);

    let p = CHAIN_STATEMENT_LEN;
    if raw[p..p + 4] != PAY1_MAGIC {
        return Err(R::Pay1Magic);
    }
    let segments = u32_at(raw, p + 4);
    if segments == 0 {
        return Err(R::Pay1Segments);
    }
    let block_height = u32_at(raw, p + 8);
    let block_hash = arr32(raw, p + 12);
    let confirmations = u32_at(raw, p + 44);
    let txid = arr32(raw, p + 48);
    let vout = u32_at(raw, p + 80);
    let value_sats = u64::from_le_bytes(raw[p + 84..p + 92].try_into().expect("8 bytes"));
    let script_len = u32_at(raw, p + 92);
    let script_sha256 = arr32(raw, p + 96);

    if block_height > steps {
        return Err(R::HeightAboveTip);
    }
    if u64::from(confirmations) != u64::from(steps) - u64::from(block_height) + 1 {
        return Err(R::ConfirmationsInconsistent);
    }
    if confirmations < policy.min_confirmations {
        return Err(R::ConfirmationsBelowThreshold {
            got: confirmations,
            need: policy.min_confirmations,
        });
    }
    if script_len > MAX_SCRIPT_LEN {
        return Err(R::ScriptLength { got: script_len });
    }
    if value_sats == 0 {
        return Err(R::ZeroValue);
    }
    Ok(DepositAttestation {
        outpoint: Outpoint { txid, vout },
        value_sats,
        script_len,
        script_sha256,
        block_height,
        block_hash,
        confirmations,
        tip_height: steps,
        segments,
        tip_state: raw[zn_at..zn_at + STATE_BYTES]
            .try_into()
            .expect("state bytes"),
        proofs_digest,
    })
}

/// SHA-256 of the public values: the digest a wrap proof commits.
pub fn committed_values_sha256(raw: &[u8]) -> [u8; 32] {
    sha256(raw)
}

/// Checks a wrap proof's verified statement against the policy and the public values, then parses
/// them. The wrap proof itself must already have been verified by the native verifier.
pub fn check_wrap_statement(
    statement: &WrapStatement,
    raw: &[u8],
    policy: &DepositPolicy,
) -> Result<DepositAttestation, DepositRefusal> {
    if statement.program_vk_digest != policy.program_key {
        return Err(DepositRefusal::ProgramKey);
    }
    if statement.committed_values_sha256 != committed_values_sha256(raw) {
        return Err(DepositRefusal::CommittedValuesDigest);
    }
    parse_public_values(raw, policy)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayRefusal {
    /// The outpoint was credited already; the block hash it was credited under is returned.
    AlreadyConsumed { block_hash: [u8; 32] },
}

/// Outpoints already credited. An outpoint is credited at most once, whatever block it confirms
/// in, so a reorg that re-confirms the transaction cannot credit it twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsumedOutpoints {
    seen: BTreeMap<Outpoint, [u8; 32]>,
}

impl ConsumedOutpoints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    pub fn contains(&self, outpoint: &Outpoint) -> bool {
        self.seen.contains_key(outpoint)
    }

    /// Records the attestation's outpoint, or refuses when it was recorded before.
    pub fn consume(&mut self, att: &DepositAttestation) -> Result<(), ReplayRefusal> {
        if let Some(block_hash) = self.seen.get(&att.outpoint) {
            return Err(ReplayRefusal::AlreadyConsumed {
                block_hash: *block_hash,
            });
        }
        self.seen.insert(att.outpoint, att.block_hash);
        Ok(())
    }
}
