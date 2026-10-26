//! File 7/49: bounded source-exact legacy txEncoder.js research output.
//!
//! Source: patrickdugan/tradelayer.js@dca0888b src/txEncoder.js (archived
//! byte-identically at fixtures/legacy_types_source_txEncoder.txt).
//! Values are deterministic SOURCE RETURN STRINGS, never an authorized or
//! send-ready transaction. Amount input is expressed in exact token atoms
//! with 1e8 units/token, limited to JavaScript's 2^53-1 safe integer range.
//!
//! Historical source discrepancies preserved in the audit:
//! - encodeAmount / encodeSend omit "~" on fractional amounts because the
//!   nullish-coalescing fallback is reached only when toString is nullish.
//!   Decoder treats the resulting base36 atomic count as TOKEN UNITS.
//! - encodeSend sendAll/array are body-only; scalar is already tl2-framed.
//! - encodeCommit emits tl4, but Types.encodePayload(4) names ABSENT
//!   encodeCommitToken. Multi-entry bracketed clearLists contain literal
//!   commas, which the source decoder splits before bracket inspection.
//! - encodeIssueOrRevokeAttestation interpolates revoke directly; boolean
//!   true becomes "true", while decode requires literal "1".
//! - encodeCloseOracle ignores its ID due to JS operator precedence,
//!   returning "tlf"; Types separately prefixes "tlf" again.
//! - encodeWithdrawal uses DECIMAL scaled amount, not base36, and emits
//!   "true"/"false" for boolean withdrawAll; decodeWithdrawal expects
//!   base36 amount and literal "1" for withdrawAll.
//! - send/commit/transfer have different inner/outer marker framing.
//! - types33-35 return BODY-only, while Type34 and 35 decoder exports are
//!   absent from the historically pinned Types dispatcher.
//!
//! The complete method dispatcher accepts bounded JSON values for source
//! coercion research; the original atom-oriented wrappers remain available.
//! No RPC, scripts, private keys, real addresses, Bitcoin/Litecoin/
//! Dogecoin UTXO, list admin, activation history or settlement is validated.
//! Extra strict ASCII, 4096-byte cap, exact atoms and safe integer bounds
//! are *research fail-closed restrictions*, not historical consensus rules.
//! No trusted transaction/ledger/clearing adapter is produced by this file.

use crate::legacy_types_audit::{
    source_generic_concat_unverified, LegacyTypesError, UnverifiedGenericAssembly,
};

pub const LEGACY_ENCODER_MAX_ASCII: usize = 4_096;
pub const ATOMS_PER_TOKEN: u128 = 100_000_000;
pub const JS_SAFE_INTEGER: u128 = 9_007_199_254_740_991;
pub const MAX_SOURCE_LIST_IDS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyEncoderError {
    MissingOrMalformedField,
    NonPrintableAscii,
    SourceReturnTooLong,
    TooManyListIds,
    MismatchedMultiSendArrays,
    NotExactSafeJavaScriptNumber,
    InvalidBase36Number,
    InvalidActivationHex,
    InvalidActivationType,
    HistoricalTransactionNotAuthenticated,
    HistoricalListAndActivationUnverified,
    CanonicalSignedRevisionUnavailable,
    TrustedEconomicExecutionForbidden,
    UnknownSourceMethod,
    UnsupportedNumberRadix36,
    UnsupportedNumericCoercion,
    JsonInputLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceInnerEnvelope {
    SingleTypePrefixed,
    BodyOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceEncoderCaveat {
    FractionalAmountMissingTilde,
    MultiSendMisparsedAsScalarByDecoder,
    MultiWhitelistCommaShiftsDecoderFields,
    GenericTypesEncoderMissingType4Export,
    CommitClearlistCommasBreakDecoder,
    LongAddressSubstitutedByCallerClaimedRef,
    RawBooleanRevokeDoesNotDecodeAsTrue,
    CloseOracleIgnoresIdAndReturnsPrefixOnly,
    WithdrawalDecimalAmountVersusBase36Decoder,
    WithdrawalBooleanLiteralVersusOneFlag,
    BodyOnlyWithAbsentSourceDecoder,
    BodyOnlyWithoutCalledValidator,
    HistoricalNativeGenesisUnverified,
    NoHistoricalSignerOrNetworkEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedSourceEncoding {
    source_type_id: u8,
    source_inner_result: String,
    prefix_shape: SourceInnerEnvelope,
    caveats: Vec<SourceEncoderCaveat>,
}
impl UnverifiedSourceEncoding {
    pub const fn type_id(&self) -> u8 {
        self.source_type_id
    }
    pub fn source_return(&self) -> &str {
        &self.source_inner_result
    }
    pub const fn source_inner_prefix_shape(&self) -> SourceInnerEnvelope {
        self.prefix_shape
    }
    pub fn caveats(&self) -> &[SourceEncoderCaveat] {
        &self.caveats
    }
    /// Invokes the File5/49 read-only generic concatenation diagnostic,
    /// which refuses Type4's MISSING encodeCommitToken export. This never
    /// returns an authorized, signature-bearing or send-ready transaction.
    pub fn source_generic_types_concat_unverified(
        &self,
    ) -> Result<UnverifiedGenericAssembly, LegacyTypesError> {
        source_generic_concat_unverified(self.source_type_id, &self.source_inner_result)
    }
    pub const fn require_historically_accepted_transaction(
        &self,
    ) -> Result<(), LegacyEncoderError> {
        Err(LegacyEncoderError::HistoricalTransactionNotAuthenticated)
    }
    pub const fn require_authenticated_list_activation(&self) -> Result<(), LegacyEncoderError> {
        Err(LegacyEncoderError::HistoricalListAndActivationUnverified)
    }
    pub const fn require_signed_channel_revision(&self) -> Result<(), LegacyEncoderError> {
        Err(LegacyEncoderError::CanonicalSignedRevisionUnavailable)
    }
    pub const fn require_trusted_execution(&self) -> Result<(), LegacyEncoderError> {
        Err(LegacyEncoderError::TrustedEconomicExecutionForbidden)
    }
}
fn source_output(
    ty: u8,
    result: String,
    prefix: SourceInnerEnvelope,
    mut caveats: Vec<SourceEncoderCaveat>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    if result.len() > LEGACY_ENCODER_MAX_ASCII {
        return Err(LegacyEncoderError::SourceReturnTooLong);
    }
    if !result.is_ascii() || !result.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(LegacyEncoderError::NonPrintableAscii);
    }
    caveats.push(SourceEncoderCaveat::NoHistoricalSignerOrNetworkEvidence);
    Ok(UnverifiedSourceEncoding {
        source_type_id: ty,
        source_inner_result: result,
        prefix_shape: prefix,
        caveats,
    })
}
fn raw_text(raw: &str) -> Result<&str, LegacyEncoderError> {
    if raw.len() > LEGACY_ENCODER_MAX_ASCII {
        return Err(LegacyEncoderError::SourceReturnTooLong);
    }
    if !raw.is_ascii() || !raw.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(LegacyEncoderError::NonPrintableAscii);
    }
    Ok(raw)
}
fn required_text(raw: &str) -> Result<&str, LegacyEncoderError> {
    let s = raw_text(raw)?;
    if s.is_empty() {
        Err(LegacyEncoderError::MissingOrMalformedField)
    } else {
        Ok(s)
    }
}
fn amount_safe(atoms: u128) -> Result<u128, LegacyEncoderError> {
    if atoms > JS_SAFE_INTEGER {
        Err(LegacyEncoderError::NotExactSafeJavaScriptNumber)
    } else {
        Ok(atoms)
    }
}
fn safe_id(id: u128) -> Result<u128, LegacyEncoderError> {
    if id > JS_SAFE_INTEGER {
        Err(LegacyEncoderError::InvalidBase36Number)
    } else {
        Ok(id)
    }
}
fn b36(mut n: u128) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut text = Vec::new();
    const CHARS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    while n != 0 {
        text.push(CHARS[(n % 36) as usize]);
        n /= 36;
    }
    text.reverse();
    String::from_utf8(text).expect("base36 is ASCII")
}
fn b36_id(id: u128) -> Result<String, LegacyEncoderError> {
    Ok(b36(safe_id(id)?))
}
/// Source Encode.encodeAmount() and encodeSend() return amount.toString(36)
/// for integral token units. Nonintegral numbers are scaled to 1e8 and
/// encoded as base36 ATOMS, but the source accidentally omits the "~".
pub fn source_encode_amount_unverified(atoms: u128) -> Result<String, LegacyEncoderError> {
    amount_safe(atoms)?;
    if atoms.is_multiple_of(ATOMS_PER_TOKEN) {
        Ok(b36(atoms / ATOMS_PER_TOKEN))
    } else {
        Ok(b36(atoms))
    }
}

