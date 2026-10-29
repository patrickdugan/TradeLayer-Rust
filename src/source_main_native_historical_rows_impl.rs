// Production caller of the split source PC bridge. Existing completed-row
// replay drives all three phases synchronously; active action replay may park
// the SAME frame between prepare, recorded append and source resolution.
impl SourceContractRuntime {
    pub(crate) fn native_main_historical_rows(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        rows: V,
        height: V,
        live: bool,
    ) -> NativeMainRuntimeEvaluation {
        let mut frame = match self.native_main_rows_begin(rows, height, live) {
            Ok(frame) => frame,
            Err(error) => {
                return NativeMainRuntimeEvaluation {
                    result: Err(error),
                    validity: vec![],
                    consumed_rows: 0,
                    storage_stop: None,
                    grouping: None,
                }
            }
        };
        let mut storage_stop = None;
        let result = loop {
            match self.native_main_rows_advance_historical(history, &mut frame) {
                NativeMainRowStopV1::Complete => {
                    break frame.result.clone().expect("actual source finish")
                }
                NativeMainRowStopV1::SourceStopped(error) => break Err(error),
                NativeMainRowStopV1::StorageStopped(error) => {
                    storage_stop = Some(error);
                    break Err(SourceRuleError::Backend);
                }
                NativeMainRowStopV1::AwaitNativeWrite => {
                    let Some(write) = frame.pending_historical_write() else {
                        break Err(SourceRuleError::Backend);
                    };
                    let raw = write.exact_prepared_mutations().to_vec();
                    match self.native_main_rows_verify_historical_append(history, &mut frame, &raw)
                    {
                        NativeMainRowStopV1::AwaitNativeWrite => (),
                        NativeMainRowStopV1::StorageStopped(error) => {
                            storage_stop = Some(error);
                            break Err(SourceRuleError::Backend);
                        }
                        _ => break Err(SourceRuleError::Backend),
                    }
                    match self.native_main_rows_resolve_historical_post(history, &mut frame) {
                        NativeMainRowStopV1::Complete | NativeMainRowStopV1::AwaitNativeWrite => (),
                        NativeMainRowStopV1::SourceStopped(error) => break Err(error),
                        NativeMainRowStopV1::StorageStopped(error) => {
                            storage_stop = Some(error);
                            break Err(SourceRuleError::Backend);
                        }
                    }
                }
            }
        };
        NativeMainRuntimeEvaluation {
            result,
            validity: frame.validity,
            consumed_rows: frame.consumed_rows,
            storage_stop,
            grouping: None,
        }
    }
}
