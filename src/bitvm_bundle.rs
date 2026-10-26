//! BitVM artifact integrity and shared bounded metadata primitives. No proof verifier.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::issuance_intent::{JsonNumber, JsonObject, JsonValue};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

pub const SOURCE_SHA256: &str = "e63a2f2e09e195a03ab2e3acb3fb6f582917c628a4c08774911c4ba6558903fe";
pub const MAX_BUNDLE_BYTES: usize = 65_536;
pub const MAX_RECORDS: usize = 1024;
pub const MAX_EVENTS: usize = 4096;
pub const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_STATE_NODES: usize = 65_536;
pub const DEFAULT_BUNDLE_PATH: &str =
    r"C:\projects\UTXORef\UTXO-Ref\bitvm3\utxo_referee\artifacts\m1_challenge_bundle_latest.json";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitvmError {
    Capacity,
    NonFinite,
    UnsupportedSourceDomain,
    InvalidJson,
    InvalidCheckpoint,
    ConfigMismatch,
    Source(String),
    Io(String),
}
impl core::fmt::Display for BitvmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BitvmError {}
pub type Result<T> = core::result::Result<T, BitvmError>;
pub type Environment = BTreeMap<String, String>;
#[derive(Debug, Clone, PartialEq)]
pub struct BundleHashMatch {
    pub bundle_path: String,
    pub bundle_hash: String,
    pub selected_bucket_pct: f64,
    pub source_hashes: JsonValue,
}
pub fn default_bundle_path(environment: Option<&str>) -> &str {
    environment
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BUNDLE_PATH)
}
pub fn normalize_hex(value: &str) -> String {
    value.trim_matches(space).to_lowercase()
}
pub fn compute_bundle_hash(document: &JsonValue) -> Result<String> {
    document.to_json().map_err(|_| BitvmError::Capacity)?;
    let mut object = JsonObject::new();
    match document {
        JsonValue::Object(fields) => {
            for (key, value) in fields.iter() {
                if key != "bundleHash" {
                    put(&mut object, key, value.clone())?;
                }
            }
        }
        JsonValue::Array(values) => {
            for (i, value) in values.iter().enumerate() {
                put(&mut object, &i.to_string(), value.clone())?;
            }
        }
        JsonValue::String(text) => {
            for (i, ch) in text.chars().enumerate() {
                if ch.len_utf16() != 1 {
                    return Err(BitvmError::UnsupportedSourceDomain);
                }
                put(
                    &mut object,
                    &i.to_string(),
                    JsonValue::String(ch.to_string()),
                )?;
            }
        }
        _ => (),
    }
    Ok(hash_hex(
        object
            .to_json()
            .map_err(|_| BitvmError::Capacity)?
            .as_bytes(),
    ))
}
pub fn verify_bundle_bytes(
    expected: &str,
    path_label: &str,
    raw: &[u8],
) -> Result<BundleHashMatch> {
    if expected.len() > 4096 || path_label.len() > 4096 || raw.len() > MAX_BUNDLE_BYTES {
        return Err(BitvmError::Capacity);
    }
    let want = normalize_hex(expected);
    if want.is_empty() {
        return Err(source("Missing bundleHash"));
    }
    let text = core::str::from_utf8(raw).map_err(|_| BitvmError::InvalidJson)?;
    let parsed = JsonValue::parse(text).map_err(|_| BitvmError::InvalidJson)?;
    let fields = match &parsed {
        JsonValue::Object(object) => Some(object),
        JsonValue::Null => return Err(BitvmError::UnsupportedSourceDomain),
        _ => None,
    };
    let declared = normalize_hex(&js_string(or(
        fields.and_then(|v| v.get("bundleHash")),
        &JsonValue::from(""),
    ))?);
    if declared.is_empty() {
        return Err(source(&format!("Bundle at {path_label} has no bundleHash")));
    }
    if declared != compute_bundle_hash(&parsed)? {
        return Err(source(&format!(
            "Bundle hash self-check failed at {path_label}"
        )));
    }
    if declared != want {
        return Err(source(&format!(
            "Provided bundleHash does not match artifact at {path_label}"
        )));
    }
    let selected_bucket_pct = number(Some(or(
        fields.and_then(|v| v.get("selectedBucketPct")),
        &JsonValue::Number(JsonNumber::from_f64(0.0).expect("zero")),
    )))?;
    let source_hashes = or(
        fields.and_then(|v| v.get("sourceHashes")),
        &JsonValue::Object(JsonObject::new()),
    )
    .clone();
    Ok(BundleHashMatch {
        bundle_path: path_label.into(),
        bundle_hash: declared,
        selected_bucket_pct,
        source_hashes,
    })
}
pub fn verify_bundle_file(expected: &str, path: &Path) -> Result<BundleHashMatch> {
    if expected.len() > 4096 || path.as_os_str().len() > 4096 {
        return Err(BitvmError::Capacity);
    }
    if normalize_hex(expected).is_empty() {
        return Err(source("Missing bundleHash"));
    }
    let absolute = std::path::absolute(path).map_err(|e| BitvmError::Io(e.to_string()))?;
    let file = std::fs::File::open(&absolute).map_err(|e| BitvmError::Io(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take((MAX_BUNDLE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| BitvmError::Io(e.to_string()))?;
    verify_bundle_bytes(expected, &absolute.to_string_lossy(), &bytes)
}
pub(crate) fn hash_hex(bytes: &[u8]) -> String {
    crate::hash256::sha256(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(crate) fn source(message: &str) -> BitvmError {
    BitvmError::Source(message.to_owned())
}
pub(crate) fn put(object: &mut JsonObject, key: &str, value: JsonValue) -> Result<()> {
    object
        .insert(key.into(), value)
        .map_err(|_| BitvmError::Capacity)
}
pub(crate) fn num(value: f64) -> Result<JsonValue> {
    JsonNumber::from_f64(value)
        .map(JsonValue::Number)
        .map_err(|_| BitvmError::NonFinite)
}
pub(crate) fn text(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}
pub(crate) fn truthy(value: &JsonValue) -> bool {
    match value {
        JsonValue::Null | JsonValue::Bool(false) => false,
        JsonValue::Number(v) => v.as_f64() != 0.0,
        JsonValue::String(s) => !s.is_empty(),
        _ => true,
    }
}
pub(crate) fn or<'a>(value: Option<&'a JsonValue>, fallback: &'a JsonValue) -> &'a JsonValue {
    value.filter(|v| truthy(v)).unwrap_or(fallback)
}
pub(crate) fn string_field(object: &JsonObject, key: &str) -> Result<String> {
    js_string(or(object.get(key), &text("")))
}
pub(crate) fn number_field(object: &JsonObject, key: &str) -> Result<f64> {
    number(Some(or(object.get(key), &num(0.0)?)))
}
pub(crate) fn object(value: Option<&JsonValue>) -> Option<&JsonObject> {
    match value {
        Some(JsonValue::Object(v)) => Some(v),
        _ => None,
    }
}
pub(crate) fn js_string(value: &JsonValue) -> Result<String> {
    Ok(match value {
        JsonValue::Null => "null".into(),
        JsonValue::Bool(v) => v.to_string(),
        JsonValue::Number(v) => crate::state_oracle::source_number_text(v.as_f64())
            .map_err(|_| BitvmError::NonFinite)?,
        JsonValue::String(v) => v.clone(),
        JsonValue::Array(values) => {
            let mut parts = Vec::new();
            for value in values {
                parts.push(if matches!(value, JsonValue::Null) {
                    String::new()
                } else {
                    js_string(value)?
                });
            }
            parts.join(",")
        }
        JsonValue::Object(fields) => {
            if fields.get("toString").is_some() || fields.get("valueOf").is_some() {
                return Err(BitvmError::UnsupportedSourceDomain);
            }
            "[object Object]".into()
        }
    })
}
pub(crate) fn space(ch: char) -> bool {
    matches!(ch,'\u{9}'..='\u{d}'|'\u{20}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
pub(crate) fn number(value: Option<&JsonValue>) -> Result<f64> {
    Ok(match value {
        None => f64::NAN,
        Some(JsonValue::Null) => 0.0,
        Some(JsonValue::Bool(v)) => {
            if *v {
                1.0
            } else {
                0.0
            }
        }
        Some(JsonValue::Number(v)) => v.as_f64(),
        Some(value) => {
            let owned = js_string(value)?;
            let raw = owned.trim_matches(space);
            if raw.is_empty() {
                0.0
            } else if matches!(raw, "Infinity" | "+Infinity") {
                f64::INFINITY
            } else if raw == "-Infinity" {
                f64::NEG_INFINITY
            } else {
                let prefix = [
                    ("0x", 16),
                    ("0X", 16),
                    ("0b", 2),
                    ("0B", 2),
                    ("0o", 8),
                    ("0O", 8),
                ]
                .into_iter()
                .find_map(|(prefix, base)| raw.strip_prefix(prefix).map(|rest| (rest, base)));
                if let Some((digits, base)) = prefix {
                    if digits.is_empty()
                        || !digits.chars().all(|c| c.is_ascii() && c.is_digit(base))
                    {
                        f64::NAN
                    } else {
                        let mut value = crate::decimal::Decimal::zero();
                        for digit in digits.chars() {
                            value = value
                                .checked_mul(&crate::decimal::Decimal::from(base))
                                .and_then(|v| {
                                    v.checked_add(&crate::decimal::Decimal::from(
                                        digit.to_digit(base).expect("checked"),
                                    ))
                                })
                                .map_err(|_| BitvmError::Capacity)?;
                            if value.coefficient_digits() > 340 {
                                return Ok(f64::INFINITY);
                            }
                        }
                        value.to_string().parse().unwrap_or(f64::INFINITY)
                    }
                } else if raw
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
                {
                    raw.parse().unwrap_or(f64::NAN)
                } else {
                    f64::NAN
                }
            }
        }
    })
}
pub(crate) fn validate_env(env: &Environment) -> Result<()> {
    if env.len() > 32 || env.iter().any(|(k, v)| k.len() > 128 || v.len() > 1024) {
        Err(BitvmError::Capacity)
    } else {
        Ok(())
    }
}
pub(crate) fn millis(value: u64) -> Result<JsonValue> {
    if value > 9_007_199_254_740_991 {
        return Err(BitvmError::Capacity);
    }
    num(value as f64)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEvent {
    pub operation: String,
    pub key: String,
    pub input: JsonObject,
    pub context: JsonObject,
    pub at_ms: u64,
}
pub(crate) fn preflight(input: &JsonObject) -> Result<()> {
    input
        .to_json()
        .map(|_| ())
        .map_err(|_| BitvmError::Capacity)
}
pub(crate) fn budget(
    records: &BTreeMap<String, JsonObject>,
    journal: &[MetadataEvent],
    changes: &[(String, JsonObject)],
    event: &MetadataEvent,
) -> Result<()> {
    if journal.len() >= MAX_EVENTS {
        return Err(BitvmError::Capacity);
    }
    let added = changes
        .iter()
        .filter(|(key, _)| !records.contains_key(key))
        .count();
    if records.len() + added > MAX_RECORDS {
        return Err(BitvmError::Capacity);
    }
    let mut bytes = 0usize;
    let mut nodes = 0usize;
    fn count(o: &JsonObject, b: &mut usize, n: &mut usize) -> Result<()> {
        *b = b
            .checked_add(o.to_json().map_err(|_| BitvmError::Capacity)?.len())
            .ok_or(BitvmError::Capacity)?;
        object_nodes(o, n)?;
        if *b > MAX_STATE_BYTES {
            return Err(BitvmError::Capacity);
        }
        Ok(())
    }
    for (key, doc) in records {
        if !changes.iter().any(|(k, _)| k == key) {
            if key.len() > 2048 {
                return Err(BitvmError::Capacity);
            }
            bytes = bytes
                .checked_add(key.len() + 8)
                .ok_or(BitvmError::Capacity)?;
            count(doc, &mut bytes, &mut nodes)?;
        }
    }
    for (key, doc) in changes {
        if key.len() > 2048 {
            return Err(BitvmError::Capacity);
        }
        bytes = bytes
            .checked_add(key.len() + 8)
            .ok_or(BitvmError::Capacity)?;
        count(doc, &mut bytes, &mut nodes)?;
    }
    for entry in journal.iter().chain(core::iter::once(event)) {
        if entry.key.len() > 1024 || entry.operation.len() > 64 {
            return Err(BitvmError::Capacity);
        }
        millis(entry.at_ms)?;
        count(&entry.input, &mut bytes, &mut nodes)?;
        count(&entry.context, &mut bytes, &mut nodes)?;
        bytes = bytes
            .checked_add(entry.key.len() + entry.operation.len() + 32)
            .ok_or(BitvmError::Capacity)?;
    }
    if bytes > MAX_STATE_BYTES - 40_000 {
        return Err(BitvmError::Capacity);
    }
    Ok(())
}
pub(crate) fn checkpoint(
    magic: &[u8; 5],
    env: &Environment,
    records: &BTreeMap<String, JsonObject>,
    journal: &[MetadataEvent],
) -> Result<Vec<u8>> {
    let mut out = magic.to_vec();
    fn string(out: &mut Vec<u8>, s: &str) -> Result<()> {
        let n = u32::try_from(s.len()).map_err(|_| BitvmError::Capacity)?;
        if out
            .len()
            .checked_add(4 + s.len())
            .is_none_or(|n| n > MAX_STATE_BYTES)
        {
            return Err(BitvmError::Capacity);
        }
        out.extend(n.to_be_bytes());
        out.extend(s.as_bytes());
        if out.len() > MAX_STATE_BYTES {
            return Err(BitvmError::Capacity);
        }
        Ok(())
    }
    out.extend((env.len() as u32).to_be_bytes());
    for (k, v) in env {
        string(&mut out, k)?;
        string(&mut out, v)?;
    }
    out.extend((journal.len() as u32).to_be_bytes());
    for e in journal {
        string(&mut out, &e.operation)?;
        string(&mut out, &e.key)?;
        string(
            &mut out,
            &e.input.to_json().map_err(|_| BitvmError::Capacity)?,
        )?;
        string(
            &mut out,
            &e.context.to_json().map_err(|_| BitvmError::Capacity)?,
        )?;
        out.extend(e.at_ms.to_be_bytes());
    }
    out.extend((records.len() as u32).to_be_bytes());
    for (k, v) in records {
        string(&mut out, k)?;
        string(&mut out, &v.to_json().map_err(|_| BitvmError::Capacity)?)?;
    }
    if out.len() > MAX_STATE_BYTES {
        return Err(BitvmError::Capacity);
    }
    with_crc(out).map_err(|_| BitvmError::Capacity)
}
pub(crate) fn read_checkpoint(
    bytes: &[u8],
    magic: &[u8; 5],
    expected_env: &Environment,
) -> Result<(Vec<MetadataEvent>, BTreeMap<String, JsonObject>)> {
    validate_env(expected_env)?;
    if bytes.len() > MAX_STATE_BYTES + 4 {
        return Err(BitvmError::Capacity);
    }
    let payload = verify_crc(bytes).map_err(|_| BitvmError::InvalidCheckpoint)?;
    let mut c = Cursor::new(payload);
    if c.take(5).map_err(|_| BitvmError::InvalidCheckpoint)? != magic {
        return Err(BitvmError::InvalidCheckpoint);
    }
    fn string(c: &mut Cursor<'_>, limit: usize) -> Result<String> {
        let n = c.u32().map_err(|_| BitvmError::InvalidCheckpoint)? as usize;
        if n > limit {
            return Err(BitvmError::Capacity);
        }
        String::from_utf8(
            c.take(n)
                .map_err(|_| BitvmError::InvalidCheckpoint)?
                .to_vec(),
        )
        .map_err(|_| BitvmError::InvalidCheckpoint)
    }
    let n = c.u32().map_err(|_| BitvmError::InvalidCheckpoint)? as usize;
    if n > 32 {
        return Err(BitvmError::Capacity);
    }
    let mut env = Environment::new();
    let mut prior = String::new();
    for i in 0..n {
        let k = string(&mut c, 128)?;
        if i > 0 && k <= prior {
            return Err(BitvmError::InvalidCheckpoint);
        }
        prior = k.clone();
        let v = string(&mut c, 1024)?;
        env.insert(k, v);
    }
    if &env != expected_env {
        return Err(BitvmError::ConfigMismatch);
    }
    let n = c.u32().map_err(|_| BitvmError::InvalidCheckpoint)? as usize;
    if n > MAX_EVENTS {
        return Err(BitvmError::Capacity);
    }
    let mut events = Vec::new();
    let mut aggregate_nodes = 0;
    for _ in 0..n {
        let operation = string(&mut c, 64)?;
        let key = string(&mut c, 1024)?;
        let a = string(&mut c, 65_536)?;
        let b = string(&mut c, 65_536)?;
        let input = JsonObject::parse(&a).map_err(|_| BitvmError::InvalidCheckpoint)?;
        let context = JsonObject::parse(&b).map_err(|_| BitvmError::InvalidCheckpoint)?;
        if input.to_json().map_err(|_| BitvmError::InvalidCheckpoint)? != a
            || context
                .to_json()
                .map_err(|_| BitvmError::InvalidCheckpoint)?
                != b
        {
            return Err(BitvmError::InvalidCheckpoint);
        }
        object_nodes(&input, &mut aggregate_nodes)?;
        object_nodes(&context, &mut aggregate_nodes)?;
        let at_ms = c.u64().map_err(|_| BitvmError::InvalidCheckpoint)?;
        millis(at_ms)?;
        events.push(MetadataEvent {
            operation,
            key,
            input,
            context,
            at_ms,
        });
    }
    let n = c.u32().map_err(|_| BitvmError::InvalidCheckpoint)? as usize;
    if n > MAX_RECORDS {
        return Err(BitvmError::Capacity);
    }
    let mut records = BTreeMap::new();
    let mut prior = String::new();
    for i in 0..n {
        let k = string(&mut c, 2048)?;
        if i > 0 && k <= prior {
            return Err(BitvmError::InvalidCheckpoint);
        }
        prior = k.clone();
        let raw = string(&mut c, 65_536)?;
        let o = JsonObject::parse(&raw).map_err(|_| BitvmError::InvalidCheckpoint)?;
        if o.to_json().map_err(|_| BitvmError::InvalidCheckpoint)? != raw {
            return Err(BitvmError::InvalidCheckpoint);
        }
        object_nodes(&o, &mut aggregate_nodes)?;
        records.insert(k, o);
    }
    c.end().map_err(|_| BitvmError::InvalidCheckpoint)?;
    Ok((events, records))
}
pub(crate) fn make_object<const N: usize>(fields: [(&str, JsonValue); N]) -> Result<JsonObject> {
    let mut out = JsonObject::new();
    for (k, v) in fields {
        put(&mut out, k, v)?;
    }
    Ok(out)
}
pub(crate) fn event(
    operation: &str,
    key: &str,
    input: &JsonObject,
    context: &JsonObject,
    at_ms: u64,
    state: (&BTreeMap<String, JsonObject>, &[MetadataEvent]),
) -> Result<MetadataEvent> {
    let (records, journal) = state;
    preflight(input)?;
    preflight(context)?;
    millis(at_ms)?;
    if operation.len() > 64 || key.len() > 1024 || journal.len() >= MAX_EVENTS {
        return Err(BitvmError::Capacity);
    }
    // Aggregate borrowed requests before creating journal copies.
    let mut bytes = input.to_json().map_err(|_| BitvmError::Capacity)?.len()
        + context.to_json().map_err(|_| BitvmError::Capacity)?.len();
    let mut nodes = 0;
    object_nodes(input, &mut nodes)?;
    object_nodes(context, &mut nodes)?;
    for doc in records.values() {
        object_nodes(doc, &mut nodes)?;
        bytes = bytes
            .checked_add(doc.to_json().map_err(|_| BitvmError::Capacity)?.len())
            .ok_or(BitvmError::Capacity)?;
    }
    for e in journal {
        object_nodes(&e.input, &mut nodes)?;
        object_nodes(&e.context, &mut nodes)?;
        bytes = bytes
            .checked_add(
                e.input.to_json().map_err(|_| BitvmError::Capacity)?.len()
                    + e.context.to_json().map_err(|_| BitvmError::Capacity)?.len()
                    + e.key.len()
                    + e.operation.len()
                    + 32,
            )
            .ok_or(BitvmError::Capacity)?;
    }
    if bytes > MAX_STATE_BYTES {
        return Err(BitvmError::Capacity);
    }
    Ok(MetadataEvent {
        operation: operation.into(),
        key: key.into(),
        input: input.clone(),
        context: context.clone(),
        at_ms,
    })
}

pub(crate) fn number_text(value: f64) -> Result<String> {
    if value.is_nan() {
        Ok("NaN".into())
    } else if value == f64::INFINITY {
        Ok("Infinity".into())
    } else if value == f64::NEG_INFINITY {
        Ok("-Infinity".into())
    } else {
        crate::state_oracle::source_number_text(value).map_err(|_| BitvmError::NonFinite)
    }
}

fn object_nodes(o: &JsonObject, nodes: &mut usize) -> Result<()> {
    fn value(v: &JsonValue, nodes: &mut usize) -> Result<()> {
        *nodes = nodes.checked_add(1).ok_or(BitvmError::Capacity)?;
        if *nodes > MAX_STATE_NODES {
            return Err(BitvmError::Capacity);
        }
        match v {
            JsonValue::Array(values) => {
                for v in values {
                    value(v, nodes)?;
                }
            }
            JsonValue::Object(o) => {
                for (_, v) in o.iter() {
                    value(v, nodes)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    *nodes = nodes.checked_add(1).ok_or(BitvmError::Capacity)?;
    if *nodes > MAX_STATE_NODES {
        return Err(BitvmError::Capacity);
    }
    for (_, v) in o.iter() {
        value(v, nodes)?;
    }
    Ok(())
}
