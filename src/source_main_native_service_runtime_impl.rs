pub const NATIVE_MAIN_ACTIVE_SERVICE_PROFILE: &str = "TL-MAIN-NATIVE-ACTIVE-SERVICE-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainServiceStopV1 {
    AwaitNativeWrite,
    PendingTimer,
    Quiescent,
    DrainBound,
    SourceStopped(SourceRuleError),
    StorageStopped(String),
    KernelStopped(crate::source_main_step_kernel::MainStepError),
    ProcessExited(u8),
}
pub(crate) struct NativeMainServiceContinuationV1 {
    catchup: NativeMainCatchupContinuationV1,
    client_binding: &'static str,
    inline_profile: bool,
    borrowed_invocation: bool,
    bootstrap_start: usize,
    invocation_effect_start: usize,
    kernel: crate::source_main_step_kernel::MainStepKernelV1,
    documents: V,
    tail_started: bool,
    sync_job: Option<u64>,
    caller_result: Option<Result<DependencyReturn, SourceRuleError>>,
    block: Option<NativeMainServiceBlockFrame>,
    completed_block: Option<NativeMainServiceBlockCompletion>,
    completed_validity: Vec<StreamedValidityEvaluation>,
    live_consumed: u64,
    events: Vec<crate::source_main_step_kernel::MainStepEvaluationV1>,
    stopped: Option<SourceRuleError>,
    terminal: Option<u8>,
}
impl NativeMainServiceContinuationV1 {
    pub(crate) fn bootstrap_range(&self) -> std::ops::Range<usize> {
        self.bootstrap_start..self.invocation_effect_start
    }
    pub(crate) fn invocation_effect_start(&self) -> usize {
        self.invocation_effect_start
    }
    pub(crate) fn caller_result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        self.caller_result.as_ref()
    }
    pub(crate) fn pending_write(&self) -> Option<&NativePreparedValidityWriteV1> {
        self.block
            .as_ref()
            .and_then(|block| block.row.as_ref())
            .and_then(|row| row.pending_write())
            .or_else(|| self.catchup.pending_write())
    }
    pub(crate) fn consumed_rows(&self) -> u64 {
        self.catchup.consumed_rows()
            + self.live_consumed
            + self.block.as_ref().map_or(0, |block| {
                block.consumed
                    + block
                        .row
                        .as_ref()
                        .map_or(0, NativeMainRowContinuationV1::consumed_rows)
            })
    }
    pub(crate) fn terminal_code(&self) -> Option<u8> {
        self.terminal
    }
    pub(crate) fn next_timer(&self) -> Option<u64> {
        self.kernel.next_timer()
    }
    pub(crate) fn kernel_value(&self) -> V {
        self.kernel.value()
    }
}
fn native_main_service_row_stop(stop: NativeMainRowStopV1) -> NativeMainServiceStopV1 {
    match stop {
        NativeMainRowStopV1::AwaitNativeWrite => NativeMainServiceStopV1::AwaitNativeWrite,
        NativeMainRowStopV1::StorageStopped(e) => NativeMainServiceStopV1::StorageStopped(e),
        NativeMainRowStopV1::SourceStopped(e) => NativeMainServiceStopV1::SourceStopped(e),
        NativeMainRowStopV1::Complete => NativeMainServiceStopV1::Quiescent,
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_service_preflight(
        &self,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        self.native_main_service_preflight_file_profile(documents, false)
    }
    fn native_main_service_preflight_file_profile(
        &self,
        documents: &V,
        allow_file_retries: bool,
    ) -> Result<(), SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.core.native_main_binding.is_none()
            || !self.core.source_main_service_enabled()
        {
            return Err(SourceRuleError::UnknownDependency);
        }
        documents.validate()?;
        let V::Array(rows) = documents else {
            return Err(SourceRuleError::InvalidInput);
        };
        for row in rows {
            let V::Object(fields) = row else {
                return Err(SourceRuleError::InvalidInput);
            };
            if fields
                .iter()
                .any(|(key, _)| !["id", "raw", "document"].contains(&key.as_str()))
                || !matches!(row.get("id"), V::String(_))
                || !matches!(row.get("raw"), V::Undefined | V::String(_))
            {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        if !allow_file_retries {
            let failures = self
                .core
                .main_production
                .as_ref()
                .and_then(|production| production.service.as_ref())
                .ok_or(SourceRuleError::UnknownDependency)?
                .get("fileIO")
                .get("facts")
                .get("failures");
            if let V::Array(rows) = failures {
                if rows.iter().any(|row| matches!(row.get("method"),V::String(ref name) if ["copyFile", "unlink"].contains(&name.as_str())) && [s("EBUSY"), s("EPERM")].contains(&row.get("code"))) {
                // This new profile explicitly uses immediate recorded file IO;
                // actual 50ms retry frames remain in the existing file profile.
                return Err(SourceRuleError::UnknownDependency);
            }
            }
        }
        Ok(())
    }
    pub(crate) fn native_main_service_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        start: V,
        generation: u64,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        self.native_main_service_begin_with_loader(
            start,
            generation,
            documents,
            |core, documents| core.native_main_service_load_module(index, documents),
        )
    }
    fn native_main_service_begin_with_loader(
        &mut self,
        start: V,
        generation: u64,
        documents: V,
        load: impl FnOnce(&mut ContractCore, &V) -> Result<(), SourceRuleError>,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        self.native_main_service_preflight(&documents)?;
        let bootstrap_start = self.core.effects.len();
        load(&mut self.core, &documents)?;
        let invocation_effect_start = self.core.effects.len();
        let catchup = self.native_main_catchup_begin_owned(start)?;
        Ok(NativeMainServiceContinuationV1 {
            catchup,
            client_binding: NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING,
            bootstrap_start,
            invocation_effect_start,
            inline_profile: false,
            borrowed_invocation: false,
            kernel: crate::source_main_step_kernel::MainStepKernelV1::new(generation),
            documents,
            tail_started: false,
            sync_job: None,
            caller_result: None,
            block: None,
            completed_block: None,
            completed_validity: vec![],
            live_consumed: 0,
            events: vec![],
            stopped: None,
            terminal: None,
        })
    }
    fn native_main_service_kernel_command(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
        enqueue_only: bool,
    ) -> Result<
        crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    > {
        let shutdown = matches!(
            &command,
            crate::source_main_step_kernel::MainStepCommandV1::Shutdown(_)
        );
        let evaluation = {
            let mut loan = NativeMainServiceStepLoan {
                runtime: self,
                index,
                documents: &frame.documents,
                completed_block: &mut frame.completed_block,
            };
            if enqueue_only {
                frame.kernel.service_enqueue(&mut loan, command)
            } else {
                frame.kernel.service_drain_one(&mut loan)
            }
        }?;
        // Authoritative DB/RPC effects already live in Core. Append only these
        // real Kernel scheduling/control events, once, in their actual order.
        for effect in &evaluation.effects {
            if matches!(effect.get("kind"),V::String(ref kind) if (["timer-scheduled", "timer-fired", "timer-cancelled", "process-exit"].contains(&kind.as_str()) || (shutdown && kind == "console")))
            {
                self.core.emit(effect.clone());
            }
        }
        Ok(evaluation)
    }
    fn native_main_service_accept_evaluation(
        &mut self,
        frame: &mut NativeMainServiceContinuationV1,
        evaluation: crate::source_main_step_kernel::MainStepEvaluationV1,
    ) {
        use crate::source_main_step_kernel::{MainStepSourceResult, MainStepStop};
        if let Some(id) = frame.sync_job {
            if let Some((_, result)) = evaluation.completed.iter().find(|(job, _)| *job == id) {
                let result = match result {
                    MainStepSourceResult::Fulfilled(v) => Ok(v.clone()),
                    MainStepSourceResult::Thrown(e) => Err(Fault::Thrown(e.exception.clone())),
                };
                self.native_main_catchup_finish(&mut frame.catchup, result);
                frame.caller_result = frame.catchup.result.clone();
                frame.sync_job = None;
            }
        }
        match &evaluation.stop {
            MainStepStop::SourceOwnerStopped(e) => frame.stopped = Some(e.clone()),
            MainStepStop::ProcessExited(code) => frame.terminal = Some(*code),
            _ => (),
        }
        frame.events.push(evaluation);
    }
    pub(crate) fn native_main_service_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        limit: usize,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            runtime.native_main_service_advance_inner(index, frame, limit)
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    fn native_main_service_advance_inner(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        limit: usize,
    ) -> NativeMainServiceStopV1 {
        self.native_main_service_advance_with_access(
            frame,
            limit,
            &mut NativeMainLiveSourceMetadataAccess { index },
        )
    }
    fn native_main_service_advance_with_access(
        &mut self,
        frame: &mut NativeMainServiceContinuationV1,
        limit: usize,
        access: &mut impl NativeMainSourceMetadataAccess,
    ) -> NativeMainServiceStopV1 {
        use crate::source_main_step_kernel::MainStepCommandV1;
        if limit == 0 || limit > 4096 {
            return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        for _ in 0..limit {
            if let Some(code) = frame.terminal {
                return NativeMainServiceStopV1::ProcessExited(code);
            }
            if let Some(error) = &frame.stopped {
                return NativeMainServiceStopV1::SourceStopped(error.clone());
            }
            if frame.events.len() >= 4096 {
                frame.stopped = Some(SourceRuleError::Capacity);
                continue;
            }
            if !frame.tail_started && frame.caller_result.is_none() {
                if matches!(frame.catchup.pc, NativeMainCatchupPcV1::Tail) {
                    match access.kernel(
                        self,
                        frame,
                        MainStepCommandV1::Start {
                            method: "Main.syncIfNecessary".into(),
                            arguments: vec![],
                        },
                        true,
                    ) {
                        Ok(evaluation) => {
                            frame.tail_started = true;
                            frame.sync_job = evaluation.started;
                            self.native_main_service_accept_evaluation(frame, evaluation);
                        }
                        Err(e) => return NativeMainServiceStopV1::KernelStopped(e),
                    }
                    continue;
                }
                match self.native_main_catchup_drive_budget_with_access(
                    &mut frame.catchup,
                    Some(1),
                    access,
                ) {
                    NativeMainServiceCatchupDrive::Yielded => continue,
                    NativeMainServiceCatchupDrive::Stopped(NativeMainRowStopV1::Complete) => {
                        frame.caller_result = frame.catchup.result.clone();
                        return NativeMainServiceStopV1::Quiescent;
                    }
                    NativeMainServiceCatchupDrive::Stopped(stop) => {
                        return native_main_service_row_stop(stop)
                    }
                }
            }
            if let Some(block) = &mut frame.block {
                let stop = access.block(self, block, &frame.documents);
                if stop != NativeMainRowStopV1::Complete {
                    return native_main_service_row_stop(stop);
                }
                let mut block = frame.block.take().expect("actual completed block");
                frame.live_consumed += block.consumed;
                frame.completed_validity.append(&mut block.validity);
                let result = match block.result.take().expect("actual block result") {
                    Err(Fault::Local(error)) => {
                        frame.stopped = Some(error);
                        continue;
                    }
                    result => result,
                };
                frame.completed_block = Some(NativeMainServiceBlockCompletion {
                    request: block.request,
                    result,
                    effect_start: block.effect_start,
                });
            }
            if let Some(completed) = &frame.completed_block {
                if frame.kernel.service_next_block().as_ref() != Some(&completed.request) {
                    frame.stopped = Some(SourceRuleError::InvalidInput);
                    continue;
                }
            } else if let Some(request) = frame.kernel.service_next_block() {
                if frame.catchup.invocation.is_some() {
                    // Never manufacture outer completion to evade queue entry.
                    frame.stopped = Some(SourceRuleError::Backend);
                    continue;
                }
                let begun = if frame.borrowed_invocation {
                    self.native_main_service_block_frame(request, None, self.core.effects.len())
                } else {
                    self.native_main_service_block_begin(request)
                };
                match begun {
                    Ok(mut block) => {
                        block.inline_profile = frame.inline_profile;
                        frame.block = Some(block);
                    }
                    Err(e) => frame.stopped = Some(e),
                }
                continue;
            }
            if !frame.kernel.service_has_ready() {
                return if frame.kernel.next_timer().is_some() {
                    NativeMainServiceStopV1::PendingTimer
                } else {
                    NativeMainServiceStopV1::Quiescent
                };
            }
            match access.kernel(
                self,
                frame,
                MainStepCommandV1::DrainReady { limit: 1 },
                false,
            ) {
                Ok(evaluation) => self.native_main_service_accept_evaluation(frame, evaluation),
                Err(e) => return NativeMainServiceStopV1::KernelStopped(e),
            }
        }
        NativeMainServiceStopV1::DrainBound
    }
    pub(crate) fn native_main_service_pause(
        &mut self,
        frame: &NativeMainServiceContinuationV1,
    ) -> Result<DependencyReturn, SourceRuleError> {
        if frame.terminal.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        // Literal setPause has no DB/queue use. This is the SAME closed variable,
        // and it must remain callable while real native validity is unresolved.
        returned(self.source_main_dispatch(&DependencyCall {
            target: "Main.setPause".into(),
            arguments: vec![],
        }))
        .map(|value| match value {
            DependencyReturn::Pending(v) => DependencyReturn::Immediate(v),
            other => other,
        })
    }
    pub(crate) fn native_main_service_control(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<(), crate::source_main_step_kernel::MainStepError> {
        use crate::source_main_step_kernel::{MainStepCommandV1, MainStepError};
        if frame.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if !matches!(
            command,
            MainStepCommandV1::FireTimer { .. }
                | MainStepCommandV1::ClearTimer { .. }
                | MainStepCommandV1::Shutdown(_)
        ) {
            return Err(MainStepError::InvalidInput);
        }
        let evaluation = self.native_main_service_kernel_command(index, frame, command, true)?;
        self.native_main_service_accept_evaluation(frame, evaluation);
        Ok(())
    }
    pub(crate) fn native_main_service_append(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            runtime.native_main_service_append_inner(index, frame)
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    fn native_main_service_append_inner(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
    ) -> NativeMainServiceStopV1 {
        if frame.terminal.is_some() {
            return NativeMainServiceStopV1::ProcessExited(frame.terminal.unwrap());
        }
        if let Some(inline) = frame.block.as_mut().and_then(|b| b.inline_index.as_mut()) {
            if inline.write.is_some() {
                return native_main_service_row_stop(
                    self.core
                        .native_main_inline_index_append(index, inline, None),
                );
            }
        }
        let row = if let Some(block) = &mut frame.block {
            block.row.as_mut()
        } else {
            frame.catchup.row.as_mut()
        };
        let Some(row) = row else {
            return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        native_main_service_row_stop(self.native_main_rows_append(index, row))
    }
    pub(crate) fn native_main_service_resolve_post(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            runtime.native_main_service_resolve_post_inner(index, frame)
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    fn native_main_service_resolve_post_inner(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
    ) -> NativeMainServiceStopV1 {
        if frame.terminal.is_some() {
            return NativeMainServiceStopV1::ProcessExited(frame.terminal.unwrap());
        }
        if let Some(inline) = frame.block.as_mut().and_then(|b| b.inline_index.as_mut()) {
            if inline.write.is_some() {
                let stop = self.core.native_main_inline_index_resolve(index, inline);
                return if stop == NativeMainRowStopV1::Complete {
                    self.native_main_service_advance_inner(index, frame, 4096)
                } else {
                    native_main_service_row_stop(stop)
                };
            }
        }
        let row = if let Some(block) = &mut frame.block {
            block.row.as_mut()
        } else {
            frame.catchup.row.as_mut()
        };
        let Some(row) = row else {
            return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let stop = self.native_main_rows_resolve_post(index, row);
        if stop == NativeMainRowStopV1::Complete {
            self.native_main_service_advance_inner(index, frame, 4096)
        } else {
            native_main_service_row_stop(stop)
        }
    }
}
