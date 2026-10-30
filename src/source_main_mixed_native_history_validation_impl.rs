fn mixed_history_extra_fields() -> Vec<&'static str> {
    let mut fields = vec!["nativeRecovery", "generationEscrow"];
    fields.extend_from_slice(ACTUAL_SOURCE_FACT_ENTRY_FIELDS);
    fields.extend_from_slice(MIXED_ORDINARY_FACT_FIELDS);
    fields
}
fn mixed_history_entry_shape(value: &V) -> Result<()> {
    MixedNativeHistoryOperationV1::from_record(value)?;
    actual_source_fact_entry_shape(value)?;
    for side in ["Before", "After"] {
        let pending = value.get(&format!("ordinaryPendingPlan{side}"));
        let selection = value.get(&format!("ordinarySelection{side}"));
        actual_source_fact_blob_or_null(&pending)?;
        if pending == V::Null {
            if selection != V::Null {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
        } else {
            mixed_validity_selection(&selection)?;
        }
    }
    for name in [
        "ordinarySourceSelection",
        "ordinaryIoSelection",
        "ordinaryPlannedReplacementSelection",
    ] {
        let value = value.get(name);
        if !matches!(value, V::Undefined | V::Null) {
            mixed_validity_selection(&value)?;
        }
    }
    for name in [
        "ordinaryNativeIoFact",
        "ordinaryNativeIoPendingFact",
        "ordinaryNativeBeforeHistory",
        "ordinaryNativeCandidates",
        "ordinaryNativeMutations",
        "ordinaryNativeIoChain",
    ] {
        let value = value.get(name);
        if value != V::Undefined {
            actual_source_fact_blob_or_null(&value)?;
        }
    }
    Ok(())
}
fn mixed_validity_selection(value: &V) -> Result<()> {
    let source = if value.get("profile") == s("TL-MAIN-MIXED-NATIVE-SELECTION-13-1") {
        history_keys(value, &["profile", "kind", "actualSource"], &[])?;
        if value.get("kind") != s("OrdinaryValidity") {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        value.get("actualSource")
    } else {
        value.clone()
    };
    history_keys(
        &source,
        &[
            "profile",
            "sourceProfile",
            "originCommands",
            "originHead",
            "originPhysical",
            "serviceActions",
            "serviceHead",
            "pending",
        ],
        &[],
    )?;
    if source.get("profile") != s("TL-MAIN-ACTUAL-ORDINARY-VALIDITY-SELECTION-13-1")
        || source.get("sourceProfile") != s(MODE4_INLINE_SERVICE_PROFILE)
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    for name in ["originCommands", "serviceActions"] {
        history_count(&source.get(name))?;
    }
    for name in ["originHead", "serviceHead"] {
        history_hash(&source.get(name))?;
    }
    let pending = source.get("pending");
    history_keys(
        &pending,
        &[
            "profile",
            "kind",
            "callerScope",
            "sourceJob",
            "methodScope",
            "operationOrdinal",
            "role",
            "context",
            "route",
            "caller",
        ],
        &[],
    )?;
    if pending.get("profile") != s("TL-MAIN-RETAINED-CONSENSUS-PENDING-13-1")
        || pending.get("kind") != s("ServiceValidity")
        || pending.get("callerScope") != s("0")
        || pending.get("route") != V::Null
        || pending.get("caller") != V::Null
        || pending.get("context").get("profile")
            != s(crate::source_compact_index_v2::PREPARED_VALIDITY_PROFILE)
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    for name in ["sourceJob", "methodScope", "operationOrdinal"] {
        history_count(&pending.get(name))?;
    }
    if !matches!(history_text(&pending.get("role"))?, "Awaited" | "Detached") {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(())
}
impl VerifiedMixedNativeHistoryV1 {
    // Reconstruct the producer's actual two-key prefix from real frames under
    // the retained file lock. A fact's hashes never substitute for these bytes.
    fn mixed_producer_prefix(
        &mut self,
        raw: &[u8],
        upper: u64,
    ) -> Result<(ActiveServiceWalPinV1, Vec<u8>)> {
        let value = parse(raw)?;
        let claims = value.get("actualWal");
        let sequence = history_count(&claims.get("sequence"))?;
        if sequence == 0 || sequence >= upper {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let mut pin = self.expected.wal.clone();
        pin.prefix.physical_sequence = sequence;
        let kv = claims.get("kv");
        pin.prefix.content = JournalCompleteKvReceiptV1 {
            profile: history_text(&kv.get("profile"))?.into(),
            keys: history_count(&kv.get("keys"))?,
            bytes: history_count(&kv.get("bytes"))?,
            chain_sha256d: history_hash(&kv.get("sha256d"))?,
            complete: kv.get("complete") == V::Bool(true),
        };
        if actual_fact_wal_value(&pin) != claims {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let actual = self
            .store
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_history_prefix(&pin.prefix)?;
        if actual.entries().count() != 2
            || actual.get(CONTEXT) != Some(self.store.context.as_slice())
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let record = actual
            .get(RECORD)
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?
            .to_vec();
        Ok((pin, record))
    }
    fn verify_mixed_pending(
        &mut self,
        reference: &V,
        ordinary: bool,
        upper: u64,
    ) -> Result<Vec<u8>> {
        let raw = self.read_reference(&history_blob(reference)?)?;
        let (pin, record) = self.mixed_producer_prefix(&raw, upper)?;
        if ordinary {
            ActualValidityPlanFactExpectationV1::from_canonical_fact_bytes(
                &self.mixed_expected.metadata_namespace,
                &raw,
            )?
            .verify_origin_record(&pin, &self.store.context, &record)?;
        } else {
            ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(
                &self.mixed_expected.metadata_namespace,
                &raw,
            )?
            .verify_origin_record(&pin, &self.store.context, &record)?;
        }
        Ok(raw)
    }
    fn verify_mixed_action(
        &mut self,
        entry: &V,
        lineage: (&ActiveServiceWalBlobV1, [u8; 32], &V),
        kind: MixedNativeHistoryOperationV1,
        action: &crate::source_main_native_session::NativeMainActiveServiceActionV1,
        records: &[(u64, V)],
    ) -> Result<()> {
        let (pin, next_head, prior_source) = lineage;
        use crate::source_main_native_session::NativeMainActiveServiceActionV1 as A;
        let append = matches!(action, A::AppendPending);
        let mut phase = 0;
        let mut barrier = 0;
        let mut attempts = 0;
        let mut observed_io = Vec::new();
        for (physical, record) in records {
            let mut optional = mixed_history_extra_fields();
            optional.extend_from_slice(&["verifiedHistoryPost", "sourceHead", "historySegment"]);
            history_keys(
                record,
                &[
                    "profile",
                    "stage",
                    "sequence",
                    "previousHead",
                    "head",
                    "action",
                    "before",
                    "origin",
                    "nativeCandidates",
                    "nativeMutations",
                    "accepted",
                    "entry",
                    "verifiedHistoryBefore",
                ],
                &optional,
            )?;
            mixed_history_entry_shape(record)?;
            for name in [
                "profile",
                "sequence",
                "previousHead",
                "action",
                "before",
                "nativeCandidates",
                "nativeMutations",
                "verifiedHistoryBefore",
                "historySegment",
                "requiredNativeFactProfile",
                "requiredOrdinaryNativeFactProfile",
                "mixedOperationKind",
                "actualPendingPlanBefore",
                "actualSelectionBefore",
                "ordinaryPendingPlanBefore",
                "ordinarySelectionBefore",
            ] {
                if record.get(name) != entry.get(name) {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
            if record.get("origin") != self.origin.get("archive") {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            for (reference, ordinary) in [
                ("actualPendingPlanBefore", false),
                ("actualPendingPlanAfter", false),
                ("ordinaryPendingPlanBefore", true),
                ("ordinaryPendingPlanAfter", true),
            ] {
                if record.get(reference) != V::Null {
                    self.verify_mixed_pending(&record.get(reference), ordinary, *physical)?;
                }
            }
            match history_text(&record.get("stage"))? {
                "Intent" => {
                    if phase != 0
                        || record.get("accepted") != V::Null
                        || record.get("entry") != V::Null
                        || record.get("head") != V::Null
                        || record.get("sourceHead") != *prior_source
                        || record.get("verifiedHistoryPost") != V::Undefined
                    {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    phase = 1;
                    barrier = *physical;
                }
                "EscrowBoundIntent" => {
                    if !append
                        || !matches!(phase, 1 | 3)
                        || record.get("generationEscrow") == V::Undefined
                    {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    barrier = *physical;
                }
                "SourcePendingPlanObserved" => {
                    if phase != 1 || record.get(kind.io_key()) != V::Undefined {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    phase = 4;
                }
                "NativeIoObserved" => {
                    if !append
                        || kind == MixedNativeHistoryOperationV1::Source
                        || !matches!(phase, 1 | 3)
                    {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    let pending = self.verify_mixed_pending(
                        &record.get(kind.pending_key()),
                        kind == MixedNativeHistoryOperationV1::Validity,
                        *physical,
                    )?;
                    if history_count(&parse(&pending)?.get("actualWal").get("sequence"))? != barrier
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    let io = self.read_reference(&history_blob(&record.get(kind.io_key()))?)?;
                    let failed = if kind == MixedNativeHistoryOperationV1::Validity {
                        ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(
                            &self.mixed_expected.metadata_namespace,
                            &pending,
                            &io,
                        )?
                        .native_io()
                        .is_some()
                    } else {
                        ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                            &self.mixed_expected.metadata_namespace,
                            &pending,
                            &io,
                        )?
                        .observed_outcome()
                        .get("kind")
                            == s("NativeIoError")
                    };
                    attempts += 1;
                    observed_io.push((
                        record.get(kind.pending_key()),
                        record.get(kind.io_key()),
                        record.get(if kind == MixedNativeHistoryOperationV1::Validity {
                            "ordinaryIoSelection"
                        } else {
                            "actualIoSelection"
                        }),
                    ));
                    phase = if failed { 2 } else { 4 };
                }
                "NativeSupersessionIntent" => {
                    if phase != 2
                        || record.get("nativeRecovery") == V::Undefined
                        || matches!(record.get(kind.replacement_key()), V::Undefined | V::Null)
                    {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    phase = 3;
                    barrier = *physical;
                }
                "NativeRecoveryObserved" => {
                    if phase != 2 || record.get("nativeRecovery") == V::Undefined {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    phase = 4;
                }
                "Accepted" => {
                    if phase != 4 {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    phase = 5;
                }
                "Ack" => {
                    if phase != 5 {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                    phase = 6;
                }
                _ => return Err(ActiveServiceWalErrorV1::WrongContext),
            }
            if record.get(kind.chain_key()) != V::Undefined {
                self.verify_mixed_io_chain(
                    &record.get(kind.chain_key()),
                    kind,
                    *physical,
                    attempts,
                    &observed_io,
                )?;
            } else if attempts > 1 {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            if phase >= 5 {
                for name in mixed_history_extra_fields().into_iter().chain([
                    "accepted",
                    "verifiedHistoryPost",
                    "sourceHead",
                ]) {
                    if record.get(name) != entry.get(name) {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                }
                if record.get("entry") != pin.value() || record.get("head") != s(hex(&next_head)) {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
        }
        if phase != 6 {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(())
    }
    fn verify_mixed_io_chain(
        &mut self,
        reference: &V,
        kind: MixedNativeHistoryOperationV1,
        physical: u64,
        attempts: u64,
        observed: &[(V, V, V)],
    ) -> Result<()> {
        let mut next = reference.clone();
        let mut count = attempts;
        if count == 0
            || count
                > if kind == MixedNativeHistoryOperationV1::Validity {
                    64
                } else {
                    65
                }
            || usize::try_from(count).ok() != Some(observed.len())
        {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        while count > 0 {
            let v = parse(&self.read_reference(&history_blob(&next)?)?)?;
            history_keys(
                &v,
                &[
                    "profile",
                    "attempt",
                    "previous",
                    "selection",
                    "pendingFact",
                    "actualIoFact",
                ],
                &[],
            )?;
            let required = if kind == MixedNativeHistoryOperationV1::Validity {
                "TL-MAIN-ACTUAL-ORDINARY-IO-CHAIN-13-1"
            } else {
                "TL-MAIN-ACTUAL-INLINE-IO-CHAIN-13-1"
            };
            if v.get("profile") != s(required) || history_count(&v.get("attempt"))? != count {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let (pending_ref, io_ref, selection) = &observed
                [usize::try_from(count - 1).map_err(|_| ActiveServiceWalErrorV1::Capacity)?];
            if v.get("pendingFact") != *pending_ref
                || v.get("actualIoFact") != *io_ref
                || v.get("selection") != *selection
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let pending = self.verify_mixed_pending(
                &v.get("pendingFact"),
                kind == MixedNativeHistoryOperationV1::Validity,
                physical,
            )?;
            let io = self.read_reference(&history_blob(&v.get("actualIoFact"))?)?;
            if kind == MixedNativeHistoryOperationV1::Validity {
                ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(
                    &self.mixed_expected.metadata_namespace,
                    &pending,
                    &io,
                )?;
            } else {
                ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                    &self.mixed_expected.metadata_namespace,
                    &pending,
                    &io,
                )?;
            }
            count -= 1;
            next = v.get("previous");
        }
        if next != V::Null {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(())
    }
}
