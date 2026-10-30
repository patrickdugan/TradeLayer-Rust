// The non-strict arm is PRIVATE and unpublished: the outer reader binds every
// gap to raw feed records, and only the separate actual fresh actor closes it.
#[derive(Clone, Copy)]
enum AckFinancialContinuityV1 {
    Strict,
    PairedFreshPending,
}
// Exact new paired admission only. Neither old HISTORY nor13-2 is widened.
pub const DURABLE_FRESH_COLD_PIN_PROFILE: &str = "TL-MAIN-DURABLE-FRESH-PAIRED-TRACE-13-1";
pub const VERIFIED_DURABLE_FRESH_HISTORY_PROFILE: &str =
    "TL-MAIN-VERIFIED-DURABLE-FRESH-HISTORY-13-1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurableFreshSourceEntryV1 {
    Created,
    Acknowledged,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableFreshColdPinV1 {
    pub profile: String,
    pub outer: ActiveServiceWalPinV1,
    pub trace: crate::storage::JournalCommitTracePinV1,
    pub source_entry: DurableFreshSourceEntryV1,
    pub source: ActualSourceFactColdPinV1,
}
pub(crate) struct DurableFreshVerifiedReceiverV1 {
    raw: std::sync::Arc<Vec<u8>>,
    metadata: SourceIndexHistoryPinV1,
    pending: Option<ActualNativePlanFactExpectationV1>,
    pending_raw: Option<std::sync::Arc<Vec<u8>>>,
    prefix_raw: std::sync::Arc<Vec<u8>>,
    selection: V,
    source_actions: u64,
    source_head: [u8; 32],
}
pub struct VerifiedDurableFreshEntryV1 {
    sequence: u64,
    action: crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1,
    raw_action: std::sync::Arc<Vec<u8>>,
    before: std::sync::Arc<Vec<u8>>,
    after: std::sync::Arc<Vec<u8>>,
    before_source: std::sync::Arc<Vec<u8>>,
    after_source: std::sync::Arc<Vec<u8>>,
    before_receiver: DurableFreshVerifiedReceiverV1,
    after_receiver: DurableFreshVerifiedReceiverV1,
    outcome: V,
    entry: std::sync::Arc<Vec<u8>>,
    previous_head: [u8; 32],
    head: [u8; 32],
}
pub struct VerifiedDurableFreshOuterTraceV1 {
    store: ActiveServiceWalV1,
    independent: DurableFreshColdPinV1,
    origin: V,
    origin_receiver: DurableFreshVerifiedReceiverV1,
    origin_observation: std::sync::Arc<Vec<u8>>,
    origin_source: std::sync::Arc<Vec<u8>>,
    entries: Vec<VerifiedDurableFreshEntryV1>,
}
pub struct VerifiedDurableFreshPairedHistoryV1 {
    outer: VerifiedDurableFreshOuterTraceV1,
    source: VerifiedActualSourceFactTraceV1,
}
impl ActiveServiceWalV1 {
    pub(crate) fn capture_durable_fresh_outer_trace(
        &mut self,
    ) -> Result<(
        ActiveServiceWalPinV1,
        crate::storage::JournalCommitTracePinV1,
    )> {
        if parse(&self.context)?.get("origin").get("profile")
            != s(DURABLE_FRESH_COORDINATION_PROFILE)
            || self.record()?.get("stage") != s("Ack")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let pin = self.pin()?;
        let actual = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &pin.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        Ok((pin, actual.trace().clone()))
    }
}
struct DurableFreshBlobCacheV1 {
    bytes: u64,
    raw: BTreeMap<([u8; 32], u64), std::sync::Arc<Vec<u8>>>,
}
impl DurableFreshBlobCacheV1 {
    fn read(&mut self, store: &ActiveServiceWalV1, value: &V) -> Result<std::sync::Arc<Vec<u8>>> {
        let pin = history_blob(value)?;
        let key = (pin.sha256, pin.bytes);
        if let Some(raw) = self.raw.get(&key) {
            return Ok(raw.clone());
        }
        self.bytes = self
            .bytes
            .checked_add(pin.bytes)
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        if self.bytes > MAX_VERIFIED_ACK_HISTORY_BYTES {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        let raw = std::sync::Arc::new(store.read_blob(&pin)?);
        self.raw.insert(key, raw.clone());
        Ok(raw)
    }
}
fn durable_fresh_trace_value(trace: &crate::storage::JournalCommitTracePinV1) -> V {
    obj(&[
        ("profile", s(&trace.profile)),
        ("commits", s(trace.commits.to_string())),
        ("bytes", s(trace.bytes.to_string())),
        ("sha256d", s(hex(&trace.chain_sha256d))),
    ])
}
fn durable_fresh_observation_source(
    store: &ActiveServiceWalV1,
    raw: &[u8],
    cache: &mut DurableFreshBlobCacheV1,
) -> Result<std::sync::Arc<Vec<u8>>> {
    let value = parse(raw)?;
    history_keys(
        &value,
        &["profile", "sourceObservation", "feeds", "feedHead"],
        &[],
    )?;
    if value.get("profile") != s(DURABLE_FRESH_COORDINATION_PROFILE) {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    history_count(&value.get("feeds"))?;
    history_hash(&value.get("feedHead"))?;
    cache.read(store, &value.get("sourceObservation"))
}
impl VerifiedActualSourceFactTraceV1 {
    fn verify_durable_fresh_receiver(
        &mut self,
        store: &ActiveServiceWalV1,
        value: &V,
        cache: &mut DurableFreshBlobCacheV1,
        checked: &mut BTreeMap<u64, V>,
    ) -> Result<DurableFreshVerifiedReceiverV1> {
        let raw = cache.read(store, value)?;
        let receiver = parse(&raw)?;
        history_keys(
            &receiver,
            &[
                "profile",
                "sourcePrefix",
                "metadata",
                "pendingPlan",
                "selection",
            ],
            &[],
        )?;
        if receiver.get("profile") != s("TL-MAIN-DURABLE-FRESH-ACTUAL-RECEIVER-13-1") {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let prefix_raw = cache.read(store, &receiver.get("sourcePrefix"))?;
        let prefix = parse(&prefix_raw)?;
        history_keys(
            &prefix,
            &[
                "profile",
                "sourceWal",
                "originContext",
                "actualRecord",
                "trace",
            ],
            &[],
        )?;
        if prefix.get("profile") != s(DURABLE_FRESH_BOUNDARY_PROFILE)
            || prefix.get("originContext") != parse(&self.inner.store.context)?
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let point = history_count(&prefix.get("sourceWal").get("sequence"))?;
        let actual = self
            .prefixes
            .get(&point)
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        if prefix.get("sourceWal") != actual_fact_wal_value(&actual.pin)
            || prefix.get("actualRecord") != parse(&actual.record)?
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let actual_trace = if let Some(trace) = checked.get(&point) {
            trace.clone()
        } else {
            let trace = self
                .inner
                .store
                .journal
                .as_mut()
                .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
                .read_verified_fresh_trace_point(
                    &self.independent.wal.prefix,
                    &actual.pin.prefix,
                )?;
            let value = durable_fresh_trace_value(&trace);
            checked.insert(point, value.clone());
            value
        };
        if prefix.get("trace") != actual_trace {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let record = prefix.get("actualRecord");
        let (source_actions, source_head) =
            if record == obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]) {
                (0, sha256(&bytes(self.origin())?))
            } else {
                actual_source_fact_entry_shape(&record)?;
                if record.get("stage") != s("Ack") {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                (
                    history_count(&record.get("sequence"))?,
                    history_hash(&record.get("head"))?,
                )
            };
        let metadata_raw = cache.read(store, &receiver.get("metadata"))?;
        let metadata = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            &self.independent.metadata_namespace,
            &metadata_raw,
        )
        .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let mut pending_raw = None;
        let pending = if receiver.get("pendingPlan") == V::Null {
            None
        } else {
            let raw = cache.read(store, &receiver.get("pendingPlan"))?;
            let expected = ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(
                &self.independent.metadata_namespace,
                &raw,
            )?;
            let point = history_count(&parse(&raw)?.get("actualWal").get("sequence"))?;
            let actual = self
                .prefixes
                .get(&point)
                .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
            expected.verify_origin_record(
                &actual.pin,
                &self.inner.store.context,
                &actual.record,
            )?;
            pending_raw = Some(raw);
            Some(expected)
        };
        let selection = receiver.get("selection");
        if let Some(expected) = &pending {
            actual_source_fact_selection(&selection)?;
            if expected.native_read_context() != selection.get("pending").get("context") {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
        } else if selection != V::Null {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(DurableFreshVerifiedReceiverV1 {
            raw,
            metadata,
            pending,
            pending_raw,
            prefix_raw,
            selection,
            source_actions,
            source_head,
        })
    }
}
impl VerifiedDurableFreshPairedHistoryV1 {
    pub fn profile(&self) -> &'static str {
        VERIFIED_DURABLE_FRESH_HISTORY_PROFILE
    }
    pub fn entries(&self) -> &[VerifiedDurableFreshEntryV1] {
        &self.outer.entries
    }
    pub(crate) fn source(&self) -> &VerifiedActualSourceFactTraceV1 {
        &self.source
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        VerifiedActualSourceFactTraceV1,
        VerifiedDurableFreshOuterTraceV1,
    ) {
        (self.source, self.outer)
    }
    pub fn open(
        outer_root: &Path,
        outer_namespace: Namespace,
        source_root: &Path,
        source_namespace: Namespace,
        expected: &DurableFreshColdPinV1,
    ) -> Result<Self> {
        if expected.profile != DURABLE_FRESH_COLD_PIN_PROFILE
            || expected.trace.profile != crate::storage::VERIFIED_COMMIT_TRACE_PROFILE
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let root = directory(outer_root)?;
        if root != expected.outer.root
            || expected.outer.profile != PROFILE
            || expected.outer.prefix.namespace != outer_namespace
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let (journal, _) = Journal::open_existing_verified_prefix(
            &root,
            outer_namespace.clone(),
            Recovery::RejectIncompleteTail,
            &expected.outer.prefix,
        )?;
        if journal.entries()?.count() != 2 {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let context = journal
            .get(CONTEXT)?
            .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?
            .to_vec();
        let value = parse(&context)?;
        history_keys(
            &value,
            &["profile", "origin", "filesystem", "creation"],
            &[],
        )?;
        if value.get("profile") != s(PROFILE) || history_text(&value.get("creation"))?.is_empty() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let filesystem = match history_text(&value.get("filesystem"))? {
            "FileSyncOnly" => SourceDurableFilesystem::FileSyncOnly,
            "FileAndDirectorySync" => SourceDurableFilesystem::FileAndDirectorySync,
            _ => return Err(ActiveServiceWalErrorV1::WrongContext),
        };
        let origin = value.get("origin");
        history_keys(
            &origin,
            &[
                "profile",
                "rawActionCodec",
                "sourceFactsProfile",
                "sourceArchive",
                "sourceBoundary",
                "sourceObservation",
                "feedHead",
            ],
            &[],
        )?;
        if origin.get("profile") != s(DURABLE_FRESH_COORDINATION_PROFILE)
            || origin.get("rawActionCodec") != s("TLMDFI1")
            || origin.get("sourceFactsProfile") != s(ACTUAL_SOURCE_FACT_WAL_PROFILE)
            || origin.get("feedHead")
                != s(hex(&sha256(DURABLE_FRESH_COORDINATION_PROFILE.as_bytes())))
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let mut store = ActiveServiceWalV1 {
            root,
            namespace: outer_namespace,
            journal: Some(journal),
            context,
            filesystem,
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
            ambiguous: false,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            commit_probe: None,
        };
        let actual = store
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &expected.outer.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        if actual.trace() != &expected.trace {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let mut cache = DurableFreshBlobCacheV1 {
            bytes: 0,
            raw: BTreeMap::new(),
        };
        let mut checked = BTreeMap::new();
        let archive = cache.read(&store, &origin.get("sourceArchive"))?;
        let mut source = match expected.source_entry {
            DurableFreshSourceEntryV1::Acknowledged => {
                VerifiedActualSourceFactTraceV1::open_for_paired_fresh(
                    source_root,
                    source_namespace,
                    &expected.source,
                )?
            }
            DurableFreshSourceEntryV1::Created => {
                let receiver = parse(&cache.read(&store, &origin.get("sourceBoundary"))?)?;
                let metadata = cache.read(&store, &receiver.get("metadata"))?;
                let observation = cache.read(&store, &origin.get("sourceObservation"))?;
                let observed = durable_fresh_observation_source(&store, &observation, &mut cache)?;
                VerifiedActualSourceFactTraceV1::open_for_paired_created(
                    source_root,
                    source_namespace,
                    &expected.source,
                    &archive,
                    &metadata,
                    &observed,
                )?
            }
        };
        if archive.as_slice() != source.origin_archive() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let origin_receiver = source.verify_durable_fresh_receiver(
            &store,
            &origin.get("sourceBoundary"),
            &mut cache,
            &mut checked,
        )?;
        let origin_observation = cache.read(&store, &origin.get("sourceObservation"))?;
        let origin_source =
            durable_fresh_observation_source(&store, &origin_observation, &mut cache)?;
        let adopted = usize::try_from(origin_receiver.source_actions)
            .map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
        if adopted > source.entries().len() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let mut original_source = source.original_observation_bytes()?;
        for child in &source.entries()[..adopted] {
            if child.actual_before() != original_source {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            original_source = history_accepted_parts_profile(child.actual_accepted(), true)?
                .0
                .to_vec();
        }
        if original_source.as_slice() != origin_source.as_slice() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }

        let commits = actual.commits();
        if commits.is_empty() || (commits.len() - 1) % 4 != 0 || (commits.len() - 1) / 4 > 4096 {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        if commits[0].sequence != 1
            || commits[0].mutations
                != vec![
                    Mutation::Put {
                        key: CONTEXT.into(),
                        value: store.context.clone(),
                    },
                    Mutation::Put {
                        key: RECORD.into(),
                        value: bytes(&obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]))?,
                    },
                ]
        {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let mut entries = vec![];
        let mut head = sha256(&bytes(&origin)?);
        let mut previous_source = origin_receiver.source_actions;
        let mut previous_after = origin_observation.clone();
        let mut previous_receiver = origin_receiver.raw.clone();
        let mut raw_charged = 0usize;
        let mut feed_count = 0u64;
        let mut feed_head = sha256(DURABLE_FRESH_COORDINATION_PROFILE.as_bytes());
        for (at, group) in commits[1..].chunks_exact(4).enumerate() {
            let sequence = at as u64 + 1;
            let mut records = vec![];
            for (commit, stage) in
                group
                    .iter()
                    .zip(["Intent", "SourceInputsCaptured", "Accepted", "Ack"])
            {
                let [Mutation::Put { key, value }] = commit.mutations.as_slice() else {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                };
                if key != RECORD {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                }
                let record = parse(value)?;
                history_keys(
                    &record,
                    &[
                        "profile",
                        "stage",
                        "sequence",
                        "previousHead",
                        "action",
                        "before",
                        "sourceBefore",
                        "sourceAfter",
                        "after",
                        "entry",
                        "head",
                    ],
                    &[],
                )?;
                if record.get("profile") != s(DURABLE_FRESH_COORDINATION_PROFILE)
                    || record.get("stage") != s(stage)
                    || history_count(&record.get("sequence"))? != sequence
                    || record.get("previousHead") != s(hex(&head))
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                records.push(record);
            }
            let intent = &records[0];
            for record in &records[1..] {
                for field in ["action", "before", "sourceBefore"] {
                    if record.get(field) != intent.get(field) {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                }
            }
            if intent.get("sourceAfter") != V::Null
                || intent.get("after") != V::Null
                || intent.get("entry") != V::Null
                || intent.get("head") != V::Null
                || records[1].get("after") != V::Null
                || records[1].get("entry") != V::Null
                || records[1].get("head") != V::Null
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let accepted = &records[2];
            let ack = &records[3];
            for field in ["sourceAfter", "after", "entry", "head"] {
                if accepted.get(field) != ack.get(field) {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            }
            if records[1].get("sourceAfter") != ack.get("sourceAfter") {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let raw_action = cache.read(&store, &ack.get("action"))?;
            raw_charged = raw_charged
                .checked_add(raw_action.len())
                .ok_or(ActiveServiceWalErrorV1::Capacity)?;
            if raw_charged > 64 * 1024 * 1024 {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
            let action=crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::from_canonical_action_bytes(&raw_action).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)?;
            let before = cache.read(&store, &ack.get("before"))?;
            let after = cache.read(&store, &ack.get("after"))?;
            if before.as_slice() != previous_after.as_slice() {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let before_source = durable_fresh_observation_source(&store, &before, &mut cache)?;
            let after_source = durable_fresh_observation_source(&store, &after, &mut cache)?;
            let before_receiver = source.verify_durable_fresh_receiver(
                &store,
                &ack.get("sourceBefore"),
                &mut cache,
                &mut checked,
            )?;
            let after_receiver = source.verify_durable_fresh_receiver(
                &store,
                &ack.get("sourceAfter"),
                &mut cache,
                &mut checked,
            )?;
            if before_receiver.raw.as_slice() != previous_receiver.as_slice()
                || before_receiver.source_actions != previous_source
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            match &action{crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::Feed(_)=>{if before_receiver.source_actions!=after_receiver.source_actions || before_receiver.metadata!=after_receiver.metadata || before_receiver.source_head!=after_receiver.source_head{return Err(ActiveServiceWalErrorV1::WrongContext);}},crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::SourceAction(raw)=>{if after_receiver.source_actions==before_receiver.source_actions+1{let child=source.entries().get(before_receiver.source_actions as usize).ok_or(ActiveServiceWalErrorV1::WrongContext)?;if raw.canonical_action_bytes().map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)?.as_slice()!=child.raw_action() || before_source.as_slice()!=child.actual_before() || after_source.as_slice()!=history_accepted_parts_profile(child.actual_accepted(),true)?.0{return Err(ActiveServiceWalErrorV1::WrongContext);}}else if after_receiver.source_actions!=before_receiver.source_actions || before_source!=after_source{return Err(ActiveServiceWalErrorV1::WrongContext);}}}
            let entry = cache.read(&store, &ack.get("entry"))?;
            let body = parse(&entry)?;
            history_keys(
                &body,
                &[
                    "profile",
                    "sequence",
                    "previousHead",
                    "action",
                    "before",
                    "after",
                    "sourceBefore",
                    "sourceAfter",
                    "outcome",
                ],
                &[],
            )?;
            for field in [
                "profile",
                "sequence",
                "previousHead",
                "action",
                "before",
                "after",
                "sourceBefore",
                "sourceAfter",
            ] {
                if body.get(field) != ack.get(field) {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            }
            let before_observation = parse(&before)?;
            let after_observation = parse(&after)?;
            if history_count(&before_observation.get("feeds"))? != feed_count
                || before_observation.get("feedHead") != s(hex(&feed_head))
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let outcome = body.get("outcome");
            match(&action,history_text(&outcome.get("kind"))?){
            (crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::Feed(input),"ActualFeedPublished")=>{
                history_keys(&outcome,&["kind","ordinal","actualBoundary"],&[])?;
                feed_count=feed_count.checked_add(1).ok_or(ActiveServiceWalErrorV1::Capacity)?;if history_count(&outcome.get("ordinal"))?!=feed_count{return Err(ActiveServiceWalErrorV1::WrongContext);}
                let raw=bytes(&obj(&[("ordinal",s(feed_count.to_string())),("raw",input.value()),("actualBoundary",outcome.get("actualBoundary"))]))?;let mut next=feed_head.to_vec();next.extend_from_slice(&sha256(&raw));feed_head=sha256(&next);
            },
            (_,"ActualApiRejected")=>{history_keys(&outcome,&["kind","error"],&[])?;history_text(&outcome.get("error"))?;if before_source!=after_source || before_receiver.source_actions!=after_receiver.source_actions{return Err(ActiveServiceWalErrorV1::WrongContext);}},
            (crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::SourceAction(_),"ActualSourceAcknowledged")=>{history_keys(&outcome,&["kind","sourceActions","sourceHead"],&[])?;if history_count(&outcome.get("sourceActions"))?!=after_receiver.source_actions || history_hash(&outcome.get("sourceHead"))?!=after_receiver.source_head || before_receiver.source_actions+1!=after_receiver.source_actions{return Err(ActiveServiceWalErrorV1::WrongContext);}},
            _=>return Err(ActiveServiceWalErrorV1::WrongContext),
        }
            if history_count(&after_observation.get("feeds"))? != feed_count
                || after_observation.get("feedHead") != s(hex(&feed_head))
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let mut next = head.to_vec();
            next.extend_from_slice(&sha256(&entry));
            let next = sha256(&next);
            if ack.get("head") != s(hex(&next)) {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            previous_source = after_receiver.source_actions;
            previous_after = after.clone();
            previous_receiver = after_receiver.raw.clone();
            entries.push(VerifiedDurableFreshEntryV1 {
                sequence,
                action,
                raw_action,
                before,
                after,
                before_source,
                after_source,
                before_receiver,
                after_receiver,
                outcome: body.get("outcome"),
                entry,
                previous_head: head,
                head: next,
            });
            head = next;
        }
        if entries.is_empty()
            || entries.last().map(|v| v.after_receiver.source_actions)
                != Some(source.entries().len() as u64)
            || store.record()?
                != parse(match commits.last().and_then(|c| c.mutations.first()) {
                    Some(Mutation::Put { value, .. }) => value,
                    _ => return Err(ActiveServiceWalErrorV1::InvalidRecord),
                })?
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(Self {
            source,
            outer: VerifiedDurableFreshOuterTraceV1 {
                store,
                independent: expected.clone(),
                origin,
                origin_receiver,
                origin_observation,
                origin_source,
                entries,
            },
        })
    }
}

impl VerifiedDurableFreshEntryV1 {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn raw_action(&self) -> &[u8] {
        &self.raw_action
    }
    pub fn outcome(&self) -> &V {
        &self.outcome
    }
}

impl DurableFreshVerifiedReceiverV1 {
    pub(crate) fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub(crate) fn prefix_raw(&self) -> &[u8] {
        &self.prefix_raw
    }
    pub(crate) fn metadata(&self) -> &SourceIndexHistoryPinV1 {
        &self.metadata
    }
    pub(crate) fn pending(&self) -> Option<&ActualNativePlanFactExpectationV1> {
        self.pending.as_ref()
    }
    pub(crate) fn pending_raw(&self) -> Option<&[u8]> {
        self.pending_raw.as_deref().map(|v| v.as_slice())
    }
    pub(crate) fn selection(&self) -> &V {
        &self.selection
    }
    pub(crate) fn source_actions(&self) -> u64 {
        self.source_actions
    }
}
impl VerifiedDurableFreshOuterTraceV1 {
    pub(crate) fn origin_head(&self) -> [u8; 32] {
        sha256(&bytes(&self.origin).expect("verified canonical origin"))
    }
    pub(crate) fn origin_receiver(&self) -> &DurableFreshVerifiedReceiverV1 {
        &self.origin_receiver
    }
    pub(crate) fn origin_observation(&self) -> &[u8] {
        &self.origin_observation
    }
    pub(crate) fn origin_source(&self) -> &[u8] {
        &self.origin_source
    }
    pub(crate) fn entries(&self) -> &[VerifiedDurableFreshEntryV1] {
        &self.entries
    }
    pub fn pin(&self) -> &DurableFreshColdPinV1 {
        &self.independent
    }
    pub fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.store.read_blob(pin)
    }
}
impl VerifiedDurableFreshEntryV1 {
    pub(crate) fn action(
        &self,
    ) -> &crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1 {
        &self.action
    }
    pub(crate) fn before(&self) -> &[u8] {
        &self.before
    }
    pub(crate) fn after(&self) -> &[u8] {
        &self.after
    }
    pub(crate) fn before_source(&self) -> &[u8] {
        &self.before_source
    }
    pub(crate) fn after_source(&self) -> &[u8] {
        &self.after_source
    }
    pub(crate) fn before_receiver(&self) -> &DurableFreshVerifiedReceiverV1 {
        &self.before_receiver
    }
    pub(crate) fn after_receiver(&self) -> &DurableFreshVerifiedReceiverV1 {
        &self.after_receiver
    }
    pub(crate) fn entry(&self) -> &[u8] {
        &self.entry
    }
    pub(crate) fn previous_head(&self) -> [u8; 32] {
        self.previous_head
    }
    pub(crate) fn head(&self) -> [u8; 32] {
        self.head
    }
}

impl VerifiedActualSourceFactTraceV1 {
    // Private explicit Created admission for the paired profile only. Old public
    // ACK readers remain strict. Raw origin bytes come from actual outer blobs
    // and must match the real child context's independently captured hashes.
    fn open_for_paired_created(
        root: &Path,
        namespace: Namespace,
        expected: &ActualSourceFactColdPinV1,
        archive: &[u8],
        history: &[u8],
        observation: &[u8],
    ) -> Result<Self> {
        if expected.profile != ACTUAL_SOURCE_FACT_COLD_PIN_PROFILE
            || expected.trace.profile != crate::storage::VERIFIED_COMMIT_TRACE_PROFILE
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        mode4_namespace(&namespace, &expected.metadata_namespace)?;
        let root = directory(root)?;
        if expected.wal.profile != PROFILE
            || expected.wal.root != root
            || expected.wal.prefix.namespace != namespace
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let (mut journal, _) = Journal::open_existing_verified_prefix(
            &root,
            namespace.clone(),
            Recovery::RejectIncompleteTail,
            &expected.wal.prefix,
        )?;
        if journal.entries()?.count() != 2 {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let context = journal
            .get(CONTEXT)?
            .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?
            .to_vec();
        let decoded = parse(&context)?;
        history_keys(
            &decoded,
            &["profile", "origin", "filesystem", "creation"],
            &[],
        )?;
        if decoded.get("profile") != s(PROFILE)
            || history_text(&decoded.get("creation"))?.is_empty()
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let filesystem = match history_text(&decoded.get("filesystem"))? {
            "FileSyncOnly" => SourceDurableFilesystem::FileSyncOnly,
            "FileAndDirectorySync" => SourceDurableFilesystem::FileAndDirectorySync,
            _ => return Err(ActiveServiceWalErrorV1::WrongContext),
        };
        let origin = decoded.get("origin");
        actual_source_fact_origin(&origin)?;
        if history_blob(&origin.get("archive"))? != ActiveServiceWalBlobV1::of(archive)?
            || history_blob(&origin.get("requiredVerifiedHistory"))?
                != ActiveServiceWalBlobV1::of(history)?
            || history_blob(&origin.get("observation"))? != ActiveServiceWalBlobV1::of(observation)?
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let record = bytes(&obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]))?;
        if journal.get(RECORD)? != Some(record.as_slice()) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let actual = journal.read_verified_current_commits(
            &expected.wal.prefix,
            65536,
            MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
        )?;
        if actual.trace() != &expected.trace
            || actual.commits().len() != 1
            || actual.commits()[0].sequence != 1
            || actual.commits()[0].mutations
                != vec![
                    Mutation::Put {
                        key: CONTEXT.into(),
                        value: context.clone(),
                    },
                    Mutation::Put {
                        key: RECORD.into(),
                        value: record.clone(),
                    },
                ]
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let metadata = SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            &expected.metadata_namespace,
            history,
        )
        .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let store = ActiveServiceWalV1 {
            root,
            namespace,
            journal: Some(journal),
            context,
            filesystem,
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
            ambiguous: false,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            commit_probe: None,
        };
        let inner = VerifiedActiveServiceAckHistoryV1 {
            store,
            origin: origin.clone(),
            origin_archive: archive.to_vec(),
            origin_history: history.to_vec(),
            origin_observation: history_blob(&origin.get("observation"))?,
            head: sha256(&bytes(&origin)?),
            entries: vec![],
        };
        let mut prefixes = BTreeMap::new();
        prefixes.insert(
            1,
            ActualSourceFactPastPrefixV1 {
                pin: expected.wal.clone(),
                record,
            },
        );
        Ok(Self {
            inner,
            independent: expected.clone(),
            records: vec![],
            prefixes,
            paired_empty_post: Some(metadata),
            paired_origin_observation: Some(observation.to_vec()),
        })
    }
}

#[cfg(test)]
#[path = "source_main_durable_fresh_cold_reader_tests.rs"]
mod durable_fresh_cold_reader_tests;
