// Same moved-owner recovery structure as the existing validity controller,
// with a separate inline profile and real inline metadata plan/candidate APIs.
pub struct SourceMainNativeDurableInlineNoIndexV1 {
    controller: SourceMainNativeDurableActiveServiceV1,
    service: ActiveServiceWithoutNativeIndex,
}
pub struct RejectedNativeMainInlineNoIndexV1 {
    pub cause: String,
    controller: SourceMainNativeDurableActiveServiceV1,
}
impl RejectedNativeMainInlineNoIndexV1 {
    pub fn into_controller(self) -> SourceMainNativeDurableActiveServiceV1 {
        self.controller
    }
}
impl core::fmt::Debug for RejectedNativeMainInlineNoIndexV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainInlineNoIndexV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedNativeMainInlineReopenV1 {
    pub cause: String,
    owner: SourceMainNativeDurableInlineNoIndexV1,
}
impl RejectedNativeMainInlineReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeDurableInlineNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainInlineReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainInlineReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeDurableActiveServiceV1 {
    pub fn into_no_inline_index(
        mut self,
    ) -> Result<SourceMainNativeDurableInlineNoIndexV1, Box<RejectedNativeMainInlineNoIndexV1>>
    {
        if !self.inline_service_enabled()
            || self
                .active_service()
                .and_then(|s| s.pending_inline_write())
                .is_none_or(|w| w.append_plan().is_none())
            || self.phase != NativeMainActiveWalPhaseV1::NativeRecoveryRequired
            || self
                .active_service()
                .is_none_or(|s| s.pending_inline_write().is_none())
            || !self.pending.as_ref().is_some_and(|p| {
                p.executed
                    && matches!(p.action, NativeMainActiveServiceActionV1::AppendPending)
                    && p.plan.is_none()
            })
        {
            return Err(Box::new(RejectedNativeMainInlineNoIndexV1 {
                cause: "unresolved actual native append required".into(),
                controller: self,
            }));
        }
        let ActiveWalOwner::Active(service) = self.owner.take().expect("checked actual owner")
        else {
            unreachable!("checked active")
        };
        let SourceMainNativeActiveServiceV1 {
            owner,
            frame,
            source_start_error,
            origin,
            head,
            history,
            pending_observation,
            attempted_write,
            observation_limit,
            inline_profile,
        } = *service;
        let SourceMainNativeSessionV13 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head: base_head,
            history: base_history,
            halted,
        } = owner;
        let namespace = index.native_namespace().clone();
        drop(index);
        Ok(SourceMainNativeDurableInlineNoIndexV1 {
            controller: self,
            service: ActiveServiceWithoutNativeIndex {
                financial: NativeMainFinancialWithoutIndexV1 {
                    financial,
                    foundation,
                    foundation_head,
                    origin_rows,
                    binding,
                    origin_physical,
                    head: base_head,
                    history: base_history,
                    halted,
                },
                namespace,
                frame,
                source_start_error,
                origin,
                head,
                history,
                pending_observation,
                attempted_write,
                observation_limit,
                inline_profile,
            },
        })
    }
}
impl SourceMainNativeDurableInlineNoIndexV1 {
    pub fn profile(&self) -> &'static str {
        "TL-MAIN-ACTIVE-INLINE-NATIVE-RECOVERY-13-1"
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.service.financial.financial.runtime()
    }
    pub fn effects(&self) -> &[V] {
        self.financial_runtime().native_main_row_effects()
    }
    pub fn pending_inline_write(
        &self,
    ) -> &crate::source_compact_index_v2::NativePreparedInlineDatabaseV1 {
        self.service
            .frame
            .as_ref()
            .and_then(|f| f.inline_pending_write())
            .expect("retained actual plan")
    }
    pub fn try_reopen(
        self,
        root: &std::path::Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<SourceMainNativeDurableActiveServiceV1, Box<RejectedNativeMainInlineReopenV1>> {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let checked: ActiveWalResult<(SourceCompactIndex, ActiveWalNativeRecovery)> = (|| {
            let (index, observation) = SourceCompactIndex::reopen_inline_database(
                root,
                self.service.namespace.clone(),
                self.pending_inline_write(),
                candidate,
                recovery,
            )
            .map_err(|e| active_wal_backend(e.to_string()))?;
            let mut proof = obj(&[
                ("profile", s(self.profile())),
                (
                    "candidate",
                    s(match candidate {
                        Candidate::Before => "Before",
                        Candidate::Intermediate => "Intermediate",
                        Candidate::Post => "Post",
                    }),
                ),
                (
                    "snapshot",
                    physical_value(
                        &physical(&index).map_err(|e| active_wal_backend(format!("{e:?}")))?,
                    ),
                ),
                (
                    "discardedBytes",
                    s(&observation.recovery_report().discarded_bytes.to_string()),
                ),
            ]);
            let previous = self
                .controller
                .pending
                .as_ref()
                .and_then(|p| p.native_recovery.as_ref())
                .map(|p| ActiveServiceWalBlobV1::of(&p.raw))
                .transpose()?;
            proof
                .insert(
                    "previousRecovery",
                    previous.as_ref().map_or(V::Null, |p| p.value()),
                )
                .map_err(active_wal_backend)?;

            let pin = index
                .capture_verified_history_pin()
                .map_err(|e| active_wal_backend(e.to_string()))?;
            let mut fields = NativeFieldCollector::new(
                "Native13/ActiveWalVerifiedHistory/v1",
                true,
                1024 * 1024,
            );
            pin.visit_canonical_fields(&mut |p, v| fields.emit(p, v))
                .map_err(active_wal_backend)?;
            proof
                .insert(
                    "verifiedReopenedHistory",
                    s(&hex(fields.frames.as_deref().expect("retained"))),
                )
                .map_err(active_wal_backend)?;
            let (replacement, mapping) = if candidate == Candidate::Post {
                index
                    .resolve_inline_database_post(self.pending_inline_write())
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                (None, None)
            } else {
                let (replacement, mapping) = index
                    .supersede_inline_database(self.pending_inline_write(), &observation)
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                let binding = index
                    .require_inline_native_supersession(
                        self.pending_inline_write(),
                        &observation,
                        &replacement,
                        &mapping,
                    )
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(replacement.append_plan().ok_or(NativeMainActiveWalErrorV1::WrongAction)?,replacement.durable_read_context()).map_err(|e|active_wal_backend(e.to_string()))?;
                proof
                    .insert("binding", binding)
                    .map_err(active_wal_backend)?;
                proof
                    .insert(
                        "replacementCandidates",
                        candidates.independently_retained_facts().clone(),
                    )
                    .map_err(active_wal_backend)?;
                proof
                    .insert(
                        "replacementMutationsSha256",
                        s(&hex(&sha256(candidates.exact_prepared_mutations()))),
                    )
                    .map_err(active_wal_backend)?;
                (Some(Box::new(replacement)), Some(mapping))
            };
            Ok((
                index,
                ActiveWalNativeRecovery {
                    raw: active_wal_bytes(&proof)?,
                    prior: previous,
                    replacement: None,
                    mapping: None,
                    inline_replacement: replacement,
                    inline_mapping: mapping,
                    installed: false,
                },
            ))
        })();
        let (index, native_recovery) = match checked {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainInlineReopenV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        let Self {
            mut controller,
            service,
        } = self;
        let ActiveServiceWithoutNativeIndex {
            financial,
            namespace: _,
            frame,
            source_start_error,
            origin,
            head,
            history,
            pending_observation,
            attempted_write,
            observation_limit,
            inline_profile,
        } = service;
        let NativeMainFinancialWithoutIndexV1 {
            financial,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head: base_head,
            history: base_history,
            halted,
        } = financial;
        let service = SourceMainNativeActiveServiceV1 {
            owner: SourceMainNativeSessionV13 {
                financial,
                index,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head: base_head,
                history: base_history,
                halted,
            },
            frame,
            source_start_error,
            origin,
            head,
            history,
            pending_observation,
            attempted_write,
            observation_limit,
            inline_profile,
        };
        controller.owner = Some(ActiveWalOwner::Active(Box::new(service)));
        controller
            .pending
            .as_mut()
            .expect("retained original action")
            .native_recovery = Some(native_recovery);
        controller.phase = if candidate == Candidate::Post {
            NativeMainActiveWalPhaseV1::Observe
        } else {
            NativeMainActiveWalPhaseV1::NativeSupersessionIntent
        };
        Ok(controller)
    }
}
