/// Independently retained expected metadata. It grants no source result,
/// native append token, financial owner or authority authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedNativeAckHistoryPinV1 {
    segmented: SegmentedActiveHistoryPinV1,
    metadata_namespace: Namespace,
    native_post: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
}
impl MixedNativeAckHistoryPinV1 {
    pub fn wal(&self) -> &ActiveServiceWalPinV1 {
        self.segmented.wal()
    }
    pub fn trace(&self) -> &crate::storage::JournalCommitTracePinV1 {
        self.segmented.trace()
    }
    pub fn actions(&self) -> u64 {
        self.segmented.actions()
    }
    pub fn head(&self) -> [u8; 32] {
        self.segmented.head()
    }
    pub fn native_post(&self) -> &crate::source_compact_index_v2::SourceIndexHistoryPinV1 {
        &self.native_post
    }
}
impl ActiveServiceWalV1 {
    pub(crate) fn capture_mixed_native_ack_history(
        &mut self,
        actions: u64,
        head: [u8; 32],
        source_head: &V,
        index: &crate::source_compact_index_v2::SourceCompactIndex,
    ) -> Result<MixedNativeAckHistoryPinV1> {
        mode4_namespace(&self.namespace, index.native_namespace())?;
        let origin = self.origin_context()?.get("origin");
        if !mixed_native_fact_origin(&origin) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let record = self.record()?;
        if record.get("profile") != s(MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let native_post = index
            .capture_verified_history_pin()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let segmented = self.capture_segmented_ack_history(actions, head, source_head)?;
        Ok(MixedNativeAckHistoryPinV1 {
            segmented,
            metadata_namespace: index.native_namespace().clone(),
            native_post,
        })
    }
}
