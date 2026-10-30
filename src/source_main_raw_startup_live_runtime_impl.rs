impl SourceContractRuntime {
    pub(crate) fn native_main_raw_startup_live_begin(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
        prefix: &crate::source_main_startup_prefix::MainStartupPrefixV1,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        // Preflight the actual prefix clock before even constructing the service
        // metadata. No timer/source/financial primitive is executed here.
        let clock = prefix.closed_source_clock()?;
        let mut service = self.native_main_retained_consensus_begin(caller, documents)?;
        service.kernel =
            crate::source_main_step_kernel::MainStepKernelV1::from_closed_startup_clock(
                caller.kernel.generation(),
                clock,
            );
        Ok(service)
    }
}
