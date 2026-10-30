impl NativeMainServiceContinuationV1 {
    pub(crate) fn actual_inline_fact_attempted(&self) -> Option<bool> {
        self.block
            .as_ref()?
            .inline_index
            .as_ref()
            .map(|frame| frame.attempted)
    }
}
