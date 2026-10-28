//! Actual pinned orderbook.js queue documents and per-instance book caches.
//! Placement is an explicit capability over the same owner; no Ledger is copied.
include!("contract_channel_queue_source_impl.rs");
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::decimal::Decimal;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98";
pub const MAX_ROWS: usize = 1024;
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
pub const METHODS: [&str; 9] = [
    "_getOrderbookDB",
    "_addActivePair",
    "_updateActivePairs",
    "queueOnChainTokenOrder",
    "queueOnChainContractOrder",
    "processQueuedOnChainOrdersForBlock",
    "getOrderbookInstance",
    "loadOrderBook",
    "saveOrderBook",
];
type Eval<T> = Result<T, Fault>;
#[derive(Debug)]
enum Fault {
    Thrown(SourceException),
    Local(SourceRuleError),
    Boundary(SourceRuleError),
}
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
fn type_error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "TypeError".into(),
        message: message.into(),
    })
}
fn s(text: &str) -> V {
    V::String(text.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn arg(args: &[V], index: usize) -> V {
    args.get(index).cloned().unwrap_or(V::Undefined)
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => {
            let n = f64::from_bits(*b);
            n != 0.0 && !n.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(x), V::Number(y)) => f64::from_bits(*x) == f64::from_bits(*y),
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(x), V::Bool(y)) => x == y,
        (V::String(x), V::String(y)) => x == y,
        _ => false,
    }
}
fn same_key(a: &V, b: &V) -> bool {
    strict(a, b)
        || matches!((a,b),(V::Number(x),V::Number(y))if f64::from_bits(*x).is_nan()&&f64::from_bits(*y).is_nan())
}
fn primitive_key(v: &V) -> Result<(), SourceRuleError> {
    if matches!(v, V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_)) {
        Err(SourceRuleError::InvalidInput)
    } else {
        Ok(())
    }
}
fn text(v: &V) -> Eval<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(s) => s.clone(),
        V::BigNumber(s) => big_number_text(s, false)?,
        V::Promise => "[object Promise]".into(),
        V::Number(b) => {
            let n = f64::from_bits(*b);
            if n.is_nan() {
                "NaN".into()
            } else if n == f64::INFINITY {
                "Infinity".into()
            } else if n == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(n)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
            }
        }
        V::Array(values) => values
            .iter()
            .map(|v| {
                if matches!(v, V::Null | V::Undefined) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<Eval<Vec<_>>>()?
            .join(","),
        V::Object(fields) => {
            if fields.iter().any(|(key, _)| key == "toString") {
                return Err(type_error("Cannot convert object to primitive value"));
            }
            "[object Object]".into()
        }
    })
}
fn radix(text: &str, radix: u32) -> Eval<f64> {
    if text.is_empty() || !text.chars().all(|c| c.is_digit(radix)) {
        return Ok(f64::NAN);
    }
    let text = text.trim_start_matches('0');
    if text.len() > 1024 {
        return Ok(f64::INFINITY);
    }
    let mut number = Decimal::zero();
    for digit in text.chars() {
        number = number
            .checked_mul(&Decimal::from(radix))
            .and_then(|n| n.checked_add(&Decimal::from(digit.to_digit(radix).unwrap_or(0))))
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    }
    Ok(number.to_string().parse().unwrap_or(f64::INFINITY))
}
fn number(v: &V) -> Eval<f64> {
    Ok(match v {
        V::Undefined | V::Promise => f64::NAN,
        V::Null => 0.0,
        V::Bool(b) => f64::from(*b),
        V::Number(b) => f64::from_bits(*b),
        _ => {
            let t = text(v)?;
            let t = t.trim_matches(js_space);
            if t.is_empty() {
                0.0
            } else if matches!(t, "Infinity" | "+Infinity") {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else if let Some(t) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                radix(t, 16)?
            } else if let Some(t) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
                radix(t, 2)?
            } else if let Some(t) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
                radix(t, 8)?
            } else if t
                .bytes()
                .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b))
            {
                t.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
    })
}
fn prop(v: &V, key: &str) -> Eval<V> {
    if matches!(v, V::Undefined | V::Null) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(v)?
        )));
    }
    Ok(match v {
        V::Array(a) if key == "length" => V::number(a.len() as f64),
        V::String(s) if key == "length" => V::number(s.encode_utf16().count() as f64),
        V::Array(a) => key
            .parse::<usize>()
            .ok()
            .filter(|i| i.to_string() == key)
            .and_then(|i| a.get(i))
            .cloned()
            .unwrap_or(V::Undefined),
        // A standalone surrogate cannot be represented by SourceValue's UTF-8
        // String, so callers never receive an invented replacement scalar.
        V::String(s) => {
            if let Some(unit) = key
                .parse::<usize>()
                .ok()
                .filter(|i| i.to_string() == key)
                .and_then(|i| s.encode_utf16().nth(i))
            {
                V::String(String::from_utf16(&[unit]).map_err(|_| SourceRuleError::InvalidInput)?)
            } else {
                V::Undefined
            }
        }
        _ => v.get(key),
    })
}
fn set_prop(v: &mut V, key: &str, value: V) -> Eval<()> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            text(v)?
        )));
    }
    if let V::Object(fields) = v {
        if let Some((_, old)) = fields.iter_mut().find(|(k, _)| k == key) {
            *old = value;
        } else {
            fields.push((key.into(), value));
        }
        Ok(())
    } else {
        Err(type_error(format!(
            "Cannot create property '{key}' on {} '{}'",
            match v {
                V::String(_) => "string",
                V::Number(_) => "number",
                V::Bool(_) => "boolean",
                _ => return Err(SourceRuleError::InvalidInput.into()),
            },
            text(v)?
        )))
    }
}
fn put_u32(out: &mut Vec<u8>, v: usize) -> Result<(), SourceRuleError> {
    out.extend_from_slice(
        &u32::try_from(v)
            .map_err(|_| SourceRuleError::Capacity)?
            .to_be_bytes(),
    );
    Ok(())
}
fn put_string(out: &mut Vec<u8>, s: &str) -> Result<(), SourceRuleError> {
    put_u32(out, s.len())?;
    out.extend_from_slice(s.as_bytes());
    Ok(())
}
fn put_value(out: &mut Vec<u8>, v: &V) -> Result<(), SourceRuleError> {
    match v {
        V::Undefined => out.push(0),
        V::Null => out.push(1),
        V::Bool(false) => out.push(2),
        V::Bool(true) => out.push(3),
        V::Number(b) => {
            if V::number(f64::from_bits(*b)) != *v {
                return Err(SourceRuleError::InvalidInput);
            }
            out.push(4);
            out.extend_from_slice(&b.to_be_bytes());
        }
        V::String(s) => {
            out.push(5);
            put_string(out, s)?;
        }
        V::Array(a) => {
            out.push(6);
            put_u32(out, a.len())?;
            for v in a {
                put_value(out, v)?;
            }
        }
        V::Object(a) => {
            out.push(7);
            put_u32(out, a.len())?;
            for (k, v) in a {
                put_string(out, k)?;
                put_value(out, v)?;
            }
        }
        V::Promise => out.push(8),
        V::BigNumber(s) => {
            out.push(9);
            put_string(out, s)?;
        }
    }
    if out.len() > MAX_STATE_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    Ok(())
}
fn get_u32(c: &mut Cursor<'_>) -> Result<usize, SourceRuleError> {
    c.u32()
        .map(|v| v as usize)
        .map_err(|_| SourceRuleError::InvalidInput)
}
fn get_string(c: &mut Cursor<'_>) -> Result<String, SourceRuleError> {
    let len = get_u32(c)?;
    if len > crate::validity_source::MAX_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    String::from_utf8(
        c.take(len)
            .map_err(|_| SourceRuleError::InvalidInput)?
            .to_vec(),
    )
    .map_err(|_| SourceRuleError::InvalidInput)
}
#[derive(Default)]
struct DecodeValueBudget {
    nodes: usize,
    bytes: usize,
}
fn get_value_string(
    c: &mut Cursor<'_>,
    budget: &mut DecodeValueBudget,
) -> Result<String, SourceRuleError> {
    let len = get_u32(c)?;
    budget.bytes = budget
        .bytes
        .checked_add(len)
        .ok_or(SourceRuleError::Capacity)?;
    if budget.bytes > crate::validity_source::MAX_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    String::from_utf8(
        c.take(len)
            .map_err(|_| SourceRuleError::InvalidInput)?
            .to_vec(),
    )
    .map_err(|_| SourceRuleError::InvalidInput)
}
fn get_value(c: &mut Cursor<'_>, depth: usize) -> Result<V, SourceRuleError> {
    let value = get_value_budget(c, depth, &mut DecodeValueBudget::default())?;
    value.validate()?;
    Ok(value)
}
fn get_value_budget(
    c: &mut Cursor<'_>,
    depth: usize,
    budget: &mut DecodeValueBudget,
) -> Result<V, SourceRuleError> {
    budget.nodes = budget
        .nodes
        .checked_add(1)
        .ok_or(SourceRuleError::Capacity)?;
    if budget.nodes > crate::validity_source::MAX_NODES {
        return Err(SourceRuleError::Capacity);
    }
    if depth > crate::validity_source::MAX_DEPTH {
        return Err(SourceRuleError::Capacity);
    }
    let v = match c.u8().map_err(|_| SourceRuleError::InvalidInput)? {
        0 => V::Undefined,
        1 => V::Null,
        2 => V::Bool(false),
        3 => V::Bool(true),
        4 => V::Number(c.u64().map_err(|_| SourceRuleError::InvalidInput)?),
        5 => V::String(get_value_string(c, budget)?),
        6 => {
            let n = get_u32(c)?;
            if n > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            let mut a = vec![];
            for _ in 0..n {
                a.push(get_value_budget(c, depth + 1, budget)?);
            }
            V::Array(a)
        }
        7 => {
            let n = get_u32(c)?;
            if n > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            let mut a = vec![];
            for _ in 0..n {
                let k = get_value_string(c, budget)?;
                let v = get_value_budget(c, depth + 1, budget)?;
                a.push((k, v));
            }
            V::Object(a)
        }
        8 => V::Promise,
        9 => V::BigNumber(get_value_string(c, budget)?),
        _ => return Err(SourceRuleError::InvalidInput),
    };
    Ok(v)
}
fn put_rows(out: &mut Vec<u8>, rows: &[V]) -> Result<(), SourceRuleError> {
    put_u32(out, rows.len())?;
    for row in rows {
        put_value(out, row)?;
    }
    Ok(())
}
fn get_rows(c: &mut Cursor<'_>) -> Result<Vec<V>, SourceRuleError> {
    let n = get_u32(c)?;
    if n > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    let mut rows = vec![];
    for _ in 0..n {
        rows.push(get_value(c, 0)?);
    }
    Ok(rows)
}

