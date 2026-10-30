// Actual-owner controls. Child of the existing durable-service fixture tests.
// No validity/processed/financial outcome is supplied as a construction input.
use super::*;
use crate::source_main_active_service_wal::{
    ActiveServiceWalBlobV1 as FactBlob, ActiveServiceWalV1 as FactWal,
    ActualValidityIoFactExpectationV1, ActualValidityPlanFactExpectationV1,
    ACTUAL_VALIDITY_FACT_SCOPE, ACTUAL_VALIDITY_PLAN_FACT_PROFILE,
    SEGMENTED_VALIDITY_FACT_WAL_PROFILE,
};
fn fact_value(text: impl Into<String>) -> V {
    V::String(text.into())
}
fn facts_wal(temp: &Temp, profile: &str) -> FactWal {
    FactWal::create(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[
            ("profile", fact_value(profile)),
            (
                "requiredNativeFactProfile",
                fact_value(ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
            ),
            (
                "actualNativeFactScope",
                fact_value(ACTUAL_VALIDITY_FACT_SCOPE),
            ),
        ]),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn actual_pending(temp: &Temp) -> (SourceMainNativeActiveServiceV1, FactWal) {
    let (owner, docs) = setup(temp, "detached-live-rpc-failure");
    let wal = facts_wal(temp, SEGMENTED_VALIDITY_FACT_WAL_PROFILE);
    let expected = owner.expectation().unwrap();
    let mut active = owner
        .into_active_service(&expected, V::number(10.), 71, docs, 64 * 1024 * 1024)
        .unwrap();
    for _ in 0..12 {
        let expected = active.expectation().unwrap();
        active
            .apply(
                &expected,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            )
            .unwrap();
        if active.pending_write().is_some() {
            return (active, wal);
        }
    }
    panic!("actual source native await not reached")
}
fn intent(wal: &mut FactWal, service: &SourceMainNativeActiveServiceV1) {
    let write = service.pending_write().unwrap();
    let history = service
        .index()
        .capture_verified_history_pin()
        .unwrap()
        .canonical_scalar_bytes()
        .unwrap();
    let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),write.durable_read_context()).unwrap();
    let raw = candidates
        .independently_retained_facts()
        .diagnostic_json()
        .unwrap()
        .into_bytes();
    let mutations = candidates.exact_prepared_mutations().to_vec();
    let blobs = vec![history, raw, mutations];
    let refs = blobs
        .iter()
        .map(|b| FactBlob::of(b).unwrap())
        .collect::<Vec<_>>();
    let record = obj(&[
        ("profile", fact_value(SEGMENTED_VALIDITY_FACT_WAL_PROFILE)),
        ("stage", fact_value("Intent")),
        ("nativeReadContext", write.durable_read_context()),
        ("nativeBeforeHistory", refs[0].value()),
        ("nativeCandidates", refs[1].value()),
        ("nativeMutations", refs[2].value()),
    ]);
    let mut plan = wal
        .prepare(&wal.pin().unwrap(), record, blobs, refs)
        .unwrap();
    wal.attempt(&mut plan).unwrap();
}
#[test]
fn actual_validity_plan_io_success_is_owned_before_source_observation_or_resolve() {
    let temp = Temp::new();
    let (mut active, mut wal) = actual_pending(&temp);
    intent(&mut wal, &active);
    let effects = active.effects().to_vec();
    let count = active
        .financial_runtime()
        .native_main_test_row_decode_count();
    let pending = active.pending_write().unwrap() as *const _;
    let read = active.pending_native_context();
    let write = active.frame.as_ref().unwrap().pending_write().unwrap();
    let binding = wal
        .bind_actual_validity_fact(&active.owner.index, write)
        .unwrap();
    let before = active.owner.index.snapshot().unwrap();
    let attempt = active
        .owner
        .index
        .commit_validity_capture_actual_fact(write, None, &wal, &binding)
        .unwrap();
    assert!(attempt.actual_result().is_ok());
    assert!(attempt.verified_post().is_some());
    assert_ne!(active.owner.index.snapshot().unwrap(), before);
    let raw = attempt.pending_fact().canonical_fact_bytes();
    let expected = ActualValidityPlanFactExpectationV1::from_canonical_fact_bytes(
        active.index().native_namespace(),
        raw,
    )
    .unwrap();
    assert_eq!(expected.native_read_context(), read.unwrap());
    let io = attempt.canonical_io_fact_bytes().unwrap();
    let io = ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(
        active.index().native_namespace(),
        raw,
        &io,
    )
    .unwrap();
    assert!(io.native_io().is_none());
    assert_eq!(io.verified_post(), attempt.verified_post());
    assert_eq!(active.pending_write().unwrap() as *const _, pending);
    assert_eq!(active.effects(), effects);
    assert_eq!(
        active
            .financial_runtime()
            .native_main_test_row_decode_count(),
        count
    );
    assert!(active.attempted_write.is_none());
    assert!(!active.observation_pending());
    assert!(crate::source_main_active_service_wal::ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(active.index().native_namespace(),raw).is_err());
}
#[test]
fn real_partial_and_lost_return_errors_produce_typed_io_not_source_result_input() {
    for post in [false, true] {
        let temp = Temp::new();
        let (mut active, mut wal) = actual_pending(&temp);
        intent(&mut wal, &active);
        let effects = active.effects().to_vec();
        let count = active
            .financial_runtime()
            .native_main_test_row_decode_count();
        let read = active.pending_native_context();
        if post {
            active.owner.index.test_fail_after_sync()
        } else {
            active.owner.index.test_fail_after_partial_write()
        }
        let write = active.frame.as_ref().unwrap().pending_write().unwrap();
        let binding = wal
            .bind_actual_validity_fact(&active.owner.index, write)
            .unwrap();
        let attempt = active
            .owner
            .index
            .commit_validity_capture_actual_fact(write, None, &wal, &binding)
            .unwrap();
        assert!(attempt.actual_result().is_err());
        assert!(attempt.verified_post().is_none());
        let io = attempt.canonical_io_fact_bytes().unwrap();
        let parsed = ActualValidityIoFactExpectationV1::from_canonical_fact_bytes(
            active.index().native_namespace(),
            attempt.pending_fact().canonical_fact_bytes(),
            &io,
        )
        .unwrap();
        let typed = parsed.native_io().unwrap().reconstruct();
        let computed = crate::source_compact_index_v2::CompactIndexError::Storage(
            crate::storage::StorageError::Io(typed),
        )
        .to_string();
        assert_eq!(
            computed,
            attempt.actual_result().as_ref().err().unwrap().to_string()
        );
        assert_eq!(active.effects(), effects);
        assert_eq!(
            active
                .financial_runtime()
                .native_main_test_row_decode_count(),
            count
        );
        assert_eq!(active.pending_native_context(), read);
        assert!(active.attempted_write.is_none());
        assert!(!active.observation_pending());
    }
}
#[test]
fn foreign_or_unsynced_fact_binding_refuses_before_native_call_and_source_reentry() {
    let temp = Temp::new();
    let (mut active, wal) = actual_pending(&temp);
    let effects = active.effects().to_vec();
    let before = active.index().snapshot().unwrap();
    let write = active.frame.as_ref().unwrap().pending_write().unwrap();
    let binding = wal
        .bind_actual_validity_fact(&active.owner.index, write)
        .unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(active
        .owner
        .index
        .commit_validity_capture_actual_fact_before(write, None, &wal, &binding, || calls
            .set(calls.get() + 1))
        .is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(active.index().snapshot().unwrap(), before);
    assert_eq!(active.effects(), effects);
    let foreign_temp = Temp::new();
    let mut foreign = facts_wal(&foreign_temp, SEGMENTED_VALIDITY_FACT_WAL_PROFILE);
    intent(&mut foreign, &active);
    let write = active.frame.as_ref().unwrap().pending_write().unwrap();
    assert!(active
        .owner
        .index
        .commit_validity_capture_actual_fact_before(write, None, &foreign, &binding, || calls
            .set(calls.get() + 1))
        .is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(active.index().snapshot().unwrap(), before);
    assert_eq!(active.effects(), effects);
}
#[test]
fn ordinary_fact_producer_preserves_the_cold_write_fence_and_refuses_old_fact_origin() {
    let temp = Temp::new();
    let (mut active, mut wal) = actual_pending(&temp);
    intent(&mut wal, &active);
    let old = Temp::new();
    let old_wal = facts_wal(&old, "TL-MAIN-ACTIVE-SERVICE-ACTUAL-NATIVE-FACTS-WAL-13-2");
    let write = active.frame.as_ref().unwrap().pending_write().unwrap();
    assert!(old_wal
        .bind_actual_validity_fact(&active.owner.index, write)
        .is_err());
    let binding = wal
        .bind_actual_validity_fact(&active.owner.index, write)
        .unwrap();
    let before = active.owner.index.capture_verified_history_pin().unwrap();
    let effects = active.effects().to_vec();
    let cap = active
        .owner
        .index
        .begin_verified_cold_execution(&before)
        .unwrap();
    let calls = std::cell::Cell::new(0);
    let stopped = active
        .owner
        .index
        .commit_validity_capture_actual_fact_before(write, None, &wal, &binding, || {
            calls.set(calls.get() + 1)
        });
    assert!(matches!(
        stopped,
        Err(crate::source_compact_index_v2::CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(
        active.owner.index.capture_verified_history_pin().unwrap(),
        before
    );
    assert_eq!(active.effects(), effects);
    active
        .owner
        .index
        .finish_verified_cold_execution(&cap)
        .unwrap();
}
#[test]
fn serializer_failure_after_real_io_retains_whole_source_and_owned_result_without_reappend() {
    for lost in [false, true] {
        let temp = Temp::new();
        let (active, wal) = actual_pending(&temp);
        let effects = active.effects().to_vec();
        let decode = active
            .financial_runtime()
            .native_main_test_row_decode_count();
        let context = active.pending_native_context();
        let mut boundary = SourceMainNativeValidityFactBoundaryV1::adopt(active, wal).unwrap();
        boundary.prepare_intent().unwrap();
        boundary.sync_intent().unwrap();
        if lost {
            boundary.owner.owner.index.test_fail_after_sync()
        }
        boundary.attempt_native().unwrap();
        assert!(boundary.native_attempted());
        assert_eq!(
            boundary.phase(),
            NativeValidityFactBoundaryPhaseV1::Attempted
        );
        assert!(boundary.prepare_fact_budget(1).is_err());
        assert_eq!(
            boundary.phase(),
            NativeValidityFactBoundaryPhaseV1::Attempted
        );
        assert!(boundary.attempt.is_some());
        if lost {
            assert!(boundary.attempt.as_ref().unwrap().actual_result().is_err())
        } else {
            assert!(boundary.attempt.as_ref().unwrap().actual_result().is_ok())
        }
        assert!(boundary.attempt_native().is_err());
        assert!(boundary.prepare_intent().is_err());
        assert_eq!(boundary.effects(), effects);
        assert_eq!(
            boundary
                .financial_runtime()
                .native_main_test_row_decode_count(),
            decode
        );
        assert_eq!(boundary.pending_native_context(), context);
        assert!(!boundary.owner.observation_pending());
        boundary.prepare_fact().unwrap();
        boundary
            .wal
            .set_test_fault(crate::source_main_active_service_wal::TestFault::AfterCommit);
        assert!(boundary.sync_fact().is_err());
        assert_eq!(
            boundary.phase(),
            NativeValidityFactBoundaryPhaseV1::FactPrepared
        );
        assert!(boundary.attempt_native().is_err());
        assert_eq!(boundary.effects(), effects);
        assert_eq!(
            boundary.resolve_control().unwrap(),
            crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post
        );
        assert_eq!(
            boundary.phase(),
            NativeValidityFactBoundaryPhaseV1::FactSynced
        );
        assert!(boundary.attempt.is_some());
        assert!(boundary.attempt_native().is_err());
        assert_eq!(boundary.effects(), effects);
        assert_eq!(
            boundary
                .financial_runtime()
                .native_main_test_row_decode_count(),
            decode
        );
        assert_eq!(boundary.pending_native_context(), context);
    }
}

#[path = "source_main_ordinary_native_operation_tests.rs"]
mod ordinary_native_operation_tests;
