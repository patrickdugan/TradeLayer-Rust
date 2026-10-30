/// Explicit primitive-coercive caller scope. No old caller/WAL constructor
/// selects this entry implicitly and no ordinary caller/session is released.
pub struct SourceMainNativeCoerciveSyncV1 {
    caller: SourceMainNativeRetainedCallerV1,
}
pub struct SourceMainNativeCoerciveConsensusV1 {
    consensus: SourceMainNativeRetainedConsensusV1,
}
pub struct RejectedNativeMainCoerciveConsensusV1 {
    pub error: NativeMainRetainedConsensusErrorV1,
    caller: SourceMainNativeCoerciveSyncV1,
}
impl core::fmt::Debug for RejectedNativeMainCoerciveConsensusV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainCoerciveConsensusV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainCoerciveConsensusV1 {
    pub fn into_caller(self) -> SourceMainNativeCoerciveSyncV1 {
        self.caller
    }
}
impl SourceMainNativeSessionV13 {
    pub fn into_retained_coercive_sync(
        mut self,
        expected: &NativeMainSessionExpectation,
        max: V,
        scope: u64,
        generation: u64,
        documents: V,
    ) -> Result<SourceMainNativeCoerciveSyncV1, Box<RejectedNativeMainRetainedCallerV1>> {
        let checked = (|| {
            if self.halted {
                return Err(NativeMainSessionError::Halted);
            }
            if self.expectation()? != *expected {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            if scope == 0 || scope > u64::MAX - 4096 {
                return Err(NativeMainSessionError::Capacity);
            }
            self.financial
                .runtime()
                .native_main_coercive_sync_preflight(&max, &documents)
                .map_err(|error| NativeMainSessionError::Backend(error.to_string()))
        })();
        if let Err(error) = checked {
            return Err(Box::new(RejectedNativeMainRetainedCallerV1 {
                error,
                owner: self,
            }));
        }
        let raw_max = max.clone();
        let begun = self
            .financial
            .native_main_runtime_mut()
            .native_main_coercive_sync_begin(&mut self.index, max, scope, generation, &documents);
        let (frame, source_start_error) = match begun {
            Ok(frame) => {
                debug_assert!(frame.native_main_coercive_sync_enabled());
                (Some(Box::new(frame)), None)
            }
            Err(error) => (None, Some(error)),
        };
        Ok(SourceMainNativeCoerciveSyncV1 {
            caller: SourceMainNativeRetainedCallerV1 {
                owner: self,
                frame,
                source_start_error,
                documents,
                origin: expected.clone(),
                command: NativeMainRetainedCallerV1::SyncIndex { max: raw_max },
                scope,
                generation,
            },
        })
    }
}
impl SourceMainNativeCoerciveSyncV1 {
    pub fn profile(&self) -> &'static str {
        crate::source_contract_runtime::NATIVE_MAIN_COERCIVE_SYNC_PROFILE
    }
    pub fn advance(&mut self) -> NativeMainRetainedCallerStopV1 {
        self.caller.advance()
    }
    pub fn caller_result(&self) -> Option<&DependencyReturn> {
        self.caller.caller_result()
    }
    pub fn pending_descriptor(&self) -> Option<NativeMainRetainedCallerPendingV1> {
        self.caller.pending_descriptor()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.caller.pending_write()
    }
    pub fn append_pending(
        &mut self,
        selector: &NativeMainRetainedCallerPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        self.caller.append_pending(selector, permit)
    }
    pub fn resolve_post(
        &mut self,
        selector: &NativeMainRetainedCallerPendingV1,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        self.caller.resolve_post(selector)
    }
    pub fn effects(&self) -> &[V] {
        self.caller.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.caller.financial_state()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("coercive/profile", &s(self.profile()))?;
        self.caller.visit_canonical_fields(visitor)
    }
    pub fn into_consensus(
        self,
    ) -> Result<SourceMainNativeCoerciveConsensusV1, Box<RejectedNativeMainCoerciveConsensusV1>>
    {
        match self.caller.into_retained_consensus() {
            Ok(consensus) => Ok(SourceMainNativeCoerciveConsensusV1 { consensus }),
            Err(rejected) => {
                let RejectedNativeMainRetainedConsensusV1 { error, caller } = *rejected;
                Err(Box::new(RejectedNativeMainCoerciveConsensusV1 {
                    error,
                    caller: SourceMainNativeCoerciveSyncV1 { caller },
                }))
            }
        }
    }
}
impl SourceMainNativeCoerciveConsensusV1 {
    pub fn profile(&self) -> &'static str {
        crate::source_contract_runtime::NATIVE_MAIN_COERCIVE_SYNC_PROFILE
    }
    pub fn caller_result(&self) -> Option<&DependencyReturn> {
        self.consensus.caller_result()
    }
    pub fn pending_operations(&self) -> Vec<NativeMainRetainedConsensusPendingV1> {
        self.consensus.pending_operations()
    }
    pub fn advance_consensus(
        &mut self,
        limit: usize,
    ) -> Result<NativeMainRetainedConsensusStopV1, NativeMainRetainedConsensusErrorV1> {
        self.consensus.advance_consensus(limit)
    }
    pub fn advance_detached(
        &mut self,
        selector: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<NativeMainRetainedConsensusStopV1, NativeMainRetainedConsensusErrorV1> {
        self.consensus.advance_detached(selector)
    }
    pub fn append_pending(
        &mut self,
        selector: &NativeMainRetainedConsensusPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<
        crate::source_contract_runtime::NativeMainServiceStopV1,
        NativeMainRetainedConsensusErrorV1,
    > {
        self.consensus.append_pending(selector, permit)
    }
    pub fn resolve_post(
        &mut self,
        selector: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<
        crate::source_contract_runtime::NativeMainServiceStopV1,
        NativeMainRetainedConsensusErrorV1,
    > {
        self.consensus.resolve_post(selector)
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.consensus.next_timer()
    }
    pub fn fire_timer(
        &mut self,
        id: u64,
        generation: u64,
    ) -> Result<(), crate::source_main_step_kernel::MainStepError> {
        self.consensus.fire_timer(id, generation)
    }
    pub fn pause(&mut self) -> Result<DependencyReturn, SourceRuleError> {
        self.consensus.pause()
    }
    pub fn effects(&self) -> &[V] {
        self.consensus.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.consensus.financial_state()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("coercive/profile", &s(self.profile()))?;
        self.consensus.visit_canonical_fields(visitor)
    }
}
