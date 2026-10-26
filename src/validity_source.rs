//! Complete pinned validity.js diagnostic rules over explicit dependency reads.
//! A returned `valid` field is source behavior, never transaction authority.
use crate::decimal::{Decimal, Rounding};
use crate::issuance_intent::JsonValue;
use core::str::FromStr;

pub const SOURCE_SHA256: &str = "ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00";
pub const MAX_ARGUMENTS: usize = 8;
pub const MAX_NODES: usize = 8192;
pub const MAX_BYTES: usize = 1_048_576;
pub const MAX_DEPTH: usize = 24;
pub const MAX_DEPENDENCIES: usize = 1024;
pub const METHODS: [&str; 43] = [
    "isActivated",
    "isValidNumber",
    "isRestrictedProceduralToken",
    "validateActivateTradeLayer",
    "validateTokenIssue",
    "validateSend",
    "validateTradeTokenForUTXO",
    "validateCommit",
    "validateOnChainTokenForToken",
    "validateCancelOrder",
    "validateCreateWhitelist",
    "validateUpdateAdmin",
    "validateIssueOrRevokeAttestation",
    "validateAMMPool",
    "validateGrantManagedToken",
    "validateRedeemManagedToken",
    "validateCreateOracle",
    "validatePublishOracleData",
    "validateCloseOracle",
    "validateCreateContractSeries",
    "validateExerciseDerivative",
    "isTradePriceWithinLeverageBounds",
    "validateTradeContractOnchain",
    "validateTradeContractChannel",
    "hasReferencePrice",
    "hasSufficientChannelMargin",
    "checkFlipMargin",
    "validateTradeTokensChannel",
    "isListed",
    "checkWhitelist",
    "validateWithdrawal",
    "validateTransfer",
    "validateMintSynthetic",
    "validateRedeemSynthetic",
    "validatePayToTokens",
    "validateOptionTrade",
    "parseTicker",
    "validateTradeBaiUrbun",
    "validateTradeMurabaha",
    "validateStakeFraudProof",
    "validateSettleChannelPNL",
    "validateKingSettle",
    "validateBatchMoveZkRollup",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceValue {
    Undefined,
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
    Promise,
    BigNumber(String),
}
impl SourceValue {
    pub fn number(number: f64) -> Self {
        Self::Number(if number.is_nan() {
            f64::NAN.to_bits()
        } else {
            number.to_bits()
        })
    }
    pub fn object() -> Self {
        Self::Object(Vec::new())
    }
    pub fn get(&self, key: &str) -> Self {
        match self {
            Self::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map_or(Self::Undefined, |(_, v)| v.clone()),
            Self::Array(values) if key == "length" => Self::number(values.len() as f64),
            Self::String(text) if key == "length" => {
                Self::number(text.encode_utf16().count() as f64)
            }
            _ => Self::Undefined,
        }
    }
    pub fn insert(&mut self, key: &str, value: Self) -> Result<(), SourceRuleError> {
        let Self::Object(fields) = self else {
            return Err(SourceRuleError::InvalidInput);
        };
        if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
            *old = value
        } else {
            fields.push((key.into(), value))
        }
        self.validate()
    }
    pub fn from_json(value: &JsonValue) -> Result<Self, SourceRuleError> {
        let result = match value {
            JsonValue::Null => Self::Null,
            JsonValue::Bool(v) => Self::Bool(*v),
            JsonValue::Number(v) => Self::number(v.as_f64()),
            JsonValue::String(v) => Self::String(v.clone()),
            JsonValue::Array(v) => {
                Self::Array(v.iter().map(Self::from_json).collect::<Result<_, _>>()?)
            }
            JsonValue::Object(v) => Self::Object(
                v.iter()
                    .map(|(k, v)| Ok((k.to_owned(), Self::from_json(v)?)))
                    .collect::<Result<_, SourceRuleError>>()?,
            ),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), SourceRuleError> {
        fn walk(
            v: &SourceValue,
            d: usize,
            n: &mut usize,
            b: &mut usize,
        ) -> Result<(), SourceRuleError> {
            *n = n.checked_add(1).ok_or(SourceRuleError::Capacity)?;
            if *n > MAX_NODES || d > MAX_DEPTH {
                return Err(SourceRuleError::Capacity);
            }
            match v {
                SourceValue::String(s) | SourceValue::BigNumber(s) => {
                    *b = b.checked_add(s.len()).ok_or(SourceRuleError::Capacity)?
                }
                SourceValue::Array(a) => {
                    if a.len() > 1024 {
                        return Err(SourceRuleError::Capacity);
                    }
                    for v in a {
                        walk(v, d + 1, n, b)?
                    }
                }
                SourceValue::Object(a) => {
                    if a.len() > 1024 {
                        return Err(SourceRuleError::Capacity);
                    }
                    let mut names = std::collections::BTreeSet::new();
                    for (k, v) in a {
                        if !names.insert(k) {
                            return Err(SourceRuleError::InvalidInput);
                        }
                        *b = b.checked_add(k.len()).ok_or(SourceRuleError::Capacity)?;
                        walk(v, d + 1, n, b)?
                    }
                }
                _ => {}
            }
            if *b > MAX_BYTES {
                return Err(SourceRuleError::Capacity);
            }
            Ok(())
        }
        walk(self, 0, &mut 0, &mut 0)
    }
    pub fn from_diagnostic_json(text: &str) -> Result<Self, SourceRuleError> {
        if text.len() > MAX_BYTES {
            return Err(SourceRuleError::Capacity);
        }
        fn parse(v: &JsonValue) -> Result<SourceValue, SourceRuleError> {
            let JsonValue::Array(a) = v else {
                return Err(SourceRuleError::InvalidInput);
            };
            let Some(JsonValue::String(tag)) = a.first() else {
                return Err(SourceRuleError::InvalidInput);
            };
            let string = |i: usize| match a.get(i) {
                Some(JsonValue::String(v)) => Ok(v.clone()),
                _ => Err(SourceRuleError::InvalidInput),
            };
            let value = match (tag.as_str(), a.len()) {
                ("undefined", 1) => SourceValue::Undefined,
                ("null", 1) => SourceValue::Null,
                ("promise", 1) => SourceValue::Promise,
                ("boolean", 2) => {
                    if let Some(JsonValue::Bool(v)) = a.get(1) {
                        SourceValue::Bool(*v)
                    } else {
                        return Err(SourceRuleError::InvalidInput);
                    }
                }
                ("string", 2) => SourceValue::String(string(1)?),
                ("bignumber", 2) => SourceValue::BigNumber(string(1)?),
                ("number", 2) => {
                    let s = string(1)?;
                    SourceValue::number(match s.as_str() {
                        "NaN" => f64::NAN,
                        "Infinity" => f64::INFINITY,
                        "-Infinity" => f64::NEG_INFINITY,
                        _ => {
                            if s.len() != 16
                                || !s
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                            {
                                return Err(SourceRuleError::InvalidInput);
                            }
                            f64::from_bits(
                                u64::from_str_radix(&s, 16)
                                    .map_err(|_| SourceRuleError::InvalidInput)?,
                            )
                        }
                    })
                }
                ("array", 2) => {
                    let Some(JsonValue::Array(values)) = a.get(1) else {
                        return Err(SourceRuleError::InvalidInput);
                    };
                    SourceValue::Array(values.iter().map(parse).collect::<Result<_, _>>()?)
                }
                ("object", 2) => {
                    let Some(JsonValue::Array(fields)) = a.get(1) else {
                        return Err(SourceRuleError::InvalidInput);
                    };
                    let mut result = Vec::new();
                    for field in fields {
                        let JsonValue::Array(pair) = field else {
                            return Err(SourceRuleError::InvalidInput);
                        };
                        let [JsonValue::String(key), value] = pair.as_slice() else {
                            return Err(SourceRuleError::InvalidInput);
                        };
                        result.push((key.clone(), parse(value)?))
                    }
                    SourceValue::Object(result)
                }
                _ => return Err(SourceRuleError::InvalidInput),
            };
            Ok(value)
        }
        let parsed = JsonValue::parse(text).map_err(|_| SourceRuleError::InvalidInput)?;
        let value = parse(&parsed)?;
        value.validate()?;
        Ok(value)
    }
    pub fn diagnostic_json(&self) -> Result<String, SourceRuleError> {
        self.validate()?;
        fn quote(s: &str) -> Result<String, SourceRuleError> {
            JsonValue::String(s.into())
                .to_json()
                .map_err(|_| SourceRuleError::Capacity)
        }
        fn emit(v: &SourceValue, o: &mut String) -> Result<(), SourceRuleError> {
            match v {
                SourceValue::Undefined => o.push_str("[\"undefined\"]"),
                SourceValue::Null => o.push_str("[\"null\"]"),
                SourceValue::Bool(v) => o.push_str(if *v {
                    "[\"boolean\",true]"
                } else {
                    "[\"boolean\",false]"
                }),
                SourceValue::Number(bits) => {
                    let n = f64::from_bits(*bits);
                    let s = if n.is_nan() {
                        "NaN".into()
                    } else if n == f64::INFINITY {
                        "Infinity".into()
                    } else if n == f64::NEG_INFINITY {
                        "-Infinity".into()
                    } else {
                        format!("{bits:016x}")
                    };
                    o.push_str(&format!("[\"number\",\"{s}\"]"))
                }
                SourceValue::String(v) => o.push_str(&format!("[\"string\",{}]", quote(v)?)),
                SourceValue::BigNumber(v) => o.push_str(&format!("[\"bignumber\",{}]", quote(v)?)),
                SourceValue::Promise => o.push_str("[\"promise\"]"),
                SourceValue::Array(a) => {
                    o.push_str("[\"array\",[");
                    for (i, v) in a.iter().enumerate() {
                        if i > 0 {
                            o.push(',')
                        }
                        emit(v, o)?
                    }
                    o.push_str("]]")
                }
                SourceValue::Object(a) => {
                    o.push_str("[\"object\",[");
                    for (i, (k, v)) in a.iter().enumerate() {
                        if i > 0 {
                            o.push(',')
                        }
                        o.push('[');
                        o.push_str(&quote(k)?);
                        o.push(',');
                        emit(v, o)?;
                        o.push(']')
                    }
                    o.push_str("]]")
                }
            }
            if o.len() > MAX_BYTES * 4 {
                return Err(SourceRuleError::Capacity);
            }
            Ok(())
        }
        let mut out = String::new();
        emit(self, &mut out)?;
        Ok(out)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceException {
    pub name: String,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRuleError {
    Capacity,
    InvalidInput,
    UnknownMethod,
    UnsupportedNumericDomain,
    UnknownDependency,
    Backend,
    UnauthenticatedInputAndState,
}
impl core::fmt::Display for SourceRuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceRuleError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCall {
    pub method: String,
    pub arguments: Vec<SourceValue>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyCall {
    pub target: String,
    pub arguments: Vec<SourceValue>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyReturn {
    Immediate(SourceValue),
    Pending(SourceValue),
    PendingThrow(SourceException),
    Throw(SourceException),
}
pub trait SourceBackend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyObservation {
    pub call: DependencyCall,
    pub returned: DependencyReturn,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceOutcome {
    Returned(SourceValue),
    Thrown(SourceException),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvaluation {
    pub outcome: SourceOutcome,
    pub arguments_after: Vec<SourceValue>,
    pub dependencies: Vec<DependencyObservation>,
    pub deferred_rejections: Vec<SourceException>,
    pub global_writes: Vec<(String, SourceValue)>,
}
impl SourceEvaluation {
    pub const fn require_authenticated_admission(&self) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}

type V = SourceValue;
#[derive(Debug)]
enum Fault {
    Source(SourceException),
    Local(SourceRuleError),
}
type Eval<T> = Result<T, Fault>;
impl From<SourceRuleError> for Fault {
    fn from(v: SourceRuleError) -> Self {
        Self::Local(v)
    }
}
fn throw(name: &str, message: impl Into<String>) -> Fault {
    Fault::Source(SourceException {
        name: name.into(),
        message: message.into(),
    })
}
fn missing(name: &str) -> Fault {
    throw("ReferenceError", format!("{name} is not defined"))
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn truth(v: &V) -> bool {
    match v {
        V::Undefined | V::Null | V::Bool(false) => false,
        V::Number(b) => {
            let x = f64::from_bits(*b);
            x != 0.0 && !x.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn nullish(v: &V) -> bool {
    matches!(v, V::Null | V::Undefined)
}
fn js_text(v: &V) -> String {
    match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(b) => {
            let x = f64::from_bits(*b);
            if x.is_nan() {
                "NaN".into()
            } else if x == f64::INFINITY {
                "Infinity".into()
            } else if x == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(x).unwrap_or_else(|_| "NaN".into())
            }
        }
        V::String(s) | V::BigNumber(s) => s.clone(),
        V::Array(a) => a
            .iter()
            .map(|v| {
                if nullish(v) {
                    String::new()
                } else {
                    js_text(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        V::Object(_) => "[object Object]".into(),
        V::Promise => "[object Promise]".into(),
    }
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn prefixed_integer(text: &str, radix: u32) -> f64 {
    if text.is_empty() || !text.chars().all(|c| c.is_digit(radix)) {
        return f64::NAN;
    }
    let significant = text.trim_start_matches('0');
    let binary_bound = significant.len().saturating_mul(match radix {
        16 => 4,
        8 => 3,
        _ => 1,
    });
    if binary_bound > 1028 {
        return f64::INFINITY;
    }
    let mut value = Decimal::zero();
    let base = Decimal::from(radix as i128);
    for c in significant.chars() {
        let Some(d) = c.to_digit(radix) else {
            return f64::NAN;
        };
        let Ok(v) = value
            .checked_mul(&base)
            .and_then(|v| v.checked_add(&Decimal::from(d as i128)))
        else {
            return f64::NAN;
        };
        value = v
    }
    value.to_string().parse().unwrap_or(f64::NAN)
}
fn num(v: &V) -> f64 {
    match v {
        V::Number(b) => f64::from_bits(*b),
        V::Null => 0.0,
        V::Bool(b) => f64::from(*b),
        V::String(_) | V::Array(_) | V::BigNumber(_) => {
            let t = js_text(v);
            let t = t.trim_matches(js_space);
            if t.is_empty() {
                0.0
            } else if t == "Infinity" || t == "+Infinity" {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else if let Some(x) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                prefixed_integer(x, 16)
            } else if let Some(x) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
                prefixed_integer(x, 2)
            } else if let Some(x) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
                prefixed_integer(x, 8)
            } else if t
                .bytes()
                .any(|b| !b.is_ascii_digit() && !b"+-.eE".contains(&b))
            {
                f64::NAN
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::String(a), V::String(b)) => a == b,
        _ => false,
    }
}
fn loose(a: &V, b: &V) -> bool {
    if nullish(a) && nullish(b) {
        true
    } else if matches!(
        (a, b),
        (V::String(_), V::String(_)) | (V::Bool(_), V::Bool(_))
    ) {
        strict(a, b)
    } else {
        num(a) == num(b)
    }
}
fn js_less(a: &V, b: &V) -> bool {
    let stringlike = |v: &V| {
        matches!(
            v,
            V::String(_) | V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)
        )
    };
    if stringlike(a) && stringlike(b) {
        js_text(a).encode_utf16().cmp(js_text(b).encode_utf16()) == core::cmp::Ordering::Less
    } else {
        num(a) < num(b)
    }
}
fn js_less_equal(a: &V, b: &V) -> bool {
    let stringlike = |v: &V| {
        matches!(
            v,
            V::String(_) | V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)
        )
    };
    if stringlike(a) && stringlike(b) {
        js_text(a).encode_utf16().cmp(js_text(b).encode_utf16()) != core::cmp::Ordering::Greater
    } else {
        num(a) <= num(b)
    }
}
fn integer(v: &V) -> bool {
    matches!(v, V::Number(_)) && num(v).is_finite() && num(v).fract() == 0.0
}
fn valid_number(v: &V) -> bool {
    matches!(v, V::Number(_))
        && num(v).is_finite()
        && num(v) > 0.0
        && num(v) <= 9_007_199_254_740_991.0
}
fn restricted(v: &V) -> bool {
    (num(&v.get("type")) == 7.0 || strict(&v.get("type"), &s("Procedural")))
        && num(&v.get("proceduralType")) == 1.0
}
fn prop(v: &V, key: &str) -> Eval<V> {
    if nullish(v) {
        Err(throw(
            "TypeError",
            format!("Cannot read properties of {} (reading '{key}')", js_text(v)),
        ))
    } else {
        Ok(v.get(key))
    }
}
fn list(v: V) -> Vec<V> {
    if let V::Array(a) = v {
        a
    } else {
        vec![v]
    }
}
fn array(v: &V) -> Eval<Vec<V>> {
    match v {
        V::Array(a) => Ok(a.clone()),
        _ => Err(throw("TypeError", "value is not iterable")),
    }
}
fn parse_int(v: &V) -> f64 {
    parse_integer(v, false)
}
fn parse_integer(v: &V, decimal_only: bool) -> f64 {
    let owned = js_text(v);
    let text = owned.trim_start_matches(js_space);
    let (negative, text) = if let Some(t) = text.strip_prefix('-') {
        (true, t)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let hexadecimal = !decimal_only && (text.starts_with("0x") || text.starts_with("0X"));
    let text = if hexadecimal { &text[2..] } else { text };
    let length = text
        .bytes()
        .take_while(|b| {
            if hexadecimal {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        })
        .count();
    if length == 0 {
        return f64::NAN;
    }
    let magnitude = if hexadecimal {
        prefixed_integer(&text[..length], 16)
    } else {
        text[..length].parse::<f64>().unwrap_or(f64::NAN)
    };
    if negative {
        -magnitude
    } else {
        magnitude
    }
}
fn parse_float(v: &V) -> f64 {
    let owned = js_text(v);
    let t = owned.trim_start_matches(js_space);
    let bytes = t.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    if t.get(at..).is_some_and(|t| t.starts_with("Infinity")) {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1
    }
    let before = at - start;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let decimal = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1
        }
        if before == 0 && at == decimal {
            return f64::NAN;
        }
    } else if before == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let boundary = at;
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1
        }
        let exponent = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1
        }
        if at == exponent {
            at = boundary
        }
    }
    t[..at].parse().unwrap_or(f64::NAN)
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
            V::Number(_) => js_text(v),
            _ => return Ok(Self::Special(f64::NAN)),
        };
        if text.len() > 512 {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let unsigned = text.trim_start_matches(['+', '-']);
        if ["0x", "0X", "0b", "0B", "0o", "0O"]
            .iter()
            .any(|p| unsigned.starts_with(p))
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
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
    fn text(&self) -> String {
        match self {
            Self::Finite(d) => d.to_string(),
            Self::NegativeZero => "0".into(),
            Self::Special(n) => js_text(&V::number(*n)),
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
    fn abs(self) -> Self {
        match self {
            Self::Finite(d) => Self::Finite(d.abs()),
            Self::NegativeZero => Self::Finite(Decimal::zero()),
            Self::Special(n) => Self::Special(n.abs()),
        }
    }
    fn finite(&self) -> bool {
        !matches!(self, Self::Special(_))
    }
    fn negative(&self) -> bool {
        matches!(self, Self::NegativeZero)
            || matches!(self,Self::Finite(d)if d.is_negative())
            || matches!(self,Self::Special(x)if *x<0.0)
    }
    fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let decimal = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(Decimal::zero()),
            _ => None,
        };
        if let (Some(a), Some(b)) = (decimal(self), decimal(other)) {
            Some(a.cmp(&b))
        } else {
            self.number().partial_cmp(&other.number())
        }
    }
}
fn calc(a: &V, b: &V, op: u8) -> Eval<Bn> {
    Bn::new(a)?.op(Bn::new(b)?, op)
}
fn bnum(b: Bn) -> V {
    n(b.number())
}

struct Engine<'a, B: SourceBackend> {
    backend: &'a mut B,
    args: Vec<V>,
    pi: usize,
    trace: Vec<DependencyObservation>,
    charged: usize,
    deferred: Vec<SourceException>,
    global_writes: Vec<(String, V)>,
}
impl<B: SourceBackend> Engine<'_, B> {
    fn arg(&self, i: usize) -> V {
        self.args.get(i).cloned().unwrap_or(V::Undefined)
    }
    fn p(&self, k: &str) -> V {
        self.arg(self.pi).get(k)
    }
    fn set(&mut self, k: &str, v: V) -> Eval<()> {
        let current = self.arg(self.pi);
        if nullish(&current) {
            return Err(throw(
                "TypeError",
                format!(
                    "Cannot set properties of {} (setting '{k}')",
                    js_text(&current)
                ),
            ));
        }
        if matches!(current, V::Object(_)) {
            self.args[self.pi].insert(k, v).map_err(Fault::from)
        } else {
            Ok(())
        }
    }
    fn reject(&mut self, reason: &str) -> Eval<()> {
        self.set("valid", V::Bool(false))?;
        let previous = self.p("reason");
        self.set("reason", s(&(js_text(&previous) + reason)))
    }
    fn replace_reason(&mut self, reason: &str) -> Eval<()> {
        self.set("valid", V::Bool(false))?;
        self.set("reason", s(reason))
    }
    fn begin(&mut self, reason: bool) -> Eval<()> {
        if reason {
            self.set("reason", s(""))?
        }
        self.set("valid", V::Bool(true))
    }
    fn ret(&self) -> V {
        self.arg(self.pi)
    }
    fn dep(&mut self, target: &str, args: Vec<V>, awaited: bool) -> Eval<V> {
        if self.trace.len() >= MAX_DEPENDENCIES {
            return Err(SourceRuleError::Capacity.into());
        }
        let call = DependencyCall {
            target: target.into(),
            arguments: args,
        };
        let combined = V::Array(call.arguments.clone());
        combined.validate()?;
        let bytes = combined.diagnostic_json()?.len() + target.len();
        self.charged = self
            .charged
            .checked_add(bytes)
            .ok_or(SourceRuleError::Capacity)?;
        if self.charged > MAX_BYTES * 4 {
            return Err(SourceRuleError::Capacity.into());
        }
        let returned = self.backend.invoke(&call)?;
        let value = match &returned {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                v.validate()?;
                validity_domain(v)?;
                self.charged = self
                    .charged
                    .checked_add(v.diagnostic_json()?.len())
                    .ok_or(SourceRuleError::Capacity)?;
                if self.charged > MAX_BYTES * 4 {
                    return Err(SourceRuleError::Capacity.into());
                }
                v.clone()
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                if e.name.len() + e.message.len() > 4096 {
                    return Err(SourceRuleError::Capacity.into());
                }
                V::Undefined
            }
        };
        self.trace.push(DependencyObservation {
            call,
            returned: returned.clone(),
        });
        match returned {
            DependencyReturn::Throw(e) => Err(Fault::Source(e)),
            DependencyReturn::PendingThrow(e) if awaited => Err(Fault::Source(e)),
            DependencyReturn::PendingThrow(e) => {
                self.deferred.push(e);
                Ok(V::Promise)
            }
            DependencyReturn::Pending(_) if !awaited => Ok(V::Promise),
            _ => Ok(value),
        }
    }
    fn active(&mut self, ty: u8, style: u8) -> Eval<bool> {
        let value = self.dep("Activation.isTxTypeActive", vec![n(ty as f64)], true)?;
        let inactive = if style == 0 {
            loose(&value, &V::Bool(false))
        } else {
            !truth(&value)
        };
        if inactive {
            self.reject(if style == 2 {
                "Tx type not yet activated; "
            } else {
                "Tx type not yet activated "
            })?
        }
        Ok(!inactive)
    }
    fn admin(&mut self) -> Eval<V> {
        self.dep("Activation.getAdmin", vec![], false)
    }
    fn property(&mut self, id: V) -> Eval<V> {
        self.dep("Property.getPropertyData", vec![id], true)
    }
    fn listed(&mut self, id: V, address: V, derived: bool) -> Eval<bool> {
        Ok(truth(&self.dep(
            if derived {
                "ClearList.isAddressInClearlistOrDerived"
            } else {
                "ClearList.isAddressInClearlist"
            },
            vec![id, address],
            true,
        )?))
    }
    fn any_list(&mut self, ids: Vec<V>, address: V, derived: bool) -> Eval<bool> {
        for id in ids {
            if self.listed(id, address.clone(), derived)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn country(&mut self, address: V) -> Eval<bool> {
        let bans = self.dep("ClearList.getBanlist", vec![], true)?;
        let bans = if nullish(&bans) {
            V::Array(["US", "KP", "RU", "IR", "CU"].into_iter().map(s).collect())
        } else {
            bans
        };
        let info = self.dep("ClearList.getCountryCodeByAddress", vec![address], true)?;
        Ok(!truth(&info)
            || list(bans)
                .iter()
                .any(|b| strict(b, &info.get("countryCode"))))
    }
    fn reference(&mut self, key: &str, numeric: bool) -> Eval<()> {
        let r = self.p("ref");
        if truth(&r) {
            let outputs = self.dep("TxUtils.getTransactionOutputs", vec![self.arg(2)], true)?;
            let desired = if numeric { n(num(&r)) } else { r };
            let found = array(&outputs)?
                .into_iter()
                .find(|v| strict(&v.get("vout"), &desired));
            if let Some(v) = found {
                self.set(key, v.get("address"))?
            } else {
                self.reject("No channel address detectable in payload or ref: output")?
            }
        }
        Ok(())
    }
    fn address(&mut self, value: V, camel: bool, append: bool) -> Eval<()> {
        if !self.address_local(&value)? {
            let result = self.dep("TxUtils.validateAddressWrapper", vec![value], true)?;
            let good =
                truth(&prop(&result, "isvalid")?) || (camel && truth(&prop(&result, "isValid")?));
            if !good {
                if append {
                    self.reject("Destination address is not validly formed.")?
                } else {
                    self.replace_reason("Destination address is not validly formed.")?
                }
            }
        }
        Ok(())
    }
    fn address_local(&mut self, value: &V) -> Eval<bool> {
        let V::String(address) = value else {
            return Err(throw(
                "TypeError",
                if nullish(value) {
                    format!(
                        "Cannot read properties of {} (reading 'startsWith')",
                        js_text(value)
                    )
                } else {
                    "address.startsWith is not a function".into()
                },
            ));
        };
        let config = if address.starts_with(['1', '3', 'm', 'n', '2'])
            || address.to_lowercase().starts_with("bc")
        {
            Some((0u8, 5u8, "bc"))
        } else if address.starts_with(['L', 'M']) || address.to_lowercase().starts_with("ltc") {
            Some((48, 50, "ltc"))
        } else if address.starts_with('D') {
            Some((30, 22, ""))
        } else {
            None
        };
        let Some((pk, sh, hrp)) = config else {
            return Ok(false);
        };
        if address.starts_with(['1', 'L', 'D', 'm', 'n', '3', 'M', '2']) {
            let version = if address.starts_with(['3', 'M', '2']) {
                sh
            } else {
                pk
            };
            return Ok(source_base58(address, version));
        }
        if !hrp.is_empty() && address.to_lowercase().starts_with(hrp) {
            let decoded = self.dep("Bech32.decode", vec![value.clone()], false);
            return match decoded {
                Ok(v) => Ok(strict(&v.get("prefix"), &s(hrp))),
                Err(Fault::Source(_)) => Ok(false),
                Err(e) => Err(e),
            };
        }
        Ok(false)
    }
}

fn source_base58(address: &str, version: u8) -> bool {
    const ALPHABET: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut bytes = vec![0u8];
    for c in address.chars() {
        let Some(d) = ALPHABET.find(c) else {
            return false;
        };
        let mut carry = d as u32;
        for b in bytes.iter_mut().rev() {
            let x = u32::from(*b) * 58 + carry;
            *b = x as u8;
            carry = x >> 8
        }
        while carry > 0 {
            bytes.insert(0, carry as u8);
            carry >>= 8
        }
        if bytes.len() > 2048 {
            return false;
        }
    }
    let mut hex = String::new();
    for b in bytes {
        hex.push_str(&format!("{b:02x}"))
    }
    while hex.starts_with('0') && hex.len() > 1 {
        hex.remove(0);
    }
    while hex.len() < 50 {
        hex.insert(0, '0');
    }
    let leading = address.bytes().take_while(|b| *b == b'1').count();
    let hex = "00".repeat(leading) + &hex;
    let decoded: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(core::str::from_utf8(b).unwrap_or("00"), 16).unwrap_or(0))
        .collect();
    if decoded.len() < 5 || decoded[0] != version {
        return false;
    }
    let end = decoded.len() - 4;
    crate::hash256::double_sha256(&decoded[..end])[..4] == decoded[end..]
}

impl<B: SourceBackend> Engine<'_, B> {
    fn spot_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        if [
            "propertyIdOffered",
            "propertyIdDesired",
            "amountOffered",
            "amountExpected",
        ]
        .iter()
        .any(|k| !truth(&self.p(k)))
        {
            self.reject("Missing required parameters for tradeTokens ")?
        }
        if !valid_number(&self.p("amountOffered")) || !valid_number(&self.p("amountExpected")) {
            self.reject("Invalid or missing amount; ")?;
            return Ok(self.ret());
        }
        self.active(5, 0)?;
        let offered = self.p("propertyIdOffered");
        let desired = self.p("propertyIdDesired");
        if [2.0, 3.0]
            .iter()
            .any(|v| parse_int(&offered) == *v || parse_int(&desired) == *v)
        {
            self.reject("Vesting tokens cannot be traded")?
        }
        if loose(&offered, &desired) {
            self.reject("Cannot trade token against its own type")?
        }
        let admin = self.admin()?;
        if !loose(&self.arg(0), &admin) {
            let banned = self.country(self.arg(0))?;
            if [1.0, 4.0]
                .iter()
                .any(|v| loose(&offered, &n(*v)) || loose(&desired, &n(*v)))
            {
                let acc = self.listed(n(3.0), self.arg(0), false)?;
                if banned && !acc {
                    self.reject("Sender cannot handle TL or TLI from a banned country or lacking country code attestation")?
                }
            }
        }
        let balance = self.dep(
            "Tally.hasSufficientBalance",
            vec![self.arg(0), offered.clone(), self.p("amountOffered")],
            true,
        )?;
        if !truth(&prop(&balance, "hasSufficient")?) {
            self.reject("Insufficient balance for offered token; ")?
        }
        let p1 = self.property(desired)?;
        let p2 = self.property(offered)?;
        if nullish(&p1) || nullish(&p2) {
            self.reject("Null returning for propertyData")?;
            return Ok(self.ret());
        }
        if restricted(&p1) || restricted(&p2) {
            self.reject("Restricted procedural token cannot be spot token-traded; ")?
        }
        let passes1 = self.any_list(list(p1.get("whitelistId")), self.arg(0), false)?;
        let required =
            !loose(&p1.get("whitelistId"), &n(0.0)) && !loose(&p2.get("whitelistId"), &n(0.0));
        if !passes1 && required {
            self.reject("Sender address not listed in clearlist for offered token ")?
        }
        let passes2 = self.any_list(list(p2.get("whitelistId")), self.arg(0), false)?;
        if !passes2 && required {
            self.reject("Trader address not listed in clearlist ")?
        }
        Ok(self.ret())
    }
    fn cancel_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(6, 1)?;
        if !matches!(self.arg(0), V::String(_)) {
            self.reject("Invalid from address; ")?
        }
        let offered = self.p("offeredPropertyId");
        let desired = self.p("desiredPropertyId");
        if [2.0, 3.0]
            .iter()
            .any(|v| loose(&offered, &n(*v)) || loose(&desired, &n(*v)))
        {
            self.reject("Cannot have orderbooks for untradeable vesting tokens")?
        }
        let contract = self.p("isContract");
        let mut key = V::Undefined;
        if loose(&contract, &V::Bool(false)) {
            key = s(&(js_text(&offered) + "-" + &js_text(&desired)));
            for (id, label) in [(offered.clone(), "offered"), (desired.clone(), "desired")] {
                let valid = if truth(&id) && integer(&id) {
                    truth(&self.property(id)?)
                } else {
                    false
                };
                if !valid {
                    self.reject(&format!("Invalid {label} property ID; "))?
                }
            }
        }
        if truth(&contract) {
            key = offered.clone();
            let valid = if truth(&offered) && integer(&offered) {
                truth(&self.dep("Contract.getContractInfo", vec![offered.clone()], true)?)
            } else {
                false
            };
            if !valid {
                self.reject("Invalid contract ID; ")?
            }
        }
        self.dep("Orderbook.getOrderbookInstance", vec![key], true)?;
        let args = if truth(&contract) {
            vec![self.p("fromAddress"), self.p("contractId")]
        } else {
            vec![self.p("fromAddress"), V::Null, offered, desired]
        };
        let orders = self.dep("Orderbook.getOrdersForAddress", args, false)?;
        if num(&prop(&orders, "length")?) == 0.0 {
            self.reject("No orders found for the sender in the relevant orderbook; ")?
        }
        let cancel = self.p("cancelParams");
        if !matches!(cancel, V::Object(_) | V::Array(_) | V::Null | V::Promise) {
            self.reject("Invalid cancel parameters; ")?
        } else {
            let price = prop(&cancel, "price")?;
            if truth(&price) && !matches!(price, V::Number(_)) {
                self.reject("Invalid price parameter; ")?
            }
            let side = prop(&cancel, "side")?;
            if truth(&side) && !strict(&side, &s("buy")) && !strict(&side, &s("sell")) {
                self.reject("Invalid side parameter; ")?
            }
            if truth(&prop(&cancel, "txid")?) {
                self.reject("TxId parameter deprecated for now. ; ")?
            }
        }
        Ok(self.ret())
    }
    fn update_admin_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(8, 2)?;
        if !matches!(self.p("newAddress"), V::String(_)) {
            self.reject("Invalid new address; ")?
        }
        if ["whitelist", "oracle", "token"]
            .iter()
            .filter(|k| truth(&self.p(k)))
            .count()
            != 1
        {
            self.reject("Exactly one admin target (whitelist/oracle/token) must be selected; ")?
        }
        for (flag, target, missing_reason, owner_reason) in [
            (
                "whitelist",
                "ClearList.getList",
                "Whitelist not found; ",
                "Sender is not the admin of the whitelist; ",
            ),
            (
                "oracle",
                "Oracle.getOracleInfo",
                "Oracle not found; ",
                "Sender is not the admin of the oracle; ",
            ),
            (
                "token",
                "Property.getPropertyData",
                "Token not found; ",
                "Sender is not the admin of the token;",
            ),
        ] {
            if truth(&self.p(flag)) {
                let info = self.dep(target, vec![self.p("id")], true)?;
                if !truth(&info) {
                    self.reject(missing_reason)?
                } else {
                    let nested = info.get("name");
                    let (admin, backup) = if flag == "oracle" {
                        let a = info.get("adminAddress");
                        let b = info.get("backupAddress");
                        (
                            if truth(&a) {
                                a
                            } else {
                                nested.get("adminAddress")
                            },
                            if truth(&b) {
                                b
                            } else {
                                nested.get("backupAddress")
                            },
                        )
                    } else {
                        (
                            info.get(if flag == "token" {
                                "issuer"
                            } else {
                                "adminAddress"
                            }),
                            info.get("backupAddress"),
                        )
                    };
                    if !strict(&admin, &self.arg(0)) && !strict(&backup, &self.arg(0)) {
                        self.reject(owner_reason)?
                    }
                }
                if flag == "token" {
                    let kind = info.get("type");
                    if !strict(&kind, &n(2.0)) && !strict(&kind, &s("Managed")) {
                        self.reject("Not a managed token with a usable admin address")?
                    }
                }
            }
        }
        self.address(self.p("newAddress"), false, false)?;
        Ok(self.ret())
    }
    fn attestation_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(9, 2)?;
        let reference = self.p("ref");
        if !strict(&reference, &V::Bool(false)) && !matches!(reference, V::Undefined) {
            let outputs = self.dep("TxUtils.getTransactionOutputs", vec![self.arg(2)], true)?;
            let found = array(&outputs)?
                .into_iter()
                .find(|o| strict(&o.get("vout"), &n(num(&reference))));
            if let Some(output) = found.filter(|o| truth(&o.get("address"))) {
                self.set("targetAddress", output.get("address"))?
            } else {
                self.reject(&format!(
                    "Could not resolve ref:{} to an address; ",
                    js_text(&reference)
                ))?
            }
        }
        let target = self.p("targetAddress");
        if !matches!(&target,V::String(t)if !t.is_empty()) {
            self.reject("Invalid target address; ")?
        }
        let id = self.p("id");
        let zero = num(&id) == 0.0;
        let ban = zero
            && js_text(&if truth(&target) {
                target.clone()
            } else {
                s("")
            })
            .trim_matches(js_space)
            .to_uppercase()
                == "BANLIST";
        let row = self.dep("ClearList.getClearlistById", vec![id.clone()], true)?;
        if !truth(&row) && !loose(&id, &n(0.0)) {
            self.reject(&format!("Clearlist with ID {} not found; ", js_text(&id)))?
        } else if !loose(&id, &n(0.0)) {
            let a = row.get("adminAddress");
            let admin = if truth(&a) { a } else { row.get("admin") };
            if !strict(&admin, &self.arg(0)) {
                self.reject(&format!("Sender {} is not authorized to issue or revoke attestations for clearlist {}; ",js_text(&self.arg(0)),js_text(&id)))?
            }
        }
        if ban {
            let admin = self.admin()?;
            if !strict(&admin, &self.arg(0)) {
                self.reject("Only protocol admin can update global banlist via list id 0; ")?
            }
            let md = self.p("metaData");
            let text = js_text(&if truth(&md) { md } else { s("") });
            let codes = text
                .split(|c: char| c == ',' || c == ';' || c == '|' || js_space(c))
                .filter(|c| !c.is_empty())
                .map(str::to_uppercase)
                .collect::<Vec<_>>();
            if codes.is_empty()
                || codes
                    .iter()
                    .any(|c| c.len() != 2 || !c.bytes().all(|b| b.is_ascii_uppercase()))
            {
                self.reject("Banlist update requires 2-letter country codes in metaData; ")?
            }
        } else if zero && !loose(&self.arg(0), &target) {
            self.reject(
                "Sender and target address must be the same for self-cert (clearlist id 0) ",
            )?
        }
        if zero && !ban {
            let md = self.p("metaData");
            let text = js_text(&md);
            if !truth(&md)
                || text.trim_matches(js_space).len() != 2
                || !text
                    .trim_matches(js_space)
                    .bytes()
                    .all(|b| b.is_ascii_alphabetic())
            {
                self.reject("Self-cert (clearlist id 0) requires 2-letter country code metaData; ")?
            }
        }
        if loose(&self.p("revoke"), &V::Bool(true)) && !self.listed(id, target, false)? {
            self.reject("Target Address has no attestation to revoke ")?
        }
        Ok(self.ret())
    }
    fn amm_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(10, 1)?;
        if !valid_number(&self.p("amount"))
            || (!truth(&self.p("isContract")) && !valid_number(&self.p("amount2")))
        {
            self.reject("Invalid or missing amount; ")?;
            return Ok(self.ret());
        }
        if truth(&self.p("isContract")) {
            let c = self.dep("Contract.getContractInfo", vec![self.p("id")], true)?;
            if !truth(&c) {
                self.reject(&format!(
                    "Contract series {} not found; ",
                    js_text(&self.p("id"))
                ))?
            }
        } else {
            let p1 = self.property(self.p("id"))?;
            let p2 = self.property(self.p("id2"))?;
            if !truth(&p1) || !truth(&p2) {
                self.reject("Property AMM requires valid id/id2 properties; ")?;
                return Ok(self.ret());
            }
            if [2.0, 3.0]
                .iter()
                .any(|v| strict(&p1, &n(*v)) || strict(&p2, &n(*v)))
            {
                self.replace_reason("Cannot trade vesting tokens")?
            }
            for (p, label) in [
                (p1, "Sender address not in clearlist for offered token"),
                (p2, "Trader address not in clearlist"),
            ] {
                let ids = p.get("whitelistId");
                let ids = if truth(&ids) { list(ids) } else { vec![] };
                for id in ids {
                    if !self.listed(id.clone(), self.arg(0), false)? {
                        self.reject(&format!("{label} {}; ", js_text(&id)))?
                    }
                }
            }
        }
        if truth(&self.p("strategyBlob"))
            && JsonValue::parse(&js_text(&self.p("strategyBlob"))).is_err()
        {
            self.reject("Invalid strategyBlob JSON; ")?
        }
        for (key, label) in [
            ("optionsMaker", "optionsMaker"),
            ("optionsTaker", "optionsTaker"),
        ] {
            if truth(&self.p(key)) && !integer(&self.p(key)) {
                self.reject(&format!("{label} must be integer; "))?
            }
        }
        Ok(self.ret())
    }
    fn managed_rule(&mut self, redeem: bool) -> Eval<V> {
        self.begin(true)?;
        self.active(if redeem { 12 } else { 11 }, 0)?;
        let key = if redeem {
            "amountDestroyed"
        } else {
            "amountGranted"
        };
        if !valid_number(&self.p(key)) {
            self.reject("Invalid or missing amount; ")?;
            return Ok(self.ret());
        }
        if redeem {
            let p = self.property(self.p("propertyId"))?;
            let procedural = num(&p.get("type")) == 7.0 || strict(&p.get("type"), &s("Procedural"));
            let admin = self.dep(
                "Property.isManagedAndAdmin",
                vec![self.p("propertyId"), self.arg(0)],
                true,
            )?;
            if !truth(&admin) && !procedural {
                self.reject("Sender is not admin of a managed property; ")?
            }
            let balance = self.dep(
                "Tally.getTally",
                vec![self.arg(0), self.p("propertyId")],
                true,
            )?;
            let available = balance.get("available");
            if num(&if truth(&available) { available } else { n(0.0) }) < num(&self.p(key)) {
                self.reject("Cannot redeem tokens; insufficient balance; ")?
            }
            if procedural {
                let gate = self.dep(
                    "Procedural.ensureRedemptionContext",
                    vec![
                        self.p("dlcTemplateId"),
                        self.p("dlcContractId"),
                        self.p("settlementState"),
                    ],
                    true,
                )?;
                if !truth(&gate.get("valid")) {
                    self.reject(&(js_text(&gate.get("reason")) + "; "))?
                }
            }
        } else {
            let managed = self.dep(
                "Property.isManagedAndAdmin",
                vec![self.p("propertyId"), self.arg(0)],
                true,
            )?;
            if !truth(&managed) {
                self.reject("Property is not of managed type or admin does not match")?
            }
            let p = self.property(self.p("propertyId"))?;
            let procedural = num(&p.get("type")) == 7.0 || strict(&p.get("type"), &s("Procedural"));
            if !truth(&self.p("addressToGrantTo")) {
                let address = resolve_reference(&self.arg(3));
                let address = if truth(&address) {
                    address
                } else {
                    resolve_reference(&self.p("referenceAddress"))
                };
                if truth(&address) {
                    self.set("addressToGrantTo", address)?
                } else if !procedural {
                    self.set("addressToGrantTo", self.arg(0))?
                }
            }
            if !truth(&self.p("addressToGrantTo")) {
                self.reject(if procedural {
                    "Destination address missing for procedural issuance; "
                } else {
                    "Destination address missing (payload + reference); "
                })?;
                return Ok(self.ret());
            }
            self.address(self.p("addressToGrantTo"), false, true)?;
            if procedural {
                if !truth(&self.p("dlcHash")) {
                    self.reject("Missing dlcHash for procedural issuance; ")?;
                    return Ok(self.ret());
                }
                let gate = self.dep(
                    "Procedural.ensureIssuanceContext",
                    vec![
                        self.p("dlcTemplateId"),
                        self.p("dlcContractId"),
                        self.p("settlementState"),
                        self.p("dlcHash"),
                    ],
                    true,
                )?;
                if !truth(&gate.get("valid")) {
                    self.reject(&(js_text(&gate.get("reason")) + "; "))?
                }
            }
        }
        Ok(self.ret())
    }
    fn create_contract_rule(&mut self) -> Eval<V> {
        self.begin(false)?;
        self.set("reason", s(""))?;
        if strict(&self.p("native"), &V::Bool(false)) {
            let oracle = self.dep(
                "Oracle.getOracleInfo",
                vec![self.p("underlyingOracleId")],
                true,
            )?;
            if matches!(oracle, V::Null) {
                self.reject("Invalid or missing underlying oracle ID. ")?
            }
        }
        self.active(16, 0)?;
        let collateral = self.property(self.p("collateralPropertyId"))?;
        if matches!(collateral, V::Null) {
            self.reject("Invalid collateral property ID. ")?
        }
        if strict(&self.p("native"), &V::Bool(true)) && truth(&self.p("onChainData")) {
            let data = self.p("onChainData");
            let duplicate = self.dep(
                "Contract.isDuplicateNativeContract",
                vec![
                    self.p("collateralPropertyId"),
                    data.clone(),
                    self.p("notionalPropertyId"),
                ],
                true,
            )?;
            if truth(&duplicate) {
                self.reject("Collateral or on-chain pair is redundant.")?
            }
            let mut valid = true;
            for pid in array(&data)? {
                let pair = array(&pid)?;
                let a = pair.first().cloned().unwrap_or(V::Undefined);
                let b = pair.get(1).cloned().unwrap_or(V::Undefined);
                let pa = self.dep("Property.getPropertyData", vec![a.clone()], false)?;
                let pb = self.dep("Property.getPropertyData", vec![b.clone()], false)?;
                if (!matches!(a, V::Null) && nullish(&pa))
                    || (!matches!(b, V::Null) && nullish(&pb))
                {
                    valid = false;
                    break;
                }
            }
            if !valid {
                self.reject("Invalid on-chain data format or property IDs. ")?
            }
            if num(&data.get("length")) == 0.0 {
                self.reject("Array of on-chain pairs for native settlement data is empty.")?
            }
        }
        let cp = parse_int(&self.p("collateralPropertyId"));
        let np = parse_int(&self.p("notionalPropertyId"));
        if (cp == 2.0 && np == 2.0) || cp == 3.0 {
            self.reject("Vesting tokens cannot be used as collateral or hedged")?
        }
        if !matches!(self.p("notionalPropertyId"), V::Null)
            && loose(&self.p("native"), &V::Bool(true))
        {
            let p = self.property(self.p("notionalPropertyId"))?;
            if matches!(p, V::Null) {
                self.reject("Invalid notional property ID. ")?
            }
        }
        if !matches!(self.p("notionalValue"), V::Number(_))
            || loose(&self.p("notionalValue"), &n(0.0))
        {
            self.reject("Notional value must be a non-zero number. ")?
        }
        if !integer(&self.p("expiryPeriod")) {
            self.reject("Expiry period must be an integer. ")?
        }
        if !integer(&self.p("series")) {
            self.reject("Series must be an integer. ")?
        }
        if !matches!(self.p("inverse"), V::Bool(_)) {
            self.reject("Inverse must be a boolean. ")?
        }
        if !matches!(self.p("fee"), V::Bool(_)) {
            self.reject("Fee must be a boolean. ")?
        }
        Ok(self.ret())
    }
}

fn resolve_reference(v: &V) -> V {
    if !truth(v) {
        return s("");
    }
    match v {
        V::String(_) => v.clone(),
        V::Array(a) => a
            .iter()
            .find_map(|v| match v.get("address") {
                V::String(s) if !s.is_empty() => Some(V::String(s)),
                _ => None,
            })
            .unwrap_or_else(|| s("")),
        V::Object(_) => {
            for key in ["address", "senderAddress"] {
                let address = v.get(key);
                if matches!(&address,V::String(s)if !s.is_empty()) {
                    return address;
                }
            }
            s("")
        }
        _ => s(""),
    }
}

impl<B: SourceBackend> Engine<'_, B> {
    fn contract_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        let details = self.dep("Contract.getContractInfo", vec![self.p("contractId")], true)?;
        self.active(18, 0)?;
        if !valid_number(&self.p("price")) {
            self.reject("Price is not a valid number; ")?
        }
        let reference = self.reference_price(self.p("contractId"), self.p("block"))?;
        if !truth(&reference) {
            self.reject("No reference price exists for contract at this block; ")?;
            return Ok(self.ret());
        }
        let leverage = prop(&details, "leverage")?;
        let check = price_band(&object(&[
            ("tradePrice", self.p("price")),
            ("markPrice", reference),
            ("leverage", leverage),
            ("bufferBps", n(65.0)),
        ]))?;
        if !truth(&check.get("valid")) {
            self.reject(&(js_text(&check.get("reason")) + "; "))?;
            return Ok(self.ret());
        }
        if nullish(&self.arg(0)) {
            self.reject("Sender is null")?
        }
        if nullish(&details) {
            self.reject("contractId not found")?;
            return Ok(self.ret());
        }
        let admin = self.admin()?;
        if !loose(&self.arg(0), &admin) && self.country(self.arg(0))? {
            self.reject("Sender cannot trade contracts from a banned country or lacking country code attestation")?
        }
        self.dep("Margin.loadMarginMap", vec![self.p("contractId")], true)?;
        let initial = self.dep(
            "Contract.getInitialMargin",
            vec![self.p("contractId"), self.p("price")],
            true,
        )?;
        let mut total = calc(&initial, &self.p("amount"), 2)?.number();
        let existing = self.dep(
            "Margin.getPositionForAddress",
            vec![self.arg(0), self.p("contractId")],
            true,
        )?;
        let contracts = prop(&existing, "contracts")?;
        let q = num(&contracts);
        let amount = num(&self.p("amount"));
        let buy_reduce = q < 0.0 && loose(&self.p("sell"), &V::Bool(false));
        let sell_reduce = q > 0.0 && loose(&self.p("sell"), &V::Bool(true));
        let buy_flip = amount > q.abs() && buy_reduce;
        let sell_flip = amount > q && sell_reduce;
        if buy_flip {
            total = calc(&initial, &n(amount - q.abs()), 2)?.number()
        } else if sell_flip {
            total = calc(&initial, &n(amount - q), 2)?.number()
        }
        if (!buy_reduce && !sell_reduce) || buy_flip || sell_flip {
            let sufficient = self.dep(
                "Tally.hasSufficientBalance",
                vec![self.arg(0), details.get("collateralPropertyId"), n(total)],
                true,
            )?;
            if loose(&sufficient.get("hasSufficient"), &V::Bool(false)) {
                let undo = bn_ceil(calc(&sufficient.get("shortfall"), &initial, 3)?)?.number();
                self.set("amount", n(num(&self.p("amount")) - undo))?;
                if loose(&self.p("amount"), &n(0.0)) {
                    self.reject("Insufficient balance for initial margin")?
                }
            }
        }
        let global_balance = self.dep(
            "Tally.hasSufficientBalance",
            vec![
                self.p("senderAddress"),
                details.get("collateralPropertyId"),
                n(total),
            ],
            true,
        )?;
        self.global_writes
            .push(("hasSufficientBalance".into(), global_balance));
        let collateral = details.get("collateralPropertyId");
        let whitelist = details.get("whitelist");
        if !loose(&collateral, &n(1.0)) && !nullish(&whitelist) && !loose(&whitelist, &n(0.0)) {
            let property = self.property(collateral)?;
            if nullish(&property) {
                self.reject("Collateral propertyId not found in Property List; ")?
            }
            let lists = list(prop(&property, "whitelistId")?);
            if !self.any_list(lists, self.arg(0), false)? {
                self.reject("Sender address not in clearlist; ")?
            }
        }
        Ok(self.ret())
    }
    fn channel_lists(&mut self, channel: &V, a: V, b: V) -> Eval<()> {
        let clears = prop(channel, "clearLists")?;
        if truth(&clears) {
            let mut all = Vec::new();
            for key in ["A", "B"] {
                let v = clears.get(key);
                if truth(&v) {
                    all.extend(array(&v)?);
                }
            }
            if !all.is_empty() {
                let mut approved = false;
                for id in &all {
                    if truth(&a) && self.listed(id.clone(), a.clone(), true)? {
                        approved = true
                    }
                    if truth(&b) && self.listed(id.clone(), b.clone(), true)? {
                        approved = true
                    }
                    if approved {
                        break;
                    }
                }
                if !approved {
                    self.reject(&format!("Neither committer A ({}) nor B ({}) is attested in channel clearLists [{}]; ",js_text(&a),js_text(&b),all.iter().map(js_text).collect::<Vec<_>>().join(",")))?
                }
            }
        }
        Ok(())
    }
    fn contract_channel_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        let details = self.dep("Contract.getContractInfo", vec![self.p("contractId")], true)?;
        if num(&self.p("amount")) < 1.0 {
            self.reject("Contract amount must be a whole integer")?
        }
        if !valid_number(&self.p("price")) {
            self.reject("Price is not a valid number; ")?
        }
        self.set("amount", n(num(&self.p("amount")).floor()))?;
        self.active(19, 0)?;
        let reference = self.reference_price(self.p("contractId"), self.p("block"))?;
        if !truth(&reference) {
            self.reject("No reference price exists for contract at this block; ")?;
            return Ok(self.ret());
        }
        let check = price_band(&object(&[
            ("tradePrice", self.p("price")),
            ("markPrice", reference),
            ("leverage", prop(&details, "leverage")?),
            ("bufferBps", n(65.0)),
        ]))?;
        if !truth(&check.get("valid")) {
            self.reject(&(js_text(&check.get("reason")) + "; "))?;
            return Ok(self.ret());
        }
        if js_less(&self.p("expiryBlock"), &self.p("block"))
            || matches!(self.p("expiryBlock"), V::Undefined)
        {
            self.replace_reason("Tx confirmed in block later than expiration block")?;
            return Ok(self.ret());
        }
        if nullish(&self.arg(0)) {
            self.reject("Sender is null")?
        }
        let channel = self.dep("Channels.getChannel", vec![self.arg(0)], true)?;
        let commits = self.dep("Channels.getCommitAddresses", vec![self.arg(0)], true)?;
        let a = prop(&commits, "commitAddressA")?;
        let b = prop(&commits, "commitAddressB")?;
        if nullish(&a) && nullish(&b) {
            self.replace_reason("Tx sender is not found to be a channel address")?;
            return Ok(self.ret());
        }
        let admin = self.admin()?;
        if !loose(&self.arg(0), &admin) {
            let bans = self.dep("ClearList.getBanlist", vec![], true)?;
            let bans = if nullish(&bans) {
                ["US", "KP", "RU", "IR", "CU"].into_iter().map(s).collect()
            } else {
                list(bans)
            };
            for (address, label) in [(a.clone(), "A"), (b.clone(), "B")] {
                let country = self.dep("ClearList.getCountryCodeByAddress", vec![address], true)?;
                if !truth(&country) || bans.iter().any(|v| strict(v, &country.get("countryCode"))) {
                    self.reject(&format!("Commiter {label} cannot handle TL or TLI from a banned country or lacking country code attestation"))?
                }
            }
        }
        self.channel_lists(&channel, a.clone(), b.clone())?;
        if !truth(&self.p("valid")) {
            return Ok(self.ret());
        }
        let collateral = prop(&details, "collateralPropertyId")?;
        let initial = self.dep(
            "Contract.getInitialMargin",
            vec![self.p("contractId"), self.p("price")],
            true,
        )?;
        let total = calc(&initial, &self.p("amount"), 2)?.number();
        let key = js_text(&collateral);
        let ca = prop(&prop(&channel, "A")?, &key)?;
        let cb = prop(&prop(&channel, "B")?, &key)?;
        let ta = self.dep("Tally.getTally", vec![a.clone(), collateral.clone()], true)?;
        let tb = self.dep("Tally.getTally", vec![b.clone(), collateral], true)?;
        let ea = js_add(
            &fallback_zero(ca.clone()),
            &fallback_zero(ta.get("available")),
        );
        let eb = js_add(
            &fallback_zero(cb.clone()),
            &fallback_zero(tb.get("available")),
        );
        self.dep("Margin.getInstance", vec![self.p("contractId")], true)?;
        let pa = self.dep(
            "Margin.getPositionForAddress",
            vec![a, self.p("contractId")],
            true,
        )?;
        let pb = self.dep(
            "Margin.getPositionForAddress",
            vec![b, self.p("contractId")],
            true,
        )?;
        let qa = num(&prop(&pa, "contracts")?);
        let qb = num(&prop(&pb, "contracts")?);
        let seller_flag = self.p("columnAIsSeller");
        let a_seller = strict(&seller_flag, &V::Bool(true))
            || strict(&seller_flag, &n(1.0))
            || strict(&seller_flag, &s("1"));
        let buyer_reduce = if a_seller { qb < 0.0 } else { qa < 0.0 };
        let seller_reduce = if a_seller { qa > 0.0 } else { qb > 0.0 };
        if !buyer_reduce {
            let check = margin_check(
                ca.clone(),
                n(total),
                s(if truth(&seller_flag) { "B" } else { "A" }),
                None,
            );
            if !truth(&check.get("valid")) {
                self.reject(&js_text(&check.get("reason")))?;
                return Ok(self.ret());
            }
        }
        if !seller_reduce {
            let check = margin_check(
                cb.clone(),
                n(total),
                s(if truth(&seller_flag) { "A" } else { "B" }),
                None,
            );
            if !truth(&check.get("valid")) {
                self.reject(&js_text(&check.get("reason")))?;
                return Ok(self.ret());
            }
        }
        let (enough, label) = if !buyer_reduce && !seller_reduce {
            (num(&ea) >= total && num(&eb) >= total, "both sides")
        } else if buyer_reduce && !seller_reduce {
            (
                if a_seller {
                    num(&ea) >= total
                } else {
                    num(&eb) >= total
                },
                "sellSide",
            )
        } else if !buyer_reduce && seller_reduce {
            (
                if a_seller {
                    num(&eb) >= total
                } else {
                    num(&ea) >= total
                },
                "buySide",
            )
        } else {
            (true, "")
        };
        if !enough {
            self.reject(&format!(
                "Insufficient balance for initial margin on {label}"
            ))?
        }
        let amount = num(&self.p("amount"));
        let buy_q = if a_seller { qb } else { qa };
        let sell_q = if a_seller { qa } else { qb };
        let buy_flip = amount > buy_q.abs() && buy_q < 0.0;
        let sell_flip = amount > sell_q && sell_q > 0.0;
        let (column, flip, qty) = if buy_flip && a_seller {
            ("B", "flipLong", amount - qb.abs())
        } else if sell_flip && a_seller {
            ("A", "flipShort", amount - qa)
        } else if buy_flip {
            ("A", "flipLong", amount - qa.abs())
        } else if sell_flip {
            ("B", "flipShort", amount - qb)
        } else {
            ("", "", 0.0)
        };
        if !column.is_empty() {
            let balance = if column == "A" { ca } else { cb };
            let required = qty * num(&initial);
            let check = margin_check(balance, n(required), s(column), Some(s(flip)));
            if !truth(&check.get("valid")) {
                self.reject(&js_text(&check.get("reason")))?;
                return Ok(self.ret());
            }
            let effective = if column == "A" {
                ea.clone()
            } else {
                eb.clone()
            };
            if num(&effective) < required {
                let avail = if column == "A" {
                    ta.get("available")
                } else {
                    tb.get("available")
                };
                let shortfall = if column == "B" && flip == "flipLong" {
                    qty
                } else {
                    required - num(&js_add(&effective, &avail))
                };
                let undo = bn_ceil(calc(&n(shortfall), &initial, 3)?)?.number();
                self.set("amount", n(amount - undo))?
            }
        }
        Ok(self.ret())
    }
    fn token_channel_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        if !self.active(20, 0)? {
            return Ok(self.ret());
        }
        for key in ["amountDesired", "amountOffered"] {
            if !valid_number(&self.p(key)) {
                self.reject("Price is not a valid number; ")?
            }
        }
        if js_less(&self.p("expiryBlock"), &self.p("block"))
            || matches!(self.p("expiryBlock"), V::Undefined)
        {
            self.replace_reason("Tx confirmed in block later than expiration block")?;
            return Ok(self.ret());
        }
        let offered = self.p("propertyIdOffered");
        let desired = self.p("propertyIdDesired");
        let po = parse_int(&offered);
        let pd = parse_int(&desired);
        if (pd == 2.0 && po == 2.0) || (pd == 3.0 && po == 4.0) {
            self.reject("Vesting tokens cannot be traded")?
        }
        if loose(&offered, &desired)
            && !truth(&self.p("Id1ColoredOutput"))
            && !truth(&self.p("Id2ColoredOutput"))
        {
            self.reject("Cannot trade token against its own type")?
        }
        if loose(&offered, &n(0.0)) || loose(&desired, &n(0.0)) {
            self.reject("Should be a UTXO trade")?
        }
        let p1 = self.property(offered.clone())?;
        let p2 = self.property(desired.clone())?;
        if restricted(&p1) || restricted(&p2) {
            self.reject("Restricted procedural token cannot be token-traded in channels; ")?
        }
        let commits = self.dep(
            "Channels.getCommitAddresses",
            vec![self.p("senderAddress")],
            true,
        )?;
        let a = prop(&commits, "commitAddressA")?;
        let b = prop(&commits, "commitAddressB")?;
        if nullish(&a) && nullish(&b) {
            self.reject("Tx sender is not found to be a channel address")?;
            return Ok(self.ret());
        }
        let admin = self.admin()?;
        if !loose(&self.arg(0), &admin) {
            let bans = self.dep("ClearList.getBanlist", vec![], true)?;
            let bans = if nullish(&bans) {
                ["US", "KP", "RU", "IR", "CU"].into_iter().map(s).collect()
            } else {
                list(bans)
            };
            for (address, label) in [(a.clone(), "A"), (b.clone(), "B")] {
                let country = self.dep("ClearList.getCountryCodeByAddress", vec![address], true)?;
                if (1..=4).any(|i| loose(&offered, &n(i as f64)) || loose(&desired, &n(i as f64)))
                    && !truth(&self.dep(
                        "Activation.areActivationsAboveThreshold",
                        vec![],
                        false,
                    )?)
                {
                    let acc = self.listed(n(3.0), self.arg(0), false)?;
                    if (!truth(&country)
                        || bans.iter().any(|v| strict(v, &country.get("countryCode"))))
                        && !acc
                    {
                        self.reject(&format!("Commiter {label} cannot handle TL or TLI from a banned country or lacking country code attestation"))?
                    }
                }
            }
        }
        let channel = self.dep("Channels.getChannel", vec![self.arg(0)], true)?;
        if truth(&channel) && truth(&channel.get("clearLists")) {
            self.channel_lists(&channel, a.clone(), b.clone())?;
            if !truth(&self.p("valid")) {
                return Ok(self.ret());
            }
        }
        let offered_key = js_text(&offered);
        let desired_key = js_text(&desired);
        let offerer = self.p("columnAIsOfferer");
        if loose(&offerer, &V::Bool(true)) || loose(&offerer, &V::Bool(false)) {
            let is_a = loose(&offerer, &V::Bool(true));
            let ca = prop(
                &prop(&channel, "A")?,
                if is_a { &offered_key } else { &desired_key },
            )?;
            let cb = prop(
                &prop(&channel, "B")?,
                if is_a { &desired_key } else { &offered_key },
            )?;
            let needed_a = if is_a {
                self.p("amountOffered")
            } else {
                self.p("amountDesired")
            };
            let needed_b = if is_a {
                self.p("amountDesired")
            } else {
                self.p("amountOffered")
            };
            for (balance, needed, label) in
                [(ca.clone(), needed_a, "A"), (cb.clone(), needed_b, "B")]
            {
                if num(&balance) < num(&needed) {
                    self.reject(&format!(
                        "Insufficient channel balance in {label} (need {}, found {}); ",
                        js_text(&needed),
                        js_text(&balance)
                    ))?
                }
            }
            if !truth(&self.p("valid")) {
                return Ok(self.ret());
            }
            let sa = self.dep(
                "Tally.hasSufficientChannel",
                vec![
                    self.arg(0),
                    if is_a {
                        offered.clone()
                    } else {
                        desired.clone()
                    },
                    if is_a {
                        self.p("amountOffered")
                    } else {
                        self.p("amountDesired")
                    },
                ],
                false,
            )?;
            let sb = self.dep(
                "Tally.hasSufficientChannel",
                vec![
                    self.arg(0),
                    if is_a {
                        desired.clone()
                    } else {
                        offered.clone()
                    },
                    if is_a {
                        self.p("amountDesired")
                    } else {
                        self.p("amountOffered")
                    },
                ],
                false,
            )?;
            if js_less(&ca, &self.p("amountOffered")) || !truth(&sa.get("hasSufficient")) {
                self.reject("Column A has insufficient balance for amountOffered")?
            }
            if js_less(&cb, &self.p("amountDesired")) || !truth(&sb.get("hasSufficient")) {
                self.reject("Column B has insufficient balance for amountDesired")?
            }
        }
        let p1 = self.property(offered)?;
        let p2 = self.property(desired)?;
        if nullish(&p1) || nullish(&p2) {
            self.reject("Property data not found; ")?;
            return Ok(self.ret());
        }
        let l1 = list(p1.get("whitelistId"));
        let l2 = list(p2.get("whitelistId"));
        if !(l1.iter().any(|v| strict(v, &n(0.0))) && l2.iter().any(|v| strict(v, &n(0.0)))) {
            return Err(missing("isListed"));
        }
        Ok(self.ret())
    }
}
fn fallback_zero(v: V) -> V {
    if truth(&v) {
        v
    } else {
        n(0.0)
    }
}
fn js_add(a: &V, b: &V) -> V {
    if matches!(a, V::String(_)) || matches!(b, V::String(_)) {
        s(&(js_text(a) + &js_text(b)))
    } else {
        n(num(a) + num(b))
    }
}
fn bn_ceil(v: Bn) -> Eval<Bn> {
    match v {
        Bn::Finite(d) => {
            let q = d
                .quantize(0, Rounding::Down)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if d.is_negative() && q.is_zero() {
                Ok(Bn::NegativeZero)
            } else if !d.is_negative() && q != d {
                Ok(Bn::Finite(
                    q.checked_add(&Decimal::from(1i128))
                        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                ))
            } else {
                Ok(Bn::Finite(q))
            }
        }
        _ => Ok(v),
    }
}
fn bn_up(v: Bn, places: u32) -> Eval<Bn> {
    match v {
        Bn::Finite(d) => {
            let q = d
                .quantize(places, Rounding::Down)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if q != d {
                let unit = Decimal::from_str(&format!("1e-{places}"))
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                Ok(Bn::Finite(
                    if d.is_negative() {
                        q.checked_sub(&unit)
                    } else {
                        q.checked_add(&unit)
                    }
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                ))
            } else {
                Ok(Bn::Finite(q))
            }
        }
        _ => Ok(v),
    }
}

