#[cfg(test)]
impl SourceContractRuntime {
    pub(crate) fn native_main_completed_test_exhaust_effects(&mut self) {
        for _ in 0..65_537 {
            self.core.emit(V::Undefined);
        }
    }
}
