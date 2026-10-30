// Production Main has no source-await controller yet, so the Promise/NeDB
// domain exists only in tests. Outside them the Kernel keeps every old path.
impl MainStepKernelV1 {
    #[cfg(test)]
    fn startup_source_profile_active(&self) -> bool {
        self.startup_source_promises.is_some()
    }
    #[cfg(not(test))]
    fn startup_source_profile_active(&self) -> bool {
        false
    }
    #[cfg(not(test))]
    fn startup_visit_source_promises(
        &self,
        _visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        Ok(())
    }
}
