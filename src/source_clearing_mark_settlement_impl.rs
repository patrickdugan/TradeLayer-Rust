// Clearing's block-level marks, three passes, IOU and insurance orchestration.
impl ContractCore {
    fn source_clearing_merge(&mut self, receiver: V, key: &str, contract: V) -> R<V> {
        let ids = self.source_clearing_context_ids(key)?;
        let positions = self.source_clearing_context_positions(key)?;
        self.clearing_contexts.retain(|(k, _)| k != key);
        let mut receiver = receiver;
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
        self.call(
            "Margin.mergePositions",
            vec![receiver, positions.clone(), contract, V::Bool(true)],
        )?;
        Ok(positions)
    }
    fn source_clearing_options(&mut self, a: &[V]) -> R<V> {
        let series = arg(a, 0);
        let address = arg(a, 1);
        let receiver = self.call("Margin.getInstance", vec![series.clone()])?;
        let pos = self.margins.inspect_position(
            number(&receiver.get("sourceMarginInstance"))? as u64,
            &address,
        )?;
        let mut bag = match pos.get("options") {
            V::Object(f) => V::Object(f),
            _ => obj(&[]),
        };
        let numeric = number(&series)?;
        let alt = if numeric.is_finite() {
            s(text(&n(numeric))?)
        } else {
            series.clone()
        };
        if !same(&alt, &series) {
            let receiver = self.call("Margin.getInstance", vec![alt])?;
            let alt = self.margins.inspect_position(
                number(&receiver.get("sourceMarginInstance"))? as u64,
                &address,
            )?;
            if let V::Object(entries) = alt.get("options") {
                for (ticker, o) in entries {
                    let cur = bag.get(&ticker);
                    set(
                        &mut bag,
                        &ticker,
                        obj(&[
                            (
                                "contracts",
                                n(number(&funding_or(cur.get("contracts")))?
                                    + number(&funding_or(o.get("contracts")))?),
                            ),
                            (
                                "avgPrice",
                                n(number(&nullish(
                                    o.get("avgPrice"),
                                    nullish(cur.get("avgPrice"), n(0.0)),
                                ))?),
                            ),
                            (
                                "margin",
                                n(number(&funding_or(cur.get("margin")))?
                                    + number(&funding_or(o.get("margin")))?),
                            ),
                        ]),
                    )?;
                }
            }
        }
        let info = self.call("Contract.getContractInfo", vec![series])?;
        let vol = number(&funding_or(info.get("volAnnual")))?;
        let bpd = number(&if truth(&arg(a, 4)) {
            arg(a, 4)
        } else {
            n(144.0)
        })?;
        let bpd = if bpd.is_nan() { f64::NAN } else { bpd.max(1.0) };
        let spot = dec(&n(number(&funding_or(arg(a, 2)))?))?;
        let current = number(&funding_or(arg(a, 3)))?;
        let mut premium = FundingBn::new(&n(0.0))?;
        let mut intrinsic = FundingBn::new(&n(0.0))?;
        let mut legs = vec![];
        if let V::Object(entries) = bag {
            for (ticker, o) in entries {
                let Some(meta) = crate::options::parse_ticker(&ticker) else {
                    continue;
                };
                let qty = FundingBn::new(&funding_or(o.get("contracts")))?;
                if qty.zero() {
                    continue;
                }
                let expiry = if meta.expiry_block == 0.0 || meta.expiry_block.is_nan() {
                    0.0
                } else {
                    meta.expiry_block
                };
                let strike = dec(&n(meta.strike.unwrap_or(0.0)))?;
                let days = (expiry - current).max(0.0) / bpd;
                let typ = meta.option_type.as_source_name();
                let px = crate::options::price_eu_approx(typ, &spot, &strike, vol, days)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                let iv = crate::options::intrinsic(typ, &strike, &spot)
                    .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
                premium = premium.op(
                    FundingBn::new(&V::BigNumber(px.to_string()))?.op(qty.clone(), 2)?,
                    0,
                )?;
                intrinsic = intrinsic.op(
                    FundingBn::new(&V::BigNumber(iv.to_string()))?.op(qty.clone(), 2)?,
                    0,
                )?;
                legs.push(crate::options::OptionPosition {
                    option_type: typ.into(),
                    expiry_block: expiry,
                    strike,
                    qty: dec(&qty.value())?,
                });
            }
        }
        let maintenance = crate::options::portfolio_maintenance(&legs, &spot)
            .map_err(|_| SourceRuleError::UnsupportedNumericDomain)?;
        Ok(obj(&[
            ("premiumMTM", premium.value()),
            ("intrinsicNet", intrinsic.value()),
            ("maintNaked", V::BigNumber(maintenance.to_string())),
            ("optionCount", n(legs.len() as f64)),
        ]))
    }
    fn source_clearing_update_maps(&mut self, a: &[V]) -> R<V> {
        let block = arg(a, 0);
        let contract = arg(a, 1);
        let collateral = arg(a, 2);
        let inverse = arg(a, 3);
        let notional = arg(a, 4);
        let price_info = arg(a, 5);
        let receiver = self.call("Margin.getInstance", vec![contract.clone()])?;
        let raw = self.call(
            "Margin.getAllPositions",
            vec![receiver.clone(), contract.clone()],
        )?;
        let key = text(&self.call(
            "Clearing.initPositionCache",
            vec![contract.clone(), block.clone(), raw],
        )?)?;
        let positions = self.source_clearing_context_ids(&key)?;
        let zero = || FundingBn::new(&n(0.0));
        let empty = |positions| {
            Ok(obj(&[
                ("positions", positions),
                ("liqEvents", V::Array(vec![])),
                ("systemicLoss", V::BigNumber("0".into())),
                ("pnlDelta", V::BigNumber("0".into())),
            ]))
        };
        if positions.is_empty() {
            self.clearing_contexts.retain(|(k, _)| k != &key);
            return empty(V::Array(vec![]));
        }
        let last = FundingBn::new(&nullish(price_info.get("lastPrice"), n(0.0)))?;
        let mut current = FundingBn::new(&nullish(price_info.get("thisPrice"), n(0.0)))?;
        if last.compare(&zero()?) != Some(core::cmp::Ordering::Greater) {
            let out = self.source_clearing_merge(receiver, &key, contract)?;
            return empty(out);
        }
        if current.compare(&zero()?) != Some(core::cmp::Ordering::Greater) {
            current = last.clone();
        }
        let mut systemic = zero()?;
        let mut positive = zero()?;
        let mut negative = zero()?;
        let orderbook = self.call("Orderbook.getOrderbookInstance", vec![contract.clone()])?;
        let mut liquidations = vec![];
        for id in &positions {
            let mut pos = self.margins.inspect_position_object(*id)?;
            if !truth(&pos) || !truth(&pos.get("contracts")) || same(&pos.get("contracts"), &n(0.0))
            {
                continue;
            }
            let address = pos.get("address");
            let tally = self.call("Tally.getTally", vec![address.clone(), collateral.clone()])?;
            let pnl = FundingBn::new(&self.call(
                "Clearing.calculateClearingPNL",
                vec![obj(&[
                    ("oldContracts", pos.get("contracts")),
                    ("previousMarkPrice", last.value()),
                    ("currentMarkPrice", current.value()),
                    ("inverse", inverse.clone()),
                    ("notional", notional.clone()),
                ])],
            )?)?;
            if pnl.zero() {
                continue;
            }
            if pnl.compare(&zero()?) == Some(core::cmp::Ordering::Greater) {
                set(&mut pos, "_wasProfitable", V::Bool(true))?;
                self.margins.replace_position_object(*id, pos)?;
                continue;
            }
            let loss = pnl.abs();
            let available = FundingBn::new(&funding_or(tally.get("available")))?;
            let margin = FundingBn::new(&funding_or(tally.get("margin")))?;
            let opt = self.call(
                "Clearing.computeOptionAdjustments",
                vec![
                    contract.clone(),
                    address.clone(),
                    current.value(),
                    block.clone(),
                    n(144.0),
                ],
            )?;
            let parts = self.call(
                "Clearing.computeLossCoverage",
                vec![available.value(), margin.value(), opt.clone()],
            )?;
            let coverage = FundingBn::new(&parts.get("coverage"))?;
            negative = negative.op(loss.clone(), 0)?;
            if coverage
                .compare(&loss)
                .is_some_and(|o| o != core::cmp::Ordering::Less)
            {
                let take = source_liq_bn_min(available, loss.clone());
                let margin = loss.clone().op(take.clone(), 1)?;
                self.call(
                    "Tally.updateBalance",
                    vec![
                        address,
                        collateral.clone(),
                        n(take.negated().number()),
                        n(0.0),
                        n(margin.negated().number()),
                        n(0.0),
                        s("clearingLoss"),
                        block.clone(),
                    ],
                )?;
                self.call(
                    "PnlIou.addLoss",
                    vec![
                        contract.clone(),
                        collateral.clone(),
                        n(loss.number()),
                        block.clone(),
                    ],
                )?;
            } else {
                let shortfall = loss.clone().op(coverage.clone(), 1)?;
                liquidations.push((*id, address, loss, coverage, shortfall));
            }
        }
        let mut events = vec![];
        for (id, address, loss, coverage, shortfall) in liquidations {
            let tally = self.call("Tally.getTally", vec![address.clone(), collateral.clone()])?;
            let typ = if coverage.compare(&zero()?) == Some(core::cmp::Ordering::Greater) {
                "partial"
            } else {
                "total"
            };
            let liq = self.call(
                "Clearing.handleLiquidation",
                vec![
                    s(&key),
                    orderbook.clone(),
                    obj(&[("sourceTally", V::Bool(true))]),
                    self.margins.inspect_position_object(id)?,
                    contract.clone(),
                    block.clone(),
                    inverse.clone(),
                    collateral.clone(),
                    s(typ),
                    n(shortfall.number()),
                    notional.clone(),
                    last.value(),
                    V::Bool(true),
                    n(shortfall.number()),
                    tally,
                    price_info.clone(),
                ],
            )?;
            if !truth(&liq) {
                continue;
            }
            systemic = systemic.op(FundingBn::new(&funding_or(liq.get("systemicLoss")))?, 0)?;
            events.push(obj(&[
                ("address", address),
                ("liquidationType", s(typ)),
                ("shortfall", n(shortfall.number())),
                ("coverage", n(coverage.number())),
                ("loss", n(loss.number())),
                ("systemicLoss", liq.get("systemicLoss")),
            ]));
        }
        for id in self.source_clearing_context_ids(&key)? {
            let mut pos = self.margins.inspect_position_object(id)?;
            if truth(&pos.get("contracts")) && !same(&pos.get("contracts"), &n(0.0)) {
                let profit = FundingBn::new(&self.call(
                    "Clearing.calculateClearingPNL",
                    vec![obj(&[
                        ("oldContracts", pos.get("contracts")),
                        ("previousMarkPrice", last.value()),
                        ("currentMarkPrice", current.value()),
                        ("inverse", inverse.clone()),
                        ("notional", notional.clone()),
                    ])],
                )?)?;
                if profit.compare(&zero()?) == Some(core::cmp::Ordering::Greater) {
                    positive = positive.op(profit.clone(), 0)?;
                    self.call(
                        "Tally.updateBalance",
                        vec![
                            pos.get("address"),
                            collateral.clone(),
                            n(profit.number()),
                            n(0.0),
                            n(0.0),
                            n(0.0),
                            s("clearingProfit"),
                            block.clone(),
                        ],
                    )?;
                    self.call(
                        "PnlIou.addProfit",
                        vec![
                            contract.clone(),
                            collateral.clone(),
                            n(profit.number()),
                            block.clone(),
                        ],
                    )?;
                }
            }
            if let V::Object(fields) = &mut pos {
                fields.retain(|(k, _)| k != "_wasProfitable");
            }
            self.margins.replace_position_object(id, pos)?;
        }
        let ids = self.source_clearing_context_ids(&key)?;
        for id in ids {
            let mut p = self.margins.inspect_position_object(id)?;
            if !same(&p.get("lastMark"), &current.value()) {
                set(&mut p, "lastMark", current.value())?;
                self.margins.replace_position_object(id, p)?;
            }
        }
        let delta = positive.op(negative.op(systemic.clone(), 1)?, 1)?;
        let positions = self.source_clearing_merge(receiver, &key, contract)?;
        Ok(obj(&[
            ("positions", positions),
            ("liqEvents", V::Array(events)),
            ("systemicLoss", systemic.value()),
            ("pnlDelta", delta.value()),
        ]))
    }
    fn source_clearing_settle_ious(&mut self, a: &[V]) -> R<V> {
        let doc = self.call("PnlIou.getDoc", vec![arg(a, 0), arg(a, 1)])?;
        if !truth(&doc) {
            return Ok(V::Undefined);
        }
        let loss = FundingBn::new(&funding_or(doc.get("blockLosses")))?;
        if loss
            .compare(&FundingBn::new(&n(0.0))?)
            .is_some_and(|o| o != core::cmp::Ordering::Greater)
        {
            return Ok(V::Undefined);
        }
        let allocations = self.call(
            "PnlIou.payOutstandingIous",
            vec![arg(a, 0), arg(a, 1), n(loss.number()), arg(a, 2)],
        )?;
        for allocation in array(&allocations)? {
            self.call(
                "Tally.updateBalance",
                vec![
                    allocation.get("address"),
                    arg(a, 1),
                    allocation.get("amount"),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s("iouPayout"),
                    arg(a, 2),
                    s(""),
                ],
            )?;
        }
        Ok(V::Undefined)
    }
    fn source_clearing_distribute_insurance(&mut self, a: &[V]) -> R<V> {
        let payout = arg(a, 2);
        if !truth(&payout) || number(&payout)? <= 0.0 {
            return Ok(V::Undefined);
        }
        let prefix = format!("{}:", text(&arg(a, 0))?);
        let entries = self
            .deleverage_trades
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        let mut total = FundingBn::new(&n(0.0))?;
        for (_, trades) in &entries {
            for t in trades {
                total = total.op(FundingBn::new(&funding_or(t.get("matchSize")))?, 0)?;
            }
        }
        if total
            .compare(&FundingBn::new(&n(0.0))?)
            .is_some_and(|o| o != core::cmp::Ordering::Greater)
        {
            return Ok(V::Undefined);
        }
        let mut distributed = FundingBn::new(&n(0.0))?;
        for (key, trades) in entries {
            let mut size = FundingBn::new(&n(0.0))?;
            for t in trades {
                size = size.op(FundingBn::new(&funding_or(t.get("matchSize")))?, 0)?;
            }
            if size
                .compare(&FundingBn::new(&n(0.0))?)
                .is_some_and(|o| o != core::cmp::Ordering::Greater)
            {
                continue;
            }
            let share = FundingBn::new(&payout)?.op(size, 2)?.op(total.clone(), 3)?;
            if share
                .compare(&FundingBn::new(&n(0.0))?)
                .is_some_and(|o| o != core::cmp::Ordering::Greater)
            {
                continue;
            }
            self.call(
                "Tally.updateBalance",
                vec![
                    s(key.split(':').nth(1).unwrap_or("")),
                    arg(a, 1),
                    share.value(),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s("insuranceDelev"),
                    arg(a, 3),
                    s(""),
                ],
            )?;
            distributed = distributed.op(share, 0)?;
        }
        let dust = FundingBn::new(&payout)?.op(distributed, 1)?;
        if dust.clone().abs().compare(&FundingBn::new(&n(0.0))?)
            == Some(core::cmp::Ordering::Greater)
        {
            self.call(
                "PnlIou.absorbDust",
                vec![arg(a, 0), arg(a, 1), dust.value(), arg(a, 3)],
            )?;
        }
        Ok(V::Undefined)
    }
    fn source_clearing_additional(&mut self, a: &[V]) -> R<V> {
        let loss = FundingBn::new(&arg(a, 4))?;
        if loss
            .compare(&FundingBn::new(&n(0.0))?)
            .is_some_and(|o| o != core::cmp::Ordering::Less)
        {
            let oracle = self.call("Contract.isOracleContract", vec![arg(a, 2)])?;
            let fund = self.call("Insurance.getInstance", vec![arg(a, 2), oracle])?;
            let payout = self.call(
                "Insurance.calcPayout",
                vec![fund, loss.abs().value(), arg(a, 0)],
            )?;
            if number(&payout)? > 0.0 {
                self.call(
                    "Clearing.distributeInsuranceProRataToDelev",
                    vec![arg(a, 2), arg(a, 5), payout, arg(a, 0)],
                )?;
            }
        }
        Ok(V::Undefined)
    }
    fn source_clearing_make_settlement(&mut self, a: &[V]) -> R<V> {
        let contracts = self.call("Contract.loadContractSeries", vec![])?;
        if !truth(&contracts) {
            return Ok(V::Undefined);
        }
        for entry in array(&contracts)? {
            let id = funding_prop(&arg(&array(&entry)?, 1), "id")?;
            let price = self.call(
                "Clearing.isPriceUpdatedForBlockHeight",
                vec![id.clone(), arg(a, 0)],
            )?;
            let this = funding_prop(&price, "thisPrice")?;
            self.call(
                "Clearing.pruneInstaLiqOrders",
                vec![this.clone(), arg(a, 0), id.clone()],
            )?;
            let collateral = self.call("Contract.getCollateralId", vec![id.clone()])?;
            self.call(
                "Clearing.settleIousForBlock",
                vec![id.clone(), collateral.clone(), arg(a, 0)],
            )?;
            if !truth(&price) || !truth(&price.get("updated")) {
                continue;
            }
            let inverse = self.call("Contract.isInverse", vec![id.clone()])?;
            let notional =
                self.call("Contract.getNotionalValue", vec![id.clone(), this.clone()])?;
            let result = self.call(
                "Clearing.updateMarginMaps",
                vec![
                    arg(a, 0),
                    id.clone(),
                    collateral.clone(),
                    inverse,
                    notional.get("notionalPerContract"),
                    price,
                ],
            )?;
            self.call(
                "Clearing.performAdditionalSettlementTasks",
                vec![
                    arg(a, 0),
                    result.get("positions"),
                    id,
                    this,
                    result.get("systemicLoss"),
                    collateral,
                    result.get("pnlDelta"),
                ],
            )?;
        }
        self.call("Clearing.resetBlockTrades", vec![])?;
        Ok(V::Undefined)
    }
}
