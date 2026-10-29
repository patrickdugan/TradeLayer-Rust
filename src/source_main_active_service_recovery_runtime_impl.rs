// Metadata-only selected service-row installation; no source decoder/Logic PC.
impl SourceContractRuntime {
    pub(crate) fn native_main_service_install_supersession(
        &mut self,
        frame: &mut NativeMainServiceContinuationV1,
        replacement: Box<NativePreparedValidityWriteV1>,
        mapping: &crate::source_compact_index_v2::NativePreparedValiditySupersessionV1,
    ) -> Result<(), (SourceRuleError, Box<NativePreparedValidityWriteV1>)> {
        let row = if let Some(block) = &mut frame.block {
            block.row.as_mut()
        } else {
            frame.catchup.row.as_mut()
        };
        let Some(row) = row else {
            return Err((SourceRuleError::InvalidInput, replacement));
        };
        self.native_main_row_install_supersession(row, replacement, mapping)
    }
}
