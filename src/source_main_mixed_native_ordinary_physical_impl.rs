/// Sealed conditional physical/typed-IO closure. Source query/arguments/TLNM
/// come from the actual still-held historical source await, never Accepted.
pub struct VerifiedMixedOrdinaryClosureV1 {
    sequence: u64,
    post: SourceIndexHistoryPinV1,
    write_before: SourceIndexHistoryPinV1,
    write_context: V,
    first_error: Option<std::io::Error>,
    last_error: Option<std::io::Error>,
    attempts: usize,
    recovery_links: usize,
    replacements: usize,
}
impl VerifiedMixedOrdinaryClosureV1 {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn post(&self) -> &SourceIndexHistoryPinV1 {
        &self.post
    }
    pub fn write_before(&self) -> &SourceIndexHistoryPinV1 {
        &self.write_before
    }
    pub fn write_context(&self) -> &V {
        &self.write_context
    }
    pub fn first_error(&self) -> Option<&std::io::Error> {
        self.first_error.as_ref()
    }
    pub fn last_error(&self) -> Option<&std::io::Error> {
        self.last_error.as_ref()
    }
    pub fn attempts(&self) -> usize {
        self.attempts
    }
    pub fn recovery_links(&self) -> usize {
        self.recovery_links
    }
    pub fn replacements(&self) -> usize {
        self.replacements
    }
    pub(crate) fn take_first_error(&mut self) -> Option<std::io::Error> {
        self.first_error.take()
    }
}
impl VerifiedMixedNativeWindowAccessV1<'_> {
    pub fn verify_ordinary_operation(
        &self,
        index: &mut SourceCompactIndex,
        sequence: u64,
        source: &crate::source_compact_index_v2::NativeHistoricalValidityAwaitV1,
    ) -> Result<VerifiedMixedOrdinaryClosureV1> {
        let entry = self.entry(sequence)?;
        let value = self.entry_record(sequence)?;
        if MixedNativeHistoryOperationV1::from_record(&value)?
            != MixedNativeHistoryOperationV1::Validity
            || source.append_verified()
            || source.resolved()
            || history_accepted_parts_profile(entry.actual_accepted(), true)?
                .1
                .get("apiRejection")
                != V::Null
            || !matches!(
                self.decoded_action(entry)?,
                crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending
            )
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let before = self.mode4_pin(entry.verified_history_before())?;
        let post = self.mode4_pin(entry.verified_history_post())?;
        if source.query().snapshot != before.metadata
            || source.original_post() != &post
            || index
                .capture_verified_history_pin()
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?
                != self.checked_metadata_post()?
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let raw = entry
            .native_mutations()
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        if source.exact_prepared_mutations() != raw {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let all_escrows = self.mode4_all_escrows()?;
        let actual_before = self.mode4_view(index, &before, &all_escrows)?;
        if source
            .recomputed_write_context_at(&actual_before)
            .map_err(|_| ActiveServiceWalErrorV1::WrongPlan)?
            != source.durable_read_context()
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let actual_post = self.mode4_view(index, &post, &all_escrows)?;
        source
            .verify_independent_mixed_post(&actual_post, raw)
            .map_err(|_| ActiveServiceWalErrorV1::WrongPlan)?;
        let target_escrows = self.mode4_escrows(entry)?;
        let recoveries = self
            .mode4_chain(value.get("nativeRecovery"), false)?
            .into_iter()
            .map(|(reference, value)| Mode4Recovery { reference, value })
            .collect::<Vec<_>>();
        self.mode4_require_all_recovery_barriers(entry, &recoveries, &target_escrows)?;
        let inputs = self.ordinary_io_inputs(sequence)?;
        if inputs.is_empty() || inputs.len() > 64 {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        if inputs.len() == 1 && inputs[0].io().native_io().is_none() {
            self.verify_ordinary_success_plan(sequence, source)?;
        }
        let original_selection = inputs[0].selection().clone();
        let bind_selection = |context: V| -> Result<V> {
            let mut selected = original_selection.clone();
            let mut actual = selected.get("actualSource");
            let mut pending = actual.get("pending");
            pending
                .insert("context", context)
                .map_err(|_| ActiveServiceWalErrorV1::WrongContext)?;
            actual
                .insert("pending", pending)
                .map_err(|_| ActiveServiceWalErrorV1::WrongContext)?;
            selected
                .insert("actualSource", actual)
                .map_err(|_| ActiveServiceWalErrorV1::WrongContext)?;
            Ok(selected)
        };
        let mut candidates = Mode4Candidates::parse(
            parse(
                entry
                    .native_candidates()
                    .ok_or(ActiveServiceWalErrorV1::WrongPin)?,
            )?,
            &before,
        )?;
        if candidates.before != Mode4AppendPoint::of(&before)
            || candidates.read_context != source.durable_read_context()
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let mut lower = self
            .window
            .records
            .iter()
            .find(|(_, r)| {
                r.get("sequence") == s(sequence.to_string()) && r.get("stage") == s("Intent")
            })
            .map(|(n, _)| *n)
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        let mut recovery_at = 0;
        let mut replacements = 0;
        let mut escrow_at = 0;
        let mut terminal = false;
        let mut first_error = None;
        let mut last_error = None;
        let mut current = before.clone();
        let mut last_context = source.durable_read_context();
        for (ordinal, input) in inputs.iter().enumerate() {
            if terminal
                || input.metadata().original() != &current
                || input.metadata().native_candidates() != candidates.raw
                || input.metadata().exact_prepared_mutations() != raw
                || input.physical() <= lower
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let actual = self.mode4_view(index, &current, &all_escrows)?;
            let context = source
                .recomputed_write_context_at(&actual)
                .map_err(|_| ActiveServiceWalErrorV1::WrongPlan)?;
            if context != input.metadata().native_read_context()
                || candidates.read_context != context
            {
                return Err(ActiveServiceWalErrorV1::WrongPlan);
            }
            if bind_selection(context.clone())? != *input.selection() {
                return Err(ActiveServiceWalErrorV1::WrongPlan);
            }
            self.mode4_attempt_escrow(
                &candidates,
                &current,
                &target_escrows,
                &mut escrow_at,
                sequence,
                (lower, input.physical()),
            )?;
            last_context = context;
            let error = input.io().native_io().map(|io| io.reconstruct());
            if ordinal == 0 {
                first_error = input.io().native_io().map(|io| io.reconstruct());
            }
            last_error = error;
            if last_error.is_none() {
                if input.io().verified_post() != Some(&post)
                    || candidates.post != Mode4AppendPoint::of(&post)
                    || ordinal + 1 != inputs.len()
                    || recovery_at != recoveries.len()
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                terminal = true;
                continue;
            }
            let recovery = recoveries
                .get(recovery_at)
                .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
            recovery_at += 1;
            let proof = &recovery.value;
            let candidate_value = proof.get("candidate");
            let candidate = history_text(&candidate_value)?;
            let replacement = matches!(candidate, "Before" | "Intermediate");
            history_keys(
                proof,
                &[
                    "profile",
                    "candidate",
                    "snapshot",
                    "discardedBytes",
                    "previousRecovery",
                    "verifiedReopenedHistory",
                ],
                if replacement {
                    &[
                        "binding",
                        "replacementCandidates",
                        "replacementMutationsSha256",
                    ]
                } else {
                    &[]
                },
            )?;
            if proof.get("profile") != s("TL-MAIN-ACTIVE-NATIVE-RECOVERY-13-1") {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let reopened =
                self.mode4_pin(&mode4_decode_hex(&proof.get("verifiedReopenedHistory"))?)?;
            let expected = match candidate {
                "Before" => &candidates.before,
                "Intermediate" => candidates
                    .intermediate
                    .as_ref()
                    .ok_or(ActiveServiceWalErrorV1::WrongPin)?,
                "Post" => &candidates.post,
                _ => return Err(ActiveServiceWalErrorV1::WrongContext),
            };
            if Mode4AppendPoint::of(&reopened) != *expected
                || proof.get("snapshot") != mode4_physical(&reopened)
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            history_count(&proof.get("discardedBytes"))?;
            if let Some(produced_post) = input.io().verified_post() {
                if Mode4AppendPoint::of(produced_post) != candidates.post
                    || (candidate == "Post" && produced_post != &reopened)
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
            let actual = self.mode4_view(index, &reopened, &all_escrows)?;
            if replacement {
                replacements += 1;
                let at = self.mode4_barrier(
                    sequence,
                    "NativeSupersessionIntent",
                    "nativeRecovery",
                    &recovery.reference,
                )?;
                if at <= input.physical() {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                let expected_context = source
                    .recomputed_write_context_at(&actual)
                    .map_err(|_| ActiveServiceWalErrorV1::WrongPlan)?;
                let expected_binding = obj(&[
                    (
                        "profile",
                        s(crate::source_compact_index_v2::PREPARED_VALIDITY_REOPEN_PROFILE),
                    ),
                    ("matched", s(candidate)),
                    ("identity", s(&reopened.storage.persistent_identity)),
                    (
                        "catalog",
                        s(reopened.storage.canonical_catalog.to_string_lossy()),
                    ),
                    (
                        "namespace",
                        s(reopened
                            .storage
                            .namespace
                            .path(Path::new(""))
                            .to_string_lossy()),
                    ),
                    ("snapshot", mode4_snapshot(&reopened)),
                    (
                        "completeState",
                        mode4_state(source.recomputed_before_state()),
                    ),
                    ("discardedBytes", proof.get("discardedBytes")),
                ]);
                if proof.get("binding") != expected_binding
                    || history_hash(&proof.get("replacementMutationsSha256"))? != sha256(raw)
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                let next = Mode4Candidates::parse(proof.get("replacementCandidates"), &before)?;
                if next.before != Mode4AppendPoint::of(&reopened)
                    || next.read_context != expected_context
                    || next.raw.get("targetLogBytes") != candidates.raw.get("targetLogBytes")
                    || next.raw.get("retainedAncestors") != candidates.raw.get("retainedAncestors")
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                let record = self
                    .window
                    .records
                    .iter()
                    .find(|(n, _)| *n == at)
                    .map(|(_, r)| r)
                    .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
                if record.get("ordinaryPlannedReplacementSelection")
                    != bind_selection(expected_context)?
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                current = reopened;
                candidates = next;
                lower = at;
            } else {
                let at = self.mode4_barrier(
                    sequence,
                    "NativeRecoveryObserved",
                    "nativeRecovery",
                    &recovery.reference,
                )?;
                if at <= input.physical()
                    || reopened != post
                    || ordinal + 1 != inputs.len()
                    || recovery_at != recoveries.len()
                    || proof.get("binding") != V::Undefined
                    || proof.get("replacementCandidates") != V::Undefined
                    || proof.get("replacementMutationsSha256") != V::Undefined
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                source
                    .verify_independent_mixed_post(&actual, raw)
                    .map_err(|_| ActiveServiceWalErrorV1::WrongPlan)?;
                terminal = true;
            }
        }
        if !terminal || escrow_at != target_escrows.len() || recovery_at != recoveries.len() {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(VerifiedMixedOrdinaryClosureV1 {
            sequence,
            post,
            write_before: current,
            write_context: last_context,
            first_error,
            last_error,
            attempts: inputs.len(),
            recovery_links: recoveries.len(),
            replacements,
        })
    }
}
impl ActualValidityPlanFactExpectationV1 {
    pub(crate) fn verify_recomputed_mixed_plan(
        &self,
        source: &crate::source_compact_index_v2::NativeHistoricalValidityAwaitV1,
    ) -> Result<()> {
        if self.native_read_context() != source.durable_read_context()
            || self.exact_prepared_mutations() != source.exact_prepared_mutations()
            || &self.original().metadata
                != source
                    .mixed_bound_write_before()
                    .unwrap_or(&source.query().snapshot)
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
}
