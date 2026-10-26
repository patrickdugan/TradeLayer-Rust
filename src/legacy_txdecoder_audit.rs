//! File 6/49: exact-source bounded txDecoder.js body layout observations.
//!
//! Pinned JS: patrickdugan/tradelayer.js@dca0888b, src/txDecoder.js.
//! This module only decodes caller-supplied BODY text: it never parses an
//! actual Bitcoin/Litecoin/Dogecoin transaction or verifies script, witness,
//! first-input sender, activation, list administrator, UTXO or signatures.
//!
//! Source-specific hazards included in the read-only research surface:
//! - Type4 comma split occurs BEFORE bracketed clearlist parsing. [7,b]
//!   becomes separate fields and produces NaN/field drift, not two lists.
//! - Type9 calls parts[3].toUpperCase() unconditionally, throwing if absent.
//! - Type4/22 call startsWith on a required indexed field, throwing if absent.
//! - Type4/22 references stay raw TEXT after ref:, Type21 ref is parseInt
//!   BASE TEN and becomes false if NaN, not base36 list-id parsing.
//! - Type23 decodeSettleChannelPNL is defined TWICE: the later base94
//!   method shadows the earlier base256 method. Neither is authorized here.
//! - decodeCloseOracle returns a primitive integer, not a decoded object.
//! - Standalone decodeAbstractionBridge exists but Types dispatch Type34
//!   invokes ABSENT decodeCrossLayerBridge. Never silently alias those.
//! - Types32/33 return decoded params WITHOUT a valid flag or validator.
//!
//! The preserved strict projection requires canonical full-field integers,
//! printable ASCII and safe integer atoms. The complete historical observation
//! API instead retains source prefix parsing, Unicode, NaN, infinities,
//! undefined and signed zero. Its explicit input/output resource limits and
//! characterized host numeric policy are NOT historical acceptance rules.
//!
//! Outputs are unverified observations, with no trusted adapter and no
//! method to turn them into ledger or channel authorization. Every permission
//! request permanently returns a denial.

use crate::wire::{parse_base36, WireError};

pub const MAX_DECODER_BODY_BYTES: usize = 4096;
pub const MAX_SOURCE_SAFE_INTEGER: u128 = 9_007_199_254_740_991;
pub const MAX_SOURCE_CLEARLISTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyDecoderError {
    UnsupportedOrUnportedLayout(u8),
    MissingSourceDecoderExport(u8),
    BodyTooLong,
    NonPrintableSourceText,
    NestedOrFramedBody,
    MissingIndexedFieldWouldThrow(&'static str),
    MissingAttestationMetadataWouldThrow,
    NonCanonicalLegacyNumeric(&'static str),
    AmountNotExactlyRepresentable,
    MalformedSyntheticProperty,
    BracketedListSplitBeforeParsing,
    TooManyClearlistIds,
    WrongSourceBodyType,
    HistoricalTransactionNotAuthenticated,
    RegistryAndActivationUnverified,
    CanonicalSignedChannelRevisionAbsent,
    TrustedEconomicExecutionForbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePropertyId {
    Regular(u64),
    /// Source decodePropertyId accepts both s-<id>-<id> and s<id>-<id>.
    /// It converts the two base36 fields to decimal JS numeric IDs.
    Synthetic {
        collateral_id: u64,
        contract_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChannelLocation {
    /// The source stores the channel address directly, including empty.
    Address(String),
    /// Type4/22 keep text after ref: without numeric normalization.
    RawReference(String),
    /// Type21 uses decimal parseInt, with NaN mapped to false.
    WithdrawalReference(Option<u64>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnverifiedSourceBody {
    Commit {
        property: SourcePropertyId,
        amount_atoms: u128,
        channel: SourceChannelLocation,
        pay_enabled: bool,
        clear_lists: Vec<u64>,
        colored_output: bool,
        commit_clearlist_id: Option<u64>,
    },
    CreateWhitelist {
        backup_address: String,
        name: String,
        url: String,
        description: String,
    },
    UpdateAdmin {
        new_address: String,
        whitelist: bool,
        oracle: bool,
        token: bool,
        list_id: u64,
        update_backup: bool,
    },
    Attestation {
        revoke: bool,
        list_id: u64,
        target_address: String,
        metadata_upper: String,
    },
    /// Legacy close oracle decoder returns a primitive number.
    CloseOraclePrimitive(u64),
    Withdrawal {
        withdraw_all: bool,
        property: SourcePropertyId,
        amount_atoms: u128,
        column_flag: bool,
        channel: SourceChannelLocation,
    },
    Transfer {
        property: SourcePropertyId,
        amount_atoms: u128,
        column_a: bool,
        destination: SourceChannelLocation,
        pay: bool,
        pay_reference: String,
    },
    StakeFraudRelay {
        action: u64,
        oracle_id: u64,
        staked_property_id: u64,
        amount_atoms: u128,
        accused_address: String,
        evidence_hash: String,
        relay_type: u64,
        state_hash: String,
        dlc_ref: String,
        settlement_state: String,
        relay_blob: String,
        auto_roll: bool,
        next_dlc_ref: String,
    },
    /// Source Type32 decoder returns only ordinalRevealJSON, no valid flag.
    ZkBatchOpaque { ordinal_reveal_json: String },
    /// Type33 has a decoder, but Types has its validator commented out.
    ColoredCoin {
        action_decimal: u64,
        property_id: u64,
        sats_ratio: u64,
        home_address: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDecoderCaveat {
    AmountFloatLimitedToExactSafeAtoms,
    CommitBracketedClearlistCommasMisparse,
    AttestationMissingMetadataThrows,
    TwoDifferentReferenceNumberRules,
    DuplicateSettlementDecoderShadowsFirst,
    CloseOraclePrimitiveReplacesBlockField,
    Types32And33ValidatorCommentedOut,
    StandaloneBridgeHelperNotDispatchedByType34,
    SourceJavaScriptAllowsPartialParseInt,
    NoChainOrHistoricalStateEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedDecodedBody {
    type_id: u8,
    fields: UnverifiedSourceBody,
    caveats: Vec<SourceDecoderCaveat>,
}
impl UnverifiedDecodedBody {
    pub const fn type_id(&self) -> u8 {
        self.type_id
    }
    pub fn source_fields(&self) -> &UnverifiedSourceBody {
        &self.fields
    }
    pub fn caveats(&self) -> &[SourceDecoderCaveat] {
        &self.caveats
    }
    pub const fn require_historically_accepted_transaction(
        &self,
    ) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::HistoricalTransactionNotAuthenticated)
    }
    pub const fn require_accepted_registry_transition(&self) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::RegistryAndActivationUnverified)
    }
    pub const fn require_canonical_signed_channel_revision(
        &self,
    ) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::CanonicalSignedChannelRevisionAbsent)
    }
    pub const fn require_trusted_execution(&self) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
    }
}

fn body_check(body: &str) -> Result<(), LegacyDecoderError> {
    if body.len() > MAX_DECODER_BODY_BYTES {
        return Err(LegacyDecoderError::BodyTooLong);
    }
    if !body.is_ascii() || !body.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(LegacyDecoderError::NonPrintableSourceText);
    }
    // Caller must pass the exact POST-TYPE body. Do not double-decode
    // generic Types outer prefixes or silently consume arbitrary selectors.
    if body.starts_with("tl") {
        return Err(LegacyDecoderError::NestedOrFramedBody);
    }
    Ok(())
}
fn source_field<'a>(parts: &'a [&str], index: usize) -> &'a str {
    parts.get(index).copied().unwrap_or("")
}
fn numeric(raw: &str, default_zero: bool, field: &'static str) -> Result<u64, LegacyDecoderError> {
    let value = if raw.is_empty() && default_zero {
        "0"
    } else {
        raw
    };
    if value.is_empty() {
        return Err(LegacyDecoderError::NonCanonicalLegacyNumeric(field));
    }
    let n = parse_base36(value)
        .map_err(|_err: WireError| LegacyDecoderError::NonCanonicalLegacyNumeric(field))?;
    if n > MAX_SOURCE_SAFE_INTEGER {
        return Err(LegacyDecoderError::NonCanonicalLegacyNumeric(field));
    }
    u64::try_from(n).map_err(|_| LegacyDecoderError::NonCanonicalLegacyNumeric(field))
}
fn amount_atoms(raw: &str) -> Result<u128, LegacyDecoderError> {
    // Source BigNumber(base36).div(1e8).toNumber; restrict to safe integer
    // atoms to avoid claiming equivalence beyond JS Number precision.
    let atoms = u128::from(numeric(raw, true, "amount")?);
    if atoms > MAX_SOURCE_SAFE_INTEGER {
        return Err(LegacyDecoderError::AmountNotExactlyRepresentable);
    }
    Ok(atoms)
}

