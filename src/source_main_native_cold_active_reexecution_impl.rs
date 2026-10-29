use crate::source_compact_index_v2::{
    NativeColdIndexExecutionV1, NativeHistoricalValidityPlaybackV1, SourceIndexHistoryPinV1,
};
use crate::source_main_active_service_wal::VerifiedActiveServiceAckHistoryV1;

struct NativeMainColdServiceStateV1 {
    owner: NativeMainFinancialWithoutIndexV1,
    frame: Option<Box<NativeMainServiceContinuationV1>>,
    source_start_error: Option<SourceRuleError>,
    origin: NativeMainSessionExpectation,
    head: NativeMainActiveServiceHeadV1,
    history: Vec<NativeMainActiveServiceReceiptV1>,
    pending_observation: Option<NativeMainActiveServicePendingV1>,
    attempted_write: Option<V>,
    observation_limit: usize,
    started: bool,
}
/// Read-only verified financial/source reexecution of an entire acknowledged
/// HISTORY-WAL transcript. No live Session/head, writer or replay owner escapes.
pub struct NativeMainColdAcknowledgedServiceV1 {
    state: NativeMainColdServiceStateV1,
    index: SourceCompactIndex,
    capability: NativeColdIndexExecutionV1,
    transcript: VerifiedActiveServiceAckHistoryV1,
    verified_actions: usize,
}
pub struct RejectedNativeMainColdAcknowledgedServiceV1 {
    pub cause: String,
    owner: NativeMainColdAcknowledgedServiceV1,
}
impl core::fmt::Debug for NativeMainColdAcknowledgedServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeMainColdAcknowledgedServiceV1")
            .field("verified_actions", &self.verified_actions)
            .finish_non_exhaustive()
    }
}
impl core::fmt::Debug for RejectedNativeMainColdAcknowledgedServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainColdAcknowledgedServiceV1")
            .field("cause", &self.cause)
            .field("verified_actions", &self.owner.verified_actions)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainColdAcknowledgedServiceV1 {
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial_runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.owner.index()
    }
    pub fn verified_actions(&self) -> usize {
        self.owner.verified_actions
    }
}
impl NativeMainColdAcknowledgedServiceV1 {
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.state.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.index
    }
    pub fn verified_actions(&self) -> usize {
        self.verified_actions
    }
    pub fn original_action_receipts(&self) -> &[NativeMainActiveServiceReceiptV1] {
        &self.state.history
    }
    pub fn acknowledged_wal_head(&self) -> [u8; 32] {
        self.transcript.head()
    }
    pub fn actual_post(&self) -> &SourceIndexHistoryPinV1 {
        self.capability.original_current()
    }
    pub fn reexecute(
        prefix: NativeMainColdPrefixV1,
        transcript: VerifiedActiveServiceAckHistoryV1,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainColdAcknowledgedServiceV1>> {
        let NativeMainColdPrefixV1 {
            parts,
            capability,
            completed_tip,
        } = prefix;
        let NativeMainHistoricalColdPartsV1 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
        } = parts;
        let origin = NativeMainSessionExpectation {
            session: head.clone(),
            physical: cold_prefix_physical(&completed_tip),
        };
        let state = NativeMainColdServiceStateV1 {
            owner: NativeMainFinancialWithoutIndexV1 {
                financial,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head,
                history,
                halted: false,
            },
            frame: None,
            source_start_error: None,
            origin,
            head: NativeMainActiveServiceHeadV1 {
                actions: 0,
                commitment: [0; 32],
            },
            history: vec![],
            pending_observation: None,
            attempted_write: None,
            observation_limit,
            started: false,
        };
        let mut owner = Self {
            state,
            index,
            capability,
            transcript,
            verified_actions: 0,
        };
        let result = owner.run(&completed_tip);
        match result {
            Ok(()) => Ok(owner),
            Err(cause) => Err(Box::new(RejectedNativeMainColdAcknowledgedServiceV1 {
                cause,
                owner,
            })),
        }
    }
    fn run(&mut self, completed_tip: &SourceIndexHistoryPinV1) -> Result<(), String> {
        if self.state.observation_limit == 0
            || self.state.observation_limit > ACTIVE_WAL_OBSERVATION_LIMIT
        {
            return Err("invalid raw observation limit".into());
        }
        let archive_pin = NativeMainArchivePin {
            head: self.state.owner.head.clone(),
            foundation: self.state.owner.foundation_head,
            origin: self.state.owner.origin_physical.clone(),
            current: cold_prefix_physical(completed_tip),
            archive_sha256: sha256(self.transcript.origin_archive()),
        };
        let archive = inspect_archive(self.transcript.origin_archive(), &archive_pin)
            .map_err(|e| format!("origin archive: {e:?}"))?;
        if archive.foundation != self.state.owner.foundation
            || archive.binding != self.state.owner.binding
            || archive.origin_rows != logical_read(&self.state.owner.origin_rows)
            || archive.entries.len() != self.state.owner.history.len()
        {
            return Err("original completed prefix does not bind WAL archive".into());
        }
        for (at, ((command, encoded), actual)) in archive
            .entries
            .iter()
            .zip(&self.state.owner.history)
            .enumerate()
        {
            if command != &actual.command
                || *encoded != logical_receipt_bytes(actual).map_err(|e| format!("{e:?}"))?
            {
                return Err("completed original receipt mismatch".into());
            }
            if archive.physical_entries.get(at)
                != Some(&(
                    physical_value(&actual.before),
                    actual.after.as_ref().map_or(V::Null, physical_value),
                ))
            {
                return Err("completed original physical receipt mismatch".into());
            }
        }
        let ns = self.index.native_namespace().clone();
        let origin = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            &ns,
            self.transcript.origin_history(),
        )
        .map_err(|e| e.to_string())?;
        if origin != *completed_tip {
            return Err("WAL origin does not match verified completed endpoint".into());
        }
        let mut boundaries = vec![origin.clone()];
        let mut before_pins = vec![];
        let mut post_pins = vec![];
        let mut actions = vec![];
        for (at, entry) in self.transcript.entries().iter().enumerate() {
            let action =
                NativeMainActiveServiceActionV1::from_canonical_action_bytes(entry.raw_action())
                    .map_err(|e| format!("raw action {at}: {e:?}"))?;
            let before = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                &ns,
                entry.verified_history_before(),
            )
            .map_err(|e| e.to_string())?;
            let post = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                &ns,
                entry.verified_history_post(),
            )
            .map_err(|e| e.to_string())?;
            if &before != boundaries.last().expect("origin retained") {
                return Err(format!("metadata Before discontinuity at {at}"));
            }
            if entry.native_recovery().is_some() {
                return Err("cold native recovery/supersession proof not admitted".into());
            }
            if at == 0 && !matches!(action, NativeMainActiveServiceActionV1::Start { .. }) {
                return Err("history lacks raw Start".into());
            }
            if at > 0 && matches!(action, NativeMainActiveServiceActionV1::Start { .. }) {
                return Err("duplicate raw Start".into());
            }
            if before != post {
                if !matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                    || before.metadata.publication != post.metadata.publication
                    || before.metadata.sequence.checked_add(1) != Some(post.metadata.sequence)
                    || entry.native_mutations().is_none()
                    || entry.native_candidates().is_none()
                {
                    return Err("unsupported native transition".into());
                }
                boundaries.push(post.clone());
            } else if matches!(action, NativeMainActiveServiceActionV1::AppendPending) {
                return Err("unresolved/rejected native append is not cold replayable".into());
            }
            if !matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                && (entry.native_mutations().is_some() || entry.native_candidates().is_some())
            {
                return Err("native evidence on non-append action".into());
            }
            before_pins.push(before);
            post_pins.push(post);
            actions.push(action);
        }
        if boundaries.last() != Some(self.capability.original_current()) {
            return Err("final actual Post differs from full acknowledged history".into());
        }
        let mut history = NativeHistoricalValidityPlaybackV1::open(&mut self.index, &boundaries)
            .map_err(|e| e.to_string())?;
        let origin_observation = self
            .transcript
            .read_reference(self.transcript.origin_observation())
            .map_err(|e| e.to_string())?;
        if self.state.observation(
            &origin,
            history.source_actual_rows().map_err(|e| e.to_string())?,
        )? != origin_observation
        {
            return Err("actual reconstructed origin observation mismatch".into());
        }
        for (at, ((entry, action), (before, post))) in self
            .transcript
            .entries()
            .iter()
            .zip(actions)
            .zip(before_pins.iter().zip(&post_pins))
            .enumerate()
        {
            if history.source_history_pin() != before
                || self
                    .state
                    .observation(
                        before,
                        history.source_actual_rows().map_err(|e| e.to_string())?,
                    )?
                    .as_slice()
                    != entry.actual_before()
            {
                return Err(format!("actual source Before mismatch at {at}"));
            }
            if let Some(raw) = entry.native_candidates() {
                self.state.verify_candidates(raw, before, post)?;
            }
            let api_rejection = self
                .state
                .execute(
                    &mut history,
                    &self.capability,
                    action,
                    entry.native_mutations(),
                )
                .err();
            if history.source_history_pin() != post {
                return Err(format!("actual source metadata Post mismatch at {at}"));
            }
            let fields = self.state.observation(
                post,
                history.source_actual_rows().map_err(|e| e.to_string())?,
            )?;
            let mut accepted = b"TLMAP\x01".to_vec();
            accepted.extend_from_slice(&(fields.len() as u64).to_be_bytes());
            accepted.extend_from_slice(&fields);
            let diagnostic = obj(&[(
                "apiRejection",
                api_rejection
                    .as_ref()
                    .map_or(V::Null, |e| s(&format!("{e:?}"))),
            )]);
            let diagnostic = diagnostic.diagnostic_json().map_err(|e| e.to_string())?;
            accepted.extend_from_slice(&(diagnostic.len() as u64).to_be_bytes());
            accepted.extend_from_slice(diagnostic.as_bytes());
            if accepted.as_slice() != entry.actual_accepted() {
                return Err(format!("actual source Accepted mismatch at {at}"));
            }
            self.verified_actions += 1;
        }
        if !history.complete() {
            return Err("unresolved historical native await".into());
        }
        drop(history);
        if self
            .index
            .capture_verified_history_pin()
            .map_err(|e| e.to_string())?
            != *self.capability.original_current()
        {
            return Err("actual final disk changed during cold execution".into());
        }
        Ok(())
    }
}

