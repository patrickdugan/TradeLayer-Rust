// A distinct explicit source composition; existing service/WAL selectors cannot
// enter it or fall through to its detached source child.
pub use crate::source_contract_runtime::{
    NativeMainRetainedConsensusOperationKindV1, NativeMainRetainedConsensusPendingV1,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeMainRetainedCallerRouteV1 {
    value: V,
}
impl NativeMainRetainedCallerRouteV1 {
    pub(crate) fn value(&self) -> &V {
        &self.value
    }
    fn from_actual_caller(
        caller: &SourceMainNativeRetainedCallerV1,
    ) -> Result<Self, SourceRuleError> {
        let invocation = caller
            .frame
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .native_main_retained_invocation_route()?;
        Ok(Self {
            value: obj(&[
                ("profile", s("TL-MAIN-ACTUAL-CALLER-ROUTE-13-1")),
                (
                    "entryCommands",
                    s(&caller.origin.session.commands.to_string()),
                ),
                ("entryHead", s(&hex(&caller.origin.session.commitment))),
                ("entryBefore", physical_value(&caller.origin.physical)),
                (
                    "originalPhysical",
                    physical_value(&caller.owner.origin_physical),
                ),
                (
                    "foundationCommands",
                    s(&caller.owner.foundation_head.commands.to_string()),
                ),
                (
                    "foundationHead",
                    s(&hex(&caller.owner.foundation_head.commitment)),
                ),
                (
                    "actualFoundationBytes",
                    s(&hex(&sha256(&caller.owner.foundation))),
                ),
                ("declaredGeneration", s(&caller.generation.to_string())),
                ("retainedInvocation", invocation),
            ]),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainRetainedConsensusErrorV1 {
    InvalidInput,
    PendingOtherOperation,
    WriteAlreadyAttempted,
    NoAttemptedWrite,
    Local(SourceRuleError),
    Backend(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainRetainedConsensusStopV1 {
    CallerReturned,
    Service(crate::source_contract_runtime::NativeMainServiceStopV1),
    Detached(NativeMainRetainedCallerStopV1),
}
pub struct RejectedNativeMainRetainedConsensusV1 {
    pub error: NativeMainRetainedConsensusErrorV1,
    caller: SourceMainNativeRetainedCallerV1,
}
impl RejectedNativeMainRetainedConsensusV1 {
    pub fn into_caller(self) -> SourceMainNativeRetainedCallerV1 {
        self.caller
    }
}
impl core::fmt::Debug for RejectedNativeMainRetainedConsensusV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainRetainedConsensusV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
pub struct SourceMainNativeRetainedConsensusV1 {
    caller: SourceMainNativeRetainedCallerV1,
    service: Box<crate::source_contract_runtime::NativeMainServiceContinuationV1>,
    attempted: Option<NativeMainRetainedConsensusPendingV1>,
    route: NativeMainRetainedCallerRouteV1,
}
impl SourceMainNativeRetainedCallerV1 {
    pub fn into_retained_consensus(
        self,
    ) -> Result<SourceMainNativeRetainedConsensusV1, Box<RejectedNativeMainRetainedConsensusV1>>
    {
        let checked = (|| {
            self.owner
                .index
                .snapshot()
                .map_err(|e| NativeMainRetainedConsensusErrorV1::Backend(e.to_string()))?;
            let caller = self
                .frame
                .as_deref()
                .ok_or(NativeMainRetainedConsensusErrorV1::InvalidInput)?;
            self.owner
                .financial
                .runtime()
                .native_main_retained_consensus_begin(caller, self.documents.clone())
                .map_err(NativeMainRetainedConsensusErrorV1::Local)
                .and_then(|service| {
                    NativeMainRetainedCallerRouteV1::from_actual_caller(&self)
                        .map(|route| (service, route))
                        .map_err(NativeMainRetainedConsensusErrorV1::Local)
                })
        })();
        match checked {
            Ok((service, route)) => {
                let attempted = self
                    .frame
                    .as_ref()
                    .and_then(|f| f.native_main_retained_consensus_attempted())
                    .map(|p| p.bind_route(&route));
                Ok(SourceMainNativeRetainedConsensusV1 {
                    caller: self,
                    service: Box::new(service),
                    attempted,
                    route,
                })
            }
            Err(error) => Err(Box::new(RejectedNativeMainRetainedConsensusV1 {
                error,
                caller: self,
            })),
        }
    }
}
impl SourceMainNativeRetainedConsensusV1 {
    pub fn caller_result(&self) -> Option<&DependencyReturn> {
        self.caller.caller_result()
    }
    pub fn effects(&self) -> &[V] {
        self.caller.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.caller.financial_state()
    }
    pub fn pending_operations(&self) -> Vec<NativeMainRetainedConsensusPendingV1> {
        let Some(caller) = self.caller.frame.as_deref() else {
            return vec![];
        };
        self.caller
            .owner
            .financial
            .runtime()
            .native_main_retained_consensus_pending(caller, &self.service, &self.route)
    }
    pub fn advance_consensus(
        &mut self,
        limit: usize,
    ) -> Result<NativeMainRetainedConsensusStopV1, NativeMainRetainedConsensusErrorV1> {
        let caller = self
            .caller
            .frame
            .as_deref_mut()
            .ok_or(NativeMainRetainedConsensusErrorV1::InvalidInput)?;
        let (stop, returned) = self
            .caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_advance(
                &mut self.caller.owner.index,
                caller,
                &mut self.service,
                limit,
            )
            .map_err(NativeMainRetainedConsensusErrorV1::Local)?;
        Ok(if returned {
            NativeMainRetainedConsensusStopV1::CallerReturned
        } else {
            NativeMainRetainedConsensusStopV1::Service(stop)
        })
    }
    pub fn advance_detached(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<NativeMainRetainedConsensusStopV1, NativeMainRetainedConsensusErrorV1> {
        if operation.kind() != NativeMainRetainedConsensusOperationKindV1::CallerInline
            || !self.pending_operations().contains(operation)
        {
            return Err(NativeMainRetainedConsensusErrorV1::InvalidInput);
        }
        if self.service.pending_native_context().is_some() {
            return Err(NativeMainRetainedConsensusErrorV1::PendingOtherOperation);
        }
        let caller = self
            .caller
            .frame
            .as_deref_mut()
            .ok_or(NativeMainRetainedConsensusErrorV1::InvalidInput)?;
        let stop = self
            .caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_detached(
                &mut self.caller.owner.index,
                caller,
                &mut self.service,
                &self.caller.documents,
            )
            .map_err(NativeMainRetainedConsensusErrorV1::Local)?;
        Ok(NativeMainRetainedConsensusStopV1::Detached(stop))
    }
    pub fn append_pending(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<
        crate::source_contract_runtime::NativeMainServiceStopV1,
        NativeMainRetainedConsensusErrorV1,
    > {
        if self.attempted.is_some() {
            return Err(NativeMainRetainedConsensusErrorV1::WriteAlreadyAttempted);
        }
        if !self.pending_operations().contains(operation)
            || operation.value().get("context") == V::Null
        {
            return Err(NativeMainRetainedConsensusErrorV1::InvalidInput);
        }
        // Retain the actual selected attempt BEFORE native IO, including errors.
        self.attempted = Some(operation.clone());
        self.caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_append(
                &mut self.caller.owner.index,
                self.caller
                    .frame
                    .as_deref_mut()
                    .ok_or(NativeMainRetainedConsensusErrorV1::InvalidInput)?,
                &mut self.service,
                operation,
                &self.route,
                permit,
            )
            .map_err(NativeMainRetainedConsensusErrorV1::Local)
    }
    pub fn resolve_post(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<
        crate::source_contract_runtime::NativeMainServiceStopV1,
        NativeMainRetainedConsensusErrorV1,
    > {
        if self.attempted.as_ref() != Some(operation) {
            return Err(NativeMainRetainedConsensusErrorV1::NoAttemptedWrite);
        }
        let stop = self
            .caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_resolve(
                &self.caller.owner.index,
                self.caller
                    .frame
                    .as_deref_mut()
                    .ok_or(NativeMainRetainedConsensusErrorV1::InvalidInput)?,
                &mut self.service,
                operation,
                &self.route,
            )
            .map_err(NativeMainRetainedConsensusErrorV1::Local)?;
        if !self.pending_operations().contains(operation) {
            self.attempted = None;
        }
        Ok(stop)
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.service.next_timer()
    }
    pub fn fire_timer(
        &mut self,
        id: u64,
        generation: u64,
    ) -> Result<(), crate::source_main_step_kernel::MainStepError> {
        self.caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_control(
                &mut self.caller.owner.index,
                &mut self.service,
                crate::source_main_step_kernel::MainStepCommandV1::FireTimer { id, generation },
            )
    }
    pub fn pause(&mut self) -> Result<DependencyReturn, SourceRuleError> {
        self.caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_pause(&self.service)
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-RETAINED-CONSENSUS-SOURCE-13-1"))?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "actualRoute",
            self.route.value(),
            visitor,
        )?;
        self.caller
            .visit_canonical_fields(&mut |p, v| visitor(&format!("caller/{p}"), v))?;
        self.service
            .visit_canonical_fields(&mut |p, v| visitor(&format!("service/{p}"), v))?;
        visitor("attempted/present", &V::Bool(self.attempted.is_some()))?;
        if let Some(operation) = &self.attempted {
            crate::source_contract_runtime::native_main_row_visit_value(
                "attempted/operation",
                &operation.value(),
                visitor,
            )?;
        }
        let pending = self.pending_operations();
        visitor("pending/count", &s(&pending.len().to_string()))?;
        for (at, p) in pending.iter().enumerate() {
            crate::source_contract_runtime::native_main_row_visit_value(
                &format!("pending/{at}"),
                &p.value(),
                visitor,
            )?;
        }
        Ok(())
    }
}
