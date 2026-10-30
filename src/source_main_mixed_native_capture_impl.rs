// WIP private new-origin hooks. Optional fields are supplied by the separate
// current-text factory/controller transform; old profiles never call these.
impl SourceMainNativeDurableActiveServiceV1 {
    fn capture_mixed_ordinary_plan(&mut self) -> ActiveWalResult<Option<(V, Vec<u8>)>> {
        if self.mixed_ordinary_facts.is_none() {
            return Ok(None);
        }
        let Some(service) = self.active_service() else {
            return Ok(None);
        };
        let selection = match service.actual_mixed_native_selection()? {
            Some(selection @ MixedSourceNativeSelectionV1::Validity(_)) => selection.value(),
            _ => {
                self.mixed_ordinary_facts
                    .as_mut()
                    .expect("explicit mixed profile")
                    .current = None;
                return Ok(None);
            }
        };
        let write = service
            .pending_write()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let facts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),write.durable_read_context()).map_err(active_wal_backend)?;
        let signature = active_wal_bytes(&obj(&[
            (
                "originHead",
                selection.get("actualSource").get("originHead"),
            ),
            (
                "originPhysical",
                selection.get("actualSource").get("originPhysical"),
            ),
            ("pending", selection.get("actualSource").get("pending")),
            ("readContext", write.durable_read_context()),
            ("candidates", facts.independently_retained_facts().clone()),
            (
                "mutationsSHA256",
                s(&hex(&sha256(facts.exact_prepared_mutations()))),
            ),
        ]))?;
        if let Some(cached) = self
            .mixed_ordinary_facts
            .as_ref()
            .and_then(|f| f.current.as_ref())
        {
            if cached.signature == signature {
                return Ok(Some((selection, cached.raw.clone())));
            }
        }
        // A new plan must be captured while its actual Before is healthy. An
        // already held plan at Post/uncertain native storage uses the previous
        // exact cache; no current-state substitution is allowed.
        let binding = self.wal.bind_actual_validity_fact(service.index(), write)?;
        let raw = binding
            .capture_pending_fact(&self.wal, service.index(), write)?
            .canonical_fact_bytes()
            .to_vec();
        self.mixed_ordinary_facts
            .as_mut()
            .expect("explicit mixed profile")
            .current = Some(MixedOrdinaryCachedPlanV1 {
            signature,
            raw: raw.clone(),
        });
        Ok(Some((selection, raw)))
    }
    fn capture_mixed_ordinary_native_once(
        &mut self,
        service: &mut SourceMainNativeActiveServiceV1,
    ) -> ActiveWalResult<()> {
        let expected = service.expectation()?;
        service.escrow_append_preflight(&expected)?;
        let selected = service
            .actual_mixed_native_selection()?
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if !matches!(selected, MixedSourceNativeSelectionV1::Validity(_)) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        if self.wal.record()?.get("ordinarySourceSelection") != selected.value() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let slots = self
            .pending
            .as_mut()
            .and_then(|p| p.mixed_ordinary_slots.as_mut())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if slots.attempt.is_some() || slots.delivered {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        service
            .history
            .try_reserve(1)
            .map_err(|_| NativeMainActiveWalErrorV1::Capacity)?;
        let write = service
            .frame
            .as_ref()
            .and_then(|f| f.pending_write())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let binding = self
            .wal
            .bind_actual_validity_fact(&service.owner.index, write)?;
        let context = write.durable_read_context();
        let attempted = &mut service.attempted_write;
        let actual = service
            .owner
            .index
            .commit_validity_capture_actual_fact_before(
                write,
                self.pending.as_ref().and_then(|p| p.escrow_permit.as_ref()),
                &self.wal,
                &binding,
                || *attempted = Some(context),
            )
            .map_err(active_wal_backend)?;
        // The actual owned native Result is installed before formatting,
        // post-capture, source delivery or observation can return an error.
        let slots = self
            .pending
            .as_mut()
            .and_then(|p| p.mixed_ordinary_slots.as_mut())
            .expect("same explicit slots");
        slots.original_physical_before = Some(expected.physical);
        slots.attempt = Some(MixedActualValidityCaptureV1 {
            selection: selected.value(),
            attempt: actual,
        });
        Ok(())
    }
    fn deliver_mixed_ordinary_after_fact(&mut self) -> ActiveWalResult<()> {
        let Some(slots) = self
            .pending
            .as_mut()
            .and_then(|p| p.mixed_ordinary_slots.as_mut())
        else {
            return Ok(());
        };
        if slots.delivered || slots.attempt.is_none() {
            return Ok(());
        }
        let captured = slots.attempt.as_ref().expect("actual owned native Result");
        let actual = &captured.attempt;
        let raw = actual
            .canonical_io_fact_bytes()
            .map_err(active_wal_backend)?;
        let record = self.wal.record()?;
        if record.get("profile") != s(MIXED_SOURCE_FACT_WAL_PROFILE)
            || record.get("stage") != s("NativeIoObserved")
            || record.get("ordinaryNativeIoFact") != ActiveServiceWalBlobV1::of(&raw)?.value()
            || self.wal.read_blob(&ActiveServiceWalBlobV1::of(&raw)?)? != raw
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let service = match self.owner.as_mut() {
            Some(ActiveWalOwner::Active(service)) => service,
            _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
        };
        if service.pending_observation.is_some() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let before = slots
            .original_physical_before
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .clone();
        let stop = service
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_deliver_actual_validity_result(
                &service.owner.index,
                service
                    .frame
                    .as_mut()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                actual,
            )
            .map_err(active_wal_backend)?;
        // No fallible post-delivery operation may lose this source result.
        service.pending_observation = Some(NativeMainActiveServicePendingV1 {
            action: NativeMainActiveServiceActionV1::AppendPending,
            result: NativeMainActiveServiceActionResultV1::Stop(stop),
            before,
        });
        slots.delivered = true;
        if actual.actual_result().is_err() {
            self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
            return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
        }
        Ok(())
    }
}
