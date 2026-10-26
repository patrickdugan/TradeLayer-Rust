//! Literal source contract placement and matching over the existing queue owner.
//! Reserve and match settlement remain explicit, mutable owner capabilities.
use crate::contract_queue_source_owner::{parse_book_json_unverified, ContractQueueSourceOwner};
use crate::decimal::{Decimal, Rounding};
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
use core::cmp::Ordering;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98";

/// A genuine dependency implementation can retain the same queue while
/// performing settlement or harvesting ordered database effects. There is no
/// blanket implementation: an application may override this owner-aware hook.
pub trait ContractPlacementBackend: SourceBackend {
    fn invoke_in_queue(
        &mut self,
        _queue: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.invoke(call)
    }
}
#[derive(Debug)]
enum Fault {
    Thrown(SourceException),
    Local(SourceRuleError),
    Boundary(SourceRuleError),
}
type Eval<T> = Result<T, Fault>;
impl From<SourceRuleError> for Fault {
    fn from(v: SourceRuleError) -> Self {
        Self::Local(v)
    }
}
fn type_error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "TypeError".into(),
        message: message.into(),
    })
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
fn object(fields: &[(&str, V)]) -> V {
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
            let f = f64::from_bits(*b);
            f != 0.0 && !f.is_nan()
        }
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::String(a), V::String(b)) => a == b,
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        _ => false,
    }
}
fn text(v: &V) -> Eval<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(s) => s.clone(),
        V::BigNumber(t) => Bn::parse(t)?.text()?,
        V::Promise => "[object Promise]".into(),
        V::Number(b) => {
            let f = f64::from_bits(*b);
            if f.is_nan() {
                "NaN".into()
            } else if f == f64::INFINITY {
                "Infinity".into()
            } else if f == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(f)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?
            }
        }
        V::Array(a) => a
            .iter()
            .map(|v| {
                if matches!(v, V::Undefined | V::Null) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<Eval<Vec<_>>>()?
            .join(","),
        V::Object(o) => {
            if o.iter().any(|(k, _)| k == "toString") {
                return Err(type_error("Cannot convert object to primitive value"));
            }
            "[object Object]".into()
        }
    })
}
fn number(v: &V) -> Eval<f64> {
    if let V::Object(o) = v {
        if o.iter().any(|(k, _)| k == "toString") {
            return Err(type_error("Cannot convert object to primitive value"));
        }
    }
    crate::oracle_source_owner::source_number_unverified(v).map_err(Fault::Local)
}
fn greater(a: &V, b: &V) -> Eval<bool> {
    if let (V::String(a), V::String(b)) = (a, b) {
        return Ok(a.encode_utf16().cmp(b.encode_utf16()) == Ordering::Greater);
    }
    Ok(number(a)? > number(b)?)
}
fn prop(v: &V, key: &str) -> Eval<V> {
    if matches!(v, V::Undefined | V::Null) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(v)?
        )));
    }
    Ok(match v {
        V::Array(a) if key == "length" => n(a.len() as f64),
        V::String(s) if key == "length" => n(s.encode_utf16().count() as f64),
        V::Array(a) => key
            .parse::<usize>()
            .ok()
            .filter(|i| i.to_string() == key)
            .and_then(|i| a.get(i))
            .cloned()
            .unwrap_or(V::Undefined),
        _ => v.get(key),
    })
}
fn set(v: &mut V, key: &str, value: V) -> Eval<()> {
    if let V::Object(fields) = v {
        if let Some((_, old)) = fields.iter_mut().find(|(k, _)| k == key) {
            *old = value;
        } else {
            fields.push((key.into(), value));
        }
        return Ok(());
    }
    if matches!(v, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            text(v)?
        )));
    }
    Err(type_error(format!(
        "Cannot create property '{key}' on {} '{}'",
        match v {
            V::Number(_) => "number",
            V::Bool(_) => "boolean",
            V::String(_) => "string",
            _ => return Err(SourceRuleError::InvalidInput.into()),
        },
        text(v)?
    )))
}
fn side_mut<'a>(book: &'a mut V, side: &str) -> Eval<&'a mut Vec<V>> {
    match book {
        V::Object(o) => match o.iter_mut().find(|(k, _)| k == side).map(|(_, v)| v) {
            Some(V::Array(a)) => Ok(a),
            _ => Err(type_error(format!(
                "orderBook.{side}.sort is not a function"
            ))),
        },
        _ => Err(SourceRuleError::InvalidInput.into()),
    }
}
fn empty_book() -> V {
    object(&[("buy", V::Array(vec![])), ("sell", V::Array(vec![]))])
}
fn result(book: V, matches: Vec<V>) -> V {
    object(&[("orderBook", book), ("matches", V::Array(matches))])
}

