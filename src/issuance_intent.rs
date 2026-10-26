//! Bounded issuance-intent metadata, with explicit repair of the missing source
//! database category. A stored intent never issues tokens or proves authority.
use crate::codec::{verify_crc, with_crc, Cursor};
use std::collections::{BTreeMap, BTreeSet};

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "dafabd4d8a51e805cac299b33e815a2367ffb1b934f943c4519454a671aec932";
pub const MAX_INTENTS: usize = 1024;
pub const MAX_INTENT_WRITES: usize = 4096;
pub const MAX_JSON_BYTES: usize = 65_536;
pub const MAX_JSON_NODES: usize = 4096;
pub const MAX_JSON_DEPTH: usize = 16;
pub const MAX_SELECTOR_BYTES: usize = 1024;
pub const MAX_STORE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_STORE_NODES: usize = 65_536;
pub const MAX_SAFE_MILLIS: u64 = 9_007_199_254_740_991;
const MAGIC: &[u8; 5] = b"TLII\x01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentError {
    InvalidJson,
    InvalidNumber,
    UnsupportedSurrogate,
    MissingSelector,
    SelectorLimit,
    InvalidMillis,
    DepthLimit,
    Capacity,
    DuplicateId,
    ImmutableId,
    InvalidCheckpoint,
}
impl core::fmt::Display for IntentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IntentError {}
type Result<T> = core::result::Result<T, IntentError>;

/// Finite binary64, including JSON's rounded/underflowed number semantics.
/// Negative zero is normalized as JSON.stringify does. No NaN can be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonNumber(u64);
impl JsonNumber {
    pub fn from_f64(value: f64) -> Result<Self> {
        if !value.is_finite() {
            return Err(IntentError::InvalidNumber);
        }
        Ok(Self(if value == 0.0 { 0 } else { value.to_bits() }))
    }
    pub fn as_f64(self) -> f64 {
        f64::from_bits(self.0)
    }
    fn text(self) -> Result<String> {
        crate::state_oracle::source_number_text(self.as_f64())
            .map_err(|_| IntentError::InvalidNumber)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<JsonValue>),
    Object(JsonObject),
}
impl From<&str> for JsonValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
impl JsonValue {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_JSON_BYTES {
            return Err(IntentError::Capacity);
        }
        let mut parser = JsonParser {
            text,
            at: 0,
            nodes: 0,
        };
        let value = parser.value(0)?;
        parser.space();
        if parser.at != text.len() {
            return Err(IntentError::InvalidJson);
        }
        value.stats()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<String> {
        self.stats()?;
        let mut output = String::new();
        write_json(self, &mut output)?;
        Ok(output)
    }
    fn stats(&self) -> Result<Stats> {
        let mut stats = Stats::default();
        measure(self, 0, &mut stats)?;
        Ok(stats)
    }
}

/// Own enumerable JSON fields. Integer-index keys enumerate numerically first;
/// overwriting a key retains its original slot among other string keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonObject(Vec<(String, JsonValue)>);
impl JsonObject {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn parse(text: &str) -> Result<Self> {
        match JsonValue::parse(text)? {
            JsonValue::Object(object) => Ok(object),
            _ => Err(IntentError::InvalidJson),
        }
    }
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &JsonValue)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }
    pub fn insert(&mut self, key: String, value: JsonValue) -> Result<()> {
        let mut stats = Stats { nodes: 1, bytes: 2 };
        for (name, existing) in &self.0 {
            if name != &key {
                measure_field(name, existing, 1, &mut stats)?;
            }
        }
        measure_field(&key, &value, 1, &mut stats)?;
        self.insert_unchecked(key, value);
        Ok(())
    }
    pub fn to_json(&self) -> Result<String> {
        object_stats(self)?;
        let mut output = String::new();
        write_object(self, &mut output)?;
        Ok(output)
    }
    fn insert_unchecked(&mut self, key: String, value: JsonValue) {
        if let Some((_, slot)) = self.0.iter_mut().find(|(name, _)| name == &key) {
            *slot = value;
        } else if let Some(index) = array_index(&key) {
            let at = self
                .0
                .iter()
                .position(|(name, _)| array_index(name).is_none_or(|old| old > index))
                .unwrap_or(self.0.len());
            self.0.insert(at, (key, value));
        } else {
            self.0.push((key, value));
        }
    }
}
fn array_index(key: &str) -> Option<u32> {
    let value: u32 = key.parse().ok()?;
    (value != u32::MAX && value.to_string() == key).then_some(value)
}

