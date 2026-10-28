// Included in source_contract_runtime.rs. Literal active Logic channel methods
// (the later settleChannelPNL definition), not the standalone Scaling offset.
fn returned_source(value: SourceOutcome) -> Result<DependencyReturn, SourceRuleError> {
    Ok(match value {
        SourceOutcome::Returned(value) => DependencyReturn::Pending(value),
        SourceOutcome::Thrown(error) => DependencyReturn::PendingThrow(error),
    })
}
fn source_channel_column_write(channel: &mut V, column: &str, property: &str, value: V) -> R<()> {
    let mut balances = funding_prop(channel, column)?;
    set(&mut balances, property, value)?;
    set(channel, column, balances)
}
impl SourceContractRuntime {
    fn source_lifecycle_raw_logic(
        &mut self,
        kind: &V,
        p: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let (method, fields): (&str, &[&str]) =
            match number(kind).map_err(|_| SourceRuleError::InvalidInput)? as u32 {
                16 => (
                    "createContractSeries",
                    &[
                        "senderAddress",
                        "native",
                        "underlyingOracleId",
                        "onChainData",
                        "notionalPropertyId",
                        "notionalValue",
                        "collateralPropertyId",
                        "leverage",
                        "expiryPeriod",
                        "series",
                        "inverse",
                        "fee",
                        "block",
                        "whitelist",
                    ],
                ),
                17 => (
                    "exerciseDerivative",
                    &[
                        "contractId",
                        "amount",
                        "contractsRegistry",
                        "senderAddress",
                        "block",
                    ],
                ),
                19 => (
                    "tradeContractChannel",
                    &[
                        "contractId",
                        "price",
                        "amount",
                        "columnAIsSeller",
                        "expiryBlock",
                        "insurance",
                        "senderAddress",
                        "block",
                        "txid",
                        "columnAIsMaker",
                    ],
                ),
                20 => (
                    "tradeTokensChannel",
                    &[
                        "propertyIdOffered",
                        "propertyIdDesired",
                        "amountOffered",
                        "amountDesired",
                        "expiryBlock",
                        "columnAIsOfferer",
                        "senderAddress",
                        "block",
                        "txid",
                        "columnAIsMaker",
                    ],
                ),
                21 => (
                    "withdrawal",
                    &[
                        "withdrawAll",
                        "channelAddress",
                        "propertyId",
                        "amount",
                        "senderAddress",
                        "block",
                        "columnIsB",
                    ],
                ),
                22 => (
                    "transfer",
                    &[
                        "senderAddress",
                        "toChannelAddress",
                        "propertyId",
                        "amount",
                        "isColumnA",
                        "pay",
                        "payRefAddress",
                        "block",
                        "txid",
                    ],
                ),
                23 => (
                    "settleChannelPNL",
                    &["channelAddress", "txParams", "block", "txid"],
                ),
                31 => ("batchSettlement", &[]),
                _ => return Err(SourceRuleError::InvalidInput),
            };
        let arguments = if method == "batchSettlement" {
            vec![p.clone()]
        } else {
            fields.iter().map(|f| p.get(f)).collect()
        };
        Ok(
            match self.invoke_dependency(&DependencyCall {
                target: format!("Logic.{method}"),
                arguments,
            })? {
                DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
                    SourceOutcome::Returned(V::Undefined)
                }
                DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                    SourceOutcome::Thrown(error)
                }
            },
        )
    }
}
impl ContractCore {
    fn source_channel_sufficiency_begin(&mut self) -> R<V> {
        if self.channel_sufficiency_reads.len() >= 2 {
            return Err(SourceRuleError::Capacity.into());
        }
        let pending = match self.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        self.channel_sufficiency_reads.push(pending);
        // At these two statically un-awaited raw20 callsites the only value
        // read is the Promise object, never its later sufficiency result. The
        // actual load continuations below still own all observable side effects.
        Ok(V::Promise)
    }
    fn source_channel_sufficiency_drain(&mut self) -> R<()> {
        for pending in std::mem::take(&mut self.channel_sufficiency_reads) {
            if !pending {
                continue;
            }
            let read = (|| {
                let row = self.find("tallyMap", obj(&[("_id", s("tallyMap"))]))?;
                let entries = Self::tally_parsed_entries(&row)?;
                self.tally = entries;
                self.tally_static_initialized = true;
                Ok(())
            })();
            // loadFromDB catches read/JSON failures; hasSufficientChannel also
            // catches its later getTally failure. Neither Promise is awaited.
            match read {
                Err(Fault::Thrown(_)) => (),
                Err(e) => return Err(e),
                Ok(_) => (),
            }
        }
        Ok(())
    }
    fn source_channel_balance_tick(&mut self) -> R<()> {
        let mut jobs = std::mem::take(&mut self.channel_balance_jobs);
        self.source_channel_jobs_tick(&mut jobs)?;
        self.channel_balance_jobs = jobs;
        Ok(())
    }
    fn source_channel_balance_capture(&mut self) -> R<()> {
        if self.channel_balance_stage != 0 {
            return Ok(());
        }
        let mut jobs = std::mem::take(&mut self.channel_balance_jobs);
        for job in &mut jobs {
            self.source_channel_job_advance(job, 0)?;
        }
        self.channel_balance_jobs = jobs;
        self.channel_balance_stage = 1;
        Ok(())
    }
    fn source_channel_balance_first_write(&mut self) -> R<()> {
        if self.channel_balance_stage >= 2 {
            return Ok(());
        }
        self.source_channel_balance_capture()?;
        let mut jobs = std::mem::take(&mut self.channel_balance_jobs);
        self.source_channel_jobs_after_first_write(&mut jobs)?;
        self.channel_balance_jobs = jobs;
        self.channel_balance_stage = 2;
        Ok(())
    }
    fn source_channel_balance_drain(&mut self) -> R<()> {
        self.source_channel_balance_first_write()?;
        while self.channel_balance_jobs.iter().any(|job| job.phase < 6) {
            self.source_channel_balance_tick()?;
        }
        self.channel_balance_jobs.clear();
        self.channel_balance_stage = 0;
        self.channel_save_writes = 0;
        Ok(())
    }
    fn source_channel_begin_inverse(&mut self, id: V) -> R<()> {
        if self.channel_inverse_jobs.len() >= MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        let failure = match self.database("contractList") {
            Ok(_) => None,
            Err(Fault::Thrown(e)) => Some(e),
            Err(e) => return Err(e),
        };
        self.channel_inverse_jobs.push((id, failure));
        Ok(())
    }
    fn source_channel_finish_inverse(&mut self) -> R<()> {
        for (id, failure) in std::mem::take(&mut self.channel_inverse_jobs) {
            let result = if let Some(error) = failure {
                Err(Fault::Thrown(error))
            } else {
                (|| {
                    let t = text(&id)?;
                    let trimmed = t.trim_start_matches(funding_js_space);
                    let offset = usize::from(trimmed.starts_with(['-', '+']));
                    let end = offset
                        + trimmed[offset..]
                            .bytes()
                            .take_while(u8::is_ascii_digit)
                            .count();
                    self.find(
                        "contractList",
                        obj(&[
                            ("id", n(trimmed[..end].parse::<f64>().unwrap_or(f64::NAN))),
                            ("type", s("contractSeries")),
                        ]),
                    )
                })()
            };
            match result {
                Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                Err(e) => return Err(e),
                Ok(_) => (),
            }
        }
        Ok(())
    }
    fn source_channel_txutils(&mut self, target: &str, a: &[V]) -> R<V> {
        if target == "TxUtils.getRawTransaction" {
            if self.globals.get("sourceRpcTransactions") != V::Bool(true) {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            let value = self
                .rows("sourceRpcTransactions")
                .iter()
                .find(|r| same(&r.get("txid"), &arg(a, 0)))
                .map_or(V::Undefined, |r| r.get("transaction"));
            self.emit(obj(&[
                ("kind", s("rpc")),
                ("method", s(target)),
                ("args", V::Array(a.to_vec())),
                ("returned", value.clone()),
            ]));
            return Ok(value);
        }
        if target == "TxUtils.getAddressTypeUniversal" {
            let address = arg(a, 0);
            let V::String(address) = address else {
                return Err(if matches!(address, V::Undefined | V::Null) {
                    type_error(format!(
                        "Cannot read properties of {} (reading 'startsWith')",
                        text(&address)?
                    ))
                } else {
                    type_error("address.startsWith is not a function")
                });
            };
            let kind = if ["1", "L", "M", "m", "n"]
                .iter()
                .any(|p| address.starts_with(p))
            {
                "p2pkh"
            } else if ["3", "2"].iter().any(|p| address.starts_with(p)) {
                "p2sh"
            } else if ["bc1q", "ltc1q", "tb1q", "tltc1q"]
                .iter()
                .any(|p| address.starts_with(p))
            {
                "p2wpkh"
            } else if ["bc1p", "ltc1p", "tb1p", "tltc1p"]
                .iter()
                .any(|p| address.starts_with(p))
            {
                "p2tr"
            } else {
                "unknown"
            };
            return Ok(s(kind));
        }
        let vin = arg(a, 0);
        let kind = arg(a, 1);
        let asm = |vin: &V| -> R<Option<Vec<String>>> {
            let sig = funding_prop(vin, "scriptSig")?;
            if !truth(&sig) {
                return Ok(None);
            }
            let asm = funding_prop(&sig, "asm")?;
            if !truth(&asm) {
                return Ok(None);
            }
            let V::String(asm) = asm else {
                return Err(type_error("vin.scriptSig.asm.split is not a function"));
            };
            Ok(Some(asm.split(' ').map(str::to_owned).collect()))
        };
        let valid =
            |key: &str| matches!(key.len(), 66 | 130) && key.bytes().all(|c| c.is_ascii_hexdigit());
        if kind == s("p2pkh") {
            if let Some(asm) = asm(&vin)? {
                if let Some(key) = asm.last().filter(|key| valid(key)) {
                    return Ok(s(key));
                }
            }
        } else if kind == s("p2wpkh") {
            let witness = funding_prop(&vin, "txinwitness")?;
            // A string index is one UTF-16 code unit, so it cannot match a
            // 66/130-character key. This also avoids decoding a lone surrogate.
            if matches!(witness, V::String(_)) {
                return Ok(V::Null);
            }
            if truth(&witness) {
                let length = match &witness {
                    V::Array(a) => a.len() as f64,
                    V::String(t) => t.encode_utf16().count() as f64,
                    _ => number(&witness.get("length"))?,
                };
                if length > 1. {
                    let key = registry_index(&witness, 1)?;
                    if valid(&source_channel_string(&key)?) {
                        return Ok(key);
                    }
                }
            }
        } else if kind == s("p2sh") {
            if let Some(asm) = asm(&vin)? {
                let keys = asm
                    .into_iter()
                    .filter(|k| valid(k))
                    .map(s)
                    .collect::<Vec<_>>();
                return Ok(if keys.is_empty() {
                    V::Null
                } else {
                    V::Array(keys)
                });
            }
        }
        Ok(V::Null)
    }
    fn source_channel_queue_capability(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.harvest_queue(queue);
        let mut backend = FinancialBackend { core: self, queue };
        returned_source(match call.target.as_str() {
            "Orderbook.processTokenMatches" => {
                crate::token_settlement_source::process_token_matches(
                    &call.arguments[1..],
                    &mut backend,
                )?
            }
            "Orderbook.processContractMatches" => {
                process_contract_matches_actual(&call.arguments[1..], &mut backend)?
            }
            _ => return Err(SourceRuleError::UnknownDependency),
        })
    }
    fn source_lifecycle_logic(&mut self, method: &str, a: &[V]) -> R<V> {
        match method {
            "createContractSeries" => self.source_registry_logic_create(a),
            "tradeContractChannel" => self.source_channel_contract_logic(a),
            "tradeTokensChannel" => self.source_channel_token_logic(a),
            "withdrawal" => self.source_channel_withdrawal_logic(a),
            "transfer" => self.source_channel_transfer_logic(a),
            "settleChannelPNL" => self.source_channel_settle_logic(a),
            "batchSettlement" => self.source_channel_king_logic(arg(a, 0)),
            "processKeepAlive" => self.source_channel_keep_alive(a),
            "processClosePosition" => self.source_channel_close_logic(a),
            "processNetSettle" => self.source_channel_net_logic(a),
            "recordSettlement" => self.source_lifecycle_scaling("recordSettlement", a),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_channel_withdrawal_logic(&mut self, a: &[V]) -> R<V> {
        if self.spot_lifecycle_enabled() && self.source_spot_main_active {
            return self.source_main_withdrawal_begin(a);
        }
        let cached = self
            .channels
            .state()
            .registry
            .iter()
            .any(|(k, v)| same(k, &arg(a, 1)) && truth(v));
        let loading = if cached {
            None
        } else {
            Some(self.database("channels"))
        };
        let row = self.channels.begin_withdrawal(&[
            arg(a, 5),
            arg(a, 4),
            arg(a, 3),
            arg(a, 1),
            arg(a, 2),
            arg(a, 0),
            arg(a, 6),
        ])?;
        // Both async calls have now reached their first await. Only then do
        // their DB continuations run, in the order the source enqueued them.
        let saving = self.database("withdrawQueue");
        if let Some(loading) = loading {
            let result = loading
                .and_then(|_| self.db("channels", "findAsync", vec![obj(&[])]))
                .and_then(|rows| {
                    self.channels.replace_loaded_registry(array(&rows)?)?;
                    Ok(V::Undefined)
                });
            match result {
                Err(Fault::Thrown(e)) if e.message.contains("does not exist") => {
                    self.channels.replace_loaded_registry(vec![])?
                }
                Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                Err(e) => return Err(e),
                Ok(_) => (),
            }
        }
        let result = saving.and_then(|_| {
            self.update(
                "withdrawQueue",
                obj(&[(
                    "_id",
                    s(format!(
                        "withdrawal-{}-{}",
                        text(&row.get("blockHeight"))?,
                        text(&row.get("senderAddress"))?
                    )),
                )]),
                obj(&[("$set", obj(&[("data", row)]))]),
                true,
            )
        });
        match result {
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        Ok(V::Undefined)
    }
    fn source_channel_contract_logic(&mut self, a: &[V]) -> R<V> {
        let commits = self.call("Channels.getCommitAddresses", vec![arg(a, 6)])?;
        let book = self.call("Orderbook.getOrderbookInstance", vec![arg(a, 0)])?;
        let im = self.call("Contract.getInitialMargin", vec![arg(a, 0), arg(a, 1)])?;
        let margin = FundingBn::new(&im)?
            .op(FundingBn::new(&arg(a, 2))?, 2)?
            .number();
        self.source_channel_begin_inverse(arg(a, 0))?;
        let sell_a = truth(&arg(a, 3));
        let (seller, buyer) = if sell_a {
            (commits.get("commitAddressA"), commits.get("commitAddressB"))
        } else {
            (commits.get("commitAddressB"), commits.get("commitAddressA"))
        };
        let sell = obj(&[
            ("contractId", arg(a, 0)),
            ("amount", arg(a, 2)),
            ("price", arg(a, 1)),
            ("block", arg(a, 7)),
            ("sellSide", V::Bool(true)),
            ("marginUsed", n(margin)),
            ("sellerAddress", seller),
            ("txid", arg(a, 8)),
            (
                "maker",
                V::Bool(if sell_a {
                    truth(&arg(a, 9))
                } else {
                    !truth(&arg(a, 9))
                }),
            ),
        ]);
        let buy = obj(&[
            ("contractId", arg(a, 0)),
            ("amount", arg(a, 2)),
            ("price", arg(a, 1)),
            ("block", arg(a, 7)),
            ("buySide", V::Bool(true)),
            ("marginUsed", n(margin)),
            ("buyerAddress", buyer),
            ("txid", arg(a, 8)),
            (
                "maker",
                V::Bool(if sell_a {
                    !truth(&arg(a, 9))
                } else {
                    truth(&arg(a, 9))
                }),
            ),
        ]);
        let m = obj(&[
            ("sellOrder", sell),
            ("buyOrder", buy),
            ("price", arg(a, 1)),
            ("channelAddress", arg(a, 6)),
            ("tradePrice", arg(a, 1)),
            ("txid", arg(a, 8)),
        ]);
        let settled = self.call(
            "Orderbook.processContractMatches",
            vec![book, V::Array(vec![m]), arg(a, 7), V::Bool(true)],
        );
        self.source_channel_finish_inverse()?;
        settled?;
        Ok(V::Undefined)
    }
    fn source_channel_token_logic(&mut self, a: &[V]) -> R<V> {
        let commits = self.call("Channels.getCommitAddresses", vec![arg(a, 6)])?;
        let key = s(format!("{}-{}", text(&arg(a, 0))?, text(&arg(a, 1))?));
        let book = self.call("Orderbook.getOrderbookInstance", vec![key.clone()])?;
        let offered = truth(&arg(a, 5));
        let (seller, buyer) = if offered {
            (commits.get("commitAddressA"), commits.get("commitAddressB"))
        } else {
            (commits.get("commitAddressB"), commits.get("commitAddressA"))
        };
        let sell = obj(&[
            ("offeredPropertyId", arg(a, 0)),
            ("desiredPropertyId", arg(a, 1)),
            ("amountOffered", arg(a, 2)),
            ("amountExpected", arg(a, 3)),
            ("blockTime", arg(a, 7)),
            ("sender", seller),
            ("maker", if offered { arg(a, 9) } else { V::Bool(false) }),
        ]);
        let buy = obj(&[
            ("offeredPropertyId", arg(a, 0)),
            ("desiredPropertyId", arg(a, 1)),
            ("amountOffered", arg(a, 3)),
            ("amountExpected", arg(a, 2)),
            ("blockTime", arg(a, 7)),
            ("sender", buyer),
            ("maker", if !offered { arg(a, 9) } else { V::Bool(false) }),
        ]);
        let price = FundingBn::new(&arg(a, 2))?.op(FundingBn::new(&arg(a, 3))?, 3)?;
        let m = obj(&[
            ("sellOrder", sell),
            ("buyOrder", buy),
            ("amountOfTokenA", n(FundingBn::new(&arg(a, 2))?.number())),
            ("amountOfTokenB", n(FundingBn::new(&arg(a, 3))?.number())),
            ("tradePrice", n(price.number())),
            ("channel", arg(a, 6)),
            ("buyerTx", arg(a, 8)),
            ("sellerTx", arg(a, 8)),
        ]);
        self.call(
            "Orderbook.processTokenMatches",
            vec![book, V::Array(vec![m]), arg(a, 7), arg(a, 8), V::Bool(true)],
        )?;
        // The un-awaited price function returns a Promise to BigNumber.
        self.source_channel_token_price_begin(arg(a, 1))?;
        self.call(
            "VolumeIndex.saveVolumeDataById",
            vec![
                key,
                arg(a, 2),
                n(f64::NAN),
                price.value(),
                arg(a, 7),
                s("token"),
            ],
        )?;
        Ok(s(format!(
            "Trade executed in channel {}",
            text(&arg(a, 6))?
        )))
    }
    fn source_channel_mutate(&mut self, key: &V, value: V) -> R<()> {
        self.channels.mutate_cached_channel(key, |channel| {
            *channel = value;
            Ok(())
        })?;
        Ok(())
    }
    fn source_channel_transfer_logic(&mut self, a: &[V]) -> R<V> {
        let from = arg(a, 0);
        let to = arg(a, 1);
        let property = source_channel_string(&arg(a, 2))?;
        let column = if truth(&arg(a, 4)) { "A" } else { "B" };
        let mut source = self.call("Channels.getChannel", vec![from.clone()])?;
        self.call(
            "Channels.recordCommitToChannel",
            vec![
                to.clone(),
                from.clone(),
                arg(a, 2),
                arg(a, 3),
                V::Bool(false),
                s(""),
                arg(a, 7),
                arg(a, 8),
            ],
        )?;
        let mut dest = self.call("Channels.getChannel", vec![to.clone()])?;
        // A same-key lookup returns the identical mutated object.
        if same(&from, &to) {
            source = dest.clone();
        }
        let participants = funding_prop(&dest, "participants")?;
        let source_participant = funding_prop(&funding_prop(&source, "participants")?, column)?;
        let dest_column =
            if registry_equal(&funding_prop(&participants, "A")?, &source_participant)? {
                "A"
            } else if registry_equal(&funding_prop(&participants, "B")?, &source_participant)? {
                "B"
            } else {
                " "
            };
        let before = funding_prop(&funding_prop(&source, column)?, &property)?;
        source_channel_column_write(
            &mut source,
            column,
            &property,
            n(number(&before)? - number(&arg(a, 3))?),
        )?;
        self.source_channel_mutate(&from, source.clone())?;
        if same(&from, &to) {
            dest = source.clone();
        }
        let recipient = if truth(&arg(a, 5)) && truth(&arg(a, 6)) {
            arg(a, 6)
        } else {
            source_participant
        };
        set_path(&mut dest, &format!("participants.{dest_column}"), recipient)?;
        self.source_channel_mutate(&to, dest.clone())?;
        if same(&from, &to) {
            source = dest;
        }
        if !self.channel_balance_jobs.is_empty() {
            return Err(SourceRuleError::Backend.into());
        }
        self.channel_balance_stage = 0;
        self.channel_save_writes = 0;
        for (key, amount, kind) in [
            (from.clone(), n(-number(&arg(a, 3))?), "transferDebit"),
            (to.clone(), arg(a, 3), "transferCredit"),
        ] {
            let job =
                self.source_channel_job_begin(&[key, arg(a, 2), amount, s(kind), arg(a, 7)])?;
            self.channel_balance_jobs.push(job);
        }
        // channelsRegistry.set is a synchronous map write, without setChannel's save.
        self.source_channel_mutate(&from, source)?;
        let saved = self.call("Channels.saveChannelsRegistry", vec![]);
        if self.spot_lifecycle_enabled() && self.source_spot_main_active {
            self.source_transfer_balance_return()?;
        } else {
            self.source_channel_balance_drain()?;
        }
        saved?;
        Ok(V::Undefined)
    }
    fn source_lifecycle_scaling(&mut self, method: &str, a: &[V]) -> R<V> {
        self.database("scaling")?;
        match method {
            "isSettlementNeutralized" => {
                let doc = self.find("scaling", obj(&[("_id", arg(a, 0))]))?;
                if !truth(&doc) || !truth(&doc.get("settlements")) {
                    return Ok(V::Bool(false));
                }
                for row in array(&doc.get("settlements"))? {
                    if same(&funding_prop(&row, "txid")?, &arg(a, 1)) {
                        return Ok(V::Bool(row.get("status") == s("neutralized")));
                    }
                }
                Ok(V::Undefined)
            }
            "getTradeStatus" => {
                let docs = array(&self.db("scaling", "findAsync", vec![obj(&[])])?)?;
                for doc in docs {
                    if truth(&doc.get("trades")) {
                        for row in array(&doc.get("trades"))? {
                            if same(&funding_prop(&row, "txid")?, &arg(a, 0)) {
                                return Ok(row);
                            }
                        }
                    }
                }
                Ok(obj(&[("status", s("unpublished"))]))
            }
            "recordSettlement" => {
                self.update(
                    "scaling",
                    obj(&[("_id", arg(a, 0))]),
                    obj(&[(
                        "$push",
                        obj(&[(
                            "settlements",
                            obj(&[
                                ("txid", arg(a, 1)),
                                ("settleType", arg(a, 2)),
                                ("block", arg(a, 3)),
                                ("status", s("live")),
                                ("timestamp", n(self.time_millis as f64)),
                            ]),
                        )]),
                    )]),
                    true,
                )?;
                Ok(V::Undefined)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_channel_empty_scaling(&self, key: V) -> V {
        obj(&[
            ("_id", key),
            ("trades", V::Array(vec![])),
            ("keepAlives", V::Array(vec![])),
            ("settlements", V::Array(vec![])),
            ("closes", V::Array(vec![])),
            ("netSettles", V::Array(vec![])),
            ("kingSettlements", V::Array(vec![])),
        ])
    }
    fn source_channel_push(&self, doc: &mut V, key: &str, row: V) -> R<()> {
        let mut entries = if truth(&doc.get(key)) {
            array(&doc.get(key))?
        } else {
            vec![]
        };
        if entries.len() >= MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        entries.push(row);
        set(doc, key, V::Array(entries))
    }
    fn source_channel_keep_alive(&mut self, a: &[V]) -> R<V> {
        self.database("scaling")?;
        let key = arg(a, 0);
        let query = obj(&[("_id", key.clone())]);
        let mut doc = self.find("scaling", query.clone())?;
        if !truth(&doc) {
            doc = self.source_channel_empty_scaling(key);
        }
        self.source_channel_push(
            &mut doc,
            "keepAlives",
            obj(&[
                ("tradeTxid", arg(a, 1)),
                ("settleTxid", arg(a, 3)),
                ("block", arg(a, 2)),
                ("timestamp", n(self.time_millis as f64)),
            ]),
        )?;
        self.source_channel_edit_first(
            &mut doc,
            "trades",
            "txid",
            &arg(a, 1),
            &[("status", s("live")), ("keepAliveBlock", arg(a, 2))],
        )?;
        self.update("scaling", query, doc, true)?;
        Ok(V::Undefined)
    }
    fn source_channel_edit_first(
        &self,
        doc: &mut V,
        field: &str,
        id_key: &str,
        id: &V,
        writes: &[(&str, V)],
    ) -> R<()> {
        if !truth(&doc.get(field)) {
            return Ok(());
        }
        let mut rows = array(&doc.get(field))?;
        for row in &mut rows {
            if same(&funding_prop(row, id_key)?, id) {
                for (key, value) in writes {
                    set(row, key, value.clone())?;
                }
                break;
            }
        }
        set(doc, field, V::Array(rows))
    }
    fn source_channel_close_logic(&mut self, a: &[V]) -> R<V> {
        self.database("scaling")?;
        let key = arg(a, 0);
        let query = obj(&[("_id", key.clone())]);
        let mut doc = self.find("scaling", query.clone())?;
        if !truth(&doc) {
            return Ok(V::Undefined);
        }
        let original = if truth(&doc.get("trades")) {
            array(&doc.get("trades"))?
                .into_iter()
                .find(|r| same(&r.get("txid"), &arg(a, 1)))
        } else {
            None
        };
        let Some(original) = original else {
            return Ok(V::Undefined);
        };
        let params = original.get("params");
        let price = if truth(&params.get("price")) {
            params.get("price")
        } else {
            n(0.)
        };
        let contracts = if truth(&params.get("amount")) {
            params.get("amount")
        } else {
            n(0.)
        };
        let diff = number(&arg(a, 3))? - number(&price)?;
        let pnl = (diff * number(&contracts)?).abs();
        let direction = if if truth(&arg(a, 4)) {
            diff < 0.
        } else {
            diff > 0.
        } {
            "BtoA"
        } else {
            "AtoB"
        };
        let writes = [("status", s("neutralized")), ("neutralizedBy", arg(a, 6))];
        self.source_channel_edit_first(&mut doc, "trades", "txid", &arg(a, 1), &writes)?;
        self.source_channel_edit_first(&mut doc, "keepAlives", "settleTxid", &arg(a, 2), &writes)?;
        self.source_channel_push(
            &mut doc,
            "closes",
            obj(&[
                ("settleTxid", arg(a, 6)),
                ("tradeTxid", arg(a, 1)),
                ("keepAliveTxid", arg(a, 2)),
                ("markPrice", arg(a, 3)),
                ("entryPrice", price),
                ("contracts", contracts),
                ("pnlAmount", n(pnl)),
                ("pnlDirection", s(direction)),
                ("columnAIsSeller", arg(a, 4)),
                ("block", arg(a, 5)),
                ("timestamp", n(self.time_millis as f64)),
            ]),
        )?;
        self.update("scaling", query, doc, true)?;
        if truth(&params.get("contractId")) {
            self.source_channel_contract_logic(&[
                params.get("contractId"),
                arg(a, 3),
                params.get("amount"),
                V::Bool(!truth(&arg(a, 4))),
                source_js_plus(&arg(a, 5), &n(10.))?,
                V::Bool(false),
                key,
                arg(a, 5),
                source_js_plus(&arg(a, 6), &s("_close"))?,
                params.get("columnAIsMaker"),
            ])?;
        }
        Ok(V::Undefined)
    }
    fn source_channel_net_logic(&mut self, a: &[V]) -> R<V> {
        self.database("scaling")?;
        let key = arg(a, 0);
        let query = obj(&[("_id", key.clone())]);
        let mut doc = self.find("scaling", query.clone())?;
        if !truth(&doc) {
            doc = self.source_channel_empty_scaling(key);
        }
        for id in [arg(a, 1), arg(a, 2)] {
            if truth(&id) {
                for field in ["trades", "keepAlives", "settlements", "closes"] {
                    if truth(&doc.get(field)) {
                        let mut rows = array(&doc.get(field))?;
                        for row in &mut rows {
                            if same(&funding_prop(row, "txid")?, &id)
                                || same(&row.get("settleTxid"), &id)
                            {
                                set(row, "status", s("neutralized"))?;
                                set(row, "neutralizedBy", arg(a, 6))?;
                                break;
                            }
                        }
                        set(&mut doc, field, V::Array(rows))?;
                    }
                }
            }
        }
        let dir = if (truth(&arg(a, 4)) && number(&arg(a, 3))? >= 0.)
            || (!truth(&arg(a, 4)) && number(&arg(a, 3))? < 0.)
        {
            "AtoB"
        } else {
            "BtoA"
        };
        self.source_channel_push(
            &mut doc,
            "netSettles",
            obj(&[
                ("settleTxid", arg(a, 6)),
                ("txidNeutralized1", arg(a, 1)),
                ("txidNeutralized2", arg(a, 2)),
                ("netAmount", arg(a, 3)),
                ("columnAIsSeller", arg(a, 4)),
                ("direction", s(dir)),
                ("block", arg(a, 5)),
                ("status", s("pending")),
                ("timestamp", n(self.time_millis as f64)),
            ]),
        )?;
        self.update("scaling", query, doc, true)?;
        Ok(V::Undefined)
    }
    fn source_channel_king_logic(&mut self, p: V) -> R<V> {
        let key = funding_prop(&p, "senderAddress")?;
        let mut channel = self.call("Channels.getChannel", vec![key.clone()])?;
        self.call("Channels.getCommitAddresses", vec![key.clone()])?;
        self.database("scaling")?;
        let query = obj(&[("_id", key.clone())]);
        let mut doc = self.find("scaling", query.clone())?;
        if truth(&doc) {
            for field in [
                "trades",
                "keepAlives",
                "settlements",
                "closes",
                "netSettles",
            ] {
                if truth(&doc.get(field)) {
                    let mut rows = array(&doc.get(field))?;
                    for row in &mut rows {
                        let block = funding_prop(row, "block")?;
                        if number(&block)? >= number(&p.get("blockStart"))?
                            && number(&block)? <= number(&p.get("blockEnd"))?
                            && row.get("status") != s("neutralized")
                            && row.get("status") != s("swept")
                        {
                            set(row, "status", s("swept"))?;
                            set(row, "sweptBy", p.get("txid"))?;
                        }
                    }
                    set(&mut doc, field, V::Array(rows))?;
                }
            }
            self.update("scaling", query.clone(), doc.clone(), true)?;
        }
        let property = p.get("propertyId");
        if matches!(property, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading 'toString')",
                text(&property)?
            )));
        }
        if matches!(&property,V::Object(fields) if fields.iter().any(|(key,_)|key=="toString")) {
            return Err(type_error("propertyId.toString is not a function"));
        }
        let property = source_channel_string(&property)?;
        let amount = number(&p.get("netAmount"))?.abs();
        let (debit, credit) = if truth(&p.get("aPaysBDirection")) {
            ("A", "B")
        } else {
            ("B", "A")
        };
        let before = funding_or(funding_prop(&funding_prop(&channel, debit)?, &property)?);
        source_channel_column_write(&mut channel, debit, &property, n(number(&before)? - amount))?;
        self.source_channel_mutate(&key, channel.clone())?;
        let before = funding_or(funding_prop(&funding_prop(&channel, credit)?, &property)?);
        source_channel_column_write(
            &mut channel,
            credit,
            &property,
            source_js_plus(&before, &n(amount))?,
        )?;
        self.source_channel_mutate(&key, channel.clone())?;
        self.call("Channels.setChannel", vec![key.clone(), channel])?;
        if !truth(&doc) {
            doc = obj(&[("_id", key), ("kingSettlements", V::Array(vec![]))]);
        }
        self.source_channel_push(
            &mut doc,
            "kingSettlements",
            obj(&[
                ("txid", p.get("txid")),
                ("blockStart", p.get("blockStart")),
                ("blockEnd", p.get("blockEnd")),
                ("propertyId", p.get("propertyId")),
                ("netAmount", p.get("netAmount")),
                ("aPaysBDirection", p.get("aPaysBDirection")),
                ("channelRoot", p.get("channelRoot")),
                ("totalContracts", p.get("totalContracts")),
                ("neutralizedCount", p.get("neutralizedCount")),
                ("settledAtBlock", p.get("block")),
                ("timestamp", n(self.time_millis as f64)),
            ]),
        )?;
        self.update("scaling", query, doc, true)?;
        Ok(V::Undefined)
    }
    fn source_channel_settle_logic(&mut self, a: &[V]) -> R<V> {
        let p = arg(a, 1);
        if matches!(p, V::Null | V::Undefined) {
            return Err(type_error(format!(
                "Cannot destructure property 'txidNeutralized1' of 'txParams' as it is {}.",
                text(&p)?
            )));
        }
        self.call("Channels.getChannel", vec![arg(a, 0)])?;
        self.call("Channels.getCommitAddresses", vec![arg(a, 0)])?;
        let kind = p.get("settleType");
        if kind == n(0.) {
            self.source_channel_keep_alive(&[
                arg(a, 0),
                p.get("txidNeutralized1"),
                arg(a, 2),
                arg(a, 3),
            ])?;
        } else if kind == n(1.) {
            self.source_channel_close_logic(&[
                arg(a, 0),
                p.get("txidNeutralized1"),
                p.get("txidNeutralized2"),
                p.get("markPrice"),
                p.get("columnAIsSeller"),
                arg(a, 2),
                arg(a, 3),
            ])?;
        } else if kind == n(2.) {
            self.source_channel_net_logic(&[
                arg(a, 0),
                p.get("txidNeutralized1"),
                p.get("txidNeutralized2"),
                p.get("netAmount"),
                p.get("columnAIsSeller"),
                arg(a, 2),
                arg(a, 3),
            ])?;
        } else if kind == n(3.) {
            let mut k = obj(&[
                ("senderAddress", arg(a, 0)),
                (
                    "blockStart",
                    n(number(&nullish(p.get("blockStart"), arg(a, 2)))?),
                ),
                (
                    "blockEnd",
                    n(number(&nullish(p.get("blockEnd"), arg(a, 2)))?),
                ),
                ("propertyId", n(number(&p.get("propertyId"))?)),
                ("netAmount", n(number(&nullish(p.get("netAmount"), n(0.)))?)),
                (
                    "aPaysBDirection",
                    nullish(
                        p.get("aPaysBDirection"),
                        nullish(p.get("columnAIsSeller"), V::Bool(false)),
                    ),
                ),
                (
                    "channelRoot",
                    if truth(&p.get("channelRoot")) {
                        p.get("channelRoot")
                    } else if truth(&p.get("txidNeutralized1")) {
                        p.get("txidNeutralized1")
                    } else {
                        s("")
                    },
                ),
            ]);
            for field in ["totalContracts", "neutralizedCount"] {
                set(&mut k, field, n(number(&nullish(p.get(field), n(0.)))?))?;
            }
            set(&mut k, "block", arg(a, 2))?;
            set(&mut k, "txid", arg(a, 3))?;
            self.source_channel_king_logic(k)?;
        } else {
            return Ok(V::Undefined);
        }
        self.source_lifecycle_scaling(
            "recordSettlement",
            &[arg(a, 0), arg(a, 3), kind, arg(a, 2)],
        )?;
        Ok(V::Undefined)
    }
}
fn source_channel_string(value: &V) -> R<String> {
    match value {
        V::Object(fields) if fields.iter().any(|(key, _)| key == "toString") => {
            Err(type_error("Cannot convert object to primitive value"))
        }
        V::Array(values) => Ok(values
            .iter()
            .map(|v| {
                if matches!(v, V::Undefined | V::Null) {
                    Ok(String::new())
                } else {
                    source_channel_string(v)
                }
            })
            .collect::<R<Vec<_>>>()?
            .join(",")),
        V::BigNumber(_) => Ok(crate::channels_source::source_big_number_text(value)?),
        _ => text(value),
    }
}
fn source_js_plus(a: &V, b: &V) -> R<V> {
    let primitive = |value: &V| -> R<V> {
        match value {
            V::Object(_) | V::Array(_) | V::Promise | V::BigNumber(_) => {
                Ok(s(source_channel_string(value)?))
            }
            _ => Ok(value.clone()),
        }
    };
    let a = primitive(a)?;
    let b = primitive(b)?;
    if matches!(a, V::String(_)) || matches!(b, V::String(_)) {
        Ok(s(format!("{}{}", text(&a)?, text(&b)?)))
    } else {
        Ok(n(number(&a)? + number(&b)?))
    }
}
