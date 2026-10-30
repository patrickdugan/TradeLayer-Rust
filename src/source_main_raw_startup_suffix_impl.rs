// WIP explicit actual-index suffix. No old constructor/Session/head exposure.
pub const INDEX_SUFFIX_PROFILE: &str = "TL-MAIN-RAW-STARTUP-INDEX-SUFFIX-13-1";
pub struct SourceMainRawStartupIndexSuffixV1 {
    startup: SourceMainRawStartupV1,
    frame: Option<Box<crate::source_contract_runtime::NativeMainRetainedCallerFrameV1>>,
    documents: V,
    production_io: V,
    scope: u64,
    start_error: Option<SourceRuleError>,
    service: Option<Box<crate::source_contract_runtime::NativeMainServiceContinuationV1>>,
}
pub struct RejectedNativeRawStartupSuffixV1 {
    pub error: SourceRuleError,
    startup: SourceMainRawStartupV1,
}
impl core::fmt::Debug for RejectedNativeRawStartupSuffixV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeRawStartupSuffixV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeRawStartupSuffixV1 {
    pub fn into_startup(self) -> SourceMainRawStartupV1 {
        self.startup
    }
}
impl SourceMainRawStartupV1 {
    pub fn into_initialize_index_suffix(
        mut self,
        scope: u64,
        documents: V,
        production_io: V,
    ) -> Result<SourceMainRawStartupIndexSuffixV1, Box<RejectedNativeRawStartupSuffixV1>> {
        let preflight = (|| {
            let prefix = self.prefix.as_ref().ok_or(SourceRuleError::InvalidInput)?;
            self.financial
                .runtime()
                .native_main_raw_startup_suffix_preflight(
                    prefix,
                    &self.io,
                    scope,
                    &documents,
                    &production_io,
                )
        })();
        if let Err(error) = preflight {
            return Err(Box::new(RejectedNativeRawStartupSuffixV1 {
                error,
                startup: self,
            }));
        }
        let begun = self
            .financial
            .native_main_runtime_mut()
            .native_main_raw_startup_suffix_begin(
                &mut self.index,
                &mut self.io,
                scope,
                self.generation,
                production_io.clone(),
            );
        let (frame, start_error) = match begun {
            Ok(value) => (Some(Box::new(value)), None),
            Err(error) => (None, Some(error)),
        };
        Ok(SourceMainRawStartupIndexSuffixV1 {
            startup: self,
            frame,
            documents,
            production_io,
            scope,
            start_error,
            service: None,
        })
    }
}
impl SourceMainRawStartupIndexSuffixV1 {
    pub fn profile(&self) -> &'static str {
        INDEX_SUFFIX_PROFILE
    }
    pub fn advance(&mut self) -> crate::source_contract_runtime::NativeMainRetainedCallerStopV1 {
        use crate::source_contract_runtime::{NativeMainRetainedCallerStopV1, NativeMainRowStopV1};
        let Some(frame) = self.frame.as_deref_mut() else {
            if let Some(result) = self.startup.io.main_outcome() {
                return match result {
                    Ok(_) => NativeMainRetainedCallerStopV1::CallerReturned,
                    Err(error) => NativeMainRetainedCallerStopV1::Native(
                        NativeMainRowStopV1::SourceStopped(error.clone()),
                    ),
                };
            }
            return NativeMainRetainedCallerStopV1::Native(NativeMainRowStopV1::SourceStopped(
                self.start_error
                    .clone()
                    .unwrap_or(SourceRuleError::InvalidInput),
            ));
        };
        self.startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_advance(&mut self.startup.index, frame, &self.documents)
    }
    /// Executes the existing ACTUAL catchup/consensus/service body on the same
    /// retained Main invocation. Ready is not converted into a supplied result.
    pub fn advance_consensus(
        &mut self,
        limit: usize,
    ) -> Result<
        (
            crate::source_contract_runtime::NativeMainServiceStopV1,
            bool,
        ),
        SourceRuleError,
    > {
        let frame = self
            .frame
            .as_deref_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        if self.service.is_none() {
            let service = self
                .startup
                .financial
                .runtime()
                .native_main_retained_consensus_begin(frame, self.documents.clone())?;
            self.service = Some(Box::new(service));
        }
        self.startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_consensus_advance(
                &mut self.startup.index,
                frame,
                self.service
                    .as_deref_mut()
                    .expect("same actual service frame"),
                limit,
            )
    }
    pub fn pending_descriptor(
        &self,
    ) -> Option<crate::source_contract_runtime::NativeMainRetainedCallerPendingV1> {
        self.frame.as_ref()?.pending_descriptor()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.frame.as_ref()?.pending_write()
    }
    pub fn append_pending(
        &mut self,
        descriptor: &crate::source_contract_runtime::NativeMainRetainedCallerPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        let Some(frame) = self.frame.as_deref_mut() else {
            return crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
                SourceRuleError::InvalidInput,
            );
        };
        self.startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_append(&mut self.startup.index, frame, descriptor, permit)
    }
    pub fn resolve_post(
        &mut self,
        descriptor: &crate::source_contract_runtime::NativeMainRetainedCallerPendingV1,
    ) -> crate::source_contract_runtime::NativeMainRowStopV1 {
        let Some(frame) = self.frame.as_deref_mut() else {
            return crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
                SourceRuleError::InvalidInput,
            );
        };
        self.startup
            .financial
            .native_main_runtime_mut()
            .native_main_retained_caller_resolve(&self.startup.index, frame, descriptor)
    }
    pub fn caller_result(&self) -> Option<&crate::validity_source::DependencyReturn> {
        if let Some(frame) = &self.frame {
            return frame.caller_result();
        }
        self.startup.io.main_outcome()?.as_ref().ok()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.startup.financial_state()
    }
    pub fn effects(&self) -> &[V] {
        self.startup.effects()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor(
            "indexSuffix/profile",
            &V::String(INDEX_SUFFIX_PROFILE.into()),
        )?;
        visitor("indexSuffix/scope", &V::String(self.scope.to_string()))?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "indexSuffix/documents",
            &self.documents,
            visitor,
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "indexSuffix/productionIo",
            &self.production_io,
            visitor,
        )?;
        visitor(
            "indexSuffix/startError",
            &self
                .start_error
                .as_ref()
                .map_or(V::Null, |e| V::String(format!("{e:?}"))),
        )?;
        self.startup.visit_canonical_fields(visitor)?;
        visitor(
            "indexSuffix/service/present",
            &V::Bool(self.service.is_some()),
        )?;
        if let Some(service) = &self.service {
            service.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("indexSuffix/service/{path}"), value)
            })?;
        }

        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("indexSuffix/frame/{path}"), value)
            })?;
        }
        Ok(())
    }
}
