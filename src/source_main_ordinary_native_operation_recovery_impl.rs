// A no-index type owns every live source/financial/control field and drops
// only the actual mutable index before one explicitly named candidate reopen.
pub struct SourceMainNativeValidityFactOperationNoIndexV1 {
    service: ActiveServiceWithoutNativeIndex,
    control: OrdinaryValidityOperationWithoutIndexV1,
}
struct OrdinaryValidityOperationWithoutIndexV1 {
    wal: ActiveServiceWalV1,
    selection: V,
    native_phase: NativeValidityFactBoundaryPhaseV1,
    binding: Option<crate::source_main_active_service_wal::ActualValidityFactBindingV1>,
    attempt: Option<crate::source_compact_index_v2::ActualValidityAppendAttemptV1>,
    io_attempted: bool,
    fact_raw: Option<Vec<u8>>,
    state: OrdinaryValidityOperationStateV1,
}
pub struct RejectedNativeValidityOperationCloseV1 {
    pub cause: String,
    owner: SourceMainNativeValidityFactOperationV1,
}
impl RejectedNativeValidityOperationCloseV1 {
    pub fn into_owner(self) -> SourceMainNativeValidityFactOperationV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeValidityOperationCloseV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeValidityOperationCloseV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedNativeValidityOperationReopenV1 {
    pub cause: String,
    owner: SourceMainNativeValidityFactOperationNoIndexV1,
}
impl RejectedNativeValidityOperationReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeValidityFactOperationNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeValidityOperationReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeValidityOperationReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeValidityFactOperationV1 {
    fn require_durable_failed_attempt(&self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::NativeRecoveryRequired
            || !self.state.delivered
            || self.boundary.control.is_some()
            || self.boundary.phase != NativeValidityFactBoundaryPhaseV1::FactSynced
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let actual = self
            .boundary
            .attempt
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        if actual.actual_result().is_ok() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let raw = actual
            .canonical_io_fact_bytes()
            .map_err(ordinary_boundary_error)?;
        let pending = actual.pending_fact().canonical_fact_bytes();
        let record = self.boundary.wal.record()?;
        if record.get("profile")
            != s(crate::source_main_active_service_wal::ORDINARY_VALIDITY_OPERATION_WAL_PROFILE)
            || record.get("stage") != s("NativeIoObserved")
            || record.get("sourceSelection") != self.boundary.selection
            || record.get("actualPendingPlan") != ActiveServiceWalBlobV1::of(pending)?.value()
            || record.get("actualNativeIo") != ActiveServiceWalBlobV1::of(&raw)?.value()
            || self
                .boundary
                .wal
                .read_blob(&ActiveServiceWalBlobV1::of(pending)?)?
                != pending
            || self
                .boundary
                .wal
                .read_blob(&ActiveServiceWalBlobV1::of(&raw)?)?
                != raw
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        Ok(())
    }
    pub fn into_no_index(
        self,
    ) -> Result<
        SourceMainNativeValidityFactOperationNoIndexV1,
        Box<RejectedNativeValidityOperationCloseV1>,
    > {
        if let Err(error) = self.require_durable_failed_attempt() {
            return Err(Box::new(RejectedNativeValidityOperationCloseV1 {
                cause: error.to_string(),
                owner: self,
            }));
        }
        let Self { boundary, state } = self;
        let SourceMainNativeValidityFactBoundaryV1 {
            owner,
            wal,
            selection,
            phase,
            control: _,
            binding,
            attempt,
            io_attempted,
            fact_raw,
        } = boundary;
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
        } = owner;
        let SourceMainNativeSessionV13 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding: source_binding,
            origin_physical,
            head: base_head,
            history: base_history,
            halted,
        } = owner;
        let namespace = index.native_namespace().clone();
        drop(index);
        Ok(SourceMainNativeValidityFactOperationNoIndexV1 {
            service: ActiveServiceWithoutNativeIndex {
                financial: NativeMainFinancialWithoutIndexV1 {
                    financial,
                    foundation,
                    foundation_head,
                    origin_rows,
                    binding: source_binding,
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
            control: OrdinaryValidityOperationWithoutIndexV1 {
                wal,
                selection,
                native_phase: phase,
                binding,
                attempt,
                io_attempted,
                fact_raw,
                state,
            },
        })
    }
    pub fn prepare_recovery_intent(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::NativeRecoveryRequired
            || self.boundary.control.is_some()
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let recovery = self
            .state
            .recovery
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if recovery.installed {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let (mut record, mut blobs, mut refs) =
            self.record_with_dependencies("NativeSupersessionIntent")?;
        if let Some(write) = &recovery.replacement {
            let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),write.durable_read_context()).map_err(ordinary_boundary_error)?;
            let history = recovery
                .history
                .canonical_scalar_bytes()
                .map_err(ordinary_boundary_error)?;
            let candidate = active_wal_bytes(candidates.independently_retained_facts())?;
            let mutations = candidates.exact_prepared_mutations().to_vec();
            for (key, raw) in [
                ("nativeBeforeHistory", history),
                ("nativeCandidates", candidate),
                ("nativeMutations", mutations),
            ] {
                let pin = ActiveServiceWalBlobV1::of(&raw)?;
                record
                    .insert(key, pin.value())
                    .map_err(ordinary_boundary_error)?;
                refs.push(pin);
                blobs.push(raw);
            }
            record
                .insert("nativeReadContext", write.durable_read_context())
                .map_err(ordinary_boundary_error)?;
        } else {
            record
                .insert("stage", s("NativeRecoveryObserved"))
                .map_err(ordinary_boundary_error)?;
        }
        self.boundary.control = Some(self.boundary.wal.prepare(
            &self.boundary.wal.pin()?,
            record,
            blobs,
            refs,
        )?);
        self.state.phase = NativeValidityOperationPhaseV1::RecoveryPrepared;
        Ok(())
    }
    pub fn sync_recovery_intent(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::RecoveryPrepared {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        self.boundary.wal.attempt(
            self.boundary
                .control
                .as_mut()
                .ok_or(NativeMainActiveWalErrorV1::NoAction)?,
        )?;
        self.boundary.control = None;
        self.state.phase = NativeValidityOperationPhaseV1::RecoveryIntentSynced;
        self.after_recovery_intent()
    }
    /// Retries only metadata installation/checking after the already-synced
    /// actual mapping. No source callback or native append is issued here.
    pub fn finish_recovery_intent(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::RecoveryIntentSynced
            || self.boundary.control.is_some()
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        self.after_recovery_intent()
    }
    fn after_recovery_intent(&mut self) -> ActiveWalResult<()> {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let recovery = self
            .state
            .recovery
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if recovery.candidate == Candidate::Post {
            let write = self
                .boundary
                .owner
                .pending_write()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            self.boundary
                .owner
                .index()
                .resolve_prepared_validity_post(write)
                .map_err(ordinary_boundary_error)?;
            // Metadata Post is proven. The first actual StorageStopped remains
            // the source result; no native append or source resume occurs.
            self.state.phase = NativeValidityOperationPhaseV1::ResultDelivered;
            return Ok(());
        }
        if self.state.prior_attempts.len() >= 63 {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        self.state
            .prior_attempts
            .try_reserve(1)
            .map_err(|_| NativeMainActiveWalErrorV1::Capacity)?;
        if !recovery.installed {
            let replacement = recovery
                .replacement
                .take()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let owner = &mut self.boundary.owner;
            if let Err((error, replacement)) = owner
                .owner
                .financial
                .native_main_runtime_mut()
                .native_main_service_install_supersession(
                    owner
                        .frame
                        .as_mut()
                        .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                    replacement,
                    recovery
                        .mapping
                        .as_ref()
                        .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                )
            {
                recovery.replacement = Some(replacement);
                return Err(ordinary_boundary_error(error));
            }
            recovery.installed = true;
        }
        self.boundary.binding = Some(
            self.boundary.wal.bind_actual_validity_fact(
                self.boundary.owner.index(),
                self.boundary
                    .owner
                    .pending_write()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
            )?,
        );
        self.state.phase = NativeValidityOperationPhaseV1::ReplacementIntentSynced;
        Ok(())
    }
    /// Exactly one native-only call after the actual replacement mapping has
    /// been synced. Original source observation, financial effects and raw
    /// action stay held; another failed return requires a new checked reopen.
    pub fn attempt_replacement(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::ReplacementIntentSynced
            || self.boundary.control.is_some()
            || self.boundary.phase != NativeValidityFactBoundaryPhaseV1::FactSynced
            || !self.state.recovery.as_ref().is_some_and(|r| r.installed)
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let mut retained = 0usize;
        for actual in self
            .state
            .prior_attempts
            .iter()
            .chain(self.boundary.attempt.iter())
        {
            retained = retained
                .checked_add(actual.pending_fact().canonical_fact_bytes().len())
                .and_then(|n| n.checked_add(actual.canonical_io_fact_bytes().ok()?.len()))
                .ok_or(NativeMainActiveWalErrorV1::Capacity)?;
        }
        if retained
            .checked_add(2 * crate::source_main_active_service_wal::MAX_BLOB)
            .is_none_or(|n| n > 256 * 1024 * 1024)
        {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        let owner = &mut self.boundary.owner;
        let write = owner
            .frame
            .as_ref()
            .and_then(|f| f.pending_write())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let context = write.durable_read_context();
        let attempted = &mut owner.attempted_write;
        let mut issued = false;
        let actual = owner
            .owner
            .index
            .commit_validity_capture_actual_fact_before(
                write,
                None,
                &self.boundary.wal,
                self.boundary
                    .binding
                    .as_ref()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                || {
                    issued = true;
                    *attempted = Some(context)
                },
            )
            .map_err(ordinary_boundary_error)?;
        // Capacity was reserved before IO. Preserve the entire prior result
        // chain and the new actual result before any serialization can fail.
        self.state.prior_attempts.push(
            self.boundary
                .attempt
                .take()
                .expect("original retained result"),
        );
        self.boundary.attempt = Some(actual);
        self.boundary.io_attempted = issued;
        self.boundary.fact_raw = None;
        self.boundary.phase = NativeValidityFactBoundaryPhaseV1::Attempted;
        Ok(())
    }
}
impl SourceMainNativeValidityFactOperationNoIndexV1 {
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.service.financial.financial.runtime()
    }
    pub fn effects(&self) -> &[V] {
        self.financial_runtime().native_main_row_effects()
    }
    pub fn try_reopen(
        self,
        root: &Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<SourceMainNativeValidityFactOperationV1, Box<RejectedNativeValidityOperationReopenV1>>
    {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let checked = (|| -> ActiveWalResult<_> {
            if self.control.state.recovery_count >= 64 {
                return Err(NativeMainActiveWalErrorV1::Capacity);
            }
            let write = self
                .service
                .frame
                .as_ref()
                .and_then(|f| f.pending_write())
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let (index, observation) = SourceCompactIndex::reopen_prepared_validity(
                root,
                self.service.namespace.clone(),
                write,
                candidate,
                recovery,
            )
            .map_err(ordinary_boundary_error)?;
            let history = index
                .capture_verified_history_pin()
                .map_err(ordinary_boundary_error)?;
            let prior = self
                .control
                .state
                .recovery
                .as_ref()
                .map(|r| ActiveServiceWalBlobV1::of(&r.raw))
                .transpose()?;
            let actual = self
                .control
                .attempt
                .as_ref()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let mut proof = obj(&[
                ("profile", s("TL-MAIN-ORDINARY-VALIDITY-RECOVERY-13-1")),
                (
                    "candidate",
                    s(match candidate {
                        Candidate::Before => "Before",
                        Candidate::Intermediate => "Intermediate",
                        Candidate::Post => "Post",
                    }),
                ),
                ("sourceSelection", self.control.selection.clone()),
                (
                    "actualPendingFact",
                    ActiveServiceWalBlobV1::of(actual.pending_fact().canonical_fact_bytes())?
                        .value(),
                ),
                (
                    "actualIoFact",
                    ActiveServiceWalBlobV1::of(
                        &actual
                            .canonical_io_fact_bytes()
                            .map_err(ordinary_boundary_error)?,
                    )?
                    .value(),
                ),
                (
                    "verifiedReopenedHistoryHex",
                    s(&hex(&history
                        .canonical_scalar_bytes()
                        .map_err(ordinary_boundary_error)?)),
                ),
                (
                    "previousRecovery",
                    prior
                        .as_ref()
                        .map_or(V::Null, ActiveServiceWalBlobV1::value),
                ),
                (
                    "discardedBytes",
                    s(&observation.recovery_report().discarded_bytes.to_string()),
                ),
            ]);
            let (replacement, mapping) = if candidate == Candidate::Post {
                index
                    .resolve_prepared_validity_post(write)
                    .map_err(ordinary_boundary_error)?;
                (None, None)
            } else {
                let (replacement, mapping) = index
                    .supersede_prepared_validity(write, &observation)
                    .map_err(ordinary_boundary_error)?;
                // This component has no generation-escrow profile. Refuse a
                // competing publication before append, retaining no-index owner.
                if replacement.append_plan().intermediate().is_some() {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
                let binding = index
                    .require_actual_native_supersession(write, &observation, &replacement, &mapping)
                    .map_err(ordinary_boundary_error)?;
                proof
                    .insert("binding", binding)
                    .map_err(ordinary_boundary_error)?;
                let facts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(replacement.append_plan(),replacement.durable_read_context()).map_err(ordinary_boundary_error)?;
                proof
                    .insert(
                        "replacementCandidates",
                        facts.independently_retained_facts().clone(),
                    )
                    .map_err(ordinary_boundary_error)?;
                proof
                    .insert(
                        "replacementMutationsSha256",
                        s(&hex(&sha256(facts.exact_prepared_mutations()))),
                    )
                    .map_err(ordinary_boundary_error)?;
                (Some(Box::new(replacement)), Some(mapping))
            };
            Ok((
                index,
                OrdinaryValidityOperationRecoveryV1 {
                    raw: active_wal_bytes(&proof)?,
                    prior,
                    candidate,
                    history,
                    replacement,
                    mapping,
                    installed: false,
                },
            ))
        })();
        let (index, observed) = match checked {
            Ok(v) => v,
            Err(error) => {
                return Err(Box::new(RejectedNativeValidityOperationReopenV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        let Self { service, control } = self;
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
        let owner = SourceMainNativeActiveServiceV1 {
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
        let OrdinaryValidityOperationWithoutIndexV1 {
            wal,
            selection,
            native_phase,
            binding,
            attempt,
            io_attempted,
            fact_raw,
            mut state,
        } = control;
        state.recovery_count += 1;
        state.recovery = Some(observed);
        Ok(SourceMainNativeValidityFactOperationV1 {
            boundary: SourceMainNativeValidityFactBoundaryV1 {
                owner,
                wal,
                selection,
                phase: native_phase,
                control: None,
                binding,
                attempt,
                io_attempted,
                fact_raw,
            },
            state,
        })
    }
}
