//! Bounded source-value operations shared by the literal contract runtime.
use crate::decimal::{Decimal, Rounding};
use crate::validity_source::{
    DependencyReturn, SourceException, SourceOutcome, SourceRuleError, SourceValue as V,
};

#[derive(Debug)]
pub(crate) enum Fault {
    Thrown(SourceException),
    Local(SourceRuleError),
}
pub(crate) type R<T> = Result<T, Fault>;
impl From<SourceRuleError> for Fault {
    fn from(e: SourceRuleError) -> Self {
        Self::Local(e)
    }
}
pub(crate) fn error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "Error".into(),
        message: message.into(),
    })
}
pub(crate) fn type_error(message: impl Into<String>) -> Fault {
    Fault::Thrown(SourceException {
        name: "TypeError".into(),
        message: message.into(),
    })
}
pub(crate) fn s(t: impl Into<String>) -> V {
    V::String(t.into())
}
pub(crate) fn n(x: f64) -> V {
    V::number(x)
}
pub(crate) fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
pub(crate) fn arg(args: &[V], i: usize) -> V {
    args.get(i).cloned().unwrap_or(V::Undefined)
}
pub(crate) fn truth(v: &V) -> bool {
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
pub(crate) fn text(v: &V) -> R<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::String(s) => s.clone(),
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
        V::BigNumber(s) => s.clone(),
        V::Array(a) => a
            .iter()
            .map(|v| {
                if matches!(v, V::Null | V::Undefined) {
                    Ok(String::new())
                } else {
                    text(v)
                }
            })
            .collect::<R<Vec<_>>>()?
            .join(","),
        V::Object(_) => "[object Object]".into(),
        V::Promise => "[object Promise]".into(),
    })
}
pub(crate) fn number(v: &V) -> R<f64> {
    crate::oracle_source_owner::source_number_unverified(v).map_err(Fault::Local)
}
pub(crate) fn dec(v: &V) -> R<Decimal> {
    crate::oracle_source_owner::source_finite_decimal_unverified(v).map_err(Fault::Local)
}
pub(crate) fn d0(v: &V) -> R<Decimal> {
    if truth(v) {
        dec(v)
    } else {
        Ok(Decimal::zero())
    }
}
pub(crate) fn dn(d: &Decimal) -> R<V> {
    Ok(n(d
        .to_string()
        .parse()
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?))
}
pub(crate) fn q(d: &Decimal, places: u32) -> R<Decimal> {
    d.quantize(places, Rounding::HalfUp)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
pub(crate) fn add(a: &Decimal, b: &Decimal) -> R<Decimal> {
    a.checked_add(b)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
pub(crate) fn sub(a: &Decimal, b: &Decimal) -> R<Decimal> {
    a.checked_sub(b)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
pub(crate) fn mul(a: &Decimal, b: &Decimal) -> R<Decimal> {
    a.checked_mul(b)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
pub(crate) fn div(a: &Decimal, b: &Decimal) -> R<Decimal> {
    a.checked_div(b, 20, Rounding::HalfUp)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
pub(crate) fn min(a: Decimal, b: Decimal) -> Decimal {
    if a < b {
        a
    } else {
        b
    }
}
pub(crate) fn array(v: &V) -> R<Vec<V>> {
    if let V::Array(a) = v {
        Ok(a.clone())
    } else {
        Err(type_error("value is not iterable"))
    }
}
pub(crate) fn set(v: &mut V, k: &str, x: V) -> R<()> {
    v.insert(k, x).map_err(Fault::Local)?;
    if let V::Object(fields) = v {
        fields.sort_by_key(|(key, _)| {
            key.parse::<u32>()
                .ok()
                .filter(|i| *i != u32::MAX && i.to_string() == *key)
        });
        let split = fields.partition_point(|(key, _)| {
            key.parse::<u32>()
                .ok()
                .filter(|i| *i != u32::MAX && i.to_string() == *key)
                .is_none()
        });
        fields.rotate_left(split)
    };
    Ok(())
}
pub(crate) fn nullish(v: V, fallback: V) -> V {
    if matches!(v, V::Undefined | V::Null) {
        fallback
    } else {
        v
    }
}
pub(crate) fn json(v: &V) -> R<String> {
    crate::property_catalog::json_stringify(v)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
pub(crate) fn parse(t: &str) -> R<V> {
    crate::contract_queue_source_owner::parse_book_json_unverified(t).map_err(|e| match e {
        crate::issuance_intent::IntentError::InvalidJson => type_error("Unexpected token in JSON"),
        _ => Fault::Local(SourceRuleError::UnsupportedNumericDomain),
    })
}
pub(crate) fn awaited(r: DependencyReturn) -> R<V> {
    match r {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => Ok(v),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Err(Fault::Thrown(e)),
    }
}
pub(crate) fn outcome(r: SourceOutcome) -> R<V> {
    match r {
        SourceOutcome::Returned(v) => Ok(v),
        SourceOutcome::Thrown(e) => Err(Fault::Thrown(e)),
    }
}
pub(crate) fn returned(r: R<V>) -> Result<DependencyReturn, SourceRuleError> {
    match r {
        Ok(v) => Ok(DependencyReturn::Pending(v)),
        Err(Fault::Thrown(e)) => Ok(DependencyReturn::PendingThrow(e)),
        Err(Fault::Local(e)) => Err(e),
    }
}
pub(crate) fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (V::Object(_) | V::Array(_), _) => false,
        _ => a == b,
    }
}
pub(crate) fn get_path(v: &V, path: &str) -> V {
    path.split('.').fold(v.clone(), |v, k| v.get(k))
}
pub(crate) fn set_path(v: &mut V, path: &str, value: V) -> R<()> {
    if let Some((head, tail)) = path.split_once('.') {
        let mut child = v.get(head);
        if matches!(child, V::Undefined | V::Null) {
            child = obj(&[])
        };
        set_path(&mut child, tail, value)?;
        set(v, head, child)
    } else {
        set(v, path, value)
    }
}