pub fn source_decode_property_id_unverified(
    raw: &str,
) -> Result<SourcePropertyId, LegacyDecoderError> {
    if let Some(trim) = raw.strip_prefix('s') {
        let segments: Vec<_> = trim.split('-').collect();
        let (collateral, contract) = match segments.as_slice() {
            [a, b] if !a.is_empty() && !b.is_empty() => (*a, *b),
            ["", a, b] if !a.is_empty() && !b.is_empty() => (*a, *b),
            _ => return Err(LegacyDecoderError::MalformedSyntheticProperty),
        };
        return Ok(SourcePropertyId::Synthetic {
            collateral_id: numeric(collateral, false, "synthetic collateral")?,
            contract_id: numeric(contract, false, "synthetic contract")?,
        });
    }
    Ok(SourcePropertyId::Regular(numeric(
        raw,
        false,
        "property id",
    )?))
}

fn literal_flag(value: &str) -> bool {
    value == "1"
}

fn source_decimal_ref(raw: &str) -> Option<u64> {
    // Historical parseInt(raw,10) takes an integer PREFIX, not a complete
    // canonical decimal number. Its NaN becomes false for withdrawal refs.
    let rest = raw.trim_start();
    let positive = rest.strip_prefix('+').unwrap_or(rest);
    if positive.starts_with('-') {
        return None;
    }
    let digits: String = positive.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits
        .parse::<u64>()
        .ok()
        .filter(|n| u128::from(*n) <= MAX_SOURCE_SAFE_INTEGER)
}

fn location_string(raw: &str) -> SourceChannelLocation {
    match raw.strip_prefix("ref:") {
        Some(tail) => {
            SourceChannelLocation::RawReference(tail.split(':').next().unwrap_or("").to_owned())
        }
        None => SourceChannelLocation::Address(raw.to_owned()),
    }
}

fn source_decode_commit(parts: &[&str]) -> Result<UnverifiedSourceBody, LegacyDecoderError> {
    let channel = parts
        .get(2)
        .ok_or(LegacyDecoderError::MissingIndexedFieldWouldThrow(
            "commit channel field",
        ))?;
    let property = source_decode_property_id_unverified(source_field(parts, 0))?;
    let raw_amount = source_field(parts, 1);
    let amount_atoms = amount_atoms(raw_amount)?;
    let mut clear_lists = Vec::new();
    let list_field = source_field(parts, 4);
    if list_field.starts_with('[') && list_field.ends_with(']') {
        let entries = &list_field[1..list_field.len() - 1];
        for value in entries.split(',') {
            clear_lists.push(numeric(value, false, "clearlist id")?);
        }
    } else if !list_field.is_empty() {
        if list_field.starts_with('[') || list_field.ends_with(']') {
            return Err(LegacyDecoderError::BracketedListSplitBeforeParsing);
        }
        clear_lists.push(numeric(list_field, false, "clearlist id")?);
    }
    if clear_lists.len() > MAX_SOURCE_CLEARLISTS {
        return Err(LegacyDecoderError::TooManyClearlistIds);
    }
    Ok(UnverifiedSourceBody::Commit {
        property,
        amount_atoms,
        channel: location_string(channel),
        pay_enabled: literal_flag(source_field(parts, 3)),
        clear_lists,
        colored_output: literal_flag(source_field(parts, 5)),
        commit_clearlist_id: match parts.get(6).filter(|s| !s.is_empty()) {
            None => None,
            Some(field) => Some(numeric(field, false, "commit clearlist id")?),
        },
    })
}

fn source_decode_withdrawal(parts: &[&str]) -> Result<UnverifiedSourceBody, LegacyDecoderError> {
    let place = source_field(parts, 4);
    let channel = match place.strip_prefix("ref:") {
        Some(raw) => SourceChannelLocation::WithdrawalReference(source_decimal_ref(
            raw.split(':').next().unwrap_or(""),
        )),
        None => SourceChannelLocation::Address(place.to_owned()),
    };
    Ok(UnverifiedSourceBody::Withdrawal {
        withdraw_all: literal_flag(source_field(parts, 0)),
        property: source_decode_property_id_unverified(source_field(parts, 1))?,
        amount_atoms: amount_atoms(source_field(parts, 2))?,
        column_flag: literal_flag(source_field(parts, 3)),
        channel,
    })
}

fn source_decode_transfer(parts: &[&str]) -> Result<UnverifiedSourceBody, LegacyDecoderError> {
    let place = parts
        .get(3)
        .ok_or(LegacyDecoderError::MissingIndexedFieldWouldThrow(
            "transfer destination",
        ))?;
    Ok(UnverifiedSourceBody::Transfer {
        property: source_decode_property_id_unverified(source_field(parts, 0))?,
        amount_atoms: amount_atoms(source_field(parts, 1))?,
        column_a: literal_flag(source_field(parts, 2)),
        destination: location_string(place),
        pay: literal_flag(source_field(parts, 4)),
        pay_reference: source_field(parts, 5).to_owned(),
    })
}

