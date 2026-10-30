// Explicit raw-startup source continuation. It owns the one original startup,
// index, borrowed invocation and real service frame. There is no financial
// owner/head/checkpoint release or automatic admission by an old profile.
pub const LIVE_INITIALIZE_PROFILE: &str = "TL-MAIN-RAW-STARTUP-LIVE-INITIALIZE-13-1";
pub struct SourceMainRawStartupLiveInitializeV1 {
    suffix: SourceMainRawStartupIndexSuffixV1,
    service: Box<crate::source_contract_runtime::NativeMainServiceContinuationV1>,
    route: crate::source_main_native_session::NativeMainRetainedCallerRouteV1,
    attempted: Option<crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1>,
}
pub struct RejectedNativeRawStartupLiveV1 {
    pub error: SourceRuleError,
    suffix: SourceMainRawStartupIndexSuffixV1,
}
impl core::fmt::Debug for RejectedNativeRawStartupLiveV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeRawStartupLiveV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeRawStartupLiveV1 {
    pub fn into_suffix(self) -> SourceMainRawStartupIndexSuffixV1 {
        self.suffix
    }
}
impl SourceMainRawStartupV1 {
    pub(crate) fn actual_live_route_fields(&self) -> Result<V, SourceRuleError> {
        let binding = self
            .index
            .native_observed_append_binding()
            .map_err(|_| SourceRuleError::Backend)?;
        Ok(V::Object(vec![
            (
                "foundationCommands".into(),
                V::String(self.foundation_head.commands.to_string()),
            ),
            (
                "foundationHead".into(),
                V::String(hex_startup(&self.foundation_head.commitment)),
            ),
            (
                "actualFoundationBytes".into(),
                V::String(hex_startup(&crate::hash256::sha256(&self.foundation))),
            ),
            ("actualMetadataEntry".into(), binding.value().clone()),
            (
                "declaredGeneration".into(),
                V::String(self.generation.to_string()),
            ),
        ]))
    }
}
fn hex_startup(bytes: &[u8]) -> String {
    use core::fmt::Write;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(value, "{byte:02x}").expect("String writing cannot fail");
    }
    value
}
impl SourceMainRawStartupIndexSuffixV1 {
    pub fn into_live_initialize(
        self,
    ) -> Result<SourceMainRawStartupLiveInitializeV1, Box<RejectedNativeRawStartupLiveV1>> {
        let checked = (|| {
            if self.service.is_some() || self.start_error.is_some() {
                return Err(SourceRuleError::InvalidInput);
            }
            let frame = self.frame.as_deref().ok_or(SourceRuleError::InvalidInput)?;
            let prefix = self
                .startup
                .prefix
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?;
            let service = self
                .startup
                .financial
                .runtime()
                .native_main_raw_startup_live_begin(frame, prefix, self.documents.clone())?;
            let route = crate::source_main_native_session::NativeMainRetainedCallerRouteV1::from_actual_raw_startup_suffix(&self.startup, frame)?;
            Ok((service, route))
        })();
        match checked {
            Ok((service, route)) => Ok(SourceMainRawStartupLiveInitializeV1 {
                suffix: self,
                service: Box::new(service),
                route,
                attempted: None,
            }),
            Err(error) => Err(Box::new(RejectedNativeRawStartupLiveV1 {
                error,
                suffix: self,
            })),
        }
    }
}
impl SourceMainRawStartupLiveInitializeV1 {
    pub fn profile(&self) -> &'static str {
        LIVE_INITIALIZE_PROFILE
    }
    pub fn caller_result(&self) -> Option<&crate::validity_source::DependencyReturn> {
        self.suffix.caller_result()
    }
    pub fn effects(&self) -> &[V] {
        self.suffix.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.suffix.financial_state()
    }
    pub fn pending_operations(
        &self,
    ) -> Vec<crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1> {
        let Some(frame) = self.suffix.frame.as_deref() else {
            return vec![];
        };
        self.suffix
            .startup
            .financial
            .runtime()
            .native_main_retained_consensus_pending(frame, &self.service, &self.route)
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
        self.suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_advance(
                &mut self.suffix.startup.index,
                self.suffix
                    .frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                &mut self.service,
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
        self.suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_append(
                &mut self.suffix.startup.index,
                self.suffix
                    .frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                &mut self.service,
                operation,
                &self.route,
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
            .suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_resolve(
                &self.suffix.startup.index,
                self.suffix
                    .frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                &mut self.service,
                operation,
                &self.route,
            )?;
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
        if self.attempted.is_some() {
            return Err(crate::source_main_step_kernel::MainStepError::OwnerStopped);
        }
        self.suffix
            .startup
            .financial
            .native_main_runtime_mut()
            .native_main_service_control(
                &mut self.suffix.startup.index,
                &mut self.service,
                crate::source_main_step_kernel::MainStepCommandV1::FireTimer { id, generation },
            )
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor(
            "liveInitialize/profile",
            &V::String(LIVE_INITIALIZE_PROFILE.into()),
        )?;
        self.suffix.visit_canonical_fields(visitor)?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "liveInitialize/actualRoute",
            self.route.value(),
            visitor,
        )?;
        self.service.visit_canonical_fields(&mut |p, v| {
            visitor(&format!("liveInitialize/service/{p}"), v)
        })?;
        visitor(
            "liveInitialize/attempted/present",
            &V::Bool(self.attempted.is_some()),
        )?;
        if let Some(attempted) = &self.attempted {
            crate::source_contract_runtime::native_main_row_visit_value(
                "liveInitialize/attempted/operation",
                &attempted.value(),
                visitor,
            )?;
        }
        Ok(())
    }
}
