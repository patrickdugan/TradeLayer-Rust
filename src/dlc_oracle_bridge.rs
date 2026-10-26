//! Native ECDSA relay checks, with source compatibility and an independently
//! configured payload-bound policy. Neither result authorizes Ledger changes.
use crate::hash256::sha256;
use crate::issuance_intent::{IntentError, JsonNumber, JsonObject, JsonValue};
use crate::secp256k1::{
    canonical_compressed_public_key, verify_ecdsa_digest, verify_ecdsa_digest_strict, SecpError,
};
use crate::state_oracle::source_number_text;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "954a7fbf30b66e52b587a2d7108e080c41da812acff77ccd457c6e547cb1587f";
pub const MAX_RELAY_JSON_BYTES: usize = 65_536;
pub const MAX_PAYLOAD_BYTES: usize = 32_768;
pub const TRUSTED_POLICY: &str = "TrustedPayloadBoundV1";
const ORDER: [u8; 32] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
    0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlcBridgeError {
    Capacity,
    UnsupportedUtf16,
    UnsupportedNumber,
    SourceTypeError(&'static str),
    InvalidExpectation,
    TrustedMismatch(&'static str),
    Arithmetic,
}
impl core::fmt::Display for DlcBridgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DlcBridgeError {}
type Result<T> = core::result::Result<T, DlcBridgeError>;
fn json_error(error: IntentError) -> DlcBridgeError {
    match error {
        IntentError::Capacity | IntentError::DepthLimit => DlcBridgeError::Capacity,
        IntentError::UnsupportedSurrogate => DlcBridgeError::UnsupportedUtf16,
        IntentError::InvalidNumber => DlcBridgeError::UnsupportedNumber,
        _ => DlcBridgeError::SourceTypeError("JSON conversion"),
    }
}
fn bounded(value: &JsonValue) -> Result<()> {
    // The shared serializer measures the borrowed tree before allocating.
    value.to_json().map_err(json_error).map(|_| ())
}
fn get<'a>(value: &'a JsonValue, name: &str) -> Option<&'a JsonValue> {
    match value {
        JsonValue::Object(object) => object.get(name),
        _ => None,
    }
}
fn truthy(value: &JsonValue) -> bool {
    match value {
        JsonValue::Null | JsonValue::Bool(false) => false,
        JsonValue::Number(value) => value.as_f64() != 0.0,
        JsonValue::String(value) => !value.is_empty(),
        _ => true,
    }
}
fn selected<'a>(value: &'a JsonValue, names: &[&str]) -> Option<&'a JsonValue> {
    names
        .iter()
        .filter_map(|name| get(value, name))
        .find(|value| truthy(value))
}
fn js_trim(value: &str) -> &str {
    // Unicode White_Space additionally includes NEXT LINE; ECMAScript does
    // not. ECMAScript does additionally include the BOM code point.
    value.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}
