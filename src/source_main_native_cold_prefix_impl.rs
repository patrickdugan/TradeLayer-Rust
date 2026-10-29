/// Explicit cold reconstruction only. The historical completed endpoint and
/// the actual final disk owner are separate independently checked expectations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainColdPrefixExpectationV1 {
    pub completed: NativeMainHistoricalRowsExpectationV1,
    pub actual_post: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
}

// The same raw-foundation/completed-row body returns these parts to either its
// original exact-current caller or the sealed cold carrier. No financial clone.
struct NativeMainHistoricalColdPartsV1 {
    financial: SourceContractReplay,
    index: SourceCompactIndex,
    foundation: Vec<u8>,
    foundation_head: SourceContractReplayHead,
    origin_rows: RowStreamReceipt,
    binding: V,
    origin_physical: NativePhysicalBinding,
    head: NativeMainSessionHead,
    history: Vec<NativeMainSessionReceipt>,
}

fn cold_prefix_physical(
    pin: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
) -> NativePhysicalBinding {
    NativePhysicalBinding {
        persistent_identity: pin.storage.persistent_identity.clone(),
        canonical_catalog: pin.storage.canonical_catalog.clone(),
        metadata: pin.metadata.clone(),
    }
}

/// Retains ONE rebuilt financial prefix and the same actual final Post index
/// with its opaque write guard. It cannot release an ordinary stale Session,
/// replay owner, checkpoint, mutable Runtime or index.
pub struct NativeMainColdPrefixV1 {
    parts: NativeMainHistoricalColdPartsV1,
    capability: crate::source_compact_index_v2::NativeColdIndexExecutionV1,
    completed_tip: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
}

impl core::fmt::Debug for NativeMainColdPrefixV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeMainColdPrefixV1")
            .field("completed_commands", &self.parts.head.commands)
            .field("completed_tip", &self.completed_tip.metadata)
            .field("actual_post", &self.capability.original_current().metadata)
            .finish_non_exhaustive()
    }
}

pub struct RejectedNativeMainColdPrefixV1 {
    pub error: NativeMainSessionError,
    rejected: Box<RejectedNativeMainHistoricalRowsV1>,
    capability: Option<crate::source_compact_index_v2::NativeColdIndexExecutionV1>,
}

impl core::fmt::Debug for RejectedNativeMainColdPrefixV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainColdPrefixV1")
            .field("error", &self.error)
            .field("write_guard_retained", &self.capability.is_some())
            .finish_non_exhaustive()
    }
}

impl RejectedNativeMainColdPrefixV1 {
    pub fn index(&self) -> &SourceCompactIndex {
        self.rejected.index()
    }
    pub fn financial_runtime(&self) -> Option<&SourceContractRuntime> {
        self.rejected.financial_runtime()
    }
    pub fn actual_completed_prefix(&self) -> &[NativeMainSessionReceipt] {
        self.rejected.actual_prefix()
    }
    pub fn write_guard_retained(&self) -> bool {
        self.capability.is_some()
    }
}

impl NativeMainColdPrefixV1 {
    pub fn reexecute(
        bytes: &[u8],
        expected: &NativeMainColdPrefixExpectationV1,
        mut index: SourceCompactIndex,
    ) -> Result<Self, Box<RejectedNativeMainColdPrefixV1>> {
        let capability = match index.begin_verified_cold_execution(&expected.actual_post) {
            Ok(capability) => capability,
            Err(error) => {
                let error = cold_history_error(error);
                return Err(Box::new(RejectedNativeMainColdPrefixV1 {
                    error: error.clone(),
                    rejected: Box::new(RejectedNativeMainHistoricalRowsV1 {
                        error,
                        index,
                        financial: None,
                        actual_prefix: vec![],
                    }),
                    capability: None,
                }));
            }
        };
        match SourceMainNativeSessionV13::rebuild_historical_rows_prefix(
            bytes,
            &expected.completed,
            &expected.actual_post,
            index,
        ) {
            Ok(parts) => Ok(Self {
                parts,
                capability,
                completed_tip: expected.completed.current.clone(),
            }),
            Err(rejected) => Err(Box::new(RejectedNativeMainColdPrefixV1 {
                error: rejected.error.clone(),
                rejected,
                capability: Some(capability),
            })),
        }
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.parts.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.parts.index
    }
    pub fn actual_post(&self) -> &crate::source_compact_index_v2::SourceIndexHistoryPinV1 {
        self.capability.original_current()
    }
    pub fn completed_tip(&self) -> &crate::source_compact_index_v2::SourceIndexHistoryPinV1 {
        &self.completed_tip
    }
    /// Explicitly the reconstructed historical prefix, not a current hot head.
    pub fn completed_prefix_head(&self) -> &NativeMainSessionHead {
        &self.parts.head
    }
    pub fn completed_prefix(&self) -> &[NativeMainSessionReceipt] {
        &self.parts.history
    }
}
