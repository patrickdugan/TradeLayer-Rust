// Explicit V12 cold module/startup state. All category IO delegates to the
// existing Core row owner; opening a source datastore creates no second ledger.
const SOURCE_MAIN_DB_CATEGORIES: [&str; 32] = [
    "ammRegistry",
    "ammState",
    "txIndex",
    "propertyList",
    "oracleList",
    "oracleData",
    "contractList",
    "tallyMap",
    "tallyMapDelta",
    "marginMapDelta",
    "marginMaps",
    "clearlists",
    "attestations",
    "clearing",
    "consensus",
    "persistence",
    "volumeIndex",
    "channels",
    "withdrawQueue",
    "activations",
    "insurance",
    "orderBooks",
    "feeCache",
    "tradeHistory",
    "fundingEvents",
    "vaults",
    "syntheticTokens",
    "liquidations",
    "scaling",
    "channelDelta",
    "iou",
    "procedural",
];
struct SourceMainStartupActivationBackend {
    calls: Vec<(String, Vec<V>, DependencyReturn)>,
    cursor: usize,
}
impl SourceBackend for SourceMainStartupActivationBackend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let Some((target, args, value)) = self.calls.get(self.cursor) else {
            return Err(SourceRuleError::UnknownDependency);
        };
        if target != &call.target || args != &call.arguments {
            return Err(SourceRuleError::InvalidInput);
        }
        self.cursor += 1;
        Ok(value.clone())
    }
}
impl ContractCore {
    fn source_main_startup_get(&self, key: &str) -> R<V> {
        Ok(self.source_main_service_get("startupModules")?.get(key))
    }
    fn source_main_startup_set(&mut self, key: &str, value: V) -> R<()> {
        let mut state = self.source_main_service_get("startupModules")?;
        set(&mut state, key, value)?;
        self.source_main_service_set("startupModules", state)
    }
    fn source_main_startup_configure(&mut self) -> R<V> {
        if !self.main_production_enabled()
            || self.main_activation_locked
            || !self.source_main_service_enabled()
            || self.source_main_service_get("startupModules")? != V::Undefined
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let p = self
            .main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        p.index = crate::txindex_source_owner::TxIndexSourceOwner::new();
        p.genesis = V::Undefined;
        p.parse_block = V::Undefined;
        self.source_main_service_set("extraOwn", obj(&[]))?;
        self.source_main_service_set(
            "startupModules",
            obj(&[
                ("imported", V::Bool(false)),
                (
                    "database",
                    obj(&[
                        ("initialized", V::Bool(false)),
                        ("initializing", V::Bool(false)),
                        ("path", s("")),
                        ("test", V::Bool(false)),
                        ("categories", V::Array(vec![])),
                    ]),
                ),
                (
                    "class",
                    obj(&[
                        ("sameSingleton", V::Bool(false)),
                        ("isInitializing", V::Bool(false)),
                        ("persistenceInitialized", V::Undefined),
                        ("persistenceSame", V::Bool(false)),
                    ]),
                ),
            ]),
        )?;
        Ok(V::Undefined)
    }
    fn source_main_startup_import(&mut self) -> R<V> {
        if truth(&self.source_main_startup_get("imported")?) {
            return Ok(obj(&[("$mainClass", n(1.))]));
        }
        // The actual original Database automatic init, Main initialization
        // IIFE and Activation initialization IIFE run on the recorded VM/host
        // await profile. Their IO phases interleave before category opening.
        self.source_main_service_client(vec![])?;
        self.emit(obj(&[("kind", s("module")), ("name", s("main"))]));
        self.emit(obj(&[("kind", s("module")), ("name", s("reOrg"))]));
        self.source_main_service_client(vec![])?;
        let activation_client = self.source_main_service_client(vec![])?;
        let chain = self.source_main_production_io("Client.getChain", &[])?;
        let activation_chain = self.source_main_production_io("Client.getChain", &[]);
        let activation_chain_ok = activation_chain.is_ok();
        let activation_chain = returned(activation_chain)?;
        self.source_main_service_timer(n(500.));
        let test = self.source_main_production_io("Client.getTest", &[])?;
        let activation_test = if activation_chain_ok {
            Some(returned(
                self.source_main_production_io("Client.getTest", &[]),
            )?)
        } else {
            None
        };
        let mut owner = std::mem::take(&mut self.main_activation);
        let mut calls = vec![
            (
                "Client.getInstance".into(),
                vec![],
                DependencyReturn::Pending(activation_client),
            ),
            ("Client.getChain".into(), vec![], activation_chain),
        ];
        if let Some(test) = activation_test {
            calls.push(("Client.getTest".into(), vec![], test));
        }
        let mut backend = SourceMainStartupActivationBackend { calls, cursor: 0 };
        let evaluated = owner.execute_unverified(
            crate::activation::ActivationCall {
                method: "init".into(),
                arguments: vec![],
            },
            &mut backend,
        );
        self.main_activation = owner;
        match outcome(evaluated?.outcome) {
            Ok(_) => (),
            Err(Fault::Thrown(e)) => self
                .source_unhandled_rejections
                .push(obj(&[("name", s(e.name)), ("message", s(e.message))])),
            Err(e) => return Err(e),
        }
        if backend.cursor != backend.calls.len() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let V::String(chain) = chain else {
            return Err(type_error("chain.toLowerCase is not a function"));
        };
        let suffix = if test == V::Bool(false) {
            "main"
        } else {
            "test"
        };
        let path = format!("/tl-data/{}-{suffix}", chain.to_lowercase());
        let categories = SOURCE_MAIN_DB_CATEGORIES
            .into_iter()
            .map(s)
            .collect::<Vec<_>>();
        for category in SOURCE_MAIN_DB_CATEGORIES {
            self.emit(obj(&[
                ("kind", s("datastore-open")),
                ("filename", s(format!("{path}/{category}.db"))),
                ("autoload", V::Bool(true)),
                ("category", s(category)),
            ]));
        }
        self.source_main_startup_set(
            "database",
            obj(&[
                ("initialized", V::Bool(true)),
                ("initializing", V::Bool(false)),
                ("path", s(path)),
                ("test", test),
                ("categories", V::Array(categories)),
            ]),
        )?;
        self.source_main_startup_set("imported", V::Bool(true))?;
        Ok(obj(&[("$mainClass", n(1.))]))
    }
    fn source_main_startup_main_instance(&mut self) -> R<V> {
        let mut class = self.source_main_startup_get("class")?;
        if !truth(&class.get("sameSingleton")) && !truth(&class.get("isInitializing")) {
            set(&mut class, "isInitializing", V::Bool(true))?;
            set(&mut class, "sameSingleton", V::Bool(true))?;
            self.source_main_startup_set("class", class.clone())?;
            let p = self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?;
            p.genesis = n(600000.);
            p.parse_block = n(0.);
            // Actual constructor stores its returned getInstance Promise. The
            // original index singleton and this Core remain the sole owners.
            self.source_main_production_index("getInstance", vec![])?;
            let chain = self.source_main_production_io("Client.getChain", &[])?;
            let test = self.source_main_production_io("Client.getTest", &[])?;
            self.source_main_service_persistence_options(&[obj(&[
                ("network", chain),
                ("test", test),
                ("snapshotInterval", n(1000.)),
            ])])?;
            let mut own = self.source_main_service_get("extraOwn")?;
            set(
                &mut own,
                "blockchainPersistence",
                obj(&[("$persistenceInstance", n(1.))]),
            )?;
            self.source_main_service_set("extraOwn", own)?;
            set(&mut class, "persistenceInitialized", V::Bool(true))?;
            set(&mut class, "isInitializing", V::Bool(false))?;
            set(&mut class, "persistenceSame", V::Bool(true))?;
            self.source_main_startup_set("class", class)?;
        }
        Ok(obj(&[("$mainInstance", n(1.))]))
    }
}
impl SourceContractRuntime {
    fn source_main_startup_dispatch(&mut self, target: &str, _a: &[V]) -> R<V> {
        match target {
            "SourceMain.importStartupModules" => self.core.source_main_startup_import(),
            "Main.getInstance" => {
                if self.core.source_main_service_get("startupModules")? == V::Undefined {
                    self.core.source_main_snapshot_main_instance()
                } else {
                    self.core.source_main_startup_main_instance()
                }
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
