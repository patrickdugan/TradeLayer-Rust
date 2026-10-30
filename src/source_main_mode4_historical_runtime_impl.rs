trait NativeMainServiceBackendCoreV1 {
    fn service_core(&mut self) -> &mut ContractCore;
}
impl NativeMainServiceBackendCoreV1 for NativeMainIndexBackend<'_> {
    fn service_core(&mut self) -> &mut ContractCore {
        self.core
    }
}
impl NativeMainServiceBackendCoreV1 for NativeMainHistoricalIndexBackend<'_> {
    fn service_core(&mut self) -> &mut ContractCore {
        self.core
    }
}
struct NativeMainMode4SourceMetadataAccessV1<'a, 'b> {
    history: &'a mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'b>,
    capability: &'a crate::source_compact_index_v2::NativeColdIndexExecutionV1,
    transcript: &'a crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
    action_sequence: u64,
}
impl NativeMainSourceMetadataAccess for NativeMainMode4SourceMetadataAccessV1<'_, '_> {
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
        runtime.native_main_rows_advance_historical(self.history, row)
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
            core.native_main_inline_index_advance_mode4(
                self.history,
                self.capability,
                self.transcript,
                self.action_sequence,
                frame,
                documents,
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
            .map_err(|_| crate::source_main_step_kernel::MainStepError::InvalidInput)?;
        runtime.native_main_service_kernel_command(index, frame, command, enqueue)
    }
}
impl ContractCore {
    fn native_main_service_load_module_mode4(
        &mut self,
        view: &crate::source_compact_index_v2::MainIndexReadViewV1,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let evaluation = owner.load_module(&mut NativeMainServiceIndexBackend {
            base: NativeMainHistoricalIndexBackend { core: self, view },
            documents,
        });
        self.main_production
            .as_mut()
            .expect("same actual production owner")
            .index = owner;
        self.main_height_rejections
            .extend(evaluation?.deferred_rejections);
        Ok(())
    }
    fn native_main_inline_index_advance_mode4(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
        action_sequence: u64,
        frame: &mut NativeMainInlineIndexFrameV1,
        documents: &V,
    ) -> NativeMainRowStopV1 {
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
                            let index = history
                                .scoped_cold_index(capability)
                                .map_err(|_| SourceRuleError::Backend)?;
                            let pending = transcript
                                .checked_original_view(index, &original)
                                .map_err(|_| SourceRuleError::Backend)?
                                .prepare_historical_inline_database(
                                    method,
                                    call.arguments[1..].to_vec(),
                                )
                                .map_err(|_| SourceRuleError::Backend)?;
                            let pending = transcript
                                .prepare_pending_inline(index, action_sequence, pending)
                                .map_err(|_| SourceRuleError::UnknownDependency)?;
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
    fn native_main_inline_index_resolve_mode4(
        &mut self,
        frame: &mut NativeMainInlineIndexFrameV1,
    ) -> NativeMainRowStopV1 {
        if !frame.attempted {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        let Some(pending) = frame.historical.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let returned = match pending.source_mut().resolve_recorded_post() {
            Ok(v) => v,
            Err(error) => return NativeMainRowStopV1::StorageStopped(error.to_string()),
        };
        let Some(call) = frame.source.pending_call().cloned() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let Some(production) = self.main_production.as_ref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Err(error) =
            production
                .index
                .resolve_inline_source(frame.scope, &mut frame.source, returned.clone())
        {
            return NativeMainRowStopV1::SourceStopped(error);
        }
        native_main_inline_emit(self, &call, &returned);
        frame.historical = None;
        frame.attempted = false;
        frame.storage_stop = None;
        NativeMainRowStopV1::Complete
    }
}
impl NativeMainServiceContinuationV1 {
    pub(crate) fn pending_mode4_historical_context(&self) -> Option<V> {
        self.block
            .as_ref()
            .and_then(|b| b.inline_index.as_ref())
            .and_then(|f| f.historical.as_ref())
            .map(|p| p.source().durable_read_context())
            .or_else(|| {
                self.block
                    .as_ref()
                    .and_then(|b| b.row.as_ref())
                    .and_then(|r| r.pending_historical_write())
                    .map(|p| p.durable_read_context())
            })
            .or_else(|| self.pending_historical_context())
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_service_begin_cold_selected(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        start: V,
        generation: u64,
        documents: V,
        mode4: Option<(
            &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
            u64,
        )>,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        if mode4.is_some() {
            self.native_main_service_begin_mode4(history, start, generation, documents)
        } else {
            self.native_main_service_begin_historical(
                history, capability, start, generation, documents,
            )
        }
    }
    pub(crate) fn native_main_service_advance_cold_selected(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        frame: &mut NativeMainServiceContinuationV1,
        limit: usize,
        mode4: Option<(
            &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
            u64,
        )>,
    ) -> NativeMainServiceStopV1 {
        match mode4 {
            Some((transcript, sequence)) => self.native_main_service_advance_mode4(
                history, capability, transcript, sequence, frame, limit,
            ),
            None => self.native_main_service_advance_historical(history, capability, frame, limit),
        }
    }
    pub(crate) fn native_main_service_append_cold_selected(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        frame: &mut NativeMainServiceContinuationV1,
        mutations: &[u8],
        mode4: Option<(
            &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
            u64,
        )>,
    ) -> NativeMainServiceStopV1 {
        match mode4 {
            Some((transcript, sequence)) => self.native_main_service_append_mode4(
                history, capability, transcript, sequence, frame, mutations,
            ),
            None => self.native_main_service_append_historical(history, frame, mutations),
        }
    }
    pub(crate) fn native_main_service_resolve_cold_selected(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        frame: &mut NativeMainServiceContinuationV1,
        mode4: Option<(
            &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
            u64,
        )>,
    ) -> NativeMainServiceStopV1 {
        match mode4 {
            Some((transcript, sequence)) => self.native_main_service_resolve_mode4(
                history, capability, transcript, sequence, frame,
            ),
            None => self.native_main_service_resolve_historical(history, capability, frame),
        }
    }
    pub(crate) fn native_main_service_begin_mode4(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        start: V,
        generation: u64,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        let view = history.source_read_view().clone();
        let mut frame = self.native_main_service_begin_with_loader(
            start,
            generation,
            documents,
            |core, documents| core.native_main_service_load_module_mode4(&view, documents),
        )?;
        frame.enable_inline_profile();
        Ok(frame)
    }
    pub(crate) fn native_main_service_advance_mode4(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
        action_sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
        limit: usize,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            runtime.native_main_service_advance_with_access(
                frame,
                limit,
                &mut NativeMainMode4SourceMetadataAccessV1 {
                    history,
                    capability,
                    transcript,
                    action_sequence,
                },
            )
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    pub(crate) fn native_main_service_append_mode4(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
        sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
        mutations: &[u8],
    ) -> NativeMainServiceStopV1 {
        if let Some(inline) = frame.block.as_mut().and_then(|b| b.inline_index.as_mut()) {
            let Some(pending) = inline.historical.as_mut() else {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
            };
            if inline.attempted
                || pending.append_sequence().is_some_and(|s| s != sequence)
                || pending.source().exact_prepared_mutations() != mutations
            {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::InvalidInput);
            }
            inline.attempted = true;
            let result = (|| {
                transcript
                    .verify_inline_operation(
                        history
                            .scoped_cold_index(capability)
                            .map_err(|e| e.to_string())?,
                        sequence,
                        pending.source_mut(),
                    )
                    .map_err(|e| e.to_string())?;
                history
                    .install_verified_inline_post(pending.source())
                    .map_err(|e| e.to_string())
            })();
            return match result {
                Ok(()) => NativeMainServiceStopV1::AwaitNativeWrite,
                Err(error) => NativeMainServiceStopV1::StorageStopped(error),
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
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    pub(crate) fn native_main_service_resolve_mode4(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
        sequence: u64,
        frame: &mut NativeMainServiceContinuationV1,
    ) -> NativeMainServiceStopV1 {
        if let Some(inline) = frame.block.as_mut().and_then(|b| b.inline_index.as_mut()) {
            let stop = self.core.native_main_inline_index_resolve_mode4(inline);
            if stop != NativeMainRowStopV1::Complete {
                return native_main_service_row_stop(stop);
            }
            return self.native_main_service_advance_mode4(
                history, capability, transcript, sequence, frame, 4096,
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
                    &mut NativeMainMode4SourceMetadataAccessV1 {
                        history,
                        capability,
                        transcript,
                        action_sequence: sequence,
                    },
                )
            } else {
                native_main_service_row_stop(stop)
            }
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
}
