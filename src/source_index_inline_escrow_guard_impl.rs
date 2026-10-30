// Temporary safety prerequisite: successful inline cross-publication source
// operations still require a retained operation/await seam. Refusal is not parity.
pub(crate) struct SourceIndexInlineEscrowScopeV1 {
    token: u64,
}
impl SourceCompactIndex {
    pub(crate) fn begin_inline_escrow_guard(&mut self) -> Result<SourceIndexInlineEscrowScopeV1> {
        self.require_live_metadata_write()?;
        if self.inline_escrow_guard.is_some() {
            return Err(CompactIndexError::WrongContext);
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let token = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map_err(|_| CompactIndexError::Limit)?;
        self.inline_escrow_guard = Some(token);
        Ok(SourceIndexInlineEscrowScopeV1 { token })
    }
    pub(crate) fn finish_inline_escrow_guard(
        &mut self,
        scope: SourceIndexInlineEscrowScopeV1,
    ) -> Result<()> {
        if self.inline_escrow_guard != Some(scope.token) {
            return Err(CompactIndexError::WrongContext);
        }
        self.inline_escrow_guard = None;
        Ok(())
    }
    pub(crate) fn inline_escrow_scope_active(&self) -> bool {
        self.inline_escrow_guard.is_some()
    }
    fn commit_inline_metadata(&mut self, mutations: Vec<Mutation>) -> Result<u64> {
        self.require_unretained_metadata_write()?;
        if self.inline_escrow_guard.is_none() {
            return Ok(self.journal.commit(mutations)?);
        }
        self.require_live_metadata_write()?;
        let plan = self.journal.prepare_native_append(mutations)?;
        if plan.intermediate().is_some() {
            return Err(CompactIndexError::Source(
                SourceRuleError::UnknownDependency,
            ));
        }
        Ok(self
            .journal
            .commit_prepared_native_append(&plan)?
            .logical_sequence)
    }
}
