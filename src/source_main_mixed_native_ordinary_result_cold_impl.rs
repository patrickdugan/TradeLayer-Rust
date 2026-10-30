impl NativeMainRowContinuationV1 {
    fn mixed_known_recorded_error(&self, error: &str) -> bool {
        self.current.as_ref().is_some_and(|row| {
            row.ambiguous.as_deref() == Some(error)
                && row
                    .historical_write
                    .as_ref()
                    .is_some_and(|write| write.matches_mixed_recorded_error(error))
        })
    }
}
impl SourceContractRuntime {
    fn native_main_service_append_mixed_ordinary(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        transcript: &crate::source_main_active_service_wal::VerifiedMixedNativeWindowAccessV1<'_>,
        sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
        request: (
            &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
            &[u8],
            Option<V>,
            &mut Option<String>,
        ),
    ) -> NativeMainServiceStopV1 {
        let (capability, mutations, selection, verification) = request;
        let checked = (|| -> Result<_, String> {
            let source = frame
                .mixed_historical_validity()
                .ok_or("same source-computed ordinary await absent")?;
            if source.exact_prepared_mutations() != mutations {
                return Err("actual source ordinary TLNM differs".into());
            }
            let mut closure = {
                let index = history
                    .scoped_cold_index(capability)
                    .map_err(|e| e.to_string())?;
                transcript
                    .verify_ordinary_operation(index, sequence, source)
                    .map_err(|e| e.to_string())?
            };
            let inputs = transcript
                .ordinary_io_inputs(sequence)
                .map_err(|e| e.to_string())?;
            let first = inputs
                .first()
                .ok_or("independent actual ordinary IO absent")?;
            if selection.as_ref() != Some(first.selection()) {
                return Err("actual source ordinary selector differs".into());
            }
            let replacements = closure.replacements();
            let last_context = closure.write_context().clone();
            let before = {
                let index = history
                    .scoped_cold_index(capability)
                    .map_err(|e| e.to_string())?;
                transcript
                    .checked_original_view(index, closure.write_before())
                    .map_err(|e| e.to_string())?
            };
            let post = {
                let index = history
                    .scoped_cold_index(capability)
                    .map_err(|e| e.to_string())?;
                transcript
                    .checked_original_view(index, closure.post())
                    .map_err(|e| e.to_string())?
            };
            let first_error = closure.take_first_error();
            Ok((replacements, last_context, before, post, first_error))
        })();
        let (replacements, last_context, before, post, first_error) = match checked {
            Ok(v) => v,
            Err(error) => {
                *verification = Some(error.clone());
                return NativeMainServiceStopV1::StorageStopped(error);
            }
        };
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            let row = if let Some(block) = &mut frame.block {
                block.row.as_mut()
            } else {
                frame.catchup.row.as_mut()
            };
            let Some(row) = row else {
                return Err("actual ordinary row absent".to_string());
            };
            let current = row
                .current
                .as_mut()
                .ok_or("actual ordinary current row absent")?;
            if current.ambiguous.is_some() {
                return Err("ordinary native action was already attempted".into());
            }
            let awaiting = current
                .historical_write
                .as_mut()
                .ok_or("same historical ordinary await absent")?;
            awaiting
                .install_mixed_write_context(&before, &post, mutations)
                .map_err(|e| e.to_string())?;
            if awaiting.durable_read_context() != last_context {
                return Err("installed actual replacement context differs".into());
            }
            history
                .verify_recorded_native_append(awaiting, mutations)
                .map_err(|e| e.to_string())?;
            // Deliver the FIRST independently typed native result exactly
            // once. Later native-only retries never call this source handler.
            let source_error = first_error.map(|io| {
                crate::source_compact_index_v2::CompactIndexError::Storage(
                    crate::storage::StorageError::Io(io),
                )
                .to_string()
            });
            let first_stop = if let Some(error) = &source_error {
                current.ambiguous = Some(error.clone());
                native_main_service_row_stop(
                    runtime.native_main_rows_advance_historical(history, row),
                )
            } else {
                NativeMainServiceStopV1::AwaitNativeWrite
            };
            let current = row.current.as_mut().ok_or("same held source row lost")?;
            if replacements > 0 {
                // Actual supersession install cleared the source error; retry
                // IO results are held as facts and are not delivered again.
                current.ambiguous = None;
            } else if let Some(error) = source_error {
                current
                    .historical_write
                    .as_mut()
                    .ok_or("same held await lost")?
                    .retain_mixed_recorded_error(error)
                    .map_err(|e| e.to_string())?;
            }
            Ok(first_stop)
        }) {
            Ok(Ok(stop)) => stop,
            Ok(Err(error)) => {
                *verification = Some(error.clone());
                NativeMainServiceStopV1::StorageStopped(error)
            }
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
}
