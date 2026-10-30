// Literal pure setPause on the SAME financial Core while an await is held.
// It does not enter a new Main invocation, dispatch a queue, or select IO.
impl SourceContractRuntime {
    pub(crate) fn native_main_retained_caller_selected_pause(
        &mut self,
    ) -> Result<DependencyReturn, SourceRuleError> {
        returned(self.source_main_dispatch(&DependencyCall {
            target: "Main.setPause".into(),
            arguments: vec![],
        }))
        .map(|v| match v {
            DependencyReturn::Pending(v) => DependencyReturn::Immediate(v),
            other => other,
        })
    }
}
impl ContractCore {
    fn native_main_selected_inline_install(
        &mut self,
        index: &SourceCompactIndex,
        frame: NativeMainInlineIoBorrowV1<'_>,
        replacement: Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>,
        mapping: &crate::source_compact_index_v2::NativeInlineSupersessionV1,
    ) -> Result<
        (),
        (
            SourceRuleError,
            Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>,
        ),
    > {
        let checked = (|| {
            if self.native_main_binding.is_none()
                || !*frame.attempted
                || frame.storage_stop.is_none()
                || frame.local_stop.is_some()
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let original = frame
                .write
                .as_deref()
                .ok_or(SourceRuleError::InvalidInput)?;
            let call = frame
                .source
                .pending_call()
                .ok_or(SourceRuleError::InvalidInput)?;
            if call.target != format!("db.{}", replacement.method())
                || call.arguments.get(1..) != Some(replacement.arguments())
            {
                return Err(SourceRuleError::InvalidInput);
            }
            self.main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .index
                .require_inline_owner(frame.scope, frame.source)?;
            index
                .require_inline_installation(original, &replacement, mapping)
                .map_err(|_| SourceRuleError::InvalidInput)?;
            Ok(())
        })();
        if let Err(error) = checked {
            return Err((error, replacement));
        }
        *frame.write = Some(replacement);
        *frame.attempted = false;
        *frame.storage_stop = None;
        Ok(())
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_selected_caller_install_inline(
        &mut self,
        index: &SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        operation: &NativeMainRetainedConsensusPendingV1,
        replacement: Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>,
        mapping: &crate::source_compact_index_v2::NativeInlineSupersessionV1,
    ) -> Result<
        (),
        (
            SourceRuleError,
            Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>,
        ),
    > {
        if operation.kind != NativeMainRetainedConsensusOperationKindV1::CallerInline
            || caller.pending_descriptor().as_ref() != operation.caller.as_ref()
            || caller.pending_write().map(|w| w.durable_read_context()) != operation.context
        {
            return Err((SourceRuleError::InvalidInput, replacement));
        }
        if let Some(inline) = &mut caller.inline {
            return self.core.native_main_selected_inline_install(
                index,
                NativeMainInlineIoBorrowV1 {
                    scope: inline.scope,
                    source: &mut inline.source,
                    write: &mut inline.write,
                    attempted: &mut inline.attempted,
                    storage_stop: &mut inline.storage_stop,
                    local_stop: &mut inline.local_stop,
                },
                replacement,
                mapping,
            );
        }
        let Some(source) = caller.extract.as_mut().and_then(|e| e.pending_source_mut()) else {
            return Err((SourceRuleError::InvalidInput, replacement));
        };
        self.core.native_main_selected_inline_install(
            index,
            caller.extract_io.borrow(operation.method_scope, source),
            replacement,
            mapping,
        )
    }
}
