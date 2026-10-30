// Administrative metadata fixtures below execute actual WAL/Journal/index IO.
// Opaque source observations are NOT asserted as genuine financial execution.
use super::*;
use crate::source_compact_index_v2::{
    NativePreparedInlineDatabaseV1, NativePreparedValidityCandidateV1 as Candidate,
};
use crate::source_main_native_session::NativeMainActiveServiceActionV1 as Action;
static SERIAL: AtomicU64 = AtomicU64::new(0);
static SYNCED_BARRIERS: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    index: PathBuf,
    wal: PathBuf,
    escrow: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_MODE4_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-mode4-cold-closure-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for name in ["index", "wal", "escrow"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        Self {
            index: root.join("index").canonicalize().unwrap(),
            wal: root.join("wal").canonicalize().unwrap(),
            escrow: root.join("escrow").canonicalize().unwrap(),
            root,
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let parent = self.root.parent().unwrap();
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(parent));
            let _ = fs::remove_dir_all(root);
        }
    }
}
fn wal_ns() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn meta_ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn metadata(temp: &Temp, compact: bool) -> SourceCompactIndex {
    let mut index =
        SourceCompactIndex::create(&temp.index, meta_ns(), "actual raw metadata Mode4 fixture")
            .unwrap();
    if compact {
        index.native_main_test_compaction_fixture().unwrap();
    }
    index
}
fn arguments() -> Vec<V> {
    vec![
        obj(&[("_id", s("MaxHeight"))]),
        obj(&[("$set", obj(&[("value", V::number(11.))]))]),
        obj(&[("upsert", V::Bool(true))]),
    ]
}
struct Fixture {
    store: ActiveServiceWalV1,
    head: [u8; 32],
    last: V,
    origin: V,
    archive: Vec<u8>,
    history: Vec<u8>,
    observation: Vec<u8>,
}
fn fixture(temp: &Temp, index: &SourceCompactIndex) -> Fixture {
    let archive = b"opaque source archive; administrative metadata fixture only".to_vec();
    let observation = b"opaque raw constructor observation".to_vec();
    let history = index
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let origin = obj(&[
        ("profile", s(MODE4_INLINE_WAL_PROFILE)),
        (
            "archive",
            ActiveServiceWalBlobV1::of(&archive).unwrap().value(),
        ),
        (
            "observation",
            ActiveServiceWalBlobV1::of(&observation).unwrap().value(),
        ),
        (
            "requiredVerifiedHistory",
            ActiveServiceWalBlobV1::of(&history).unwrap().value(),
        ),
        (
            "generationEscrow",
            obj(&[
                ("profile", s("TL-COMPACT-GENERATION-ESCROW-13-1")),
                ("root", s(temp.escrow.to_string_lossy())),
                ("durability", s("FileSyncOnly")),
            ]),
        ),
        (
            "requiredInlineServiceProfile",
            s(MODE4_INLINE_SERVICE_PROFILE),
        ),
    ]);
    let store = ActiveServiceWalV1::create(
        &temp.wal,
        wal_ns(),
        origin.clone(),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let head = sha256(&bytes(&origin).unwrap());
    Fixture {
        store,
        head,
        last: V::Null,
        origin,
        archive,
        history,
        observation,
    }
}
struct Pending {
    sequence: u64,
    action: Vec<u8>,
    before: Vec<u8>,
    history_before: Vec<u8>,
    candidates: Option<Vec<u8>>,
    mutations: Option<Vec<u8>>,
    recovery: Option<Vec<u8>>,
    escrow: Option<Vec<u8>>,
}
fn pin(raw: &[u8]) -> V {
    ActiveServiceWalBlobV1::of(raw).unwrap().value()
}
fn optional(raw: &Option<Vec<u8>>) -> V {
    raw.as_ref().map_or(V::Null, |v| pin(v))
}
impl Fixture {
    fn begin(
        &mut self,
        sequence: u64,
        action: Action,
        index: &SourceCompactIndex,
        plan: Option<&NativePreparedInlineDatabaseV1>,
    ) -> Pending {
        let pending = self.pending(sequence, action, index, plan);
        self.barrier(&pending, "Intent", None, None, None);
        pending
    }
    fn pending(
        &self,
        sequence: u64,
        action: Action,
        index: &SourceCompactIndex,
        plan: Option<&NativePreparedInlineDatabaseV1>,
    ) -> Pending {
        let (history_before, candidates, mutations) = {
            let history = index
                .capture_verified_history_pin()
                .unwrap()
                .canonical_scalar_bytes()
                .unwrap();
            let native=plan.and_then(|p|p.append_plan().map(|a|(a,p.durable_read_context()))).map(|(a,c)|crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(a,c).unwrap());
            (
                history,
                native
                    .as_ref()
                    .map(|v| bytes(v.independently_retained_facts()).unwrap()),
                native
                    .as_ref()
                    .map(|v| v.exact_prepared_mutations().to_vec()),
            )
        };
        Pending {
            sequence,
            action: action.canonical_action_bytes().unwrap(),
            before: self.observation.clone(),
            history_before,
            candidates,
            mutations,
            recovery: None,
            escrow: None,
        }
    }
    fn barrier(
        &mut self,
        p: &Pending,
        stage: &str,
        accepted: Option<&[u8]>,
        entry: Option<&[u8]>,
        history_post: Option<&[u8]>,
    ) {
        let mut record = obj(&[
            ("profile", s(MODE4_INLINE_WAL_PROFILE)),
            ("stage", s(stage)),
            ("sequence", s(p.sequence.to_string())),
            ("previousHead", s(hex(&self.head))),
            (
                "head",
                if accepted.is_some() {
                    s(hex(&self.next_head(p.sequence, entry.expect("final entry"))))
                } else {
                    V::Null
                },
            ),
            ("action", pin(&p.action)),
            ("before", pin(&p.before)),
            ("origin", self.origin.get("archive")),
            ("nativeCandidates", optional(&p.candidates)),
            ("nativeMutations", optional(&p.mutations)),
            ("accepted", accepted.map_or(V::Null, pin)),
            ("entry", entry.map_or(V::Null, pin)),
        ]);
        if let Some(raw) = &p.recovery {
            record.insert("nativeRecovery", pin(raw)).unwrap();
        }
        record
            .insert("verifiedHistoryBefore", pin(&p.history_before))
            .unwrap();
        if let Some(raw) = history_post {
            record.insert("verifiedHistoryPost", pin(raw)).unwrap();
        }
        if let Some(raw) = &p.escrow {
            record.insert("generationEscrow", pin(raw)).unwrap();
        }
        let mut blobs = vec![
            self.archive.clone(),
            self.history.clone(),
            p.action.clone(),
            p.before.clone(),
            p.history_before.clone(),
        ];
        for raw in [&p.candidates, &p.mutations, &p.recovery, &p.escrow]
            .into_iter()
            .flatten()
        {
            blobs.push(raw.clone());
        }
        for raw in [accepted, entry, history_post].into_iter().flatten() {
            blobs.push(raw.to_vec());
        }
        let refs = blobs
            .iter()
            .map(|v| ActiveServiceWalBlobV1::of(v).unwrap())
            .collect();
        let mut prepared = self
            .store
            .prepare(&self.store.pin().unwrap(), record, blobs, refs)
            .unwrap();
        self.store.attempt(&mut prepared).unwrap();
    }
    fn next_head(&self, sequence: u64, entry: &[u8]) -> [u8; 32] {
        let mut raw = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
        raw.extend_from_slice(&self.head);
        raw.extend_from_slice(&sequence.to_be_bytes());
        raw.extend_from_slice(&ActiveServiceWalBlobV1::of(entry).unwrap().sha256);
        sha256(&raw)
    }
    fn finish(&mut self, p: Pending, index: &SourceCompactIndex) {
        self.finish_inner(p, index, false)
    }
    fn finish_inner(&mut self, p: Pending, index: &SourceCompactIndex, reordered: bool) {
        let observation =
            format!("opaque actual metadata fixture observation {}", p.sequence).into_bytes();
        self.finish_observation(p, index, reordered, observation, V::Null);
    }
    fn finish_observation(
        &mut self,
        p: Pending,
        index: &SourceCompactIndex,
        reordered: bool,
        observation: Vec<u8>,
        rejection: V,
    ) {
        let history = index
            .capture_verified_history_pin()
            .unwrap()
            .canonical_scalar_bytes()
            .unwrap();
        let mut diagnostic = obj(&[("apiRejection", rejection)]);
        if let Some(raw) = &p.recovery {
            diagnostic.insert("nativeRecovery", pin(raw)).unwrap();
        }
        if let Some(raw) = &p.escrow {
            diagnostic.insert("generationEscrow", pin(raw)).unwrap();
        }
        let suffix = bytes(&diagnostic).unwrap();
        let mut accepted = b"TLMAP\x01".to_vec();
        accepted.extend_from_slice(&(observation.len() as u64).to_be_bytes());
        accepted.extend_from_slice(&observation);
        accepted.extend_from_slice(&(suffix.len() as u64).to_be_bytes());
        accepted.extend_from_slice(&suffix);
        let mut value = obj(&[
            ("profile", s(MODE4_INLINE_WAL_PROFILE)),
            ("sequence", s(p.sequence.to_string())),
            ("previousEntry", self.last.clone()),
            ("previousHead", s(hex(&self.head))),
            ("action", pin(&p.action)),
            ("before", pin(&p.before)),
            ("nativeCandidates", optional(&p.candidates)),
            ("nativeMutations", optional(&p.mutations)),
            ("accepted", pin(&accepted)),
        ]);
        if let Some(raw) = &p.recovery {
            value.insert("nativeRecovery", pin(raw)).unwrap();
        }
        value
            .insert("verifiedHistoryBefore", pin(&p.history_before))
            .unwrap();
        value.insert("verifiedHistoryPost", pin(&history)).unwrap();
        if let Some(raw) = &p.escrow {
            value.insert("generationEscrow", pin(raw)).unwrap();
        }
        let entry = bytes(&value).unwrap();
        self.barrier(
            &p,
            "Accepted",
            Some(&accepted),
            Some(&entry),
            Some(&history),
        );
        if reordered {
            self.barrier(&p, "Intent", None, None, None);
        }
        self.barrier(&p, "Ack", Some(&accepted), Some(&entry), Some(&history));
        self.head = self.next_head(p.sequence, &entry);
        self.last = pin(&entry);
        self.observation = observation;
    }
    fn start(&mut self, index: &SourceCompactIndex) {
        let p = self.begin(
            1,
            Action::Start {
                start: V::number(10.),
                generation: 1,
                documents: V::Array(vec![]),
            },
            index,
            None,
        );
        self.finish(p, index);
    }
}
#[test]
fn mode4_actual_barriers_and_source_computed_inline_tlnm_close_without_live_permit() {
    let temp = Temp::new();
    let mut index = metadata(&temp, false);
    let mut f = fixture(&temp, &index);
    f.store.set_test_commit_probe(|plan, after| {
        let record = plan.record().unwrap();
        assert!(matches!(
            history_text(&record.get("stage")).unwrap(),
            "Intent" | "Accepted" | "Ack"
        ));
        if after {
            SYNCED_BARRIERS.fetch_add(1, Ordering::Relaxed);
        }
    });
    let initial_barriers = SYNCED_BARRIERS.load(Ordering::Relaxed);
    f.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let mut source = view
        .prepare_historical_inline_database("updateAsync", arguments())
        .unwrap();
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments())
        .unwrap();
    let p = f.begin(2, Action::AppendPending, &index, Some(&live));
    index.append_inline_database(&live, None).unwrap();
    f.finish(p, &index);
    let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    assert_eq!(
        SYNCED_BARRIERS.load(Ordering::Relaxed) - initial_barriers,
        6
    );
    let path = f.store.journal_path();
    drop(f);
    let original = fs::read(&path).unwrap();
    assert!(VerifiedActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected.wal).is_err());
    let reader =
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
    assert_eq!(reader.entries().len(), 2);
    let closed = reader
        .verify_inline_operation(&mut index, 2, &mut source)
        .unwrap();
    assert_eq!(closed.before(), &before);
    assert_eq!(closed.sequence(), 2);
    assert_eq!(closed.recovery_links(), 0);
    assert_eq!(closed.escrow_links(), 0);
    assert!(source.append_verified());
    assert!(!source.resolved());
    assert_eq!(
        source.resolve_recorded_post().unwrap(),
        crate::validity_source::DependencyReturn::Pending(V::number(1.))
    );
    drop(reader);
    assert_eq!(fs::read(&path).unwrap(), original);
}
fn copy_before_attempt(
    temp: &Temp,
    f: &mut Fixture,
    p: &mut Pending,
    index: &mut SourceCompactIndex,
    plan: &NativePreparedInlineDatabaseV1,
) -> Option<crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1> {
    plan.append_plan().unwrap().intermediate()?;
    let point = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&point),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let scalar = permit
        .independently_retained_pin()
        .canonical_scalar_bytes()
        .unwrap();
    p.escrow = Some(
        bytes(&obj(&[
            ("profile", s("TL-ACTIVE-GENERATION-ESCROW-CHAIN-13-1")),
            (
                "previous",
                p.escrow.as_ref().map_or(V::Null, |raw| pin(raw)),
            ),
            ("nativeReadContext", plan.durable_read_context()),
            ("expectedEscrowFieldsHex", s(hex(&scalar))),
        ]))
        .unwrap(),
    );
    f.barrier(p, "EscrowBoundIntent", None, None, None);
    Some(permit)
}
fn recovered_case(
    temp: &Temp,
    candidate: Candidate,
    compact: bool,
    forge_mutation_hash: bool,
) -> (
    Fixture,
    SourceCompactIndex,
    NativeHistoricalInlineDatabaseV1,
) {
    let mut index = metadata(temp, compact);
    let mut f = fixture(temp, &index);
    f.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let source = view
        .prepare_historical_inline_database("updateAsync", arguments())
        .unwrap();
    drop(view);
    let original = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments())
        .unwrap();
    if compact {
        assert!(original.append_plan().unwrap().intermediate().is_some());
    }
    let mut p = f.begin(2, Action::AppendPending, &index, Some(&original));
    let permit = copy_before_attempt(temp, &mut f, &mut p, &mut index, &original);
    match candidate {
        Candidate::Before => index.test_fail_after_partial_write(),
        Candidate::Post => index.test_fail_after_sync(),
        Candidate::Intermediate => index.native_main_test_compaction_fault("after-current-sync"),
    }
    assert!(index
        .append_inline_database(&original, permit.as_ref())
        .is_err());
    drop(index);
    let recovery = if candidate == Candidate::Before {
        Recovery::DiscardIncompleteTail
    } else {
        Recovery::RejectIncompleteTail
    };
    let (mut index, observed) = SourceCompactIndex::reopen_inline_database(
        &temp.index,
        meta_ns(),
        &original,
        candidate,
        recovery,
    )
    .unwrap();
    let reopened = index.capture_verified_history_pin().unwrap();
    let mut proof = obj(&[
        ("profile", s("TL-MAIN-ACTIVE-INLINE-NATIVE-RECOVERY-13-1")),
        (
            "candidate",
            s(match candidate {
                Candidate::Before => "Before",
                Candidate::Intermediate => "Intermediate",
                Candidate::Post => "Post",
            }),
        ),
        ("snapshot", mode4_physical(&reopened)),
        (
            "discardedBytes",
            s(observed.recovery_report().discarded_bytes.to_string()),
        ),
        ("previousRecovery", V::Null),
        (
            "verifiedReopenedHistory",
            s(hex(&reopened.canonical_scalar_bytes().unwrap())),
        ),
    ]);
    if candidate == Candidate::Post {
        index.resolve_inline_database_post(&original).unwrap();
        p.recovery = Some(bytes(&proof).unwrap());
    } else {
        let (replacement, mapping) = index
            .supersede_inline_database(&original, &observed)
            .unwrap();
        let binding = index
            .require_inline_native_supersession(&original, &observed, &replacement, &mapping)
            .unwrap();
        let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(replacement.append_plan().unwrap(),replacement.durable_read_context()).unwrap();
        proof.insert("binding", binding).unwrap();
        proof
            .insert(
                "replacementCandidates",
                candidates.independently_retained_facts().clone(),
            )
            .unwrap();
        let mut mutation_hash = sha256(candidates.exact_prepared_mutations());
        if forge_mutation_hash {
            mutation_hash[0] ^= 1;
        }
        proof
            .insert("replacementMutationsSha256", s(hex(&mutation_hash)))
            .unwrap();
        p.recovery = Some(bytes(&proof).unwrap());
        f.barrier(&p, "NativeSupersessionIntent", None, None, None);
        let replacement_permit =
            copy_before_attempt(temp, &mut f, &mut p, &mut index, &replacement);
        index
            .append_inline_database(&replacement, replacement_permit.as_ref())
            .unwrap();
        index.resolve_inline_database_post(&replacement).unwrap();
    }
    f.finish(p, &index);
    (f, index, source)
}
#[test]
fn mode4_native_before_and_lost_post_recoveries_bind_actual_files_and_original_mutations() {
    for candidate in [Candidate::Before, Candidate::Post] {
        let temp = Temp::new();
        let (mut f, mut index, mut source) = recovered_case(&temp, candidate, false, false);
        let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
        drop(f);
        let reader =
            VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
        let before = index.capture_verified_history_pin().unwrap();
        let closure = reader
            .verify_inline_operation(&mut index, 2, &mut source)
            .unwrap();
        assert_eq!(closure.recovery_links(), 1);
        assert_eq!(closure.escrow_links(), 0);
        assert!(source.append_verified());
        assert!(!source.resolved());
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    }
}
#[test]
fn mode4_intermediate_requires_actual_synced_copy_and_preserves_same_recomputed_plan_on_failure() {
    let temp = Temp::new();
    let (mut f, mut index, mut source) =
        recovered_case(&temp, Candidate::Intermediate, true, false);
    let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    drop(f);
    let reader =
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
    let escrows = reader.mode4_escrows(&reader.entries()[1]).unwrap();
    assert_eq!(escrows.len(), 1);
    let manifest = escrows[0].pin.storage.copied_generation.join("GENINFO");
    let original = fs::read(&manifest).unwrap();
    let mut forged = original.clone();
    forged[0] ^= 1;
    fs::write(&manifest, &forged).unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    assert!(reader
        .verify_inline_operation(&mut index, 2, &mut source)
        .is_err());
    assert!(!source.append_verified());
    assert!(!source.resolved());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    fs::write(&manifest, original).unwrap();
    let closure = reader
        .verify_inline_operation(&mut index, 2, &mut source)
        .unwrap();
    assert_eq!(closure.recovery_links(), 1);
    assert_eq!(closure.escrow_links(), 1);
    assert!(source.append_verified());
    assert!(!source.resolved());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn mode4_rehashed_false_replacement_mutation_claim_does_not_become_source_answer() {
    let temp = Temp::new();
    let (mut f, mut index, mut source) = recovered_case(&temp, Candidate::Before, false, true);
    let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    drop(f);
    let reader =
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    assert!(reader
        .verify_inline_operation(&mut index, 2, &mut source)
        .is_err());
    assert!(!source.append_verified());
    assert!(!source.resolved());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn mode4_independently_pinned_ack_without_ordered_intent_barriers_is_rejected() {
    for reordered in [false, true] {
        let temp = Temp::new();
        let index = metadata(&temp, false);
        let mut f = fixture(&temp, &index);
        f.start(&index);
        let pending = f.pending(2, Action::Pause, &index, None);
        f.finish_inner(pending, &index, reordered);
        let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
        let path = f.store.journal_path();
        drop(f);
        let raw = fs::read(&path).unwrap();
        assert!(
            VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), raw);
    }
}
fn rechain_wal(raw: &mut [u8]) {
    let mut at = raw.windows(4).position(|v| v == b"TXN1").unwrap();
    let mut previous = crate::codec::crc32(&raw[..at]);
    while at < raw.len() {
        assert_eq!(&raw[at..at + 4], b"TXN1");
        let payload = u32::from_be_bytes(raw[at + 4..at + 8].try_into().unwrap()) as usize;
        raw[at + 16..at + 20].copy_from_slice(&previous.to_be_bytes());
        let header = crate::codec::crc32(&raw[at..at + 20]);
        raw[at + 20..at + 24].copy_from_slice(&header.to_be_bytes());
        previous = crate::codec::crc32(&raw[at..at + 24 + payload]);
        raw[at + 24 + payload..at + 28 + payload].copy_from_slice(&previous.to_be_bytes());
        at += payload + 32;
    }
}
#[test]
fn mode4_crc_valid_past_rewrite_with_same_ack_and_kv_cannot_select_a_new_trace() {
    let temp = Temp::new();
    let index = metadata(&temp, false);
    let mut f = fixture(&temp, &index);
    f.start(&index);
    let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    let path = f.store.journal_path();
    drop(f);
    let mut raw = fs::read(&path).unwrap();
    let at = raw.windows(6).position(|v| v == b"Intent").unwrap();
    raw[at..at + 6].copy_from_slice(b"Forged");
    rechain_wal(&mut raw);
    fs::write(&path, &raw).unwrap();
    assert!(VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).is_err());
    let (mut actual, _) = Journal::open_existing_verified_prefix(
        &temp.wal,
        wal_ns(),
        Recovery::RejectIncompleteTail,
        &expected.wal.prefix,
    )
    .unwrap();
    let trace = actual
        .read_verified_current_commits(
            &expected.wal.prefix,
            65536,
            MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
        )
        .unwrap()
        .trace()
        .clone();
    drop(actual);
    assert_ne!(trace, expected.trace);
    let mut rebound = expected;
    rebound.trace = trace;
    assert!(
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &rebound).is_err(),
        "a supplied new trace cannot substitute for a valid intent/ACK graph"
    );
    assert_eq!(fs::read(&path).unwrap(), raw);
}

