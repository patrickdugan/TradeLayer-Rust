include!("source_main_mixed_native_cold_difference_impl.rs");
pub const NATIVE_MAIN_COLD_MIXED_FACT_SOURCE_PROFILE: &str =
    "TL-MAIN-COLD-SEGMENTED-MIXED-FACT-SOURCE-13-1";
pub struct NativeMainColdMixedFactServiceV1 {
    state: NativeMainColdServiceStateV1,
    index: SourceCompactIndex,
    capability: NativeColdIndexExecutionV1,
    transcript: crate::source_main_active_service_wal::VerifiedMixedNativeHistoryV1,
    verified_actions: u64,
    segments: u64,
}
pub struct RejectedNativeMainColdMixedFactServiceV1 {
    pub cause: String,
    owner: NativeMainColdMixedFactServiceV1,
}
impl core::fmt::Debug for RejectedNativeMainColdMixedFactServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainColdMixedFactServiceV1")
            .field("cause", &self.cause)
            .field("verified_actions", &self.owner.verified_actions)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainColdMixedFactServiceV1 {
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial_runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.owner.index()
    }
    pub fn verified_actions(&self) -> u64 {
        self.owner.verified_actions
    }
}
fn mixed_segmented_cold_observation(
    state: &NativeMainColdServiceStateV1,
    pin: &SourceIndexHistoryPinV1,
    rows: RowStreamReceipt,
    archived: Option<&(
        u64,
        crate::source_main_active_service_wal::ActiveServiceWalBlobV1,
    )>,
    window: usize,
) -> Result<Vec<u8>, String> {
    let mut raw = state.mixed_fact_observation(pin, rows)?;
    let mut fields = NativeFieldCollector::new("Native13/SegmentedActiveHistory/v1", true, 4096);
    fields
        .emit("profile", &s(SEGMENTED_ACTIVE_HISTORY_PROFILE))
        .map_err(|e| e.to_string())?;
    fields
        .emit(
            "archivedActions",
            &s(&archived.map_or(0, |s| s.0).to_string()),
        )
        .map_err(|e| e.to_string())?;
    fields
        .emit("acknowledgedWindow", &s(&window.to_string()))
        .map_err(|e| e.to_string())?;
    let source = if state.started {
        obj(&[
            ("actions", s(&state.head.actions.to_string())),
            ("commitment", s(&hex(&state.head.commitment))),
        ])
    } else {
        V::Null
    };
    native_step_value_fields("sourceHead", &source, &mut fields).map_err(|e| e.to_string())?;
    native_step_value_fields(
        "previousSegment",
        &archived.map_or(V::Null, |s| s.1.value()),
        &mut fields,
    )
    .map_err(|e| e.to_string())?;
    raw.extend_from_slice(fields.frames.as_deref().expect("retained actual fields"));
    if raw.len() > state.observation_limit {
        return Err("segmented observation budget".into());
    }
    Ok(raw)
}
impl NativeMainColdMixedFactServiceV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_COLD_MIXED_FACT_SOURCE_PROFILE
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.state.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.index
    }
    pub fn verified_actions(&self) -> u64 {
        self.verified_actions
    }
    pub fn verified_segments(&self) -> u64 {
        self.segments
    }
    pub fn actual_post(&self) -> &SourceIndexHistoryPinV1 {
        self.capability.original_current()
    }
    pub fn acknowledged_wal_head(&self) -> [u8; 32] {
        self.transcript.head()
    }
    pub fn reexecute(
        prefix: NativeMainColdPrefixV1,
        transcript: crate::source_main_active_service_wal::VerifiedMixedNativeHistoryV1,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainColdMixedFactServiceV1>> {
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
            inline_profile: true,
        };

        let mut owner = Self {
            state,
            index,
            capability,
            transcript,
            verified_actions: 0,
            segments: 0,
        };
        match owner.run(&completed_tip) {
            Ok(()) => Ok(owner),
            Err(cause) => Err(Box::new(RejectedNativeMainColdMixedFactServiceV1 {
                cause,
                owner,
            })),
        }
    }
    fn run(&mut self, completed_tip: &SourceIndexHistoryPinV1) -> Result<(), String> {
        if self.state.observation_limit == 0
            || self.state.observation_limit > ACTIVE_WAL_OBSERVATION_LIMIT
        {
            return Err("invalid observation limit".into());
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
            return Err("segmented WAL origin differs from actual completed prefix".into());
        }
        let mut boundaries = vec![origin.clone()];
        for window in 0..self.transcript.window_count() {
            let entries = self
                .transcript
                .read_window(window)
                .map_err(|e| e.to_string())?;
            for entry in entries.entries() {
                let action = NativeMainActiveServiceActionV1::from_canonical_action_bytes(
                    entry.raw_action(),
                )
                .map_err(|e| format!("{e:?}"))?;
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
                if &before != boundaries.last().expect("actual origin retained") {
                    return Err("native boundary gap".into());
                }
                if before != post {
                    if !matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                        || before.metadata.sequence.checked_add(1) != Some(post.metadata.sequence)
                        || entry.native_mutations().is_none()
                        || entry.native_candidates().is_none()
                    {
                        return Err(
                            "unsupported segmented native transition or unavailable publication"
                                .into(),
                        );
                    }
                    boundaries.push(post);
                }
            }
        }
        if boundaries.last() != Some(self.capability.original_current()) {
            return Err("actual final native Post differs from acknowledged endpoint".into());
        }
        let first_window = self.transcript.read_window(0).map_err(|e| e.to_string())?;
        let access = self
            .transcript
            .window_access(&first_window)
            .map_err(|e| e.to_string())?;
        let mut views = Vec::new();
        for pin in &boundaries {
            views.push(
                crate::source_compact_index_v2::MainIndexReadViewV1::from_verified(
                    access
                        .checked_original_view(&mut self.index, pin)
                        .map_err(|e| e.to_string())?,
                ),
            );
        }
        let mut history = NativeHistoricalValidityPlaybackV1::open_verified_views(
            &mut self.index,
            &self.capability,
            &boundaries,
            views,
        )
        .map_err(|e| e.to_string())?;
        let actual_origin = self.state.mixed_fact_observation(
            &origin,
            history.source_actual_rows().map_err(|e| e.to_string())?,
        )?;
        if actual_origin
            != self
                .transcript
                .read_reference(self.transcript.origin_observation())
                .map_err(|e| e.to_string())?
        {
            return Err("actual origin comparison mismatch".into());
        }
        let mut archived = None;
        let mut acknowledged_window = 0usize;
        for window in 0..self.transcript.window_count() {
            let entries = self
                .transcript
                .read_window(window)
                .map_err(|e| e.to_string())?;
            let access = self
                .transcript
                .window_access(&entries)
                .map_err(|e| e.to_string())?;
            for entry in entries.entries() {
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
                if history.source_history_pin() != &before {
                    return Err("actual historical Before differs".into());
                }
                let before_fields = mixed_segmented_cold_observation(
                    &self.state,
                    &before,
                    history.source_actual_rows().map_err(|e| e.to_string())?,
                    archived.as_ref(),
                    acknowledged_window,
                )?;
                if before_fields.as_slice() != entry.actual_before() {
                    return Err(format!(
                        "actual segmented Before mismatch at {}: {}",
                        entry.sequence,
                        mixed_cold_comparison_difference(entry.actual_before(), &before_fields)
                    ));
                }
                self.state
                    .verify_mixed_fact_selection(&access, entry.sequence, false)?;
                let action = NativeMainActiveServiceActionV1::from_canonical_action_bytes(
                    entry.raw_action(),
                )
                .map_err(|e| format!("{e:?}"))?;
                let mut verification = None;
                let api_error = self
                    .state
                    .execute_mixed_facts(
                        &mut history,
                        &self.capability,
                        action,
                        entry.native_mutations(),
                        (&access, entry.sequence, &mut verification),
                    )
                    .err();
                if let Some(error) = verification {
                    return Err(format!(
                        "actual mixed source verification failed at {}: {error}",
                        entry.sequence
                    ));
                }
                if api_error.is_some() {
                    self.state
                        .verify_mixed_fact_selection(&access, entry.sequence, true)?;
                }
                if history.source_history_pin() != &post {
                    return Err("actual historical Post differs".into());
                }
                let fields = mixed_segmented_cold_observation(
                    &self.state,
                    &post,
                    history.source_actual_rows().map_err(|e| e.to_string())?,
                    archived.as_ref(),
                    acknowledged_window,
                )?;
                let mut accepted = b"TLMAP\x01".to_vec();
                accepted.extend_from_slice(&(fields.len() as u64).to_be_bytes());
                accepted.extend_from_slice(&fields);
                let mut diagnostic = obj(&[(
                    "apiRejection",
                    api_error.as_ref().map_or(V::Null, |e| s(&format!("{e:?}"))),
                )]);
                let (recovery, escrow) = access
                    .source_administrative_refs(entry.sequence)
                    .map_err(|e| e.to_string())?;
                for (key, pin) in [("nativeRecovery", recovery), ("generationEscrow", escrow)] {
                    if let Some(pin) = pin {
                        diagnostic
                            .insert(key, pin.value())
                            .map_err(|e| e.to_string())?;
                    }
                }
                let diagnostic = active_wal_bytes(&diagnostic).map_err(|e| e.to_string())?;
                accepted.extend_from_slice(&(diagnostic.len() as u64).to_be_bytes());
                accepted.extend_from_slice(&diagnostic);
                if accepted.as_slice() != entry.actual_accepted() {
                    return Err(format!(
                        "actual segmented Accepted mismatch at {}: {}",
                        entry.sequence,
                        mixed_cold_comparison_difference(entry.actual_accepted(), &accepted)
                    ));
                }
                self.verified_actions = self
                    .verified_actions
                    .checked_add(1)
                    .ok_or("source counter overflow")?;
                acknowledged_window += 1;
            }
            if let Some((actions, pin, source_head)) = entries.checkpoint() {
                let actual = if self.state.started {
                    obj(&[
                        ("actions", s(&self.state.head.actions.to_string())),
                        ("commitment", s(&hex(&self.state.head.commitment))),
                    ])
                } else {
                    V::Null
                };
                if actions != self.verified_actions || source_head != &actual {
                    return Err("actual source checkpoint head differs".into());
                }
                // All actions in this window have executed once and compared. The same
                // financial/source frame remains; this clears acknowledged receipt RAM only.
                self.state.history.clear();
                archived = Some((actions, pin.clone()));
                acknowledged_window = 0;
                self.segments += 1;
            }
        }
        if self.verified_actions != self.transcript.actions() {
            return Err("segmented raw action count differs".into());
        }
        if !history.complete() && !history.all_recorded_transitions_verified() {
            return Err(
                "pending native cold endpoint is retained but not resumable in this profile".into(),
            );
        }
        drop(history);
        if self
            .index
            .capture_verified_history_pin()
            .map_err(|e| e.to_string())?
            != *self.capability.original_current()
        {
            return Err("actual final disk changed during cold replay".into());
        }
        Ok(())
    }
}
