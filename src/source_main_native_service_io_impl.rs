// Data-only additional RPC capability for the explicit active-service profile.
// The original TxIndex owner executes sender/output/decoder logic itself.
struct NativeMainServiceIndexBackend<'a, B> {
    base: B,
    documents: &'a V,
}
impl<B: SourceBackend + NativeMainServiceBackendCoreV1> SourceBackend
    for NativeMainServiceIndexBackend<'_, B>
{
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let Some(method) = call.target.strip_prefix("client.") else {
            return self.base.invoke(call);
        };
        if !["getRawTransaction", "decoderawtransaction"].contains(&method) {
            return self.base.invoke(call);
        }
        if call.arguments.first() != Some(&obj(&[("$rpcClient", n(1.))])) {
            return Err(SourceRuleError::InvalidInput);
        }
        let args = &call.arguments[1..];
        let rows = array(self.documents).map_err(|_| SourceRuleError::InvalidInput)?;
        let returned = if method == "getRawTransaction" {
            rows.iter()
                .find(|row| row.get("id") == arg(args, 0))
                .map_or(V::Undefined, |row| {
                    if arg(args, 1) == V::Bool(false) && row.get("raw") != V::Undefined {
                        row.get("raw")
                    } else {
                        row.get("document")
                    }
                })
        } else {
            rows.iter()
                .find(|row| row.get("raw") == arg(args, 0))
                .map_or(V::Undefined, |row| row.get("document"))
        };
        // The genuine oracle's declared getRawTransaction(id, verbose, ...rest)
        // records its missing verbose argument as Undefined. This is its data
        // adapter descriptor; the actual owner's call arguments stay intact.
        let mut recorded = args.to_vec();
        if method == "getRawTransaction" && recorded.len() == 1 {
            recorded.push(V::Undefined);
        }
        self.base.service_core().emit(obj(&[
            ("kind", s("rpc-data")),
            ("method", s(format!("Client.{method}"))),
            ("args", V::Array(recorded)),
            ("returned", returned.clone()),
        ]));
        Ok(DependencyReturn::Pending(returned))
    }
}
impl ContractCore {
    fn native_main_service_index(
        &mut self,
        index: &mut SourceCompactIndex,
        method: &str,
        args: Vec<V>,
        documents: &V,
    ) -> R<V> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.evaluate(
            None,
            &SourceCall {
                method: method.into(),
                arguments: args,
            },
            &mut NativeMainServiceIndexBackend {
                base: NativeMainIndexBackend { core: self, index },
                documents,
            },
        );
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let evaluation = result?;
        for rejection in evaluation.deferred_rejections {
            self.main_height_rejections.push(rejection);
        }
        outcome(evaluation.outcome)
    }
}

// Actual TxUtils import-time initializer, on the SAME authoritative index.
impl ContractCore {
    fn native_main_service_load_module(
        &mut self,
        index: &mut SourceCompactIndex,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.load_module(&mut NativeMainServiceIndexBackend {
            base: NativeMainIndexBackend { core: self, index },
            documents,
        });
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let evaluation = result?;
        for rejection in evaluation.deferred_rejections {
            self.main_height_rejections.push(rejection);
        }
        Ok(())
    }
}

