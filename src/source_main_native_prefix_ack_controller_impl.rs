// Retain the actual sealed ACK before control IO; source stays unresolved.
impl SourceMainNativeRowSessionV1 {
    pub fn retained_prefix_ack(&self) -> Option<&NativePreparedPrefixAckV1> {
        self.prefix_ack.as_ref()
    }
    pub fn prepare_prefix_ack(&mut self) -> NativeMainRowStopV1 {
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::NativeIntent
                | NativeMainRowControllerPcV1::NativeAppendAttempted
                | NativeMainRowControllerPcV1::NativePost
        ) {
            return self.advance();
        }
        let Some(write) = self.pending_write() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let prepared =
            match self
                .store
                .prepare_native_prefix_ack(&self.permit, &self.owner.index, write)
            {
                Ok(prepared) => prepared,
                Err(error) => return NativeMainRowStopV1::StorageStopped(error.to_string()),
            };
        self.prefix_ack = Some(prepared);
        self.pc = NativeMainRowControllerPcV1::PrefixAckPrepared;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    pub fn sync_prepared_prefix_ack(&mut self) -> NativeMainRowStopV1 {
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::PrefixAckPrepared
                | NativeMainRowControllerPcV1::PrefixAckAttempted
        ) {
            return self.advance();
        }
        let prepared = self.prefix_ack.as_ref().expect("retained actual ACK");
        if matches!(self.pc, NativeMainRowControllerPcV1::PrefixAckAttempted)
            && self.store.pin().ok().as_ref() != Some(prepared.independently_expected_before())
        {
            return NativeMainRowStopV1::StorageStopped(
                "control ACK ambiguity requires exact synced-Post reopen; no blind retry".into(),
            );
        }
        self.pc = NativeMainRowControllerPcV1::PrefixAckAttempted;
        if let Err(error) = self
            .store
            .sync_prepared_native_prefix_ack(&mut self.permit, prepared)
        {
            return NativeMainRowStopV1::StorageStopped(error.to_string());
        }
        self.native_main_resume_acknowledged_source()
    }
    fn native_main_resolve_prepared_prefix_ack(&mut self) -> NativeMainRowStopV1 {
        if matches!(self.pc, NativeMainRowControllerPcV1::PrefixAckSynced) {
            return self.native_main_resume_acknowledged_source();
        }
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::PrefixAckPrepared
                | NativeMainRowControllerPcV1::PrefixAckAttempted
        ) {
            let stop = self.prepare_prefix_ack();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        self.sync_prepared_prefix_ack()
    }
    fn native_main_resume_acknowledged_source(&mut self) -> NativeMainRowStopV1 {
        self.superseding_active = false;
        self.pc = NativeMainRowControllerPcV1::Source;
        let stop = self
            .frame
            .as_mut()
            .expect("actual source frame")
            .resolve_post(
                self.owner.financial.native_main_runtime_mut(),
                &mut self.owner.index,
            );
        if stop == NativeMainRowStopV1::Complete {
            self.advance()
        } else {
            stop
        }
    }
}
fn native_main_prefix_ack_fields(
    prepared: Option<&NativePreparedPrefixAckV1>,
    recovered: Option<&NativeIntentStorePinV1>,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor("prefixAck/present", &V::Bool(prepared.is_some()))?;
    if let Some(prepared) = prepared {
        visitor("prefixAck/profile", &s(prepared.profile()))?;
        native_main_supersession_store_pin(
            "prefixAck/before",
            prepared.independently_expected_before(),
            visitor,
        )?;
        native_main_supersession_store_pin(
            "prefixAck/after",
            prepared.independently_expected_after(),
            visitor,
        )?;
        for (name, blob) in [
            ("nativeIntent", prepared.original_native_intent()),
            ("prefix", prepared.acknowledged_prefix()),
        ] {
            visitor(
                &format!("prefixAck/{name}/bytes"),
                &s(&blob.bytes.to_string()),
            )?;
            visitor(&format!("prefixAck/{name}/sha256"), &s(&hex(&blob.sha256)))?;
        }
    }
    visitor("prefixAck/recovered", &V::Bool(recovered.is_some()))?;
    if let Some(pin) = recovered {
        native_main_supersession_store_pin("prefixAck/recoveredPost", pin, visitor)?;
    }
    Ok(())
}