#[derive(Clone, Debug)]
enum Bn {
    Finite(Decimal, bool),
    Inf(bool),
    Nan,
}
impl Bn {
    fn parse(t: &str) -> Eval<Self> {
        let t = t.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
        Ok(match t {
            "Infinity" | "+Infinity" => Self::Inf(false),
            "-Infinity" => Self::Inf(true),
            _ => {
                let (negative, unsigned) = if let Some(t) = t.strip_prefix('-') {
                    (true, t)
                } else {
                    (false, t.strip_prefix('+').unwrap_or(t))
                };
                let radix = if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
                    Some(16)
                } else if unsigned.starts_with("0b") || unsigned.starts_with("0B") {
                    Some(2)
                } else if unsigned.starts_with("0o") || unsigned.starts_with("0O") {
                    Some(8)
                } else {
                    None
                };
                if let Some(radix) = radix {
                    let digits = &unsigned[2..];
                    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                        Self::Nan
                    } else {
                        let mut value = Decimal::zero();
                        for digit in digits.chars() {
                            value = value
                                .checked_mul(&Decimal::from(radix))
                                .and_then(|n| {
                                    n.checked_add(&Decimal::from(
                                        digit.to_digit(radix).unwrap_or(0),
                                    ))
                                })
                                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                        }
                        let zero = value.is_zero();
                        Self::Finite(
                            if negative { value.negated() } else { value },
                            negative && zero,
                        )
                    }
                } else {
                    match t.parse::<Decimal>() {
                        Ok(d) => {
                            let neg = d.is_zero() && t.starts_with('-');
                            Self::Finite(d, neg)
                        }
                        Err(crate::decimal::DecimalError::LimitExceeded) => {
                            return Err(SourceRuleError::UnsupportedNumericDomain.into())
                        }
                        Err(_) => Self::Nan,
                    }
                }
            }
        })
    }
    fn new(v: &V) -> Eval<Self> {
        match v {
            V::Bool(_) | V::Null | V::Undefined | V::Promise => Ok(Self::Nan),
            V::BigNumber(t) | V::String(t) => Self::parse(t),
            _ => Self::parse(&text(v)?),
        }
    }
    fn zero(&self) -> bool {
        matches!(self,Self::Finite(d,_)if d.is_zero())
    }
    fn negative(&self) -> bool {
        match self {
            Self::Finite(d, z) => d.is_negative() || *z,
            Self::Inf(n) => *n,
            Self::Nan => false,
        }
    }
    fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Nan, _) | (_, Self::Nan) => None,
            (Self::Inf(a), Self::Inf(b)) => Some(b.cmp(a)),
            (Self::Inf(true), _) | (_, Self::Inf(false)) => Some(Ordering::Less),
            (Self::Inf(false), _) | (_, Self::Inf(true)) => Some(Ordering::Greater),
            (Self::Finite(a, _), Self::Finite(b, _)) => Some(a.cmp(b)),
        }
    }
    fn sub(&self, other: &Self) -> Eval<Self> {
        Ok(match (self, other) {
            (Self::Nan, _) | (_, Self::Nan) => Self::Nan,
            (Self::Inf(a), Self::Inf(b)) if a == b => Self::Nan,
            (Self::Inf(a), _) => Self::Inf(*a),
            (_, Self::Inf(b)) => Self::Inf(!b),
            (Self::Finite(a, az), Self::Finite(b, bz)) => Self::Finite(
                a.checked_sub(b)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                a.is_zero() && b.is_zero() && *az && !*bz,
            ),
        })
    }
    fn mul(&self, other: &Self) -> Eval<Self> {
        Ok(match (self, other) {
            (Self::Nan, _) | (_, Self::Nan) => Self::Nan,
            (Self::Inf(_), b) | (b, Self::Inf(_)) if b.zero() => Self::Nan,
            (Self::Inf(_), _) | (_, Self::Inf(_)) => Self::Inf(self.negative() != other.negative()),
            (Self::Finite(a, _), Self::Finite(b, _)) => {
                let d = a
                    .checked_mul(b)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let neg = d.is_zero() && self.negative() != other.negative();
                Self::Finite(d, neg)
            }
        })
    }
    fn negate(&self) -> Self {
        match self {
            Self::Nan => Self::Nan,
            Self::Inf(n) => Self::Inf(!n),
            Self::Finite(d, z) => Self::Finite(d.negated(), d.is_zero() && !*z),
        }
    }
    fn add(&self, other: &Self) -> Eval<Self> {
        self.sub(&other.negate())
    }
    fn value(&self) -> Eval<V> {
        Ok(V::BigNumber(if self.zero() && self.negative() {
            "-0".into()
        } else {
            self.text()?
        }))
    }
    fn round(&self) -> Eval<Self> {
        Ok(match self {
            Self::Finite(d, z) => Self::Finite(
                d.quantize(8, Rounding::HalfUp)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                *z || d.is_negative(),
            ),
            _ => self.clone(),
        })
    }
    fn num(&self) -> f64 {
        match self {
            Self::Nan => f64::NAN,
            Self::Inf(true) => f64::NEG_INFINITY,
            Self::Inf(false) => f64::INFINITY,
            Self::Finite(d, z) => {
                if d.is_zero() && *z {
                    -0.0
                } else {
                    d.to_string().parse().unwrap_or(if d.is_negative() {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    })
                }
            }
        }
    }
    fn text(&self) -> Eval<String> {
        Ok(match self {
            Self::Nan => "NaN".into(),
            Self::Inf(true) => "-Infinity".into(),
            Self::Inf(false) => "Infinity".into(),
            Self::Finite(d, _) => {
                let exponent = d.coefficient_digits() as i32 - d.decimal_places() as i32 - 1;
                if !d.is_zero() && (exponent <= -7 || exponent >= 21) {
                    let raw = d.abs().to_string();
                    let ds = raw.chars().filter(|c| *c != '.').collect::<String>();
                    let ds = ds.trim_start_matches('0').trim_end_matches('0');
                    format!(
                        "{}{}{}e{}{exponent}",
                        if d.is_negative() { "-" } else { "" },
                        &ds[..1],
                        if ds.len() > 1 {
                            format!(".{}", &ds[1..])
                        } else {
                            String::new()
                        },
                        if exponent >= 0 { "+" } else { "" }
                    )
                } else {
                    d.to_string()
                }
            }
        })
    }
}
fn fixed_eight(number: f64) -> Eval<String> {
    if !number.is_finite() {
        return text(&n(number));
    }
    if number.abs() >= 1e21 {
        return text(&n(number));
    }
    // Number.toFixed rounds the exact binary64 product, not its shortest text.
    let bits = number.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mantissa = (bits & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
    let binary = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut value = Decimal::from(mantissa);
    let power = Decimal::from(2u32);
    for _ in 0..binary.unsigned_abs() {
        value = if binary >= 0 {
            value.checked_mul(&power)
        } else {
            value.checked_div(&power, 1100, Rounding::HalfUp)
        }
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    }
    value = value
        .quantize(8, Rounding::HalfUp)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
    let mut out = value.to_string();
    if !out.contains('.') {
        out.push('.');
    }
    let decimals = out.split('.').nth(1).unwrap_or("").len();
    out.extend(std::iter::repeat_n('0', 8 - decimals));
    if number < 0.0 {
        out.insert(0, '-');
    }
    Ok(out)
}
trait Runtime {
    fn dep(&mut self, target: &str, args: Vec<V>) -> Eval<V>;
    fn sync(&mut self, _book: &V) -> Eval<()> {
        Ok(())
    }
}
fn dep_value(value: Result<DependencyReturn, SourceRuleError>) -> Eval<V> {
    match value.map_err(Fault::Boundary)? {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
            v.validate()?;
            Ok(v)
        }
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Err(Fault::Thrown(e)),
    }
}
struct Pure<'a, B>(&'a mut B);
impl<B: SourceBackend> Runtime for Pure<'_, B> {
    fn dep(&mut self, target: &str, args: Vec<V>) -> Eval<V> {
        dep_value(self.0.invoke(&DependencyCall {
            target: target.into(),
            arguments: args,
        }))
    }
}
struct Host<'q, 'b, B> {
    queue: &'q mut ContractQueueSourceOwner,
    backend: &'b mut B,
    instance: u64,
    alias: Option<String>,
}
impl<B: ContractPlacementBackend> Runtime for Host<'_, '_, B> {
    fn dep(&mut self, target: &str, args: Vec<V>) -> Eval<V> {
        dep_value(self.backend.invoke_in_queue(
            self.queue,
            &DependencyCall {
                target: target.into(),
                arguments: args,
            },
        ))
    }
    fn sync(&mut self, book: &V) -> Eval<()> {
        if let Some(key) = &self.alias {
            self.queue
                .set_cached_book_unverified(self.instance, key, book.clone())?;
        }
        Ok(())
    }
}
fn checked(result: Eval<V>) -> Eval<V> {
    match result {
        Ok(value) => {
            value.validate()?;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}
fn source_outcome(result: Eval<V>) -> Result<SourceOutcome, SourceRuleError> {
    match result {
        Ok(v) => {
            v.validate()?;
            Ok(SourceOutcome::Returned(v))
        }
        Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
        Err(Fault::Local(e) | Fault::Boundary(e)) => Err(e),
    }
}

fn insert_run(order: &V, book: &mut V, sell: &V) -> Eval<V> {
    if let V::String(t) = book {
        match parse_book_json_unverified(t) {
            Ok(parsed) => *book = parsed,
            Err(crate::issuance_intent::IntentError::InvalidJson) => return Ok(V::Undefined),
            Err(_) => return Err(SourceRuleError::Capacity.into()),
        }
    }
    if !truth(book) {
        *book = empty_book();
    }
    let side = if truth(sell) { "sell" } else { "buy" };
    let value = prop(book, side)?;
    let mut rows = match value {
        V::Array(a) => a,
        v if !truth(&v) => vec![],
        _ => return Err(type_error("bookSide.findIndex is not a function")),
    };
    let mut index = rows.len();
    for (i, row) in rows.iter().enumerate() {
        if greater(&prop(row, "time")?, &prop(order, "time")?)? {
            index = i;
            break;
        }
    }
    rows.insert(index, order.clone());
    set(book, side, V::Array(rows))?;
    book.validate()?;
    Ok(book.clone())
}
/// The isLiq argument is deliberately unused by the original insert method.
pub fn insert_order(
    order: &V,
    book: &mut V,
    is_sell: &V,
    _is_liq: &V,
) -> Result<SourceOutcome, SourceRuleError> {
    order.validate()?;
    book.validate()?;
    is_sell.validate()?;
    let before = book.clone();
    let result = if matches!(book, V::Object(_) | V::Array(_)) {
        checked(insert_run(order, book, is_sell))
    } else {
        checked(insert_run(order, &mut book.clone(), is_sell))
    };
    if matches!(result, Err(Fault::Local(_))) {
        *book = before;
    }
    source_outcome(result)
}

fn compare_orders(a: &V, b: &V, buy: bool, time: bool) -> Eval<Ordering> {
    let ap = Bn::new(&prop(a, "price")?)?;
    let bp = Bn::new(&prop(b, "price")?)?;
    let price = if buy {
        bp.compare(&ap)
    } else {
        ap.compare(&bp)
    };
    if let Some(p) = price {
        if p != Ordering::Equal {
            return Ok(p);
        }
    }
    if !time {
        return Ok(Ordering::Equal);
    }
    Ok(
        (number(&prop(a, "blockTime")?)? - number(&prop(b, "blockTime")?)?)
            .partial_cmp(&0.0)
            .unwrap_or(Ordering::Equal),
    )
}
fn sort_side<R: Runtime>(book: &mut V, side: &str, host: &mut R) -> Eval<()> {
    sort_side_by(book, side, host, side == "buy", true, "orderBook")
}
fn sort_side_by<R: Runtime>(
    book: &mut V,
    side: &str,
    host: &mut R,
    buy: bool,
    time: bool,
    context: &str,
) -> Eval<()> {
    // V8 sorts a work array and only copies elements back after all comparator
    // calls succeed. A comparator exception must leave the source cache's
    // original element ordering intact.
    let value = prop(book, side)?;
    if !matches!(value, V::Array(_)) {
        let _ = prop(&value, "sort")?;
        return Err(type_error(format!(
            "{context}.{side}.sort is not a function"
        )));
    }
    let mut rows = side_mut(book, side)?.clone();
    let length = rows.len();
    if length >= 64
        && rows.iter().any(|row| {
            !matches!(row, V::Object(_))
                || !matches!(
                    Bn::new(&row.get("price")),
                    Ok(Bn::Finite(_, _) | Bn::Inf(_))
                )
                || (time && !number(&row.get("blockTime")).is_ok_and(|n| n.is_finite()))
        })
    {
        // V8's large-run merge path is observable for a non-transitive or
        // throwing comparator. Do not substitute another runtime's order.
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }

    if length < 2 {
        return Ok(());
    }
    let descending = compare_orders(&rows[1], &rows[0], buy, time)? == Ordering::Less;
    let mut run = 2;
    while run < length {
        let c = compare_orders(&rows[run], &rows[run - 1], buy, time)?;
        if if descending {
            c != Ordering::Less
        } else {
            c == Ordering::Less
        } {
            break;
        }
        run += 1;
    }
    if descending {
        rows[..run].reverse();
    }
    for at in run..length {
        let pivot = rows[at].clone();
        let (mut low, mut high) = (0, at);
        while low < high {
            let mid = low + (high - low) / 2;
            if compare_orders(&pivot, &rows[mid], buy, time)? == Ordering::Less {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        rows[low..=at].rotate_right(1);
    }
    *side_mut(book, side)? = rows;
    host.sync(book)
}

fn head(book: &V, side: &str) -> Eval<V> {
    prop(&prop(book, side)?, "0")
}
fn shift<R: Runtime>(book: &mut V, side: &str, host: &mut R) -> Eval<()> {
    side_mut(book, side)?.remove(0);
    host.sync(book)
}
fn order_set<R: Runtime>(book: &mut V, side: &str, key: &str, value: V, host: &mut R) -> Eval<()> {
    set(&mut side_mut(book, side)?[0], key, value)?;
    host.sync(book)
}
fn match_run<R: Runtime>(book: &mut V, host: &mut R) -> Eval<V> {
    let mut matches = vec![];
    for _round in 0..2048 {
        if !truth(book)
            || strict(&prop(&prop(book, "buy")?, "length")?, &n(0.0))
            || strict(&prop(&prop(book, "sell")?, "length")?, &n(0.0))
        {
            return Ok(result(book.clone(), matches));
        }
        let buy_len = number(&prop(&prop(book, "buy")?, "length")?)?;
        let sell_len = number(&prop(&prop(book, "sell")?, "length")?)?;
        let iterations = buy_len.min(sell_len).min(10000.0) as usize;
        sort_side(book, "buy", host)?;
        sort_side(book, "sell", host)?;
        for _ in 0..iterations {
            if side_mut(book, "buy")?.is_empty() || side_mut(book, "sell")?.is_empty() {
                break;
            }
            let mut sell = head(book, "sell")?;
            let mut buy = head(book, "buy")?;
            if Bn::new(&prop(&sell, "amount")?)?.zero() {
                shift(book, "sell", host)?;
                continue;
            }
            if Bn::new(&prop(&buy, "amount")?)?.zero() {
                shift(book, "buy", host)?;
                continue;
            }
            let bm = truth(&prop(&buy, "isMarket")?);
            let sm = truth(&prop(&sell, "isMarket")?);
            if !bm
                && !sm
                && Bn::new(&prop(&buy, "price")?)?.compare(&Bn::new(&prop(&sell, "price")?)?)
                    == Some(Ordering::Less)
            {
                break;
            }
            let mut price = if bm && !sm {
                prop(&sell, "price")?
            } else if sm {
                // Source (sm && !bm) || (bm && sm) is exactly sm.
                prop(&buy, "price")?
            } else if greater(&prop(&buy, "blockTime")?, &prop(&sell, "blockTime")?)? {
                prop(&sell, "price")?
            } else {
                prop(&buy, "price")?
            };
            let (mut seller_maker, mut buyer_maker) = if bm && !sm {
                (true, false)
            } else if sm && !bm {
                (false, true)
            } else {
                (
                    greater(&prop(&buy, "blockTime")?, &prop(&sell, "blockTime")?)?,
                    greater(&prop(&sell, "blockTime")?, &prop(&buy, "blockTime")?)?,
                )
            };
            order_set(book, "sell", "maker", V::Bool(seller_maker), host)?;
            order_set(book, "buy", "maker", V::Bool(buyer_maker), host)?;
            let sender = |v: &V| -> Eval<V> {
                let s = prop(v, "sender")?;
                if truth(&s) {
                    Ok(s)
                } else {
                    prop(v, "address")
                }
            };
            if strict(&sender(&sell)?, &sender(&buy)?) {
                shift(book, if seller_maker { "sell" } else { "buy" }, host)?;
                continue;
            }
            if !bm && !sm && strict(&prop(&sell, "blockTime")?, &prop(&buy, "blockTime")?) {
                price = prop(&buy, "price")?;
                if truth(&prop(&sell, "post")?) {
                    price = prop(&sell, "price")?;
                    seller_maker = true;
                    buyer_maker = false;
                } else if truth(&prop(&buy, "post")?) {
                    buyer_maker = true;
                    seller_maker = false;
                } else {
                    seller_maker = false;
                    buyer_maker = false;
                }
                order_set(book, "sell", "maker", V::Bool(seller_maker), host)?;
                order_set(book, "buy", "maker", V::Bool(buyer_maker), host)?;
            }
            sell = head(book, "sell")?;
            buy = head(book, "buy")?;
            let sbn = Bn::new(&prop(&sell, "amount")?)?;
            let bbn = Bn::new(&prop(&buy, "amount")?)?;
            let amount = match sbn.compare(&bbn) {
                Some(Ordering::Less | Ordering::Equal) => sbn.clone(),
                Some(Ordering::Greater) => bbn.clone(),
                None => Bn::Nan,
            };
            let mut initial = host.dep(
                "Contract.getInitialMargin",
                vec![prop(&buy, "contractId")?, price.clone()],
            )?;
            if !truth(&initial) || number(&initial)?.is_nan() {
                initial = n(0.0);
            }
            let margin = Bn::new(&initial)?.mul(&amount)?.round()?.num();
            let margin = if margin.is_nan() { 0.0 } else { margin };
            let txid = if seller_maker {
                prop(&sell, "txid")?
            } else {
                prop(&buy, "txid")?
            };
            let mut sell_match = sell.clone();
            for (k, v) in [
                ("contractId", prop(&sell, "contractId")?),
                ("amount", n(amount.num())),
                ("sellerAddress", sender(&sell)?),
                ("txid", prop(&sell, "txid")?),
                ("maker", V::Bool(seller_maker)),
                ("liq", {
                    let l = prop(&sell, "isLiq")?;
                    if truth(&l) {
                        l
                    } else {
                        V::Bool(false)
                    }
                }),
                ("marginUsed", n(margin)),
                ("initialReduce", prop(&sell, "initialReduce")?),
            ] {
                set(&mut sell_match, k, v)?;
            }
            let mut buy_match = buy.clone();
            for (k, v) in [
                ("contractId", prop(&buy, "contractId")?),
                ("amount", n(amount.num())),
                ("buyerAddress", sender(&buy)?),
                ("txid", prop(&buy, "txid")?),
                ("liq", {
                    let l = prop(&buy, "isLiq")?;
                    if truth(&l) {
                        l
                    } else {
                        V::Bool(false)
                    }
                }),
                ("maker", V::Bool(buyer_maker)),
                ("marginUsed", n(margin)),
                ("initialReduce", prop(&buy, "initialReduce")?),
            ] {
                set(&mut buy_match, k, v)?;
            }
            matches.push(object(&[
                ("sellOrder", sell_match),
                ("buyOrder", buy_match),
                ("tradePrice", price),
                ("txid", txid),
            ]));
            let sa = sbn.sub(&amount)?.num();
            let ba = bbn.sub(&amount)?.num();
            order_set(book, "sell", "amount", n(sa), host)?;
            order_set(book, "buy", "amount", n(ba), host)?;
            if sa > 0.0 {
                order_set(
                    book,
                    "sell",
                    "initMargin",
                    s(&fixed_eight(number(&initial)? * sa)?),
                    host,
                )?;
            }
            if ba > 0.0 {
                order_set(
                    book,
                    "buy",
                    "initMargin",
                    s(&fixed_eight(number(&initial)? * ba)?),
                    host,
                )?;
            }
            if sa == 0.0 {
                shift(book, "sell", host)?;
            }
            if ba == 0.0 {
                shift(book, "buy", host)?;
            }
            if matches.len() > 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
        }
        let buy = head(book, "buy")?;
        let sell = head(book, "sell")?;
        if side_mut(book, "buy")?.is_empty() || side_mut(book, "sell")?.is_empty() {
            return Ok(result(book.clone(), matches));
        }
        if !truth(&prop(&buy, "isMarket")?)
            && !truth(&prop(&sell, "isMarket")?)
            && Bn::new(&prop(&buy, "price")?)?
                .compare(&Bn::new(&prop(&sell, "price")?)?)
                .is_none_or(|c| c == Ordering::Less)
        {
            return Ok(result(book.clone(), matches));
        }
    }
    Err(SourceRuleError::Capacity.into())
}
pub fn match_contract_orders<B: SourceBackend>(
    book: &mut V,
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    book.validate()?;
    let before = book.clone();
    let result = checked(match_run(book, &mut Pure(backend)));
    if matches!(result, Err(Fault::Local(_))) {
        *book = before;
    }
    source_outcome(result)
}

fn invoke_outcome(outcome: Result<SourceOutcome, SourceRuleError>) -> Eval<V> {
    match outcome.map_err(Fault::Local)? {
        SourceOutcome::Returned(v) => Ok(v),
        SourceOutcome::Thrown(e) => Err(Fault::Thrown(e)),
    }
}
fn instance_id(handle: &V) -> Eval<u64> {
    let V::Number(bits) = handle.get("orderbookInstance") else {
        return Err(SourceRuleError::InvalidInput.into());
    };
    let f = f64::from_bits(bits);
    if f < 1.0 || f.fract() != 0.0 || f > 1024.0 {
        return Err(SourceRuleError::InvalidInput.into());
    }
    Ok(f as u64)
}
fn add_run<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    args: &[V],
    backend: &mut B,
) -> Eval<V> {
    let contract = arg(args, 0);
    let price = arg(args, 1);
    let amount = arg(args, 2);
    let sell = arg(args, 3);
    let block = arg(args, 5);
    let txid = arg(args, 6);
    let sender = arg(args, 7);
    let key = text(&contract)?;
    let handle = arg(args, 12);
    let instance = if truth(&handle) {
        let id = instance_id(&handle)?;
        queue.instance_value(id)?;
        id
    } else {
        let v = invoke_outcome(queue.evaluate(&SourceCall {
            method: "getOrderbookInstance".into(),
            arguments: vec![s(&key)],
        }))?;
        instance_id(&v)?
    };
    let mut host = Host {
        queue,
        backend,
        instance,
        alias: None,
    };
    let map = host.dep("Margin.loadMarginMap", vec![contract.clone()])?;
    if matches!(map, V::Undefined | V::Null) {
        let _ = prop(&map, "getPositionForAddress")?;
    }
    if !matches!(map, V::Object(_)) {
        return Err(type_error(
            "marginMap.getPositionForAddress is not a function",
        ));
    }
    let position = host.dep(
        "Margin.getPositionForAddress",
        vec![sender.clone(), contract.clone()],
    )?;
    let c = prop(&position, "contracts")?;
    let contracts = number(&if truth(&c) { c } else { n(0.0) })?;
    let reducing = contracts < 0.0 && strict(&sell, &V::Bool(false))
        || contracts > 0.0 && strict(&sell, &V::Bool(true));
    let mut initial = n(0.0);
    let mut must_reserve = !reducing;
    if !reducing {
        initial = host.dep(
            "Contract.moveCollateralToReserve",
            vec![
                sender.clone(),
                contract.clone(),
                amount.clone(),
                price.clone(),
                block.clone(),
                txid.clone(),
            ],
        )?;
    } else if truth(&sell) && contracts > 0.0 && greater(&amount, &n(contracts))?
        || !truth(&sell) && contracts < 0.0 && greater(&amount, &n(contracts.abs()))?
    {
        let flip = number(&amount)?
            - if truth(&sell) {
                contracts
            } else {
                contracts.abs()
            };
        if flip > 0.0 {
            must_reserve = true;
            initial = host.dep(
                "Contract.moveCollateralToReserve",
                vec![
                    sender.clone(),
                    contract.clone(),
                    n(flip),
                    price.clone(),
                    block.clone(),
                    txid.clone(),
                ],
            )?;
        }
    }
    if must_reserve && (!truth(&initial) || number(&initial)? <= 0.0) {
        let current = invoke_outcome(host.queue.evaluate_instance(
            instance,
            &SourceCall {
                method: "loadOrderBook".into(),
                arguments: vec![s(&key), V::Bool(false)],
            },
        ))?;
        return Ok(result(current, vec![]));
    }
    let order = object(&[
        ("contractId", contract),
        ("amount", amount),
        ("price", price),
        ("blockTime", block.clone()),
        ("sell", sell.clone()),
        ("initMargin", initial),
        ("sender", sender),
        ("txid", txid),
        ("isLiq", arg(args, 8)),
        ("reduce", arg(args, 9)),
        ("post", arg(args, 10)),
        ("stop", arg(args, 11)),
        ("initialReduce", V::Bool(reducing)),
    ]);
    let (outcome, alias) = host.queue.load_book_with_alias(instance, s(&key))?;
    let mut book = invoke_outcome(Ok(outcome))?;
    host.alias = alias;
    let was_string = matches!(book, V::String(_));
    let inserted = insert_run(&order, &mut book, &sell)?;
    if was_string || !truth(&inserted) {
        host.alias = None;
    }
    if matches!(inserted, V::Undefined) {
        book = V::Undefined;
    } else {
        host.sync(&book)?;
    }
    let matched = match_run(&mut book, &mut host)?;
    invoke_outcome(host.queue.evaluate_instance(
        instance,
        &SourceCall {
            method: "saveOrderBook".into(),
            arguments: vec![prop(&matched, "orderBook")?, s(&key)],
        },
    ))?;
    let matches = prop(&matched, "matches")?;
    if truth(&matches) && number(&prop(&matches, "length")?)? > 0.0 {
        host.dep(
            "Orderbook.processContractMatches",
            vec![
                object(&[("orderbookInstance", n(instance as f64))]),
                matches,
                block,
                V::Bool(false),
            ],
        )?;
    }
    Ok(matched)
}
pub fn add_contract_order<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    args: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    if args.len() > 13 {
        return Err(SourceRuleError::Capacity);
    }
    for v in args {
        v.validate()?;
    }
    let before = queue.clone();
    let result = checked(add_run(queue, args, backend));
    if matches!(result, Err(Fault::Local(_))) {
        *queue = before;
    }
    source_outcome(result)
}
fn own(v: &V, key: &str) -> Eval<bool> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading 'hasOwnProperty')",
            text(v)?
        )));
    }
    if matches!(v,V::Object(o)if o.iter().any(|(k,_)|k=="hasOwnProperty")) {
        return Err(type_error("order.hasOwnProperty is not a function"));
    }
    Ok(matches!(v,V::Object(o)if o.iter().any(|(k,_)|k==key)))
}
fn is_valid_order_run(order: &V, contract: &V) -> Eval<V> {
    let mut token_fields = true;
    for key in [
        "offeredPropertyId",
        "desiredPropertyId",
        "amountOffered",
        "amountExpected",
        "blockTime",
        "sender",
        "price",
    ] {
        if !own(order, key)? {
            token_fields = false;
            break;
        }
    }
    let mut token_types = true;
    for key in [
        "offeredPropertyId",
        "desiredPropertyId",
        "amountOffered",
        "amountExpected",
        "blockTime",
        "sender",
        "price",
    ] {
        let value = prop(order, key)?;
        if if key == "sender" {
            !matches!(value, V::String(_))
        } else {
            !matches!(value, V::Number(_))
        } {
            token_types = false;
            break;
        }
    }
    let contract_branch = matches!(contract, V::Null | V::Undefined)
        || matches!(contract, V::Bool(true))
        || number(contract)? == 1.0;
    if contract_branch {
        let mut fields = true;
        for key in [
            "contractId",
            "amount",
            "side",
            "initMargin",
            "txid",
            "isLiq",
            "reduce",
            "post",
        ] {
            if !own(order, key)? {
                fields = false;
                break;
            }
        }
        let mut types = true;
        for key in [
            "contractId",
            "amount",
            "side",
            "initMargin",
            "txid",
            "isLiq",
            "reduce",
            "post",
        ] {
            let v = prop(order, key)?;
            let ok = match key {
                "contractId" | "amount" | "initMargin" => matches!(v, V::Number(_)),
                "txid" => matches!(v, V::String(_)),
                _ => matches!(v, V::Bool(_)),
            };
            if !ok {
                types = false;
                break;
            }
        }
        return Ok(V::Bool(fields && types));
    }
    Ok(V::Bool(token_fields && token_types))
}
pub fn is_valid_order(order: &V, contract: &V) -> Result<SourceOutcome, SourceRuleError> {
    order.validate()?;
    contract.validate()?;
    source_outcome(is_valid_order_run(order, contract))
}
pub fn is_valid_orderbook(book: &V, contract: &V) -> Result<SourceOutcome, SourceRuleError> {
    fn run(book: &V, contract: &V) -> Eval<V> {
        if !matches!(book, V::Object(_) | V::Array(_)) {
            return Ok(V::Bool(false));
        }
        if matches!(book,V::Object(o)if o.iter().any(|(k,_)|k=="hasOwnProperty")) {
            return Err(type_error("data.hasOwnProperty is not a function"));
        }
        let has = own(book, "buy")? && own(book, "sell")?;
        let mut good = [false, false];
        for (i, side) in ["buy", "sell"].iter().enumerate() {
            if let V::Array(rows) = prop(book, side)? {
                good[i] = true;
                for (index, row) in rows.iter().enumerate() {
                    // Array.every passes index as contract; thisArg is the supplied contract.
                    if !truth(&is_valid_order_run(row, &n(index as f64))?) {
                        good[i] = false;
                        break;
                    }
                }
            }
        }
        let _ = prop(&prop(book, "buy")?, "length")?;
        let _ = prop(&prop(book, "sell")?, "length")?;
        let _ = contract;
        Ok(V::Bool(has && good[0] && good[1]))
    }
    book.validate()?;
    source_outcome(run(book, contract))
}

