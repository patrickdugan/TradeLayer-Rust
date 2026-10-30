pub const ACTUAL_SOURCE_INLINE_CLOSURE_PROFILE: &str =
    "TL-MAIN-ACTUAL-SOURCE-INLINE-METADATA-CLOSURE-13-1";
pub struct VerifiedActualSourceInlineClosureV1 {
    data: InlinePhysicalClosureDataV1,
    wal: ActualSourceFactColdPinV1,
}
impl VerifiedActualSourceInlineClosureV1 {
    pub fn profile(&self) -> &'static str {
        ACTUAL_SOURCE_INLINE_CLOSURE_PROFILE
    }
    pub fn sequence(&self) -> u64 {
        self.data.sequence
    }
    pub fn before(&self) -> &SourceIndexHistoryPinV1 {
        &self.data.before
    }
    pub fn post(&self) -> &SourceIndexHistoryPinV1 {
        &self.data.post
    }
    pub fn wal_pin(&self) -> &ActualSourceFactColdPinV1 {
        &self.wal
    }
}
impl InlinePhysicalHistoryAccessV1 for VerifiedActualSourceFactTraceV1 {
    fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        VerifiedActualSourceFactTraceV1::entries(self)
    }
    fn origin(&self) -> &V {
        VerifiedActualSourceFactTraceV1::origin(self)
    }
    fn origin_history(&self) -> &[u8] {
        VerifiedActualSourceFactTraceV1::origin_history(self)
    }
    fn entry_value(&self, entry: &VerifiedActiveServiceAckEntryV1) -> Result<V> {
        parse(&self.read_reference(&entry.entry)?)
    }
    fn decoded_action(
        &self,
        entry: &VerifiedActiveServiceAckEntryV1,
    ) -> Result<crate::source_main_native_session::NativeMainActiveServiceActionV1> {
        if self
            .entries()
            .get(entry.sequence.saturating_sub(1) as usize)
            != Some(entry)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(entry.raw_action()).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)
    }
    fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        VerifiedActualSourceFactTraceV1::read_reference(self, pin)
    }
    fn closure_metadata_namespace(&self) -> &Namespace {
        &self.independent.metadata_namespace
    }
    fn closure_records(&self) -> &[(u64, V)] {
        &self.records
    }
}
impl VerifiedActualSourceFactTraceV1 {
    pub fn checked_metadata_post(&self) -> Result<SourceIndexHistoryPinV1> {
        self.mode4_pin(
            self.entries()
                .last()
                .ok_or(ActiveServiceWalErrorV1::WrongContext)?
                .verified_history_post(),
        )
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
    ) -> Result<VerifiedActualSourceInlineClosureV1> {
        self.require_actual_recorded_barriers()?;
        let data = self.verify_inline_operation_data(index, sequence, source)?;
        Ok(VerifiedActualSourceInlineClosureV1 {
            data,
            wal: self.independent.clone(),
        })
    }
}
