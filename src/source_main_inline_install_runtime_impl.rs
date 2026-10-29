impl SourceContractRuntime {
    pub(crate) fn native_main_service_install_inline_supersession(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainServiceContinuationV1,
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
            if self.core.native_main_binding.is_none()
                || !frame.inline_profile
                || frame.terminal.is_some()
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let inline = frame
                .block
                .as_ref()
                .and_then(|b| b.inline_index.as_ref())
                .ok_or(SourceRuleError::InvalidInput)?;
            if !inline.attempted || inline.storage_stop.is_none() {
                return Err(SourceRuleError::InvalidInput);
            }
            let original = inline
                .write
                .as_deref()
                .ok_or(SourceRuleError::InvalidInput)?;
            let call = inline
                .source
                .pending_call()
                .ok_or(SourceRuleError::InvalidInput)?;
            if call.target != format!("db.{}", replacement.method())
                || call.arguments.get(1..) != Some(replacement.arguments())
            {
                return Err(SourceRuleError::InvalidInput);
            }
            self.core
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .index
                .require_inline_owner(inline.scope, &inline.source)?;
            index
                .require_inline_installation(original, &replacement, mapping)
                .map_err(|_| SourceRuleError::InvalidInput)?;
            Ok(())
        })();
        if let Err(error) = checked {
            return Err((error, replacement));
        }
        let inline = frame
            .block
            .as_mut()
            .and_then(|b| b.inline_index.as_mut())
            .expect("validated same inline frame");
        inline.write = Some(replacement);
        inline.attempted = false;
        inline.storage_stop = None;
        Ok(())
    }
}