#[test]
fn mode4_rejected_reappend_requires_prior_post_exact_plan_and_unchanged_observation() {
    for fault in [
        "valid",
        "no-prior",
        "mutations",
        "configuration",
        "observation",
        "rejection",
        "resolved",
    ] {
        let temp = Temp::new();
        let mut index = metadata(&temp, false);
        let mut f = fixture(&temp, &index);
        f.start(&index);
        let before = index.capture_verified_history_pin().unwrap();
        let live = index
            .prepare_inline_database(&before.metadata, "updateAsync", arguments())
            .unwrap();
        let mut sequence = 2;
        if fault != "no-prior" {
            let p = f.begin(sequence, Action::AppendPending, &index, Some(&live));
            index.append_inline_database(&live, None).unwrap();
            f.finish(p, &index);
            sequence += 1;
        }
        if fault == "resolved" {
            let p = f.begin(sequence, Action::ResolvePost, &index, None);
            f.finish(p, &index);
            sequence += 1;
        }
        let mut p = f.pending(sequence, Action::AppendPending, &index, Some(&live));
        if fault == "mutations" {
            let mut args = arguments();
            args[1] = obj(&[("$set", obj(&[("value", V::number(12.))]))]);
            let other = index
                .prepare_inline_database(&index.snapshot().unwrap(), "updateAsync", args)
                .unwrap();
            let native = crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(
                other.append_plan().unwrap(), other.durable_read_context()).unwrap();
            assert_ne!(
                p.mutations.as_deref().unwrap(),
                native.exact_prepared_mutations()
            );
            p.mutations = Some(native.exact_prepared_mutations().to_vec());
        }
        if fault == "configuration" {
            let mut value = parse(p.candidates.as_ref().unwrap()).unwrap();
            let target = history_count(&value.get("targetLogBytes")).unwrap();
            value
                .insert(
                    "targetLogBytes",
                    s(if target == 65536 { "131072" } else { "65536" }),
                )
                .unwrap();
            p.candidates = Some(bytes(&value).unwrap());
        }
        let mut observation = p.before.clone();
        if fault == "observation" {
            observation.extend_from_slice(b" changed source fields");
        }
        let rejection = s(if fault == "rejection" {
            "NoAttemptedWrite"
        } else {
            "WriteAlreadyAttempted"
        });
        f.barrier(&p, "Intent", None, None, None);
        f.finish_observation(p, &index, false, observation, rejection);
        let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
        let path = f.store.journal_path();
        let current = index.capture_verified_history_pin().unwrap();
        drop(f);
        let original = fs::read(&path).unwrap();
        let result = VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected);
        assert_eq!(result.is_ok(), fault == "valid", "{fault}");
        drop(result);
        assert_eq!(fs::read(path).unwrap(), original, "{fault}");
        assert_eq!(
            index.capture_verified_history_pin().unwrap(),
            current,
            "{fault}"
        );
    }
}

