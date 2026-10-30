pub const NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE: &str =
    "TL-MAIN-AUTOMATIC-FILE-NATIVE-SERVICE-13-1";
struct NativeMainAutomaticFilePrefixV1 {
    request: crate::source_main_step_kernel::MainServiceBlockRequest,
    invocation: Option<SourceMainInvocationV1>,
    effect_start: usize,
}
impl NativeMainServiceContinuationV1 {
    pub(crate) fn automatic_file_profile(&self) -> bool {
        self.native_file_profile
    }
    fn visit_automatic_file_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        if !self.native_file_profile {
            return Ok(());
        }
        visitor(
            "automaticFile/profile",
            &s(NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE),
        )?;
        self.visit_automatic_effect_provenance(visitor)?;
        visitor(
            "automaticFile/prefix/present",
            &V::Bool(self.file_prefix.is_some()),
        )?;
        if let Some(prefix) = &self.file_prefix {
            visitor(
                "automaticFile/prefix/job",
                &s(prefix.request.job.to_string()),
            )?;
            native_main_row_visit_value(
                "automaticFile/prefix/block",
                &prefix.request.block,
                visitor,
            )?;
            native_main_row_visit_value(
                "automaticFile/prefix/height",
                &prefix.request.height,
                visitor,
            )?;
            visitor(
                "automaticFile/prefix/effectStart",
                &s(prefix.effect_start.to_string()),
            )?;
            visitor(
                "automaticFile/prefix/invocation/present",
                &V::Bool(prefix.invocation.is_some()),
            )?;
            if let Some(i) = &prefix.invocation {
                visitor("automaticFile/prefix/invocation/target", &s(&i.target))?;
                visitor(
                    "automaticFile/prefix/invocation/spotMain",
                    &V::Bool(i.spot_main),
                )?;
                visitor(
                    "automaticFile/prefix/invocation/rejectedBefore",
                    &s(i.rejected_before.to_string()),
                )?;
                visitor(
                    "automaticFile/prefix/invocation/transferRejectedBefore",
                    &s(i.transfer_rejected_before.to_string()),
                )?;
            }
        }
        Ok(())
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_automatic_file_preflight(
        &self,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        self.native_main_service_preflight_file_profile(documents, true)
    }
    pub(crate) fn native_main_automatic_file_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        start: V,
        generation: u64,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        let mut frame = self.native_main_service_begin_with_loader_profile(
            start,
            generation,
            documents,
            true,
            |core, documents| core.native_main_service_load_module(index, documents),
        )?;
        frame
            .kernel
            .enable_automatic_native_files()
            .map_err(|_| SourceRuleError::InvalidInput)?;
        frame.native_file_profile = true;
        frame.inline_profile = true;
        Ok(frame)
    }
    fn native_main_automatic_file_prefix_begin(
        &mut self,
        frame: &mut NativeMainServiceContinuationV1,
        request: crate::source_main_step_kernel::MainServiceBlockRequest,
    ) -> Result<(), SourceRuleError> {
        if !frame.native_file_profile
            || frame.file_prefix.is_some()
            || frame.block.is_some()
            || frame.catchup.invocation.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let effect_start = self.core.effects.len();
        let invocation = if frame.borrowed_invocation {
            None
        } else {
            Some(self.source_main_invocation_begin(&DependencyCall {
                target: "Main.processBlock".into(),
                arguments: vec![request.block.clone(), request.height.clone()],
            })?)
        };
        frame.file_prefix = Some(Box::new(NativeMainAutomaticFilePrefixV1 {
            request,
            invocation,
            effect_start,
        }));
        Ok(())
    }
    fn native_main_automatic_file_tail_frame(
        &self,
        frame: &mut NativeMainServiceContinuationV1,
        request: crate::source_main_step_kernel::MainServiceBlockRequest,
    ) -> Result<NativeMainServiceBlockFrame, SourceRuleError> {
        if !frame.native_file_profile
            || frame.file_prefix.as_ref().is_none_or(|p| {
                p.request != request || (!frame.borrowed_invocation && p.invocation.is_none())
            })
        {
            return Err(SourceRuleError::InvalidInput);
        }
        frame
            .kernel
            .service_park_automatic_file_tail(&request)
            .map_err(|_| SourceRuleError::InvalidInput)?;
        let prefix = frame.file_prefix.take().expect("same actual file prefix");
        let invocation_effect_start = prefix.effect_start;
        let mut block = self.native_main_service_block_frame(
            request,
            prefix.invocation,
            self.core.effects.len(),
        )?;
        block.file_invocation_effect_start = Some(invocation_effect_start);
        block.pc = NativeMainServiceBlockPc::InlineIndex;
        block.inline_profile = true;
        block.file_prefix_done = true;
        Ok(block)
    }
    fn native_main_automatic_file_accept(
        &mut self,
        frame: &mut NativeMainServiceContinuationV1,
        evaluation: &crate::source_main_step_kernel::MainStepEvaluationV1,
    ) {
        use crate::source_main_step_kernel::MainStepSourceResult;
        let result = frame.file_prefix.as_ref().and_then(|p| {
            evaluation
                .completed
                .iter()
                .find(|(job, _)| *job == p.request.job)
                .map(|(_, result)| result)
        });
        if let Some(result) = result {
            // A genuine early reorg result or thrown prefix never reached the
            // native tail. Finish ONLY its retained actual invocation.
            let prefix = frame.file_prefix.take().expect("same completed file child");
            let result = match result {
                MainStepSourceResult::Fulfilled(value) => Ok(value.clone()),
                MainStepSourceResult::Thrown(error) => Err(Fault::Thrown(error.exception.clone())),
            };
            if let Some(invocation) = prefix.invocation {
                if let Err(error) = self.source_main_invocation_finish(invocation, result) {
                    frame.stopped = Some(error);
                }
            }
        }
    }
    pub(crate) fn native_main_automatic_file_enqueue(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<u64, crate::source_main_step_kernel::MainStepError> {
        use crate::source_main_step_kernel::MainStepError;
        if !frame.native_file_profile || frame.terminal.is_some() || frame.stopped.is_some() {
            return Err(MainStepError::InvalidInput);
        }
        let evaluation = self.native_main_service_kernel_command(index, frame, command, true)?;
        let started = evaluation.started;
        self.native_main_service_accept_evaluation(frame, evaluation);
        started.ok_or(MainStepError::InvalidInput)
    }
}

#[cfg(test)]
include!("source_main_automatic_file_test_access_impl.rs");
