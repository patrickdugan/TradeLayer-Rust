impl VerifiedActualSourceFactTraceV1 {
    pub fn original_observation_bytes(&self) -> Result<Vec<u8>> {
        if let Some(raw) = &self.paired_origin_observation {
            // Created has no child Intent/blob publication. These bytes
            // came from the actual outer store and were bound to the
            // immutable child origin. They remain comparisons only.
            if ActiveServiceWalBlobV1::of(raw)? != history_blob(&self.origin().get("observation"))?
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            return Ok(raw.clone());
        }
        self.read_reference(&history_blob(&self.origin().get("observation"))?)
    }
    pub fn decoded_action(
        &self,
        entry: &VerifiedActiveServiceAckEntryV1,
    ) -> Result<crate::source_main_native_session::NativeMainActiveServiceActionV1> {
        <Self as InlinePhysicalHistoryAccessV1>::decoded_action(self, entry)
    }
    pub fn source_administrative_refs(
        &self,
        sequence: u64,
    ) -> Result<(
        Option<ActiveServiceWalBlobV1>,
        Option<ActiveServiceWalBlobV1>,
    )> {
        if sequence == 0 || usize::try_from(sequence).map_or(true, |n| n > self.entries().len()) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let mut recovery = None;
        let mut escrow = None;
        for (_, record) in self
            .records
            .iter()
            .filter(|(_, r)| r.get("sequence") == s(sequence.to_string()))
        {
            if matches!(history_text(&record.get("stage"))?, "Accepted" | "Ack") {
                break;
            }
            let value = record.get("nativeRecovery");
            if value != V::Undefined {
                recovery = Some(history_blob(&value)?);
            }
            let value = record.get("generationEscrow");
            if value != V::Undefined {
                escrow = Some(history_blob(&value)?);
            }
        }
        Ok((recovery, escrow))
    }
}
