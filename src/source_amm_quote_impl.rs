// V11 callable AMM math/coordinator. Every receiver, registry row and book is
// owned by the existing Core arena/cache/queue; this contains no balance store.
fn amm_quote_primitive(value: &V) -> R<V> {
    match value {
        V::BigNumber(raw) => {
            let canonical = crate::channels_source::source_big_number_text(value)?;
            // Number/default hints use genuine BigNumber.valueOf, which keeps
            // the minus sign of zero. String(BigNumber) instead drops it.
            Ok(s(
                if canonical == "0" && raw.trim_start().starts_with('-') {
                    "-0".to_string()
                } else {
                    canonical
                },
            ))
        }
        V::Object(_) | V::Array(_) | V::Promise => Ok(s(source_channel_string(value)?)),
        _ => Ok(value.clone()),
    }
}
fn amm_quote_number(value: &V) -> R<f64> {
    number(&amm_quote_primitive(value)?)
}
fn amm_quote_fixed(value: f64) -> R<f64> {
    if value != 0. && value.abs() < 0.000000005 {
        return Ok(if value.is_sign_negative() { -0. } else { 0. });
    }
    crate::amm::to_fixed_eight(value).map_err(|_| SourceRuleError::UnsupportedNumericDomain.into())
}
fn amm_quote_min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.min(right)
    }
}
fn amm_quote_max(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.max(right)
    }
}
impl ContractCore {
    // The pinned getter constructs once, awaits the genuine prototype loader,
    // then assigns its exact result to that receiver's cache. AMM source
    // observers wrap that nested loader; the older compound Queue getter hides
    // its invocation. Lower these AMM callsites through existing owner methods.
    fn source_amm_observed_orderbook_getter(&mut self, key: V) -> R<V> {
        let receiver = self.call("Orderbook.newInstance", vec![key.clone()])?;
        let book = self.call(
            "Orderbook.loadOrderBook",
            vec![receiver.clone(), key.clone()],
        )?;
        self.call("Orderbook.setCachedBook", vec![receiver.clone(), key, book])?;
        Ok(receiver)
    }

