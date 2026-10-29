// Read-only owner/CAS validation for an actual replacement append. No mutation.
impl CompactingJournal {
    pub(crate) fn require_superseding_append_owner(
        &self,
        plan: &NativeAppendPlan,
    ) -> Result<(), StorageError> {
        self.check()?;
        if plan.owner_token != self.owner_token
            || plan.identity != self.catalog_identity
            || plan.config.target_log_bytes != self.config.target_log_bytes
            || plan.config.retained_ancestors != self.config.retained_ancestors
            || plan.origin_catalog != self.catalog.canonicalize()?
            || plan.before.publication != self.publication
            || plan.before.logical_sequence != self.sequence()?
        {
            return Err(bad("foreign or stale superseding append plan"));
        }
        self.inner.validate_commit(&plan.mutations)
    }
}
