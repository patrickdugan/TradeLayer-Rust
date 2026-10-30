impl StartupPromiseQueue {
    pub(crate) fn actual_delivery(&self) -> Option<&Delivery> {
        self.active.as_ref()
    }
    pub(crate) fn visit_promise_key(
        &self,
        path: &str,
        key: PromiseKey,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        self.position(key)?;
        startup_source_key_visit(
            path,
            key.owner,
            key.generation,
            "creation",
            key.creation,
            visitor,
        )
    }
    pub(crate) fn visit_payload_key(
        &self,
        path: &str,
        key: ActualPayloadRef,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        self.payload_position(key)?;
        startup_source_key_visit(path, key.owner, key.generation, "slot", key.slot, visitor)
    }
    pub(crate) fn visit_source_job_key(
        &self,
        path: &str,
        key: SourceJobKey,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        self.source_job_position(key)?;
        startup_source_key_visit(
            path,
            key.owner,
            key.generation,
            "serial",
            key.serial,
            visitor,
        )
    }
    pub(crate) fn visit_continuation_key(
        &self,
        path: &str,
        key: SourceContinuationKey,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        self.source_continuation_binding(key)?;
        startup_source_key_visit(
            path,
            key.owner,
            key.generation,
            "serial",
            key.serial,
            visitor,
        )
    }
}
fn startup_source_key_visit(
    path: &str,
    owner: u64,
    generation: u64,
    name: &str,
    ordinal: u64,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    for (field, value) in [
        ("owner", owner),
        ("generation", generation),
        (name, ordinal),
    ] {
        visitor(&format!("{path}/{field}"), &V::String(value.to_string()))?;
    }
    Ok(())
}