#[derive(Debug, Clone, Copy, Default)]
struct Stats {
    nodes: usize,
    bytes: usize,
}
impl Stats {
    fn add(&mut self, nodes: usize, bytes: usize) -> Result<()> {
        self.nodes = self.nodes.checked_add(nodes).ok_or(IntentError::Capacity)?;
        self.bytes = self.bytes.checked_add(bytes).ok_or(IntentError::Capacity)?;
        if self.nodes > MAX_JSON_NODES || self.bytes > MAX_JSON_BYTES {
            return Err(IntentError::Capacity);
        }
        Ok(())
    }
}
fn quoted_bytes(value: &str) -> usize {
    2 + value
        .chars()
        .map(|ch| match ch {
            '"' | '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            ch if ch < '\u{20}' => 6,
            ch => ch.len_utf8(),
        })
        .sum::<usize>()
}
fn measure_field(key: &str, value: &JsonValue, depth: usize, stats: &mut Stats) -> Result<()> {
    if key.len() > MAX_JSON_BYTES {
        return Err(IntentError::Capacity);
    }
    // A comma is conservatively counted even for the first field. This gives
    // the same documented one-byte headroom for construction and decoding.
    stats.add(0, quoted_bytes(key) + 2)?;
    measure(value, depth, stats)
}
fn object_stats(object: &JsonObject) -> Result<Stats> {
    let mut stats = Stats::default();
    measure_object(object, 0, &mut stats)?;
    Ok(stats)
}
fn measure_object(object: &JsonObject, depth: usize, stats: &mut Stats) -> Result<()> {
    if depth > MAX_JSON_DEPTH {
        return Err(IntentError::DepthLimit);
    }
    stats.add(1, 2)?;
    for (key, value) in &object.0 {
        measure_field(key, value, depth + 1, stats)?;
    }
    Ok(())
}
fn measure(value: &JsonValue, depth: usize, stats: &mut Stats) -> Result<()> {
    if depth > MAX_JSON_DEPTH {
        return Err(IntentError::DepthLimit);
    }
    match value {
        JsonValue::Object(object) => measure_object(object, depth, stats),
        JsonValue::Array(values) => {
            stats.add(1, 2)?;
            for value in values {
                stats.add(0, 1)?;
                measure(value, depth + 1, stats)?;
            }
            Ok(())
        }
        JsonValue::Null => stats.add(1, 4),
        JsonValue::Bool(value) => stats.add(1, if *value { 4 } else { 5 }),
        JsonValue::Number(value) => stats.add(1, value.text()?.len()),
        JsonValue::String(value) => {
            if value.len() > MAX_JSON_BYTES {
                return Err(IntentError::Capacity);
            }
            stats.add(1, quoted_bytes(value))
        }
    }
}
fn write_string(value: &str, output: &mut String) {
    use core::fmt::Write;
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{8}' => output.push_str("\\b"),
            '\u{c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch < '\u{20}' => write!(output, "\\u{:04x}", ch as u32).expect("String writer"),
            ch => output.push(ch),
        }
    }
    output.push('"');
}
fn write_object(object: &JsonObject, output: &mut String) -> Result<()> {
    output.push('{');
    for (index, (key, value)) in object.0.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        write_string(key, output);
        output.push(':');
        write_json(value, output)?;
    }
    output.push('}');
    Ok(())
}
fn write_json(value: &JsonValue, output: &mut String) -> Result<()> {
    match value {
        JsonValue::Null => output.push_str("null"),
        JsonValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        JsonValue::Number(value) => output.push_str(&value.text()?),
        JsonValue::String(value) => write_string(value, output),
        JsonValue::Object(object) => write_object(object, output)?,
        JsonValue::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_json(value, output)?;
            }
            output.push(']');
        }
    }
    Ok(())
}

