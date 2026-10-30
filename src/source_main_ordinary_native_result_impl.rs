impl SourceContractRuntime {
    // One literal source result handler shared by the ordinary live append and
    // the explicit retained actual-result writer. It does not run native IO.
    pub(crate) fn native_main_rows_deliver_native_result(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainRowContinuationV1,
        result: &Result<
            crate::source_compact_index_v2::StreamedValidityEvaluation,
            crate::source_compact_index_v2::CompactIndexError,
        >,
    ) -> NativeMainRowStopV1 {
        let Some(row) = frame.current.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Some(error) = &row.ambiguous {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        if row.write.is_none() {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        match result {
            // The source await still requires its distinct durable resolution.
            Ok(_) => return NativeMainRowStopV1::AwaitNativeWrite,
            Err(error) => row.ambiguous = Some(error.to_string()),
        }
        self.native_main_rows_advance(index, frame)
    }
    pub(crate) fn native_main_service_deliver_actual_validity_result(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        actual: &crate::source_compact_index_v2::ActualValidityAppendAttemptV1,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            if frame.terminal.is_some() || frame.inline_pending_write().is_some() {
                return Err(SourceRuleError::InvalidInput);
            }
            let row = if let Some(block) = frame.block.as_mut() {
                block.row.as_mut()
            } else {
                frame.catchup.row.as_mut()
            }
            .ok_or(SourceRuleError::InvalidInput)?;
            if row
                .current
                .as_ref()
                .is_none_or(|current| current.write.is_none() || current.ambiguous.is_some())
            {
                return Err(SourceRuleError::InvalidInput);
            }
            Ok(native_main_service_row_stop(
                runtime.native_main_rows_deliver_native_result(index, row, actual.actual_result()),
            ))
        })
        .and_then(|result| result)
    }
}
