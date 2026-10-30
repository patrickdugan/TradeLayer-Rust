const ACTIVE_ESCROW_WAL_PROFILE: &str = "TL-MAIN-ACTIVE-SERVICE-ESCROW-WAL-13-1";
#[derive(Clone, Debug)]
struct ActiveServiceEscrowConfigV1 {
    root: PathBuf,
    durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    inline_profile: bool,
}
#[derive(Clone, Copy)]
enum ActiveEscrowRoute {
    Action,
    NativeReplacement,
}
impl SourceMainNativeDurableActiveServiceV1 {
    pub fn create_with_generation_escrow(
        owner: SourceMainNativeSessionV13,
        wal_root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        escrow_root: PathBuf,
        durability: crate::compact_journal::GenerationEscrowDurabilityV1,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_generation_escrow_internal(
            owner,
            wal_root,
            namespace,
            filesystem,
            observation_limit,
            (escrow_root, durability, false),
        )
    }
    fn create_generation_escrow_internal(
        owner: SourceMainNativeSessionV13,
        wal_root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        entry: (
            PathBuf,
            crate::compact_journal::GenerationEscrowDurabilityV1,
            bool,
        ),
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        let (root, durability, inline) = entry;
        Self::create_generation_escrow_fact_entry(
            owner,
            wal_root,
            namespace,
            filesystem,
            observation_limit,
            (root, durability, inline, false),
        )
    }
    fn create_generation_escrow_fact_entry(
        owner: SourceMainNativeSessionV13,
        wal_root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
        entry: (
            PathBuf,
            crate::compact_journal::GenerationEscrowDurabilityV1,
            bool,
            bool,
        ),
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        let (escrow_root, durability, inline_profile, actual_fact_mode) = entry;
        let checked = (|| {
            let metadata = std::fs::symlink_metadata(&escrow_root).map_err(active_wal_backend)?;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || !escrow_root.is_absolute()
                || escrow_root.canonicalize().map_err(active_wal_backend)? != escrow_root
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
            }
            let (_, catalog) = owner
                .index()
                .native_store_context()
                .map_err(active_wal_backend)?;
            if catalog.starts_with(&escrow_root) || escrow_root.starts_with(&catalog) {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            #[cfg(not(unix))]
            if durability
                == crate::compact_journal::GenerationEscrowDurabilityV1::FileAndDirectorySync
            {
                return Err(NativeMainActiveWalErrorV1::Backend(
                    "directory sync unavailable for explicit escrow mode".into(),
                ));
            }
            Ok(ActiveServiceEscrowConfigV1 {
                root: escrow_root,
                durability,
                inline_profile,
            })
        })();
        match checked {
            Ok(config) => Self::create_internal_fact_entry(
                owner,
                wal_root,
                namespace,
                filesystem,
                observation_limit,
                true,
                (Some(config), actual_fact_mode),
            ),
            Err(error) => Err(Box::new(RejectedNativeMainActiveWalV1 { error, owner })),
        }
    }

