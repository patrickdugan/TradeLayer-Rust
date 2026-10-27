// Actual source metadata/read helpers reached by clearingFunction's final checks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSupplyDelta {
    arguments: Vec<V>,
    key: String,
    phase: u8,
    existing: V,
    tally_pending: bool,
    failure: Option<crate::validity_source::SourceException>,
}
struct SourceChannelsBackend<'a>(&'a mut ContractCore);
impl SourceBackend for SourceChannelsBackend<'_> {
    fn invoke(&mut self, c: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if self.0.lifecycle_enabled() && c.target == "UUID.v4" {
            return self.0.invoke(&DependencyCall {
                target: "uuid.v4".into(),
                arguments: c.arguments.clone(),
            });
        }
        if let Some(method) = c.target.strip_prefix("DB.") {
            let result = if method == "getDatabase" {
                self.0.database(
                    &text(&arg(&c.arguments, 0)).map_err(|_| SourceRuleError::InvalidInput)?,
                )
            } else {
                self.0.db(
                    &text(&arg(&c.arguments, 0)).map_err(|_| SourceRuleError::InvalidInput)?,
                    method,
                    c.arguments[1..].to_vec(),
                )
            };
            return returned(result);
        }
        self.0.invoke(c)
    }
}
impl ContractCore {
    fn source_supply_rejection(&mut self, error: crate::validity_source::SourceException) -> R<()> {
        if self.source_unhandled_rejections.len() >= MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity.into());
        }
        self.source_unhandled_rejections.push(obj(&[
            ("name", s(error.name)),
            ("message", s(error.message)),
        ]));
        Ok(())
    }
    fn source_record_tally_start(&mut self, a: &[V]) -> R<SourceSupplyDelta> {
        let uuid = self.new_uuid();
        self.database("tallyMapDelta")?;
        Ok(SourceSupplyDelta {
            arguments: a.to_vec(),
            key: format!(
                "{}-{}-{}-{}",
                text(&arg(a, 0))?,
                text(&arg(a, 2))?,
                uuid,
                text(&arg(a, 1))?
            ),
            phase: 0,
            existing: V::Undefined,
            tally_pending: false,
            failure: None,
        })
    }
    fn source_record_tally_prefix(&mut self, job: &mut SourceSupplyDelta) -> R<()> {
        // getTally is async and intentionally not awaited by recordTallyMapDelta.
        // Its DB factory starts before the delta find; the load resumes later.
        job.tally_pending = match self.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        match self.find("tallyMapDelta", obj(&[("_id", s(&job.key))])) {
            Ok(value) => job.existing = value,
            Err(Fault::Thrown(e)) => job.failure = Some(e),
            Err(e) => return Err(e),
        }
        job.phase = 1;
        Ok(())
    }
    fn source_record_tally_finish(&mut self, job: &mut SourceSupplyDelta) -> R<V> {
        if job.tally_pending {
            let read = (|| {
                let row = self.find("tallyMap", obj(&[("_id", s("tallyMap"))]))?;
                self.tally = Self::tally_parsed_entries(&row)?;
                self.tally_static_initialized = true;
                Ok(())
            })();
            match read {
                Err(Fault::Thrown(_)) => (),
                Err(e) => return Err(e),
                _ => (),
            }
            job.tally_pending = false;
        }
        // The un-awaited getTally promise can reject independently of the
        // delta write; the caller still uses Promise properties as undefined.
        let observed = (|| {
            if !self.tally_static_initialized {
                return Err(type_error(
                    "Cannot read properties of undefined (reading 'has')",
                ));
            }
            if let Some((_, properties)) = self
                .tally
                .iter()
                .find(|(address, _)| same(address, &arg(&job.arguments, 0)))
            {
                let balance = funding_prop(properties, &text(&arg(&job.arguments, 2))?)?;
                if truth(&balance) {
                    for field in [
                        "amount",
                        "available",
                        "reserved",
                        "margin",
                        "vesting",
                        "channelBalance",
                    ] {
                        funding_prop(&balance, field)?;
                    }
                }
            }
            Ok(())
        })();
        match observed {
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
            _ => (),
        }
        if let Some(error) = job.failure.take() {
            return Err(Fault::Thrown(error));
        }
        let a = &job.arguments;
        let data = obj(&[
            ("address", arg(a, 0)),
            ("block", arg(a, 1)),
            ("property", arg(a, 2)),
            ("total", n(f64::NAN)),
            ("avail", arg(a, 4)),
            ("res", arg(a, 5)),
            ("mar", arg(a, 6)),
            ("vest", arg(a, 7)),
            ("channel", arg(a, 8)),
            ("type", arg(a, 9)),
            (
                "tx",
                if truth(&arg(a, 10)) {
                    arg(a, 10)
                } else {
                    s("")
                },
            ),
        ]);
        if truth(&job.existing) {
            self.db(
                "tallyMapDelta",
                "updateAsync",
                vec![
                    obj(&[("_id", s(&job.key))]),
                    obj(&[("$set", obj(&[("data", data)]))]),
                ],
            )?;
        } else {
            self.db(
                "tallyMapDelta",
                "insertAsync",
                vec![obj(&[("_id", s(&job.key)), ("data", data)])],
            )?;
        }
        self.tally_mod = V::Bool(true);
        job.phase = 2;
        Ok(V::Undefined)
    }
    fn source_clearing_record_tally_delta(&mut self, a: &[V]) -> R<V> {
        let mut job = self.source_record_tally_start(a)?;
        self.source_record_tally_prefix(&mut job)?;
        self.source_record_tally_finish(&mut job)
    }
    fn source_supply_start_delta(&mut self, a: &[V]) -> R<()> {
        if self.source_supply_delta.is_some() {
            return Err(SourceRuleError::Backend.into());
        }
        match self.source_record_tally_start(a) {
            Ok(job) => self.source_supply_delta = Some(job),
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
        }
        Ok(())
    }
    fn source_supply_after_database(&mut self, name: &str) -> R<()> {
        if name != "insurance" {
            return Ok(());
        }
        let Some(mut job) = self.source_supply_delta.take() else {
            return Ok(());
        };
        if job.phase == 0 {
            self.source_record_tally_prefix(&mut job)?;
        }
        self.source_supply_delta = Some(job);
        Ok(())
    }
    fn source_supply_after_db(&mut self, name: &str, op: &str) -> R<()> {
        if name != "insurance" || op != "findOneAsync" {
            return Ok(());
        }
        let Some(mut job) = self.source_supply_delta.take() else {
            return Ok(());
        };
        if job.phase == 1 {
            match self.source_record_tally_finish(&mut job) {
                Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                Err(e) => return Err(e),
                _ => (),
            }
        } else {
            self.source_supply_delta = Some(job);
        }
        Ok(())
    }
    fn source_supply_drain_delta(&mut self) -> R<()> {
        let Some(mut job) = self.source_supply_delta.take() else {
            return Ok(());
        };
        if job.phase == 0 {
            self.source_record_tally_prefix(&mut job)?;
        }
        match self.source_record_tally_finish(&mut job) {
            Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
            Err(e) => return Err(e),
            _ => (),
        }
        Ok(())
    }
    fn source_clearing_property(&mut self, method: &str, _a: &[V]) -> R<V> {
        if method != "getPropertyIndex" {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        if self.property_cache.is_empty() {
            let result = (|| {
                self.database("propertyList")?;
                let row = self.find("propertyList", obj(&[("_id", s("propertyIndex"))]))?;
                let value = if truth(&row) && truth(&row.get("value")) {
                    match row.get("value") {
                        V::String(t) => parse(&t)?,
                        v => v,
                    }
                } else {
                    V::Array(vec![])
                };
                let mut entries = vec![];
                if let V::Array(rows) = value {
                    if rows.iter().all(|r| matches!(r,V::Array(v)if v.len()==2)) {
                        for pair in rows {
                            let v = array(&pair)?;
                            let key = arg(&v, 0);
                            let key = if matches!(&key,V::Number(bits)if f64::from_bits(*bits)==0.0)
                            {
                                n(0.0)
                            } else {
                                key
                            };
                            let value = arg(&v, 1);
                            if let Some((_, p)) = entries
                                .iter_mut()
                                .find(|(k, _)| oracle_source_same_key(k, &key))
                            {
                                *p = value;
                            } else {
                                entries.push((key, value));
                            }
                        }
                    }
                }
                self.property_cache = entries;
                Ok(())
            })();
            match result {
                Err(Fault::Thrown(_)) => (),
                Err(e) => return Err(e),
                _ => (),
            }
        }
        Ok(V::Array(
            self.property_cache
                .iter()
                .map(|(k, p)| {
                    Ok(obj(&[
                        ("id", k.clone()),
                        ("ticker", funding_prop(p, "ticker")?),
                        ("totalInCirculation", funding_prop(p, "totalInCirculation")?),
                        ("type", funding_prop(p, "type")?),
                    ]))
                })
                .collect::<R<Vec<_>>>()?,
        ))
    }
    fn source_clearing_vault_query(&mut self, method: &str, _a: &[V]) -> R<V> {
        if !matches!(
            method,
            "getTotalBalanceForProperty" | "getTotalOutstandingForProperty"
        ) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        if !self.vault_initialized {
            self.vault_initialized = true;
            self.database("vaults")?;
            let row = self.find("vaults", obj(&[]))?;
            // findOne returns one document, not the array required by source.
            // Imported documents cannot populate the constructor's Map here.
            if matches!(row, V::Array(_)) {
                return Err(SourceRuleError::InvalidInput.into());
            }
            self.database("syntheticTokens")?;
            match self.find("syntheticTokens", obj(&[])) {
                Err(Fault::Thrown(e)) => self.source_supply_rejection(e)?,
                Err(Fault::Local(e)) => return Err(e.into()),
                _ => (),
            }
        }
        Ok(V::BigNumber("0".into()))
    }
    fn source_clearing_tally_query(&mut self, method: &str, a: &[V]) -> R<V> {
        if method == "loadFeeCacheForProperty" {
            let result = (|| {
                self.database("feeCache")?;
                let rows = array(&self.db("feeCache", "findAsync", vec![obj(&[])])?)?;
                let prefix = format!("{}-", text(&arg(a, 0))?);
                let mut total = FundingBn::new(&n(0.0))?;
                for row in rows {
                    let id = funding_prop(&row, "_id")?;
                    let V::String(id) = id else {
                        return Err(type_error("doc._id.startsWith is not a function"));
                    };
                    if id.starts_with(&prefix) {
                        total = total
                            .op(FundingBn::new(&funding_or(row.get("value")))?, 0)?
                            .op(FundingBn::new(&funding_or(row.get("stash")))?, 0)?;
                    }
                }
                Ok(total.value())
            })();
            return funding_caught(result, V::BigNumber("0".into()));
        }
        self.tally_load_rows()?;
        if !self.tally_static_initialized {
            return Err(type_error(format!(
                "Cannot read properties of undefined (reading '{}')",
                if method == "getTotalForProperty" {
                    "entries"
                } else {
                    "values"
                }
            )));
        }
        let key = text(&arg(a, 0))?;
        if method == "getTotalForProperty" {
            let mut total = FundingBn::new(&n(0.0))?;
            for (_, properties) in &self.tally {
                let balance = funding_prop(properties, &key)?;
                if !truth(&balance) {
                    continue;
                }
                let mut parts = vec![];
                for field in ["available", "reserved", "margin", "channelBalance"] {
                    parts.push(FundingBn::new(&funding_or(balance.get(field)))?);
                }
                if parts.iter().any(|p| p.number().is_nan()) {
                    continue;
                }
                for p in parts {
                    total = total.op(p, 0)?;
                }
            }
            return Ok(total.value());
        }
        let fields = [
            "amount",
            "available",
            "reserved",
            "margin",
            "vesting",
            "channelBalance",
        ];
        let mut out = obj(&[]);
        for field in fields {
            let mut total = FundingBn::new(&n(0.0))?;
            for (_, properties) in &self.tally {
                let balance = funding_prop(properties, &key)?;
                if truth(&balance) {
                    total = total.op(FundingBn::new(&funding_or(balance.get(field)))?, 0)?;
                }
            }
            set(
                &mut out,
                field,
                n(total.q(8, FundingRounding::HalfUp)?.number()),
            )?;
        }
        Ok(out)
    }
    fn source_clearing_verify_net(&mut self) -> R<V> {
        let contracts = self.call("Contract.getAllContracts", vec![])?;
        let mut net = FundingBn::new(&n(0.0))?;
        for contract in array(&contracts)? {
            let map = self.call("Margin.loadMarginMap", vec![funding_prop(&contract, "id")?])?;
            let positions = self.call("Margin.getAllPositions", vec![map])?;
            for pos in array(&positions)? {
                net = net.op(FundingBn::new(&funding_prop(&pos, "contracts")?)?, 0)?;
            }
        }
        Ok(n(net.number()))
    }
    fn source_clearing_supply(&mut self, a: &[V]) -> R<V> {
        let properties = self.call("Property.getPropertyIndex", vec![])?;
        let sats = |v: V| {
            FundingBn::new(&funding_or(v))?
                .op(FundingBn::new(&n(100_000_000.0))?, 2)?
                .q(0, FundingRounding::HalfUp)
        };
        let from_sats = |v: FundingBn| v.op(FundingBn::new(&n(100_000_000.0))?, 3);
        for property in array(&properties)? {
            let id = property.get("id");
            let mut total = FundingBn::new(&n(0.0))?;
            for target in [
                "Tally.getTotalForProperty",
                "Tally.loadFeeCacheForProperty",
                "Insurance.getTotalBalanceForProperty",
            ] {
                total = total.op(sats(self.call(target, vec![id.clone()])?)?, 0)?;
            }
            if matches!(id, V::Number(_)) {
                total = total.op(
                    sats(self.call("Vaults.getTotalBalanceForProperty", vec![id.clone()])?)?,
                    0,
                )?;
            }
            if same(&id, &n(1.0)) || same(&id, &n(4.0)) {
                let vest = self.call(
                    "Tally.getTotalTally",
                    vec![if same(&id, &n(1.0)) { n(2.0) } else { n(3.0) }],
                )?;
                total = total.op(sats(vest.get("vesting"))?, 0)?;
            }
            total = total.op(
                sats(self.call("PnlIou.getTotalForProperty", vec![id.clone()])?)?,
                0,
            )?;
            let mut expected = FundingBn::new(&property.get("totalInCirculation"))?.value();
            if matches!(&id,V::String(s)if s.starts_with("s-")) {
                expected = self.call("Vaults.getTotalOutstandingForProperty", vec![id.clone()])?;
            }
            let expected = sats(expected)?;
            let diff = total.clone().op(expected.clone(), 1)?;
            if diff.zero()
                || same(&id, &n(2.0))
                || same(&id, &n(3.0))
                || same(&id, &n(4.0))
                || same(&property.get("type"), &n(2.0))
            {
                continue;
            }
            let one = FundingBn::new(&n(1.0))?;
            if diff.clone().abs().compare(&one) == Some(core::cmp::Ordering::Greater) {
                let formatted = insurance_fixed_bn8(&from_sats(diff.clone())?)?;
                if number(&id)? == 5.0 {
                    self.call(
                        if diff.negative() {
                            "PnlIou.addLoss"
                        } else {
                            "PnlIou.addProfit"
                        },
                        vec![
                            n(0.0),
                            n(number(&id)?),
                            n(from_sats(diff.abs())?.number()),
                            arg(a, 0),
                        ],
                    )?;
                    continue;
                }
                return Err(error(format!("❌ Supply mismatch for Property {}, diff {}: Expected {}, Found {} on block {}",text(&id)?,formatted,insurance_fixed_bn8(&from_sats(expected)?)?,insurance_fixed_bn8(&from_sats(total)?)?,text(&arg(a,0))?)));
            } else if diff.compare(&FundingBn::new(&n(-1.0))?) == Some(core::cmp::Ordering::Equal) {
                let amount = from_sats(diff)?.number();
                self.source_supply_start_delta(&[
                    s("system"),
                    arg(a, 0),
                    id.clone(),
                    n(amount),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    n(0.0),
                    s("salvageDust"),
                    s(""),
                ])?;
                let deposited: R<()> = (|| {
                    let fund =
                        self.call("Insurance.getInstance", vec![id.clone(), V::Bool(false)])?;
                    self.call(
                        "Insurance.deposit",
                        vec![fund, s(text(&id)?), s("0.00000001"), arg(a, 0)],
                    )?;
                    Ok(())
                })();
                self.source_supply_drain_delta()?;
                deposited?;
            }
        }
        Ok(V::Undefined)
    }
    fn source_clearing_function(&mut self, a: &[V]) -> R<V> {
        self.call("Channels.removeEmptyChannels", vec![arg(a, 0)])?;
        self.call("Clearing.applyFundingRates", vec![arg(a, 0)])?;
        self.call("Clearing.makeSettlement", vec![arg(a, 0)])?;
        if self.contract_mod {
            let net = self.call("Clearing.verifyNetContracts", vec![])?;
            if !same(&net, &n(0.0)) {
                return Err(error(format!(
                    "❌ Clearing failed on block {}: Net contracts imbalance detected: {}",
                    text(&arg(a, 0))?,
                    text(&net)?
                )));
            }
            self.call("Contract.setModFlag", vec![V::Bool(false)])?;
        }
        if truth(&self.tally_mod) {
            self.call("Clearing.getTotalTokenBalances", vec![arg(a, 0)])?;
            self.call("Tally.setModFlag", vec![V::Bool(false)])?;
        }
        Ok(V::Undefined)
    }
}
fn insurance_fixed_bn8(value: &FundingBn) -> R<String> {
    if let FundingBn::Special(number) = value {
        return Ok(if number.is_nan() {
            "NaN"
        } else if *number < 0.0 {
            "-Infinity"
        } else {
            "Infinity"
        }
        .into());
    }
    let d = dec(&value.value())?;
    let text = q(&d, 8)?.to_string();
    let places = text.split('.').nth(1).map_or(0, str::len);
    Ok(if places == 0 {
        format!("{text}.00000000")
    } else {
        format!("{text}{}", "0".repeat(8 - places))
    })
}
