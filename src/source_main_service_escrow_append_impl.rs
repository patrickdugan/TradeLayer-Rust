impl SourceMainNativeActiveServiceV1 {
    fn escrow_append_preflight(
        &self,
        expected: &NativeMainActiveServiceExpectationV1,
    ) -> Result<(), NativeMainActiveServiceErrorV1> {
        if self.expectation()? != *expected {
            return Err(NativeMainActiveServiceErrorV1::WrongExpectation);
        }
        if self.history.len() >= MAX_SERVICE_ACTIONS {
            return Err(NativeMainActiveServiceErrorV1::Capacity);
        }
        let frame = self
            .frame
            .as_ref()
            .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
        if let Some(code) = frame.terminal_code() {
            return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code));
        }
        let write = frame
            .pending_write()
            .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
        if self.attempted_write.as_ref() == Some(&write.durable_read_context()) {
            return Err(NativeMainActiveServiceErrorV1::WriteAlreadyAttempted);
        }
        Ok(())
    }
    fn apply_escrow_append(
        &mut self,
        expected: &NativeMainActiveServiceExpectationV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        self.escrow_append_preflight(expected)?;
        self.history
            .try_reserve(1)
            .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
        let frame = self.frame.as_mut().expect("checked real frame");
        self.attempted_write = Some(
            frame
                .pending_write()
                .expect("checked real write")
                .durable_read_context(),
        );
        let stop = self
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_append_with_escrow(&mut self.owner.index, frame, permit);
        if frame
            .pending_write()
            .map(|write| write.durable_read_context())
            .as_ref()
            != self.attempted_write.as_ref()
        {
            self.attempted_write = None;
        }
        self.pending_observation = Some(NativeMainActiveServicePendingV1 {
            action: NativeMainActiveServiceActionV1::AppendPending,
            result: NativeMainActiveServiceActionResultV1::Stop(stop),
            before: expected.physical.clone(),
        });
        self.retry_observation()
    }
}
