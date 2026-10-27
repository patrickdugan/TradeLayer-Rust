// Included in source_contract_runtime.rs. Literal pinned option expiry and
// exercise mutate the sole Margin object arena and call actual Tally custody.
fn expiry_prop(value: &V, key: &str) -> R<V> {
    if matches!(value, V::Null | V::Undefined) {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{key}')",
            text(value)?
        )));
    }
    Ok(value.get(key))
}
fn expiry_or_zero(value: V) -> V {
    if truth(&value) {
        value
    } else {
        n(0.0)
    }
}
fn expiry_fields(value: &V) -> R<Vec<(String, V)>> {
    match value {
        V::Object(fields) => Ok(fields.clone()),
        V::Array(values) => Ok(values
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect()),
        V::String(value) if value.is_ascii() => Ok(value
            .chars()
            .enumerate()
            .map(|(i, c)| (i.to_string(), s(c.to_string())))
            .collect()),
        V::String(_) | V::BigNumber(_) => Err(SourceRuleError::UnsupportedNumericDomain.into()),
        V::Null | V::Undefined => Err(type_error("Cannot convert undefined or null to object")),
        _ => Ok(vec![]),
    }
}
fn expiry_delete_option(position: &mut V, ticker: &str) -> R<()> {
    let mut options = expiry_prop(position, "options")?;
    let V::Object(fields) = &mut options else {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    };
    fields.retain(|(key, _)| key != ticker);
    set(position, "options", options)
}
fn expiry_resolve_target(series: &V, info: &V, option_expiry: &V) -> R<V> {
    let sid = n(number(series)?);
    if !truth(info) {
        return Ok(sid);
    }
    let contracts = expiry_prop(info, "contracts")?;
    if !truth(&contracts) {
        return Ok(sid);
    }
    let V::Array(unexpired) = expiry_prop(&contracts, "unexpired")? else {
        return Ok(sid);
    };
    let expiry = number(&expiry_or_zero(option_expiry.clone()))?;
    if !expiry.is_finite() || expiry <= 0.0 {
        return Ok(sid);
    }
    let mut candidates = vec![];
    for contract in unexpired {
        let id = if truth(&contract) {
            let id = expiry_prop(&contract, "id")?;
            if matches!(id, V::Null | V::Undefined) {
                sid.clone()
            } else {
                id
            }
        } else {
            sid.clone()
        };
        // `c && c.expirationBlock` preserves false/zero/null before Number.
        let expiration = number(&if truth(&contract) {
            expiry_prop(&contract, "expirationBlock")?
        } else {
            contract
        })?;
        if expiration.is_finite() && expiration >= expiry {
            candidates.push((expiration, id));
        }
    }
    candidates.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    Ok(candidates.first().map_or(sid, |(_, id)| id.clone()))
}
fn expiry_exercise_contracts(meta: &V, quantity: &V) -> R<V> {
    let signed = FundingBn::new(&expiry_or_zero(quantity.clone()))?;
    let absolute = signed.clone().abs();
    if absolute.zero() {
        return Ok(n(0.0));
    }
    let positive = signed.compare(&FundingBn::new(&n(0.0))?) == Some(core::cmp::Ordering::Greater);
    let call = same(&expiry_prop(meta, "type")?, &s("Call"));
    Ok(n(if call == positive {
        absolute.number()
    } else {
        absolute.negated().number()
    }))
}
fn expiry_apply_assigned(position: &mut V, delta: &V, price: &V, transfer: &V) -> R<()> {
    let delta = FundingBn::new(&expiry_or_zero(delta.clone()))?;
    if delta.zero() {
        return Ok(());
    }
    let price = FundingBn::new(&expiry_or_zero(price.clone()))?;
    let margin = FundingBn::new(&expiry_or_zero(transfer.clone()))?;
    let before = FundingBn::new(&expiry_or_zero(expiry_prop(position, "contracts")?))?;
    let after = before.clone().op(delta.clone(), 0)?;
    let zero = FundingBn::new(&n(0.0))?;
    let before_sign = before.compare(&zero);
    let after_sign = after.compare(&zero);
    let delta_sign = delta.compare(&zero);
    let flip = !before.zero()
        && !delta.zero()
        && before_sign != delta_sign
        && delta.clone().abs().compare(&before.clone().abs()) == Some(core::cmp::Ordering::Greater);
    let average = if after.zero() {
        n(0.0)
    } else if !flip && (before.zero() || before_sign == after_sign) {
        let old = before.op(
            FundingBn::new(&expiry_or_zero(expiry_prop(position, "avgPrice")?))?,
            2,
        )?;
        let new = delta.op(price.clone(), 2)?;
        n(old.op(new, 0)?.op(after.clone(), 3)?.number())
    } else {
        n(price.number())
    };
    set(position, "avgPrice", average)?;
    set(position, "contracts", n(after.number()))?;
    let new_margin =
        FundingBn::new(&expiry_or_zero(expiry_prop(position, "margin")?))?.op(margin, 0)?;
    set(position, "margin", n(new_margin.number()))?;
    let mark = number(&expiry_prop(position, "lastMark")?)?;
    set(
        position,
        "lastMark",
        n(if mark.is_finite() {
            mark
        } else {
            price.number()
        }),
    )
}
fn expiry_intrinsic(kind: crate::options::OptionType, strike: f64, spot: f64) -> R<FundingBn> {
    let strike = FundingBn::new(&expiry_or_zero(n(strike)))?;
    let spot = FundingBn::new(&expiry_or_zero(n(spot)))?;
    let difference = if kind == crate::options::OptionType::Call {
        spot.op(strike, 1)?
    } else {
        strike.op(spot, 1)?
    };
    Ok(match difference.compare(&FundingBn::new(&n(0.0))?) {
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal) => FundingBn::new(&n(0.0))?,
        _ => difference,
    })
}
impl ContractCore {
    fn source_clearing_expiry_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "resolveOptionExerciseTarget" => {
                expiry_resolve_target(&arg(a, 0), &arg(a, 1), &arg(a, 2))
            }
            "computeExerciseContracts" => expiry_exercise_contracts(&arg(a, 0), &arg(a, 1)),
            "applyAssignedUnderlyingPosition" => {
                let handle = arg(a, 0).get("sourcePositionObject");
                let raw = number(&handle)?;
                if !matches!(handle, V::Number(_))
                    || !raw.is_finite()
                    || raw < 0.0
                    || raw.fract() != 0.0
                    || raw > usize::MAX as f64
                {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                let object = raw as usize;
                let mut position = self.margins.inspect_position_object(object)?;
                let result =
                    expiry_apply_assigned(&mut position, &arg(a, 1), &arg(a, 2), &arg(a, 3));
                self.margins.replace_position_object(object, position)?;
                result?;
                Ok(V::Undefined)
            }
            "settleOptionExpiries" => self.source_settle_option_expiries(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_settle_option_expiries(&mut self, a: &[V]) -> R<V> {
        let series = arg(a, 0);
        let block = arg(a, 1);
        let spot = arg(a, 2);
        let txid = arg(a, 4);
        let margin = self.call("Margin.getInstance", vec![series.clone()])?;
        let info = self.call("Contract.getContractInfo", vec![series.clone()])?;
        if !truth(&info) {
            return Ok(V::Undefined);
        }
        let collateral = expiry_prop(&info, "collateralPropertyId")?;
        if matches!(margin, V::Null | V::Undefined) {
            expiry_prop(&margin, "getExpiringTickersUpTo")?;
        }
        let expiring = self.call(
            "Margin.getExpiringTickersUpTo",
            vec![margin.clone(), block.clone()],
        )?;
        let mut tickers = if let V::Array(values) = expiring {
            values
        } else {
            vec![]
        };
        let instance = number(&margin.get("sourceMarginInstance"))? as u64;
        let entries = self.margins.position_object_entries(instance)?;
        if tickers.is_empty() {
            for (_, object) in &entries {
                let position = self.margins.inspect_position_object(*object)?;
                if !truth(&position) {
                    continue;
                }
                let options = expiry_prop(&position, "options")?;
                if !truth(&options) {
                    continue;
                }
                for (ticker, _) in expiry_fields(&options)? {
                    let Some(meta) = crate::options::parse_ticker(&ticker) else {
                        continue;
                    };
                    let expiry = number(&expiry_or_zero(n(meta.expiry_block)))?;
                    if expiry <= number(&block)? && !tickers.contains(&s(&ticker)) {
                        tickers.push(s(ticker));
                    }
                }
            }
        }
        if tickers.is_empty() {
            return Ok(V::Undefined);
        }
        for (address, object) in entries {
            let mut position = self.margins.inspect_position_object(object)?;
            if !truth(&position) || !truth(&expiry_prop(&position, "options")?) {
                continue;
            }
            for ticker in &tickers {
                let key = text(ticker)?;
                let option = expiry_prop(&expiry_prop(&position, "options")?, &key)?;
                if !truth(&option) {
                    continue;
                }
                let quantity = FundingBn::new(&expiry_or_zero(expiry_prop(&option, "contracts")?))?;
                if quantity.zero() {
                    expiry_delete_option(&mut position, &key)?;
                    self.margins
                        .replace_position_object(object, position.clone())?;
                    continue;
                }
                let Some(meta) = crate::options::parse_ticker(&key) else {
                    continue;
                };
                let strike = number(&expiry_or_zero(meta.strike.map_or(V::Null, n)))?;
                let intrinsic = expiry_intrinsic(
                    meta.option_type,
                    strike,
                    number(&expiry_or_zero(spot.clone()))?,
                )?;
                let held = FundingBn::new(&expiry_or_zero(expiry_prop(&option, "margin")?))?;
                let itm = intrinsic.compare(&FundingBn::new(&n(0.0))?)
                    == Some(core::cmp::Ordering::Greater);
                if itm {
                    let meta_value = obj(&[("type", s(meta.option_type.as_source_name()))]);
                    let exercise = expiry_exercise_contracts(&meta_value, &quantity.value())?;
                    let target = expiry_resolve_target(&series, &info, &n(meta.expiry_block))?;
                    let raw_strike = meta.strike.map_or(V::Null, n);
                    let entry = if truth(&raw_strike) {
                        raw_strike
                    } else if truth(&spot) {
                        spot.clone()
                    } else {
                        n(0.0)
                    };
                    let applied = expiry_apply_assigned(
                        &mut position,
                        &exercise,
                        &n(number(&entry)?),
                        &held.value(),
                    );
                    self.margins
                        .replace_position_object(object, position.clone())?;
                    applied?;
                    self.call(
                        "Margin.recordMarginMapDelta",
                        vec![
                            margin.clone(),
                            address.clone(),
                            s(text(&target)?),
                            n(number(&expiry_or_zero(expiry_prop(
                                &position,
                                "contracts",
                            )?))?),
                            exercise,
                            n(number(&entry)?),
                            n(0.0),
                            n(held.number()),
                            s("optionExpireAssign"),
                            block.clone(),
                        ],
                    )?;
                } else {
                    // Clearing's top-level TallyMap captures the in-progress
                    // CommonJS export when Tally is the entry module. Unlike
                    // funding's local require, this reference stays partial.
                    if self.entry == "tally-first" {
                        return Err(type_error("TallyMap.updateBalance is not a function"));
                    }
                    let delta = if held.zero() {
                        n(0.0)
                    } else {
                        n(held.clone().negated().number())
                    };
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            address.clone(),
                            collateral.clone(),
                            n(0.0),
                            n(0.0),
                            delta,
                            n(0.0),
                            s("optionExpire"),
                            block.clone(),
                            txid.clone(),
                        ],
                    )?;
                }
                expiry_delete_option(&mut position, &key)?;
                self.margins
                    .replace_position_object(object, position.clone())?;
                self.call(
                    "Margin.recordMarginMapDelta",
                    vec![
                        margin.clone(),
                        address.clone(),
                        ticker.clone(),
                        n(0.0),
                        n(quantity.negated().number()),
                        n(intrinsic.number()),
                        n(0.0),
                        if held.zero() {
                            n(0.0)
                        } else {
                            n(held.negated().number())
                        },
                        s(if itm {
                            "optionExpireExercised"
                        } else {
                            "optionExpire"
                        }),
                        block.clone(),
                    ],
                )?;
            }
        }
        self.call(
            "Margin.cleanupExpiredTickersUpTo",
            vec![margin.clone(), block.clone()],
        )?;
        self.call("Margin.saveMarginMap", vec![margin, block])?;
        Ok(V::Undefined)
    }
}
