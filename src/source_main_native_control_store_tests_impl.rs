#[cfg(test)]
impl NativeIntentStoreV1 {
    pub(crate) fn native_main_test_control_fail_after_sync(&mut self) {
        self.journal.test_fail_after_sync();
    }
    pub(crate) fn native_main_test_control_fail_after_partial_write(&mut self) {
        self.journal.test_fail_after_partial_write();
    }
}
