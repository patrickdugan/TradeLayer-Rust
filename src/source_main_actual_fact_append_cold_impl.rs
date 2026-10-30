impl SourceContractRuntime {
    pub(crate) fn native_main_service_append_actual_facts(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
        request: (&[u8], Option<V>, &mut Option<String>),
    ) -> NativeMainServiceStopV1 {
        let (mutations, selection, verification) = request;
        if let Some(inline) = frame.block.as_mut().and_then(|b| b.inline_index.as_mut()) {
            if inline.attempted {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
            }
            let result = (|| -> Result<NativeMainServiceStopV1, String> {
                let pending = inline
                    .historical
                    .as_ref()
                    .ok_or("actual source await absent")?;
                if (pending.source().requires_native_write()
                    && pending.source().exact_prepared_mutations() != mutations)
                    || (!pending.source().requires_native_write() && !mutations.is_empty())
                {
                    return Err("source-computed TLNM mismatch".into());
                }
                let method = pending.source().method().to_owned();
                let arguments = pending.source().arguments().to_vec();
                let old = pending.source().original_read().clone();
                let mut original = {
                    let index = history
                        .scoped_cold_index(capability)
                        .map_err(|e| e.to_string())?;
                    transcript
                        .checked_original_view(index, &old)
                        .map_err(|e| e.to_string())?
                        .prepare_historical_inline_database(&method, arguments.clone())
                        .map_err(|e| e.to_string())?
                };
                let closure = {
                    let index = history
                        .scoped_cold_index(capability)
                        .map_err(|e| e.to_string())?;
                    transcript
                        .verify_inline_operation(index, sequence, &mut original)
                        .map_err(|e| e.to_string())?
                };
                let final_post = closure.post().clone();
                let inputs = transcript
                    .source_io_inputs(sequence)
                    .map_err(|e| e.to_string())?;
                if inputs.is_empty() {
                    return Err("independent actual native IO fact missing".into());
                }
                let actual_selection = selection.ok_or("actual source selector absent")?;
                let mut first = None;
                for (at, input) in inputs.iter().enumerate() {
                    let before = input.metadata().original();
                    if at > 0 {
                        let current = history.source_history_pin().clone();
                        if &current != before {
                            let view = {
                                let index = history
                                    .scoped_cold_index(capability)
                                    .map_err(|e| e.to_string())?;
                                transcript
                                    .checked_original_view(index, before)
                                    .map_err(|e| e.to_string())?
                            };
                            history
                                .install_actual_fact_reopen_view(capability, &current, view)
                                .map_err(|e| e.to_string())?;
                        }
                        let next = {
                            let index = history
                                .scoped_cold_index(capability)
                                .map_err(|e| e.to_string())?;
                            let source = transcript
                                .checked_original_view(index, before)
                                .map_err(|e| e.to_string())?
                                .prepare_historical_inline_database(&method, arguments.clone())
                                .map_err(|e| e.to_string())?;
                            transcript
                                .prepare_actual_pending_from_metadata(
                                    index,
                                    source,
                                    input.metadata(),
                                )
                                .map_err(|e| e.to_string())?
                        };
                        self.core
                            .main_production
                            .as_ref()
                            .ok_or("actual source owner absent")?
                            .index
                            .require_inline_owner(inline.scope, &inline.source)
                            .map_err(|e| e.to_string())?;
                        inline.historical = Some(Box::new(next));
                        inline.attempted = false;
                        inline.storage_stop = None;
                    }
                    let current = inline
                        .historical
                        .as_ref()
                        .ok_or("same pending metadata plan absent")?;
                    input
                        .metadata()
                        .verify_recomputed_plan(current.source())
                        .map_err(|e| e.to_string())?;
                    let mut actual = actual_selection.clone();
                    let mut descriptor = actual.get("pending");
                    descriptor
                        .insert("context", current.source().durable_read_context())
                        .map_err(|e| e.to_string())?;
                    actual
                        .insert("pending", descriptor)
                        .map_err(|e| e.to_string())?;
                    if &actual != input.selection() {
                        return Err("actual source selector does not match produced IO fact".into());
                    }
                    inline.attempted = true;
                    let outcome = input.io().observed_outcome();
                    let stop = match outcome.get("kind") {
                        V::String(kind)
                            if kind == "NativeAppendReturned" || kind == "NoNativeMutation" =>
                        {
                            inline.storage_stop = None;
                            NativeMainServiceStopV1::AwaitNativeWrite
                        }
                        V::String(kind) if kind == "NativeIoError" => {
                            let V::String(display) = outcome.get("exactSourceDisplay") else {
                                return Err("typed native display absent".into());
                            };
                            inline.storage_stop = Some(display.clone());
                            NativeMainServiceStopV1::StorageStopped(display)
                        }
                        _ => {
                            return Err(
                                "unsupported unacknowledgeable structural native refusal".into()
                            )
                        }
                    };
                    if first.is_none() {
                        first = Some(stop);
                    }
                }
                let current = inline
                    .historical
                    .as_mut()
                    .ok_or("same source plan absent")?;
                if current.source().requires_native_write() {
                    let post = {
                        let index = history
                            .scoped_cold_index(capability)
                            .map_err(|e| e.to_string())?;
                        transcript
                            .checked_original_view(index, &final_post)
                            .map_err(|e| e.to_string())?
                    };
                    let computed = current.source().exact_prepared_mutations().to_vec();
                    current
                        .source_mut()
                        .verify_recorded_native_append(&post, &computed)
                        .map_err(|e| e.to_string())?;
                }
                history
                    .install_verified_inline_post(current.source())
                    .map_err(|e| e.to_string())?;
                Ok(first.expect("nonempty actual IO facts"))
            })();
            return match result {
                Ok(stop) => stop,
                Err(error) => {
                    *verification = Some(error.clone());
                    NativeMainServiceStopV1::StorageStopped(error)
                }
            };
        }
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            let row = if let Some(block) = &mut frame.block {
                block.row.as_mut()
            } else {
                frame.catchup.row.as_mut()
            };
            let Some(row) = row else {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::UnknownDependency);
            };
            native_main_service_row_stop(
                runtime.native_main_rows_verify_historical_append(history, row, mutations),
            )
        }) {
            Ok(stop) => {
                if let NativeMainServiceStopV1::StorageStopped(error) = &stop {
                    *verification = Some(error.clone());
                }
                stop
            }
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
}
