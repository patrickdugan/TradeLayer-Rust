pub struct VerifiedMixedInlineClosureV1 {
    data: InlinePhysicalClosureDataV1,
}
impl VerifiedMixedInlineClosureV1 {
    pub fn post(&self) -> &SourceIndexHistoryPinV1 {
        &self.data.post
    }
}
impl InlinePhysicalHistoryAccessV1 for VerifiedMixedNativeWindowAccessV1<'_> {
    fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        self.entries()
    }
    fn origin(&self) -> &V {
        self.origin()
    }
    fn origin_history(&self) -> &[u8] {
        self.origin_history()
    }
    fn entry_value(&self, entry: &VerifiedActiveServiceAckEntryV1) -> Result<V> {
        parse(&self.read_reference(&entry.entry)?)
    }
    fn decoded_action(
        &self,
        entry: &VerifiedActiveServiceAckEntryV1,
    ) -> Result<crate::source_main_native_session::NativeMainActiveServiceActionV1> {
        if self.entry(entry.sequence)? != entry {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(entry.raw_action()).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)
    }
    fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.read_reference(pin)
    }
    fn closure_metadata_namespace(&self) -> &Namespace {
        &self.reader.mixed_expected.metadata_namespace
    }
    fn closure_records(&self) -> &[(u64, V)] {
        &self.window.records
    }
    fn closure_entry(&self, sequence: u64) -> Result<&VerifiedActiveServiceAckEntryV1> {
        self.entry(sequence)
    }
    fn closure_final_history_post(&self) -> Result<SourceIndexHistoryPinV1> {
        Ok(self.reader.mixed_expected.native_post.clone())
    }
    fn mode4_all_escrows(&self) -> Result<Vec<Mode4Escrow>> {
        self.mixed_all_escrows()
    }
}
impl VerifiedMixedNativeWindowAccessV1<'_> {
    pub fn checked_metadata_post(&self) -> Result<SourceIndexHistoryPinV1> {
        Ok(self.reader.mixed_expected.native_post.clone())
    }
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
    // Public conditional metadata proof only: actual recorded barrier/IO-chain
    // and physical closure checks grant no live permit or financial replay.
    pub fn verify_inline_operation(
        &self,
        index: &mut SourceCompactIndex,
        sequence: u64,
        source: &mut NativeHistoricalInlineDatabaseV1,
    ) -> Result<VerifiedMixedInlineClosureV1> {
        let data = self.verify_inline_operation_data(index, sequence, source)?;
        Ok(VerifiedMixedInlineClosureV1 { data })
    }
}
