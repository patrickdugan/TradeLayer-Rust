#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeMainStartupNestedPcV1 {
    Single,
    First,
    DrainFirstIncoming,
    BeginOuter,
    Outer,
}
pub(crate) struct NativeMainStartupNestedConsensusV1 {
    pc: NativeMainStartupNestedPcV1,
    completed_first: Option<Box<NativeMainCatchupContinuationV1>>,
    first_result: Option<Result<DependencyReturn, SourceRuleError>>,
    parked: Option<crate::source_main_step_kernel::MainStartupParkedBlockV1>,
}
impl NativeMainStartupNestedConsensusV1 {
    pub(crate) fn new(nested: bool) -> Self {
        Self {
            pc: if nested {
                NativeMainStartupNestedPcV1::First
            } else {
                NativeMainStartupNestedPcV1::Single
            },
            completed_first: None,
            first_result: None,
            parked: None,
        }
    }
    pub(crate) fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("pc", &s(format!("{:?}", self.pc)))?;
        visitor(
            "completedFirst/present",
            &V::Bool(self.completed_first.is_some()),
        )?;
        if let Some(first) = &self.completed_first {
            first.visit_canonical_fields(&mut |p, v| visitor(&format!("completedFirst/{p}"), v))?;
        }
        visitor("firstResult/present", &V::Bool(self.first_result.is_some()))?;
        if let Some(result) = &self.first_result {
            match result {
                Ok(value) => native_main_row_visit_value(
                    "firstResult/value",
                    &crate::source_contract_replay::SourceContractCommandOutcome::Dependency(
                        value.clone(),
                    )
                    .value(),
                    visitor,
                )?,
                Err(error) => visitor("firstResult/local", &s(format!("{error:?}")))?,
            }
        }
        visitor("parked/present", &V::Bool(self.parked.is_some()))?;
        if let Some(token) = &self.parked {
            native_main_row_visit_value("parked/value", &token.value(), visitor)?;
        }
        Ok(())
    }
}
impl SourceContractRuntime {
    fn native_main_startup_service_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        service: &mut NativeMainServiceContinuationV1,
        limit: usize,
    ) -> NativeMainServiceStopV1 {
        match self.native_main_service_with_client_scope(service.client_binding, |runtime| {
            runtime.native_main_service_advance_with_access_startup(
                service,
                limit,
                &mut NativeMainLiveSourceMetadataAccess { index },
                true,
            )
        }) {
            Ok(stop) => stop,
            Err(error) => NativeMainServiceStopV1::SourceStopped(error),
        }
    }
    pub(crate) fn native_main_startup_literal_tracking_ready(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
    ) -> bool {
        caller.command == NativeMainRetainedCallerV1::Initialize
            && caller.pc == NativeMainRetainedCallerPcV1::InstanceInit
            && caller.invocation.is_some()
            && caller.caller.is_none()
            && caller.local_stop.is_none()
            && caller.inline.as_ref().is_some_and(|inline| {
                inline.source.pending_call().is_none()
                    && inline.write.is_none()
                    && inline.result.is_none()
                    && !inline.attempted
                    && inline.storage_stop.is_none()
                    && inline.local_stop.is_none()
            })
    }
    pub(crate) fn native_main_startup_tracked_caller_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        documents: &V,
        nested: &mut bool,
    ) -> NativeMainRetainedCallerStopV1 {
        match self.native_main_service_with_client_scope(
            NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING,
            |runtime| {
                runtime.native_main_retained_caller_advance_inner_tracked(
                    index,
                    caller,
                    documents,
                    Some(nested),
                )
            },
        ) {
            Ok(Ok(stop)) => stop,
            Ok(Err(error)) | Err(error) => {
                NativeMainRetainedCallerStopV1::Native(NativeMainRowStopV1::SourceStopped(error))
            }
        }
    }
    fn native_main_startup_publish_parent(
        &mut self,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
    ) -> Result<bool, SourceRuleError> {
        let mut returned_now = false;
        if caller.caller.is_none() {
            if let Some(result) = &service.caller_result {
                caller.caller = Some(match result {
                    Ok(DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)) => {
                        DependencyReturn::Pending(V::Undefined)
                    }
                    Ok(DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error)) => {
                        DependencyReturn::PendingThrow(error.clone())
                    }
                    Err(error) => {
                        caller.local_stop = Some(error.clone());
                        return Err(error.clone());
                    }
                });
                returned_now = true;
            }
        }
        if caller.caller.is_some() && service.block.is_none() && service.completed_block.is_none() {
            self.native_main_retained_consensus_finish_if_ready(caller, service);
        }
        Ok(returned_now)
    }
    pub(crate) fn native_main_startup_nested_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        nested: &mut NativeMainStartupNestedConsensusV1,
        limit: usize,
    ) -> Result<(NativeMainServiceStopV1, bool), SourceRuleError> {
        use NativeMainStartupNestedPcV1 as Pc;
        if limit == 0 || limit > 4096 {
            return Err(SourceRuleError::InvalidInput);
        }
        for _ in 0..limit {
            match nested.pc {
                Pc::Single => {
                    return self.native_main_retained_consensus_advance(index, caller, service, 1)
                }
                Pc::First => {
                    let stop = self.native_main_startup_service_advance(index, service, 1);
                    if let Some(result) = &service.caller_result {
                        match result {
                            Ok(DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)) => {
                                nested.pc = Pc::DrainFirstIncoming
                            }
                            Ok(DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)) => {
                                // return Promise from syncIndex is NOT awaited by
                                // its try/catch. The actual rejection propagates
                                // through initOrLoad and prevents the outer call.
                                nested.pc = Pc::Outer;
                                return Ok((
                                    stop,
                                    self.native_main_startup_publish_parent(caller, service)?,
                                ));
                            }
                            Err(error) => return Err(error.clone()),
                        }
                    }
                    if stop != NativeMainServiceStopV1::DrainBound {
                        return Ok((stop, false));
                    }
                }
                Pc::DrainFirstIncoming => {
                    let stop = self.native_main_startup_service_advance(index, service, 1);
                    match stop {
                        NativeMainServiceStopV1::AwaitNativeWrite => {
                            if service.pending_native_context().is_none() {
                                return Err(SourceRuleError::InvalidInput);
                            }
                            let block = service
                                .block
                                .as_ref()
                                .ok_or(SourceRuleError::InvalidInput)?;
                            nested.parked = Some(
                                service
                                    .kernel
                                    .startup_park_block(&block.request)
                                    .map_err(|_| SourceRuleError::InvalidInput)?,
                            );
                            nested.pc = Pc::BeginOuter;
                        }
                        NativeMainServiceStopV1::PendingTimer
                        | NativeMainServiceStopV1::Quiescent => nested.pc = Pc::BeginOuter,
                        NativeMainServiceStopV1::DrainBound => (),
                        _ => return Ok((stop, false)),
                    }
                }
                Pc::BeginOuter => {
                    let replacement =
                        self.native_main_catchup_frame(caller.genesis.clone(), None)?;
                    nested.completed_first = Some(Box::new(std::mem::replace(
                        &mut service.catchup,
                        replacement,
                    )));
                    nested.first_result = service.caller_result.take();
                    service.tail_started = false;
                    service.sync_job = None;
                    nested.pc = Pc::Outer;
                }
                Pc::Outer => {
                    let stop = if nested.parked.is_some()
                        && service.tail_started
                        && service.caller_result.is_none()
                    {
                        // The one real live block remains held; only another
                        // actually ready source continuation may run here.
                        if service.kernel.service_next_block().is_some() {
                            return Ok((NativeMainServiceStopV1::AwaitNativeWrite, false));
                        }
                        let evaluation=self.native_main_service_with_client_scope(service.client_binding,|runtime|runtime.native_main_service_kernel_command(index,service,crate::source_main_step_kernel::MainStepCommandV1::DrainReady{limit:1},false))?.map_err(|_|SourceRuleError::InvalidInput)?;
                        self.native_main_service_accept_evaluation(service, evaluation);
                        NativeMainServiceStopV1::DrainBound
                    } else {
                        self.native_main_startup_service_advance(index, service, 1)
                    };
                    let returned = self.native_main_startup_publish_parent(caller, service)?;
                    if returned || stop != NativeMainServiceStopV1::DrainBound {
                        return Ok((stop, returned));
                    }
                }
            }
        }
        Ok((NativeMainServiceStopV1::DrainBound, false))
    }
    pub(crate) fn native_main_startup_nested_unpark_after_resolve(
        &mut self,
        service: &mut NativeMainServiceContinuationV1,
        nested: &mut NativeMainStartupNestedConsensusV1,
    ) -> Result<(), SourceRuleError> {
        if let Some(token) = &nested.parked {
            service
                .kernel
                .startup_unpark_block(token)
                .map_err(|_| SourceRuleError::InvalidInput)?;
            nested.parked = None;
        }
        Ok(())
    }
    pub(crate) fn native_main_startup_nested_pending(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
        service: &NativeMainServiceContinuationV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    ) -> Vec<NativeMainRetainedConsensusPendingV1> {
        let mut pending = self.native_main_retained_consensus_pending(caller, service, route);
        for operation in &mut pending {
            let job = operation.source_job & !(1 << 63);
            if job != 0 && service.kernel.startup_job_is_detached_incoming(job - 1) {
                operation.role = crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Detached;
            }
        }
        pending
    }
    fn native_main_startup_nested_transport_operation(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
        service: &NativeMainServiceContinuationV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<NativeMainRetainedConsensusPendingV1, SourceRuleError> {
        if !self
            .native_main_startup_nested_pending(caller, service, route)
            .contains(operation)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        self.native_main_retained_consensus_pending(caller, service, route)
            .into_iter()
            .find(|candidate| {
                let mut compared = candidate.clone();
                compared.role = operation.role;
                compared == *operation
            })
            .ok_or(SourceRuleError::InvalidInput)
    }
    pub(crate) fn native_main_startup_nested_append(
        &mut self,
        index: &mut SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
        operation: &NativeMainRetainedConsensusPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        let transport =
            self.native_main_startup_nested_transport_operation(caller, service, route, operation)?;
        self.native_main_retained_consensus_append(
            index, caller, service, &transport, route, permit,
        )
    }
    pub(crate) fn native_main_startup_nested_resolve(
        &mut self,
        index: &SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
        nested: &mut NativeMainStartupNestedConsensusV1,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        let transport =
            self.native_main_startup_nested_transport_operation(caller, service, route, operation)?;
        let stop =
            self.native_main_retained_consensus_resolve(index, caller, service, &transport, route)?;
        let held_job = service
            .block
            .as_ref()
            .map(|block| (1 << 63) | (block.request.job + 1));
        if stop == NativeMainServiceStopV1::Quiescent
            && held_job == Some(operation.source_job)
            && nested.parked.is_some()
        {
            self.native_main_startup_nested_unpark_after_resolve(service, nested)?;
        }
        Ok(stop)
    }
}
