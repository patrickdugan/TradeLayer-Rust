impl SourceMainNativeSegmentedActiveServiceV1 {
    /// Capture while holding the actual existing writer, at an acknowledged
    /// stable boundary. The returned pin is metadata expectation, not a permit.
    pub fn capture_acknowledged_history(
        &mut self,
    ) -> ActiveWalResult<crate::source_main_active_service_wal::SegmentedActiveHistoryPinV1> {
        self.require_action_route()?;
        let c = &mut self.controller;
        if c.phase != NativeMainActiveWalPhaseV1::Idle || c.pending.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let actions = c.archived_active_actions() + c.history.len() as u64;
        let source = c.actual_active_source_head();
        Ok(c.wal
            .capture_segmented_ack_history(actions, c.head, &source)?)
    }
}