/// Caller supplies only the body AFTER the type selector. The actual JS
/// accepts malformed partial numeric strings and may yield NaN; this strict
/// Rust subset rejects those instead of inventing a safe economic value.
/// It does not accept a full OP_RETURN or a generic double-prefixed body.
pub fn source_decode_body_unverified(
    type_id: u8,
    body: &str,
) -> Result<UnverifiedDecodedBody, LegacyDecoderError> {
    body_check(body)?;
    if [28, 29, 34, 35].contains(&type_id) {
        return Err(LegacyDecoderError::MissingSourceDecoderExport(type_id));
    }
    let parts: Vec<_> = body.split(',').collect();
    let mut caveats = vec![SourceDecoderCaveat::NoChainOrHistoricalStateEvidence];
    let fields = match type_id {
        4 => {
            caveats.push(SourceDecoderCaveat::CommitBracketedClearlistCommasMisparse);
            caveats.push(SourceDecoderCaveat::AmountFloatLimitedToExactSafeAtoms);
            caveats.push(SourceDecoderCaveat::TwoDifferentReferenceNumberRules);
            source_decode_commit(&parts)?
        }
        7 => UnverifiedSourceBody::CreateWhitelist {
            backup_address: source_field(&parts, 0).to_owned(),
            name: source_field(&parts, 1).to_owned(),
            url: source_field(&parts, 2).to_owned(),
            description: source_field(&parts, 3).to_owned(),
        },
        8 => UnverifiedSourceBody::UpdateAdmin {
            new_address: source_field(&parts, 0).to_owned(),
            whitelist: literal_flag(source_field(&parts, 1)),
            oracle: literal_flag(source_field(&parts, 2)),
            token: literal_flag(source_field(&parts, 3)),
            list_id: numeric(source_field(&parts, 4), true, "admin target id")?,
            update_backup: literal_flag(source_field(&parts, 5)),
        },
        9 => {
            caveats.push(SourceDecoderCaveat::AttestationMissingMetadataThrows);
            let metadata = parts
                .get(3)
                .ok_or(LegacyDecoderError::MissingAttestationMetadataWouldThrow)?;
            UnverifiedSourceBody::Attestation {
                revoke: literal_flag(source_field(&parts, 0)),
                list_id: numeric(source_field(&parts, 1), true, "attestation list id")?,
                target_address: source_field(&parts, 2).to_owned(),
                metadata_upper: metadata.to_ascii_uppercase(),
            }
        }
        15 => UnverifiedSourceBody::CloseOraclePrimitive(numeric(
            body,
            true,
            "close oracle primitive",
        )?),
        21 => {
            caveats.push(SourceDecoderCaveat::TwoDifferentReferenceNumberRules);
            caveats.push(SourceDecoderCaveat::AmountFloatLimitedToExactSafeAtoms);
            source_decode_withdrawal(&parts)?
        }
        22 => {
            caveats.push(SourceDecoderCaveat::TwoDifferentReferenceNumberRules);
            caveats.push(SourceDecoderCaveat::AmountFloatLimitedToExactSafeAtoms);
            source_decode_transfer(&parts)?
        }
        30 => {
            caveats.push(SourceDecoderCaveat::AmountFloatLimitedToExactSafeAtoms);
            UnverifiedSourceBody::StakeFraudRelay {
                action: numeric(source_field(&parts, 0), true, "stake action")?,
                oracle_id: numeric(source_field(&parts, 1), true, "stake oracle")?,
                staked_property_id: numeric(source_field(&parts, 2), true, "stake property")?,
                amount_atoms: amount_atoms(source_field(&parts, 3))?,
                accused_address: source_field(&parts, 4).to_owned(),
                evidence_hash: source_field(&parts, 5).to_owned(),
                relay_type: numeric(source_field(&parts, 6), true, "stake relay type")?,
                state_hash: source_field(&parts, 7).to_owned(),
                dlc_ref: source_field(&parts, 8).to_owned(),
                settlement_state: source_field(&parts, 9).to_owned(),
                relay_blob: source_field(&parts, 10).to_owned(),
                auto_roll: literal_flag(source_field(&parts, 11)),
                next_dlc_ref: source_field(&parts, 12).to_owned(),
            }
        }
        32 => {
            caveats.push(SourceDecoderCaveat::Types32And33ValidatorCommentedOut);
            UnverifiedSourceBody::ZkBatchOpaque {
                ordinal_reveal_json: body.to_owned(),
            }
        }
        33 => {
            caveats.push(SourceDecoderCaveat::Types32And33ValidatorCommentedOut);
            let action = source_field(&parts, 0);
            if action.is_empty() || !action.bytes().all(|x| x.is_ascii_digit()) {
                return Err(LegacyDecoderError::NonCanonicalLegacyNumeric(
                    "colored action",
                ));
            }
            let action_decimal = action
                .parse::<u64>()
                .map_err(|_| LegacyDecoderError::NonCanonicalLegacyNumeric("colored action"))?;
            UnverifiedSourceBody::ColoredCoin {
                action_decimal,
                property_id: numeric(source_field(&parts, 1), false, "colored property")?,
                sats_ratio: numeric(source_field(&parts, 2), false, "colored ratio")?,
                home_address: source_field(&parts, 3).to_owned(),
            }
        }
        _ => return Err(LegacyDecoderError::UnsupportedOrUnportedLayout(type_id)),
    };
    Ok(UnverifiedDecodedBody {
        type_id,
        fields,
        caveats,
    })
}

/// Historical Type34 Types dispatcher calls decodeCrossLayerBridge, which
/// DOES NOT exist in txDecoder.js. The standalone decodeAbstractionBridge
/// helper DOES exist but cannot be silently substituted into Type34.
/// This method is an isolated read-only helper observation only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedStandaloneBridge {
    property_id: u64,
    amount: u64,
}
impl UnverifiedStandaloneBridge {
    pub const fn property_id(&self) -> u64 {
        self.property_id
    }
    pub const fn amount(&self) -> u64 {
        self.amount
    }
    pub const fn require_trusted_cross_layer_bridge(&self) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
    }
}
pub fn source_standalone_abstraction_bridge_helper_unverified(
    body: &str,
) -> Result<UnverifiedStandaloneBridge, LegacyDecoderError> {
    body_check(body)?;
    let parts: Vec<_> = body.split(',').collect();
    Ok(UnverifiedStandaloneBridge {
        property_id: numeric(source_field(&parts, 0), false, "bridge property")?,
        amount: numeric(source_field(&parts, 1), false, "bridge amount")?,
    })
}

/// Source first decodeSettleChannelPNL (base256) is SHADOWED by a second
/// same-named property (base94) in the same object literal. Runtime uses
/// the SECOND definition. Neither source Type23 settlement is authorized.
pub const fn source_type23_uses_later_base94_decoder() -> bool {
    true
}

