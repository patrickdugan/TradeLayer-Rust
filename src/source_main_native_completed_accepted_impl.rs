impl NativeIntentStoreV1 {
    pub fn prepare_completed_accepted(
        &self,
        permit: &NativeCommandIntentPermitV1,
        accepted_bytes: &[u8],
        after: &V,
    ) -> Result<NativePreparedCompletedAcceptedV1> {
        self.check(permit)?;
        if accepted_bytes.len() > permit.receipt_budget {
            return Err(NativeIntentStoreError::Capacity);
        }
        let accepted = superseding_blob(accepted_bytes)?;
        let before_state = self.completed_mark_state(completed_command_intent_state(permit))?;
        let after_state =
            self.completed_mark_state(completed_accepted_state(permit, &accepted, after))?;
        let lineage = self.completed_lineage(permit, before_state, after_state)?;
        Ok(NativePreparedCompletedAcceptedV1 {
            lineage,
            accepted,
            raw: accepted_bytes.to_vec(),
            context: after.clone(),
        })
    }
    pub fn sync_prepared_completed_accepted(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        prepared: &NativePreparedCompletedAcceptedV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.completed_require_before(&prepared.lineage, permit)?;
        if self.write_blob(&prepared.raw)? != prepared.accepted {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        self.completed_sync_state(&prepared.lineage)?;
        permit.pin = prepared.lineage.after.clone();
        Ok(permit.pin.clone())
    }
    fn require_prepared_completed_accepted_post(
        &self,
        prepared: &NativePreparedCompletedAcceptedV1,
    ) -> Result<()> {
        self.completed_require_post(&prepared.lineage)?;
        if self.read_blob(&prepared.accepted)? != prepared.raw {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        Ok(())
    }
    pub fn open_synced_completed_accepted(
        root: &Path,
        namespace: Namespace,
        prepared: &NativePreparedCompletedAcceptedV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        let opened = Self::completed_open_post(root, namespace, &prepared.lineage, filesystem)?;
        opened.require_prepared_completed_accepted_post(prepared)?;
        Ok(opened)
    }
    pub fn recover_synced_completed_accepted(
        &self,
        prepared: &NativePreparedCompletedAcceptedV1,
        permit: &mut NativeCommandIntentPermitV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.require_prepared_completed_accepted_post(prepared)?;
        self.completed_require_retained_permit(&prepared.lineage, permit)?;
        permit.token = self.token;
        permit.pin = prepared.lineage.after.clone();
        Ok(permit.pin.clone())
    }
}
