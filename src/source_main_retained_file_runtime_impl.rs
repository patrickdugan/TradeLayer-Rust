// New explicit borrower: primitives execute on the same retained Core. No
// invocation begin/finish, effects drain, filesystem mirror or second index.
struct NativeMainRetainedFileLoanV1<'a> {
    runtime: &'a mut SourceContractRuntime,
    index: &'a SourceCompactIndex,
}
impl crate::source_main_step_kernel::sealed::Sealed for NativeMainRetainedFileLoanV1<'_> {}
impl crate::source_main_step_kernel::MainStepOwner for NativeMainRetainedFileLoanV1<'_> {
    fn preflight(
        &self,
        operation: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> Result<(), SourceRuleError> {
        use crate::source_main_step_kernel::{FileStepPrimitive as File, MainStepPrimitive as Op};
        if self.runtime.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.runtime.core.native_main_binding.is_none()
            || !self.runtime.core.source_main_service_enabled()
            || self.runtime.core.effects_exhausted
            || self.index.snapshot().is_err()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        match operation {
            Op::File(operation) if matches!(operation.as_ref(), File::BlockTail(..)) => {
                Err(SourceRuleError::UnknownDependency)
            }
            Op::File(_) => Ok(()),
            Op::Atomic(call)
                if [
                    "Main.setPause",
                    "Main.saveTrackHeight",
                    "Main.loadTrackHeight",
                    "Main.loadMaxProcessedHeight",
                ]
                .contains(&call.target.as_str()) =>
            {
                Ok(())
            }
            Op::PublishUnhandled(_) => Ok(()),
            _ => Err(SourceRuleError::UnknownDependency),
        }
    }
    fn execute(
        &mut self,
        operation: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> crate::source_main_step_kernel::MainStepPrimitiveExecution {
        use crate::source_main_step_kernel::{
            MainStepLegacyThrow, MainStepPrimitive as Op, MainStepPrimitiveExecution,
            MainStepPrimitiveOutcome,
        };
        let start = self.runtime.core.effects.len();
        let result = match operation {
            Op::File(operation) => self.runtime.native_main_retained_file_primitive(operation),
            Op::Atomic(call) => self.runtime.source_main_dispatch(call),
            Op::PublishUnhandled(error) => {
                if self.runtime.core.source_unhandled_rejections.len() >= MAX_TABLE_ROWS {
                    Err(SourceRuleError::Capacity.into())
                } else {
                    let value = obj(&[
                        ("name", s(&error.exception.name)),
                        ("message", s(&error.exception.message)),
                    ]);
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
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        };
        let effects = self.runtime.core.effects[start..].to_vec();
        let code = if matches!(operation,Op::File(operation)if matches!(operation.as_ref(),crate::source_main_step_kernel::FileStepPrimitive::CopyOnce(..)|crate::source_main_step_kernel::FileStepPrimitive::DeleteOnce(..)))
        {
            self.runtime.core.source_main_fs_failure_code(start)
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
impl SourceContractRuntime {
    pub(crate) fn native_main_retained_file_preflight(
        &self,
        command: &NativeMainRetainedCallerV1,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        self.native_main_service_preflight_file_profile(documents, true)?;
        if let NativeMainRetainedCallerV1::SyncIndex { max } = command {
            native_main_retained_height(max)?;
        }
        Ok(())
    }
    pub(crate) fn native_main_retained_file_apply(
        &mut self,
        index: &SourceCompactIndex,
        kernel: &mut crate::source_main_step_kernel::MainStepKernelV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<
        crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    > {
        let shutdown = matches!(
            &command,
            crate::source_main_step_kernel::MainStepCommandV1::Shutdown(_)
        );
        let evaluation = kernel.apply(
            &mut NativeMainRetainedFileLoanV1 {
                runtime: self,
                index,
            },
            command,
        )?;
        for effect in &evaluation.effects {
            if matches!(effect.get("kind"),V::String(ref kind) if ["timer-scheduled","timer-fired","timer-cancelled","process-exit"].contains(&kind.as_str())||(shutdown&&kind=="console"))
            {
                self.core.emit(effect.clone());
            }
        }
        Ok(evaluation)
    }
}
