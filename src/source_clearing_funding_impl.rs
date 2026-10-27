// Included in source_contract_runtime.rs. Pinned clearing funding methods and
// their actual Contract/Oracle/Volume read graph; no separate balance authority.
use crate::decimal::{Decimal as FundingDecimal, Rounding as FundingRounding};
use core::str::FromStr as _;
fn funding_js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}
#[derive(Clone)]
enum FundingBn {
    Finite(FundingDecimal),
    NegativeZero,
    Special(f64),
}
impl FundingBn {
    fn new(v: &V) -> R<Self> {
        if matches!(v,V::Number(bits)if *bits==(-0.0f64).to_bits()) {
            return Ok(Self::NegativeZero);
        }
        let text = match v {
            V::String(s) | V::BigNumber(s) => s.trim_matches(funding_js_space).to_string(),
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
        if let Ok(d) = FundingDecimal::from_str(&text) {
            if d.is_zero() && text.starts_with('-') {
                Ok(Self::NegativeZero)
            } else {
                Ok(Self::Finite(d))
            }
        } else {
            if matches!(
                FundingDecimal::from_str(&text),
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
            Self::NegativeZero => Some(FundingDecimal::zero()),
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
                _ => a.checked_div(&b, 20, FundingRounding::HalfUp),
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
                        Self::Finite(FundingDecimal::zero())
                    }
                }
                _ => signed_infinity(),
            })
        }
    }
    fn q(self, places: u32, rounding: FundingRounding) -> R<Self> {
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
            Self::NegativeZero => Self::Finite(FundingDecimal::zero()),
            Self::Special(n) => Self::Special(n.abs()),
        }
    }
    fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let decimal = |v: &Self| match v {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(FundingDecimal::zero()),
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
impl FundingBn {
    fn finite(&self) -> bool {
        !matches!(self, Self::Special(_))
    }
    fn zero(&self) -> bool {
        matches!(self, Self::NegativeZero) || matches!(self, Self::Finite(d) if d.is_zero())
    }
    fn negative(&self) -> bool {
        matches!(self, Self::NegativeZero)
            || matches!(self, Self::Finite(d) if d.is_negative())
            || matches!(self, Self::Special(n) if *n < 0.0)
    }
    fn negated(self) -> Self {
        match self {
            Self::NegativeZero => Self::Finite(FundingDecimal::zero()),
            Self::Finite(d) if d.is_zero() => Self::NegativeZero,
            Self::Finite(d) => Self::Finite(d.negated()),
            Self::Special(n) => Self::Special(-n),
        }
    }
    fn value(&self) -> V {
        V::BigNumber(match self {
            Self::Finite(d) => d.to_string(),
            Self::NegativeZero => "-0".into(),
            Self::Special(n) if n.is_nan() => "NaN".into(),
            Self::Special(n) if *n < 0.0 => "-Infinity".into(),
            Self::Special(_) => "Infinity".into(),
        })
    }
}
fn funding_prop(value: &V, key: &str) -> R<V> {
    if matches!(value, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(value)?
        )));
    }
    Ok(value.get(key))
}
fn funding_or(value: V) -> V {
    if truth(&value) {
        value
    } else {
        n(0.0)
    }
}
fn funding_caught<T>(result: R<T>, fallback: T) -> R<T> {
    match result {
        Err(Fault::Thrown(_)) => Ok(fallback),
        other => other,
    }
}
impl ContractCore {
    fn source_funding_perp_contracts(&mut self) -> R<V> {
        let result = (|| {
            self.database("contractList")?;
            let rows = array(&self.db(
                "contractList",
                "findAsync",
                vec![obj(&[("type", s("contractSeries"))])],
            )?)?;
            let mut ids = vec![];
            for row in rows {
                let data = funding_prop(&row, "data")?;
                let expiry = funding_prop(&data, "expiryPeriod")?;
                let mut perpetual = same(&expiry, &n(0.0));
                if !perpetual {
                    let ticker = funding_prop(&data, "ticker")?;
                    if truth(&ticker) {
                        perpetual = match ticker {
                            V::String(t) => t.contains("PERP"),
                            V::Array(a) => a.iter().any(|v| same(v, &s("PERP"))),
                            _ => {
                                return Err(type_error(
                                    "doc.data.ticker.includes is not a function",
                                ))
                            }
                        };
                    }
                }
                if perpetual {
                    ids.push(funding_prop(&data, "id")?);
                }
            }
            Ok(V::Array(ids))
        })();
        funding_caught(result, V::Array(vec![]))
    }
    fn source_funding_oracle_twap(&mut self, a: &[V]) -> R<V> {
        let result = (|| {
            self.database("oracleData")?;
            let start = n(number(&arg(a, 1))? - number(&arg(a, 2))?);
            let rows = array(&self.db(
                "oracleData",
                "findAsync",
                vec![obj(&[
                    ("oracleId", arg(a, 0)),
                    ("blockHeight", obj(&[("$gte", start), ("$lte", arg(a, 1))])),
                ])],
            )?)?;
            if rows.is_empty() {
                return Ok(V::Null);
            }
            let mut sum = FundingBn::new(&n(0.0))?;
            for row in &rows {
                let data = funding_prop(row, "data")?;
                let price = funding_prop(&data, "price")?;
                sum = sum.op(FundingBn::new(&price)?, 0)?;
            }
            Ok(n(sum
                .op(FundingBn::new(&n(rows.len() as f64))?, 3)?
                .q(8, FundingRounding::HalfUp)?
                .number()))
        })();
        funding_caught(result, V::Null)
    }
    fn source_funding_volume_vwap(&mut self, a: &[V]) -> R<V> {
        let result = (|| {
            self.database("volumeIndex")?;
            let start = n(number(&arg(a, 2))? - number(&arg(a, 3))?);
            let pairs = V::Array(vec![
                s(format!("{}-{}", text(&arg(a, 0))?, text(&arg(a, 1))?)),
                s(format!("{}-{}", text(&arg(a, 1))?, text(&arg(a, 0))?)),
            ]);
            let rows = array(&self.db(
                "volumeIndex",
                "findAsync",
                vec![obj(&[
                    ("_id", obj(&[("$in", pairs)])),
                    (
                        "value.blockHeight",
                        obj(&[("$gte", start), ("$lte", arg(a, 2))]),
                    ),
                ])],
            )?)?;
            if rows.is_empty() {
                return Ok(V::Null);
            }
            let mut volume = FundingBn::new(&n(0.0))?;
            let mut cost = volume.clone();
            for row in rows {
                let value = funding_prop(&row, "value")?;
                let p = FundingBn::new(&funding_prop(&value, "price")?)?;
                let v = FundingBn::new(&funding_prop(&value, "volume")?)?;
                volume = volume.op(v.clone(), 0)?;
                cost = cost.op(v.op(p, 2)?, 0)?;
            }
            if volume.zero() {
                return Ok(V::Null);
            }
            Ok(n(cost
                .op(volume, 3)?
                .q(8, FundingRounding::HalfUp)?
                .number()))
        })();
        funding_caught(result, V::Null)
    }
    fn source_funding_index_price(&mut self, a: &[V]) -> R<V> {
        // The later class definition at clearing.js:770 shadows the earlier one.
        let result = (|| {
            let info = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
            if !truth(&info) {
                return Ok(V::Null);
            }
            if truth(&funding_prop(&info, "native")?) {
                let key = format!(
                    "{}-{}",
                    text(&funding_prop(&info, "notionalPropertyId")?)?,
                    text(&funding_prop(&info, "collateralPropertyId")?)?
                );
                self.database("volumeIndex")?;
                let mut rows =
                    array(&self.db("volumeIndex", "findAsync", vec![obj(&[("_id", s(key))])])?)?;
                if rows.is_empty() {
                    return Ok(V::Null);
                }
                // Comparisons use Number subtraction; equal/NaN comparisons retain
                // source stable ordering. Validate dereferences before sorting.
                let mut decorated = Vec::with_capacity(rows.len());
                for row in &rows {
                    let value = funding_prop(row, "value")?;
                    decorated.push(number(&funding_prop(&value, "blockHeight")?)?);
                }
                let mut indexes = (0..rows.len()).collect::<Vec<_>>();
                indexes.sort_by(|i, j| {
                    (decorated[*j] - decorated[*i])
                        .partial_cmp(&0.0)
                        .unwrap_or(core::cmp::Ordering::Equal)
                });
                let source = std::mem::take(&mut rows);
                for i in indexes {
                    let value = funding_prop(&source[i], "value")?;
                    let height = funding_prop(&value, "blockHeight")?;
                    if crate::oracle_source_owner::source_greater_equal_unverified(
                        &arg(a, 1),
                        &height,
                    )? {
                        return funding_prop(&value, "price");
                    }
                }
                Ok(V::Null)
            } else {
                let price = self.call(
                    "Oracle.getOraclePrice",
                    vec![funding_prop(&info, "underlyingOracleId")?],
                )?;
                if let V::Number(bits) = price {
                    let value = f64::from_bits(bits);
                    return Ok(if value.is_finite() { n(value) } else { V::Null });
                }
                if !truth(&price)
                    || crate::oracle_source_owner::source_greater_unverified(
                        &funding_prop(&price, "blockHeight")?,
                        &arg(a, 1),
                    )?
                {
                    return Ok(V::Null);
                }
                funding_prop(&price, "price")
            }
        })();
        funding_caught(result, V::Null)
    }
    fn source_funding_clamp(&mut self, a: &[V]) -> R<V> {
        let bps = FundingBn::new(&funding_or(arg(a, 0)))?;
        if !bps.finite()
            || bps.clone().abs().compare(&FundingBn::new(&n(5.0))?)
                == Some(core::cmp::Ordering::Less)
        {
            return Ok(V::BigNumber("0".into()));
        }
        let negative = bps.negative();
        let reduced = bps.abs().op(FundingBn::new(&n(5.0))?, 1)?;
        Ok(if negative { reduced.negated() } else { reduced }.value())
    }
    fn source_funding_calculate(&mut self, a: &[V]) -> R<V> {
        let result = (|| {
            let info = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
            if !truth(&info) {
                return Ok(n(0.0));
            }
            let vwap = if truth(&funding_prop(&info, "native")?) {
                self.call(
                    "VolumeIndex.getVWAP",
                    vec![
                        funding_prop(&info, "notionalPropertyId")?,
                        funding_prop(&info, "collateralPropertyId")?,
                        arg(a, 1),
                        n(192.0),
                    ],
                )?
            } else {
                self.call(
                    "Oracle.getTWAP",
                    vec![
                        funding_prop(&info, "underlyingOracleId")?,
                        arg(a, 1),
                        n(192.0),
                    ],
                )?
            };
            if !truth(&vwap) {
                return Ok(n(0.0));
            }
            let index = self.call("Clearing.getIndexPrice", vec![arg(a, 0), arg(a, 1)])?;
            if !truth(&index) {
                return Ok(n(0.0));
            }
            let basis = FundingBn::new(&index)?
                .op(FundingBn::new(&vwap)?, 1)?
                .op(FundingBn::new(&vwap)?, 3)?
                .op(FundingBn::new(&n(10000.0))?, 2)?
                .q(2, FundingRounding::HalfUp)?;
            let clamped = self.call("Clearing.clampFundingRate", vec![basis.value()])?;
            let mut rate = FundingBn::new(&funding_or(clamped))?
                .op(FundingBn::new(&n(8.0))?, 3)?
                .q(4, FundingRounding::HalfUp)?;
            if rate.clone().abs().compare(&FundingBn::new(&n(12.5))?)
                == Some(core::cmp::Ordering::Greater)
            {
                rate = FundingBn::new(&n(if rate.negative() { -12.5 } else { 12.5 }))?
            }
            Ok(n(rate.number()))
        })();
        funding_caught(result, n(0.0))
    }
    fn source_funding_payments(&mut self, a: &[V]) -> R<V> {
        let total = FundingBn::new(&funding_or(arg(a, 2)))?;
        if !total.finite() || total.zero() {
            return Ok(V::Undefined);
        }
        let collateral = self.call("Contract.getCollateralId", vec![arg(a, 3)])?;
        for (side, input) in [arg(a, 0), arg(a, 1)].iter().enumerate() {
            let positions = match input {
                V::Array(a) => a.clone(),
                _ => {
                    funding_prop(input, "reduce")?;
                    return Err(type_error(if side == 0 {
                        "payers.reduce is not a function"
                    } else {
                        "receivers.reduce is not a function"
                    }));
                }
            };
            let mut sum = FundingBn::new(&n(0.0))?;
            for pos in &positions {
                let contracts =
                    FundingBn::new(&n(number(&funding_or(pos.get("contracts")))?.abs()))?;
                if contracts.finite() {
                    sum = sum.op(contracts, 0)?
                }
            }
            if sum.zero() {
                return Ok(V::Undefined);
            }
            for pos in positions {
                let contracts =
                    FundingBn::new(&n(number(&funding_or(pos.get("contracts")))?.abs()))?;
                if !contracts.finite() || contracts.zero() {
                    continue;
                }
                let amount = total
                    .clone()
                    .op(contracts.op(sum.clone(), 3)?, 2)?
                    .q(8, FundingRounding::HalfUp)?;
                if !amount.finite() {
                    continue;
                }
                let address = funding_prop(&pos, "address")?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address,
                        collateral.clone(),
                        n(if side == 0 {
                            -amount.number()
                        } else {
                            amount.number()
                        }),
                        n(0.0),
                        n(0.0),
                        n(0.0),
                        s(if side == 0 {
                            "fundingFee"
                        } else {
                            "fundingCredit"
                        }),
                        arg(a, 4),
                    ],
                )?;
            }
        }
        Ok(V::Undefined)
    }
    fn source_funding_positions(&mut self, a: &[V]) -> R<V> {
        let handle = self.call("Margin.getInstance", vec![arg(a, 0)])?;
        let positions = self.call("Margin.getAllPositions", vec![handle, arg(a, 0)])?;
        let reference = self.call("Clearing.getIndexPrice", vec![arg(a, 0), arg(a, 2)])?;
        let blob = self.call("Contract.getNotionalValue", vec![arg(a, 0), reference])?;
        let notional = nullish(blob.get("notionalValue"), nullish(blob, V::number(0.0)));
        let notional = FundingBn::new(&funding_or(n(number(&notional)?)))?;
        let rate = FundingBn::new(&funding_or(arg(a, 1)))?;
        let positions = array(&positions)?;
        if positions.is_empty()
            || !notional.finite()
            || notional
                .compare(&FundingBn::new(&n(0.0))?)
                .is_some_and(|v| v != core::cmp::Ordering::Greater)
            || !rate.finite()
        {
            return Ok(V::Undefined);
        }
        let mut longs = vec![];
        let mut shorts = vec![];
        for pos in &positions {
            let count = number(&funding_prop(pos, "contracts")?)?;
            if count > 0.0 {
                longs.push(pos.clone())
            } else if count < 0.0 {
                shorts.push(pos.clone())
            }
        }
        let rate_number = number(&arg(a, 1))?;
        let mut long_total = FundingBn::new(&n(0.0))?;
        let mut short_total = long_total.clone();
        for pos in positions {
            let contracts =
                FundingBn::new(&n(
                    number(&funding_or(funding_prop(&pos, "contracts")?))?.abs()
                ))?;
            if !contracts.finite()
                || contracts
                    .compare(&FundingBn::new(&n(0.0))?)
                    .is_some_and(|v| v != core::cmp::Ordering::Greater)
            {
                continue;
            }
            let amount = contracts
                .op(notional.clone(), 2)?
                .op(rate.clone().op(FundingBn::new(&n(10000.0))?, 3)?, 2)?
                .q(8, FundingRounding::HalfUp)?;
            if !amount.finite() {
                continue;
            }
            let count = number(&funding_prop(&pos, "contracts")?)?;
            if rate_number > 0.0 && count > 0.0 {
                long_total = long_total.op(amount, 0)?
            } else if rate_number < 0.0 && count < 0.0 {
                short_total = short_total.op(amount, 0)?
            }
        }
        if rate_number > 0.0 {
            self.call(
                "Clearing.processFundingPayments",
                vec![
                    V::Array(longs),
                    V::Array(shorts),
                    long_total.value(),
                    arg(a, 0),
                    arg(a, 2),
                ],
            )?;
        } else if rate_number < 0.0 {
            self.call(
                "Clearing.processFundingPayments",
                vec![
                    V::Array(shorts),
                    V::Array(longs),
                    short_total.value(),
                    arg(a, 0),
                    arg(a, 2),
                ],
            )?;
        }
        Ok(V::Undefined)
    }
    fn source_clearing_funding_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "clampFundingRate" => self.source_funding_clamp(a),
            "getIndexPrice" => self.source_funding_index_price(a),
            "calculateFundingRate" => self.source_funding_calculate(a),
            "processFundingPayments" => self.source_funding_payments(a),
            "applyFundingToPositions" => self.source_funding_positions(a),
            "applyFundingRates" => {
                if number(&arg(a, 0))? % 24.0 != 0.0 {
                    return Ok(V::Undefined);
                }
                let contracts = self.call("Contract.getAllPerpContracts", vec![])?;
                for id in array(&contracts)? {
                    let rate =
                        self.call("Clearing.calculateFundingRate", vec![id.clone(), arg(a, 0)])?;
                    if matches!(&rate,V::Number(bits)if f64::from_bits(*bits)==0.0) {
                        continue;
                    }
                    self.call(
                        "Clearing.applyFundingToPositions",
                        vec![id.clone(), rate.clone(), arg(a, 0)],
                    )?;
                    self.call("Clearing.saveFundingEvent", vec![id, rate, arg(a, 0)])?;
                }
                Ok(V::Undefined)
            }
            "saveFundingEvent" => {
                let result = (|| {
                    self.database("fundingEvents")?;
                    let key = s(format!(
                        "funding-{}-{}",
                        text(&arg(a, 0))?,
                        text(&arg(a, 2))?
                    ));
                    let timestamp = self.call("Clock.nowISOString", vec![])?;
                    self.update(
                        "fundingEvents",
                        obj(&[("_id", key.clone())]),
                        obj(&[
                            ("_id", key),
                            ("contractId", arg(a, 0)),
                            ("fundingRate", arg(a, 1)),
                            ("blockHeight", arg(a, 2)),
                            ("timestamp", timestamp),
                        ]),
                        true,
                    )?;
                    Ok(V::Undefined)
                })();
                funding_caught(result, V::Undefined)
            }
            "loadFundingEvents" => {
                let result = (|| {
                    self.database("fundingEvents")?;
                    self.db(
                        "fundingEvents",
                        "findAsync",
                        vec![obj(&[
                            ("contractId", arg(a, 0)),
                            (
                                "blockHeight",
                                obj(&[("$gte", arg(a, 1)), ("$lte", arg(a, 2))]),
                            ),
                        ])],
                    )
                })();
                funding_caught(result, V::Array(vec![]))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
