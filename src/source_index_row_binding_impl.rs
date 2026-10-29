impl SourceCompactIndex {
    pub fn native_observed_append_binding(
        &self,
    ) -> Result<crate::source_main_native_intent_store::NativeObservedAppendBindingV1> {
        crate::source_main_native_intent_store::NativeObservedAppendBindingV1::observe(
            &self.journal,
        )
        .map_err(|_| CompactIndexError::Source(SourceRuleError::Backend))
    }
}