fn value_size(value: &V) -> usize {
    64usize.saturating_add(match value {
        V::String(s) | V::BigNumber(s) => s.len(),
        V::Array(a) => a.iter().map(value_size).fold(0usize, usize::saturating_add),
        V::Object(o) => o
            .iter()
            .map(|(k, v)| k.len().saturating_add(value_size(v)))
            .fold(0usize, usize::saturating_add),
        _ => 0,
    })
}
fn validate_rows(rows: &[V]) -> Result<(), SourceRuleError> {
    if rows.len() > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    for (i, row) in rows.iter().enumerate() {
        row.validate()?;
        if !matches!(row, V::Object(_)) {
            return Err(SourceRuleError::InvalidInput);
        }
        primitive_key(&row.get("_id"))?;
        if rows[..i]
            .iter()
            .any(|v| same_key(&v.get("_id"), &row.get("_id")))
        {
            return Err(SourceRuleError::InvalidInput);
        }
    }
    Ok(())
}
fn set_object(object: &mut V, key: &str, value: V) -> Eval<()> {
    if key == "__proto__" {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    set_prop(object, key, value)?;
    if let V::Object(fields) = object {
        order_integer_fields(fields);
    }
    Ok(())
}
fn order_integer_fields(fields: &mut [(String, V)]) {
    let index = |s: &str| {
        s.parse::<u32>()
            .ok()
            .filter(|n| *n < u32::MAX && n.to_string() == s)
    };
    fields.sort_by(|(a, _), (b, _)| match (index(a), index(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => core::cmp::Ordering::Less,
        (None, Some(_)) => core::cmp::Ordering::Greater,
        _ => core::cmp::Ordering::Equal,
    });
}

// The issuance JSON representation deliberately normalizes -0 and excludes
// nonfinite numbers. JSON.parse has different Number semantics: both -0 and
// overflow to Infinity are valid results. Keep those bits in the source owner.
struct BookJsonParser<'a> {
    text: &'a str,
    at: usize,
    nodes: usize,
}
pub(crate) fn parse_book_json_unverified(
    text: &str,
) -> Result<V, crate::issuance_intent::IntentError> {
    BookJsonParser::parse(text)
}
impl<'a> BookJsonParser<'a> {
    fn parse(text: &'a str) -> Result<V, crate::issuance_intent::IntentError> {
        use crate::issuance_intent::{IntentError, MAX_JSON_BYTES};
        if text.len() > MAX_JSON_BYTES {
            return Err(IntentError::Capacity);
        }
        let mut parser = Self {
            text,
            at: 0,
            nodes: 0,
        };
        let value = parser.value(0)?;
        parser.space();
        if parser.at != text.len() {
            return Err(IntentError::InvalidJson);
        }
        Ok(value)
    }
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
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
    fn string(&mut self) -> Result<String, crate::issuance_intent::IntentError> {
        use crate::issuance_intent::{IntentError, JsonValue};
        let start = self.at;
        if !self.eat(b'"') {
            return Err(IntentError::InvalidJson);
        }
        loop {
            match self.text.as_bytes().get(self.at) {
                None => return Err(IntentError::InvalidJson),
                Some(b'"') => {
                    self.at += 1;
                    return match JsonValue::parse(&self.text[start..self.at])? {
                        JsonValue::String(value) => Ok(value),
                        _ => Err(IntentError::InvalidJson),
                    };
                }
                Some(b'\\') => {
                    self.at += 1;
                    if self.at == self.text.len() {
                        return Err(IntentError::InvalidJson);
                    }
                    self.at += 1;
                }
                Some(_) => self.at += 1,
            }
        }
    }
    fn number(&mut self) -> Result<V, crate::issuance_intent::IntentError> {
        use crate::issuance_intent::IntentError;
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
        self.text[start..self.at]
            .parse::<f64>()
            .map(V::number)
            .map_err(|_| IntentError::InvalidNumber)
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
    fn value(&mut self, depth: usize) -> Result<V, crate::issuance_intent::IntentError> {
        use crate::issuance_intent::{IntentError, MAX_JSON_DEPTH, MAX_JSON_NODES};
        if depth > MAX_JSON_DEPTH {
            return Err(IntentError::DepthLimit);
        }
        self.nodes += 1;
        if self.nodes > MAX_JSON_NODES {
            return Err(IntentError::Capacity);
        }
        self.space();
        match self.text.as_bytes().get(self.at).copied() {
            Some(b'n' | b't' | b'f') => {
                let (word, value) = match self.text.as_bytes()[self.at] {
                    b'n' => ("null", V::Null),
                    b't' => ("true", V::Bool(true)),
                    _ => ("false", V::Bool(false)),
                };
                if !self.text[self.at..].starts_with(word) {
                    return Err(IntentError::InvalidJson);
                }
                self.at += word.len();
                Ok(value)
            }
            Some(b'"') => self.string().map(V::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'[') => {
                self.at += 1;
                self.space();
                let mut values = Vec::new();
                if self.eat(b']') {
                    return Ok(V::Array(values));
                }
                loop {
                    if values.len() >= MAX_ROWS {
                        return Err(IntentError::Capacity);
                    }
                    values.push(self.value(depth + 1)?);
                    self.space();
                    if self.eat(b']') {
                        break;
                    }
                    if !self.eat(b',') {
                        return Err(IntentError::InvalidJson);
                    }
                }
                Ok(V::Array(values))
            }
            Some(b'{') => {
                self.at += 1;
                self.space();
                let mut fields: Vec<(String, V)> = Vec::new();
                if self.eat(b'}') {
                    return Ok(V::Object(fields));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    if !self.eat(b':') {
                        return Err(IntentError::InvalidJson);
                    }
                    let value = self.value(depth + 1)?;
                    if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == &key) {
                        *old = value;
                    } else {
                        if fields.len() >= MAX_ROWS {
                            return Err(IntentError::Capacity);
                        }
                        fields.push((key, value));
                    }
                    self.space();
                    if self.eat(b'}') {
                        break;
                    }
                    if !self.eat(b',') {
                        return Err(IntentError::InvalidJson);
                    }
                }
                // JSON.parse defines __proto__ as an own data field; unlike
                // assigning the book's cache key, parsing it mutates no prototype.
                order_integer_fields(&mut fields);
                Ok(V::Object(fields))
            }
            _ => Err(IntentError::InvalidJson),
        }
    }
}
fn big_number_text(value: &str, signed_zero: bool) -> Eval<String> {
    let text = value.trim_matches(js_space);
    let d = text.parse::<Decimal>();
    Ok(if let Ok(d) = d {
        if d.is_zero() && signed_zero && text.starts_with('-') {
            "-0".into()
        } else {
            let exponent = d.coefficient_digits() as i32 - d.decimal_places() as i32 - 1;
            if !d.is_zero() && (exponent <= -7 || exponent >= 21) {
                let full = d.abs().to_string();
                let digits = full.chars().filter(|c| *c != '.').collect::<String>();
                let digits = digits.trim_start_matches('0').trim_end_matches('0');
                format!(
                    "{}{}{}e{}{exponent}",
                    if d.is_negative() { "-" } else { "" },
                    &digits[..1],
                    if digits.len() > 1 {
                        format!(".{}", &digits[1..])
                    } else {
                        String::new()
                    },
                    if exponent >= 0 { "+" } else { "" }
                )
            } else {
                d.to_string()
            }
        }
    } else if matches!(text, "NaN" | "Infinity" | "-Infinity") {
        text.into()
    } else {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    })
}
fn normalize_json(value: &V) -> Eval<V> {
    Ok(match value {
        V::Promise => obj(&[]),
        V::BigNumber(value) => s(&big_number_text(value, true)?),
        V::Array(a) => V::Array(a.iter().map(normalize_json).collect::<Eval<_>>()?),
        V::Object(o) => V::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), normalize_json(v)?)))
                .collect::<Eval<_>>()?,
        ),
        _ => value.clone(),
    })
}
fn json_stringify(value: &V) -> Eval<V> {
    if matches!(value, V::Undefined) {
        return Ok(V::Undefined);
    }
    crate::property_catalog::json_stringify(&normalize_json(value)?)
        .map(|v| s(&v))
        .map_err(|e| match e {
            crate::property_catalog::CatalogError::Capacity => SourceRuleError::Capacity.into(),
            _ => SourceRuleError::UnsupportedNumericDomain.into(),
        })
}
fn empty_book() -> V {
    obj(&[("buy", V::Array(vec![])), ("sell", V::Array(vec![]))])
}

