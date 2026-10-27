// Included in source_contract_runtime.rs. Actual source metadata/effect owners.
impl ContractCore {
    fn oracle_price(&mut self, id: V) -> R<V> {
        self.database("oracleData")?;
        let rows = array(&self.db("oracleData", "findAsync", vec![obj(&[("oracleId", id)])])?)?;
        let mut best: Option<V> = None;
        for row in rows {
            if !crate::oracle_source_owner::source_number_unverified(&row.get("data").get("price"))?
                .is_finite()
            {
                continue;
            };
            if let Some(prior) = &best {
                if crate::oracle_source_owner::source_greater_unverified(
                    &row.get("blockHeight"),
                    &prior.get("blockHeight"),
                )? {
                    best = Some(row)
                }
            } else {
                best = Some(row)
            }
        }
        Ok(best.map_or(n(1.0), |v| v.get("data").get("price")))
    }
    fn clearlist_contains(&mut self, a: &[V]) -> R<V> {
        self.database("attestations")?;
        let rows = self.db(
            "attestations",
            "findAsync",
            vec![obj(&[
                ("data.address", arg(a, 1)),
                ("data.status", s("active")),
                (
                    "$or",
                    V::Array(vec![
                        obj(&[("data.listId", arg(a, 0))]),
                        obj(&[("data.clearlistId", arg(a, 0))]),
                    ]),
                ),
            ])],
        )?;
        Ok(V::Bool(!array(&rows)?.is_empty()))
    }
    fn clearing_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "recordTrade" => self.record_clearing_trade(a),
            "isPriceUpdatedForBlockHeight" => self.price_updated(a),
            _ if self.global_clearing_enabled() => self.source_clearing_global_call(method, a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn record_clearing_trade(&mut self, a: &[V]) -> R<V> {
        let key = format!("{}:{}", text(&arg(a, 0))?, text(&arg(a, 1))?);
        let index = if let Some(i) = self.block_trades.iter().position(|(k, _)| k == &key) {
            i
        } else {
            self.block_trades.push((
                key,
                obj(&[("openedSoFar", n(0.0)), ("trades", V::Array(vec![]))]),
            ));
            self.block_trades.len() - 1
        };
        let entry = &mut self.block_trades[index].1;
        if matches!(entry.get("pools"), V::Undefined) {
            let pool = obj(&[
                ("qty", V::BigNumber("0".into())),
                ("cost", V::BigNumber("0".into())),
            ]);
            set(
                entry,
                "pools",
                obj(&[("long", pool.clone()), ("short", pool)]),
            )?
        };
        let opened = dec(&n(number(&arg(a, 2))?.abs()))?;
        let closed = dec(&n(number(&arg(a, 3))?.abs()))?;
        let price = d0(&arg(a, 4))?;
        let buy = truth(&arg(a, 6));
        let open_name = if buy { "long" } else { "short" };
        let close_name = if buy { "short" } else { "long" };
        let mut pools = entry.get("pools");
        let mut open = pools.get(open_name);
        let mut close = pools.get(close_name);
        let before = dec(&open.get("qty"))?;
        if opened > crate::decimal::Decimal::zero() {
            let cost = add(&dec(&open.get("cost"))?, &mul(&opened, &price)?)?;
            set(
                &mut open,
                "qty",
                V::BigNumber(add(&before, &opened)?.to_string()),
            )?;
            set(&mut open, "cost", V::BigNumber(cost.to_string()))?;
        }
        let mut consumed = crate::decimal::Decimal::zero();
        let mut average = V::Null;
        if closed > crate::decimal::Decimal::zero()
            && dec(&close.get("qty"))? > crate::decimal::Decimal::zero()
        {
            consumed = min(closed.clone(), dec(&close.get("qty"))?);
            let avg = div(&dec(&close.get("cost"))?, &dec(&close.get("qty"))?)?;
            average = dn(&avg)?;
            let qty = sub(&dec(&close.get("qty"))?, &consumed)?;
            let cost = sub(&dec(&close.get("cost"))?, &mul(&avg, &consumed)?)?;
            set(&mut close, "qty", V::BigNumber(qty.to_string()))?;
            set(&mut close, "cost", V::BigNumber(cost.to_string()))?;
        }
        set(&mut pools, open_name, open)?;
        set(&mut pools, close_name, close)?;
        let signed = if buy {
            number(&dn(&opened)?)?
        } else {
            -number(&dn(&opened)?)?
        };
        let trade = obj(&[
            ("opened", n(signed)),
            ("closed", dn(&closed)?),
            ("consumedFromOpened", dn(&consumed)?),
            ("price", dn(&price)?),
            ("openedBefore", dn(&before)?),
            ("consumedAvgPrice", average),
            ("txid", arg(a, 5)),
        ]);
        let mut trades = array(&entry.get("trades"))?;
        trades.push(trade.clone());
        set(entry, "trades", V::Array(trades))?;
        set(
            entry,
            "openedSoFar",
            dn(&add(
                &dec(&pools.get("long").get("qty"))?,
                &dec(&pools.get("short").get("qty"))?,
            )?)?,
        )?;
        set(entry, "pools", pools)?;
        Ok(trade)
    }
    fn price_updated(&mut self, a: &[V]) -> R<V> {
        let id = arg(a, 0);
        let height = arg(a, 1);
        self.database("oracleData")?;
        self.database("volumeIndex")?;
        let result = (|| {
            let oracle = truth(&self.call("Contract.isOracleContract", vec![id.clone()])?);
            let oracle_id = if oracle {
                self.call("Contract.getOracleId", vec![id.clone()])?
            } else {
                V::Null
            };
            let cache_key = if oracle {
                oracle_id.clone()
            } else {
                id.clone()
            };
            let cache = if oracle {
                &self.oracle_marks
            } else {
                &self.native_marks
            };
            let old = cache
                .iter()
                .find(|(k, _)| same(k, &cache_key))
                .map_or(V::Null, |(_, v)| nullish(v.get("price"), V::Null));
            let mut update: Option<(V, V)> = None;
            let mut current = V::Null;
            if oracle {
                let rows = array(&self.db(
                    "oracleData",
                    "findAsync",
                    vec![obj(&[
                        ("oracleId", oracle_id.clone()),
                        ("blockHeight", height.clone()),
                    ])],
                )?)?;
                current = rows
                    .first()
                    .map_or(V::Null, |v| nullish(v.get("data").get("price"), V::Null));
                if !matches!(current, V::Null) {
                    update = Some((current.clone(), height.clone()))
                } else if matches!(old, V::Null) {
                    let rows = array(&self.db(
                        "oracleData",
                        "findAsync",
                        vec![obj(&[("oracleId", oracle_id.clone())])],
                    )?)?;
                    if let Some(mut best) = rows.first().cloned() {
                        for row in rows.into_iter().skip(1) {
                            if crate::oracle_source_owner::source_greater_unverified(
                                &if truth(&row.get("blockHeight")) {
                                    row.get("blockHeight")
                                } else {
                                    n(0.0)
                                },
                                &if truth(&best.get("blockHeight")) {
                                    best.get("blockHeight")
                                } else {
                                    n(0.0)
                                },
                            )? {
                                best = row
                            }
                        }
                        let p = nullish(best.get("data").get("price"), V::Null);
                        if !matches!(p, V::Null) {
                            update = Some((p, best.get("blockHeight")))
                        }
                    }
                }
            } else {
                let info = match self.call("Contract.getContractInfo", vec![id.clone()]) {
                    Ok(v) => v,
                    Err(Fault::Thrown(_)) => V::Null,
                    Err(e) => return Err(e),
                };
                let mut rows = vec![];
                if !matches!(info.get("notionalPropertyId"), V::Null | V::Undefined)
                    && !matches!(info.get("collateralPropertyId"), V::Null | V::Undefined)
                {
                    rows = array(&self.db(
                        "volumeIndex",
                        "findAsync",
                        vec![obj(&[(
                            "_id",
                            s(format!(
                                "{}-{}",
                                text(&info.get("notionalPropertyId"))?,
                                text(&info.get("collateralPropertyId"))?
                            )),
                        )])],
                    )?)?
                };
                if rows.is_empty() {
                    rows = array(&self.db(
                        "volumeIndex",
                        "findAsync",
                        vec![obj(&[("_id", id.clone())])],
                    )?)?
                };
                if let Some(row) = rows.first() {
                    let block = nullish(
                        row.get("value").get("blockHeight"),
                        nullish(row.get("blockHeight"), V::Null),
                    );
                    let price = nullish(
                        row.get("value").get("price"),
                        nullish(row.get("data").get("price"), V::Null),
                    );
                    if !matches!(price, V::Null) && same(&block, &height) {
                        current = price.clone();
                        update = Some((price, block));
                    } else if matches!(old, V::Null)
                        && !matches!(price, V::Null)
                        && !matches!(block, V::Null)
                    {
                        update = Some((price, block))
                    }
                }
            }
            let cache = if oracle {
                &mut self.oracle_marks
            } else {
                &mut self.native_marks
            };
            if let Some((price, block)) = update {
                let value = obj(&[("price", price), ("blockHeight", block)]);
                if let Some((_, v)) = cache.iter_mut().find(|(k, _)| same(k, &cache_key)) {
                    *v = value
                } else {
                    cache.push((cache_key.clone(), value))
                }
            };
            let last = if !matches!(current, V::Null) {
                old
            } else {
                cache
                    .iter()
                    .find(|(k, _)| same(k, &cache_key))
                    .map_or(V::Null, |(_, v)| nullish(v.get("price"), V::Null))
            };
            Ok(obj(&[
                (
                    "updated",
                    V::Bool(
                        !matches!(current, V::Null)
                            && (matches!(last, V::Null) || !same(&current, &last)),
                    ),
                ),
                ("lastPrice", last),
                ("thisPrice", current),
                ("blockHeight", height.clone()),
                ("contractId", id.clone()),
                ("isOracle", V::Bool(oracle)),
                ("oracleId", oracle_id),
            ]))
        })();
        match result {
            Err(Fault::Thrown(e)) => Ok(obj(&[
                ("updated", V::Bool(false)),
                ("lastPrice", V::Null),
                ("thisPrice", V::Null),
                ("blockHeight", height),
                ("contractId", id),
                ("isOracle", V::Bool(false)),
                ("oracleId", V::Null),
                ("error", s(e.message)),
            ])),
            other => other,
        }
    }
    fn cumulative_volumes(&mut self, block: f64) -> R<Vec<f64>> {
        self.database("volumeIndex")?;
        let mut totals = vec![];
        for (id, field) in [
            ("globalCumulativeVolume", "globalCumulativeVolume"),
            ("contractCumulativeVolume", "contractCumulativeVolume"),
            ("ltcPairCumulativeVolume", "ltcPairTotalVolume"),
        ] {
            let result = (|| {
                let mut rows = array(&self.db(
                    "volumeIndex",
                    "findAsync",
                    vec![obj(&[
                        ("_id", s(id)),
                        ("value.block", obj(&[("$lt", n(block))])),
                    ])],
                )?)?;
                rows.sort_by(|a, b| {
                    let a = number(&a.get("value").get("block")).unwrap_or(f64::NAN);
                    let b = number(&b.get("value").get("block")).unwrap_or(f64::NAN);
                    b.partial_cmp(&a).unwrap_or(core::cmp::Ordering::Equal)
                });
                rows.first().map_or(Ok(0.0), |r| {
                    number(&if truth(&r.get("value").get(field)) {
                        r.get("value").get(field)
                    } else {
                        n(0.0)
                    })
                })
            })();
            totals.push(match result {
                Ok(v) => v,
                Err(Fault::Thrown(_)) => 0.0,
                Err(e) => return Err(e),
            });
        }
        Ok(totals)
    }
    fn liquidity_reward(&mut self, method: &str, a: &[V]) -> R<V> {
        let ctx = if method == "baselineLiquidityReward" {
            arg(a, 3)
        } else {
            arg(a, 2)
        };
        if matches!(ctx, V::Null) {
            return Err(type_error(
                "Cannot read properties of null (reading 'feePaid')",
            ));
        }
        let fee = if method == "baselineLiquidityReward" {
            number(&arg(a, 1))?
        } else if !matches!(ctx.get("feePaid"), V::Null | V::Undefined) {
            number(&ctx.get("feePaid"))?
        } else {
            let volume = d0(&arg(a, 0))?;
            let rate = dec(&n(number(&nullish(ctx.get("feeRate"), n(0.00005)))?))?;
            number(&dn(&mul(&volume, &rate)?)?)?
        };
        let mut cumulative = number(&nullish(ctx.get("cumulativeLtc"), n(0.0)))?;
        if cumulative.is_nan() || cumulative <= 0.0 {
            let block = number(&if truth(&ctx.get("block")) {
                ctx.get("block")
            } else {
                n(0.0)
            })?;
            let block = if block > 0.0 { block } else { 1.0 } + 1.0;
            let totals = match self.cumulative_volumes(block) {
                Ok(v) => v,
                Err(Fault::Thrown(_)) => vec![0.0; 3],
                Err(e) => return Err(e),
            };
            cumulative = if totals[2] != 0.0 && !totals[2].is_nan() {
                totals[2]
            } else {
                totals[0]
            };
        }
        let fee = if fee.is_nan() { 0.0 } else { fee.max(0.0) };
        if fee <= 0.0 {
            return Ok(n(0.0));
        };
        let max = number(&nullish(ctx.get("maxShare"), n(0.35)))?;
        let min_share = number(&nullish(ctx.get("minShare"), n(0.02)))?;
        let pivot = number(&nullish(ctx.get("pivotLtc"), n(1000.0)))?.max(1.0);
        let slope = number(&nullish(ctx.get("slope"), n(1.0)))?.max(0.0);
        let cumulative = if cumulative.is_nan() {
            0.0
        } else {
            cumulative.max(0.0)
        };
        let share = (max / (1.0 + (1.0 + cumulative / pivot).log10() * slope))
            .min(max)
            .max(min_share)
            .min(0.99);
        let reward = mul(&dec(&n(fee))?, &dec(&n(share))?)?
            .quantize(8, crate::decimal::Rounding::Down)
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
        dn(&reward)
    }
    fn volume_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "getLastPrice" if self.lifecycle_enabled() => self.source_registry_last_price(a),
            "getTokenPriceInLTC" if self.lifecycle_enabled() => self.source_channel_token_price(a),
            "getVWAP" if self.global_clearing_enabled() => self.source_funding_volume_vwap(a),
            "getContractUnitLTCValue" => {
                if self.entry == "volume-first" {
                    return Err(type_error(
                        "VolumeIndex.getContractUnitLTCValue is not a function",
                    ));
                };
                let result = (|| {
                    let raw = self.call("Contract.getContractInfo", vec![arg(a, 0)])?;
                    let info = if truth(&raw.get("data")) {
                        raw.get("data")
                    } else {
                        raw
                    };
                    let property = nullish(info.get("notionalPropertyId"), n(0.0));
                    let token = if property == n(0.0) || property == s("0") || property == s("LTC")
                    {
                        1.0
                    } else {
                        self.database("volumeIndex")?;
                        let row = self.find(
                            "volumeIndex",
                            obj(&[("_id", s(format!("0-{}", text(&property)?)))]),
                        )?;
                        if truth(&row.get("value").get("price")) {
                            number(&row.get("value").get("price"))?
                        } else {
                            0.001
                        }
                    };
                    let value = number(&nullish(info.get("notionalValue"), n(1.0)))? * token;
                    let leverage = number(&nullish(info.get("leverage"), n(1.0)))?;
                    let leverage = if leverage == 0.0 { 1.0 } else { leverage };
                    if !value.is_finite() || value <= 0.0 {
                        return Ok(n(0.0));
                    };
                    let value = value / leverage;
                    Ok(n(crate::volume_index::number_fixed_eight(value)
                        .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?))
                })();
                match result {
                    Err(Fault::Thrown(_)) => Ok(n(0.0)),
                    other => other,
                }
            }
            "saveVolumeDataById" => {
                let id = arg(a, 0);
                self.database("volumeIndex")?;
                self.update(
                    "volumeIndex",
                    obj(&[("_id", id.clone())]),
                    obj(&[
                        ("_id", id),
                        (
                            "value",
                            obj(&[
                                ("blockHeight", arg(a, 4)),
                                ("rawVolume", arg(a, 1)),
                                ("ltcVolume", arg(a, 2)),
                                ("price", arg(a, 3)),
                                ("type", arg(a, 5)),
                            ]),
                        ),
                    ]),
                    true,
                )?;
                self.database("volumeIndex")?;
                let global =
                    self.find("volumeIndex", obj(&[("_id", s("globalCumulativeVolume"))]))?;
                let ltc =
                    self.find("volumeIndex", obj(&[("_id", s("ltcPairCumulativeVolume"))]))?;
                let total = q(
                    &add(
                        &d0(&global.get("value").get("globalCumulativeVolume"))?,
                        &d0(&arg(a, 2))?,
                    )?,
                    8,
                )?;
                let mut pair = d0(&ltc.get("value").get("ltcPairTotalVolume"))?;
                if arg(a, 5) == s("UTXO") {
                    pair = q(&add(&pair, &d0(&arg(a, 2))?)?, 8)?
                };
                self.update(
                    "volumeIndex",
                    obj(&[("_id", s("ltcPairCumulativeVolume"))]),
                    obj(&[
                        ("_id", s("ltcPairCumulativeVolume")),
                        (
                            "value",
                            obj(&[("ltcPairTotalVolume", dn(&pair)?), ("block", arg(a, 4))]),
                        ),
                    ]),
                    true,
                )?;
                self.update(
                    "volumeIndex",
                    obj(&[("_id", s("globalCumulativeVolume"))]),
                    obj(&[
                        ("_id", s("globalCumulativeVolume")),
                        (
                            "value",
                            obj(&[
                                ("globalCumulativeVolume", dn(&total)?),
                                ("block", arg(a, 4)),
                            ]),
                        ),
                    ]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "baselineLiquidityReward" | "calculateLiquidityReward" => {
                self.liquidity_reward(method, a)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn record_contract_trade(&mut self, a: &[V]) -> R<V> {
        let trade = arg(a, 0);
        let block = arg(a, 1);
        let key = format!("contract-{}", text(&trade.get("contractId"))?);
        let record = obj(&[
            ("key", s(&key)),
            ("type", s("contract")),
            ("trade", trade),
            ("blockHeight", block.clone()),
            ("sellerTx", arg(a, 2)),
            ("buyerTx", arg(a, 3)),
        ]);
        self.database("tradeHistory")?;
        let uuid = self.new_uuid();
        let id = format!("{key}-{uuid}-{}", text(&block)?);
        let mut doc = obj(&[("_id", s(&id))]);
        if let V::Object(fields) = record {
            for (k, v) in fields {
                set(&mut doc, &k, v)?
            }
        };
        self.update("tradeHistory", obj(&[("_id", s(id))]), doc, true)?;
        Ok(V::Undefined)
    }
    fn save_pnl(&mut self, a: &[V]) -> R<V> {
        let params = arg(a, 0);
        let key = format!(
            "rPNL-{}-{}-{}",
            text(&params.get("address"))?,
            text(&params.get("contractId"))?,
            text(&params.get("height"))?
        );
        self.database("tradeHistory")?;
        self.update(
            "tradeHistory",
            obj(&[("_id", s(&key))]),
            obj(&[("$set", obj(&[("value", s(json(&params)?))]))]),
            true,
        )?;
        Ok(s(key))
    }
    fn iou_audit(&mut self, event: V) -> R<()> {
        self.database("iou")?;
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
        if let V::Object(fields) = event {
            for (k, v) in fields {
                set(&mut doc, &k, v)?
            }
        };
        match self.db("iou", "insertAsync", vec![doc]) {
            Err(Fault::Thrown(_)) => Ok(()),
            other => other.map(|_| ()),
        }
    }
    fn iou_call(&mut self, method: &str, a: &[V]) -> R<V> {
        if method == "addIouClaims" {
            let delta = d0(&arg(a, 7))?;
            if delta <= crate::decimal::Decimal::zero() {
                return Ok(V::Null);
            };
            let buyer = d0(&arg(a, 5))?.max(crate::decimal::Decimal::zero());
            let seller = d0(&arg(a, 6))?.max(crate::decimal::Decimal::zero());
            let sum = add(&buyer, &seller)?;
            if sum <= crate::decimal::Decimal::zero() {
                return Ok(V::Null);
            };
            let b = mul(&delta, &div(&buyer, &sum)?)?;
            let se = mul(&delta, &div(&seller, &sum)?)?;
            let key = format!(
                "claims:{}:{}",
                text(&n(number(&arg(a, 0))?))?,
                text(&n(number(&arg(a, 1))?))?
            );
            self.database("iou")?;
            let mut doc = self.find("iou", obj(&[("_id", s(&key))]))?;
            if !truth(&doc) {
                doc = obj(&[
                    ("_id", s(&key)),
                    ("contractId", n(number(&arg(a, 0))?)),
                    ("propertyId", n(number(&arg(a, 1))?)),
                    ("claims", obj(&[])),
                    ("lastBlock", arg(a, 2)),
                ])
            };
            let mut claims = doc.get("claims");
            for (address, amount) in [(arg(a, 3), b.clone()), (arg(a, 4), se.clone())] {
                if amount > crate::decimal::Decimal::zero() {
                    let address = text(&address)?;
                    let next = add(&d0(&claims.get(&address))?, &amount)?;
                    set(&mut claims, &address, s(q(&next, 20)?.to_string()))?
                }
            }
            set(&mut doc, "claims", claims)?;
            set(&mut doc, "lastBlock", arg(a, 2))?;
            self.database("iou")?;
            self.update("iou", obj(&[("_id", s(&key))]), doc.clone(), true)?;
            self.iou_audit(obj(&[
                ("event", s("claim_accrual")),
                ("block", arg(a, 2)),
                ("contractId", n(number(&arg(a, 0))?)),
                ("propertyId", n(number(&arg(a, 1))?)),
                ("delta", s(q(&delta, 20)?.to_string())),
                ("buyerAddr", arg(a, 3)),
                ("sellerAddr", arg(a, 4)),
                ("buyerShare", s(q(&b, 20)?.to_string())),
                ("sellerShare", s(q(&se, 20)?.to_string())),
            ]))?;
            return Ok(doc);
        }
        if !matches!(method, "addProfit" | "addLoss") {
            if self.global_clearing_enabled() {
                return self.source_iou_settlement_call(method, a);
            }
            return Err(SourceRuleError::UnknownDependency.into());
        };
        let amount = d0(&arg(a, 2))?;
        if amount <= crate::decimal::Decimal::zero() {
            return Ok(V::Null);
        };
        let key = format!("{}:{}", text(&arg(a, 0))?, text(&arg(a, 1))?);
        self.database("iou")?;
        let existing = match self.find("iou", obj(&[("_id", s(&key))])) {
            Ok(v) => v,
            Err(Fault::Thrown(_)) => V::Null,
            Err(e) => return Err(e),
        };
        let current = d0(&existing.get("amount"))?;
        let new_block =
            !truth(&existing) || number(&existing.get("lastBlock"))? != number(&arg(a, 3))?;
        let start = if new_block {
            current.clone()
        } else if truth(&existing.get("blockStartAmount")) {
            dec(&existing.get("blockStartAmount"))?
        } else {
            current.clone()
        };
        let prior = |name: &str| {
            if new_block {
                Ok(crate::decimal::Decimal::zero())
            } else {
                d0(&existing.get(name))
            }
        };
        let profit = method == "addProfit";
        let delta = if profit {
            sub(&prior("blockDelta")?, &amount)?
        } else {
            add(&prior("blockDelta")?, &amount)?
        };
        let losses = if profit {
            prior("blockLosses")?
        } else {
            add(&prior("blockLosses")?, &amount)?
        };
        let profits = if profit {
            add(&prior("blockProfits")?, &amount)?
        } else {
            prior("blockProfits")?
        };
        let updated = if profit {
            sub(&current, &amount)?
        } else {
            add(&current, &amount)?
        };
        let doc = obj(&[
            ("_id", s(&key)),
            ("contractId", n(number(&arg(a, 0))?)),
            ("propertyId", n(number(&arg(a, 1))?)),
            ("amount", s(q(&updated, 20)?.to_string())),
            ("lastBlock", arg(a, 3)),
            ("blockStartAmount", s(q(&start, 20)?.to_string())),
            ("blockDelta", s(q(&delta, 20)?.to_string())),
            ("blockLosses", s(q(&losses, 20)?.to_string())),
            ("blockProfits", s(q(&profits, 20)?.to_string())),
        ]);
        self.update("iou", obj(&[("_id", s(&key))]), doc.clone(), true)?;
        self.iou_audit(obj(&[
            (
                "event",
                s(if profit {
                    "bucket_profit"
                } else {
                    "bucket_loss"
                }),
            ),
            ("block", arg(a, 3)),
            ("contractId", n(number(&arg(a, 0))?)),
            ("propertyId", n(number(&arg(a, 1))?)),
            (
                if profit { "profit" } else { "loss" },
                s(q(&amount, 20)?.to_string()),
            ),
            ("amountBefore", s(q(&current, 20)?.to_string())),
            ("amountAfter", s(q(&updated, 20)?.to_string())),
            ("blockLosses", s(q(&losses, 20)?.to_string())),
            ("blockProfits", s(q(&profits, 20)?.to_string())),
        ]))?;
        Ok(doc)
    }
}
