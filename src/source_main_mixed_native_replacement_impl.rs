impl SourceMainNativeDurableActiveServiceV1 {
    fn prepare_mixed_ordinary_replacement(&mut self) -> ActiveWalResult<()> {
        if self.phase != NativeMainActiveWalPhaseV1::NativeSupersessionIntent {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let service = self
            .active_service()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let source = service
            .pending_write()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let replacement = self
            .pending
            .as_ref()
            .and_then(|p| p.native_recovery.as_ref())
            .and_then(|r| r.replacement.as_deref())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if source.query() != replacement.query()
            || source.update_arguments() != replacement.update_arguments()
            || source.pinned_read() != replacement.pinned_read()
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let mut planned = service
            .actual_mixed_native_selection()?
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .value();
        let mut actual = planned.get("actualSource");
        let mut pending = actual.get("pending");
        pending
            .insert("context", replacement.durable_read_context())
            .map_err(active_wal_backend)?;
        actual
            .insert("pending", pending)
            .map_err(active_wal_backend)?;
        planned
            .insert("actualSource", actual)
            .map_err(active_wal_backend)?;
        let alias = self.mixed_ordinary_alias_from_plan(replacement, Some(planned))?;
        self.ensure_mixed_ordinary_io_chain(true)?;
        self.pending
            .as_mut()
            .and_then(|p| p.mixed_ordinary_slots.as_mut())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .alias = Some(alias);
        Ok(())
    }
    fn execute_mixed_ordinary_replacement(&mut self) -> ActiveWalResult<()> {
        let pending = self
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        if !pending.executed
            || !pending
                .native_recovery
                .as_ref()
                .is_some_and(|r| r.installed && r.mapping.is_some())
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let slots = pending
            .mixed_ordinary_slots
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if !slots.delivered || slots.retries.len() >= 63 {
            return Err(NativeMainActiveWalErrorV1::Capacity);
        }
        slots
            .retries
            .try_reserve(1)
            .map_err(|_| NativeMainActiveWalErrorV1::Capacity)?;
        let chain = slots
            .io_chain
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if self.wal.read_blob(&ActiveServiceWalBlobV1::of(chain)?)? != *chain {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let _ = self.capture_mixed_ordinary_plan()?;
        let slots = self
            .pending
            .as_ref()
            .and_then(|p| p.mixed_ordinary_slots.as_ref())
            .expect("preflighted actual slots");
        let mut retained = 0usize;
        for io in slots.attempt.iter().chain(slots.retries.iter()) {
            retained = retained
                .checked_add(io.attempt.pending_fact().canonical_fact_bytes().len())
                .and_then(|n| n.checked_add(io.attempt.canonical_io_fact_bytes().ok()?.len()))
                .ok_or(NativeMainActiveWalErrorV1::Capacity)?;
        }
        let next = self
            .mixed_ordinary_facts
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
        let selected = service
            .actual_mixed_native_selection()?
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if !matches!(selected, MixedSourceNativeSelectionV1::Validity(_)) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let record = self.wal.record()?;
        if record.get("ordinaryPlannedReplacementSelection") != selected.value()
            || !matches!(record.get("stage"),V::String(ref s) if s=="NativeSupersessionIntent"||s=="EscrowBoundIntent")
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let write = service
            .frame
            .as_ref()
            .and_then(|f| f.pending_write())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let binding = self.wal.bind_actual_validity_fact(service.index(), write)?;
        let context = write.durable_read_context();
        let attempted = &mut service.attempted_write;
        let phase = &mut self.phase;
        let actual = service
            .owner
            .index
            .commit_validity_capture_actual_fact_before(
                write,
                self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
                &self.wal,
                &binding,
                || {
                    *attempted = Some(context);
                    *phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
                },
            )
            .map_err(active_wal_backend)?;
        let success = actual.actual_result().is_ok();
        self.pending
            .as_mut()
            .and_then(|p| p.mixed_ordinary_slots.as_mut())
            .expect("reserved same slots")
            .retries
            .push(MixedActualValidityCaptureV1 {
                selection: selected.value(),
                attempt: actual,
            });
        self.actual_facts
            .as_mut()
            .expect("mixed fact mode")
            .needs_barrier = true;
        self.ensure_mixed_ordinary_io_chain(false)?;
        if !success {
            return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
        }
        let context =
            Self::verify_current_native_post(self.active_service().expect("same actual owner"))?;
        self.pending
            .as_mut()
            .expect("same executed source action")
            .append_context = Some(context);
        self.phase = NativeMainActiveWalPhaseV1::Observe;
        Ok(())
    }
}
