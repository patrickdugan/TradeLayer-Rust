enum SourceMainPrefixLog {
    MaxIndexed(V),
    AboutToInit(V),
    GenesisExisting(V),
    GenesisInserted(V),
    TallyMissing,
    ConsensusMissing,
    NoTransactions,
    Catchup(V, V),
    ParseHeight(V),
    FundingSkips(V),
    TradingSkips(V),
    Remaining(V, V, usize, usize),
    PartialReplay,
}
enum SourceMainCatchupTail {
    Complete,
    Sync,
}
// Explicit MainOrchestrationV12 only. Source metadata never duplicates balances.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceMainProduction {
    index: crate::txindex_source_owner::TxIndexSourceOwner,
    client: V,
    io: V,
    counts: Vec<(String, u64)>,
    genesis: V,
    parse_block: V,
    service: Option<V>,
}
struct MainIndexBackend<'a>(&'a mut ContractCore);
impl SourceBackend for MainIndexBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let result = if call.target == "ClientWrapper.getInstance" {
            let client = self
                .0
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .client
                .clone();
            self.0.emit(obj(&[
                ("kind", s("rpc-input")),
                ("method", s("Client.getInstance")),
                ("args", V::Array(call.arguments.clone())),
                ("returned", client.clone()),
            ]));
            Ok(obj(&[("$rpcClient", n(1.))]))
        } else if let Some(method) = call.target.strip_prefix("client.") {
            if call.arguments.first() != Some(&obj(&[("$rpcClient", n(1.))])) {
                return Err(SourceRuleError::InvalidInput);
            }
            self.0
                .source_main_production_io(&format!("Client.{method}"), &call.arguments[1..])
        } else {
            self.0.dispatch(call)
        };
        match result {
            Ok(v) => Ok(DependencyReturn::Pending(v)),
            Err(Fault::Thrown(e)) => Ok(DependencyReturn::PendingThrow(e)),
            Err(Fault::Local(e)) => Err(e),
        }
    }
}
impl ContractCore {
    fn main_production_enabled(&self) -> bool {
        matches!(
            self.policy,
            SourceContractRuntimePolicy::MainOrchestrationV12
                | SourceContractRuntimePolicy::RichThrownV14
        )
    }
    fn source_main_production_configure(&mut self, a: &[V]) -> R<V> {
        if a.len() != 1 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        if self.main_activation_profile != "source-registry" {
            return Err(SourceRuleError::InvalidInput.into());
        }
        if self.main_production.is_some() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let config = arg(a, 0);
        let V::Object(fields) = &config else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields
            .iter()
            .any(|(k, _)| !matches!(k.as_str(), "client" | "io" | "genesisBlock"))
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let client = config.get("client");
        let io = config.get("io");
        let V::Object(cf) = &client else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        let V::Object(iof) = &io else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if cf
            .iter()
            .any(|(k, _)| !matches!(k.as_str(), "chain" | "test"))
            || iof.iter().any(|(k, _)| {
                !matches!(k.as_str(), "height" | "blocks" | "networkInfo" | "failures")
            })
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let genesis = if config.get("genesisBlock") == V::Undefined {
            n(600000.)
        } else {
            config.get("genesisBlock")
        };
        let g = number(&genesis)?;
        if !g.is_finite() || g < 0. || g.fract() != 0. || g > 9_007_199_254_740_991. {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.main_production = Some(SourceMainProduction {
            index: crate::txindex_source_owner::TxIndexSourceOwner::new(),
            client,
            io,
            counts: vec![],
            genesis,
            parse_block: n(0.),
            service: None,
        });
        self.source_main_production_index("getInstance", vec![])?;
        Ok(V::Undefined)
    }
    fn source_main_production_index(&mut self, method: &str, args: Vec<V>) -> R<V> {
        self.source_main_production_index_scope(None, method, args)
    }
    fn source_main_production_index_scope(
        &mut self,
        instance: Option<u64>,
        method: &str,
        args: Vec<V>,
    ) -> R<V> {
        self.source_main_production_index_trace(instance, method, args)
            .map(|v| v.0)
    }
    fn source_main_production_index_trace(
        &mut self,
        instance: Option<u64>,
        method: &str,
        args: Vec<V>,
    ) -> R<(V, Vec<crate::validity_source::DependencyObservation>)> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.evaluate(
            instance,
            &SourceCall {
                method: method.into(),
                arguments: args,
            },
            &mut MainIndexBackend(self),
        );
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let result = result?;
        for e in result.deferred_rejections {
            self.main_height_rejections.push(e);
        }
        Ok((outcome(result.outcome)?, result.dependencies))
    }
    fn source_main_production_io(&mut self, method: &str, args: &[V]) -> R<V> {
        if self.source_main_service_enabled() {
            self.source_main_service_set("lastRpc", s(method))?;
        }
        let p = self
            .main_production
            .as_mut()
            .ok_or(SourceRuleError::UnknownDependency)?;
        let ordinal = if let Some((_, n)) = p.counts.iter_mut().find(|(k, _)| k == method) {
            *n = n.checked_add(1).ok_or(SourceRuleError::Capacity)?;
            *n
        } else {
            if p.counts.len() >= 16 {
                return Err(SourceRuleError::Capacity.into());
            }
            p.counts.push((method.into(), 1));
            1
        };
        let io = p.io.clone();
        let client = p.client.clone();
        let failures = match io.get("failures") {
            V::Undefined => vec![],
            V::Array(v) => v,
            _ => return Err(SourceRuleError::InvalidInput.into()),
        };
        let failure = failures
            .iter()
            .find(|f| f.get("method") == s(method) && f.get("ordinal") == n(ordinal as f64));
        if let Some(f) = failure {
            let name = if truth(&f.get("name")) {
                f.get("name")
            } else {
                s("Error")
            };
            let message = if f.get("message") == V::Undefined {
                s("")
            } else {
                s(source_channel_string(&f.get("message"))?)
            };
            let thrown = obj(&[
                ("name", name.clone()),
                ("message", message.clone()),
                ("code", f.get("code")),
            ]);
            self.emit(obj(&[
                ("kind", s("rpc-input")),
                ("method", s(method)),
                ("args", V::Array(args.to_vec())),
                ("ordinal", n(ordinal as f64)),
                ("thrown", thrown),
            ]));
            return Err(Fault::Thrown(crate::validity_source::SourceException {
                name: text(&name)?,
                message: text(&message)?,
            }));
        }
        let blocks = match io.get("blocks") {
            V::Undefined => vec![],
            V::Array(v) => v,
            _ => return Err(SourceRuleError::InvalidInput.into()),
        };
        let value = match method {
            "Client.getBlockCount" => io.get("height"),
            "Client.getNetworkInfo" => io.get("networkInfo"),
            "Client.getChain" => client.get("chain"),
            "Client.getTest" => client.get("test"),
            "Client.getBlockHash" => blocks
                .iter()
                .find(|b| b.get("height") == arg(args, 0))
                .map_or(V::Undefined, |b| b.get("hash")),
            "Client.getBlock" => blocks
                .iter()
                .find(|b| b.get("hash") == arg(args, 0))
                .map_or(V::Undefined, |b| b.get("block")),
            _ => return Err(SourceRuleError::UnknownDependency.into()),
        };
        self.emit(obj(&[
            ("kind", s("rpc-input")),
            ("method", s(method)),
            ("args", V::Array(args.to_vec())),
            ("ordinal", n(ordinal as f64)),
            ("returned", value.clone()),
        ]));
        Ok(value)
    }
    fn source_main_production_state(&self) -> Result<V, SourceRuleError> {
        let Some(p) = &self.main_production else {
            return Ok(V::Null);
        };
        let mut value = obj(&[
            ("index", p.index.state_value()?),
            ("client", p.client.clone()),
            (
                "rpcBindings",
                V::Array(vec![V::Array(vec![
                    obj(&[("$rpcClient", n(1.))]),
                    p.client.clone(),
                ])]),
            ),
            ("io", p.io.clone()),
            (
                "counts",
                V::Array(
                    p.counts
                        .iter()
                        .map(|(k, v)| V::Array(vec![s(k), n(*v as f64)]))
                        .collect(),
                ),
            ),
            ("genesisBlock", p.genesis.clone()),
            ("parseBlock", p.parse_block.clone()),
        ]);
        if let Some(service) = &p.service {
            set(&mut value, "service", service.clone())
                .map_err(|_| SourceRuleError::InvalidInput)?;
        }
        Ok(value)
    }
    fn source_main_block_volumes(&mut self, height: V) -> R<V> {
        let result = (|| {
            self.database("volumeIndex")?;
            let rows = array(&self.db(
                "volumeIndex",
                "findAsync",
                vec![obj(&[("value.blockHeight", height)])],
            )?)?;
            if rows.is_empty() {
                return Ok(n(0.));
            }
            let (mut ltc, mut total) = (
                crate::decimal::Decimal::zero(),
                crate::decimal::Decimal::zero(),
            );
            for row in rows {
                let value = dec(&funding_prop(&funding_prop(&row, "value")?, "ltcVolume")?)?;
                if text(&funding_prop(&row, "_id")?)?.contains('0') {
                    ltc = add(&ltc, &value)?;
                }
                total = add(&total, &value)?;
            }
            Ok(obj(&[("ltcPairs", dn(&ltc)?), ("global", dn(&total)?)]))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Undefined),
            other => other,
        }
    }
    fn source_main_update_vesting(&mut self, cumulative: V, block: V, height: V) -> R<()> {
        let p2 = self.call("Property.getPropertyData", vec![n(2.)])?;
        let p3 = self.call("Property.getPropertyData", vec![n(3.)])?;
        if !truth(&p2) || !truth(&p3) {
            return Ok(());
        }
        // Ordinary source Number addition is preserved; no BigNumber repair.
        let a = source_main_add_number(
            &cumulative.get("ltcPairTotalVolume"),
            &block.get("ltcPairs"),
        )?;
        let b = source_main_add_number(
            &cumulative.get("globalCumulativeVolume"),
            &block.get("global"),
        )?;
        for (id, volume, min, max, property) in [
            (2., a, 1000., 1_000_000_000., p2),
            (3., b, 10_000_000_000., 1_000_000_000_000., p3),
        ] {
            let fraction = if volume >= min {
                ((volume - min) / (max - min)).min(1.)
            } else {
                0.
            };
            let vesting = fraction * number(&property.get("totalInCirculation"))?;
            if vesting < 1e-8 {
                continue;
            }
            // Unchanged getAddressesWithBalanceForProperty calls the method
            // on an unawaited getDatabase Promise and catches that TypeError.
            // Its original asynchronous handle acquisition still executes.
            match self.database("tallyMap") {
                Ok(_) => (),
                Err(Fault::Thrown(e)) => self.main_height_rejections.push(e),
                Err(e) => return Err(e),
            };
            self.call("Property.getPropertyData", vec![n(id)])?;
            let _ = &height;
        }
        Ok(())
    }
}
impl SourceContractRuntime {
    fn source_main_production_dispatch(&mut self, target: &str, a: &[V]) -> R<V> {
        match target {
            "Main.getCurrentBlockHeight" => self
                .core
                .source_main_production_io("Client.getBlockCount", &[]),
            "Main.checkBlockLag" => self.source_main_production_lag(),
            "Main.checkSync" => {
                self.core
                    .source_main_production_index("getInstance", vec![])?;
                let p = self
                    .core
                    .main_production
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?;
                Ok(obj(&[
                    ("consensus", p.parse_block.clone()),
                    ("txIndex", n(0.)),
                ]))
            }
            "Main.blockHandlerEnd" => {
                self.core
                    .source_liquidity_cumulative_volumes(V::Undefined)?;
                // Actual getBlockVolumes returns0/an ordinary object/undefined.
                // None passes Main's original numeric >0 object guard.
                self.core.source_main_block_volumes(arg(a, 1))?;
                Ok(V::Undefined)
            }
            "Main.initOrLoadTxIndex" => {
                let max = self.source_main_production_init_prefix()?;
                self.source_main_production_sync_index(max)
            }
            "Main.syncIndex" => self.source_main_production_sync_index(arg(a, 0)),
            "Main.constructOrLoadConsensus" => {
                let g = self
                    .core
                    .main_production
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .genesis
                    .clone();
                self.source_main_production_catchup(g)
            }
            "Main.constructConsensusFromIndex" => self.source_main_production_catchup(arg(a, 0)),
            _ if self.core.source_main_service_enabled() => {
                self.source_main_service_dispatch(target, a)
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    fn source_main_production_lag(&mut self) -> R<V> {
        let tip = self
            .core
            .source_main_production_io("Client.getBlockCount", &[])?;
        let mut track = self.core.source_main_height_read("TrackHeight")?;
        if matches!(track, V::Null | V::Undefined) {
            track = self.core.source_main_height_read("MaxProcessedHeight")?;
        }
        Ok(obj(&[
            ("lag", n(number(&tip)? - number(&track)?)),
            ("chainTip", tip),
            ("maxTrack", track),
        ]))
    }
    fn source_main_production_init_prefix(&mut self) -> R<V> {
        self.source_main_production_init_prefix_observed(&mut |_, _| Ok(()))
    }
    fn source_main_production_init_prefix_observed<F>(&mut self, log: &mut F) -> R<V>
    where
        F: FnMut(&mut Self, SourceMainPrefixLog) -> R<()>,
    {
        let mut max = self
            .core
            .source_main_production_index("findMaxIndexedBlock", vec![])?;
        log(self, SourceMainPrefixLog::MaxIndexed(max.clone()))?;
        if same(&max, &n(0.)) || max == V::Null {
            let g = self
                .core
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .genesis
                .clone();
            log(self, SourceMainPrefixLog::AboutToInit(g.clone()))?;
            let (_, dependencies) = self.core.source_main_production_index_trace(
                None,
                "initializeIndex",
                vec![g.clone()],
            )?;
            // This branch comes from the actual same-owner dependency receipt,
            // never another query or a guessed final-state/fixture result.
            let found = dependencies
                .iter()
                .find(|d| d.call.target == "db.findOneAsync");
            let existing = found
                .and_then(|d| match &d.returned {
                    DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                        Some(v.clone())
                    }
                    _ => None,
                })
                .ok_or(SourceRuleError::Backend)?;
            if truth(&existing) {
                log(
                    self,
                    SourceMainPrefixLog::GenesisExisting(existing.get("value")),
                )?;
            } else {
                log(self, SourceMainPrefixLog::GenesisInserted(g.clone()))?;
            }
            max = g;
        }
        Ok(max)
    }
    fn source_main_production_sync_index(&mut self, max: V) -> R<V> {
        if max == V::Null {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let V::Number(bits) = max else {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        };
        let max_number = f64::from_bits(bits);
        if !max_number.is_finite()
            || max_number < 0.
            || max_number.fract() != 0.
            || max_number > 9_007_199_254_740_991.
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let tip = match self
            .core
            .source_main_production_io("Client.getBlockCount", &[])
        {
            Ok(v) => v,
            Err(Fault::Thrown(_)) => return Ok(V::Undefined),
            Err(e) => return Err(e),
        };
        let V::Number(tip_bits) = tip else {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        };
        let tip_number = f64::from_bits(tip_bits);
        if !tip_number.is_finite()
            || tip_number < 0.
            || tip_number.fract() != 0.
            || tip_number > 9_007_199_254_740_991.
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        let awaited = if tip_number > max_number && max_number != 0. {
            Some(vec![V::Number(bits)])
        } else if max_number == 0. {
            Some(vec![self
                .core
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .genesis
                .clone()])
        } else {
            None
        };
        if let Some(args) = awaited {
            return match self
                .core
                .source_main_production_index("extractBlockData", args)
            {
                Err(Fault::Thrown(_)) => Ok(V::Undefined),
                other => other,
            };
        }
        if max_number == tip_number {
            // Original returned catchup Promise is not awaited in this try.
            // Its later rejection must escape the source catch.
            let genesis = self
                .core
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .genesis
                .clone();
            return self.source_main_production_catchup(genesis);
        }
        Ok(V::Undefined)
    }
    fn source_main_production_catchup(&mut self, start: V) -> R<V> {
        if matches!(
            self.source_main_production_catchup_prefix(start)?,
            SourceMainCatchupTail::Complete
        ) {
            return Ok(V::Undefined);
        }
        if self.core.source_main_service_enabled() {
            return self.source_main_service_sync(&[]);
        }
        self.source_main_production_lag()?;
        // Actual success here starts processIncomingBlocks without awaiting it.
        // This finite phase profile has no stable boundary for that live loop.
        Err(SourceRuleError::UnknownDependency.into())
    }
    fn source_main_production_catchup_prefix(&mut self, start: V) -> R<SourceMainCatchupTail> {
        self.source_main_production_catchup_prefix_observed(start, &mut |_, _| Ok(()))
    }
    fn source_main_production_catchup_prefix_observed<F>(
        &mut self,
        start: V,
        log: &mut F,
    ) -> R<SourceMainCatchupTail>
    where
        F: FnMut(&mut Self, SourceMainPrefixLog) -> R<()>,
    {
        if !matches!(&start,V::Number(bits) if f64::from_bits(*bits).is_finite() && f64::from_bits(*bits)>=0. && f64::from_bits(*bits).fract()==0.)
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        self.core
            .source_main_production_index("findMaxIndexedBlock", vec![])?;
        self.core.database("txIndex")?;
        self.core.source_send_tally_instance_initialized = true;
        // Original Tally.getInstance is unawaited: its load factory executes
        // before Main's consensus factory, and its read resumes at that await.
        let tally_pending = match self.core.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        let consensus_factory = self.core.database("consensus");
        let mut tally_missing = false;
        if tally_pending {
            let loaded = self.core.find("tallyMap", obj(&[("_id", s("tallyMap"))]));
            if let Ok(row) = &loaded {
                tally_missing = !truth(row) || !truth(&row.get("data"));
            }
            match loaded.and_then(|row| ContractCore::tally_parsed_entries(&row)) {
                Ok(entries) => {
                    self.core.tally = entries;
                    self.core.tally_static_initialized = true;
                }
                Err(Fault::Thrown(_)) => (),
                Err(e) => return Err(e),
            }
        }
        consensus_factory?;
        let consensus_row = self
            .core
            .find("consensus", obj(&[("_id", s("MaxProcessedHeight"))]))?;
        // Unawaited Tally.loadFromDB's log resumes after Main has initiated
        // its awaited consensus find, as in the original Promise graph.
        if tally_missing {
            log(self, SourceMainPrefixLog::TallyMissing)?;
        }
        if !truth(&consensus_row) {
            log(self, SourceMainPrefixLog::ConsensusMissing)?;
        }
        let all = array(&self.core.db("txIndex", "findAsync", vec![obj(&[])])?)?;
        let mut grouped: Vec<(f64, Vec<V>, Vec<V>)> = vec![];
        for row in all {
            let id = funding_prop(&row, "_id")?;
            let V::String(id) = id else {
                return Err(type_error("txData._id.startsWith is not a function"));
            };
            if !id.starts_with("tx-") {
                continue;
            }
            let raw = id.split('-').nth(1).unwrap_or("");
            let h = source_main_parse_int10(raw);
            let pos = grouped
                .iter()
                .position(|(n, _, _)| *n == h || n.is_nan() && h.is_nan())
                .unwrap_or_else(|| {
                    grouped.push((h, vec![], vec![]));
                    grouped.len() - 1
                });
            let value = row.get("value");
            if !truth(&value) || !truth(&value.get("payload")) {
                continue;
            }
            let payload = funding_prop(&value, "payload")?;
            let V::String(payload) = payload else {
                return Err(type_error("payload.slice is not a function"));
            };
            let kind = payload.chars().next().and_then(|c| c.to_digit(36));
            if matches!(kind, Some(4 | 20)) {
                grouped[pos].1.push(row);
            } else {
                grouped[pos].2.push(row);
            }
        }
        let last = grouped
            .iter()
            .map(|(h, _, _)| *h)
            .fold(f64::NEG_INFINITY, |a, b| {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                }
            });
        if grouped.is_empty() || last == 0. || last.is_nan() {
            log(self, SourceMainPrefixLog::NoTransactions)?;
            return Ok(SourceMainCatchupTail::Complete);
        }
        log(self, SourceMainPrefixLog::Catchup(n(last), start.clone()))?;
        let mut h = number(&start)?;
        const MAX_FINITE_MAIN_HEIGHT: f64 = 9_007_199_254_740_991.;
        if h > MAX_FINITE_MAIN_HEIGHT || last > MAX_FINITE_MAIN_HEIGHT {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        if !h.is_finite() || h.fract() != 0. || h < 0. || !last.is_finite() || last - h > 1024. {
            return Err(SourceRuleError::Capacity.into());
        }
        let mut max = h - 1.;
        let mut iterations = 0usize;
        while h <= last {
            if iterations >= crate::txindex_source_owner::MAX_BLOCKS {
                return Err(SourceRuleError::Capacity.into());
            }
            iterations += 1;
            self.core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .parse_block = n(h);
            if h % 10000. == 1. {
                log(self, SourceMainPrefixLog::ParseHeight(n(h)))?;
            }
            let mut skip = true;
            if let Some((_, funding, trading)) = grouped.iter().find(|(g, _, _)| *g == h) {
                let one = self.source_main_process_rows(V::Array(funding.clone()), n(h), false)?;
                log(self, SourceMainPrefixLog::FundingSkips(one.clone()))?;
                let two = self.source_main_process_rows(V::Array(trading.clone()), n(h), false)?;
                log(self, SourceMainPrefixLog::TradingSkips(two.clone()))?;
                skip = number(&one)? + number(&two)? >= (funding.len() + trading.len()) as f64;
                if !skip {
                    log(
                        self,
                        SourceMainPrefixLog::Remaining(
                            one.clone(),
                            two.clone(),
                            funding.len(),
                            trading.len(),
                        ),
                    )?;
                    if number(&one)? + number(&two)? > 0. {
                        log(self, SourceMainPrefixLog::PartialReplay)?;
                    }
                }
            }
            if h % 10000. == 0. {
                skip = false;
            }
            if !skip {
                for target in [
                    "Tally.flushQueuedFeeAccruals",
                    "Logic.settleLiquidityRewards",
                ] {
                    self.source_main_phase(target, vec![n(h)])?;
                }
                for target in [
                    "Orderbook.processQueuedOnChainOrdersForBlock",
                    "Orderbook.processQueuedChannelTradesForBlock",
                ] {
                    self.core.emit(obj(&[
                        ("kind", s("main-phase")),
                        ("target", s(target)),
                        ("args", V::Array(vec![n(h)])),
                    ]));
                    self.evaluate_queue(&SourceCall {
                        method: target.strip_prefix("Orderbook.").unwrap().into(),
                        arguments: vec![n(h)],
                    })
                    .map_err(Fault::Local)
                    .and_then(outcome)?;
                }
                let cumulative = self.core.source_liquidity_cumulative_volumes(n(h))?;
                let volume = self.core.source_main_block_volumes(n(h))?;
                if number(&funding_prop(&volume, "global")?)? > 0. {
                    self.core
                        .source_main_update_vesting(cumulative, volume, n(h))?;
                }
                self.source_main_phase("Channels.processWithdrawals", vec![n(h)])?;
                self.source_main_phase("Clearing.clearingFunction", vec![n(h), V::Bool(false)])?;
            }
            max = h;
            let next = h + 1.;
            if next <= h || !next.is_finite() {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
            h = next;
        }
        self.core
            .source_main_height_write("MaxProcessedHeight", n(max))?;
        Ok(SourceMainCatchupTail::Sync)
    }
}
fn source_main_parse_int10(value: &str) -> f64 {
    use crate::decimal::Decimal;
    let value = value.trim_start_matches(|c:char| matches!(u32::from(c), 0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff));
    let (negative, rest) = if let Some(v) = value.strip_prefix('-') {
        (true, v)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        let length = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
        if length == 0 {
            return f64::NAN;
        }
        let digits = hex[..length].trim_start_matches('0');
        // Parse the integer exactly and round only once. Repeated binary64
        // multiplication/addition can double-round long hexadecimal prefixes.
        // At most 256 significant hex digits need conversion: any larger
        // integer exceeds binary64, while 256 digits fit in 309 decimal digits,
        // well within Decimal's 4,096-digit exact arithmetic bound.
        let value = if digits.len() > 256 {
            f64::INFINITY
        } else {
            let mut integer = Decimal::zero();
            for byte in digits.bytes() {
                let digit = if byte.is_ascii_digit() {
                    byte - b'0'
                } else {
                    byte.to_ascii_lowercase() - b'a' + 10
                };
                let Ok(next) = integer
                    .checked_mul(&Decimal::from(16))
                    .and_then(|v| v.checked_add(&Decimal::from(i128::from(digit))))
                else {
                    return f64::NAN;
                };
                integer = next;
            }
            integer.to_string().parse().unwrap_or(f64::INFINITY)
        };
        return if negative { -value } else { value };
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        f64::NAN
    } else {
        let n = digits.parse::<f64>().unwrap_or(f64::INFINITY);
        if negative {
            -n
        } else {
            n
        }
    }
}

fn source_main_add_number(a: &V, b: &V) -> R<f64> {
    if matches!(a, V::String(_)) || matches!(b, V::String(_)) {
        number(&s(format!("{}{}", text(a)?, text(b)?)))
    } else {
        Ok(number(a)? + number(b)?)
    }
}