/// An injected rejection of the declared orderBooks adapter. Source catches
/// apply at their original boundaries; successful earlier writes remain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDbFailure {
    pub operation: String,
    pub skip: usize,
    pub exception: SourceException,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDbCall {
    pub operation: String,
    pub query: V,
    pub update: V,
    pub options: V,
}
impl QueueDbCall {
    pub fn value(&self) -> V {
        obj(&[
            ("collection", s("orderBooks")),
            ("operation", s(&self.operation)),
            ("query", self.query.clone()),
            ("update", self.update.clone()),
            ("options", self.options.clone()),
        ])
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Instance {
    id: u64,
    key: V,
    tick_size: V,
    books: V,
    block: V,
}
impl Instance {
    fn value(&self) -> V {
        obj(&[
            ("tickSize", self.tick_size.clone()),
            ("orderBookKey", self.key.clone()),
            ("orderBooks", self.books.clone()),
            ("block", self.block.clone()),
        ])
    }
    fn handle(&self) -> V {
        obj(&[("orderbookInstance", V::number(self.id as f64))])
    }
}
/// The DB collection is global to this owner, while each get-instance call
/// constructs a fresh instance. No repaired typed queue or second Ledger exists.
#[derive(Debug, Clone, Default)]
pub struct ContractQueueSourceOwner {
    rows: Vec<V>,
    instances: Vec<Instance>,
    failures: Vec<QueueDbFailure>,
    db_calls: Vec<QueueDbCall>,
    placement_calls: Vec<DependencyCall>,
    db_bytes: usize,
    placement_bytes: usize,
    // Ephemeral execution observations. Never part of V1 state/checkpoint/Eq.
    detailed_db_calls: Vec<V>,
    detailed_db_bytes: usize,
    detailed_db_exhausted: bool,
}
impl PartialEq for ContractQueueSourceOwner {
    fn eq(&self, o: &Self) -> bool {
        self.rows == o.rows && self.instances == o.instances && self.failures == o.failures
    }
}
impl Eq for ContractQueueSourceOwner {}
impl ContractQueueSourceOwner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn rows(&self) -> &[V] {
        &self.rows
    }
    pub fn import_rows(&mut self, rows: Vec<V>) -> Result<(), SourceRuleError> {
        validate_rows(&rows)?;
        let mut next = self.clone();
        next.rows = rows;
        next.checkpoint()?;
        *self = next;
        Ok(())
    }
    pub fn fail_database_call(&mut self, failure: QueueDbFailure) -> Result<(), SourceRuleError> {
        if !matches!(
            failure.operation.as_str(),
            "getDatabase" | "findOneAsync" | "updateAsync" | "loadDatabase"
        ) {
            return Err(SourceRuleError::InvalidInput);
        }
        if self.failures.len() >= MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        s(&failure.exception.name).validate()?;
        s(&failure.exception.message).validate()?;
        let mut next = self.clone();
        next.failures.push(failure);
        next.checkpoint()?;
        *self = next;
        Ok(())
    }
    pub fn take_db_calls(&mut self) -> Vec<QueueDbCall> {
        self.db_bytes = 0;
        core::mem::take(&mut self.db_calls)
    }
    pub fn take_detailed_db_calls(&mut self) -> Result<Vec<V>, SourceRuleError> {
        self.detailed_db_bytes = 0;
        let events = core::mem::take(&mut self.detailed_db_calls);
        if core::mem::take(&mut self.detailed_db_exhausted) {
            Err(SourceRuleError::Capacity)
        } else {
            Ok(events)
        }
    }
    pub fn take_placement_calls(&mut self) -> Vec<DependencyCall> {
        self.placement_bytes = 0;
        core::mem::take(&mut self.placement_calls)
    }
    pub fn state_value(&self) -> V {
        obj(&[
            ("orderBooks", V::Array(self.rows.clone())),
            (
                "instances",
                V::Array(
                    self.instances
                        .iter()
                        .map(|i| V::Array(vec![V::number(i.id as f64), i.value()]))
                        .collect(),
                ),
            ),
        ])
    }
    pub fn instance_value(&self, id: u64) -> Result<V, SourceRuleError> {
        Ok(self.instance(id)?.value())
    }
    fn instance(&self, id: u64) -> Result<&Instance, SourceRuleError> {
        self.instances
            .iter()
            .find(|i| i.id == id)
            .ok_or(SourceRuleError::InvalidInput)
    }
    fn instance_mut(&mut self, id: u64) -> Result<&mut Instance, SourceRuleError> {
        self.instances
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or(SourceRuleError::InvalidInput)
    }
    pub fn create_instance(
        &mut self,
        key: V,
        tick_size: Option<V>,
    ) -> Result<u64, SourceRuleError> {
        key.validate()?;
        let tick_size = tick_size.unwrap_or_else(|| V::BigNumber("0.00000001".into()));
        tick_size.validate()?;
        if self.instances.len() >= MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        let id = self.instances.len() as u64 + 1;
        let instance = Instance {
            id,
            key,
            tick_size,
            books: obj(&[]),
            block: V::number(1.0),
        };
        instance.value().validate()?;
        let before = self.clone();
        self.instances.push(instance);
        if let Err(e) = self.checkpoint() {
            *self = before;
            return Err(e);
        }
        Ok(id)
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.encode_mode(true)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.checkpoint()
    }
    pub fn durable_checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.encode_mode(false)
    }
    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::decode_mode(bytes, true)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::from_checkpoint(bytes)
    }
    pub fn restore_durable(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::decode_mode(bytes, false)
    }
    fn encode_mode(&self, complete: bool) -> Result<Vec<u8>, SourceRuleError> {
        validate_rows(&self.rows)?;
        let mut out = b"TLQS\x01".to_vec();
        out.push(u8::from(complete));
        put_rows(&mut out, &self.rows)?;
        if complete {
            if self.instances.len() > MAX_ROWS || self.failures.len() > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            put_u32(&mut out, self.instances.len())?;
            for (i, instance) in self.instances.iter().enumerate() {
                if instance.id != i as u64 + 1 {
                    return Err(SourceRuleError::InvalidInput);
                }
                instance.value().validate()?;
                out.extend_from_slice(&instance.id.to_be_bytes());
                put_value(&mut out, &instance.key)?;
                put_value(&mut out, &instance.tick_size)?;
                put_value(&mut out, &instance.books)?;
                put_value(&mut out, &instance.block)?;
            }
            put_u32(&mut out, self.failures.len())?;
            for f in &self.failures {
                put_string(&mut out, &f.operation)?;
                put_u32(&mut out, f.skip)?;
                put_string(&mut out, &f.exception.name)?;
                put_string(&mut out, &f.exception.message)?;
            }
        }
        if out.len() > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        with_crc(out).map_err(|_| SourceRuleError::Capacity)
    }
    fn decode_mode(bytes: &[u8], complete: bool) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_STATE_BYTES + 4 {
            return Err(SourceRuleError::Capacity);
        }
        let mut c = Cursor::new(verify_crc(bytes).map_err(|_| SourceRuleError::InvalidInput)?);
        if c.take(5).map_err(|_| SourceRuleError::InvalidInput)? != b"TLQS\x01"
            || c.u8().map_err(|_| SourceRuleError::InvalidInput)? != u8::from(complete)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut owner = Self::new();
        owner.rows = get_rows(&mut c)?;
        if complete {
            let len = get_u32(&mut c)?;
            if len > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            for _ in 0..len {
                let id = c.u64().map_err(|_| SourceRuleError::InvalidInput)?;
                let key = get_value(&mut c, 0)?;
                let tick_size = get_value(&mut c, 0)?;
                let books = get_value(&mut c, 0)?;
                if !matches!(books, V::Object(_)) {
                    return Err(SourceRuleError::InvalidInput);
                }
                let block = get_value(&mut c, 0)?;
                owner.instances.push(Instance {
                    id,
                    key,
                    tick_size,
                    books,
                    block,
                });
            }
            let len = get_u32(&mut c)?;
            if len > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            for _ in 0..len {
                let operation = get_string(&mut c)?;
                if !matches!(
                    operation.as_str(),
                    "getDatabase" | "findOneAsync" | "updateAsync" | "loadDatabase"
                ) {
                    return Err(SourceRuleError::InvalidInput);
                }
                owner.failures.push(QueueDbFailure {
                    operation,
                    skip: get_u32(&mut c)?,
                    exception: SourceException {
                        name: get_string(&mut c)?,
                        message: get_string(&mut c)?,
                    },
                });
            }
        }
        c.end().map_err(|_| SourceRuleError::InvalidInput)?;
        if owner.encode_mode(complete)? != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(owner)
    }
    pub fn evaluate(&mut self, call: &SourceCall) -> Result<SourceOutcome, SourceRuleError> {
        self.evaluate_with_capability(call, &mut |_, _| Err(SourceRuleError::UnknownDependency))
    }
    pub fn evaluate_with_backend<B: SourceBackend>(
        &mut self,
        call: &SourceCall,
        backend: &mut B,
    ) -> Result<SourceOutcome, SourceRuleError> {
        self.evaluate_with_capability(call, &mut |_, c| backend.invoke(c))
    }
    pub fn evaluate_with_capability<F>(
        &mut self,
        call: &SourceCall,
        capability: &mut F,
    ) -> Result<SourceOutcome, SourceRuleError>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        self.evaluate_mode(None, call, capability)
    }
    pub fn evaluate_instance(
        &mut self,
        instance: u64,
        call: &SourceCall,
    ) -> Result<SourceOutcome, SourceRuleError> {
        self.evaluate_mode(Some(instance), call, &mut |_, _| {
            Err(SourceRuleError::UnknownDependency)
        })
    }
    fn evaluate_mode<F>(
        &mut self,
        instance: Option<u64>,
        call: &SourceCall,
        capability: &mut F,
    ) -> Result<SourceOutcome, SourceRuleError>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        if !METHODS.contains(&call.method.as_str()) {
            return Err(SourceRuleError::UnknownMethod);
        }
        if call.arguments.len() > 8 {
            return Err(SourceRuleError::Capacity);
        }
        for v in &call.arguments {
            v.validate()?;
        }
        if let Some(id) = instance {
            self.instance(id)?;
        }
        let before = self.clone();
        let result = self.run(instance, &call.method, &call.arguments, capability);
        if let Err(Fault::Local(e)) = &result {
            let e = e.clone();
            *self = before;
            return Err(e);
        }
        if let Err(e) = self.checkpoint() {
            *self = before;
            return Err(e);
        }
        match result {
            Ok(v) => Ok(SourceOutcome::Returned(v)),
            Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
            Err(Fault::Boundary(e) | Fault::Local(e)) => Err(e),
        }
    }
    pub fn invoke_source_with_backend<B: SourceBackend>(
        &mut self,
        method: &str,
        args: &[V],
        backend: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        Ok(
            match self.evaluate_with_backend(
                &SourceCall {
                    method: method.into(),
                    arguments: args.to_vec(),
                },
                backend,
            )? {
                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
            },
        )
    }
    pub fn invoke_source(
        &mut self,
        method: &str,
        args: &[V],
    ) -> Result<DependencyReturn, SourceRuleError> {
        Ok(
            match self.evaluate(&SourceCall {
                method: method.into(),
                arguments: args.to_vec(),
            })? {
                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
            },
        )
    }
    fn db_call(&mut self, operation: &str, query: V, update: V, options: V) -> Eval<()> {
        let call = QueueDbCall {
            operation: operation.into(),
            query,
            update,
            options,
        };
        let bytes = value_size(&call.value());
        while !self.db_calls.is_empty()
            && (self.db_calls.len() >= MAX_ROWS * 16
                || self.db_bytes.saturating_add(bytes) > MAX_STATE_BYTES)
        {
            self.db_bytes = self
                .db_bytes
                .saturating_sub(value_size(&self.db_calls.remove(0).value()));
        }
        self.db_bytes = self.db_bytes.saturating_add(bytes);
        self.db_calls.push(call);
        if let Some(i) = self.failures.iter().position(|f| f.operation == operation) {
            if self.failures[i].skip == 0 {
                return Err(Fault::Thrown(self.failures.remove(i).exception));
            }
            self.failures[i].skip -= 1;
        }
        Ok(())
    }
    fn database(&mut self) -> Eval<()> {
        let result = self.db_call("getDatabase", V::Undefined, V::Undefined, V::Undefined);
        self.detailed_db(
            "getDatabase",
            vec![s("orderBooks")],
            &result
                .as_ref()
                .map(|_| obj(&[("database", s("orderBooks"))])),
        );
        result
    }
    fn reload(&mut self) -> Eval<()> {
        let result = self.db_call("loadDatabase", V::Undefined, V::Undefined, V::Undefined);
        self.detailed_db(
            "loadDatabase",
            vec![],
            &result.as_ref().map(|_| V::Undefined),
        );
        result
    }
    fn find(&mut self, key: V) -> Eval<V> {
        let query = obj(&[("_id", key.clone())]);
        let result = (|| {
            self.db_call("findOneAsync", query.clone(), V::Undefined, V::Undefined)?;
            Ok(self
                .rows
                .iter()
                .find(|r| strict(&r.get("_id"), &key))
                .cloned()
                .unwrap_or(V::Null))
        })();
        self.detailed_db("findOneAsync", vec![query], &result.as_ref().cloned());
        result
    }
    fn update(&mut self, key: V, doc: V) -> Eval<()> {
        let query = obj(&[("_id", key.clone())]);
        let options = obj(&[("upsert", V::Bool(true))]);
        let args = vec![query.clone(), doc.clone(), options.clone()];
        let result = (|| {
            self.db_call("updateAsync", query, doc.clone(), options)?;
            doc.validate()?;
            if let Some(i) = self.rows.iter().position(|r| strict(&r.get("_id"), &key)) {
                self.rows[i] = doc;
            } else {
                if self.rows.len() >= MAX_ROWS {
                    return Err(SourceRuleError::Capacity.into());
                }
                self.rows.push(doc);
            }
            Ok(())
        })();
        self.detailed_db(
            "updateAsync",
            args,
            &result.as_ref().map(|_| V::number(1.0)),
        );
        result
    }
    fn detailed_db(&mut self, method: &str, args: Vec<V>, result: &Result<V, &Fault>) {
        let mut event = if method == "getDatabase" {
            obj(&[
                ("kind", s("database")),
                ("name", s("orderBooks")),
                ("method", s(method)),
                ("args", V::Array(args)),
            ])
        } else {
            obj(&[
                ("kind", s("db")),
                ("collection", s("orderBooks")),
                ("op", s(method.strip_suffix("Async").unwrap_or(method))),
                ("method", s(method)),
                ("args", V::Array(args)),
            ])
        };
        match result {
            Ok(v) => {
                if set_object(&mut event, "returned", v.clone()).is_err() {
                    self.detailed_db_exhausted = true;
                    return;
                }
            }
            Err(Fault::Thrown(e)) => {
                if set_object(
                    &mut event,
                    "thrown",
                    obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                )
                .is_err()
                {
                    self.detailed_db_exhausted = true;
                    return;
                }
            }
            _ => (),
        }
        let bytes = value_size(&event);
        if self.detailed_db_calls.len() >= MAX_ROWS * 16
            || self.detailed_db_bytes.saturating_add(bytes) > MAX_STATE_BYTES
        {
            self.detailed_db_exhausted = true;
            return;
        }
        self.detailed_db_bytes += bytes;
        self.detailed_db_calls.push(event);
    }
    pub(crate) fn load_book_with_alias(
        &mut self,
        instance: u64,
        key: V,
    ) -> Result<(SourceOutcome, Option<String>), SourceRuleError> {
        let before = self.clone();
        let mut parsed = false;
        let result = self.load_book_status(instance, key.clone(), &mut parsed);
        match result {
            Ok(value) => {
                let alias = if parsed && matches!(value, V::Object(_) | V::Array(_)) {
                    Some(text(&key).map_err(|_| SourceRuleError::InvalidInput)?)
                } else {
                    None
                };
                Ok((SourceOutcome::Returned(value), alias))
            }
            Err(Fault::Thrown(error)) => Ok((SourceOutcome::Thrown(error), None)),
            Err(Fault::Local(error)) => {
                *self = before;
                Err(error)
            }
            Err(Fault::Boundary(error)) => Err(error),
        }
    }
    pub(crate) fn set_cached_book_unverified(
        &mut self,
        instance: u64,
        key: &str,
        value: V,
    ) -> Result<(), SourceRuleError> {
        value.validate()?;
        let before = self.clone();
        match self
            .instance_mut(instance)
            .map_err(Fault::Local)
            .and_then(|i| set_object(&mut i.books, key, value))
        {
            Ok(()) => Ok(()),
            Err(Fault::Local(error) | Fault::Boundary(error)) => {
                *self = before;
                Err(error)
            }
            Err(Fault::Thrown(_)) => {
                *self = before;
                Err(SourceRuleError::InvalidInput)
            }
        }
    }
    fn load_book(&mut self, instance: u64, key: V) -> Eval<V> {
        self.load_book_status(instance, key, &mut false)
    }
    fn load_book_status(&mut self, instance: u64, key: V, parsed_alias: &mut bool) -> Eval<V> {
        let key = text(&key)?;
        self.database()?;
        self.source_load_book_after_factory(instance, key, parsed_alias)
    }
    fn source_load_book_after_factory(
        &mut self,
        instance: u64,
        key: String,
        parsed_alias: &mut bool,
    ) -> Eval<V> {
        let result = (|| -> Eval<V> {
            let row = self.find(s(&key))?;
            let value = if truth(&row) {
                prop(&row, "value")?
            } else {
                V::Undefined
            };
            if !truth(&value) {
                return Ok(empty_book());
            }
            let parsed = match BookJsonParser::parse(&text(&value)?) {
                Ok(v) => v,
                Err(crate::issuance_intent::IntentError::InvalidJson) => return Ok(empty_book()),
                Err(
                    crate::issuance_intent::IntentError::Capacity
                    | crate::issuance_intent::IntentError::DepthLimit,
                ) => return Err(SourceRuleError::Capacity.into()),
                Err(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
            };
            set_object(
                &mut self.instance_mut(instance)?.books,
                &key,
                parsed.clone(),
            )?;
            *parsed_alias = true;
            Ok(parsed)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(empty_book()),
            other => other,
        }
    }
    fn get_instance(&mut self, key: V) -> Eval<u64> {
        let id = self.create_instance(key.clone(), None)?;
        let book = self.load_book(id, key.clone())?;
        set_object(&mut self.instance_mut(id)?.books, &text(&key)?, book)?;
        Ok(id)
    }
    fn save_book(&mut self, data: V, key: V) -> Eval<V> {
        let key = text(&key)?;
        self.database()?;
        let value = json_stringify(&data)?;
        self.update(s(&key), obj(&[("_id", s(&key)), ("value", value)]))?;
        Ok(V::Undefined)
    }
    fn active_pair(&mut self, key: V) -> Eval<V> {
        self.database()?;
        let row = self.find(s("activePairs"))?;
        let mut pairs = match row.get("pairs") {
            V::Array(a) => a,
            _ => vec![],
        };
        if !pairs.iter().any(|v| same_key(v, &key)) {
            pairs.push(key);
            self.update(
                s("activePairs"),
                obj(&[("_id", s("activePairs")), ("pairs", V::Array(pairs))]),
            )?;
            self.reload()?;
        }
        Ok(V::Undefined)
    }
    fn update_active(&mut self, pairs: V) -> Eval<V> {
        self.database()?;
        self.update(
            s("activePairs"),
            obj(&[("_id", s("activePairs")), ("pairs", pairs)]),
        )?;
        self.reload()?;
        Ok(V::Undefined)
    }
    fn queue(&mut self, kind: &str, args: &[V]) -> Eval<V> {
        self.database()?;
        let key = text(&arg(args, 0))?;
        let queue = s(&format!("queue-{key}"));
        let row = self.find(queue.clone())?;
        let mut entries = match row.get("orders") {
            V::Array(a) => a,
            _ => vec![],
        };
        entries.push(obj(&[
            ("kind", s(kind)),
            ("orderBookKey", s(&key)),
            ("sender", arg(args, 1)),
            ("blockHeight", V::number(number(&arg(args, 3))?)),
            ("txid", arg(args, 4)),
            (
                if kind == "contract" {
                    "params"
                } else {
                    "order"
                },
                arg(args, 2),
            ),
        ]));
        self.update(
            queue.clone(),
            obj(&[("_id", queue), ("orders", V::Array(entries))]),
        )?;
        self.reload()?;
        self.active_pair(s(&key))
    }
    fn placement<F>(&mut self, call: DependencyCall, capability: &mut F) -> Eval<()>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        let bytes = value_size(&V::Array(call.arguments.clone())).saturating_add(call.target.len());
        while !self.placement_calls.is_empty()
            && (self.placement_calls.len() >= MAX_ROWS * 16
                || self.placement_bytes.saturating_add(bytes) > MAX_STATE_BYTES)
        {
            let old = self.placement_calls.remove(0);
            self.placement_bytes = self.placement_bytes.saturating_sub(
                value_size(&V::Array(old.arguments)).saturating_add(old.target.len()),
            );
        }
        self.placement_bytes = self.placement_bytes.saturating_add(bytes);
        self.placement_calls.push(call.clone());
        match capability(self, &call).map_err(Fault::Boundary)? {
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => Ok(()),
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Err(Fault::Thrown(e)),
        }
    }
    fn process<F>(&mut self, block: V, capability: &mut F) -> Eval<V>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        let height = number(&block)?;
        self.database()?;
        let active = self.find(s("activePairs"))?;
        let original = match active.get("pairs") {
            V::Array(a) if !a.is_empty() => a,
            _ => return Ok(V::Undefined),
        };
        let mut active = original.iter().cloned().enumerate().collect::<Vec<_>>();
        for (index, pair) in original.into_iter().enumerate() {
            let queue = s(&format!("queue-{}", text(&pair)?));
            let row = self.find(queue.clone())?;
            let entries = match row.get("orders") {
                V::Array(a) if !a.is_empty() => a,
                _ => {
                    remove_active_pair(&mut active, index, &pair);
                    self.update(
                        queue.clone(),
                        obj(&[("_id", queue), ("orders", V::Array(vec![]))]),
                    )?;
                    continue;
                }
            };
            let mut ready = vec![];
            let mut future = vec![];
            for entry in entries {
                let entry_height = number(&prop(&entry, "blockHeight")?)?;
                if !entry_height.is_nan() && entry_height <= height {
                    ready.push(entry);
                } else {
                    future.push(entry);
                }
            }
            if ready.is_empty() {
                if future.is_empty() {
                    remove_active_pair(&mut active, index, &pair);
                }
                continue;
            }
            sort_ready(&mut ready)?;
            let instance = self.get_instance(pair.clone())?;
            for entry in ready {
                match prop(&entry, "kind")? {
                    V::String(kind) if kind == "token" => self.placement(
                        DependencyCall {
                            target: "Orderbook.addTokenOrder".into(),
                            arguments: vec![
                                prop(&entry, "order")?,
                                prop(&entry, "blockHeight")?,
                                prop(&entry, "txid")?,
                                self.instance(instance)?.handle(),
                            ],
                        },
                        capability,
                    )?,
                    V::String(kind) if kind == "contract" => {
                        let params = prop(&entry, "params")?;
                        let mut args = vec![];
                        for key in [
                            "contractId",
                            "price",
                            "amount",
                            "sell",
                            "insurance",
                            "blockTime",
                        ] {
                            args.push(prop(&params, key)?);
                        }
                        args.push(prop(&entry, "txid")?);
                        args.push(prop(&entry, "sender")?);
                        let liq = prop(&params, "isLiq")?;
                        args.push(if truth(&liq) { liq } else { V::Bool(false) });
                        for key in ["reduce", "post", "stop"] {
                            args.push(prop(&params, key)?);
                        }
                        args.push(self.instance(instance)?.handle());
                        self.placement(
                            DependencyCall {
                                target: "Orderbook.addContractOrder".into(),
                                arguments: args,
                            },
                            capability,
                        )?;
                    }
                    _ => (),
                }
            }
            self.load_book(instance, pair.clone())?;
            self.update(
                queue.clone(),
                obj(&[("_id", queue), ("orders", V::Array(future.clone()))]),
            )?;
            if future.is_empty() {
                remove_active_pair(&mut active, index, &pair);
            }
        }
        self.update_active(V::Array(
            active.into_iter().map(|(_, value)| value).collect(),
        ))
    }
    fn run<F>(
        &mut self,
        instance: Option<u64>,
        method: &str,
        args: &[V],
        capability: &mut F,
    ) -> Eval<V>
    where
        F: FnMut(&mut Self, &DependencyCall) -> Result<DependencyReturn, SourceRuleError>,
    {
        match method {
            "_getOrderbookDB" => {
                self.database()?;
                Ok(obj(&[("sourceDatabase", s("orderBooks"))]))
            }
            "_addActivePair" => self.active_pair(arg(args, 0)),
            "_updateActivePairs" => self.update_active(arg(args, 0)),
            "queueOnChainTokenOrder" => self.queue("token", args),
            "queueOnChainContractOrder" => self.queue("contract", args),
            "processQueuedOnChainOrdersForBlock" => self.process(arg(args, 0), capability),
            "getOrderbookInstance" => {
                let id = self.get_instance(arg(args, 0))?;
                Ok(self.instance(id)?.handle())
            }
            "loadOrderBook" => {
                self.load_book(instance.ok_or(SourceRuleError::InvalidInput)?, arg(args, 0))
            }
            "saveOrderBook" => {
                self.instance(instance.ok_or(SourceRuleError::InvalidInput)?)?;
                self.save_book(arg(args, 0), arg(args, 1))
            }
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
}
impl SourceBackend for ContractQueueSourceOwner {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let method = call
            .target
            .strip_prefix("Orderbook.")
            .ok_or(SourceRuleError::UnknownDependency)?;
        self.invoke_source(method, &call.arguments)
    }
}

