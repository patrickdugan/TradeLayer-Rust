// Arithmetic-only reuse of the current source FundingBn algorithm; no balances or owners.
use core::str::FromStr as _;
#[derive(Clone)]
enum BookHelperBn {
    Finite(Decimal),
    NegativeZero,
    Special(f64),
}
impl BookHelperBn {
    fn new(v: &V) -> Eval<Self> {
        if matches!(v, V::Object(_) | V::Array(_)) {
            book_helper_string(v)?;
        }
        if matches!(v,V::Number(bits)if *bits==(-0.0f64).to_bits()) {
            return Ok(Self::NegativeZero);
        }
        let text = match v {
            V::String(s) | V::BigNumber(s) => s.trim_matches(js_space).to_string(),
            V::Number(_) | V::Array(_) => book_helper_string(v)?,
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
            let d = crate::oracle_source_owner::source_finite_decimal_unverified(&V::String(
                text.clone(),
            ))?;
            return Ok(if d.is_zero() && text.starts_with('-') {
                Self::NegativeZero
            } else {
                Self::Finite(d)
            });
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
        let aneg = matches!(self, Self::NegativeZero)
            || matches!(&self,Self::Finite(d)if d.is_negative())
            || matches!(&self,Self::Special(n)if *n<0.0);
        let bneg = matches!(b, Self::NegativeZero)
            || matches!(&b,Self::Finite(d)if d.is_negative())
            || matches!(&b,Self::Special(n)if *n<0.0);
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
                _ => a.checked_div(&b, 20, crate::decimal::Rounding::HalfUp),
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
            let a_special = match &self {
                Self::Special(n) => Some(*n),
                _ => None,
            };
            let b_special = match &b {
                Self::Special(n) => Some(*n),
                _ => None,
            };
            if a_special.is_some_and(f64::is_nan) || b_special.is_some_and(f64::is_nan) {
                return Ok(Self::Special(f64::NAN));
            }
            let signed_infinity = || {
                Self::Special(if aneg ^ bneg {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                })
            };
            Ok(match kind {
                0 => match (a_special, b_special) {
                    (Some(a), Some(b)) => Self::Special(a + b),
                    (Some(a), None) => Self::Special(a),
                    (None, Some(b)) => Self::Special(b),
                    _ => unreachable!(),
                },
                1 => match (a_special, b_special) {
                    (Some(a), Some(b)) => Self::Special(a - b),
                    (Some(a), None) => Self::Special(a),
                    (None, Some(b)) => Self::Special(-b),
                    _ => unreachable!(),
                },
                2 if finite(&self).is_some_and(|d| d.is_zero())
                    || finite(&b).is_some_and(|d| d.is_zero()) =>
                {
                    Self::Special(f64::NAN)
                }
                2 => signed_infinity(),
                _ if a_special.is_some() && b_special.is_some() => Self::Special(f64::NAN),
                _ if b_special.is_some() => {
                    if aneg ^ bneg {
                        Self::NegativeZero
                    } else {
                        Self::Finite(Decimal::zero())
                    }
                }
                _ => signed_infinity(),
            })
        }
    }
    fn q(self, places: u32, rounding: crate::decimal::Rounding) -> Eval<Self> {
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
    fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let decimal = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(Decimal::zero()),
            _ => None,
        };
        if let (Some(a), Some(b)) = (decimal(self), decimal(other)) {
            Some(a.cmp(&b))
        } else {
            match (self, other) {
                (Self::Special(a), Self::Special(b)) => a.partial_cmp(b),
                (Self::Special(a), _) => {
                    if a.is_nan() {
                        None
                    } else if *a > 0.0 {
                        Some(core::cmp::Ordering::Greater)
                    } else {
                        Some(core::cmp::Ordering::Less)
                    }
                }
                (_, Self::Special(b)) => {
                    if b.is_nan() {
                        None
                    } else if *b > 0.0 {
                        Some(core::cmp::Ordering::Less)
                    } else {
                        Some(core::cmp::Ordering::Greater)
                    }
                }
                _ => None,
            }
        }
    }
}
