// Real metadata/WAL prerequisites only. No financial owner or source outcome is
// constructed here; private Core controls exercise the controller separately.
use super::*;
use crate::source_compact_index_v2::{
    NativePreparedValidityCandidateV1 as Candidate, SourceCompactIndex,
};
use crate::source_main_retained_caller_action::RetainedMainCallerActionV1 as Action;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
static COMMIT_PROBE: AtomicU64 = AtomicU64::new(0);
fn actual_commit_probe(plan: &PreparedActiveServiceWalWriteV1, committed: bool) {
    assert!(plan.attempted());
    assert_ne!(plan.before(), plan.post());
    COMMIT_PROBE.fetch_or(if committed { 2 } else { 1 }, Ordering::SeqCst);
}
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_ACTIVE_WAL_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "selected-caller-metadata-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = fs::remove_dir_all(root);
        }
    }
}
const SELECTED: &str = "TL-MAIN-RETAINED-CALLER-SELECTED-WAL-13-1";
fn index(t: &Temp) -> SourceCompactIndex {
    SourceCompactIndex::create(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual selected metadata only",
    )
    .unwrap()
}
fn store(t: &Temp) -> ActiveServiceWalV1 {
    let root = t.root.join("wal");
    fs::create_dir(&root).unwrap();
    ActiveServiceWalV1::create(
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[
            ("profile", s(SELECTED)),
            ("admittedActions", s("SelectedActions")),
        ]),
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn prepare(
    index: &SourceCompactIndex,
) -> crate::source_compact_index_v2::NativePreparedInlineDatabaseV1 {
    index
        .prepare_inline_database(
            &index.snapshot().unwrap(),
            "updateAsync",
            vec![
                obj(&[("_id", s("MaxHeight"))]),
                obj(&[("$set", obj(&[("value", V::number(11.))]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )
        .unwrap()
}
fn barrier(
    wal: &ActiveServiceWalV1,
    stage: &str,
    raw: Vec<u8>,
    facts: Vec<u8>,
    mutations: Vec<u8>,
) -> PreparedActiveServiceWalWriteV1 {
    let pins = [&raw, &facts, &mutations]
        .into_iter()
        .map(|b| ActiveServiceWalBlobV1::of(b).unwrap())
        .collect::<Vec<_>>();
    wal.prepare(
        &wal.pin().unwrap(),
        obj(&[
            ("profile", s(SELECTED)),
            ("stage", s(stage)),
            ("rawAction", pins[0].value()),
            ("nativeCandidates", pins[1].value()),
            ("nativeMutations", pins[2].value()),
        ]),
        vec![raw, facts, mutations],
        pins,
    )
    .unwrap()
}
fn actual_facts(
    plan: &crate::source_compact_index_v2::NativePreparedInlineDatabaseV1,
) -> (Vec<u8>, Vec<u8>) {
    let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(plan.append_plan().unwrap(),plan.durable_read_context()).unwrap();
    (
        bytes(actual.independently_retained_facts()).unwrap(),
        actual.exact_prepared_mutations().to_vec(),
    )
}
#[test]
fn selected_raw_intent_pins_actual_opaque_native_candidates_before_any_native_write() {
    for fault in [TestFault::BeforeCommit, TestFault::AfterCommit] {
        let t = Temp::new();
        let mut index = index(&t);
        let plan = prepare(&index);
        let before = index.snapshot().unwrap();
        let (facts, mutations) = actual_facts(&plan);
        // This selector is only raw input in a Store-only test, never a constructed
        // retained-caller capability or a receiver/financial/source outcome.
        let raw = Action::AppendSelected {
            selector: plan.durable_read_context(),
        }
        .canonical_action_bytes()
        .unwrap();
        let mut wal = store(&t);
        COMMIT_PROBE.store(0, Ordering::SeqCst);
        wal.set_test_commit_probe(actual_commit_probe);
        let mut intent = barrier(
            &wal,
            "Intent",
            raw.clone(),
            facts.clone(),
            mutations.clone(),
        );
        wal.set_test_fault(fault);
        assert!(wal.attempt(&mut intent).is_err());
        assert_eq!(index.snapshot().unwrap(), before);
        let resolved = wal.resolve(&intent).unwrap();
        if fault == TestFault::BeforeCommit {
            assert_eq!(resolved, ActiveServiceWalResolvedV1::Before);
            let mut same = wal.rearm_before(&intent).unwrap();
            assert_eq!(same.record().unwrap(), intent.record().unwrap());
            wal.attempt(&mut same).unwrap();
        } else {
            assert_eq!(resolved, ActiveServiceWalResolvedV1::Post);
            assert!(wal.rearm_before(&intent).is_err());
        }
        for expected in [raw, facts, mutations] {
            let pin = ActiveServiceWalBlobV1::of(&expected).unwrap();
            assert_eq!(wal.read_blob(&pin).unwrap(), expected);
        }
        assert_eq!(COMMIT_PROBE.load(Ordering::SeqCst), 3);
        index.append_inline_database(&plan, None).unwrap();
        assert_eq!(index.snapshot().unwrap().sequence, before.sequence + 1);
        index.resolve_inline_database_post(&plan).unwrap();
    }
}
#[test]
fn selected_actual_before_supersession_preserves_mutations_and_requires_synced_mapping_before_append(
) {
    let t = Temp::new();
    let index = index(&t);
    let original = prepare(&index);
    let before = index.snapshot().unwrap();
    let original_mutations = original.append_plan().unwrap().mutations().to_vec();
    drop(index);
    let (mut index, observed) = SourceCompactIndex::reopen_inline_database(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &original,
        Candidate::Before,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let (replacement, mapping) = index
        .supersede_inline_database(&original, &observed)
        .unwrap();
    let proof = index
        .require_inline_native_supersession(&original, &observed, &replacement, &mapping)
        .unwrap();
    assert_eq!(replacement.method(), original.method());
    assert_eq!(replacement.arguments(), original.arguments());
    assert_eq!(replacement.before_state(), original.before_state());
    assert_eq!(replacement.after_state(), original.after_state());
    assert_eq!(
        replacement.append_plan().unwrap().mutations(),
        original_mutations
    );
    assert!(
        index.append_inline_database(&original, None).is_err(),
        "old owner append token must remain refused"
    );
    let mut wal = store(&t);
    let (facts, mutations) = actual_facts(&replacement);
    let raw = bytes(&proof).unwrap();
    let mut intent = barrier(&wal, "NativeSupersessionIntent", raw, facts, mutations);
    wal.set_test_fault(TestFault::AfterCommit);
    assert!(wal.attempt(&mut intent).is_err());
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(
        wal.resolve(&intent).unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        wal.record().unwrap().get("stage"),
        s("NativeSupersessionIntent")
    );
    index.append_inline_database(&replacement, None).unwrap();
    index.resolve_inline_database_post(&replacement).unwrap();
    assert_eq!(index.snapshot().unwrap().sequence, before.sequence + 1);
}
#[test]
fn selected_actual_applied_post_reopens_without_append_and_accepted_dependency_loss_is_refused() {
    let t = Temp::new();
    let mut index = index(&t);
    let plan = prepare(&index);
    index.append_inline_database(&plan, None).unwrap();
    let post = index.snapshot().unwrap();
    let (facts, mutations) = actual_facts(&plan);
    drop(index);
    assert!(SourceCompactIndex::reopen_inline_database(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &plan,
        Candidate::Before,
        Recovery::RejectIncompleteTail
    )
    .is_err());
    let (index, _) = SourceCompactIndex::reopen_inline_database(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &plan,
        Candidate::Post,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    index.resolve_inline_database_post(&plan).unwrap();
    assert_eq!(index.snapshot().unwrap(), post);
    let mut wal = store(&t);
    let mut accepted = barrier(
        &wal,
        "Accepted",
        b"actual metadata post observation, not financial state".to_vec(),
        facts,
        mutations,
    );
    wal.set_test_fault(TestFault::AfterCommit);
    assert!(wal.attempt(&mut accepted).is_err());
    let missing = accepted.references()[1].clone();
    let path = wal
        .root
        .join(BLOBS)
        .join(format!("{}.asw1", hex(&missing.sha256)));
    fs::remove_file(path).unwrap();
    assert!(wal.resolve(&accepted).is_err());
    assert!(matches!(wal.pin(), Err(ActiveServiceWalErrorV1::Ambiguous)));
    assert_eq!(index.snapshot().unwrap(), post);
}