// A SourceValue is an owned tree. Distinct reference-valued entries remain
// distinct even when their fields compare equal; slice() retains each original
// entry identity while filter(k !== pairKey) removes that very entry.
fn remove_active_pair(active: &mut Vec<(usize, V)>, index: usize, pair: &V) {
    if matches!(
        pair,
        V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
    ) {
        active.retain(|(i, _)| *i != index);
    } else {
        active.retain(|(_, value)| !strict(value, pair));
    }
}

fn locale_ascii(a: &str, b: &str) -> Result<core::cmp::Ordering, SourceRuleError> {
    const PRIMARY: &[u8] =
        b"_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";
    if !a
        .bytes()
        .chain(b.bytes())
        .all(|v| (0x20..=0x7e).contains(&v))
    {
        return Err(SourceRuleError::UnsupportedNumericDomain);
    }
    let rank = |v: u8| {
        if v == b' ' {
            0
        } else {
            PRIMARY
                .iter()
                .position(|x| *x == v.to_ascii_lowercase())
                .map(|i| i + 1)
                .unwrap_or(usize::MAX)
        }
    };
    Ok(a.bytes().map(rank).cmp(b.bytes().map(rank)).then_with(|| {
        a.bytes()
            .map(|v| v.is_ascii_uppercase())
            .cmp(b.bytes().map(|v| v.is_ascii_uppercase()))
    }))
}
fn compare_sender(a: &V, b: &V, txa: &V, txb: &V) -> Eval<core::cmp::Ordering> {
    let alen = prop(a, "length")?;
    let blen = prop(b, "length")?;
    let tail = |value: &V, len: &V, index: usize| -> Eval<Option<(u8, String)>> {
        let n = number(len)? - index as f64;
        let key = text(&V::number(n))?;
        let ch = prop(value, &key)?;
        if !truth(&ch) {
            return Ok(None);
        }
        let V::String(ch) = ch else {
            return Err(type_error("ch.toLowerCase is not a function"));
        };
        Ok(Some((
            u8::from(ch.as_str() >= "0" && ch.as_str() <= "9"),
            ch.to_lowercase(),
        )))
    };
    for i in 1..=3 {
        let x = tail(a, &alen, i)?;
        let y = tail(b, &blen, i)?;
        let rank = |v: Option<(u8, String)>| v.unwrap_or((2, String::new()));
        let c = rank(x).cmp(&rank(y));
        if c != core::cmp::Ordering::Equal {
            return Ok(c);
        }
    }
    let V::String(a) = a else {
        return Err(type_error("a.localeCompare is not a function"));
    };
    let c = locale_ascii(a, &text(b)?)?;
    if c != core::cmp::Ordering::Equal {
        return Ok(c);
    }
    if truth(txa) && truth(txb) {
        let V::String(a) = txa else {
            return Err(type_error("txidA.localeCompare is not a function"));
        };
        return Ok(locale_ascii(a, &text(txb)?)?);
    }
    Ok(core::cmp::Ordering::Equal)
}
fn compare_ready(a: &V, b: &V) -> Eval<core::cmp::Ordering> {
    let ha = number(&prop(a, "blockHeight")?)?;
    let hb = number(&prop(b, "blockHeight")?)?;
    if ha != hb {
        return Ok(ha.partial_cmp(&hb).unwrap_or(core::cmp::Ordering::Equal));
    }
    compare_sender(
        &prop(a, "sender")?,
        &prop(b, "sender")?,
        &prop(a, "txid")?,
        &prop(b, "txid")?,
    )
}
fn sort_ready(rows: &mut [V]) -> Eval<()> {
    // V8's small-array natural run + stable binary insertion path. For the
    // consistent supported ASCII comparator it also yields the large-array
    // stable result without changing any persisted rows during sorting.
    if rows.len() < 2 {
        return Ok(());
    }
    let descending = compare_ready(&rows[1], &rows[0])? == core::cmp::Ordering::Less;
    let mut run = 2;
    while run < rows.len() {
        let c = compare_ready(&rows[run], &rows[run - 1])?;
        if if descending {
            c != core::cmp::Ordering::Less
        } else {
            c == core::cmp::Ordering::Less
        } {
            break;
        }
        run += 1;
    }
    if descending {
        rows[..run].reverse();
    }
    for at in run..rows.len() {
        let pivot = rows[at].clone();
        let (mut low, mut high) = (0, at);
        while low < high {
            let mid = low + (high - low) / 2;
            if compare_ready(&pivot, &rows[mid])? == core::cmp::Ordering::Less {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        rows[low..=at].rotate_right(1);
    }
    Ok(())
}

include!("source_spot_queue_fragments_impl.rs");
