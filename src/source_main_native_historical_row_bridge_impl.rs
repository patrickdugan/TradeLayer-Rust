enum NativeMainPreparedRowValidityV1 {
    Complete(Box<StreamedValidityEvaluation>),
    Live(Box<NativePreparedValidityWriteV1>),
    Historical(Box<crate::source_compact_index_v2::NativeHistoricalValidityAwaitV1>),
}
enum NativeMainPrepareRowFailureV1 {
    Local(SourceRuleError),
    Metadata(crate::source_compact_index_v2::CompactIndexError),
}
// Original live query/update hook order, using the original actual opaque plan.
fn native_main_prepare_live_row_validity(
    core: &mut ContractCore,
    index: &SourceCompactIndex,
    row: &NativeMainActualRowV1,
) -> Result<NativeMainPreparedRowValidityV1, NativeMainPrepareRowFailureV1> {
    let query = index
        .snapshot()
        .and_then(|pin| {
            index.prepare_validity_read(
                &pin,
                row.id.clone(),
                row.kind.clone(),
                row.params.get("valid"),
                row.params.get("reason"),
            )
        })
        .map_err(NativeMainPrepareRowFailureV1::Metadata)?;
    let selected = match query {
        NativePreparedValidityQueryV1::Complete(value) => {
            return Ok(NativeMainPreparedRowValidityV1::Complete(value))
        }
        NativePreparedValidityQueryV1::Selected(value) => value,
    };
    let prepared = match core.check_failure("txIndex", "updateAsync") {
        Ok(()) => index
            .snapshot()
            .and_then(|pin| selected.prepare_write(index, &pin)),
        Err(Fault::Thrown(error)) => selected
            .source_rejected(index, error)
            .map(|value| NativePreparedValidityOperationV1::Complete(Box::new(value))),
        Err(Fault::Local(error)) => return Err(NativeMainPrepareRowFailureV1::Local(error)),
    }
    .map_err(NativeMainPrepareRowFailureV1::Metadata)?;
    Ok(match prepared {
        NativePreparedValidityOperationV1::Complete(value) => {
            NativeMainPreparedRowValidityV1::Complete(value)
        }
        NativePreparedValidityOperationV1::Write(write) => {
            NativeMainPreparedRowValidityV1::Live(write)
        }
    })
}
impl SourceContractRuntime {
    pub(crate) fn native_main_rows_advance_historical(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        frame: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        self.native_main_rows_advance_with_preparer(frame,&mut |core,row| {
            let mut local_stop=None;
            let prepared=history.prepare_source_update(row.id.clone(),row.kind.clone(),row.params.get("valid"),row.params.get("reason"),&mut |_|match core.check_failure("txIndex","updateAsync") {
                Ok(())=>Ok(None),Err(Fault::Thrown(error))=>Ok(Some(DependencyReturn::PendingThrow(error))),Err(Fault::Local(error))=>{local_stop=Some(error.clone());Err(error.into())},
            });
            if let Some(error)=local_stop {return Err(NativeMainPrepareRowFailureV1::Local(error))}
            Ok(match prepared.map_err(NativeMainPrepareRowFailureV1::Metadata)? {
                crate::source_compact_index_v2::NativeHistoricalValidityOperationV1::Complete(value)=>NativeMainPreparedRowValidityV1::Complete(value),
                crate::source_compact_index_v2::NativeHistoricalValidityOperationV1::Await(value)=>NativeMainPreparedRowValidityV1::Historical(value),
            })
        })
    }
    pub(crate) fn native_main_rows_verify_historical_append(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        frame: &mut NativeMainRowContinuationV1,
        expected_mutations: &[u8],
    ) -> NativeMainRowStopV1 {
        let Some(row) = frame.current.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Some(error) = &row.ambiguous {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        let Some(write) = row.historical_write.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        match history.verify_recorded_native_append(write, expected_mutations) {
            Ok(()) => NativeMainRowStopV1::AwaitNativeWrite,
            Err(error) => {
                row.ambiguous = Some(error.to_string());
                NativeMainRowStopV1::StorageStopped(error.to_string())
            }
        }
    }
    pub(crate) fn native_main_rows_resolve_historical_post(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        frame: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        let Some(row) = frame.current.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Some(error) = &row.ambiguous {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        let Some(write) = row.historical_write.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        match history.resolve_recorded_native_post(write) {
            Ok(evaluation) => {
                self.native_main_row_validity_result(frame, evaluation);
                self.native_main_rows_advance_historical(history, frame)
            }
            Err(error) => {
                frame.current.as_mut().expect("same actual row").ambiguous =
                    Some(error.to_string());
                NativeMainRowStopV1::StorageStopped(error.to_string())
            }
        }
    }
}
