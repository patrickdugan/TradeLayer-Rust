impl SourceMainNativeDurableActiveServiceV1 {
    pub fn capture_inline_cold_pin(
        &mut self,
    ) -> ActiveWalResult<crate::source_main_active_service_wal::InlineActiveServiceColdPinV1> {
        if !self.inline_service_enabled()
            || self.phase != NativeMainActiveWalPhaseV1::Idle
            || self.pending.is_some()
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let namespace = self.index().native_namespace().clone();
        Ok(self.wal.capture_inline_cold_pin(&namespace)?)
    }
}
