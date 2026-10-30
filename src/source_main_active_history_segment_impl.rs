// New metadata profile only. A checkpoint never executes source, appends native
// metadata, imports Before/Accepted as state, or creates a financial owner.
pub const SEGMENTED_ACTIVE_HISTORY_PROFILE: &str = "TL-MAIN-SEGMENTED-ACTIVE-HISTORY-13-1";
pub const ACTIVE_HISTORY_SEGMENT_PROFILE: &str = "TL-MAIN-ACK-HISTORY-SEGMENT-13-1";
const SEGMENT_WINDOW_LIMIT: usize = 256;
const SEGMENT_GRAPH_LIMIT: usize = 8192;
const SEGMENT_GRAPH_BYTES: u64 = 256 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveHistorySegmentCursorV1 {
    pub ordinal: u64,
    pub actions: u64,
    pub wal_head: [u8; 32],
    pub source_head: V,
    pub last_entry: ActiveServiceWalBlobV1,
    pub segment: ActiveServiceWalBlobV1,
    pub after: ActiveServiceWalPinV1,
    pub trace: crate::storage::JournalCommitTracePinV1,
}
pub struct PreparedActiveHistoryCheckpointV1 {
    write: PreparedActiveServiceWalWriteV1,
    before_trace: crate::storage::JournalCommitTracePinV1,
    ordinal: u64,
    actions: u64,
    wal_head: [u8; 32],
    source_head: V,
    last_entry: ActiveServiceWalBlobV1,
    segment: ActiveServiceWalBlobV1,
    dependencies: Vec<ActiveServiceWalBlobV1>,
    finished: bool,
}
impl PreparedActiveHistoryCheckpointV1 {
    pub fn before(&self) -> &ActiveServiceWalPinV1 {
        self.write.before()
    }
    pub fn post(&self) -> &ActiveServiceWalPinV1 {
        self.write.post()
    }
    pub fn attempted(&self) -> bool {
        self.write.attempted()
    }
}
/// Sealed after actual ACK+trace/blob verification. Not constructible from a
/// cursor/record/financial observation; the live controller must consume it.
pub struct SyncedActiveHistoryCheckpointV1 {
    owner: u64,
    cursor: ActiveHistorySegmentCursorV1,
}
impl SyncedActiveHistoryCheckpointV1 {
    pub fn cursor(&self) -> &ActiveHistorySegmentCursorV1 {
        &self.cursor
    }
}
fn segment_count(value: &V) -> Result<u64> {
    let V::String(text) = value else {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    let number = text
        .parse::<u64>()
        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
    if number.to_string() != *text {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(number)
}
fn segment_hash(value: &V) -> Result<[u8; 32]> {
    let V::String(text) = value else {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    let mut out = [0; 32];
    for (at, value) in out.iter_mut().enumerate() {
        *value = u8::from_str_radix(&text[at * 2..at * 2 + 2], 16)
            .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
    }
    Ok(out)
}
fn segment_blob(value: &V) -> Result<ActiveServiceWalBlobV1> {
    let pin = ActiveServiceWalBlobV1 {
        bytes: segment_count(&value.get("bytes"))?,
        sha256: segment_hash(&value.get("sha256"))?,
    };
    if pin.value() != *value {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(pin)
}
fn segment_source_head(value: &V) -> Result<()> {
    if *value == V::Null {
        return Ok(());
    }
    let count = segment_count(&value.get("actions"))?;
    let hash = segment_hash(&value.get("commitment"))?;
    if *value
        != obj(&[
            ("actions", s(count.to_string())),
            ("commitment", s(hex(&hash))),
        ])
    {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(())
}
fn segment_collect(
    value: &V,
    skip_previous_entry: bool,
    out: &mut Vec<ActiveServiceWalBlobV1>,
) -> Result<()> {
    match value {
        V::Object(fields) => {
            if fields.len() == 2
                && fields.iter().any(|(key, _)| key == "bytes")
                && fields.iter().any(|(key, _)| key == "sha256")
            {
                out.push(segment_blob(value)?);
                return Ok(());
            }
            for (key, value) in fields {
                if !(skip_previous_entry
                    && ["previousEntry", "historySegment"].contains(&key.as_str()))
                {
                    segment_collect(value, false, out)?;
                }
            }
        }
        V::Array(values) => {
            for value in values {
                segment_collect(value, false, out)?;
            }
        }
        _ => (),
    }
    Ok(())
}
impl ActiveServiceWalV1 {
    fn require_segment_scope(&self) -> Result<V> {
        let origin = self.origin_context()?.get("origin");
        if origin.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(origin)
    }
    fn segment_graph(
        &self,
        entries: &[ActiveServiceWalBlobV1],
    ) -> Result<Vec<ActiveServiceWalBlobV1>> {
        let mut pending = entries.to_vec();
        let mut opaque = vec![];
        for pin in entries {
            let entry = parse(&self.read_blob(pin)?)?;
            for field in ["action", "before", "accepted"] {
                opaque.push(segment_blob(&entry.get(field))?.sha256);
            }
        }
        let mut checked = Vec::<ActiveServiceWalBlobV1>::new();
        let mut charged = 0u64;
        while let Some(pin) = pending.pop() {
            if checked.iter().any(|known| known == &pin) {
                continue;
            }
            if checked.len() >= SEGMENT_GRAPH_LIMIT {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
            charged = charged
                .checked_add(pin.bytes)
                .ok_or(ActiveServiceWalErrorV1::Capacity)?;
            if charged > SEGMENT_GRAPH_BYTES {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
            let raw = self.read_blob(&pin)?;
            // Stream/typed wire blobs stay opaque. Diagnostic metadata chains
            // retain their nested blob roots; financial observations are not
            // used to reconstruct or execute anything here.
            if let Ok(value) = if opaque.contains(&pin.sha256) {
                Err(ActiveServiceWalErrorV1::InvalidRecord)
            } else {
                parse(&raw)
            } {
                segment_collect(
                    &value,
                    value.get("profile") == s(SEGMENTED_ACTIVE_HISTORY_PROFILE),
                    &mut pending,
                )?;
            }
            checked.push(pin);
        }
        Ok(checked)
    }
    pub fn prepare_acknowledged_history_checkpoint(
        &mut self,
        expected: &ActiveServiceWalPinV1,
        previous: Option<&ActiveHistorySegmentCursorV1>,
        entries: &[ActiveServiceWalBlobV1],
    ) -> Result<PreparedActiveHistoryCheckpointV1> {
        let origin = self.require_segment_scope()?;
        if self.pin()? != *expected {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        if entries.is_empty() || entries.len() > SEGMENT_WINDOW_LIMIT {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        let record = self.record()?;
        if record.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
            || record.get("stage") != s("Ack")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        if let Some(previous) = previous {
            let descriptor = parse(&self.read_blob(&previous.segment)?)?;
            let V::Array(archived) = descriptor.get("entries") else {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            };
            if descriptor.get("profile") != s(ACTIVE_HISTORY_SEGMENT_PROFILE)
                || segment_count(&descriptor.get("ordinal"))? != previous.ordinal
                || segment_count(&descriptor.get("lastAction"))? != previous.actions
                || segment_hash(&descriptor.get("walHead"))? != previous.wal_head
                || descriptor.get("sourceHead") != previous.source_head
                || archived.last() != Some(&previous.last_entry.value())
                || segment_count(&descriptor.get("walSequence"))?.checked_add(1)
                    != Some(previous.after.prefix.physical_sequence)
                || previous.after.root != self.root
                || previous.after.profile != PROFILE
            {
                return Err(ActiveServiceWalErrorV1::WrongPlan);
            }
            let snapshot = self
                .journal
                .as_mut()
                .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
                .read_verified_history_prefix(&previous.after.prefix)?;
            let actual = parse(
                snapshot
                    .get(RECORD)
                    .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?,
            )?;
            if actual.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
                || actual.get("stage") != s("SegmentAck")
                || actual.get("historySegment") != previous.segment.value()
                || actual.get("lastEntry") != previous.last_entry.value()
                || segment_count(&actual.get("sequence"))? != previous.actions
                || segment_hash(&actual.get("head"))? != previous.wal_head
                || actual.get("sourceHead") != previous.source_head
                || actual.get("actualAck") != descriptor.get("actualAck")
            {
                return Err(ActiveServiceWalErrorV1::WrongPlan);
            }
        }
        let from = previous.map_or(0, |p| p.actions);
        let count = from
            .checked_add(entries.len() as u64)
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        if segment_count(&record.get("sequence"))? != count {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let prior_segment = previous.map_or(V::Null, |p| p.segment.value());
        if !matches!(record.get("historySegment"), V::Null | V::Undefined)
            && record.get("historySegment") != prior_segment
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        if previous.is_some() && record.get("historySegment") != prior_segment {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let mut prior_entry = previous.map_or(V::Null, |p| p.last_entry.value());
        let mut head = previous.map_or_else(
            || sha256(&bytes(&origin).expect("validated actual origin")),
            |p| p.wal_head,
        );
        for (at, pin) in entries.iter().enumerate() {
            let entry = parse(&self.read_blob(pin)?)?;
            let sequence = from + at as u64 + 1;
            if entry.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
                || segment_count(&entry.get("sequence"))? != sequence
                || entry.get("previousEntry") != prior_entry
                || segment_hash(&entry.get("previousHead"))? != head
            {
                return Err(ActiveServiceWalErrorV1::WrongPlan);
            }
            let action = self.read_blob(&segment_blob(&entry.get("action"))?)?;
            crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(&action).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)?;
            self.read_blob(&segment_blob(&entry.get("before"))?)?;
            self.read_blob(&segment_blob(&entry.get("accepted"))?)?;
            segment_source_head(&entry.get("sourceHead"))?;
            let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
            chain.extend_from_slice(&head);
            chain.extend_from_slice(&sequence.to_be_bytes());
            chain.extend_from_slice(&pin.sha256);
            head = sha256(&chain);
            prior_entry = pin.value();
        }
        if segment_hash(&record.get("head"))? != head || record.get("entry") != prior_entry {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let last = parse(&self.read_blob(entries.last().expect("nonempty"))?)?;
        if record.get("action") != last.get("action")
            || record.get("before") != last.get("before")
            || record.get("accepted") != last.get("accepted")
            || record.get("sourceHead") != last.get("sourceHead")
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        segment_source_head(&record.get("sourceHead"))?;
        let from_wal = previous.map_or(0, |p| p.after.prefix.physical_sequence);
        let trace = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commit_window(
                &expected.prefix,
                from_wal,
                previous.map(|p| &p.trace),
                8192,
                64 * 1024 * 1024,
            )?
            .after()
            .clone();
        let mut dependencies = self.segment_graph(entries)?;
        if let Some(previous) = previous {
            self.read_blob(&previous.segment)?;
            dependencies.push(previous.segment.clone());
        }
        let ordinal = previous
            .map_or(Some(1), |p| p.ordinal.checked_add(1))
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        let descriptor = obj(&[
            ("profile", s(ACTIVE_HISTORY_SEGMENT_PROFILE)),
            ("ordinal", s(ordinal.to_string())),
            ("firstAction", s((from + 1).to_string())),
            ("lastAction", s(count.to_string())),
            ("previousSegment", prior_segment),
            ("sourceHead", record.get("sourceHead")),
            ("walHead", s(hex(&head))),
            (
                "entries",
                V::Array(entries.iter().map(ActiveServiceWalBlobV1::value).collect()),
            ),
            (
                "actualAck",
                ActiveServiceWalBlobV1::of(&bytes(&record)?)?.value(),
            ),
            (
                "walSequence",
                s(expected.prefix.physical_sequence.to_string()),
            ),
            (
                "trace",
                obj(&[
                    ("profile", s(&trace.profile)),
                    ("bytes", s(trace.bytes.to_string())),
                    ("commits", s(trace.commits.to_string())),
                    ("chain", s(hex(&trace.chain_sha256d))),
                ]),
            ),
        ]);
        let raw = bytes(&descriptor)?;
        let segment = ActiveServiceWalBlobV1::of(&raw)?;
        let ack_raw = bytes(&record)?;
        let ack_pin = ActiveServiceWalBlobV1::of(&ack_raw)?;
        let checkpoint = obj(&[
            ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
            ("stage", s("SegmentAck")),
            ("sequence", s(count.to_string())),
            ("head", s(hex(&head))),
            ("sourceHead", record.get("sourceHead")),
            ("lastEntry", prior_entry),
            ("historySegment", segment.value()),
            ("actualAck", ack_pin.value()),
        ]);
        let write = self.prepare(
            expected,
            checkpoint,
            vec![raw, ack_raw],
            vec![segment.clone(), ack_pin],
        )?;
        Ok(PreparedActiveHistoryCheckpointV1 {
            write,
            before_trace: trace,
            ordinal,
            actions: count,
            wal_head: head,
            source_head: record.get("sourceHead"),
            last_entry: entries.last().expect("nonempty").clone(),
            segment,
            dependencies,
            finished: false,
        })
    }
    fn verify_history_checkpoint_dependencies(
        &self,
        plan: &PreparedActiveHistoryCheckpointV1,
    ) -> Result<()> {
        for pin in &plan.dependencies {
            self.read_blob(pin)?;
        }
        Ok(())
    }
    pub fn attempt_acknowledged_history_checkpoint(
        &mut self,
        plan: &mut PreparedActiveHistoryCheckpointV1,
    ) -> Result<ActiveServiceWalPinV1> {
        if plan.finished {
            return Err(ActiveServiceWalErrorV1::AlreadyAttempted);
        }
        self.require_segment_scope()?;
        self.require_plan(&plan.write)?;
        self.verify_history_checkpoint_dependencies(plan)?;
        self.attempt(&mut plan.write)
    }
    pub fn resolve_acknowledged_history_checkpoint(
        &mut self,
        plan: &PreparedActiveHistoryCheckpointV1,
    ) -> Result<ActiveServiceWalResolvedV1> {
        if plan.finished {
            return Err(ActiveServiceWalErrorV1::AlreadyAttempted);
        }
        self.require_segment_scope()?;
        self.resolve(&plan.write)
    }
    pub fn rearm_acknowledged_history_checkpoint(
        &self,
        plan: &PreparedActiveHistoryCheckpointV1,
    ) -> Result<PreparedActiveHistoryCheckpointV1> {
        if plan.finished {
            return Err(ActiveServiceWalErrorV1::AlreadyAttempted);
        }
        let write = self.rearm_before(&plan.write)?;
        Ok(PreparedActiveHistoryCheckpointV1 {
            write,
            before_trace: plan.before_trace.clone(),
            ordinal: plan.ordinal,
            actions: plan.actions,
            wal_head: plan.wal_head,
            source_head: plan.source_head.clone(),
            last_entry: plan.last_entry.clone(),
            segment: plan.segment.clone(),
            dependencies: plan.dependencies.clone(),
            finished: false,
        })
    }
    pub fn finish_acknowledged_history_checkpoint(
        &mut self,
        plan: &mut PreparedActiveHistoryCheckpointV1,
    ) -> Result<SyncedActiveHistoryCheckpointV1> {
        if plan.finished || !plan.write.attempted() {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        self.require_plan(&plan.write)?;
        let post = self.pin()?;
        if &post != plan.post() {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        self.verify_history_checkpoint_dependencies(plan)?;
        let window = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commit_window(
                &post.prefix,
                plan.before_trace.commits,
                Some(&plan.before_trace),
                1,
                64 * 1024 * 1024,
            )?;
        if window.commits().len() != 1
            || window.commits()[0].mutations
                != vec![Mutation::Put {
                    key: RECORD.into(),
                    value: plan.write.record.clone(),
                }]
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        self.read_blob(&plan.segment)?;
        let trace = window.after().clone();
        plan.finished = true;
        Ok(SyncedActiveHistoryCheckpointV1 {
            owner: self.owner,
            cursor: ActiveHistorySegmentCursorV1 {
                ordinal: plan.ordinal,
                actions: plan.actions,
                wal_head: plan.wal_head,
                source_head: plan.source_head.clone(),
                last_entry: plan.last_entry.clone(),
                segment: plan.segment.clone(),
                after: post,
                trace,
            },
        })
    }
    pub fn require_synced_history_checkpoint(
        &self,
        proof: &SyncedActiveHistoryCheckpointV1,
    ) -> Result<()> {
        if proof.owner != self.owner || self.pin()? != proof.cursor.after {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
}