fn validate_fixed_method(value: &V, name: &str) -> Eval<()> {
    match value {
        V::Number(_) => Ok(()),
        V::BigNumber(t) => {
            Bn::parse(t)?;
            Ok(())
        }
        V::Null | V::Undefined => {
            let _ = prop(value, "toFixed")?;
            Ok(())
        }
        _ => Err(type_error(format!("{name}.toFixed is not a function"))),
    }
}
fn cancel_run<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    args: &[V],
    backend: &mut B,
) -> Eval<V> {
    let address = arg(args, 0);
    let contract = arg(args, 1);
    let required = arg(args, 2);
    let collateral = arg(args, 3);
    let block = arg(args, 4);
    let key = text(&contract)?;
    let instance = queue.create_instance(contract, None)?;
    let (outcome, alias) = queue.load_book_with_alias(instance, s(&key))?;
    let mut book = invoke_outcome(Ok(outcome))?;
    let mut host = Host {
        queue,
        backend,
        instance,
        alias,
    };
    validate_fixed_method(&required, "requiredMargin")?;
    sort_side_by(&mut book, "sell", &mut host, true, false, "obForContract")?;
    sort_side_by(&mut book, "buy", &mut host, false, false, "obForContract")?;
    let mut freed = Bn::parse("0")?;
    for side in ["buy", "sell"] {
        let mut at = side_mut(&mut book, side)?.len();
        while at > 0 {
            at -= 1;
            let order = side_mut(&mut book, side)?[at].clone();
            let sender = prop(&order, "sender")?;
            let sender = if truth(&sender) {
                sender
            } else {
                prop(&order, "address")?
            };
            if !strict(&sender, &address) {
                continue;
            }
            let margin = prop(&order, "initMargin")?;
            freed = freed.add(&Bn::new(&margin)?)?;
            side_mut(&mut book, side)?.remove(at);
            host.sync(&book)?;
            host.dep(
                "Tally.updateBalance",
                vec![
                    address.clone(),
                    collateral.clone(),
                    margin.clone(),
                    n(-number(&margin)?),
                    n(0.0),
                    n(0.0),
                    s("excessOrderCancellation"),
                    block.clone(),
                ],
            )?;
            if freed
                .compare(&Bn::new(&required)?)
                .is_some_and(|c| c != Ordering::Less)
            {
                return Ok(V::Undefined);
            }
        }
    }
    invoke_outcome(host.queue.evaluate_instance(
        instance,
        &SourceCall {
            method: "saveOrderBook".into(),
            arguments: vec![book, s(&key)],
        },
    ))?;
    Ok(V::Undefined)
}
/// Literal static cancellation. The source's sufficient-funds early return
/// deliberately omits saveOrderBook; do not repair that persistence defect.
pub fn cancel_excess_orders<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    args: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    if args.len() > 5 {
        return Err(SourceRuleError::Capacity);
    }
    for v in args {
        v.validate()?;
    }
    let before = queue.clone();
    let result = checked(cancel_run(queue, args, backend));
    if matches!(result, Err(Fault::Local(_))) {
        *queue = before;
    }
    source_outcome(result)
}
fn queue_dep<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    backend: &mut B,
    target: &str,
    args: Vec<V>,
) -> Eval<V> {
    dep_value(backend.invoke_in_queue(
        queue,
        &DependencyCall {
            target: target.into(),
            arguments: args,
        },
    ))
}
fn cross_run<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    args: &[V],
    backend: &mut B,
) -> Eval<V> {
    let address = arg(args, 0);
    let property = arg(args, 1);
    let mut remaining = arg(args, 2);
    let skip = arg(args, 3);
    let height = arg(args, 4);
    let mut breakdown = 0.0;
    let finish = |remaining: V, total: f64| {
        object(&[
            ("remaining", remaining),
            ("breakdown", object(&[("fromCrossReserve", n(total))])),
        ])
    };
    let contracts = match queue_dep(queue, backend, "Contract.getAllContracts", vec![]) {
        Ok(v) => v,
        Err(Fault::Thrown(_)) => return Ok(finish(remaining, breakdown)),
        Err(e) => return Err(e),
    };
    let initial = queue_dep(
        queue,
        backend,
        "Tally.getTally",
        vec![address.clone(), property.clone()],
    )?;
    let available = prop(&initial, "available")?;
    let mut baseline = Bn::new(&if truth(&available) { available } else { n(0.0) })?;
    validate_fixed_method(&remaining, "remaining")?;
    let contracts = match contracts {
        V::Array(a) => a,
        V::String(t) => t.chars().map(|c| s(&c.to_string())).collect(),
        _ => return Err(type_error("allContracts is not iterable")),
    };
    for contract in contracts {
        let id = prop(&contract, "id")?;
        if !truth(&id) || strict(&id, &skip) {
            continue;
        }
        cancel_run(
            queue,
            &[
                address.clone(),
                id,
                remaining.clone(),
                property.clone(),
                height.clone(),
            ],
            backend,
        )?;
        let after = queue_dep(
            queue,
            backend,
            "Tally.getTally",
            vec![address.clone(), property.clone()],
        )?;
        let available = prop(&after, "available")?;
        let after_available = Bn::new(&if truth(&available) { available } else { n(0.0) })?;
        let mut freed = after_available.sub(&baseline)?;
        if freed.compare(&Bn::parse("0")?) == Some(Ordering::Less) {
            freed = Bn::parse("0")?;
        }
        let rem = Bn::new(&remaining)?;
        let used = match rem.compare(&freed) {
            Some(Ordering::Greater) => freed,
            Some(_) => rem,
            None => Bn::Nan,
        };
        if used.compare(&Bn::parse("0")?) == Some(Ordering::Greater) {
            // The source reads an undeclared `block`, not its blockHeight
            // parameter, while evaluating the debit arguments. Preserve that
            // ambient lookup and its ReferenceError before the Tally call.
            let block = queue_dep(queue, backend, "SourceGlobal.get", vec![s("block")])?;
            queue_dep(
                queue,
                backend,
                "Tally.updateBalance",
                vec![
                    address.clone(),
                    property.clone(),
                    n(-used.num()),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s("loss_from_cross_contract_reserve"),
                    block,
                ],
            )?;
            breakdown += used.num();
            if !matches!(remaining, V::BigNumber(_)) {
                return Err(type_error("remaining.minus is not a function"));
            }
            remaining = Bn::new(&remaining)?.sub(&used)?.value()?;
            baseline = after_available.sub(&used)?;
        }
        if !matches!(remaining, V::BigNumber(_)) {
            return Err(type_error("remaining.lte is not a function"));
        }
        if Bn::new(&remaining)?
            .compare(&Bn::parse("0")?)
            .is_some_and(|c| c != Ordering::Greater)
        {
            break;
        }
    }
    Ok(finish(remaining, breakdown))
}
/// Literal all-contract scan; reads actual Tally changes after cancellation.
/// No balance change or freed amount is inferred from a cancellation result.
pub fn source_cross_contract_reserve<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    args: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    if args.len() > 5 {
        return Err(SourceRuleError::Capacity);
    }
    for v in args {
        v.validate()?;
    }
    let before = queue.clone();
    let result = checked(cross_run(queue, args, backend));
    if matches!(result, Err(Fault::Local(_))) {
        *queue = before;
    }
    source_outcome(result)
}
