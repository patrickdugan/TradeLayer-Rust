// Scoped suspended-Main bridge. The only mutable financial and index owners
// are lent by the native13 session; no owner or journal is cloned into a job.
pub(crate) struct NativeMainStepOwnerLoan<'a> {
    pub runtime: &'a mut SourceContractRuntime,
    pub index: &'a mut crate::source_compact_index_v2::SourceCompactIndex,
    pub validity: &'a mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
    pub consumed_rows: &'a mut u64,
    pub storage_stop: &'a mut Option<String>,
}

pub(crate) struct NativeMainStepRuntimeEvaluation {
    pub evaluation: Result<
        crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    >,
    pub validity: Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
    pub consumed_rows: u64,
    pub storage_stop: Option<String>,
}

impl crate::source_main_step_kernel::sealed::Sealed for NativeMainStepOwnerLoan<'_> {}

impl SourceContractRuntime {
    pub(crate) fn native_main_step_apply(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        kernel: &mut crate::source_main_step_kernel::MainStepKernelV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> NativeMainStepRuntimeEvaluation {
        let mut validity = vec![];
        let mut consumed_rows = 0;
        let mut storage_stop = None;
        let evaluation = kernel.apply(
            &mut NativeMainStepOwnerLoan {
                runtime: self,
                index,
                validity: &mut validity,
                consumed_rows: &mut consumed_rows,
                storage_stop: &mut storage_stop,
            },
            command,
        );
        NativeMainStepRuntimeEvaluation {
            evaluation,
            validity,
            consumed_rows,
            storage_stop,
        }
    }

    fn source_main_step_rows(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        rows: V,
        height: V,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed_rows: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        let V::Array(rows) = rows else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        let rows = rows.into_iter().map(|row| {
            *consumed_rows += 1;
            Ok(row)
        });
        self.native_main_process_rows_with(index, rows, height, true, validity, storage_stop)
    }

    fn source_main_step_update_io(&mut self, facts: V) -> R<V> {
        facts.validate()?;
        let V::Object(fields) = &facts else {
            return Err(SourceRuleError::InvalidInput.into());
        };
        if fields.iter().any(|(key, _)| {
            !matches!(
                key.as_str(),
                "height" | "blocks" | "networkInfo" | "failures"
            )
        }) {
            return Err(SourceRuleError::InvalidInput.into());
        }
        for name in ["blocks", "failures"] {
            if !matches!(facts.get(name), V::Undefined | V::Array(_)) {
                return Err(SourceRuleError::InvalidInput.into());
            }
        }
        // Data-only changes preserve the real client, receivers, RPC ordinals,
        // file facts and all financial state. Validate the whole proposed value
        // before publishing any field.
        let mut updated = self
            .core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .io
            .clone();
        for (key, value) in fields {
            set(&mut updated, key, value.clone())?;
        }
        updated.validate()?;
        self.core
            .main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .io = updated;
        Ok(V::Undefined)
    }
}

impl NativeMainStepOwnerLoan<'_> {
    fn no_hidden_file_timer(&self) -> Result<(), SourceRuleError> {
        let facts = self
            .runtime
            .core
            .source_main_service_get("fileIO")
            .map_err(|_| SourceRuleError::InvalidInput)?
            .get("facts")
            .get("failures");
        if let V::Array(rows) = facts {
            if rows.iter().any(|row| {
                matches!(row.get("method"), V::String(ref method)
                    if matches!(method.as_str(), "copyFile" | "unlink"))
                    && [s("EBUSY"), s("EPERM")].contains(&row.get("code"))
            }) {
                // This explicit immediate-IO profile cannot run a legacy
                // auto-drained 50ms retry and call the result suspended.
                return Err(SourceRuleError::UnknownDependency);
            }
        }
        Ok(())
    }

