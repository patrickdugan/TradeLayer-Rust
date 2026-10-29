// Callable Main source policies. One Core owns collections, Tally and queues.
// This is a callable transaction/mid-phase profile, not daemon startup.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceMainHeightJob {
    height: V,
    factory_error: Option<crate::validity_source::SourceException>,
}
struct SourceMainActivationBackend<'a> {
    core: &'a mut ContractCore,
}
impl SourceBackend for SourceMainActivationBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let value = match call.target.as_str() {
            "DB.getDatabase" if call.arguments == [s("activations")] => {
                self.core.database("activations")
            }
            "DB.findAsync" => self
                .core
                .db("activations", "findAsync", call.arguments.clone()),
            "DB.updateAsync" => self
                .core
                .db("activations", "updateAsync", call.arguments.clone()),
            _ if self.core.activation_lifecycle_enabled() => match call.target.as_str() {
                "Client.getInstance" | "Client.getChain" | "Client.getTest" => self
                    .core
                    .source_activation_client(&call.target, &call.arguments),
                "Vesting.getInstance" => self
                    .core
                    .source_activation_vesting_instance(&call.arguments),
                "Vesting.initializeTokens" => self
                    .core
                    .source_activation_initialize(arg(&call.arguments, 0)),
                "Consensus.pushLatestActivationToConsensusVector" => {
                    self.core.source_activation_consensus_push()
                }
                _ => return Err(SourceRuleError::UnknownDependency),
            },
            _ => return Err(SourceRuleError::UnknownDependency),
        };
        returned(value)
    }
}
impl ContractCore {
    fn source_main_configure(&mut self, a: &[V]) -> R<V> {
        let V::Object(fields) = arg(a, 0) else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields.len() != 1 || fields[0].0 != "activationProfile" {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let V::String(profile) = &fields[0].1 else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if !matches!(profile.as_str(), "source-registry" | "declared-answer")
            || (self.main_activation_locked && self.main_activation_profile != *profile)
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.main_activation_profile = profile.clone();
        Ok(V::Undefined)
    }
    fn source_main_activation(&mut self, method: &str, a: &[V]) -> R<V> {
        if self.source_token_threshold_enabled() && method == "areActivationsAboveThreshold" {
            return self.source_token_threshold_begin(a);
        }
        if !((self.activation_lifecycle_enabled() && ["init", "activate"].contains(&method))
            || [
                "getAdmin",
                "getActivationBlock",
                "initializeTxRegistry",
                "isTxTypeActive",
                "checkActivationBlock",
                "areActivationsAboveThreshold",
                "loadActivationsList",
                "saveActivationsList",
            ]
            .contains(&method))
        {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let mut owner = std::mem::take(&mut self.main_activation);
        let evaluation = owner.execute_unverified(
            crate::activation::ActivationCall {
                method: method.into(),
                arguments: a.to_vec(),
            },
            &mut SourceMainActivationBackend { core: self },
        );
        self.main_activation = owner;
        outcome(evaluation?.outcome)
    }
    fn source_main_state(&self) -> V {
        obj(&[
            ("profile", s("TransactionsAndMidV8")),
            ("activationProfile", s(&self.main_activation_profile)),
            ("activationLocked", V::Bool(self.main_activation_locked)),
            (
                "activation",
                self.main_activation.state().diagnostic_value(),
            ),
            (
                "activationActions",
                n(self.main_activation.action_count() as f64),
            ),
            (
                "pendingHeight",
                self.main_height_job.as_ref().map_or(V::Null, |j| {
                    obj(&[
                        ("height", j.height.clone()),
                        (
                            "factoryError",
                            j.factory_error.as_ref().map_or(V::Null, |e| {
                                obj(&[("name", s(&e.name)), ("message", s(&e.message))])
                            }),
                        ),
                    ])
                }),
            ),
            (
                "heightRejections",
                V::Array(
                    self.main_height_rejections
                        .iter()
                        .map(|e| obj(&[("name", s(&e.name)), ("message", s(&e.message))]))
                        .collect(),
                ),
            ),
        ])
    }
    fn source_main_check_processed(&mut self, id: V) -> R<V> {
        self.database("consensus")?;
        let row = self.find("consensus", obj(&[("_id", id)]))?;
        Ok(if truth(&row) {
            V::Bool(row.get("processed") == V::Bool(true))
        } else {
            row
        })
    }
    fn source_main_mark_processed(&mut self, id: V, params: V) -> R<()> {
        self.database("consensus")?;
        self.db(
            "consensus",
            "updateAsync",
            vec![
                obj(&[("_id", id)]),
                obj(&[(
                    "$set",
                    obj(&[("processed", V::Bool(true)), ("params", params)]),
                )]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(())
    }
    fn source_main_index_validity(&mut self, id: &V, kind: V, valid: V, reason: V) -> R<()> {
        self.database("txIndex")?;
        // Source /^tx/ is transported through the existing anchored-literal
        // DB query capability. No dynamic regular-expression engine is claimed.
        let rows = array(&self.db(
            "txIndex",
            "findAsync",
            vec![obj(&[("_id", obj(&[("$regex", s("^tx"))]))])],
        )?)?;
        let suffix = format!("-{}", source_channel_string(id)?);
        for row in rows {
            let V::String(key) = row.get("_id") else {
                return Err(type_error("txData._id.endsWith is not a function"));
            };
            if key.ends_with(&suffix) {
                self.db(
                    "txIndex",
                    "updateAsync",
                    vec![
                        obj(&[("_id", s(key))]),
                        obj(&[(
                            "$set",
                            obj(&[("type", kind), ("valid", valid), ("reason", reason)]),
                        )]),
                        obj(&[("upsert", V::Bool(true))]),
                    ],
                )?;
                break;
            }
        }
        Ok(())
    }
    fn source_main_height_read(&mut self, key: &str) -> R<V> {
        self.database("consensus")?;
        let row = self.find("consensus", obj(&[("_id", s(key))]))?;
        Ok(if truth(&row) {
            row.get("value")
        } else {
            V::Null
        })
    }
    fn source_main_height_write_after_factory(&mut self, key: &str, height: V) -> R<V> {
        self.db(
            "consensus",
            "updateAsync",
            vec![
                obj(&[("_id", s(key))]),
                obj(&[("$set", obj(&[("value", height)]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )?;
        Ok(V::Undefined)
    }
    fn source_main_height_write(&mut self, key: &str, height: V) -> R<V> {
        self.database("consensus")?;
        self.source_main_height_write_after_factory(key, height)
    }
    fn source_main_start_height_job(&mut self, height: V) -> R<()> {
        if self.main_height_job.is_some() {
            return Err(SourceRuleError::Capacity.into());
        }
        let factory_error = match self.database("consensus") {
            Ok(_) => None,
            Err(Fault::Thrown(e)) => Some(e),
            Err(e) => return Err(e),
        };
        self.main_height_job = Some(SourceMainHeightJob {
            height,
            factory_error,
        });
        Ok(())
    }
    fn source_main_drain_height_job(&mut self) -> R<()> {
        let Some(job) = self.main_height_job.take() else {
            return Ok(());
        };
        let result = if let Some(error) = job.factory_error {
            Err(Fault::Thrown(error))
        } else {
            self.source_main_height_write_after_factory("MaxProcessedHeight", job.height)
        };
        match result {
            Ok(_) => Ok(()),
            Err(Fault::Thrown(e)) => {
                if self.main_height_rejections.len() >= MAX_TABLE_ROWS {
                    return Err(SourceRuleError::Capacity.into());
                }
                self.main_height_rejections.push(e);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
    fn source_main_after_database(&mut self) -> R<()> {
        self.source_main_drain_height_job()
    }
}
impl SourceContractRuntime {
    fn source_main_invoke(
        &mut self,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.source_main_invoke_with(call, true, &mut |runtime, call| {
            runtime.source_main_dispatch(call)
        })
    }
    fn source_main_invoke_with(
        &mut self,
        call: &DependencyCall,
        rollback: bool,
        dispatch: &mut impl FnMut(&mut Self, &DependencyCall) -> R<V>,
    ) -> Result<DependencyReturn, SourceRuleError> {
        let before = rollback.then(|| self.clone());
        if self.core.main_production_enabled()
            && self.core.source_main_service_enabled()
            && self
                .core
                .source_main_service_get("startupModules")
                .map_err(|_| SourceRuleError::InvalidInput)?
                != V::Undefined
            && call.target.starts_with("Main.")
            && !truth(
                &self
                    .core
                    .source_main_startup_get("imported")
                    .map_err(|_| SourceRuleError::InvalidInput)?,
            )
        {
            return Err(SourceRuleError::UnknownDependency);
        }
        if self.core.main_activation_profile != "source-registry" {
            return Err(SourceRuleError::InvalidInput);
        }
        self.core.main_activation_locked = true;
        let rejected_before = self.core.main_height_rejections.len();
        let transfer_rejected_before = self.core.source_unhandled_rejections.len();
        let spot_main = self.core.spot_lifecycle_enabled();
        if spot_main {
            self.source_spot_main_enter()?;
        }
        let result = dispatch(self, call);
        // Native IO/local stops retain the exact accepted source prefix and
        // borrowed queue continuation. No generic post-call draining may run.
        if !rollback {
            if let Err(Fault::Local(error)) = &result {
                return Err(error.clone());
            }
        }
        let result = if spot_main {
            match self.source_spot_main_leave() {
                Ok(()) => result,
                Err(e) => Err(e),
            }
        } else {
            result
        };
        let result = match self.core.source_main_drain_height_job() {
            Ok(()) => result,
            Err(e) => Err(e),
        };
        let new_rejections = self.core.main_height_rejections[rejected_before..].to_vec();
        for e in new_rejections {
            self.core.emit(obj(&[
                ("kind", s("unhandled-rejection")),
                (
                    "thrown",
                    obj(&[("name", s(e.name)), ("message", s(e.message))]),
                ),
            ]));
        }
        // Source un-awaited raw22 rejection notifications occur after Main's
        // awaited work. Their exceptions already live in replay-bound source
        // diagnostics; no suspended transfer job crosses this boundary.
        if self.core.spot_lifecycle_enabled() {
            let new_rejections =
                self.core.source_unhandled_rejections[transfer_rejected_before..].to_vec();
            for error in new_rejections {
                self.core.emit(obj(&[
                    ("kind", s("unhandled-rejection")),
                    ("thrown", error),
                ]));
            }
        }
        let result = if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity.into())
        } else {
            result
        };
        self.core.returned_position_origin = None;
        self.core.collateral_position_origin = None;
        self.core.position_identity_active = false;
        match result {
            Ok(v)
                if self.core.main_production_enabled()
                    && ["Main.setPause", "SourceMain.importStartupModules"]
                        .contains(&call.target.as_str()) =>
            {
                Ok(DependencyReturn::Immediate(v))
            }
            Ok(v) => Ok(DependencyReturn::Pending(v)),
            Err(Fault::Thrown(e)) => Ok(DependencyReturn::PendingThrow(e)),
            Err(Fault::Local(e)) => {
                if let Some(before) = before {
                    *self = before;
                }
                Err(e)
            }
        }
    }
    fn source_main_dispatch(&mut self, call: &DependencyCall) -> R<V> {
        let a = &call.arguments;
        match call.target.as_str() {
            "Main.processTxSet" => self.source_main_process_rows(arg(a, 0), arg(a, 1), false),
            "Main.processTx" => self.source_main_process_rows(arg(a, 0), arg(a, 1), true),
            "Main.blockHandlerMid" => self.source_main_mid(a),
            "Main.loadMaxProcessedHeight" => {
                self.core.source_main_height_read("MaxProcessedHeight")
            }
            "Main.loadTrackHeight" => self.core.source_main_height_read("TrackHeight"),
            "Main.saveMaxProcessedHeight" => self
                .core
                .source_main_height_write("MaxProcessedHeight", arg(a, 0)),
            "Main.saveTrackHeight" => self.core.source_main_height_write("TrackHeight", arg(a, 0)),
            _ if self.core.main_production_enabled() && self.core.main_production.is_some() => {
                self.source_main_production_dispatch(&call.target, a)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_main_process_rows(&mut self, rows: V, height: V, live: bool) -> R<V> {
        let V::Array(rows) = rows else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if rows.len() > MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        self.source_main_process_rows_iter(
            rows.into_iter().map(Ok),
            height,
            live,
            &mut |core, id, kind, valid, reason| {
                core.source_main_index_validity(id, kind, valid, reason)
            },
        )
    }
    fn source_main_process_rows_iter(
        &mut self,
        rows: impl Iterator<Item = R<V>>,
        height: V,
        live: bool,
        validity: &mut impl FnMut(&mut ContractCore, &V, V, V, V) -> R<()>,
    ) -> R<V> {
        let mut skips = 0usize;
        let mut processed_any = false;
        for row in rows {
            let row = row?;
            let mut id = s("unknown");
            let mut kind = n(-1.);
            match self.source_main_one_row(
                &row,
                (&height, live),
                &mut id,
                &mut kind,
                &mut processed_any,
                validity,
            ) {
                Ok(true) => skips += 1,
                Ok(false) => (),
                Err(Fault::Thrown(e)) => {
                    let params = obj(&[
                        ("valid", V::Bool(false)),
                        (
                            "reason",
                            s(format!("Fatal tx processing error: {}", e.message)),
                        ),
                        ("block", height.clone()),
                        ("txid", id.clone()),
                    ]);
                    if id != s("unknown") {
                        self.core
                            .source_main_mark_processed(id.clone(), params.clone())?;
                        if number(&kind)? >= 0. {
                            validity(
                                &mut self.core,
                                &id,
                                kind,
                                V::Bool(false),
                                params.get("reason"),
                            )?;
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(if live {
            V::Bool(processed_any)
        } else {
            n(skips as f64)
        })
    }
    fn source_main_one_row(
        &mut self,
        row: &V,
        height_live: (&V, bool),
        id: &mut V,
        kind: &mut V,
        processed_any: &mut bool,
        validity: &mut impl FnMut(&mut ContractCore, &V, V, V, V) -> R<()>,
    ) -> R<bool> {
        let (height, live) = height_live;
        let value = if live {
            row.clone()
        } else {
            funding_prop(row, "value")?
        };
        *id = funding_prop(&value, "txId")?;
        let V::String(txid) = id else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if txid.len() > 256 || txid.is_empty() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        if truth(&self.core.source_main_check_processed(id.clone())?) {
            return Ok(true);
        }
        let payload = funding_prop(&value, "payload")?;
        let V::String(payload) = payload else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if !payload.is_ascii() || payload.len() > 65_536 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let ty = payload.as_bytes().first().and_then(|b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'z' => Some(b - b'a' + 10),
            b'A'..=b'Z' => Some(b - b'A' + 10),
            _ => None,
        });
        *kind = n(ty.map_or(f64::NAN, f64::from));
        // Original Types/Logic are shared with the owned standalone graph.
        // These kinds are exercised through actual-registry Main corpora,
        // including source-thrown prefixes; no success is inferred from a
        // standalone handler or a missing dependency.
        let inherited_kind = matches!(
            ty,
            Some(1 | 2 | 3 | 4 | 7 | 8 | 9 | 11 | 12 | 13 | 14 | 15 | 18 | 32)
        );
        let upper_kind = self.core.spot_lifecycle_enabled()
            && matches!(
                ty,
                Some(
                    16 | 17
                        | 19
                        | 20
                        | 21
                        | 22
                        | 23
                        | 24
                        | 25
                        | 26
                        | 27
                        | 28
                        | 29
                        | 30
                        | 31
                        | 33
                        | 34
                        | 35
                )
            );
        let spot_kind = self.core.spot_lifecycle_enabled() && matches!(ty, Some(5 | 6));
        let activation_kind = self.core.activation_lifecycle_enabled() && matches!(ty, Some(0));
        let amm_kind = self.core.amm_lifecycle_enabled() && matches!(ty, Some(10));
        if !(inherited_kind || upper_kind || spot_kind || activation_kind || amm_kind) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let sender = funding_prop(&value, "sender")?;
        let reference = funding_prop(&value, "reference")?;
        let args = vec![
            id.clone(),
            kind.clone(),
            if live { s("tl") } else { value.get("marker") },
            s(&payload[1..]),
            funding_prop(&sender, "senderAddress")?,
            reference.clone(),
            funding_prop(&sender, "amount")?,
            n(number(&funding_prop(&reference, "amount")?)? / 100_000_000.),
            height.clone(),
        ];
        let evaluation = self.source_main_decode_reused(&args)?;
        let mut params = match evaluation {
            crate::types_source::TypesOutcome::Returned(p) => p,
            crate::types_source::TypesOutcome::Rejected(e) => return Err(Fault::Thrown(e)),
            crate::types_source::TypesOutcome::ReturnedError(_) => {
                return Err(SourceRuleError::UnsupportedNumericDomain.into())
            }
        };
        if !matches!(params, V::Object(_)) {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        set(&mut params, "block", height.clone())?;
        if live && number(&params.get("type"))? > 0. {
            return Err(Fault::Thrown(crate::validity_source::SourceException {
                name: "ReferenceError".into(),
                message: "activationInstance is not defined".into(),
            }));
        }
        if params.get("valid") == V::Bool(true) {
            if live {
                *processed_any = true;
            }
            match self.source_main_logic_reused(&args, &params)? {
                SourceOutcome::Returned(_) => (),
                SourceOutcome::Thrown(e) => {
                    set(&mut params, "valid", V::Bool(false))?;
                    let reason = if truth(&params.get("reason")) {
                        source_channel_string(&params.get("reason"))?
                    } else {
                        String::new()
                    };
                    set(
                        &mut params,
                        "reason",
                        s(format!("{reason} Logic error: {}", e.message)),
                    )?;
                }
            }
        }
        self.last_parameters = Some(params.clone());
        self.core
            .source_main_mark_processed(id.clone(), params.clone())?;
        validity(
            &mut self.core,
            id,
            kind.clone(),
            params.get("valid"),
            params.get("reason"),
        )?;
        Ok(false)
    }
    fn source_main_phase(&mut self, target: &str, args: Vec<V>) -> R<V> {
        self.core.emit(obj(&[
            ("kind", s("main-phase")),
            ("target", s(target)),
            ("args", V::Array(args.clone())),
        ]));
        match self.invoke_dependency(&DependencyCall {
            target: target.into(),
            arguments: args,
        })? {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => Ok(v),
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Err(Fault::Thrown(e)),
        }
    }
    fn source_main_mid(&mut self, a: &[V]) -> R<V> {
        let rows = arg(a, 0);
        let height = arg(a, 1);
        if !matches!(height,V::Number(bits)if f64::from_bits(bits).is_finite()&&f64::from_bits(bits)>=0.&&f64::from_bits(bits).fract()==0.&&f64::from_bits(bits)<=9_007_199_254_740_991.)
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let reward_queue = self.core.source_liquidity_rewards_state().get("queue");
        if (!self.core.spot_lifecycle_enabled() && !self.core.fee_buffer.is_empty())
            || !matches!(reward_queue,V::Array(ref v)if v.is_empty())
        {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let mut did_work = false;
        if truth(&rows) {
            let V::Array(values) = &rows else {
                return Err(SourceRuleError::InvalidInput.into());
            };
            if !values.is_empty() {
                match self.source_main_process_rows(rows, height.clone(), true) {
                    Ok(v) => {
                        did_work = truth(&v);
                        self.core.source_main_start_height_job(height.clone())?;
                    }
                    Err(Fault::Thrown(_)) => (),
                    Err(e) => return Err(e),
                }
            }
        }
        if did_work {
            self.source_main_phase("Tally.flushQueuedFeeAccruals", vec![height.clone()])?;
            self.source_main_phase("Logic.settleLiquidityRewards", vec![height.clone()])?;
            // Exact completed-drain39 empty-buffer suspension: source height
            // continuation starts after rewards invocation, before queue drain.
            self.core.source_main_drain_height_job()?;
            self.core.emit(obj(&[
                ("kind", s("main-phase")),
                ("target", s("Orderbook.processQueuedOnChainOrdersForBlock")),
                ("args", V::Array(vec![height.clone()])),
            ]));
            self.evaluate_queue(&SourceCall {
                method: "processQueuedOnChainOrdersForBlock".into(),
                arguments: vec![height.clone()],
            })
            .map_err(Fault::Local)
            .and_then(outcome)?;
            self.core.emit(obj(&[
                ("kind", s("main-phase")),
                ("target", s("Orderbook.processQueuedChannelTradesForBlock")),
                ("args", V::Array(vec![height.clone()])),
            ]));
            self.evaluate_queue(&SourceCall {
                method: "processQueuedChannelTradesForBlock".into(),
                arguments: vec![height.clone()],
            })
            .map_err(Fault::Local)
            .and_then(outcome)?;
            self.source_main_phase(
                "Clearing.clearingFunction",
                vec![height.clone(), V::Bool(false)],
            )?;
        }
        self.source_main_phase("Channels.processWithdrawals", vec![height.clone()])?;
        Ok(height)
    }
}
