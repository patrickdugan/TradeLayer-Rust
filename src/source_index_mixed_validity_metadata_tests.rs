// Actual files and metadata command inputs only; no financial Core/result import.
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MIXED_NATIVE_META_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.canonicalize().unwrap().join(format!(
            "mixed-validity-old-view-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn text(v: &str) -> V {
    V::String(v.into())
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn make_index(t: &Temp) -> SourceCompactIndex {
    let mut index = SourceCompactIndex::create(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "literal metadata source operations",
    )
    .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            object(&[
                ("_id", text("tx-1-id")),
                (
                    "value",
                    object(&[("txId", text("id")), ("payload", text("metadata"))]),
                ),
            ]),
        )
        .unwrap();
    index
}
fn plan(index: &SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    let NativePreparedValidityQueryV1::Selected(p) = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            text("id"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!("selected")
    };
    let NativePreparedValidityOperationV1::Write(p) =
        p.prepare_write(index, &index.snapshot().unwrap()).unwrap()
    else {
        panic!("write")
    };
    *p
}
fn metadata_intent(
    t: &Temp,
    index: &SourceCompactIndex,
    plan: &NativePreparedValidityWriteV1,
) -> crate::source_main_active_service_wal::ActiveServiceWalV1 {
    use crate::source_main_active_service_wal::{ActiveServiceWalBlobV1, ActiveServiceWalV1};
    let mut wal = ActiveServiceWalV1::create(
        &t.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        object(&[
            (
                "profile",
                text(crate::source_main_active_service_wal::MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE),
            ),
            (
                "actualNativeFactScope",
                text(crate::source_main_active_service_wal::MIXED_SOURCE_NATIVE_FACT_SCOPE),
            ),
            (
                "requiredNativeFactProfile",
                text(crate::source_main_active_service_wal::ACTUAL_NATIVE_FACT_PROFILE),
            ),
            (
                "requiredOrdinaryNativeFactProfile",
                text(crate::source_main_active_service_wal::ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
            ),
            (
                "ordinaryNativeFactScope",
                text(crate::source_main_active_service_wal::ACTUAL_VALIDITY_FACT_SCOPE),
            ),
            (
                "requiredInlineServiceProfile",
                text(crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE),
            ),
            (
                "underlyingSourceHistoryProfile",
                text(crate::source_main_active_service_wal::MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE),
            ),
            ("acknowledgedWindowLimit", text("256")),
        ]),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let facts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(plan.append_plan(),plan.durable_read_context()).unwrap();
    let blobs = vec![
        index
            .capture_verified_history_pin()
            .unwrap()
            .canonical_scalar_bytes()
            .unwrap(),
        facts
            .independently_retained_facts()
            .diagnostic_json()
            .unwrap()
            .into_bytes(),
        facts.exact_prepared_mutations().to_vec(),
    ];
    let refs = blobs
        .iter()
        .map(|raw| ActiveServiceWalBlobV1::of(raw).unwrap())
        .collect::<Vec<_>>();
    let record = object(&[
        (
            "profile",
            text(crate::source_main_active_service_wal::MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE),
        ),
        ("stage", text("Intent")),
        ("ordinaryNativeReadContext", plan.durable_read_context()),
        ("ordinaryNativeBeforeHistory", refs[0].value()),
        ("ordinaryNativeCandidates", refs[1].value()),
        ("ordinaryNativeMutations", refs[2].value()),
    ]);
    let mut prepared = wal
        .prepare(&wal.pin().unwrap(), record, blobs, refs)
        .unwrap();
    wal.attempt(&mut prepared).unwrap();
    wal
}
#[test]
fn mixed_actual_validity_producer_retains_success_inside_recorded_scope_with_exact_typed_post() {
    let t = Temp::new();
    let mut index = make_index(&t);
    let p = plan(&index);
    let wal = metadata_intent(&t, &index, &p);
    let binding = wal.bind_actual_validity_fact(&index, &p).unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let scope = index.begin_recorded_native_only_scope().unwrap();
    let calls = std::cell::Cell::new(0);
    let actual = index
        .commit_validity_capture_actual_fact_before(&p, None, &wal, &binding, || {
            calls.set(calls.get() + 1)
        })
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert!(actual.actual_result().is_ok());
    assert_ne!(index.capture_verified_history_pin().unwrap(), before);
    assert_eq!(
        actual.verified_post(),
        Some(&index.capture_verified_history_pin().unwrap())
    );
    let pending = actual.pending_fact().canonical_fact_bytes();
    let io = actual.canonical_io_fact_bytes().unwrap();
    let decoded=crate::source_main_active_service_wal::ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(index.native_namespace(),pending,&io).unwrap();
    assert!(decoded.native_io().is_none());
    assert_eq!(decoded.verified_post(), actual.verified_post());
    index.finish_recorded_native_only_scope(scope).unwrap();
    assert!(index
        .commit_validity_capture_actual_fact(&p, None, &wal, &binding)
        .is_err());
    assert_eq!(calls.get(), 1);
}
#[test]
fn mixed_actual_validity_partial_and_lost_returns_remain_native_errors_without_inferred_success() {
    for post in [false, true] {
        let t = Temp::new();
        let mut index = make_index(&t);
        let p = plan(&index);
        let wal = metadata_intent(&t, &index, &p);
        let binding = wal.bind_actual_validity_fact(&index, &p).unwrap();
        let scope = index.begin_recorded_native_only_scope().unwrap();
        if post {
            index.test_fail_after_sync();
        } else {
            index.test_fail_after_partial_write();
        }
        let calls = std::cell::Cell::new(0);
        let actual = index
            .commit_validity_capture_actual_fact_before(&p, None, &wal, &binding, || {
                calls.set(calls.get() + 1)
            })
            .unwrap();
        assert_eq!(calls.get(), 1);
        assert!(actual.actual_result().is_err());
        assert!(actual.verified_post().is_none());
        let raw = actual.canonical_io_fact_bytes().unwrap();
        let io=crate::source_main_active_service_wal::ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(index.native_namespace(),actual.pending_fact().canonical_fact_bytes(),&raw).unwrap();
        assert!(io.native_io().is_some());
        assert!(io.verified_post().is_none());
        assert!(actual.actual_result().is_err());
        index.finish_recorded_native_only_scope(scope).unwrap();
        assert!(index
            .commit_validity_capture_actual_fact(&p, None, &wal, &binding)
            .is_err());
        assert_eq!(calls.get(), 1);
    }
}
#[test]
fn mixed_validity_actual_original_write_view_recomputes_context_without_io_or_resolution() {
    let t = Temp::new();
    let mut index = make_index(&t);
    let before = index.capture_verified_history_pin().unwrap();
    let p = plan(&index);
    index.commit_prepared_validity(&p).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let actual = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let wrong = VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap();
    let io = index.storage_io_counters().unwrap();
    let mut playback =
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before, post.clone()]).unwrap();
    let NativeHistoricalValidityOperationV1::Await(mut awaiting) = playback
        .prepare_source_update(
            text("id"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
            &mut |_| Ok(None),
        )
        .unwrap()
    else {
        panic!("actual await")
    };
    assert_eq!(
        awaiting.recomputed_write_context_at(&actual).unwrap(),
        p.durable_read_context()
    );
    assert!(matches!(
        awaiting.recomputed_write_context_at(&wrong),
        Err(CompactIndexError::WrongContext)
    ));
    assert!(!awaiting.append_verified());
    assert!(!awaiting.resolved());
    assert_eq!(
        awaiting.recomputed_before_state(),
        p.expected_before_state()
    );
    assert_eq!(awaiting.recomputed_after_state(), p.expected_after_state());
    playback
        .verify_recorded_native_append(
            &mut awaiting,
            &crate::source_main_native_intent_store::native_prepared_mutation_bytes(
                p.append_plan().mutations(),
            )
            .unwrap(),
        )
        .unwrap();
    drop(playback);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(io.confirmed_append_bytes, after.confirmed_append_bytes);
    assert_eq!(io.published_compactions, after.published_compactions);
}
#[test]
fn mixed_validity_intermediate_rebase_keeps_original_query_read_and_uses_actual_new_write_before() {
    let t = Temp::new();
    let mut index = make_index(&t);
    index.native_main_test_compaction_fixture().unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let mut p = plan(&index);
    assert!(p.append_plan().intermediate().is_some());
    let escrow_root = t.0.join("source-generation-escrow");
    std::fs::create_dir(&escrow_root).unwrap();
    let permit = index
        .retain_verified_history_generation(
            &escrow_root,
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let expected_escrow = permit.independently_retained_pin().clone();
    p.put.append.test_append_fault("partial");
    assert!(index
        .commit_prepared_validity_with_escrow(&p, Some(&permit))
        .is_err());
    drop(index);
    let (mut index, observed) = SourceCompactIndex::reopen_prepared_validity(
        &t.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &p,
        NativePreparedValidityCandidateV1::Intermediate,
        Recovery::DiscardIncompleteTail,
    )
    .unwrap();
    let reopened = index.capture_verified_history_pin().unwrap();
    let write_view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &reopened).unwrap();
    let (replacement, mapping) = index.supersede_prepared_validity(&p, &observed).unwrap();
    index
        .require_actual_native_supersession(&p, &observed, &replacement, &mapping)
        .unwrap();
    assert!(replacement.append_plan().intermediate().is_none());
    index.commit_prepared_validity(&replacement).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let first = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_copy(&expected_escrow, &before).unwrap(),
    );
    let next = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap(),
    );
    let cap = index.begin_verified_cold_execution(&post).unwrap();
    let io = index.storage_io_counters().unwrap();
    let mut playback = NativeHistoricalValidityPlaybackV1::open_verified_views(
        &mut index,
        &cap,
        &[before.clone(), post.clone()],
        vec![first, next],
    )
    .unwrap();
    let NativeHistoricalValidityOperationV1::Await(awaiting) = playback
        .prepare_source_update(
            text("id"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
            &mut |_| Ok(None),
        )
        .unwrap()
    else {
        panic!("await")
    };
    let context = awaiting.recomputed_write_context_at(&write_view).unwrap();
    assert_eq!(context, replacement.durable_read_context());
    assert_ne!(context, p.durable_read_context());
    assert_eq!(awaiting.query().snapshot, before.metadata);
    assert_eq!(
        awaiting.exact_prepared_mutations(),
        crate::source_main_native_intent_store::native_prepared_mutation_bytes(
            replacement.append_plan().mutations()
        )
        .unwrap()
        .as_slice()
    );
    assert!(!awaiting.append_verified());
    assert!(!awaiting.resolved());
    drop(playback);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(io.confirmed_append_bytes, after.confirmed_append_bytes);
    assert_eq!(io.copied_checkpoint_bytes, after.copied_checkpoint_bytes);
    index.finish_verified_cold_execution(&cap).unwrap();
}
#[test]
fn mixed_recorded_scope_refuses_unretained_put_and_discard_before_io_but_keeps_real_prepared_path()
{
    let t = Temp::new();
    let mut index = make_index(&t);
    let prepared = plan(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let io = index.storage_io_counters().unwrap();
    let scope = index.begin_recorded_native_only_scope().unwrap();
    assert!(index.recorded_native_only_scope_active());
    let fresh = object(&[("_id", text("fresh-unretained")), ("value", V::number(7.))]);
    assert!(matches!(
        index.import_row(&before.metadata, fresh),
        Err(CompactIndexError::Source(
            SourceRuleError::UnknownDependency
        ))
    ));
    assert!(matches!(
        index.discard_rows(
            &before.metadata,
            &["tx-1-id".into()],
            "wrong-unretained-context"
        ),
        Err(CompactIndexError::Source(
            SourceRuleError::UnknownDependency
        ))
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    assert_eq!(index.get("fresh-unretained").unwrap(), None);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(io.confirmed_append_bytes, after.confirmed_append_bytes);
    assert_eq!(io.published_compactions, after.published_compactions);
    index.commit_prepared_validity(&prepared).unwrap();
    index.finish_recorded_native_only_scope(scope).unwrap();
    assert!(!index.recorded_native_only_scope_active());
    assert_eq!(
        index.get("tx-1-id").unwrap().unwrap().get("valid"),
        V::Bool(true)
    );
}
#[test]
fn mixed_recorded_scope_token_is_same_owner_and_clears_after_real_lost_return() {
    let t = Temp::new();
    let mut index = make_index(&t);
    let t2 = Temp::new();
    let mut foreign = make_index(&t2);
    let source = index.begin_recorded_native_only_scope().unwrap();
    let wrong = NativeRecordedOnlyScopeV1 {
        token: source.token,
    };
    assert!(foreign.finish_recorded_native_only_scope(wrong).is_err());
    assert!(index.recorded_native_only_scope_active());
    let prepared = plan(&index);
    index.test_fail_after_sync();
    assert!(index.commit_prepared_validity(&prepared).is_err());
    index.finish_recorded_native_only_scope(source).unwrap();
    assert!(!index.recorded_native_only_scope_active());
}

#[path = "source_index_mixed_historical_result_metadata_tests.rs"]
mod mixed_historical_result_metadata_tests;
