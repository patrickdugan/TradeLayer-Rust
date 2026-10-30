impl SourceMainNativeDurableSelectedCallerV1 {
    fn prepare_selected_generation_escrow(&mut self) -> CallerSelectedResult<bool> {
        if self.pending.as_ref().is_some_and(|p| p.escrow_replacement) {
            self.install_selected_replacement()?;
        }
        let operation = self
            .pending
            .as_ref()
            .and_then(|p| p.operation.as_ref())
            .cloned()
            .ok_or(RetainedCallerSelectedWalErrorV1::WrongSelector)?;
        let owner = self
            .owner
            .as_ref()
            .ok_or(RetainedCallerSelectedWalErrorV1::NotStarted)?;
        let actual = owner.native(&operation)?;
        if actual.append().is_none_or(|p| p.intermediate().is_none()) {
            self.pending.as_mut().expect("pending").escrow_permit = None;
            return Ok(false);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.escrow_permit.is_some())
        {
            return Ok(true);
        }
        let context = actual.context();
        let index = &mut self
            .owner
            .as_mut()
            .expect("same actual owner")
            .session_mut()
            .index;
        let point = index
            .capture_verified_history_pin()
            .map_err(caller_selected_backend)?;
        let permit = index
            .retain_verified_history_generation(
                &self.escrow_root,
                std::slice::from_ref(&point),
                self.escrow_durability,
            )
            .map_err(caller_selected_backend)?;
        let mut fields =
            NativeFieldCollector::new("Native13/ActualGenerationEscrow/v1", true, 1024 * 1024);
        permit
            .independently_retained_pin()
            .visit_canonical_fields(&mut |p, v| fields.emit(p, v))
            .map_err(caller_selected_backend)?;
        let p = self.pending.as_mut().expect("pending");
        let prior = p
            .escrow_chain
            .as_ref()
            .map(|r| CallerStartBlob::of(r))
            .transpose()?;
        let chain = caller_selected_bytes(&obj(&[
            ("profile", s("TL-MAIN-SELECTED-CALLER-ESCROW-CHAIN-13-1")),
            (
                "previous",
                prior.as_ref().map_or(V::Null, CallerStartBlob::value),
            ),
            (
                "actualOriginRoute",
                self.route
                    .as_ref()
                    .expect("sealed actual route")
                    .value()
                    .clone(),
            ),
            ("selectedNativeContext", context),
            (
                "expectedEscrowFieldsHex",
                s(&hex(fields
                    .frames
                    .as_deref()
                    .expect("actual retained escrow fields"))),
            ),
        ]))?;
        p.escrow_permit = Some(permit);
        p.escrow_chain = Some(chain);
        p.escrow_prior = prior;
        Ok(true)
    }
    fn execute_selected_escrow_route(&mut self) -> CallerSelectedResult<()> {
        if self.pending.as_ref().is_some_and(|p| p.escrow_replacement) {
            self.append_replacement_native_only()
        } else {
            self.execute_once()
        }
    }
}
