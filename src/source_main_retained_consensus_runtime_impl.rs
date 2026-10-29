// Explicit new borrower of the retained Main invocation. It executes the real
// catchup/Kernel/block PCs; no second invocation enter/leave or source retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainRetainedConsensusOperationKindV1 {
    CallerInline,
    ServiceInline,
    ServiceValidity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainRetainedConsensusPendingV1 {
    kind: NativeMainRetainedConsensusOperationKindV1,
    caller: Option<NativeMainRetainedCallerPendingV1>,
    caller_scope: u64,
    source_job: u64,
    method_scope: u64,
    operation_ordinal: u64,
    role: crate::txindex_source_owner::TxIndexExtractAwaitRoleV1,
    context: Option<V>,
    route: Option<crate::source_main_native_session::NativeMainRetainedCallerRouteV1>,
}
impl NativeMainRetainedConsensusPendingV1 {
    pub(crate) fn bind_route(
        mut self,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    ) -> Self {
        self.route = Some(route.clone());
        self
    }
    pub fn kind(&self) -> NativeMainRetainedConsensusOperationKindV1 {
        self.kind.clone()
    }
    pub fn value(&self) -> V {
        obj(&[
            ("profile", s("TL-MAIN-RETAINED-CONSENSUS-PENDING-13-1")),
            ("kind", s(format!("{:?}", self.kind))),
            ("callerScope", s(self.caller_scope.to_string())),
            ("sourceJob", s(self.source_job.to_string())),
            ("methodScope", s(self.method_scope.to_string())),
            ("operationOrdinal", s(self.operation_ordinal.to_string())),
            ("role", s(format!("{:?}", self.role))),
            ("context", self.context.clone().unwrap_or(V::Null)),
            (
                "route",
                self.route.as_ref().map_or(V::Null, |r| r.value().clone()),
            ),
            (
                "caller",
                self.caller
                    .as_ref()
                    .map_or(V::Null, NativeMainRetainedCallerPendingV1::value),
            ),
        ])
    }
}
impl NativeMainRetainedCallerFrameV1 {
    pub(crate) fn native_main_retained_invocation_route(&self) -> Result<V, SourceRuleError> {
        let invocation = self
            .invocation
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?;
        Ok(obj(&[
            ("actualGeneration", s(self.kernel.generation().to_string())),
            ("target", s(&invocation.target)),
            ("spotMain", V::Bool(invocation.spot_main)),
            ("rejectedBefore", s(invocation.rejected_before.to_string())),
            (
                "transferRejectedBefore",
                s(invocation.transfer_rejected_before.to_string()),
            ),
        ]))
    }
    pub(crate) fn native_main_retained_consensus_attempted(
        &self,
    ) -> Option<NativeMainRetainedConsensusPendingV1> {
        if self.inline.as_ref().is_some_and(|i| i.attempted) || self.extract_io.attempted {
            self.native_main_retained_consensus_descriptor()
        } else {
            None
        }
    }
    pub(crate) fn native_main_retained_consensus_descriptor(
        &self,
    ) -> Option<NativeMainRetainedConsensusPendingV1> {
        let caller = self.pending_descriptor().or_else(|| {
            let active = self.extract.as_ref()?.active_descriptor()?;
            Some(NativeMainRetainedCallerPendingV1 {
                caller_scope: self.caller_scope,
                source_job: active.source_job().checked_add(1)?,
                method_scope: active.method_scope(),
                operation_ordinal: active.operation_ordinal(),
                role: active.role(),
            })
        })?;
        Some(NativeMainRetainedConsensusPendingV1 {
            kind: NativeMainRetainedConsensusOperationKindV1::CallerInline,
            caller: Some(caller.clone()),
            caller_scope: self.caller_scope,
            source_job: caller.source_job,
            method_scope: caller.method_scope,
            operation_ordinal: caller.operation_ordinal,
            role: caller.role,
            context: self.pending_write().map(|w| w.durable_read_context()),
            route: None,
        })
    }
}
impl NativeMainServiceContinuationV1 {
    pub(crate) fn native_main_retained_consensus_descriptor(
        &self,
        caller_scope: u64,
    ) -> Option<NativeMainRetainedConsensusPendingV1> {
        let job = self
            .block
            .as_ref()
            .map_or(0, |b| b.request.job.saturating_add(1));
        if job >= 1 << 63 {
            return None;
        }
        let source_job = (1 << 63) | job;
        let role = if self.caller_result.is_some() {
            crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Detached
        } else {
            crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Awaited
        };
        if let Some(inline) = self.block.as_ref().and_then(|b| b.inline_index.as_ref()) {
            if let Some(write) = inline.pending_write() {
                return Some(NativeMainRetainedConsensusPendingV1 {
                    kind: NativeMainRetainedConsensusOperationKindV1::ServiceInline,
                    caller: None,
                    caller_scope,
                    source_job,
                    method_scope: inline.scope,
                    operation_ordinal: inline.source.pending_write_ordinal(),
                    role,
                    context: Some(write.durable_read_context()),
                    route: None,
                });
            }
        }
        let row = if let Some(block) = &self.block {
            block.row.as_ref()
        } else {
            self.catchup.row.as_ref()
        }?;
        let write = row.pending_write()?;
        let completed = self
            .block
            .as_ref()
            .map_or(self.catchup.validity.len(), |b| b.validity.len());
        let ordinal = completed.checked_add(row.validity.len())?;
        Some(NativeMainRetainedConsensusPendingV1 {
            kind: NativeMainRetainedConsensusOperationKindV1::ServiceValidity,
            caller: None,
            caller_scope,
            source_job,
            method_scope: caller_scope,
            operation_ordinal: u64::try_from(ordinal).ok()?,
            role,
            context: Some(write.durable_read_context()),
            route: None,
        })
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_retained_consensus_begin(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
        documents: V,
    ) -> Result<NativeMainServiceContinuationV1, SourceRuleError> {
        self.native_main_retained_consensus_service_frame(caller, documents)
    }
    pub(crate) fn native_main_retained_consensus_pending(
        &self,
        caller: &NativeMainRetainedCallerFrameV1,
        service: &NativeMainServiceContinuationV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    ) -> Vec<NativeMainRetainedConsensusPendingV1> {
        let mut out = vec![];
        if let Some(p) = caller.native_main_retained_consensus_descriptor() {
            out.push(p.bind_route(route))
        }
        if let Some(p) = service.native_main_retained_consensus_descriptor(caller.caller_scope) {
            out.push(p.bind_route(route))
        }
        out
    }
    pub(crate) fn native_main_retained_consensus_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        limit: usize,
    ) -> Result<(NativeMainServiceStopV1, bool), SourceRuleError> {
        if limit == 0
            || limit > 4096
            || (caller.invocation.is_none() && caller.final_result.is_none())
        {
            return Err(SourceRuleError::InvalidInput);
        }
        if service.caller_result.is_some() && caller.extract.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        if caller.pending_write().is_some() {
            // A pending detached return may coexist with the genuine read-only
            // empty-index consensus path. No second native plan can be created
            // in this bounded admission, and no live job is driven after the
            // consensus caller turn until that first operation resolves.
            if service.caller_result.is_some() {
                return Err(SourceRuleError::InvalidInput);
            }
            let mut has_rows = false;
            index
                .visit_rows(|row| {
                    if matches!(row.get("_id"),V::String(ref id) if id.starts_with("tx-")) {
                        has_rows = true;
                    }
                    Ok(true)
                })
                .map_err(|_| SourceRuleError::Backend)?;
            if has_rows {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        for _ in 0..limit {
            let stop = self.native_main_service_advance(index, service, 1);
            if caller.caller.is_none() {
                if let Some(result) = &service.caller_result {
                    match result {
                        Ok(DependencyReturn::Pending(v) | DependencyReturn::Immediate(v)) => {
                            caller.caller = Some(DependencyReturn::Pending(
                                if matches!(
                                    caller.command,
                                    NativeMainRetainedCallerV1::SyncIndex { .. }
                                ) {
                                    v.clone()
                                } else {
                                    V::Undefined
                                },
                            ));
                        }
                        Ok(DependencyReturn::PendingThrow(e) | DependencyReturn::Throw(e)) => {
                            caller.caller = Some(DependencyReturn::PendingThrow(e.clone()));
                        }
                        Err(e) => {
                            caller.local_stop = Some(e.clone());
                            return Err(e.clone());
                        }
                    }
                    // Preserve the source caller turn before further discarded
                    // incoming work. The parent invocation/queue stays retained.
                    self.native_main_retained_consensus_finish_if_ready(caller, service);
                    return Ok((stop, true));
                }
            }
            if stop != NativeMainServiceStopV1::DrainBound {
                return Ok((stop, false));
            }
        }
        Ok((NativeMainServiceStopV1::DrainBound, false))
    }
    pub(crate) fn native_main_retained_consensus_detached(
        &mut self,
        index: &mut SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        documents: &V,
    ) -> Result<NativeMainRetainedCallerStopV1, SourceRuleError> {
        if !service.borrowed_invocation
            || service.pending_native_context().is_some()
            || caller.invocation.is_none()
            || !caller
                .extract
                .as_ref()
                .is_some_and(|e| e.detached_pending())
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let stop = self.native_main_retained_caller_advance(index, caller, documents);
        if caller.extract.is_none() {
            if caller.caller.is_some() {
                self.native_main_retained_consensus_finish_if_ready(caller, service);
            } else {
                self.native_main_retained_consensus_notifications(caller)?;
            }
        }
        Ok(stop)
    }
    fn native_main_retained_consensus_finish_if_ready(
        &mut self,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
    ) {
        if caller.extract.is_some() || caller.invocation.is_none() {
            return;
        }
        let Some(result) = caller.caller.clone() else {
            return;
        };
        let result = match result {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => Ok(v),
            DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => Err(Fault::Thrown(e)),
        };
        // The actual caller AND its detached child are finished. Complete the
        // ONE original header now; retain the whole financial/index owner and
        // all effects. Later discarded blocks use their existing real headers.
        self.native_main_retained_caller_finish(caller, result);
        service.borrowed_invocation = false;
        if let Some(Err(error)) = &caller.final_result {
            caller.local_stop = Some(error.clone());
            service.stopped = Some(error.clone());
        }
    }
    fn native_main_retained_consensus_notifications(
        &mut self,
        caller: &mut NativeMainRetainedCallerFrameV1,
    ) -> Result<(), SourceRuleError> {
        let invocation = caller
            .invocation
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        let rejected = self.core.main_height_rejections[invocation.rejected_before..].to_vec();
        for e in rejected {
            self.core.emit(obj(&[
                ("kind", s("unhandled-rejection")),
                (
                    "thrown",
                    obj(&[("name", s(e.name)), ("message", s(e.message))]),
                ),
            ]));
        }
        invocation.rejected_before = self.core.main_height_rejections.len();
        if invocation.spot_main {
            let rejected = self.core.source_unhandled_rejections
                [invocation.transfer_rejected_before..]
                .to_vec();
            for error in rejected {
                self.core.emit(obj(&[
                    ("kind", s("unhandled-rejection")),
                    ("thrown", error),
                ]));
            }
            invocation.transfer_rejected_before = self.core.source_unhandled_rejections.len();
        }
        if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            Ok(())
        }
    }
    pub(crate) fn native_main_retained_consensus_append(
        &mut self,
        index: &mut SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        operation: &NativeMainRetainedConsensusPendingV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        if !self
            .native_main_retained_consensus_pending(caller, service, route)
            .contains(operation)
            || operation.context.is_none()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        match operation.kind {
            NativeMainRetainedConsensusOperationKindV1::CallerInline => {
                if service.pending_native_context().is_some() {
                    return Err(SourceRuleError::InvalidInput);
                }
                Ok(native_main_service_row_stop(
                    self.native_main_retained_caller_append(
                        index,
                        caller,
                        operation
                            .caller
                            .as_ref()
                            .ok_or(SourceRuleError::InvalidInput)?,
                        permit,
                    ),
                ))
            }
            _ => {
                if caller.pending_write().is_some() {
                    return Err(SourceRuleError::InvalidInput);
                }
                Ok(self.native_main_service_append_with_escrow(index, service, permit))
            }
        }
    }
    pub(crate) fn native_main_retained_consensus_resolve(
        &mut self,
        index: &SourceCompactIndex,
        caller: &mut NativeMainRetainedCallerFrameV1,
        service: &mut NativeMainServiceContinuationV1,
        operation: &NativeMainRetainedConsensusPendingV1,
        route: &crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    ) -> Result<NativeMainServiceStopV1, SourceRuleError> {
        if !self
            .native_main_retained_consensus_pending(caller, service, route)
            .contains(operation)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let stop = match operation.kind {
            NativeMainRetainedConsensusOperationKindV1::CallerInline => self
                .native_main_retained_caller_resolve(
                    index,
                    caller,
                    operation
                        .caller
                        .as_ref()
                        .ok_or(SourceRuleError::InvalidInput)?,
                ),
            NativeMainRetainedConsensusOperationKindV1::ServiceInline => {
                self.core.native_main_inline_index_resolve(
                    index,
                    service
                        .block
                        .as_mut()
                        .and_then(|b| b.inline_index.as_mut())
                        .ok_or(SourceRuleError::InvalidInput)?,
                )
            }
            NativeMainRetainedConsensusOperationKindV1::ServiceValidity => {
                let row = if let Some(block) = &mut service.block {
                    block.row.as_mut()
                } else {
                    service.catchup.row.as_mut()
                }
                .ok_or(SourceRuleError::InvalidInput)?;
                self.native_main_rows_resolve_post(index, row)
            }
        };
        Ok(native_main_service_row_stop(stop))
    }
}
