use super::*;
use crate::source_main_active_service_wal::{
    ActiveServiceWalBlobV1 as Blob, ActiveServiceWalV1 as Wal, ACTUAL_NATIVE_FACT_PROFILE,
    ACTUAL_NATIVE_FACT_WAL_PROFILE,
};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_ACTUAL_FACTS_TMP")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let p = parent.canonicalize().unwrap().join(format!(
            "actual-native-facts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let (Ok(actual), Some(parent)) = (self.0.canonicalize(), self.0.parent()) {
            if parent
                .canonicalize()
                .is_ok_and(|expected| actual.parent() == Some(expected.as_path()))
            {
                let _ = std::fs::remove_dir_all(actual);
            }
        }
    }
}
fn o(f: &[(&str, V)]) -> V {
    V::Object(f.iter().map(|(k, v)| ((*k).into(), v.clone())).collect())
}
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn store(t: &Temp, profile: &str) -> Wal {
    Wal::create(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        o(&[
            ("profile", s(profile)),
            ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
        ]),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn index(t: &Temp) -> SourceCompactIndex {
    SourceCompactIndex::create(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual native fact fixture",
    )
    .unwrap()
}
fn plan(i: &SourceCompactIndex, value: f64) -> NativePreparedInlineDatabaseV1 {
    i.prepare_inline_database(
        &i.snapshot().unwrap(),
        "updateAsync",
        vec![
            o(&[("_id", s("MaxHeight"))]),
            o(&[("$set", o(&[("value", V::number(value))]))]),
            o(&[("upsert", V::Bool(true))]),
        ],
    )
    .unwrap()
}
fn intent(w: &mut Wal, i: &SourceCompactIndex, p: &NativePreparedInlineDatabaseV1) {
    let before = i
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(p.append_plan().unwrap(),p.durable_read_context()).unwrap();
    let candidates = actual
        .independently_retained_facts()
        .diagnostic_json()
        .unwrap()
        .into_bytes();
    let mutations = actual.exact_prepared_mutations().to_vec();
    let blobs = vec![before, candidates, mutations];
    let refs = blobs
        .iter()
        .map(|b| Blob::of(b).unwrap())
        .collect::<Vec<_>>();
    let record = o(&[
        ("profile", s(ACTUAL_NATIVE_FACT_WAL_PROFILE)),
        ("stage", s("Intent")),
        ("nativeReadContext", p.durable_read_context()),
        ("nativeBeforeHistory", refs[0].value()),
        ("nativeCandidates", refs[1].value()),
        ("nativeMutations", refs[2].value()),
    ]);
    let mut write = w.prepare(&w.pin().unwrap(), record, blobs, refs).unwrap();
    w.attempt(&mut write).unwrap();
}
#[test]
fn pending_fact_is_actual_pre_io_metadata_and_append_retains_owned_result_without_source_release() {
    let t = Temp::new();
    let mut i = index(&t);
    let mut w = store(&t, ACTUAL_NATIVE_FACT_WAL_PROFILE);
    let p = plan(&i, 11.);
    let pre = i.capture_verified_history_pin().unwrap();
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    let raw = binding.capture_pending_plan_fact(&w, &i, &p).unwrap();
    assert!(!raw.canonical_fact_bytes().is_empty());
    assert_eq!(i.capture_verified_history_pin().unwrap(), pre);
    assert!(
        i.append_inline_database_capture_actual_fact(&p, None, &w, &binding)
            .is_err(),
        "no synced native Intent"
    );
    assert_eq!(i.capture_verified_history_pin().unwrap(), pre);
    intent(&mut w, &i, &p);
    assert!(
        binding.capture_pending_plan_fact(&w, &i, &p).is_err(),
        "old WAL point cannot authorize new action"
    );
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    let attempt = i
        .append_inline_database_capture_actual_fact(&p, None, &w, &binding)
        .unwrap();
    assert!(attempt.actual_result().is_ok());
    assert_eq!(attempt.outcome().get("kind"), s("NativeAppendReturned"));
    assert!(attempt.verified_post().is_some());
    assert!(!attempt.canonical_io_fact_bytes().unwrap().is_empty());
    assert!(attempt.into_actual_result().is_ok());
    i.resolve_inline_database_post(&p).unwrap();
}
#[test]
fn actual_lost_native_return_records_typed_io_before_caller_display_and_never_fabricates_post() {
    let t = Temp::new();
    let mut i = index(&t);
    let mut w = store(&t, ACTUAL_NATIVE_FACT_WAL_PROFILE);
    let p = plan(&i, 11.);
    intent(&mut w, &i, &p);
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    i.test_fail_after_sync();
    let attempt = i
        .append_inline_database_capture_actual_fact(&p, None, &w, &binding)
        .unwrap();
    assert!(attempt.actual_result().is_err());
    assert_eq!(attempt.outcome().get("kind"), s("NativeIoError"));
    assert_eq!(attempt.outcome().get("ioKind"), s("Other"));
    assert_eq!(
        attempt.outcome().get("exactSourceDisplay"),
        s(attempt.actual_result().as_ref().unwrap_err().to_string())
    );
    assert!(attempt.verified_post().is_none());
    assert!(matches!(
        i.snapshot(),
        Err(CompactIndexError::Storage(StorageError::Poisoned))
    ));
    let raw = attempt.canonical_io_fact_bytes().unwrap();
    assert!(String::from_utf8(raw).unwrap().contains("NativeIoError"));
    drop(i);
    let (i, _) = SourceCompactIndex::reopen_inline_database(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &p,
        NativePreparedValidityCandidateV1::Post,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    i.resolve_inline_database_post(&p).unwrap();
}
#[test]
fn old_modes_foreign_wal_and_changed_plan_config_cannot_promote_fact_to_append_authority() {
    let a = Temp::new();
    let mut i = index(&a);
    let p = plan(&i, 11.);
    let old = store(&a, "TL-MAIN-ACTIVE-SERVICE-INLINE-ESCROW-WAL-13-1");
    assert!(old.bind_actual_inline_native_fact(&i, &p).is_err());
    drop(old);
    let b = Temp::new();
    let mut w = store(&b, ACTUAL_NATIVE_FACT_WAL_PROFILE);
    intent(&mut w, &i, &p);
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    let foreign = Temp::new();
    let mut other_wal = store(&foreign, ACTUAL_NATIVE_FACT_WAL_PROFILE);
    intent(&mut other_wal, &i, &p);
    assert!(
        binding
            .capture_pending_plan_fact(&other_wal, &i, &p)
            .is_err(),
        "another actual WAL cannot use this lease/prefix binding"
    );
    let other_root = Temp::new();
    let other_index = index(&other_root);
    assert!(
        w.bind_actual_inline_native_fact(&other_index, &p).is_err(),
        "equal logical metadata is not the same physical native owner"
    );
    i.configure_compaction(crate::compact_journal::CompactionConfig {
        target_log_bytes: 16 * 1024 * 1024,
        retained_ancestors: 2,
    })
    .unwrap();
    let replacement = plan(&i, 11.);
    assert_eq!(replacement.durable_read_context(), p.durable_read_context());
    assert!(
        binding
            .capture_pending_plan_fact(&w, &i, &replacement)
            .is_err(),
        "complete candidate/config comparison required even with same read-context"
    );
    assert!(i
        .append_inline_database_capture_actual_fact(&replacement, None, &w, &binding)
        .is_err());
    assert_eq!(i.snapshot().unwrap(), *p.expected());
}

#[test]
fn actual_partial_native_write_fact_preserves_typed_error_and_exact_before_recovery() {
    let t = Temp::new();
    let mut i = index(&t);
    let mut w = store(&t, ACTUAL_NATIVE_FACT_WAL_PROFILE);
    let p = plan(&i, 11.);
    let before = i.capture_verified_history_pin().unwrap();
    intent(&mut w, &i, &p);
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    i.test_fail_after_partial_write();
    let attempt = i
        .append_inline_database_capture_actual_fact(&p, None, &w, &binding)
        .unwrap();
    assert!(attempt.actual_result().is_err());
    assert_eq!(attempt.outcome().get("kind"), s("NativeIoError"));
    assert!(attempt.verified_post().is_none());
    let exact = attempt.actual_result().as_ref().unwrap_err().to_string();
    assert_eq!(attempt.outcome().get("exactSourceDisplay"), s(&exact));
    assert!(attempt.into_actual_result().is_err());
    drop(i);
    let (i, _) = SourceCompactIndex::reopen_inline_database(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &p,
        NativePreparedValidityCandidateV1::Before,
        Recovery::DiscardIncompleteTail,
    )
    .unwrap();
    assert_eq!(i.capture_verified_history_pin().unwrap(), before);
}

#[test]
fn new_fact_append_entry_cannot_write_through_the_existing_cold_guard() {
    let t = Temp::new();
    let mut i = index(&t);
    let mut w = store(&t, ACTUAL_NATIVE_FACT_WAL_PROFILE);
    let p = plan(&i, 11.);
    intent(&mut w, &i, &p);
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    let before = i.capture_verified_history_pin().unwrap();
    let wal_before = w.pin().unwrap();
    let io = i.storage_io_counters().unwrap();
    let _guard = i.begin_verified_cold_execution(&before).unwrap();
    assert!(matches!(
        i.append_inline_database_capture_actual_fact(&p, None, &w, &binding),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(i.capture_verified_history_pin().unwrap(), before);
    assert_eq!(w.pin().unwrap(), wal_before);
    let after = i.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
}
