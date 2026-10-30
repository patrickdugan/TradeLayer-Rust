// Source-only retained caller owner. This entry never releases the old Session,
// drains financial effects, publishes a legacy head or admits a WAL selector.
pub use crate::source_contract_runtime::{
    NativeMainRetainedCallerPendingV1, NativeMainRetainedCallerStopV1, NativeMainRetainedCallerV1,
};
pub struct SourceMainNativeRetainedCallerV1 {
    owner: SourceMainNativeSessionV13,
    frame: Option<Box<crate::source_contract_runtime::NativeMainRetainedCallerFrameV1>>,
    source_start_error: Option<SourceRuleError>,
    documents: V,
    origin: NativeMainSessionExpectation,
    command: NativeMainRetainedCallerV1,
    scope: u64,
    generation: u64,
}
pub struct RejectedNativeMainRetainedCallerV1 {
    pub error: NativeMainSessionError,
    owner: SourceMainNativeSessionV13,
}
impl core::fmt::Debug for RejectedNativeMainRetainedCallerV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainRetainedCallerV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainRetainedCallerV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
impl SourceMainNativeSessionV13 {
    pub fn into_retained_main_caller(
        self,
        expected: &NativeMainSessionExpectation,
        command: NativeMainRetainedCallerV1,
        scope: u64,
        generation: u64,
        documents: V,
    ) -> Result<SourceMainNativeRetainedCallerV1, Box<RejectedNativeMainRetainedCallerV1>> {
        self.into_retained_main_caller_profile(
            expected, command, scope, generation, documents, false,
        )
    }
    fn into_retained_main_caller_profile(
        mut self,
        expected: &NativeMainSessionExpectation,
        command: NativeMainRetainedCallerV1,
        scope: u64,
        generation: u64,
        documents: V,
        file_retries: bool,
    ) -> Result<SourceMainNativeRetainedCallerV1, Box<RejectedNativeMainRetainedCallerV1>> {
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
            let runtime = self.financial.runtime();
            (if file_retries {
                runtime.native_main_retained_file_preflight(&command, &documents)
            } else {
                runtime.native_main_retained_caller_preflight(&command, &documents)
            })
            .map_err(|e| NativeMainSessionError::Backend(e.to_string()))
        })();
        if let Err(error) = checked {
            return Err(Box::new(RejectedNativeMainRetainedCallerV1 {
                error,
                owner: self,
            }));
        }
        let raw_command = command.clone();
        let begun = self
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_begin(
                &mut self.index,
                command,
                scope,
                generation,
                &documents,
            );
        let (frame, source_start_error) = match begun {
            Ok(frame) => (Some(Box::new(frame)), None),
            Err(error) => (None, Some(error)),
        };
        Ok(SourceMainNativeRetainedCallerV1 {
            owner: self,
            frame,
            source_start_error,
            documents,
            origin: expected.clone(),
            command: raw_command,
            scope,
            generation,
        })
    }
}
impl SourceMainNativeRetainedCallerV1 {
    pub fn advance(&mut self) -> NativeMainRetainedCallerStopV1 {
        let Some(frame) = self.frame.as_deref_mut() else {
            return NativeMainRetainedCallerStopV1::Native(
                crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
                    self.source_start_error
                        .clone()
                        .unwrap_or(SourceRuleError::InvalidInput),
                ),
            );
        };
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_advance(&mut self.owner.index, frame, &self.documents)
    }
    pub fn caller_result(&self) -> Option<&DependencyReturn> {
        self.frame.as_ref()?.caller_result()
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.frame.as_ref()?.next_timer()
    }
    pub fn fire_timer(&mut self, id: u64, generation: u64) -> Result<(), SourceRuleError> {
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_fire(
                self.frame
                    .as_deref_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
                id,
                generation,
            )
    }
    pub fn pending_descriptor(&self) -> Option<NativeMainRetainedCallerPendingV1> {
        self.frame.as_ref()?.pending_descriptor()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.frame.as_ref()?.pending_write()
    }
    pub fn append_pending(
        &mut self,
        descriptor: &NativeMainRetainedCallerPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        let Some(frame) = self.frame.as_deref_mut() else {
            return crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
                SourceRuleError::InvalidInput,
            );
        };
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_append(&mut self.owner.index, frame, descriptor, permit)
    }
    pub fn resolve_post(
        &mut self,
        descriptor: &NativeMainRetainedCallerPendingV1,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        let Some(frame) = self.frame.as_deref_mut() else {
            return crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
                SourceRuleError::InvalidInput,
            );
        };
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_resolve(&self.owner.index, frame, descriptor)
    }
    pub fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.owner.financial.runtime().state_value()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("caller/present", &V::Bool(self.frame.is_some()))?;
        visitor("entry/command", &s(self.command.kind()))?;
        if let NativeMainRetainedCallerV1::SyncIndex { max } = &self.command {
            crate::source_contract_runtime::native_main_row_visit_value("entry/max", max, visitor)?;
        }
        visitor("entry/scope", &s(&self.scope.to_string()))?;
        visitor("entry/generation", &s(&self.generation.to_string()))?;
        visitor(
            "origin/commands",
            &s(&self.origin.session.commands.to_string()),
        )?;
        visitor("origin/head", &s(&hex(&self.origin.session.commitment)))?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "origin/physical",
            &physical_value(&self.origin.physical),
            visitor,
        )?;
        let current = physical(&self.owner.index).map_err(|_| SourceRuleError::Backend)?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "current/physical",
            &physical_value(&current),
            visitor,
        )?;
        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |p, v| visitor(&format!("caller/{p}"), v))?;
        }
        visitor(
            "sourceStartError",
            &self
                .source_start_error
                .as_ref()
                .map_or(V::Null, |e| s(&format!("{e:?}"))),
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "documents",
            &self.documents,
            visitor,
        )?;
        visitor("effects/count", &s(&self.effects().len().to_string()))?;
        for (at, effect) in self.effects().iter().enumerate() {
            crate::source_contract_runtime::native_main_row_visit_value(
                &format!("effects/{at}"),
                effect,
                visitor,
            )?;
        }
        visitor(
            "financialCommitment",
            &s(&hex(&self
                .owner
                .financial
                .native_main_financial_commitment()
                .map_err(|_| SourceRuleError::Backend)?)),
        )
    }
}