// Full source-shaped observations complement the older strict typed subset.
// In particular Undefined, NaN and signed zero must not disappear in JSON.
pub const HISTORICAL_DECODER_METHODS: &[&str] = &[
    "decodeAmount",
    "decodeActivateTradeLayer",
    "decodeTokenIssue",
    "decodeSend",
    "decodePropertyId",
    "decodeTradeTokenForUTXO",
    "decodeCommitToken",
    "decodeOnChainTokenForToken",
    "decodeCancelOrder",
    "decodeCreateWhitelist",
    "decodeUpdateAdmin",
    "decodeIssueOrRevokeAttestation",
    "decodeAMMPool",
    "decodeGrantManagedToken",
    "decodeRedeemManagedToken",
    "decodeCreateOracle",
    "decodePublishOracleData",
    "decodeCloseOracle",
    "decodeCreateFutureContractSeries",
    "decodeExerciseDerivative",
    "decodeTradeContractOnchain",
    "decodeTradeContractChannel",
    "decodeTradeTokensChannel",
    "decodeWithdrawal",
    "decodeTransfer",
    "decodeMintSynthetic",
    "decodeRedeemSynthetic",
    "decodePayToTokens",
    "decodeOptionTrade",
    "decodeSettleChannelPNL",
    "decodeKingSettle",
    "decodeStakeFraudProof",
    "decodeBatchMoveZkRollup",
    "decodePublishNewTx",
    "decodeColoredCoin",
    "decodeAbstractionBridge",
];
pub const MAX_HISTORICAL_OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalDecoderError {
    Capacity,
    Arithmetic,
    MissingMethod(String),
    SourceThrow { name: &'static str, message: String },
}
impl core::fmt::Display for HistoricalDecoderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for HistoricalDecoderError {}
type HistoricalResult<T> = Result<T, HistoricalDecoderError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoricalNumber(u64);
impl HistoricalNumber {
    pub fn as_f64(self) -> f64 {
        f64::from_bits(self.0)
    }
    fn new(value: f64) -> Self {
        Self(if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalValue {
    Undefined,
    Null,
    Bool(bool),
    Number(HistoricalNumber),
    String(String),
    Array(Vec<HistoricalValue>),
    Object(Vec<(String, HistoricalValue)>),
}
impl HistoricalValue {
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
    /// Tagged JSON retains undefined/nonfinite/signed-zero source distinctions.
    /// It is diagnostic data, not a transaction encoding or admission proof.
    pub fn diagnostic_json(&self) -> HistoricalResult<String> {
        fn preflight(
            value: &HistoricalValue,
            depth: usize,
            nodes: &mut usize,
            bytes: &mut usize,
        ) -> HistoricalResult<()> {
            if depth > 32 {
                return Err(HistoricalDecoderError::Capacity);
            }
            *nodes = nodes
                .checked_add(1)
                .ok_or(HistoricalDecoderError::Capacity)?;
            if *nodes > 32768 {
                return Err(HistoricalDecoderError::Capacity);
            }
            let text_bytes = match value {
                HistoricalValue::String(text) => text.len(),
                HistoricalValue::Object(fields) => {
                    fields.iter().try_fold(0_usize, |total, (key, _)| {
                        total
                            .checked_add(key.len())
                            .ok_or(HistoricalDecoderError::Capacity)
                    })?
                }
                _ => 0,
            };
            *bytes = bytes
                .checked_add(text_bytes)
                .ok_or(HistoricalDecoderError::Capacity)?;
            if *bytes > MAX_HISTORICAL_OUTPUT_BYTES {
                return Err(HistoricalDecoderError::Capacity);
            }
            match value {
                HistoricalValue::Array(values) => {
                    for value in values {
                        preflight(value, depth + 1, nodes, bytes)?;
                    }
                }
                HistoricalValue::Object(fields) => {
                    for (_, value) in fields {
                        preflight(value, depth + 1, nodes, bytes)?;
                    }
                }
                _ => (),
            }
            Ok(())
        }
        preflight(self, 0, &mut 0, &mut 0)?;
        fn string(value: &str) -> HistoricalResult<String> {
            crate::issuance_intent::JsonValue::String(value.to_owned())
                .to_json()
                .map_err(|_| HistoricalDecoderError::Capacity)
        }
        fn write(value: &HistoricalValue, out: &mut String) -> HistoricalResult<()> {
            match value {
                HistoricalValue::Undefined => out.push_str("[\"undefined\"]"),
                HistoricalValue::Null => out.push_str("[\"null\"]"),
                HistoricalValue::Bool(value) => out.push_str(if *value {
                    "[\"boolean\",true]"
                } else {
                    "[\"boolean\",false]"
                }),
                HistoricalValue::Number(value) => {
                    let number = value.as_f64();
                    let text = if number.is_nan() {
                        "NaN".to_owned()
                    } else if number == f64::INFINITY {
                        "Infinity".to_owned()
                    } else if number == f64::NEG_INFINITY {
                        "-Infinity".to_owned()
                    } else {
                        format!("{:016x}", number.to_bits())
                    };
                    out.push_str(&format!("[\"number\",\"{text}\"]"));
                }
                HistoricalValue::String(value) => {
                    out.push_str("[\"string\",");
                    out.push_str(&string(value)?);
                    out.push(']');
                }
                HistoricalValue::Array(values) => {
                    out.push_str("[\"array\",[");
                    for (index, value) in values.iter().enumerate() {
                        if index != 0 {
                            out.push(',');
                        }
                        write(value, out)?;
                    }
                    out.push_str("]]");
                }
                HistoricalValue::Object(fields) => {
                    out.push_str("[\"object\",[");
                    for (index, (key, value)) in fields.iter().enumerate() {
                        if index != 0 {
                            out.push(',');
                        }
                        out.push('[');
                        out.push_str(&string(key)?);
                        out.push(',');
                        write(value, out)?;
                        out.push(']');
                    }
                    out.push_str("]]");
                }
            }
            if out.len() > MAX_HISTORICAL_OUTPUT_BYTES {
                return Err(HistoricalDecoderError::Capacity);
            }
            Ok(())
        }
        let mut output = String::new();
        write(self, &mut output)?;
        Ok(output)
    }
}
fn hn(value: f64) -> HistoricalValue {
    HistoricalValue::Number(HistoricalNumber::new(value))
}
fn hs(value: &str) -> HistoricalValue {
    HistoricalValue::String(value.to_owned())
}
fn hb(value: bool) -> HistoricalValue {
    HistoricalValue::Bool(value)
}
macro_rules! historical_object { ($($key:literal => $value:expr),* $(,)?) => { HistoricalValue::Object(vec![$(($key.to_owned(), $value)),*]) }; }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalDecodedBody {
    method: String,
    value: HistoricalValue,
}
impl HistoricalDecodedBody {
    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn value(&self) -> &HistoricalValue {
        &self.value
    }
    pub fn diagnostic_json(&self) -> HistoricalResult<String> {
        self.value.diagnostic_json()
    }
    pub const fn require_trusted_execution(&self) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
    }
    pub const fn require_historically_accepted_transaction(
        &self,
    ) -> Result<(), LegacyDecoderError> {
        Err(LegacyDecoderError::HistoricalTransactionNotAuthenticated)
    }
}

fn historical_space(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
/// Source VM parseInt numeric policy: signed prefixes, optional hexadecimal
/// autodetection, and V8's five-digit radix36 accumulation above safe integers.
fn historical_parse_int(raw: &str, radix: u32) -> f64 {
    let trimmed = raw.trim_start_matches(historical_space);
    let (negative, mut rest) = if let Some(rest) = trimmed.strip_prefix('-') {
        (true, rest)
    } else {
        (false, trimmed.strip_prefix('+').unwrap_or(trimmed))
    };
    let radix = if radix == 0 {
        if rest.starts_with("0x") || rest.starts_with("0X") {
            rest = &rest[2..];
            16
        } else {
            10
        }
    } else {
        radix
    };
    let digits: Vec<_> = rest
        .chars()
        .map_while(|ch| ch.to_digit(radix).filter(|_| ch.is_ascii()))
        .collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let number = if radix == 36 {
        let mut number = 0.0;
        let first_nonzero = digits
            .iter()
            .position(|digit| *digit != 0)
            .unwrap_or(digits.len());
        for chunk in digits[first_nonzero..].chunks(5) {
            let mut part = 0_u32;
            let mut multiplier = 1_u32;
            for digit in chunk {
                part = part * 36 + digit;
                multiplier *= 36;
            }
            number = number * f64::from(multiplier) + f64::from(part);
        }
        number
    } else if radix == 10 {
        let text: String = digits
            .iter()
            .map(|digit| char::from(b'0' + *digit as u8))
            .collect();
        text.parse::<f64>().unwrap_or(f64::INFINITY)
    } else {
        let mut number = crate::decimal::Decimal::zero();
        for digit in digits {
            let Ok(next) = number
                .checked_mul(&crate::decimal::Decimal::from(radix))
                .and_then(|value| value.checked_add(&crate::decimal::Decimal::from(digit)))
            else {
                return if negative {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
            };
            number = next;
        }
        number.to_string().parse::<f64>().unwrap_or(f64::INFINITY)
    };
    if negative {
        -number
    } else {
        number
    }
}
fn historical_parse_float(raw: &str) -> f64 {
    let text = raw.trim_start_matches(historical_space);
    let mut at = 0;
    if text
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'+' | b'-'))
    {
        at += 1;
    }
    if text[at..].starts_with("Infinity") {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let start_digits = at;
    while text.as_bytes().get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    let mut digits = at - start_digits;
    if text.as_bytes().get(at) == Some(&b'.') {
        at += 1;
        let before = at;
        while text.as_bytes().get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        digits += at - before;
    }
    if digits == 0 {
        return f64::NAN;
    }
    if text
        .as_bytes()
        .get(at)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        let exponent_at = at;
        at += 1;
        if text
            .as_bytes()
            .get(at)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            at += 1;
        }
        let before = at;
        while text.as_bytes().get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == before {
            at = exponent_at;
        }
    }
    text[..at].parse().unwrap_or(f64::NAN)
}
fn historical_number_text(number: f64) -> HistoricalResult<String> {
    if number.is_nan() {
        return Ok("NaN".into());
    }
    if number == f64::INFINITY {
        return Ok("Infinity".into());
    }
    if number == f64::NEG_INFINITY {
        return Ok("-Infinity".into());
    }
    crate::state_oracle::source_number_text(number).map_err(|_| HistoricalDecoderError::Arithmetic)
}
fn historical_loose_one(raw: &str) -> bool {
    let raw = raw.trim_matches(historical_space);
    for prefix in ["0x", "0X", "0b", "0B", "0o", "0O"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            // Only Number(raw)==1 is needed. Each radix has the same unique
            // digit representation for one after any number of leading zeros.
            return rest.trim_start_matches('0') == "1";
        }
    }
    raw.chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.' | 'e' | 'E'))
        && raw.parse::<f64>().ok() == Some(1.0)
}
enum HistoricalBigNumber {
    Finite(crate::decimal::Decimal, bool),
    Special(f64),
}
// Mirrors the explicit-base constructor's validation and parseNumeric retry,
// including uniform-case conversion, conditional plus stripping, and the
// surprising empty/minus-only base36 zero cases. Decimal constructors have a
// different parser and do not use this normalization.
enum HistoricalRadixInput {
    Text(String),
    Special(f64),
}
fn historical_radix_prefix(raw: &str) -> Option<(u32, String)> {
    let negative = raw.starts_with('-');
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(rest) = digits.strip_prefix(prefix) {
            let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
            if rest.chars().next().is_some_and(word) && rest.chars().all(|ch| word(ch) || ch == '.')
            {
                return Some((
                    radix,
                    if negative {
                        format!("-{rest}")
                    } else {
                        rest.to_owned()
                    },
                ));
            }
        }
    }
    None
}
fn historical_bn_radix_input(input: &str, base: u32) -> HistoricalResult<HistoricalRadixInput> {
    let mut input = input.to_owned();
    for _ in 0..32 {
        let negative = input.starts_with('-');
        let mut digits = input.strip_prefix('-').unwrap_or(&input).to_owned();
        let mut changed = false;
        let valid = loop {
            let mut dot = false;
            let mut invalid = None;
            for (index, ch) in digits.chars().enumerate() {
                if (ch.is_ascii_digit() || ch.is_ascii_lowercase()) && ch.is_digit(base) {
                    continue;
                }
                if ch == '.' && index != 0 && !dot {
                    dot = true;
                    continue;
                }
                invalid = Some(ch);
                break;
            }
            let Some(ch) = invalid else {
                break true;
            };
            if ch != '.' && !changed {
                let upper = digits.to_uppercase();
                let lower = digits.to_lowercase();
                if digits == upper && !lower.is_empty() {
                    digits = lower;
                    changed = true;
                    continue;
                }
                if digits == lower && !upper.is_empty() {
                    digits = upper;
                    changed = true;
                    continue;
                }
            }
            break false;
        };
        if valid {
            return Ok(HistoricalRadixInput::Text(if negative {
                format!("-{digits}")
            } else {
                digits
            }));
        }
        let trimmed = input.trim_matches(historical_space);
        let normalized = if let Some(rest) = trimmed.strip_prefix('+') {
            if rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
            {
                rest
            } else {
                trimmed
            }
        } else {
            trimmed
        };
        if matches!(normalized, "Infinity" | "-Infinity" | "NaN" | "-NaN") {
            return Ok(HistoricalRadixInput::Special(match normalized {
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => f64::NAN,
            }));
        }
        let mut normalized = normalized.to_owned();
        if let Some((radix, rest)) = historical_radix_prefix(&normalized) {
            if radix == base {
                normalized = rest;
            }
        }
        if normalized.ends_with('.')
            && normalized.len() > 1
            && !normalized[..normalized.len() - 1].contains('.')
        {
            normalized.pop();
        }
        if normalized.starts_with('.') && normalized.len() > 1 && !normalized[1..].contains('.') {
            normalized.insert(0, '0');
        }
        if normalized == input {
            return Ok(HistoricalRadixInput::Special(f64::NAN));
        }
        input = normalized;
    }
    Err(HistoricalDecoderError::Capacity)
}
fn historical_big_number(raw: &str, base: u32) -> HistoricalResult<HistoricalBigNumber> {
    use crate::decimal::{Decimal, Rounding};
    let normalized;
    let raw = if base == 36 {
        normalized = match historical_bn_radix_input(raw, base)? {
            HistoricalRadixInput::Text(value) => value,
            HistoricalRadixInput::Special(number) => {
                return Ok(HistoricalBigNumber::Special(number))
            }
        };
        normalized.as_str()
    } else {
        raw
    };
    let raw = raw.trim_matches(historical_space);
    if matches!(raw, "NaN" | "+NaN" | "-NaN") {
        return Ok(HistoricalBigNumber::Special(f64::NAN));
    }
    if matches!(raw, "Infinity" | "+Infinity" | "-Infinity") {
        return Ok(HistoricalBigNumber::Special(if raw.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        }));
    }
    let (negative, rest) = if let Some(rest) = raw.strip_prefix('-') {
        (true, rest)
    } else {
        (false, raw.strip_prefix('+').unwrap_or(raw))
    };
    if base == 10 {
        let unsigned = raw.strip_prefix('+').unwrap_or(raw);
        if let Some((radix, digits)) = historical_radix_prefix(unsigned) {
            return match historical_bn_radix_input(&digits, radix)? {
                HistoricalRadixInput::Text(text) => historical_big_number(&text, radix),
                HistoricalRadixInput::Special(number) => Ok(HistoricalBigNumber::Special(number)),
            };
        }
    }
    if base == 10 {
        if raw.is_empty() {
            return Ok(HistoricalBigNumber::Special(f64::NAN));
        }
        return match raw.parse::<Decimal>() {
            Ok(value) => Ok(HistoricalBigNumber::Finite(value, negative)),
            Err(crate::decimal::DecimalError::LimitExceeded) => Ok(HistoricalBigNumber::Special(
                raw.parse::<f64>().unwrap_or(f64::NAN),
            )),
            Err(_) => Ok(HistoricalBigNumber::Special(f64::NAN)),
        };
    }
    if rest.is_empty() {
        return Ok(if raw.is_empty() || raw == "-" {
            HistoricalBigNumber::Finite(Decimal::zero(), negative)
        } else {
            HistoricalBigNumber::Special(f64::NAN)
        });
    }
    let mut split = rest.split('.');
    let integer = split.next().unwrap_or("");
    let fraction = split.next().unwrap_or("");
    if split.next().is_some() || integer.is_empty() && fraction.is_empty() {
        return Ok(HistoricalBigNumber::Special(f64::NAN));
    }
    let parse_digits = |value: &str| {
        value
            .chars()
            .map(|ch| {
                ch.to_digit(base)
                    .filter(|_| ch.is_ascii())
                    .map(|digit| digit as u8)
            })
            .collect::<Option<Vec<_>>>()
    };
    let (Some(integer_digits), Some(mut fraction_digits)) =
        (parse_digits(integer), parse_digits(fraction))
    else {
        return Ok(HistoricalBigNumber::Special(f64::NAN));
    };
    let mut value = Decimal::zero();
    for digit in integer_digits {
        value = value
            .checked_mul(&Decimal::from(base))
            .and_then(|value| value.checked_add(&Decimal::from(u32::from(digit))))
            .map_err(|_| HistoricalDecoderError::Capacity)?;
        // Every caller divides by at most 1e8: a 341-digit coefficient
        // remains outside binary64's finite range after that division.
        if value.coefficient_digits() > 340 {
            return Ok(HistoricalBigNumber::Special(if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }));
        }
    }
    if !fraction_digits.is_empty() {
        let mut decimal_fraction = String::new();
        for _ in 0..21 {
            let mut carry = 0_u32;
            for digit in fraction_digits.iter_mut().rev() {
                let current = u32::from(*digit) * 10 + carry;
                *digit = (current % base) as u8;
                carry = current / base;
            }
            decimal_fraction.push(char::from(b'0' + carry as u8));
        }
        let fractional: Decimal = format!("0.{decimal_fraction}")
            .parse()
            .map_err(|_| HistoricalDecoderError::Arithmetic)?;
        value = value
            .checked_add(
                &fractional
                    .quantize(20, Rounding::HalfUp)
                    .map_err(|_| HistoricalDecoderError::Arithmetic)?,
            )
            .map_err(|_| HistoricalDecoderError::Arithmetic)?;
    }
    if negative {
        value = value.negated();
    }
    Ok(HistoricalBigNumber::Finite(value, negative))
}
fn historical_divide(
    value: HistoricalBigNumber,
    divisor: u32,
    places: Option<u32>,
) -> HistoricalResult<f64> {
    use crate::decimal::{Decimal, Rounding};
    match value {
        HistoricalBigNumber::Special(number) => Ok(number),
        HistoricalBigNumber::Finite(value, negative) => {
            let mut value = value
                .checked_div(&Decimal::from(divisor), 20, Rounding::HalfUp)
                .map_err(|_| HistoricalDecoderError::Arithmetic)?;
            if let Some(places) = places {
                value = value
                    .quantize(places, Rounding::Down)
                    .map_err(|_| HistoricalDecoderError::Arithmetic)?;
            }
            let number = value
                .to_string()
                .parse::<f64>()
                .map_err(|_| HistoricalDecoderError::Arithmetic)?;
            Ok(if number == 0.0 && negative {
                -0.0
            } else {
                number
            })
        }
    }
}
fn historical_number_divide(number: f64, divisor: u32) -> HistoricalResult<f64> {
    if !number.is_finite() {
        return Ok(number);
    }
    historical_divide(
        HistoricalBigNumber::Finite(
            historical_number_text(number)?
                .parse()
                .map_err(|_| HistoricalDecoderError::Arithmetic)?,
            number.is_sign_negative(),
        ),
        divisor,
        None,
    )
}
fn historical_amount(raw: &str) -> HistoricalResult<f64> {
    historical_divide(historical_big_number(raw, 36)?, 100_000_000, Some(8))
}
fn historical_flagged_amount(raw: &str) -> HistoricalResult<f64> {
    let decimal = raw.ends_with('~');
    let number = historical_parse_int(if decimal { &raw[..raw.len() - 1] } else { raw }, 36);
    if decimal {
        historical_number_divide(number, 100_000_000)
    } else {
        Ok(number)
    }
}
fn historical_property(raw: &str) -> HistoricalResult<HistoricalValue> {
    let Some(rest) = raw.strip_prefix('s') else {
        return Ok(hn(historical_parse_int(raw, 36)));
    };
    let segments: Vec<_> = rest.split('-').collect();
    let pair = match segments.as_slice() {
        [first, second] => Some((*first, *second)),
        ["", first, second] => Some((*first, *second)),
        _ => None,
    };
    let Some((first, second)) = pair else {
        return Ok(hs("s-NaN-NaN"));
    };
    let first = historical_parse_int(first, 36);
    let second = historical_parse_int(second, 36);
    if first.is_nan() || second.is_nan() {
        Ok(hs("s-NaN-NaN"))
    } else {
        Ok(hs(&format!(
            "s-{}-{}",
            historical_number_text(first)?,
            historical_number_text(second)?
        )))
    }
}
fn historical_throw(field: &str) -> HistoricalDecoderError {
    HistoricalDecoderError::SourceThrow {
        name: "TypeError",
        message: format!("Cannot read properties of undefined (reading '{field}')"),
    }
}
fn historical_base94(raw: &str, float: bool) -> HistoricalResult<f64> {
    let value = crate::legacy_base::base94_to_decimal(Some(raw)).map_err(|error| {
        HistoricalDecoderError::SourceThrow {
            name: "Error",
            // Converter errors can contain a lone UTF16 surrogate when the
            // first invalid digit is non-BMP. Match the reference runner's
            // Buffer.from(message, 'utf8') diagnostic projection explicitly.
            message: if error == crate::legacy_base::LegacyBaseError::UnrepresentableUtf16Error {
                "Digit is not in alphabet: \u{fffd}".to_owned()
            } else {
                error.to_string()
            },
        }
    })?;
    let text = value.as_deref().unwrap_or("null");
    Ok(if float {
        historical_parse_float(text)
    } else {
        historical_parse_int(text, 0)
    })
}

