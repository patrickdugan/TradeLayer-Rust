trait NativeMainSourceMetadataAccess {
    fn find_max(&mut self, core: &mut ContractCore) -> R<V>;
    fn read_view(
        &mut self,
    ) -> Result<
        crate::source_compact_index_v2::SourceIndexReadSnapshot,
        crate::source_compact_index_v2::CompactIndexError,
    >;

    fn prepare(
        &mut self,
        runtime: &mut SourceContractRuntime,
        start: V,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        storage_stop: &mut Option<String>,
    ) -> R<Option<NativeMainCatchupPreparedV1>>;
    fn rows(
        &mut self,
        runtime: &mut SourceContractRuntime,
        row: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1;
    fn block(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1;
    fn kernel(
        &mut self,
        runtime: &mut SourceContractRuntime,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
        enqueue: bool,
    ) -> Result<
        crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    >;
}
struct NativeMainLiveSourceMetadataAccess<'a> {
    index: &'a mut SourceCompactIndex,
}
impl NativeMainSourceMetadataAccess for NativeMainLiveSourceMetadataAccess<'_> {
    fn find_max(&mut self, core: &mut ContractCore) -> R<V> {
        core.native_main_index(self.index, "findMaxIndexedBlock", vec![])
    }
    fn read_view(
        &mut self,
    ) -> Result<
        crate::source_compact_index_v2::SourceIndexReadSnapshot,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        self.index
            .snapshot()
            .and_then(|pin| self.index.read_generation(&pin))
    }

    fn prepare(
        &mut self,
        runtime: &mut SourceContractRuntime,
        start: V,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        storage_stop: &mut Option<String>,
    ) -> R<Option<NativeMainCatchupPreparedV1>> {
        runtime.native_main_catchup_prepare_continuation(start, self.index, grouping, storage_stop)
    }
    fn rows(
        &mut self,
        runtime: &mut SourceContractRuntime,
        row: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        runtime.native_main_rows_advance(self.index, row)
    }
    fn block(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        runtime.native_main_service_block_advance(self.index, block, documents)
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
        runtime.native_main_service_kernel_command(self.index, frame, command, enqueue)
    }
}
struct NativeMainHistoricalSourceMetadataAccess<'a, 'b> {
    history: &'a mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'b>,
    capability: &'a crate::source_compact_index_v2::NativeColdIndexExecutionV1,
}
impl NativeMainServiceContinuationV1 {
    pub(crate) fn pending_historical_context(&self) -> Option<V> {
        self.catchup
            .row
            .as_ref()?
            .pending_historical_write()
            .map(|write| write.durable_read_context())
    }
}
impl NativeMainSourceMetadataAccess for NativeMainHistoricalSourceMetadataAccess<'_, '_> {
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
        crate::source_compact_index_v2::SourceIndexReadSnapshot,
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
        _runtime: &mut SourceContractRuntime,
        _block: &mut NativeMainServiceBlockFrame,
        _documents: &V,
    ) -> NativeMainRowStopV1 {
        // Retained inline processBlockData operations are a separate required
        // bridge. Refuse BEFORE their first source/native IO, never hot retry.
        NativeMainRowStopV1::StorageStopped("cold inline block continuation unavailable".into())
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
impl SourceContractRuntime {
    pub(crate) fn native_main_service_begin_historical(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        start: V,
        generation: u64,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        let index = history
            .scoped_cold_index(capability)
            .map_err(|_| SourceRuleError::Backend)?;
        self.native_main_service_begin(index, start, generation, documents)
    }
    pub(crate) fn native_main_service_advance_historical(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        frame: &mut NativeMainServiceContinuationV1,
        limit: usize,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            runtime.native_main_service_advance_with_access(
                frame,
                limit,
                &mut NativeMainHistoricalSourceMetadataAccess {
                    history,
                    capability,
                },
            )
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    pub(crate) fn native_main_service_append_historical(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        frame: &mut NativeMainServiceContinuationV1,
        mutations: &[u8],
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            if frame.block.is_some() {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::UnknownDependency);
            }
            let Some(row) = frame.catchup.row.as_mut() else {
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
    pub(crate) fn native_main_service_resolve_historical(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        frame: &mut NativeMainServiceContinuationV1,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(frame.client_binding, |runtime| {
            if frame.block.is_some() {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::UnknownDependency);
            }
            let Some(row) = frame.catchup.row.as_mut() else {
                return NativeMainServiceStopV1::SourceStopped(SourceRuleError::UnknownDependency);
            };
            let stop = runtime.native_main_rows_resolve_historical_post(history, row);
            if stop == NativeMainRowStopV1::Complete {
                runtime.native_main_service_advance_with_access(
                    frame,
                    4096,
                    &mut NativeMainHistoricalSourceMetadataAccess {
                        history,
                        capability,
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
    pub(crate) fn native_main_service_control_historical(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
        capability: &crate::source_compact_index_v2::NativeColdIndexExecutionV1,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<(), crate::source_main_step_kernel::MainStepError> {
        // The original control entry handles timer checks/terminal state and
        // executes literal Pause/Shutdown on the same actual financial owner.
        let index = history
            .scoped_cold_index(capability)
            .map_err(|_| crate::source_main_step_kernel::MainStepError::InvalidInput)?;
        self.native_main_service_control(index, frame, command)
    }
}
