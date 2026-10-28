// Opt-in finite original Main/Persistence composition. Only Core owns rows.
impl ContractCore {
    fn source_main_service_enabled(&self) -> bool {
        self.main_production
            .as_ref()
            .is_some_and(|p| p.service.is_some())
    }
    fn source_main_service_get(&self, k: &str) -> R<V> {
        Ok(self
            .main_production
            .as_ref()
            .and_then(|p| p.service.as_ref())
            .ok_or(SourceRuleError::UnknownDependency)?
            .get(k))
    }
    fn source_main_service_set(&mut self, k: &str, v: V) -> R<()> {
        set(
            self.main_production
                .as_mut()
                .and_then(|p| p.service.as_mut())
                .ok_or(SourceRuleError::UnknownDependency)?,
            k,
            v,
        )
    }
    fn source_main_service_configure(&mut self, a: &[V]) -> R<V> {
        if !self.main_production_enabled() || self.source_main_service_enabled() || a.len() != 1 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let config = arg(a, 0);
        let V::Object(fields) = &config else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields.iter().any(|(k, _)| k != "fileInputs") {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let mut files: Vec<V> = vec![];
        for row in match config.get("fileInputs") {
            V::Undefined => vec![],
            V::Array(v) => v,
            _ => return Err(SourceRuleError::InvalidInput.into()),
        } {
            let path = text(&row.get("path"))?;
            let bytes = row.get("bytes");
            if !path.starts_with("/tl-data/")
                || path
                    .split('/')
                    .skip(1)
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || !matches!(bytes, V::String(_))
            {
                return Err(SourceRuleError::InvalidInput.into());
            }
            if let Some(old) = files
                .iter_mut()
                .find(|p| arg(&array(p).unwrap_or_default(), 0) == s(&path))
            {
                *old = V::Array(vec![s(path), bytes]);
            } else {
                files.push(V::Array(vec![s(path), bytes]));
            }
        }
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .service = Some(obj(&[
            ("persistence", V::Null),
            ("directories", V::Array(vec![s("/tl-data")])),
            ("files", V::Array(files)),
            ("extraOwn", obj(&[])),
            ("lastRpc", V::Undefined),
            ("pause", V::Bool(false)),
        ]));
        Ok(V::Undefined)
    }
    fn source_main_service_timer(&mut self, delay: V) {
        self.emit(obj(&[("kind", s("timer")), ("delay", delay)]));
    }
    fn source_main_service_client(&mut self, args: Vec<V>) -> R<V> {
        let client = self
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .client
            .clone();
        self.emit(obj(&[
            ("kind", s("rpc-input")),
            ("method", s("Client.getInstance")),
            ("args", V::Array(args)),
            ("returned", client.clone()),
        ]));
        Ok(client)
    }
    fn source_main_service_mkdir(&mut self, path: String) -> R<()> {
        let mut directories = array(&self.source_main_service_get("directories")?)?;
        let mut part = String::new();
        for item in path.split('/').filter(|p| !p.is_empty()) {
            part.push('/');
            part.push_str(item);
            if !directories.contains(&s(&part)) {
                directories.push(s(&part));
            }
        }
        self.source_main_service_set("directories", V::Array(directories))?;
        self.emit(obj(&[
            ("kind", s("filesystem")),
            ("method", s("mkdir")),
            (
                "args",
                V::Array(vec![s(path), obj(&[("recursive", V::Bool(true))])]),
            ),
            ("returned", V::Undefined),
        ]));
        Ok(())
    }
    fn source_main_service_persistence(&mut self) -> R<V> {
        let existing = self.source_main_service_get("persistence")?;
        if existing != V::Null {
            return Ok(existing);
        }
        let mut p = obj(&[
            ("rawNetwork", V::Null),
            ("rawTestFlag", V::Null),
            ("snapshotInterval", n(1000.)),
            ("networkFull", V::Null),
            ("dbDir", V::Null),
            ("snapshotsDir", V::Null),
            ("backupsDir", V::Null),
            ("client", V::Null),
        ]);
        self.source_main_service_set("persistence", p.clone())?;
        let client = self.source_main_service_client(vec![V::Bool(true)])?;
        set(&mut p, "client", client)?;
        self.source_main_service_set("persistence", p.clone())?;
        let chain = self.source_main_production_io("Client.getChain", &[])?;
        let test = self.source_main_production_io("Client.getTest", &[])?;
        let V::String(chain) = chain else {
            if matches!(chain, V::Null | V::Undefined) {
                return Err(type_error(format!(
                    "Cannot read properties of {} (reading 'toLowerCase')",
                    text(&chain)?
                )));
            }
            return Err(type_error("chain.toLowerCase is not a function"));
        };
        let chain = chain.to_lowercase().trim_matches(|c:char| matches!(u32::from(c),0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)).to_string();
        if chain.is_empty() || chain.contains(['/', '\\']) || chain == "." || chain == ".." {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let network = format!("{chain}-{}", if truth(&test) { "test" } else { "main" });
        set(&mut p, "networkFull", s(&network))?;
        for (key, suffix) in [
            ("dbDir", ""),
            ("snapshotsDir", "-snapshots"),
            ("backupsDir", "-backups"),
        ] {
            set(&mut p, key, s(format!("/tl-data/{network}{suffix}")))?;
        }
        self.source_main_service_set("persistence", p.clone())?;
        for key in ["dbDir", "snapshotsDir", "backupsDir"] {
            self.source_main_service_mkdir(text(&p.get(key))?)?;
        }
        Ok(p)
    }
    fn source_main_service_error_code(&self) -> R<V> {
        let method = self.source_main_service_get("lastRpc")?;
        let p = self
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?;
        let count = p
            .counts
            .iter()
            .find(|(m, _)| s(m) == method)
            .map_or(0, |(_, n)| *n);
        Ok(match p.io.get("failures") {
            V::Array(rows) => rows
                .iter()
                .find(|f| f.get("method") == method && f.get("ordinal") == n(count as f64))
                .map_or(V::Undefined, |f| {
                    if f.get("message") != V::Undefined
                        && source_channel_string(&f.get("message")).is_err()
                    {
                        V::Undefined
                    } else {
                        f.get("code")
                    }
                }),
            _ => V::Undefined,
        })
    }
    fn source_main_service_network(&mut self) -> R<V> {
        let result = (|| {
            let info = self.source_main_production_io("Client.getNetworkInfo", &[])?;
            let active = funding_prop(&info, "networkactive")?;
            let connections = funding_prop(&info, "connections")?;
            Ok(if !truth(&active) {
                obj(&[
                    ("status", V::Bool(false)),
                    ("reason", s("Network inactive")),
                ])
            } else if same(&connections, &n(0.)) {
                obj(&[("status", V::Bool(false)), ("reason", s("No connections"))])
            } else {
                obj(&[("status", V::Bool(true)), ("connections", connections)])
            })
        })();
        match result {
            Err(Fault::Thrown(e)) => {
                let code = self.source_main_service_error_code()?;
                if code == s("ECONNREFUSED") || code == s("ETIMEDOUT") {
                    Ok(obj(&[
                        ("status", V::Bool(false)),
                        ("reason", s("Connection refused or timeout")),
                    ]))
                } else {
                    Err(Fault::Thrown(e))
                }
            }
            other => other,
        }
    }
    fn source_main_service_reorg_check(&mut self, height: V, incoming: V) -> R<V> {
        let result = (|| {
            let prev = number(&height)? - 1.;
            if prev <= 0. {
                return Ok(V::Bool(false));
            }
            self.database("persistence")?;
            let row = self.find("persistence", obj(&[("height", n(prev))]))?;
            let local = if truth(&row.get("hash")) {
                row.get("hash")
            } else {
                V::Null
            };
            if local != V::Null && local == incoming {
                return Ok(V::Bool(false));
            }
            let canonical = (|| {
                let hash = self.source_main_production_io("Client.getBlockHash", &[n(prev)])?;
                let block = self.source_main_production_io("Client.getBlock", &[hash])?;
                funding_prop(&block, "hash")
            })();
            // Original undefined logger.warn throws inside the inner RPC catch;
            // the enclosing source catch returns false.
            let canonical = match canonical {
                Ok(v) => v,
                Err(Fault::Thrown(_)) => return Ok(V::Bool(false)),
                Err(e) => return Err(e),
            };
            if canonical != local {
                return Ok(obj(&[
                    ("reorg", V::Bool(true)),
                    ("depth", n(1.)),
                    ("expected", canonical),
                    ("received", local),
                ]));
            }
            Ok(V::Bool(canonical != incoming))
        })();
        match result {
            Err(Fault::Thrown(_)) => Ok(V::Bool(false)),
            other => other,
        }
    }
    fn source_main_service_reorg_no_snapshots(&mut self) -> R<V> {
        self.database("persistence")?;
        let mut rows = array(&self.db(
            "persistence",
            "findAsync",
            vec![obj(&[("height", obj(&[("$exists", V::Bool(true))]))])],
        )?)?;
        rows.sort_by(|a, b| {
            number(&a.get("height"))
                .unwrap_or(f64::NAN)
                .partial_cmp(&number(&b.get("height")).unwrap_or(f64::NAN))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(last) = rows.last().cloned() else {
            return Ok(V::Null);
        };
        let mut h = number(&last.get("height"))?;
        let canonical = self.source_main_production_io("Client.getBlockHash", &[n(h)])?;
        if canonical == last.get("hash") {
            return Ok(V::Null);
        }
        let mut ancestor = 0.;
        let mut count = 0;
        while h > 0. {
            if h > 9_007_199_254_740_991. || !h.is_finite() {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
            if count >= 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            count += 1;
            self.database("persistence")?;
            let local = self.find(
                "persistence",
                obj(&[("_id", s(format!("block-{}", text(&n(h))?)))]),
            )?;
            let node = self.source_main_production_io("Client.getBlockHash", &[n(h)])?;
            if truth(&local) && local.get("hash") == node {
                ancestor = h;
                break;
            }
            h -= 1.;
        }
        let p = self.source_main_service_get("persistence")?;
        let dir = text(&p.get("snapshotsDir"))?;
        let prefix = format!("{dir}/");
        let directories = array(&self.source_main_service_get("directories")?)?;
        let files = array(&self.source_main_service_get("files")?)?;
        let mut keys = directories.clone();
        for file in files {
            keys.push(arg(&array(&file)?, 0));
        }
        let mut names = vec![];
        for key in keys {
            let path = text(&key)?;
            if let Some(tail) = path.strip_prefix(&prefix) {
                if !tail.is_empty() && !tail.contains('/') && !names.contains(&s(tail)) {
                    names.push(s(tail));
                }
            }
        }
        if names.iter().any(|name| {
            directories.contains(&s(format!("{dir}/{}", text(name).unwrap_or_default())))
        }) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        self.emit(obj(&[
            ("kind", s("filesystem")),
            ("method", s("readdir")),
            (
                "args",
                V::Array(vec![s(dir), obj(&[("withFileTypes", V::Bool(true))])]),
            ),
            ("returned", V::Array(names)),
        ]));
        Ok(obj(&[
            ("restoredFrom", n(0.)),
            ("commonAncestor", n(ancestor)),
            ("lastLocal", last.get("height")),
        ]))
    }
}
impl SourceContractRuntime {
    fn source_main_service_fetch(&mut self, a: &[V]) -> R<V> {
        let retry_value = if arg(a, 1) == V::Undefined {
            n(3.)
        } else {
            arg(a, 1)
        };
        let retries = number(&retry_value)?;
        let delay = if arg(a, 2) == V::Undefined {
            1000.
        } else {
            number(&arg(a, 2))?
        };
        let mut attempt = 0.;
        let mut iterations = 0usize;
        while attempt <= retries {
            if iterations >= 1024 {
                return Err(SourceRuleError::Capacity.into());
            }
            iterations += 1;
            match self
                .core
                .source_main_production_index("fetchBlockData", vec![arg(a, 0)])
            {
                Ok(v) => return Ok(v),
                Err(Fault::Thrown(e)) => {
                    if self.core.source_main_service_error_code()? != s("ETIMEDOUT")
                        || same(&retry_value, &n(attempt))
                    {
                        return Err(Fault::Thrown(e));
                    }
                    self.core
                        .source_main_service_timer(n(delay * 2_f64.powf(attempt)));
                }
                Err(e) => return Err(e),
            }
            attempt += 1.;
        }
        Ok(V::Undefined)
    }
    fn source_main_service_begin(&mut self, block: V, height: V) -> R<V> {
        self.core.source_main_service_persistence()?;
        let reorg = self.core.source_main_service_reorg_check(
            height.clone(),
            funding_prop(&block, "previousblockhash")?,
        )?;
        if truth(&reorg) {
            let info = self.core.source_main_service_reorg_no_snapshots()?;
            if matches!(info.get("restoredFrom"), V::Number(_)) {
                return Ok(info.get("restoredFrom"));
            }
        }
        self.core.database("persistence")?;
        self.core.update(
            "persistence",
            obj(&[("_id", s(format!("block-{}", text(&height)?)))]),
            obj(&[(
                "$set",
                obj(&[
                    ("height", height.clone()),
                    ("hash", funding_prop(&block, "hash")?),
                    ("prevHash", funding_prop(&block, "previousblockhash")?),
                    ("createdAt", n(self.core.time_millis as f64)),
                ]),
            )]),
            true,
        )?;
        if number(&height)? % 1000. == 0. {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let txs = array(
            &self
                .core
                .source_main_production_index("processBlockData", vec![block, height.clone()])?,
        )?;
        let (mut funding, mut other) = (vec![], vec![]);
        for tx in txs {
            let payload = funding_prop(&tx, "payload")?;
            let V::String(payload) = payload else {
                return Err(type_error("tx.payload.startsWith is not a function"));
            };
            if payload.starts_with('4') || payload.starts_with('m') {
                funding.push(tx);
            } else {
                other.push(tx);
            }
        }
        if !funding.is_empty() {
            self.source_main_process_rows(V::Array(funding), height, true)?;
        }
        Ok(V::Array(other))
    }
    fn source_main_service_block(&mut self, block: V, height: V) -> R<V> {
        let tx = self.source_main_service_begin(block.clone(), height.clone())?;
        if matches!(tx, V::Number(_)) {
            return Ok(obj(&[("reOrg", V::Bool(true)), ("restore", tx)]));
        }
        let height = self.source_main_mid(&[tx, height])?;
        self.source_main_production_dispatch(
            "Main.blockHandlerEnd",
            &[funding_prop(&block, "hash")?, height.clone()],
        )?;
        Ok(obj(&[("reOrg", V::Bool(false)), ("restore", height)]))
    }
    fn source_main_service_initialize(&mut self) -> R<V> {
        self.core.source_main_service_timer(n(1500.));
        let client = self
            .core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .client
            .clone();
        if !truth(&client.get("chain")) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        self.core
            .source_main_production_index("getInstance", vec![])?;
        let test = self.core.source_main_production_io("Client.getTest", &[])?;
        let mut extra = self.core.source_main_service_get("extraOwn")?;
        set(&mut extra, "test", test.clone())?;
        self.core
            .source_main_service_set("extraOwn", extra.clone())?;
        let chain = self
            .core
            .source_main_production_io("Client.getChain", &[])?;
        set(&mut extra, "chain", chain.clone())?;
        self.core.source_main_service_set("extraOwn", extra)?;
        let genesis = if test == V::Undefined || chain == V::Undefined {
            None
        } else {
            match text(&chain)?.as_str() {
                "BTC" => Some(if truth(&test) { 3520000. } else { 880000. }),
                "DOGE" => Some(if truth(&test) { 6815000. } else { 5520000. }),
                "LTC" => Some(if truth(&test) { 3082500. } else { 2819000. }),
                _ => None,
            }
        };
        if let Some(h) = genesis {
            self.core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .genesis = n(h);
        }
        let genesis = self
            .core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .genesis
            .clone();
        match self.core.source_main_production_index_scope(
            Some(1),
            "initializeOrLoadDB",
            vec![genesis.clone()],
        ) {
            Ok(_) | Err(Fault::Thrown(_)) => (),
            Err(e) => return Err(e),
        }
        if !truth(
            &self
                .core
                .source_main_production_index("checkForIndex", vec![])?,
        ) {
            self.source_main_production_dispatch("Main.initOrLoadTxIndex", &[])?;
        }
        self.source_main_production_catchup(genesis)
    }
    fn source_main_service_recovery(&mut self) -> R<V> {
        for _ in 0..1024 {
            if truth(&self.core.source_main_service_network()?.get("status")) {
                let track = self.core.source_main_height_read("TrackHeight")?;
                self.core.source_main_height_read("MaxProcessedHeight")?;
                return Ok(track);
            }
            self.core.source_main_service_timer(n(10000.));
        }
        Err(SourceRuleError::Capacity.into())
    }
    fn source_main_service_live(&mut self, a: &[V]) -> R<V> {
        let mut latest = arg(a, 1);
        if !matches!(latest, V::Number(_) | V::Null | V::Undefined) {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        for _ in 0..1024 {
            let tip = self
                .core
                .source_main_production_io("Client.getBlockCount", &[])?;
            let track = self.core.source_main_height_read("TrackHeight")?;
            if number(&track)? > number(&latest)? {
                if !matches!(track, V::Number(_) | V::Null | V::Undefined) {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                latest = track;
            }
            let mut block_number = number(&latest)? + 1.;
            let max = number(&tip)?;
            let mut block_iterations = 0usize;
            while block_number <= max {
                if block_number > 9_007_199_254_740_991.
                    || max > 9_007_199_254_740_991.
                    || block_number < 0.
                    || block_number.fract() != 0.
                    || max < 0.
                    || max.fract() != 0.
                {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                if !block_number.is_finite() || !max.is_finite() || max - block_number > 1024. {
                    return Err(SourceRuleError::Capacity.into());
                }

                if block_iterations >= 1024 {
                    return Err(SourceRuleError::Capacity.into());
                }
                block_iterations += 1;
                if !truth(&self.core.source_main_service_network()?.get("status")) {
                    block_number = number(&self.source_main_service_recovery()?)?;
                }
                let block = self.source_main_service_fetch(&[n(block_number)])?;
                let result = self.source_main_service_block(block, n(block_number))?;
                if result.get("reOrg") == V::Bool(true) {
                    return self.source_main_service_sync(&[result.get("restore")]);
                }
                self.core
                    .source_main_height_write("TrackHeight", n(block_number))?;
                block_number += 1.;
            }
            if truth(&self.core.source_main_service_get("pause")?) {
                return Err(SourceRuleError::UnknownDependency.into());
            }
            self.core.source_main_service_timer(n(10000.));
            self.core.source_main_height_write("TrackHeight", tip)?;
        }
        Err(SourceRuleError::Capacity.into())
    }
    fn source_main_service_sync(&mut self, a: &[V]) -> R<V> {
        let mut lag = self.source_main_production_lag()?;
        if truth(&arg(a, 0)) {
            set(&mut lag, "maxTrack", arg(a, 0))?;
        }
        if truth(&self.core.source_main_service_get("pause")?) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        match self.source_main_service_live(&[
            lag.get("lag"),
            lag.get("maxTrack"),
            lag.get("chainTip"),
        ]) {
            Ok(_) => (),
            Err(Fault::Thrown(e)) => self
                .core
                .source_unhandled_rejections
                .push(obj(&[("name", s(e.name)), ("message", s(e.message))])),
            Err(e) => return Err(e),
        }
        Ok(V::Undefined)
    }
    fn source_main_service_dispatch(&mut self, target: &str, a: &[V]) -> R<V> {
        match target {
            "Main.checkNetworkStatus" => self.core.source_main_service_network(),
            "Main.fetchWithRetry" => self.source_main_service_fetch(a),
            "Main.blockHandlerBegin" => self.source_main_service_begin(arg(a, 0), arg(a, 1)),
            "Main.processBlock" => self.source_main_service_block(arg(a, 0), arg(a, 1)),
            "Main.initialize" => self.source_main_service_initialize(),
            "Main.enterRecoveryMode" => self.source_main_service_recovery(),
            "Main.processIncomingBlocks" => self.source_main_service_live(a),
            "Main.syncIfNecessary" => self.source_main_service_sync(a),
            "Main.handleReorg" => Err(type_error(
                "Cannot read properties of undefined (reading 'handleReorg')",
            )),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
