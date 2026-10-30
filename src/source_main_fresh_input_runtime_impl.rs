impl SourceContractRuntime {
    pub(crate) fn native_main_feed_fresh_rpc(
        &mut self,
        frame: &mut NativeMainServiceContinuationV1,
        input: &crate::source_main_fresh_input::NativeMainFreshRpcInputV1,
    ) -> Result<(), SourceRuleError> {
        if frame.terminal.is_some() || frame.stopped.is_some() || self.core.effects_exhausted {
            return Err(SourceRuleError::InvalidInput);
        }
        // Validate the complete proposed document set before publishing IO.
        // This does not execute, drain or replace any source/Kernel job.
        let documents = input.documents().unwrap_or(&frame.documents);
        self.native_main_service_preflight(documents)?;
        let documents = input.documents().cloned();
        self.source_main_step_update_io(input.io().clone())
            .map_err(|error| match error {
                Fault::Local(error) => error,
                Fault::Thrown(_) => SourceRuleError::Backend,
            })?;
        if let Some(documents) = documents {
            frame.documents = documents;
        }
        Ok(())
    }
}
#[cfg(test)]
impl NativeMainServiceContinuationV1 {
    pub(crate) fn fresh_input_test_live_request(
        &self,
    ) -> Option<crate::source_main_step_kernel::MainServiceBlockRequest> {
        self.block.as_ref().map(|block| block.request.clone())
    }
    pub(crate) fn fresh_input_test_documents(&self) -> &V {
        &self.documents
    }
}