    fn run(&mut self, operation: &crate::source_main_step_kernel::MainStepPrimitive) -> R<V> {
        use crate::source_main_step_kernel::MainStepPrimitive as Op;
        match operation {
            Op::File(operation) => self.file_run(operation),
            Op::BlockCount => self
                .runtime
                .core
                .source_main_production_io("Client.getBlockCount", &[]),
            Op::HeightRead(name) => self.runtime.core.source_main_height_read(name),
            Op::HeightWrite { name, value } => self
                .runtime
                .core
                .source_main_height_write(name, value.clone()),
            Op::NetworkStatus => self.runtime.core.source_main_service_network(),
            Op::PauseRead => self.runtime.core.source_main_service_get("pause"),
            Op::FetchBlock(height) => self.runtime.core.native_main_index(
                self.index,
                "fetchBlockData",
                vec![height.clone()],
            ),
            Op::ProcessBlock { block, height } => {
                let call = DependencyCall {
                    target: "Main.processBlock".into(),
                    arguments: vec![block.clone(), height.clone()],
                };
                let index = &mut *self.index;
                let validity = &mut *self.validity;
                let consumed_rows = &mut *self.consumed_rows;
                let storage_stop = &mut *self.storage_stop;
                let result =
                    self.runtime
                        .source_main_invoke_with(&call, false, &mut |runtime, _| {
                            runtime.source_main_step_block(
                                index,
                                block.clone(),
                                height.clone(),
                                validity,
                                consumed_rows,
                                storage_stop,
                            )
                        })?;
                match result {
                    DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => {
                        Ok(value)
                    }
                    DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                        Err(Fault::Thrown(error))
                    }
                }
            }
            Op::Atomic(call) => {
                let result =
                    self.runtime
                        .source_main_invoke_with(call, false, &mut |runtime, call| {
                            runtime.source_main_dispatch(call)
                        })?;
                match result {
                    DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => {
                        Ok(value)
                    }
                    DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                        Err(Fault::Thrown(error))
                    }
                }
            }
            Op::UpdateIo(facts) => self.runtime.source_main_step_update_io(facts.clone()),
            Op::PublishUnhandled(thrown) => {
                let value = obj(&[
                    ("name", s(&thrown.exception.name)),
                    ("message", s(&thrown.exception.message)),
                ]);
                if self.runtime.core.source_unhandled_rejections.len() >= MAX_TABLE_ROWS {
                    return Err(SourceRuleError::Capacity.into());
                }
                self.runtime
                    .core
                    .source_unhandled_rejections
                    .push(value.clone());
                self.runtime.core.emit(obj(&[
                    ("kind", s("unhandled-rejection")),
                    ("thrown", value),
                ]));
                Ok(V::Undefined)
            }
            Op::Number(value) => Ok(n(amm_quote_number(value)?)),
            Op::Add(left, right) => {
                source_js_plus(&amm_quote_primitive(left)?, &amm_quote_primitive(right)?)
            }
            Op::Greater(left, right) => Ok(V::Bool(amm_static_greater(left, right)?)),
            Op::LessEqual(left, right) => {
                let left = amm_quote_primitive(left)?;
                let right = amm_quote_primitive(right)?;
                Ok(V::Bool(
                    if let (V::String(left), V::String(right)) = (&left, &right) {
                        left.encode_utf16().cmp(right.encode_utf16()).is_le()
                    } else {
                        number(&left)? <= number(&right)?
                    },
                ))
            }
            Op::Backoff { delay, attempt } => {
                Ok(n(amm_quote_number(delay)? * 2_f64.powf(*attempt as f64)))
            }
        }
    }
}

impl crate::source_main_step_kernel::MainStepOwner for NativeMainStepOwnerLoan<'_> {
    fn preflight(
        &self,
        operation: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> Result<(), SourceRuleError> {
        use crate::source_main_step_kernel::MainStepPrimitive as Op;
        if self.runtime.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.runtime.core.native_main_binding.is_none()
            || !self.runtime.core.effects.is_empty()
            || self.runtime.core.effects_exhausted
        {
            return Err(SourceRuleError::InvalidInput);
        }
        match operation {
            Op::ProcessBlock { .. } => self.no_hidden_file_timer(),
            Op::Atomic(call)
                if !matches!(
                    call.target.as_str(),
                    "Main.setPause"
                        | "Main.saveTrackHeight"
                        | "Main.loadTrackHeight"
                        | "Main.loadMaxProcessedHeight"
                        | "Main.checkBlockLag"
                        | "Main.getCurrentBlockHeight"
                        | "Main.checkNetworkStatus"
                ) =>
            {
                Err(SourceRuleError::UnknownDependency)
            }
            _ => Ok(()),
        }
    }

    fn execute(
        &mut self,
        operation: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> crate::source_main_step_kernel::MainStepPrimitiveExecution {
        use crate::source_main_step_kernel::{
            MainStepLegacyThrow, MainStepPrimitiveExecution, MainStepPrimitiveOutcome,
        };
        let result = self.run(operation);
        let effects = self.runtime.take_effects();
        // Only these literal call sites rethrow the directly observed RPC
        // error. A prior caught filesystem/RPC error in ProcessBlock is NOT
        // the later thrown value; do not attach its code by display matching.
        let direct_rpc_error = match operation {
            crate::source_main_step_kernel::MainStepPrimitive::BlockCount
            | crate::source_main_step_kernel::MainStepPrimitive::FetchBlock(_)
            | crate::source_main_step_kernel::MainStepPrimitive::NetworkStatus => true,
            crate::source_main_step_kernel::MainStepPrimitive::Atomic(call) => matches!(
                call.target.as_str(),
                "Main.getCurrentBlockHeight" | "Main.checkNetworkStatus" | "Main.checkBlockLag"
            ),
            _ => false,
        };
        let file_error = matches!(operation,crate::source_main_step_kernel::MainStepPrimitive::File(operation) if matches!(operation.as_ref(),crate::source_main_step_kernel::FileStepPrimitive::CopyOnce(..)|crate::source_main_step_kernel::FileStepPrimitive::DeleteOnce(..)));
        let code = if file_error {
            effects
                .iter()
                .rev()
                .find(|effect| {
                    effect.get("kind") == s("filesystem") && effect.get("thrown") != V::Undefined
                })
                .map_or(V::Undefined, |effect| effect.get("thrown").get("code"))
        } else if direct_rpc_error {
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
        let outcome = match result {
            Ok(value) => MainStepPrimitiveOutcome::Returned(value),
            Err(Fault::Thrown(exception)) => {
                MainStepPrimitiveOutcome::Thrown(MainStepLegacyThrow { exception, code })
            }
            Err(Fault::Local(error)) => MainStepPrimitiveOutcome::Stopped(error),
        };
        MainStepPrimitiveExecution { outcome, effects }
    }
}