    fn after_raw_action_intent(&mut self) -> ActiveWalResult<()> {
        if self.generation_escrow.is_some()
            && self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.action, NativeMainActiveServiceActionV1::AppendPending))
        {
            self.begin_escrow_route(ActiveEscrowRoute::Action);
        } else {
            self.execute_once()?;
            self.phase = NativeMainActiveWalPhaseV1::Observe;
        }
        Ok(())
    }
    fn after_native_supersession_intent(&mut self) -> ActiveWalResult<()> {
        self.install_native_replacement()?;
        if self.generation_escrow.is_some() {
            self.pending.as_mut().expect("pending").escrow_permit = None;
            self.begin_escrow_route(ActiveEscrowRoute::NativeReplacement);
        } else {
            self.phase = NativeMainActiveWalPhaseV1::NativeSupersessionAppend;
        }
        Ok(())
    }

    fn execute_once(&mut self) -> ActiveWalResult<()> {
        if self.generation_escrow.is_none() {
            return self.execute_once_unscoped();
        }
        let scope = match self.owner.as_mut().expect("same actual owner") {
            ActiveWalOwner::Ready(owner) => owner.index.begin_inline_escrow_guard(),
            ActiveWalOwner::Active(owner) => owner.owner.index.begin_inline_escrow_guard(),
        }
        .map_err(active_wal_backend)?;
        let recorded = if self.mixed_ordinary_facts.is_some() {
            let started = match self.owner.as_mut().expect("same mixed owner") {
                ActiveWalOwner::Ready(owner) => owner.index.begin_recorded_native_only_scope(),
                ActiveWalOwner::Active(owner) => {
                    owner.owner.index.begin_recorded_native_only_scope()
                }
            };
            match started {
                Ok(scope) => Some(scope),
                Err(error) => {
                    match self.owner.as_mut().expect("same owner") {
                        ActiveWalOwner::Ready(owner) => {
                            owner.index.finish_inline_escrow_guard(scope)
                        }
                        ActiveWalOwner::Active(owner) => {
                            owner.owner.index.finish_inline_escrow_guard(scope)
                        }
                    }
                    .map_err(active_wal_backend)?;
                    return Err(active_wal_backend(error));
                }
            }
        } else {
            None
        };
        let result = self.execute_once_unscoped();
        let recorded_cleared = if let Some(scope) = recorded {
            match self.owner.as_mut().expect("same retained mixed owner") {
                ActiveWalOwner::Ready(owner) => {
                    owner.index.finish_recorded_native_only_scope(scope)
                }
                ActiveWalOwner::Active(owner) => {
                    owner.owner.index.finish_recorded_native_only_scope(scope)
                }
            }
        } else {
            Ok(())
        };
        // Clear using only the retained private token, even when the actual
        // index is poisoned. No replacement writer/snapshot is manufactured.
        let cleared = match self
            .owner
            .as_mut()
            .expect("actual owner retained on every result")
        {
            ActiveWalOwner::Ready(owner) => owner.index.finish_inline_escrow_guard(scope),
            ActiveWalOwner::Active(owner) => owner.owner.index.finish_inline_escrow_guard(scope),
        };
        cleared.map_err(active_wal_backend)?;
        recorded_cleared.map_err(active_wal_backend)?;
        result
    }
    fn owner_observation(&self) -> ActiveWalResult<Vec<u8>> {
        let mut raw = self.owner_observation_unscoped()?;
        if self.generation_escrow.is_some() {
            Self::append_escrow_scope_observation(
                &mut raw,
                self.index().inline_escrow_scope_active(),
            )?;
        }
        if self.actual_facts.is_some() {
            if self.mixed_ordinary_facts.is_some() {
                Self::append_actual_facts_scope_observation_for(&mut raw, MIXED_SOURCE_FACT_SCOPE)?;
            } else {
                Self::append_actual_facts_scope_observation(&mut raw)?;
            }
        }
        if self.mixed_ordinary_facts.is_some() {
            Self::append_mixed_ordinary_scope_observation(&mut raw)?;
            Self::append_mixed_recorded_scope(
                &mut raw,
                self.index().recorded_native_only_scope_active(),
            )?;
        }
        self.append_segment_observation(&mut raw)?;
        Ok(raw)
    }
    fn append_escrow_scope_observation(raw: &mut Vec<u8>, active: bool) -> ActiveWalResult<()> {
        let mut fields = NativeFieldCollector::new("Native13/ActiveEscrowScope/v1", true, 1024);
        fields
            .emit("inlineEscrowScopeActive", &V::Bool(active))
            .map_err(active_wal_backend)?;
        raw.extend_from_slice(fields.frames.as_deref().expect("retained"));
        Ok(())
    }
    fn begin_escrow_route(&mut self, route: ActiveEscrowRoute) {
        self.pending
            .as_mut()
            .expect("actual pending action")
            .escrow_route = Some(route);
        self.phase = NativeMainActiveWalPhaseV1::EscrowPrepare;
    }
    fn prepare_active_generation_escrow(&mut self) -> ActiveWalResult<bool> {
        let config = self
            .generation_escrow
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .clone();
        let Some(ActiveWalOwner::Active(service)) = self.owner.as_mut() else {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        };
        // A genuine typed pre-execution rejection must not cause archive IO.
        if matches!(
            self.pending.as_ref().and_then(|p| p.escrow_route),
            Some(ActiveEscrowRoute::Action)
        ) {
            let expected = service.expectation()?;
            if service.escrow_append_preflight(&expected).is_err() {
                self.pending.as_mut().expect("pending").escrow_permit = None;
                return Ok(false);
            }
        }
        let (read_context, append) = Self::pending_native_action_parts(service)?;
        if append.is_none_or(|plan| plan.intermediate().is_none()) {
            self.pending.as_mut().expect("pending").escrow_permit = None;
            return Ok(false);
        }
        let point = service
            .owner
            .index
            .capture_verified_history_pin()
            .map_err(active_wal_backend)?;
        let permit = service
            .owner
            .index
            .retain_verified_history_generation(
                &config.root,
                std::slice::from_ref(&point),
                config.durability,
            )
            .map_err(active_wal_backend)?;
        let mut fields =
            NativeFieldCollector::new("Native13/ActualGenerationEscrow/v1", true, 1024 * 1024);
        permit
            .independently_retained_pin()
            .visit_canonical_fields(&mut |p, v| fields.emit(p, v))
            .map_err(active_wal_backend)?;
        let previous = self
            .pending
            .as_ref()
            .and_then(|p| p.escrow_chain.as_ref())
            .map(|raw| ActiveServiceWalBlobV1::of(raw))
            .transpose()?;
        let raw = active_wal_bytes(&obj(&[
            ("profile", s("TL-ACTIVE-GENERATION-ESCROW-CHAIN-13-1")),
            ("previous", previous.as_ref().map_or(V::Null, |p| p.value())),
            ("nativeReadContext", read_context),
            (
                "expectedEscrowFieldsHex",
                s(&hex(fields
                    .frames
                    .as_deref()
                    .expect("actual retained fields"))),
            ),
        ]))?;
        let p = self.pending.as_mut().expect("pending");
        p.escrow_permit = Some(permit);
        p.escrow_chain = Some(raw);
        p.escrow_prior = previous;
        Ok(true)
    }
    fn execute_escrow_route(&mut self) -> ActiveWalResult<()> {
        let route = self
            .pending
            .as_ref()
            .and_then(|p| p.escrow_route)
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        match route {
            ActiveEscrowRoute::Action => {
                self.execute_once()?;
                self.phase = NativeMainActiveWalPhaseV1::Observe;
            }
            ActiveEscrowRoute::NativeReplacement => {
                if self.mixed_ordinary_facts.is_some()
                    && self
                        .active_service()
                        .is_some_and(|s| s.pending_write().is_some())
                {
                    return self.execute_mixed_ordinary_replacement();
                }
                if self.actual_facts.is_some() {
                    return self.execute_actual_fact_replacement();
                }
                // Mark attempted before native IO; no error permits blind retry.
                self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
                let Some(ActiveWalOwner::Active(service)) = self.owner.as_mut() else {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                };
                let frame = service
                    .frame
                    .as_mut()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
                let permit = self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref());
                let stop = service
                    .owner
                    .financial
                    .native_main_runtime_mut()
                    .native_main_service_append_with_escrow(
                        &mut service.owner.index,
                        frame,
                        permit,
                    );
                if stop != NativeMainServiceStopV1::AwaitNativeWrite {
                    return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
                }
                self.pending.as_mut().expect("pending").append_context =
                    Some(Self::verify_current_native_post(service)?);
                self.phase = NativeMainActiveWalPhaseV1::Observe;
            }
        }
        Ok(())
    }
    fn advance_escrow_barrier(&mut self) -> ActiveWalResult<()> {
        match self.phase {
            NativeMainActiveWalPhaseV1::EscrowPrepare => {
                if self.prepare_active_generation_escrow()? {
                    self.phase = NativeMainActiveWalPhaseV1::EscrowBoundIntent;
                } else {
                    self.execute_escrow_route()?;
                }
            }
            NativeMainActiveWalPhaseV1::EscrowBoundIntent => {
                self.write_barrier("EscrowBoundIntent")?;
                self.execute_escrow_route()?;
            }
            _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
        }
        Ok(())
    }
}
