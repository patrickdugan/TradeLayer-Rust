impl SourceMainNativeMixedFactServiceV1 {
    /// Actual current writer lock, ACK/source head and complete native Post are
    /// captured together. This is metadata evidence, never a financial export.
    pub fn capture_cold_history_pin(
        &mut self,
    ) -> ActiveWalResult<crate::source_main_active_service_wal::MixedNativeAckHistoryPinV1> {
        self.require_action_route()?;
        let actual = self.actual_mut();
        if actual.fact_barrier_pending()
            || actual.deferred_error.is_some()
            || actual.inner.phase != NativeMainActiveWalPhaseV1::Idle
            || actual.inner.pending.is_some()
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let base = &mut actual.inner;
        let actions = base.archived_active_actions() + base.history.len() as u64;
        let source_head = base.actual_active_source_head();
        let index = match base
            .owner
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
        {
            ActiveWalOwner::Ready(owner) => &owner.index,
            ActiveWalOwner::Active(owner) => &owner.owner.index,
        };
        Ok(base
            .wal
            .capture_mixed_native_ack_history(actions, base.head, &source_head, index)?)
    }
}
