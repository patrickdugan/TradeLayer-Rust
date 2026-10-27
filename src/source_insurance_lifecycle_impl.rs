// Included in source_contract_runtime.rs. One insurance singleton cache and the
// same Core DB/Tally authority; adapter handles never supply fund outcomes.
#[derive(Clone)]
enum InsuranceBn {
    Finite(crate::decimal::Decimal),
    NegativeZero,
    Special(f64),
}
impl InsuranceBn {
    fn new(value: &V) -> R<Self> {
        let value = outcome(crate::oracle_source_owner::source_bignumber_value_unverified(value)?)?;
        let V::BigNumber(value) = value else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        Ok(match value.as_str() {
            "-0" => Self::NegativeZero,
            "NaN" => Self::Special(f64::NAN),
            "Infinity" => Self::Special(f64::INFINITY),
            "-Infinity" => Self::Special(f64::NEG_INFINITY),
            _ => Self::Finite(
                value
                    .parse()
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?,
            ),
        })
    }
    fn zero() -> Self {
        Self::Finite(crate::decimal::Decimal::zero())
    }
    fn finite(&self) -> Option<crate::decimal::Decimal> {
        match self {
            Self::Finite(d) => Some(d.clone()),
            Self::NegativeZero => Some(crate::decimal::Decimal::zero()),
            Self::Special(_) => None,
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Finite(d) => d.to_string().parse().unwrap_or(if d.is_negative() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }),
            Self::NegativeZero => -0.0,
            Self::Special(n) => *n,
        }
    }
    fn abs(&self) -> Self {
        match self {
            Self::Finite(d) => Self::Finite(d.abs()),
            Self::NegativeZero => Self::zero(),
            Self::Special(n) => Self::Special(n.abs()),
        }
    }
    fn negate(&self) -> Self {
        match self {
            Self::Finite(d) if d.is_zero() => Self::NegativeZero,
            Self::Finite(d) => Self::Finite(d.negated()),
            Self::NegativeZero => Self::zero(),
            Self::Special(n) => Self::Special(-n),
        }
    }
    fn plus(&self, other: &Self) -> R<Self> {
        match (self.finite(), other.finite()) {
            (Some(a), Some(b)) => {
                let d = add(&a, &b)?;
                Ok(
                    if d.is_zero()
                        && matches!(self, Self::NegativeZero)
                        && matches!(other, Self::NegativeZero)
                    {
                        Self::NegativeZero
                    } else {
                        Self::Finite(d)
                    },
                )
            }
            _ => Ok(Self::Special(match (self, other) {
                (Self::Special(a), Self::Special(b)) => a + b,
                (Self::Special(a), _) => *a,
                (_, Self::Special(b)) => *b,
                _ => unreachable!(),
            })),
        }
    }
    fn minus(&self, other: &Self) -> R<Self> {
        self.plus(&other.negate())
    }
    fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        match (self, other) {
            (Self::Special(a), _) if a.is_nan() => None,
            (_, Self::Special(b)) if b.is_nan() => None,
            (Self::Special(a), Self::Special(b)) => a.partial_cmp(b),
            (Self::Special(a), _) => Some(if a.is_sign_negative() {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Greater
            }),
            (_, Self::Special(b)) => Some(if b.is_sign_negative() {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Less
            }),
            _ => self.finite().unwrap().partial_cmp(&other.finite().unwrap()),
        }
    }
    fn gt(&self, other: &Self) -> bool {
        self.compare(other) == Some(core::cmp::Ordering::Greater)
    }
    fn le(&self, other: &Self) -> bool {
        self.compare(other)
            .is_some_and(|v| v != core::cmp::Ordering::Greater)
    }
    fn minimum(&self, other: &Self) -> Self {
        if self.compare(other).is_none() {
            Self::Special(f64::NAN)
        } else if self.gt(other) {
            other.clone()
        } else {
            self.clone()
        }
    }
    fn quantize(&self, places: u32, rounding: crate::decimal::Rounding) -> R<Self> {
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
            _ => Ok(self.clone()),
        }
    }
    fn fixed(&self, places: Option<u32>) -> R<String> {
        let value = if let Some(places) = places {
            self.quantize(places, crate::decimal::Rounding::HalfUp)?
        } else {
            self.clone()
        };
        let mut text = match &value {
            Self::Finite(d) => d.to_string(),
            Self::NegativeZero => "0".into(),
            Self::Special(n) => {
                if n.is_nan() {
                    "NaN".into()
                } else if n.is_sign_negative() {
                    "-Infinity".into()
                } else {
                    "Infinity".into()
                }
            }
        };
        if places.is_some()
            && matches!(self,Self::Finite(d)if d.is_negative()&&!d.is_zero())
            && matches!(value, Self::NegativeZero)
        {
            text.insert(0, '-');
        }
        if !matches!(value, Self::Special(_)) {
            if let Some(places) = places {
                let had = text.split_once('.').map_or(0, |(_, d)| d.len());
                if places > 0 && had == 0 {
                    text.push('.')
                };
                for _ in had..places as usize {
                    text.push('0')
                }
            }
        }
        Ok(text)
    }
    fn value(&self) -> V {
        V::BigNumber(match self {
            Self::Finite(d) => fee_bn_text(d),
            Self::NegativeZero => "0".into(),
            Self::Special(n) => {
                if n.is_nan() {
                    "NaN".into()
                } else if n.is_sign_negative() {
                    "-Infinity".into()
                } else {
                    "Infinity".into()
                }
            }
        })
    }
}
fn insurance_series_text(value: &V) -> R<String> {
    match value {
        V::Null => Err(type_error(
            "Cannot read properties of null (reading 'toString')",
        )),
        V::Undefined => Err(type_error(
            "Cannot read properties of undefined (reading 'toString')",
        )),
        _ => text(value),
    }
}
// Core's object is a diagnostic representation of InsuranceFund.instances,
// which is a Map: integer-looking keys retain insertion order here.
fn insurance_cache_set(cache: &mut V, key: &str, value: V) -> R<()> {
    cache.insert(key, value).map_err(Fault::Local)
}
fn insurance_js_plus(left: &V, right: &V) -> R<V> {
    let primitive = |v: &V| match v {
        V::Array(_) | V::Object(_) | V::BigNumber(_) | V::Promise => text(v).map(s),
        _ => Ok(v.clone()),
    };
    let left = primitive(left)?;
    let right = primitive(right)?;
    Ok(
        if matches!(left, V::String(_)) || matches!(right, V::String(_)) {
            s(format!("{}{}", text(&left)?, text(&right)?))
        } else {
            n(number(&left)? + number(&right)?)
        },
    )
}
impl ContractCore {
    fn insurance_handle(&self, key: &str) -> V {
        obj(&[("sourceInsuranceInstance", s(key))])
    }
    fn insurance_receiver(&self, a: &[V]) -> R<String> {
        let V::String(key) = arg(a, 0).get("sourceInsuranceInstance") else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if matches!(self.insurance.get(&key), V::Undefined) {
            return Err(SourceRuleError::InvalidInput.into());
        };
        Ok(key)
    }
    fn insurance_get_instance(&mut self, contract: V, oracle: V) -> R<String> {
        let oracle = if matches!(oracle, V::Undefined) {
            V::Bool(false)
        } else {
            oracle
        };
        let key = Self::source_insurance_key(&contract, truth(&oracle))?;
        if matches!(self.insurance.get(&key), V::Undefined) {
            let mut instance = obj(&[
                ("contractSeriesId", contract),
                ("balances", V::Array(vec![])),
                ("hedgeRatio", n(0.5)),
                ("oracle", oracle),
            ]);
            self.source_insurance_load(&mut instance)?;
            insurance_cache_set(&mut self.insurance, &key, instance)?
        };
        Ok(key)
    }
    fn insurance_save_snapshot(&mut self, key: &str, snapshot: V) -> R<V> {
        let instance = self.insurance.get(key);
        let height = self.call("TxUtils.getBlockCount", vec![])?;
        let snapshot = if truth(&snapshot) {
            snapshot
        } else {
            obj(&[
                ("balances", instance.get("balances")),
                ("contractSeriesId", instance.get("contractSeriesId")),
                ("hedgeRatio", instance.get("hedgeRatio")),
                ("block", height),
            ])
        };
        self.database("insurance")?;
        self.update(
            "insurance",
            obj(&[("key", s(key))]),
            obj(&[("$set", obj(&[("value", snapshot)]))]),
            true,
        )?;
        Ok(V::Undefined)
    }
    fn insurance_get_snapshot(&mut self, key: &str) -> R<V> {
        let instance = self.insurance.get(key);
        self.database("insurance")?;
        let series = insurance_series_text(&instance.get("contractSeriesId"))?;
        let row = self.find("insurance", obj(&[("key", s(series))]))?;
        Ok(if truth(&row) {
            row.get("value")
        } else {
            V::Null
        })
    }
    fn insurance_record_event(&mut self, event_type: V, data: V, block: V) -> R<V> {
        let event = obj(&[
            ("type", event_type),
            ("data", data),
            ("timestamp", block.clone()),
        ]);
        self.database("insurance")?;
        self.db(
            "insurance",
            "insertAsync",
            vec![obj(&[
                ("key", s(format!("event-{}", text(&block)?))),
                ("value", event),
            ])],
        )?;
        Ok(V::Undefined)
    }
    fn insurance_withdraw(&mut self, key: &str, amount: V, property: V, block: V) -> R<V> {
        let mut instance = self.insurance.get(key);
        let V::Array(mut balances) = instance.get("balances") else {
            return Err(type_error("this.balances.find is not a function"));
        };
        let Some(index) = balances
            .iter()
            .position(|b| same(&b.get("propertyId"), &property))
        else {
            return Err(error("Insufficient balance in the insurance fund"));
        };
        let available = balances[index].get("amountAvailable");
        if crate::oracle_source_owner::source_greater_unverified(&amount, &available)? {
            return Err(error("Insufficient balance in the insurance fund"));
        };
        set(
            &mut balances[index],
            "amountAvailable",
            n(number(&available)? - number(&amount)?),
        )?;
        set(&mut instance, "balances", V::Array(balances))?;
        insurance_cache_set(&mut self.insurance, key, instance.clone())?;
        self.insurance_record_event(
            s("withdraw"),
            obj(&[
                ("contractId", instance.get("contractSeriesId")),
                ("propertyId", property),
                ("amount", amount),
            ]),
            block,
        )?;
        self.insurance_save_snapshot(key, V::Null)
    }
    fn insurance_calc_payout(&mut self, key: &str, loss: V, block: V) -> R<V> {
        let mut instance = self.insurance.get(key);
        let V::Array(mut balances) = instance.get("balances") else {
            return Err(type_error("this.balances is not iterable"));
        };
        let mut total = InsuranceBn::zero();
        for balance in &balances {
            total = total.plus(&InsuranceBn::new(&balance.get("amountAvailable"))?)?
        }
        if !matches!(loss, V::BigNumber(_)) {
            return Err(type_error(if matches!(loss, V::Null) {
                "Cannot read properties of null (reading 'abs')"
            } else if matches!(loss, V::Undefined) {
                "Cannot read properties of undefined (reading 'abs')"
            } else {
                "lossBN.abs is not a function"
            }));
        };
        let loss = InsuranceBn::new(&loss)?;
        let payout = if loss.abs().gt(&total) {
            total
        } else {
            loss.clone()
        };
        let mut remaining = payout.abs();
        for index in 0..balances.len() {
            if remaining.le(&InsuranceBn::zero()) {
                break;
            };
            let available = InsuranceBn::new(&balances[index].get("amountAvailable"))?;
            if available.gt(&InsuranceBn::zero()) {
                let debit = available.minimum(&remaining);
                let after = available
                    .minus(&debit)?
                    .quantize(8, crate::decimal::Rounding::HalfUp)?;
                set(&mut balances[index], "amountAvailable", n(after.number()))?;
                remaining = remaining.minus(&debit)?;
                set(&mut instance, "balances", V::Array(balances.clone()))?;
                insurance_cache_set(&mut self.insurance, key, instance.clone())?;
            }
        }
        if remaining.gt(&InsuranceBn::zero()) {
            return Err(type_error("this.handleDeficit is not a function"));
        };
        let series = instance.get("contractSeriesId");
        let loss_n = n(loss.quantize(8, crate::decimal::Rounding::HalfUp)?.number());
        let payout_n = n(payout
            .quantize(8, crate::decimal::Rounding::HalfUp)?
            .number());
        let timestamp = self.call("Clock.nowISOString", vec![])?;
        let event = obj(&[
            ("type", s("payout")),
            ("contractId", series.clone()),
            ("block", block.clone()),
            ("totalLoss", loss_n.clone()),
            ("payoutAmount", payout_n.clone()),
            ("timestamp", timestamp),
        ]);
        self.database("insurance")?;
        self.db(
            "insurance",
            "insertAsync",
            vec![obj(&[
                (
                    "key",
                    s(format!("payout-{}-{}", text(&series)?, text(&block)?)),
                ),
                ("value", event),
            ])],
        )?;
        self.insurance_record_event(
            s("payout"),
            obj(&[
                ("contractId", series),
                ("block", block),
                ("totalLoss", loss_n),
                ("payoutAmount", payout_n),
            ]),
            V::Undefined,
        )?;
        self.insurance_save_snapshot(key, V::Null)?;
        Ok(payout
            .quantize(8, crate::decimal::Rounding::HalfUp)?
            .value())
    }
    fn insurance_liquidate(&mut self, a: &[V]) -> R<V> {
        let result = (|| {
            let key = self.insurance_get_instance(arg(a, 1), arg(a, 2))?;
            let instance = self.insurance.get(&key);
            insurance_series_text(&instance.get("contractSeriesId"))?;
            let snapshot = self.insurance_get_snapshot(&key)?;
            if !truth(&snapshot) {
                return Err(error("Insurance fund snapshot not found"));
            };
            let V::Array(balances) = snapshot.get("balances") else {
                return Err(type_error("snapshot.balances.reduce is not a function"));
            };
            let mut total = n(0.0);
            for balance in balances {
                total = insurance_js_plus(&total, &balance.get("amountAvailable"))?
            }
            if same(&total, &n(0.0)) {
                return Ok(V::Undefined);
            };
            let payout = n(number(&total)? / 2.0);
            self.call(
                "Tally.updateBalance",
                vec![
                    arg(a, 0),
                    arg(a, 1),
                    payout,
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s("credit"),
                    snapshot.get("block"),
                ],
            )?;
            Err(type_error("InsuranceFund.updateFeeCache is not a function"))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn insurance_total(&mut self, property: V) -> R<V> {
        self.database("insurance")?;
        let rows = array(&self.db("insurance", "findAsync", vec![obj(&[])])?)?;
        let mut total = InsuranceBn::zero();
        for row in rows {
            let value = row.get("value");
            let balances = value.get("balances");
            if truth(&value) && truth(&balances) {
                let V::Array(balances) = balances else {
                    return Err(type_error("entry.value.balances is not iterable"));
                };
                for balance in balances {
                    if text(&balance.get("propertyId"))? == text(&property)? {
                        let available = balance.get("amountAvailable");
                        total = total.plus(&InsuranceBn::new(&if truth(&available) {
                            available
                        } else {
                            n(0.0)
                        })?)?
                    }
                }
            }
        }
        Ok(total.value())
    }
    fn insurance_summary(&mut self) -> R<V> {
        self.database("insurance")?;
        let rows = array(&self.db("insurance", "findAsync", vec![obj(&[])])?)?;
        let mut summary: Vec<(String, InsuranceBn)> = vec![];
        for row in rows {
            let value = row.get("value");
            let balances = value.get("balances");
            if truth(&value) && truth(&balances) {
                let V::Array(balances) = balances else {
                    return Err(type_error("entry.value.balances is not iterable"));
                };
                for balance in balances {
                    let property = text(&balance.get("propertyId"))?;
                    if matches!(
                        property.as_str(),
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
                    ) {
                        return Err(type_error(
                            "insuranceSummary[propertyId].plus is not a function",
                        ));
                    };
                    let available = balance.get("amountAvailable");
                    let amount =
                        InsuranceBn::new(&if truth(&available) { available } else { n(0.0) })?;
                    if let Some((_, old)) = summary.iter_mut().find(|(p, _)| p == &property) {
                        *old = old.plus(&amount)?
                    } else {
                        summary.push((property, amount))
                    }
                }
            }
        }
        let mut totals = obj(&[]);
        for (property, total) in summary {
            set(&mut totals, &property, s(total.fixed(Some(8))?))?
        }
        self.update(
            "insurance",
            obj(&[("_id", s("insuranceSummary"))]),
            obj(&[("$set", obj(&[("totals", totals)]))]),
            true,
        )?;
        Ok(V::Undefined)
    }
    fn source_insurance_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getInstance" => {
                let key = self.insurance_get_instance(arg(a, 0), arg(a, 1))?;
                Ok(self.insurance_handle(&key))
            }
            "liquidate" => self.insurance_liquidate(a),
            "getTotalBalanceForProperty" => self.insurance_total(arg(a, 0)),
            "updateInsuranceSummary" => self.insurance_summary(),
            "getInsuranceFundBalance" => {
                self.database("insurance")?;
                let row = self.find("insurance", obj(&[("_id", arg(a, 0))]))?;
                Ok(if truth(&row) {
                    InsuranceBn::new(&row.get("amount"))?
                } else {
                    InsuranceBn::zero()
                }
                .value())
            }
            "loadFromSnapshot" | "deposit" | "withdraw" | "saveSnapshot" | "getSnapshot"
            | "recordEvent" | "getPayouts" | "calcPayout" => {
                let key = self.insurance_receiver(a)?;
                let args = &a[1..];
                match method {
                    "loadFromSnapshot" => {
                        let mut instance = self.insurance.get(&key);
                        self.source_insurance_load(&mut instance)?;
                        insurance_cache_set(&mut self.insurance, &key, instance)?;
                        Ok(V::Undefined)
                    }
                    "deposit" => self.source_insurance_deposit(
                        &key,
                        arg(args, 0),
                        arg(args, 1),
                        arg(args, 2),
                    ),
                    "withdraw" => {
                        self.insurance_withdraw(&key, arg(args, 0), arg(args, 1), arg(args, 2))
                    }
                    "saveSnapshot" => self.insurance_save_snapshot(&key, arg(args, 0)),
                    "getSnapshot" => self.insurance_get_snapshot(&key),
                    "recordEvent" => {
                        self.insurance_record_event(arg(args, 0), arg(args, 1), arg(args, 2))
                    }
                    "calcPayout" => self.insurance_calc_payout(&key, arg(args, 0), arg(args, 1)),
                    _ => {
                        self.database("insurance")?;
                        let query = obj(&[
                            (
                                "key",
                                obj(&[("$regex", s(format!("^payout-{}-", text(&arg(args, 0))?)))]),
                            ),
                            (
                                "value.block",
                                obj(&[("$gte", arg(args, 1)), ("$lte", arg(args, 2))]),
                            ),
                        ]);
                        let rows = array(&self.db("insurance", "findAsync", vec![query])?)?;
                        Ok(V::Array(rows.into_iter().map(|r| r.get("value")).collect()))
                    }
                }
            }
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
}