fn cold_service_pending_context(frame: &NativeMainServiceContinuationV1) -> Option<V> {
    frame.pending_historical_context()
}
fn cold_service_native_pin(pin: &SourceIndexHistoryPinV1) -> V {
    let p = &pin.metadata.publication;
    obj(&[
        ("logicalSequence", s(&pin.metadata.sequence.to_string())),
        ("generation", s(&p.generation.to_string())),
        ("parent", p.parent.map_or(V::Null, |v| s(&v.to_string()))),
        (
            "physicalCreation",
            s(&p.physical_creation_sequence.to_string()),
        ),
        ("logicalCreation", s(&p.creation_sequence.to_string())),
        ("manifest", s(&hex(&p.manifest_sha256d))),
    ])
}
impl NativeMainColdServiceStateV1 {
    fn profile(&self) -> &'static str {
        NATIVE_ACTIVE_SERVICE_SESSION_PROFILE
    }
    fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    fn observation(
        &self,
        pin: &SourceIndexHistoryPinV1,
        actual_rows: RowStreamReceipt,
    ) -> Result<Vec<u8>, String> {
        let mut fields = NativeFieldCollector::new(
            "Native13/ActiveWalObservation/v1",
            true,
            self.observation_limit,
        );
        if self.started {
            fields
                .emit("ownerKind", &s("ActiveService"))
                .map_err(|e| e.to_string())?;
            self.visit_canonical_fields(&mut |p, v| fields.emit(p, v))
                .map_err(|e| e.to_string())?;
            if let Some(receipt) = self.history.last() {
                receipt
                    .result
                    .visit_fields(&mut |p, v| fields.emit(&format!("result/{p}"), v))
                    .map_err(|e| e.to_string())?;
            }
        } else {
            fields
                .emit("ownerKind", &s("NativeSession"))
                .map_err(|e| e.to_string())?;
            fields
                .emit("head/commands", &s(&self.owner.head.commands.to_string()))
                .map_err(|e| e.to_string())?;
            fields
                .emit("head/commitment", &s(&hex(&self.owner.head.commitment)))
                .map_err(|e| e.to_string())?;
        }
        let financial = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|e| e.to_string())?;
        fields
            .emit("financial", &s(&hex(&financial)))
            .map_err(|e| e.to_string())?;
        native_step_value_fields("rows", &logical_read(&actual_rows), &mut fields)
            .map_err(|e| e.to_string())?;
        native_step_value_fields(
            "physical",
            &physical_value(&cold_prefix_physical(pin)),
            &mut fields,
        )
        .map_err(|e| e.to_string())?;
        Ok(fields.frames.expect("retained actual source fields"))
    }
    fn verify_candidates(
        &self,
        raw: &[u8],
        before: &SourceIndexHistoryPinV1,
        post: &SourceIndexHistoryPinV1,
    ) -> Result<(), String> {
        let text = std::str::from_utf8(raw).map_err(|_| "native candidate UTF8")?;
        let facts =
            crate::property_catalog::parse_diagnostic_value(text).map_err(|e| e.to_string())?;
        let V::Object(fields) = &facts else {
            return Err("native candidate object".into());
        };
        let keys = [
            "profile",
            "identity",
            "catalog",
            "before",
            "intermediate",
            "post",
            "targetLogBytes",
            "retainedAncestors",
            "readContext",
        ];
        if fields.len() != keys.len()
            || fields
                .iter()
                .zip(keys)
                .any(|((key, _), expected)| key != expected)
            || facts
                .diagnostic_json()
                .map_err(|e| e.to_string())?
                .as_bytes()
                != raw
        {
            return Err("native candidate canonical fields".into());
        }
        let context = self
            .frame
            .as_deref()
            .and_then(cold_service_pending_context)
            .ok_or("no actual historical native await")?;
        let after = obj(&[
            ("identity", s(&post.storage.persistent_identity)),
            (
                "catalog",
                s(&post.storage.canonical_catalog.to_string_lossy()),
            ),
            ("append", cold_service_native_pin(post)),
        ]);
        if facts.get("profile") != s("TL-MAIN-NATIVE-PREPARED-INTENT-13-1")
            || facts.get("identity") != s(&before.storage.persistent_identity)
            || facts.get("catalog") != s(&before.storage.canonical_catalog.to_string_lossy())
            || facts.get("before") != cold_service_native_pin(before)
            || facts.get("intermediate") != V::Null
            || facts.get("post") != after
            || facts.get("readContext") != context
        {
            return Err("native candidate differs from actual source/history".into());
        }
        for key in ["targetLogBytes", "retainedAncestors"] {
            let V::String(text) = facts.get(key) else {
                return Err("native configuration scalar".into());
            };
            let value = text
                .parse::<u64>()
                .map_err(|_| "native configuration integer")?;
            if value.to_string() != text {
                return Err("native configuration canonical integer".into());
            }
        }
        Ok(())
    }
    fn execute(
        &mut self,
        history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        capability: &NativeColdIndexExecutionV1,
        action: NativeMainActiveServiceActionV1,
        mutations: Option<&[u8]>,
    ) -> Result<(), NativeMainActiveServiceErrorV1> {
        let before = cold_prefix_physical(history.source_history_pin());
        if let NativeMainActiveServiceActionV1::Start {
            start,
            generation,
            documents,
        } = &action
        {
            if self.started {
                return Err(NativeMainActiveServiceErrorV1::InvalidInput);
            }
            start
                .validate()
                .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
            self.owner
                .financial
                .runtime()
                .native_main_service_preflight(documents)
                .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
            self.history
                .try_reserve(1)
                .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
            let mut seed = NATIVE_ACTIVE_SERVICE_SESSION_PROFILE.as_bytes().to_vec();
            seed.extend_from_slice(&self.origin.session.commands.to_be_bytes());
            seed.extend_from_slice(&self.origin.session.commitment);
            self.head = NativeMainActiveServiceHeadV1 {
                actions: 0,
                commitment: sha256(&seed),
            };
            let begun = self
                .owner
                .financial
                .native_main_runtime_mut()
                .native_main_service_begin_historical(
                    history,
                    capability,
                    start.clone(),
                    *generation,
                    documents.clone(),
                );
            let result = match begun {
                Ok(frame) => {
                    self.frame = Some(Box::new(frame));
                    NativeMainActiveServiceActionResultV1::Started(Ok(()))
                }
                Err(error) => {
                    self.source_start_error = Some(error.clone());
                    NativeMainActiveServiceActionResultV1::Started(Err(error))
                }
            };
            self.started = true;
            self.pending_observation = Some(NativeMainActiveServicePendingV1 {
                action,
                result,
                before,
            });
        } else {
            if !self.started {
                return Err(NativeMainActiveServiceErrorV1::NoFrame);
            }
            if self.history.len() >= MAX_SERVICE_ACTIONS {
                return Err(NativeMainActiveServiceErrorV1::Capacity);
            }
            self.history
                .try_reserve(1)
                .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
            let frame = self
                .frame
                .as_ref()
                .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
            if let Some(code) = frame.terminal_code() {
                return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code));
            }
            match &action {
                NativeMainActiveServiceActionV1::Advance { limit }
                    if *limit == 0 || *limit > 4096 =>
                {
                    return Err(NativeMainActiveServiceErrorV1::InvalidInput)
                }
                NativeMainActiveServiceActionV1::Shutdown(args)
                    if args.len() > 8 || args.iter().any(|v| v.validate().is_err()) =>
                {
                    return Err(NativeMainActiveServiceErrorV1::InvalidInput)
                }
                NativeMainActiveServiceActionV1::AppendPending => {
                    let context = cold_service_pending_context(frame)
                        .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                    if self.attempted_write.as_ref() == Some(&context) {
                        return Err(NativeMainActiveServiceErrorV1::WriteAlreadyAttempted);
                    }
                }
                NativeMainActiveServiceActionV1::ResolvePost => {
                    let context = cold_service_pending_context(frame)
                        .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                    if self.attempted_write.as_ref() != Some(&context) {
                        return Err(NativeMainActiveServiceErrorV1::NoAttemptedWrite);
                    }
                }
                _ => (),
            }
            let frame = self.frame.as_mut().expect("same actual cold source frame");
            let runtime = self.owner.financial.native_main_runtime_mut();
            use crate::source_main_step_kernel::MainStepCommandV1 as Command;
            let result = match &action {
                NativeMainActiveServiceActionV1::Advance { limit } => {
                    NativeMainActiveServiceActionResultV1::Stop(
                        runtime.native_main_service_advance_historical(
                            history, capability, frame, *limit,
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::Pause => {
                    NativeMainActiveServiceActionResultV1::Pause(
                        runtime.native_main_service_pause(frame),
                    )
                }
                NativeMainActiveServiceActionV1::AppendPending => {
                    self.attempted_write = cold_service_pending_context(frame);
                    NativeMainActiveServiceActionResultV1::Stop(
                        runtime.native_main_service_append_historical(
                            history,
                            frame,
                            mutations.ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?,
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::ResolvePost => {
                    NativeMainActiveServiceActionResultV1::Stop(
                        runtime.native_main_service_resolve_historical(history, capability, frame),
                    )
                }
                NativeMainActiveServiceActionV1::FireTimer { id, generation } => {
                    NativeMainActiveServiceActionResultV1::Control(
                        runtime.native_main_service_control_historical(
                            history,
                            capability,
                            frame,
                            Command::FireTimer {
                                id: *id,
                                generation: *generation,
                            },
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::ClearTimer { id, generation } => {
                    NativeMainActiveServiceActionResultV1::Control(
                        runtime.native_main_service_control_historical(
                            history,
                            capability,
                            frame,
                            Command::ClearTimer {
                                id: *id,
                                generation: *generation,
                            },
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::Shutdown(args) => {
                    NativeMainActiveServiceActionResultV1::Control(
                        runtime.native_main_service_control_historical(
                            history,
                            capability,
                            frame,
                            Command::Shutdown(args.clone()),
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::Start { .. } => {
                    unreachable!("handled constructor raw action")
                }
            };
            if cold_service_pending_context(frame).as_ref() != self.attempted_write.as_ref() {
                self.attempted_write = None;
            }
            self.pending_observation = Some(NativeMainActiveServicePendingV1 {
                action,
                result,
                before,
            });
        }
        let post = history.source_history_pin();
        self.publish(
            history
                .source_actual_rows()
                .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?,
            cold_prefix_physical(post),
        )
        .map(|_| ())
    }
}
