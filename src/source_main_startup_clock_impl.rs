// A source clock is metadata, not financial state or a replay/head capability.
// Construction is private to a fully quiescent actual startup prefix.
pub(crate) struct MainStartupClockHandoffV1 {
    logical_time: u64,
    next_timer: u64,
}
impl MainStartupClockHandoffV1 {
    pub(crate) fn into_parts(self) -> (u64, u64) {
        (self.logical_time, self.next_timer)
    }
}
impl MainStartupPrefixV1 {
    pub(crate) fn closed_source_clock(&self) -> Result<MainStartupClockHandoffV1, SourceRuleError> {
        if self.main != MainState::SuffixReady
            || self.database != DbState::Complete
            || !self.ready.is_empty()
            || !self.timers.is_empty()
            || self.automatic_token.is_some()
            || self.global_token.is_some()
            || self.notification_pending
            || self.stopped.is_some()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(MainStartupClockHandoffV1 {
            logical_time: self.logical_time,
            next_timer: self.next_timer,
        })
    }
}
