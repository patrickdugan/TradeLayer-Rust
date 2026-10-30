pub const NATIVE_MAIN_COLD_MODE4_SOURCE_PROFILE: &str =
    "TL-MAIN-COLD-MODE4-SOURCE-REEXECUTION-13-1";
/// Reexecutes raw ordinary single-inline actions on ONE actual financial
/// prefix. The final native index and WAL locks stay retained and readonly.
/// Recovery histories need independent raw IO facts and are not admitted here.
pub struct NativeMainColdMode4ServiceV1 {
    state: NativeMainColdServiceStateV1,
    index: SourceCompactIndex,
    capability: NativeColdIndexExecutionV1,
    transcript: crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
    verified_actions: usize,
    verified_inline_operations: usize,
}
pub struct RejectedNativeMainColdMode4ServiceV1 {
    pub cause: String,
    owner: NativeMainColdMode4ServiceV1,
}
impl core::fmt::Debug for NativeMainColdMode4ServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeMainColdMode4ServiceV1")
            .field("verified_actions", &self.verified_actions)
            .field(
                "verified_inline_operations",
                &self.verified_inline_operations,
            )
            .finish_non_exhaustive()
    }
}
impl core::fmt::Debug for RejectedNativeMainColdMode4ServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainColdMode4ServiceV1")
            .field("cause", &self.cause)
            .field("verified_actions", &self.owner.verified_actions)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainColdMode4ServiceV1 {
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
impl NativeMainColdMode4ServiceV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_COLD_MODE4_SOURCE_PROFILE
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
        transcript: crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainColdMode4ServiceV1>> {
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
        match owner.run(&completed_tip) {
            Ok(()) => Ok(owner),
            Err(cause) => Err(Box::new(RejectedNativeMainColdMode4ServiceV1 {
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
            if self
                .transcript
                .entry_has_native_recovery(entry)
                .map_err(|e| e.to_string())?
            {
                return Err("old Mode4 recovery has no independent raw native-IO-stop fact".into());
            }
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
            } else if matches!(action, NativeMainActiveServiceActionV1::AppendPending) {
                return Err(
                    "rejected/no-write append remains a separate raw-action coverage boundary"
                        .into(),
                );
            }
            previous = post.clone();
            points.push((before, post));
            actions.push(action);
        }
        if &previous != self.capability.original_current() {
            return Err("actual final Post differs from full acknowledged history".into());
        }
        let provider = NativeMainMode4HistoryProviderV1 {
            transcript: &self.transcript,
            capability: &self.capability,
        };
        let mut history = provider
            .playback(&mut self.index, &boundaries)
            .map_err(|e| e.to_string())?;
        if self.state.observation(
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
            if history.source_history_pin() != &before
                || self
                    .state
                    .observation(
                        &before,
                        history.source_actual_rows().map_err(|e| e.to_string())?,
                    )?
                    .as_slice()
                    != entry.actual_before()
            {
                return Err(format!("actual source Before mismatch at {at}"));
            }
            let inline = self
                .state
                .frame
                .as_deref()
                .and_then(cold_service_pending_context)
                .is_some_and(|v| {
                    v.get("profile") == s(crate::source_compact_index_v2::INLINE_DATABASE_PROFILE)
                });
            if !inline {
                if let Some(raw) = entry.native_candidates() {
                    self.state.verify_candidates(raw, &before, &post)?;
                }
            }
            let rejection = self
                .state
                .execute_with_mode4(
                    &mut history,
                    &self.capability,
                    action,
                    entry.native_mutations(),
                    Some((&self.transcript, entry.sequence)),
                )
                .err();
            if history.source_history_pin() != &post {
                return Err(format!("actual metadata Post mismatch at {at}"));
            }
            let fields = self.state.observation(
                &post,
                history.source_actual_rows().map_err(|e| e.to_string())?,
            )?;
            let mut accepted = b"TLMAP\x01".to_vec();
            accepted.extend_from_slice(&(fields.len() as u64).to_be_bytes());
            accepted.extend_from_slice(&fields);
            let diagnostic = obj(&[(
                "apiRejection",
                rejection.as_ref().map_or(V::Null, |e| s(&format!("{e:?}"))),
            )]);
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
        }
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
