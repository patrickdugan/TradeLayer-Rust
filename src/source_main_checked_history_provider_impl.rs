// One source reconstruction body; providers change only the actual immutable
// metadata view. Neither provider carries financial state or return answers.
trait NativeMainActualHistoryProviderV1 {
    fn read(
        &self,
        index: &mut SourceCompactIndex,
        pin: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    ) -> Result<
        crate::source_compact_index_v2::MainIndexReadViewV1,
        crate::source_compact_index_v2::CompactIndexError,
    >;
    fn playback<'a>(
        &self,
        index: &'a mut SourceCompactIndex,
        pins: &[crate::source_compact_index_v2::SourceIndexHistoryPinV1],
    ) -> Result<
        crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'a>,
        crate::source_compact_index_v2::CompactIndexError,
    >;
}
struct NativeMainOriginalHistoryProviderV1;
impl NativeMainActualHistoryProviderV1 for NativeMainOriginalHistoryProviderV1 {
    fn read(
        &self,
        index: &mut SourceCompactIndex,
        pin: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    ) -> Result<
        crate::source_compact_index_v2::MainIndexReadViewV1,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        index
            .read_verified_history_generation(pin)
            .map(crate::source_compact_index_v2::MainIndexReadViewV1::from_original)
    }
    fn playback<'a>(
        &self,
        index: &'a mut SourceCompactIndex,
        pins: &[crate::source_compact_index_v2::SourceIndexHistoryPinV1],
    ) -> Result<
        crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'a>,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1::open(index, pins)
    }
}
struct NativeMainMode4HistoryProviderV1<'a> {
    transcript: &'a crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
    capability: &'a crate::source_compact_index_v2::NativeColdIndexExecutionV1,
}
impl NativeMainActualHistoryProviderV1 for NativeMainMode4HistoryProviderV1<'_> {
    fn read(
        &self,
        index: &mut SourceCompactIndex,
        pin: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    ) -> Result<
        crate::source_compact_index_v2::MainIndexReadViewV1,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        self.transcript
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
    /// Separate checked Mode4 admission. Old original-file APIs retain their
    /// original provider, physical admission and refusal rules.
    pub fn reexecute_with_mode4_views(
        bytes: &[u8],
        expected: &NativeMainColdPrefixExpectationV1,
        mut index: SourceCompactIndex,
        transcript: &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
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
            && transcript.checked_metadata_post().is_ok_and(|post|post==expected.actual_post)
            && crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                index.native_namespace(),transcript.origin_history())
                .is_ok_and(|origin|origin==expected.completed.current);
        let result = if bound {
            SourceMainNativeSessionV13::rebuild_historical_rows_prefix_with_provider(
                bytes,
                &expected.completed,
                &expected.actual_post,
                index,
                &NativeMainMode4HistoryProviderV1 {
                    transcript,
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
