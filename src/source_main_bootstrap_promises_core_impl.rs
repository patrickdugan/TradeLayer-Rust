impl ContractCore {
    fn ordered_bootstrap_promises_v2_configure(&mut self, a: &[V]) -> R<V> {
        if a.len() != 1 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let config = arg(a, 0);
        ob_keys(
            &config,
            &[
                "profile",
                "production",
                "service",
                "clientCalls",
                "rejectionObjects",
            ],
        )?;
        let (facts, promises) = OrderedBootstrapPromisesV2::new(
            ob_array(config.get("clientCalls"))?,
            config.get("rejectionObjects"),
        )?;
        let mut normalized = vec![
            ("profile", s(ORDERED_BOOTSTRAP_PROFILE)),
            ("production", config.get("production")),
            ("clientCalls", V::Array(facts)),
        ];
        if config.get("service") != V::Undefined {
            normalized.push(("service", config.get("service")));
        }
        // Same existing validator and owner setup, with no eager static init.
        self.ordered_bootstrap_configure(&[obj(&normalized)])?;
        let mut state = self.ordered_bootstrap_state()?;
        state.promise_v2 = Some(promises);
        self.ordered_bootstrap_store(&state)?;
        Ok(state.value()?)
    }
    fn ordered_bootstrap_promises_v2_request(
        &mut self,
        state: &mut OrderedBootstrap,
        role: &str,
    ) -> R<()> {
        let at = ob_v2_role(role)?;
        if state.promises()?.clients[at] != 0 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let ordinal = self.source_main_production_next_ordinal("Client.getInstance")?;
        if ordinal != (at + 1) as u64 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let mode = ob_string(state.facts[at].get("mode"))?;
        self.ordered_bootstrap_emit(
            state,
            obj(&[
                ("kind", s("client-request")),
                ("role", s(role)),
                ("ordinal", ob_ordinal(ordinal as usize)),
                (
                    "args",
                    V::Array(if at == 0 { vec![] } else { vec![V::Bool(true)] }),
                ),
                ("mode", s(&mode)),
            ]),
        )?;
        let error = if mode == "reject" {
            Some(state.v2_rejection(at)?)
        } else {
            None
        };
        {
            let p = state.promises_mut()?;
            p.clients[at] = match mode.as_str() {
                "resolve" => 2,
                "reject" => 3,
                "deferred" => 1,
                _ => return Err(SourceRuleError::InvalidInput.into()),
            };
            p.reasons[at] = error;
        }
        state.v2_status(
            at,
            if mode == "deferred" {
                "AwaitClient"
            } else {
                "Queued"
            },
        );
        // Awaiting a host-realm native Promise from the original VM requires
        // the thenable-assimilation job before its client reaction is installed.
        state.v2_job(obj(&[("kind", s("assimilate")), ("role", s(role))]))?;
        Ok(())
    }
    fn ordered_bootstrap_promises_v2_settle(
        &mut self,
        state: &mut OrderedBootstrap,
        role: &str,
        reject: bool,
    ) -> R<()> {
        let at = ob_v2_role(role)?;
        if state.promises()?.clients[at] != 1 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let error = if reject {
            Some(state.v2_rejection(at)?)
        } else {
            None
        };
        let registered = {
            let p = state.promises_mut()?;
            p.clients[at] = if reject { 3 } else { 2 };
            p.reasons[at] = error;
            p.registered[at]
        };
        state.v2_status(at, "Queued");
        if registered {
            state.v2_job(obj(&[("kind", s("bridge")), ("role", s(role))]))?;
        }
        Ok(())
    }
    fn ordered_bootstrap_promises_v2_attach(
        &mut self,
        state: &mut OrderedBootstrap,
        label: String,
    ) -> R<()> {
        if !state.constructed
            || label.is_empty()
            || label.len() > 128
            || state.promises()?.handlers.len() >= 16
            || state.promises()?.handlers.iter().any(|h| h.label == label)
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let settled = matches!(state.index_status.as_str(), "Fulfilled" | "Rejected");
        let was_handled = state.index_handled;
        state.index_handled = true;
        let id = state.promises()?.handlers.len();
        state
            .promises_mut()?
            .handlers
            .push(OrderedBootstrapHandlerV2 {
                label,
                scheduled: settled,
                done: false,
                ran: false,
            });
        if settled {
            state.v2_job(obj(&[
                ("kind", s("catch")),
                ("handler", ob_ordinal(id)),
                ("status", s(&state.index_status)),
            ]))?;
        }
        if !was_handled && state.notified.iter().any(|r| r == "index") {
            state.handled_pending = true;
        }
        Ok(())
    }
    fn ordered_bootstrap_promises_v2_run(
        &mut self,
        state: &mut OrderedBootstrap,
        stop: Option<usize>,
    ) -> R<()> {
        let mut jobs = 0;
        while !state.jobs.is_empty() {
            jobs += 1;
            if jobs > 256 {
                return Err(SourceRuleError::Capacity.into());
            }
            let job = state.jobs.remove(0);
            let kind = ob_string(job.get("kind"))?;
            if kind == "checkpoint" {
                if stop != Some(ob_usize(job.get("epoch"))?) {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                return Ok(());
            }
            if kind == "catch" {
                let id = ob_usize(job.get("handler"))?;
                let rejected = job.get("status") == s("Rejected");
                let label = {
                    let h = state
                        .promises_mut()?
                        .handlers
                        .get_mut(id)
                        .ok_or(SourceRuleError::InvalidInput)?;
                    if !h.scheduled || h.done {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    h.done = true;
                    h.ran = rejected;
                    h.label.clone()
                };
                if rejected {
                    let reference = state.index_error.ok_or(SourceRuleError::InvalidInput)?;
                    self.ordered_bootstrap_emit(
                        state,
                        obj(&[
                            ("kind", s("caller-catch")),
                            ("promise", OrderedBootstrap::promise("index")),
                            ("handler", s(label)),
                            ("errorRef", ob_ordinal(reference)),
                        ]),
                    )?;
                }
                continue;
            }
            let role = ob_string(job.get("role"))?;
            let at = ob_v2_role(&role)?;
            match kind.as_str() {
                "assimilate" => {
                    let ready = {
                        let p = state.promises_mut()?;
                        if p.registered[at] || p.clients[at] == 0 {
                            return Err(SourceRuleError::InvalidInput.into());
                        }
                        p.registered[at] = true;
                        p.clients[at] >= 2
                    };
                    if ready {
                        state.v2_job(obj(&[("kind", s("bridge")), ("role", s(&role))]))?;
                    }
                }
                "bridge" => {
                    if !state.promises()?.registered[at] || state.promises()?.clients[at] < 2 {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    state.v2_job(obj(&[("kind", s("init")), ("role", s(&role))]))?;
                }
                "init" => {
                    let p = state.promises()?;
                    let rejected = p.clients[at] == 3;
                    let mut reference = p.reasons[at];
                    let pointer = obj(&[("$rpcClient", n(1.))]);
                    let fields = self
                        .main_production
                        .as_ref()
                        .ok_or(SourceRuleError::InvalidInput)?
                        .client
                        .clone();
                    let source_error = if rejected {
                        let error =
                            state.error_value(reference.ok_or(SourceRuleError::InvalidInput)?)?;
                        Some(crate::validity_source::SourceException {
                            name: ob_string(error.get("name"))?,
                            message: ob_string(error.get("message"))?,
                        })
                    } else {
                        None
                    };
                    let mut native_error = None;
                    {
                        let owner = &mut self
                            .main_production
                            .as_mut()
                            .ok_or(SourceRuleError::InvalidInput)?
                            .index;
                        if at == 0 {
                            if let Some(error) = source_error {
                                owner.ordered_bootstrap_import_reject(error)?;
                            } else {
                                owner.ordered_bootstrap_import_fulfill(pointer)?;
                            }
                        } else if !rejected {
                            if let SourceOutcome::Thrown(error) =
                                owner.ordered_bootstrap_static_fulfill(pointer, &fields)?
                            {
                                native_error = Some(error);
                            }
                        }
                    }
                    if let Some(error) = native_error {
                        reference = Some(state.error(obj(&[
                            ("name", s(error.name)),
                            ("message", s(error.message)),
                        ]))?);
                    }
                    state.v2_status(at, "AwaitOuter");
                    state.v2_job(obj(&[
                        ("kind", s("outer")),
                        ("role", s(&role)),
                        (
                            "status",
                            s(if reference.is_some() {
                                "Rejected"
                            } else {
                                "Fulfilled"
                            }),
                        ),
                        ("error", ob_optional_value(reference)),
                    ]))?;
                }
                "outer" => {
                    let status = ob_string(job.get("status"))?;
                    let reference = ob_optional(job.get("error"))?;
                    if (if at == 0 {
                        &state.import_status
                    } else {
                        &state.index_status
                    }) != "AwaitOuter"
                        || !["Fulfilled", "Rejected"].contains(&status.as_str())
                        || (status == "Rejected") != reference.is_some()
                    {
                        return Err(SourceRuleError::InvalidInput.into());
                    }
                    state.v2_status(at, &status);
                    if at == 0 {
                        state.import_error = reference;
                    } else {
                        state.index_error = reference;
                    }
                    if status == "Rejected" {
                        state.rejected.push(role.clone());
                    }
                    if at == 1 {
                        let pending = state
                            .promises()?
                            .handlers
                            .iter()
                            .enumerate()
                            .filter(|(_, h)| !h.scheduled)
                            .map(|(id, _)| id)
                            .collect::<Vec<_>>();
                        for id in pending {
                            state.promises_mut()?.handlers[id].scheduled = true;
                            state.v2_job(obj(&[
                                ("kind", s("catch")),
                                ("handler", ob_ordinal(id)),
                                ("status", s(&status)),
                            ]))?;
                        }
                    }
                    self.ordered_bootstrap_emit(
                        state,
                        obj(&[
                            ("kind", s("promise-settled")),
                            ("promise", OrderedBootstrap::promise(&role)),
                            ("role", s(role)),
                            ("status", s(status)),
                            ("errorRef", ob_optional_value(reference)),
                        ]),
                    )?;
                }
                _ => return Err(SourceRuleError::InvalidInput.into()),
            }
        }
        if stop.is_some() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        Ok(())
    }
    fn ordered_bootstrap_promises_v2_checkpoint(&mut self, state: &mut OrderedBootstrap) -> R<()> {
        let epoch = {
            let p = state.promises_mut()?;
            p.epoch = p.epoch.checked_add(1).ok_or(SourceRuleError::Capacity)?;
            p.epoch
        };
        state.v2_job(obj(&[
            ("kind", s("checkpoint")),
            ("epoch", ob_ordinal(epoch)),
        ]))?;
        self.ordered_bootstrap_promises_v2_run(state, Some(epoch))
    }
    fn ordered_bootstrap_promises_v2_end_turn(&mut self, state: &mut OrderedBootstrap) -> R<()> {
        self.ordered_bootstrap_promises_v2_run(state, None)?;
        // The legacy notification routine now receives an empty queue. It
        // retains the exact source-global history and flat notification shape.
        self.ordered_bootstrap_end_turn(state)
    }
}
