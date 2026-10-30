pub const LITERAL_STARTUP_NESTED_PROFILE: &str = "TL-MAIN-RAW-STARTUP-NESTED-CONSENSUS-13-1";
pub struct SourceMainRawStartupLiteralIndexSuffixV1 {
    suffix: SourceMainRawStartupIndexSuffixV1,
    nested_consensus: bool,
}
pub struct SourceMainRawStartupLiteralInitializeV1 {
    live: SourceMainRawStartupLiveInitializeV1,
    nested: crate::source_contract_runtime::NativeMainStartupNestedConsensusV1,
    attempted: Option<crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1>,
}
pub struct RejectedLiteralStartupSuffixV1 {
    pub error: SourceRuleError,
    suffix: SourceMainRawStartupIndexSuffixV1,
}
impl core::fmt::Debug for RejectedLiteralStartupSuffixV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedLiteralStartupSuffixV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedLiteralStartupSuffixV1 {
    pub fn into_suffix(self) -> SourceMainRawStartupIndexSuffixV1 {
        self.suffix
    }
}
pub struct RejectedLiteralStartupLiveV1 {
    pub error: SourceRuleError,
    suffix: SourceMainRawStartupLiteralIndexSuffixV1,
}
impl core::fmt::Debug for RejectedLiteralStartupLiveV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedLiteralStartupLiveV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedLiteralStartupLiveV1 {
    pub fn into_literal_suffix(self) -> SourceMainRawStartupLiteralIndexSuffixV1 {
        self.suffix
    }
}
impl SourceMainRawStartupIndexSuffixV1 {
    pub fn into_literal_source(
        self,
    ) -> Result<SourceMainRawStartupLiteralIndexSuffixV1, Box<RejectedLiteralStartupSuffixV1>> {
        if self.service.is_some()
            || self.start_error.is_some()
            || self.frame.as_ref().is_none_or(|frame| {
                !self
                    .startup
                    .financial
                    .runtime()
                    .native_main_startup_literal_tracking_ready(frame)
            })
        {
            return Err(Box::new(RejectedLiteralStartupSuffixV1 {
                error: SourceRuleError::InvalidInput,
                suffix: self,
            }));
        }
        Ok(SourceMainRawStartupLiteralIndexSuffixV1 {
            suffix: self,
            nested_consensus: false,
        })
    }
}
impl SourceMainRawStartupLiteralIndexSuffixV1 {
    pub fn advance(&mut self) -> crate::source_contract_runtime::NativeMainRetainedCallerStopV1 {
        let Some(frame) = self.suffix.frame.as_deref_mut() else {
            return self.suffix.advance();
        };
        self.suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_startup_tracked_caller_advance(
                &mut self.suffix.startup.index,
                frame,
                &self.suffix.documents,
                &mut self.nested_consensus,
            )
    }
    pub fn pending_descriptor(
        &self,
    ) -> Option<crate::source_contract_runtime::NativeMainRetainedCallerPendingV1> {
        self.suffix.pending_descriptor()
    }
    pub fn append_pending(
        &mut self,
        operation: &crate::source_contract_runtime::NativeMainRetainedCallerPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        self.suffix.append_pending(operation, permit)
    }
    pub fn resolve_post(
        &mut self,
        operation: &crate::source_contract_runtime::NativeMainRetainedCallerPendingV1,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        self.suffix.resolve_post(operation)
    }
    pub fn into_live_initialize(
        self,
    ) -> Result<SourceMainRawStartupLiteralInitializeV1, Box<RejectedLiteralStartupLiveV1>> {
        let Self {
            suffix,
            nested_consensus,
        } = self;
        match suffix.into_live_initialize() {
            Ok(live) => Ok(SourceMainRawStartupLiteralInitializeV1 {
                live,
                nested: crate::source_contract_runtime::NativeMainStartupNestedConsensusV1::new(
                    nested_consensus,
                ),
                attempted: None,
            }),
            Err(rejected) => {
                let error = rejected.error.clone();
                Err(Box::new(RejectedLiteralStartupLiveV1 {
                    error,
                    suffix: Self {
                        suffix: rejected.into_suffix(),
                        nested_consensus,
                    },
                }))
            }
        }
    }
}
impl SourceMainRawStartupLiteralInitializeV1 {
    pub fn profile(&self) -> &'static str {
        LITERAL_STARTUP_NESTED_PROFILE
    }
    pub fn caller_result(&self) -> Option<&crate::validity_source::DependencyReturn> {
        self.live.caller_result()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.live.financial_state()
    }
    pub fn effects(&self) -> &[V] {
        self.live.effects()
    }
    pub fn pending_operations(
        &self,
    ) -> Vec<crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1> {
        let Some(frame) = self.live.suffix.frame.as_deref() else {
            return vec![];
        };
        self.live
            .suffix
            .startup
            .financial
            .runtime()
            .native_main_startup_nested_pending(frame, &self.live.service, &self.live.route)
    }
    pub fn advance(
        &mut self,
        limit: usize,
    ) -> Result<
        (
            crate::source_contract_runtime::NativeMainServiceStopV1,
            bool,
        ),
        SourceRuleError,
    > {
        if self.attempted.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        self.live
            .suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_startup_nested_advance(
                &mut self.live.suffix.startup.index,
                self.live
                    .suffix
                    .frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                &mut self.live.service,
                &mut self.nested,
                limit,
            )
    }
    pub fn append_pending(
        &mut self,
        operation: &crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<crate::source_contract_runtime::NativeMainServiceStopV1, SourceRuleError> {
        if self.attempted.is_some()
            || !self.pending_operations().contains(operation)
            || operation.value().get("context") == V::Null
        {
            return Err(SourceRuleError::InvalidInput);
        }
        self.attempted = Some(operation.clone());
        self.live
            .suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_startup_nested_append(
                &mut self.live.suffix.startup.index,
                self.live
                    .suffix
                    .frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                &mut self.live.service,
                &self.live.route,
                operation,
                permit,
            )
    }
    pub fn resolve_post(
        &mut self,
        operation: &crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1,
    ) -> Result<crate::source_contract_runtime::NativeMainServiceStopV1, SourceRuleError> {
        if self.attempted.as_ref() != Some(operation) {
            return Err(SourceRuleError::InvalidInput);
        }
        let stop = self
            .live
            .suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_startup_nested_resolve(
                &self.live.suffix.startup.index,
                self.live
                    .suffix
                    .frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                &mut self.live.service,
                &self.live.route,
                &mut self.nested,
                operation,
            )?;
        if !self.pending_operations().contains(operation) {
            self.attempted = None;
        }
        Ok(stop)
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.live.next_timer()
    }
    pub fn fire_timer(
        &mut self,
        id: u64,
        generation: u64,
    ) -> Result<(), crate::source_main_step_kernel::MainStepError> {
        if self.attempted.is_some() {
            return Err(crate::source_main_step_kernel::MainStepError::OwnerStopped);
        }
        self.live.fire_timer(id, generation)
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor(
            "literalStartup/profile",
            &V::String(LITERAL_STARTUP_NESTED_PROFILE.into()),
        )?;
        self.live.visit_canonical_fields(visitor)?;
        self.nested
            .visit_fields(&mut |p, v| visitor(&format!("literalStartup/nested/{p}"), v))?;
        visitor(
            "literalStartup/attempted/present",
            &V::Bool(self.attempted.is_some()),
        )?;
        if let Some(operation) = &self.attempted {
            crate::source_contract_runtime::native_main_row_visit_value(
                "literalStartup/attempted/value",
                &operation.value(),
                visitor,
            )?;
        }
        Ok(())
    }
}
