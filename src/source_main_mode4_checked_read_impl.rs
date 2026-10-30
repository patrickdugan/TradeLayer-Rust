impl VerifiedInlineActiveServiceAckHistoryV1 {
    pub(crate) fn original_observation_bytes(&self) -> Result<Vec<u8>> {
        self.read_reference(&history_blob(&self.origin().get("observation"))?)
    }
    pub(crate) fn entry_has_native_recovery(
        &self,
        entry: &VerifiedActiveServiceAckEntryV1,
    ) -> Result<bool> {
        let value = self.entry_value(entry)?.get("nativeRecovery");
        Ok(value != V::Null && value != V::Undefined)
    }
    pub fn checked_metadata_post(&self) -> Result<SourceIndexHistoryPinV1> {
        let last = self
            .entries()
            .last()
            .ok_or(ActiveServiceWalErrorV1::WrongContext)?;
        self.mode4_pin(last.verified_history_post())
    }
    // Returns an actually opened/verified immutable view, not a pin-to-permit
    // conversion. The actual final owner and its complete prefix remain bound.
    pub fn checked_original_view(
        &self,
        index: &mut SourceCompactIndex,
        pin: &SourceIndexHistoryPinV1,
    ) -> Result<VerifiedOriginalMetadataViewV1> {
        if index
            .capture_verified_history_pin()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?
            != self.checked_metadata_post()?
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        self.mode4_view(index, pin, &self.mode4_all_escrows()?)
    }
}