// Actual primitive loan; observes suffixes without taking/draining the parked
// financial prefix or resetting the existing effects_exhausted capacity flag.
struct NativeMainServiceStepLoan<'a> {
    runtime: &'a mut SourceContractRuntime,
    index: &'a mut SourceCompactIndex,
    documents: &'a V,
    completed_block: &'a mut Option<NativeMainServiceBlockCompletion>,
    native_files: bool,
    owner_effect_ranges: Option<&'a mut Vec<(usize, usize, usize)>>,
}
impl crate::source_main_step_kernel::sealed::Sealed for NativeMainServiceStepLoan<'_> {}
impl crate::source_main_step_kernel::MainStepOwner for NativeMainServiceStepLoan<'_> {
    fn record_owner_effect_range(&mut self, trace: usize, start: usize, end: usize) {
        if let Some(ranges) = self.owner_effect_ranges.as_mut() {
            ranges.push((trace, start, end));
        }
    }
    fn preflight(
        &self,
        operation: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> Result<(), SourceRuleError> {
        use crate::source_main_step_kernel::MainStepPrimitive as Op;
        if self.runtime.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.runtime.core.native_main_binding.is_none()
            || !self.runtime.core.source_main_service_enabled()
            || self.runtime.core.effects_exhausted
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if let Op::ProcessBlock { block, height } = operation {
            let completed = self
                .completed_block
                .as_ref()
                .ok_or(SourceRuleError::Backend)?;
            if &completed.request.block != block || &completed.request.height != height {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        if let Op::File(operation) = operation {
            if !self.native_files {
                return Err(SourceRuleError::UnknownDependency);
            }
            if let crate::source_main_step_kernel::FileStepPrimitive::BlockTail(block, height) =
                operation.as_ref()
            {
                let completed = self
                    .completed_block
                    .as_ref()
                    .ok_or(SourceRuleError::Backend)?;
                if &completed.request.block != block || &completed.request.height != height {
                    return Err(SourceRuleError::InvalidInput);
                }
            }
        }
        if let Op::Atomic(call) = operation {
            if ![
                "Main.checkBlockLag",
                "Main.setPause",
                "Main.saveTrackHeight",
                "Main.loadTrackHeight",
                "Main.loadMaxProcessedHeight",
                "Main.getCurrentBlockHeight",
                "Main.checkNetworkStatus",
            ]
            .contains(&call.target.as_str())
            {
                return Err(SourceRuleError::UnknownDependency);
            }
        }
        Ok(())
    }
    fn execute(
        &mut self,
        operation: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> crate::source_main_step_kernel::MainStepPrimitiveExecution {
        use crate::source_main_step_kernel::{
            MainStepLegacyThrow, MainStepPrimitive as Op, MainStepPrimitiveExecution,
            MainStepPrimitiveOutcome,
        };
        let before = if matches!(operation, Op::ProcessBlock { .. })
            || matches!(operation,Op::File(operation) if matches!(operation.as_ref(),crate::source_main_step_kernel::FileStepPrimitive::BlockTail(..)))
        {
            self.completed_block
                .as_ref()
                .expect("preflighted actual completion")
                .effect_start
        } else {
            self.runtime.core.effects.len()
        };
        let result: R<V> = match operation {
            Op::ProcessBlock { .. } => {
                self.completed_block
                    .take()
                    .expect("preflighted actual block completion")
                    .result
            }
            Op::File(operation)
                if matches!(
                    operation.as_ref(),
                    crate::source_main_step_kernel::FileStepPrimitive::BlockTail(..)
                ) =>
            {
                self.completed_block
                    .take()
                    .expect("preflighted actual file/native completion")
                    .result
            }
            Op::File(operation) => self.runtime.native_main_retained_file_primitive(operation),
            Op::Atomic(call) => {
                // checkBlockLag and setPause bodies use their existing owner.
                // Reentering the nonreentrant invocation while catchup owns the
                // SAME queue would manufacture Backend before real source IO.
                self.runtime.source_main_dispatch(call)
            }
            Op::FetchBlock(height) => self.runtime.core.native_main_service_index(
                self.index,
                "fetchBlockData",
                vec![height.clone()],
                self.documents,
            ),
            _ => {
                let mut validity = vec![];
                let mut consumed_rows = 0;
                let mut storage_stop = None;
                NativeMainStepOwnerLoan {
                    runtime: self.runtime,
                    index: self.index,
                    validity: &mut validity,
                    consumed_rows: &mut consumed_rows,
                    storage_stop: &mut storage_stop,
                }
                .run(operation)
            }
        };
        let effects = self.runtime.core.effects[before..].to_vec();
        let direct_rpc = matches!(
            operation,
            Op::BlockCount | Op::FetchBlock(_) | Op::NetworkStatus
        ) || matches!(operation, Op::Atomic(call) if ["Main.checkBlockLag", "Main.getCurrentBlockHeight", "Main.checkNetworkStatus"].contains(&call.target.as_str()));
        let code = if matches!(operation,Op::File(operation) if matches!(operation.as_ref(),crate::source_main_step_kernel::FileStepPrimitive::CopyOnce(..)|crate::source_main_step_kernel::FileStepPrimitive::DeleteOnce(..)))
        {
            self.runtime.core.source_main_fs_failure_code(before)
        } else if direct_rpc {
            effects
                .iter()
                .rev()
                .find(|effect| {
                    effect.get("kind") == s("rpc-input") && effect.get("thrown") != V::Undefined
                })
                .map_or(V::Undefined, |effect| effect.get("thrown").get("code"))
        } else {
            V::Undefined
        };
        let outcome = if self.runtime.core.effects_exhausted {
            MainStepPrimitiveOutcome::Stopped(SourceRuleError::Capacity)
        } else {
            match result {
                Ok(value) => MainStepPrimitiveOutcome::Returned(value),
                Err(Fault::Thrown(exception)) => {
                    MainStepPrimitiveOutcome::Thrown(MainStepLegacyThrow { exception, code })
                }
                Err(Fault::Local(error)) => MainStepPrimitiveOutcome::Stopped(error),
            }
        };
        MainStepPrimitiveExecution { outcome, effects }
    }
}

// Explicit new-profile loan only. No persisted flag, legacy codec/field change,
// or inference from policy/native binding/service configuration alone.
const NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING: &str = "TL-MAIN-ACTIVE-SERVICE-SHARED-CLIENT-1";
impl ContractCore {
    fn native_main_service_client_rpc(&mut self, target: &str, args: &[V]) -> R<V> {
        if self.native_active_service_client_scope
            != Some(NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING)
            || self.policy != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.native_main_binding.is_none()
            || !self.source_main_service_enabled()
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.activation_client_locked = true;
        match target {
            "Client.getInstance" => {
                let client = self
                    .main_production
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .client
                    .clone();
                self.emit(obj(&[
                    ("kind", s("rpc-input")),
                    ("method", s(target)),
                    ("args", V::Array(args.to_vec())),
                    ("returned", client),
                ]));
                Ok(obj(&[("sourceActivationClient", V::Bool(true))]))
            }
            "Client.getChain" | "Client.getTest" => self.source_main_production_io(target, args),
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
}
impl SourceContractRuntime {
    fn native_main_service_with_client_scope<T>(
        &mut self,
        binding: &'static str,
        operation: impl FnOnce(&mut Self) -> T,
    ) -> Result<T, SourceRuleError> {
        if binding != NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING
            || self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.core.native_main_binding.is_none()
            || !self.core.source_main_service_enabled()
            || self.core.native_active_service_client_scope.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        self.core.native_active_service_client_scope = Some(binding);
        // Reset this loan on source/local return and Rust unwind alike. This
        // does not catch/convert a panic to a source outcome or undo finances.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(self)));
        self.core.native_active_service_client_scope = None;
        match result {
            Ok(value) => Ok(value),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    pub(crate) fn native_main_service_client_scope_value(&self) -> V {
        self.core
            .native_active_service_client_scope
            .map_or(V::Null, s)
    }
}
