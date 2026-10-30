impl RetainedCallerSelectedOwnerV1 {
    fn install_replacement(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
        replacement: RetainedCallerSelectedReplacementV1,
    ) -> Result<
        (),
        (
            RetainedCallerSelectedWalErrorV1,
            RetainedCallerSelectedReplacementV1,
        ),
    > {
        use crate::source_contract_runtime::NativeMainRetainedConsensusOperationKindV1 as Kind;
        match replacement {
            RetainedCallerSelectedReplacementV1::Inline { write, mapping } => {
                let result = match (self, operation.kind()) {
                    (Self::Caller(c), Kind::CallerInline) => c
                        .owner
                        .financial
                        .native_main_runtime_mut()
                        .native_main_selected_caller_install_inline(
                            &c.owner.index,
                            c.frame.as_deref_mut().expect("retained frame"),
                            operation,
                            write,
                            &mapping,
                        ),
                    (Self::Consensus(c), Kind::CallerInline) => {
                        let result = c
                            .caller
                            .owner
                            .financial
                            .native_main_runtime_mut()
                            .native_main_selected_caller_install_inline(
                                &c.caller.owner.index,
                                c.caller.frame.as_deref_mut().expect("retained frame"),
                                operation,
                                write,
                                &mapping,
                            );
                        if result.is_ok() {
                            c.attempted = None;
                        }
                        result
                    }
                    (Self::Consensus(c), Kind::ServiceInline) => {
                        let result = c
                            .caller
                            .owner
                            .financial
                            .native_main_runtime_mut()
                            .native_main_service_install_inline_supersession(
                                &c.caller.owner.index,
                                &mut c.service,
                                write,
                                &mapping,
                            );
                        if result.is_ok() {
                            c.attempted = None;
                        }
                        result
                    }
                    _ => Err((SourceRuleError::InvalidInput, write)),
                };
                result.map_err(|(error, write)| {
                    (
                        caller_selected_backend(error),
                        RetainedCallerSelectedReplacementV1::Inline { write, mapping },
                    )
                })
            }
            RetainedCallerSelectedReplacementV1::Validity { write, mapping } => {
                let result = match (self, operation.kind()) {
                    (Self::Consensus(c), Kind::ServiceValidity) => {
                        let result = c
                            .caller
                            .owner
                            .financial
                            .native_main_runtime_mut()
                            .native_main_service_install_supersession(
                                &mut c.service,
                                write,
                                &mapping,
                            );
                        if result.is_ok() {
                            c.attempted = None;
                        }
                        result
                    }
                    _ => Err((SourceRuleError::InvalidInput, write)),
                };
                result.map_err(|(error, write)| {
                    (
                        caller_selected_backend(error),
                        RetainedCallerSelectedReplacementV1::Validity { write, mapping },
                    )
                })
            }
        }
    }
    fn append_selected_native_only(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> CallerSelectedResult<()> {
        match self {
            Self::Caller(c) => {
                let actual = c
                    .pending_descriptor()
                    .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?;
                if operation.value().get("caller") != actual.value() {
                    return Err(RetainedCallerSelectedWalErrorV1::WrongSelector);
                }
                c.append_pending(&actual, permit);
            }
            Self::Consensus(c) => {
                c.append_pending(operation, permit)
                    .map_err(caller_selected_backend)?;
            }
        }
        Ok(())
    }
}
impl SourceMainNativeDurableSelectedCallerV1 {
    fn install_selected_replacement(&mut self) -> CallerSelectedResult<()> {
        let operation = self
            .pending
            .as_ref()
            .and_then(|p| p.operation.as_ref())
            .cloned()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?;
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.replacement.is_some())
        {
            let replacement = self
                .pending
                .as_mut()
                .expect("pending")
                .replacement
                .take()
                .expect("checked actual replacement");
            match self
                .owner
                .as_mut()
                .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?
                .install_replacement(&operation, replacement)
            {
                Ok(()) => (),
                Err((error, replacement)) => {
                    self.pending.as_mut().expect("pending").replacement = Some(replacement);
                    return Err(error);
                }
            }
            // Source identity is unchanged; actual replacement read context may
            // differ. Derive its new selector only from the installed real plan.
            let owner = self.owner.as_ref().expect("same owner");
            let route = self.route.as_ref().expect("same route");
            let old = operation.value();
            let selected = owner
                .operations(route)
                .into_iter()
                .filter(|p| {
                    let v = p.value();
                    [
                        "kind",
                        "callerScope",
                        "sourceJob",
                        "methodScope",
                        "operationOrdinal",
                        "role",
                        "route",
                    ]
                    .iter()
                    .all(|name| v.get(name) == old.get(name))
                })
                .collect::<Vec<_>>();
            if selected.len() != 1 {
                return Err(RetainedCallerSelectedWalErrorV1::WrongSelector);
            }
            self.pending.as_mut().expect("pending").operation = selected.into_iter().next();
        }
        Ok(())
    }
    fn append_replacement_native_only(&mut self) -> CallerSelectedResult<()> {
        self.install_selected_replacement()?;
        let operation = self
            .pending
            .as_ref()
            .and_then(|p| p.operation.as_ref())
            .cloned()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?;
        self.phase = RetainedCallerSelectedWalPhaseV1::NativeRecoveryRequired; // before actual native IO
        self.owner
            .as_mut()
            .expect("same owner")
            .append_selected_native_only(
                &operation,
                self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
            )?;
        let owner = self.owner.as_ref().expect("same owner");
        owner
            .native(&operation)?
            .verify_post(&owner.session().index)
            .map_err(RetainedCallerSelectedWalErrorV1::Backend)?;
        self.phase = RetainedCallerSelectedWalPhaseV1::Observe;
        Ok(())
    }
}
