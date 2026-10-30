impl NativeMainServiceContinuationV1 {
    pub(crate) fn actual_inline_fact_descriptor(&self) -> Option<V> {
        self.inline_pending_write()?;
        self.native_main_retained_consensus_descriptor(0)
            .map(|descriptor| descriptor.value())
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_service_append_capture_actual(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualNativeFactBindingV1,
    ) -> Result<
        (
            NativeMainServiceStopV1,
            crate::source_compact_index_v2::ActualInlineAppendAttemptV1,
        ),
        SourceRuleError,
    > {
        self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            if frame.terminal.is_some() || runtime.core.native_main_binding.is_none() {
                return Err(SourceRuleError::InvalidInput);
            }
            let inline = frame
                .block
                .as_mut()
                .and_then(|b| b.inline_index.as_mut())
                .ok_or(SourceRuleError::InvalidInput)?;
            if inline.attempted || inline.storage_stop.is_some() || inline.local_stop.is_some() {
                return Err(SourceRuleError::InvalidInput);
            }
            let write = inline.write.as_ref().ok_or(SourceRuleError::InvalidInput)?;
            // The callback runs after all producer/Intent guards and directly
            // before the single existing native append body. No source resume.
            let attempted = &mut inline.attempted;
            let actual = index
                .append_inline_database_capture_actual_fact_before(
                    write,
                    permit,
                    wal,
                    binding,
                    || *attempted = true,
                )
                .map_err(|_| SourceRuleError::InvalidInput)?;
            let stop = match actual.actual_result() {
                Ok(()) => NativeMainServiceStopV1::AwaitNativeWrite,
                Err(error) => {
                    let error = error.to_string();
                    inline.storage_stop = Some(error.clone());
                    NativeMainServiceStopV1::StorageStopped(error)
                }
            };
            // The same actual Main binding was checked before IO. No post-IO
            // guard is allowed to drop this owned result/evidence. Effects and
            // the source invocation are never drained or finished here.
            Ok((stop, actual))
        })
        .and_then(|result| result)
    }
}
