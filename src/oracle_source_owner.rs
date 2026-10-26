//! Full-document pinned oracle.js owner. Source exceptions retain partial writes.
//! Metadata ownership contains no Ledger and performs no caller authentication.
use crate::codec::{verify_crc, with_crc, Cursor};
use crate::decimal::{Decimal, Rounding};
use crate::hash256::sha256;
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
use core::str::FromStr;
pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d";
pub const MAX_ROWS: usize = 1024;
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
pub const METHODS: [&str; 19] = [
    "getAllOracles",
    "addOracle",
    "getOracleInfo",
    "getOraclePrice",
    "publishData",
    "load",
    "isAdmin",
    "verifyAdmin",
    "updateAdmin",
    "recordStake",
    "getStake",
    "applyFraudProof",
    "relayTradeLayerState",
    "createOracle",
    "getNextId",
    "saveOracleData",
    "loadOracleData",
    "closeOracle",
    "getTWAP",
];
/// Read-side bridge to the same JavaScript Number coercion used by this owner.
/// Source exceptions cannot become a fabricated usable market observation.
pub(crate) fn source_number_unverified(value: &V) -> Result<f64, SourceRuleError> {
    number(value).map_err(|fault| match fault {
        Fault::Local(e) => e,
        Fault::Thrown(_) => SourceRuleError::UnsupportedNumericDomain,
    })
}
pub(crate) fn source_greater_unverified(left: &V, right: &V) -> Result<bool, SourceRuleError> {
    relational(left, right)
        .map(|o| o == Some(core::cmp::Ordering::Greater))
        .map_err(|fault| match fault {
            Fault::Local(e) => e,
            Fault::Thrown(_) => SourceRuleError::UnsupportedNumericDomain,
        })
}
type Eval<T> = Result<T, Fault>;
#[derive(Debug)]
enum Fault {
    Thrown(SourceException),
    Local(SourceRuleError),
}
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
fn error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "Error".into(),
        message: message.into(),
    })
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
fn or(values: &[V]) -> V {
    values
        .iter()
        .find(|v| truth(v))
        .cloned()
        .unwrap_or_else(|| values.last().cloned().unwrap_or(V::Undefined))
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
fn map_index(map: &[(V, V)], key: &V) -> Option<usize> {
    map.iter().position(|(k, _)| same_key(k, key))
}
fn map_get<'a>(map: &'a [(V, V)], key: &V) -> Option<&'a V> {
    map_index(map, key).map(|i| &map[i].1)
}
fn map_set(map: &mut Vec<(V, V)>, key: V, value: V) -> Result<(), SourceRuleError> {
    primitive_key(&key)?;
    let key = if matches!(&key,V::Number(b)if f64::from_bits(*b)==0.0) {
        V::number(0.0)
    } else {
        key
    };
    if let Some(i) = map_index(map, &key) {
        map[i].1 = value;
    } else {
        if map.len() == MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        map.push((key, value));
    }
    Ok(())
}
fn text(v: &V) -> Eval<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(s) | V::BigNumber(s) => s.clone(),
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
fn parse_int(v: &V) -> Eval<f64> {
    let t = text(v)?;
    let t = t.trim_start_matches(js_space);
    let (sign, t) = if let Some(t) = t.strip_prefix('-') {
        (-1.0, t)
    } else {
        (1.0, t.strip_prefix('+').unwrap_or(t))
    };
    let (rad, t) = if let Some(t) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (16, t)
    } else {
        (10, t)
    };
    let digits: String = t.chars().take_while(|c| c.is_digit(rad)).collect();
    Ok(if digits.is_empty() {
        f64::NAN
    } else if rad == 10 {
        sign * digits.parse::<f64>().unwrap_or(f64::INFINITY)
    } else {
        sign * radix(&digits, rad)?
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
fn array(v: &V) -> Eval<&[V]> {
    if let V::Array(a) = v {
        Ok(a)
    } else {
        Err(SourceRuleError::InvalidInput.into())
    }
}
fn path(row: &V, key: &str) -> V {
    key.split('.').fold(row.clone(), |row, key| row.get(key))
}
fn matches_query(row: &V, query: &V) -> Eval<bool> {
    let V::Object(fields) = query else {
        return Ok(false);
    };
    for (key, value) in fields {
        let actual = path(row, key);
        if let V::Object(operators) = value {
            for (op, v) in operators {
                if !match op.as_str() {
                    "$exists" => strict(&V::Bool(actual != V::Undefined), v),
                    "$ne" => !strict(&actual, v),
                    "$gte" => {
                        relational(&actual, v)?.is_some_and(|o| o != core::cmp::Ordering::Less)
                    }
                    "$lte" => {
                        relational(&actual, v)?.is_some_and(|o| o != core::cmp::Ordering::Greater)
                    }
                    _ => false,
                } {
                    return Ok(false);
                }
            }
        } else if !strict(&actual, value) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn validate_rows(rows: &[V]) -> Result<(), SourceRuleError> {
    if rows.len() > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    let mut bytes = 0usize;
    for (i, row) in rows.iter().enumerate() {
        row.validate()?;
        if !matches!(row, V::Object(_)) {
            return Err(SourceRuleError::InvalidInput);
        }
        primitive_key(&row.get("_id"))?;
        if rows[..i]
            .iter()
            .any(|r| same_key(&r.get("_id"), &row.get("_id")))
        {
            return Err(SourceRuleError::InvalidInput);
        }
        bytes = bytes
            .checked_add(row.diagnostic_json()?.len())
            .ok_or(SourceRuleError::Capacity)?;
        if bytes > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
    }
    Ok(())
}

#[derive(Clone)]
enum Bn {
    Finite(Decimal),
    NegativeZero,
    Special(f64),
}
impl Bn {
    fn new(v: &V) -> Eval<Self> {
        if matches!(v,V::Number(bits)if *bits==(-0.0f64).to_bits()) {
            return Ok(Self::NegativeZero);
        }
        let text = match v {
            V::String(s) | V::BigNumber(s) => s.trim_matches(js_space).to_string(),
            V::Number(_) => text(v)?,
            _ => text(v)?,
        };
        if text.len() > 512 {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let unsigned = text.trim_start_matches(['+', '-']);
        if let Some(radix) = if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
            Some(16)
        } else if unsigned.starts_with("0b") || unsigned.starts_with("0B") {
            Some(2)
        } else if unsigned.starts_with("0o") || unsigned.starts_with("0O") {
            Some(8)
        } else {
            None
        } {
            let body = &unsigned[2..];
            let parts = body.split('.').collect::<Vec<_>>();
            if parts.len() > 2
                || parts[0].is_empty()
                || !parts.iter().all(|p| p.chars().all(|c| c.is_digit(radix)))
            {
                return Ok(Self::Special(f64::NAN));
            }
            let mut value = Decimal::zero();
            let mut divisor = Decimal::from(1u32);
            for c in parts.iter().flat_map(|p| p.chars()) {
                value = value
                    .checked_mul(&Decimal::from(radix))
                    .and_then(|n| n.checked_add(&Decimal::from(c.to_digit(radix).unwrap_or(0))))
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            }
            for _ in 0..parts.get(1).map_or(0, |p| p.len()) {
                divisor = divisor
                    .checked_mul(&Decimal::from(radix))
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            }
            value = value
                .checked_div(&divisor, 20, Rounding::HalfUp)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if value.is_zero() && text.starts_with('-') {
                return Ok(Self::NegativeZero);
            }
            return Ok(Self::Finite(if text.starts_with('-') {
                value.negated()
            } else {
                value
            }));
        }
        if let Ok(d) = Decimal::from_str(&text) {
            if d.is_zero() && text.starts_with('-') {
                Ok(Self::NegativeZero)
            } else {
                Ok(Self::Finite(d))
            }
        } else {
            if matches!(
                Decimal::from_str(&text),
                Err(crate::decimal::DecimalError::LimitExceeded)
            ) {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
            Ok(Self::Special(match text.as_str() {
                "Infinity" | "+Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => f64::NAN,
            }))
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Finite(d) => d.to_string().parse().unwrap_or(f64::NAN),
            Self::NegativeZero => -0.0,
            Self::Special(n) => *n,
        }
    }
    fn op(self, b: Self, kind: u8) -> Eval<Self> {
        let finite = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(Decimal::zero()),
            Self::Special(_) => None,
        };
        let aneg =
            matches!(self, Self::NegativeZero) || matches!(&self,Self::Finite(d)if d.is_negative());
        let bneg =
            matches!(b, Self::NegativeZero) || matches!(&b,Self::Finite(d)if d.is_negative());
        if let (Some(a), Some(b)) = (finite(&self), finite(&b)) {
            if kind == 3 && b.is_zero() {
                return Ok(Self::Special(if a.is_zero() {
                    f64::NAN
                } else if aneg ^ bneg {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                }));
            }
            let d = match kind {
                0 => a.checked_add(&b),
                1 => a.checked_sub(&b),
                2 => a.checked_mul(&b),
                _ => a.checked_div(&b, 20, Rounding::HalfUp),
            }
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            let negzero = d.is_zero()
                && match kind {
                    0 => a.is_zero() && b.is_zero() && aneg && bneg,
                    1 => a.is_zero() && b.is_zero() && aneg && !bneg,
                    _ => aneg ^ bneg,
                };
            Ok(if negzero {
                Self::NegativeZero
            } else {
                Self::Finite(d)
            })
        } else {
            let a = self.number();
            let b = b.number();
            Ok(Self::Special(match kind {
                0 => a + b,
                1 => a - b,
                2 => a * b,
                _ => a / b,
            }))
        }
    }
    fn q(self, places: u32, rounding: Rounding) -> Eval<Self> {
        match self {
            Self::Finite(d) => {
                let q = d
                    .quantize(places, rounding)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                Ok(if q.is_zero() && d.is_negative() {
                    Self::NegativeZero
                } else {
                    Self::Finite(q)
                })
            }
            _ => Ok(self),
        }
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
fn put_map(out: &mut Vec<u8>, map: &[(V, V)]) -> Result<(), SourceRuleError> {
    if map.len() > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    put_u32(out, map.len())?;
    for (k, v) in map {
        primitive_key(k)?;
        k.validate()?;
        v.validate()?;
        put_value(out, k)?;
        put_value(out, v)?;
    }
    Ok(())
}
fn get_map(c: &mut Cursor<'_>) -> Result<Vec<(V, V)>, SourceRuleError> {
    let n = get_u32(c)?;
    if n > MAX_ROWS {
        return Err(SourceRuleError::Capacity);
    }
    let mut map = vec![];
    for _ in 0..n {
        let k = get_value(c, 0)?;
        let v = get_value(c, 0)?;
        if map_get(&map, &k).is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        map_set(&mut map, k, v)?;
    }
    Ok(map)
}

fn relational(a: &V, b: &V) -> Eval<Option<core::cmp::Ordering>> {
    if let (V::String(a), V::String(b)) = (a, b) {
        Ok(Some(a.encode_utf16().cmp(b.encode_utf16())))
    } else {
        Ok(number(a)?.partial_cmp(&number(b)?))
    }
}
fn spread(value: &V) -> Eval<Vec<(String, V)>> {
    Ok(match value {
        V::Object(o) => o.clone(),
        V::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        V::String(s) => s
            .encode_utf16()
            .enumerate()
            .map(|(i, c)| {
                Ok((
                    i.to_string(),
                    V::String(String::from_utf16(&[c]).map_err(|_| SourceRuleError::InvalidInput)?),
                ))
            })
            .collect::<Eval<_>>()?,
        _ => vec![],
    })
}
fn merged(base: V, extra: &V) -> Eval<V> {
    let mut result = V::Object(spread(&base)?);
    for (k, v) in spread(extra)? {
        set_prop(&mut result, &k, v)?;
    }
    if let V::Object(fields) = &mut result {
        // Object.keys enumerates array-index keys before ordinary properties.
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
    Ok(result)
}
fn optional(value: &V, key: &str) -> Eval<V> {
    if matches!(value, V::Null | V::Undefined) {
        Ok(V::Undefined)
    } else {
        prop(value, key)
    }
}
fn value_size(value: &V) -> usize {
    let payload = match value {
        V::String(s) | V::BigNumber(s) => s.len(),
        V::Array(a) => a.iter().map(value_size).fold(0usize, usize::saturating_add),
        V::Object(o) => o
            .iter()
            .map(|(k, v)| k.len().saturating_add(value_size(v)))
            .fold(0usize, usize::saturating_add),
        _ => 0,
    };
    64usize.saturating_add(payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleDbFailure {
    pub collection: String,
    pub operation: String,
    pub skip: usize,
    pub exception: SourceException,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleDbCall {
    pub collection: String,
    pub operation: String,
    pub query: V,
    pub update: V,
    pub options: V,
}
impl OracleDbCall {
    pub fn value(&self) -> V {
        obj(&[
            ("collection", s(&self.collection)),
            ("operation", s(&self.operation)),
            ("query", self.query.clone()),
            ("update", self.update.clone()),
            ("options", self.options.clone()),
        ])
    }
}
/// Full NeDB documents and typed Map keys retain insertion order. Reads clone
/// DB values; volatile cache and watermark state have an independent lifecycle.
#[derive(Debug, Clone, Default)]
pub struct OracleSourceOwner {
    list_rows: Vec<V>,
    data_rows: Vec<V>,
    oracles: Vec<(V, V)>,
    watermarks: Vec<(V, V)>,
    failures: Vec<OracleDbFailure>,
    calls: Vec<OracleDbCall>,
    dependency_calls: Vec<DependencyCall>,
    calls_bytes: usize,
    dependency_bytes: usize,
}
impl PartialEq for OracleSourceOwner {
    fn eq(&self, other: &Self) -> bool {
        self.list_rows == other.list_rows
            && self.data_rows == other.data_rows
            && self.oracles == other.oracles
            && self.watermarks == other.watermarks
            && self.failures == other.failures
    }
}
impl Eq for OracleSourceOwner {}
struct MissingInsurance;
impl SourceBackend for MissingInsurance {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
impl OracleSourceOwner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn rows(&self, collection: &str) -> Result<&[V], SourceRuleError> {
        match collection {
            "oracleList" => Ok(&self.list_rows),
            "oracleData" => Ok(&self.data_rows),
            _ => Err(SourceRuleError::UnknownDependency),
        }
    }
    pub fn import_rows(&mut self, collection: &str, rows: Vec<V>) -> Result<(), SourceRuleError> {
        validate_rows(&rows)?;
        let mut next = self.clone();
        match collection {
            "oracleList" => next.list_rows = rows,
            "oracleData" => next.data_rows = rows,
            _ => return Err(SourceRuleError::UnknownDependency),
        }
        next.checkpoint()?;
        *self = next;
        Ok(())
    }
    pub fn fail_database_call(&mut self, failure: OracleDbFailure) -> Result<(), SourceRuleError> {
        if !matches!(failure.collection.as_str(), "oracleList" | "oracleData")
            || !matches!(
                failure.operation.as_str(),
                "getDatabase" | "findAsync" | "findOneAsync" | "insertAsync" | "updateAsync"
            )
        {
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
    pub fn take_db_calls(&mut self) -> Vec<OracleDbCall> {
        self.calls_bytes = 0;
        core::mem::take(&mut self.calls)
    }
    pub fn take_dependency_calls(&mut self) -> Vec<DependencyCall> {
        self.dependency_bytes = 0;
        core::mem::take(&mut self.dependency_calls)
    }
    pub fn state_value(&self) -> V {
        fn pairs(m: &[(V, V)]) -> V {
            V::Array(
                m.iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            )
        }
        obj(&[
            ("oracleList", V::Array(self.list_rows.clone())),
            ("oracleData", V::Array(self.data_rows.clone())),
            ("oracles", pairs(&self.oracles)),
            ("lastOracleUpdateBlock", pairs(&self.watermarks)),
        ])
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.encode_checkpoint(true)
    }
    pub fn durable_checkpoint(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.encode_checkpoint(false)
    }
    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::decode_checkpoint(bytes, true)
    }
    pub fn restore_durable(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::decode_checkpoint(bytes, false)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SourceRuleError> {
        self.checkpoint()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SourceRuleError> {
        Self::from_checkpoint(bytes)
    }
    pub fn encode_checkpoint(&self, complete: bool) -> Result<Vec<u8>, SourceRuleError> {
        validate_rows(&self.list_rows)?;
        validate_rows(&self.data_rows)?;
        let mut bytes = b"TLOS\x01".to_vec();
        bytes.push(u8::from(complete));
        put_rows(&mut bytes, &self.list_rows)?;
        put_rows(&mut bytes, &self.data_rows)?;
        if complete {
            put_map(&mut bytes, &self.oracles)?;
            put_map(&mut bytes, &self.watermarks)?;
            put_u32(&mut bytes, self.failures.len())?;
            for f in &self.failures {
                put_string(&mut bytes, &f.collection)?;
                put_string(&mut bytes, &f.operation)?;
                put_u32(&mut bytes, f.skip)?;
                put_string(&mut bytes, &f.exception.name)?;
                put_string(&mut bytes, &f.exception.message)?;
            }
        }
        if bytes.len() > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        with_crc(bytes).map_err(|_| SourceRuleError::Capacity)
    }
    pub fn decode_checkpoint(bytes: &[u8], complete: bool) -> Result<Self, SourceRuleError> {
        if bytes.len() > MAX_STATE_BYTES + 4 {
            return Err(SourceRuleError::Capacity);
        }
        let mut c = Cursor::new(verify_crc(bytes).map_err(|_| SourceRuleError::InvalidInput)?);
        if c.take(5).map_err(|_| SourceRuleError::InvalidInput)? != b"TLOS\x01"
            || c.u8().map_err(|_| SourceRuleError::InvalidInput)? != u8::from(complete)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut owner = Self::new();
        owner.list_rows = get_rows(&mut c)?;
        owner.data_rows = get_rows(&mut c)?;
        if complete {
            owner.oracles = get_map(&mut c)?;
            owner.watermarks = get_map(&mut c)?;
            let len = get_u32(&mut c)?;
            if len > MAX_ROWS {
                return Err(SourceRuleError::Capacity);
            }
            for _ in 0..len {
                let f = OracleDbFailure {
                    collection: get_string(&mut c)?,
                    operation: get_string(&mut c)?,
                    skip: get_u32(&mut c)?,
                    exception: SourceException {
                        name: get_string(&mut c)?,
                        message: get_string(&mut c)?,
                    },
                };
                if !matches!(f.collection.as_str(), "oracleList" | "oracleData")
                    || !matches!(
                        f.operation.as_str(),
                        "getDatabase"
                            | "findAsync"
                            | "findOneAsync"
                            | "insertAsync"
                            | "updateAsync"
                    )
                {
                    return Err(SourceRuleError::InvalidInput);
                }
                owner.failures.push(f);
            }
        }
        c.end().map_err(|_| SourceRuleError::InvalidInput)?;
        if owner.encode_checkpoint(complete)? != bytes {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(owner)
    }
    pub fn evaluate(&mut self, call: &SourceCall) -> Result<SourceOutcome, SourceRuleError> {
        self.evaluate_with_backend(call, &mut MissingInsurance)
    }
    pub fn evaluate_with_backend<B: SourceBackend>(
        &mut self,
        call: &SourceCall,
        backend: &mut B,
    ) -> Result<SourceOutcome, SourceRuleError> {
        if !METHODS.contains(&call.method.as_str()) {
            return Err(SourceRuleError::UnknownMethod);
        }
        if call.arguments.len() > 8 {
            return Err(SourceRuleError::Capacity);
        }
        for a in &call.arguments {
            a.validate()?;
        }
        let before = self.clone();
        let result = self.run(&call.method, &call.arguments, backend);
        if let Err(Fault::Local(e)) = &result {
            if *e != SourceRuleError::UnknownDependency {
                let e = e.clone();
                *self = before;
                return Err(e);
            }
        }
        if let Err(e) = self.checkpoint() {
            *self = before;
            return Err(e);
        }
        match result {
            Ok(v) => Ok(SourceOutcome::Returned(v)),
            Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
            Err(Fault::Local(e)) => Err(e),
        }
    }
    pub fn invoke_source(
        &mut self,
        method: &str,
        arguments: &[V],
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.invoke_source_with_backend(method, arguments, &mut MissingInsurance)
    }
    pub fn invoke_source_with_backend<B: SourceBackend>(
        &mut self,
        method: &str,
        arguments: &[V],
        backend: &mut B,
    ) -> Result<DependencyReturn, SourceRuleError> {
        Ok(
            match self.evaluate_with_backend(
                &SourceCall {
                    method: method.into(),
                    arguments: arguments.to_vec(),
                },
                backend,
            )? {
                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
            },
        )
    }
    pub fn replay(&mut self, calls: &[SourceCall]) -> Result<Vec<SourceOutcome>, SourceRuleError> {
        if calls.len() > MAX_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        calls.iter().map(|c| self.evaluate(c)).collect()
    }
    fn db_call(
        &mut self,
        collection: &str,
        operation: &str,
        query: V,
        update: V,
        options: V,
    ) -> Eval<()> {
        let call = OracleDbCall {
            collection: collection.into(),
            operation: operation.into(),
            query,
            update,
            options,
        };
        let bytes = value_size(&call.value());
        while !self.calls.is_empty()
            && (self.calls.len() >= MAX_ROWS * 16
                || self.calls_bytes.saturating_add(bytes) > MAX_STATE_BYTES)
        {
            self.calls_bytes = self
                .calls_bytes
                .saturating_sub(value_size(&self.calls.remove(0).value()));
        }
        self.calls_bytes = self.calls_bytes.saturating_add(bytes);
        self.calls.push(call);
        if let Some(i) = self
            .failures
            .iter()
            .position(|f| f.collection == collection && f.operation == operation)
        {
            if self.failures[i].skip == 0 {
                return Err(Fault::Thrown(self.failures.remove(i).exception));
            }
            self.failures[i].skip -= 1;
        }
        Ok(())
    }
    fn database(&mut self, collection: &str) -> Eval<()> {
        self.db_call(
            collection,
            "getDatabase",
            V::Undefined,
            V::Undefined,
            V::Undefined,
        )
    }
    fn find(&mut self, collection: &str, query: V, one: bool) -> Eval<V> {
        self.db_call(
            collection,
            if one { "findOneAsync" } else { "findAsync" },
            query.clone(),
            V::Undefined,
            V::Undefined,
        )?;
        let mut rows = vec![];
        for row in self.rows(collection)? {
            if matches_query(row, &query)? {
                rows.push(row.clone());
            }
        }
        Ok(if one {
            rows.into_iter().next().unwrap_or(V::Null)
        } else {
            V::Array(rows)
        })
    }
    fn insert(&mut self, doc: V) -> Eval<()> {
        self.db_call(
            "oracleList",
            "insertAsync",
            doc.clone(),
            V::Undefined,
            V::Undefined,
        )?;
        if self
            .list_rows
            .iter()
            .any(|r| strict(&r.get("_id"), &doc.get("_id")))
        {
            return Err(error("duplicate ID"));
        }
        if self.list_rows.len() == MAX_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        doc.validate()?;
        self.list_rows.push(doc);
        Ok(())
    }
    fn update(&mut self, collection: &str, key: V, update: V, set: bool, upsert: bool) -> Eval<()> {
        primitive_key(&key)?;
        let query = obj(&[("_id", key.clone())]);
        self.db_call(
            collection,
            "updateAsync",
            query.clone(),
            if set {
                obj(&[("$set", update.clone())])
            } else {
                update.clone()
            },
            if upsert {
                obj(&[("upsert", V::Bool(true))])
            } else {
                obj(&[])
            },
        )?;
        let rows = if collection == "oracleList" {
            &mut self.list_rows
        } else {
            &mut self.data_rows
        };
        let i = if let Some(i) = rows.iter().position(|r| strict(&r.get("_id"), &key)) {
            i
        } else if upsert {
            if rows.len() == MAX_ROWS {
                return Err(SourceRuleError::Capacity.into());
            }
            rows.len()
        } else {
            return Ok(());
        };
        let doc = if set {
            merged(rows.get(i).cloned().unwrap_or(query), &update)?
        } else {
            update
        };
        doc.validate()?;
        if i == rows.len() {
            rows.push(doc);
        } else {
            rows[i] = doc;
        }
        Ok(())
    }
    fn load(&mut self) -> Eval<V> {
        let result = (|| -> Eval<()> {
            self.database("oracleList")?;
            let rows = self.find("oracleList", obj(&[]), false)?;
            for row in array(&rows)? {
                map_set(&mut self.oracles, prop(row, "_id")?, row.clone())?;
            }
            Ok(())
        })();
        match result {
            Err(Fault::Local(e)) => Err(Fault::Local(e)),
            _ => Ok(V::Undefined),
        }
    }
    fn price(&mut self, id: V) -> Eval<V> {
        self.database("oracleData")?;
        let rows = self.find("oracleData", obj(&[("oracleId", id)]), false)?;
        let mut latest: Option<V> = None;
        for row in array(&rows)? {
            if !truth(row) {
                continue;
            }
            let data = prop(row, "data")?;
            if !truth(&data) {
                continue;
            }
            if !number(&prop(&data, "price")?)?.is_finite() {
                continue;
            }
            if match &latest {
                None => true,
                Some(old) => {
                    relational(&row.get("blockHeight"), &old.get("blockHeight"))?
                        == Some(core::cmp::Ordering::Greater)
                }
            } {
                latest = Some(row.clone());
            }
        }
        Ok(latest
            .map(|r| r.get("data").get("price"))
            .unwrap_or(V::number(1.0)))
    }
    fn next_id(&mut self) -> Eval<V> {
        self.database("oracleList")?;
        let rows = self.find("oracleList", obj(&[]), false)?;
        let mut max = 0.0;
        for row in array(&rows)? {
            let mut id = optional(row, "id")?;
            if matches!(id, V::Null | V::Undefined) {
                let key = text(&or(&[optional(row, "_id")?, s("")]))?;
                id = key.split('-').nth(1).map(s).unwrap_or(V::Undefined);
            }
            let id = number(&id)?;
            if id.is_finite() && id > max {
                max = id;
            }
        }
        if max > 0.0 {
            return Ok(V::number(max + 1.0));
        }
        for (key, _) in &self.oracles {
            let V::String(key) = key else {
                return Err(type_error("key.split is not a function"));
            };
            let id = parse_int(&key.split('-').nth(1).map(s).unwrap_or(V::Undefined))?;
            if id > max {
                max = id;
            }
        }
        Ok(V::number(max + 1.0))
    }
    fn save(&mut self, id: V, data: V, block: V) -> Eval<V> {
        self.database("oracleData")?;
        let key = s(&format!("oracle-{}-{}", text(&id)?, text(&block)?));
        self.update(
            "oracleData",
            key.clone(),
            obj(&[
                ("_id", key),
                ("type", s("oracle")),
                ("oracleId", id),
                ("data", data),
                ("blockHeight", block),
            ]),
            false,
            true,
        )?;
        Ok(V::Undefined)
    }
    fn twap(&mut self, id: V, block: V, trailing: V) -> Eval<V> {
        let result = (|| -> Eval<V> {
            self.database("oracleData")?;
            let rows = self.find(
                "oracleData",
                obj(&[
                    ("oracleId", id),
                    (
                        "blockHeight",
                        obj(&[
                            ("$gte", V::number(number(&block)? - number(&trailing)?)),
                            ("$lte", block),
                        ]),
                    ),
                ]),
                false,
            )?;
            if array(&rows)?.is_empty() {
                return Ok(V::Null);
            }
            let mut sum = Bn::new(&V::number(0.0))?;
            for row in array(&rows)? {
                let p = Bn::new(&prop(&prop(row, "data")?, "price")?)?;
                sum = sum.op(p, 0)?;
            }
            Ok(V::number(
                sum.op(Bn::new(&V::number(array(&rows)?.len() as f64))?, 3)?
                    .q(8, Rounding::HalfUp)?
                    .number(),
            ))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Null),
            other => other,
        }
    }
    fn run<B: SourceBackend>(&mut self, method: &str, args: &[V], backend: &mut B) -> Eval<V> {
        let a = |i| arg(args, i);
        let id = a(0);
        let key = || -> Eval<V> { Ok(s(&format!("oracle-{}", text(&id)?))) };
        match method {
            "load" => self.load(),
            "getAllOracles" => {
                self.load()?;
                Ok(V::Array(
                    self.oracles.iter().map(|(_, v)| v.clone()).collect(),
                ))
            }
            "getNextId" => self.next_id(),
            "getOraclePrice" => self.price(id),
            "getTWAP" => self.twap(id, a(1), a(2)),
            "getOracleInfo" => {
                if self.oracles.is_empty() {
                    self.load()?;
                }
                let key = key()?;
                if let Some(value) = map_get(&self.oracles, &key).filter(|v| truth(v)) {
                    return Ok(value.clone());
                }
                self.database("oracleList")?;
                self.find("oracleList", obj(&[("_id", key)]), true)
            }
            "addOracle" => {
                map_set(&mut self.oracles, id.clone(), a(1))?;
                self.database("oracleList")?;
                let doc = merged(obj(&[("_id", id)]), &a(1))?;
                self.insert(doc)?;
                Ok(V::Bool(true))
            }
            "createOracle" => {
                let id = self.next_id()?;
                let key = s(&format!("oracle-{}", text(&id)?));
                let config = if truth(&a(0)) && matches!(a(0), V::Object(_) | V::Array(_)) {
                    a(0)
                } else {
                    obj(&[("name", a(0)), ("adminAddress", a(1))])
                };
                let display = or(&[
                    prop(&config, "name")?,
                    prop(&config, "ticker")?,
                    key.clone(),
                ]);
                let lag = number(&prop(&config, "lag")?)?;
                let lists = prop(&config, "clearlists")?;
                let doc = obj(&[
                    ("_id", key.clone()),
                    ("id", id.clone()),
                    ("name", display.clone()),
                    ("ticker", or(&[prop(&config, "ticker")?, display])),
                    ("url", or(&[prop(&config, "url")?, s("")])),
                    (
                        "backupAddress",
                        or(&[prop(&config, "backupAddress")?, s("")]),
                    ),
                    (
                        "clearlists",
                        if matches!(lists, V::Array(_)) {
                            lists
                        } else {
                            V::Array(vec![])
                        },
                    ),
                    ("lag", V::number(if lag.is_finite() { lag } else { 1.0 })),
                    (
                        "adminAddress",
                        or(&[prop(&config, "adminAddress")?, a(1), s("")]),
                    ),
                    ("data", obj(&[])),
                ]);
                self.database("oracleList")?;
                self.insert(doc.clone())?;
                map_set(&mut self.oracles, key, doc)?;
                Ok(id)
            }
            "isAdmin" => {
                let key = s(&format!("oracle-{}", text(&a(1))?));
                self.database("oracleList")?;
                let row = self.find("oracleList", obj(&[("_id", key)]), true)?;
                let name = optional(&row, "name")?;
                let admin = or(&[
                    optional(&row, "adminAddress")?,
                    optional(&name, "adminAddress")?,
                ]);
                let backup = or(&[
                    optional(&row, "backupAddress")?,
                    optional(&name, "backupAddress")?,
                ]);
                Ok(V::Bool(strict(&admin, &a(0)) || strict(&backup, &a(0))))
            }
            "verifyAdmin" => {
                let key = key()?;
                let row = if let Some(row) = map_get(&self.oracles, &key).filter(|v| truth(v)) {
                    row.clone()
                } else {
                    self.database("oracleList")?;
                    self.find("oracleList", obj(&[("_id", key)]), true)?
                };
                if !truth(&row) {
                    Ok(row)
                } else {
                    Ok(V::Bool(strict(&prop(&row, "adminAddress")?, &a(1))))
                }
            }
            "updateAdmin" => {
                let key = key()?;
                self.database("oracleList")?;
                let mut row = self.find("oracleList", obj(&[("_id", key.clone())]), true)?;
                if !truth(&row) {
                    return Err(error("Oracle not found"));
                }
                let field = if truth(&a(2)) {
                    "backupAddress"
                } else {
                    "adminAddress"
                };
                set_prop(&mut row, field, a(1))?;
                self.update(
                    "oracleList",
                    key.clone(),
                    obj(&[(field, a(1))]),
                    true,
                    false,
                )?;
                map_set(&mut self.oracles, key, row)?;
                Ok(V::Undefined)
            }
            "saveOracleData" => self.save(id, a(1), a(2)),
            "loadOracleData" => {
                self.database("oracleData")?;
                let start = if matches!(a(1), V::Undefined) {
                    V::number(0.0)
                } else {
                    a(1)
                };
                let end = if matches!(a(2), V::Undefined) {
                    V::number(9_007_199_254_740_991.0)
                } else {
                    a(2)
                };
                let rows = self.find(
                    "oracleData",
                    obj(&[
                        ("oracleId", id),
                        ("type", s("oracle")),
                        ("blockHeight", obj(&[("$gte", start), ("$lte", end)])),
                    ]),
                    false,
                )?;
                Ok(V::Array(
                    array(&rows)?
                        .iter()
                        .map(|r| {
                            obj(&[
                                ("blockHeight", r.get("blockHeight")),
                                ("data", r.get("data")),
                            ])
                        })
                        .collect(),
                ))
            }
            "publishData" => {
                let block = a(5);
                if let Some(last) = map_get(&self.watermarks, &id) {
                    if !matches!(last, V::Undefined)
                        && relational(last, &block)?.is_some_and(|o| o != core::cmp::Ordering::Less)
                    {
                        return Ok(V::Undefined);
                    }
                }
                map_set(&mut self.watermarks, id.clone(), block.clone())?;
                let mut data = obj(&[
                    ("price", a(1)),
                    ("high", a(2)),
                    ("low", a(3)),
                    ("close", a(4)),
                ]);
                let last = self.price(id.clone())?;
                // Constructed priceBN is unused in source but its domain still matters.
                Bn::new(&a(1))?;
                let up = Bn::new(&V::number(1.05))?
                    .op(Bn::new(&last)?, 2)?
                    .q(4, Rounding::HalfUp)?
                    .number();
                let down = Bn::new(&V::number(0.95))?
                    .op(Bn::new(&last)?, 2)?
                    .q(4, Rounding::HalfUp)?
                    .number();
                if number(&last)? != 1.0 {
                    if number(&a(1))? > up {
                        set_prop(&mut data, "price", V::number(up))?;
                    } else if number(&a(1))? < down {
                        set_prop(&mut data, "price", V::number(down))?;
                    }
                }
                let key = key()?;
                let mut metadata = map_get(&self.oracles, &key)
                    .filter(|v| truth(v))
                    .cloned()
                    .unwrap_or(V::Null);
                if !truth(&metadata) {
                    self.database("oracleList")?;
                    metadata = self.find("oracleList", obj(&[("_id", key.clone())]), true)?;
                }
                if !truth(&metadata) {
                    metadata = obj(&[("_id", key.clone()), ("id", V::number(number(&id)?))]);
                }
                let meta = merged(
                    metadata,
                    &obj(&[
                        ("data", data.clone()),
                        ("lastPublishedBlock", block.clone()),
                    ]),
                )?;
                map_set(&mut self.oracles, key, meta)?;
                self.save(id, data, block)
            }
            "recordStake" => {
                self.database("oracleData")?;
                let key = s(&format!("oracle-stake-{}-{}", text(&id)?, text(&a(1))?));
                let prev = self.find("oracleData", obj(&[("_id", key.clone())]), true)?;
                let amount = Bn::new(&V::number(number(&or(&[
                    optional(&prev, "amount")?,
                    V::number(0.0),
                ]))?))?
                .op(Bn::new(&a(3))?, 0)?
                .q(8, Rounding::Down)?
                .number();
                let doc = obj(&[
                    ("_id", key.clone()),
                    ("type", s("stake")),
                    ("oracleId", id),
                    ("stakerAddress", a(1)),
                    ("stakedPropertyId", a(2)),
                    ("amount", V::number(amount)),
                    ("blockHeight", a(4)),
                ]);
                self.update("oracleData", key, doc.clone(), true, true)?;
                Ok(doc)
            }
            "getStake" => {
                self.database("oracleData")?;
                self.find(
                    "oracleData",
                    obj(&[(
                        "_id",
                        s(&format!("oracle-stake-{}-{}", text(&id)?, text(&a(1))?)),
                    )]),
                    true,
                )
            }
            "applyFraudProof" => {
                self.database("oracleData")?;
                let key = s(&format!("oracle-stake-{}-{}", text(&id)?, text(&a(1))?));
                let prev = self.find("oracleData", obj(&[("_id", key.clone())]), true)?;
                if !truth(&prev) {
                    return Ok(obj(&[("slashed", V::number(0.0))]));
                }
                let current = number(&or(&[prop(&prev, "amount")?, V::number(0.0)]))?;
                let wanted = number(&or(&[a(3), V::number(0.0)]))?;
                let slash = if current.is_nan() || wanted.is_nan() {
                    f64::NAN
                } else if current == 0.0 && wanted == 0.0 {
                    if current.is_sign_negative() || wanted.is_sign_negative() {
                        -0.0
                    } else {
                        0.0
                    }
                } else {
                    current.min(wanted)
                };
                let next = Bn::new(&V::number(current))?
                    .op(Bn::new(&V::number(slash))?, 1)?
                    .q(8, Rounding::Down)?
                    .number();
                self.update(
                    "oracleData",
                    key,
                    obj(&[("amount", V::number(next)), ("blockHeight", a(5))]),
                    true,
                    true,
                )?;
                let evidence = text(&or(&[a(4), s("")]))?;
                let prefix =
                    String::from_utf16(&evidence.encode_utf16().take(24).collect::<Vec<_>>())
                        .map_err(|_| SourceRuleError::InvalidInput)?;
                let key = s(&format!(
                    "oracle-fraud-{}-{}-{prefix}",
                    text(&id)?,
                    text(&a(5))?
                ));
                self.update(
                    "oracleData",
                    key.clone(),
                    obj(&[
                        ("_id", key),
                        ("type", s("fraudProof")),
                        ("oracleId", id),
                        ("accusedAddress", a(1)),
                        ("challengerAddress", a(2)),
                        ("slashAmount", V::number(slash)),
                        ("evidenceHash", a(4)),
                        ("blockHeight", a(5)),
                    ]),
                    true,
                    true,
                )?;
                Ok(obj(&[("slashed", V::number(slash))]))
            }
            "relayTradeLayerState" => {
                self.database("oracleData")?;
                let blob = if matches!(a(6), V::Undefined) {
                    s("")
                } else {
                    a(6)
                };
                let parsed = match text(&blob) {
                    Ok(t) => match crate::dlc_oracle_bridge::source_parse_relay_blob(
                        &crate::issuance_intent::JsonValue::String(t),
                    ) {
                        Ok(Some(v)) => V::from_json(&v)?,
                        Ok(None) => V::Null,
                        Err(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
                    },
                    Err(Fault::Thrown(_)) => V::Null,
                    Err(e) => return Err(e),
                };
                let signature = text(&or(&[optional(&parsed, "signatureHex")?, s("")]))?
                    .trim_matches(js_space)
                    .to_lowercase();
                let effective_state = text(&or(&[a(3), optional(&parsed, "stateHash")?, s("")]))?;
                let effective_dlc = text(&or(&[a(4), s("")]))?;
                if !signature.is_empty() {
                    let digest = sha256(signature.as_bytes())
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    let sigkey = s(&format!("oracle-relay-sig-{}-{digest}", text(&id)?));
                    let prev = self.find("oracleData", obj(&[("_id", sigkey.clone())]), true)?;
                    if truth(&prev) {
                        if text(&or(&[prop(&prev, "stateHash")?, s("")]))? != effective_state
                            || text(&or(&[prop(&prev, "dlcRef")?, s("")]))? != effective_dlc
                        {
                            return Err(error(
                                "Relay signature replay detected for different stateHash/dlcRef",
                            ));
                        }
                    } else {
                        self.update(
                            "oracleData",
                            sigkey.clone(),
                            obj(&[
                                ("_id", sigkey),
                                ("type", s("relaySigUse")),
                                ("oracleId", id.clone()),
                                ("signatureHex", s(&signature)),
                                ("stateHash", s(&effective_state)),
                                ("dlcRef", s(&effective_dlc)),
                                ("firstSeenBlock", a(5)),
                            ]),
                            false,
                            true,
                        )?;
                    }
                }
                let key = s(&format!(
                    "oracle-relay-{}-{}-{}",
                    text(&id)?,
                    text(&a(2))?,
                    text(&a(5))?
                ));
                let doc = obj(&[
                    ("_id", key.clone()),
                    ("type", s("relay")),
                    ("oracleId", id),
                    ("senderAddress", a(1)),
                    ("relayType", a(2)),
                    ("stateHash", a(3)),
                    ("dlcRef", a(4)),
                    ("relayBlob", blob),
                    ("blockHeight", a(5)),
                ]);
                self.update("oracleData", key, doc.clone(), false, true)?;
                Ok(doc)
            }
            "closeOracle" => {
                let key = key()?;
                self.database("oracleList")?;
                let mut row = self.find("oracleList", obj(&[("_id", key.clone())]), true)?;
                if !truth(&row) {
                    return Err(error("Oracle not found"));
                }
                set_prop(&mut row, "closed", V::Bool(true))?;
                self.update(
                    "oracleList",
                    key.clone(),
                    obj(&[("closed", V::Bool(true))]),
                    true,
                    false,
                )?;
                map_set(&mut self.oracles, key, row.clone())?;
                let call = DependencyCall {
                    target: "insurance.liquidate".into(),
                    arguments: vec![prop(&row, "adminAddress")?, V::Bool(true)],
                };
                let bytes =
                    value_size(&V::Array(call.arguments.clone())).saturating_add(call.target.len());
                while !self.dependency_calls.is_empty()
                    && (self.dependency_calls.len() >= MAX_ROWS * 16
                        || self.dependency_bytes.saturating_add(bytes) > MAX_STATE_BYTES)
                {
                    let old = self.dependency_calls.remove(0);
                    self.dependency_bytes = self.dependency_bytes.saturating_sub(
                        value_size(&V::Array(old.arguments)).saturating_add(old.target.len()),
                    );
                }
                self.dependency_bytes = self.dependency_bytes.saturating_add(bytes);
                self.dependency_calls.push(call.clone());
                match backend.invoke(&call)? {
                    DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                        Ok(V::Undefined)
                    }
                    DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                        Err(Fault::Thrown(e))
                    }
                }
            }
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
}
impl SourceBackend for OracleSourceOwner {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let method = call
            .target
            .strip_prefix("Oracle.")
            .or_else(|| call.target.strip_prefix("oracle."))
            .ok_or(SourceRuleError::UnknownDependency)?;
        self.invoke_source(method, &call.arguments)
    }
}
