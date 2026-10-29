impl SourceContractRuntime {
    fn ordered_bootstrap_promises_v2_invoke(
        &mut self,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.core.native_main_binding.is_some()
            || call.arguments.len() != 1
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let input = arg(&call.arguments, 0);
        ob_keys(&input, &["profile", "action", "txid", "handler"])?;
        let V::Object(fields) = &input else {
            return Err(SourceRuleError::InvalidInput);
        };
        if input.get("profile") != s(ORDERED_BOOTSTRAP_PROMISES_V2_PROFILE)
            || fields
                .iter()
                .any(|(key, _)| !matches!(key.as_str(), "profile" | "action" | "txid" | "handler"))
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let action = ob_string(input.get("action"))?;
        if input.get("txid") != V::Undefined && action != "CallTxUtilsRaw" {
            return Err(SourceRuleError::InvalidInput);
        }
        if input.get("handler") != V::Undefined && action != "AttachIndexHandler" {
            return Err(SourceRuleError::InvalidInput);
        }
        let allowed = [
            "ImportTxUtils",
            "ConstructMain",
            "ResolveImport",
            "RejectImport",
            "ResolveIndex",
            "RejectIndex",
            "DrainMicrotasks",
            "HostAwaitCheckpoint",
            "EndTurn",
            "CallMainHeight",
            "AttachIndexHandler",
            "CallTxUtilsRaw",
        ];
        if !allowed.contains(&action.as_str()) {
            return Err(SourceRuleError::UnknownDependency);
        }
        let mut state = self
            .core
            .ordered_bootstrap_state()
            .map_err(ordered_bootstrap_local)?;
        if state.steps >= ORDERED_BOOTSTRAP_MAX_STEPS
            || self.core.effects_exhausted
            || self.core.effects.len() > 65_400
        {
            return Err(SourceRuleError::Capacity);
        }
        // All supplied fields describe explicit host/client controls. There is
        // no parameter for a source result, poststate, actor or validation.
        match action.as_str() {
            "ImportTxUtils" => {
                if !state.imported {
                    self.core
                        .main_production
                        .as_mut()
                        .ok_or(SourceRuleError::InvalidInput)?
                        .index
                        .ordered_bootstrap_import_begin()?;
                    state.imported = true;
                    self.core
                        .ordered_bootstrap_promises_v2_request(&mut state, "import")
                        .map_err(ordered_bootstrap_local)?;
                }
            }
            "ConstructMain" => {
                if !state.imported {
                    return Err(SourceRuleError::InvalidInput);
                }
                if !state.constructed {
                    self.core
                        .main_production
                        .as_mut()
                        .ok_or(SourceRuleError::InvalidInput)?
                        .index
                        .ordered_bootstrap_constructor()?;
                    let p = self.core.main_production.as_mut().unwrap();
                    p.genesis = n(600_000.);
                    p.parse_block = n(0.);
                    state.constructed = true;
                    self.core
                        .ordered_bootstrap_promises_v2_request(&mut state, "index")
                        .map_err(ordered_bootstrap_local)?;
                }
            }
            "ResolveImport" | "RejectImport" | "ResolveIndex" | "RejectIndex" => {
                let role = if action.ends_with("Import") {
                    "import"
                } else {
                    "index"
                };
                let status = if role == "import" {
                    &state.import_status
                } else {
                    &state.index_status
                };
                if status != "AwaitClient" {
                    return Err(SourceRuleError::InvalidInput);
                }
                let outcome = if action.starts_with("Resolve") {
                    "resolve"
                } else {
                    "reject"
                };
                self.core
                    .ordered_bootstrap_promises_v2_settle(&mut state, role, outcome == "reject")
                    .map_err(ordered_bootstrap_local)?;
            }
            "DrainMicrotasks" => self
                .core
                .ordered_bootstrap_promises_v2_run(&mut state, None)
                .map_err(ordered_bootstrap_local)?,
            "EndTurn" => self
                .core
                .ordered_bootstrap_promises_v2_end_turn(&mut state)
                .map_err(ordered_bootstrap_local)?,
            "HostAwaitCheckpoint" => self
                .core
                .ordered_bootstrap_promises_v2_checkpoint(&mut state)
                .map_err(ordered_bootstrap_local)?,
            "AttachIndexHandler" => self
                .core
                .ordered_bootstrap_promises_v2_attach(&mut state, ob_string(input.get("handler"))?)
                .map_err(ordered_bootstrap_local)?,
            "CallMainHeight" => {
                if !state.constructed {
                    return Err(SourceRuleError::InvalidInput);
                }
                // The real existing Main method/RPC path executes. Metadata
                // only retains the original callback error identity carrier.
                state.caller_active = true;
                state.last_rpc_error = None;
                self.core
                    .ordered_bootstrap_store(&state)
                    .map_err(ordered_bootstrap_local)?;
                let result = self.source_main_invoke(&DependencyCall {
                    target: "Main.getCurrentBlockHeight".into(),
                    arguments: vec![],
                });
                state = self
                    .core
                    .ordered_bootstrap_state()
                    .map_err(ordered_bootstrap_local)?;
                state.caller_active = false;
                let result = match result {
                    Ok(DependencyReturn::Immediate(value) | DependencyReturn::Pending(value)) => {
                        obj(&[("status", s("Fulfilled")), ("value", value)])
                    }
                    Ok(DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error)) => {
                        let reference = if let Some(reference) = state.last_rpc_error {
                            reference
                        } else {
                            state.error(obj(&[
                                ("name", s(error.name)),
                                ("message", s(error.message)),
                            ]))?
                        };
                        obj(&[
                            ("status", s("Rejected")),
                            ("errorRef", ob_ordinal(reference)),
                        ])
                    }
                    Err(error) => {
                        self.core
                            .ordered_bootstrap_store(&state)
                            .map_err(ordered_bootstrap_local)?;
                        return Err(error);
                    }
                };
                self.core
                    .ordered_bootstrap_promises_v2_run(&mut state, None)
                    .map_err(ordered_bootstrap_local)?;
                state.callers.push(result.clone());
                self.core
                    .ordered_bootstrap_emit(
                        &mut state,
                        obj(&[
                            ("kind", s("caller-result")),
                            ("method", s("Main.getCurrentBlockHeight")),
                            ("result", result),
                        ]),
                    )
                    .map_err(ordered_bootstrap_local)?;
            }
            "CallTxUtilsRaw" => {
                let V::String(txid) = input.get("txid") else {
                    return Err(SourceRuleError::InvalidInput);
                };
                if !state.imported {
                    return Err(SourceRuleError::InvalidInput);
                }
                let owner = self
                    .core
                    .main_production
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .index
                    .state_value()?;
                if truth(&owner.get("txUtilsClient")) {
                    // This profile has no raw-transaction IO lease before
                    // adoption. It never supplies a decoder/business answer.
                    return Err(SourceRuleError::UnknownDependency);
                }
                // Literal guard is outside getRawTransaction's try/catch.
                // Awaiting the unqualified, missing init throws a NEW native
                // ReferenceError, not the earlier client rejection object.
                let reference = state.error(obj(&[
                    ("name", s("ReferenceError")),
                    ("message", s("init is not defined")),
                ]))?;
                let result = obj(&[
                    ("status", s("Rejected")),
                    ("errorRef", ob_ordinal(reference)),
                ]);
                // Awaiting this actual async rejection still lets already
                // queued import/static reactions run before caller observation.
                self.core
                    .ordered_bootstrap_promises_v2_run(&mut state, None)
                    .map_err(ordered_bootstrap_local)?;
                state.callers.push(result.clone());
                self.core
                    .ordered_bootstrap_emit(
                        &mut state,
                        obj(&[
                            ("kind", s("caller-result")),
                            ("method", s("TxUtils.getRawTransaction")),
                            ("args", V::Array(vec![s(txid)])),
                            ("result", result),
                        ]),
                    )
                    .map_err(ordered_bootstrap_local)?;
            }
            _ => unreachable!("validated action"),
        }
        state.steps += 1;
        self.core
            .ordered_bootstrap_store(&state)
            .map_err(ordered_bootstrap_local)?;
        Ok(DependencyReturn::Immediate(state.value()?))
    }
}
