// Metadata-plan installation only, after the controller's new durable intent.
// The original source row PC/params/skip and financial owner never reexecute.
impl SourceContractRuntime {
    pub(crate) fn native_main_row_install_supersession(
        &mut self,
        frame: &mut NativeMainRowContinuationV1,
        replacement: Box<NativePreparedValidityWriteV1>,
        mapping: &crate::source_compact_index_v2::NativePreparedValiditySupersessionV1,
    ) -> Result<(), (SourceRuleError, Box<NativePreparedValidityWriteV1>)> {
        let Some(row) = frame.current.as_mut() else {
            return Err((SourceRuleError::InvalidInput, replacement));
        };
        let Some(original) = row.write.as_ref() else {
            return Err((SourceRuleError::InvalidInput, replacement));
        };
        if !matches!(
            row.pc,
            NativeMainRowPcV1::PrimaryValidity | NativeMainRowPcV1::FatalValidity
        ) || frame.result.is_some()
            || original.query() != replacement.query()
            || original.update_arguments() != replacement.update_arguments()
            || original.pinned_read() != replacement.pinned_read()
            || original.expected_before_state() != replacement.expected_before_state()
            || original.expected_after_state() != replacement.expected_after_state()
            || original.append_plan().mutations() != replacement.append_plan().mutations()
            || original.append_plan().catalog_context()
                != replacement.append_plan().catalog_context()
            || mapping.original_before() != original.append_plan().before()
            || mapping.original_after() != original.append_plan().after()
            || mapping.replacement_before() != replacement.append_plan().before()
            || mapping.replacement_after() != replacement.append_plan().after()
            || mapping.reopened() != replacement.write_expected()
        {
            return Err((SourceRuleError::InvalidInput, replacement));
        }
        row.write = Some(replacement);
        row.ambiguous = None;
        Ok(())
    }
    pub(crate) fn native_main_catchup_install_supersession(
        &mut self,
        frame: &mut NativeMainCatchupContinuationV1,
        replacement: Box<NativePreparedValidityWriteV1>,
        mapping: &crate::source_compact_index_v2::NativePreparedValiditySupersessionV1,
    ) -> Result<(), (SourceRuleError, Box<NativePreparedValidityWriteV1>)> {
        let Some(row) = frame.row.as_mut() else {
            return Err((SourceRuleError::InvalidInput, replacement));
        };
        self.native_main_row_install_supersession(row, replacement, mapping)
    }
}