#[test]
fn mode4_rejected_noop_update_never_supplies_a_historical_source_return() {
    let temp = Temp::new();
    let mut index = metadata(&temp, false);
    let mut f = fixture(&temp, &index);
    f.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let args = vec![
        obj(&[("_id", s("absent-noop-update"))]),
        obj(&[("$set", obj(&[("value", V::number(11.))]))]),
        obj(&[("upsert", V::Bool(false))]),
    ];
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let mut successful = view
        .prepare_historical_inline_database("updateAsync", args.clone())
        .unwrap();
    let mut rejected = view
        .prepare_historical_inline_database("updateAsync", args.clone())
        .unwrap();
    assert!(!successful.requires_native_write());
    drop(view);
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", args)
        .unwrap();
    assert!(live.append_plan().is_none());
    let p = f.begin(2, Action::AppendPending, &index, Some(&live));
    index.append_inline_database(&live, None).unwrap();
    f.finish(p, &index);
    let p = f.begin(3, Action::AppendPending, &index, Some(&live));
    let observation = p.before.clone();
    f.finish_observation(p, &index, false, observation, s("WriteAlreadyAttempted"));
    let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    drop(f);
    let reader =
        VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).unwrap();
    reader
        .verify_inline_operation(&mut index, 2, &mut successful)
        .unwrap();
    // An unmatched update has no append to verify. Its computed no-write
    // outcome remains separately resolvable from the original Prepared phase.
    assert!(!successful.append_verified());
    assert!(!successful.resolved());
    assert!(reader
        .verify_inline_operation(&mut index, 3, &mut rejected)
        .is_err());
    assert!(!rejected.append_verified());
    assert!(!rejected.resolved());
    assert_eq!(
        successful.resolve_recorded_post().unwrap(),
        crate::validity_source::DependencyReturn::Pending(V::number(0.))
    );
    assert!(successful.resolve_recorded_post().is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}

