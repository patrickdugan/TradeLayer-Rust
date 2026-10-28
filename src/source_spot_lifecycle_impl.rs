// Future explicit spot lifecycle only. The existing Core and queue arena own
// every balance, collection, receiver and book. No conserving spot substitute.
fn spot_handle(value: &V) -> R<u64> {
    let n = spot_number(&value.get("orderbookInstance"))?;
    if !n.is_finite() || n <= 0. || n.fract() != 0. || n > u64::MAX as f64 {
        return Err(SourceRuleError::InvalidInput.into());
    }
    Ok(n as u64)
}
impl ContractCore {
    fn source_spot_trace(&mut self, target: &str, args: Vec<V>) -> usize {
        let at = self.effects.len();
        self.emit(obj(&[
            ("kind", s("dependency")),
            ("target", s(target)),
            ("args", V::Array(args)),
        ]));
        at
    }
    fn source_spot_trace_end(&mut self, at: usize, result: &R<V>) {
        if let Some(event) = self.effects.get_mut(at) {
            match result {
                Ok(v) => {
                    let _ = set(event, "returned", v.clone());
                }
                Err(Fault::Thrown(e)) => {
                    let _ = set(
                        event,
                        "thrown",
                        obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    );
                }
                _ => (),
            }
        }
    }
    fn source_spot_load_alias(
        &mut self,
        receiver: &V,
        key: V,
        extra: Option<V>,
    ) -> R<(V, Option<String>)> {
        let id = spot_handle(receiver)?;
        let mut args = vec![receiver.clone(), key.clone()];
        if let Some(extra) = extra {
            args.push(extra);
        }
        let trace = self.source_spot_trace("Orderbook.loadOrderBook", args);
        let mut queue = self
            .queue_context
            .take()
            .ok_or(SourceRuleError::UnknownDependency)?;
        let loaded = self.source_spot_load_around_pending(&mut queue, id, key);
        self.harvest_queue(&mut queue);
        self.queue_context = Some(queue);
        let result = loaded.and_then(|(result, alias)| Ok((outcome(result)?, alias)));
        let plain = match &result {
            Ok((v, _)) => Ok(v.clone()),
            Err(Fault::Thrown(e)) => Err(Fault::Thrown(e.clone())),
            Err(Fault::Local(e)) => Err(Fault::Local(e.clone())),
        };
        self.source_spot_trace_end(trace, &plain);
        result
    }
    fn source_spot_cache(&mut self, receiver: &V, key: &str, book: V) -> R<()> {
        let queue = self
            .queue_context
            .as_mut()
            .ok_or(SourceRuleError::UnknownDependency)?;
        queue
            .set_cached_book_unverified(spot_handle(receiver)?, key, book)
            .map_err(Fault::Local)
    }
    fn source_spot_rpc(&mut self, method: &str, a: &[V]) -> R<V> {
        if self.globals.get("sourceSpotRpcDocuments") != V::Bool(true) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let (collection, field, document, args) = if method == "getRawTransaction" {
            (
                "sourceRpcTransactions",
                "txid",
                "transaction",
                vec![arg(a, 0), V::Bool(true)],
            )
        } else {
            ("sourceRpcBlocks", "hash", "block", vec![arg(a, 0)])
        };
        let value = self
            .rows(collection)
            .iter()
            .find(|r| spot_strict(&r.get(field), &arg(a, 0)))
            .map_or(V::Undefined, |r| r.get(document));
        self.emit(obj(&[
            ("kind", s("rpc")),
            (
                "method",
                s(if method == "getRawTransaction" {
                    "Client.getRawTransaction"
                } else {
                    "Client.getBlock"
                }),
            ),
            ("args", V::Array(args)),
            ("returned", value.clone()),
        ]));
        if method == "getRawTransaction" {
            return Ok(value);
        }
        // Original getBlockHeight catches a missing/invalid block receiver.
        match funding_prop(&value, "height") {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn source_spot_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        if method == "onChainTokenToToken" {
            let pair = s(format!(
                "{}-{}",
                source_channel_string(&arg(a, 1))?,
                source_channel_string(&arg(a, 2))?
            ));
            let tx = self.call("TxUtils.getRawTransaction", vec![arg(a, 5)])?;
            let confirmed = self.call(
                "TxUtils.getBlockHeight",
                vec![funding_prop(&tx, "blockhash")?],
            )?;
            let stop = arg(a, 7);
            let mut post = arg(a, 8);
            if stop == V::Bool(true) && post == V::Bool(true) {
                post = V::Bool(false);
            }
            let order = obj(&[
                ("offeredPropertyId", arg(a, 1)),
                ("desiredPropertyId", arg(a, 2)),
                ("amountOffered", arg(a, 3)),
                ("amountExpected", arg(a, 4)),
                ("blockTime", confirmed),
                ("sender", arg(a, 0)),
                ("stop", stop),
                ("post", post),
            ]);
            self.call(
                "Orderbook.queueOnChainTokenOrder",
                vec![pair, arg(a, 0), order.clone(), arg(a, 6), arg(a, 5)],
            )?;
            return Ok(order);
        }
        if method != "cancelOrder" {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let from = arg(a, 0);
        let contract = spot_loose_bool(&arg(a, 1), true)?;
        let token = spot_loose_bool(&arg(a, 1), false)?;
        let offered = arg(a, 2);
        let desired = arg(a, 3);
        let all = arg(a, 4);
        let params = arg(a, 5);
        let block = arg(a, 6);
        let key = if contract {
            offered.clone()
        } else if token {
            s(format!(
                "{}-{}",
                source_channel_string(&offered)?,
                source_channel_string(&desired)?
            ))
        } else {
            V::Undefined
        };
        let receiver = self.call("Orderbook.newInstance", vec![key])?;
        let mut cancelled = V::Array(vec![]);
        if contract {
            if truth(&all) {
                cancelled = self.call(
                    "Orderbook.cancelAllContractOrders",
                    vec![
                        receiver.clone(),
                        from.clone(),
                        offered.clone(),
                        block.clone(),
                    ],
                )?;
            }
            if truth(&funding_prop(&params, "txid")?) {
                cancelled = self.call(
                    "Orderbook.cancelContractOrderByTxid",
                    vec![
                        receiver.clone(),
                        from.clone(),
                        offered.clone(),
                        params.get("txid"),
                        block.clone(),
                    ],
                )?;
            }
            if truth(&funding_prop(&params, "price")?) {
                if truth(&params.get("buy")) {
                    cancelled = self.call(
                        "Orderbook.cancelContractBuyOrdersByPrice",
                        vec![
                            receiver.clone(),
                            from.clone(),
                            offered.clone(),
                            params.get("price"),
                            params.get("buy"),
                            block.clone(),
                        ],
                    )?;
                }
                if truth(&params.get("sell")) {
                    return Err(type_error(
                        "orderbook.cancelContractSellOrdersByPrice is not a function",
                    ));
                }
            }
            self.call(
                "Orderbook.saveOrderBook",
                vec![receiver, s(source_channel_string(&offered)?)],
            )?;
        } else if token {
            if truth(&all) && truth(&offered) && truth(&desired) {
                cancelled = self.call(
                    "Orderbook.cancelAllTokenOrders",
                    vec![
                        receiver.clone(),
                        from.clone(),
                        offered.clone(),
                        desired.clone(),
                        block.clone(),
                    ],
                )?;
            }
            if truth(&funding_prop(&params, "txid")?) {
                return Err(type_error(
                    "orderbook.cancelTokenOrdersByTxid is not a function",
                ));
            }
            if (truth(&funding_prop(&params, "price")?)
                || truth(&funding_prop(&params, "orderType")?))
                && truth(&params.get("price"))
            {
                let _ = self.call(
                    "Orderbook.cancelTokenSellOrdersByPrice",
                    vec![
                        receiver.clone(),
                        from.clone(),
                        offered.clone(),
                        desired.clone(),
                        params.get("price"),
                        block.clone(),
                    ],
                )?;
                cancelled = self.call(
                    "Orderbook.cancelTokenBuyOrdersByPrice",
                    vec![
                        receiver.clone(),
                        from,
                        offered.clone(),
                        desired.clone(),
                        params.get("price"),
                        block,
                    ],
                )?;
            }
            self.call(
                "Orderbook.saveOrderBook",
                vec![
                    receiver,
                    s(format!(
                        "{}-{}",
                        source_channel_string(&offered)?,
                        source_channel_string(&desired)?
                    )),
                ],
            )?;
        }
        Ok(cancelled)
    }
    fn source_spot_settle_matches(
        &mut self,
        receiver: &V,
        matches: &mut V,
        block: V,
        txid: V,
    ) -> R<V> {
        let trace = self.source_spot_trace(
            "Orderbook.processTokenMatches",
            vec![
                receiver.clone(),
                matches.clone(),
                block.clone(),
                txid.clone(),
                V::Bool(false),
            ],
        );
        // Reuse the original detailed evaluator once. Its mutated arguments
        // retain source orderRole tags in addTokenOrder's returned matchResult.
        let result = (|| -> R<V> {
            let evaluation = crate::token_settlement_source::evaluate_token_settlement(
                &[matches.clone(), block, txid, V::Bool(false)],
                false,
                self,
            )?;
            *matches = arg(&evaluation.arguments_after, 0);
            outcome(evaluation.outcome)
        })();
        self.source_spot_trace_end(trace, &result);
        result
    }
    fn source_spot_add_token_order(&mut self, a: &[V]) -> R<V> {
        let mut order = arg(a, 0);
        let block = arg(a, 1);
        let txid = arg(a, 2);
        self.call(
            "Tally.updateBalance",
            vec![
                funding_prop(&order, "sender")?,
                funding_prop(&order, "offeredPropertyId")?,
                n(-spot_number(&funding_prop(&order, "amountOffered")?)?),
                funding_prop(&order, "amountOffered")?,
                n(0.),
                n(0.),
                s("tokenOrder"),
                block.clone(),
            ],
        )?;
        let key = spot_key(
            &funding_prop(&order, "offeredPropertyId")?,
            &funding_prop(&order, "desiredPropertyId")?,
        )?;
        let receiver = self.call("Orderbook.newInstance", vec![key.clone()])?;
        let (mut book, mut alias) =
            self.source_spot_load_alias(&receiver, key.clone(), Some(V::Bool(false)))?;
        let price = self.call(
            "Orderbook.calculatePrice",
            vec![
                receiver.clone(),
                funding_prop(&order, "amountOffered")?,
                funding_prop(&order, "amountExpected")?,
            ],
        )?;
        set(&mut order, "price", price)?;
        let V::String(txid) = txid else {
            return Err(type_error("txid.slice is not a function"));
        };
        let units: Vec<u16> = txid.encode_utf16().collect();
        let shortened = units
            .iter()
            .take(3)
            .chain(units.iter().skip(units.len().saturating_sub(4)))
            .copied()
            .collect::<Vec<_>>();
        set(
            &mut order,
            "txid",
            s(String::from_utf16(&shortened)
                .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?),
        )?;
        let sell = V::Bool(
            spot_compare(
                &order.get("offeredPropertyId"),
                &order.get("desiredPropertyId"),
            )? == Some(core::cmp::Ordering::Less),
        );
        let trace = self.source_spot_trace(
            "Orderbook.insertOrder",
            vec![
                receiver.clone(),
                order.clone(),
                book.clone(),
                sell.clone(),
                V::Bool(false),
            ],
        );
        let was_string = matches!(book, V::String(_));
        let inserted = crate::contract_placement_source::insert_order(
            &order,
            &mut book,
            &sell,
            &V::Bool(false),
        )
        .map_err(Fault::Local)
        .and_then(outcome);
        self.source_spot_trace_end(trace, &inserted);
        let inserted = inserted?;
        if was_string || !truth(&inserted) {
            alias = None;
        }
        if inserted == V::Undefined {
            book = V::Undefined;
        }
        if let Some(key) = &alias {
            self.source_spot_cache(&receiver, key, book.clone())?;
        }
        let trace = self.source_spot_trace(
            "Orderbook.matchTokenOrders",
            vec![receiver.clone(), book.clone()],
        );
        let tick = self
            .queue_context
            .as_ref()
            .ok_or(SourceRuleError::UnknownDependency)?
            .instance_value(spot_handle(&receiver)?)?
            .get("tickSize");
        let matched = spot_match_token_book(&mut book, &tick);
        if let Some(key) = &alias {
            self.source_spot_cache(&receiver, key, book.clone())?;
        }
        self.source_spot_trace_end(trace, &matched);
        let mut matched = matched?;
        let mut matches = funding_prop(&matched, "matches")?;
        if matches!(matches,V::Array(ref rows)if !rows.is_empty()) {
            self.source_spot_settle_matches(&receiver, &mut matches, block, arg(a, 2))?;
            set(&mut matched, "matches", matches)?;
        }
        self.call(
            "Orderbook.saveOrderBook",
            vec![receiver, funding_prop(&matched, "orderBook")?, key],
        )?;
        Ok(matched)
    }
    fn source_spot_add_token_in_queue(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        a: &[V],
    ) -> Result<DependencyReturn, SourceRuleError> {
        if self.queue_context.is_some() {
            return Err(SourceRuleError::Backend);
        }
        self.harvest_queue(queue);
        self.queue_context = Some(std::mem::replace(queue, ContractQueueSourceOwner::new()));
        let trace = self.source_spot_trace(
            "Orderbook.addTokenOrder",
            vec![arg(a, 3), arg(a, 0), arg(a, 1), arg(a, 2)],
        );
        let result = self.source_spot_add_token_order(a);
        self.source_spot_trace_end(trace, &result);
        *queue = self.queue_context.take().ok_or(SourceRuleError::Backend)?;
        self.harvest_queue(queue);
        returned(result)
    }
    fn source_spot_queue_capability(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.harvest_queue(queue);
        if call.target == "Orderbook.addTokenOrder" {
            return self.source_spot_add_token_in_queue(queue, &call.arguments);
        }
        if call.target == "Orderbook.addContractOrder" {
            let mut backend = PlacementBackend { core: self };
            return returned_source(crate::contract_placement_source::add_contract_order(
                queue,
                &call.arguments,
                &mut backend,
            )?);
        }
        if matches!(
            call.target.as_str(),
            "Orderbook.processContractMatches" | "Orderbook.processTokenMatches"
        ) {
            return self.source_channel_queue_capability(queue, call);
        }
        self.invoke(call)
    }
    fn source_spot_cancel_criteria(&mut self, receiver: &V, a: &[V]) -> R<V> {
        let key = arg(a, 1);
        let criteria = arg(a, 2);
        let token = arg(a, 3);
        let amm = arg(a, 4);
        let (mut book, alias) = self.source_spot_load_alias(receiver, key.clone(), None)?;
        if matches!(book, V::Undefined | V::Null) {
            return Ok(V::Array(vec![]));
        }
        let result = (|| -> R<V> {
            let mut cancelled = vec![];
            let mut reserve = n(0.);
            if truth(&amm) {
                for side in ["buy", "sell"] {
                    let mut rows = array(&funding_prop(&book, side)?)?;
                    for i in (0..rows.len()).rev() {
                        if spot_strict(&funding_prop(&rows[i], "sender")?, &s("amm")) {
                            cancelled.push(rows.remove(i));
                            set(&mut book, side, V::Array(rows.clone()))?;
                        }
                    }
                }
            } else if !matches!(funding_prop(&criteria, "txid")?, V::Undefined | V::Null) {
                if spot_loose_bool(&criteria.get("buy"), true)? {
                    let mut rows = array(&funding_prop(&book, "buy")?)?;
                    for i in (0..rows.len()).rev() {
                        if spot_strict(&funding_prop(&rows[i], "txid")?, &criteria.get("txid")) {
                            cancelled.push(rows.remove(i));
                            set(&mut book, "buy", V::Array(rows.clone()))?;
                        }
                    }
                }
                if spot_loose_bool(&criteria.get("buy"), false)? {
                    let rows = array(&funding_prop(&book, "sell")?)?;
                    for i in (0..rows.len()).rev() {
                        if spot_strict(&funding_prop(&rows[i], "txid")?, &criteria.get("txid")) {
                            cancelled.push(rows[i].clone());
                            // Literal source bug: sell txid loop splices buy side.
                            let mut buy = array(&funding_prop(&book, "buy")?)?;
                            if i < buy.len() {
                                buy.remove(i);
                            }
                            set(&mut book, "buy", V::Array(buy))?;
                        }
                    }
                } else {
                    // This else belongs to `criteria.buy==false` INSIDE the txid
                    // guard in the pinned source. Price/address-only cancels skip it.
                    for side in ["buy", "sell"] {
                        let mut rows = array(&funding_prop(&book, side)?)?;
                        for i in (0..rows.len()).rev() {
                            let order = rows[i].clone();
                            let price = funding_prop(&criteria, "price")?;
                            let price_test = if !matches!(price, V::Undefined | V::Null) {
                                let cmp = spot_compare(&funding_prop(&order, "price")?, &price)?;
                                if truth(&criteria.get("buy")) {
                                    matches!(
                                        cmp,
                                        Some(
                                            core::cmp::Ordering::Less | core::cmp::Ordering::Equal
                                        )
                                    )
                                } else {
                                    matches!(
                                        cmp,
                                        Some(
                                            core::cmp::Ordering::Greater
                                                | core::cmp::Ordering::Equal
                                        )
                                    )
                                }
                            } else {
                                false
                            };
                            if price_test
                                || !matches!(criteria.get("address"), V::Undefined | V::Null)
                                    && spot_strict(
                                        &funding_prop(&order, "sender")?,
                                        &criteria.get("address"),
                                    )
                            {
                                cancelled.push(rows.remove(i));
                                set(&mut book, side, V::Array(rows.clone()))?;
                                reserve = spot_plus(
                                    &reserve,
                                    &funding_prop(
                                        &order,
                                        if spot_loose_bool(&token, true)? {
                                            "amountOffered"
                                        } else {
                                            "initMargin"
                                        },
                                    )?,
                                )?;
                            }
                        }
                    }
                }
            }
            // returnFromReserve is a non-index array property used only by source
            // diagnostics on these paths; its coercions above are still executed.
            let _ = reserve;
            Ok(V::Array(cancelled))
        })();
        // Parsed book objects are shared with the receiver cache before the
        // loop runs. Keep prior splices when a later source property read throws.
        if let Some(alias) = alias {
            self.source_spot_cache(receiver, &alias, book.clone())?;
        }
        let cancelled = result?;
        let key_text = source_channel_string(&key)?;
        self.source_spot_cache(receiver, &key_text, book.clone())?;
        self.call("Orderbook.saveOrderBook", vec![receiver.clone(), book, key])?;
        Ok(cancelled)
    }
    fn source_spot_orderbook(&mut self, method: &str, a: &[V]) -> R<V> {
        if a.is_empty() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let receiver = arg(a, 0);
        let original = &a[1..];
        spot_handle(&receiver)?;
        if method == "saveOrderBook" {
            return self.source_spot_save_around_pending(a);
        }
        if method == "calculatePrice" {
            return spot_price(&arg(original, 0), &arg(original, 1));
        }
        if method == "normalizeOrderBookKey" {
            return spot_key(&arg(original, 0), &arg(original, 1));
        }
        if method == "addTokenOrder" {
            return self.source_spot_add_token_order(original);
        }
        if method == "cancelOrdersByCriteria" {
            return self.source_spot_cancel_criteria(&receiver, original);
        }
        if method == "matchTokenOrders" {
            let mut book = arg(original, 0);
            let tick = self
                .queue_context
                .as_ref()
                .ok_or(SourceRuleError::UnknownDependency)?
                .instance_value(spot_handle(&receiver)?)?
                .get("tickSize");
            return spot_match_token_book(&mut book, &tick);
        }
        if method == "getOrdersForAddress" {
            if self.source_spot_main_active {
                return self.source_spot_orders_read_begin(&receiver, original);
            }
            let contract = arg(original, 1);
            let key = if truth(&contract) {
                s(source_channel_string(&contract)?)
            } else {
                s(format!(
                    "{}-{}",
                    source_channel_string(&arg(original, 2))?,
                    source_channel_string(&arg(original, 3))?
                ))
            };
            let result = (|| -> R<V> {
                let (book, _) = self.source_spot_load_alias(&receiver, key, None)?;
                let buy = funding_prop(&book, "buy")?;
                let sell = funding_prop(&book, "sell")?;
                if !truth(&buy) || !truth(&sell) {
                    return Ok(V::Array(vec![]));
                }
                let mut found = vec![];
                for rows in [array(&buy)?, array(&sell)?] {
                    for order in rows {
                        if spot_strict(&funding_prop(&order, "sender")?, &arg(original, 0)) {
                            found.push(order);
                        }
                    }
                }
                Ok(V::Array(found))
            })();
            return match result {
                Err(Fault::Thrown(_)) => Ok(V::Array(vec![])),
                other => other,
            };
        }
        let from = arg(original, 0);
        let offered = arg(original, 1);
        let is_token = method.starts_with("cancelToken") || method == "cancelAllTokenOrders";
        let key = if is_token {
            spot_key(&offered, &arg(original, 2))?
        } else {
            offered.clone()
        };
        let mut criteria = if method.starts_with("cancelAll") {
            obj(&[("address", from.clone())])
        } else if method.ends_with("ByTxid") {
            obj(&[("txid", arg(original, if is_token { 3 } else { 2 }))])
        } else {
            obj(&[
                ("price", arg(original, if is_token { 3 } else { 2 })),
                ("buy", V::Bool(method == "cancelTokenBuyOrdersByPrice")),
            ])
        };
        if method == "cancelAllTokenOrders" {
            set(
                &mut criteria,
                "buy",
                V::Bool(
                    spot_compare(&offered, &arg(original, 2))?
                        == Some(core::cmp::Ordering::Greater),
                ),
            )?;
        }
        let cancelled = self.call(
            "Orderbook.cancelOrdersByCriteria",
            vec![receiver, from.clone(), key, criteria],
        )?;
        if method == "cancelTokenOrderByTxid" {
            return Err(Fault::Thrown(crate::validity_source::SourceException {
                name: "ReferenceError".into(),
                message: "order is not defined".into(),
            }));
        }
        if method == "cancelContractOrderByTxid" {
            let rows = array(&cancelled)?;
            let first = arg(&rows, 0);
            let _ = self.call(
                "Contract.getInitialMargin",
                vec![offered.clone(), funding_prop(&first, "price")?],
            )?;
            let reserve = funding_prop(&first, "initMargin")?;
            let collateral = self.call("Contract.getCollateralId", vec![offered.clone()])?;
            self.call(
                "Tally.updateBalance",
                vec![
                    from,
                    collateral,
                    reserve.clone(),
                    n(-spot_number(&reserve)?),
                    n(0.),
                    n(0.),
                    s("contractCancel"),
                    arg(original, 3),
                ],
            )?;
        } else {
            let collateral = if !is_token {
                self.call("Contract.getCollateralId", vec![offered.clone()])?
            } else {
                offered.clone()
            };
            for order in array(&cancelled)? {
                let reserve = funding_prop(
                    &order,
                    if is_token {
                        "amountOffered"
                    } else {
                        "initMargin"
                    },
                )?;
                let reserve = if method == "cancelAllContractOrders" {
                    n(spot_parse_float(&reserve)?)
                } else {
                    reserve
                };
                self.call(
                    "Tally.updateBalance",
                    vec![
                        from.clone(),
                        collateral.clone(),
                        reserve.clone(),
                        n(-spot_number(&reserve)?),
                        n(0.),
                        n(0.),
                        s(if is_token {
                            "tokenCancel"
                        } else {
                            "contractCancel"
                        }),
                        arg(
                            original,
                            if is_token {
                                if method == "cancelAllTokenOrders" {
                                    3
                                } else {
                                    4
                                }
                            } else if method == "cancelAllContractOrders" {
                                2
                            } else {
                                4
                            },
                        ),
                    ],
                )?;
            }
        }
        Ok(cancelled)
    }
}
impl SourceContractRuntime {
    fn source_spot_raw_logic(&mut self, kind: &V, p: &V) -> Result<SourceOutcome, SourceRuleError> {
        let (method, fields): (&str, &[&str]) = if spot_strict(kind, &n(5.)) {
            (
                "onChainTokenToToken",
                &[
                    "senderAddress",
                    "propertyIdOffered",
                    "propertyIdDesired",
                    "amountOffered",
                    "amountExpected",
                    "txid",
                    "block",
                    "stop",
                    "post",
                ],
            )
        } else {
            (
                "cancelOrder",
                &[
                    "senderAddress",
                    "isContract",
                    "offeredPropertyId",
                    "desiredPropertyId",
                    "cancelAll",
                    "cancelParams",
                    "block",
                ],
            )
        };
        match self.invoke_dependency(&DependencyCall {
            target: format!("Logic.{method}"),
            arguments: fields.iter().map(|k| p.get(k)).collect(),
        })? {
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                Ok(SourceOutcome::Returned(V::Undefined))
            }
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
                Ok(SourceOutcome::Thrown(e))
            }
        }
    }
}
