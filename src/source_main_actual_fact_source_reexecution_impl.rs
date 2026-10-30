pub const NATIVE_MAIN_COLD_ACTUAL_FACT_SOURCE_PROFILE: &str =
    "TL-MAIN-COLD-ACTUAL-FACT-SOURCE-REEXECUTION-13-1";
/// Reexecutes exact13-2 raw actions on ONE actual financial prefix using
/// independent actual plan/typed IO facts and actual verified old page views.
/// The final native index/cold capability/WAL locks remain readonly and retained.
pub struct NativeMainColdActualFactServiceV1 {
    state: NativeMainColdServiceStateV1,
    index: SourceCompactIndex,
    capability: NativeColdIndexExecutionV1,
    transcript: crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
    verified_actions: usize,
    verified_inline_operations: usize,
}
pub struct RejectedNativeMainColdActualFactServiceV1 {
    pub cause: String,
    owner: NativeMainColdActualFactServiceV1,
}
impl core::fmt::Debug for NativeMainColdActualFactServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeMainColdActualFactServiceV1")
            .field("verified_actions", &self.verified_actions)
            .field(
                "verified_inline_operations",
                &self.verified_inline_operations,
            )
            .finish_non_exhaustive()
    }
}
impl core::fmt::Debug for RejectedNativeMainColdActualFactServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainColdActualFactServiceV1")
            .field("cause", &self.cause)
            .field("verified_actions", &self.owner.verified_actions)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainColdActualFactServiceV1 {
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
impl NativeMainColdActualFactServiceV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_COLD_ACTUAL_FACT_SOURCE_PROFILE
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.state.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.index
    }
    pub fn verified_actions(&self) -> usize {
        self.verified_actions
    }
    pub fn verified_inline_operations(&self) -> usize {
        self.verified_inline_operations
    }
    pub fn original_action_receipts(&self) -> &[NativeMainActiveServiceReceiptV1] {
        &self.state.history
    }
    pub fn actual_post(&self) -> &SourceIndexHistoryPinV1 {
        self.capability.original_current()
    }
    pub fn reexecute(
        prefix: NativeMainColdPrefixV1,
        transcript: crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainColdActualFactServiceV1>> {
        Self::reexecute_with_interleave(
            prefix,
            transcript,
            observation_limit,
            &mut NativeMainNoActualFactInterleaveV1,
        )
    }
    fn reexecute_with_interleave(
        prefix: NativeMainColdPrefixV1,
        transcript: crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        observation_limit: usize,
        interleave: &mut impl NativeMainActualFactInterleaveV1,
    ) -> Result<Self, Box<RejectedNativeMainColdActualFactServiceV1>> {
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
            verified_inline_operations: 0,
        };
        match owner.run_with_interleave(&completed_tip, interleave) {
            Ok(()) => Ok(owner),
            Err(cause) => Err(Box::new(RejectedNativeMainColdActualFactServiceV1 {
                cause,
                owner,
            })),
        }
    }
    fn run_with_interleave(
        &mut self,
        completed_tip: &SourceIndexHistoryPinV1,
        interleave: &mut impl NativeMainActualFactInterleaveV1,
    ) -> Result<(), String> {
        if self.state.observation_limit == 0
            || self.state.observation_limit > ACTIVE_WAL_OBSERVATION_LIMIT
        {
            return Err("invalid observation limit".into());
        }
        let expected = NativeMainArchivePin {
            head: self.state.owner.head.clone(),
            foundation: self.state.owner.foundation_head,
            origin: self.state.owner.origin_physical.clone(),
            current: cold_prefix_physical(completed_tip),
            archive_sha256: sha256(self.transcript.origin_archive()),
        };
        let archive = inspect_archive(self.transcript.origin_archive(), &expected)
            .map_err(|e| format!("origin archive: {e:?}"))?;
        if archive.foundation != self.state.owner.foundation
            || archive.binding != self.state.owner.binding
            || archive.origin_rows != logical_read(&self.state.owner.origin_rows)
            || archive.entries.len() != self.state.owner.history.len()
        {
            return Err("original completed prefix binding".into());
        }
        for (at, ((command, encoded), actual)) in archive
            .entries
            .iter()
            .zip(&self.state.owner.history)
            .enumerate()
        {
            if command != &actual.command
                || *encoded != logical_receipt_bytes(actual).map_err(|e| format!("{e:?}"))?
                || archive.physical_entries.get(at)
                    != Some(&(
                        physical_value(&actual.before),
                        actual.after.as_ref().map_or(V::Null, physical_value),
                    ))
            {
                return Err("original completed receipt mismatch".into());
            }
        }
        let ns = self.index.native_namespace().clone();
        let decode = |raw: &[u8]| {
            SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&ns, raw)
                .map_err(|e| e.to_string())
        };
        let origin = decode(self.transcript.origin_history())?;
        if origin != *completed_tip {
            return Err("WAL origin differs from actual rebuilt prefix".into());
        }
        let mut boundaries = vec![origin.clone()];
        let mut actions = vec![];
        let mut points = vec![];
        let mut previous = origin.clone();
        for entry in self.transcript.entries() {
            let action = self
                .transcript
                .decoded_action(entry)
                .map_err(|e| e.to_string())?;
            let before = decode(entry.verified_history_before())?;
            let post = decode(entry.verified_history_post())?;
            if before != previous {
                return Err("metadata action continuity".into());
            }
            if post != before {
                if !matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                    || before.metadata.sequence.checked_add(1) != Some(post.metadata.sequence)
                    || entry.native_candidates().is_none()
                    || entry.native_mutations().is_none()
                {
                    return Err("unsupported metadata transition".into());
                }
                boundaries.push(post.clone());
            }
            previous = post.clone();
            points.push((before, post));
            actions.push(action);
        }
        if &previous != self.capability.original_current() {
            return Err("actual final Post differs from full acknowledged history".into());
        }
        let provider = NativeMainActualFactHistoryProviderV1 {
            transcript: &self.transcript,
            capability: &self.capability,
        };
        let mut history = provider
            .playback(&mut self.index, &boundaries)
            .map_err(|e| e.to_string())?;
        if self.state.actual_fact_observation(
            &origin,
            history.source_actual_rows().map_err(|e| e.to_string())?,
        )? != self
            .transcript
            .original_observation_bytes()
            .map_err(|e| e.to_string())?
        {
            return Err("actual origin observation mismatch".into());
        }
        for (at, ((entry, action), (before, post))) in self
            .transcript
            .entries()
            .iter()
            .zip(actions)
            .zip(points)
            .enumerate()
        {
            interleave.before_source(
                &mut self.state,
                &mut history,
                &self.capability,
                &self.transcript,
                entry.sequence,
            )?;
            if history.source_history_pin() != &before
                || self
                    .state
                    .actual_fact_observation(
                        &before,
                        history.source_actual_rows().map_err(|e| e.to_string())?,
                    )?
                    .as_slice()
                    != entry.actual_before()
            {
                return Err(format!("actual source Before mismatch at {at}"));
            }
            self.state
                .verify_actual_fact_selection(&self.transcript, entry.sequence, false)?;
            let inline = self
                .state
                .frame
                .as_deref()
                .and_then(cold_service_pending_context)
                .is_some_and(|v| {
                    v.get("profile") == s(crate::source_compact_index_v2::INLINE_DATABASE_PROFILE)
                });
            let repeated = matches!(action, NativeMainActiveServiceActionV1::AppendPending)
                && self
                    .state
                    .frame
                    .as_deref()
                    .and_then(cold_service_pending_context)
                    .as_ref()
                    == self.state.attempted_write.as_ref()
                && self.state.attempted_write.is_some();
            if !inline && !repeated {
                if let Some(raw) = entry.native_candidates() {
                    self.state.verify_candidates(raw, &before, &post)?;
                }
            }
            let mut verification = None;
            let rejection = self
                .state
                .execute_actual_facts(
                    &mut history,
                    &self.capability,
                    action,
                    entry.native_mutations(),
                    (&self.transcript, entry.sequence, &mut verification),
                )
                .err();
            if let Some(reason) = verification {
                return Err(format!(
                    "independent fact/source verification failed at {at}: {reason}"
                ));
            }
            if rejection.is_some() {
                self.state
                    .verify_actual_fact_selection(&self.transcript, entry.sequence, true)?;
            }
            if history.source_history_pin() != &post {
                return Err(format!("actual metadata Post mismatch at {at}"));
            }
            let fields = self.state.actual_fact_observation(
                &post,
                history.source_actual_rows().map_err(|e| e.to_string())?,
            )?;
            let mut accepted = b"TLMAP\x01".to_vec();
            accepted.extend_from_slice(&(fields.len() as u64).to_be_bytes());
            accepted.extend_from_slice(&fields);
            let mut diagnostic = obj(&[(
                "apiRejection",
                rejection.as_ref().map_or(V::Null, |e| s(&format!("{e:?}"))),
            )]);
            let (recovery, escrow) = self
                .transcript
                .source_administrative_refs(entry.sequence)
                .map_err(|e| e.to_string())?;
            if let Some(reference) = recovery {
                diagnostic
                    .insert("nativeRecovery", reference.value())
                    .map_err(|e| e.to_string())?;
            }
            if let Some(reference) = escrow {
                diagnostic
                    .insert("generationEscrow", reference.value())
                    .map_err(|e| e.to_string())?;
            }
            let diagnostic = diagnostic.diagnostic_json().map_err(|e| e.to_string())?;
            accepted.extend_from_slice(&(diagnostic.len() as u64).to_be_bytes());
            accepted.extend_from_slice(diagnostic.as_bytes());
            if accepted.as_slice() != entry.actual_accepted() {
                return Err(format!("actual source Accepted mismatch at {at}"));
            }
            self.verified_actions += 1;
            if inline && post != before {
                self.verified_inline_operations += 1;
            }
            interleave.after_source(
                &mut self.state,
                &mut history,
                &self.capability,
                &self.transcript,
                entry.sequence,
            )?;
        }
        interleave.finish(
            &mut self.state,
            &mut history,
            &self.capability,
            &self.transcript,
            self.verified_actions as u64,
        )?;
        if !history.complete() {
            return Err("historical native transitions remain".into());
        }
        drop(history);
        if self
            .index
            .capture_verified_history_pin()
            .map_err(|e| e.to_string())?
            != *self.capability.original_current()
        {
            return Err("final physical owner changed during readonly source replay".into());
        }
        Ok(())
    }
}
