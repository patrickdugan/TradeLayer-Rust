impl ActualSourceActionFactSlotsV1 {
    fn latest_io(&self) -> Option<&ActualInlineServiceAppendCaptureV1> {
        self.retry_io.last().or(self.io.as_ref())
    }
}
impl NativeMainActualFactVerifiedReopenV1 {
    pub fn prepare_verified_replacement(
        mut self,
    ) -> Result<SourceMainNativeActualFactServiceV1, Box<RejectedNativeMainActualFactVerifiedPostV1>>
    {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let checked = (|| {
            if !matches!(self.candidate, Candidate::Before | Candidate::Intermediate)
                || self.inner.inner.phase != NativeMainActiveWalPhaseV1::NativeSupersessionIntent
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            let controller = &self.inner.inner;
            let service = controller
                .active_service()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let source = service
                .pending_inline_write()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let replacement = controller
                .pending
                .as_ref()
                .and_then(|p| p.native_recovery.as_ref())
                .and_then(|r| r.inline_replacement.as_deref())
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            if source.method() != replacement.method()
                || source.arguments() != replacement.arguments()
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            let mut planned = service
                .actual_inline_fact_selection()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            let mut pending = planned.get("pending");
            pending
                .insert("context", replacement.durable_read_context())
                .map_err(active_wal_backend)?;
            planned
                .insert("pending", pending)
                .map_err(active_wal_backend)?;
            controller.actual_native_alias_from_plan(replacement, Some(planned))
        })();
        let alias = match checked {
            Ok(alias) => alias,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainActualFactVerifiedPostV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        if let Err(error) = self.inner.inner.ensure_actual_io_chain(true) {
            return Err(Box::new(RejectedNativeMainActualFactVerifiedPostV1 {
                cause: error.to_string(),
                owner: self,
            }));
        }
        self.inner
            .inner
            .pending
            .as_mut()
            .expect("same original action")
            .actual_fact_slots
            .as_mut()
            .expect("actual fact slots")
            .native_alias = Some(alias);
        // The existing actual supersession barrier includes the new alias and
        // first real IO-chain link BEFORE install or any native-only retry.
        Ok(self.inner)
    }
}
impl SourceMainNativeActiveServiceV1 {
    fn capture_actual_inline_replacement(
        &mut self,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualNativeFactBindingV1,
    ) -> Result<ActualInlineServiceAppendCaptureV1, NativeMainActiveServiceErrorV1> {
        let selection = self
            .actual_inline_fact_selection()
            .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
        let record = wal
            .record()
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        if record.get("plannedReplacementSelection") != selection
            || !matches!(record.get("stage"),V::String(ref stage) if stage=="NativeSupersessionIntent" || stage=="EscrowBoundIntent")
        {
            return Err(NativeMainActiveServiceErrorV1::InvalidInput);
        }
        let frame = self
            .frame
            .as_mut()
            .ok_or(NativeMainActiveServiceErrorV1::InvalidInput)?;
        let (_stop, attempt) = self
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_append_capture_actual(
                &mut self.owner.index,
                frame,
                permit,
                wal,
                binding,
            )
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        self.attempted_write = frame.pending_native_context();
        // Preserve the FIRST real StorageStopped source observation. The new
        // actual native return is separately recorded, never made source success.
        Ok(ActualInlineServiceAppendCaptureV1 { selection, attempt })
    }
}
impl SourceMainNativeDurableActiveServiceV1 {
    fn ensure_actual_io_chain(&mut self, force: bool) -> ActiveWalResult<()> {
        let Some(slots) = self
            .pending
            .as_mut()
            .and_then(|p| p.actual_fact_slots.as_mut())
        else {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        };
        if slots.io.is_none() || (!force && slots.retry_io.is_empty() && slots.io_chain.is_none()) {
            return Ok(());
        }
        let count = 1 + slots.retry_io.len();
        if slots.io_chain_count == count {
            return Ok(());
        }
        if slots.io_chain_count.checked_add(1) != Some(count) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let io = slots
            .latest_io()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let pending = io.attempt.pending_fact().canonical_fact_bytes();
        let actual = io
            .attempt
            .canonical_io_fact_bytes()
            .map_err(active_wal_backend)?;
        let previous = slots
            .io_chain
            .as_ref()
            .map(|raw| ActiveServiceWalBlobV1::of(raw))
            .transpose()?;
        let raw = active_wal_bytes(&obj(&[
            ("profile", s("TL-MAIN-ACTUAL-INLINE-IO-CHAIN-13-1")),
            ("attempt", s(&count.to_string())),
            (
                "previous",
                previous
                    .as_ref()
                    .map_or(V::Null, ActiveServiceWalBlobV1::value),
            ),
            ("selection", io.selection.clone()),
            ("pendingFact", ActiveServiceWalBlobV1::of(pending)?.value()),
            ("actualIoFact", ActiveServiceWalBlobV1::of(&actual)?.value()),
        ]))?;
        ActiveServiceWalBlobV1::of(&raw)?;
        slots.io_prior = previous;
        slots.io_chain = Some(raw);
        slots.io_chain_count = count;
        Ok(())
    }
    fn append_actual_io_chain_fields(&self, value: &mut V) -> ActiveWalResult<()> {
        if let Some(raw) = self
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
            .and_then(|s| s.io_chain.as_ref())
        {
            value
                .insert(
                    "actualNativeIoChain",
                    ActiveServiceWalBlobV1::of(raw)?.value(),
                )
                .map_err(active_wal_backend)?;
        }
        Ok(())
    }
    fn execute_actual_fact_replacement(&mut self) -> ActiveWalResult<()> {
        // Only an actually installed real replacement, mapped by the previous
        // synced supersession intent, can reach this native-only entry.
        let p = self
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        if !p.executed
            || !p
                .native_recovery
                .as_ref()
                .is_some_and(|r| r.installed && r.inline_mapping.is_some())
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let slots = p
            .actual_fact_slots
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if slots.retry_io.len() >= 64 {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        slots
            .retry_io
            .try_reserve(1)
            .map_err(|_| NativeMainActiveWalErrorV1::Capacity)?;
        let chain = slots
            .io_chain
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if self.wal.read_blob(&ActiveServiceWalBlobV1::of(chain)?)? != *chain {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        // This captures the newly installed real plan at healthy actual Before,
        // before IO/observation. It never recaptures old Before from actual Post.
        let _ = self.capture_actual_inline_fact_input()?;
        // Combined retained-fact budget, not just a vector-slot cap. Reserve
        // one full allowed IO-fact envelope before issuing the next native IO.
        let slots = self
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
            .expect("preflighted fact slots");
        let mut retained = 0usize;
        for io in slots.io.iter().chain(slots.retry_io.iter()) {
            retained = retained
                .checked_add(io.attempt.pending_fact().canonical_fact_bytes().len())
                .and_then(|n| n.checked_add(io.attempt.canonical_io_fact_bytes().ok()?.len()))
                .ok_or(NativeMainActiveWalErrorV1::Capacity)?;
        }
        let next = self
            .actual_facts
            .as_ref()
            .and_then(|f| f.current.as_ref())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .raw
            .len();
        if retained
            .checked_add(next)
            .and_then(|n| n.checked_add(crate::source_main_active_service_wal::MAX_BLOB))
            .is_none_or(|n| n > 256 * 1024 * 1024)
        {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        let service = match self.owner.as_mut() {
            Some(ActiveWalOwner::Active(service)) => service,
            _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
        };
        let write = service
            .pending_inline_write()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let binding = self
            .wal
            .bind_actual_inline_native_fact(&service.owner.index, write)?;
        self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
        let captured = service.capture_actual_inline_replacement(
            self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
            &self.wal,
            &binding,
        );
        let io = match captured {
            Ok(io) => io,
            Err(error) => {
                // Pre-IO fact guards do not mark the opaque plan attempted. A
                // retry here is ONLY this retained native operation, never Main.
                if service
                    .frame
                    .as_ref()
                    .and_then(|f| f.actual_inline_fact_attempted())
                    == Some(false)
                {
                    self.phase = NativeMainActiveWalPhaseV1::NativeSupersessionAppend;
                }
                return Err(error.into());
            }
        };
        let success = io.attempt.actual_result().is_ok();
        self.pending
            .as_mut()
            .expect("same action")
            .actual_fact_slots
            .as_mut()
            .expect("fact slots")
            .retry_io
            .push(io);
        self.actual_facts.as_mut().expect("fact mode").needs_barrier = true;
        self.ensure_actual_io_chain(false)?;
        if !success {
            return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
        }
        let service = self.active_service().expect("same actual owner");
        let context = Self::verify_current_native_post(service)?;
        self.pending.as_mut().expect("same action").append_context = Some(context);
        self.phase = NativeMainActiveWalPhaseV1::Observe;
        Ok(())
    }
}