/// Execute one active pinned decoder's STRING argument semantics. The input
/// remains an untrusted post-selector body; no source name is an authority.
pub fn source_decode_historical_method_unverified(
    method: &str,
    body: &str,
) -> HistoricalResult<HistoricalDecodedBody> {
    if body.len() > MAX_DECODER_BODY_BYTES || method.len() > 128 {
        return Err(HistoricalDecoderError::Capacity);
    }
    if !HISTORICAL_DECODER_METHODS.contains(&method) {
        return Err(HistoricalDecoderError::MissingMethod(method.to_owned()));
    }
    let parts: Vec<_> = body.split(',').collect();
    let field = |index| parts.get(index).copied().unwrap_or("");
    let source_field = |index| parts.get(index).copied().unwrap_or("undefined");
    let zero = |index| {
        let value = field(index);
        if value.is_empty() {
            "0"
        } else {
            value
        }
    };
    let number = |index| hn(historical_parse_int(zero(index), 36));
    let flag = |index| hb(field(index) == "1");
    let string = |index| hs(field(index));
    let property = |index| historical_property(field(index));
    let amount = |index| historical_amount(zero(index)).map(hn);
    let value = match method {
        "decodeAmount" => hn(historical_flagged_amount(body)?),
        "decodePropertyId" => historical_property(body)?,
        "decodeActivateTradeLayer" => {
            let types = if field(0).contains(';') {
                field(0)
                    .split(';')
                    .map(|value| historical_parse_int(value, 10))
                    .filter(|value| !value.is_nan())
                    .map(hn)
                    .collect()
            } else {
                vec![hn(historical_parse_int(field(0), 10))]
            };
            historical_object!("txTypesToActivate" => HistoricalValue::Array(types), "codeHash" => parts.get(1).map_or(HistoricalValue::Undefined, |value| hs(value)))
        }
        "decodeTokenIssue" => {
            historical_object!("initialAmount" => number(0), "ticker" => string(1), "whitelists" => HistoricalValue::Array(if field(2).is_empty() { vec![] } else { field(2).split(';').map(|value| hn(historical_parse_int(value, 36))).collect() }), "managed" => flag(3), "backupAddress" => string(4), "nft" => flag(5), "coloredCoinHybrid" => flag(6), "proceduralType" => if field(7).is_empty() { HistoricalValue::Null } else { number(7) })
        }
        "decodeSend" => {
            let send: Vec<_> = body.split(';').collect();
            let get = |index| send.get(index).copied().unwrap_or("");
            let send_all = get(0) == "1";
            if send_all {
                historical_object!("sendAll" => hb(true), "address" => hs(get(1)))
            } else if send.len() == 4 || send.len() == 5 {
                let property = historical_parse_int(get(2), 36);
                historical_object!("sendAll" => hb(false), "address" => hs(get(1)), "propertyIds" => hn(if property.is_nan() || property == 0.0 { 0.0 } else { property }), "amounts" => hn(historical_flagged_amount(if get(3).is_empty() { "0" } else { get(3) })?), "isColoredOutput" => hb(get(5) == "1"))
            } else if send
                .get(2)
                .ok_or_else(|| historical_throw("includes"))?
                .contains(',')
            {
                let amounts: Vec<_> = get(3)
                    .split(',')
                    .map(historical_flagged_amount)
                    .collect::<HistoricalResult<_>>()?;
                let properties = get(2).split(',').enumerate().map(|(index, value)| historical_object!("propertyIds" => hn(historical_parse_int(value, 36)), "amounts" => amounts.get(index).copied().map_or(HistoricalValue::Undefined, hn))).collect();
                historical_object!("sendAll" => hb(false), "propertyIds" => HistoricalValue::Array(properties))
            } else {
                HistoricalValue::Undefined
            }
        }
        "decodeTradeTokenForUTXO" => {
            historical_object!("propertyId" => property(0)?, "amount" => amount(1)?, "columnA" => flag(2), "satsExpected" => hn(historical_parse_int(source_field(3), 36)), "tokenOutput" => hn(historical_parse_int(zero(4), 0)), "payToAddress" => hn(historical_parse_int(zero(5), 0)), "isColoredOutput" => flag(6), "tagWithdraw" => if field(7).is_empty() { HistoricalValue::Null } else { string(7) })
        }
        "decodeCommitToken" => {
            if parts.get(2).is_none() {
                return Err(historical_throw("startsWith"));
            }
            let reference = field(2)
                .strip_prefix("ref:")
                .map(|rest| rest.split(':').next().unwrap_or(""));
            let lists = if field(4).starts_with('[') && field(4).ends_with(']') {
                field(4)[1..field(4).len() - 1]
                    .split(',')
                    .map(|value| hn(historical_parse_int(value, 36)))
                    .collect()
            } else if !field(4).is_empty() {
                vec![hn(historical_parse_int(field(4), 36))]
            } else {
                vec![]
            };
            historical_object!("propertyId" => property(0)?, "amount" => amount(1)?, "channelAddress" => hs(if reference.is_some() { "" } else { field(2) }), "ref" => reference.map_or(hb(false), hs), "payEnabled" => flag(3), "clearLists" => HistoricalValue::Array(lists), "isColoredOutput" => flag(5), "commitClearlistId" => if field(6).is_empty() { HistoricalValue::Null } else { number(6) })
        }
        "decodeOnChainTokenForToken" => {
            historical_object!("propertyIdOffered" => property(0)?, "propertyIdDesired" => property(1)?, "amountOffered" => amount(2)?, "amountExpected" => amount(3)?, "stop" => flag(4), "post" => flag(5))
        }
        "decodeCancelOrder" => {
            let contract = historical_loose_one(field(0));
            let txid_index = if contract { 3 } else { 4 };
            let parameters = if field(txid_index).encode_utf16().count() > 20 {
                historical_object!("txid" => string(txid_index))
            } else {
                historical_object!("price" => if contract { hs(zero(3)) } else { hn(historical_divide(historical_big_number(zero(3), 10)?, 8, None)?) }, "side" => hs(zero(if contract { 4 } else { 5 })))
            };
            historical_object!("isContract" => hb(contract), "offeredPropertyId" => property(1)?, "desiredPropertyId" => if contract { HistoricalValue::Undefined } else { property(2)? }, "cancelAll" => number(if contract { 2 } else { 3 }), "cancelParams" => parameters)
        }
        "decodeCreateWhitelist" => {
            historical_object!("backupAddress" => string(0), "name" => string(1), "url" => string(2), "description" => string(3))
        }
        "decodeUpdateAdmin" => {
            historical_object!("newAddress" => string(0), "whitelist" => flag(1), "oracle" => flag(2), "token" => flag(3), "id" => number(4), "updateBackup" => flag(5))
        }
        "decodeIssueOrRevokeAttestation" => {
            if parts.get(3).is_none() {
                return Err(historical_throw("toUpperCase"));
            }
            historical_object!("revoke" => flag(0), "id" => number(1), "targetAddress" => string(2), "metaData" => hs(&field(3).to_uppercase()))
        }
        "decodeAMMPool" => {
            historical_object!("isRedeem" => flag(0), "isContract" => flag(1), "id" => property(2)?, "amount" => number(3), "id2" => property(4)?, "amount2" => number(5), "optionsMaker" => hn(historical_parse_int(zero(7), 0)), "optionsTaker" => hn(historical_parse_int(zero(8), 0)), "strategyBlob" => string(9))
        }
        "decodeGrantManagedToken" => {
            historical_object!("propertyId" => property(0)?, "amountGranted" => amount(1)?, "addressToGrantTo" => string(2), "dlcHash" => hs(if field(7).is_empty() { field(3) } else { field(7) }), "dlcTemplateId" => string(4), "dlcContractId" => string(5), "settlementState" => string(6))
        }
        "decodeRedeemManagedToken" => {
            historical_object!("propertyId" => property(0)?, "amountDestroyed" => amount(1)?, "dlcTemplateId" => string(2), "dlcContractId" => string(3), "settlementState" => string(4))
        }
        "decodeCreateOracle" => {
            historical_object!("ticker" => string(0), "url" => string(1), "backupAddress" => string(2), "whitelists" => HistoricalValue::Array(if field(3).is_empty() { vec![] } else { field(3).split(';').map(|value| hn(historical_parse_int(value, 36))).collect() }), "lag" => number(4))
        }
        "decodePublishOracleData" => {
            let mut fields = vec![
                ("oracleId".into(), number(0)),
                (
                    "price".into(),
                    hn(historical_divide(
                        if historical_parse_int(zero(1), 36).is_finite() {
                            HistoricalBigNumber::Finite(
                                historical_number_text(historical_parse_int(zero(1), 36))?
                                    .parse()
                                    .map_err(|_| HistoricalDecoderError::Arithmetic)?,
                                historical_parse_int(zero(1), 36).is_sign_negative(),
                            )
                        } else {
                            HistoricalBigNumber::Special(historical_parse_int(zero(1), 36))
                        },
                        10_000,
                        Some(4),
                    )?),
                ),
            ];
            for (index, key) in [(2, "high"), (3, "low"), (4, "close")] {
                if !field(index).is_empty() {
                    fields.push((key.into(), number(index)));
                }
            }
            HistoricalValue::Object(fields)
        }
        "decodeCloseOracle" => hn(historical_parse_int(
            if body.is_empty() { "0" } else { body },
            36,
        )),
        "decodeCreateFutureContractSeries" => {
            let native = field(0) == "1";
            let pairs = if native || field(2).is_empty() {
                vec![]
            } else {
                field(2)
                    .split(';')
                    .map(|pair| {
                        HistoricalValue::Array(
                            pair.split(':')
                                .map(|value| {
                                    if value.is_empty() {
                                        HistoricalValue::Null
                                    } else {
                                        hn(historical_parse_int(value, 36))
                                    }
                                })
                                .collect(),
                        )
                    })
                    .collect()
            };
            historical_object!("native" => hb(native), "underlyingOracleId" => number(1), "onChainData" => HistoricalValue::Array(pairs), "notionalPropertyId" => property(3)?, "notionalValue" => number(4), "collateralPropertyId" => property(5)?, "leverage" => hn(historical_parse_float(zero(6))), "expiryPeriod" => if field(7).is_empty() { HistoricalValue::Null } else { number(7) }, "series" => if field(8).is_empty() { HistoricalValue::Null } else { number(8) }, "inverse" => flag(9), "fee" => flag(10))
        }
        "decodeExerciseDerivative" => {
            historical_object!("derivativeContractId" => number(0), "amount" => number(1))
        }
        "decodeTradeContractOnchain" | "decodeTradeContractChannel" => {
            let raw_price = historical_parse_int(source_field(1), 36);
            let price = historical_number_divide(
                if raw_price.is_nan() || raw_price == 0.0 {
                    0.0
                } else {
                    raw_price
                },
                100_000_000,
            )?;
            if method == "decodeTradeContractOnchain" {
                historical_object!("contractId" => number(0), "price" => hn(price), "amount" => number(2), "sell" => flag(3), "insurance" => flag(4), "reduce" => flag(5), "post" => flag(6), "stop" => flag(7))
            } else {
                historical_object!("contractId" => number(0), "price" => hn(price), "amount" => number(2), "columnAIsSeller" => flag(3), "expiryBlock" => number(4), "insurance" => flag(5), "columnAIsMaker" => flag(6))
            }
        }
        "decodeTradeTokensChannel" => {
            historical_object!("propertyIdOffered" => property(0)?, "propertyIdDesired" => property(1)?, "amountOffered" => amount(2)?, "amountDesired" => amount(3)?, "columnAIsOfferer" => flag(4), "expiryBlock" => number(5), "columnAIsMaker" => flag(6), "Id1ColoredOutput" => flag(7), "Id2ColoredOutput" => flag(8))
        }
        "decodeWithdrawal" => {
            let reference = field(4).strip_prefix("ref:");
            let parsed = reference
                .map(|value| historical_parse_int(value.split(':').next().unwrap_or(""), 10));
            historical_object!("withdrawAll" => flag(0), "propertyId" => property(1)?, "amount" => amount(2)?, "column" => flag(3), "channelAddress" => hs(if reference.is_some() { "" } else { field(4) }), "ref" => match parsed { Some(number) if number.is_finite() => hn(number), _ => hb(false) })
        }
        "decodeTransfer" => {
            if parts.get(3).is_none() {
                return Err(historical_throw("startsWith"));
            }
            let reference = field(3)
                .strip_prefix("ref:")
                .map(|value| value.split(':').next().unwrap_or(""));
            historical_object!("propertyId" => property(0)?, "amount" => amount(1)?, "isColumnA" => flag(2), "toChannelAddress" => hs(if reference.is_some() { "" } else { field(3) }), "ref" => reference.map_or(hb(false), hs), "pay" => flag(4), "payRef" => string(5))
        }
        "decodeMintSynthetic" => {
            historical_object!("propertyId" => property(0)?, "contractId" => number(1), "amount" => amount(2)?)
        }
        "decodeRedeemSynthetic" => {
            historical_object!("propertyId" => number(0), "contractId" => number(1), "amount" => amount(2)?)
        }
        "decodePayToTokens" => {
            historical_object!("propertyIdTarget" => number(0), "propertyIdUsed" => number(1), "amount" => amount(2)?)
        }
        "decodeOptionTrade" => {
            let mut result = vec![
                ("contractId".into(), string(0)),
                ("ticker".into(), string(0)),
                ("price".into(), hn(historical_flagged_amount(zero(1))?)),
                ("amount".into(), number(2)),
                ("columnAIsSeller".into(), flag(3)),
                ("expiryBlock".into(), number(4)),
                ("columnAIsMaker".into(), flag(5)),
            ];
            if parts.len() > 6 {
                result.extend([
                    ("comboTicker".into(), string(6)),
                    ("comboPrice".into(), hn(historical_flagged_amount(zero(7))?)),
                    ("comboAmount".into(), number(8)),
                ]);
            }
            HistoricalValue::Object(result)
        }
        "decodeSettleChannelPNL" => {
            let optional_base = |index| {
                if field(index).is_empty() {
                    Ok(HistoricalValue::Undefined)
                } else {
                    historical_base94(field(index), false).map(hn)
                }
            };
            historical_object!("txidNeutralized1" => string(0), "txidNeutralized2" => string(1), "markPrice" => hn(historical_base94(zero(2), true)?), "settleType" => hn(historical_parse_int(zero(3), 0)), "columnAIsSeller" => flag(4), "columnAIsMaker" => flag(5), "netAmount" => hn(historical_base94(zero(6), true)?), "expiryBlock" => hn(historical_base94(zero(7), false)?), "blockStart" => optional_base(8)?, "blockEnd" => optional_base(9)?, "propertyId" => optional_base(10)?, "aPaysBDirection" => if field(11).is_empty() { HistoricalValue::Undefined } else { flag(11) }, "channelRoot" => if field(12).is_empty() { HistoricalValue::Undefined } else { string(12) }, "totalContracts" => optional_base(13)?, "neutralizedCount" => optional_base(14)?)
        }
        "decodeKingSettle" => {
            historical_object!("blockStart" => hn(historical_base94(zero(0), false)?), "blockEnd" => hn(historical_base94(zero(1), false)?), "propertyId" => hn(historical_base94(zero(2), false)?), "netAmount" => hn(historical_base94(zero(3), true)?), "aPaysBDirection" => flag(4), "channelRoot" => string(5), "totalContracts" => hn(historical_base94(zero(6), false)?), "neutralizedCount" => hn(historical_base94(zero(7), false)?))
        }
        "decodeStakeFraudProof" => {
            historical_object!("action" => number(0), "oracleId" => number(1), "stakedPropertyId" => number(2), "amount" => amount(3)?, "accusedAddress" => string(4), "evidenceHash" => string(5), "relayType" => number(6), "stateHash" => string(7), "dlcRef" => string(8), "settlementState" => string(9), "relayBlob" => string(10), "autoRoll" => flag(11), "nextDlcRef" => string(12))
        }
        "decodeBatchMoveZkRollup" | "decodePublishNewTx" => {
            historical_object!("ordinalRevealJSON" => hs(body))
        }
        "decodeColoredCoin" => {
            historical_object!("encodeDecodeRecode" => hn(historical_parse_int(source_field(0), 10)), "propertyId" => hn(historical_parse_int(source_field(1), 36)), "satsRatio" => hn(historical_parse_int(source_field(2), 36)), "homeAddress" => string(3))
        }
        "decodeAbstractionBridge" => {
            historical_object!("propertyId" => hn(historical_parse_int(source_field(0), 36)), "amount" => hn(historical_parse_int(source_field(1), 36)))
        }
        _ => unreachable!("method checked above"),
    };
    Ok(HistoricalDecodedBody {
        method: method.to_owned(),
        value,
    })
}

/// Uses the existing exact Types route inventory. Missing named exports remain
/// missing; the standalone bridge is never substituted for Type34.
pub fn source_decode_historical_type_unverified(
    type_id: u8,
    body: &str,
) -> HistoricalResult<HistoricalDecodedBody> {
    let route = crate::legacy_types_audit::source_route(type_id)
        .ok_or_else(|| HistoricalDecoderError::MissingMethod(format!("type {type_id}")))?;
    if !route.decoder_exported {
        return Err(HistoricalDecoderError::MissingMethod(
            route.decoder.to_owned(),
        ));
    }
    source_decode_historical_method_unverified(route.decoder, body)
}
