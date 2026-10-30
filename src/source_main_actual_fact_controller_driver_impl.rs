/// Explicit opt-in writer. The base controller is private and cannot escape
/// around the mandatory pre-observation fact barrier.
pub struct SourceMainNativeActualFactServiceV1 {
    inner: SourceMainNativeDurableActiveServiceV1,
    fact_barrier: bool,
    deferred_error: Option<NativeMainActiveWalErrorV1>,
}
pub struct RejectedNativeMainActualFactServiceV1 {
    pub error: NativeMainActiveWalErrorV1,
    owner: SourceMainNativeSessionV13,
}
impl RejectedNativeMainActualFactServiceV1 {
    pub fn into_owner(self) -> SourceMainNativeSessionV13 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActualFactServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActualFactServiceV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeActualFactServiceV1 {
    pub fn create(
        owner: SourceMainNativeSessionV13,
        wal_root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        escrow_root: PathBuf,
        durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    ) -> Result<Self, Box<RejectedNativeMainActualFactServiceV1>> {
        match SourceMainNativeDurableActiveServiceV1::create_generation_escrow_fact_entry(
            owner,
            wal_root,
            namespace,
            filesystem,
            observation_limit,
            (escrow_root, durability, true, true),
        ) {
            Ok(inner) => Ok(Self {
                inner,
                fact_barrier: false,
                deferred_error: None,
            }),
            Err(rejected) => {
                let RejectedNativeMainActiveWalV1 { error, owner } = *rejected;
                Err(Box::new(RejectedNativeMainActualFactServiceV1 {
                    error,
                    owner,
                }))
            }
        }
    }
    pub fn profile(&self) -> &'static str {
        ACTUAL_SOURCE_FACT_WAL_PROFILE
    }
    pub fn phase(&self) -> NativeMainActiveWalPhaseV1 {
        self.inner.phase
    }
    pub fn fact_barrier_pending(&self) -> bool {
        self.fact_barrier
            || self
                .inner
                .actual_facts
                .as_ref()
                .is_some_and(|f| f.needs_barrier)
    }
    pub fn head(&self) -> [u8; 32] {
        self.inner.head()
    }
    pub fn history(&self) -> &[NativeMainActiveWalReceiptV1] {
        self.inner.history()
    }
    pub fn wal_pin(&self) -> ActiveWalResult<ActiveServiceWalPinV1> {
        self.inner.wal_pin()
    }
    pub fn read_history_blob(&self, pin: &ActiveServiceWalBlobV1) -> ActiveWalResult<Vec<u8>> {
        self.inner.read_history_blob(pin)
    }
    pub fn prepare_action(
        &mut self,
        action: NativeMainActiveServiceActionV1,
    ) -> ActiveWalResult<()> {
        if self.fact_barrier_pending() || self.deferred_error.is_some() {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        self.inner.prepare_action(action)
    }
    fn sync_actual_facts(&mut self) -> ActiveWalResult<()> {
        if !self
            .inner
            .actual_facts
            .as_ref()
            .is_some_and(|f| f.needs_barrier)
        {
            return Ok(());
        }
        if !self.fact_barrier {
            // Pure retry of producer/serialization only. The executed marker
            // already forbids repeating the source action or native append.
            self.inner.complete_actual_fact_input()?;
            self.fact_barrier = true;
        }
        let stage = if self
            .inner
            .actual_facts
            .as_ref()
            .is_some_and(|facts| facts.recovery_observation)
        {
            "NativeRecoveryObserved"
        } else if self
            .inner
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
            .is_some_and(|s| s.io.is_some())
        {
            "NativeIoObserved"
        } else {
            "SourcePendingPlanObserved"
        };
        self.inner.write_barrier(stage)?;
        self.inner
            .actual_facts
            .as_mut()
            .expect("explicit fact mode")
            .needs_barrier = false;
        self.inner
            .actual_facts
            .as_mut()
            .expect("explicit fact mode")
            .recovery_observation = false;
        self.fact_barrier = false;
        Ok(())
    }
    pub fn advance(&mut self) -> ActiveWalResult<NativeMainActiveWalPhaseV1> {
        loop {
            self.sync_actual_facts()?;
            if let Some(error) = self.deferred_error.take() {
                return Err(error);
            }
            let result = self.inner.advance();
            if self
                .inner
                .actual_facts
                .as_ref()
                .is_some_and(|f| f.needs_barrier)
            {
                self.deferred_error = result.err();
                continue;
            }
            return result;
        }
    }
    /// Resolves only the retained control-store plan. It never calls an action
    /// or native append again, including an applied write with a lost return.
    pub fn resolve_control_write(&mut self) -> ActiveWalResult<ActiveServiceWalResolvedV1> {
        if !self.fact_barrier {
            // Resolving a lost Intent return may execute the action exactly
            // once. Its new native/source facts still precede this API return.
            let result = self.inner.resolve_control_write();
            if self
                .inner
                .actual_facts
                .as_ref()
                .is_some_and(|f| f.needs_barrier)
            {
                match result {
                    Ok(resolved) => {
                        self.sync_actual_facts()?;
                        return Ok(resolved);
                    }
                    Err(error) => {
                        self.deferred_error = Some(error);
                        self.sync_actual_facts()?;
                        return Err(self.deferred_error.take().expect("retained real result"));
                    }
                }
            }
            return result;
        }
        let p = self
            .inner
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let plan = p
            .plan
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let resolved = self.inner.wal.resolve(plan)?;
        match resolved {
            ActiveServiceWalResolvedV1::Before => p.plan = Some(self.inner.wal.rearm_before(plan)?),
            ActiveServiceWalResolvedV1::Post => {
                p.plan = None;
                self.inner
                    .actual_facts
                    .as_mut()
                    .expect("explicit fact mode")
                    .needs_barrier = false;
                self.inner
                    .actual_facts
                    .as_mut()
                    .expect("explicit fact mode")
                    .recovery_observation = false;
                self.fact_barrier = false;
            }
        }
        Ok(resolved)
    }
    // Native recovery deliberately has no base-controller escape. A later
    // versioned bridge must carry this wrapper and its actual fact chain.
}
impl SourceMainNativeDurableActiveServiceV1 {
    fn complete_actual_fact_input(&mut self) -> ActiveWalResult<()> {
        self.ensure_actual_io_chain(false)?;
        let after = self.capture_actual_inline_fact_input()?;
        self.pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?
            .actual_fact_slots
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .after = after;
        Ok(())
    }
    fn actual_fact_action_alias(
        &self,
        action: &NativeMainActiveServiceActionV1,
    ) -> ActiveWalResult<Option<ActualSourceNativeAliasV1>> {
        if !matches!(action, NativeMainActiveServiceActionV1::AppendPending) {
            return Ok(None);
        }
        let Some(service) = self.active_service() else {
            return Ok(None);
        };
        let Ok(expected) = service.expectation() else {
            return Ok(None);
        };
        // A genuine API rejection (including repeated AppendPending at Post)
        // cannot become a fresh native attempt or inherit a stale IO error.
        if service.escrow_append_preflight(&expected).is_err() {
            return Ok(None);
        }
        service
            .pending_inline_write()
            .map(|p| self.actual_native_alias_from_plan(p, None))
            .transpose()
    }
    fn execute_actual_fact_once(&mut self) -> ActiveWalResult<()> {
        let p = self
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        if p.executed {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        p.executed = true;
        let action = p.action.clone();
        // Before any post-execution fallible capture, keep the real owner in
        // Observe. The outer writer parks here until facts are durable.
        self.phase = NativeMainActiveWalPhaseV1::Observe;
        self.actual_facts
            .as_mut()
            .expect("explicit fact mode")
            .needs_barrier = true;
        match self.owner.take().expect("same retained owner") {
            ActiveWalOwner::Ready(owner) => {
                let NativeMainActiveServiceActionV1::Start {
                    start,
                    generation,
                    documents,
                } = action
                else {
                    unreachable!("constructor action checked before intent")
                };
                match owner.expectation() {
                    Err(error) => {
                        self.pending.as_mut().expect("pending").source_error = Some(
                            NativeMainActiveServiceErrorV1::Backend(format!("{error:?}")),
                        );
                        self.owner = Some(ActiveWalOwner::Ready(owner));
                    }
                    Ok(expected) => match owner.into_active_service_fact_entry(
                        &expected,
                        start,
                        generation,
                        documents,
                        (self.observation_limit, self.inline_service_enabled()),
                        true,
                    ) {
                        Ok(service) => self.owner = Some(ActiveWalOwner::Active(Box::new(service))),
                        Err(rejected) => {
                            let RejectedNativeMainActiveServiceV1 { error, owner } = *rejected;
                            self.pending.as_mut().expect("pending").source_error = Some(error);
                            self.owner = Some(ActiveWalOwner::Ready(Box::new(owner)));
                        }
                    },
                }
            }
            ActiveWalOwner::Active(mut service) => {
                let result = (|| {
                    let expected = service.expectation()?;
                    if matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                        && service.pending_inline_write().is_some()
                    {
                        service.escrow_append_preflight(&expected)?;
                        let write = service
                            .pending_inline_write()
                            .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                        let binding = self
                            .wal
                            .bind_actual_inline_native_fact(&service.owner.index, write)
                            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
                        // Exactly one native attempt per current raw action in
                        // this milestone; the owned result cannot be replaced.
                        if self
                            .pending
                            .as_ref()
                            .and_then(|p| p.actual_fact_slots.as_ref())
                            .is_some_and(|s| s.io.is_some())
                        {
                            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
                        }
                        let io = service.apply_actual_inline_capture(
                            &expected,
                            self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
                            &self.wal,
                            &binding,
                        )?;
                        self.pending
                            .as_mut()
                            .expect("pending")
                            .actual_fact_slots
                            .as_mut()
                            .expect("fact slots")
                            .io = Some(io);
                        Ok(())
                    } else {
                        service.stage_action(&expected, action)
                    }
                })();
                if let Err(error) = result {
                    if !service.observation_pending() {
                        self.pending.as_mut().expect("pending").source_error = Some(error);
                    }
                }
                self.owner = Some(ActiveWalOwner::Active(service));
            }
        }
        let stopped = self
            .pending
            .as_ref()
            .is_some_and(|p| p.source_error.is_none())
            && self.active_service().is_some_and(|s| {
                matches!(
                    s.pending_observation.as_ref().map(|p| &p.result),
                    Some(NativeMainActiveServiceActionResultV1::Stop(
                        NativeMainServiceStopV1::StorageStopped(_)
                    ))
                )
            });
        if stopped {
            self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
        }
        self.complete_actual_fact_input()?;
        if stopped {
            return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
        }
        Ok(())
    }
    fn add_actual_fact_blobs(&self, blobs: &mut Vec<Vec<u8>>) -> ActiveWalResult<()> {
        if self.actual_facts.is_none() {
            return Ok(());
        }
        blobs.extend(self.actual_fact_blobs()?);
        // New-profile preparation only. Old sixteen-blob capacity and bytes
        // are untouched. Existing dependencies must verify; corruption is not
        // treated as absence and never triggers an overwrite.
        let mut unique = Vec::<Vec<u8>>::new();
        for raw in blobs.drain(..) {
            let pin = ActiveServiceWalBlobV1::of(&raw)?;
            match self.wal.read_blob(&pin) {
                Ok(existing) => {
                    if existing != raw {
                        return Err(NativeMainActiveWalErrorV1::WrongAction);
                    }
                }
                Err(ActiveServiceWalErrorV1::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    if !unique.iter().any(|existing| existing == &raw) {
                        unique.push(raw);
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        *blobs = unique;
        Ok(())
    }
}
