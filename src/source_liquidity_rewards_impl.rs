// Included in source_contract_runtime.rs under the explicit V6 policy.
// These are the actual Logic private maps; Tally remains the sole balance owner.
const LIQUIDITY_QUEUE: &str = "sourceLiquidityRewardQueue";
const LIQUIDITY_VOLUMES: &str = "sourceLiquidityRewardVolumeCache";
fn liquidity_index(value: &V, index: usize) -> V {
    if let V::Array(values) = value {
        arg(values, index)
    } else {
        V::Undefined
    }
}
impl ContractCore {
    fn source_liquidity_rewards_state(&self) -> V {
        obj(&[
            ("queue", self.source_liquidity_map_value(LIQUIDITY_QUEUE)),
            (
                "volumeCache",
                self.source_liquidity_map_value(LIQUIDITY_VOLUMES),
            ),
        ])
    }
    fn source_liquidity_map_value(&self, key: &str) -> V {
        match self.globals.get(key) {
            V::Undefined => V::Array(vec![]),
            v => v,
        }
    }
    fn source_liquidity_map(&self, key: &str) -> R<Vec<V>> {
        array(&self.source_liquidity_map_value(key))
    }
    fn source_liquidity_store(&mut self, key: &str, map: Vec<V>) -> R<()> {
        self.globals.insert(key, V::Array(map))?;
        Ok(())
    }
    fn source_liquidity_lookup(&self, key: &str, block: &str) -> R<V> {
        Ok(self
            .source_liquidity_map(key)?
            .into_iter()
            .find(|row| liquidity_index(row, 0) == s(block))
            .map_or(V::Undefined, |r| liquidity_index(&r, 1)))
    }
    fn source_liquidity_cache(&mut self, key: &str, block: &str, value: V) -> R<()> {
        let mut map = self.source_liquidity_map(key)?;
        if let Some(row) = map.iter_mut().find(|r| liquidity_index(r, 0) == s(block)) {
            *row = V::Array(vec![s(block), value]);
        } else {
            map.push(V::Array(vec![s(block), value]));
        }
        self.source_liquidity_store(key, map)
    }
    fn source_liquidity_delete(&mut self, key: &str, block: &str) -> R<()> {
        let mut map = self.source_liquidity_map(key)?;
        map.retain(|r| liquidity_index(r, 0) != s(block));
        // JS Map.delete on an allocated empty Map has no further side effects.
        if !matches!(self.globals.get(key), V::Undefined) {
            self.source_liquidity_store(key, map)?;
        }
        Ok(())
    }
    fn source_liquidity_queue(&mut self, a: &[V]) -> R<V> {
        let amount = FundingBn::new(&if truth(&arg(a, 1)) { arg(a, 1) } else { n(0.) })?
            .q(8, FundingRounding::Down)?;
        if amount.number() <= 0. {
            return Ok(V::Undefined);
        }
        let mut address = arg(a, 0);
        if matches!(
            address,
            V::Array(_) | V::Object(_) | V::Promise | V::BigNumber(_)
        ) {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        if matches!(&address,V::Number(bits)if f64::from_bits(*bits)==0.) {
            address = n(0.);
        }
        let block = source_channel_string(&arg(a, 2))?;
        let found = self.source_liquidity_lookup(LIQUIDITY_QUEUE, &block)?;
        let mut entries = if truth(&found) {
            array(&found)?
        } else {
            vec![]
        };
        if let Some(pair) = entries
            .iter_mut()
            .find(|pair| oracle_source_same_key(&liquidity_index(pair, 0), &address))
        {
            let mut entry = liquidity_index(pair, 1);
            let total = FundingBn::new(&entry.get("amount"))?
                .op(amount, 0)?
                .q(8, FundingRounding::Down)?
                .value();
            set(&mut entry, "amount", total)?;
            *pair = V::Array(vec![address, entry]);
        } else {
            let reason = if matches!(arg(a, 4), V::Undefined) {
                s("liquidityReward")
            } else {
                arg(a, 4)
            };
            entries.push(V::Array(vec![
                address,
                obj(&[
                    ("amount", amount.value()),
                    ("txid", arg(a, 3)),
                    ("reason", reason),
                ]),
            ]));
        }
        self.source_liquidity_cache(LIQUIDITY_QUEUE, &block, V::Array(entries))?;
        Ok(V::Undefined)
    }
    fn source_liquidity_settle(&mut self, a: &[V]) -> R<V> {
        let block = source_channel_string(&arg(a, 0))?;
        let found = self.source_liquidity_lookup(LIQUIDITY_QUEUE, &block)?;
        if !truth(&found) {
            return Ok(V::Undefined);
        }
        let entries = array(&found)?;
        if entries.is_empty() {
            return Ok(V::Undefined);
        }
        for pair in &entries {
            let entry = liquidity_index(pair, 1);
            let amount = FundingBn::new(&entry.get("amount"))?;
            if amount.number() <= 0. {
                continue;
            }
            let reason = if truth(&entry.get("reason")) {
                entry.get("reason")
            } else {
                s("liquidityReward")
            };
            self.call(
                "Tally.updateBalance",
                vec![
                    liquidity_index(pair, 0),
                    n(3.),
                    n(amount.number()),
                    n(0.),
                    n(0.),
                    n(0.),
                    reason,
                    arg(a, 0),
                    entry.get("txid"),
                ],
            )?;
        }
        self.source_liquidity_delete(LIQUIDITY_QUEUE, &block)?;
        self.source_liquidity_delete(LIQUIDITY_VOLUMES, &block)?;
        Ok(V::Undefined)
    }
    fn source_liquidity_rewards_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "queueLiquidityReward" => self.source_liquidity_queue(a),
            "settleLiquidityRewards" => self.source_liquidity_settle(a),
            "tradeTokenForUTXO" => self.source_liquidity_utxo_trade(a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}

impl ContractCore {
    fn source_liquidity_env(&self, key: &str, fallback: V) -> V {
        let v = self.source_proof_environment.get(key);
        if truth(&v) {
            v
        } else {
            fallback
        }
    }
    fn source_liquidity_listed(&mut self, list: V, address: V) -> R<V> {
        match self.call("ClearList.isAddressInClearlist", vec![list, address]) {
            Ok(v) => Ok(v),
            Err(Fault::Thrown(_)) => Ok(V::Bool(false)),
            Err(e) => Err(e),
        }
    }
    fn source_liquidity_utxo_trade(&mut self, a: &[V]) -> R<V> {
        // The first three source constructors are evaluated even though two
        // locals are otherwise unused. Unsupported domains remain explicit.
        let received = FundingBn::new(&arg(a, 7))?;
        let _expected = FundingBn::new(&arg(a, 5))?;
        let _decoded = FundingBn::new(&arg(a, 3))?;
        let tokens = number(&arg(a, 3))?;
        let sender = arg(a, 0);
        let receiver = arg(a, 1);
        let property = arg(a, 2);
        let property_key = source_channel_string(&property)?;
        let delivery = arg(a, 6);
        let block = arg(a, 11);
        let txid = arg(a, 12);
        let mut price = arg(a, 8);
        let mut column = arg(a, 4);
        let mut channel = self.call("Channels.getChannel", vec![sender.clone()])?;
        if !truth(&channel) {
            return Ok(V::Undefined);
        }
        let mut balance = V::Undefined;
        if column == V::Bool(true) {
            balance = funding_or(channel.get("A").get(&property_key));
            if balance == n(0.) {
                column = V::Bool(false);
                balance = funding_or(channel.get("B").get(&property_key));
            }
        } else if column == V::Bool(false) {
            balance = funding_or(channel.get("B").get(&property_key));
            if balance == n(0.) {
                column = V::Bool(true);
                balance = funding_or(channel.get("A").get(&property_key));
            }
        }
        if balance == n(0.) {
            return Ok(V::Undefined);
        }
        if tokens > number(&balance)? {
            return Err(type_error("Assignment to constant variable."));
        }
        if column == V::Bool(true) || column == V::Bool(false) {
            let key = if column == V::Bool(true) { "A" } else { "B" };
            let prior = channel.get(key).get(&property_key);
            source_channel_column_write(
                &mut channel,
                key,
                &property_key,
                n(number(&prior)? - tokens),
            )?;
        }
        self.call("Channels.setChannel", vec![sender.clone(), channel])?;
        self.call(
            "Tally.updateChannelBalance",
            vec![
                sender.clone(),
                property.clone(),
                n(-tokens),
                s("UTXOTokenTradeDebit"),
                block.clone(),
            ],
        )?;
        let fee = FundingBn::new(&arg(a, 3))?
            .op(FundingBn::new(&n(0.00005))?, 2)?
            .q(8, FundingRounding::HalfUp)?
            .number();
        let net = FundingBn::new(&n(tokens))?
            .op(FundingBn::new(&n(fee))?, 1)?
            .q(8, FundingRounding::HalfUp)?
            .number();
        let withdrawal = arg(a, 10);
        if matches!(&withdrawal, V::String(_)) {
            self.call(
                "Tally.updateChannelBalance",
                vec![
                    delivery.clone(),
                    property.clone(),
                    n(net),
                    s("UTXOTokenTradeCredit"),
                    block.clone(),
                ],
            )?;
            self.call(
                "Channels.recordCommitToChannel",
                vec![
                    delivery.clone(),
                    withdrawal,
                    property.clone(),
                    arg(a, 3),
                    V::Bool(false),
                    V::Null,
                    block.clone(),
                ],
            )?;
        } else {
            self.call(
                "Tally.updateBalance",
                vec![
                    delivery.clone(),
                    property.clone(),
                    n(net),
                    n(0.),
                    n(0.),
                    n(0.),
                    s("UTXOTokenTradeCredit"),
                    block.clone(),
                ],
            )?;
        }
        self.call(
            "Tally.updateBalance",
            vec![
                delivery.clone(),
                property.clone(),
                n(-fee),
                n(0.),
                n(0.),
                n(0.),
                s("utxoFee"),
                block.clone(),
                txid.clone(),
            ],
        )?;
        self.call(
            "Tally.updateFeeCache",
            vec![property.clone(), n(fee), n(1.), block.clone()],
        )?;
        let key = source_js_plus(&s("0-"), &property)?;
        let coin = received
            .op(FundingBn::new(&n(1e8))?, 3)?
            .q(8, FundingRounding::Down)?;
        if !number(&price)?.is_finite() {
            price = n(coin
                .clone()
                .op(FundingBn::new(&arg(a, 3))?, 3)?
                .q(8, FundingRounding::HalfUp)?
                .number());
        }
        self.call(
            "VolumeIndex.saveVolumeDataById",
            vec![
                key.clone(),
                arg(a, 3),
                n(coin.number()),
                price.clone(),
                block.clone(),
                s("UTXO"),
            ],
        )?;
        let trade = obj(&[
            ("offeredPropertyId", n(0.)),
            ("desiredPropertyId", property.clone()),
            ("amountOffered", arg(a, 3)),
            ("amountExpected", n(coin.number())),
            ("price", price),
            ("takerFee", n(fee)),
            ("block", block.clone()),
            ("buyer", delivery),
            ("seller", receiver.clone()),
            ("takerTxId", txid.clone()),
        ]);
        self.call("Orderbook.getOrderbookInstance", vec![key])?;
        self.call(
            "Orderbook.recordTokenTrade",
            vec![trade, block.clone(), txid.clone()],
        )?;
        let address_list = n(number(
            &self.source_liquidity_env("LIQUIDITY_REWARD_ADDRESS_CLEARLIST_ID", n(2.)),
        )?);
        let issuer_list = n(number(
            &self.source_liquidity_env("LIQUIDITY_REWARD_ISSUER_CLEARLIST_ID", n(1.)),
        )?);
        let raw = source_channel_string(
            &self.source_liquidity_env("LIQUIDITY_REWARD_PROPERTY_ALLOWLIST", s("")),
        )?;
        let allowlist = raw
            .split(',')
            .map(|s| {
                number(&crate::source_contract_values::s(
                    s.trim_matches(funding_js_space),
                ))
            })
            .collect::<R<Vec<_>>>()?
            .into_iter()
            .filter(|v| v.is_finite())
            .collect::<Vec<_>>();
        let listed_a = self.source_liquidity_listed(address_list.clone(), sender.clone())?;
        let listed_b = self.source_liquidity_listed(address_list, receiver.clone())?;
        let allow = allowlist.is_empty() || allowlist.contains(&number(&property)?);
        let token_listed = if source_channel_string(&property)?.starts_with("s-") {
            allow
        } else {
            let info = self.call("Property.getPropertyData", vec![property.clone()])?;
            let issuer = info.get("issuer");
            let by_issuer = if truth(&issuer) {
                self.source_liquidity_listed(issuer_list, issuer)?
            } else {
                V::Bool(false)
            };
            allow && (!allowlist.is_empty() || truth(&by_issuer))
        };
        let participants = u32::from(truth(&listed_a)) + u32::from(truth(&listed_b));
        if token_listed && participants > 0 {
            let block_key = source_channel_string(&block)?;
            let mut cumulative = self.source_liquidity_lookup(LIQUIDITY_VOLUMES, &block_key)?;
            if !truth(&cumulative) {
                cumulative = match self.call(
                    "VolumeIndex.getCumulativeVolumes",
                    vec![source_js_plus(&block, &n(1.))?],
                ) {
                    Ok(v) => v,
                    Err(Fault::Thrown(_)) => V::Null,
                    Err(e) => return Err(e),
                };
                self.source_liquidity_cache(
                    LIQUIDITY_VOLUMES,
                    &block_key,
                    if truth(&cumulative) {
                        cumulative.clone()
                    } else {
                        obj(&[])
                    },
                )?;
            }
            let cumulative_ltc = if truth(&cumulative.get("ltcPairTotalVolume")) {
                cumulative.get("ltcPairTotalVolume")
            } else {
                funding_or(cumulative.get("globalCumulativeVolume"))
            };
            let mut ctx = obj(&[
                ("feePaid", n(fee)),
                ("cumulativeLtc", n(number(&cumulative_ltc)?)),
                ("block", block.clone()),
            ]);
            for (key, env, default) in [
                ("maxShare", "LIQUIDITY_REWARD_MAX_SHARE", 0.35),
                ("minShare", "LIQUIDITY_REWARD_MIN_SHARE", 0.02),
                ("pivotLtc", "LIQUIDITY_REWARD_PIVOT_LTC", 1000.),
                ("slope", "LIQUIDITY_REWARD_SLOPE", 1.),
            ] {
                set(
                    &mut ctx,
                    key,
                    n(number(&self.source_liquidity_env(env, n(default)))?),
                )?;
            }
            let budget = self.call(
                "VolumeIndex.calculateLiquidityReward",
                vec![arg(a, 3), property, ctx],
            )?;
            if number(&budget)? > 0. {
                let per = FundingBn::new(&budget)?
                    .op(FundingBn::new(&n(f64::from(participants)))?, 3)?
                    .q(8, FundingRounding::Down)?;
                let spent = per
                    .clone()
                    .op(FundingBn::new(&n(f64::from(participants)))?, 2)?;
                let remainder = FundingBn::new(&budget)?
                    .op(spent, 1)?
                    .q(8, FundingRounding::Down)?;
                if truth(&listed_a) {
                    self.call(
                        "Logic.queueLiquidityReward",
                        vec![
                            sender.clone(),
                            per.value(),
                            block.clone(),
                            txid.clone(),
                            s("liquidityReward"),
                        ],
                    )?;
                }
                if truth(&listed_b) {
                    let extra = if remainder.number() > 0. && !truth(&listed_a) {
                        remainder.clone()
                    } else {
                        FundingBn::new(&n(0.))?
                    };
                    self.call(
                        "Logic.queueLiquidityReward",
                        vec![
                            receiver,
                            per.op(extra, 0)?.value(),
                            block,
                            txid,
                            s("liquidityReward"),
                        ],
                    )?;
                } else if truth(&listed_a) && remainder.number() > 0. {
                    self.call(
                        "Logic.queueLiquidityReward",
                        vec![
                            sender,
                            remainder.value(),
                            block,
                            txid,
                            s("liquidityRewardRemainder"),
                        ],
                    )?;
                }
            }
        }
        Ok(V::Undefined)
    }
}

impl ContractCore {
    fn source_liquidity_cumulative_volumes(&mut self, block: V) -> R<V> {
        self.database("volumeIndex")?;
        let mut totals = Vec::new();
        for (id, field) in [
            ("globalCumulativeVolume", "globalCumulativeVolume"),
            ("contractCumulativeVolume", "contractCumulativeVolume"),
            ("ltcPairCumulativeVolume", "ltcPairTotalVolume"),
        ] {
            let result: R<V> = (|| {
                let value = self.db(
                    "volumeIndex",
                    "findAsync",
                    vec![obj(&[
                        ("_id", s(id)),
                        ("value.block", obj(&[("$lt", block.clone())])),
                    ])],
                )?;
                let V::Array(mut rows) = value else {
                    return Ok(n(0.));
                };
                if rows.is_empty() {
                    return Ok(n(0.));
                }
                let mut failure = None;
                rows.sort_by(|a, b| {
                    let compared: R<core::cmp::Ordering> = (|| {
                        let b = number(&funding_prop(&funding_prop(b, "value")?, "block")?)?;
                        let a = number(&funding_prop(&funding_prop(a, "value")?, "block")?)?;
                        let delta = b - a;
                        Ok(if delta.is_nan() || delta == 0. {
                            core::cmp::Ordering::Equal
                        } else if delta < 0. {
                            core::cmp::Ordering::Less
                        } else {
                            core::cmp::Ordering::Greater
                        })
                    })();
                    match compared {
                        Ok(order) => order,
                        Err(e) => {
                            if failure.is_none() {
                                failure = Some(e);
                            }
                            core::cmp::Ordering::Equal
                        }
                    }
                });
                if let Some(e) = failure {
                    return Err(e);
                }
                Ok(funding_or(funding_prop(
                    &funding_prop(&rows[0], "value")?,
                    field,
                )?))
            })();
            totals.push(match result {
                Ok(v) => v,
                Err(Fault::Thrown(_)) => n(0.),
                Err(e) => return Err(e),
            });
        }
        Ok(obj(&[
            ("globalCumulativeVolume", totals[0].clone()),
            ("ltcPairTotalVolume", totals[2].clone()),
            ("contractCumulativeVolume", totals[1].clone()),
        ]))
    }
}

fn liquidity_math_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0. && b == 0. {
        if a.is_sign_negative() && b.is_sign_negative() {
            -0.
        } else {
            0.
        }
    } else {
        a.max(b)
    }
}
fn liquidity_math_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0. && b == 0. {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.
        } else {
            0.
        }
    } else {
        a.min(b)
    }
}
impl ContractCore {
    fn source_liquidity_calculate_reward(&mut self, a: &[V]) -> R<V> {
        let ctx = if matches!(arg(a, 2), V::Undefined) {
            obj(&[])
        } else {
            arg(a, 2)
        };
        let paid = funding_prop(&ctx, "feePaid")?;
        let fee = if !matches!(paid, V::Null | V::Undefined) {
            number(&paid)?
        } else {
            FundingBn::new(&funding_or(arg(a, 0)))?
                .op(
                    FundingBn::new(&n(number(&nullish(
                        funding_prop(&ctx, "feeRate")?,
                        n(0.00005),
                    ))?))?,
                    2,
                )?
                .number()
        };
        let mut cumulative = number(&nullish(funding_prop(&ctx, "cumulativeLtc")?, n(0.)))?;
        if cumulative.is_nan() || cumulative <= 0. {
            let block = number(&funding_or(funding_prop(&ctx, "block")?))?;
            let read = self.call(
                "VolumeIndex.getCumulativeVolumes",
                vec![n(if block > 0. { block + 1. } else { 2. })],
            );
            let volumes = match read {
                Ok(v) => v,
                Err(Fault::Thrown(_)) => V::Null,
                Err(e) => return Err(e),
            };
            cumulative = number(&if truth(&volumes.get("ltcPairTotalVolume")) {
                volumes.get("ltcPairTotalVolume")
            } else {
                funding_or(volumes.get("globalCumulativeVolume"))
            })?;
        }
        // calculateAttenuatedLiquidityReward first applies feePaid || 0.
        let fee = liquidity_math_max(if fee == 0. || fee.is_nan() { 0. } else { fee }, 0.);
        if fee <= 0. {
            return Ok(n(0.));
        }
        let max = number(&nullish(funding_prop(&ctx, "maxShare")?, n(0.35)))?;
        let min = number(&nullish(funding_prop(&ctx, "minShare")?, n(0.02)))?;
        let pivot = liquidity_math_max(
            number(&nullish(funding_prop(&ctx, "pivotLtc")?, n(1000.)))?,
            1.,
        );
        let slope = liquidity_math_max(number(&nullish(funding_prop(&ctx, "slope")?, n(1.)))?, 0.);
        let cumulative = liquidity_math_max(
            if cumulative == 0. || cumulative.is_nan() {
                0.
            } else {
                cumulative
            },
            0.,
        );
        let attenuation = max / (1. + (1. + cumulative / pivot).log10() * slope);
        let clamped = liquidity_math_max(min, liquidity_math_min(max, attenuation));
        let share = liquidity_math_min(0.99, clamped);
        Ok(n(FundingBn::new(&n(fee))?
            .op(FundingBn::new(&n(share))?, 2)?
            .q(8, FundingRounding::Down)?
            .number()))
    }
}
