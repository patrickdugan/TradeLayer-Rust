impl SourceMainNativeDurableActiveServiceV1 {
    fn mixed_ordinary_alias_from_plan(
        &self,
        write: &crate::source_compact_index_v2::NativePreparedValidityWriteV1,
        planned: Option<V>,
    ) -> ActiveWalResult<ActualSourceNativeAliasV1> {
        let service = self
            .active_service()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let selected = service
            .actual_mixed_native_selection()?
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if !matches!(selected, MixedSourceNativeSelectionV1::Validity(_)) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let history = service
            .index()
            .capture_verified_history_pin()
            .map_err(active_wal_backend)?;
        if &history.metadata != write.write_expected() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),write.durable_read_context()).map_err(active_wal_backend)?;
        Ok(ActualSourceNativeAliasV1 {
            selection: selected.value(),
            planned_selection: planned,
            history: history
                .canonical_scalar_bytes()
                .map_err(active_wal_backend)?,
            context: write.durable_read_context(),
            candidates: Some(active_wal_bytes(candidates.independently_retained_facts())?),
            mutations: Some(candidates.exact_prepared_mutations().to_vec()),
        })
    }
    fn mixed_ordinary_action_alias(
        &self,
        action: &NativeMainActiveServiceActionV1,
    ) -> ActiveWalResult<Option<ActualSourceNativeAliasV1>> {
        if self.mixed_ordinary_facts.is_none()
            || !matches!(action, NativeMainActiveServiceActionV1::AppendPending)
        {
            return Ok(None);
        }
        let Some(service) = self.active_service() else {
            return Ok(None);
        };
        let Ok(expected) = service.expectation() else {
            return Ok(None);
        };
        if service.escrow_append_preflight(&expected).is_err() {
            return Ok(None);
        }
        match service.actual_mixed_native_selection()? {
            Some(MixedSourceNativeSelectionV1::Validity(_)) => self
                .mixed_ordinary_alias_from_plan(
                    service
                        .pending_write()
                        .ok_or(NativeMainActiveWalErrorV1::WrongAction)?,
                    None,
                )
                .map(Some),
            _ => Ok(None),
        }
    }
    fn mixed_ordinary_blobs(&self) -> ActiveWalResult<Vec<Vec<u8>>> {
        let Some(slots) = self
            .pending
            .as_ref()
            .and_then(|p| p.mixed_ordinary_slots.as_ref())
        else {
            return Ok(vec![]);
        };
        let mut blobs = vec![];
        for (_, raw) in [&slots.before, &slots.after].into_iter().flatten() {
            blobs.push(raw.clone());
        }
        if let Some(io) = slots.latest_io() {
            blobs.push(io.attempt.pending_fact().canonical_fact_bytes().to_vec());
            blobs.push(
                io.attempt
                    .canonical_io_fact_bytes()
                    .map_err(active_wal_backend)?,
            );
        }
        if let Some(alias) = &slots.alias {
            blobs.push(alias.history.clone());
            for raw in [&alias.candidates, &alias.mutations].into_iter().flatten() {
                blobs.push(raw.clone());
            }
        }
        if let Some(raw) = &slots.io_chain {
            blobs.push(raw.clone());
        }
        Ok(blobs)
    }
    fn append_mixed_ordinary_record(&self, record: &mut V) -> ActiveWalResult<()> {
        let Some(slots) = self
            .pending
            .as_ref()
            .and_then(|p| p.mixed_ordinary_slots.as_ref())
        else {
            return Ok(());
        };
        let inline = self
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
            .is_some_and(|s| s.native_alias.is_some());
        if inline && slots.alias.is_some() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        record
            .insert(
                "mixedOperationKind",
                s(if inline {
                    "InlineDatabase"
                } else if slots.alias.is_some() {
                    "OrdinaryValidity"
                } else {
                    "SourceAction"
                }),
            )
            .map_err(active_wal_backend)?;
        record
            .insert(
                "requiredOrdinaryNativeFactProfile",
                s(crate::source_main_active_service_wal::ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
            )
            .map_err(active_wal_backend)?;
        for (name, input) in [("Before", &slots.before), ("After", &slots.after)] {
            record
                .insert(
                    &format!("ordinaryPendingPlan{name}"),
                    input
                        .as_ref()
                        .map(|(_, raw)| ActiveServiceWalBlobV1::of(raw).map(|p| p.value()))
                        .transpose()?
                        .unwrap_or(V::Null),
                )
                .map_err(active_wal_backend)?;
            record
                .insert(
                    &format!("ordinarySelection{name}"),
                    input
                        .as_ref()
                        .map_or(V::Null, |(selection, _)| selection.clone()),
                )
                .map_err(active_wal_backend)?;
        }
        if let Some(io) = slots.latest_io() {
            let raw = io
                .attempt
                .canonical_io_fact_bytes()
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "ordinaryNativeIoFact",
                    ActiveServiceWalBlobV1::of(&raw)?.value(),
                )
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "ordinaryNativeIoPendingFact",
                    ActiveServiceWalBlobV1::of(io.attempt.pending_fact().canonical_fact_bytes())?
                        .value(),
                )
                .map_err(active_wal_backend)?;
            record
                .insert("ordinaryIoSelection", io.selection.clone())
                .map_err(active_wal_backend)?;
        }
        if let Some(alias) = &slots.alias {
            record
                .insert("ordinarySourceSelection", alias.selection.clone())
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "ordinaryPlannedReplacementSelection",
                    alias.planned_selection.clone().unwrap_or(V::Null),
                )
                .map_err(active_wal_backend)?;
            record
                .insert("ordinaryNativeReadContext", alias.context.clone())
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "ordinaryNativeBeforeHistory",
                    ActiveServiceWalBlobV1::of(&alias.history)?.value(),
                )
                .map_err(active_wal_backend)?;
            for (key, raw) in [
                ("ordinaryNativeCandidates", &alias.candidates),
                ("ordinaryNativeMutations", &alias.mutations),
            ] {
                record
                    .insert(
                        key,
                        raw.as_ref()
                            .map(|raw| ActiveServiceWalBlobV1::of(raw).map(|p| p.value()))
                            .transpose()?
                            .unwrap_or(V::Null),
                    )
                    .map_err(active_wal_backend)?;
            }
        }
        if let Some(raw) = &slots.io_chain {
            record
                .insert(
                    "ordinaryNativeIoChain",
                    ActiveServiceWalBlobV1::of(raw)?.value(),
                )
                .map_err(active_wal_backend)?;
        }
        Ok(())
    }
    fn ensure_mixed_ordinary_io_chain(&mut self, force: bool) -> ActiveWalResult<()> {
        let Some(slots) = self
            .pending
            .as_mut()
            .and_then(|p| p.mixed_ordinary_slots.as_mut())
        else {
            return Ok(());
        };
        if slots.attempt.is_none()
            || (!force && slots.retries.is_empty() && slots.io_chain.is_none())
        {
            return Ok(());
        }
        let count = 1 + slots.retries.len();
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
        let raw_io = io
            .attempt
            .canonical_io_fact_bytes()
            .map_err(active_wal_backend)?;
        let previous = slots
            .io_chain
            .as_ref()
            .map(|raw| ActiveServiceWalBlobV1::of(raw))
            .transpose()?;
        let raw = active_wal_bytes(&obj(&[
            ("profile", s("TL-MAIN-ACTUAL-ORDINARY-IO-CHAIN-13-1")),
            ("attempt", s(&count.to_string())),
            (
                "previous",
                previous
                    .as_ref()
                    .map_or(V::Null, ActiveServiceWalBlobV1::value),
            ),
            ("selection", io.selection.clone()),
            ("pendingFact", ActiveServiceWalBlobV1::of(pending)?.value()),
            ("actualIoFact", ActiveServiceWalBlobV1::of(&raw_io)?.value()),
        ]))?;
        ActiveServiceWalBlobV1::of(&raw)?;
        slots.io_prior = previous;
        slots.io_chain = Some(raw);
        slots.io_chain_count = count;
        Ok(())
    }
}
