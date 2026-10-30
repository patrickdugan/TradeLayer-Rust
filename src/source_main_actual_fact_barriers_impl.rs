impl VerifiedActualSourceFactTraceV1 {
    fn verify_actual_io_chain(&self, reference: &V, physical: u64) -> Result<()> {
        let mut next = reference.clone();
        let mut expected = None;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..65 {
            let pin = history_blob(&next)?;
            if !seen.insert((pin.bytes, pin.sha256)) {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let value = parse(&self.read_reference(&pin)?)?;
            history_keys(
                &value,
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
            let count = history_count(&value.get("attempt"))?;
            if value.get("profile") != s("TL-MAIN-ACTUAL-INLINE-IO-CHAIN-13-1")
                || count == 0
                || count > 65
                || expected.is_some_and(|n| n != count)
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            actual_source_fact_selection(&value.get("selection"))?;
            let raw_pending = self.read_reference(&history_blob(&value.get("pendingFact"))?)?;
            let pending = self.pending_fact(&value.get("pendingFact"))?;
            let raw_io = self.read_reference(&history_blob(&value.get("actualIoFact"))?)?;
            ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                &self.independent.metadata_namespace,
                &raw_pending,
                &raw_io,
            )?;
            let seq = history_count(&parse(&raw_pending)?.get("actualWal").get("sequence"))?;
            if seq >= physical
                || pending.native_read_context()
                    != value.get("selection").get("pending").get("context")
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            if count == 1 {
                if value.get("previous") != V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                return Ok(());
            }
            expected = Some(count - 1);
            next = value.get("previous");
        }
        Err(ActiveServiceWalErrorV1::Capacity)
    }
    fn require_actual_recorded_barriers(&self) -> Result<()> {
        let mut cursor = 0;
        for entry in self.entries() {
            let value = parse(&self.read_reference(&entry.entry)?)?;
            let action=crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(entry.raw_action()).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)?;
            let append = matches!(
                action,
                crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending
            );
            // 0=Intent,1=source/first IO,2=need recovery,3=native-only retry,
            // 4=ready Accepted,5=Accepted,6=Ack. No source action replay stage.
            let mut phase = 0;
            let mut attempt_barrier = 0;
            let mut io_attempts = 0;
            loop {
                let (physical, record) = self
                    .records
                    .get(cursor)
                    .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?;
                if history_count(&record.get("sequence"))? != entry.sequence {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                for name in [
                    "profile",
                    "sequence",
                    "previousHead",
                    "action",
                    "before",
                    "nativeCandidates",
                    "nativeMutations",
                    "verifiedHistoryBefore",
                    "requiredNativeFactProfile",
                    "actualPendingPlanBefore",
                    "actualSelectionBefore",
                ] {
                    if record.get(name) != value.get(name) {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                }
                if record.get("origin") != self.origin().get("archive") {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                match history_text(&record.get("stage"))? {
                    "Intent" => {
                        if phase != 0 {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                        phase = 1;
                        attempt_barrier = *physical;
                        if record.get("actualNativeIoFact") != V::Undefined
                            || record.get("accepted") != V::Null
                            || record.get("entry") != V::Null
                        {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                    }
                    "EscrowBoundIntent" => {
                        if !append
                            || !matches!(phase, 1 | 3)
                            || record.get("generationEscrow") == V::Undefined
                        {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                        attempt_barrier = *physical;
                    }
                    "SourcePendingPlanObserved" => {
                        if phase != 1 || record.get("actualNativeIoFact") != V::Undefined {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                        phase = 4;
                    }
                    "NativeIoObserved" => {
                        if !append || !matches!(phase, 1 | 3) {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                        let pending = self.read_reference(&history_blob(
                            &record.get("actualNativeIoPendingFact"),
                        )?)?;
                        if history_count(&parse(&pending)?.get("actualWal").get("sequence"))?
                            != attempt_barrier
                        {
                            return Err(ActiveServiceWalErrorV1::WrongPin);
                        }
                        let io =
                            self.read_reference(&history_blob(&record.get("actualNativeIoFact"))?)?;
                        let io = ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                            &self.independent.metadata_namespace,
                            &pending,
                            &io,
                        )?;
                        io_attempts += 1;
                        if io_attempts > 1 && record.get("actualNativeIoChain") == V::Undefined {
                            return Err(ActiveServiceWalErrorV1::WrongPin);
                        }
                        phase = if io.observed_outcome().get("kind") == s("NativeIoError") {
                            2
                        } else {
                            4
                        };
                    }
                    "NativeSupersessionIntent" => {
                        if phase != 2
                            || record.get("nativeRecovery") == V::Undefined
                            || record.get("plannedReplacementSelection") == V::Null
                            || record.get("plannedReplacementSelection") == V::Undefined
                        {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                        phase = 3;
                        attempt_barrier = *physical;
                    }
                    "NativeRecoveryObserved" => {
                        if phase != 2 || record.get("nativeRecovery") == V::Undefined {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                        phase = 4;
                    }
                    "Accepted" => {
                        if phase != 4
                            || record.get("accepted") == V::Null
                            || record.get("entry") == V::Null
                        {
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
                if record.get("actualNativeIoChain") != V::Undefined {
                    self.verify_actual_io_chain(&record.get("actualNativeIoChain"), *physical)?;
                    let chain = parse(
                        &self.read_reference(&history_blob(&record.get("actualNativeIoChain"))?)?,
                    )?;
                    if history_count(&chain.get("attempt"))? != io_attempts {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                }
                if matches!(phase, 5 | 6) {
                    for name in ACTUAL_SOURCE_FACT_ENTRY_FIELDS.iter().copied().chain([
                        "nativeRecovery",
                        "generationEscrow",
                        "accepted",
                        "verifiedHistoryPost",
                    ]) {
                        if record.get(name) != value.get(name) {
                            return Err(ActiveServiceWalErrorV1::WrongPin);
                        }
                    }
                    if record.get("entry") != entry.entry.value()
                        || record.get("head") != s(hex(&entry.head))
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                }
                cursor += 1;
                if phase == 6 {
                    break;
                }
            }
        }
        if cursor != self.records.len() {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(())
    }
}
