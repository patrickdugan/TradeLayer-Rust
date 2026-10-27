//! Literal pinned Orderbook token settlement over live SourceBackend owners.
//! No balances, channel allocations or supplied financial verdicts live here.
use crate::source_contract_values::*;
use crate::validity_source::{
    DependencyCall, DependencyObservation, SourceBackend, SourceEvaluation, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_SHA256: &str = "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98";
use crate::decimal::{Decimal as TokenDecimal, Rounding as TokenRounding};
use core::str::FromStr as _;
fn token_js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}
#[derive(Clone)]
enum TokenBn {
    Finite(TokenDecimal),
    NegativeZero,
    Special(f64),
}
impl TokenBn {
    fn floor(self, places: u32) -> R<Self> {
        if let Self::Finite(value) = &self {
            let down = value
                .quantize(places, TokenRounding::Down)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
            if value.is_negative() && down != *value {
                let unit = TokenDecimal::from_str(&format!("1e-{places}"))
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                return Ok(Self::Finite(
                    down.checked_sub(&unit)
                        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
                ));
            }
        }
        self.q(places, TokenRounding::Down)
    }
    fn new(v: &V) -> R<Self> {
        if matches!(v,V::Number(bits)if *bits==(-0.0f64).to_bits()) {
            return Ok(Self::NegativeZero);
        }
        let text = match v {
            V::String(s) | V::BigNumber(s) => s.trim_matches(token_js_space).to_string(),
            V::Number(_) | V::Array(_) => text(v)?,
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
        if let Ok(d) = TokenDecimal::from_str(&text) {
            if d.is_zero() && text.starts_with('-') {
                Ok(Self::NegativeZero)
            } else {
                Ok(Self::Finite(d))
            }
        } else {
            if matches!(
                TokenDecimal::from_str(&text),
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
    fn op(self, b: Self, kind: u8) -> R<Self> {
        let finite = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(TokenDecimal::zero()),
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
                _ => a.checked_div(&b, 20, TokenRounding::HalfUp),
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
                        Self::Finite(TokenDecimal::zero())
                    }
                }
                _ => signed_infinity(),
            })
        }
    }
    fn q(self, places: u32, rounding: TokenRounding) -> R<Self> {
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
impl TokenBn {
    fn negated(self) -> Self {
        match self {
            Self::NegativeZero => Self::Finite(TokenDecimal::zero()),
            Self::Finite(d) if d.is_zero() => Self::NegativeZero,
            Self::Finite(d) => Self::Finite(d.negated()),
            Self::Special(n) => Self::Special(-n),
        }
    }
}

fn prop(v: &V, key: &str) -> R<V> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(v)?
        )));
    }
    Ok(v.get(key))
}
fn less(a: &V, b: &V) -> R<bool> {
    let primitive = |v: &V| -> R<V> {
        Ok(match v {
            V::BigNumber(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
            V::Object(fields) if fields.iter().any(|(key, _)| key == "toString") => {
                return Err(type_error("Cannot convert object to primitive value"))
            }
            V::Array(_) | V::Object(_) | V::Promise => s(text(v)?),
            _ => v.clone(),
        })
    };
    let (a, b) = (primitive(a)?, primitive(b)?);
    Ok(if let (V::String(a), V::String(b)) = (&a, &b) {
        a.encode_utf16().cmp(b.encode_utf16()) == core::cmp::Ordering::Less
    } else {
        number(&a)? < number(&b)?
    })
}
fn assign_role(value: &mut V, role: &str) -> R<()> {
    if matches!(value, V::Object(_)) {
        return set(value, "orderRole", s(role));
    }
    let kind = match value {
        V::String(_) => "string",
        V::Number(_) => "number",
        V::Bool(_) => "boolean",
        // Named properties on Arrays are outside the SourceValue array domain.
        _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
    };
    Err(type_error(format!(
        "Cannot create property 'orderRole' on {kind} '{}'",
        text(value)?
    )))
}
struct Frame<'a, B> {
    backend: &'a mut B,
    arguments: Vec<V>,
    dependencies: Vec<DependencyObservation>,
    bytes: usize,
}
impl<B: SourceBackend> Frame<'_, B> {
    fn call(&mut self, target: &str, arguments: Vec<V>) -> R<V> {
        if let Some(method) = target.strip_prefix("Tally.") {
            // The pinned token methods reference an undeclared lowercase
            // identifier. The normal realm throws before this method call.
            // Only an explicit alternate realm binding to the genuine Tally
            // namespace can reach the financial dependency below.
            let owner = self.call("SourceGlobal.get", vec![s("tallyMap")])?;
            if matches!(owner, V::Undefined | V::Null) {
                return Err(type_error(format!(
                    "Cannot read properties of {} (reading '{method}')",
                    text(&owner)?
                )));
            }
            if owner != obj(&[("sourceTally", V::Bool(true))]) {
                return Err(SourceRuleError::UnknownDependency.into());
            }
        }
        if self.dependencies.len() >= 16384 {
            return Err(SourceRuleError::Capacity.into());
        }
        let call = DependencyCall {
            target: target.into(),
            arguments,
        };
        self.bytes = self.bytes.saturating_add(
            V::Array(call.arguments.clone()).diagnostic_json()?.len() + target.len(),
        );
        if self.bytes > 16 * 1024 * 1024 {
            return Err(SourceRuleError::Capacity.into());
        }
        let returned = self.backend.invoke(&call)?;
        self.dependencies.push(DependencyObservation {
            call,
            returned: returned.clone(),
        });
        awaited(returned)
    }
    fn balance(
        &mut self,
        address: V,
        property: V,
        available: f64,
        reserve: f64,
        channel: bool,
    ) -> R<()> {
        self.call(
            "Tally.updateBalance",
            vec![
                address,
                property,
                n(available),
                n(reserve),
                n(0.0),
                n(0.0),
                V::Bool(true),
                V::Bool(false),
                V::Bool(channel),
                arg(&self.arguments, 2),
            ],
        )?;
        Ok(())
    }
    fn run(&mut self, channel_only: bool) -> R<V> {
        let matches = arg(&self.arguments, 0);
        if !channel_only && !matches!(matches, V::Array(ref a) if !a.is_empty()) {
            return Ok(V::Undefined);
        }
        let channel = channel_only || truth(&arg(&self.arguments, 3));
        if channel_only {
            prop(&matches, "length")?;
        }
        let rows = match matches {
            V::Array(a) => a,
            V::String(s) if channel_only => s.chars().map(|c| V::String(c.to_string())).collect(),
            _ => return Err(type_error("matches is not iterable")),
        };
        for (index, mut m) in rows.into_iter().enumerate() {
            let mut sell = prop(&m, "sellOrder")?;
            if !truth(&sell) {
                continue;
            }
            let mut buy = prop(&m, "buyOrder")?;
            if !truth(&buy) {
                continue;
            }
            let seller = prop(&sell, "senderAddress")?;
            let buyer = prop(&buy, "senderAddress")?;
            let property_a = prop(&sell, "offeredPropertyId")?;
            let property_b = prop(&buy, "desiredPropertyId")?;
            let role = if channel {
                0
            } else if less(&prop(&sell, "blockTime")?, &prop(&buy, "blockTime")?)? {
                1
            } else if less(&prop(&buy, "blockTime")?, &prop(&sell, "blockTime")?)? {
                2
            } else {
                0
            };
            if !channel {
                for seller_first in if role == 1 {
                    [true, false]
                } else {
                    [false, true]
                } {
                    let (leg, key, name) = if seller_first {
                        (
                            &mut sell,
                            "sellOrder",
                            if role == 1 {
                                "maker"
                            } else if role == 2 {
                                "taker"
                            } else {
                                "split"
                            },
                        )
                    } else {
                        (
                            &mut buy,
                            "buyOrder",
                            if role == 2 {
                                "maker"
                            } else if role == 1 {
                                "taker"
                            } else {
                                "split"
                            },
                        )
                    };
                    assign_role(leg, name)?;
                    set(&mut m, key, leg.clone())?;
                    if let V::Array(rows) = &mut self.arguments[0] {
                        rows[index] = m.clone();
                    }
                }
            }
            let amount_a = TokenBn::new(&prop(&m, "amountOfTokenA")?)?;
            let amount_b = TokenBn::new(&prop(&m, "amountOfTokenB")?)?;
            let mut seller_credit = amount_b.clone();
            let mut buyer_credit = amount_a.clone();
            let mut rebate = TokenBn::new(&n(0.0))?;
            let mut total_fee = TokenBn::new(&n(0.0))?;
            if !channel {
                if role == 1 || role == 2 {
                    let gross = if role == 1 {
                        amount_b.clone()
                    } else {
                        amount_a.clone()
                    }
                    .op(TokenBn::new(&n(0.0002))?, 2)?;
                    rebate = gross.clone().op(TokenBn::new(&n(2.0))?, 3)?;
                    total_fee = gross.op(TokenBn::new(&n(2.0))?, 3)?;
                    self.call(
                        "Tally.updateFeeCache",
                        vec![
                            if role == 1 {
                                property_b.clone()
                            } else {
                                property_a.clone()
                            },
                            n(total_fee.clone().floor(8)?.number()),
                            V::Null,
                            arg(&self.arguments, 1),
                        ],
                    )?;
                    if role == 1 {
                        seller_credit = amount_b
                            .clone()
                            .op(rebate.clone(), 0)?
                            .q(8, TokenRounding::HalfUp)?;
                        buyer_credit = amount_a.clone().op(TokenBn::new(&n(0.0))?, 1)?;
                    } else {
                        buyer_credit = amount_a
                            .clone()
                            .op(rebate.clone(), 0)?
                            .q(8, TokenRounding::HalfUp)?;
                        seller_credit = amount_b.clone().op(TokenBn::new(&n(0.0))?, 1)?;
                    }
                } else {
                    let fee_a = amount_a
                        .clone()
                        .op(TokenBn::new(&n(0.0001))?, 2)?
                        .floor(8)?;
                    let fee_b = amount_b
                        .clone()
                        .op(TokenBn::new(&n(0.0001))?, 2)?
                        .floor(8)?;
                    self.call(
                        "Tally.updateFeeCache",
                        vec![
                            property_b.clone(),
                            n(fee_a.number()),
                            V::Null,
                            arg(&self.arguments, 1),
                        ],
                    )?;
                    self.call(
                        "Tally.updateFeeCache",
                        vec![
                            property_a.clone(),
                            n(fee_b.number()),
                            V::Null,
                            arg(&self.arguments, 1),
                        ],
                    )?;
                    total_fee = fee_a.op(fee_b, 0)?;
                }
            }
            self.balance(
                seller.clone(),
                property_a.clone(),
                0.0,
                amount_a.clone().negated().number(),
                channel,
            )?;
            self.balance(
                seller.clone(),
                property_b.clone(),
                seller_credit.number(),
                0.0,
                channel,
            )?;
            self.balance(
                buyer.clone(),
                property_b.clone(),
                0.0,
                amount_b.clone().negated().number(),
                channel,
            )?;
            self.balance(
                buyer.clone(),
                property_a.clone(),
                buyer_credit.number(),
                0.0,
                channel,
            )?;
            if !channel {
                self.call(
                    "Orderbook.recordTokenTrade",
                    vec![
                        obj(&[
                            ("offeredPropertyId", property_a),
                            ("desiredPropertyId", property_b),
                            ("amountOffered", n(amount_a.number())),
                            ("amountExpected", n(amount_b.number())),
                            ("price", prop(&m, "tradePrice")?),
                            ("buyerRole", prop(&buy, "orderRole")?),
                            ("sellerRole", prop(&sell, "orderRole")?),
                            ("takerFee", n(total_fee.number())),
                            ("makerRebate", n(rebate.number())),
                            ("block", arg(&self.arguments, 1)),
                            ("buyer", buyer),
                            ("seller", seller),
                            ("takerTxId", arg(&self.arguments, 2)),
                        ]),
                        arg(&self.arguments, 1),
                        arg(&self.arguments, 2),
                    ],
                )?;
            }
        }
        Ok(V::Undefined)
    }
}

pub fn evaluate_token_settlement<B: SourceBackend>(
    arguments: &[V],
    channel_only: bool,
    backend: &mut B,
) -> Result<SourceEvaluation, SourceRuleError> {
    if arguments.len() > 8 {
        return Err(SourceRuleError::Capacity);
    }
    for value in arguments {
        value.validate()?;
    }
    let mut frame = Frame {
        backend,
        arguments: arguments.to_vec(),
        dependencies: vec![],
        bytes: 0,
    };
    let outcome = match frame.run(channel_only) {
        Ok(v) => SourceOutcome::Returned(v),
        Err(Fault::Thrown(e)) => SourceOutcome::Thrown(e),
        Err(Fault::Local(e)) => return Err(e),
    };
    Ok(SourceEvaluation {
        outcome,
        arguments_after: frame.arguments,
        dependencies: frame.dependencies,
        deferred_rejections: vec![],
        global_writes: vec![],
    })
}
pub fn process_token_matches<B: SourceBackend>(
    arguments: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    Ok(evaluate_token_settlement(arguments, false, backend)?.outcome)
}
pub fn process_token_channel_trades<B: SourceBackend>(
    arguments: &[V],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError> {
    Ok(evaluate_token_settlement(arguments, true, backend)?.outcome)
}
