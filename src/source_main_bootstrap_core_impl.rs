impl ContractCore {
    fn ordered_bootstrap_present(&self) -> bool {
        self.main_production
            .as_ref()
            .and_then(|p| p.service.as_ref())
            .is_some_and(|service| service.get(ORDERED_BOOTSTRAP_KEY) != V::Undefined)
    }
    fn ordered_bootstrap_state(&self) -> R<OrderedBootstrap> {
        Ok(OrderedBootstrap::owned(
            self.source_main_service_get(ORDERED_BOOTSTRAP_KEY)?,
        )?)
    }
    fn ordered_bootstrap_store(&mut self, state: &OrderedBootstrap) -> R<()> {
        self.source_main_service_set(ORDERED_BOOTSTRAP_KEY, state.value()?)
    }
    fn ordered_bootstrap_configure(&mut self, a: &[V]) -> R<V> {
        if self.policy != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.main_activation_locked
            || a.len() != 1
            || self.main_production.is_some()
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let config = arg(a, 0);
        ob_keys(
            &config,
            &["profile", "production", "service", "clientCalls"],
        )?;
        let V::Object(fields) = &config else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields.iter().any(|(k, _)| {
            !matches!(
                k.as_str(),
                "profile" | "production" | "service" | "clientCalls"
            )
        }) || config.get("profile") != s(ORDERED_BOOTSTRAP_PROFILE)
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let production = config.get("production");
        ob_keys(&production, &["client", "io", "genesisBlock"])?;
        let client = production.get("client");
        ob_keys(&client, &["chain", "test"])?;
        ob_keys(
            &production.get("io"),
            &["height", "blocks", "networkInfo", "failures"],
        )?;
        let io = production.get("io");
        ob_data(&io)?;
        if io.get("failures") != V::Undefined {
            for failure in ob_array(io.get("failures"))? {
                ob_keys(&failure, &["method", "ordinal", "name", "message", "code"])?;
                for key in ["name", "message"] {
                    if !matches!(failure.get(key), V::Undefined | V::String(_)) {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                }
            }
        }
        let (V::String(chain), V::Bool(test)) = (client.get("chain"), client.get("test")) else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if !["BTC", "LTC", "DOGE"].contains(&chain.as_str()) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        crate::storage::Namespace::new(&chain, test, "txIndex")
            .map_err(|_| SourceRuleError::InvalidInput)?;
        let state = OrderedBootstrap::new(ob_array(config.get("clientCalls"))?)?;
        // Only this new entry suppresses static init; old configure remains
        // exactly its previous eager getInstance behavior.
        self.source_main_production_configure_mode(&[production], false)?;
        self.source_main_service_configure(&[match config.get("service") {
            V::Undefined => obj(&[]),
            v => v,
        }])?;
        self.ordered_bootstrap_store(&state)?;
        Ok(state.value()?)
    }
    fn ordered_bootstrap_emit(&mut self, state: &mut OrderedBootstrap, event: V) -> R<()> {
        state.event(event.clone())?;
        if [s("unhandled-rejection"), s("rejection-handled")].contains(&event.get("kind")) {
            self.emit(event);
            return Ok(());
        }
        self.emit(obj(&[
            ("kind", s("ordered-bootstrap")),
            ("profile", s(ORDERED_BOOTSTRAP_PROFILE)),
            ("event", event),
        ]));
        Ok(())
    }
    fn ordered_bootstrap_request(&mut self, state: &mut OrderedBootstrap, role: &str) -> R<()> {
        let at = if role == "import" { 0 } else { 1 };
        let args = if at == 0 { vec![] } else { vec![V::Bool(true)] };
        let ordinal = self.source_main_production_next_ordinal("Client.getInstance")?;
        if ordinal != (at + 1) as u64 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let fact = state.facts[at].clone();
        let mode = ob_string(fact.get("mode"))?;
        self.ordered_bootstrap_emit(
            state,
            obj(&[
                ("kind", s("client-request")),
                ("role", s(role)),
                ("ordinal", ob_ordinal(ordinal as usize)),
                ("args", V::Array(args)),
                ("mode", s(&mode)),
            ]),
        )?;
        if mode == "deferred" {
            if at == 0 {
                state.import_status = "AwaitClient".into();
            } else {
                state.index_status = "AwaitClient".into();
            }
        } else {
            self.ordered_bootstrap_queue(state, role, &mode)?;
        }
        Ok(())
    }
    fn ordered_bootstrap_queue(
        &mut self,
        state: &mut OrderedBootstrap,
        role: &str,
        outcome: &str,
    ) -> R<()> {
        if state.jobs.len() >= 8 {
            return Err(SourceRuleError::Capacity.into());
        }
        let error = if outcome == "reject" {
            Some(state.error(state.facts[if role == "import" { 0 } else { 1 }].get("rejection"))?)
        } else {
            None
        };
        state.jobs.push(obj(&[
            ("kind", s("settle")),
            ("role", s(role)),
            ("outcome", s(outcome)),
            ("error", ob_optional_value(error)),
        ]));
        if role == "import" {
            state.import_status = "Queued".into();
        } else {
            state.index_status = "Queued".into();
        }
        Ok(())
    }
    fn ordered_bootstrap_drain(&mut self, state: &mut OrderedBootstrap) -> R<()> {
        while !state.jobs.is_empty() {
            let job = state.jobs.remove(0);
            let kind = ob_string(job.get("kind"))?;
            if kind == "handle" {
                let event = obj(&[
                    ("kind", s("caller-catch")),
                    ("promise", OrderedBootstrap::promise("index")),
                    ("errorRef", ob_optional_value(state.index_error)),
                ]);
                self.ordered_bootstrap_emit(state, event)?;
                if state.notified.iter().any(|role| role == "index") {
                    state.handled_pending = true;
                }
                continue;
            }
            let role = ob_string(job.get("role"))?;
            let outcome = ob_string(job.get("outcome"))?;
            let error = ob_optional(job.get("error"))?;
            let pointer = obj(&[("$rpcClient", n(1.))]);
            let bound_client_fields = self
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .client
                .clone();
            let mut native_rejection = None;
            {
                let owner = &mut self
                    .main_production
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .index;
                match (role.as_str(), outcome.as_str()) {
                    ("import", "resolve") => owner.ordered_bootstrap_import_fulfill(pointer)?,
                    ("import", "reject") => {
                        let value =
                            state.error_value(error.ok_or(SourceRuleError::InvalidInput)?)?;
                        owner.ordered_bootstrap_import_reject(
                            crate::validity_source::SourceException {
                                name: ob_string(value.get("name"))?,
                                message: ob_string(value.get("message"))?,
                            },
                        )?;
                    }
                    ("index", "resolve") => {
                        if let SourceOutcome::Thrown(error) =
                            owner.ordered_bootstrap_static_fulfill(pointer, &bound_client_fields)?
                        {
                            native_rejection = Some(error);
                        }
                    }
                    ("index", "reject") => (),
                    _ => return Err(SourceRuleError::InvalidInput.into()),
                }
            }
            let error = if let Some(error) = native_rejection {
                Some(state.error(obj(&[
                    ("name", s(error.name)),
                    ("message", s(error.message)),
                ]))?)
            } else {
                error
            };
            let rejected = outcome == "reject" || error.is_some();
            let status = if rejected { "Rejected" } else { "Fulfilled" };
            if role == "import" {
                state.import_status = status.into();
                state.import_error = error;
            } else {
                state.index_status = status.into();
                state.index_error = error;
            }
            if rejected {
                state.rejected.push(role.clone());
                if role == "index" && state.index_handled {
                    state.jobs.push(obj(&[("kind", s("handle"))]));
                }
            }
            let event = obj(&[
                ("kind", s("promise-settled")),
                ("promise", OrderedBootstrap::promise(&role)),
                ("role", s(&role)),
                ("status", s(status)),
                ("errorRef", ob_optional_value(error)),
            ]);
            self.ordered_bootstrap_emit(state, event)?;
        }
        Ok(())
    }
    fn ordered_bootstrap_end_turn(&mut self, state: &mut OrderedBootstrap) -> R<()> {
        self.ordered_bootstrap_drain(state)?;
        for role in state.rejected.clone() {
            if state.notified.contains(&role) || (role == "index" && state.index_handled) {
                continue;
            }
            let error = if role == "import" {
                state.import_error
            } else {
                state.index_error
            }
            .ok_or(SourceRuleError::InvalidInput)?;
            if self.source_unhandled_rejections.len() >= MAX_TABLE_ROWS {
                return Err(SourceRuleError::Capacity.into());
            }
            let thrown = state.error_value(error)?.clone();
            let event = obj(&[
                ("kind", s("unhandled-rejection")),
                ("promise", OrderedBootstrap::promise(&role)),
                ("errorRef", ob_ordinal(error)),
                ("thrown", thrown.clone()),
                ("isMainIndexPromise", V::Bool(role == "index")),
            ]);
            self.ordered_bootstrap_emit(state, event)?;
            // Preserve the established source-global notification history;
            // the complete reason reference remains in bootstrap metadata.
            self.source_unhandled_rejections.push(obj(&[
                ("name", thrown.get("name")),
                ("message", thrown.get("message")),
            ]));
            state.notified.push(role);
        }
        if state.handled_pending {
            self.ordered_bootstrap_emit(
                state,
                obj(&[
                    ("kind", s("rejection-handled")),
                    ("promise", OrderedBootstrap::promise("index")),
                    ("isMainIndexPromise", V::Bool(true)),
                ]),
            )?;
            state.handled_pending = false;
        }
        Ok(())
    }
    fn ordered_bootstrap_capture_rpc(&mut self, error: &V) -> R<()> {
        if !self.ordered_bootstrap_present() {
            return Ok(());
        }
        let mut state = self.ordered_bootstrap_state()?;
        if state.caller_active {
            state.last_rpc_error = Some(state.error(error.clone())?);
            self.ordered_bootstrap_store(&state)?;
        }
        Ok(())
    }
}