struct JsonParser<'a> {
    text: &'a str,
    at: usize,
    nodes: usize,
}
impl JsonParser<'_> {
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> bool {
        if self.text.as_bytes().get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self, depth: usize) -> Result<JsonValue> {
        if depth > MAX_JSON_DEPTH {
            return Err(IntentError::DepthLimit);
        }
        self.nodes += 1;
        if self.nodes > MAX_JSON_NODES {
            return Err(IntentError::Capacity);
        }
        self.space();
        let first = *self
            .text
            .as_bytes()
            .get(self.at)
            .ok_or(IntentError::InvalidJson)?;
        match first {
            b'n' | b't' | b'f' => {
                let (word, value) = match first {
                    b'n' => ("null", JsonValue::Null),
                    b't' => ("true", JsonValue::Bool(true)),
                    _ => ("false", JsonValue::Bool(false)),
                };
                if !self.text[self.at..].starts_with(word) {
                    return Err(IntentError::InvalidJson);
                }
                self.at += word.len();
                Ok(value)
            }
            b'"' => self.string().map(JsonValue::String),
            b'[' => {
                self.at += 1;
                self.space();
                let mut values = Vec::new();
                if self.eat(b']') {
                    return Ok(JsonValue::Array(values));
                }
                loop {
                    values.push(self.value(depth + 1)?);
                    self.space();
                    if self.eat(b']') {
                        break;
                    }
                    if !self.eat(b',') {
                        return Err(IntentError::InvalidJson);
                    }
                }
                Ok(JsonValue::Array(values))
            }
            b'{' => {
                self.at += 1;
                self.space();
                let mut object = JsonObject::new();
                if self.eat(b'}') {
                    return Ok(JsonValue::Object(object));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    if !self.eat(b':') {
                        return Err(IntentError::InvalidJson);
                    }
                    let value = self.value(depth + 1)?;
                    object.insert_unchecked(key, value);
                    self.space();
                    if self.eat(b'}') {
                        break;
                    }
                    if !self.eat(b',') {
                        return Err(IntentError::InvalidJson);
                    }
                }
                Ok(JsonValue::Object(object))
            }
            b'-' | b'0'..=b'9' => self.number().map(JsonValue::Number),
            _ => Err(IntentError::InvalidJson),
        }
    }
    fn number(&mut self) -> Result<JsonNumber> {
        let start = self.at;
        self.eat(b'-');
        if !self.eat(b'0') {
            if !self
                .text
                .as_bytes()
                .get(self.at)
                .is_some_and(|b| matches!(b, b'1'..=b'9'))
            {
                return Err(IntentError::InvalidJson);
            }
            self.digits();
        }
        if self.eat(b'.') {
            let old = self.at;
            self.digits();
            if old == self.at {
                return Err(IntentError::InvalidJson);
            }
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            let old = self.at;
            self.digits();
            if old == self.at {
                return Err(IntentError::InvalidJson);
            }
        }
        JsonNumber::from_f64(
            self.text[start..self.at]
                .parse()
                .map_err(|_| IntentError::InvalidNumber)?,
        )
    }
    fn digits(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(u8::is_ascii_digit)
        {
            self.at += 1;
        }
    }
    fn hex4(&mut self) -> Result<u16> {
        let mut value = 0;
        for _ in 0..4 {
            let byte = *self
                .text
                .as_bytes()
                .get(self.at)
                .ok_or(IntentError::InvalidJson)?;
            self.at += 1;
            value = value * 16
                + match byte {
                    b'0'..=b'9' => u16::from(byte - b'0'),
                    b'a'..=b'f' => u16::from(byte - b'a' + 10),
                    b'A'..=b'F' => u16::from(byte - b'A' + 10),
                    _ => return Err(IntentError::InvalidJson),
                };
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String> {
        if !self.eat(b'"') {
            return Err(IntentError::InvalidJson);
        }
        let mut value = String::new();
        loop {
            let byte = *self
                .text
                .as_bytes()
                .get(self.at)
                .ok_or(IntentError::InvalidJson)?;
            self.at += 1;
            match byte {
                b'"' => return Ok(value),
                b'\\' => {
                    let escaped = *self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .ok_or(IntentError::InvalidJson)?;
                    self.at += 1;
                    match escaped {
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        b'/' => value.push('/'),
                        b'b' => value.push('\u{8}'),
                        b'f' => value.push('\u{c}'),
                        b'n' => value.push('\n'),
                        b'r' => value.push('\r'),
                        b't' => value.push('\t'),
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xd800..=0xdbff).contains(&high) {
                                if !self.eat(b'\\') || !self.eat(b'u') {
                                    return Err(IntentError::UnsupportedSurrogate);
                                }
                                let low = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&low) {
                                    return Err(IntentError::UnsupportedSurrogate);
                                }
                                0x10000 + ((u32::from(high) - 0xd800) << 10) + u32::from(low)
                                    - 0xdc00
                            } else if (0xdc00..=0xdfff).contains(&high) {
                                return Err(IntentError::UnsupportedSurrogate);
                            } else {
                                u32::from(high)
                            };
                            value.push(char::from_u32(code).ok_or(IntentError::InvalidJson)?);
                        }
                        _ => return Err(IntentError::InvalidJson),
                    }
                }
                0..=31 => return Err(IntentError::InvalidJson),
                32..=127 => value.push(char::from(byte)),
                _ => {
                    self.at -= 1;
                    let ch = self.text[self.at..]
                        .chars()
                        .next()
                        .ok_or(IntentError::InvalidJson)?;
                    self.at += ch.len_utf8();
                    value.push(ch);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentWrite {
    pub selector: String,
    pub intent: JsonObject,
    pub updated_at_ms: u64,
}

/// Explicit metadata repair: stable first-insertion row ordering and deep JSON
/// ID identity. NeDB's object-index/array-ID anomalies are not persistence here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssuanceIntentStore {
    documents: Vec<JsonObject>,
    journal: Vec<IntentWrite>,
    addresses: BTreeMap<String, BTreeSet<usize>>,
}
impl IssuanceIntentStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn documents(&self) -> &[JsonObject] {
        &self.documents
    }
    pub fn journal(&self) -> &[IntentWrite] {
        &self.journal
    }
    pub fn get_intent(&self, selector: &str) -> Option<&JsonObject> {
        self.documents.iter().find(
            |document| matches!(document.get("_id"), Some(JsonValue::String(id)) if id == selector),
        )
    }
    pub fn get_intents_by_address(&self, address: &str) -> Vec<&JsonObject> {
        self.addresses
            .get(address)
            .into_iter()
            .flatten()
            .map(|index| &self.documents[*index])
            .collect()
    }
    pub fn record_intent(
        &mut self,
        selector: &str,
        intent: &JsonObject,
        updated_at_ms: u64,
    ) -> Result<&JsonObject> {
        if selector.is_empty() {
            return Err(IntentError::MissingSelector);
        }
        if selector.len() > MAX_SELECTOR_BYTES {
            return Err(IntentError::SelectorLimit);
        }
        if updated_at_ms > MAX_SAFE_MILLIS {
            return Err(IntentError::InvalidMillis);
        }
        let input_stats = object_stats(intent)?;
        let default_id = JsonValue::String(selector.to_owned());
        let timestamp = JsonValue::Number(JsonNumber::from_f64(updated_at_ms as f64)?);
        let stored_id = intent.get("_id").unwrap_or(&default_id);
        let mut output_stats = Stats { nodes: 1, bytes: 2 };
        measure_field("_id", stored_id, 1, &mut output_stats)?;
        for (key, value) in intent.iter() {
            if key != "_id" && key != "updatedAt" {
                measure_field(key, value, 1, &mut output_stats)?;
            }
        }
        measure_field("updatedAt", &timestamp, 1, &mut output_stats)?;
        let selected = self
            .documents
            .iter()
            .position(|document| document.get("_id") == Some(&default_id));
        if selected.is_some() && stored_id != &default_id {
            return Err(IntentError::ImmutableId);
        }
        if selected.is_none()
            && self
                .documents
                .iter()
                .any(|document| document.get("_id") == Some(stored_id))
        {
            return Err(IntentError::DuplicateId);
        }
        if self.journal.len() >= MAX_INTENT_WRITES
            || selected.is_none() && self.documents.len() >= MAX_INTENTS
        {
            return Err(IntentError::Capacity);
        }
        // Bound both retained journal input and resulting documents BEFORE any
        // object clone. Overwritten input fields still consume journal budget.
        let mut bytes = selector
            .len()
            .checked_add(16)
            .and_then(|value| value.checked_add(input_stats.bytes))
            .and_then(|value| value.checked_add(output_stats.bytes))
            .ok_or(IntentError::Capacity)?;
        let mut nodes = input_stats
            .nodes
            .checked_add(output_stats.nodes)
            .ok_or(IntentError::Capacity)?;
        for (index, document) in self.documents.iter().enumerate() {
            if selected != Some(index) {
                let stats = object_stats(document)?;
                bytes = bytes
                    .checked_add(stats.bytes)
                    .ok_or(IntentError::Capacity)?;
                nodes = nodes
                    .checked_add(stats.nodes)
                    .ok_or(IntentError::Capacity)?;
            }
        }
        for event in &self.journal {
            let stats = object_stats(&event.intent)?;
            bytes = bytes
                .checked_add(stats.bytes)
                .and_then(|value| value.checked_add(event.selector.len() + 16))
                .ok_or(IntentError::Capacity)?;
            nodes = nodes
                .checked_add(stats.nodes)
                .ok_or(IntentError::Capacity)?;
        }
        // Include checkpoint framing/row lengths as well as conservative JSON
        // sizes, so an accepted store always fits its own checkpoint cap.
        bytes = bytes
            .checked_add(32 + 4 * (self.documents.len() + 1))
            .ok_or(IntentError::Capacity)?;
        if bytes > MAX_STORE_BYTES || nodes > MAX_STORE_NODES {
            return Err(IntentError::Capacity);
        }
        let mut document = JsonObject::new();
        document.insert_unchecked("_id".into(), default_id);
        for (key, value) in intent.iter() {
            document.insert_unchecked(key.to_owned(), value.clone());
        }
        document.insert_unchecked("updatedAt".into(), timestamp);
        let event = IntentWrite {
            selector: selector.to_owned(),
            intent: intent.clone(),
            updated_at_ms,
        };
        let index = selected.unwrap_or(self.documents.len());
        if selected.is_some() {
            self.documents[index] = document;
        } else {
            self.documents.push(document);
        }
        self.journal.push(event);
        self.rebuild_addresses();
        Ok(&self.documents[index])
    }
    fn rebuild_addresses(&mut self) {
        fn visit(
            value: &JsonValue,
            index: usize,
            addresses: &mut BTreeMap<String, BTreeSet<usize>>,
        ) {
            match value {
                JsonValue::String(address) => {
                    addresses.entry(address.clone()).or_default().insert(index);
                }
                JsonValue::Array(values) => {
                    for value in values {
                        visit(value, index, addresses);
                    }
                }
                _ => {}
            }
        }
        let mut addresses: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (index, document) in self.documents.iter().enumerate() {
            if let Some(value) = document.get("recipientAddress") {
                visit(value, index, &mut addresses);
            }
        }
        self.addresses = addresses;
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = MAGIC.to_vec();
        put_count(&mut bytes, self.journal.len())?;
        for event in &self.journal {
            put_text(&mut bytes, &event.selector)?;
            bytes.extend_from_slice(&event.updated_at_ms.to_be_bytes());
            put_text(&mut bytes, &event.intent.to_json()?)?;
        }
        put_count(&mut bytes, self.documents.len())?;
        for document in &self.documents {
            put_text(&mut bytes, &document.to_json()?)?;
        }
        if bytes.len() + 4 > MAX_STORE_BYTES {
            return Err(IntentError::Capacity);
        }
        with_crc(bytes).map_err(|_| IntentError::InvalidCheckpoint)
    }
    pub fn from_checkpoint_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_STORE_BYTES {
            return Err(IntentError::Capacity);
        }
        let payload = verify_crc(bytes).map_err(|_| IntentError::InvalidCheckpoint)?;
        let mut reader = Cursor::new(payload);
        if reader.take(5).map_err(|_| IntentError::InvalidCheckpoint)? != MAGIC {
            return Err(IntentError::InvalidCheckpoint);
        }
        let count = read_count(&mut reader, MAX_INTENT_WRITES)?;
        let mut store = Self::new();
        for _ in 0..count {
            let selector = read_text(&mut reader, MAX_SELECTOR_BYTES)?;
            let millis = reader.u64().map_err(|_| IntentError::InvalidCheckpoint)?;
            let intent = read_object(&mut reader)?;
            store
                .record_intent(&selector, &intent, millis)
                .map_err(|_| IntentError::InvalidCheckpoint)?;
        }
        let count = read_count(&mut reader, MAX_INTENTS)?;
        if count != store.documents.len() {
            return Err(IntentError::InvalidCheckpoint);
        }
        for expected in &store.documents {
            if &read_object(&mut reader)? != expected {
                return Err(IntentError::InvalidCheckpoint);
            }
        }
        reader.end().map_err(|_| IntentError::InvalidCheckpoint)?;
        if store.checkpoint_bytes()? != bytes {
            return Err(IntentError::InvalidCheckpoint);
        }
        Ok(store)
    }
}
fn put_count(bytes: &mut Vec<u8>, count: usize) -> Result<()> {
    bytes.extend_from_slice(
        &u32::try_from(count)
            .map_err(|_| IntentError::Capacity)?
            .to_be_bytes(),
    );
    Ok(())
}
fn put_text(bytes: &mut Vec<u8>, text: &str) -> Result<()> {
    if bytes
        .len()
        .checked_add(text.len() + 4)
        .is_none_or(|value| value > MAX_STORE_BYTES - 4)
    {
        return Err(IntentError::Capacity);
    }
    put_count(bytes, text.len())?;
    bytes.extend_from_slice(text.as_bytes());
    Ok(())
}
fn read_count(reader: &mut Cursor<'_>, cap: usize) -> Result<usize> {
    let count = reader.u32().map_err(|_| IntentError::InvalidCheckpoint)? as usize;
    if count > cap {
        Err(IntentError::InvalidCheckpoint)
    } else {
        Ok(count)
    }
}
fn read_text(reader: &mut Cursor<'_>, cap: usize) -> Result<String> {
    let count = read_count(reader, cap)?;
    let text = std::str::from_utf8(
        reader
            .take(count)
            .map_err(|_| IntentError::InvalidCheckpoint)?,
    )
    .map_err(|_| IntentError::InvalidCheckpoint)?;
    Ok(text.to_owned())
}
fn read_object(reader: &mut Cursor<'_>) -> Result<JsonObject> {
    let text = read_text(reader, MAX_JSON_BYTES)?;
    let object = JsonObject::parse(&text).map_err(|_| IntentError::InvalidCheckpoint)?;
    if object
        .to_json()
        .map_err(|_| IntentError::InvalidCheckpoint)?
        != text
    {
        return Err(IntentError::InvalidCheckpoint);
    }
    Ok(object)
}
