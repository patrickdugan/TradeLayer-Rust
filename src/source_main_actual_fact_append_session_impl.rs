struct ActualInlineServiceAppendCaptureV1 {
    selection: V,
    attempt: crate::source_compact_index_v2::ActualInlineAppendAttemptV1,
}
impl SourceMainNativeActiveServiceV1 {
    fn actual_inline_fact_selection(&self) -> Option<V> {
        let pending = self.frame.as_ref()?.actual_inline_fact_descriptor()?;
        Some(obj(&[
            ("profile", s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1")),
            ("sourceProfile", s(self.profile())),
            (
                "originCommands",
                s(&self.origin.session.commands.to_string()),
            ),
            ("originHead", s(&hex(&self.origin.session.commitment))),
            ("originPhysical", physical_value(&self.origin.physical)),
            ("serviceActions", s(&self.head.actions.to_string())),
            ("serviceHead", s(&hex(&self.head.commitment))),
            ("pending", pending),
        ]))
    }
    // New explicit fact-bound API; old apply/apply_escrow_append are unchanged.
    // Crucially this DOES NOT capture/retry source observation. The active WAL
    // must sync NativeIoObserved while this same source result remains pending.
    fn apply_actual_inline_capture(
        &mut self,
        expected: &NativeMainActiveServiceExpectationV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualNativeFactBindingV1,
    ) -> Result<ActualInlineServiceAppendCaptureV1, NativeMainActiveServiceErrorV1> {
        self.escrow_append_preflight(expected)?;
        let selection = self
            .actual_inline_fact_selection()
            .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
        if wal
            .record()
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?
            .get("actualSourceSelection")
            != selection
        {
            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
        }
        self.history
            .try_reserve(1)
            .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
        let frame = self.frame.as_mut().expect("preflighted same source frame");
        let (stop, attempt) = self
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_append_capture_actual(
                &mut self.owner.index,
                frame,
                permit,
                wal,
                binding,
            )
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        self.attempted_write = frame.pending_native_context();
        self.pending_observation = Some(NativeMainActiveServicePendingV1 {
            action: NativeMainActiveServiceActionV1::AppendPending,
            result: NativeMainActiveServiceActionResultV1::Stop(stop),
            before: expected.physical.clone(),
        });
        Ok(ActualInlineServiceAppendCaptureV1 { selection, attempt })
    }
}
