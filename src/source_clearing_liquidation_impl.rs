// Included in source_contract_runtime.rs. Literal pinned liquidation orchestration.
// The declared storage adapter uses structuredClone, which strips the genuine
// BigNumber prototype while preserving its enumerable base-1e14 coefficients.
fn source_storage_clone(value: &V) -> R<V> {
    Ok(match value {
        V::BigNumber(text) => {
            let negative = text.starts_with('-');
            if matches!(
                text.as_str(),
                "NaN" | "Infinity" | "-Infinity" | "+Infinity"
            ) {
                obj(&[
                    (
                        "s",
                        if text == "NaN" {
                            V::Null
                        } else {
                            n(if negative { -1.0 } else { 1.0 })
                        },
                    ),
                    ("e", V::Null),
                    ("c", V::Null),
                ])
            } else {
                let value = FundingDecimal::from_str(text)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let plain = value.abs().to_string();
                let (integer, fraction) = plain.split_once('.').unwrap_or((&plain, ""));
                if value.is_zero() {
                    obj(&[
                        ("s", n(if negative { -1.0 } else { 1.0 })),
                        ("e", n(0.0)),
                        ("c", V::Array(vec![n(0.0)])),
                    ])
                } else {
                    let (digits, exponent) = if integer != "0" {
                        (format!("{integer}{fraction}"), integer.len() as i64 - 1)
                    } else {
                        let leading = fraction.bytes().take_while(|b| *b == b'0').count();
                        (fraction[leading..].to_string(), -(leading as i64) - 1)
                    };
                    let first = ((exponent + 1).rem_euclid(14)) as usize;
                    let first = if first == 0 { 14 } else { first };
                    let mut coefficients = vec![];
                    let mut at = 0;
                    let mut size = first;
                    while at < digits.len() {
                        let end = (at + size).min(digits.len());
                        let mut coefficient = digits[at..end].to_string();
                        coefficient.extend(std::iter::repeat_n('0', size - (end - at)));
                        let value = coefficient
                            .parse::<u64>()
                            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                        coefficients.push(n(value as f64));
                        at = end;
                        size = 14;
                    }
                    while coefficients.len() > 1 && coefficients.last() == Some(&n(0.0)) {
                        coefficients.pop();
                    }
                    obj(&[
                        ("s", n(if negative { -1.0 } else { 1.0 })),
                        ("e", n(exponent as f64)),
                        ("c", V::Array(coefficients)),
                    ])
                }
            }
        }
        V::Array(a) => V::Array(a.iter().map(source_storage_clone).collect::<R<Vec<_>>>()?),
        V::Object(fields) => V::Object(
            fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), source_storage_clone(v)?)))
                .collect::<R<Vec<_>>>()?,
        ),
        V::Promise => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
        other => other.clone(),
    })
}
fn source_liq_bn_min(a: FundingBn, b: FundingBn) -> FundingBn {
    match a.compare(&b) {
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal) => a,
        Some(_) => b,
        None => FundingBn::Special(f64::NAN),
    }
}
fn source_liq_gt(a: &FundingBn, b: f64) -> R<bool> {
    Ok(a.compare(&FundingBn::new(&n(b))?) == Some(core::cmp::Ordering::Greater))
}
fn source_liq_lte(a: &FundingBn, b: &FundingBn) -> bool {
    a.compare(b)
        .is_some_and(|v| v != core::cmp::Ordering::Greater)
}
fn source_liq_spread(value: &V) -> R<V> {
    match value {
        V::Object(fields) => Ok(V::Object(fields.clone())),
        V::Undefined | V::Null | V::Number(_) | V::Bool(_) => Ok(obj(&[])),
        V::Array(a) => Ok(V::Object(
            a.iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
        )),
        V::String(t) => {
            let mut fields = vec![];
            for (i, c) in t.encode_utf16().enumerate() {
                let c = char::from_u32(u32::from(c))
                    .ok_or(SourceRuleError::UnsupportedNumericDomain)?;
                fields.push((i.to_string(), s(c.to_string())))
            }
            Ok(V::Object(fields))
        }
        _ => Err(SourceRuleError::UnsupportedNumericDomain.into()),
    }
}
fn source_liq_bankruptcy(a: &[V]) -> R<V> {
    let last = FundingBn::new(&arg(a, 0))?;
    let equity = FundingBn::new(&arg(a, 1))?;
    let contracts = FundingBn::new(&arg(a, 2))?;
    let notional = FundingBn::new(&arg(a, 3))?;
    if contracts.zero() || source_liq_lte(&equity, &FundingBn::new(&n(0.0))?) {
        return Ok(last.q(30, FundingRounding::HalfUp)?.value());
    }
    let long = source_liq_gt(&contracts, 0.0)?;
    let dent = equity.op(contracts.abs().op(notional, 2)?, 3)?;
    let result = if !truth(&arg(a, 4)) {
        last.op(dent, if long { 1 } else { 0 })?
    } else {
        let inverse = FundingBn::new(&n(1.0))?
            .op(last, 3)?
            .op(dent, if long { 0 } else { 1 })?;
        FundingBn::new(&n(1.0))?.op(inverse, 3)?
    };
    Ok(result.q(30, FundingRounding::HalfUp)?.value())
}
fn source_liq_amm_sweep<F: FnMut(&V) -> R<()>>(book: &mut V, a: &[V], mut changed: F) -> R<V> {
    let liq = arg(a, 0);
    let sell = truth(&funding_prop(&liq, "sell")?);
    let side_name = if sell { "buy" } else { "sell" };
    let mut side = match book.get(side_name) {
        V::Array(a) => a,
        _ => vec![],
    };
    let cap = FundingBn::new(&funding_or(arg(a, 1)))?;
    let boundary = FundingBn::new(&funding_or(arg(a, 2)))?;
    let inverse = truth(&arg(a, 3));
    let mut result = obj(&[("filledSize", n(0.0)), ("matches", V::Array(vec![]))]);
    if source_liq_lte(&cap, &FundingBn::new(&n(0.0))?) || side.is_empty() {
        return Ok(result);
    }
    let mut levels = vec![];
    for (i, o) in side.iter().enumerate() {
        if truth(o)
            && same(&o.get("sender"), &s("amm"))
            && number(&funding_or(o.get("amount")))? > 0.0
        {
            let price = FundingBn::new(&funding_or(o.get("price")))?;
            let time = number(&funding_or(o.get("blockTime")))?;
            levels.push((i, price, time));
        }
    }
    levels.sort_by(|a, b| {
        let order = if sell {
            b.1.compare(&a.1)
        } else {
            a.1.compare(&b.1)
        };
        match order {
            Some(core::cmp::Ordering::Equal) => (a.2 - b.2)
                .partial_cmp(&0.0)
                .unwrap_or(core::cmp::Ordering::Equal),
            Some(o) => o,
            None => core::cmp::Ordering::Equal,
        }
    });
    let mut remaining = cap;
    let mut filled = FundingBn::new(&n(0.0))?;
    let mut matches = vec![];
    for (i, _, _) in levels {
        if source_liq_lte(&remaining, &FundingBn::new(&n(0.0))?) {
            break;
        }
        let level = side[i].clone();
        let price = FundingBn::new(&funding_or(level.get("price")))?;
        let available = FundingBn::new(&funding_or(level.get("amount")))?;
        if !price.finite()
            || !available.finite()
            || source_liq_lte(&available, &FundingBn::new(&n(0.0))?)
        {
            continue;
        }
        let good = if sell ^ inverse {
            source_liq_lte(&boundary, &price)
        } else {
            source_liq_lte(&price, &boundary)
        };
        if !good {
            continue;
        }
        let take = source_liq_bn_min(remaining.clone(), available);
        if source_liq_lte(&take, &FundingBn::new(&n(0.0))?) {
            continue;
        }
        let mut liq_leg = source_liq_spread(&liq)?;
        for (k, v) in [
            ("amount", n(take.number())),
            ("contractId", liq.get("contractId")),
            ("sender", liq.get("address")),
            (
                "txid",
                if truth(&liq.get("txid")) {
                    liq.get("txid")
                } else {
                    s("liq")
                },
            ),
            ("maker", V::Bool(false)),
            ("liq", V::Bool(true)),
        ] {
            set(&mut liq_leg, k, v)?
        }
        let mut amm_leg = source_liq_spread(&level)?;
        for (k, v) in [
            ("amount", n(take.number())),
            ("contractId", liq.get("contractId")),
            (
                "sender",
                if truth(&level.get("sender")) {
                    level.get("sender")
                } else {
                    s("amm")
                },
            ),
            (
                "txid",
                if truth(&level.get("txid")) {
                    level.get("txid")
                } else {
                    s("amm")
                },
            ),
            ("maker", V::Bool(true)),
            ("liq", V::Bool(truth(&level.get("isLiq")))),
        ] {
            set(&mut amm_leg, k, v)?
        }
        let tx = amm_leg.get("txid");
        let (mut sell_leg, mut buy_leg) = if sell {
            (liq_leg, amm_leg)
        } else {
            (amm_leg, liq_leg)
        };
        let seller = if sell {
            liq.get("address")
        } else {
            sell_leg.get("sender")
        };
        let buyer = if sell {
            buy_leg.get("sender")
        } else {
            liq.get("address")
        };
        set(&mut sell_leg, "sellerAddress", seller)?;
        set(&mut buy_leg, "buyerAddress", buyer)?;
        matches.push(obj(&[
            ("sellOrder", sell_leg),
            ("buyOrder", buy_leg),
            ("tradePrice", n(price.number())),
            ("txid", tx),
        ]));
        filled = filled.op(take.clone(), 0)?;
        remaining = remaining.op(take.clone(), 1)?;
        set(
            &mut side[i],
            "amount",
            n(FundingBn::new(&funding_or(level.get("amount")))?
                .op(take, 1)?
                .number()),
        )?;
        set(book, side_name, V::Array(side.clone()))?;
        changed(book)?;
    }
    set(&mut result, "filledSize", n(filled.number()))?;
    set(&mut result, "matches", V::Array(matches))?;
    let mut retained = vec![];
    for level in side {
        if number(&funding_or(funding_prop(&level, "amount")?))? > 0.0 {
            retained.push(level)
        }
    }
    set(book, side_name, V::Array(retained))?;
    changed(book)?;
    Ok(result)
}
/// Exact pure source AMM sweep boundary; book mutations survive source throws.
pub fn sweep_amm_liquidity_first_source(
    book: &mut V,
    arguments: &[V],
) -> Result<SourceOutcome, SourceRuleError> {
    if arguments.len() > 4 {
        return Err(SourceRuleError::Capacity);
    };
    book.validate()?;
    for value in arguments {
        value.validate()?
    }
    let before = book.clone();
    match source_liq_amm_sweep(book, arguments, |_| Ok(())) {
        Ok(v) => Ok(SourceOutcome::Returned(v)),
        Err(Fault::Thrown(e)) => Ok(SourceOutcome::Thrown(e)),
        Err(Fault::Local(e)) => {
            *book = before;
            Err(e)
        }
    }
}
impl ContractCore {
    fn source_liquidation_prune(&mut self, a: &[V]) -> R<V> {
        let price = arg(a, 1);
        let block = arg(a, 2);
        let contract = arg(a, 3);
        let notional = arg(a, 4);
        let inverse = truth(&arg(a, 5));
        let key = s(text(&contract)?);
        let collateral = self.call("Contract.getCollateralId", vec![contract.clone()])?;
        let receiver = self.call("Orderbook.newInstance", vec![key.clone()])?;
        let mut book = self.call(
            "Orderbook.loadOrderBook",
            vec![receiver.clone(), key.clone()],
        )?;
        if !truth(&book)
            || (!matches!(book.get("buy"), V::Array(_)) && !matches!(book.get("sell"), V::Array(_)))
        {
            return Ok(n(0.0));
        }
        let mut pruned = 0.0;
        for side_name in ["buy", "sell"] {
            let mut side = match book.get(side_name) {
                V::Array(a) => a,
                _ => vec![],
            };
            for i in (0..side.len()).rev() {
                let order = side[i].clone();
                if !truth(&order) {
                    continue;
                }
                let sender = funding_prop(&order, "sender")?;
                let address = if truth(&sender) {
                    sender
                } else {
                    funding_prop(&order, "address")?
                };
                if !truth(&address) {
                    continue;
                }
                let virtual_amm = matches!(&address,V::String(t)if t=="amm"||t.starts_with("amm:"));
                let qty = number(&funding_or(funding_prop(&order, "amount")?))?.abs();
                if !qty.is_finite() || qty <= 0.0 {
                    continue;
                }
                let tally =
                    self.call("Tally.getTally", vec![address.clone(), collateral.clone()])?;
                if !truth(&tally) {
                    continue;
                }
                let available = number(&funding_or(funding_prop(&tally, "available")?))?;
                let margin = number(&funding_or(funding_prop(&tally, "margin")?))?;
                let total = available + margin;
                let mut reserve = number(&funding_or(funding_prop(&order, "initMargin")?))?;
                if !total.is_finite() || total <= 0.0 {
                    side.remove(i);
                    pruned += 1.0;
                    set(&mut book, side_name, V::Array(side.clone()))?;
                    self.call(
                        "Orderbook.setCachedBook",
                        vec![receiver.clone(), key.clone(), book.clone()],
                    )?;
                    if reserve > 0.0 && !virtual_amm {
                        self.call(
                            "Tally.updateBalance",
                            vec![
                                address,
                                collateral.clone(),
                                n(reserve),
                                n(-reserve),
                                n(0.0),
                                n(0.0),
                                s("contractCancel"),
                                block.clone(),
                            ],
                        )?;
                    }
                    continue;
                }
                let order_price = number(&funding_prop(&order, "price")?)?;
                if !order_price.is_finite() || order_price <= 0.0 {
                    continue;
                }
                let current = number(&price)?;
                let delta = if !inverse {
                    if side_name == "sell" {
                        current - order_price
                    } else {
                        order_price - current
                    }
                } else if side_name == "sell" {
                    1.0 / order_price - 1.0 / current
                } else {
                    1.0 / current - 1.0 / order_price
                };
                let worst = qty
                    * number(&notional)?
                    * if delta.is_nan() {
                        f64::NAN
                    } else {
                        delta.max(0.0)
                    };
                if worst > total {
                    side.remove(i);
                    pruned += 1.0;
                    set(&mut book, side_name, V::Array(side.clone()))?;
                    self.call(
                        "Orderbook.setCachedBook",
                        vec![receiver.clone(), key.clone(), book.clone()],
                    )?;
                    if !reserve.is_finite() || reserve <= 0.0 {
                        let initial = self.call(
                            "Contract.getInitialMargin",
                            vec![contract.clone(), n(order_price)],
                        )?;
                        reserve = number(&funding_or(initial))? * qty;
                    }
                    if reserve > 0.0 && !virtual_amm {
                        self.call(
                            "Tally.updateBalance",
                            vec![
                                address,
                                collateral.clone(),
                                n(reserve),
                                n(-reserve),
                                n(0.0),
                                n(0.0),
                                s("contractCancel"),
                                block.clone(),
                            ],
                        )?;
                    }
                }
            }
        }
        if pruned > 0.0 {
            self.call(
                "Orderbook.setCachedBook",
                vec![receiver.clone(), key.clone(), book.clone()],
            )?;
            self.call("Orderbook.saveOrderBook", vec![receiver, book, key])?;
        }
        Ok(n(pruned))
    }
    fn source_liquidation_estimate(&mut self, a: &[V]) -> R<V> {
        let receiver = arg(a, 0);
        let liq = arg(a, 1);
        let inverse = truth(&arg(a, 5));
        let key = s(text(&arg(a, 6))?);
        let amount = number(&funding_or(funding_prop(&liq, "amount")?))?.abs();
        let mut result = obj(&[
            ("filled", V::Bool(false)),
            ("filledSize", n(0.0)),
            ("goodFilledSize", n(0.0)),
            ("badFilledSize", n(0.0)),
            ("remainder", n(amount)),
            ("avgFillPrice", V::Null),
            ("fills", V::Array(vec![])),
        ]);
        let mut book = self.call("Orderbook.inspectBook", vec![receiver.clone(), key.clone()])?;
        if !truth(&book) {
            book = self.call(
                "Orderbook.loadOrderBook",
                vec![receiver.clone(), key.clone()],
            )?;
            self.call("Orderbook.setCachedBook", vec![receiver, key, book.clone()])?;
        }
        let sell = truth(&funding_prop(&liq, "sell")?);
        let side = funding_prop(&book, if sell { "buy" } else { "sell" })?;
        if !truth(&side) || number(&side.get("length"))? == 0.0 {
            return Ok(result);
        }
        let levels = match side {
            V::Array(a) => a,
            _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
        };
        let mut remaining = amount;
        let mut weighted = 0.0;
        let mut filled = 0.0;
        let mut fills = vec![];
        for level in levels {
            if remaining <= 0.0 {
                break;
            }
            let owner = funding_prop(&liq, "address")?;
            if truth(&owner) && same(&funding_prop(&level, "sender")?, &owner) {
                continue;
            }
            let price = number(&funding_prop(&level, "price")?)?;
            let size = number(&funding_prop(&level, "amount")?)?;
            if !price.is_finite() || !size.is_finite() || size <= 0.0 {
                continue;
            }
            let boundary = number(&arg(a, 4))?;
            let good = if sell ^ inverse {
                price >= boundary
            } else {
                price <= boundary
            };
            if !good {
                break;
            }
            let take = if remaining.is_nan() || size.is_nan() {
                f64::NAN
            } else {
                remaining.min(size)
            };
            filled += take;
            weighted += take * price;
            fills.push(obj(&[
                ("price", n(price)),
                ("size", n(take)),
                ("good", V::Bool(true)),
            ]));
            remaining -= take;
        }
        set(&mut result, "filledSize", n(filled))?;
        set(&mut result, "goodFilledSize", n(filled))?;
        set(&mut result, "remainder", n(amount - filled))?;
        set(&mut result, "fills", V::Array(fills))?;
        if filled > 0.0 {
            set(&mut result, "filled", V::Bool(true))?;
            set(&mut result, "avgFillPrice", n(weighted / filled))?
        }
        Ok(result)
    }
    fn source_liquidation_cache_values(&self, key: &str) -> R<(Vec<usize>, V)> {
        let ids = self.source_clearing_context_ids(key)?;
        let values = ids
            .iter()
            .map(|id| {
                self.margins
                    .inspect_position_object(*id)
                    .map_err(Fault::Local)
            })
            .collect::<R<Vec<_>>>()?;
        Ok((ids, V::Array(values)))
    }
    fn source_liquidation_handle(&mut self, a: &[V]) -> R<V> {
        if arg(a, 2) != obj(&[("sourceTally", V::Bool(true))]) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let ctx = text(&arg(a, 0))?;
        let receiver = arg(a, 1);
        let position = arg(a, 3);
        let contract = arg(a, 4);
        let block = arg(a, 5);
        let inverse = truth(&arg(a, 6));
        let collateral = arg(a, 7);
        let notional = arg(a, 10);
        let mark = arg(a, 11);
        let info = arg(a, 15);
        let handle = self.call("Margin.getInstance", vec![contract.clone()])?;
        let address = funding_prop(&position, "address")?;
        self.source_liquidation_cache_values(&ctx)?;
        let tally = if truth(&arg(a, 14)) {
            arg(a, 14)
        } else {
            self.call("Tally.getTally", vec![address.clone(), collateral.clone()])?
        };
        let maintenance = self.call(
            "Margin.computeMaintenanceMarginRequirement",
            vec![handle.clone(), contract.clone(), position.clone()],
        )?;
        let maintenance = FundingBn::new(&funding_or(maintenance))?;
        let margin = FundingBn::new(&funding_or(funding_prop(&tally, "margin")?))?;
        let available = FundingBn::new(&funding_or(funding_prop(&tally, "available")?))?;
        let equity = margin.clone().op(available.clone(), 0)?;
        let deficit = maintenance.op(equity, 1)?;
        let absolute = number(&funding_prop(&position, "contracts")?)?.abs();
        let mut amount = if source_liq_gt(&deficit, 0.0)? && source_liq_lte(&deficit, &margin) {
            let initial = self.call(
                "Contract.getInitialMargin",
                vec![contract.clone(), mark.clone()],
            )?;
            source_liq_bn_min(
                FundingBn::new(&n(absolute))?,
                deficit
                    .op(FundingBn::new(&initial)?, 3)?
                    .q(8, FundingRounding::HalfUp)?,
            )
            .number()
        } else {
            absolute
        };
        if amount <= 0.0 {
            amount = absolute
        }
        let mut budget = FundingBn::new(&nullish(arg(a, 13), n(0.0)))?;
        if source_liq_lte(&budget, &FundingBn::new(&n(0.0))?) {
            budget = margin.op(available, 0)?
        }
        let computed = self.call(
            "Clearing.computeLiquidationPriceFromLoss",
            vec![
                mark.clone(),
                n(budget.number()),
                funding_prop(&position, "contracts")?,
                notional.clone(),
                arg(a, 6),
            ],
        )?;
        let mut liq = self.call(
            "Margin.generateLiquidationOrder",
            vec![
                handle.clone(),
                position.clone(),
                contract.clone(),
                n(amount),
                block.clone(),
                mark.clone(),
                computed.clone(),
            ],
        )?;
        if !truth(&liq) || same(&liq, &s("err:0 contracts")) {
            return Ok(V::Null);
        }
        set(&mut liq, "amount", n(amount))?;
        if !truth(&liq.get("price")) {
            set(&mut liq, "price", computed.clone())?
        }
        if !truth(&liq.get("bankruptcyPrice")) {
            set(&mut liq, "bankruptcyPrice", computed.clone())?
        }
        let bankruptcy = liq.get("bankruptcyPrice");
        let sell = number(&funding_prop(&position, "contracts")?)? > 0.0;
        set(&mut liq, "sell", V::Bool(sell))?;
        let estimate = self.call(
            "Orderbook.estimateLiquidation",
            vec![
                receiver.clone(),
                liq.clone(),
                notional.clone(),
                computed.clone(),
                computed.clone(),
                arg(a, 6),
                contract.clone(),
            ],
        )?;
        let can_fill =
            truth(&estimate) && number(&funding_or(estimate.get("goodFilledSize")))? > 0.0;
        // The source log dereferences splat even after its guarded canObFill test.
        funding_prop(&estimate, "goodFilledSize")?;
        let loss = FundingBn::new(&funding_or(arg(a, 9)))?;
        let mut book_fill = FundingBn::new(&n(0.0))?;
        if can_fill {
            let key = s(text(&contract)?);
            let cached = self.call("Orderbook.inspectBook", vec![receiver.clone(), key.clone()])?;
            let mut book = if truth(&cached) {
                cached
            } else {
                obj(&[("buy", V::Array(vec![])), ("sell", V::Array(vec![]))])
            };
            let safe = number(&funding_or(estimate.get("goodFilledSize")))?;
            let mut remaining = safe;
            let mut liq_book = source_liq_spread(&liq)?;
            set(&mut liq_book, "amount", n(safe))?;
            let mut trades = vec![];
            let sweep = source_liq_amm_sweep(
                &mut book,
                &[liq_book.clone(), n(remaining), computed.clone(), arg(a, 6)],
                |changed| {
                    self.call(
                        "Orderbook.setCachedBook",
                        vec![receiver.clone(), key.clone(), changed.clone()],
                    )?;
                    Ok(())
                },
            )?;
            let sweep_matches = array(&sweep.get("matches"))?;
            if !sweep_matches.is_empty() {
                let result = self.call(
                    "Orderbook.processContractMatches",
                    vec![
                        receiver.clone(),
                        V::Array(sweep_matches),
                        block.clone(),
                        V::Bool(false),
                        mark.clone(),
                    ],
                )?;
                if let V::Array(mut added) = result {
                    trades.append(&mut added)
                }
                remaining -= number(&funding_or(sweep.get("filledSize")))?;
            }
            let mut matched = obj(&[("matches", V::Array(vec![])), ("orderBook", book.clone())]);
            if remaining > 0.0 {
                let mut remainder = source_liq_spread(&liq_book)?;
                set(&mut remainder, "amount", n(remaining))?;
                book = self.call(
                    "Orderbook.insertOrder",
                    vec![
                        receiver.clone(),
                        remainder.clone(),
                        book,
                        remainder.get("sell"),
                        V::Bool(true),
                    ],
                )?;
                matched = self.call(
                    "Orderbook.matchContractOrders",
                    vec![receiver.clone(), book.clone()],
                )?;
                let matches = matched.get("matches");
                if truth(&matches) && number(&matches.get("length"))? > 0.0 {
                    let result = self.call(
                        "Orderbook.processContractMatches",
                        vec![
                            receiver.clone(),
                            matches,
                            block.clone(),
                            V::Bool(false),
                            mark.clone(),
                        ],
                    )?;
                    if let V::Array(mut added) = result {
                        trades.append(&mut added)
                    }
                }
            }
            let saved = matched.get("orderBook");
            self.call(
                "Orderbook.saveOrderBook",
                vec![
                    receiver.clone(),
                    if truth(&saved) { saved } else { book },
                    key,
                ],
            )?;
            let mut filled = 0.0;
            if let V::Array(matches) = matched.get("matches") {
                for trade in matches {
                    let sell = funding_prop(&trade, "sellOrder")?;
                    let buy = funding_prop(&trade, "buyOrder")?;
                    let qty = number(&nullish(
                        sell.get("amount"),
                        nullish(buy.get("amount"), n(0.0)),
                    ))?;
                    if qty > 0.0 {
                        filled += qty
                    }
                }
            }
            book_fill = FundingBn::new(&n((filled
                + number(&funding_or(sweep.get("filledSize")))?)
            .min(amount)))?;
            let mut final_positions: Vec<(V, V)> = vec![];
            for trade in trades {
                for (address_key, position_key) in [
                    ("buyerAddress", "buyerPosition"),
                    ("sellerAddress", "sellerPosition"),
                ] {
                    let addr = funding_prop(&trade, address_key)?;
                    let pos = funding_prop(&trade, position_key)?;
                    if truth(&addr) && truth(&pos) {
                        if let Some((_, old)) =
                            final_positions.iter_mut().find(|(k, _)| same(k, &addr))
                        {
                            *old = pos
                        } else {
                            final_positions.push((addr, pos))
                        }
                    }
                }
            }
            for (addr, pos) in final_positions {
                self.source_clearing_replace_cached(&ctx, addr, pos, true)?;
            }
        }
        let adl = FundingBn::new(&n(amount))?.op(book_fill.clone(), 1)?;
        let remainder = if source_liq_gt(&adl, 0.0)? {
            adl.number()
        } else {
            0.0
        };
        let mut residual = FundingBn::new(&n(0.0))?;
        if remainder > 0.0 {
            let qty = FundingBn::new(&n(remainder))?.q(8, FundingRounding::HalfUp)?;
            let last = funding_prop(&info, "lastPrice")?;
            let last = FundingBn::new(&if truth(&last) { last } else { mark.clone() })?;
            let this = funding_prop(&info, "thisPrice")?;
            let this = FundingBn::new(&if truth(&this) { this } else { mark.clone() })?;
            let not = FundingBn::new(&if truth(&notional) {
                notional.clone()
            } else {
                n(1.0)
            })?;
            if !inverse {
                residual = qty.op(last.op(this, 1)?.abs(), 2)?.op(not, 2)?;
            } else if source_liq_gt(&last, 0.0)? && source_liq_gt(&this, 0.0)? {
                let inv_last = FundingBn::new(&n(1.0))?.op(last, 3)?;
                let inv_this = FundingBn::new(&n(1.0))?.op(this, 3)?;
                residual = qty.op(inv_this.op(inv_last, 1)?.abs(), 2)?.op(not, 2)?;
            }
            residual = residual.q(8, FundingRounding::HalfUp)?;
        }
        // Both properties are read by the source log even when remainder == 0.
        funding_prop(&info, "lastPrice")?;
        funding_prop(&info, "thisPrice")?;
        let fresh = self.call("Tally.getTally", vec![address.clone(), collateral.clone()])?;
        let full = FundingBn::new(&funding_or(funding_prop(&fresh, "margin")?))?
            .op(
                FundingBn::new(&funding_or(funding_prop(&fresh, "available")?))?,
                0,
            )?
            .q(8, FundingRounding::HalfUp)?;
        let seized = source_liq_bn_min(full, residual).q(8, FundingRounding::HalfUp)?;
        let pool = seized.number();
        if pool > 0.0 {
            let avail = FundingBn::new(&funding_or(funding_prop(&fresh, "available")?))?;
            let seize_avail =
                source_liq_bn_min(avail, seized.clone()).q(8, FundingRounding::HalfUp)?;
            let seize_margin = seized
                .clone()
                .op(seize_avail.clone(), 1)?
                .q(8, FundingRounding::HalfUp)?;
            self.call(
                "Tally.updateBalance",
                vec![
                    address.clone(),
                    collateral.clone(),
                    n(-seize_avail.number()),
                    n(0.0),
                    n(-seize_margin.number()),
                    n(0.0),
                    s("liquidationPoolDebit"),
                    block.clone(),
                ],
            )?;
        }
        let systemic =
            if loss.compare(&FundingBn::new(&n(pool))?) == Some(core::cmp::Ordering::Greater) {
                loss.op(FundingBn::new(&n(pool))?, 1)?
                    .q(8, FundingRounding::HalfUp)?
            } else {
                FundingBn::new(&n(0.0))?
            };
        let mut result = obj(&[
            ("counterparties", V::Array(vec![])),
            ("poolAssignments", V::Array(vec![])),
        ]);
        if remainder > 0.0 {
            let (ids, positions) = self.source_liquidation_cache_values(&ctx)?;
            let mut receiver = handle.clone();
            set(
                &mut receiver,
                "sourcePositionObjectAliases",
                V::Array(
                    ids.iter()
                        .enumerate()
                        .map(|(i, id)| {
                            obj(&[
                                ("argument", n(0.0)),
                                ("index", n(i as f64)),
                                ("object", n(*id as f64)),
                            ])
                        })
                        .collect(),
                ),
            )?;
            result = self.call(
                "Margin.simpleDeleverage",
                vec![
                    receiver,
                    positions,
                    contract.clone(),
                    n(remainder),
                    V::Bool(sell),
                    bankruptcy,
                    address.clone(),
                    arg(a, 6),
                    notional,
                    block.clone(),
                    mark.clone(),
                    collateral.clone(),
                    n(pool),
                ],
            )?;
        }
        let mut remaining = FundingBn::new(&n(pool))?;
        let assignments = result.get("poolAssignments");
        for assignment in array(&if truth(&assignments) {
            assignments
        } else {
            V::Array(vec![])
        })? {
            if source_liq_lte(&remaining, &FundingBn::new(&n(0.0))?) {
                break;
            }
            let share = FundingBn::new(&funding_or(funding_prop(&assignment, "poolShare")?))?;
            if source_liq_lte(&share, &FundingBn::new(&n(0.0))?) {
                continue;
            }
            let credit =
                source_liq_bn_min(share, remaining.clone()).q(8, FundingRounding::HalfUp)?;
            self.call(
                "Tally.updateBalance",
                vec![
                    funding_prop(&assignment, "address")?,
                    collateral.clone(),
                    n(credit.number()),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s("deleveragePoolCredit"),
                    block.clone(),
                ],
            )?;
            remaining = remaining.op(credit, 1)?;
        }
        let counterparties = result.get("counterparties");
        let counterparties = if truth(&counterparties) {
            counterparties
        } else {
            V::Array(vec![])
        };
        for cp in array(&counterparties)? {
            let addr = funding_prop(&cp, "address")?;
            self.source_clearing_replace_cached(
                &ctx,
                addr.clone(),
                source_liq_spread(&funding_prop(&cp, "updatedPosition")?)?,
                false,
            )?;
            self.call(
                "Clearing.recordDeleverageTrade",
                vec![contract.clone(), addr, cp],
            )?;
        }
        let (_, cache) = self.source_liquidation_cache_values(&ctx)?;
        let old = array(&cache)?
            .into_iter()
            .find(|p| same(&p.get("address"), &address))
            .ok_or_else(|| {
                error(format!(
                    "Position for {} not found in cache",
                    text(&address).unwrap_or_default()
                ))
            })?;
        let mut zero = source_liq_spread(&old)?;
        for (k, v) in [
            ("contracts", n(0.0)),
            ("margin", n(0.0)),
            ("unrealizedPNL", n(0.0)),
            ("averagePrice", V::Null),
            ("bankruptcyPrice", V::Null),
            ("lastMark", mark),
        ] {
            set(&mut zero, k, v)?
        }
        self.source_clearing_replace_cached(&ctx, address, zero, false)?;
        Ok(obj(&[
            ("liquidation", liq),
            ("systemicLoss", n(systemic.number())),
            ("counterparties", counterparties),
            (
                "totalDeleveraged",
                n(book_fill
                    .op(FundingBn::new(&n(remainder))?, 0)?
                    .q(8, FundingRounding::HalfUp)?
                    .number()),
            ),
        ]))
    }
    fn source_clearing_liquidation_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "computeLiquidationPriceFromLoss" => source_liq_bankruptcy(a),
            "handleLiquidation" => self.source_liquidation_handle(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