fn js_string(value: &JsonValue) -> Result<String> {
    let text = match value {
        JsonValue::Null => "null".into(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => {
            source_number_text(value.as_f64()).map_err(|_| DlcBridgeError::Arithmetic)?
        }
        JsonValue::String(value) => value.clone(),
        JsonValue::Array(values) => {
            let mut output = String::new();
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                if !matches!(value, JsonValue::Null) {
                    output.push_str(&js_string(value)?);
                }
                if output.len() > MAX_RELAY_JSON_BYTES {
                    return Err(DlcBridgeError::Capacity);
                }
            }
            output
        }
        JsonValue::Object(object) => {
            // A JSON own toString is never callable and shadows the inherited
            // conversion; valueOf cannot return a primitive without a function.
            if object.get("toString").is_some() {
                return Err(DlcBridgeError::SourceTypeError(
                    "Cannot convert object to primitive value",
                ));
            }
            "[object Object]".into()
        }
    };
    if text.len() > MAX_RELAY_JSON_BYTES {
        return Err(DlcBridgeError::Capacity);
    }
    Ok(text)
}
fn js_number(value: &JsonValue) -> Result<f64> {
    Ok(match value {
        JsonValue::Null => 0.0,
        JsonValue::Bool(value) => f64::from(u8::from(*value)),
        JsonValue::Number(value) => value.as_f64(),
        JsonValue::String(_) | JsonValue::Array(_) | JsonValue::Object(_) => {
            let text = js_string(value)?;
            let text = js_trim(&text);
            if text.is_empty() {
                0.0
            } else if let Some((digits, radix)) = text
                .strip_prefix("0x")
                .or_else(|| text.strip_prefix("0X"))
                .map(|v| (v, 16))
                .or_else(|| {
                    text.strip_prefix("0b")
                        .or_else(|| text.strip_prefix("0B"))
                        .map(|v| (v, 2))
                })
                .or_else(|| {
                    text.strip_prefix("0o")
                        .or_else(|| text.strip_prefix("0O"))
                        .map(|v| (v, 8))
                })
            {
                if digits.is_empty() || !digits.chars().all(|c| c.is_ascii() && c.is_digit(radix)) {
                    f64::NAN
                } else {
                    let digits = digits.trim_start_matches('0');
                    if digits.len() > 1024 {
                        f64::INFINITY
                    } else {
                        let mut value = crate::decimal::Decimal::zero();
                        for digit in digits.chars() {
                            value = value
                                .checked_mul(&crate::decimal::Decimal::from(radix))
                                .and_then(|n| {
                                    n.checked_add(&crate::decimal::Decimal::from(
                                        digit.to_digit(radix).expect("validated digit"),
                                    ))
                                })
                                .map_err(|_| DlcBridgeError::Capacity)?;
                        }
                        value.to_string().parse::<f64>().unwrap_or(f64::INFINITY)
                    }
                }
            } else {
                text.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
    })
}
fn json_number(value: f64) -> Result<JsonValue> {
    // JSON.stringify serializes intermediate NaN and infinities as null.
    if value.is_finite() {
        Ok(JsonValue::Number(
            JsonNumber::from_f64(value).map_err(json_error)?,
        ))
    } else {
        Ok(JsonValue::Null)
    }
}
fn insert(object: &mut JsonObject, name: &str, value: JsonValue) -> Result<()> {
    object.insert(name.into(), value).map_err(json_error)
}

/// Source aliases/defaults preserve raw JSON field values and insertion order.
pub fn source_from_node_dlc_like(raw: &JsonValue) -> Result<Option<JsonValue>> {
    bounded(raw)?;
    if !matches!(raw, JsonValue::Object(_) | JsonValue::Array(_)) {
        return Ok(None);
    }
    let mut object = JsonObject::new();
    for (name, aliases) in [
        ("eventId", &["eventId", "oracleEventId", "event_id"][..]),
        ("outcome", &["outcome"][..]),
        ("outcomeIndex", &["outcomeIndex", "outcome_index"][..]),
        (
            "stateHash",
            &["stateHash", "tradeLayerStateHash", "state_hash"][..],
        ),
        (
            "timestamp",
            &["timestamp", "eventMaturityEpoch", "event_maturity_epoch"][..],
        ),
        ("payloadHash", &["payloadHash", "payload_hash"][..]),
        (
            "balancePayloadB64",
            &["balancePayloadB64", "balance_payload_b64"][..],
        ),
        (
            "oraclePubkeyHex",
            &["oraclePubkeyHex", "oraclePubkey", "oracle_public_key"][..],
        ),
        ("signatureHex", &["signatureHex"][..]),
    ] {
        let found = if name == "outcomeIndex" {
            aliases
                .iter()
                .filter_map(|name| get(raw, name))
                .find(|v| !matches!(v, JsonValue::Null))
        } else if matches!(name, "outcome" | "signatureHex") {
            selected(raw, aliases).or_else(|| {
                let array = get(
                    raw,
                    if name == "outcome" {
                        "outcomes"
                    } else {
                        "signatures"
                    },
                );
                match array {
                    Some(JsonValue::Array(values)) => values.first().filter(|v| truthy(v)),
                    _ => None,
                }
            })
        } else {
            selected(raw, aliases)
        };
        let default = if matches!(name, "outcomeIndex" | "timestamp") {
            json_number(0.0)?
        } else {
            JsonValue::String(String::new())
        };
        insert(&mut object, name, found.cloned().unwrap_or(default))?;
    }
    Ok(Some(JsonValue::Object(object)))
}

/// Complete Buffer.from(string, 'base64') decoder, including URL alphabet,
/// ignored characters and UTF-16 low bytes. Limits precede allocation.
fn base64_decode(text: &str) -> Result<Vec<u8>> {
    if text.len() > MAX_RELAY_JSON_BYTES {
        return Err(DlcBridgeError::Capacity);
    }
    let mut output = Vec::new();
    let (mut accumulator, mut bits) = (0u32, 0u32);
    for unit in text.encode_utf16() {
        let byte = unit as u8;
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        accumulator = (accumulator << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if output.len() == MAX_PAYLOAD_BYTES {
                return Err(DlcBridgeError::Capacity);
            }
            output.push((accumulator >> bits) as u8);
        }
    }
    Ok(output)
}
fn buffer_from_json(value: &JsonValue) -> Result<Vec<u8>> {
    if let JsonValue::String(value) = value {
        return base64_decode(value);
    }
    let (length, array, object) = match value {
        JsonValue::Array(values) => (values.len(), Some(values), None),
        JsonValue::Object(object) => {
            if let Some(length) = object.get("length") {
                let length = match length {
                    JsonValue::Number(value) if value.as_f64() > 0.0 => {
                        let value = value.as_f64().floor();
                        if value > MAX_PAYLOAD_BYTES as f64 {
                            return Err(DlcBridgeError::Capacity);
                        }
                        value as usize
                    }
                    _ => 0,
                };
                (length, None, Some(object))
            } else if matches!(object.get("type"), Some(JsonValue::String(s)) if s=="Buffer") {
                if let Some(JsonValue::Array(values)) = object.get("data") {
                    (values.len(), Some(values), None)
                } else {
                    return Err(DlcBridgeError::SourceTypeError("Invalid Buffer argument"));
                }
            } else {
                return Err(DlcBridgeError::SourceTypeError("Invalid Buffer argument"));
            }
        }
        _ => return Err(DlcBridgeError::SourceTypeError("Invalid Buffer argument")),
    };
    if length > MAX_PAYLOAD_BYTES {
        return Err(DlcBridgeError::Capacity);
    }
    let mut output = Vec::with_capacity(length);
    for index in 0..length {
        let value = array
            .and_then(|a| a.get(index))
            .or_else(|| object.and_then(|object| object.get(&index.to_string())));
        let number = value.map_or(Ok(0.0), js_number)?;
        output.push(if number.is_finite() {
            number.trunc().rem_euclid(256.0) as u8
        } else {
            0
        });
    }
    Ok(output)
}

/// JSON.parse first coerces raw to a string; an object is not already parsed.
/// Malformed JSON returns None, while unsupported UTF-16/resource cases err.
pub fn source_parse_relay_blob(relay: &JsonValue) -> Result<Option<JsonValue>> {
    bounded(relay)?;
    if !truthy(relay) {
        return Ok(None);
    }
    let text = match relay {
        JsonValue::String(text) if text.starts_with("b64:") => {
            String::from_utf8_lossy(&base64_decode(&text[4..])?).into_owned()
        }
        value => match js_string(value) {
            Ok(text) => text,
            Err(DlcBridgeError::SourceTypeError(_)) => return Ok(None),
            Err(error) => return Err(error),
        },
    };
    let parsed = match JsonValue::parse(&text) {
        Ok(value) => value,
        Err(IntentError::InvalidJson) => return Ok(None),
        Err(error) => return Err(json_error(error)),
    };
    Ok(Some(source_from_node_dlc_like(&parsed)?.unwrap_or(parsed)))
}

pub fn canonical_relay_message(bundle: &JsonValue) -> Result<String> {
    canonical_message(bundle, true)
}
pub fn relay_message_digest(bundle: &JsonValue) -> Result<[u8; 32]> {
    Ok(sha256(canonical_relay_message(bundle)?.as_bytes()))
}
fn canonical_message(bundle: &JsonValue, include_payload: bool) -> Result<String> {
    bounded(bundle)?;
    if matches!(bundle, JsonValue::Null) {
        return Err(DlcBridgeError::SourceTypeError(
            "Cannot read properties of null",
        ));
    }
    let mut canonical = JsonObject::new();
    for name in [
        "eventId",
        "outcome",
        "outcomeIndex",
        "stateHash",
        "timestamp",
    ] {
        let value = get(bundle, name).filter(|value| truthy(value));
        let value = if matches!(name, "outcomeIndex" | "timestamp") {
            json_number(value.map_or(Ok(0.0), js_number)?)?
        } else {
            JsonValue::String(value.map_or(Ok(String::new()), js_string)?)
        };
        insert(&mut canonical, name, value)?;
    }
    if include_payload {
        if let Some(value) = get(bundle, "payloadHash").filter(|v| !matches!(v, JsonValue::Null)) {
            insert(
                &mut canonical,
                "payloadHash",
                JsonValue::String(js_string(value)?),
            )?;
        }
    }
    canonical.to_json().map_err(json_error)
}
fn from_hex_prefix(text: &str) -> Vec<u8> {
    let mut output = Vec::new();
    for pair in text.as_bytes().chunks_exact(2) {
        let digit = |b: u8| char::from(b).to_digit(16).map(|v| v as u8);
        let (Some(high), Some(low)) = (digit(pair[0]), digit(pair[1])) else {
            break;
        };
        output.push(high << 4 | low);
    }
    output
}
fn encoded_buffer(value: &JsonValue, sizes: &[usize]) -> Result<Option<Vec<u8>>> {
    bounded(value)?;
    let JsonValue::String(text) = value else {
        return Ok(None);
    };
    let text = js_trim(text);
    if !sizes.contains(&text.encode_utf16().count()) {
        return Ok(None);
    }
    Ok(Some(from_hex_prefix(text)))
}
pub fn source_pubkey_buffer(value: &JsonValue) -> Result<Option<Vec<u8>>> {
    encoded_buffer(value, &[66, 130])
}
pub fn source_signature_buffer(value: &JsonValue) -> Result<Option<Vec<u8>>> {
    encoded_buffer(value, &[128])
}
fn source_key(mut key: Vec<u8>) -> Result<Vec<u8>> {
    // tiny-secp accepts valid hybrid SEC1 encodings as libsecp256k1 does.
    // The native primitive deliberately accepts only ordinary SEC1, so check
    // the hybrid parity before translating it to the same affine point.
    if key.len() == 65 && matches!(key[0], 6 | 7) {
        if key[64] & 1 != key[0] & 1 {
            return Err(DlcBridgeError::SourceTypeError("Expected Point"));
        }
        key[0] = 4;
    }
    canonical_compressed_public_key(&key).map_err(|e| match e {
        SecpError::Arithmetic => DlcBridgeError::Arithmetic,
        _ => DlcBridgeError::SourceTypeError("Expected Point"),
    })?;
    Ok(key)
}
fn source_verify(digest: &[u8; 32], key: &[u8], sig: &[u8]) -> Result<bool> {
    // tiny performs length/range checks before curve parsing, and accepts
    // zero scalars syntactically but returns false for their verification.
    if !matches!(key.len(), 32 | 33 | 65) {
        return Err(DlcBridgeError::SourceTypeError("Expected Point"));
    }
    if sig.len() != 64 || &sig[..32] >= ORDER.as_slice() || &sig[32..] >= ORDER.as_slice() {
        return Err(DlcBridgeError::SourceTypeError("Expected Signature"));
    }
    let key = source_key(key.to_vec())?;
    if sig[..32].iter().all(|v| *v == 0) || sig[32..].iter().all(|v| *v == 0) {
        return Ok(false);
    }
    let signature: &[u8; 64] = sig.try_into().expect("checked signature length");
    verify_ecdsa_digest(digest, &key, signature).map_err(|e| match e {
        SecpError::Arithmetic => DlcBridgeError::Arithmetic,
        SecpError::InvalidPublicKey => DlcBridgeError::SourceTypeError("Expected Point"),
        SecpError::InvalidSignature => DlcBridgeError::SourceTypeError("Expected Signature"),
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayVerdict {
    pub valid: bool,
    pub reason: Option<&'static str>,
}
impl RelayVerdict {
    fn invalid(reason: &'static str) -> Self {
        Self {
            valid: false,
            reason: Some(reason),
        }
    }
    pub fn to_json(&self) -> String {
        self.reason.map_or_else(
            || "{\"valid\":true}".into(),
            |reason| format!("{{\"valid\":false,\"reason\":\"{reason}\"}}"),
        )
    }
}
pub fn source_validate_relay_bundle(
    bundle: &JsonValue,
    expected_state_hash: Option<&JsonValue>,
) -> Result<RelayVerdict> {
    bounded(bundle)?;
    if let Some(expected) = expected_state_hash {
        bounded(expected)?;
    }
    if !matches!(bundle, JsonValue::Object(_) | JsonValue::Array(_)) {
        return Ok(RelayVerdict::invalid("Missing relay bundle"));
    }
    let pubkey = get(bundle, "oraclePubkeyHex")
        .map(source_pubkey_buffer)
        .transpose()?
        .flatten();
    let signature = get(bundle, "signatureHex")
        .map(source_signature_buffer)
        .transpose()?
        .flatten();
    let (Some(pubkey), Some(signature)) = (pubkey, signature) else {
        return Ok(RelayVerdict::invalid(
            "Invalid oracle pubkey/signature encoding",
        ));
    };
    let digest = sha256(canonical_relay_message(bundle)?.as_bytes());
    let mut verified = source_verify(&digest, &pubkey, &signature)?;
    if !verified {
        verified = source_verify(
            &sha256(canonical_message(bundle, false)?.as_bytes()),
            &pubkey,
            &signature,
        )?;
    }
    if !verified {
        return Ok(RelayVerdict::invalid("Invalid oracle relay signature"));
    }
    if let Some(hash) = get(bundle, "payloadHash").filter(|v| truthy(v)) {
        let hash = js_string(hash)?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(RelayVerdict::invalid("Invalid payloadHash format"));
        }
        if let Some(payload) = get(bundle, "balancePayloadB64").filter(|v| truthy(v)) {
            if hash_hex(&sha256(&buffer_from_json(payload)?)) != hash.to_ascii_lowercase() {
                return Ok(RelayVerdict::invalid(
                    "payloadHash mismatch for balance payload",
                ));
            }
        }
    }
    if let Some(expected) = expected_state_hash.filter(|v| truthy(v)) {
        let state = get(bundle, "stateHash")
            .filter(|v| truthy(v))
            .map_or(Ok(String::new()), js_string)?;
        if state != js_string(expected)? {
            return Ok(RelayVerdict::invalid("Relay state hash mismatch"));
        }
    }
    Ok(RelayVerdict {
        valid: true,
        reason: None,
    })
}
fn hash_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// These expectations must come from an independently trusted configuration,
/// never from the relay being checked. The payload is exact raw bytes, not a
/// claim that its self-hash is an authenticated transition or balance root.
pub struct TrustedRelayExpectation<'a> {
    pub oracle_public_key: &'a [u8],
    pub event_id: &'a str,
    pub outcome: &'a str,
    pub outcome_index: f64,
    pub state_hash: &'a str,
    pub timestamp: f64,
    pub payload: &'a [u8],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedRelayEvidence {
    pub oracle_public_key: [u8; 33],
    pub canonical_message: String,
    pub message_digest: [u8; 32],
    pub payload_hash: [u8; 32],
}
pub fn validate_trusted_payload_bound(
    bundle: &JsonValue,
    expected: &TrustedRelayExpectation<'_>,
) -> Result<TrustedRelayEvidence> {
    bounded(bundle)?;
    if expected.payload.len() > MAX_PAYLOAD_BYTES
        || [expected.event_id, expected.outcome, expected.state_hash]
            .iter()
            .any(|s| s.is_empty() || s.len() > 4096)
        || [expected.outcome_index, expected.timestamp]
            .iter()
            .any(|n| !n.is_finite() || *n < 0.0 || n.fract() != 0.0 || *n > 9_007_199_254_740_991.0)
    {
        return Err(DlcBridgeError::InvalidExpectation);
    }
    let key = canonical_compressed_public_key(expected.oracle_public_key)
        .map_err(|_| DlcBridgeError::InvalidExpectation)?;
    let required_string = |name: &'static str| match get(bundle, name) {
        Some(JsonValue::String(value)) => Ok(value.as_str()),
        _ => Err(DlcBridgeError::TrustedMismatch(name)),
    };
    for (name, value) in [
        ("eventId", expected.event_id),
        ("outcome", expected.outcome),
        ("stateHash", expected.state_hash),
    ] {
        if required_string(name)? != value {
            return Err(DlcBridgeError::TrustedMismatch(name));
        }
    }
    for (name, value) in [
        ("outcomeIndex", expected.outcome_index),
        ("timestamp", expected.timestamp),
    ] {
        if !matches!(get(bundle,name),Some(JsonValue::Number(n)) if n.as_f64()==value) {
            return Err(DlcBridgeError::TrustedMismatch(name));
        }
    }
    let public_hex = required_string("oraclePubkeyHex")?;
    if !js_trim(public_hex).bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(DlcBridgeError::TrustedMismatch("oraclePubkeyHex"));
    }
    let bundle_key = source_pubkey_buffer(&JsonValue::String(public_hex.into()))?
        .ok_or(DlcBridgeError::TrustedMismatch("oraclePubkeyHex"))?;
    if canonical_compressed_public_key(&bundle_key)
        .map_err(|_| DlcBridgeError::TrustedMismatch("oraclePubkeyHex"))?
        != key
    {
        return Err(DlcBridgeError::TrustedMismatch("oraclePubkeyHex"));
    }
    let payload_hash = sha256(expected.payload);
    if required_string("payloadHash")? != hash_hex(&payload_hash) {
        return Err(DlcBridgeError::TrustedMismatch("payloadHash"));
    }
    if base64_decode(required_string("balancePayloadB64")?)? != expected.payload {
        return Err(DlcBridgeError::TrustedMismatch("balancePayloadB64"));
    }
    let signature =
        source_signature_buffer(&JsonValue::String(required_string("signatureHex")?.into()))?
            .ok_or(DlcBridgeError::TrustedMismatch("signatureHex"))?;
    let signature: &[u8; 64] = signature
        .as_slice()
        .try_into()
        .map_err(|_| DlcBridgeError::TrustedMismatch("signatureHex"))?;
    let canonical_message = canonical_relay_message(bundle)?;
    let message_digest = sha256(canonical_message.as_bytes());
    match verify_ecdsa_digest_strict(&message_digest, &key, signature) {
        Ok(true) => Ok(TrustedRelayEvidence {
            oracle_public_key: key,
            canonical_message,
            message_digest,
            payload_hash,
        }),
        Ok(false) | Err(SecpError::InvalidSignature | SecpError::InvalidPublicKey) => {
            Err(DlcBridgeError::TrustedMismatch("signatureHex"))
        }
        Err(SecpError::Arithmetic) => Err(DlcBridgeError::Arithmetic),
    }
}
