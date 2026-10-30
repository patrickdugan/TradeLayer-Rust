// Explicit mixed-profile actual original/copy view admission. One unchanged
// raw-foundation financial reconstruction body owns the financial Replay.
struct NativeMainMixedFactHistoryProviderV1<'a> {
    access: crate::source_main_active_service_wal::VerifiedMixedNativeWindowAccessV1<'a>,
    capability: &'a crate::source_compact_index_v2::NativeColdIndexExecutionV1,
}
impl NativeMainActualHistoryProviderV1 for NativeMainMixedFactHistoryProviderV1<'_> {
    fn read(
        &self,
        index: &mut SourceCompactIndex,
        pin: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    ) -> Result<
        crate::source_compact_index_v2::MainIndexReadViewV1,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        self.access
            .checked_original_view(index, pin)
            .map(crate::source_compact_index_v2::MainIndexReadViewV1::from_verified)
            .map_err(|_| crate::source_compact_index_v2::CompactIndexError::WrongContext)
    }
    fn playback<'a>(
        &self,
        index: &'a mut SourceCompactIndex,
        pins: &[crate::source_compact_index_v2::SourceIndexHistoryPinV1],
    ) -> Result<
        crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'a>,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        let views = pins
            .iter()
            .map(|pin| self.read(index, pin))
            .collect::<Result<Vec<_>, _>>()?;
        crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1::open_verified_views(
            index,
            self.capability,
            pins,
            views,
        )
    }
}
impl NativeMainColdPrefixV1 {
    /// Separate exact mixed-reader admission. Old c05/original-file APIs keep
    /// their original provider/physical binding and refusal behavior.
    pub fn reexecute_with_mixed_fact_views(
        bytes: &[u8],
        expected: &NativeMainColdPrefixExpectationV1,
        mut index: SourceCompactIndex,
        transcript: &mut crate::source_main_active_service_wal::VerifiedMixedNativeHistoryV1,
    ) -> Result<Self, Box<RejectedNativeMainColdPrefixV1>> {
        let capability = match index.begin_verified_cold_execution(&expected.actual_post) {
            Ok(value) => value,
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
        let bound=transcript.origin_archive()==bytes
            && transcript.pin().native_post()==&expected.actual_post
            && crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(index.native_namespace(),transcript.origin_history()).is_ok_and(|origin|origin==expected.completed.current);
        let window = if bound {
            transcript.read_window(0).ok()
        } else {
            None
        };
        let access = window
            .as_ref()
            .and_then(|window| transcript.window_access(window).ok());
        let result = if let Some(access) = access {
            SourceMainNativeSessionV13::rebuild_historical_rows_prefix_with_provider(
                bytes,
                &expected.completed,
                &expected.actual_post,
                index,
                &NativeMainMixedFactHistoryProviderV1 {
                    access,
                    capability: &capability,
                },
            )
        } else {
            Err(Box::new(RejectedNativeMainHistoricalRowsV1 {
                error: NativeMainSessionError::WrongPhysicalBinding,
                index,
                financial: None,
                actual_prefix: vec![],
            }))
        };
        match result {
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
}
