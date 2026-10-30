// Administrative metadata protocol only; this does not construct a financial
// selector/caller. Actual Runtime must compare the full real source selection.
use super::*;
use crate::source_main_active_service_wal::{
    ActiveServiceWalBlobV1 as Blob, ActiveServiceWalV1 as Wal, ACTUAL_NATIVE_FACT_PROFILE,
    ACTUAL_SOURCE_FACT_SCOPE, ACTUAL_SOURCE_FACT_WAL_PROFILE,
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
            "actual-source-fact-scope-{}-{}",
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
                .is_ok_and(|p| actual.parent() == Some(p.as_path()))
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
fn create(t: &Temp, scope: Option<&str>) -> (SourceCompactIndex, Wal) {
    let i = SourceCompactIndex::create(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual source fact scope metadata",
    )
    .unwrap();
    let mut origin = o(&[
        ("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE)),
        ("requiredNativeFactProfile", s(ACTUAL_NATIVE_FACT_PROFILE)),
    ]);
    if let Some(scope) = scope {
        origin.insert("actualNativeFactScope", s(scope)).unwrap();
    }
    let w = Wal::create(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        origin,
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    (i, w)
}
fn plan(i: &SourceCompactIndex) -> NativePreparedInlineDatabaseV1 {
    i.prepare_inline_database(
        &i.snapshot().unwrap(),
        "updateAsync",
        vec![
            o(&[("_id", s("MaxHeight"))]),
            o(&[("$set", o(&[("value", V::number(11.))]))]),
            o(&[("upsert", V::Bool(true))]),
        ],
    )
    .unwrap()
}
#[test]
fn source_scope_is_immutable_and_attempt_callback_follows_all_actual_fact_guards() {
    let t = Temp::new();
    let (i, w) = create(&t, None);
    let p = plan(&i);
    assert!(w.bind_actual_inline_native_fact(&i, &p).is_err());
    drop(w);
    drop(i);
    let t = Temp::new();
    let (mut i, mut w) = create(&t, Some(ACTUAL_SOURCE_FACT_SCOPE));
    let p = plan(&i);
    let before = i.capture_verified_history_pin().unwrap();
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    let mut attempted = false;
    assert!(i
        .append_inline_database_capture_actual_fact_before(&p, None, &w, &binding, || attempted =
            true)
        .is_err());
    assert!(!attempted);
    assert_eq!(i.capture_verified_history_pin().unwrap(), before);
    let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(p.append_plan().unwrap(),p.durable_read_context()).unwrap();
    let raw_before = before.canonical_scalar_bytes().unwrap();
    let candidates = actual
        .independently_retained_facts()
        .diagnostic_json()
        .unwrap()
        .into_bytes();
    let mutations = actual.exact_prepared_mutations().to_vec();
    let blobs = vec![raw_before, candidates, mutations];
    let refs = blobs
        .iter()
        .map(|b| Blob::of(b).unwrap())
        .collect::<Vec<_>>();
    let record = o(&[
        ("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE)),
        ("stage", s("Intent")),
        (
            "actualSourceSelection",
            o(&[("profile", s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1"))]),
        ),
        ("actualNativeReadContext", p.durable_read_context()),
        ("actualNativeBeforeHistory", refs[0].value()),
        ("actualNativeCandidates", refs[1].value()),
        ("actualNativeMutations", refs[2].value()),
    ]);
    let mut intent = w.prepare(&w.pin().unwrap(), record, blobs, refs).unwrap();
    w.attempt(&mut intent).unwrap();
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    i.test_fail_after_sync();
    let actual = i
        .append_inline_database_capture_actual_fact_before(&p, None, &w, &binding, || {
            attempted = true
        })
        .unwrap();
    assert!(attempted);
    assert!(actual.actual_result().is_err());
    assert_eq!(actual.outcome().get("kind"), s("NativeIoError"));
}

#[test]
fn decoded_pending_fact_is_only_an_expectation_and_recomputes_against_actual_old_pages() {
    use crate::source_main_active_service_wal::ActualNativePlanFactExpectationV1 as Expected;
    let t = Temp::new();
    let (mut i, w) = create(&t, Some(ACTUAL_SOURCE_FACT_SCOPE));
    let p = plan(&i);
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    let raw = binding.capture_pending_plan_fact(&w, &i, &p).unwrap();
    let raw = raw.canonical_fact_bytes();
    let expected = Expected::from_canonical_fact_bytes(i.native_namespace(), raw).unwrap();
    let before = i.capture_verified_history_pin().unwrap();
    assert_eq!(expected.original(), &before);
    expected
        .verify_origin_record(
            &w.pin().unwrap(),
            w.origin_context()
                .unwrap()
                .diagnostic_json()
                .unwrap()
                .as_bytes(),
            w.record().unwrap().diagnostic_json().unwrap().as_bytes(),
        )
        .unwrap();
    assert!(expected
        .verify_origin_record(
            &w.pin().unwrap(),
            b"different actual context",
            b"different actual record"
        )
        .is_err());
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut i, &before).unwrap();
    let historical = view
        .prepare_historical_inline_database(p.method(), p.arguments().to_vec())
        .unwrap();
    expected.verify_recomputed_plan(&historical).unwrap();
    let wrong = view
        .prepare_historical_inline_database(
            "updateAsync",
            vec![
                o(&[("_id", s("different"))]),
                o(&[("$set", o(&[("value", V::number(11.))]))]),
                o(&[("upsert", V::Bool(true))]),
            ],
        )
        .unwrap();
    assert!(expected.verify_recomputed_plan(&wrong).is_err());
    let parsed =
        crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(raw).unwrap()).unwrap();
    let mut extra = parsed.clone();
    extra.insert("notProduced", V::Bool(true)).unwrap();
    assert!(Expected::from_canonical_fact_bytes(
        i.native_namespace(),
        extra.diagnostic_json().unwrap().as_bytes()
    )
    .is_err());
    // The public V encoder already refuses duplicate keys. Supply malformed
    // external bytes directly so this exercises the decoder, not that encoder.
    let duplicate = format!(
        "{},\"profile\":\"{}\"}}",
        &std::str::from_utf8(raw).unwrap()[..raw.len() - 1],
        ACTUAL_NATIVE_FACT_PROFILE
    );
    assert!(
        Expected::from_canonical_fact_bytes(i.native_namespace(), duplicate.as_bytes()).is_err()
    );
    let mut trailing = raw.to_vec();
    trailing.push(b' ');
    assert!(Expected::from_canonical_fact_bytes(i.native_namespace(), &trailing).is_err());
    let oversized = vec![b' '; crate::source_main_active_service_wal::MAX_BLOB + 1];
    assert!(Expected::from_canonical_fact_bytes(i.native_namespace(), &oversized).is_err());
    assert_eq!(i.capture_verified_history_pin().unwrap(), before);
}

#[test]
fn typed_io_decoder_binds_the_real_attempt_not_a_source_answer_or_fabricated_post() {
    use crate::source_main_active_service_wal::ActualNativeIoFactExpectationV1 as Expected;
    let t = Temp::new();
    let (mut i, mut w) = create(&t, Some(ACTUAL_SOURCE_FACT_SCOPE));
    let p = plan(&i);
    let before = i.capture_verified_history_pin().unwrap();
    let actual=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(p.append_plan().unwrap(),p.durable_read_context()).unwrap();
    let blobs = vec![
        before.canonical_scalar_bytes().unwrap(),
        actual
            .independently_retained_facts()
            .diagnostic_json()
            .unwrap()
            .into_bytes(),
        actual.exact_prepared_mutations().to_vec(),
    ];
    let refs = blobs
        .iter()
        .map(|b| Blob::of(b).unwrap())
        .collect::<Vec<_>>();
    let record = o(&[
        ("profile", s(ACTUAL_SOURCE_FACT_WAL_PROFILE)),
        ("stage", s("Intent")),
        (
            "actualSourceSelection",
            o(&[("profile", s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1"))]),
        ),
        ("actualNativeReadContext", p.durable_read_context()),
        ("actualNativeBeforeHistory", refs[0].value()),
        ("actualNativeCandidates", refs[1].value()),
        ("actualNativeMutations", refs[2].value()),
    ]);
    let mut intent = w.prepare(&w.pin().unwrap(), record, blobs, refs).unwrap();
    w.attempt(&mut intent).unwrap();
    let binding = w.bind_actual_inline_native_fact(&i, &p).unwrap();
    i.test_fail_after_sync();
    let attempt = i
        .append_inline_database_capture_actual_fact(&p, None, &w, &binding)
        .unwrap();
    let raw = attempt.canonical_io_fact_bytes().unwrap();
    let pending = attempt.pending_fact().canonical_fact_bytes();
    let expected =
        Expected::from_canonical_fact_bytes(i.native_namespace(), pending, &raw).unwrap();
    assert_eq!(&expected.observed_outcome(), attempt.outcome());
    assert!(expected.expected_verified_post().is_none());
    let mut forged =
        crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&raw).unwrap())
            .unwrap();
    forged
        .insert(
            "verifiedPostHex",
            s(prepared_hex(&before.canonical_scalar_bytes().unwrap())),
        )
        .unwrap();
    assert!(Expected::from_canonical_fact_bytes(
        i.native_namespace(),
        pending,
        forged.diagnostic_json().unwrap().as_bytes()
    )
    .is_err());
    let mut different = pending.to_vec();
    different.push(b' ');
    assert!(Expected::from_canonical_fact_bytes(i.native_namespace(), &different, &raw).is_err());
}
