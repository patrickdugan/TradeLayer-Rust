impl NativeIntentStoreV1 {
    pub fn prepare_completed_command_ack(
        &self,
        permit: &NativeCommandIntentPermitV1,
    ) -> Result<NativePreparedCompletedCommandAckV1> {
        self.check(permit)?;
        let state = self.state()?;
        if state.get("kind") != text("Accepted") {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let accepted = superseding_blob_reference(&state.get("accepted"))?;
        let accepted_raw = self.read_blob(&accepted)?;
        let context = state.get("after");
        let before_state =
            self.completed_mark_state(completed_accepted_state(permit, &accepted, &context))?;
        let raw = completed_command_ack_bytes(permit, &accepted, &context)?;
        let ack = superseding_blob(&raw)?;
        let after_state = self.completed_mark_state(obj(&[
            ("kind", text("Idle")),
            ("accepted", context.clone()),
            ("ack", blob_value(&ack)),
        ]))?;
        let lineage = self.completed_lineage(permit, before_state, after_state)?;
        Ok(NativePreparedCompletedCommandAckV1 {
            lineage,
            accepted,
            accepted_raw,
            ack,
            raw,
            context,
        })
    }
    pub fn sync_prepared_completed_command_ack(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        prepared: &NativePreparedCompletedCommandAckV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.completed_require_before(&prepared.lineage, permit)?;
        if self.read_blob(&prepared.accepted)? != prepared.accepted_raw
            || self.write_blob(&prepared.raw)? != prepared.ack
        {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        self.completed_sync_state(&prepared.lineage)?;
        permit.reservation = Vec::new();
        permit.pin = prepared.lineage.after.clone();
        Ok(permit.pin.clone())
    }
    fn require_prepared_completed_command_ack_post(
        &self,
        prepared: &NativePreparedCompletedCommandAckV1,
    ) -> Result<()> {
        self.completed_require_post(&prepared.lineage)?;
        if self.read_blob(&prepared.accepted)? != prepared.accepted_raw
            || self.read_blob(&prepared.ack)? != prepared.raw
        {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        Ok(())
    }
    pub fn open_synced_completed_command_ack(
        root: &Path,
        namespace: Namespace,
        prepared: &NativePreparedCompletedCommandAckV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        let opened = Self::completed_open_post(root, namespace, &prepared.lineage, filesystem)?;
        opened.require_prepared_completed_command_ack_post(prepared)?;
        Ok(opened)
    }
    pub fn recover_synced_completed_command_ack(
        &self,
        prepared: &NativePreparedCompletedCommandAckV1,
        permit: &mut NativeCommandIntentPermitV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.require_prepared_completed_command_ack_post(prepared)?;
        self.completed_require_retained_permit(&prepared.lineage, permit)?;
        // Clearing the original reservation is permitted only after exact Post.
        permit.reservation = Vec::new();
        permit.token = self.token;
        permit.pin = prepared.lineage.after.clone();
        Ok(permit.pin.clone())
    }
}