impl<B: SourceBackend> Engine<'_, B> {
    fn withdrawal_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.reference("channelAddress", true)?;
        self.active(21, 0)?;
        if !loose(&self.p("withdrawAll"), &V::Bool(true))
            && (nullish(&self.p("propertyId")) || nullish(&self.p("amount")))
        {
            self.reject("propertyId or amount not specified while withdrawAll is false")?
        }
        let commits = self.dep(
            "Channels.getCommitAddresses",
            vec![self.p("channelAddress")],
            true,
        )?;
        let a = prop(&commits, "commitAddressA")?;
        let b = prop(&commits, "commitAddressB")?;
        if !truth(&a) && !truth(&b) {
            self.reject("Channel not instantiated; ")?;
            return Ok(self.ret());
        }
        let channel = self.dep("Channels.getChannel", vec![self.p("channelAddress")], true)?;
        let participants = prop(&channel, "participants")?;
        let a = prop(&participants, "A")?;
        let b = prop(&participants, "B")?;
        let mut balance = V::Undefined;
        if !loose(&self.arg(0), &a) && !loose(&self.arg(0), &b) {
            self.reject("Sender not authorized for the channel")?
        } else if loose(&self.arg(0), &a) {
            balance = prop(&prop(&channel, "A")?, &js_text(&self.p("propertyId")))?;
            if loose(&self.p("column"), &V::Bool(false)) {
                self.reject("Sender does not match with column")?
            }
        } else {
            balance = prop(&prop(&channel, "B")?, &js_text(&self.p("propertyId")))?;
            if loose(&self.p("column"), &V::Bool(true)) {
                self.reject("Sender does not match with column")?
            }
        }
        if matches!(self.p("column"), V::Undefined) {
            self.reject("column parameter not specified")?
        }
        if js_less(&balance, &self.p("amount")) {
            self.reject("Insufficient balance for withdrawal; ")?
        }
        Ok(self.ret())
    }
    fn transfer_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        if !valid_number(&self.p("amount")) {
            self.reject("Amount is not a valid number; ")?
        }
        self.reference("toChannelAddress", false)?;
        self.active(22, 0)?;
        self.address(self.p("toChannelAddress"), false, false)?;
        let valid = self.dep("Channels.isValidChannel", vec![self.arg(0)], false)?;
        if !truth(&valid) {
            self.reject("Invalid source channel; ")?
        }
        let channel = self.dep("Channels.getChannel", vec![self.arg(0)], true)?;
        let mut balance = n(0.0);
        if !truth(&channel) {
            self.reject("Sender is not a channel.")?
        } else {
            let key = js_text(&self.p("propertyId"));
            let a = fallback_zero(prop(&prop(&channel, "A")?, &key)?);
            let b = fallback_zero(prop(&prop(&channel, "B")?, &key)?);
            balance = if truth(&self.p("isColumnA")) { a } else { b }
        }
        if !js_less_equal(&self.p("amount"), &balance) {
            self.reject("Insufficient balance for transfer; ")?
        }
        let other = self.dep(
            "Channels.getChannel",
            vec![self.p("toChannelAddress")],
            true,
        )?;
        if !nullish(&other) {
            let participants = prop(&other, "participants")?;
            let a = prop(&participants, "A")?;
            let b = prop(&participants, "B")?;
            if !loose(&a, &s(""))
                && !loose(&a, &self.arg(0))
                && !loose(&b, &s(""))
                && !loose(&b, &self.arg(0))
            {
                self.reject("Both columns of the desired transferee channel are occupied by commiters other than the commiter owning the transfered tokens.")?
            }
        }
        let pay = self.p("pay");
        if matches!(pay, V::Undefined) || strict(&pay, &s("")) {
            self.set("pay", V::Bool(false))?
        } else if !matches!(pay, V::Bool(_)) {
            self.reject("pay is not a boolean; ")?
        }
        if truth(&self.p("pay")) && truth(&self.p("payRef")) {
            let reference = num(&self.p("payRef"));
            if !reference.is_finite() || reference.fract() != 0.0 || reference < 0.0 {
                self.reject("payRef is not a valid integer; ")?
            } else if truth(&channel) {
                let column = if truth(&self.p("isColumnA")) {
                    "A"
                } else {
                    "B"
                };
                let enabled = channel.get("payEnabled");
                if !truth(&enabled) || !truth(&enabled.get(column)) {
                    self.reject(&format!("Pay not enabled for column {column}; "))?
                }
                let clears = channel.get("clearLists");
                let lists = if truth(&clears) {
                    clears.get(column)
                } else {
                    V::Undefined
                };
                if truth(&lists) {
                    let outputs =
                        self.dep("TxUtils.getTransactionOutputs", vec![self.arg(2)], true)?;
                    let found = array(&outputs)?
                        .into_iter()
                        .find(|o| strict(&o.get("vout"), &n(reference)));
                    if let Some(out) = found {
                        let addresses = prop(&prop(&out, "scriptPubKey")?, "addresses")?;
                        let address = if let V::Array(a) = addresses {
                            a.first().cloned().unwrap_or(V::Undefined)
                        } else {
                            V::Undefined
                        };
                        self.set("payRefAddress", address.clone())?;
                        let mut results = Vec::new();
                        for id in array(&lists)? {
                            results.push(self.listed(id, address.clone(), false)?)
                        }
                        if !results.contains(&true) {
                            self.reject(&format!(
                                "No valid attestation for payRef address {}; ",
                                js_text(&address)
                            ))?
                        }
                    } else {
                        self.reject("Invalid payRef output; ")?
                    }
                }
            } else {
                self.reject("Sender channel not found; ")?
            }
        }
        Ok(self.ret())
    }
    fn mint_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        let rounded = Bn::new(&self.p("amount"))?.q(0, Rounding::Down)?.number();
        self.active(24, 1)?;
        if rounded < 1.0 {
            self.reject("Amount less than one")?
        }
        self.set("amount", n(rounded))?;
        let contract = self.dep("Contract.getContractInfo", vec![self.p("contractId")], true)?;
        if !truth(&contract) {
            self.reject("hedge contract not found")?;
            return Ok(self.ret());
        }
        let data = prop(&contract, "onChainData")?;
        let row = if let V::Array(a) = data {
            a.first().cloned().unwrap_or(V::Undefined)
        } else {
            V::Undefined
        };
        if nullish(&row) {
            return Err(throw(
                "TypeError",
                format!("Cannot read properties of {} (reading '0')", js_text(&row)),
            ));
        }
        let pair = array(&row)?;
        let pair = s(&(js_text(pair.first().unwrap_or(&V::Undefined))
            + "-"
            + &js_text(pair.get(1).unwrap_or(&V::Undefined))));
        let collateral = contract.get("collateralPropertyId");
        let notional = contract.get("notionalValue");
        if !truth(&contract.get("inverse")) {
            self.reject("Cannot mint synthetics with linear contracts")?
        }
        if !truth(&contract.get("native")) {
            self.reject("Cannot mint synthetics with oracle contracts... no one man should have all that power")?
        }
        self.dep("Margin.getInstance", vec![self.p("contractId")], true)?;
        let position = self.dep(
            "Margin.getPositionForAddress",
            vec![self.arg(0), self.p("contractId")],
            true,
        )?;
        let contracts = prop(&position, "contracts")?;
        if !truth(&contracts) {
            self.reject("Null contracts cannot hedge a mint")?
        }
        let gross = calc(&contracts, &notional, 2)?
            .q(8, Rounding::Down)?
            .number();
        if num(&self.p("amount")) > gross.abs() {
            if gross <= 0.0 {
                self.set(
                    "amount",
                    bnum(Bn::new(&n(gross.abs()))?.q(8, Rounding::Down)?),
                )?;
                self.set("reason",s(&(js_text(&self.p("reason"))+"insufficient contracts to hedge total, minting based on available contracts")))?
            } else {
                self.reject("insufficient contracts to hedge the amount requested")?
            }
        }
        let mark = self.dep("Volume.getLastPrice", vec![pair, self.p("block")], true)?;
        if !truth(&mark) {
            self.reject("cannot identify price to value the mint")?
        }
        let target = bn_up(
            calc(&self.p("amount"), &notional, 2)?.op(Bn::new(&position.get("avgPrice"))?, 3)?,
            8,
        )?;
        let slice = if num(&contracts).abs() > 0.0 {
            calc(&position.get("margin"), &self.p("amount"), 2)?
                .op(Bn::new(&n(num(&contracts).abs()))?, 3)?
                .q(8, Rounding::Down)?
        } else {
            Bn::new(&n(0.0))?
        };
        let mut required = target.op(slice.clone(), 1)?;
        if required.negative() {
            required = Bn::new(&n(0.0))?
        }
        self.set("margin", bnum(slice))?;
        self.set("grossRequired", bnum(required.clone()))?;
        let balance = self.dep(
            "Tally.hasSufficientBalance",
            vec![self.arg(0), collateral, V::BigNumber(required.text())],
            true,
        )?;
        if loose(&balance.get("hasSufficient"), &V::Bool(false)) {
            return Err(missing("initMargin"));
        }
        let contracts = calc(&self.p("amount"), &notional, 3)?.q(0, Rounding::Down)?;
        self.set("contracts", bnum(contracts))?;
        Ok(self.ret())
    }
    fn redeem_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        let rounded = num(&self.p("amount")).floor();
        self.set(
            "propertyId",
            s(&("s-".to_owned()
                + &js_text(&self.p("propertyId"))
                + "-"
                + &js_text(&self.p("contractId")))),
        )?;
        self.active(25, 0)?;
        if rounded < 1.0 {
            self.reject("Amount less than one")?
        }
        self.set("amount", n(rounded))?;
        self.dep("Margin.getInstance", vec![self.p("contractId")], true)?;
        let position = self.dep(
            "Margin.getPositionForAddress",
            vec![self.arg(0), self.p("contractId")],
            true,
        )?;
        if num(&prop(&position, "contracts")?) > 0.0 {
            self.reject(
                "Redemption will close existing longs, move synths to a new address to redeem",
            )?
        }
        let exists = self.dep("Synthetic.exists", vec![self.p("propertyId")], true)?;
        if loose(&exists, &V::Bool(false)) {
            self.reject("Token is not of a synthetic nature")?
        }
        let sufficient = self.dep(
            "Tally.hasSufficientBalance",
            vec![
                self.p("senderAddress"),
                self.p("propertyId"),
                self.p("amount"),
            ],
            true,
        )?;
        if loose(&sufficient.get("hasSufficient"), &V::Bool(false)) {
            if num(&sufficient.get("available")) >= 1.0 {
                self.set("amount", n(num(&sufficient.get("available")).floor()))?
            } else {
                self.reject("insufficient tokens to redeem in this amount")?
            }
        }
        Ok(self.ret())
    }
    fn stake_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(30, 1)?;
        let action = num(&self.p("action"));
        if ![0.0, 1.0, 2.0].contains(&action) {
            self.reject("Invalid action, expected 0/1/2; ")?;
            return Ok(self.ret());
        }
        let oracle = self.dep("Oracle.getOracleInfo", vec![self.p("oracleId")], true)?;
        if !truth(&oracle) {
            self.reject("Invalid oracle id; ")?;
            return Ok(self.ret());
        }
        if action == 0.0 {
            let pid = self.p("stakedPropertyId");
            if !integer(&pid) || num(&pid) <= 0.0 {
                self.reject("Invalid staked property id; ")?
            } else {
                let property = self.property(pid.clone())?;
                if !truth(&property) {
                    self.reject("Staked property does not exist; ")?
                }
            }
            if !valid_number(&self.p("amount")) {
                self.reject("Invalid stake amount; ")?
            } else {
                let tally = self.dep("Tally.getTally", vec![self.arg(0), pid], true)?;
                if num(&fallback_zero(tally.get("available")))
                    < num(&fallback_zero(self.p("amount")))
                {
                    self.reject("Insufficient available balance for stake; ")?
                }
            }
        }
        if action == 1.0 {
            for (key, reason) in [
                ("accusedAddress", "Missing accused address; "),
                ("evidenceHash", "Missing evidence hash; "),
            ] {
                if !truth(&self.p(key)) || !matches!(self.p(key), V::String(_)) {
                    self.reject(reason)?
                }
            }
            if !valid_number(&self.p("amount")) {
                self.reject("Invalid slash amount; ")?
            }
        }
        if action == 2.0 {
            let nested = oracle.get("name");
            let admin = oracle.get("adminAddress");
            let admin = if truth(&admin) {
                admin
            } else {
                nested.get("adminAddress")
            };
            let backup = oracle.get("backupAddress");
            let backup = if truth(&backup) {
                backup
            } else {
                nested.get("backupAddress")
            };
            if !strict(&self.arg(0), &admin) && !strict(&self.arg(0), &backup) {
                self.reject("Relay sender is not oracle admin/backup; ")?
            }
            if !truth(&self.p("stateHash")) || !matches!(self.p("stateHash"), V::String(_)) {
                self.reject("Missing state hash; ")?
            }
            if !integer(&self.p("relayType")) || num(&self.p("relayType")) < 0.0 {
                self.reject("Invalid relay type; ")?
            }
            if truth(&self.p("autoRoll")) && !truth(&self.p("nextDlcRef")) {
                self.reject("autoRoll requires nextDlcRef; ")?
            }
            if truth(&self.p("relayBlob")) {
                let bundle = self.dep("Dlc.parseRelayBlob", vec![self.p("relayBlob")], false)?;
                let check = self.dep(
                    "Dlc.validateRelayBundle",
                    vec![bundle, self.p("stateHash")],
                    false,
                )?;
                if !truth(&prop(&check, "valid")?) {
                    self.reject(&(js_text(&check.get("reason")) + "; "))?
                }
            }
        }
        Ok(self.ret())
    }
    fn settle_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        if !self.active(23, 2)? {
            return Ok(self.ret());
        }
        let ty = self.p("settleType");
        if ![0.0, 1.0, 2.0, 3.0].iter().any(|i| strict(&ty, &n(*i))) {
            self.reject(&format!(
                "Invalid settleType: {}, expected 0-3; ",
                js_text(&ty)
            ))?;
            return Ok(self.ret());
        }
        let channel = self.dep("Channels.getChannel", vec![self.arg(0)], true)?;
        if !truth(&channel) {
            self.reject("Channel not found for sender; ")?;
            return Ok(self.ret());
        }
        let commits = self.dep("Channels.getCommitAddresses", vec![self.arg(0)], true)?;
        if !truth(&prop(&commits, "commitAddressA")?) || !truth(&prop(&commits, "commitAddressB")?)
        {
            self.reject("Sender is not a channel participant; ")?;
            return Ok(self.ret());
        }
        let king = strict(&ty, &n(3.0));
        if !king && !truth(&self.p("txidNeutralized1")) {
            self.reject("Missing txidNeutralized1 (trade reference); ")?;
            return Ok(self.ret());
        }
        if !king {
            let neutralized = self.dep(
                "Scaling.isSettlementNeutralized",
                vec![self.arg(0), self.p("txidNeutralized1")],
                true,
            )?;
            if truth(&neutralized) {
                self.reject("Settlement already superseded by later settlement; ")?;
                return Ok(self.ret());
            }
        }
        match num(&ty) as u8 {
            0 => {
                let trade = self.dep(
                    "Scaling.getTradeStatus",
                    vec![self.p("txidNeutralized1")],
                    true,
                )?;
                if strict(&prop(&trade, "status")?, &s("expired")) {
                    self.reject("Cannot keep-alive an expired trade; ")?
                }
            }
            1 => {
                if !truth(&self.p("markPrice")) || num(&self.p("markPrice")) <= 0.0 {
                    self.reject("Invalid mark price for close; ")?
                }
                if !truth(&self.p("txidNeutralized2")) {
                    self.reject("Missing txidNeutralized2 (keep-alive reference); ")?
                }
                if truth(&self.p("contractId")) {
                    let details =
                        self.dep("Contract.getContractInfo", vec![self.p("contractId")], true)?;
                    if truth(&details) {
                        let reference =
                            self.reference_price(self.p("contractId"), self.p("block"))?;
                        let check = price_band(&object(&[
                            ("tradePrice", self.p("markPrice")),
                            ("markPrice", reference),
                            ("leverage", details.get("leverage")),
                            ("bufferBps", n(100.0)),
                        ]))?;
                        if !truth(&check.get("valid")) {
                            self.reject("Mark price outside leverage bounds; ")?
                        }
                    }
                }
            }
            2 => {
                if nullish(&self.p("netAmount")) {
                    self.reject("Missing netAmount for NET_SETTLE; ")?
                }
                if nullish(&self.p("propertyId")) {
                    self.reject("Missing propertyId for NET_SETTLE; ")?
                }
                if matches!(self.p("columnAIsSeller"), V::Undefined) {
                    self.reject("Missing columnAIsSeller direction flag; ")?
                }
                if truth(&channel) && truth(&self.p("propertyId")) {
                    let column = if truth(&self.p("columnAIsSeller")) {
                        "A"
                    } else {
                        "B"
                    };
                    let balance = fallback_zero(prop(
                        &prop(&channel, column)?,
                        &js_text(&self.p("propertyId")),
                    )?);
                    if num(&self.p("netAmount")).abs() > num(&balance) {
                        self.reject(&format!(
                            "Net amount {} exceeds payer balance {}; ",
                            js_text(&self.p("netAmount")),
                            js_text(&balance)
                        ))?
                    }
                }
            }
            3 => {
                if nullish(&self.p("propertyId")) {
                    self.reject("Missing propertyId for KING_SETTLE; ")?
                }
                if nullish(&self.p("netAmount")) {
                    self.reject("Missing netAmount for KING_SETTLE; ")?
                }
                if matches!(self.p("aPaysBDirection"), V::Undefined)
                    && matches!(self.p("columnAIsSeller"), V::Undefined)
                {
                    self.reject("Missing direction flag for KING_SETTLE; ")?
                }
                if !truth(&self.p("channelRoot")) {
                    self.reject("Missing channelRoot for KING_SETTLE; ")?
                }
                if !matches!(self.p("blockStart"), V::Undefined)
                    && !matches!(self.p("blockEnd"), V::Undefined)
                    && js_less(&self.p("blockEnd"), &self.p("blockStart"))
                {
                    self.reject("Invalid block range for KING_SETTLE (start > end); ")?
                }
            }
            _ => {}
        }
        if truth(&self.p("expiryBlock")) && js_less(&self.p("expiryBlock"), &self.p("block")) {
            self.reject(&format!(
                "Settlement expired at block {}, current: {}; ",
                js_text(&self.p("expiryBlock")),
                js_text(&self.p("block"))
            ))?
        }
        Ok(self.ret())
    }
    fn king_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        if !self.active(31, 2)? {
            return Ok(self.ret());
        }
        let mut reason = None;
        if !truth(&self.p("blockStart")) || !truth(&self.p("blockEnd")) {
            reason = Some("Missing block range (blockStart/blockEnd); ")
        } else if js_less(&self.p("blockEnd"), &self.p("blockStart")) {
            reason = Some("Invalid block range: start > end; ")
        } else if js_less(&self.p("block"), &self.p("blockStart")) {
            reason = Some("Cannot settle future block range; ")
        }
        if let Some(r) = reason {
            self.reject(r)?;
            return Ok(self.ret());
        }
        let channel = self.dep("Channels.getChannel", vec![self.arg(0)], true)?;
        if !truth(&channel) {
            self.reject("Channel not found; ")?;
            return Ok(self.ret());
        }
        let commits = self.dep("Channels.getCommitAddresses", vec![self.arg(0)], true)?;
        if !truth(&prop(&commits, "commitAddressA")?) || !truth(&prop(&commits, "commitAddressB")?)
        {
            self.reject("Invalid channel participants; ")?;
            return Ok(self.ret());
        }
        let reason = if !truth(&self.p("propertyId")) || num(&self.p("propertyId")) < 1.0 {
            Some("Invalid propertyId; ")
        } else if matches!(self.p("netAmount"), V::Undefined) {
            Some("Missing netAmount; ")
        } else if matches!(self.p("aPaysBDirection"), V::Undefined) {
            Some("Missing payment direction (aPaysBDirection); ")
        } else if !truth(&self.p("channelRoot")) {
            Some("Missing channelRoot (founding UTXO reference); ")
        } else {
            None
        };
        if let Some(r) = reason {
            self.reject(r)?;
            return Ok(self.ret());
        }
        let balance = fallback_zero(prop(
            &prop(
                &channel,
                if truth(&self.p("aPaysBDirection")) {
                    "A"
                } else {
                    "B"
                },
            )?,
            &js_text(&self.p("propertyId")),
        )?);
        if num(&self.p("netAmount")).abs() > num(&balance) {
            self.reject(&format!(
                "Net amount {} exceeds payer balance {}; ",
                js_text(&self.p("netAmount")),
                js_text(&balance)
            ))?;
            return Ok(self.ret());
        }
        self.dep("DB.getDatabase", vec![s("scaling")], true)?;
        let doc = self.dep(
            "DB.findOneAsync",
            vec![object(&[("_id", self.arg(0))])],
            true,
        )?;
        let mut count = 0;
        if truth(&doc) {
            for key in ["settlements", "trades", "netSettles"] {
                let entries = doc.get(key);
                if truth(&entries) {
                    for item in array(&entries)? {
                        let block = prop(&item, "block")?;
                        let status = prop(&item, "status")?;
                        if js_less_equal(&self.p("blockStart"), &block)
                            && js_less_equal(&block, &self.p("blockEnd"))
                            && !strict(&status, &s("neutralized"))
                            && !strict(&status, &s("swept"))
                        {
                            count += 1
                        }
                    }
                }
            }
        }
        self.set("neutralizedCount", n(count as f64))?;
        Ok(self.ret())
    }
}

