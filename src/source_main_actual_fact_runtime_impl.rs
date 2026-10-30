// Literal current live/common source PCs; only immutable metadata/fact adapter differs.
struct NativeMainActualFactSourceMetadataAccessV1<'a, 'b> {
    history: &'a mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'b>,
    capability: &'a crate::source_compact_index_v2::NativeColdIndexExecutionV1,
    transcript: &'a crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
    action_sequence: u64,
    verification: &'a mut Option<String>,
}
impl NativeMainSourceMetadataAccess for NativeMainActualFactSourceMetadataAccessV1<'_, '_> {
    fn find_max(&mut self, core: &mut ContractCore) -> R<V> {
        core.native_main_index_historical_scope(
            self.history.source_read_view(),
            None,
            "findMaxIndexedBlock",
            vec![],
        )
    }
    fn read_view(
        &mut self,
    ) -> Result<
        crate::source_compact_index_v2::MainIndexReadViewV1,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        Ok(self.history.source_read_view().clone())
    }
    fn prepare(
        &mut self,
        runtime: &mut SourceContractRuntime,
        start: V,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        storage_stop: &mut Option<String>,
    ) -> R<Option<NativeMainCatchupPreparedV1>> {
        runtime.native_main_catchup_prepare_with_metadata(start, self, grouping, storage_stop)
    }
    fn rows(
        &mut self,
        runtime: &mut SourceContractRuntime,
        row: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        let stop = runtime.native_main_rows_advance_historical(self.history, row);
        if let NativeMainRowStopV1::StorageStopped(error) = &stop {
            *self.verification = Some(error.clone());
        }
        stop
    }
    fn block(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        if !block.inline_profile {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::UnknownDependency);
        }
        runtime.native_main_service_block_advance_with_metadata(block, documents, self)
    }
    fn inline_block(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        runtime.native_main_inline_block_advance_with(block, documents, |core, frame, documents| {
            core.native_main_inline_index_advance_actual_facts(
                self.history,
                self.capability,
                self.transcript,
                self.action_sequence,
                frame,
                (documents, &mut *self.verification),
            )
        })
    }
    fn kernel(
        &mut self,
        runtime: &mut SourceContractRuntime,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
        enqueue: bool,
    ) -> Result<
        crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    > {
        let index = self
            .history
            .scoped_cold_index(self.capability)
            .map_err(|error| {
                *self.verification = Some(error.to_string());
                crate::source_main_step_kernel::MainStepError::InvalidInput
            })?;
        runtime.native_main_service_kernel_command(index, frame, command, enqueue)
    }
}