fn code_hash_base36(source_hex: &str) -> Result<String, LegacyEncoderError> {
    // BigInt("0x" + hex).toString(36) supports 256 bits. Reproduce it
    // without third-party crates using repeated division of base256 bytes.
    if source_hex.len() != 64 || !source_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(LegacyEncoderError::InvalidActivationHex);
    }
    let mut bytes: Vec<u8> = source_hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let n = std::str::from_utf8(pair).expect("hex ASCII checked");
            u8::from_str_radix(n, 16).expect("hex digit checked")
        })
        .collect();
    let mut output = Vec::new();
    while bytes.iter().any(|b| *b != 0) {
        let mut remainder: u16 = 0;
        for byte in &mut bytes {
            let dividend = (remainder << 8) + u16::from(*byte);
            *byte = (dividend / 36) as u8;
            remainder = dividend % 36;
        }
        output.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[usize::from(remainder)]);
    }
    if output.is_empty() {
        return Ok("0".into());
    }
    output.reverse();
    Ok(String::from_utf8(output).expect("base36 ASCII"))
}

/// The original method encodes txTypeToActivate joined by semicolon and
/// BigInt(hex) as base36. It does not initialize native tokens itself.
/// This narrower diagnostic accepts only canonical 0..35 source type IDs.
pub fn source_encode_activate_unverified(
    type_ids: &[u8],
    hash_hex_256: &str,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    if type_ids.iter().any(|ty| *ty > 35) {
        return Err(LegacyEncoderError::InvalidActivationType);
    }
    let hash = code_hash_base36(hash_hex_256)?;
    let types = type_ids
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(";");
    source_output(
        0,
        format!("tl0{types},{hash}"),
        SourceInnerEnvelope::SingleTypePrefixed,
        vec![SourceEncoderCaveat::HistoricalNativeGenesisUnverified],
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ClaimedTokenIssue<'a> {
    pub initial_amount: u128,
    pub ticker: &'a str,
    pub whitelists: &'a [u64],
    pub managed: bool,
    pub backup_address: &'a str,
    pub nft: bool,
    pub colored_coin_hybrid: bool,
    pub procedural_type: Option<u64>,
}
pub fn source_encode_token_issue_unverified(
    c: ClaimedTokenIssue<'_>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    required_text(c.ticker)?;
    raw_text(c.backup_address)?;
    if c.whitelists.len() > MAX_SOURCE_LIST_IDS {
        return Err(LegacyEncoderError::TooManyListIds);
    }
    let whites = c
        .whitelists
        .iter()
        .map(|n| b36_id(u128::from(*n)))
        .collect::<Result<Vec<_>, _>>()?
        .join(",");
    let procedural = c
        .procedural_type
        .map(|n| b36_id(u128::from(n)))
        .transpose()?
        .unwrap_or_default();
    let raw = format!(
        "tl1{},{},{},{},{},{},{},{}",
        b36_id(c.initial_amount)?,
        c.ticker,
        whites,
        u8::from(c.managed),
        c.backup_address,
        u8::from(c.nft),
        u8::from(c.colored_coin_hybrid),
        procedural
    );
    source_output(
        1,
        raw,
        SourceInnerEnvelope::SingleTypePrefixed,
        if c.whitelists.len() > 1 {
            vec![SourceEncoderCaveat::MultiWhitelistCommaShiftsDecoderFields]
        } else {
            vec![]
        },
    )
}

#[derive(Debug, Clone, Copy)]
pub enum ClaimedSend<'a> {
    SendAll {
        recipient: &'a str,
    },
    Single {
        recipient: &'a str,
        property_id: u64,
        amount_atoms: u128,
    },
    Multi {
        property_ids: &'a [u64],
        amount_atoms: &'a [u128],
    },
}
pub fn source_encode_send_unverified(
    input: ClaimedSend<'_>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    match input {
        ClaimedSend::SendAll { recipient } => {
            required_text(recipient)?;
            source_output(
                2,
                format!("1;{recipient}"),
                SourceInnerEnvelope::BodyOnly,
                vec![],
            )
        }
        ClaimedSend::Single {
            recipient,
            property_id,
            amount_atoms,
        } => {
            required_text(recipient)?;
            let mut caveats = vec![];
            if !amount_atoms.is_multiple_of(ATOMS_PER_TOKEN) {
                caveats.push(SourceEncoderCaveat::FractionalAmountMissingTilde);
            }
            source_output(
                2,
                format!(
                    "tl20;{recipient};{};{}",
                    b36_id(u128::from(property_id))?,
                    source_encode_amount_unverified(amount_atoms)?
                ),
                SourceInnerEnvelope::SingleTypePrefixed,
                caveats,
            )
        }
        ClaimedSend::Multi {
            property_ids,
            amount_atoms,
        } => {
            if property_ids.len() != amount_atoms.len() {
                return Err(LegacyEncoderError::MismatchedMultiSendArrays);
            }
            if property_ids.len() > MAX_SOURCE_LIST_IDS {
                return Err(LegacyEncoderError::TooManyListIds);
            }
            let ids = property_ids
                .iter()
                .map(|n| b36_id(u128::from(*n)))
                .collect::<Result<Vec<_>, _>>()?
                .join(",");
            let amounts = amount_atoms
                .iter()
                .map(|n| source_encode_amount_unverified(*n))
                .collect::<Result<Vec<_>, _>>()?
                .join(",");
            let mut caveats = vec![SourceEncoderCaveat::MultiSendMisparsedAsScalarByDecoder];
            if amount_atoms
                .iter()
                .any(|n| !n.is_multiple_of(ATOMS_PER_TOKEN))
            {
                caveats.push(SourceEncoderCaveat::FractionalAmountMissingTilde);
            }
            source_output(
                2,
                format!("0;;{ids};{amounts}"),
                SourceInnerEnvelope::BodyOnly,
                caveats,
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ClaimedClearLists<'a> {
    NotSet,
    Scalar(u64),
    Array(&'a [u64]),
}
#[derive(Debug, Clone, Copy)]
pub struct ClaimedCommit<'a> {
    pub property_id: u64,
    pub amount_atoms: u128,
    pub channel_address: &'a str,
    pub ref_vout: u64,
    pub pay_enabled: bool,
    pub clear_lists: ClaimedClearLists<'a>,
    pub colored_output: bool,
    pub commit_clearlist_id: Option<u64>,
}
fn optional_long_ref(address: &str, ref_vout: u64) -> Result<(String, bool), LegacyEncoderError> {
    required_text(address)?;
    if address.len() > 42 {
        Ok((format!("ref:{}", safe_id(u128::from(ref_vout))?), true))
    } else {
        Ok((address.into(), false))
    }
}
pub fn source_encode_commit_unverified(
    c: ClaimedCommit<'_>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    amount_safe(c.amount_atoms)?;
    let (channel, long) = optional_long_ref(c.channel_address, c.ref_vout)?;
    let lists = match c.clear_lists {
        ClaimedClearLists::NotSet => String::new(),
        ClaimedClearLists::Scalar(n) => b36_id(u128::from(n))?,
        ClaimedClearLists::Array(items) => {
            if items.len() > MAX_SOURCE_LIST_IDS {
                return Err(LegacyEncoderError::TooManyListIds);
            }
            format!(
                "[{}]",
                items
                    .iter()
                    .map(|n| b36_id(u128::from(*n)))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            )
        }
    };
    let tail = c
        .commit_clearlist_id
        .map(|id| b36_id(u128::from(id)))
        .transpose()?
        .map(|v| format!(",{v}"))
        .unwrap_or_default();
    let mut caveats = vec![SourceEncoderCaveat::GenericTypesEncoderMissingType4Export];
    if matches!(c.clear_lists,ClaimedClearLists::Array(items) if items.len()>1) {
        caveats.push(SourceEncoderCaveat::CommitClearlistCommasBreakDecoder);
    }
    if long {
        caveats.push(SourceEncoderCaveat::LongAddressSubstitutedByCallerClaimedRef);
    }
    source_output(
        4,
        format!(
            "tl4{},{},{channel},{},{},{}{}",
            b36_id(u128::from(c.property_id))?,
            b36(c.amount_atoms),
            u8::from(c.pay_enabled),
            lists,
            u8::from(c.colored_output),
            tail
        ),
        SourceInnerEnvelope::SingleTypePrefixed,
        caveats,
    )
}

pub fn source_encode_create_whitelist_unverified(
    backup: &str,
    name: &str,
    url: &str,
    description: &str,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    for s in [backup, name, url, description] {
        raw_text(s)?;
    }
    source_output(
        7,
        format!("tl7{backup},{name},{url},{description}"),
        SourceInnerEnvelope::SingleTypePrefixed,
        vec![],
    )
}
pub fn source_encode_update_admin_unverified(
    new_address: &str,
    whitelist: bool,
    oracle: bool,
    token: bool,
    id: u64,
    update_backup: bool,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    raw_text(new_address)?;
    source_output(
        8,
        format!(
            "tl8{new_address},{},{},{},{},{}",
            u8::from(whitelist),
            u8::from(oracle),
            u8::from(token),
            b36_id(u128::from(id))?,
            u8::from(update_backup)
        ),
        SourceInnerEnvelope::SingleTypePrefixed,
        vec![],
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRevokeArgument {
    Numeric(bool),
    Boolean(bool),
}
pub fn source_encode_attestation_unverified(
    revoke: SourceRevokeArgument,
    id: u64,
    target: &str,
    metadata: &str,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    raw_text(target)?;
    raw_text(metadata)?;
    let literal = match revoke {
        SourceRevokeArgument::Numeric(true) => "1",
        SourceRevokeArgument::Numeric(false) => "0",
        SourceRevokeArgument::Boolean(true) => "true",
        SourceRevokeArgument::Boolean(false) => "false",
    };
    source_output(
        9,
        format!(
            "tl9{literal},{},{target},{metadata}",
            b36_id(u128::from(id))?
        ),
        SourceInnerEnvelope::SingleTypePrefixed,
        if matches!(revoke, SourceRevokeArgument::Boolean(_)) {
            vec![SourceEncoderCaveat::RawBooleanRevokeDoesNotDecodeAsTrue]
        } else {
            vec![]
        },
    )
}
pub fn source_encode_close_oracle_unverified(
    _oracle_id: u64,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    // Operator precedence: (marker + typeStr?.toString(36)) ?? fallback.
    // The concatenation is never nullish; oracle ID is discarded.
    source_output(
        15,
        "tlf".into(),
        SourceInnerEnvelope::SingleTypePrefixed,
        vec![SourceEncoderCaveat::CloseOracleIgnoresIdAndReturnsPrefixOnly],
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ClaimedWithdrawal<'a> {
    pub withdraw_all: bool,
    pub property_id: u64,
    pub amount_atoms: u128,
    pub column: u8,
    pub channel_address: &'a str,
    pub ref_vout: u64,
}
pub fn source_encode_withdrawal_unverified(
    c: ClaimedWithdrawal<'_>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    amount_safe(c.amount_atoms)?;
    if c.column > 1 {
        return Err(LegacyEncoderError::MissingOrMalformedField);
    }
    let (channel, long) = optional_long_ref(c.channel_address, c.ref_vout)?;
    let mut caveats = vec![
        SourceEncoderCaveat::WithdrawalDecimalAmountVersusBase36Decoder,
        SourceEncoderCaveat::WithdrawalBooleanLiteralVersusOneFlag,
    ];
    if long {
        caveats.push(SourceEncoderCaveat::LongAddressSubstitutedByCallerClaimedRef);
    }
    source_output(
        21,
        format!(
            "tll{},{},{},{},{channel}",
            c.withdraw_all,
            b36_id(u128::from(c.property_id))?,
            c.amount_atoms,
            c.column
        ),
        SourceInnerEnvelope::SingleTypePrefixed,
        caveats,
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ClaimedTransfer<'a> {
    pub property_id: u64,
    pub amount_atoms: u128,
    pub is_column_a: bool,
    pub destination_address: &'a str,
    pub ref_vout: u64,
    pub pay: bool,
    pub pay_ref: &'a str,
}
pub fn source_encode_transfer_unverified(
    c: ClaimedTransfer<'_>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    amount_safe(c.amount_atoms)?;
    raw_text(c.pay_ref)?;
    let (destination, long) = optional_long_ref(c.destination_address, c.ref_vout)?;
    source_output(
        22,
        format!(
            "tlm{},{},{},{destination},{},{}",
            b36_id(u128::from(c.property_id))?,
            b36(c.amount_atoms),
            u8::from(c.is_column_a),
            u8::from(c.pay),
            c.pay_ref
        ),
        SourceInnerEnvelope::SingleTypePrefixed,
        if long {
            vec![SourceEncoderCaveat::LongAddressSubstitutedByCallerClaimedRef]
        } else {
            vec![]
        },
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ClaimedStakeFraudRelay<'a> {
    pub action: u64,
    pub oracle_id: u64,
    pub staked_property_id: u64,
    pub amount_atoms: u128,
    pub accused_address: &'a str,
    pub evidence_hash: &'a str,
    pub relay_type: u64,
    pub state_hash: &'a str,
    pub dlc_ref: &'a str,
    pub settlement_state: &'a str,
    pub relay_blob: &'a str,
    pub auto_roll: bool,
    pub next_dlc_ref: &'a str,
}
pub fn source_encode_stake_fraud_relay_unverified(
    c: ClaimedStakeFraudRelay<'_>,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    amount_safe(c.amount_atoms)?;
    for s in [
        c.accused_address,
        c.evidence_hash,
        c.state_hash,
        c.dlc_ref,
        c.settlement_state,
        c.relay_blob,
        c.next_dlc_ref,
    ] {
        raw_text(s)?;
    }
    let fields = vec![
        b36_id(u128::from(c.action))?,
        b36_id(u128::from(c.oracle_id))?,
        b36_id(u128::from(c.staked_property_id))?,
        b36(c.amount_atoms),
        c.accused_address.into(),
        c.evidence_hash.into(),
        b36_id(u128::from(c.relay_type))?,
        c.state_hash.into(),
        c.dlc_ref.into(),
        c.settlement_state.into(),
        c.relay_blob.into(),
        u8::from(c.auto_roll).to_string(),
        c.next_dlc_ref.into(),
    ];
    source_output(
        30,
        format!("tlu{}", fields.join(",")),
        SourceInnerEnvelope::SingleTypePrefixed,
        vec![],
    )
}

pub fn source_encode_colored_coin_unverified(
    action: u8,
    property_id: u64,
    sats_ratio: u64,
    home_address: &str,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    raw_text(home_address)?;
    source_output(
        33,
        format!(
            "{action},{},{},{}",
            b36_id(u128::from(property_id))?,
            b36_id(u128::from(sats_ratio))?,
            home_address
        ),
        SourceInnerEnvelope::BodyOnly,
        vec![SourceEncoderCaveat::BodyOnlyWithoutCalledValidator],
    )
}
pub fn source_encode_bridge_unverified(
    property_id: u64,
    amount: u64,
    destination_address: &str,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    raw_text(destination_address)?;
    source_output(
        34,
        format!(
            "{},{},{}",
            b36_id(u128::from(property_id))?,
            b36_id(u128::from(amount))?,
            destination_address
        ),
        SourceInnerEnvelope::BodyOnly,
        vec![SourceEncoderCaveat::BodyOnlyWithAbsentSourceDecoder],
    )
}
pub fn source_encode_smart_bind_unverified(
    contract_hash: &str,
    contract_type: &str,
    host_chain: &str,
) -> Result<UnverifiedSourceEncoding, LegacyEncoderError> {
    for s in [contract_hash, contract_type, host_chain] {
        raw_text(s)?;
    }
    source_output(
        35,
        format!("{contract_hash},{contract_type},{host_chain}"),
        SourceInnerEnvelope::BodyOnly,
        vec![SourceEncoderCaveat::BodyOnlyWithAbsentSourceDecoder],
    )
}

/// All own callable properties of the pinned source's exported `Encode` object.
pub const SOURCE_ENCODER_METHODS: &[&str] = &[
    "encodeAmount",
    "encodeActivateTradeLayer",
    "encodeTokenIssue",
    "encodeSend",
    "encodePropertyId",
    "encodeTradeTokenForUTXO",
    "encodeCommit",
    "encodeOnChainTokenForToken",
    "encodeCancelOrder",
    "encodeCreateWhitelist",
    "encodeUpdateAdmin",
    "encodeIssueOrRevokeAttestation",
    "encodeAMMPool",
    "encodeCreateOracle",
    "encodeGrantManagedToken",
    "encodeRedeemManagedToken",
    "encodePublishOracleData",
    "encodeCloseOracle",
    "encodeCreateFutureContractSeries",
    "encodeExerciseDerivative",
    "encodeTradeContractOnchain",
    "encodeTradeContractChannel",
    "encodeTradeTokensChannel",
    "encodeWithdrawal",
    "encodeTransfer",
    "encodeSettleChannelPNL",
    "encodeKingSettle",
    "encodeMintSynthetic",
    "encodeRedeemSynthetic",
    "encodePayToTokens",
    "encodeOptionTrade",
    "encodeTradeBaiUrbun",
    "encodeTradeMurabaha",
    "encodeStakeFraudProof",
    "encodeBatchSettlement",
    "encodeBatchMoveZkRollup",
    "encodeColoredCoin",
    "encodeAbstractionBridge",
    "encodeBindSmartContract",
];

/// Source helpers return strings; the empty batch-settlement stub returns
/// JavaScript `undefined`. Neither is converted into a fabricated transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceEncoderReturn {
    Encoding(UnverifiedSourceEncoding),
    Helper(String),
    Undefined,
}
impl SourceEncoderReturn {
    pub fn source_string(&self) -> Option<&str> {
        match self {
            Self::Encoding(value) => Some(value.source_return()),
            Self::Helper(value) => Some(value),
            Self::Undefined => None,
        }
    }
}

use crate::decimal::{Decimal, Rounding};
use crate::issuance_intent::JsonValue;
use core::str::FromStr;

type EncoderResult<T> = Result<T, LegacyEncoderError>;
const MAX_ENCODER_JSON_BYTES: usize = 32_768;
const MAX_ENCODER_NUMERIC_BYTES: usize = 512;

fn enc_input_domain(value: &JsonValue) -> EncoderResult<()> {
    match value {
        JsonValue::String(text) => {
            raw_text(text)?;
        }
        JsonValue::Array(values) => {
            if values.len() > MAX_SOURCE_LIST_IDS {
                return Err(LegacyEncoderError::TooManyListIds);
            }
            for value in values {
                enc_input_domain(value)?;
            }
        }
        JsonValue::Object(object) => {
            for (key, value) in object.iter() {
                raw_text(key)?;
                enc_input_domain(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn enc_prefixed_number(digits: &str, radix: u32) -> EncoderResult<f64> {
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return Ok(f64::NAN);
    }
    u64::from_str_radix(digits, radix)
        .map(|value| value as f64)
        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)
}

fn enc_field<'a>(value: &'a JsonValue, key: &str) -> Option<&'a JsonValue> {
    match value {
        JsonValue::Object(object) => object.get(key),
        _ => None,
    }
}
fn enc_truthy(value: Option<&JsonValue>) -> bool {
    match value {
        None | Some(JsonValue::Null) | Some(JsonValue::Bool(false)) => false,
        Some(JsonValue::Number(value)) => value.as_f64() != 0.0,
        Some(JsonValue::String(value)) => !value.is_empty(),
        _ => true,
    }
}
fn enc_nullish(value: Option<&JsonValue>) -> bool {
    matches!(value, None | Some(JsonValue::Null))
}
fn enc_flag(value: Option<&JsonValue>) -> String {
    if enc_truthy(value) { "1" } else { "0" }.to_owned()
}
fn enc_array(value: Option<&JsonValue>) -> EncoderResult<&[JsonValue]> {
    match value {
        Some(JsonValue::Array(values)) if values.len() <= MAX_SOURCE_LIST_IDS => Ok(values),
        Some(JsonValue::Array(_)) => Err(LegacyEncoderError::TooManyListIds),
        _ => Err(LegacyEncoderError::MissingOrMalformedField),
    }
}
/// String conversion is separate from Array.join: null/undefined produce empty
/// joined fields, but literal "null"/"undefined" in template interpolation.
fn enc_string(value: Option<&JsonValue>) -> EncoderResult<String> {
    Ok(match value {
        None => "undefined".to_owned(),
        Some(JsonValue::Null) => "null".to_owned(),
        Some(JsonValue::Bool(value)) => value.to_string(),
        Some(JsonValue::Number(value)) => {
            crate::state_oracle::source_number_text(value.as_f64())
                .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?
        }
        Some(JsonValue::String(value)) => value.clone(),
        Some(JsonValue::Object(object)) => {
            if object.get("toString").is_some() {
                return Err(LegacyEncoderError::UnsupportedNumericCoercion);
            }
            "[object Object]".to_owned()
        }
        Some(JsonValue::Array(values)) => {
            if values.len() > MAX_SOURCE_LIST_IDS {
                return Err(LegacyEncoderError::TooManyListIds);
            }
            values
                .iter()
                .map(|value| enc_join_field(Some(value)))
                .collect::<EncoderResult<Vec<_>>>()?
                .join(",")
        }
    })
}
fn enc_join_field(value: Option<&JsonValue>) -> EncoderResult<String> {
    if enc_nullish(value) {
        Ok(String::new())
    } else {
        enc_string(value)
    }
}
fn enc_number(value: Option<&JsonValue>) -> EncoderResult<f64> {
    Ok(match value {
        None => f64::NAN,
        Some(JsonValue::Null) => 0.0,
        Some(JsonValue::Bool(value)) => u8::from(*value) as f64,
        Some(JsonValue::Number(value)) => value.as_f64(),
        Some(JsonValue::Object(object)) => {
            if object.get("toString").is_some() {
                return Err(LegacyEncoderError::UnsupportedNumericCoercion);
            }
            f64::NAN
        }
        Some(JsonValue::String(_)) | Some(JsonValue::Array(_)) => {
            let text = enc_string(value)?;
            let text = text.trim();
            if text.is_empty() {
                0.0
            } else if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                enc_prefixed_number(hex, 16)?
            } else if let Some(bin) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
                enc_prefixed_number(bin, 2)?
            } else if let Some(oct) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
                enc_prefixed_number(oct, 8)?
            } else if matches!(text, "Infinity" | "+Infinity" | "-Infinity")
                || text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
            {
                text.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
    })
}
fn enc_number_radix36(number: f64) -> EncoderResult<String> {
    // V8's fractional radix36 digit selection is outside this adapter's domain.
    if !number.is_finite() || number.fract() != 0.0 || number.abs() > JS_SAFE_INTEGER as f64 {
        return Err(LegacyEncoderError::UnsupportedNumberRadix36);
    }
    Ok(format!(
        "{}{}",
        if number < 0.0 { "-" } else { "" },
        b36(number.abs() as u128)
    ))
}
fn enc_radix36(value: Option<&JsonValue>) -> EncoderResult<String> {
    match value {
        None | Some(JsonValue::Null) => Ok("0".to_owned()),
        Some(JsonValue::Number(value)) => enc_number_radix36(value.as_f64()),
        // String/Boolean/Array/Object.toString ignore the radix argument.
        _ => enc_string(value),
    }
}
fn enc_radix_list(value: Option<&JsonValue>, separator: &str) -> EncoderResult<String> {
    enc_array(value)?
        .iter()
        .map(|value| enc_radix36(Some(value)))
        .collect::<EncoderResult<Vec<_>>>()
        .map(|values| values.join(separator))
}
fn enc_decimal(value: Option<&JsonValue>) -> EncoderResult<Decimal> {
    let text = match value {
        Some(JsonValue::Number(_)) | Some(JsonValue::String(_)) => enc_string(value)?,
        _ => return Err(LegacyEncoderError::UnsupportedNumericCoercion),
    };
    if text.len() > MAX_ENCODER_NUMERIC_BYTES {
        return Err(LegacyEncoderError::UnsupportedNumericCoercion);
    }
    Decimal::from_str(text.trim()).map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)
}
fn enc_scaled(value: Option<&JsonValue>, scale: &str) -> EncoderResult<Decimal> {
    enc_decimal(value)?
        .checked_mul(&Decimal::from_str(scale).expect("constant decimal"))
        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)
}
fn enc_bigint_radix36(text: &str) -> String {
    let mut digits: Vec<u32> = text.bytes().map(|b| u32::from(b - b'0')).collect();
    let mut output = Vec::new();
    while digits.iter().any(|&digit| digit != 0) {
        let mut carry = 0;
        for digit in &mut digits {
            let next = carry * 10 + *digit;
            *digit = next / 36;
            carry = next % 36;
        }
        output.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[carry as usize] as char);
    }
    if output.is_empty() {
        "0".to_owned()
    } else {
        output.into_iter().rev().collect()
    }
}
/// BigNumber.toString(36): 20 radix-36 fractional places, ROUND_HALF_UP.
fn enc_bn_radix36(value: &Decimal) -> EncoderResult<String> {
    let absolute = value.abs();
    let integer = absolute
        .quantize(0, Rounding::Down)
        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
    let mut fraction = absolute
        .checked_sub(&integer)
        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
    let mut fractional = Vec::new();
    let radix = Decimal::from_str("36").expect("constant");
    for _ in 0..20 {
        if fraction.is_zero() {
            break;
        }
        let scaled = fraction
            .checked_mul(&radix)
            .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
        let digit = scaled
            .quantize(0, Rounding::Down)
            .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
        fractional.push(digit.to_string().parse::<u8>().expect("radix digit"));
        fraction = scaled
            .checked_sub(&digit)
            .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
    }
    let mut integer = integer;
    if fraction >= Decimal::from_str("0.5").expect("constant") {
        let mut carry = true;
        for digit in fractional.iter_mut().rev() {
            if *digit == 35 {
                *digit = 0;
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            integer = integer
                .checked_add(&Decimal::from_str("1").expect("constant"))
                .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
        }
    }
    while fractional.last() == Some(&0) {
        fractional.pop();
    }
    let zero = integer.is_zero() && fractional.is_empty();
    let mut output = if value.is_negative() && !zero {
        "-".to_owned()
    } else {
        String::new()
    };
    output.push_str(&enc_bigint_radix36(&integer.to_string()));
    if !fractional.is_empty() {
        output.push('.');
        output.extend(
            fractional
                .into_iter()
                .map(|digit| b"0123456789abcdefghijklmnopqrstuvwxyz"[digit as usize] as char),
        );
    }
    Ok(output)
}
fn enc_scaled_number(value: Option<&JsonValue>, scale: &str) -> EncoderResult<String> {
    let number = enc_scaled(value, scale)?
        .to_string()
        .parse::<f64>()
        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
    enc_number_radix36(number)
}
fn enc_scaled_integer(value: Option<&JsonValue>, rounding: Rounding) -> EncoderResult<String> {
    enc_bn_radix36(
        &enc_scaled(value, "100000000")?
            .quantize(0, rounding)
            .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?,
    )
}
fn enc_amount(value: Option<&JsonValue>) -> EncoderResult<String> {
    if enc_number(value)? % 1.0 == 0.0 {
        enc_radix36(value)
    } else {
        enc_scaled_integer(value, Rounding::HalfUp)
    }
}
fn enc_fallback<'a>(value: Option<&'a JsonValue>, fallback: &'a JsonValue) -> &'a JsonValue {
    if enc_truthy(value) {
        value.expect("truthy value exists")
    } else {
        fallback
    }
}
fn enc_ref(value: Option<&JsonValue>, reference: Option<&JsonValue>) -> EncoderResult<String> {
    match value {
        Some(JsonValue::String(text)) => {
            if text.encode_utf16().count() > 42 {
                Ok(format!(
                    "ref:{}",
                    if enc_truthy(reference) {
                        enc_string(reference)?
                    } else {
                        "0".to_owned()
                    }
                ))
            } else {
                Ok(text.clone())
            }
        }
        _ => Err(LegacyEncoderError::MissingOrMalformedField),
    }
}
fn enc_base94(value: Option<&JsonValue>) -> EncoderResult<String> {
    let text = if enc_truthy(value) {
        enc_string(value)?
    } else {
        "0".to_owned()
    };
    crate::legacy_base::base94_encode_decimal(Some(&text))
        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?
        .ok_or(LegacyEncoderError::UnsupportedNumericCoercion)
}
fn enc_property_id(value: Option<&JsonValue>) -> EncoderResult<String> {
    if let Some(JsonValue::String(text)) = value {
        if text.starts_with("s-") {
            let mut parts = text.split('-').skip(1);
            let parse = |part: Option<&str>| -> EncoderResult<String> {
                let Some(part) = part else {
                    return Ok("NaN".to_owned());
                };
                let text = part.trim_start();
                let (negative, text) = if let Some(rest) = text.strip_prefix('-') {
                    (true, rest)
                } else {
                    (false, text.strip_prefix('+').unwrap_or(text))
                };
                let (radix, text) = if let Some(hex) =
                    text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
                {
                    (16, hex)
                } else {
                    (10, text)
                };
                let digits: String = text.chars().take_while(|c| c.is_digit(radix)).collect();
                if digits.is_empty() {
                    return Ok("NaN".to_owned());
                }
                let number = u64::from_str_radix(&digits, radix)
                    .map_err(|_| LegacyEncoderError::UnsupportedNumberRadix36)?
                    as f64;
                enc_number_radix36(if negative { -number } else { number })
            };
            return Ok(format!(
                "s-{}-{}",
                parse(parts.next())?,
                parse(parts.next())?
            ));
        }
    }
    enc_radix36(value)
}

/// Executes the complete pinned encoder API within an explicit local domain.
/// JSON distinguishes missing fields, null, Boolean, Number and numeric String;
/// it cannot express functions, symbols, BigInts or custom prototypes. Number
/// radix36 is restricted to signed safe integers. BigNumber decimal arithmetic
/// and rounding do not use binary64 until an actual source `.toNumber()` call.
/// Output remains the existing unverified source-string type.
pub fn source_encode_method_unverified(
    method: &str,
    input: &JsonValue,
) -> EncoderResult<SourceEncoderReturn> {
    if !SOURCE_ENCODER_METHODS.contains(&method) {
        return Err(LegacyEncoderError::UnknownSourceMethod);
    }
    if input
        .to_json()
        .map_err(|_| LegacyEncoderError::JsonInputLimit)?
        .len()
        > MAX_ENCODER_JSON_BYTES
    {
        return Err(LegacyEncoderError::JsonInputLimit);
    }
    enc_input_domain(input)?;
    if method == "encodeBatchSettlement" {
        return Ok(SourceEncoderReturn::Undefined);
    }
    if method == "encodeAmount" || method == "encodePropertyId" {
        let text = if method == "encodeAmount" {
            enc_amount(Some(input))?
        } else {
            enc_property_id(Some(input))?
        };
        raw_text(&text)?;
        return Ok(SourceEncoderReturn::Helper(text));
    }
    if method != "encodeCloseOracle" && !matches!(input, JsonValue::Object(_)) {
        return Err(LegacyEncoderError::MissingOrMalformedField);
    }
    let field = |name: &str| enc_field(input, name);
    let raw = |name: &str| enc_join_field(field(name));
    let radix = |name: &str| enc_radix36(field(name));
    let flag = |name: &str| enc_flag(field(name));
    let scale = |name: &str| enc_scaled_number(field(name), "100000000");
    let fields = |names: &[&str]| {
        names
            .iter()
            .map(|name| radix(name))
            .collect::<EncoderResult<Vec<_>>>()
    };
    let zero = JsonValue::parse("0").expect("constant JSON");
    let empty = JsonValue::String(String::new());
    let or_empty = |name: &str| enc_join_field(Some(enc_fallback(field(name), &empty)));
    let mut prefix = SourceInnerEnvelope::SingleTypePrefixed;
    let mut caveats = Vec::new();
    let (ty, payload): (u8, Vec<String>) = match method {
        "encodeActivateTradeLayer" => {
            let types = match field("txTypeToActivate") {
                Some(JsonValue::Array(values)) => {
                    enc_array(field("txTypeToActivate"))?;
                    values
                        .iter()
                        .map(|value| enc_join_field(Some(value)))
                        .collect::<EncoderResult<Vec<_>>>()?
                        .join(";")
                }
                value => enc_string(value)?,
            };
            let hash = enc_string(field("codeHash"))?;
            if hash.is_empty() || hash.len() > 512 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(LegacyEncoderError::InvalidActivationHex);
            }
            let mut digits = vec![0u32];
            for byte in hash.bytes() {
                let mut carry = (byte as char).to_digit(16).expect("validated");
                for digit in &mut digits {
                    let next = *digit * 16 + carry;
                    *digit = next % 10;
                    carry = next / 10;
                }
                while carry != 0 {
                    digits.push(carry % 10);
                    carry /= 10;
                }
            }
            let decimal: String = digits
                .into_iter()
                .rev()
                .map(|d| (b'0' + d as u8) as char)
                .collect();
            (0, vec![types, enc_bigint_radix36(&decimal)])
        }
        "encodeTokenIssue" => (
            1,
            vec![
                radix("initialAmount")?,
                raw("ticker")?,
                enc_radix_list(field("whitelists"), ",")?,
                flag("managed"),
                raw("backupAddress")?,
                flag("nft"),
                flag("coloredCoinHybrid"),
                if enc_nullish(field("proceduralType")) {
                    String::new()
                } else {
                    radix("proceduralType")?
                },
            ],
        ),
        "encodeSend" => {
            if enc_truthy(field("sendAll")) {
                prefix = SourceInnerEnvelope::BodyOnly;
                (2, vec![format!("1;{}", enc_string(field("address"))?)])
            } else if matches!(field("propertyId"), Some(JsonValue::Array(_)))
                && matches!(field("amount"), Some(JsonValue::Array(_)))
            {
                prefix = SourceInnerEnvelope::BodyOnly;
                let amounts = enc_array(field("amount"))?
                    .iter()
                    .map(|v| enc_amount(Some(v)))
                    .collect::<EncoderResult<Vec<_>>>()?
                    .join(",");
                (
                    2,
                    vec![format!(
                        "0;;{};{amounts}",
                        enc_radix_list(field("propertyId"), ",")?
                    )],
                )
            } else {
                let amount = match field("amount") {
                    Some(JsonValue::Array(v)) => {
                        enc_array(field("amount"))?;
                        v.first()
                    }
                    value => value,
                };
                (
                    2,
                    vec![format!(
                        "0;{};{};{}",
                        raw("address")?,
                        radix("propertyId")?,
                        enc_amount(amount)?
                    )],
                )
            }
        }
        "encodeTradeTokenForUTXO" => (
            3,
            vec![
                radix("propertyId")?,
                scale("amount")?,
                raw("columnA")?,
                scale("satsExpected")?,
                raw("tokenOutput")?,
                raw("payToAddress")?,
                flag("isColoredOutput"),
            ],
        ),
        "encodeCommit" => {
            let lists = if !enc_truthy(field("clearLists")) {
                String::new()
            } else if matches!(field("clearLists"), Some(JsonValue::Array(_))) {
                format!("[{}]", enc_radix_list(field("clearLists"), ",")?)
            } else {
                radix("clearLists")?
            };
            let mut values = vec![
                radix("propertyId")?,
                enc_bn_radix36(&enc_scaled(field("amount"), "100000000")?)?,
                enc_ref(field("channelAddress"), field("ref"))?,
                flag("payEnabled"),
                lists,
                flag("isColoredOutput"),
            ];
            if let Some(JsonValue::Number(value)) = field("commitClearlistId") {
                if value.as_f64().fract() == 0.0 {
                    values.push(radix("commitClearlistId")?);
                }
            }
            caveats.push(SourceEncoderCaveat::GenericTypesEncoderMissingType4Export);
            (4, values)
        }
        "encodeOnChainTokenForToken" => (
            5,
            vec![
                radix("propertyIdOffered")?,
                radix("propertyIdDesired")?,
                scale("amountOffered")?,
                scale("amountExpected")?,
                flag("stop"),
                flag("post"),
            ],
        ),
        "encodeCancelOrder" => {
            let contract = enc_truthy(field("isContract"));
            let mut values = vec![enc_string(field("isContract"))?];
            if contract {
                values.push(radix("contractId")?);
            } else {
                values.extend(fields(&["offeredPropertyId", "desiredPropertyId"])?);
            }
            values.push(flag("cancelAll"));
            if let Some(params) = field("cancelParams") {
                let price = enc_field(params, "price");
                if price.is_some() {
                    let kind = if enc_nullish(field("isContract")) {
                        f64::NAN
                    } else {
                        enc_number(field("isContract"))?
                    };
                    values.push(if kind == 0.0 {
                        enc_bn_radix36(&enc_scaled(price, "8")?)?
                    } else if kind == 1.0 {
                        enc_radix36(price)?
                    } else {
                        "undefined".to_owned()
                    });
                    values.push(enc_radix36(enc_field(params, "side"))?);
                }
                if enc_truthy(enc_field(params, "txid")) {
                    values.push(enc_string(enc_field(params, "txid"))?);
                }
            }
            (6, values)
        }
        "encodeCreateWhitelist" => (
            7,
            ["backupAddress", "name", "url", "description"]
                .iter()
                .map(|name| raw(name))
                .collect::<EncoderResult<_>>()?,
        ),
        "encodeUpdateAdmin" => (
            8,
            vec![
                raw("newAddress")?,
                flag("whitelist"),
                flag("oracle"),
                flag("token"),
                radix("id")?,
                flag("updateBackup"),
            ],
        ),
        "encodeIssueOrRevokeAttestation" => (
            9,
            vec![
                raw("revoke")?,
                radix("id")?,
                raw("targetAddress")?,
                raw("metaData")?,
            ],
        ),
        "encodeAMMPool" => {
            let strict_flag = |name: &str| {
                match field(name) {
                    Some(JsonValue::Bool(true)) => "1",
                    Some(JsonValue::Number(v)) if v.as_f64() == 1.0 => "1",
                    Some(JsonValue::String(v)) if v == "1" => "1",
                    _ => "0",
                }
                .to_owned()
            };
            let convert = |name: &str| -> EncoderResult<String> {
                if let Some(JsonValue::String(text)) = field(name) {
                    let digits = text.strip_prefix('-').unwrap_or(text);
                    if !digits.is_empty()
                        && digits.bytes().all(|b| b.is_ascii_alphanumeric())
                        && digits.bytes().any(|b| b.is_ascii_alphabetic())
                    {
                        return Ok(text.to_ascii_lowercase());
                    }
                }
                let number = enc_number(field(name))?;
                if number.is_finite() {
                    enc_number_radix36(number.trunc())
                } else {
                    Ok("0".to_owned())
                }
            };
            (
                10,
                vec![
                    strict_flag("isRedeem"),
                    strict_flag("isContract"),
                    convert("id")?,
                    convert("amount")?,
                    convert("id2")?,
                    convert("amount2")?,
                ],
            )
        }
        "encodeGrantManagedToken" | "encodeRedeemManagedToken" => {
            let property = field("propertyid")
                .filter(|value| !matches!(value, JsonValue::Null))
                .or_else(|| field("propertyId"));
            let mut values = vec![enc_radix36(property)?];
            if method == "encodeGrantManagedToken" {
                values.extend([
                    scale("amountGranted")?,
                    raw("addressToGrantTo")?,
                    String::new(),
                ]);
            } else {
                let amount = field("amountDestroyed")
                    .filter(|v| !matches!(v, JsonValue::Null))
                    .or_else(|| field("amountGranted").filter(|v| !matches!(v, JsonValue::Null)))
                    .unwrap_or(&zero);
                values.push(enc_scaled_number(Some(amount), "100000000")?);
            }
            for name in ["dlcTemplateId", "dlcContractId", "settlementState"] {
                values.push(or_empty(name)?);
            }
            if method == "encodeGrantManagedToken" {
                values.push(or_empty("dlcHash")?);
            }
            (
                if method == "encodeGrantManagedToken" {
                    11
                } else {
                    12
                },
                values,
            )
        }
        "encodeCreateOracle" => (
            13,
            vec![
                raw("ticker")?,
                raw("url")?,
                raw("backupAddress")?,
                enc_radix_list(field("whitelists"), ",")?,
                radix("lag")?,
                raw("enum")?,
            ],
        ),
        "encodePublishOracleData" => {
            let mut values = vec![
                radix("oracleid")?,
                enc_scaled_number(field("price"), "10000")?,
            ];
            for name in ["high", "low", "close"] {
                if field(name).is_some() {
                    values.push(radix(name)?);
                }
            }
            (14, values)
        }
        "encodeCloseOracle" => {
            caveats.push(SourceEncoderCaveat::CloseOracleIgnoresIdAndReturnsPrefixOnly);
            (15, Vec::new())
        }
        "encodeCreateFutureContractSeries" => {
            let chain = if let Some(JsonValue::Array(values)) = field("onChainData") {
                enc_array(field("onChainData"))?;
                values
                    .iter()
                    .map(|value| {
                        let first = enc_array(Some(value))?.first();
                        let pair = enc_array(first)?;
                        Ok(format!(
                            "{}:{}",
                            enc_radix36(pair.first())?,
                            enc_radix36(pair.get(1))?
                        ))
                    })
                    .collect::<EncoderResult<Vec<_>>>()?
                    .join(";")
            } else if let Some(JsonValue::String(value)) = field("onChainData") {
                if !value.is_empty() {
                    return Err(LegacyEncoderError::MissingOrMalformedField);
                }
                String::new()
            } else if let Some(JsonValue::Object(object)) = field("onChainData") {
                if enc_number(object.get("length"))? > 0.0 {
                    return Err(LegacyEncoderError::MissingOrMalformedField);
                }
                String::new()
            } else {
                String::new()
            };
            (
                16,
                vec![
                    flag("native"),
                    radix("underlyingOracleId")?,
                    chain,
                    radix("notionalPropertyId")?,
                    radix("notionalValue")?,
                    radix("collateralPropertyId")?,
                    raw("leverage")?,
                    radix("expiryPeriod")?,
                    radix("series")?,
                    flag("inverse"),
                    flag("fee"),
                ],
            )
        }
        "encodeExerciseDerivative" => (17, fields(&["derivativeContractId", "amount"])?),
        "encodeTradeContractOnchain" => {
            let mut values = vec![
                radix("contractId")?,
                enc_scaled_integer(field("price"), Rounding::HalfUp)?,
                radix("amount")?,
            ];
            values.extend(
                ["sell", "insurance", "reduce", "post", "stop"]
                    .iter()
                    .map(|name| flag(name)),
            );
            (18, values)
        }
        "encodeTradeContractChannel" => (
            19,
            vec![
                radix("contractId")?,
                enc_scaled_integer(field("price"), Rounding::HalfUp)?,
                radix("amount")?,
                flag("columnAIsSeller"),
                radix("expiryBlock")?,
                flag("insurance"),
                flag("columnAIsMaker"),
            ],
        ),
        "encodeTradeTokensChannel" => (
            20,
            vec![
                radix("propertyId1")?,
                radix("propertyId2")?,
                scale("amountOffered1")?,
                scale("amountDesired2")?,
                flag("columnAIsOfferer"),
                radix("expiryBlock")?,
                flag("columnAIsMaker"),
                flag("Id1ColoredOutput"),
                flag("Id2ColoredOutput"),
            ],
        ),
        "encodeWithdrawal" => {
            let number: f64 = enc_scaled(field("amountOffered"), "100000000")?
                .to_string()
                .parse()
                .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
            let amount = crate::state_oracle::source_number_text(number)
                .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?;
            (
                21,
                vec![
                    raw("withdrawAll")?,
                    radix("propertyId")?,
                    amount,
                    raw("column")?,
                    enc_ref(field("channelAddress"), field("ref"))?,
                ],
            )
        }
        "encodeTransfer" => (
            22,
            vec![
                radix("propertyId")?,
                enc_bn_radix36(&enc_scaled(field("amount"), "100000000")?)?,
                flag("isColumnA"),
                enc_ref(field("destinationAddr"), field("ref"))?,
                flag("pay"),
                or_empty("payRef")?,
            ],
        ),
        "encodeSettleChannelPNL" => {
            let first = field("txidNeutralized1")
                .filter(|v| enc_truthy(Some(v)))
                .or_else(|| field("tradeid").filter(|v| enc_truthy(Some(v))));
            let second = field("txidNeutralized2")
                .filter(|v| enc_truthy(Some(v)))
                .or_else(|| field("settleid").filter(|v| enc_truthy(Some(v))));
            let kind = if field("settleType").is_some() {
                let number = enc_number(field("settleType"))?;
                if number.is_nan() {
                    "NaN".to_owned()
                } else if !number.is_finite() {
                    if number.is_sign_negative() {
                        "-Infinity"
                    } else {
                        "Infinity"
                    }
                    .to_owned()
                } else {
                    crate::state_oracle::source_number_text(number)
                        .map_err(|_| LegacyEncoderError::UnsupportedNumericCoercion)?
                }
            } else {
                flag("close")
            };
            let mut values = vec![
                enc_join_field(first)?,
                enc_join_field(second)?,
                enc_base94(field("markPrice"))?,
                kind,
                flag("columnAIsSeller"),
                flag("columnAIsMaker"),
                enc_base94(field("netAmount"))?,
                enc_base94(field("expiryBlock"))?,
            ];
            if ["blockStart", "blockEnd", "propertyId"]
                .iter()
                .any(|name| field(name).is_some())
            {
                for name in ["blockStart", "blockEnd", "propertyId"] {
                    values.push(enc_base94(field(name))?);
                }
                values.push(flag("aPaysBDirection"));
                values.push(or_empty("channelRoot")?);
                for name in ["totalContracts", "neutralizedCount"] {
                    values.push(enc_base94(field(name))?);
                }
            }
            (23, values)
        }
        "encodeKingSettle" => {
            let mut values = Vec::new();
            for name in ["blockStart", "blockEnd", "propertyId", "netAmount"] {
                values.push(enc_base94(field(name))?);
            }
            values.push(flag("aPaysBDirection"));
            values.push(or_empty("channelRoot")?);
            for name in ["totalContracts", "neutralizedCount"] {
                values.push(enc_base94(field(name))?);
            }
            (31, values)
        }
        "encodeMintSynthetic" | "encodeRedeemSynthetic" => (
            if method == "encodeMintSynthetic" {
                24
            } else {
                25
            },
            fields(&["propertyIdUsed", "contractIdUsed", "amount"])?,
        ),
        "encodePayToTokens" => (
            26,
            fields(&["propertyIdTarget", "propertyIdUsed", "amount"])?,
        ),
        "encodeOptionTrade" => {
            let mut values = vec![
                raw("contractId")?,
                enc_amount(field("price"))?,
                radix("amount")?,
                flag("columnAIsSeller"),
                radix("expiryBlock")?,
                flag("columnAIsMaker"),
            ];
            if enc_truthy(field("comboTicker")) && enc_truthy(field("comboAmount")) {
                values.extend([
                    raw("comboTicker")?,
                    enc_amount(field("comboPrice"))?,
                    radix("comboAmount")?,
                ]);
            }
            (27, values)
        }
        "encodeTradeBaiUrbun" => (
            28,
            fields(&[
                "propertyIdDownPayment",
                "propertyIdToBeSold",
                "price",
                "amount",
                "expiryBlock",
                "tradeExpiryBlock",
            ])?,
        ),
        "encodeTradeMurabaha" => (
            29,
            fields(&[
                "propertyIdDownPayment",
                "downPaymentPercent",
                "propertyIdToBeSold",
                "price",
                "amount",
                "expiryBlock",
                "installmentInterval",
                "tradeExpiryBlock",
            ])?,
        ),
        "encodeStakeFraudProof" => {
            let num = |name: &str| {
                enc_number_radix36(enc_number(Some(enc_fallback(field(name), &zero)))?)
            };
            (
                30,
                vec![
                    num("action")?,
                    num("oracleId")?,
                    num("stakedPropertyId")?,
                    enc_scaled_integer(Some(enc_fallback(field("amount"), &zero)), Rounding::Down)?,
                    or_empty("accusedAddress")?,
                    or_empty("evidenceHash")?,
                    num("relayType")?,
                    or_empty("stateHash")?,
                    or_empty("dlcRef")?,
                    or_empty("settlementState")?,
                    or_empty("relayBlob")?,
                    flag("autoRoll"),
                    or_empty("nextDlcRef")?,
                ],
            )
        }
        "encodeBatchMoveZkRollup" => {
            let payments = enc_array(field("payments"))?
                .iter()
                .map(|payment| {
                    Ok([
                        enc_join_field(enc_field(payment, "fromAddress"))?,
                        enc_radix_list(enc_field(payment, "propertyIds"), ":")?,
                        enc_radix_list(enc_field(payment, "amounts"), ":")?,
                        enc_join_field(enc_field(payment, "toAddress"))?,
                        enc_radix_list(enc_field(payment, "sentPropertyIds"), ":")?,
                        enc_radix_list(enc_field(payment, "sentAmounts"), ":")?,
                    ]
                    .join(","))
                })
                .collect::<EncoderResult<Vec<_>>>()?
                .join(";");
            let json = |name: &str| match field(name) {
                None => Ok(String::new()),
                Some(value) => value
                    .to_json()
                    .map_err(|_| LegacyEncoderError::JsonInputLimit),
            };
            prefix = SourceInnerEnvelope::BodyOnly;
            (
                32,
                vec![[
                    raw("proof")?,
                    payments,
                    json("miscLogic")?,
                    json("miscData")?,
                ]
                .join("|")],
            )
        }
        "encodeColoredCoin" => {
            prefix = SourceInnerEnvelope::BodyOnly;
            (
                33,
                vec![
                    raw("encodeDecodeRecode")?,
                    radix("propertyId")?,
                    radix("SatsRatio")?,
                    raw("homeAddress")?,
                ],
            )
        }
        "encodeAbstractionBridge" => {
            prefix = SourceInnerEnvelope::BodyOnly;
            (
                34,
                vec![
                    radix("propertyId")?,
                    radix("amount")?,
                    raw("destinationAddr")?,
                ],
            )
        }
        "encodeBindSmartContract" => {
            prefix = SourceInnerEnvelope::BodyOnly;
            (
                35,
                vec![raw("contractHash")?, raw("type")?, raw("hostChain")?],
            )
        }
        _ => return Err(LegacyEncoderError::UnknownSourceMethod),
    };
    let body = payload.join(",");
    let text = if prefix == SourceInnerEnvelope::BodyOnly {
        body
    } else {
        format!("tl{}{body}", b36(u128::from(ty)))
    };
    Ok(SourceEncoderReturn::Encoding(source_output(
        ty, text, prefix, caveats,
    )?))
}
