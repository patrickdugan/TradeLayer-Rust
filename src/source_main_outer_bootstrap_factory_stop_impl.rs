// Checked after EACH real factory, including the unawaited child factory.
impl SourceContractRuntime {
    fn native_main_outer_bootstrap_capture_factory(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        name: &'static str,
        site: NativeMainOuterBootstrapSiteV1,
    ) -> Result<(), SourceRuleError> {
        let actual = self.core.database(name);
        frame.retain_factory(actual, site)?;
        // Retain the actual factory result first. A recorder/resource failure
        // must prevent the NEXT source operation, not become a source throw.
        if self.core.effects_exhausted {
            return Err(frame.stop(SourceRuleError::Capacity));
        }
        Ok(())
    }
}
