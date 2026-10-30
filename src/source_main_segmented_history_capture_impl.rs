/// Independently retain this expectation outside the WAL before cold reopening.
/// It binds actual physical bytes/namespace and claims no actor authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedActiveHistoryPinV1 {
    wal: ActiveServiceWalPinV1,
    trace: crate::storage::JournalCommitTracePinV1,
    actions: u64,
    head: [u8; 32],
    source_head: V,
    latest_segment: Option<ActiveServiceWalBlobV1>,
}
impl SegmentedActiveHistoryPinV1 {
    pub fn wal(&self) -> &ActiveServiceWalPinV1 {
        &self.wal
    }
    pub fn trace(&self) -> &crate::storage::JournalCommitTracePinV1 {
        &self.trace
    }
    pub fn actions(&self) -> u64 {
        self.actions
    }
    pub fn head(&self) -> [u8; 32] {
        self.head
    }
    pub fn source_head(&self) -> &V {
        &self.source_head
    }
    pub fn latest_segment(&self) -> Option<&ActiveServiceWalBlobV1> {
        self.latest_segment.as_ref()
    }
}
impl ActiveServiceWalV1 {
    pub fn capture_segmented_ack_history(
        &mut self,
        actions: u64,
        head: [u8; 32],
        source_head: &V,
    ) -> Result<SegmentedActiveHistoryPinV1> {
        self.require_segment_scope()?;
        let record = self.record()?;
        if ![s("Ack"), s("SegmentAck")].contains(&record.get("stage"))
            || segment_count(&record.get("sequence"))? != actions
            || segment_hash(&record.get("head"))? != head
            || record.get("sourceHead") != *source_head
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        segment_source_head(source_head)?;
        let latest_segment = match record.get("historySegment") {
            V::Null | V::Undefined => None,
            ref v => Some(segment_blob(v)?),
        };
        let wal = self.pin()?;
        let trace = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commit_window(
                &wal.prefix,
                wal.prefix.physical_sequence,
                None,
                1,
                1,
            )?
            .after()
            .clone();
        Ok(SegmentedActiveHistoryPinV1 {
            wal,
            trace,
            actions,
            head,
            source_head: source_head.clone(),
            latest_segment,
        })
    }
}