impl<B: SourceBackend> Engine<'_, B> {
    fn option_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        if !truth(&self.p("contractId")) {
            self.set("contractId", self.p("ticker"))?
        }
        if nullish(&self.p("blockHeight")) {
            self.set("blockHeight", self.p("block"))?
        }
        let ticker = self.p("contractId");
        let Some(primary) = option_ticker(&ticker) else {
            self.reject("Invalid primary ticker; ")?;
            return Ok(self.ret());
        };
        let series = num(&s(&primary.series_id));
        if !series.is_finite() {
            self.reject("Invalid option series id; ")?;
            return Ok(self.ret());
        }
        let combo = if truth(&self.p("comboTicker")) {
            let Some(t) = option_ticker(&self.p("comboTicker")) else {
                self.reject("Invalid combo ticker; ")?;
                return Ok(self.ret());
            };
            Some(t)
        } else {
            None
        };
        let info = self.dep("Contract.getContractInfo", vec![n(series)], true)?;
        if !truth(&info) {
            self.reject("Option series not found; ")?;
            return Ok(self.ret());
        }
        let collateral = info.get("collateralPropertyId");
        let commits = self.dep("Channels.getCommitAddresses", vec![self.arg(0)], true)?;
        let a = prop(&commits, "commitAddressA")?;
        let b = prop(&commits, "commitAddressB")?;
        if !truth(&a) && !truth(&b) {
            self.reject("Tx sender is not a channel address; ")?;
            return Ok(self.ret());
        }
        let channel = self.dep("Channels.getChannel", vec![self.arg(0)], true)?;
        let key = js_text(&collateral);
        let ba = fallback_zero(prop(&prop(&channel, "A")?, &key)?);
        let bb = fallback_zero(prop(&prop(&channel, "B")?, &key)?);
        let ta = self.dep("Tally.getTally", vec![a.clone(), collateral.clone()], true)?;
        let tb = self.dep("Tally.getTally", vec![b.clone(), collateral], true)?;
        let ea = js_add(&ba, &fallback_zero(ta.get("available")));
        let eb = js_add(&bb, &fallback_zero(tb.get("available")));
        let amount = num(&fallback_zero(self.p("amount")));
        let price = num(&fallback_zero(self.p("price")));
        let combo_amount = num(&fallback_zero(self.p("comboAmount")));
        let has_combo = truth(&self.p("comboTicker")) && truth(&self.p("comboAmount"));
        let strike = number_or_zero(primary.strike.unwrap_or(0.0));
        let mut net_premium = price * amount;
        let mut required;
        if has_combo {
            let secondary = combo.as_ref().ok_or(SourceRuleError::InvalidInput)?;
            let qty = js_min(amount, combo_amount);
            let width = (strike - number_or_zero(secondary.strike.unwrap_or(0.0))).abs();
            let notional = num(&if truth(&info.get("notionalValue")) {
                info.get("notionalValue")
            } else {
                n(1.0)
            })
            .abs();
            required = width * qty * notional;
            net_premium = price * amount - num(&fallback_zero(self.p("comboPrice"))) * combo_amount;
            if net_premium > 0.0 {
                required = js_max(0.0, required - net_premium)
            } else {
                required = net_premium.abs()
            }
            let leftover = (amount - combo_amount).abs();
            if leftover > 0.0 {
                required += strike / 10.0 * leftover
            }
        } else {
            required = strike / 10.0 * amount
        }
        self.set("netPremium", n(net_premium))?;
        self.dep("Margin.getInstance", vec![n(series)], true)?;
        let flag = self.p("columnAIsSeller");
        let a_seller = strict(&flag, &V::Bool(true))
            || strict(&flag, &n(1.0))
            || strict(&flag, &s("1"))
            || strict(&flag, &s("true"));
        let seller = if a_seller { a.clone() } else { b.clone() };
        let buyer = if a_seller { b } else { a };
        let effective_seller = if a_seller { ea.clone() } else { eb.clone() };
        let effective_buyer = if a_seller { eb } else { ea };
        let seller_blob = self.dep("Margin.margins.get", vec![seller], false)?;
        let buyer_blob = self.dep("Margin.margins.get", vec![buyer], false)?;
        let seller_bag = fallback_object(seller_blob.get("options"));
        let buyer_bag = fallback_object(buyer_blob.get("options"));
        let tk = js_text(&ticker);
        let seller_opt = fallback_option(seller_bag.get(&tk));
        let buyer_opt = fallback_option(buyer_bag.get(&tk));
        let seller_rf = crate::options::compute_reduce_flip(
            num(&fallback_zero(seller_opt.get("contracts"))),
            -amount.abs(),
        );
        let buyer_rf = crate::options::compute_reduce_flip(
            num(&fallback_zero(buyer_opt.get("contracts"))),
            amount.abs(),
        );
        self.set("sellerReducing", V::Bool(seller_rf.closed_qty > 0.0))?;
        self.set("buyerReducing", V::Bool(buyer_rf.closed_qty > 0.0))?;
        self.set(
            "closedContractsSeller",
            n(number_or_zero(seller_rf.closed_qty)),
        )?;
        self.set(
            "closedContractsBuyer",
            n(number_or_zero(buyer_rf.closed_qty)),
        )?;
        let rpnl = |rf: &crate::options::ReduceFlip, opt: &V| -> Eval<V> {
            if rf.closed_qty == 0.0 || rf.existing_side.is_none() {
                return Ok(V::BigNumber("0".into()));
            }
            let side = rf.existing_side.map(|s| s.as_source_name());
            let pnl = crate::options::rpnl_for_close(
                side,
                &finite_decimal(rf.closed_qty)?,
                &finite_decimal(price)?,
                &finite_decimal(num(&opt.get("avgPrice")))?,
            )
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            Ok(V::BigNumber(pnl.to_string()))
        };
        self.set("rpnlSeller", rpnl(&seller_rf, &seller_opt)?)?;
        self.set("rpnlBuyer", rpnl(&buyer_rf, &buyer_opt)?)?;
        if !has_combo {
            for (rf, bag) in [(&seller_rf, &seller_bag), (&buyer_rf, &buyer_bag)] {
                if rf.closed_qty > 0.0
                    && rf.existing_side == Some(crate::options::PositionSide::Long)
                    && has_short_bucket(bag, &primary, &tk)
                {
                    self.reject("Spread unwind requires combo trade; cover short leg first; ")?
                }
            }
        } else {
            let ctk = js_text(&self.p("comboTicker"));
            for (rf, bag, delta) in [
                (&seller_rf, &seller_bag, combo_amount.abs()),
                (&buyer_rf, &buyer_bag, -combo_amount.abs()),
            ] {
                let opt = fallback_option(bag.get(&ctk));
                let crf = crate::options::compute_reduce_flip(
                    num(&fallback_zero(opt.get("contracts"))),
                    delta,
                );
                let long = if rf.existing_side == Some(crate::options::PositionSide::Long) {
                    rf.closed_qty
                } else {
                    0.0
                } + if crf.existing_side == Some(crate::options::PositionSide::Long) {
                    crf.closed_qty
                } else {
                    0.0
                };
                let short = if rf.existing_side == Some(crate::options::PositionSide::Short) {
                    rf.closed_qty
                } else {
                    0.0
                } + if crf.existing_side == Some(crate::options::PositionSide::Short) {
                    crf.closed_qty
                } else {
                    0.0
                };
                if long > 0.0 && short < long {
                    self.reject("Combo unwind must cover shorts first; ")?
                }
            }
        }
        let mut portfolio_delta = 0.0;
        if seller_rf.closed_qty > 0.0 {
            let mark = num(&self.reference_price(n(series), self.p("blockHeight"))?);
            let mark = number_or_zero(mark);
            let before = portfolio(&seller_bag, mark)?;
            let mut after = seller_bag.clone();
            let mut primary_opt = fallback_option(after.get(&tk));
            primary_opt.insert(
                "contracts",
                n(num(&fallback_zero(primary_opt.get("contracts"))) - amount.abs()),
            )?;
            after.insert(&tk, primary_opt)?;
            if has_combo {
                let ctk = js_text(&self.p("comboTicker"));
                let mut op = fallback_option(after.get(&ctk));
                op.insert(
                    "contracts",
                    n(num(&fallback_zero(op.get("contracts"))) + combo_amount.abs()),
                )?;
                after.insert(&ctk, op)?
            }
            let after = portfolio(&after, mark)?;
            portfolio_delta = number_fixed(after - before, 8)?
        }
        self.set(
            "creditMargin",
            n(if seller_rf.closed_qty > 0.0 {
                portfolio_delta
            } else {
                required
            }),
        )?;
        if seller_rf.closed_qty <= 0.0 && num(&effective_seller) < required {
            self.reject("Insufficient collateral for option margin; ")?
        }
        if seller_rf.closed_qty > 0.0
            && portfolio_delta > 0.0
            && num(&effective_seller) < portfolio_delta
        {
            self.reject("Insufficient collateral for option margin transition; ")?
        }
        if net_premium > 0.0 && num(&effective_buyer) < net_premium {
            self.reject("Insufficient collateral for option premium; ")?
        }
        if truth(&self.p("blockHeight")) && num(&self.p("blockHeight")) >= primary.expiry_block {
            self.reject("Option already expired; ")?
        }
        Ok(self.ret())
    }
}
fn option_ticker(v: &V) -> Option<crate::options::ParsedTicker> {
    if let V::String(t) = v {
        crate::options::parse_ticker(t)
    } else {
        None
    }
}
fn number_or_zero(n: f64) -> f64 {
    if n == 0.0 || n.is_nan() {
        0.0
    } else {
        n
    }
}
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else {
        a.max(b)
    }
}
fn fallback_object(v: V) -> V {
    if truth(&v) {
        v
    } else {
        V::object()
    }
}
fn fallback_option(v: V) -> V {
    if truth(&v) {
        v
    } else {
        object(&[
            ("contracts", n(0.0)),
            ("avgPrice", n(0.0)),
            ("margin", n(0.0)),
        ])
    }
}
fn has_short_bucket(bag: &V, primary: &crate::options::ParsedTicker, ignored: &str) -> bool {
    if let V::Object(entries) = bag {
        for (ticker, op) in entries {
            if ticker == ignored {
                continue;
            }
            if let Some(meta) = crate::options::parse_ticker(ticker) {
                if meta.expiry_block == primary.expiry_block
                    && meta.option_type == primary.option_type
                    && num(&fallback_zero(op.get("contracts"))) < 0.0
                {
                    return true;
                }
            }
        }
    }
    false
}
fn portfolio(bag: &V, spot: f64) -> Eval<f64> {
    let mut legs = Vec::new();
    if let V::Object(entries) = bag {
        for (ticker, op) in entries {
            let Some(meta) = crate::options::parse_ticker(ticker) else {
                continue;
            };
            let q = num(&fallback_zero(op.get("contracts")));
            if q == 0.0 || q.is_nan() {
                continue;
            }
            legs.push(crate::options::OptionPosition {
                option_type: meta.option_type.as_source_name().into(),
                expiry_block: number_or_zero(meta.expiry_block),
                strike: finite_decimal(number_or_zero(meta.strike.unwrap_or(0.0)))?,
                qty: finite_decimal(q)?,
            })
        }
    }
    let result = crate::options::portfolio_maintenance(&legs, &finite_decimal(spot)?)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    Ok(result.to_string().parse().unwrap_or(f64::NAN))
}
fn number_fixed(value: f64, places: u32) -> Eval<f64> {
    if !value.is_finite() || value.abs() >= 1e21 {
        return Ok(value);
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mantissa = if exponent == 0 {
        bits & ((1u64 << 52) - 1)
    } else {
        (bits & ((1u64 << 52) - 1)) | (1u64 << 52)
    };
    let binary = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut d = Decimal::from(mantissa as i128);
    let two = Decimal::from(2i128);
    if binary >= 0 {
        for _ in 0..binary {
            d = d
                .checked_mul(&two)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
        }
    } else {
        let mut divisor = Decimal::from(1i128);
        for _ in 0..-binary {
            divisor = divisor
                .checked_mul(&two)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
        }
        d = d
            .checked_div(&divisor, (-binary) as u32, Rounding::HalfUp)
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
    }
    if value.is_sign_negative() {
        d = d.negated()
    }
    let q = d
        .quantize(places, Rounding::HalfUp)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    if q.is_zero() && value < 0.0 {
        Ok(-0.0)
    } else {
        Ok(q.to_string().parse().unwrap_or(f64::NAN))
    }
}

fn validity_domain(value: &SourceValue) -> Result<(), SourceRuleError> {
    match value {
        SourceValue::Object(fields) => {
            for (k, v) in fields {
                if matches!(k.as_str(), "toString" | "valueOf") {
                    return Err(SourceRuleError::InvalidInput);
                }
                validity_domain(v)?
            }
        }
        SourceValue::Array(values) => {
            for v in values {
                validity_domain(v)?
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn evaluate_unverified<B: SourceBackend>(
    call: &SourceCall,
    backend: &mut B,
) -> Result<SourceEvaluation, SourceRuleError> {
    if !METHODS.contains(&call.method.as_str()) {
        return Err(SourceRuleError::UnknownMethod);
    }
    if call.arguments.len() > MAX_ARGUMENTS {
        return Err(SourceRuleError::Capacity);
    }
    V::Array(call.arguments.clone()).validate()?;
    validity_domain(&V::Array(call.arguments.clone()))?;
    let pi = if matches!(
        call.method.as_str(),
        "validatePayToTokens"
            | "validateTradeBaiUrbun"
            | "validateTradeMurabaha"
            | "validateBatchMoveZkRollup"
    ) {
        0
    } else if call.method == "checkWhitelist" {
        3
    } else {
        1
    };
    if (call.method.starts_with("validate") || call.method == "checkWhitelist")
        && matches!(
            call.arguments.get(pi),
            Some(V::Array(_) | V::Promise | V::BigNumber(_))
        )
    {
        return Err(SourceRuleError::InvalidInput);
    }
    let mut engine = Engine {
        backend,
        args: call.arguments.clone(),
        pi,
        trace: Vec::new(),
        charged: 0,
        deferred: Vec::new(),
        global_writes: Vec::new(),
    };
    let result = engine.evaluate(&call.method);
    let outcome = match result {
        Ok(v) => {
            v.validate()?;
            SourceOutcome::Returned(v)
        }
        Err(Fault::Source(e)) => SourceOutcome::Thrown(e),
        Err(Fault::Local(e)) => return Err(e),
    };
    V::Array(engine.args.clone()).validate()?;
    Ok(SourceEvaluation {
        outcome,
        arguments_after: engine.args,
        dependencies: engine.trace,
        deferred_rejections: engine.deferred,
        global_writes: engine.global_writes,
    })
}

fn finite_decimal(n: f64) -> Eval<Decimal> {
    Decimal::from_str(&js_text(&V::number(n)))
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
fn fixed2(v: Bn) -> Eval<String> {
    let v = v.q(2, Rounding::HalfUp)?;
    let mut text = v.text();
    if !v.finite() {
        return Ok(text);
    }
    if let Some((_, fraction)) = text.split_once('.') {
        let count = fraction.len();
        text.push_str(&"0".repeat(2usize.saturating_sub(count)));
    } else {
        text.push_str(".00")
    }
    Ok(text)
}
fn price_band(config: &V) -> Eval<V> {
    let price = config.get("tradePrice");
    let mark = config.get("markPrice");
    let leverage = config.get("leverage");
    if nullish(&price) || nullish(&mark) || !truth(&leverage) || num(&leverage) <= 0.0 {
        return Ok(object(&[
            ("valid", V::Bool(false)),
            ("reason", s("Missing tradePrice, markPrice, or leverage")),
        ]));
    }
    let price = Bn::new(&price)?;
    let mark = Bn::new(&mark)?;
    let zero = Bn::new(&n(0.0))?;
    if !price.finite()
        || !mark.finite()
        || mark
            .compare(&zero)
            .is_some_and(|o| o != core::cmp::Ordering::Greater)
    {
        return Ok(object(&[
            ("valid", V::Bool(false)),
            ("reason", s("Invalid numeric values for price or mark")),
        ]));
    }
    let deviation = price.op(mark.clone(), 1)?.abs().op(mark, 3)?;
    let max_move = calc(&n(1.0), &leverage, 3)?.op(Bn::new(&n(2.0))?, 3)?;
    let buffer = config.get("bufferBps");
    let buffer = if matches!(buffer, V::Undefined) {
        n(65.0)
    } else {
        buffer
    };
    let allowed = max_move.op(calc(&buffer, &n(10_000.0), 3)?, 1)?;
    if allowed
        .compare(&zero)
        .is_some_and(|o| o != core::cmp::Ordering::Greater)
    {
        return Ok(object(&[
            ("valid", V::Bool(false)),
            (
                "reason",
                s("Leverage buffer exceeds allowable price movement"),
            ),
        ]));
    }
    if deviation
        .compare(&allowed)
        .is_some_and(|o| o != core::cmp::Ordering::Less)
    {
        let d = fixed2(deviation.op(Bn::new(&n(100.0))?, 2)?)?;
        let a = fixed2(allowed.op(Bn::new(&n(100.0))?, 2)?)?;
        return Ok(object(&[
            ("valid", V::Bool(false)),
            (
                "reason",
                s(&format!(
                    "Trade price deviates {d}% from mark; max allowed {a}%"
                )),
            ),
        ]));
    }
    Ok(object(&[("valid", V::Bool(true))]))
}
fn margin_check(balance: V, required: V, label: V, flip: Option<V>) -> V {
    if js_less(&balance, &required) && flip.as_ref().is_none_or(|_| num(&required) > 0.0) {
        let extra = flip.map_or(String::new(), |v| format!(" for {}", js_text(&v)));
        object(&[
            ("valid", V::Bool(false)),
            (
                "reason",
                s(&format!(
                    "Insufficient channel balance in {}{extra} (need {}, found {}); ",
                    js_text(&label),
                    js_text(&required),
                    js_text(&balance)
                )),
            ),
        ])
    } else {
        object(&[("valid", V::Bool(true)), ("reason", s(""))])
    }
}
fn validity_ticker(value: &V) -> V {
    let V::String(text) = value else {
        return V::Null;
    };
    if text.is_empty() {
        return V::Null;
    }
    let p: Vec<_> = text.split('-').collect();
    if p.len() < 3 {
        return V::Null;
    }
    object(&[
        ("seriesId", s(p[0])),
        ("expiryBlock", n(parse_integer(&s(p[1]), true))),
        (
            "type",
            match p[2] {
                "C" => s("Call"),
                "P" => s("Put"),
                _ => V::Null,
            },
        ),
        (
            "strike",
            p.get(3)
                .filter(|v| !v.is_empty())
                .map_or(V::Null, |v| n(parse_float(&s(v)))),
        ),
        ("raw", value.clone()),
    ])
}

impl<B: SourceBackend> Engine<'_, B> {
    fn evaluate(&mut self, method: &str) -> Eval<V> {
        if self.args.len() > 1 {
            let pair = match method {
                "validatePayToTokens" => Some(("tallyMap", "hasSufficientBalance")),
                "validateTradeBaiUrbun" | "validateTradeMurabaha" => {
                    Some(("channelRegistry", "isValidChannel"))
                }
                "validateBatchMoveZkRollup" => Some(("zkVerifier", "verifyProof")),
                _ => None,
            };
            if let Some((var, key)) = pair {
                let supplied = self.arg(1);
                return Err(throw(
                    "TypeError",
                    if nullish(&supplied) {
                        format!(
                            "Cannot read properties of {} (reading '{key}')",
                            js_text(&supplied)
                        )
                    } else {
                        format!("{var}.{key} is not a function")
                    },
                ));
            }
        }
        match method {
            "isActivated" => Ok(V::Bool(true)),
            "isValidNumber" => Ok(V::Bool(valid_number(&self.arg(0)))),
            "isRestrictedProceduralToken" => Ok(V::Bool(restricted(&self.arg(0)))),
            "isTradePriceWithinLeverageBounds" => price_band(&self.arg(0)),
            "hasReferencePrice" => self.reference_price(self.arg(0), self.arg(1)),
            "hasSufficientChannelMargin" => {
                Ok(margin_check(self.arg(0), self.arg(1), self.arg(2), None))
            }
            "checkFlipMargin" => Ok(margin_check(
                self.arg(0),
                self.arg(1),
                self.arg(2),
                Some(self.arg(3)),
            )),
            "parseTicker" => Ok(validity_ticker(&self.arg(0))),
            "isListed" => Ok(V::Bool(self.any_list(
                array(&self.arg(0))?,
                self.arg(1),
                false,
            )?)),
            "checkWhitelist" => Err(missing("isListed")),
            "validateActivateTradeLayer" => self.activate_rule(),
            "validateTokenIssue" => self.issue_rule(),
            "validateSend" => self.send_rule(),
            "validateTradeTokenForUTXO" => self.utxo_rule(),
            "validateCommit" => self.commit_rule(),
            "validateOnChainTokenForToken" => self.spot_rule(),
            "validateCancelOrder" => self.cancel_rule(),
            "validateCreateWhitelist" => {
                self.begin(true)?;
                self.active(7, 0)?;
                if !truth(&self.p("backupAddress"))
                    || !matches!(self.p("backupAddress"), V::String(_))
                {
                    self.reject("Invalid backup address; ")?
                }
                self.address(self.p("backupAddress"), true, false)?;
                if !matches!(self.p("name"), V::String(_)) {
                    self.reject("Invalid name; ")?
                }
                Ok(self.ret())
            }
            "validateUpdateAdmin" => self.update_admin_rule(),
            "validateIssueOrRevokeAttestation" => self.attestation_rule(),
            "validateAMMPool" => self.amm_rule(),
            "validateGrantManagedToken" => self.managed_rule(false),
            "validateRedeemManagedToken" => self.managed_rule(true),
            "validateCreateOracle" => {
                self.begin(true)?;
                self.active(13, 0)?;
                Ok(self.ret())
            }
            "validatePublishOracleData" => {
                self.begin(true)?;
                let admin = self.dep(
                    "Oracle.isAdmin",
                    vec![self.arg(0), self.p("oracleId")],
                    true,
                )?;
                self.set("valid", admin.clone())?;
                if loose(&admin, &V::Bool(false)) {
                    self.set("reason", s("Sender is not admin of the specified oracle; "))?
                }
                for (k, label) in [
                    ("price", "Price"),
                    ("high", "High"),
                    ("low", "Low"),
                    ("close", "Close"),
                ] {
                    if k == "high" && !matches!(self.arg(self.pi), V::Object(_)) {
                        return Err(throw(
                            "TypeError",
                            format!(
                                "Cannot use 'in' operator to search for 'high' in {}",
                                js_text(&self.arg(self.pi))
                            ),
                        ));
                    }
                    let value = self.p(k);
                    let present = matches!(self.arg(self.pi),V::Object(ref fields)if fields.iter().any(|(name,_)|name==k));
                    if (k == "price" || present) && !valid_number(&value) {
                        self.reject(&format!("{label} is not a valid number; "))?
                    }
                }
                self.active(14, 0)?;
                let oracle = self.dep("Oracle.getOracleInfo", vec![self.p("oracleId")], true)?;
                if !truth(&oracle) {
                    self.reject("Oracle not found; ")?
                }
                Ok(self.ret())
            }
            "validateCloseOracle" => {
                self.begin(true)?;
                let admin = self.dep(
                    "Oracle.isAdmin",
                    vec![self.arg(0), self.p("oracleId")],
                    false,
                )?;
                self.set("valid", admin.clone())?;
                if !truth(&admin) {
                    self.set("reason", s("Sender is not admin of the specified oracle; "))?
                }
                self.active(15, 0)?;
                Ok(self.ret())
            }
            "validateCreateContractSeries" => self.create_contract_rule(),
            "validateExerciseDerivative" => {
                self.begin(true)?;
                self.active(17, 0)?;
                Err(missing("derivativeRegistry"))
            }
            "validateTradeContractOnchain" => self.contract_rule(),
            "validateTradeContractChannel" => self.contract_channel_rule(),
            "validateTradeTokensChannel" => self.token_channel_rule(),
            "validateWithdrawal" => self.withdrawal_rule(),
            "validateTransfer" => self.transfer_rule(),
            "validateMintSynthetic" => self.mint_rule(),
            "validateRedeemSynthetic" => self.redeem_rule(),
            "validatePayToTokens" => {
                let reply = self.dep(
                    "GuestTally.hasSufficientBalance",
                    vec![
                        self.p("senderAddress"),
                        self.p("propertyIdUsed"),
                        self.p("amount"),
                    ],
                    false,
                )?;
                prop(&reply, "hasSufficient")
            }
            "validateOptionTrade" => self.option_rule(),
            "validateTradeBaiUrbun" | "validateTradeMurabaha" => {
                let channel = self.dep(
                    "GuestChannels.isValidChannel",
                    vec![self.p("channelAddress")],
                    false,
                )?;
                let mut args = vec![self.p("propertyIdDownPayment")];
                let murabaha = method == "validateTradeMurabaha";
                if murabaha {
                    args.push(self.p("downPaymentPercent"))
                }
                for key in ["propertyIdToBeSold", "price", "amount", "expiryBlock"] {
                    args.push(self.p(key))
                }
                if murabaha {
                    args.push(self.p("installmentInterval"))
                }
                let terms = self.dep(
                    if murabaha {
                        "Murabaha.isValidMurabahaTerms"
                    } else {
                        "Bai.isValidBaiUrbunTerms"
                    },
                    args,
                    false,
                )?;
                Ok(if truth(&channel) { terms } else { channel })
            }
            "validateStakeFraudProof" => self.stake_rule(),
            "validateSettleChannelPNL" => self.settle_rule(),
            "validateKingSettle" => self.king_rule(),
            "validateBatchMoveZkRollup" => {
                let proof = self.dep("Zk.verifyProof", vec![self.p("zkProof")], false)?;
                let payments = self.dep(
                    "GuestTally.arePaymentsValid",
                    vec![self.p("payments")],
                    false,
                )?;
                Ok(if truth(&proof) { payments } else { proof })
            }
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
    fn reference_price(&mut self, id: V, height: V) -> Eval<V> {
        let c = self.dep("Contract.getContractInfo", vec![id], true)?;
        if !truth(&c) {
            return Ok(V::Bool(false));
        }
        if truth(&c.get("underlyingOracleId")) {
            return self.dep(
                "Oracle.getOraclePrice",
                vec![c.get("underlyingOracleId")],
                true,
            );
        }
        let pairs = prop(&c, "onChainData")?;
        if num(&prop(&pairs, "length")?) > 0.0 {
            let price = self.dep(
                "Volume.getLastPrice",
                vec![c.get("indexPair"), height.clone()],
                true,
            )?;
            if !nullish(&price) && num(&price) > 0.0 {
                return Ok(price);
            }
        }
        let mark = c.get("firstMark");
        let price = mark.get("price");
        if truth(&mark)
            && num(&mark.get("blockHeight")) <= num(&height)
            && matches!(price, V::Number(_))
            && num(&price).is_finite()
            && num(&price) > 0.0
        {
            return Ok(V::Bool(true));
        }
        Ok(V::Bool(false))
    }
    fn activate_rule(&mut self) -> Eval<V> {
        self.begin(false)?;
        let raw = self.p("txTypesToActivate");
        let numbers = if let V::Array(values) = raw {
            values
                .iter()
                .map(num)
                .filter(|n| !n.is_nan())
                .collect::<Vec<_>>()
        } else if !matches!(self.p("txTypeToActivate"), V::Undefined) && !matches!(raw, V::Null) {
            let n = num(&raw);
            if n.is_nan() {
                vec![]
            } else {
                vec![n]
            }
        } else {
            vec![]
        };
        if numbers.is_empty() {
            self.replace_reason("Tx Type contains non-integer or invalid values")?;
            return Ok(self.ret());
        }
        let admin = self.admin()?;
        if !strict(&self.arg(0), &admin) {
            self.replace_reason("Not sent from admin address")?;
            return Ok(self.ret());
        }
        if numbers.iter().any(|n| *n > 35.0 || *n < 0.0) {
            self.replace_reason("Tx Type out of bounds")?;
            return Ok(self.ret());
        }
        self.set(
            "txTypesToActivate",
            V::Array(numbers.into_iter().map(n).collect()),
        )?;
        Ok(self.ret())
    }
    fn issue_rule(&mut self) -> Eval<V> {
        self.begin(false)?;
        self.active(1, 0)?;
        let amount = self.p("initialAmount");
        if !integer(&amount) || num(&amount) <= 0.0 {
            self.reject("Invalid initial amount; ")?
        }
        let ticker = self.p("ticker");
        if !matches!(&ticker,V::String(t)if t.encode_utf16().count()<=6) {
            self.reject("Invalid ticker; ")?
        }
        let exists = self.dep("Property.doesTickerExist", vec![ticker.clone()], true)?;
        if truth(&exists) {
            self.reject("Ticker already exists; ")?
        }
        let V::String(text) = ticker else {
            return Err(throw(
                "TypeError",
                if nullish(&ticker) {
                    format!(
                        "Cannot read properties of {} (reading 'startsWith')",
                        js_text(&ticker)
                    )
                } else {
                    "params.ticker.startsWith is not a function".into()
                },
            ));
        };
        if text.starts_with('s') {
            self.reject("Ticker cannot start with \"s\"; ")?
        }
        if strict(&self.p("type"), &s("native")) {
            self.reject("Invalid property ID for native type; ")?
        }
        if strict(&self.p("type"), &s("vesting")) {
            self.reject("Invalid property ID for vesting type; ")?
        }
        let procedural = self.p("proceduralType");
        if !nullish(&procedural) {
            if !truth(&self.p("managed")) {
                self.reject("Procedural token type requires managed issuance; ")?
            }
            if !integer(&procedural) || num(&procedural) < 0.0 {
                self.reject("Invalid procedural type enum; ")?
            }
        }
        Ok(self.ret())
    }
    fn send_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(2, 0)?;
        self.address(self.p("address"), false, false)?;
        let ids = list(self.p("propertyIds"));
        let mut property = V::Null;
        for id in ids {
            let p = self.property(id.clone())?;
            if nullish(&p) {
                self.replace_reason("propertyId not found in Property List")?;
                return Ok(self.ret());
            }
            if restricted(&p) {
                self.reject(&format!(
                    "Property {} is restricted procedural and non-transferable; ",
                    js_text(&id)
                ))?
            }
            if !truth(&property) {
                property = p
            }
        }
        let admin = self.admin()?;
        let pid = self.p("propertyIds");
        if !loose(&self.arg(0), &admin) && (loose(&pid, &n(1.0)) || loose(&pid, &n(4.0))) {
            let banned = self.country(self.arg(0))?;
            let accredited = self.listed(n(3.0), self.arg(0), false)?;
            if banned
                && !accredited
                && truth(&self.dep("Activation.areActivationsAboveThreshold", vec![], false)?)
            {
                self.reject("Sender cannot handle TL or TLI from a banned country or lacking country code attestation")?
            }
        }
        let tally = self.dep("Tally.getTally", vec![self.arg(0), pid.clone()], true)?;
        if loose(&tally, &n(0.0)) {
            self.dep("Tally.getAddressBalances", vec![self.arg(0)], true)?;
        }
        let available = prop(&tally, "available")?;
        if js_less(&available, &self.p("amounts")) || matches!(available, V::Undefined) {
            self.reject("Insufficient available balance")?
        }
        let lists = list(prop(&property, "whitelistId")?);
        let attest = self.dep(
            "ClearList.getAttestations",
            vec![self.p("recipientAddress")],
            true,
        )?;
        let attest = match attest {
            V::Array(a) => a,
            _ => {
                return Err(throw(
                    "TypeError",
                    "recipientAttestations.map is not a function",
                ))
            }
        };
        let mut recipient_lists = Vec::new();
        for a in attest {
            recipient_lists.push(prop(&prop(&a, "data")?, "clearlistId")?)
        }
        let mut send = false;
        for id in lists {
            if self.listed(id, self.arg(0), false)? {
                send = true
            }
        }
        if !send && !loose(&property.get("whitelistId"), &n(0.0)) {
            self.reject("Sender address not whitelisted in clearlist")?
        }
        let receive = self.any_list(recipient_lists, self.p("recipientAddress"), false)?;
        if !receive && !loose(&property.get("whitelistId"), &n(0.0)) {
            self.reject("Recipient address not whitelisted in clearlist; ")?
        }
        if matches!(self.p("amounts"), V::Array(_)) {
            return Err(missing("isValidAmount"));
        }
        if !valid_number(&self.p("amounts")) {
            self.reject("Invalid or missing amount; ")?
        }
        Ok(self.ret())
    }
    fn utxo_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.active(3, 1)?;
        let property = self.property(self.p("propertyId"))?;
        if nullish(&property) {
            self.reject("Invalid property ID; ")?
        }
        if restricted(&property) {
            self.reject("Restricted procedural token cannot be token-traded; ")?
        }
        if !valid_number(&self.p("amount")) {
            self.reject("Invalid or missing token amount; ")?;
            return Ok(self.ret());
        }
        let balance = self.dep(
            "Tally.hasSufficientChannel",
            vec![self.arg(0), self.p("propertyId"), self.p("amount")],
            true,
        )?;
        if !truth(&prop(&balance, "hasSufficient")?) {
            self.set("valid", V::Bool(true))?;
            self.set(
                "reason",
                s(&(js_text(&self.p("reason")) + " Insufficient Tokens ")),
            )?;
            self.set(
                "amount",
                n(num(&self.p("amount")) - num(&balance.get("shortfall"))),
            )?
        }
        if !truth(&self.p("amount")) || num(&self.p("amount")).is_nan() {
            self.reject("Invalid amount")?
        }
        let sats = self.p("satsExpected");
        if !integer(&sats) || num(&sats) < 0.0 {
            self.set("valid", V::Bool(true))?;
            self.set(
                "reason",
                s(&(js_text(&self.p("reason")) + "Invalid sats expected; ")),
            )?
        }
        let outputs = self.arg(3);
        let len = prop(&outputs, "length")?;
        if num(&len) == 0.0 {
            self.reject("No outputs; ")?;
            return Ok(V::Undefined);
        }
        let outs = if matches!(outputs, V::Array(_)) && truth(&len) {
            outputs
        } else {
            self.dep("TxUtils.getTransactionOutputs", vec![self.arg(2)], true)?
        };
        let mut received = 0.0;
        if integer(&self.p("payToAddress")) {
            let i = num(&self.p("payToAddress")) as usize;
            let out = if let V::Array(a) = outs {
                a.get(i).cloned().unwrap_or(V::Undefined)
            } else {
                V::Undefined
            };
            received += num(&prop(&out, "satoshis")?)
        } else {
            self.set("valid", V::Bool(true))?;
            self.set(
                "reason",
                s(&(js_text(&self.p("reason")) + "missing payToAddress, defaulting to 1")),
            )?;
            self.set("payToAddress", n(1.0))?;
        }
        self.set("satsReceived", n(received))?;
        let expected = calc(&sats, &n(1e8), 3)?.q(8, Rounding::HalfUp)?;
        let price = expected
            .clone()
            .op(Bn::new(&self.p("amount"))?, 3)?
            .q(8, Rounding::HalfUp)?;
        self.set("price", bnum(price))?;
        if received < expected.number() {
            self.set("valid", V::Bool(true))?;
            self.set(
                "reason",
                s(&(js_text(&self.p("reason"))
                    + "Received LTC (undefined) is less than expected; ")),
            )?;
            self.set("paymentPercent", n(f64::NAN))?
        } else {
            self.set("paymentPercent", n(100.0))?
        }
        if !integer(&self.p("tokenOutput")) {
            self.set("valid", V::Bool(true))?;
            self.set(
                "reason",
                s(&(js_text(&self.p("reason")) + "tokenOutput not an integer")),
            )?;
            if loose(&self.p("payToAddress"), &n(0.0)) {
                self.set("tokenOutput", n(1.0))?
            } else {
                return Err(missing("reference"));
            }
            return Err(missing("reference"));
        }
        Ok(self.ret())
    }
    fn commit_rule(&mut self) -> Eval<V> {
        self.begin(true)?;
        self.set("txid", self.arg(2))?;
        self.reference("channelAddress", true)?;
        if !loose(&self.p("channelAddress"), &s("")) {
            self.address(self.p("channelAddress"), false, false)?
        }
        if !valid_number(&self.p("amount")) {
            self.reject("Invalid or missing amount; ")?;
            return Ok(self.ret());
        }
        let sufficient = self.dep(
            "Tally.hasSufficientBalance",
            vec![
                self.p("senderAddress"),
                self.p("propertyId"),
                self.p("amount"),
            ],
            true,
        )?;
        if loose(&sufficient.get("hasSufficient"), &V::Bool(false)) {
            self.reject("Insufficient token balance for commitment")?
        }
        self.active(4, 0)?;
        if [2.0, 3.0]
            .iter()
            .any(|n| loose(&self.p("propertyId"), &V::number(*n)))
        {
            self.replace_reason("Cannot trade vesting tokens")?
        }
        let property = self.property(self.p("propertyId"))?;
        if nullish(&property) {
            self.replace_reason("Null returning for propertyData")?;
            return Ok(self.ret());
        }
        if restricted(&property) {
            self.reject(
                "Restricted procedural token cannot be committed/traded in token channels; ",
            )?
        }
        let passes = self.any_list(list(property.get("whitelistId")), self.arg(0), false)?;
        let admin = self.admin()?;
        if !loose(&self.arg(0), &admin) {
            let banned = self.country(self.arg(0))?;
            if (1..=4).any(|i| loose(&self.p("propertyId"), &n(i as f64))) {
                let acc = self.listed(n(3.0), self.arg(0), false)?;
                if banned
                    && !acc
                    && truth(&self.dep("Activation.areActivationsAboveThreshold", vec![], false)?)
                {
                    self.reject("Sender cannot handle TL or TLI from a banned country or lacking country code attestation")?
                }
            }
        }
        let channel = self.dep("Channels.getChannel", vec![self.p("channelAddress")], true)?;
        if truth(&channel) {
            let tx = self.dep("TxUtils.getRawTransaction", vec![self.arg(2)], true)?;
            let vins = prop(&tx, "vin")?;
            let vin = if let V::Array(a) = vins {
                a.first().cloned().unwrap_or(V::Undefined)
            } else {
                V::Undefined
            };
            let kind = self.dep("TxUtils.getAddressTypeUniversal", vec![self.arg(0)], false)?;
            let keys = self.dep("TxUtils.extractPubkeyByType", vec![vin, kind], false)?;
            if !truth(&keys) || num(&keys.get("length")) == 0.0 {
                self.reject("Could not extract pubkey from sender's input.")?;
                return Ok(self.ret());
            }
            let cp = channel.get("channelPubkeys");
            let keys = [cp.get("A"), cp.get("B")]
                .into_iter()
                .filter(truth)
                .collect::<Vec<_>>();
            if keys.len() == 2 {
                self.dep("Vesting.getInstance", vec![], true)?;
                let chain = self.dep("Vesting.getChain", vec![], false)?;
                let test = self.dep("Vesting.getTest", vec![], false)?;
                let a = self.dep(
                    "TxUtils.createMultisig",
                    vec![
                        keys[0].clone(),
                        keys[1].clone(),
                        chain.clone(),
                        test.clone(),
                        self.arg(0),
                    ],
                    true,
                )?;
                let b = self.dep(
                    "TxUtils.createMultisig",
                    vec![keys[1].clone(), keys[0].clone(), chain, test, self.arg(0)],
                    true,
                )?;
                if !strict(&a, &self.p("channelAddress")) && !strict(&b, &self.p("channelAddress"))
                {
                    self.reject("Commiter is not a party to the multisig channel.")?;
                    return Ok(self.ret());
                }
            }
        }
        if !passes && !loose(&property.get("whitelistId"), &n(0.0)) {
            self.reject("Sender address not listed in clearlist for the token")?
        }
        if !matches!(self.p("payEnabled"), V::Bool(_)) {
            self.reject("payEnabled is not a boolean. ")?
        }
        let clear = self.p("clearLists");
        if truth(&clear) {
            let V::Array(a) = clear else {
                return Err(throw(
                    "TypeError",
                    "params.clearLists.filter is not a function",
                ));
            };
            if a.iter().any(|v| !integer(v)) {
                self.reject("clearLists contains non-integer values. ")?
            }
        }
        let id = self.p("commitClearlistId");
        if !nullish(&id) {
            if !integer(&id) || num(&id) < 0.0 {
                self.reject("commitClearlistId must be a non-negative integer. ")?
            } else if num(&id) > 0.0 && !self.listed(id.clone(), self.arg(0), true)? {
                self.reject(&format!(
                    "Sender address not listed in commit clearlist {}; ",
                    js_text(&id)
                ))?
            }
        }
        Ok(self.ret())
    }
}
