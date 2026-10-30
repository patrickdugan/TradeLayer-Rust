const ACTUAL_SOURCE_FACT_WAL_PROFILE: &str = "TL-MAIN-ACTIVE-SERVICE-ACTUAL-NATIVE-FACTS-WAL-13-2";
const ACTUAL_SOURCE_FACT_SCOPE: &str = "StandaloneSingleInlineSource-1";
#[derive(Default)]
struct ActualSourceFactModeStateV1 {
    current: Option<ActualSourceCachedPlanFactV1>,
    needs_barrier: bool,
    recovery_observation: bool,
}
struct ActualSourceCachedPlanFactV1 {
    signature: Vec<u8>,
    raw: Vec<u8>,
}
#[derive(Default)]
struct ActualSourceFactInputV1 {
    plan: Option<Vec<u8>>,
    selection: Option<V>,
}
#[derive(Default)]
struct ActualSourceActionFactSlotsV1 {
    before: ActualSourceFactInputV1,
    after: ActualSourceFactInputV1,
    io: Option<ActualInlineServiceAppendCaptureV1>,
    retry_io: Vec<ActualInlineServiceAppendCaptureV1>,
    io_chain: Option<Vec<u8>>,
    io_prior: Option<ActiveServiceWalBlobV1>,
    io_chain_count: usize,
    native_alias: Option<ActualSourceNativeAliasV1>,
}
struct ActualSourceNativeAliasV1 {
    selection: V,
    planned_selection: Option<V>,
    history: Vec<u8>,
    context: V,
    candidates: Option<Vec<u8>>,
    mutations: Option<Vec<u8>>,
}
impl SourceMainNativeDurableActiveServiceV1 {
    fn actual_native_alias_from_plan(
        &self,
        write: &crate::source_compact_index_v2::NativePreparedInlineDatabaseV1,
        planned_selection: Option<V>,
    ) -> ActiveWalResult<ActualSourceNativeAliasV1> {
        let service = self
            .active_service()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let selection = service
            .actual_inline_fact_selection()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let point = self
            .index()
            .capture_verified_history_pin()
            .map_err(active_wal_backend)?;
        if &point.metadata != write.expected() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let (candidates, mutations) = if let Some(append) = write.append_plan() {
            let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(append,write.durable_read_context()).map_err(active_wal_backend)?;
            (
                Some(active_wal_bytes(actual.independently_retained_facts())?),
                Some(actual.exact_prepared_mutations().to_vec()),
            )
        } else {
            (None, None)
        };
        Ok(ActualSourceNativeAliasV1 {
            selection,
            planned_selection,
            history: point.canonical_scalar_bytes().map_err(active_wal_backend)?,
            context: write.durable_read_context(),
            candidates,
            mutations,
        })
    }
    fn actual_fact_blobs(&self) -> ActiveWalResult<Vec<Vec<u8>>> {
        let Some(slots) = self
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
        else {
            return Ok(vec![]);
        };
        let mut blobs = vec![];
        for raw in [&slots.before.plan, &slots.after.plan]
            .into_iter()
            .flatten()
        {
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
        if let Some(alias) = &slots.native_alias {
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
    fn append_actual_fact_record_fields(&self, record: &mut V) -> ActiveWalResult<()> {
        let Some(slots) = self
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
        else {
            return Ok(());
        };
        record
            .insert(
                "requiredNativeFactProfile",
                s(crate::source_main_active_service_wal::ACTUAL_NATIVE_FACT_PROFILE),
            )
            .map_err(active_wal_backend)?;
        for (name, input) in [("Before", &slots.before), ("After", &slots.after)] {
            let reference = input
                .plan
                .as_ref()
                .map(|raw| ActiveServiceWalBlobV1::of(raw).map(|p| p.value()))
                .transpose()?
                .unwrap_or(V::Null);
            record
                .insert(&format!("actualPendingPlan{name}"), reference)
                .map_err(active_wal_backend)?;
            record
                .insert(
                    &format!("actualSelection{name}"),
                    input.selection.clone().unwrap_or(V::Null),
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
                    "actualNativeIoFact",
                    ActiveServiceWalBlobV1::of(&raw)?.value(),
                )
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "actualNativeIoPendingFact",
                    ActiveServiceWalBlobV1::of(io.attempt.pending_fact().canonical_fact_bytes())?
                        .value(),
                )
                .map_err(active_wal_backend)?;
            record
                .insert("actualIoSelection", io.selection.clone())
                .map_err(active_wal_backend)?;
        }
        if let Some(alias) = &slots.native_alias {
            record
                .insert("actualSourceSelection", alias.selection.clone())
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "plannedReplacementSelection",
                    alias.planned_selection.clone().unwrap_or(V::Null),
                )
                .map_err(active_wal_backend)?;
            record
                .insert("actualNativeReadContext", alias.context.clone())
                .map_err(active_wal_backend)?;
            record
                .insert(
                    "actualNativeBeforeHistory",
                    ActiveServiceWalBlobV1::of(&alias.history)?.value(),
                )
                .map_err(active_wal_backend)?;
            for (name, raw) in [
                ("actualNativeCandidates", &alias.candidates),
                ("actualNativeMutations", &alias.mutations),
            ] {
                record
                    .insert(
                        name,
                        raw.as_ref()
                            .map(|raw| ActiveServiceWalBlobV1::of(raw).map(|p| p.value()))
                            .transpose()?
                            .unwrap_or(V::Null),
                    )
                    .map_err(active_wal_backend)?;
            }
        }
        self.append_actual_io_chain_fields(record)?;
        Ok(())
    }
    fn capture_actual_inline_fact_input(&mut self) -> ActiveWalResult<ActualSourceFactInputV1> {
        if self.actual_facts.is_none() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let actual = match self.owner.as_ref() {
            Some(ActiveWalOwner::Active(active)) => active,
            _ => return Ok(ActualSourceFactInputV1::default()),
        };
        let Some(write) = actual.pending_inline_write() else {
            self.actual_facts.as_mut().expect("fact mode").current = None;
            return Ok(ActualSourceFactInputV1::default());
        };
        let selection = actual
            .actual_inline_fact_selection()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let (candidates, mutations) = if let Some(append) = write.append_plan() {
            let value=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(append,write.durable_read_context()).map_err(active_wal_backend)?;
            (
                value.independently_retained_facts().clone(),
                s(&hex(value.exact_prepared_mutations())),
            )
        } else {
            (V::Null, V::Null)
        };
        // The native plan signature includes full predictions/config/TLNM and
        // actual source identity, not just method or a mutable display string.
        let signature = active_wal_bytes(&obj(&[
            ("originHead", selection.get("originHead")),
            ("originPhysical", selection.get("originPhysical")),
            ("pending", selection.get("pending")),
            ("readContext", write.durable_read_context()),
            ("candidates", candidates),
            ("mutations", mutations),
        ]))?;
        if let Some(cached) = self.actual_facts.as_ref().and_then(|f| f.current.as_ref()) {
            if cached.signature == signature {
                return Ok(ActualSourceFactInputV1 {
                    plan: Some(cached.raw.clone()),
                    selection: Some(selection),
                });
            }
        }
        // A newly created source plan must still name its actual healthy
        // Before. An old held plan at Post must reuse its prior actual fact.
        let binding = self
            .wal
            .bind_actual_inline_native_fact(&actual.owner.index, write)?;
        let raw = binding
            .capture_pending_plan_fact(&self.wal, &actual.owner.index, write)?
            .canonical_fact_bytes()
            .to_vec();
        self.actual_facts.as_mut().expect("fact mode").current =
            Some(ActualSourceCachedPlanFactV1 {
                signature,
                raw: raw.clone(),
            });
        Ok(ActualSourceFactInputV1 {
            plan: Some(raw),
            selection: Some(selection),
        })
    }
    fn append_actual_facts_scope_observation(raw: &mut Vec<u8>) -> ActiveWalResult<()> {
        Self::append_actual_facts_scope_observation_for(raw, ACTUAL_SOURCE_FACT_SCOPE)
    }
    fn append_actual_facts_scope_observation_for(
        raw: &mut Vec<u8>,
        scope: &str,
    ) -> ActiveWalResult<()> {
        let mut fields = NativeFieldCollector::new("Native13/ActualNativeFactsMode/v1", true, 1024);
        fields
            .emit(
                "requiredProfile",
                &s(crate::source_main_active_service_wal::ACTUAL_NATIVE_FACT_PROFILE),
            )
            .map_err(active_wal_backend)?;
        fields
            .emit("sourceScope", &s(scope))
            .map_err(active_wal_backend)?;
        raw.extend_from_slice(fields.frames.as_deref().expect("retained scope"));
        Ok(())
    }
}
