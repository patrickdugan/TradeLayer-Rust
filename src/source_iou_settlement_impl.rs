// Included in the same ContractCore as the source IOU accrual owner. Allocation
// is literal source accounting, not a conserving payout or a second ledger.
impl InsuranceBn {
    fn negative(&self) -> bool {
        match self {
            Self::Finite(d) => d.is_negative(),
            Self::NegativeZero => true,
            Self::Special(n) => n.is_sign_negative(),
        }
    }
    fn is_zero(&self) -> bool {
        self.finite().is_some_and(|d| d.is_zero())
    }
    fn times(&self, other: &Self) -> R<Self> {
        let negative = self.negative() != other.negative();
        match (self.finite(), other.finite()) {
            (Some(a), Some(b)) => {
                let product = mul(&a, &b)?;
                Ok(if product.is_zero() && negative {
                    Self::NegativeZero
                } else {
                    Self::Finite(product)
                })
            }
            _ => {
                let nan = matches!(self,Self::Special(n)if n.is_nan())
                    || matches!(other,Self::Special(n)if n.is_nan())
                    || self.is_zero()
                    || other.is_zero();
                Ok(Self::Special(if nan {
                    f64::NAN
                } else if negative {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                }))
            }
        }
    }
    fn divided(&self, other: &Self) -> R<Self> {
        let negative = self.negative() != other.negative();
        let zero = || {
            if negative {
                Self::NegativeZero
            } else {
                Self::zero()
            }
        };
        let infinite = || {
            Self::Special(if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            })
        };
        if matches!(self,Self::Special(n)if n.is_nan())
            || matches!(other,Self::Special(n)if n.is_nan())
        {
            return Ok(Self::Special(f64::NAN));
        };
        match (self.finite(), other.finite()) {
            (Some(a), Some(b)) => {
                if b.is_zero() {
                    return Ok(if a.is_zero() {
                        Self::Special(f64::NAN)
                    } else {
                        infinite()
                    });
                };
                let value = div(&a, &b)?;
                Ok(if value.is_zero() {
                    zero()
                } else {
                    Self::Finite(value)
                })
            }
            (None, None) => Ok(Self::Special(f64::NAN)),
            (None, Some(_)) => Ok(infinite()),
            (Some(_), None) => Ok(zero()),
        }
    }
    fn radix_ten(&self) -> R<String> {
        self.quantize(20, crate::decimal::Rounding::HalfUp)?
            .fixed(None)
    }
}
fn iou_source_entries(value: &V) -> R<Vec<(String, V)>> {
    let mut fields = match value {
        V::Object(fields) => fields.clone(),
        V::Array(values) => values
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        V::String(value) => {
            if value.chars().any(|c| c.len_utf16() != 1) {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            };
            value
                .chars()
                .enumerate()
                .map(|(i, v)| (i.to_string(), s(v.to_string())))
                .collect()
        }
        V::Null | V::Undefined => {
            return Err(type_error("Cannot convert undefined or null to object"))
        }
        V::BigNumber(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
        _ => vec![],
    };
    // Object.entries enumerates index keys numerically, then ordinary insertion
    // order. This also covers imported source documents with explicit fields.
    let index = |key: &str| {
        key.parse::<u32>()
            .ok()
            .filter(|i| *i != u32::MAX && i.to_string() == key)
    };
    fields.sort_by_key(|(k, _)| index(k));
    let split = fields.partition_point(|(k, _)| index(k).is_none());
    fields.rotate_left(split);
    Ok(fields)
}
impl ContractCore {
    fn iou_source_key(contract: &V, property: &V) -> R<String> {
        Ok(format!("{}:{}", text(contract)?, text(property)?))
    }
    fn iou_source_claim_key(contract: &V, property: &V) -> R<String> {
        Ok(format!(
            "claims:{}:{}",
            text(&n(number(contract)?))?,
            text(&n(number(property)?))?
        ))
    }
    fn iou_source_get_doc(&mut self, contract: V, property: V) -> R<V> {
        let key = Self::iou_source_key(&contract, &property)?;
        self.database("iou")?;
        match self.find("iou", obj(&[("_id", s(key))])) {
            Err(Fault::Thrown(_)) => Ok(V::Null),
            other => other.map(|v| if truth(&v) { v } else { V::Null }),
        }
    }
    fn iou_source_bucket(&mut self, contract: V, property: V) -> R<V> {
        let mut doc = self.iou_source_get_doc(contract, property)?;
        if !truth(&doc) {
            return Ok(V::Null);
        };
        for field in [
            "amount",
            "blockStartAmount",
            "blockDelta",
            "blockLosses",
            "blockProfits",
        ] {
            let original = doc.get(field);
            let value = InsuranceBn::new(&if truth(&original) { original } else { n(0.0) })?;
            set(&mut doc, field, value.value())?
        }
        Ok(doc)
    }
    fn iou_source_claims(&mut self, contract: V, property: V) -> R<V> {
        self.database("iou")?;
        let key = Self::iou_source_claim_key(&contract, &property)?;
        self.find("iou", obj(&[("_id", s(key))]))
    }
    fn iou_source_save_claims(&mut self, doc: V) -> R<V> {
        self.database("iou")?;
        if matches!(doc, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading '_id')",
                if matches!(doc, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        };
        self.update("iou", obj(&[("_id", doc.get("_id"))]), doc, true)?;
        Ok(V::Undefined)
    }
    fn iou_source_audit(&mut self, event: V) -> R<V> {
        self.database("iou")?;
        if matches!(event, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading 'block')",
                if matches!(event, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        };
        self.random = self
            .random
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let random = f64::from(self.random) / 4_294_967_296.0;
        self.emit(obj(&[("kind", s("random")), ("returned", n(random))]));
        let id = format!(
            "audit:{}:{}:{}:{}:{}",
            text(&event.get("block"))?,
            text(&event.get("contractId"))?,
            text(&event.get("propertyId"))?,
            self.time_millis,
            text(&n(random))?
        );
        let mut doc = obj(&[
            ("_id", s(id)),
            ("type", s("audit")),
            ("ts", n(self.time_millis as f64)),
        ]);
        for (k, v) in iou_source_entries(&event)? {
            set(&mut doc, &k, v)?
        }
        match self.db("iou", "insertAsync", vec![doc]) {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other.map(|_| V::Undefined),
        }
    }
    fn iou_source_get(&mut self, contract: V, property: V) -> R<V> {
        let key = Self::iou_source_key(&contract, &property)?;
        self.database("iou")?;
        let result = (|| {
            let doc = self.find("iou", obj(&[("_id", s(key))]))?;
            Ok(if !truth(&doc) {
                InsuranceBn::zero()
            } else {
                InsuranceBn::new(&doc.get("amount"))?
            }
            .value())
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(InsuranceBn::zero().value()),
            other => other,
        }
    }
    fn iou_source_pay(&mut self, a: &[V]) -> R<V> {
        let contract = arg(a, 0);
        let property = arg(a, 1);
        let mark = arg(a, 2);
        let block = arg(a, 3);
        let delta = InsuranceBn::new(&if truth(&mark) { mark } else { n(0.0) })?;
        if delta.le(&InsuranceBn::zero()) {
            return Ok(V::Array(vec![]));
        };
        let bucket = self.iou_source_bucket(contract.clone(), property.clone())?;
        if !truth(&bucket) {
            return Ok(V::Array(vec![]));
        };
        if number(&bucket.get("lastBlock"))? != number(&block)? {
            return Ok(V::Array(vec![]));
        };
        let available = InsuranceBn::new(&bucket.get("blockLosses"))?;
        if available.le(&InsuranceBn::zero()) {
            return Ok(V::Array(vec![]));
        };
        let cap = delta.minimum(&available);
        if cap.le(&InsuranceBn::zero()) {
            return Ok(V::Array(vec![]));
        };
        let mut claims = self.iou_source_claims(contract.clone(), property.clone())?;
        if !truth(&claims) || !truth(&claims.get("claims")) {
            return Ok(V::Array(vec![]));
        };
        let mut entries = vec![];
        for (address, value) in iou_source_entries(&claims.get("claims"))? {
            let claim = InsuranceBn::new(&if truth(&value) { value } else { n(0.0) })?;
            if claim.gt(&InsuranceBn::zero()) {
                entries.push((address, claim))
            }
        }
        if entries.is_empty() {
            return Ok(V::Array(vec![]));
        };
        let mut total = InsuranceBn::zero();
        for (_, claim) in &entries {
            total = total.plus(claim)?
        }
        let payout = cap.minimum(&total);
        if payout.le(&InsuranceBn::zero()) {
            return Ok(V::Array(vec![]));
        };
        self.iou_source_audit(obj(&[
            ("event", s("payout_summary")),
            ("block", block.clone()),
            ("contractId", contract.clone()),
            ("propertyId", property.clone()),
            ("payout", s(payout.radix_ten()?)),
            ("blockLosses", s(available.radix_ten()?)),
            ("totalClaims", s(total.radix_ten()?)),
            ("claimants", n(entries.len() as f64)),
        ]))?;
        // The source loads MarginMap but never uses it to credit anyone. Do not
        // replace this read with a fabricated handle or repair the omission.
        self.call("Margin.getInstance", vec![contract.clone()])?;
        let mut allocations = vec![];
        let mut map = claims.get("claims");
        for (address, claim) in entries {
            let share = payout.times(&claim)?.divided(&total)?;
            let remaining = claim.minus(&share)?;
            if remaining.le(&InsuranceBn::zero()) {
                match &mut map {
                    V::Object(fields) => fields.retain(|(k, _)| k != &address),
                    V::Array(_) | V::String(_) => {
                        return Err(SourceRuleError::UnsupportedNumericDomain.into())
                    }
                    _ => (),
                }
            } else {
                set(&mut map, &address, s(remaining.radix_ten()?))?
            };
            allocations.push(obj(&[("address", s(&address)), ("amount", share.value())]));
            let nonnegative = if remaining.compare(&InsuranceBn::zero()).is_none() {
                InsuranceBn::Special(f64::NAN)
            } else if remaining.gt(&InsuranceBn::zero()) {
                remaining
            } else {
                InsuranceBn::zero()
            };
            self.iou_source_audit(obj(&[
                ("event", s("payout_item")),
                ("block", block.clone()),
                ("contractId", contract.clone()),
                ("propertyId", property.clone()),
                ("address", s(&address)),
                ("paid", s(share.radix_ten()?)),
                ("claimAfter", s(nonnegative.radix_ten()?)),
            ]))?;
        }
        set(&mut claims, "claims", map)?;
        set(&mut claims, "lastBlock", block)?;
        self.iou_source_save_claims(claims)?;
        Ok(V::Array(allocations))
    }
    fn source_iou_settlement_call(&mut self, method: &str, a: &[V]) -> R<V> {
        let contract = arg(a, 0);
        let property = arg(a, 1);
        match method {
            "absorbDust" => Err(type_error("PnlIou.absorbDust is not a function")),
            "key" => Ok(s(Self::iou_source_key(&contract, &property)?)),
            "claimKey" => Ok(s(Self::iou_source_claim_key(&contract, &property)?)),
            "_db" => self.database("iou"),
            "get" => self.iou_source_get(contract, property),
            "getDoc" => self.iou_source_get_doc(contract, property),
            "getBucket" => self.iou_source_bucket(contract, property),
            "getClaimMap" => self.iou_source_claims(contract, property),
            "saveClaimMap" => self.iou_source_save_claims(contract),
            "audit" => self.iou_source_audit(contract),
            "payOutstandingIous" => self.iou_source_pay(a),
            "blockReductionTowardZero" => {
                let start = contract.get("blockStartAmount");
                let end = contract.get("amount");
                let start = InsuranceBn::new(&if truth(&start) { start } else { n(0.0) })?;
                let end = InsuranceBn::new(&if truth(&end) { end } else { n(0.0) })?;
                let reduced = start.abs().minus(&end.abs())?;
                Ok(if reduced.gt(&InsuranceBn::zero()) {
                    reduced
                } else {
                    InsuranceBn::zero()
                }
                .value())
            }
            "addDelta" => {
                let value = arg(a, 2);
                let value = InsuranceBn::new(&if truth(&value) { value } else { n(0.0) })?;
                if value.is_zero() {
                    return Ok(V::Null);
                };
                let method = if value.gt(&InsuranceBn::zero()) {
                    "addLoss"
                } else {
                    "addProfit"
                };
                self.call(
                    &format!("PnlIou.{method}"),
                    vec![
                        contract,
                        property,
                        if method == "addProfit" {
                            value.abs().value()
                        } else {
                            value.value()
                        },
                        arg(a, 3),
                    ],
                )
            }
            "applyToLosers" => {
                let deficit = InsuranceBn::new(&self.iou_source_get(contract.clone(), arg(a, 3))?)?;
                if deficit
                    .compare(&InsuranceBn::zero())
                    .is_some_and(|o| o != core::cmp::Ordering::Less)
                {
                    return Ok(V::Undefined);
                };
                if !matches!(property, V::BigNumber(_)) {
                    return Err(type_error(if matches!(property, V::Null) {
                        "Cannot read properties of null (reading 'lte')"
                    } else if matches!(property, V::Undefined) {
                        "Cannot read properties of undefined (reading 'lte')"
                    } else {
                        "markDelta.lte is not a function"
                    }));
                };
                let delta = InsuranceBn::new(&property)?;
                if delta.le(&InsuranceBn::zero()) {
                    return Ok(V::Undefined);
                };
                let payout = delta.minimum(&deficit.abs()).negate();
                self.source_iou_settlement_call(
                    "addDelta",
                    &[contract, arg(a, 3), payout.value(), arg(a, 2)],
                )?;
                Ok(V::Undefined)
            }
            "getTotalForProperty" => {
                self.database("iou")?;
                let pid = number(&contract)?;
                let docs = match self.db("iou", "findAsync", vec![obj(&[])]) {
                    Err(Fault::Thrown(_)) => return Ok(InsuranceBn::zero().value()),
                    other => array(&other?)?,
                };
                let mut total = InsuranceBn::zero();
                for doc in docs {
                    if doc.get("type") != s("audit") && number(&doc.get("propertyId"))? == pid {
                        let amount = doc.get("amount");
                        total = total.plus(&InsuranceBn::new(&if truth(&amount) {
                            amount
                        } else {
                            n(0.0)
                        })?)?
                    }
                }
                Ok(
                    if total
                        .compare(&InsuranceBn::zero())
                        .is_some_and(|o| o != core::cmp::Ordering::Less)
                    {
                        total.value()
                    } else {
                        n(0.0)
                    },
                )
            }
            "delete" | "deleteForProperty" => {
                self.database("iou")?;
                let query = if method == "delete" {
                    obj(&[("_id", s(Self::iou_source_key(&contract, &property)?))])
                } else {
                    obj(&[("propertyId", n(number(&contract)?))])
                };
                self.db(
                    "iou",
                    "removeAsync",
                    vec![
                        query,
                        obj(&[("multi", V::Bool(method == "deleteForProperty"))]),
                    ],
                )?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownMethod.into()),
        }
    }
}
