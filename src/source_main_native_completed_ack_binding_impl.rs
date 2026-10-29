impl NativeIntentStoreV1 {
    pub(crate) fn require_actual_completed_command_ack(
        &self,
        permit: &NativeCommandIntentPermitV1,
        after: &V,
        accepted: &NativeIntentBlobV1,
    ) -> Result<()> {
        self.check(permit)?;
        if permit.pending_native.is_some() {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let state = self.state()?;
        if state.get("kind") != text("Idle") || state.get("accepted") != *after {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let ack = superseding_blob_reference(&state.get("ack"))?;
        let record = parse(&self.read_blob(&ack)?)?;
        if record.get("command") != blob_value(&permit.command)
            || record.get("nativePrefix")
                != permit.native_prefix.as_ref().map_or(V::Null, blob_value)
            || record.get("acceptedBlob") != blob_value(accepted)
            || record.get("after") != *after
            || record.get("previousAck") != permit.previous_ack
        {
            return Err(NativeIntentStoreError::WrongContext);
        }
        self.read_blob(accepted)?;
        Ok(())
    }
}
