//! Complete bounded source Channels metadata over explicit external capabilities.
//! This is not a Ledger, signed revision owner, or authenticated channel adapter.
use crate::decimal::{Decimal, Rounding};
use crate::issuance_intent::{JsonNumber, JsonValue};
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue,
};
use core::str::FromStr;

pub const SOURCE_SHA256: &str = "b3fa22beab9a9fdb63246daa12582f86591a4217863240321f88fdff0160ec79";
pub const MAX_ACTIONS: usize = 4096;
pub const MAX_CHANNELS: usize = 1024;
pub const MAX_RECORDS: usize = 16384;
pub const MAX_DEPENDENCIES: usize = 4096;
pub const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TRACE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TRACE_NODES: usize = 65_536;
pub const MAX_TIE_SCAN: usize = 4096;
pub const METHODS: [&str; 43] = [
    "addToRegistry",
    "removeFromRegistry",
    "setChannel",
    "saveChannelsRegistry",
    "getChannelBalancesForAddress",
    "sumBalance",
    "toChannelPropMap",
    "loadChannelsRegistry",
    "savePendingWithdrawalToDB",
    "loadPendingWithdrawalsFromDB",
    "removePendingWithdrawalFromDB",
    "recordChannelDelta",
    "recordTokenTrade",
    "recordContractTrade",
    "saveTrade",
    "getChannel",
    "isValidChannel",
    "getCommitAddresses",
    "addCommitment",
    "getCommitments",
    "compareCharacters",
    "recordParticipantChange",
    "debitInitMarginFromChannel",
    "assignColumnBasedOnAddress",
    "resolveColumn",
    "assignColumnBasedOnLastCharacter",
    "tieBreakerByBackChar",
    "predictColumnForAddress",
    "bumpColumnAssignment",
    "processChannelCommits",
    "handleChannelPubkey",
    "recordCommitToChannel",
    "areBothPartiesCommitted",
    "addToWithdrawalQueue",
    "processWithdrawals",
    "removeEmptyChannels",
    "isChannelEmpty",
    "removeChannelFromDB",
    "adjustChannelBalances",
    "processWithdrawal",
    "processWithdrawAll",
    "processTransfer",
    "updateChannelWithColumnAssignments",
];
type V = SourceValue;
type Result<T> = core::result::Result<T, SourceRuleError>;
type Eval<T> = core::result::Result<T, Fault>;
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
fn thrown(name: &str, message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: name.into(),
        message: message.into(),
    })
}
fn error(message: impl Into<String>) -> Fault {
    thrown("Error", message)
}
fn type_error(message: impl Into<String>) -> Fault {
    thrown("TypeError", message)
}
fn strv(s: impl Into<String>) -> V {
    V::String(s.into())
}
fn obj(fields: Vec<(&str, V)>) -> V {
    V::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(v) => *v,
        V::Number(v) => {
            let n = f64::from_bits(*v);
            n != 0.0 && !n.is_nan()
        }
        V::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn nullish(v: &V) -> bool {
    matches!(v, V::Undefined | V::Null)
}
fn primitive_equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise, _)
        | (_, V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise) => false,
        _ => a == b,
    }
}
fn same_key(a: &V, b: &V) -> bool {
    primitive_equal(a, b)
        || matches!((a,b),(V::Number(x),V::Number(y))if f64::from_bits(*x).is_nan()&&f64::from_bits(*y).is_nan())
}
fn canonical_map_key(key: V) -> V {
    match key {
        V::Number(bits) if f64::from_bits(bits) == 0.0 => V::number(0.0),
        other => other,
    }
}
fn inherited_object_key(key: &str) -> bool {
    matches!(
        key,
        "__proto__"
            | "constructor"
            | "toString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "toLocaleString"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    )
}
fn reject_own_coercion(value: &V) -> Eval<()> {
    if matches!(value, V::Object(fields) if fields.iter().any(|(key, _)| matches!(key.as_str(), "toString" | "valueOf")))
    {
        return Err(SourceRuleError::InvalidInput.into());
    }
    Ok(())
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn number_text(n: f64) -> Result<String> {
    if n.is_nan() {
        return Ok("NaN".into());
    }
    if n == f64::INFINITY {
        return Ok("Infinity".into());
    }
    if n == f64::NEG_INFINITY {
        return Ok("-Infinity".into());
    }
    JsonValue::Number(JsonNumber::from_f64(n).map_err(|_| SourceRuleError::InvalidInput)?)
        .to_json()
        .map_err(|_| SourceRuleError::InvalidInput)
}
fn text(v: &V) -> Eval<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        V::String(v) | V::BigNumber(v) => v.clone(),
        V::Number(v) => number_text(f64::from_bits(*v))?,
        V::Promise => "[object Promise]".into(),
        V::Array(a) => {
            let mut out = Vec::new();
            for v in a {
                out.push(if nullish(v) { String::new() } else { text(v)? })
            }
            out.join(",")
        }
        V::Object(a) => {
            if a.iter()
                .any(|(k, _)| matches!(k.as_str(), "toString" | "valueOf"))
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
            "[object Object]".into()
        }
    })
}
fn radix_decimal(s: &str, radix: u32) -> Eval<Decimal> {
    if s.len() > 1024 {
        return Err(SourceRuleError::Capacity.into());
    }
    let mut n = Decimal::zero();
    for c in s.chars() {
        let Some(d) = c.to_digit(radix) else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        n = n
            .checked_mul(&Decimal::from(radix))
            .and_then(|n| n.checked_add(&Decimal::from(d)))
            .map_err(|_| SourceRuleError::Capacity)?
    }
    Ok(n)
}
fn number(v: &V) -> Eval<f64> {
    Ok(match v {
        V::Undefined | V::Promise => f64::NAN,
        V::Null => 0.0,
        V::Bool(v) => f64::from(u8::from(*v)),
        V::Number(v) => f64::from_bits(*v),
        _ => {
            let s = text(v)?;
            let s = s.trim_matches(js_space);
            if s.is_empty() {
                0.0
            } else if s == "Infinity" || s == "+Infinity" {
                f64::INFINITY
            } else if s == "-Infinity" {
                f64::NEG_INFINITY
            } else if let Some((rest, radix)) = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .map(|s| (s, 16))
                .or_else(|| {
                    s.strip_prefix("0o")
                        .or_else(|| s.strip_prefix("0O"))
                        .map(|s| (s, 8))
                })
                .or_else(|| {
                    s.strip_prefix("0b")
                        .or_else(|| s.strip_prefix("0B"))
                        .map(|s| (s, 2))
                })
            {
                if rest.is_empty() || !rest.chars().all(|c| c.is_digit(radix)) {
                    f64::NAN
                } else {
                    radix_decimal(rest, radix)?
                        .to_string()
                        .parse()
                        .unwrap_or(f64::INFINITY)
                }
            } else if s
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
            {
                s.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
    })
}
fn add(a: &V, b: &V) -> Eval<V> {
    if matches!(
        a,
        V::String(_) | V::Array(_) | V::Object(_) | V::BigNumber(_) | V::Promise
    ) || matches!(
        b,
        V::String(_) | V::Array(_) | V::Object(_) | V::BigNumber(_) | V::Promise
    ) {
        Ok(strv(format!("{}{}", text(a)?, text(b)?)))
    } else {
        Ok(V::number(number(a)? + number(b)?))
    }
}
fn subtract(a: &V, b: &V) -> Eval<V> {
    Ok(V::number(number(a)? - number(b)?))
}
fn loose_equal(a: &V, b: &V) -> Eval<bool> {
    if primitive_equal(a, b) {
        return Ok(true);
    }
    if nullish(a) && nullish(b) {
        return Ok(true);
    }
    if nullish(a) || nullish(b) {
        return Ok(false);
    }
    if matches!(
        (a, b),
        (V::Number(_), V::String(_))
            | (V::String(_), V::Number(_))
            | (V::Bool(_), _)
            | (_, V::Bool(_))
    ) {
        return Ok(number(a)? == number(b)?);
    }
    let object_like =
        |v: &V| matches!(v, V::Array(_) | V::Object(_) | V::BigNumber(_) | V::Promise);
    if object_like(a) && matches!(b, V::Number(_) | V::String(_)) {
        return loose_equal(&strv(text(a)?), b);
    }
    if object_like(b) && matches!(a, V::Number(_) | V::String(_)) {
        return loose_equal(a, &strv(text(b)?));
    }
    Ok(false)
}
fn index_key(s: &str) -> Option<u32> {
    s.parse::<u32>()
        .ok()
        .filter(|n| *n != u32::MAX && n.to_string() == s)
}
fn ordered(fields: &[(String, V)]) -> Vec<(String, V)> {
    let mut nums: Vec<_> = fields
        .iter()
        .filter(|(k, _)| index_key(k).is_some())
        .cloned()
        .collect();
    nums.sort_by_key(|(k, _)| index_key(k));
    nums.extend(
        fields
            .iter()
            .filter(|(k, _)| index_key(k).is_none())
            .cloned(),
    );
    nums
}
fn get(v: &V, key: &str) -> Eval<V> {
    if !nullish(v)
        && inherited_object_key(key)
        && !matches!(v, V::Object(fields) if fields.iter().any(|(name, _)| name == key))
    {
        // Inherited functions/prototypes need a reference graph, not an
        // invented undefined value or ordinary copied property object.
        return Err(SourceRuleError::InvalidInput.into());
    }
    match v {
        V::Array(a) => Ok(if key == "length" {
            V::number(a.len() as f64)
        } else {
            index_key(key)
                .and_then(|i| a.get(i as usize))
                .cloned()
                .unwrap_or(V::Undefined)
        }),
        V::Null | V::Undefined => Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            if matches!(v, V::Null) {
                "null"
            } else {
                "undefined"
            }
        ))),
        V::String(s) => {
            if key == "length" {
                Ok(V::number(s.encode_utf16().count() as f64))
            } else if let Some(i) = index_key(key) {
                let out = s
                    .encode_utf16()
                    .nth(i as usize)
                    .map(|c| char::from_u32(u32::from(c)).ok_or(SourceRuleError::InvalidInput))
                    .transpose()?;
                Ok(out.map(|c| strv(c.to_string())).unwrap_or(V::Undefined))
            } else {
                Ok(V::Undefined)
            }
        }
        _ => Ok(v.get(key)),
    }
}
fn optional(v: &V, key: &str) -> Eval<V> {
    if nullish(v) {
        Ok(V::Undefined)
    } else {
        get(v, key)
    }
}
fn set(v: &mut V, key: &str, value: V) -> Eval<()> {
    match v {
        V::Object(fields) => {
            if key == "__proto__" {
                return Err(SourceRuleError::InvalidInput.into());
            }
            if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
                *v = value
            } else {
                fields.push((key.into(), value))
            }
            *fields = ordered(fields);
            Ok(())
        }
        V::Array(a) => {
            let at = index_key(key).ok_or(SourceRuleError::InvalidInput)? as usize;
            if at > a.len() {
                return Err(SourceRuleError::InvalidInput.into());
            }
            if at >= 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            if at == a.len() {
                a.push(value)
            } else {
                a[at] = value
            }
            Ok(())
        }
        V::Null | V::Undefined => Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            if matches!(v, V::Null) {
                "null"
            } else {
                "undefined"
            }
        ))),
        V::String(s)
            if key == "length"
                || index_key(key).is_some_and(|i| (i as usize) < s.encode_utf16().count()) =>
        {
            Err(type_error(format!(
                "Cannot assign to read only property '{key}' of string '{s}'"
            )))
        }
        _ => Err(type_error(format!(
            "Cannot create property '{key}' on {} '{}'",
            match v {
                V::String(_) => "string",
                V::Bool(_) => "boolean",
                _ => "number",
            },
            text(v)?
        ))),
    }
}
fn set_path(v: &mut V, path: &[&str], value: V) -> Eval<()> {
    if path.len() == 1 {
        return set(v, path[0], value);
    }
    let mut child = get(v, path[0])?;
    let result = set_path(&mut child, &path[1..], value);
    if let V::Object(fields) = v {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == path[0]) {
            *v = child
        }
    } else if let V::Array(values) = v {
        if let Some(slot) = index_key(path[0]).and_then(|i| values.get_mut(i as usize)) {
            *slot = child;
        }
    }
    result
}
fn array(v: &V, context: &str) -> Eval<Vec<V>> {
    if nullish(v) && context.contains(".map") {
        get(v, "map")?;
    }
    match v {
        V::Array(a) => Ok(a.clone()),
        _ => Err(type_error(context)),
    }
}
fn iterable(v: &V, context: &str) -> Eval<Vec<V>> {
    if let V::String(s) = v {
        if s.chars().count() > MAX_RECORDS {
            return Err(SourceRuleError::Capacity.into());
        }
        Ok(s.chars().map(|c| strv(c.to_string())).collect())
    } else {
        array(v, context)
    }
}
fn entries(v: &V) -> Eval<Vec<(String, V)>> {
    match v {
        V::Null | V::Undefined => Err(type_error("Cannot convert undefined or null to object")),
        V::Object(a) => Ok(ordered(a)),
        V::Array(a) => Ok(a
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect()),
        V::String(s) => {
            if s.encode_utf16().count() > MAX_RECORDS {
                return Err(SourceRuleError::Capacity.into());
            }
            s.encode_utf16()
                .enumerate()
                .map(|(i, c)| {
                    let c = char::from_u32(u32::from(c)).ok_or(SourceRuleError::InvalidInput)?;
                    Ok((i.to_string(), strv(c.to_string())))
                })
                .collect()
        }
        _ => Ok(vec![]),
    }
}
fn fallback(v: V, other: V) -> V {
    if truth(&v) {
        v
    } else {
        other
    }
}
fn strarg(v: &V, method: &str) -> Eval<String> {
    if let V::String(s) = v {
        Ok(s.clone())
    } else {
        Err(type_error(format!("{method} is not a function")))
    }
}
fn push(v: &mut V, value: V) -> Eval<()> {
    if nullish(v) {
        get(v, "push")?;
    }
    if let V::Array(a) = v {
        if a.len() >= MAX_RECORDS {
            return Err(SourceRuleError::Capacity.into());
        }
        a.push(value);
        Ok(())
    } else {
        Err(type_error("channel.commits.push is not a function"))
    }
}
enum Bn {
    Finite(Decimal, bool),
    Nonfinite(f64),
}
fn bn(v: &V) -> Eval<Bn> {
    let s = match v {
        V::Number(n) => {
            let n = f64::from_bits(*n);
            if !n.is_finite() {
                return Ok(Bn::Nonfinite(n));
            }
            if n == 0.0 {
                return Ok(Bn::Finite(Decimal::zero(), n.is_sign_negative()));
            }
            number_text(n)?
        }
        V::String(s) | V::BigNumber(s) => s.trim_matches(js_space).into(),
        V::Array(_) => text(v)?.trim_matches(js_space).into(),
        V::Object(fields)
            if fields
                .iter()
                .any(|(key, _)| matches!(key.as_str(), "toString" | "valueOf")) =>
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        _ => return Ok(Bn::Nonfinite(f64::NAN)),
    };
    if s == "Infinity" || s == "+Infinity" {
        return Ok(Bn::Nonfinite(f64::INFINITY));
    }
    if s == "-Infinity" {
        return Ok(Bn::Nonfinite(f64::NEG_INFINITY));
    }
    let (negative, body) = if let Some(s) = s.strip_prefix('-') {
        (true, s)
    } else {
        (false, s.strip_prefix('+').unwrap_or(&s))
    };
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(rest) = body.strip_prefix(prefix) {
            if rest.len() > 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            let lower = rest.to_ascii_lowercase();
            if rest.is_empty()
                || rest.starts_with('.')
                || rest.matches('.').count() > 1
                || !(rest == lower || rest == rest.to_ascii_uppercase())
                || !lower
                    .chars()
                    .all(|c| c == '.' || c.is_ascii() && c.is_digit(radix))
            {
                return Ok(Bn::Nonfinite(f64::NAN));
            }
            let (integer, fraction) = lower.split_once('.').unwrap_or((&lower, ""));
            let mut n = radix_decimal(integer, radix)?;
            if !fraction.is_empty() {
                let mut digits: Vec<u32> = fraction
                    .chars()
                    .map(|c| c.to_digit(radix).expect("validated digits"))
                    .collect();
                let mut decimal = String::with_capacity(23);
                decimal.push_str("0.");
                for _ in 0..21 {
                    let mut carry = 0;
                    for digit in digits.iter_mut().rev() {
                        let value = *digit * 10 + carry;
                        *digit = value % radix;
                        carry = value / radix;
                    }
                    decimal.push(char::from(b'0' + carry as u8));
                }
                let fractional = Decimal::from_str(&decimal)
                    .map_err(|_| SourceRuleError::Capacity)?
                    .quantize(20, Rounding::HalfUp)
                    .map_err(|_| SourceRuleError::Capacity)?;
                n = n
                    .checked_add(&fractional)
                    .map_err(|_| SourceRuleError::Capacity)?;
            }
            return Ok(Bn::Finite(if negative { n.negated() } else { n }, negative));
        }
    }
    match Decimal::from_str(&s) {
        Ok(n) => Ok(Bn::Finite(n, negative)),
        Err(crate::decimal::DecimalError::LimitExceeded) => Err(SourceRuleError::Capacity.into()),
        Err(_) => Ok(Bn::Nonfinite(f64::NAN)),
    }
}
fn rounded_bn(n: Decimal, negative_zero: bool) -> Eval<f64> {
    let negative = n.is_negative() || (n.is_zero() && negative_zero);
    let q = n
        .quantize(8, Rounding::HalfUp)
        .map_err(|_| SourceRuleError::Capacity)?;
    let value = q
        .to_string()
        .parse::<f64>()
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    Ok(if value == 0.0 && negative {
        -0.0
    } else {
        value
    })
}
fn rounded_value(v: &V) -> Eval<f64> {
    match bn(v)? {
        Bn::Finite(n, z) => rounded_bn(n, z),
        Bn::Nonfinite(n) => Ok(n),
    }
}
fn rounded_sum(a: &V, b: &V, minus: bool) -> Eval<f64> {
    match (bn(a)?, bn(b)?) {
        (Bn::Finite(a, az), Bn::Finite(b, bz)) => {
            let bothzero = a.is_zero() && b.is_zero();
            let n = if minus {
                a.checked_sub(&b)
            } else {
                a.checked_add(&b)
            }
            .map_err(|_| SourceRuleError::Capacity)?;
            let negative_zero = bothzero
                && if minus {
                    (-0.0f64).copysign(if az { -1.0 } else { 1.0 })
                        - (-0.0f64).copysign(if bz { -1.0 } else { 1.0 })
                } else {
                    (-0.0f64).copysign(if az { -1.0 } else { 1.0 })
                        + (-0.0f64).copysign(if bz { -1.0 } else { 1.0 })
                }
                .is_sign_negative();
            rounded_bn(n, negative_zero)
        }
        (Bn::Nonfinite(a), Bn::Nonfinite(b)) => Ok(if minus { a - b } else { a + b }),
        (Bn::Nonfinite(a), _) => Ok(a),
        (_, Bn::Nonfinite(b)) => Ok(if minus { -b } else { b }),
    }
}
fn bn_lt(a: &V, b: &V) -> Eval<bool> {
    Ok(match (bn(a)?, bn(b)?) {
        (Bn::Finite(a, _), Bn::Finite(b, _)) => a < b,
        (Bn::Nonfinite(a), Bn::Nonfinite(b)) => a < b,
        (Bn::Nonfinite(a), _) => a == f64::NEG_INFINITY,
        (_, Bn::Nonfinite(b)) => b == f64::INFINITY,
    })
}
pub(crate) fn source_big_number_text(v: &V) -> Result<String> {
    bn_text(v).map_err(|fault| match fault {
        Fault::Local(e) => e,
        Fault::Thrown(_) => SourceRuleError::UnsupportedNumericDomain,
    })
}
fn bn_text(v: &V) -> Eval<String> {
    match bn(v)? {
        Bn::Nonfinite(n) => Ok(number_text(n)?),
        Bn::Finite(n, _) => {
            let s = n.to_string();
            let raw = s.strip_prefix('-').unwrap_or(&s);
            let integer = raw.find('.').unwrap_or(raw.len());
            let digits = raw.replace('.', "");
            let Some(first) = digits.find(|c: char| c != '0') else {
                return Ok("0".into());
            };
            let exponent = integer as i32 - first as i32 - 1;
            if !(-6..21).contains(&exponent) {
                let significant = digits[first..].trim_end_matches('0');
                let coefficient = if significant.len() == 1 {
                    significant.into()
                } else {
                    format!("{}.{}", &significant[..1], &significant[1..])
                };
                Ok(format!(
                    "{}{coefficient}e{}{exponent}",
                    if n.is_negative() { "-" } else { "" },
                    if exponent >= 0 { "+" } else { "" }
                ))
            } else {
                Ok(s)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChannelsState {
    pub registry: Vec<(V, V)>,
    pub pending_withdrawals: Vec<V>,
}
impl ChannelsState {
    pub fn diagnostic_value(&self) -> V {
        obj(vec![
            (
                "registry",
                V::Array(
                    self.registry
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "pendingWithdrawals",
                V::Array(self.pending_withdrawals.clone()),
            ),
        ])
    }
    fn validate(&self) -> Result<()> {
        if self.registry.len() > MAX_CHANNELS || self.pending_withdrawals.len() > MAX_RECORDS {
            return Err(SourceRuleError::Capacity);
        }
        let (mut nodes, mut bytes) = (0, 0);
        for (i, (k, v)) in self.registry.iter().enumerate() {
            if matches!(k, V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)) {
                return Err(SourceRuleError::InvalidInput);
            }
            if self.registry[..i]
                .iter()
                .any(|(prior, _)| same_key(prior, k))
            {
                return Err(SourceRuleError::InvalidInput);
            }
            k.validate()?;
            v.validate()?;
            measure_value(k, &mut nodes, &mut bytes, 0)?;
            measure_value(v, &mut nodes, &mut bytes, 0)?;
        }
        for v in &self.pending_withdrawals {
            v.validate()?;
            measure_value(v, &mut nodes, &mut bytes, 0)?;
        }
        if nodes > MAX_RECORDS || bytes > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelsDependency {
    pub call: DependencyCall,
    pub returned: DependencyReturn,
    pub awaited: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelsEvaluation {
    pub outcome: SourceOutcome,
    pub arguments_after: Vec<V>,
    pub dependencies: Vec<ChannelsDependency>,
    pub deferred_rejections: Vec<SourceException>,
    pub swallowed: Vec<(String, SourceException)>,
}
impl ChannelsEvaluation {
    pub const fn require_authenticated_admission(&self) -> Result<()> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
impl ChannelsEvaluation {
    pub fn diagnostic_value(&self, state: &ChannelsState) -> V {
        let exception =
            |e: &SourceException| obj(vec![("name", strv(&e.name)), ("message", strv(&e.message))]);
        let outcome = match &self.outcome {
            SourceOutcome::Returned(v) => {
                obj(vec![("kind", strv("returned")), ("value", v.clone())])
            }
            SourceOutcome::Thrown(e) => {
                obj(vec![("kind", strv("thrown")), ("error", exception(e))])
            }
        };
        let deps = self
            .dependencies
            .iter()
            .map(|d| {
                let returned = match &d.returned {
                    DependencyReturn::Immediate(v) => {
                        obj(vec![("kind", strv("immediate")), ("value", v.clone())])
                    }
                    DependencyReturn::Pending(v) => {
                        obj(vec![("kind", strv("pending")), ("value", v.clone())])
                    }
                    DependencyReturn::Throw(e) => {
                        obj(vec![("kind", strv("throw")), ("error", exception(e))])
                    }
                    DependencyReturn::PendingThrow(e) => obj(vec![
                        ("kind", strv("pending-throw")),
                        ("error", exception(e)),
                    ]),
                };
                obj(vec![
                    ("target", strv(&d.call.target)),
                    ("arguments", V::Array(d.call.arguments.clone())),
                    ("awaited", V::Bool(d.awaited)),
                    ("returned", returned),
                ])
            })
            .collect();
        obj(vec![
            ("outcome", outcome),
            ("argumentsAfter", V::Array(self.arguments_after.clone())),
            ("state", state.diagnostic_value()),
            ("dependencies", V::Array(deps)),
            (
                "deferredRejections",
                V::Array(self.deferred_rejections.iter().map(exception).collect()),
            ),
            (
                "swallowed",
                V::Array(
                    self.swallowed
                        .iter()
                        .map(|(stage, e)| {
                            obj(vec![("stage", strv(stage)), ("error", exception(e))])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceChannels {
    state: ChannelsState,
    actions: usize,
}
impl SourceChannels {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_state_unverified(mut state: ChannelsState) -> Result<Self> {
        state.validate()?;
        for (key, _) in &mut state.registry {
            if matches!(key, V::Number(bits) if f64::from_bits(*bits) == 0.0) {
                *key = V::number(0.0);
            }
        }
        Ok(Self { state, actions: 0 })
    }
    pub fn state(&self) -> &ChannelsState {
        &self.state
    }
    pub const fn action_count(&self) -> usize {
        self.actions
    }
    /// Internal reference-path mutation: equivalent to writing through the
    /// object returned by Channels.getChannel. This is not setChannel and
    /// deliberately performs no save, dependency call, or action increment.
    pub(crate) fn mutate_cached_channel<F>(&mut self, key: &V, mutation: F) -> Result<()>
    where
        F: FnOnce(&mut V) -> Result<()>,
    {
        let (_, channel) = self
            .state
            .registry
            .iter_mut()
            .find(|(k, _)| same_key(k, key))
            .ok_or(SourceRuleError::InvalidInput)?;
        mutation(channel)?;
        self.state.validate()
    }
    pub(crate) fn replace_loaded_registry(&mut self, rows: Vec<V>) -> Result<()> {
        let mut registry = Vec::new();
        for row in rows {
            let key = canonical_map_key(row.get("_id"));
            let value = row.get("data");
            if let Some((_, old)) = registry.iter_mut().find(|(k, _)| same_key(k, &key)) {
                *old = value;
            } else {
                registry.push((key, value));
            }
        }
        self.state.registry = registry;
        self.state.validate()
    }
    pub(crate) fn begin_withdrawal(&mut self, args: &[V]) -> Result<V> {
        let column = if loose_equal(&arg(args, 6), &V::Bool(false))
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
        {
            strv("A")
        } else if loose_equal(&arg(args, 6), &V::Bool(true))
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
        {
            strv("B")
        } else {
            arg(args, 6)
        };
        let row = obj(vec![
            ("withdrawAll", arg(args, 5)),
            ("blockHeight", arg(args, 0)),
            ("senderAddress", arg(args, 1)),
            ("amount", arg(args, 2)),
            ("channel", arg(args, 3)),
            ("propertyId", arg(args, 4)),
            ("column", column),
        ]);
        if self.state.pending_withdrawals.len() >= MAX_RECORDS {
            return Err(SourceRuleError::Capacity);
        }
        self.state.pending_withdrawals.push(row.clone());
        self.state.validate()?;
        Ok(row)
    }
    pub fn execute_unverified<B: SourceBackend>(
        &mut self,
        call: &SourceCall,
        backend: &mut B,
    ) -> Result<ChannelsEvaluation> {
        if self.actions >= MAX_ACTIONS || call.arguments.len() > 16 {
            return Err(SourceRuleError::Capacity);
        }
        if call.method != "constructor" && !METHODS.contains(&call.method.as_str()) {
            return Err(SourceRuleError::UnknownMethod);
        }
        for v in &call.arguments {
            v.validate()?
        }
        let (mut nodes, mut bytes) = (0, 0);
        for v in &call.arguments {
            measure_value(v, &mut nodes, &mut bytes, 0)?;
        }
        if nodes > MAX_RECORDS || bytes > MAX_STATE_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        self.state.validate()?;
        let mut staged = self.clone();
        let mut args = call.arguments.clone();
        let mut context = Context {
            owner: &mut staged,
            backend,
            dependencies: vec![],
            deferred: vec![],
            swallowed: vec![],
            depth: 0,
            trace_nodes: 0,
            trace_bytes: 0,
        };
        let result = context.run(&call.method, &mut args);
        let outcome = match result {
            Ok(v) => {
                v.validate()?;
                SourceOutcome::Returned(v)
            }
            Err(Fault::Thrown(e)) => SourceOutcome::Thrown(e),
            Err(Fault::Local(e)) => return Err(e),
        };
        let evaluation = ChannelsEvaluation {
            outcome,
            arguments_after: args,
            dependencies: context.dependencies,
            deferred_rejections: context.deferred,
            swallowed: context.swallowed,
        };
        staged.state.validate()?;
        staged.actions += 1;
        *self = staged;
        Ok(evaluation)
    }
}
struct Context<'a, B> {
    owner: &'a mut SourceChannels,
    backend: &'a mut B,
    dependencies: Vec<ChannelsDependency>,
    deferred: Vec<SourceException>,
    swallowed: Vec<(String, SourceException)>,
    depth: usize,
    trace_nodes: usize,
    trace_bytes: usize,
}
impl<B: SourceBackend> Context<'_, B> {
    fn dep(&mut self, target: &str, arguments: Vec<V>, awaited: bool) -> Eval<V> {
        if self.dependencies.len() >= MAX_DEPENDENCIES {
            return Err(SourceRuleError::Capacity.into());
        }
        for a in &arguments {
            a.validate()?;
            measure_value(a, &mut self.trace_nodes, &mut self.trace_bytes, 0)?;
        }
        let call = DependencyCall {
            target: target.into(),
            arguments,
        };
        let returned = self.backend.invoke(&call)?;
        match &returned {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                v.validate()?;
                measure_value(v, &mut self.trace_nodes, &mut self.trace_bytes, 0)?;
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                if e.name.len() > 256 || e.message.len() > 65536 {
                    return Err(SourceRuleError::Capacity.into());
                }
                self.trace_bytes = self
                    .trace_bytes
                    .checked_add(e.name.len() + e.message.len())
                    .ok_or(SourceRuleError::Capacity)?;
            }
        }
        if self.trace_nodes > MAX_TRACE_NODES || self.trace_bytes > MAX_TRACE_BYTES {
            return Err(SourceRuleError::Capacity.into());
        }
        let out = match &returned {
            DependencyReturn::Immediate(v) => Ok(v.clone()),
            DependencyReturn::Pending(v) => Ok(if awaited { v.clone() } else { V::Promise }),
            DependencyReturn::Throw(e) => Err(Fault::Thrown(e.clone())),
            DependencyReturn::PendingThrow(e) if awaited => Err(Fault::Thrown(e.clone())),
            DependencyReturn::PendingThrow(e) => {
                self.deferred.push(e.clone());
                Ok(V::Promise)
            }
        };
        self.dependencies.push(ChannelsDependency {
            call,
            returned,
            awaited,
        });
        out
    }
    fn db(&mut self, category: &str) -> Eval<()> {
        self.dep("DB.getDatabase", vec![strv(category)], true)?;
        Ok(())
    }
    fn db_call(&mut self, method: &str, category: &str, mut args: Vec<V>) -> Eval<V> {
        args.insert(0, strv(category));
        self.dep(&format!("DB.{method}"), args, true)
    }
    fn lookup(&self, key: &V) -> V {
        self.owner
            .state
            .registry
            .iter()
            .find(|(k, _)| same_key(k, key))
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined)
    }
    fn has(&self, key: &V) -> bool {
        self.owner
            .state
            .registry
            .iter()
            .any(|(k, _)| same_key(k, key))
    }
    fn store(&mut self, key: V, value: V) -> Eval<()> {
        let key = canonical_map_key(key);
        if matches!(
            key,
            V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise
        ) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        if let Some((_, v)) = self
            .owner
            .state
            .registry
            .iter_mut()
            .find(|(k, _)| same_key(k, &key))
        {
            *v = value
        } else {
            if self.owner.state.registry.len() >= MAX_CHANNELS {
                return Err(SourceRuleError::Capacity.into());
            }
            self.owner.state.registry.push((key, value))
        }
        Ok(())
    }
    fn delete(&mut self, key: &V) {
        self.owner.state.registry.retain(|(k, _)| !same_key(k, key))
    }
    fn load(&mut self) -> Eval<()> {
        self.db("channels")?;
        let result = self.db_call("findAsync", "channels", vec![obj(vec![])]);
        let out = (|| {
            let entries = result?;
            let entries = array(&entries, "entries.map is not a function")?;
            let mut rows = vec![];
            for e in entries {
                let key = canonical_map_key(get(&e, "_id")?);
                let value = get(&e, "data")?;
                if let Some((_, v)) = rows.iter_mut().find(|(k, _)| same_key(k, &key)) {
                    *v = value
                } else {
                    rows.push((key, value))
                }
            }
            self.owner.state.registry = rows;
            Ok(())
        })();
        match out {
            Err(Fault::Thrown(e)) if e.message.contains("does not exist") => {
                self.swallowed.push(("loadChannelsRegistry".into(), e));
                self.owner.state.registry.clear();
                Ok(())
            }
            other => other,
        }
    }
    fn save(&mut self) -> Eval<()> {
        self.db("channels")?;
        let entries = self.owner.state.registry.clone();
        for (k, data) in entries {
            let key = text(&k)?;
            self.db_call(
                "updateAsync",
                "channels",
                vec![
                    obj(vec![("_id", strv(key))]),
                    obj(vec![("$set", obj(vec![("data", data)]))]),
                    obj(vec![("upsert", V::Bool(true))]),
                ],
            )?;
        }
        Ok(())
    }
    fn channel(&mut self, key: &V) -> Eval<V> {
        let mut v = self.lookup(key);
        if !truth(&v) {
            self.load()?;
            v = self.lookup(key);
            if !truth(&v) {
                v = V::Null
            }
        }
        Ok(v)
    }
    fn delta(&mut self, p: &V) -> Eval<()> {
        if nullish(p) {
            return Err(type_error(format!(
                "Cannot destructure property 'channelId' of '{}' as it is {}.",
                text(p)?,
                text(p)?
            )));
        }
        let uuid = self.dep("UUID.v4", vec![], false)?;
        self.db("channelDelta")?;
        let channel = get(p, "channelId")?;
        let column = get(p, "column")?;
        let property = get(p, "propertyId")?;
        let block = default_undefined(get(p, "block")?, V::number(0.0));
        let key = format!(
            "{}-{}-{}-{}-{}",
            text(&channel)?,
            text(&property)?,
            text(&column)?,
            text(&block)?,
            text(&uuid)?
        );
        let timestamp = self.dep("Clock.now", vec![], false)?;
        let delta = obj(vec![
            ("channelId", channel),
            ("column", column),
            ("propertyId", property),
            ("amount", get(p, "amount")?),
            ("type", get(p, "type")?),
            (
                "participant",
                default_undefined(get(p, "participant")?, strv("")),
            ),
            ("block", block),
            ("txid", default_undefined(get(p, "txid")?, strv(""))),
            ("memo", default_undefined(get(p, "memo")?, strv(""))),
            ("timestamp", timestamp),
        ]);
        self.db_call(
            "insertAsync",
            "channelDelta",
            vec![obj(vec![("_id", strv(key)), ("data", delta)])],
        )?;
        Ok(())
    }
    fn participant_change(&mut self, a: &[V]) -> Eval<()> {
        let channel = arg(a, 0);
        let column = arg(a, 1);
        let participant = arg(a, 2);
        let block = arg(a, 3);
        let prev = default_undefined(arg(a, 4), strv(""));
        let memo = default_undefined(arg(a, 5), strv(""));
        let memo = if truth(&memo) {
            memo
        } else {
            strv(format!(
                "Set participant for {} to {}{}",
                text(&column)?,
                text(&participant)?,
                if truth(&prev) {
                    format!(" (prev: {})", text(&prev)?)
                } else {
                    String::new()
                }
            ))
        };
        self.delta(&obj(vec![
            ("channelId", channel),
            ("column", column),
            ("propertyId", V::Null),
            ("amount", V::number(0.0)),
            ("type", strv("participantChange")),
            ("participant", participant),
            ("block", block),
            ("txid", strv("")),
            ("memo", memo),
        ]))
    }
    fn run(&mut self, method: &str, args: &mut [V]) -> Eval<V> {
        if self.depth >= 32 {
            return Err(SourceRuleError::Capacity.into());
        }
        self.depth += 1;
        let out = self.dispatch(method, args);
        self.depth -= 1;
        out
    }
    fn dispatch(&mut self, method: &str, args: &mut [V]) -> Eval<V> {
        let a = |i| arg(args, i);
        match method {
            "constructor" => Ok(obj(vec![])),
            "addToRegistry" => {
                self.store(a(0), obj(vec![("commiterA", a(1)), ("commiterB", a(2))]))?;
                self.save()?;
                Ok(V::Undefined)
            }
            "removeFromRegistry" => {
                self.delete(&a(0));
                self.save()?;
                Ok(V::Undefined)
            }
            "setChannel" => {
                self.store(a(0), a(1))?;
                self.save()?;
                Ok(V::Undefined)
            }
            "saveChannelsRegistry" => {
                self.save()?;
                Ok(V::Undefined)
            }
            "loadChannelsRegistry" => {
                self.load()?;
                Ok(V::Undefined)
            }
            "getChannel" => self.channel(&a(0)),
            "isValidChannel" => {
                let mut c = self.lookup(&a(0));
                if !truth(&c) {
                    self.load()?;
                    c = self.lookup(&a(0))
                }
                Ok(if truth(&c) {
                    V::Undefined
                } else {
                    V::Bool(false)
                })
            }
            "getCommitAddresses" => {
                let mut c = self.lookup(&a(0));
                if !truth(&c) {
                    self.load()?;
                    c = self.lookup(&a(0))
                }
                if truth(&c) && truth(&get(&c, "participants")?) {
                    let p = get(&c, "participants")?;
                    Ok(obj(vec![
                        ("commitAddressA", get(&p, "A")?),
                        ("commitAddressB", get(&p, "B")?),
                    ]))
                } else {
                    Ok(obj(vec![
                        ("commitAddressA", V::Null),
                        ("commitAddressB", V::Null),
                    ]))
                }
            }
            "addCommitment" => Err(type_error(
                "Cannot read properties of undefined (reading 'updateAsync')",
            )),
            "getCommitments" => Err(type_error(
                "Cannot read properties of undefined (reading 'findOneAsync')",
            )),
            "savePendingWithdrawalToDB" => {
                self.withdrawal_db("updateAsync", &a(0))?;
                Ok(V::Undefined)
            }
            "removePendingWithdrawalFromDB" => {
                self.withdrawal_db("removeAsync", &a(0))?;
                Ok(V::Undefined)
            }
            "loadPendingWithdrawalsFromDB" => self.load_withdrawals(),
            "recordChannelDelta" => {
                self.delta(&a(0))?;
                Ok(V::Undefined)
            }
            "recordParticipantChange" => {
                self.participant_change(args)?;
                Ok(V::Undefined)
            }
            "recordTokenTrade" | "recordContractTrade" => {
                let trade = a(0);
                let contract = method == "recordContractTrade";
                let key = if contract {
                    format!("contract-{}", text(&get(&trade, "contractId")?)?)
                } else {
                    format!(
                        "token-{}-{}",
                        text(&get(&trade, "offeredPropertyId")?)?,
                        text(&get(&trade, "desiredPropertyId")?)?
                    )
                };
                self.save_trade(&obj(vec![
                    ("key", strv(key)),
                    ("type", strv(if contract { "contract" } else { "token" })),
                    ("trade", trade),
                    ("blockHeight", a(1)),
                    ("txid", a(2)),
                ]))?;
                Ok(V::Undefined)
            }
            "saveTrade" => {
                self.save_trade(&a(0))?;
                Ok(V::Undefined)
            }
            "compareCharacters" => {
                let (x, y) = (a(0), a(1));
                Ok(V::number(if primitive_equal(&x, &y) {
                    0.0
                } else {
                    let (nx, ny) = (!number(&x)?.is_nan(), !number(&y)?.is_nan());
                    if nx && !ny {
                        -1.0
                    } else if !nx && ny {
                        1.0
                    } else if relational_lt(&x, &y)? {
                        -1.0
                    } else {
                        1.0
                    }
                }))
            }
            "resolveColumn" => {
                address_value(&a(1))?;
                participant_values(&a(0))?;
                let p = optional(&a(0), "participants")?;
                let aa = optional(&p, "A")?;
                let bb = optional(&p, "B")?;
                let addr = a(1);
                Ok(if !truth(&addr) {
                    V::Null
                } else if primitive_equal(&aa, &addr) {
                    strv("A")
                } else if primitive_equal(&bb, &addr) {
                    strv("B")
                } else {
                    V::Null
                })
            }
            "assignColumnBasedOnLastCharacter" => {
                last_column(&a(0), &default_undefined(a(1), V::number(1.0)))
            }
            "tieBreakerByBackChar" => tie_break_callback(
                &a(0),
                &a(1),
                &a(2),
                args.get(3).is_some_and(|v| !matches!(v, V::Undefined)),
            ),
            "predictColumnForAddress" => predict(&a(0), &a(1), &a(2)),
            "assignColumnBasedOnAddress" => {
                let mut channel = a(0);
                let result = self.assign(&mut channel, &a(1), &a(2), &a(3));
                if !args.is_empty() {
                    args[0] = channel
                }
                result
            }
            "bumpColumnAssignment" => {
                let mut channel = a(0);
                let result = self.bump(
                    &mut channel,
                    &a(1),
                    &a(2),
                    &default_undefined(a(3), V::number(0.0)),
                );
                if !args.is_empty() {
                    args[0] = channel
                }
                result
            }
            "areBothPartiesCommitted" => {
                let channel = self.lookup(&a(0));
                if !truth(&channel) {
                    Ok(V::Bool(false))
                } else {
                    Ok(V::Bool(primitive_equal(
                        &get(&get(&channel, "participants")?, "size")?,
                        &V::number(2.0),
                    )))
                }
            }
            "processChannelCommits" => {
                let mut call = vec![a(1)];
                let both = self.run("areBothPartiesCommitted", &mut call)?;
                if truth(&both) {
                    return Err(type_error("Channels.assignColumns is not a function"));
                }
                Ok(V::Undefined)
            }
            "updateChannelWithColumnAssignments" => Err(type_error(
                "Cannot read properties of undefined (reading 'get')",
            )),
            "isChannelEmpty" => Ok(V::Bool(is_empty(&a(0))?)),
            "removeChannelFromDB" => {
                self.db("channels")?;
                self.db_call(
                    "removeAsync",
                    "channels",
                    vec![obj(vec![("_id", strv(text(&a(0))?))])],
                )?;
                Ok(V::Undefined)
            }
            "removeEmptyChannels" => {
                let rows = self.owner.state.registry.clone();
                for (key, data) in rows {
                    if is_empty(&data)? {
                        self.delete(&key);
                        let mut call = vec![];
                        self.run("removeChannelFromDB", &mut call)?;
                    }
                }
                Ok(V::Undefined)
            }
            "sumBalance" => {
                if nullish(&a(0)) {
                    get(&a(0), "reduce")?;
                }
                let rows = array(&a(0), "rows.reduce is not a function")?;
                let mut sum = V::number(0.0);
                for row in rows {
                    sum = add(&sum, &fallback(get(&row, "amount")?, V::number(0.0)))?
                }
                Ok(sum)
            }
            "toChannelPropMap" => {
                let rows = iterable(&a(0), "rows is not iterable")?;
                let mut map = obj(vec![]);
                for row in rows {
                    let key = text(&get(&row, "channel")?)?;
                    let property = text(&get(&row, "propertyId")?)?;
                    if inherited_object_key(&key) || inherited_object_key(&property) {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    let mut channel = fallback(get(&map, &key)?, obj(vec![]));
                    let amount = add(
                        &fallback(get(&channel, &property)?, V::number(0.0)),
                        &get(&row, "amount")?,
                    )?;
                    set(&mut channel, &property, amount)?;
                    set(&mut map, &key, channel)?
                }
                Ok(map)
            }
            "adjustChannelBalances" => {
                let key = a(0);
                let property = text(&a(1))?;
                let column = text(&a(3))?;
                let mut channel = self.lookup(&key);
                let out = (|| {
                    let value = add(&get(&get(&channel, &column)?, &property)?, &a(2))?;
                    set_path(&mut channel, &[&column, &property], value)?;
                    if !truth(&channel) {
                        return Err(error("Trade channel not found"));
                    }
                    Ok(V::Undefined)
                })();
                if self.has(&key) {
                    self.store(key, channel)?
                }
                out
            }
            "handleChannelPubkey" => {
                let mut channel = a(0);
                let out = self.pubkey(&mut channel, &a(1), &a(2), &a(3));
                if !args.is_empty() {
                    args[0] = channel
                }
                out
            }
            "debitInitMarginFromChannel" => self.debit(args),
            "recordCommitToChannel" => self.commit(args),
            "addToWithdrawalQueue" => {
                let column = if loose_equal(&a(6), &V::Bool(false))? {
                    strv("A")
                } else if loose_equal(&a(6), &V::Bool(true))? {
                    strv("B")
                } else {
                    a(6)
                };
                let row = obj(vec![
                    ("withdrawAll", a(5)),
                    ("blockHeight", a(0)),
                    ("senderAddress", a(1)),
                    ("amount", a(2)),
                    ("channel", a(3)),
                    ("propertyId", a(4)),
                    ("column", column),
                ]);
                self.owner.state.pending_withdrawals.push(row.clone());
                self.withdrawal_db("updateAsync", &row)?;
                Ok(V::Undefined)
            }
            "processWithdrawal" => {
                let mut channel = a(1);
                let out = self.withdraw(&a(0), &mut channel, &a(2), &a(3), &a(4), &a(5), false);
                if args.len() > 1 {
                    args[1] = channel
                }
                out
            }
            "processWithdrawAll" => {
                let mut channel = a(1);
                let out = self.withdraw_all(&a(0), &mut channel, &a(2), &a(3), false);
                if args.len() > 1 {
                    args[1] = channel
                }
                out
            }
            "processWithdrawals" => self.process_withdrawals(&a(0)),
            "processTransfer" => self.transfer(&a(0)),
            "getChannelBalancesForAddress" => self.balances(&a(0), &a(1)),
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
}
fn arg(a: &[V], i: usize) -> V {
    a.get(i).cloned().unwrap_or(V::Undefined)
}
fn default_undefined(v: V, other: V) -> V {
    if matches!(v, V::Undefined) {
        other
    } else {
        v
    }
}
fn address_value(v: &V) -> Eval<()> {
    if matches!(v, V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_)) {
        Err(SourceRuleError::InvalidInput.into())
    } else {
        Ok(())
    }
}
fn participant_values(channel: &V) -> Eval<()> {
    let p = optional(channel, "participants")?;
    for side in ["A", "B"] {
        address_value(&optional(&p, side)?)?
    }
    Ok(())
}
fn measure_value(v: &V, nodes: &mut usize, bytes: &mut usize, depth: usize) -> Result<()> {
    if matches!(v, V::BigNumber(value) if value == "-0") {
        // BigNumber.valueOf preserves -0 while String(BigNumber) does not.
        // A lone text value cannot stand for both source coercion hints.
        return Err(SourceRuleError::InvalidInput);
    }
    *nodes = nodes.checked_add(1).ok_or(SourceRuleError::Capacity)?;
    *bytes = bytes.checked_add(24).ok_or(SourceRuleError::Capacity)?;
    if depth > 24 || *nodes > MAX_TRACE_NODES || *bytes > MAX_TRACE_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    match v {
        V::String(s) | V::BigNumber(s) => {
            *bytes = bytes
                .checked_add(s.len())
                .ok_or(SourceRuleError::Capacity)?;
        }
        V::Array(a) => {
            for v in a {
                measure_value(v, nodes, bytes, depth + 1)?
            }
        }
        V::Object(a) => {
            for (k, v) in a {
                *bytes = bytes
                    .checked_add(k.len())
                    .ok_or(SourceRuleError::Capacity)?;
                measure_value(v, nodes, bytes, depth + 1)?
            }
        }
        _ => {}
    }
    if *bytes > MAX_TRACE_BYTES {
        return Err(SourceRuleError::Capacity);
    }
    Ok(())
}
fn relational_lt(a: &V, b: &V) -> Eval<bool> {
    let primitive = |v: &V| -> Eval<V> {
        Ok(
            if matches!(v, V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)) {
                strv(text(v)?)
            } else {
                v.clone()
            },
        )
    };
    let (a, b) = (primitive(a)?, primitive(b)?);
    if let (V::String(a), V::String(b)) = (&a, &b) {
        Ok(a.encode_utf16().cmp(b.encode_utf16()).is_lt())
    } else {
        Ok(number(&a)? < number(&b)?)
    }
}
fn last_column(addr: &V, last: &V) -> Eval<V> {
    let V::String(s) = addr else {
        return Ok(strv("A"));
    };
    if s.is_empty() {
        return Ok(strv("A"));
    }
    let index = s.encode_utf16().count() as f64 - number(last)?;
    if index < 0.0 {
        return Ok(strv("A"));
    }
    let ch = if index.fract() == 0.0 && index.is_finite() {
        get(addr, &number_text(index)?)?
    } else {
        V::Undefined
    };
    if nullish(&ch) {
        get(&ch, "toUpperCase")?;
    }
    let ch = strarg(&ch, "lastChar.toUpperCase")?;
    Ok(strv(
        if [
            "A", "C", "E", "G", "I", "K", "M", "O", "Q", "S", "U", "W", "Y", "1", "3", "5", "7",
            "9",
        ]
        .contains(&ch.to_uppercase().as_str())
        {
            "A"
        } else {
            "B"
        },
    ))
}
fn tie_break(a: &V, b: &V, column: &V) -> Eval<V> {
    tie_break_callback(a, b, column, false)
}
fn tie_break_callback(a: &V, b: &V, column: &V, invalid_callback: bool) -> Eval<V> {
    let alen = number(&get(a, "length")?)?;
    let blen = number(&get(b, "length")?)?;
    let len = if alen.is_nan() || blen.is_nan() {
        0.0
    } else {
        alen.min(blen)
    };
    if len >= 1.0 && invalid_callback {
        return Err(type_error("assignColFunc is not a function"));
    }
    if len > MAX_TIE_SCAN as f64 {
        return Err(SourceRuleError::Capacity.into());
    }
    let mut winner = a.clone();
    let mut loser = b.clone();
    for n in 1..=(len.max(0.0) as usize) {
        let ca = last_column(a, &V::number(n as f64))?;
        let cb = last_column(b, &V::number(n as f64))?;
        if primitive_equal(&ca, column) && !primitive_equal(&cb, column) {
            break;
        }
        if primitive_equal(&cb, column) && !primitive_equal(&ca, column) {
            winner = b.clone();
            loser = a.clone();
            break;
        }
    }
    Ok(obj(vec![
        ("winner", winner),
        ("loser", loser),
        ("winnerColumn", column.clone()),
        (
            "loserColumn",
            strv(if primitive_equal(column, &strv("A")) {
                "B"
            } else {
                "A"
            }),
        ),
    ]))
}
fn predict(channel: &V, addr: &V, cp: &V) -> Eval<V> {
    address_value(addr)?;
    address_value(cp)?;
    participant_values(channel)?;
    if !truth(channel) || !truth(&get(channel, "participants")?) {
        return last_column(addr, &V::number(1.0));
    }
    let p = get(channel, "participants")?;
    let (a, b) = (get(&p, "A")?, get(&p, "B")?);
    if !truth(&a) && !truth(&b) {
        return last_column(addr, &V::number(1.0));
    }
    if primitive_equal(&a, addr) {
        return Ok(strv("A"));
    }
    if primitive_equal(&b, addr) {
        return Ok(strv("B"));
    }
    let column = last_column(addr, &V::number(1.0))?;
    let current = get(&p, &text(&column)?)?;
    if !primitive_equal(&current, cp) && !truth(&current) {
        return Ok(column);
    }
    if primitive_equal(&current, cp) || (truth(&current) && !primitive_equal(&current, addr)) {
        return get(&tie_break(addr, cp, &column)?, "winnerColumn");
    }
    Ok(V::Null)
}
fn is_empty(channel: &V) -> Eval<bool> {
    if !truth(channel) || !truth(&get(channel, "participants")?) {
        return Ok(true);
    }
    for side in ["A", "B"] {
        for (_, v) in entries(&fallback(get(channel, side)?, obj(vec![])))? {
            if !primitive_equal(&v, &V::number(0.0)) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

impl<B: SourceBackend> Context<'_, B> {
    fn withdrawal_db(&mut self, method: &str, row: &V) -> Eval<V> {
        let key = format!(
            "withdrawal-{}-{}",
            text(&get(row, "blockHeight")?)?,
            text(&get(row, "senderAddress")?)?
        );
        self.db("withdrawQueue")?;
        let mut args = vec![obj(vec![("_id", strv(key))])];
        if method == "updateAsync" {
            args.push(obj(vec![("$set", obj(vec![("data", row.clone())]))]));
            args.push(obj(vec![("upsert", V::Bool(true))]))
        }
        self.db_call(method, "withdrawQueue", args)
    }
    fn load_withdrawals(&mut self) -> Eval<V> {
        self.db("withdrawQueue")?;
        let query = obj(vec![(
            "_id",
            obj(vec![(
                "$regex",
                obj(vec![
                    ("$regexp", strv("^withdrawal-")),
                    ("$flags", strv("")),
                ]),
            )]),
        )]);
        let result = self.db_call("findAsync", "withdrawQueue", vec![query])?;
        let rows = array(&result, "entries.map is not a function")?;
        Ok(V::Array(
            rows.iter().map(|r| get(r, "data")).collect::<Eval<_>>()?,
        ))
    }
    fn save_trade(&mut self, trade: &V) -> Eval<()> {
        self.db("tradeHistory")?;
        let key = format!(
            "{}-{}-{}",
            text(&get(trade, "key")?)?,
            text(&get(trade, "txid")?)?,
            text(&get(trade, "blockHeight")?)?
        );
        let mut doc = obj(vec![("_id", strv(&key))]);
        for (k, v) in entries(trade)? {
            set(&mut doc, &k, v)?
        }
        self.db_call(
            "updateAsync",
            "tradeHistory",
            vec![
                obj(vec![("_id", strv(key))]),
                doc,
                obj(vec![("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(())
    }
    fn assign(&mut self, channel: &mut V, addr: &V, cp: &V, block: &V) -> Eval<V> {
        address_value(addr)?;
        address_value(cp)?;
        participant_values(channel)?;
        let column = last_column(addr, &V::number(1.0))?;
        let key = text(&column)?;
        let mut p = get(channel, "participants")?;
        if !truth(&optional(&p, "A")?) && !truth(&get(&p, "B")?) {
            set(&mut p, &key, addr.clone())?;
            set(channel, "participants", p)?;
            self.participant_change(&[
                get(channel, "channel")?,
                column,
                addr.clone(),
                block.clone(),
                strv(""),
                strv("Initial assignment"),
            ])?;
            return Ok(channel.clone());
        }
        if primitive_equal(&get(&p, "A")?, addr) || primitive_equal(&get(&p, "B")?, addr) {
            return Ok(channel.clone());
        }
        let current = get(&p, &key)?;
        if !primitive_equal(&current, cp) && !truth(&current) {
            let prev = fallback(current, strv(""));
            set_path(channel, &["participants", &key], addr.clone())?;
            self.participant_change(&[
                get(channel, "channel")?,
                column,
                addr.clone(),
                block.clone(),
                prev,
                strv("Assigned via open slot (no conflict)"),
            ])?;
            return Ok(channel.clone());
        }
        if loose_equal(&current, cp)? || (truth(&current) && !primitive_equal(&current, addr)) {
            let tie = tie_break(addr, cp, &column)?;
            let winner_column = get(&tie, "winnerColumn")?;
            let desired_a = get(
                &tie,
                if primitive_equal(&winner_column, &strv("A")) {
                    "winner"
                } else {
                    "loser"
                },
            )?;
            let desired_b = get(
                &tie,
                if primitive_equal(&winner_column, &strv("B")) {
                    "winner"
                } else {
                    "loser"
                },
            )?;
            let prev_a = fallback(get(&p, "A")?, strv(""));
            let prev_b = fallback(get(&p, "B")?, strv(""));
            let force_a = fallback(desired_a, prev_a.clone());
            let force_b = fallback(
                desired_b,
                if truth(&prev_a) && !primitive_equal(&prev_a, &force_a) {
                    prev_a
                } else {
                    prev_b
                },
            );
            return self.bump(channel, &force_a, &force_b, block);
        }
        Ok(V::Undefined)
    }
    fn bump(&mut self, channel: &mut V, force_a: &V, force_b: &V, block: &V) -> Eval<V> {
        address_value(force_a)?;
        address_value(force_b)?;
        participant_values(channel)?;
        if !truth(channel) {
            return Err(error("Channel object is required for bumpColumnAssignment"));
        }
        if !truth(&get(channel, "participants")?) {
            set(
                channel,
                "participants",
                obj(vec![("A", strv("")), ("B", strv(""))]),
            )?
        }
        for side in ["A", "B"] {
            if !truth(&get(channel, side)?) {
                set(channel, side, obj(vec![]))?
            }
        }
        let p = get(channel, "participants")?;
        let (pa, pb) = (get(&p, "A")?, get(&p, "B")?);
        let supplied = |v: &V| !nullish(v) && !primitive_equal(v, &strv(""));
        let na = if supplied(force_a) {
            force_a.clone()
        } else {
            fallback(pa.clone(), strv(""))
        };
        let mut nb = if supplied(force_b) {
            force_b.clone()
        } else {
            fallback(pb.clone(), strv(""))
        };
        if truth(&na) && primitive_equal(&na, &nb) {
            if truth(&pa) && !primitive_equal(&pa, &na) {
                nb = pa.clone()
            } else if truth(&pb) && !primitive_equal(&pb, &na) {
                nb = pb.clone()
            }
        }
        if primitive_equal(&pa, &na) && primitive_equal(&pb, &nb) {
            return Ok(channel.clone());
        }
        if primitive_equal(&pa, &nb) || primitive_equal(&pb, &na) {
            let aa = get(channel, "A")?;
            let bb = get(channel, "B")?;
            set_path(channel, &["participants", "A"], pb.clone())?;
            set_path(channel, &["participants", "B"], pa.clone())?;
            set(channel, "A", bb)?;
            set(channel, "B", aa)?;
            if !(primitive_equal(&pa, &nb) && primitive_equal(&pb, &na)) {
                let p = get(channel, "participants")?;
                if !primitive_equal(&get(&p, "A")?, &na) || !primitive_equal(&get(&p, "B")?, &nb) {
                    set_path(
                        channel,
                        &["participants", "A"],
                        fallback(na.clone(), get(&p, "A")?),
                    )?;
                    set_path(
                        channel,
                        &["participants", "B"],
                        fallback(nb.clone(), get(&p, "B")?),
                    )?
                }
            }
        } else {
            set_path(channel, &["participants", "A"], na)?;
            set_path(channel, &["participants", "B"], nb)?
        }
        for (side, prev) in [("A", pa), ("B", pb)] {
            let next = get(&get(channel, "participants")?, side)?;
            if !primitive_equal(&next, &prev) {
                self.participant_change(&[
                    get(channel, "channel")?,
                    strv(side),
                    next,
                    block.clone(),
                    prev,
                    strv("Rotated (bumpColumnAssignment)"),
                ])?
            }
        }
        Ok(channel.clone())
    }
    fn pubkey(&mut self, channel: &mut V, column: &V, sender: &V, txid: &V) -> Eval<V> {
        let tx = self.dep("TxUtils.getRawTransaction", vec![txid.clone()], true)?;
        let vin = get(&get(&tx, "vin")?, "0")?;
        let script = self.dep(
            "TxUtils.getAddressTypeUniversal",
            vec![sender.clone()],
            false,
        )?;
        let key = fallback(
            self.dep("TxUtils.extractPubkeyByType", vec![vin, script], true)?,
            V::Array(vec![]),
        );
        set_path(channel, &["channelPubkeys", &text(column)?], key)?;
        let keys = get(channel, "channelPubkeys")?;
        let (a, b) = (get(&keys, "A")?, get(&keys, "B")?);
        let mut valid = true;
        let mut reason = String::new();
        if truth(&a) && truth(&b) {
            self.dep("Vesting.getInstance", vec![], true)?;
            let chain = self.dep("Vesting.getChain", vec![], false)?;
            let test = self.dep("Vesting.getTest", vec![], false)?;
            let first = self.dep(
                "TxUtils.createMultisig",
                vec![
                    a.clone(),
                    b.clone(),
                    chain.clone(),
                    test.clone(),
                    sender.clone(),
                ],
                true,
            )?;
            let second = self.dep(
                "TxUtils.createMultisig",
                vec![b, a, chain, test, sender.clone()],
                true,
            )?;
            let address = get(channel, "channel")?;
            if !primitive_equal(&address, &first) && !primitive_equal(&address, &second) {
                valid = false;
                reason = "Multisig does not match channel address.".into()
            }
        }
        Ok(obj(vec![
            ("channel", channel.clone()),
            ("valid", V::Bool(valid)),
            ("reason", strv(reason)),
        ]))
    }
    fn debit(&mut self, args: &[V]) -> Eval<V> {
        reject_own_coercion(&arg(args, 3))?;
        address_value(&arg(args, 1))?;
        let key = arg(args, 0);
        let mut channel = self.channel(&key)?;
        participant_values(&channel)?;
        let result = (|| {
            if !truth(&channel) || !truth(&get(&channel, "participants")?) {
                return Err(error(format!(
                    "Channel {} not found or malformed",
                    text(&key)?
                )));
            }
            let p = get(&channel, "participants")?;
            let participant = arg(args, 1);
            let column = if primitive_equal(&get(&p, "A")?, &participant) {
                strv("A")
            } else if primitive_equal(&get(&p, "B")?, &participant) {
                strv("B")
            } else {
                V::Null
            };
            let col = text(&column)?;
            let property = text(&arg(args, 2))?;
            if !truth(&get(&channel, &col)?) {
                set(&mut channel, &col, obj(vec![]))?
            }
            if !matches!(get(&get(&channel, &col)?, &property)?, V::Number(_)) {
                set_path(&mut channel, &[&col, &property], V::number(0.0))?
            }
            let balance = get(&get(&channel, &col)?, &property)?;
            let amount = arg(args, 3);
            let bal = bn_text(&balance)?;
            let amt = bn_text(&amount)?;
            let insufficient = bn_lt(&balance, &amount)?;
            if insufficient {
                return Err(error(format!(
                    "Insufficient channel balance: {bal} < {amt} in {} {col} {property}",
                    text(&key)?
                )));
            }
            let after = V::number(rounded_sum(&balance, &amount, true)?);
            set_path(&mut channel, &[&col, &property], after)?;
            self.store(key.clone(), channel.clone())?;
            self.save()?;
            self.delta(&obj(vec![
                ("channelId", key.clone()),
                ("column", column),
                ("propertyId", arg(args, 2)),
                ("amount", V::number(-rounded_value(&amount)?)),
                (
                    "type",
                    default_undefined(arg(args, 5), strv("debitChannelContractTradeInitMargin")),
                ),
                ("participant", participant),
                ("block", arg(args, 4)),
                ("txid", arg(args, 6)),
                ("memo", strv("")),
            ]))?;
            Ok(V::Bool(true))
        })();
        if self.has(&key) {
            self.store(key, channel)?
        }
        result
    }
    fn commit(&mut self, args: &[V]) -> Eval<V> {
        reject_own_coercion(&arg(args, 3))?;
        address_value(&arg(args, 1))?;
        let key = arg(args, 0);
        let sender = arg(args, 1);
        if !self.has(&key) {
            self.store(
                key.clone(),
                obj(vec![
                    ("participants", obj(vec![("A", strv("")), ("B", strv(""))])),
                    ("channel", key.clone()),
                    ("commits", V::Array(vec![])),
                    ("A", obj(vec![])),
                    ("B", obj(vec![])),
                    (
                        "clearLists",
                        obj(vec![("A", V::Array(vec![])), ("B", V::Array(vec![]))]),
                    ),
                    (
                        "payEnabled",
                        obj(vec![("A", V::Bool(false)), ("B", V::Bool(false))]),
                    ),
                    ("lastCommitmentTime", arg(args, 6)),
                    ("lastUsedColumn", V::Null),
                    (
                        "channelPubkeys",
                        obj(vec![("A", strv("")), ("B", strv(""))]),
                    ),
                ]),
            )?
        }
        let mut channel = self.lookup(&key);
        let result = (|| {
            let p = get(&channel, "participants")?;
            let a = get(&p, "A")?;
            let b = get(&p, "B")?;
            let cp = if !primitive_equal(&a, &sender) && truth(&a) {
                a
            } else if !primitive_equal(&b, &sender) && truth(&b) {
                b
            } else {
                strv("")
            };
            let column = predict(&channel, &sender, &cp)?;
            let result = self.pubkey(&mut channel, &column, &sender, &arg(args, 7))?;
            if !truth(&get(&result, "valid")?) {
                return Ok(V::Undefined);
            }
            self.assign(&mut channel, &sender, &cp, &arg(args, 6))?;
            let p = get(&channel, "participants")?;
            let column = if !truth(&sender) {
                V::Null
            } else if primitive_equal(&get(&p, "A")?, &sender) {
                strv("A")
            } else if primitive_equal(&get(&p, "B")?, &sender) {
                strv("B")
            } else {
                V::Null
            };
            if nullish(&column) {
                return Ok(V::Undefined);
            }
            let col = text(&column)?;
            let property = text(&arg(args, 2))?;
            let before = fallback(get(&get(&channel, &col)?, &property)?, V::number(0.0));
            let after = rounded_sum(&before, &arg(args, 3), false)?;
            set_path(&mut channel, &[&col, &property], V::number(after))?;
            let record = obj(vec![
                ("senderAddress", sender.clone()),
                ("propertyId", arg(args, 2)),
                ("tokenAmount", arg(args, 3)),
                ("block", arg(args, 6)),
                ("columnAssigned", column.clone()),
                ("payEnabled", arg(args, 4)),
                (
                    "commitClearlistId",
                    default_undefined(arg(args, 8), V::Null),
                ),
            ]);
            self.delta(&obj(vec![
                ("channelId", get(&channel, "channel")?),
                ("column", column.clone()),
                ("propertyId", arg(args, 2)),
                ("amount", V::number(rounded_value(&arg(args, 3))?)),
                ("type", strv("creditCommit")),
                ("participant", sender.clone()),
                ("block", arg(args, 6)),
                ("txid", arg(args, 7)),
                ("memo", strv("Commit")),
            ]))?;
            if !truth(&get(&channel, "clearLists")?) {
                set(
                    &mut channel,
                    "clearLists",
                    obj(vec![("A", V::Array(vec![])), ("B", V::Array(vec![]))]),
                )?
            }
            if !truth(&get(&channel, "payEnabled")?) {
                set(
                    &mut channel,
                    "payEnabled",
                    obj(vec![("A", V::Bool(false)), ("B", V::Bool(false))]),
                )?
            }
            if let V::Array(a) = arg(args, 5) {
                if !a.is_empty() {
                    set_path(&mut channel, &["clearLists", &col], V::Array(a))?
                }
            }
            if truth(&arg(args, 4)) {
                set_path(&mut channel, &["payEnabled", &col], V::Bool(true))?
            }
            set_path(&mut channel, &["participants", &col], sender.clone())?;
            let mut commits = get(&channel, "commits")?;
            push(&mut commits, record)?;
            set(&mut channel, "commits", commits)?;
            set(&mut channel, "lastCommitmentTime", arg(args, 6))?;
            set(&mut channel, "lastUsedColumn", column)?;
            self.store(key.clone(), channel.clone())?;
            self.save()?;
            Ok(channel.clone())
        })();
        self.store(key, channel)?;
        result
    }
    // Mirrors the six source parameters plus the explicit local alias context.
    #[allow(clippy::too_many_arguments)]
    fn withdraw(
        &mut self,
        sender: &V,
        channel: &mut V,
        amount: &V,
        property: &V,
        column: &V,
        block: &V,
        linked: bool,
    ) -> Eval<V> {
        let key = get(channel, "channel")?;
        let result = (|| {
            let mut amount = amount.clone();
            let has = self.dep(
                "Tally.hasSufficientReserve",
                vec![key.clone(), property.clone(), amount.clone()],
                true,
            )?;
            if loose_equal(&get(&has, "hasSufficient")?, &V::Bool(false))? {
                amount = subtract(&amount, &get(&has, "shortfall")?)?
            }
            let col = text(column)?;
            let property_key = text(property)?;
            let after = subtract(&get(&get(channel, &col)?, &property_key)?, &amount)?;
            set_path(channel, &[&col, &property_key], after)?;
            if linked {
                self.flush_existing(&key, channel)?;
            }
            self.dep(
                "Tally.updateChannelBalance",
                vec![
                    key.clone(),
                    property.clone(),
                    V::number(-number(&amount)?),
                    strv("channelWithdrawalPull"),
                    block.clone(),
                ],
                true,
            )?;
            self.dep(
                "Tally.updateBalance",
                vec![
                    sender.clone(),
                    property.clone(),
                    amount,
                    V::number(0.0),
                    V::number(0.0),
                    V::number(0.0),
                    strv("channelWithdrawalComplete"),
                    block.clone(),
                ],
                true,
            )?;
            self.store(key.clone(), channel.clone())?;
            Ok(V::Undefined)
        })();
        if linked {
            self.flush_existing(&key, channel)?;
        }
        result
    }
    fn flush_existing(&mut self, key: &V, channel: &V) -> Eval<()> {
        if self.has(key) {
            self.store(key.clone(), channel.clone())?
        }
        Ok(())
    }
    fn withdraw_all(
        &mut self,
        sender: &V,
        channel: &mut V,
        column: &V,
        block: &V,
        linked: bool,
    ) -> Eval<V> {
        let rows = entries(&get(channel, &text(column)?)?)?;
        for (property, amount) in rows {
            let p = parse_int_number(&property)?;
            self.withdraw(
                sender,
                channel,
                &amount,
                &V::number(p),
                column,
                block,
                linked,
            )?;
        }
        Ok(V::Undefined)
    }
    fn process_withdrawals(&mut self, height: &V) -> Eval<V> {
        if self.owner.state.pending_withdrawals.is_empty() {
            let loaded = self.load_withdrawals()?;
            let loaded = array(&loaded, "pendingWithdrawalsFromDB is not iterable")?;
            if loaded.is_empty() {
                return Ok(V::Undefined);
            }
            self.owner.state.pending_withdrawals.extend(loaded)
        }
        let mut i = 0;
        while i < self.owner.state.pending_withdrawals.len() {
            if i >= MAX_RECORDS {
                return Err(SourceRuleError::Capacity.into());
            }
            let row = self.owner.state.pending_withdrawals[i].clone();
            let sender = get(&row, "senderAddress")?;
            address_value(&sender)?;
            let amount = get(&row, "amount")?;
            let key = get(&row, "channel")?;
            let property = get(&row, "propertyId")?;
            let all = get(&row, "withdrawAll")?;
            let mut channel = self.channel(&key)?;
            let removed_missing = loose_equal(&channel, &V::Undefined)?;
            if removed_missing {
                self.owner.state.pending_withdrawals.remove(i);
                self.withdrawal_db("removeAsync", &row)?;
            }
            let mature = relational_ge(height, &add(&get(&row, "blockHeight")?, &V::number(7.0))?)?;
            if mature {
                // A different data.channel would create shared aliases in the
                // source Map. This finite value owner does not model graphs.
                if !nullish(&channel) && !same_key(&get(&channel, "channel")?, &key) {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                let p = get(&channel, "participants")?;
                let column = if loose_equal(&get(&p, "A")?, &sender)? {
                    strv("A")
                } else if loose_equal(&get(&p, "B")?, &sender)? {
                    strv("B")
                } else {
                    if !removed_missing {
                        i += 1
                    }
                    continue;
                };
                if loose_equal(&all, &V::Bool(true))? {
                    self.withdraw_all(&sender, &mut channel, &column, height, true)?;
                }
                let balance = get(&get(&channel, &text(&column)?)?, &text(&property)?)?;
                let enough = relational_ge(&balance, &amount)? && !number(&amount)?.is_nan();
                if enough && !truth(&all) {
                    self.withdraw(
                        &sender,
                        &mut channel,
                        &amount,
                        &property,
                        &column,
                        height,
                        true,
                    )?;
                }
                if !removed_missing {
                    self.owner.state.pending_withdrawals.remove(i);
                } else {
                    i = i.saturating_sub(1);
                }
                self.withdrawal_db("removeAsync", &row)?;
            } else if !removed_missing {
                i += 1
            }
        }
        self.save()?;
        Ok(V::Undefined)
    }
    fn transfer(&mut self, transaction: &V) -> Eval<V> {
        if nullish(transaction) {
            return Err(type_error(format!(
                "Cannot destructure property 'fromChannel' of 'transaction' as it is {}.",
                text(transaction)?
            )));
        }
        let from = get(transaction, "fromChannel")?;
        let to = get(transaction, "toChannel")?;
        let amount = get(transaction, "amount")?;
        let property = text(&get(transaction, "propertyId")?)?;
        let is_a = get(transaction, "transferorIsColumnA")?;
        let dest = get(transaction, "destinationColumn")?;
        let mut source = self.lookup(&from);
        let mut destination = self.lookup(&to);
        if !truth(&source) || !truth(&destination) {
            return Err(error("Channel(s) not found"));
        }
        let source_col = if truth(&is_a) { "A" } else { "B" };
        let source_addr = optional(&get(&source, "participants")?, source_col)?;
        let dest_addr = optional(&get(&destination, "participants")?, &text(&dest)?)?;
        let mut lists = vec![];
        for c in [&source, &destination] {
            let clear = optional(c, "clearLists")?;
            for side in ["A", "B"] {
                let side = fallback(optional(&clear, side)?, V::Array(vec![]));
                match side {
                    V::Array(a) => lists.extend(a),
                    V::String(s) => lists.extend(s.chars().map(|c| strv(c.to_string()))),
                    _ => return Err(type_error("((intermediate value) || []) is not iterable")),
                }
            }
        }
        if !lists.is_empty() {
            let (mut sa, mut da) = (false, false);
            for list in &lists {
                if !sa && truth(&source_addr) {
                    sa = truth(&self.dep(
                        "ClearList.isAddressInClearlistOrDerived",
                        vec![list.clone(), source_addr.clone()],
                        true,
                    )?)
                }
                if !da && truth(&dest_addr) {
                    da = truth(&self.dep(
                        "ClearList.isAddressInClearlistOrDerived",
                        vec![list.clone(), dest_addr.clone()],
                        true,
                    )?)
                }
                if sa || da {
                    break;
                }
            }
            if !sa && !da {
                return Err(error(format!("Neither transferor ({}) nor destination ({}) is attested in channel clearLists [{}]",text(&source_addr)?,text(&dest_addr)?,text(&V::Array(lists))?)));
            }
        }
        let same = same_key(&from, &to);
        let out = (|| {
            if truth(&is_a) && (loose_equal(&dest, &strv("A"))? || loose_equal(&dest, &strv("B"))?)
            {
                let target = if loose_equal(&dest, &strv("A"))? {
                    "A"
                } else {
                    "B"
                };
                let value = subtract(&get(&get(&source, "A")?, &property)?, &amount)?;
                set_path(&mut source, &["A", &property], value)?;
                if same {
                    destination = source.clone()
                }
                let value = add(&get(&get(&destination, target)?, &property)?, &amount)?;
                set_path(&mut destination, &[target, &property], value)?;
                if same {
                    source = destination.clone()
                }
            } else if !truth(&is_a) && loose_equal(&dest, &strv("A"))? {
                let value = subtract(&get(&get(&source, "B")?, &property)?, &amount)?;
                set_path(&mut source, &["B", &property], value)?;
                if same {
                    destination = source.clone()
                }
                let value = add(&get(&destination, "A")?, &amount)?;
                set(&mut destination, "A", value)?;
                if same {
                    source = destination.clone()
                }
            } else if !truth(&is_a) && loose_equal(&dest, &strv("B"))? {
                let value = subtract(&get(&get(&source, "A")?, &property)?, &amount)?;
                set_path(&mut source, &["A", &property], value)?;
                if same {
                    destination = source.clone()
                }
                let value = add(&get(&destination, "B")?, &amount)?;
                set(&mut destination, "B", value)?;
                if same {
                    source = destination.clone()
                }
            }
            Ok(V::Undefined)
        })();
        self.store(from, source)?;
        self.store(to, destination)?;
        out
    }
    fn balances(&mut self, address: &V, property: &V) -> Eval<V> {
        if !truth(address) {
            return Err(error("address required"));
        }
        self.db("channels")?;
        let address = strarg(address, "address.trim")?;
        let addr = address.trim_matches(js_space).to_string();
        let lower = addr.to_lowercase();
        let filter = if nullish(property) || primitive_equal(property, &strv("")) {
            None
        } else {
            let n = number(property)?;
            if n.is_nan() {
                return Err(error("propertyId must be a number"));
            }
            Some(n)
        };
        let clauses = vec![
            obj(vec![("data.participants.A", strv(&addr))]),
            obj(vec![("data.participants.B", strv(&addr))]),
            obj(vec![("data.participants.A", strv(&lower))]),
            obj(vec![("data.participants.B", strv(&lower))]),
            obj(vec![(
                "data.commits",
                obj(vec![(
                    "$elemMatch",
                    obj(vec![("senderAddress", strv(&addr))]),
                )]),
            )]),
            obj(vec![(
                "data.commits",
                obj(vec![(
                    "$elemMatch",
                    obj(vec![("senderAddress", strv(&lower))]),
                )]),
            )]),
        ];
        let returned = self.db_call(
            "findAsync",
            "channels",
            vec![obj(vec![("$or", V::Array(clauses))])],
        )?;
        let docs = iterable(
            &fallback(returned, V::Array(vec![])),
            "(entries || []) is not iterable",
        )?;
        let mut rows = vec![];
        for doc in docs {
            let data = fallback(get(&doc, "data")?, obj(vec![]));
            let key = fallback(get(&data, "channel")?, get(&doc, "_id")?);
            let p = optional(&data, "participants")?;
            let a = text(&fallback(optional(&p, "A")?, strv("")))?.to_lowercase();
            let b = text(&fallback(optional(&p, "B")?, strv("")))?.to_lowercase();
            let commits = fallback(get(&data, "commits")?, V::Array(vec![]));
            let side = if !a.is_empty() && a == lower {
                Some("A".to_string())
            } else if !b.is_empty() && b == lower {
                Some("B".to_string())
            } else {
                let commits = iterable(&commits, "(data.commits || []) is not iterable")?;
                let mut last = None;
                for commit in commits.iter().rev() {
                    if text(&fallback(get(commit, "senderAddress")?, strv("")))?.to_lowercase()
                        == lower
                    {
                        last = Some(commit);
                        break;
                    }
                }
                let column = last
                    .map(|v| get(v, "columnAssigned"))
                    .transpose()?
                    .unwrap_or(V::Undefined);
                if primitive_equal(&column, &strv("A")) || primitive_equal(&column, &strv("B")) {
                    Some(text(&column)?)
                } else {
                    let last = get(&data, "lastUsedColumn")?;
                    if primitive_equal(&last, &strv("A")) || primitive_equal(&last, &strv("B")) {
                        Some(text(&last)?)
                    } else {
                        None
                    }
                }
            };
            let Some(side) = side else { continue };
            let values = fallback(get(&data, &side)?, obj(vec![]));
            let pairs = if let Some(filter) = filter {
                let key = number_text(filter)?;
                vec![(key.clone(), fallback(get(&values, &key)?, V::number(0.0)))]
            } else {
                entries(&values)?
            };
            for (property, amount) in pairs {
                let pid = number(&strv(property))?;
                let n = number(&amount)?;
                if !n.is_finite() || n <= 0.0 || filter.is_some_and(|f| pid != f) {
                    continue;
                }
                let count = if let V::Array(a) = &commits {
                    a.len()
                } else {
                    0
                };
                let mut pay = false;
                if let V::Array(a) = &commits {
                    for commit in a {
                        if text(&fallback(get(commit, "senderAddress")?, strv("")))?.to_lowercase()
                            == lower
                            && primitive_equal(&get(commit, "payEnabled")?, &V::Bool(true))
                        {
                            pay = true;
                            break;
                        }
                    }
                }
                let last = get(&data, "lastCommitmentTime")?;
                rows.push(obj(vec![
                    ("channel", key.clone()),
                    ("column", strv(&side)),
                    ("side", strv(&side)),
                    ("propertyId", V::number(pid)),
                    ("amount", V::number(n)),
                    (
                        "participants",
                        obj(vec![
                            ("A", fallback(optional(&p, "A")?, strv(""))),
                            ("B", fallback(optional(&p, "B")?, strv(""))),
                        ]),
                    ),
                    (
                        "counterparty",
                        fallback(optional(&p, if side == "A" { "B" } else { "A" })?, strv("")),
                    ),
                    (
                        "lastCommitmentBlock",
                        if nullish(&last) { V::Null } else { last },
                    ),
                    ("commitCount", V::number(count as f64)),
                    ("payEnabled", V::Bool(pay)),
                    (
                        "channelPubkeys",
                        fallback(optional(&data, "channelPubkeys")?, V::Null),
                    ),
                    (
                        "redeemScript",
                        fallback(optional(&data, "redeemScript")?, V::Null),
                    ),
                    (
                        "scriptPubKey",
                        fallback(optional(&data, "scriptPubKey")?, V::Null),
                    ),
                ]));
                if rows.len() > MAX_RECORDS {
                    return Err(SourceRuleError::Capacity.into());
                }
            }
        }
        rows.sort_by(|a, b| {
            let a = number(&a.get("amount")).unwrap_or(f64::NAN);
            let b = number(&b.get("amount")).unwrap_or(f64::NAN);
            b.partial_cmp(&a).unwrap_or(core::cmp::Ordering::Equal)
        });
        Ok(V::Array(rows))
    }
}
fn relational_ge(a: &V, b: &V) -> Eval<bool> {
    let primitive = |v: &V| -> Eval<V> {
        Ok(
            if matches!(v, V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)) {
                strv(text(v)?)
            } else {
                v.clone()
            },
        )
    };
    let (a, b) = (primitive(a)?, primitive(b)?);
    if let (V::String(a), V::String(b)) = (&a, &b) {
        Ok(a.encode_utf16().cmp(b.encode_utf16()).is_ge())
    } else {
        Ok(number(&a)? >= number(&b)?)
    }
}
fn parse_int_number(s: &str) -> Eval<f64> {
    let text = s.trim_start_matches(js_space);
    let (negative, body) = if let Some(s) = text.strip_prefix('-') {
        (true, s)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let (radix, body) = if let Some(s) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X"))
    {
        (16, s)
    } else {
        (10, body)
    };
    let digits: String = body.chars().take_while(|c| c.is_digit(radix)).collect();
    if digits.is_empty() {
        return Ok(f64::NAN);
    }
    let d = radix_decimal(&digits, radix)?;
    let n = d.to_string().parse::<f64>().unwrap_or(f64::INFINITY);
    Ok(if negative { -n } else { n })
}
