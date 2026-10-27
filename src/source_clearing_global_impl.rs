// Literal clearing orchestration over the sole source state owners.
impl ContractCore {
    fn source_clearing_state(&self) -> V {
        obj(&[
            (
                "contexts",
                V::Array(
                    self.clearing_contexts
                        .iter()
                        .map(|(k, ids)| {
                            V::Array(vec![
                                s(k),
                                V::Array(ids.iter().map(|id| n(*id as f64)).collect()),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "deleverageTrades",
                V::Array(
                    self.deleverage_trades
                        .iter()
                        .map(|(k, v)| V::Array(vec![s(k), V::Array(v.clone())]))
                        .collect(),
                ),
            ),
            (
                "liquidationRecords",
                V::Array(
                    self.liquidation_marks
                        .iter()
                        .map(|(k, v)| V::Array(vec![s(k), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "contractSeries",
                V::Array(
                    self.contract_series
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            (
                "pendingHistoryJobs",
                n(self.source_history_jobs.len() as f64),
            ),
            (
                "pendingSupplyDelta",
                V::Bool(self.source_supply_delta.is_some()),
            ),
            ("tallyMod", self.tally_mod.clone()),
            ("contractModRaw", self.contract_mod_raw.clone()),
            (
                "tallyStaticInitialized",
                V::Bool(self.tally_static_initialized),
            ),
            ("channels", self.channels.state().diagnostic_value()),
            ("channelsActions", n(self.channels.action_count() as f64)),
            (
                "propertyCache",
                V::Array(
                    self.property_cache
                        .iter()
                        .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                        .collect(),
                ),
            ),
            ("vaultInitialized", V::Bool(self.vault_initialized)),
            (
                "unhandledRejections",
                V::Array(self.source_unhandled_rejections.clone()),
            ),
        ])
    }
    fn source_clearing_load_contracts(&mut self) -> R<V> {
        let result = (|| {
            self.database("contractList")?;
            let rows = array(&self.db(
                "contractList",
                "findAsync",
                vec![obj(&[("type", s("contractSeries"))])],
            )?)?;
            let mut map = Vec::new();
            for row in rows {
                let key = row.get("id");
                let value = row.get("data");
                if let Some((_, v)) = map.iter_mut().find(|(k, _)| same(k, &key)) {
                    *v = value;
                } else {
                    map.push((key, value));
                }
            }
            self.contract_series = map;
            Ok(V::Array(
                self.contract_series
                    .iter()
                    .map(|(k, v)| V::Array(vec![k.clone(), v.clone()]))
                    .collect(),
            ))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            r => r,
        }
    }
    fn source_clearing_context_ids(&self, key: &str) -> R<Vec<usize>> {
        self.clearing_contexts
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| error(format!("No clearing context for {key}")))
    }
    fn source_clearing_context_positions(&self, key: &str) -> R<V> {
        Ok(V::Array(
            self.source_clearing_context_ids(key)?
                .iter()
                .map(|id| {
                    self.margins
                        .inspect_position_object(*id)
                        .map_err(Fault::Local)
                })
                .collect::<R<Vec<_>>>()?,
        ))
    }
    fn source_clearing_replace_cached(
        &mut self,
        key: &str,
        address: V,
        position: V,
        allow_insert: bool,
    ) -> R<()> {
        let ids = self.source_clearing_context_ids(key)?;
        let mut found = None;
        for (index, id) in ids.iter().enumerate() {
            if same(
                &self.margins.inspect_position_object(*id)?.get("address"),
                &address,
            ) {
                found = Some(index);
                break;
            }
        }
        if found.is_none() && !allow_insert {
            return Err(error(format!(
                "Position for {} not found in cache",
                text(&address)?
            )));
        }
        let position = match position {
            V::Object(_) => position,
            V::Null | V::Undefined => obj(&[]),
            _ => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
        };
        let id = self.margins.allocate_position_object(position)?;
        let entry = &mut self
            .clearing_contexts
            .iter_mut()
            .find(|(k, _)| k == key)
            .ok_or(SourceRuleError::Backend)?
            .1;
        if let Some(index) = found {
            entry[index] = id;
        } else {
            entry.push(id);
        }
        Ok(())
    }
    fn source_clearing_orderbook_call(&mut self, method: &str, a: &[V]) -> R<V> {
        let mut queue = self
            .queue_context
            .take()
            .ok_or(SourceRuleError::UnknownDependency)?;
        let result = (|| {
            if method == "newInstance" {
                let id = queue.create_instance(arg(a, 0), None)?;
                return Ok(obj(&[("orderbookInstance", n(id as f64))]));
            }
            let receiver = arg(a, 0).get("orderbookInstance");
            let id = if matches!(receiver, V::Number(_)) {
                Some(number(&receiver)? as u64)
            } else {
                None
            };
            let original = if id.is_some() { &a[1..] } else { a };
            if method == "inspectBook" {
                return Ok(queue
                    .instance_value(id.ok_or(SourceRuleError::InvalidInput)?)?
                    .get("orderBooks")
                    .get(&text(&arg(original, 0))?));
            }
            if method == "setCachedBook" {
                queue.set_cached_book_unverified(
                    id.ok_or(SourceRuleError::InvalidInput)?,
                    &text(&arg(original, 0))?,
                    arg(original, 1),
                )?;
                return Ok(V::Undefined);
            }
            if method == "insertOrder" || method == "matchContractOrders" {
                let mut book = arg(original, if method == "insertOrder" { 1 } else { 0 });
                let result = if method == "insertOrder" {
                    crate::contract_placement_source::insert_order(
                        &arg(original, 0),
                        &mut book,
                        &arg(original, 2),
                        &arg(original, 3),
                    )
                } else {
                    crate::contract_placement_source::match_contract_orders(&mut book, self)
                };
                let key = text(
                    &queue
                        .instance_value(id.ok_or(SourceRuleError::InvalidInput)?)?
                        .get("orderBookKey"),
                )?;
                queue.set_cached_book_unverified(id.unwrap(), &key, book)?;
                return outcome(result?);
            }
            if method == "processContractMatches" {
                let mut b = FinancialBackend {
                    core: self,
                    queue: &mut queue,
                };
                return outcome(crate::contract_settlement_source::process_contract_matches(
                    original, &mut b,
                )?);
            }
            if method == "addContractOrder" {
                let mut b = PlacementBackend { core: self };
                return outcome(crate::contract_placement_source::add_contract_order(
                    &mut queue, original, &mut b,
                )?);
            }
            let call = SourceCall {
                method: method.into(),
                arguments: original.to_vec(),
            };
            let result = if let Some(id) = id {
                queue.evaluate_instance(id, &call)
            } else {
                queue.evaluate_with_backend(&call, self)
            };
            outcome(result?)
        })();
        self.harvest_queue(&mut queue);
        self.queue_context = Some(queue);
        result
    }
    fn source_clearing_pnl(&self, params: V, new_contract: bool) -> R<V> {
        if matches!(params, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot destructure property '{}' of '{}' as it is {}.",
                if new_contract {
                    "newContracts"
                } else {
                    "oldContracts"
                },
                text(&params)?,
                text(&params)?
            )));
        }
        let zero = || FundingBn::new(&n(0.0));
        let size = FundingBn::new(&if truth(&params.get(if new_contract {
            "newContracts"
        } else {
            "oldContracts"
        })) {
            params.get(if new_contract {
                "newContracts"
            } else {
                "oldContracts"
            })
        } else {
            n(0.0)
        })?;
        if size.zero() {
            return Ok(zero()?.value());
        }
        let previous = params.get(if new_contract {
            "avgEntryPrice"
        } else {
            "previousMarkPrice"
        });
        let current = if new_contract {
            params.get("lastPrice")
        } else {
            nullish(
                params.get("currentMarkPrice"),
                if truth(&previous) {
                    previous.clone()
                } else {
                    n(0.0)
                },
            )
        };
        let last = FundingBn::new(&if truth(&previous) { previous } else { n(0.0) })?;
        let cur = FundingBn::new(&if new_contract && !truth(&current) {
            n(0.0)
        } else {
            current
        })?;
        if last.compare(&cur) == Some(core::cmp::Ordering::Equal) || (new_contract && last.zero()) {
            return Ok(zero()?.value());
        }
        let noto = FundingBn::new(&if truth(&params.get("notional")) {
            params.get("notional")
        } else {
            n(1.0)
        })?;
        let pnl = if truth(&params.get("inverse")) {
            if !new_contract && (last.zero() || cur.zero()) {
                return Ok(zero()?.value());
            }
            size.op(
                FundingBn::new(&n(1.0))?
                    .op(last, 3)?
                    .op(FundingBn::new(&n(1.0))?.op(cur, 3)?, 1)?,
                2,
            )?
            .op(noto, 2)?
        } else {
            size.op(cur.op(last, 1)?, 2)?.op(noto, 2)?
        };
        Ok(if pnl.finite() { pnl } else { zero()? }.value())
    }
    fn source_clearing_global_call(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "applyFundingRates"
            | "calculateFundingRate"
            | "getIndexPrice"
            | "clampFundingRate"
            | "applyFundingToPositions"
            | "processFundingPayments"
            | "saveFundingEvent"
            | "loadFundingEvents" => self.source_clearing_funding_call(method, a),
            "initPositionCache" => {
                let key = format!("{}:{}", text(&arg(a, 0))?, text(&arg(a, 1))?);
                let positions = match arg(a, 2) {
                    V::Array(v) => v,
                    _ => vec![],
                };
                let mut ids = Vec::new();
                for position in positions {
                    let cloned = parse(&json(&position)?)?;
                    ids.push(self.margins.allocate_position_object(cloned)?);
                }
                if let Some((_, v)) = self.clearing_contexts.iter_mut().find(|(k, _)| k == &key) {
                    *v = ids;
                } else {
                    self.clearing_contexts.push((key.clone(), ids));
                }
                Ok(s(key))
            }
            "getPositionsFromCache" => self.source_clearing_context_positions(&text(&arg(a, 0))?),
            "flushPositionCache" => {
                let key = text(&arg(a, 0))?;
                let out = self.source_clearing_context_positions(&key)?;
                self.clearing_contexts.retain(|(k, _)| k != &key);
                Ok(out)
            }
            "addOrUpdatePositionInCache" => {
                self.source_clearing_replace_cached(
                    &text(&arg(a, 0))?,
                    arg(a, 1),
                    arg(a, 2),
                    true,
                )?;
                Ok(V::Undefined)
            }
            "calculateClearingPNL" => self.source_clearing_pnl(arg(a, 0), false),
            "calculateNewContractPNL" => self.source_clearing_pnl(arg(a, 0), true),
            "computeLossCoverage" => {
                let avail = FundingBn::new(&if truth(&arg(a, 0)) { arg(a, 0) } else { n(0.0) })?;
                let mar = FundingBn::new(&if truth(&arg(a, 1)) { arg(a, 1) } else { n(0.0) })?
                    .op(FundingBn::new(&n(2.0))?, 3)?;
                let opt = arg(a, 2);
                let mtm = FundingBn::new(&if truth(&opt.get("premiumMTM")) {
                    opt.get("premiumMTM")
                } else {
                    n(0.0)
                })?;
                let maint = FundingBn::new(&if truth(&opt.get("maintNaked")) {
                    opt.get("maintNaked")
                } else {
                    n(0.0)
                })?;
                let coverage = avail
                    .op(mar.clone(), 0)?
                    .op(mtm.clone(), 0)?
                    .op(maint.clone(), 1)?;
                Ok(obj(&[
                    ("coverage", coverage.value()),
                    ("maintBase", mar.value()),
                    ("optionMTM", mtm.value()),
                    ("optionMaint", maint.value()),
                ]))
            }
            "recordDeleverageTrade" => {
                let key = format!("{}:{}", text(&arg(a, 0))?, text(&arg(a, 1))?);
                if let Some((_, v)) = self.deleverage_trades.iter_mut().find(|(k, _)| k == &key) {
                    v.push(arg(a, 2));
                } else {
                    self.deleverage_trades.push((key, vec![arg(a, 2)]));
                }
                Ok(V::Undefined)
            }
            "recordLiquidation" => {
                let key = format!("{}:{}", text(&arg(a, 0))?, text(&arg(a, 1))?);
                if let Some((_, v)) = self.liquidation_marks.iter_mut().find(|(k, _)| k == &key) {
                    *v = arg(a, 2);
                } else {
                    self.liquidation_marks.push((key, arg(a, 2)));
                }
                Ok(V::Undefined)
            }
            "getLiquidation" => Ok(self
                .liquidation_marks
                .iter()
                .find(|(k, _)| {
                    k == &format!(
                        "{}:{}",
                        text(&arg(a, 0)).unwrap_or_default(),
                        text(&arg(a, 1)).unwrap_or_default()
                    )
                })
                .map_or(V::Undefined, |(_, v)| v.clone())),
            "getDeleverageTradesThisBlock" => {
                let key = format!("{}:{}", text(&arg(a, 0))?, text(&arg(a, 1))?);
                Ok(V::Array(
                    self.deleverage_trades
                        .iter()
                        .find(|(k, _)| k == &key)
                        .map_or(vec![], |(_, v)| v.clone()),
                ))
            }
            "resetBlockTracking" | "resetBlockTrades" => {
                self.block_trades.clear();
                self.deleverage_trades.clear();
                self.liquidation_marks.clear();
                Ok(V::Undefined)
            }
            "handleLiquidation" | "computeLiquidationPriceFromLoss" => {
                self.source_clearing_liquidation_call(method, a)
            }
            "settleOptionExpiries"
            | "resolveOptionExerciseTarget"
            | "computeExerciseContracts"
            | "applyAssignedUnderlyingPosition" => self.source_clearing_expiry_call(method, a),
            "clearingFunction" => self.source_clearing_function(a),
            "verifyNetContracts" => self.source_clearing_verify_net(),
            "getTotalTokenBalances" => self.source_clearing_supply(a),
            "updateMarginMaps" => self.source_clearing_update_maps(a),
            "computeOptionAdjustments" => self.source_clearing_options(a),
            "makeSettlement" => self.source_clearing_make_settlement(a),
            "settleIousForBlock" => self.source_clearing_settle_ious(a),
            "performAdditionalSettlementTasks" => self.source_clearing_additional(a),
            "distributeInsuranceProRataToDelev" => self.source_clearing_distribute_insurance(a),
            "pruneInstaLiqOrders" => {
                let inverse = self.call("Contract.isInverse", vec![arg(a, 2)])?;
                let notional =
                    self.call("Contract.getNotionalValue", vec![arg(a, 2), arg(a, 0)])?;
                let receiver = self.call("Orderbook.getOrderbookInstance", vec![arg(a, 2)])?;
                let notional = nullish(
                    notional.get("notionalPerContract"),
                    nullish(notional.clone(), n(1.0)),
                );
                self.call(
                    "Orderbook._pruneInstaLiqOrdersFromFreshBook",
                    vec![receiver, arg(a, 0), arg(a, 1), arg(a, 2), notional, inverse],
                )
            }
            "getBalance" => Ok(V::Undefined),
            "saveClearingSettlementEvent" | "loadClearingSettlementEvents" => {
                Err(Fault::Thrown(crate::validity_source::SourceException {
                    name: "ReferenceError".into(),
                    message: "dbInstance is not defined".into(),
                }))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
