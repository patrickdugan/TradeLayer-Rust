use super::{truth, CatalogError, Result, SourceValue as V};
use crate::decimal::{Decimal, DecimalError, Rounding};
use core::str::FromStr;

fn space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
pub(super) fn js_text(v: &V) -> Result<String> {
    Ok(match v {
        V::Undefined => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(bits) => {
            let n = f64::from_bits(*bits);
            if n.is_nan() {
                "NaN".into()
            } else if n == f64::INFINITY {
                "Infinity".into()
            } else if n == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                crate::state_oracle::source_number_text(n)
                    .map_err(|_| CatalogError::UnsupportedDomain)?
            }
        }
        V::String(s) => s.clone(),
        V::Array(values) => {
            let mut strings = Vec::new();
            for value in values {
                strings.push(if matches!(value, V::Undefined | V::Null) {
                    String::new()
                } else {
                    js_text(value)?
                })
            }
            strings.join(",")
        }
        V::Object(_) => "[object Object]".into(),
        _ => return Err(CatalogError::UnsupportedDomain),
    })
}
pub(super) fn js_number(v: &V) -> Result<f64> {
    Ok(match v {
        V::Number(bits) => f64::from_bits(*bits),
        V::Null => 0.0,
        V::Bool(b) => f64::from(*b),
        V::String(_) | V::Array(_) => {
            let text = js_text(v)?;
            let text = text.trim_matches(space);
            if text.is_empty() {
                0.0
            } else if text == "Infinity" || text == "+Infinity" {
                f64::INFINITY
            } else if text == "-Infinity" {
                f64::NEG_INFINITY
            } else if let Some((digits, radix)) = text
                .strip_prefix("0x")
                .or_else(|| text.strip_prefix("0X"))
                .map(|s| (s, 16))
                .or_else(|| {
                    text.strip_prefix("0b")
                        .or_else(|| text.strip_prefix("0B"))
                        .map(|s| (s, 2))
                })
                .or_else(|| {
                    text.strip_prefix("0o")
                        .or_else(|| text.strip_prefix("0O"))
                        .map(|s| (s, 8))
                })
            {
                if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                    f64::NAN
                } else {
                    if digits.len() > 512 {
                        return Err(CatalogError::UnsupportedDomain);
                    }
                    let mut number = Decimal::zero();
                    for c in digits.chars() {
                        number = number
                            .checked_mul(&Decimal::from(radix as i128))
                            .and_then(|n| {
                                n.checked_add(&Decimal::from(c.to_digit(radix).unwrap() as i128))
                            })
                            .map_err(|_| CatalogError::UnsupportedDomain)?
                    }
                    number.to_string().parse().unwrap_or(f64::INFINITY)
                }
            } else if text
                .bytes()
                .any(|b| !b.is_ascii_digit() && !b"+-.eE".contains(&b))
            {
                f64::NAN
            } else {
                text.parse().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    })
}
enum Big {
    Finite(Decimal, bool),
    Special(f64),
}
impl Big {
    fn new(v: &V) -> Result<Self> {
        let text = js_text(v)?.trim_matches(space).to_owned();
        if text.len() > 512 {
            return Err(CatalogError::UnsupportedDomain);
        }
        let unsigned = text.trim_start_matches(['+', '-']);
        if ["0x", "0X", "0b", "0B", "0o", "0O"]
            .iter()
            .any(|p| unsigned.starts_with(p))
        {
            return Err(CatalogError::UnsupportedDomain);
        }
        match Decimal::from_str(&text) {
            Ok(d) => {
                let negative = (d.is_zero() && text.starts_with('-'))
                    || matches!(v,V::Number(bits)if *bits==(-0.0f64).to_bits());
                Ok(Self::Finite(d, negative))
            }
            Err(DecimalError::LimitExceeded) => Err(CatalogError::UnsupportedDomain),
            Err(_) => Ok(Self::Special(match text.as_str() {
                "Infinity" | "+Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => f64::NAN,
            })),
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Finite(d, n) => {
                if d.is_zero() && *n {
                    -0.0
                } else {
                    d.to_string().parse().unwrap_or(if d.is_negative() {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    })
                }
            }
            Self::Special(n) => *n,
        }
    }
    fn q(self) -> Result<f64> {
        match self {
            Self::Finite(d, negative) => {
                let q = d
                    .quantize(8, Rounding::HalfUp)
                    .map_err(|_| CatalogError::UnsupportedDomain)?;
                if q.is_zero() && (negative || d.is_negative()) {
                    Ok(-0.0)
                } else {
                    Ok(q.to_string().parse().unwrap_or(f64::NAN))
                }
            }
            Self::Special(n) => Ok(n),
        }
    }
    fn op(self, rhs: Self, subtract: bool) -> Result<Self> {
        match (self, rhs) {
            (Self::Finite(a, an), Self::Finite(b, bn)) => {
                let negative = a.is_zero() && b.is_zero() && an && (bn != subtract);
                let sum = if subtract {
                    a.checked_sub(&b)
                } else {
                    a.checked_add(&b)
                }
                .map_err(|_| CatalogError::UnsupportedDomain)?;
                Ok(Self::Finite(sum, negative))
            }
            (a, b) => Ok(Self::Special(if subtract {
                a.number() - b.number()
            } else {
                a.number() + b.number()
            })),
        }
    }
}
pub(super) fn quantize(value: &V) -> Result<f64> {
    let zero = V::number(0.0);
    Big::new(if truth(value) { value } else { &zero })?.q()
}
pub(super) fn supply_change(value: &V, amount: &V, subtract: bool) -> Result<f64> {
    // quantize8 receives the truthy BigNumber object, so its internal NaN or -0
    // is not replaced by value||0.
    Big::new(value)?.op(Big::new(amount)?, subtract)?.q()
}
pub(super) fn less(value: &V, amount: &V) -> Result<bool> {
    let left = Big::new(value)?;
    let right = Big::new(amount)?;
    Ok(match (&left, &right) {
        (Big::Finite(a, _), Big::Finite(b, _)) => a < b,
        _ => left.number() < right.number(),
    })
}
