// Explicit metadata-operation inputs only. These are not financial results or
// a source replay input. Real file-backed WAL/index operations are exercised.
use super::*;
use crate::source_compact_index_v2::{
    NativePreparedValidityOperationV1, NativePreparedValidityQueryV1, SourceCompactIndex,
};
use crate::source_contract_replay::source_durable::SourceDurableFilesystem;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let base = std::env::var_os("ORDINARY_OPERATION_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let path = base.join(format!(
            "ordinary-op-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn setup() -> (
    Temp,
    SourceCompactIndex,
    crate::source_compact_index_v2::NativePreparedValidityWriteV1,
    ActiveServiceWalV1,
) {
    setup_origin(false)
}
fn setup_origin(
    operation: bool,
) -> (
    Temp,
    SourceCompactIndex,
    crate::source_compact_index_v2::NativePreparedValidityWriteV1,
    ActiveServiceWalV1,
) {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "explicit metadata operation",
    )
    .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[
                ("_id", s("tx-1-id")),
                (
                    "value",
                    obj(&[("payload", s("literal metadata")), ("txId", s("id"))]),
                ),
            ]),
        )
        .unwrap();
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            s("id"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!("selected actual row")
    };
    let NativePreparedValidityOperationV1::Write(write) = selected
        .prepare_write(&index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!("actual write")
    };
    let mut origin = obj(&[
        (
            "profile",
            s(if operation {
                ORDINARY_VALIDITY_OPERATION_WAL_PROFILE
            } else {
                SEGMENTED_VALIDITY_FACT_WAL_PROFILE
            }),
        ),
        (
            "requiredNativeFactProfile",
            s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
        ),
        ("actualNativeFactScope", s(ACTUAL_VALIDITY_FACT_SCOPE)),
    ]);
    if operation {
        origin
            .insert(
                "operationSourceScope",
                s("RetainedOrdinaryValidityAction-1"),
            )
            .unwrap();
    }
    let wal = ActiveServiceWalV1::create(
        &temp.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        origin,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    (temp, index, *write, wal)
}
fn intent(
    index: &SourceCompactIndex,
    write: &crate::source_compact_index_v2::NativePreparedValidityWriteV1,
    wal: &mut ActiveServiceWalV1,
    stage: &str,
    operation: bool,
) {
    let history = index
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let facts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),write.durable_read_context()).unwrap();
    let blobs = vec![
        history,
        bytes(facts.independently_retained_facts()).unwrap(),
        facts.exact_prepared_mutations().to_vec(),
    ];
    let refs = blobs
        .iter()
        .map(|b| ActiveServiceWalBlobV1::of(b).unwrap())
        .collect::<Vec<_>>();
    let mut record = obj(&[
        (
            "profile",
            wal.origin_context().unwrap().get("origin").get("profile"),
        ),
        ("stage", s(stage)),
        ("nativeReadContext", write.durable_read_context()),
        ("nativeBeforeHistory", refs[0].value()),
        ("nativeCandidates", refs[1].value()),
        ("nativeMutations", refs[2].value()),
    ]);
    if operation {
        record
            .insert(
                "operationProfile",
                s("TL-MAIN-ORDINARY-VALIDITY-OPERATION-13-1"),
            )
            .unwrap();
    }
    let mut prepared = wal
        .prepare(&wal.pin().unwrap(), record, blobs, refs)
        .unwrap();
    wal.attempt(&mut prepared).unwrap();
}
#[test]
fn original_intent_and_new_explicit_supersession_intent_bind_actual_plan_before_io() {
    for (stage, operation) in [("Intent", false), ("NativeSupersessionIntent", true)] {
        let (_temp, index, write, mut wal) = setup_origin(operation);
        intent(&index, &write, &mut wal, stage, operation);
        let before = index.capture_verified_history_pin().unwrap();
        let binding = wal.bind_actual_validity_fact(&index, &write).unwrap();
        binding.require_synced_intent(&wal).unwrap();
        let first = binding.capture_pending_fact(&wal, &index, &write).unwrap();
        let second = binding.capture_pending_fact(&wal, &index, &write).unwrap();
        assert_eq!(first.canonical_fact_bytes(), second.canonical_fact_bytes());
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    }
}
#[test]
fn supersession_label_without_new_operation_profile_refuses_exact_old_plan() {
    for marker in [false, true] {
        let (_temp, index, write, mut wal) = setup();
        intent(&index, &write, &mut wal, "NativeSupersessionIntent", marker);
        let before = index.capture_verified_history_pin().unwrap();
        let binding = wal.bind_actual_validity_fact(&index, &write).unwrap();
        assert!(matches!(
            binding.require_synced_intent(&wal),
            Err(ActiveServiceWalErrorV1::WrongContext)
        ));
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    }
}
#[test]
fn foreign_actual_wal_and_different_candidates_never_reuse_binding() {
    let (_temp, index, write, mut wal) = setup();
    intent(&index, &write, &mut wal, "Intent", false);
    let binding = wal.bind_actual_validity_fact(&index, &write).unwrap();
    let (_foreign, _index, _write, mut foreign) = setup();
    intent(
        &index,
        &write,
        &mut foreign,
        "NativeSupersessionIntent",
        true,
    );
    assert!(binding
        .capture_pending_fact(&foreign, &index, &write)
        .is_err());
    let before = index.capture_verified_history_pin().unwrap();
    let record = wal.record().unwrap();
    let mut altered = record.clone();
    altered.insert("nativeCandidates", V::Null).unwrap();
    let mut prepared = wal
        .prepare(&wal.pin().unwrap(), altered, vec![], vec![])
        .unwrap();
    wal.attempt(&mut prepared).unwrap();
    let rebound = wal.bind_actual_validity_fact(&index, &write).unwrap();
    assert!(matches!(
        rebound.require_synced_intent(&wal),
        Err(ActiveServiceWalErrorV1::WrongPin)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn real_synced_control_lost_ack_resolves_without_a_native_call_or_changed_metadata() {
    static PROBES: AtomicU64 = AtomicU64::new(0);
    fn probe(_plan: &PreparedActiveServiceWalWriteV1, post: bool) {
        PROBES.fetch_add(if post { 10 } else { 1 }, Ordering::SeqCst);
    }
    let (_temp, index, _write, mut wal) = setup();
    let native = index.capture_verified_history_pin().unwrap();
    let mut prepared = wal
        .prepare(
            &wal.pin().unwrap(),
            obj(&[("stage", s("comparison-only control"))]),
            vec![],
            vec![],
        )
        .unwrap();
    let post = prepared.post().clone();
    wal.set_test_commit_probe(probe);
    wal.set_test_fault(TestFault::AfterCommit);
    assert!(wal.attempt(&mut prepared).is_err());
    assert_eq!(PROBES.load(Ordering::SeqCst), 11);
    assert_eq!(
        wal.resolve(&prepared).unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(wal.pin().unwrap(), post);
    assert!(matches!(
        wal.attempt(&mut prepared),
        Err(ActiveServiceWalErrorV1::AlreadyAttempted)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), native);
    assert_eq!(PROBES.load(Ordering::SeqCst), 11);
}
