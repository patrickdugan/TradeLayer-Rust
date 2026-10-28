// Recorded original VM profile: host adapter Promise crossing and VM return.
const SOURCE_ACTIVATION_HOST_AWAIT: u64 = 3;
const SOURCE_ACTIVATION_VM_RETURN: u64 = 1;
// V10 composes existing owners. It introduces no registry or spendable ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceActivationInsuranceJob {
    instance: V,
    property: V,
    amount: V,
    phase: u8,
    event: V,
    snapshot: V,
    due: u64,
    order: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceActivationClearlistJob {
    data: V,
    due: u64,
    order: u64,
}

impl ContractCore {
    fn source_activation_consensus_state_hash(&mut self) -> R<V> {
        let result = (|| -> R<V> {
            let mut fields = vec![];
            for (field, table, last) in [
                ("latestTally", "tallyMap", true),
                ("latestActivation", "activations", true),
                ("channels", "channels", false),
                ("clearlists", "clearlists", false),
                ("contractList", "contractList", false),
                ("feeCache", "feeCache", false),
                ("insurance", "insurance", false),
                ("marginMaps", "marginMaps", false),
                ("oracleData", "oracleData", false),
                ("oracleList", "oracleList", false),
                ("orderBooks", "orderBooks", false),
                ("propertyList", "propertyList", false),
                ("syntheticTokens", "syntheticTokens", false),
                ("vaults", "vaults", false),
                ("volumeIndex", "volumeIndex", false),
                ("withdrawQueue", "withdrawQueue", false),
            ] {
                self.database(table)?;
                let rows = array(&self.db(table, "findAsync", vec![obj(&[])])?)?;
                fields.push((
                    field.to_string(),
                    if last {
                        rows.last().cloned().unwrap_or(V::Null)
                    } else {
                        V::Array(rows)
                    },
                ));
            }
            Ok(s(activation_sha_hex(json(&V::Object(fields))?.as_bytes())))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn source_activation_consensus_push(&mut self) -> R<V> {
        let result = (|| -> R<V> {
            self.database("activations")?;
            let rows = array(&self.db(
                "activations",
                "findAsync",
                vec![obj(&[("_id", s("activationsList"))])],
            )?)?;
            let row = arg(&rows, 0);
            if !truth(&row) || !truth(&row.get("value")) {
                return Ok(V::Undefined);
            }
            let registry = parse(&text(&row.get("value"))?)?;
            // Source for..in admits arrays. Strings enumerate only primitive
            // characters and other parsed primitives/null have no candidates.
            let values = match registry {
                V::Object(fields) => fields
                    .into_iter()
                    .map(|(_, value)| value)
                    .collect::<Vec<_>>(),
                V::Array(values) => values,
                _ => vec![],
            };
            let (mut latest, mut max) = (V::Null, n(-1.));
            for value in values {
                let block = funding_prop(&value, "activationBlock")?;
                if truth(&block) && activation_consensus_greater(&block, &max)? {
                    max = block;
                    latest = value;
                }
            }
            if latest == V::Null {
                return Ok(V::Undefined);
            }
            let code = self.source_activation_code_hash()?;
            let state = self.source_activation_consensus_state_hash()?;
            let entry = obj(&[
                ("activation", latest),
                ("codeHash", code),
                ("wasmCodeHash", s("")),
                ("consensusHash", state),
                ("blockNumber", max),
            ]);
            if self.activation_consensus_vector.len() >= 256 {
                return Err(SourceRuleError::Capacity.into());
            }
            self.activation_consensus_vector.push(entry.clone());
            Ok(entry)
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn source_activation_insurance_launch(&mut self, property: V, amount: V) -> R<()> {
        if self.activation_insurance_job.is_some() || self.activation_async_active {
            return Err(SourceRuleError::Capacity.into());
        }
        self.activation_async_active = true;
        self.activation_async_clock = 0;
        self.activation_async_serial = 0;
        let instance = obj(&[
            ("contractSeriesId", n(1.)),
            ("balances", V::Array(vec![])),
            ("hedgeRatio", n(0.5)),
            ("oracle", n(0.)),
        ]);
        match self.source_activation_child_database("insurance") {
            Err(Fault::Thrown(e)) => {
                self.source_activation_unhandled(e)?;
                return Ok(());
            }
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        let order = self.source_activation_next_order()?;
        self.activation_insurance_job = Some(SourceActivationInsuranceJob {
            instance,
            property: s(self.source_activation_native_text(&property)?),
            amount,
            phase: 0,
            event: V::Undefined,
            snapshot: V::Undefined,
            due: SOURCE_ACTIVATION_HOST_AWAIT,
            order,
        });
        Ok(())
    }
    fn source_activation_unhandled(&mut self, e: crate::validity_source::SourceException) -> R<()> {
        if self.source_unhandled_rejections.len() >= MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        self.source_unhandled_rejections
            .push(obj(&[("name", s(&e.name)), ("message", s(&e.message))]));
        Ok(())
    }
    fn source_activation_job_step(&mut self) -> R<()> {
        let Some(mut job) = self.activation_insurance_job.take() else {
            return Ok(());
        };
        let result = (|| -> R<bool> {
            let delay = match job.phase {
                0 => {
                    let row = self.find("insurance", obj(&[("key", s("1"))]))?;
                    self.source_insurance_apply_loaded(&mut job.instance, &row)?;
                    SOURCE_ACTIVATION_HOST_AWAIT + SOURCE_ACTIVATION_VM_RETURN
                }
                1 => {
                    let Some((event, snapshot)) = self.source_insurance_credit(
                        &mut job.instance,
                        job.property.clone(),
                        job.amount.clone(),
                        V::Bool(false),
                    )?
                    else {
                        return Ok(false);
                    };
                    job.event = event;
                    job.snapshot = snapshot;
                    self.database("insurance")?;
                    SOURCE_ACTIVATION_HOST_AWAIT
                }
                2 => {
                    self.db(
                        "insurance",
                        "insertAsync",
                        vec![obj(&[
                            ("key", s("event-false")),
                            ("value", job.event.clone()),
                        ])],
                    )?;
                    SOURCE_ACTIVATION_HOST_AWAIT + SOURCE_ACTIVATION_VM_RETURN
                }
                3 => {
                    self.call("TxUtils.getBlockCount", vec![])?;
                    SOURCE_ACTIVATION_HOST_AWAIT
                }
                4 => {
                    self.database("insurance")?;
                    SOURCE_ACTIVATION_HOST_AWAIT
                }
                5 => {
                    self.update(
                        "insurance",
                        obj(&[("key", s("1"))]),
                        obj(&[("$set", obj(&[("value", job.snapshot.clone())]))]),
                        true,
                    )?;
                    return Ok(false);
                }
                _ => return Err(SourceRuleError::Backend.into()),
            };
            job.phase += 1;
            job.due = self
                .activation_async_clock
                .checked_add(delay)
                .ok_or(SourceRuleError::Capacity)?;
            job.order = self.source_activation_next_order()?;
            Ok(true)
        })();
        match result {
            Ok(true) => self.activation_insurance_job = Some(job),
            Ok(false) => (),
            Err(Fault::Thrown(e)) => self.source_activation_unhandled(e)?,
            Err(e) => return Err(e),
        }
        Ok(())
    }
    fn source_activation_after_database(&mut self, _name: &str) -> R<()> {
        // Each adapter factory returns a host Promise awaited by original VM
        // code. Parallel child factories are explicitly marked below.
        self.source_activation_await_turns(SOURCE_ACTIVATION_HOST_AWAIT)
    }
    fn source_activation_after_db(&mut self, _name: &str, _op: &str) -> R<()> {
        self.source_activation_await_turns(SOURCE_ACTIVATION_HOST_AWAIT)
    }
    fn source_activation_drain(&mut self) -> R<()> {
        for _ in 0..16 {
            let insurance = self.activation_insurance_job.as_ref().map(|job| job.due);
            let clearlist = self.activation_clearlist_jobs.first().map(|job| job.due);
            let due = match (insurance, clearlist) {
                (None, None) => return Ok(()),
                (Some(a), None) | (None, Some(a)) => a,
                (Some(a), Some(b)) => a.min(b),
            };
            self.source_activation_advance_to(due.max(self.activation_async_clock))?;
        }
        Err(SourceRuleError::Backend.into())
    }
}
fn activation_sha_hex(bytes: &[u8]) -> String {
    crate::hash256::sha256(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl SourceContractRuntime {
    fn source_activation_raw_logic(
        &mut self,
        params: &V,
    ) -> Result<SourceOutcome, SourceRuleError> {
        match self.invoke_dependency(&DependencyCall {
            target: "Logic.activateTradeLayer".into(),
            arguments: vec![
                params.get("txTypesToActivate"),
                params.get("block"),
                params.get("codeHash"),
            ],
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
impl ContractCore {
    fn activation_lifecycle_enabled(&self) -> bool {
        (self.policy == SourceContractRuntimePolicy::ActivationLifecycleV10)
            || self.amm_lifecycle_enabled()
    }
    fn source_activation_state(&self) -> V {
        obj(&[
            ("profile", s("Archive25LfAwaitV2")),
            ("clientProfile", self.activation_client_profile.clone()),
            ("clientLocked", V::Bool(self.activation_client_locked)),
            (
                "awaitState",
                obj(&[
                    ("hostAdapterTurns", n(SOURCE_ACTIVATION_HOST_AWAIT as f64)),
                    ("vmReturnTurns", n(SOURCE_ACTIVATION_VM_RETURN as f64)),
                    ("active", V::Bool(self.activation_async_active)),
                    ("clock", n(self.activation_async_clock as f64)),
                    ("serial", n(self.activation_async_serial as f64)),
                    ("running", V::Bool(self.activation_async_running)),
                    ("childDepth", n(self.activation_async_child_depth as f64)),
                ]),
            ),
            ("vesting", self.activation_vesting.clone()),
            (
                "consensusVector",
                V::Array(self.activation_consensus_vector.clone()),
            ),
            (
                "nativeErrors",
                V::Array(self.activation_native_errors.clone()),
            ),
            (
                "pendingInsurance",
                self.activation_insurance_job
                    .as_ref()
                    .map_or(V::Null, |job| {
                        obj(&[
                            ("instance", job.instance.clone()),
                            ("property", job.property.clone()),
                            ("amount", job.amount.clone()),
                            ("phase", n(job.phase as f64)),
                            ("event", job.event.clone()),
                            ("snapshot", job.snapshot.clone()),
                            ("due", n(job.due as f64)),
                            ("order", n(job.order as f64)),
                        ])
                    }),
            ),
            (
                "pendingClearlists",
                V::Array(
                    self.activation_clearlist_jobs
                        .iter()
                        .map(|job| {
                            obj(&[
                                ("data", job.data.clone()),
                                ("due", n(job.due as f64)),
                                ("order", n(job.order as f64)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
    fn source_activation_configure(&mut self, a: &[V]) -> R<V> {
        let profile = arg(a, 0);
        if self.activation_client_locked || !matches!(profile, V::Null | V::Object(_)) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        if let V::Object(fields) = &profile {
            if fields.len() != 2
                || !matches!(profile.get("chain"),V::String(ref x)if !x.is_empty()&&x.len()<=32)
                || !matches!(profile.get("test"), V::Bool(_))
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
            if fields
                .iter()
                .any(|(name, _)| !matches!(name.as_str(), "chain" | "test"))
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
        }
        self.activation_client_profile = profile;
        Ok(V::Undefined)
    }
    fn source_activation_client(&mut self, target: &str, args: &[V]) -> R<V> {
        self.activation_client_locked = true;
        let mut event = obj(&[
            ("kind", s("rpc-input")),
            ("target", s(target)),
            ("args", V::Array(args.to_vec())),
        ]);
        let value = match target {
            "Client.getInstance" => {
                self.emit(event);
                if self.activation_client_profile == V::Null {
                    // Unavailable capability is a local boundary, never an
                    // invented source financial error or partial-state input.
                    return Err(SourceRuleError::UnknownDependency.into());
                }
                return Ok(obj(&[("sourceActivationClient", V::Bool(true))]));
            }
            "Client.getChain" => self.activation_client_profile.get("chain"),
            "Client.getTest" => self.activation_client_profile.get("test"),
            _ => return Err(SourceRuleError::UnknownDependency.into()),
        };
        set(&mut event, "returned", value.clone())?;
        self.emit(event);
        Ok(value)
    }
    fn source_activation_native_text(&self, value: &V) -> R<String> {
        if self.activation_lifecycle_enabled()
            && self
                .activation_native_errors
                .iter()
                .any(|known| known == value)
        {
            return Ok(format!(
                "{}: {}",
                text(&value.get("name"))?,
                text(&value.get("message"))?
            ));
        }
        text(value)
    }
    fn source_activation_vesting_instance(&mut self, args: &[V]) -> R<V> {
        if self.activation_vesting == V::Undefined {
            // The original getter drops its third argument at construction.
            self.activation_vesting = obj(&[
                ("adminAddress", arg(args, 0)),
                ("chain", arg(args, 1)),
                ("test", V::Undefined),
            ]);
            self.source_activation_client("Client.getInstance", &[])?;
            let chain = self.source_activation_client("Client.getChain", &[])?;
            let test = self.source_activation_client("Client.getTest", &[])?;
            set(&mut self.activation_vesting, "chain", chain.clone())?;
            set(&mut self.activation_vesting, "test", test)?;
            let parameters = if chain == s("BTC") {
                vec![
                    ("baseVolume", n(100.)),
                    ("minRebate", n(0.00000625)),
                    ("maxRebate", n(0.0001)),
                    ("initialTokenAmount", n(100000.)),
                    ("tickerSymbol", s("TL")),
                    ("hedgeLeverage", n(5.)),
                    ("expiryInterval", n(12960.)),
                    (
                        "sponsorAddress",
                        s("bc1qhc2cj60auf67e0pa3dfd46cvg0fehchx56vw0f"),
                    ),
                    ("freePortion", n(0.1)),
                    ("insurancePortion", n(0.1)),
                    ("vestingPortion", n(0.5)),
                    ("salePortion", n(0.)),
                ]
            } else if chain == s("DOGE") {
                vec![
                    ("baseVolume", n(2000000.)),
                    ("minRebate", n(0.00000125)),
                    ("maxRebate", n(0.0005)),
                    ("initialTokenAmount", n(200000000.)),
                    ("tickerSymbol", s("TD")),
                    ("hedgeLeverage", n(10.)),
                    ("expiryInterval", n(129600.)),
                    ("sponsorAddress", s("D8HA73pAhxK7eNXSUVhQrWpUkrszUDGs7Z")),
                    ("freePortion", n(0.25)),
                    ("insurancePortion", n(0.25)),
                    ("vestingPortion", n(0.5)),
                    ("salePortion", n(0.)),
                ]
            } else {
                vec![
                    ("baseVolume", n(1000.)),
                    ("minRebate", n(0.000003125)),
                    ("maxRebate", n(0.0001)),
                    ("initialTokenAmount", n(500000.)),
                    ("tickerSymbol", s("TLITE")),
                    ("hedgeLeverage", n(5.)),
                    ("expiryInterval", n(51840.)),
                    ("sponsorAddress", s("MWip91xMhaEmDn5oUW5NDNbWSDyG5dSK9Q")),
                    ("freePortion", n(0.1)),
                    ("salePortion", n(0.1)),
                    ("insurancePortion", n(0.3)),
                    ("vestingPortion", n(0.5)),
                ]
            };
            for (key, value) in parameters {
                set(&mut self.activation_vesting, key, value)?;
            }
        }
        Ok(obj(&[("sourceVestingInstance", V::Bool(true))]))
    }
    fn source_activation_fill_admin(&mut self) -> R<()> {
        if !matches!(
            self.activation_vesting.get("adminAddress"),
            V::Undefined | V::Null
        ) {
            return Ok(());
        }
        let chain = self.activation_vesting.get("chain");
        let test = truth(&self.activation_vesting.get("test"));
        let admin = if chain == s("BTC") {
            Some(if test {
                "tb1q8f84erfegxhaylmvpfll9m5rgwymqy4akjnnvq"
            } else {
                "bc1qktknrnx2jcchjht9anz0uy8ae02xryxq2vxeem,"
            })
        } else if chain == s("DOGE") {
            Some(if test {
                "nop27JQWbGr95ySHXZMzCg8XXxYzbCBZAW"
            } else {
                "DLSfu9qvEggkeXAgCAwBBw5BVLvMCtkewz"
            })
        } else if chain == s("LTC") {
            Some(if test {
                "tltc1qa0kd2d39nmeph3hvcx8ytv65ztcywg5sazhtw8"
            } else {
                "MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB"
            })
        } else {
            None
        };
        if let Some(admin) = admin {
            set(&mut self.activation_vesting, "adminAddress", s(admin))?;
        }
        Ok(())
    }
    fn source_activation_mul(&self, left: V, right: V) -> R<V> {
        Ok(n(FundingBn::new(&left)?
            .op(FundingBn::new(&right)?, 2)?
            .number()))
    }
    fn source_activation_create_token(&mut self, ticker: V, total: V, kind: &str) -> R<V> {
        let result = self.call("Property.createToken", vec![ticker, total, s(kind), n(0.)])?;
        if matches!(result, V::Object(_)) && result.get("name") == s("Error") {
            if self.activation_native_errors.len() >= 4 {
                return Err(SourceRuleError::Capacity.into());
            }
            self.activation_native_errors.push(result.clone());
        }
        Ok(result)
    }
    fn source_activation_initialize(&mut self, block: V) -> R<V> {
        let result = self.source_activation_initialize_inner(block);
        let continuation = self
            .source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)
            .and_then(|_| self.source_activation_drain());
        self.activation_native_errors.clear();
        self.activation_async_active = false;
        self.activation_async_clock = 0;
        self.activation_async_serial = 0;
        self.activation_async_running = false;
        self.activation_async_child_depth = 0;
        match (continuation, result) {
            (Err(e), _) => Err(e),
            (_, result) => result,
        }
    }
    fn source_activation_initialize_inner(&mut self, block: V) -> R<V> {
        let initialized = self.call("Tally.checkInitializationFlag", vec![])?;
        self.source_activation_fill_admin()?;
        if truth(&initialized) {
            return Ok(V::Undefined);
        }
        let total = self.activation_vesting.get("initialTokenAmount");
        let ticker = self.activation_vesting.get("tickerSymbol");
        let vest = self
            .source_activation_mul(total.clone(), self.activation_vesting.get("vestingPortion"))?;
        let insurance = self.source_activation_mul(
            total.clone(),
            self.activation_vesting.get("insurancePortion"),
        )?;
        let liquidity =
            self.source_activation_mul(total.clone(), self.activation_vesting.get("salePortion"))?;
        let free =
            self.source_activation_mul(total.clone(), self.activation_vesting.get("freePortion"))?;
        let income = self.source_activation_mul(total.clone(), n(3.))?;
        self.call("Property.getInstance", vec![])?;
        let token = self.source_activation_create_token(ticker.clone(), total, "Fixed")?;
        let vest_token = self.source_activation_create_token(
            s(format!("{}VEST", text(&ticker)?)),
            vest.clone(),
            "Vesting",
        )?;
        let income_token = self.source_activation_create_token(
            s(format!("{}I", text(&ticker)?)),
            income.clone(),
            "Vesting",
        )?;
        let final_token = self.source_activation_create_token(
            s(format!("{}IVEST", text(&ticker)?)),
            n(number(&income)? + 1.),
            "Native",
        )?;
        let mut params = obj(&[
            ("initialMargin", n(0.2)),
            ("maintenanceMargin", n(0.1)),
            ("expiry", s("perp")),
            ("onChainData", V::Array(vec![V::Array(vec![n(1.), n(0.)])])),
            ("expiryInterval", V::Undefined),
            ("leverage", n(5.)),
            ("seriesLength", n(6.)),
            ("native", V::Bool(true)),
            ("inverse", V::Bool(true)),
            ("fee", V::Bool(false)),
            ("notionalPropertyId", n(0.)),
            ("notionalValue", n(0.001)),
            ("collateralPropertyId", n(1.)),
        ]);
        self.source_registry_create(V::Undefined, &mut params, block)?;
        self.source_activation_insurance_launch(token.clone(), insurance)?;
        let admin = self.activation_vesting.get("adminAddress");
        for (address, property, available, vesting) in [
            (admin.clone(), token.clone(), liquidity, n(0.)),
            (admin.clone(), vest_token, vest.clone(), vest),
            (admin.clone(), income_token, n(1500000.), n(1500000.)),
            (admin.clone(), final_token, n(1.), n(0.)),
            (
                self.activation_vesting.get("sponsorAddress"),
                token,
                free,
                n(0.),
            ),
        ] {
            self.call(
                "Tally.updateBalance",
                vec![address, property, available, n(0.), n(0.), vesting],
            )?;
            self.source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)?;
        }
        let balances = self.call("Tally.getAddressBalances", vec![admin])?;
        self.source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)?;
        for (id, name, description) in [
            (
                1.,
                "Issuer Clearlist",
                "Oracles and Tokens included in Liquidity Reward",
            ),
            (
                2.,
                "Market Maker Clearlist",
                "Market Makers and active traders who do not wash trade.",
            ),
            (
                3.,
                "Accredited Clearlist",
                "Exempt from restrictions on native tokens.",
            ),
        ] {
            self.source_activation_clearlist_launch(obj(&[
                ("id", n(id)),
                ("admin", V::Undefined),
                ("name", s(name)),
                ("description", s(description)),
                ("backup", V::Undefined),
            ]))?;
        }
        self.source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)?;
        self.call("Tally.setInitializationFlag", vec![])?;
        self.source_activation_await_turns(SOURCE_ACTIVATION_VM_RETURN)?;
        Ok(balances)
    }
    fn source_activation_logic(&mut self, a: &[V]) -> R<V> {
        let values = if let V::Array(values) = arg(a, 0) {
            values
        } else {
            vec![arg(a, 0)]
        };
        if values.len() > 256 {
            return Err(SourceRuleError::Capacity.into());
        }
        let mut results = vec![];
        for value in values {
            let result = self.call(
                "Activation.activate",
                vec![value.clone(), arg(a, 1), arg(a, 2)],
            )?;
            results.push(obj(&[("txType", value), ("result", result)]));
        }
        Ok(V::Array(results))
    }
    fn source_activation_flags(&mut self, method: &str) -> R<V> {
        self.database("tallyMap")?;
        if method == "checkInitializationFlag" {
            let row = self.find("tallyMap", obj(&[("_id", s("$TLinit"))]))?;
            Ok(if truth(&row) {
                row.get("initialized")
            } else {
                V::Bool(false)
            })
        } else {
            self.update(
                "tallyMap",
                obj(&[("_id", s("$TLinit"))]),
                obj(&[("_id", s("$TLinit")), ("initialized", V::Bool(true))]),
                true,
            )?;
            Ok(V::Undefined)
        }
    }
}

impl ContractCore {
    fn source_activation_next_order(&mut self) -> R<u64> {
        self.activation_async_serial = self
            .activation_async_serial
            .checked_add(1)
            .ok_or(SourceRuleError::Capacity)?;
        if self.activation_async_serial > 1024 {
            return Err(SourceRuleError::Capacity.into());
        }
        Ok(self.activation_async_serial)
    }
    fn source_activation_clearlist_launch(&mut self, data: V) -> R<()> {
        if self.activation_clearlist_jobs.len() >= 3 {
            return Err(SourceRuleError::Capacity.into());
        }
        match self.source_activation_child_database("clearlists") {
            Err(Fault::Thrown(e)) => {
                self.source_activation_unhandled(e)?;
                return Ok(());
            }
            Err(e) => return Err(e),
            Ok(_) => (),
        }
        let order = self.source_activation_next_order()?;
        self.activation_clearlist_jobs
            .push(SourceActivationClearlistJob {
                data,
                due: self
                    .activation_async_clock
                    .checked_add(SOURCE_ACTIVATION_HOST_AWAIT)
                    .ok_or(SourceRuleError::Capacity)?,
                order,
            });
        Ok(())
    }
    fn source_activation_child_database(&mut self, name: &str) -> R<V> {
        let old = self.activation_async_child_depth;
        self.activation_async_child_depth = old.checked_add(1).ok_or(SourceRuleError::Capacity)?;
        let result = self.database(name);
        self.activation_async_child_depth = old;
        result
    }
    fn source_activation_child_find(&mut self, name: &str, query: V) -> R<V> {
        let old = self.activation_async_child_depth;
        self.activation_async_child_depth = old.checked_add(1).ok_or(SourceRuleError::Capacity)?;
        let result = self.find(name, query);
        self.activation_async_child_depth = old;
        result
    }
    fn source_activation_await_turns(&mut self, turns: u64) -> R<()> {
        if !self.activation_lifecycle_enabled()
            || !self.activation_async_active
            || self.activation_async_running
            || self.activation_async_child_depth != 0
        {
            return Ok(());
        }
        let target = self
            .activation_async_clock
            .checked_add(turns)
            .ok_or(SourceRuleError::Capacity)?;
        self.source_activation_advance_to(target)
    }
    fn source_activation_advance_to(&mut self, target: u64) -> R<()> {
        if target > 65536 {
            return Err(SourceRuleError::Capacity.into());
        }
        for _ in 0..16 {
            let insurance = self
                .activation_insurance_job
                .as_ref()
                .map(|job| (job.due, job.order));
            let clearlist = self
                .activation_clearlist_jobs
                .first()
                .map(|job| (job.due, job.order));
            let next = match (insurance, clearlist) {
                (None, None) => None,
                (Some(a), None) => Some((a, true)),
                (None, Some(a)) => Some((a, false)),
                (Some(a), Some(b)) => Some(if a <= b { (a, true) } else { (b, false) }),
            };
            let Some(((due, _), insurance)) = next else {
                self.activation_async_clock = target;
                return Ok(());
            };
            if due > target {
                self.activation_async_clock = target;
                return Ok(());
            }
            self.activation_async_clock = due;
            let old = self.activation_async_running;
            self.activation_async_running = true;
            let result = if insurance {
                self.source_activation_job_step()
            } else {
                let job = self.activation_clearlist_jobs.remove(0);
                let result = self.source_clearlist_update(
                    "clearlists",
                    job.data.get("id"),
                    obj(&[("data", job.data)]),
                    true,
                );
                match result {
                    Err(Fault::Thrown(e)) => self.source_activation_unhandled(e),
                    Err(e) => Err(e),
                    Ok(_) => Ok(()),
                }
            };
            self.activation_async_running = old;
            result?;
        }
        Err(SourceRuleError::Backend.into())
    }
}

// JSON.parse produces these finite own-data shapes, not arbitrary executable
// prototypes. Keep source ToPrimitive exceptions inside Consensus's catch.
fn activation_consensus_greater(left: &V, right: &V) -> R<bool> {
    fn primitive(value: &V) -> R<V> {
        match value {
            V::Object(_) | V::Array(_) | V::BigNumber(_) | V::Promise => {
                Ok(s(source_channel_string(value)?))
            }
            _ => Ok(value.clone()),
        }
    }
    let left = primitive(left)?;
    let right = primitive(right)?;
    if let (V::String(left), V::String(right)) = (&left, &right) {
        Ok(left.encode_utf16().cmp(right.encode_utf16()) == core::cmp::Ordering::Greater)
    } else {
        Ok(number(&left)? > number(&right)?)
    }
}
