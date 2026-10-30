// Test-only read access to actual completed Kernel observations. No source
// result, frame constructor, mutable owner or production visibility is added.
impl NativeMainServiceContinuationV1 {
    pub(crate) fn automatic_file_test_completed(
        &self,
    ) -> impl Iterator<Item = &(u64, crate::source_main_step_kernel::MainStepSourceResult)> {
        self.events.iter().flat_map(|event| &event.completed)
    }
}
