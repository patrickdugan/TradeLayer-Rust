/// One explicitly selected ordinary validity action. The whole financial,
/// native, source frame and WAL owner stays private through every barrier.
/// This does not admit an old untyped service history as typed cold input.
pub struct SourceMainNativeValidityFactOperationV1 {
    boundary: SourceMainNativeValidityFactBoundaryV1,
    state: OrdinaryValidityOperationStateV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeValidityOperationPhaseV1 {
    Native,
    ResultDelivered,
    NativeRecoveryRequired,
    RecoveryPrepared,
    RecoveryIntentSynced,
    ReplacementIntentSynced,
    AcceptedPrepared,
    AcceptedSynced,
    AckPrepared,
    Acknowledged,
}
struct OrdinaryValidityOperationStateV1 {
    before: NativeMainActiveServiceExpectationV1,
    phase: NativeValidityOperationPhaseV1,
    delivered: bool,
    observed: bool,
    accepted: Option<Vec<u8>>,
    accepted_pin: Option<ActiveServiceWalPinV1>,
    recovery: Option<OrdinaryValidityOperationRecoveryV1>,
    prior_attempts: Vec<crate::source_compact_index_v2::ActualValidityAppendAttemptV1>,
    recovery_count: usize,
    io_chain: Option<Vec<u8>>,
    io_prior: Option<ActiveServiceWalBlobV1>,
    io_chain_count: usize,
}
struct OrdinaryValidityOperationRecoveryV1 {
    raw: Vec<u8>,
    prior: Option<ActiveServiceWalBlobV1>,
    candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
    history: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    replacement: Option<Box<crate::source_compact_index_v2::NativePreparedValidityWriteV1>>,
    mapping: Option<crate::source_compact_index_v2::NativePreparedValiditySupersessionV1>,
    installed: bool,
}
impl SourceMainNativeValidityFactOperationV1 {
    pub fn adopt(
        owner: SourceMainNativeActiveServiceV1,
        wal: ActiveServiceWalV1,
    ) -> Result<Self, Box<RejectedNativeValidityFactBoundaryV1>> {
        let origin = match wal.origin_context() {
            Ok(context) => context.get("origin"),
            Err(error) => {
                return Err(Box::new(RejectedNativeValidityFactBoundaryV1 {
                    cause: error.to_string(),
                    owner,
                    wal,
                }))
            }
        };
        if origin.get("profile")
            != s(crate::source_main_active_service_wal::ORDINARY_VALIDITY_OPERATION_WAL_PROFILE)
            || origin.get("operationSourceScope") != s("RetainedOrdinaryValidityAction-1")
        {
            return Err(Box::new(RejectedNativeValidityFactBoundaryV1 {
                cause: "explicit ordinary operation origin required".into(),
                owner,
                wal,
            }));
        }
        // This read precedes all native IO and remains the actual source
        // action's original physical Before even after native recovery.
        if owner.history.len() >= MAX_SERVICE_ACTIONS {
            return Err(Box::new(RejectedNativeValidityFactBoundaryV1 {
                cause: "finite source receipt capacity".into(),
                owner,
                wal,
            }));
        }
        let before = match owner.expectation() {
            Ok(before) => before,
            Err(error) => {
                return Err(Box::new(RejectedNativeValidityFactBoundaryV1 {
                    cause: format!("{error:?}"),
                    owner,
                    wal,
                }))
            }
        };
        SourceMainNativeValidityFactBoundaryV1::adopt(owner, wal).map(|boundary| Self {
            boundary,
            state: OrdinaryValidityOperationStateV1 {
                before,
                phase: NativeValidityOperationPhaseV1::Native,
                delivered: false,
                observed: false,
                accepted: None,
                accepted_pin: None,
                recovery: None,
                prior_attempts: vec![],
                recovery_count: 0,
                io_chain: None,
                io_prior: None,
                io_chain_count: 0,
            },
        })
    }
    pub fn phase(&self) -> NativeValidityOperationPhaseV1 {
        self.state.phase
    }
    pub fn native_phase(&self) -> NativeValidityFactBoundaryPhaseV1 {
        self.boundary.phase
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.boundary.financial_runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.boundary.index()
    }
    pub fn effects(&self) -> &[V] {
        self.boundary.effects()
    }
    pub fn pending_native_context(&self) -> Option<V> {
        self.boundary.pending_native_context()
    }
    pub fn produced_io_fact(&self) -> Option<&[u8]> {
        self.boundary.produced_io_fact()
    }
    pub fn native_attempts(&self) -> usize {
        self.state.prior_attempts.len() + usize::from(self.boundary.attempt.is_some())
    }
    pub fn wal_pin(&self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        Ok(self.boundary.wal.pin()?)
    }
    pub fn source_head(&self) -> &NativeMainActiveServiceHeadV1 {
        &self.boundary.owner.head
    }
    pub fn source_result(&self) -> Option<&NativeMainActiveServiceActionResultV1> {
        self.boundary
            .owner
            .pending_observation
            .as_ref()
            .map(|p| &p.result)
            .or_else(|| {
                self.state.observed.then(|| {
                    &self
                        .boundary
                        .owner
                        .history
                        .last()
                        .expect("retained actual receipt")
                        .result
                })
            })
    }
    fn require_native(&self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::Native || self.state.delivered {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        Ok(())
    }
    pub fn prepare_intent(&mut self) -> ActiveWalResult<()> {
        self.require_native()?;
        self.boundary.prepare_intent()
    }
    pub fn sync_intent(&mut self) -> ActiveWalResult<()> {
        self.require_native()?;
        self.boundary.sync_intent()
    }
    pub fn attempt_native(&mut self) -> ActiveWalResult<()> {
        self.require_native()?;
        self.boundary.attempt_native()
    }
    pub fn prepare_fact(&mut self) -> ActiveWalResult<()> {
        if !matches!(
            self.state.phase,
            NativeValidityOperationPhaseV1::Native
                | NativeValidityOperationPhaseV1::ReplacementIntentSynced
        ) {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        if self.boundary.phase != NativeValidityFactBoundaryPhaseV1::Attempted
            || self.boundary.control.is_some()
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        self.ensure_io_chain()?;
        let actual = self
            .boundary
            .attempt
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let raw = actual
            .canonical_io_fact_bytes()
            .map_err(ordinary_boundary_error)?;
        let pending = actual.pending_fact().canonical_fact_bytes();
        let (mut record, blobs, refs) = self.record_with_dependencies("NativeIoObserved")?;
        record
            .insert(
                "actualPendingPlan",
                ActiveServiceWalBlobV1::of(pending)?.value(),
            )
            .map_err(ordinary_boundary_error)?;
        record
            .insert("actualNativeIo", ActiveServiceWalBlobV1::of(&raw)?.value())
            .map_err(ordinary_boundary_error)?;
        self.boundary.control = Some(self.boundary.wal.prepare(
            &self.boundary.wal.pin()?,
            record,
            blobs,
            refs,
        )?);
        self.boundary.fact_raw = Some(raw);
        self.boundary.phase = NativeValidityFactBoundaryPhaseV1::FactPrepared;
        Ok(())
    }
    pub fn sync_fact(&mut self) -> ActiveWalResult<()> {
        if !matches!(
            self.state.phase,
            NativeValidityOperationPhaseV1::Native
                | NativeValidityOperationPhaseV1::ReplacementIntentSynced
        ) {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        self.boundary.sync_fact()?;
        // A native-only replacement cannot turn the original source error
        // into success. The first delivered source result is retained.
        if self.state.delivered {
            self.state.phase = if self
                .boundary
                .attempt
                .as_ref()
                .expect("actual attempt")
                .actual_result()
                .is_ok()
            {
                NativeValidityOperationPhaseV1::ResultDelivered
            } else {
                NativeValidityOperationPhaseV1::NativeRecoveryRequired
            };
        }
        Ok(())
    }
    /// Applies the same handler as live native_main_rows_append, using the
    /// owned actual native Result after its fact barrier, exactly once.
    pub fn deliver_actual_result(&mut self) -> ActiveWalResult<()> {
        self.require_native()?;
        if self.boundary.phase != NativeValidityFactBoundaryPhaseV1::FactSynced
            || self.boundary.control.is_some()
            || ordinary_boundary_selection(&self.boundary.owner)? != self.boundary.selection
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        self.boundary
            .owner
            .history
            .try_reserve(1)
            .map_err(|_| NativeMainActiveWalErrorV1::Capacity)?;
        let owner = &mut self.boundary.owner;
        let attempt = self
            .boundary
            .attempt
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let stop = owner
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_deliver_actual_validity_result(
                &owner.owner.index,
                owner
                    .frame
                    .as_mut()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                attempt,
            )
            .map_err(ordinary_boundary_error)?;
        // No fallible work after actual delivery can lose the source result.
        owner.pending_observation = Some(NativeMainActiveServicePendingV1 {
            action: NativeMainActiveServiceActionV1::AppendPending,
            result: NativeMainActiveServiceActionResultV1::Stop(stop),
            before: self.state.before.physical.clone(),
        });
        self.state.delivered = true;
        self.state.phase = if attempt.actual_result().is_ok() {
            NativeValidityOperationPhaseV1::ResultDelivered
        } else {
            NativeValidityOperationPhaseV1::NativeRecoveryRequired
        };
        Ok(())
    }
    fn record_with_dependencies(
        &self,
        stage: &str,
    ) -> ActiveWalResult<(V, Vec<Vec<u8>>, Vec<ActiveServiceWalBlobV1>)> {
        let mut blobs = vec![];
        if let Some(actual) = self.boundary.attempt.as_ref() {
            let pending = actual.pending_fact().canonical_fact_bytes().to_vec();
            let io = actual
                .canonical_io_fact_bytes()
                .map_err(ordinary_boundary_error)?;
            blobs.extend([pending, io]);
        }
        let mut record = obj(&[
            (
                "profile",
                s(crate::source_main_active_service_wal::ORDINARY_VALIDITY_OPERATION_WAL_PROFILE),
            ),
            (
                "operationProfile",
                s("TL-MAIN-ORDINARY-VALIDITY-OPERATION-13-1"),
            ),
            ("stage", s(stage)),
            ("sourceSelection", self.boundary.selection.clone()),
            ("nativeAttempts", s(&self.native_attempts().to_string())),
            (
                "originalPhysicalBefore",
                physical_value(&self.state.before.physical),
            ),
        ]);
        if let Some(raw) = &self.state.io_chain {
            record
                .insert(
                    "actualNativeIoChain",
                    ActiveServiceWalBlobV1::of(raw)?.value(),
                )
                .map_err(ordinary_boundary_error)?;
            blobs.push(raw.clone());
        }
        if let Some(recovery) = &self.state.recovery {
            record
                .insert(
                    "nativeRecovery",
                    ActiveServiceWalBlobV1::of(&recovery.raw)?.value(),
                )
                .map_err(ordinary_boundary_error)?;
            blobs.push(recovery.raw.clone());
        }
        if let Some(raw) = &self.state.accepted {
            record
                .insert("accepted", ActiveServiceWalBlobV1::of(raw)?.value())
                .map_err(ordinary_boundary_error)?;
            blobs.push(raw.clone());
        }
        let mut refs = blobs
            .iter()
            .map(|b| ActiveServiceWalBlobV1::of(b))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(prior) = &self.state.io_prior {
            refs.push(prior.clone());
        }
        if let Some(prior) = self.state.recovery.as_ref().and_then(|r| r.prior.as_ref()) {
            refs.push(prior.clone());
        }
        Ok((record, blobs, refs))
    }
    fn ensure_io_chain(&mut self) -> ActiveWalResult<()> {
        let count = self.native_attempts();
        if self.state.io_chain_count == count {
            return Ok(());
        }
        if self.state.io_chain_count.checked_add(1) != Some(count) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let actual = self
            .boundary
            .attempt
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let pending = actual.pending_fact().canonical_fact_bytes();
        let io = actual
            .canonical_io_fact_bytes()
            .map_err(ordinary_boundary_error)?;
        let prior = self
            .state
            .io_chain
            .as_ref()
            .map(|raw| ActiveServiceWalBlobV1::of(raw))
            .transpose()?;
        if let (Some(raw), Some(pin)) = (&self.state.io_chain, &prior) {
            if self.boundary.wal.read_blob(pin)? != *raw {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
        }
        let raw = active_wal_bytes(&obj(&[
            ("profile", s("TL-MAIN-ORDINARY-VALIDITY-IO-CHAIN-13-1")),
            ("attempt", s(&count.to_string())),
            (
                "previous",
                prior
                    .as_ref()
                    .map_or(V::Null, ActiveServiceWalBlobV1::value),
            ),
            ("sourceSelection", self.boundary.selection.clone()),
            ("pendingFact", ActiveServiceWalBlobV1::of(pending)?.value()),
            ("actualIoFact", ActiveServiceWalBlobV1::of(&io)?.value()),
        ]))?;
        ActiveServiceWalBlobV1::of(&raw)?;
        self.state.io_prior = prior;
        self.state.io_chain = Some(raw);
        self.state.io_chain_count = count;
        Ok(())
    }
    pub fn prepare_accepted(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::ResultDelivered
            || self.boundary.control.is_some()
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        if !self.state.observed {
            self.boundary.owner.retry_observation()?;
            // The actual observer has executed. A later serialization failure
            // retries only comparison encoding, never observation or source.
            self.state.observed = true;
        }
        if self.state.accepted.is_none() {
            let mut fields = NativeFieldCollector::new(
                "Native13/OrdinaryValidityAccepted/v1",
                true,
                self.boundary.owner.observation_limit,
            );
            self.boundary
                .owner
                .visit_canonical_fields(&mut |p, v| fields.emit(p, v))
                .map_err(ordinary_boundary_error)?;
            let receipt = self
                .boundary
                .owner
                .history
                .last()
                .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
            receipt
                .result
                .visit_fields(&mut |p, v| fields.emit(&format!("result/{p}"), v))
                .map_err(ordinary_boundary_error)?;
            fields
                .emit("financial", &s(&hex(&receipt.financial_commitment)))
                .map_err(ordinary_boundary_error)?;
            native_step_value_fields("afterRows", &logical_read(&receipt.rows), &mut fields)
                .map_err(ordinary_boundary_error)?;
            native_step_value_fields(
                "beforePhysical",
                &physical_value(&receipt.before),
                &mut fields,
            )
            .map_err(ordinary_boundary_error)?;
            native_step_value_fields(
                "afterPhysical",
                &physical_value(&receipt.after),
                &mut fields,
            )
            .map_err(ordinary_boundary_error)?;
            fields
                .emit("receipt/sequence", &s(&receipt.sequence.to_string()))
                .map_err(ordinary_boundary_error)?;
            fields
                .emit("receipt/previousHead", &s(&hex(&receipt.previous_head)))
                .map_err(ordinary_boundary_error)?;
            fields
                .emit("receipt/head", &s(&hex(&receipt.head)))
                .map_err(ordinary_boundary_error)?;
            fields
                .emit(
                    "receipt/fields/count",
                    &s(&receipt.fields.fields.to_string()),
                )
                .map_err(ordinary_boundary_error)?;
            fields
                .emit(
                    "receipt/fields/bytes",
                    &s(&receipt.fields.bytes.to_string()),
                )
                .map_err(ordinary_boundary_error)?;
            fields
                .emit(
                    "receipt/fields/commitment",
                    &s(&hex(&receipt.fields.commitment)),
                )
                .map_err(ordinary_boundary_error)?;
            self.state.accepted = Some(fields.frames.expect("retained actual observation"));
        }
        let (record, blobs, refs) = self.record_with_dependencies("Accepted")?;
        self.boundary.control = Some(self.boundary.wal.prepare(
            &self.boundary.wal.pin()?,
            record,
            blobs,
            refs,
        )?);
        self.state.phase = NativeValidityOperationPhaseV1::AcceptedPrepared;
        Ok(())
    }
    pub fn sync_accepted(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::AcceptedPrepared {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let expected = self
            .boundary
            .control
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?
            .post()
            .clone();
        self.boundary.wal.attempt(
            self.boundary
                .control
                .as_mut()
                .ok_or(NativeMainActiveWalErrorV1::NoAction)?,
        )?;
        self.boundary.control = None;
        self.state.accepted_pin = Some(expected);
        self.state.phase = NativeValidityOperationPhaseV1::AcceptedSynced;
        Ok(())
    }
    pub fn prepare_ack(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::AcceptedSynced
            || self.boundary.control.is_some()
            || self.state.accepted_pin.as_ref() != Some(&self.boundary.wal.pin()?)
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let (record, blobs, refs) = self.record_with_dependencies("Ack")?;
        self.boundary.control = Some(self.boundary.wal.prepare(
            &self.boundary.wal.pin()?,
            record,
            blobs,
            refs,
        )?);
        self.state.phase = NativeValidityOperationPhaseV1::AckPrepared;
        Ok(())
    }
    pub fn sync_ack(&mut self) -> ActiveWalResult<()> {
        if self.state.phase != NativeValidityOperationPhaseV1::AckPrepared {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        self.boundary.wal.attempt(
            self.boundary
                .control
                .as_mut()
                .ok_or(NativeMainActiveWalErrorV1::NoAction)?,
        )?;
        self.boundary.control = None;
        self.state.phase = NativeValidityOperationPhaseV1::Acknowledged;
        Ok(())
    }
    pub fn resolve_control(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        if self.state.phase == NativeValidityOperationPhaseV1::Native {
            return self.boundary.resolve_control();
        }
        let plan = self
            .boundary
            .control
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let resolved = self.boundary.wal.resolve(plan)?;
        if resolved == ActiveServiceWalResolvedV1::Before {
            self.boundary.control = Some(self.boundary.wal.rearm_before(plan)?);
            return Ok(resolved);
        }
        self.boundary.control = None;
        match self.state.phase {
            NativeValidityOperationPhaseV1::RecoveryPrepared => {
                self.state.phase = NativeValidityOperationPhaseV1::RecoveryIntentSynced;
                self.after_recovery_intent()?;
            }
            NativeValidityOperationPhaseV1::ReplacementIntentSynced => {
                if self.boundary.phase != NativeValidityFactBoundaryPhaseV1::FactPrepared {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
                self.boundary.phase = NativeValidityFactBoundaryPhaseV1::FactSynced;
                self.state.phase = if self
                    .boundary
                    .attempt
                    .as_ref()
                    .expect("actual IO")
                    .actual_result()
                    .is_ok()
                {
                    NativeValidityOperationPhaseV1::ResultDelivered
                } else {
                    NativeValidityOperationPhaseV1::NativeRecoveryRequired
                };
            }
            NativeValidityOperationPhaseV1::AcceptedPrepared => {
                self.state.accepted_pin = Some(self.boundary.wal.pin()?);
                self.state.phase = NativeValidityOperationPhaseV1::AcceptedSynced;
            }
            NativeValidityOperationPhaseV1::AckPrepared => {
                self.state.phase = NativeValidityOperationPhaseV1::Acknowledged
            }
            _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
        }
        Ok(resolved)
    }
}
