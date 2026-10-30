pub const ACTUAL_SOURCE_FACT_COLD_PIN_PROFILE: &str =
    "TL-MAIN-ACTUAL-SOURCE-FACT-COMMIT-TRACE-13-1";
pub const VERIFIED_ACTUAL_SOURCE_FACT_TRACE_PROFILE: &str =
    "TL-MAIN-VERIFIED-ACTUAL-SOURCE-FACT-TRACE-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActualSourceFactColdPinV1 {
    pub profile: String,
    pub wal: ActiveServiceWalPinV1,
    pub metadata_namespace: Namespace,
    pub trace: crate::storage::JournalCommitTracePinV1,
}
struct ActualSourceFactPastPrefixV1 {
    pin: ActiveServiceWalPinV1,
    record: Vec<u8>,
}
pub struct ActualSourcePendingFactInputV1 {
    metadata: ActualNativePlanFactExpectationV1,
    selection: V,
}
impl ActualSourcePendingFactInputV1 {
    pub fn metadata(&self) -> &ActualNativePlanFactExpectationV1 {
        &self.metadata
    }
    pub fn selection(&self) -> &V {
        &self.selection
    }
}
pub struct ActualSourceIoFactInputV1 {
    physical: u64,
    metadata: ActualNativePlanFactExpectationV1,
    io: ActualNativeIoFactExpectationV1,
    selection: V,
}
impl ActualSourceIoFactInputV1 {
    pub fn physical(&self) -> u64 {
        self.physical
    }
    pub fn metadata(&self) -> &ActualNativePlanFactExpectationV1 {
        &self.metadata
    }
    pub fn io(&self) -> &ActualNativeIoFactExpectationV1 {
        &self.io
    }
    pub fn selection(&self) -> &V {
        &self.selection
    }
}
/// Read-only trace/integrity prerequisite only. It grants neither a physical
/// native closure nor financial/source replay admission or a writer capability.
pub struct VerifiedActualSourceFactTraceV1 {
    inner: VerifiedActiveServiceAckHistoryV1,
    independent: ActualSourceFactColdPinV1,
    records: Vec<(u64, V)>,
    prefixes: BTreeMap<u64, ActualSourceFactPastPrefixV1>,
    paired_empty_post: Option<SourceIndexHistoryPinV1>,
    paired_origin_observation: Option<Vec<u8>>,
}
impl ActiveServiceWalV1 {
    pub fn capture_actual_source_fact_cold_pin(
        &mut self,
        metadata_namespace: &Namespace,
    ) -> Result<ActualSourceFactColdPinV1> {
        mode4_namespace(&self.namespace, metadata_namespace)?;
        actual_source_fact_origin(&parse(&self.context)?.get("origin"))?;
        let record = self.record()?;
        if record.get("profile") != s(ACTUAL_SOURCE_FACT_WAL_PROFILE)
            || record.get("stage") != s("Ack")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        actual_source_fact_entry_shape(&record)?;
        let wal = self.pin()?;
        let history = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &wal.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        Ok(ActualSourceFactColdPinV1 {
            profile: ACTUAL_SOURCE_FACT_COLD_PIN_PROFILE.into(),
            wal,
            metadata_namespace: metadata_namespace.clone(),
            trace: history.trace().clone(),
        })
    }
}
impl VerifiedActualSourceFactTraceV1 {
    pub fn open(
        root: &Path,
        namespace: Namespace,
        expected: &ActualSourceFactColdPinV1,
    ) -> Result<Self> {
        Self::open_bound_continuity(root, namespace, expected, AckFinancialContinuityV1::Strict)
    }
    fn open_for_paired_fresh(
        root: &Path,
        namespace: Namespace,
        expected: &ActualSourceFactColdPinV1,
    ) -> Result<Self> {
        Self::open_bound_continuity(
            root,
            namespace,
            expected,
            AckFinancialContinuityV1::PairedFreshPending,
        )
    }
    fn open_bound_continuity(
        root: &Path,
        namespace: Namespace,
        expected: &ActualSourceFactColdPinV1,
        continuity: AckFinancialContinuityV1,
    ) -> Result<Self> {
        if expected.profile != ACTUAL_SOURCE_FACT_COLD_PIN_PROFILE
            || expected.trace.profile != crate::storage::VERIFIED_COMMIT_TRACE_PROFILE
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        mode4_namespace(&namespace, &expected.metadata_namespace)?;
        let mut inner = VerifiedActiveServiceAckHistoryV1::open_profile_exact_with_continuity(
            root,
            namespace,
            &expected.wal,
            ExactAckHistoryProfileV1::ActualSourceFacts,
            Some(&expected.metadata_namespace),
            continuity,
        )?;
        let actual = inner
            .store
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &expected.wal.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        if actual.trace() != &expected.trace {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let mut entries = BTreeMap::new();
        let mut records = vec![];
        let mut prefixes = BTreeMap::new();
        let mut prefix_budget = 0usize;
        for (at, commit) in actual.commits().iter().enumerate() {
            if at == 0 {
                let created = bytes(&obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]))?;
                if commit.sequence != 1
                    || commit.mutations
                        != vec![
                            Mutation::Put {
                                key: CONTEXT.into(),
                                value: inner.store.context.clone(),
                            },
                            Mutation::Put {
                                key: RECORD.into(),
                                value: created,
                            },
                        ]
                {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                }
            } else {
                let [Mutation::Put { key, value }] = commit.mutations.as_slice() else {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                };
                if key != RECORD {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                }
                let record = parse(value)?;
                let mut optional =
                    vec!["nativeRecovery", "generationEscrow", "verifiedHistoryPost"];
                optional.extend_from_slice(ACTUAL_SOURCE_FACT_ENTRY_FIELDS);
                history_keys(
                    &record,
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
                if record.get("profile") != s(ACTUAL_SOURCE_FACT_WAL_PROFILE)
                    || !matches!(
                        history_text(&record.get("stage"))?,
                        "Intent"
                            | "EscrowBoundIntent"
                            | "NativeSupersessionIntent"
                            | "SourcePendingPlanObserved"
                            | "NativeIoObserved"
                            | "NativeRecoveryObserved"
                            | "Accepted"
                            | "Ack"
                    )
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                actual_source_fact_entry_shape(&record)?;
                records.push((commit.sequence, record));
            }
            for mutation in &commit.mutations {
                let Mutation::Put { key, value } = mutation else {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                };
                entries.insert(key.clone(), value.clone());
            }
            if entries.len() != 2 || entries.get(CONTEXT) != Some(&inner.store.context) {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let mut pin = expected.wal.clone();
            pin.prefix.physical_sequence = commit.sequence;
            pin.prefix.content = complete(&entries)?;
            let record = entries
                .get(RECORD)
                .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?;
            prefix_budget = prefix_budget
                .checked_add(bytes(&actual_fact_wal_value(&pin))?.len())
                .and_then(|n| n.checked_add(record.len()))
                .ok_or(ActiveServiceWalErrorV1::Capacity)?;
            if prefix_budget > MAX_VERIFIED_ACK_HISTORY_BYTES as usize {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
            prefixes.insert(
                commit.sequence,
                ActualSourceFactPastPrefixV1 {
                    pin,
                    record: record.clone(),
                },
            );
        }
        let result = Self {
            inner,
            independent: expected.clone(),
            records,
            prefixes,
            paired_empty_post: None,
            paired_origin_observation: None,
        };
        result.require_actual_fact_dependencies()?;
        result.require_actual_recorded_barriers()?;
        Ok(result)
    }
    pub fn profile(&self) -> &'static str {
        VERIFIED_ACTUAL_SOURCE_FACT_TRACE_PROFILE
    }
    pub fn pin(&self) -> &ActualSourceFactColdPinV1 {
        &self.independent
    }
    pub fn origin(&self) -> &V {
        self.inner.origin()
    }
    pub fn origin_archive(&self) -> &[u8] {
        self.inner.origin_archive()
    }
    pub fn origin_history(&self) -> &[u8] {
        self.inner.origin_history()
    }
    pub fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        self.inner.entries()
    }
    pub fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.inner.read_reference(pin)
    }
    pub fn source_pending_input(
        &self,
        sequence: u64,
        after: bool,
    ) -> Result<Option<ActualSourcePendingFactInputV1>> {
        let index = usize::try_from(
            sequence
                .checked_sub(1)
                .ok_or(ActiveServiceWalErrorV1::WrongContext)?,
        )
        .map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
        let entry = self
            .entries()
            .get(index)
            .ok_or(ActiveServiceWalErrorV1::WrongContext)?;
        if entry.sequence != sequence {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let raw = self.read_reference(&entry.entry)?;
        let value = parse(&raw)?;
        let (reference, selection) = if after {
            (
                value.get("actualPendingPlanAfter"),
                value.get("actualSelectionAfter"),
            )
        } else {
            (
                value.get("actualPendingPlanBefore"),
                value.get("actualSelectionBefore"),
            )
        };
        if reference == V::Null {
            return Ok(None);
        }
        let metadata = self.pending_fact(&reference)?;
        if metadata.native_read_context() != selection.get("pending").get("context") {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(Some(ActualSourcePendingFactInputV1 {
            metadata,
            selection,
        }))
    }
    pub fn source_io_inputs(&self, sequence: u64) -> Result<Vec<ActualSourceIoFactInputV1>> {
        if sequence == 0 || usize::try_from(sequence).map_or(true, |n| n > self.entries().len()) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let mut inputs = vec![];
        for (physical, value) in self.records.iter().filter(|(_, record)| {
            record.get("sequence") == s(sequence.to_string())
                && record.get("stage") == s("NativeIoObserved")
        }) {
            let reference = value.get("actualNativeIoPendingFact");
            let pending = self.read_reference(&history_blob(&reference)?)?;
            let raw = self.read_reference(&history_blob(&value.get("actualNativeIoFact"))?)?;
            let metadata = self.pending_fact(&reference)?;
            let io = ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                &self.independent.metadata_namespace,
                &pending,
                &raw,
            )?;
            let selection = value.get("actualIoSelection");
            if metadata.native_read_context() != selection.get("pending").get("context") {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            inputs.push(ActualSourceIoFactInputV1 {
                physical: *physical,
                metadata,
                io,
                selection,
            });
        }
        Ok(inputs)
    }
    fn pending_fact(&self, reference: &V) -> Result<ActualNativePlanFactExpectationV1> {
        let raw = self.read_reference(&history_blob(reference)?)?;
        let fact = ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(
            &self.independent.metadata_namespace,
            &raw,
        )?;
        let value = parse(&raw)?;
        let sequence = history_count(&value.get("actualWal").get("sequence"))?;
        let prefix = self
            .prefixes
            .get(&sequence)
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        fact.verify_origin_record(&prefix.pin, &self.inner.store.context, &prefix.record)?;
        Ok(fact)
    }
    fn require_actual_fact_dependencies(&self) -> Result<()> {
        for (physical, record) in &self.records {
            for name in ["actualPendingPlanBefore", "actualPendingPlanAfter"] {
                let reference = record.get(name);
                if reference != V::Null {
                    let raw = self.read_reference(&history_blob(&reference)?)?;
                    let sequence = history_count(&parse(&raw)?.get("actualWal").get("sequence"))?;
                    if sequence >= *physical {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    let fact = self.pending_fact(&reference)?;
                    let selection = record.get(if name == "actualPendingPlanBefore" {
                        "actualSelectionBefore"
                    } else {
                        "actualSelectionAfter"
                    });
                    if fact.native_read_context() != selection.get("pending").get("context") {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                }
            }
            if record.get("actualNativeIoFact") != V::Undefined {
                let pending_ref = record.get("actualNativeIoPendingFact");
                let pending = self.read_reference(&history_blob(&pending_ref)?)?;
                let io = self.read_reference(&history_blob(&record.get("actualNativeIoFact"))?)?;
                let fact = self.pending_fact(&pending_ref)?;
                if fact.native_read_context()
                    != record
                        .get("actualIoSelection")
                        .get("pending")
                        .get("context")
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                ActualNativeIoFactExpectationV1::from_canonical_fact_bytes(
                    &self.independent.metadata_namespace,
                    &pending,
                    &io,
                )?;
                let sequence = history_count(&parse(&pending)?.get("actualWal").get("sequence"))?;
                if sequence >= *physical {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
        }
        Ok(())
    }
    // No conversion to HISTORY/Mode4, physical-closure proof, financial owner,
    // mutable index or source actor. Ordered barrier semantics/physical closure
    // and raw-source execution remain mandatory later checks in this lane.
}