#[test]
fn mode4_rejected_append_cannot_hide_a_real_extra_native_publication() {
    let temp = Temp::new();
    let mut index = metadata(&temp, false);
    let mut f = fixture(&temp, &index);
    f.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments())
        .unwrap();
    let p = f.begin(2, Action::AppendPending, &index, Some(&live));
    index.append_inline_database(&live, None).unwrap();
    f.finish(p, &index);
    let p = f.begin(3, Action::AppendPending, &index, Some(&live));
    let mut args = arguments();
    args[1] = obj(&[("$set", obj(&[("value", V::number(12.))]))]);
    let other = index
        .prepare_inline_database(&index.snapshot().unwrap(), "updateAsync", args)
        .unwrap();
    index.append_inline_database(&other, None).unwrap();
    let observation = p.before.clone();
    f.finish_observation(p, &index, false, observation, s("WriteAlreadyAttempted"));
    let expected = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    let actual = index.capture_verified_history_pin().unwrap();
    drop(f);
    assert!(VerifiedInlineActiveServiceAckHistoryV1::open(&temp.wal, wal_ns(), &expected).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), actual);
}

#[path = "source_main_mode4_independent_review_tests.rs"]
mod independent_review;

#[path = "source_main_mode4_pending_facts_tests.rs"]
mod pending_facts_tests;