    fn source_amm_quote(&self, id: usize, a: &[V]) -> R<V> {
        let mid = amm_quote_number(&amm_or_zero(arg(a, 0)))?;
        if !mid.is_finite() || mid <= 0. {
            return Ok(V::Array(vec![]));
        }
        let pool = &self.source_amm_instances[id];
        let max = amm_quote_number(&amm_or_zero(pool.get("maxQuoteSize")))?;
        let position = amm_quote_number(&amm_or_zero(pool.get("position")))?;
        let size = amm_quote_max(0., amm_quote_min(max, position));
        if size.is_nan() || size <= 0. {
            return Ok(V::Array(vec![]));
        }
        let spread = arg(a, 2);
        let spread = amm_quote_max(
            1.,
            amm_quote_number(&if truth(&spread) { spread } else { n(40.) })?,
        ) / 10000.;
        let bid = amm_quote_fixed(mid * (1. - spread))?;
        let ask = amm_quote_fixed(mid * (1. + spread))?;
        let contract = n(amm_quote_number(&amm_or_zero(pool.get("contractType")))?);
        let block = n(amm_quote_number(&amm_or_zero(arg(a, 1)))?);
        let mut orders = Vec::with_capacity(2);
        for (sell, price, side) in [(false, bid, "bid"), (true, ask, "ask")] {
            orders.push(obj(&[
                ("contractId", contract.clone()),
                ("amount", n(size)),
                ("price", n(price)),
                ("blockTime", block.clone()),
                ("sell", V::Bool(sell)),
                ("initMargin", n(0.)),
                ("sender", s("amm")),
                (
                    "txid",
                    s(format!("amm-{side}-{}-{}", text(&contract)?, text(&block)?)),
                ),
                ("isLiq", V::Bool(false)),
                ("reduce", V::Bool(false)),
                ("post", V::Bool(false)),
                ("stop", V::Bool(false)),
                ("initialReduce", V::Bool(false)),
            ]));
        }
        Ok(V::Array(orders))
    }
    fn source_amm_rebalance(&mut self, block: V) -> R<V> {
        let series = self.call("Contract.loadContractSeries", vec![])?;
        if !truth(&series) {
            return Ok(V::Undefined);
        }
        // The existing registry owner represents its actual Map as entry pairs.
        for pair in array(&series)? {
            let pair = array(&pair)?;
            let id = arg(&pair, 0);
            self.source_amm_returned_origin = None;
            let pool = match self.call("Contract.getAMM", vec![id.clone()]) {
                Err(Fault::Thrown(_)) => continue,
                Err(error) => return Err(error),
                Ok(pool) => pool,
            };
            if !truth(&pool) {
                continue;
            }
            let instance = self
                .source_amm_returned_origin
                .take()
                .ok_or(SourceRuleError::Backend)?;
            let position = amm_quote_number(&amm_or_zero(pool.get("position")))?;
            if position.is_nan() || position <= 0. {
                continue;
            }
            let mark = self.call("Clearing.getIndexPrice", vec![id.clone(), block.clone()])?;
            if !amm_quote_number(&mark)?.is_finite() || amm_quote_number(&mark)? <= 0. {
                continue;
            }
            let key = s(source_channel_string(&id)?);
            let receiver = self.source_amm_observed_orderbook_getter(key.clone())?;
            self.call(
                "Orderbook.cancelOrdersByCriteria",
                vec![
                    receiver.clone(),
                    s("amm"),
                    key.clone(),
                    obj(&[]),
                    V::Bool(false),
                    V::Bool(true),
                ],
            )?;
            let mut book = self.call(
                "Orderbook.loadOrderBook",
                vec![receiver.clone(), key.clone(), V::Bool(false)],
            )?;
            let orders = self.call(
                "AMM.quoteContractOrders",
                vec![amm_handle(instance), mark, block.clone()],
            )?;
            for order in array(&orders)? {
                let sell = V::Bool(truth(&order.get("sell")));
                book = self.call(
                    "Orderbook.insertOrder",
                    vec![receiver.clone(), order, book, sell, V::Bool(false)],
                )?;
            }
            self.call("Orderbook.saveOrderBook", vec![receiver, book, key])?;
        }
        Ok(V::Undefined)
    }
    fn source_amm_static_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "updateOrdersForAllContractAMMs" => self.source_amm_rebalance(arg(a, 0)),
            "calculateOrderSize" => {
                let distance = 0.2 * amm_quote_number(&arg(a, 1))?;
                let ratio = amm_quote_number(&arg(a, 0))? / distance;
                let part = if ratio <= 0.05 {
                    0.35
                } else if ratio <= 0.1 {
                    0.25
                } else if ratio <= 0.15 {
                    0.15
                } else {
                    0.05
                };
                Ok(n(amm_quote_number(&arg(a, 2))? * part))
            }
            "calculateRedemptionValue" => {
                if truth(&arg(a, 1)) {
                    let pool = arg(a, 2);
                    let collateral = amm_quote_number(&funding_prop(&pool, "collateralId")?)?
                        * amm_quote_number(&funding_prop(&pool, "price")?)?;
                    let tokens = funding_prop(&pool, "tokens")?;
                    let value = source_js_plus(&amm_quote_primitive(&tokens)?, &n(collateral))?;
                    Ok(n((amm_quote_number(&arg(a, 0))?
                        / amm_quote_number(&tokens)?)
                        * amm_quote_number(&value)?))
                } else {
                    Ok(n(
                        amm_quote_number(&arg(a, 0))? * amm_quote_number(&arg(a, 3))?
                    ))
                }
            }
            "generateOrders" => {
                // The source class has no declared static contractType. Neither
                // constructing instances nor the owned APIs assigns that field.
                if spot_loose_bool(&arg(a, 6), true)? {
                    return Err(Fault::Thrown(crate::validity_source::SourceException {
                        name: "ReferenceError".into(),
                        message: "Cannot access 'totalLiquidity' before initialization".into(),
                    }));
                }
                match self.source_amm_static_get("contractType") {
                    V::String(kind) if kind == "inverse" => self.call(
                        "AMMStatic.generateOrdersForInverse",
                        a[..3.min(a.len())].to_vec(),
                    ),
                    V::String(kind) if kind == "linear" => self.call(
                        "AMMStatic.generateOrdersForLinear",
                        a[..3.min(a.len())].to_vec(),
                    ),
                    _ => Err(error("Invalid contract type")),
                }
            }
            "generateTokenOrders" => {
                if self.globals.get("txid") != V::Undefined {
                    return Err(SourceRuleError::UnknownDependency.into());
                }
                let pair = s(format!(
                    "{}-{}",
                    source_channel_string(&arg(a, 0))?,
                    source_channel_string(&arg(a, 1))?
                ));
                self.source_amm_observed_orderbook_getter(pair.clone())?;
                // Retain the source's Number conversion exceptions before its
                // loop. Its free txid identifier is unbound in this profile;
                // every attempted Promise.all then catches that ReferenceError
                // before either addTokenOrder body can execute.
                let _distance = 0.30 * amm_quote_number(&arg(a, 4))?;
                let orders = amm_quote_number(&arg(a, 3))?;
                let _liquidity = amm_quote_number(&arg(a, 2))?;
                if orders == f64::INFINITY || orders > 1024. {
                    return Err(SourceRuleError::Capacity.into());
                }
                self.source_amm_record_token_failures(&pair, orders)?;
                Ok(V::Undefined)
            }
            "insertCapital"
            | "redeemCapital"
            | "requestLiquidity"
            | "runOptionStrategy"
            | "buildStraddle"
            | "buildIronFly"
            | "buildCalendar"
            | "calculateTotalPosition"
            | "getLPAddresses"
            | "getAMMOrdersAndPositions"
            | "generateOrdersForInverse"
            | "generateOrdersForLinear" => self.source_amm_remaining_call(method, a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
