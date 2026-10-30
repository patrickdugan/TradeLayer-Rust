impl SourceContractRuntime {
    pub(crate) fn native_main_null_pending(
        &self,
        frame: &NativeMainNullSyncFrameV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    ) -> Vec<(
        NativeMainNullJobV1,
        crate::source_main_native_session::NullPendingKind,
    )> {
        use crate::source_main_native_session::NullPendingKind;
        let mut out = vec![];
        for id in [
            NativeMainNullJobV1::Parent,
            NativeMainNullJobV1::DetachedInit,
        ] {
            let job = frame.job(id);
            if let Some(operation) = job.caller.pending_descriptor() {
                out.push((id, NullPendingKind::Caller(operation)));
            }
            if let Some(service) = &job.service {
                if let Some(operation) =
                    service.native_main_retained_consensus_descriptor(job.caller.caller_scope)
                {
                    out.push((id, NullPendingKind::Service(operation.bind_route(route))));
                }
            }
        }
        out
    }
    pub(crate) fn native_main_null_append(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainNullSyncFrameV1,
        id: NativeMainNullJobV1,
        operation: &crate::source_main_native_session::NullPendingKind,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        if !self
            .native_main_null_pending(frame, route)
            .contains(&(id, operation.clone()))
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let job = frame.job_mut(id);
        match operation {
            crate::source_main_native_session::NullPendingKind::Caller(caller) => {
                Ok(native_main_service_row_stop(
                    self.native_main_retained_caller_append(index, &mut job.caller, caller, permit),
                ))
            }
            crate::source_main_native_session::NullPendingKind::Service(service) => self
                .native_main_retained_consensus_append(
                    index,
                    &mut job.caller,
                    job.service
                        .as_deref_mut()
                        .ok_or(SourceRuleError::InvalidInput)?,
                    service,
                    route,
                    permit,
                ),
        }
    }
    pub(crate) fn native_main_null_resolve(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainNullSyncFrameV1,
        id: NativeMainNullJobV1,
        operation: &crate::source_main_native_session::NullPendingKind,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        if !self
            .native_main_null_pending(frame, route)
            .contains(&(id, operation.clone()))
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let job = frame.job_mut(id);
        let (stop, next) = match operation {
            crate::source_main_native_session::NullPendingKind::Caller(caller) => (
                native_main_service_row_stop(self.native_main_retained_caller_resolve(
                    index,
                    &mut job.caller,
                    caller,
                )),
                NullReady::ChildRun,
            ),
            crate::source_main_native_session::NullPendingKind::Service(service) => (
                self.native_main_retained_consensus_resolve(
                    index,
                    &mut job.caller,
                    job.service
                        .as_deref_mut()
                        .ok_or(SourceRuleError::InvalidInput)?,
                    service,
                    route,
                )?,
                NullReady::ConsensusRun(id),
            ),
        };
        if stop == NativeMainServiceStopV1::Quiescent {
            frame.ready.push_back(next);
        }
        Ok(stop)
    }
}
