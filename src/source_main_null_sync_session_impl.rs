fn null_sync_text(value: impl Into<String>) -> V {
    V::String(value.into())
}
pub use crate::source_contract_runtime::{
    NativeMainNullIoProfileV1, NativeMainNullJobV1, NativeMainNullStopV1,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NullPendingKind {
    Caller(NativeMainRetainedCallerPendingV1),
    Service(NativeMainRetainedConsensusPendingV1),
}
/// Receiver-bound route/selector for an actual retained operation, not actor
/// authorization. No public constructor accepts an expected operation result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainNullPendingV1 {
    job: NativeMainNullJobV1,
    kind: NullPendingKind,
    route: NativeMainRetainedCallerRouteV1,
}
impl NativeMainNullPendingV1 {
    pub fn job(&self) -> NativeMainNullJobV1 {
        self.job
    }
    pub fn value(&self) -> V {
        let operation = match &self.kind {
            NullPendingKind::Caller(c) => c.value(),
            NullPendingKind::Service(s) => s.value(),
        };
        obj(&[
            ("profile", null_sync_text("TL-MAIN-NULL-SYNC-PENDING-13-1")),
            ("job", null_sync_text(format!("{:?}", self.job))),
            (
                "role",
                if self.job == NativeMainNullJobV1::DetachedInit {
                    null_sync_text("Detached")
                } else {
                    operation.get("role")
                },
            ),
            ("operation", operation),
            ("route", self.route.value().clone()),
        ])
    }
}
pub struct SourceMainNativeNullSyncV1 {
    owner: SourceMainNativeSessionV13,
    frame: Option<Box<crate::source_contract_runtime::NativeMainNullSyncFrameV1>>,
    source_start_error: Option<SourceRuleError>,
    documents: V,
    route: NativeMainRetainedCallerRouteV1,
}
impl SourceMainNativeSessionV13 {
    pub fn into_retained_null_sync(
        mut self,
        expected: &NativeMainSessionExpectation,
        scope: u64,
        generation: u64,
        documents: V,
        io: NativeMainNullIoProfileV1,
    ) -> Result<SourceMainNativeNullSyncV1, Box<RejectedNativeMainRetainedCallerV1>> {
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
                .native_main_null_preflight(&documents)
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            let actual = self
                .index
                .capture_verified_history_pin()
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            Ok(NativeMainRetainedCallerRouteV1 {
                value: obj(&[
                    (
                        "profile",
                        null_sync_text("TL-MAIN-NULL-SYNC-ACTUAL-ROUTE-13-1"),
                    ),
                    (
                        "entryHead",
                        null_sync_text(hex(&expected.session.commitment)),
                    ),
                    (
                        "entryCommands",
                        null_sync_text(expected.session.commands.to_string()),
                    ),
                    ("entryBefore", physical_value(&expected.physical)),
                    (
                        "actualNativeHistory",
                        null_sync_text(hex(&actual
                            .canonical_scalar_bytes()
                            .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?)),
                    ),
                    ("originalPhysical", physical_value(&self.origin_physical)),
                    (
                        "foundationHead",
                        null_sync_text(hex(&self.foundation_head.commitment)),
                    ),
                    (
                        "actualFoundation",
                        null_sync_text(hex(&sha256(&self.foundation))),
                    ),
                    ("scope", null_sync_text(scope.to_string())),
                    ("generation", null_sync_text(generation.to_string())),
                ]),
            })
        })();
        let route = match checked {
            Ok(route) => route,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainRetainedCallerV1 {
                    error,
                    owner: self,
                }))
            }
        };
        let begun = self
            .financial
            .native_main_runtime_mut()
            .native_main_null_begin(&mut self.index, scope, generation, &documents, io);
        let (frame, source_start_error) = match begun {
            Ok(frame) => (Some(Box::new(frame)), None),
            Err(error) => (None, Some(error)),
        };
        Ok(SourceMainNativeNullSyncV1 {
            owner: self,
            frame,
            source_start_error,
            documents,
            route,
        })
    }
}
impl SourceMainNativeNullSyncV1 {
    pub fn profile(&self) -> &'static str {
        crate::source_contract_runtime::NATIVE_MAIN_NULL_SYNC_PROFILE
    }
    pub fn caller_result(&self) -> Option<&DependencyReturn> {
        self.frame.as_ref()?.caller_result()
    }
    pub fn detached_result(&self) -> Option<&DependencyReturn> {
        self.frame.as_ref()?.child_result()
    }
    pub fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.owner.financial.runtime().state_value()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.owner.index
    }
    pub fn advance(&mut self, limit: usize) -> NativeMainNullStopV1 {
        if let Some(error) = &self.source_start_error {
            return NativeMainNullStopV1::Stopped(error.clone());
        }
        let Some(frame) = self.frame.as_deref_mut() else {
            return NativeMainNullStopV1::Stopped(SourceRuleError::InvalidInput);
        };
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_null_drive(&mut self.owner.index, frame, &self.documents, limit)
    }
    pub fn release_detached_read(&mut self) -> Result<(), SourceRuleError> {
        let frame = self
            .frame
            .as_deref_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_null_release_read(frame)
    }
    pub fn release_parent_tip(&mut self) -> Result<(), SourceRuleError> {
        let frame = self
            .frame
            .as_deref_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_null_release_tip(frame)
    }
    pub fn pending_operations(&self) -> Vec<NativeMainNullPendingV1> {
        let Some(frame) = self.frame.as_deref() else {
            return vec![];
        };
        self.owner
            .financial
            .runtime()
            .native_main_null_pending(frame, &self.route)
            .into_iter()
            .map(|(job, kind)| NativeMainNullPendingV1 {
                job,
                kind,
                route: self.route.clone(),
            })
            .collect()
    }
    pub fn append_pending(
        &mut self,
        operation: &NativeMainNullPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<crate::source_contract_runtime::NativeMainServiceStopV1, SourceRuleError> {
        if !self.pending_operations().contains(operation) {
            return Err(SourceRuleError::InvalidInput);
        }
        let frame = self
            .frame
            .as_deref_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_null_append(
                &mut self.owner.index,
                frame,
                operation.job,
                &operation.kind,
                &self.route,
                permit,
            )
    }
    pub fn resolve_post(
        &mut self,
        operation: &NativeMainNullPendingV1,
    ) -> Result<crate::source_contract_runtime::NativeMainServiceStopV1, SourceRuleError> {
        if !self.pending_operations().contains(operation) {
            return Err(SourceRuleError::InvalidInput);
        }
        let frame = self
            .frame
            .as_deref_mut()
            .ok_or(SourceRuleError::InvalidInput)?;
        self.owner
            .financial
            .native_main_runtime_mut()
            .native_main_null_resolve(
                &self.owner.index,
                frame,
                operation.job,
                &operation.kind,
                &self.route,
            )
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &null_sync_text(self.profile()))?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "actualRoute",
            self.route.value(),
            visitor,
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "rawDocuments",
            &self.documents,
            visitor,
        )?;
        visitor(
            "sourceStartError",
            &self
                .source_start_error
                .as_ref()
                .map_or(V::Null, |e| null_sync_text(format!("{e:?}"))),
        )?;
        if let Some(frame) = &self.frame {
            frame.visit_fields(visitor)?;
        }
        let current = physical(&self.owner.index).map_err(|_| SourceRuleError::Backend)?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "currentPhysical",
            &physical_value(&current),
            visitor,
        )?;
        visitor(
            "financialCommitment",
            &null_sync_text(hex(&self
                .owner
                .financial
                .native_main_financial_commitment()
                .map_err(|_| SourceRuleError::Backend)?)),
        )?;
        for (at, effect) in self.effects().iter().enumerate() {
            crate::source_contract_runtime::native_main_row_visit_value(
                &format!("effects/{at}"),
                effect,
                visitor,
            )?;
        }
        Ok(())
    }
}
