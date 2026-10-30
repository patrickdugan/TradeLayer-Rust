#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeValidityFactBoundaryPhaseV1 {
    Ready,
    IntentPrepared,
    IntentSynced,
    Attempted,
    FactPrepared,
    FactSynced,
}
/// Bounded producer milestone: adopts one already-held actual ordinary native
/// await. No source step/owner release is exposed, including after IO/fact errors.
pub struct SourceMainNativeValidityFactBoundaryV1 {
    owner: SourceMainNativeActiveServiceV1,
    wal: crate::source_main_active_service_wal::ActiveServiceWalV1,
    selection: V,
    phase: NativeValidityFactBoundaryPhaseV1,
    control: Option<crate::source_main_active_service_wal::PreparedActiveServiceWalWriteV1>,
    binding: Option<crate::source_main_active_service_wal::ActualValidityFactBindingV1>,
    attempt: Option<crate::source_compact_index_v2::ActualValidityAppendAttemptV1>,
    io_attempted: bool,
    fact_raw: Option<Vec<u8>>,
}
pub struct RejectedNativeValidityFactBoundaryV1 {
    pub cause: String,
    owner: SourceMainNativeActiveServiceV1,
    wal: crate::source_main_active_service_wal::ActiveServiceWalV1,
}
impl core::fmt::Debug for RejectedNativeValidityFactBoundaryV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeValidityFactBoundaryV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeValidityFactBoundaryV1 {
    pub fn into_parts(
        self,
    ) -> (
        SourceMainNativeActiveServiceV1,
        crate::source_main_active_service_wal::ActiveServiceWalV1,
    ) {
        (self.owner, self.wal)
    }
}
fn ordinary_boundary_error(error: impl core::fmt::Display) -> NativeMainActiveWalErrorV1 {
    NativeMainActiveWalErrorV1::Backend(error.to_string())
}
fn ordinary_boundary_selection(owner: &SourceMainNativeActiveServiceV1) -> ActiveWalResult<V> {
    if owner.profile() != NATIVE_ACTIVE_SERVICE_SESSION_PROFILE
        || owner.observation_pending()
        || owner.pending_write().is_none()
    {
        return Err(NativeMainActiveWalErrorV1::WrongAction);
    }
    let descriptor = owner
        .frame
        .as_ref()
        .and_then(|f| f.native_main_retained_consensus_descriptor(0))
        .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
    Ok(obj(&[
        (
            "profile",
            s("TL-MAIN-ACTUAL-ORDINARY-VALIDITY-SELECTION-13-1"),
        ),
        ("sourceProfile", s(owner.profile())),
        (
            "originCommands",
            s(&owner.origin.session.commands.to_string()),
        ),
        ("originHead", s(&hex(&owner.origin.session.commitment))),
        ("originPhysical", physical_value(&owner.origin.physical)),
        ("serviceActions", s(&owner.head.actions.to_string())),
        ("serviceHead", s(&hex(&owner.head.commitment))),
        ("pending", descriptor.value()),
    ]))
}
impl SourceMainNativeValidityFactBoundaryV1 {
    pub fn adopt(
        owner: SourceMainNativeActiveServiceV1,
        wal: crate::source_main_active_service_wal::ActiveServiceWalV1,
    ) -> Result<Self, Box<RejectedNativeValidityFactBoundaryV1>> {
        let checked = (|| -> ActiveWalResult<V> {
            let selection = ordinary_boundary_selection(&owner)?;
            if wal.pin()?.prefix.physical_sequence != 1
                || wal.record()?.get("phase") != s("Created")
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            if owner.attempted_write.is_some()
                || owner
                    .pending_write()
                    .unwrap()
                    .append_plan()
                    .intermediate()
                    .is_some()
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            wal.bind_actual_validity_fact(owner.index(), owner.pending_write().unwrap())?;
            Ok(selection)
        })();
        match checked {
            Ok(selection) => Ok(Self {
                owner,
                wal,
                selection,
                phase: NativeValidityFactBoundaryPhaseV1::Ready,
                control: None,
                binding: None,
                attempt: None,
                io_attempted: false,
                fact_raw: None,
            }),
            Err(e) => Err(Box::new(RejectedNativeValidityFactBoundaryV1 {
                cause: e.to_string(),
                owner,
                wal,
            })),
        }
    }
    pub fn phase(&self) -> NativeValidityFactBoundaryPhaseV1 {
        self.phase
    }
    pub fn native_attempted(&self) -> bool {
        self.io_attempted
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial_runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.owner.index()
    }
    pub fn effects(&self) -> &[V] {
        self.owner.effects()
    }
    pub fn pending_native_context(&self) -> Option<V> {
        self.owner.pending_native_context()
    }
    pub fn produced_io_fact(&self) -> Option<&[u8]> {
        self.fact_raw.as_deref()
    }
    pub fn prepare_intent(&mut self) -> ActiveWalResult<()> {
        if self.phase != NativeValidityFactBoundaryPhaseV1::Ready
            || self.control.is_some()
            || self.io_attempted
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        if ordinary_boundary_selection(&self.owner)? != self.selection {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let plan = self
            .owner
            .pending_write()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let history = self
            .owner
            .index()
            .capture_verified_history_pin()
            .map_err(ordinary_boundary_error)?
            .canonical_scalar_bytes()
            .map_err(ordinary_boundary_error)?;
        let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(plan.append_plan(),plan.durable_read_context()).map_err(ordinary_boundary_error)?;
        let raw = active_wal_bytes(candidates.independently_retained_facts())?;
        let mutations = candidates.exact_prepared_mutations().to_vec();
        let raw_action = NativeMainActiveServiceActionV1::AppendPending.canonical_action_bytes()?;
        let blobs = vec![history, raw, mutations, raw_action];
        let refs = blobs
            .iter()
            .map(|v| crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(v))
            .collect::<Result<Vec<_>, _>>()?;
        let record = obj(&[
            (
                "profile",
                self.wal.origin_context()?.get("origin").get("profile"),
            ),
            ("stage", s("Intent")),
            ("sourceSelection", self.selection.clone()),
            ("rawAction", refs[3].value()),
            ("nativeReadContext", plan.durable_read_context()),
            ("nativeBeforeHistory", refs[0].value()),
            ("nativeCandidates", refs[1].value()),
            ("nativeMutations", refs[2].value()),
        ]);
        self.control = Some(self.wal.prepare(&self.wal.pin()?, record, blobs, refs)?);
        self.phase = NativeValidityFactBoundaryPhaseV1::IntentPrepared;
        Ok(())
    }
    pub fn sync_intent(&mut self) -> ActiveWalResult<()> {
        if self.phase != NativeValidityFactBoundaryPhaseV1::IntentPrepared {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        self.wal.attempt(
            self.control
                .as_mut()
                .ok_or(NativeMainActiveWalErrorV1::NoAction)?,
        )?;
        self.control = None;
        self.bind_synced_intent()
    }
    fn bind_synced_intent(&mut self) -> ActiveWalResult<()> {
        self.phase = NativeValidityFactBoundaryPhaseV1::IntentSynced;
        self.binding = Some(
            self.wal.bind_actual_validity_fact(
                self.owner.index(),
                self.owner
                    .pending_write()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
            )?,
        );
        Ok(())
    }
    pub fn attempt_native(&mut self) -> ActiveWalResult<()> {
        if self.io_attempted || self.attempt.is_some() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        if self.phase != NativeValidityFactBoundaryPhaseV1::IntentSynced
            || ordinary_boundary_selection(&self.owner)? != self.selection
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let frame = self
            .owner
            .frame
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let write = frame
            .pending_write()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let context = write.durable_read_context();
        let attempted = &mut self.owner.attempted_write;
        let io_attempted = &mut self.io_attempted;
        let actual = self
            .owner
            .owner
            .index
            .commit_validity_capture_actual_fact_before(
                write,
                None,
                &self.wal,
                self.binding
                    .as_ref()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                || {
                    *io_attempted = true;
                    *attempted = Some(context)
                },
            )
            .map_err(ordinary_boundary_error)?;
        // Absolutely no post-IO fallible capture may discard this owned result.
        self.attempt = Some(actual);
        self.phase = NativeValidityFactBoundaryPhaseV1::Attempted;
        Ok(())
    }
    pub fn prepare_fact(&mut self) -> ActiveWalResult<()> {
        self.prepare_fact_budget(crate::source_main_active_service_wal::MAX_BLOB)
    }
    fn prepare_fact_budget(&mut self, max_bytes: usize) -> ActiveWalResult<()> {
        if self.phase != NativeValidityFactBoundaryPhaseV1::Attempted || self.control.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let actual = self
            .attempt
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let raw = actual
            .canonical_io_fact_bytes()
            .map_err(ordinary_boundary_error)?;
        if raw.len() > max_bytes {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        let pending = actual.pending_fact().canonical_fact_bytes().to_vec();
        let pending_pin =
            crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(&pending)?;
        let io_pin = crate::source_main_active_service_wal::ActiveServiceWalBlobV1::of(&raw)?;
        let record = obj(&[
            (
                "profile",
                s(crate::source_main_active_service_wal::SEGMENTED_VALIDITY_FACT_WAL_PROFILE),
            ),
            ("stage", s("NativeIoObserved")),
            ("sourceSelection", self.selection.clone()),
            ("actualPendingPlan", pending_pin.value()),
            ("actualNativeIo", io_pin.value()),
        ]);
        let plan = self.wal.prepare(
            &self.wal.pin()?,
            record,
            vec![pending, raw.clone()],
            vec![pending_pin, io_pin],
        )?;
        self.fact_raw = Some(raw);
        self.control = Some(plan);
        self.phase = NativeValidityFactBoundaryPhaseV1::FactPrepared;
        Ok(())
    }
    pub fn sync_fact(&mut self) -> ActiveWalResult<()> {
        if self.phase != NativeValidityFactBoundaryPhaseV1::FactPrepared {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        self.wal.attempt(
            self.control
                .as_mut()
                .ok_or(NativeMainActiveWalErrorV1::NoAction)?,
        )?;
        self.control = None;
        self.phase = NativeValidityFactBoundaryPhaseV1::FactSynced;
        Ok(())
    }
    pub fn resolve_control(
        &mut self,
    ) -> ActiveWalResult<crate::source_main_active_service_wal::ActiveServiceWalResolvedV1> {
        use crate::source_main_active_service_wal::ActiveServiceWalResolvedV1 as R;
        let plan = self
            .control
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let resolved = self.wal.resolve(plan)?;
        match resolved {
            R::Before => {
                self.control = Some(self.wal.rearm_before(plan)?);
            }
            R::Post => {
                self.control = None;
                match self.phase {
                    NativeValidityFactBoundaryPhaseV1::IntentPrepared => {
                        self.bind_synced_intent()?
                    }
                    NativeValidityFactBoundaryPhaseV1::FactPrepared => {
                        self.phase = NativeValidityFactBoundaryPhaseV1::FactSynced
                    }
                    _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
                }
            }
        }
        Ok(resolved)
    }
}
