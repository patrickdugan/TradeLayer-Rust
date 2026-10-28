// Included by source_contract_runtime.rs. Actual Tally fee routing and reached
// Insurance deposit/event/snapshot methods; all balances use the same owner.
type FeeDecimal = crate::decimal::Decimal;
fn fee_bn(value: &V) -> R<FeeDecimal> {
    // Explicit finite BigNumber constructor domain. Null/boolean are not
    // Decimal zero/one in BigNumber; callers apply source ||0 first where used.
    if !matches!(
        value,
        V::Number(_) | V::String(_) | V::BigNumber(_) | V::Array(_)
    ) {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    if matches!(value,V::Number(bits)if !f64::from_bits(*bits).is_finite()) {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    dec(value)
}
fn fee_bn0(value: &V) -> R<FeeDecimal> {
    if truth(value) {
        fee_bn(value)
    } else {
        Ok(FeeDecimal::zero())
    }
}
fn fee_down(value: &FeeDecimal, places: u32) -> R<FeeDecimal> {
    value
        .quantize(places, crate::decimal::Rounding::Down)
        .map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
fn fee_from_sats(value: &FeeDecimal) -> R<FeeDecimal> {
    div(value, &FeeDecimal::from(100_000_000u32))
}
fn fee_bn_text(value: &FeeDecimal) -> String {
    let exponent = value.coefficient_digits() as i32 - value.decimal_places() as i32 - 1;
    if value.is_zero() || (-6..21).contains(&exponent) {
        return value.to_string();
    }
    let full = value.abs().to_string();
    let digits = full.chars().filter(|c| *c != '.').collect::<String>();
    let digits = digits.trim_start_matches('0').trim_end_matches('0');
    format!(
        "{}{}{}e{}{exponent}",
        if value.is_negative() { "-" } else { "" },
        &digits[..1],
        if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        },
        if exponent >= 0 { "+" } else { "" }
    )
}
fn fee_fixed8(value: &FeeDecimal) -> R<String> {
    let value = q(value, 8)?;
    let mut value = value.to_string();
    let existing = value.split_once('.').map_or(0, |(_, v)| v.len());
    if existing == 0 {
        value.push('.')
    }
    for _ in existing..8 {
        value.push('0')
    }
    Ok(value)
}
impl ContractCore {
    fn source_fee_resolve_block(&mut self, block: V) -> R<V> {
        if !matches!(block, V::Undefined | V::Null) {
            return Ok(block);
        }
        let result = (|| {
            self.database("consensus")?;
            let track = self.find("consensus", obj(&[("_id", s("TrackHeight"))]))?;
            let max = self.find("consensus", obj(&[("_id", s("MaxProcessedHeight"))]))?;
            Ok(if truth(&track.get("value")) {
                track.get("value")
            } else if truth(&max.get("value")) {
                max.get("value")
            } else {
                V::Null
            })
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Null),
            other => other,
        }
    }
    fn source_update_fee_cache(&mut self, a: &[V]) -> R<V> {
        let block = if a.len() >= 6 { arg(a, 5) } else { arg(a, 3) };
        self.source_queue_fee(&[arg(a, 0), arg(a, 1), arg(a, 2), block])
    }
    fn source_queue_fee(&mut self, a: &[V]) -> R<V> {
        let resolved = self.source_fee_resolve_block(arg(a, 3))?;
        let numeric = crate::oracle_source_owner::source_number_unverified(&resolved)
            .map_err(Fault::Local)?;
        if matches!(resolved, V::Null | V::Undefined) || numeric.is_nan() {
            return self.source_accrue_fee(a);
        }
        let block = s(text(&resolved)?);
        let contract = if matches!(arg(a, 2), V::Null | V::Undefined) {
            s("1")
        } else {
            s(text(&arg(a, 2))?)
        };
        let key = format!("{}-{}", text(&arg(a, 0))?, text(&contract)?);
        let amount = q(&fee_bn0(&arg(a, 1))?, 8)?;
        if amount <= FeeDecimal::zero() {
            return Ok(V::Undefined);
        }
        if self.spot_lifecycle_enabled() {
            self.source_spot_fee_buffer_initialized = true;
        }
        let bi = if let Some(i) = self
            .fee_buffer
            .iter()
            .position(|(key, _)| same(key, &block))
        {
            i
        } else {
            if self.fee_buffer.len() >= MAX_TABLE_ROWS {
                return Err(SourceRuleError::Capacity.into());
            }
            self.fee_buffer.push((block, vec![]));
            self.fee_buffer.len() - 1
        };
        if let Some(i) = self.fee_buffer[bi].1.iter().position(|(k, _)| k == &key) {
            let old = dec(&self.fee_buffer[bi].1[i].1.get("amount"))?;
            let amount = q(&add(&old, &amount)?, 8)?;
            set(
                &mut self.fee_buffer[bi].1[i].1,
                "amount",
                V::BigNumber(fee_bn_text(&amount)),
            )?;
        } else {
            if self.fee_buffer[bi].1.len() >= MAX_TABLE_ROWS {
                return Err(SourceRuleError::Capacity.into());
            }
            self.fee_buffer[bi].1.push((
                key,
                obj(&[
                    ("propertyId", arg(a, 0)),
                    ("contractId", contract),
                    ("amount", V::BigNumber(fee_bn_text(&amount))),
                ]),
            ));
        }
        Ok(V::Undefined)
    }
    fn source_flush_fees(&mut self, a: &[V]) -> R<V> {
        let resolved = self.source_fee_resolve_block(arg(a, 0))?;
        let key = text(&resolved)?;
        if self.spot_lifecycle_enabled() {
            self.source_spot_fee_buffer_initialized = true;
        }
        let Some(i) = self
            .fee_buffer
            .iter()
            .position(|(b, _)| text(b).is_ok_and(|b| b == key))
        else {
            return Ok(V::Undefined);
        };
        for (_, entry) in self.fee_buffer[i].1.clone() {
            self.call(
                "Tally.accrueFee",
                vec![
                    entry.get("propertyId"),
                    dn(&dec(&entry.get("amount"))?)?,
                    entry.get("contractId"),
                    resolved.clone(),
                ],
            )?;
        }
        self.fee_buffer.remove(i);
        Ok(V::Undefined)
    }
    fn source_flush_all_fees(&mut self) -> R<V> {
        if self.spot_lifecycle_enabled() {
            self.source_spot_fee_buffer_initialized = true;
        }
        let mut keys = self
            .fee_buffer
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        keys.sort_by(|a, b| {
            let a = number(a).unwrap_or(f64::NAN);
            let b = number(b).unwrap_or(f64::NAN);
            (a - b)
                .partial_cmp(&0.0)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        for key in keys {
            self.source_flush_fees(&[n(number(&key)?)])?;
        }
        Ok(V::Undefined)
    }
    fn source_fee_load_row(&mut self, key: &str) -> R<(FeeDecimal, FeeDecimal)> {
        let row = self.find("feeCache", obj(&[("_id", s(key))]))?;
        Ok((fee_bn0(&row.get("value"))?, fee_bn0(&row.get("stash"))?))
    }
    fn source_fee_save_row(
        &mut self,
        key: &str,
        value: FeeDecimal,
        stash: FeeDecimal,
        contract: V,
    ) -> R<()> {
        self.update(
            "feeCache",
            obj(&[("_id", s(key))]),
            obj(&[(
                "$set",
                obj(&[
                    ("value", dn(&q(&value, 8)?)?),
                    ("stash", dn(&q(&stash, 8)?)?),
                    ("contract", contract),
                ]),
            )]),
            true,
        )?;
        Ok(())
    }
    fn source_insurance_key(contract: &V, oracle: bool) -> R<String> {
        Ok(format!(
            "{}{}",
            text(contract)?,
            if oracle { "-oracle" } else { "" }
        ))
    }
    fn source_insurance_load(&mut self, instance: &mut V) -> R<()> {
        if !self.global_clearing_enabled() {
            return self.legacy_source_insurance_load(instance);
        }
        // Literal loadFromSnapshot calls .toString(), unlike the template key
        // used by getInstance. Null and undefined fail before DB access.
        insurance_series_text(&instance.get("contractSeriesId"))?;
        let key = Self::source_insurance_key(
            &instance.get("contractSeriesId"),
            truth(&instance.get("oracle")),
        )?;
        self.database("insurance")?;
        let row = self.find("insurance", obj(&[("key", s(key))]))?;
        self.source_insurance_apply_loaded(instance, &row)
    }

    fn source_insurance_instance(&mut self, contract: V, oracle: bool) -> R<String> {
        if !self.global_clearing_enabled() {
            return self.legacy_source_insurance_instance(contract, oracle);
        }
        let key = Self::source_insurance_key(&contract, oracle)?;
        if matches!(self.insurance.get(&key), V::Undefined) {
            let mut instance = obj(&[
                ("contractSeriesId", contract),
                ("balances", V::Array(vec![])),
                ("hedgeRatio", n(0.5)),
                ("oracle", V::Bool(oracle)),
            ]);
            self.source_insurance_load(&mut instance)?;
            insurance_cache_set(&mut self.insurance, &key, instance)?;
        }
        Ok(key)
    }
    fn source_insurance_deposit(&mut self, key: &str, property: V, amount: V, block: V) -> R<V> {
        if !self.global_clearing_enabled() {
            return self.legacy_source_insurance_deposit(key, property, amount, block);
        }
        let mut instance = self.insurance.get(key);
        self.source_insurance_load(&mut instance)?;
        insurance_cache_set(&mut self.insurance, key, instance.clone())?;
        let Some((event, snapshot)) =
            self.source_insurance_credit(&mut instance, property, amount, block.clone())?
        else {
            return Ok(V::Undefined);
        };
        insurance_cache_set(&mut self.insurance, key, instance)?;
        self.database("insurance")?;
        self.db(
            "insurance",
            "insertAsync",
            vec![obj(&[
                ("key", s(format!("event-{}", text(&block)?))),
                ("value", event),
            ])],
        )?;
        self.call("TxUtils.getBlockCount", vec![])?;
        self.database("insurance")?;
        self.update(
            "insurance",
            obj(&[("key", s(key))]),
            obj(&[("$set", obj(&[("value", snapshot)]))]),
            true,
        )?;
        Ok(V::Undefined)
    }

    fn source_fee_deposit(
        &mut self,
        contract: V,
        oracle: bool,
        property: V,
        sats: &FeeDecimal,
        fixed: bool,
        block: V,
    ) -> R<()> {
        let key = self.source_insurance_instance(contract, oracle)?;
        let tokens = fee_from_sats(sats)?;
        let amount = if fixed {
            s(fee_fixed8(&tokens)?)
        } else {
            dn(&tokens)?
        };
        self.source_insurance_deposit(&key, property, amount, block)?;
        Ok(())
    }
    fn source_fee_credit_dust(
        &mut self,
        property: V,
        contract: &V,
        is_spot: bool,
        key: &str,
        whole: &FeeDecimal,
        block: V,
    ) -> R<()> {
        if is_spot && text(&property)? != "1" {
            let (value, stash) = self.source_fee_load_row(key)?;
            return self.source_fee_save_row(
                key,
                add(&value, &fee_from_sats(whole)?)?,
                stash,
                s("1"),
            );
        }
        if is_spot {
            let insurance = fee_down(&div(whole, &FeeDecimal::from(2u32))?, 0)?;
            let stash = sub(whole, &insurance)?;
            if insurance > FeeDecimal::zero() {
                self.source_fee_deposit(s("1"), false, s("1"), &insurance, true, block)?;
            }
            if stash > FeeDecimal::zero() {
                let (value, old) = self.source_fee_load_row(key)?;
                self.source_fee_save_row(key, value, add(&old, &fee_from_sats(&stash)?)?, s("1"))?;
            }
            return Ok(());
        }
        self.source_fee_deposit(
            contract.clone(),
            text(contract)? != "1",
            property,
            whole,
            false,
            block,
        )
    }
    fn source_accrue_fee(&mut self, a: &[V]) -> R<V> {
        self.database("feeCache")?;
        let raw = mul(&fee_bn(&arg(a, 1))?, &FeeDecimal::from(100_000_000u32))?;
        let sats = fee_down(&raw, 0)?;
        if sats <= FeeDecimal::zero() {
            return Ok(V::Undefined);
        }
        let property = arg(a, 0);
        let contract_arg = arg(a, 2);
        let is_spot = matches!(contract_arg, V::Null | V::Undefined);
        let contract = if is_spot {
            s("1")
        } else {
            s(text(&contract_arg)?)
        };
        let cache = format!("{}-{}", text(&property)?, text(&contract)?);
        let block = self.source_fee_resolve_block(arg(a, 3))?;
        let dust = sub(&raw, &sats)?;
        if !dust.is_zero() {
            let dust_key = format!("dust::{cache}");
            let loaded = self.db("feeCache", "get", vec![s(&dust_key)]);
            let previous = match loaded {
                Ok(row) => fee_bn(&nullish(row.get("dustSats"), n(0.0)))?,
                Err(Fault::Thrown(_)) => FeeDecimal::zero(),
                Err(e) => return Err(e),
            };
            let next = add(&previous, &dust)?;
            let whole = fee_down(&next, 0)?;
            let frac = sub(&next, &whole)?;
            if whole > FeeDecimal::zero() {
                self.source_fee_credit_dust(
                    property.clone(),
                    &contract,
                    is_spot,
                    &cache,
                    &whole,
                    block.clone(),
                )?;
            }
            self.db(
                "feeCache",
                "put",
                vec![s(dust_key), obj(&[("dustSats", s(fee_bn_text(&frac)))])],
            )?;
        }
        let (value, stash) = self.source_fee_load_row(&cache)?;
        if is_spot && text(&property)? != "1" {
            self.source_fee_save_row(&cache, add(&value, &fee_from_sats(&sats)?)?, stash, s("1"))?;
            return Ok(V::Undefined);
        }
        let insurance = fee_down(&div(&sats, &FeeDecimal::from(2u32))?, 0)?;
        let stash_sats = sub(&sats, &insurance)?;
        if is_spot {
            let result = (|| {
                let key = self.source_insurance_instance(s("1"), false)?;
                if insurance > FeeDecimal::zero() {
                    self.source_insurance_deposit(
                        &key,
                        s("1"),
                        s(fee_fixed8(&fee_from_sats(&insurance)?)?),
                        block,
                    )?;
                }
                if stash_sats > FeeDecimal::zero() {
                    self.source_fee_save_row(
                        &cache,
                        value,
                        add(&stash, &fee_from_sats(&stash_sats)?)?,
                        s("1"),
                    )?;
                }
                Ok(())
            })();
            match result {
                Err(Fault::Thrown(_)) | Ok(()) => return Ok(V::Undefined),
                Err(e) => return Err(e),
            }
        }
        // This captured CommonJS export is genuinely missing in the main
        // subgraph. The synchronous TypeError precedes the source .catch.
        if self.entry != "tally-first" {
            return Err(type_error(
                "ContractRegistry.isNativeContract is not a function",
            ));
        }
        let native = match self.call("Contract.isNativeContract", vec![contract.clone()]) {
            Ok(v) => truth(&v),
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        if native {
            self.source_fee_save_row(
                &cache,
                add(&value, &fee_from_sats(&sats)?)?,
                stash,
                contract,
            )?;
        } else {
            let deposit =
                self.source_fee_deposit(contract.clone(), true, property, &insurance, false, block);
            match deposit {
                Err(Fault::Thrown(_)) | Ok(()) => {}
                Err(e) => return Err(e),
            }
            self.source_fee_save_row(
                &cache,
                value,
                add(&stash, &fee_from_sats(&stash_sats)?)?,
                contract,
            )?;
        }
        Ok(V::Undefined)
    }
}

impl ContractCore {
    fn source_insurance_apply_loaded(&mut self, instance: &mut V, row: &V) -> R<()> {
        if truth(row) && truth(&row.get("value")) {
            let value = row.get("value");
            let balances = value.get("balances");
            let hedge = value.get("hedgeRatio");
            set(
                instance,
                "balances",
                if truth(&balances) {
                    balances
                } else {
                    V::Array(vec![])
                },
            )?;
            set(
                instance,
                "hedgeRatio",
                if truth(&hedge) { hedge } else { n(0.5) },
            )?;
        }
        Ok(())
    }
    fn source_insurance_credit(
        &mut self,
        instance: &mut V,
        property: V,
        amount: V,
        block: V,
    ) -> R<Option<(V, V)>> {
        let prop = if matches!(property, V::Null | V::Undefined) {
            "unknown".into()
        } else {
            text(&property)?
        };
        let amount = InsuranceBn::new(&if truth(&amount) { amount } else { n(0.0) })?;
        let Some(amount) = amount.finite() else {
            return Ok(None);
        };
        if amount <= FeeDecimal::zero() {
            return Ok(None);
        }
        let mut balances = match instance.get("balances") {
            V::Array(a) => a,
            _ => return Err(type_error("this.balances.find is not a function")),
        };
        let matching = if self.activation_lifecycle_enabled() {
            // Original find evaluates in order and stops at the first match.
            // A null receiver or failed String conversion rejects the deposit;
            // rows after an earlier match are never inspected.
            let mut matching = None;
            for (index, balance) in balances.iter().enumerate() {
                let property = funding_prop(balance, "propertyId")?;
                if source_channel_string(&property)? == prop {
                    matching = Some(index);
                    break;
                }
            }
            matching
        } else {
            balances
                .iter()
                .position(|balance| text(&balance.get("propertyId")).is_ok_and(|p| p == prop))
        };
        if let Some(i) = matching {
            let before = InsuranceBn::new(&if truth(&balances[i].get("amountAvailable")) {
                balances[i].get("amountAvailable")
            } else {
                n(0.0)
            })?;
            let after = before
                .plus(&InsuranceBn::Finite(amount.clone()))?
                .quantize(8, crate::decimal::Rounding::Down)?;
            set(&mut balances[i], "amountAvailable", s(after.fixed(None)?))?;
        } else {
            balances.push(obj(&[
                ("propertyId", s(&prop)),
                ("amountAvailable", s(fee_down(&amount, 8)?.to_string())),
            ]));
        }
        set(instance, "balances", V::Array(balances.clone()))?;

        let contract = text(&instance.get("contractSeriesId"))?;
        let event = obj(&[
            ("type", s("deposit")),
            (
                "data",
                obj(&[
                    ("contractId", s(&contract)),
                    ("propertyId", s(prop)),
                    ("amount", s(fee_down(&amount, 8)?.to_string())),
                ]),
            ),
            ("timestamp", block.clone()),
        ]);
        let snapshot = obj(&[
            ("balances", V::Array(balances)),
            ("contractSeriesId", s(contract)),
            ("hedgeRatio", instance.get("hedgeRatio")),
            ("block", block),
        ]);
        Ok(Some((event, snapshot)))
    }
}
