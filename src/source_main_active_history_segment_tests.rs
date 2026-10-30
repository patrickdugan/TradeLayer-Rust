// Real Journal/blob/barrier controls only. The comparison-only byte fixtures
// below are NOT financial outputs, actor state or executed-source claims.
use super::*;
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("ACTIVE_HISTORY_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "active-ack-segment-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(self.parent.as_path()));
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn store(t: &Temp) -> ActiveServiceWalV1 {
    ActiveServiceWalV1::create(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[
            ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
            ("scope", s("metadata controls only; no financial owner")),
        ]),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn append_ack(
    store: &mut ActiveServiceWalV1,
    previous: Option<&ActiveHistorySegmentCursorV1>,
    prior: &mut Vec<ActiveServiceWalBlobV1>,
) -> ActiveServiceWalBlobV1 {
    let sequence = previous.map_or(0, |p| p.actions) + prior.len() as u64 + 1;
    let last = prior
        .last()
        .cloned()
        .or_else(|| previous.map(|p| p.last_entry.clone()));
    let mut head = if let Some(last) = last.as_ref() {
        let entry = parse(&store.read_blob(last).unwrap()).unwrap();
        let mut bytes = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
        bytes.extend_from_slice(&segment_hash(&entry.get("previousHead")).unwrap());
        bytes.extend_from_slice(&segment_count(&entry.get("sequence")).unwrap().to_be_bytes());
        bytes.extend_from_slice(&last.sha256);
        sha256(&bytes)
    } else {
        sha256(&bytes(&store.origin_context().unwrap().get("origin")).unwrap())
    };
    let action = crate::source_main_native_session::NativeMainActiveServiceActionV1::Pause
        .canonical_action_bytes()
        .unwrap();
    let before = b"opaque comparison-only BEFORE metadata".to_vec();
    let accepted = b"opaque comparison-only ACCEPTED metadata".to_vec();
    let action_pin = ActiveServiceWalBlobV1::of(&action).unwrap();
    let before_pin = ActiveServiceWalBlobV1::of(&before).unwrap();
    let accepted_pin = ActiveServiceWalBlobV1::of(&accepted).unwrap();
    let source_head = obj(&[
        ("actions", s(sequence.to_string())),
        ("commitment", s(hex(&sha256(&sequence.to_be_bytes())))),
    ]);
    let entry = bytes(&obj(&[
        ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
        ("sequence", s(sequence.to_string())),
        (
            "previousEntry",
            last.as_ref().map_or(V::Null, ActiveServiceWalBlobV1::value),
        ),
        ("previousHead", s(hex(&head))),
        ("action", action_pin.value()),
        ("before", before_pin.value()),
        ("accepted", accepted_pin.value()),
        ("sourceHead", source_head.clone()),
    ]))
    .unwrap();
    let entry_pin = ActiveServiceWalBlobV1::of(&entry).unwrap();
    let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
    chain.extend_from_slice(&head);
    chain.extend_from_slice(&sequence.to_be_bytes());
    chain.extend_from_slice(&entry_pin.sha256);
    head = sha256(&chain);
    let record = obj(&[
        ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
        ("stage", s("Ack")),
        ("sequence", s(sequence.to_string())),
        ("head", s(hex(&head))),
        ("action", action_pin.value()),
        ("before", before_pin.value()),
        ("accepted", accepted_pin.value()),
        ("sourceHead", source_head),
        ("entry", entry_pin.value()),
        (
            "historySegment",
            previous.map_or(V::Null, |p| p.segment.value()),
        ),
    ]);
    let mut plan = store
        .prepare(
            &store.pin().unwrap(),
            record,
            vec![action, before, accepted, entry],
            vec![action_pin, before_pin, accepted_pin, entry_pin.clone()],
        )
        .unwrap();
    store.attempt(&mut plan).unwrap();
    prior.push(entry_pin.clone());
    entry_pin
}
#[test]
fn actual_ack_checkpoint_keeps_absolute_counts_heads_and_complete_entry_blobs_across_segments() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut first = vec![];
    for _ in 0..3 {
        append_ack(&mut store, None, &mut first);
    }
    let pin = store.pin().unwrap();
    let head = store.record().unwrap().get("head");
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&pin, None, &first)
        .unwrap();
    assert_eq!(store.pin().unwrap(), pin);
    let post = store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    let proof = store
        .finish_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    store.require_synced_history_checkpoint(&proof).unwrap();
    assert_eq!(proof.cursor().actions, 3);
    assert_eq!(s(hex(&proof.cursor().wal_head)), head);
    assert_eq!(proof.cursor().after, post);
    for entry in &first {
        store.read_blob(entry).unwrap();
    }
    assert!(store
        .finish_acknowledged_history_checkpoint(&mut plan)
        .is_err());
    let cursor = proof.cursor().clone();
    let mut second = vec![];
    for _ in 0..2 {
        append_ack(&mut store, Some(&cursor), &mut second);
    }
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&store.pin().unwrap(), Some(&cursor), &second)
        .unwrap();
    store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    let proof2 = store
        .finish_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    assert_eq!(proof2.cursor().actions, 5);
    assert_eq!(proof2.cursor().ordinal, 2);
    assert!(proof2.cursor().after.prefix.physical_sequence > cursor.after.prefix.physical_sequence);
    for entry in first.iter().chain(&second) {
        store.read_blob(entry).unwrap();
    }
}
#[test]
fn before_control_failure_rearms_only_same_checkpoint_and_never_loses_ack_window() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut entries = vec![];
    append_ack(&mut store, None, &mut entries);
    let before = store.pin().unwrap();
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&before, None, &entries)
        .unwrap();
    store.set_test_fault(TestFault::BeforeCommit);
    assert!(store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .is_err());
    assert_eq!(
        store
            .resolve_acknowledged_history_checkpoint(&plan)
            .unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(store.pin().unwrap(), before);
    let mut rearmed = store.rearm_acknowledged_history_checkpoint(&plan).unwrap();
    store
        .attempt_acknowledged_history_checkpoint(&mut rearmed)
        .unwrap();
    assert_eq!(
        store
            .finish_acknowledged_history_checkpoint(&mut rearmed)
            .unwrap()
            .cursor()
            .actions,
        1
    );
}
#[test]
fn actual_partial_control_frame_resolves_before_and_exact_metadata_retry_completes() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut entries = vec![];
    append_ack(&mut store, None, &mut entries);
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&store.pin().unwrap(), None, &entries)
        .unwrap();
    store
        .journal
        .as_mut()
        .unwrap()
        .test_fail_after_partial_write();
    assert!(store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .is_err());
    assert_eq!(
        store
            .resolve_acknowledged_history_checkpoint(&plan)
            .unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    let mut rearmed = store.rearm_acknowledged_history_checkpoint(&plan).unwrap();
    store
        .attempt_acknowledged_history_checkpoint(&mut rearmed)
        .unwrap();
    store
        .finish_acknowledged_history_checkpoint(&mut rearmed)
        .unwrap();
}
static ATTEMPTS: AtomicU64 = AtomicU64::new(0);
fn probe(_: &PreparedActiveServiceWalWriteV1, completed: bool) {
    if !completed {
        ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    }
}
#[test]
fn real_synced_lost_return_resolves_post_without_second_append() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut entries = vec![];
    append_ack(&mut store, None, &mut entries);
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&store.pin().unwrap(), None, &entries)
        .unwrap();
    ATTEMPTS.store(0, Ordering::Relaxed);
    store.set_test_commit_probe(probe);
    store.journal.as_mut().unwrap().test_fail_after_sync();
    assert!(store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .is_err());
    assert_eq!(ATTEMPTS.load(Ordering::Relaxed), 1);
    assert_eq!(
        store
            .resolve_acknowledged_history_checkpoint(&plan)
            .unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    let proof = store
        .finish_acknowledged_history_checkpoint(&mut plan)
        .unwrap();
    assert_eq!(ATTEMPTS.load(Ordering::Relaxed), 1);
    assert_eq!(
        proof.cursor().after.prefix.physical_sequence,
        plan.before().prefix.physical_sequence + 1
    );
    assert!(store.rearm_acknowledged_history_checkpoint(&plan).is_err());
}
#[test]
fn foreign_store_wrong_phase_or_reordered_window_refuse_without_state_change() {
    let ta = Temp::new();
    let tb = Temp::new();
    let mut a = store(&ta);
    let mut b = store(&tb);
    let mut entries = vec![];
    append_ack(&mut a, None, &mut entries);
    append_ack(&mut a, None, &mut entries);
    let before = a.pin().unwrap();
    let mut reverse = entries.clone();
    reverse.reverse();
    assert!(a
        .prepare_acknowledged_history_checkpoint(&before, None, &reverse)
        .is_err());
    assert_eq!(a.pin().unwrap(), before);
    let mut plan = a
        .prepare_acknowledged_history_checkpoint(&before, None, &entries)
        .unwrap();
    let foreign = b.pin().unwrap();
    assert!(matches!(
        b.attempt_acknowledged_history_checkpoint(&mut plan),
        Err(ActiveServiceWalErrorV1::WrongPlan)
    ));
    assert_eq!(b.pin().unwrap(), foreign);
    let mut wrong = a
        .prepare(
            &before,
            obj(&[
                ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
                ("stage", s("Intent")),
            ]),
            vec![],
            vec![],
        )
        .unwrap();
    a.attempt(&mut wrong).unwrap();
    assert!(a
        .prepare_acknowledged_history_checkpoint(&a.pin().unwrap(), None, &entries)
        .is_err());
}
#[test]
fn missing_archived_dependency_cannot_be_blessed_by_checkpoint() {
    let t = Temp::new();
    let mut store = store(&t);
    let mut entries = vec![];
    append_ack(&mut store, None, &mut entries);
    let mut plan = store
        .prepare_acknowledged_history_checkpoint(&store.pin().unwrap(), None, &entries)
        .unwrap();
    let entry = parse(&store.read_blob(&entries[0]).unwrap()).unwrap();
    let pin = segment_blob(&entry.get("before")).unwrap();
    let path = store
        .root
        .join(BLOBS)
        .join(format!("{}.asw1", hex(&pin.sha256)));
    fs::remove_file(path).unwrap();
    let before = store.pin().unwrap();
    assert!(store
        .attempt_acknowledged_history_checkpoint(&mut plan)
        .is_err());
    assert!(!plan.attempted());
    assert_eq!(store.pin().unwrap(), before);
}
#[test]
fn finite_origin_is_never_silently_upgraded_to_segmented() {
    let t = Temp::new();
    let mut store = ActiveServiceWalV1::create(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("profile", s("TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1"))]),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let before = store.pin().unwrap();
    assert!(matches!(
        store.prepare_acknowledged_history_checkpoint(&before, None, &[]),
        Err(ActiveServiceWalErrorV1::WrongContext)
    ));
    assert_eq!(store.pin().unwrap(), before);
}

#[path = "source_main_active_history_extended_tests.rs"]
mod extended_segment_tests;
