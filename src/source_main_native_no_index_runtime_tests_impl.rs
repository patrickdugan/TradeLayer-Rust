#[cfg(test)]
impl SourceContractRuntime {
    pub(crate) fn native_main_test_row_decode_count(&self) -> usize {
        NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.get())
    }
}

#[cfg(test)]
pub(crate) fn native_main_test_row_decode_total() -> usize {
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.get())
}
