impl SourceContractRuntime {
    fn native_main_retained_consensus_service_frame(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        let ready_detached = caller.pc == NativeMainRetainedCallerPcV1::Detached
            && caller.command == NativeMainRetainedCallerV1::Initialize
            && caller.caller.is_none()
            && caller.inline.is_none()
            && caller.extract_io.local_stop.is_none()
            && caller.extract_io.storage_stop.is_none()
            && caller.extract.as_ref().is_some_and(|extract| {
                extract.detached_pending()
                    && matches!(
                        extract.caller_outcome(),
                        Some(crate::validity_source::SourceOutcome::Returned(
                            V::Undefined
                        ))
                    )
            });
        if !(caller.pc == NativeMainRetainedCallerPcV1::Consensus || ready_detached)
            || caller.invocation.is_none()
            || caller.local_stop.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let catchup = self.native_main_catchup_frame(caller.genesis.clone(), None)?;
        let generation = caller.kernel.generation();
        let bootstrap_start = caller.effect_start;
        let invocation_effect_start = caller.effect_start;
        Ok(NativeMainServiceContinuationV1 {
            catchup,
            client_binding: NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING,
            bootstrap_start,
            invocation_effect_start,
            inline_profile: true,
            borrowed_invocation: true,
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
}