impl ContractCore {
    fn native_main_inline_index_advance_actual_facts(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        action_sequence: u64,
        frame: &mut NativeMainInlineIndexFrameV1,
        request: (&V, &mut Option<String>),
    ) -> NativeMainRowStopV1 {
        let (documents, verification) = request;
        use crate::txindex_source_owner::TxIndexInlineStopV1 as Stop;
        if let Some(error) = &frame.storage_stop {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        if let Some(error) = &frame.local_stop {
            return NativeMainRowStopV1::SourceStopped(error.clone());
        }
        if frame.result.is_some() {
            return NativeMainRowStopV1::Complete;
        }
        if frame.historical.is_some() {
            return NativeMainRowStopV1::AwaitNativeWrite;
        }
        if frame.write.is_some() {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        let Some(production) = self.main_production.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let mut owner = std::mem::take(&mut production.index);
        let result = (|| loop {
            match owner.advance_inline_source(
                frame.scope,
                &mut frame.source,
                &mut NativeMainServiceIndexBackend {
                    base: NativeMainHistoricalIndexBackend {
                        core: self,
                        view: history.source_read_view(),
                    },
                    documents,
                },
            )? {
                Stop::Complete => {
                    let evaluation = frame.source.take_evaluation()?;
                    self.main_height_rejections
                        .extend(evaluation.deferred_rejections);
                    frame.result = Some(outcome(evaluation.outcome));
                    return Ok(NativeMainRowStopV1::Complete);
                }
                Stop::AwaitDatabase => {
                    if self.effects_exhausted {
                        return Err(SourceRuleError::Capacity);
                    }
                    let call = frame
                        .source
                        .pending_call()
                        .ok_or(SourceRuleError::InvalidInput)?
                        .clone();
                    let method = call
                        .target
                        .strip_prefix("db.")
                        .ok_or(SourceRuleError::InvalidInput)?;
                    match self.check_failure("txIndex", method) {
                        Err(Fault::Thrown(error)) => {
                            let returned = DependencyReturn::PendingThrow(error);
                            native_main_inline_emit(self, &call, &returned);
                            owner.resolve_inline_source(
                                frame.scope,
                                &mut frame.source,
                                returned,
                            )?;
                        }
                        Err(Fault::Local(error)) => return Err(error),
                        Ok(()) => {
                            let original = history.source_history_pin().clone();
                            let index = history.scoped_cold_index(capability).map_err(|error| {
                                *verification = Some(error.to_string());
                                SourceRuleError::Backend
                            })?;
                            let pending = transcript
                                .checked_original_view(index, &original)
                                .map_err(|error| {
                                    *verification = Some(error.to_string());
                                    SourceRuleError::Backend
                                })?
                                .prepare_historical_inline_database(
                                    method,
                                    call.arguments[1..].to_vec(),
                                )
                                .map_err(|error| {
                                    *verification = Some(error.to_string());
                                    SourceRuleError::Backend
                                })?;
                            let pending = transcript
                                .prepare_actual_pending_inline(
                                    index,
                                    action_sequence,
                                    true,
                                    pending,
                                )
                                .map_err(|error| {
                                    *verification = Some(error.to_string());
                                    SourceRuleError::UnknownDependency
                                })?;
                            frame.historical = Some(Box::new(pending));
                            return Ok(NativeMainRowStopV1::AwaitNativeWrite);
                        }
                    }
                }
            }
        })();
        self.main_production
            .as_mut()
            .expect("same actual production owner")
            .index = owner;
        match result {
            Ok(stop) => stop,
            Err(error) => {
                frame.local_stop = Some(error.clone());
                NativeMainRowStopV1::SourceStopped(error)
            }
        }
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_service_advance_actual_facts(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        action_sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
        request: (usize, &mut Option<String>),
    ) -> NativeMainServiceStopV1 {
        let (limit, verification) = request;
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            runtime.native_main_service_advance_with_access(
                frame,
                limit,
                &mut NativeMainActualFactSourceMetadataAccessV1 {
                    history,
                    capability,
                    transcript,
                    action_sequence,
                    verification,
                },
            )
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    pub(crate) fn native_main_service_resolve_actual_facts(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
        verification: &mut Option<String>,
    ) -> NativeMainServiceStopV1 {
        if let Some(inline) = frame.block.as_mut().and_then(|b| b.inline_index.as_mut()) {
            let stop = self.core.native_main_inline_index_resolve_mode4(inline);
            if stop != NativeMainRowStopV1::Complete {
                *verification = Some(format!("historical inline resolve failed: {stop:?}"));
                return native_main_service_row_stop(stop);
            }
            return self.native_main_service_advance_actual_facts(
                history,
                capability,
                transcript,
                sequence,
                frame,
                (4096, verification),
            );
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
            let stop = runtime.native_main_rows_resolve_historical_post(history, row);
            if stop == NativeMainRowStopV1::Complete {
                runtime.native_main_service_advance_with_access(
                    frame,
                    4096,
                    &mut NativeMainActualFactSourceMetadataAccessV1 {
                        history,
                        capability,
                        transcript,
                        action_sequence: sequence,
                        verification,
                    },
                )
            } else {
                if let NativeMainRowStopV1::StorageStopped(error) = &stop {
                    *verification = Some(error.clone());
                }
                native_main_service_row_stop(stop)
            }
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
}
