// Additive retained native write callback. No source decoder/Logic/mark rerun.
impl SourceContractRuntime {
    pub(crate) fn native_main_service_append_with_escrow(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            if let Some(code) = frame.terminal {
                return NativeMainServiceStopV1::ProcessExited(code);
            }
            let row = if let Some(block) = &mut frame.block {
                block.row.as_mut()
            } else {
                frame.catchup.row.as_mut()
            };
            let Some(row) = row else {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
            };
            let Some(current) = row.current.as_mut() else {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
            };
            if let Some(error) = &current.ambiguous {
                return NativeMainServiceStopV1::StorageStopped(error.clone());
            }
            let Some(write) = &current.write else {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
            };
            match index.commit_prepared_validity_with_escrow(write, permit) {
                Ok(_) => NativeMainServiceStopV1::AwaitNativeWrite,
                Err(error) => {
                    current.ambiguous = Some(error.to_string());
                    native_main_service_row_stop(runtime.native_main_rows_advance(index, row))
                }
            }
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
}
