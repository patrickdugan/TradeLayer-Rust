// Test-only child of mixed_native_recovery_cold_core_tests. Genuine owners and
// raw controls only. Observed values below are comparisons, never cold inputs.
use super::*;

#[test]
fn mixed_cold_rejected_reappend_does_not_consume_original_verified_post_error() {
    let t = temp();
    let (mut owner, archive, pin, tip) = mixed_actor_setup(&t);
    active_mut(&mut owner.actual_mut().inner)
        .owner
        .index
        .test_fail_after_sync();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    let source_stop = owner
        .active_service()
        .unwrap()
        .history()
        .last()
        .unwrap()
        .result
        .clone();
    assert!(matches!(
        source_stop,
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::StorageStopped(_))
    ));
    let state = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let decodes = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let native = owner.index().capture_verified_history_pin().unwrap();
    let io = owner.index().storage_io_counters().unwrap();
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let record = owner.actual().inner.wal.record().unwrap();
    assert_eq!(record.get("mixedOperationKind"), s("SourceAction"));
    assert_eq!(record.get("ordinaryNativeIoFact"), V::Undefined);
    assert_eq!(record.get("actualNativeIoFact"), V::Undefined);
    let accepted = fact_blob(owner.actual(), record.get("accepted"));
    let fields = u64::from_be_bytes(accepted[6..14].try_into().unwrap()) as usize;
    assert_eq!(
        fact_parse(&accepted[14 + fields + 8..]).get("apiRejection"),
        s("WriteAlreadyAttempted")
    );
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
    );
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .result,
        source_stop
    );
    assert_eq!(owner.financial_runtime().state_value().unwrap(), state);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decodes
    );
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        native
    );
    let after = owner.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    verify_recovery(owner, &t, archive, pin, tip);
}

#[test]
fn mixed_cold_second_native_error_remains_native_only_through_advance_and_pause() {
    let t = temp();
    let (mut owner, archive, pin, tip) = failed_before(&t);
    active_mut(&mut owner.actual_mut().inner)
        .owner
        .index
        .test_fail_after_sync();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    let state = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let decodes = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let native = owner.index().capture_verified_history_pin().unwrap();
    let io = owner.index().storage_io_counters().unwrap();
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
    );
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .result,
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::AwaitNativeWrite)
    );
    assert_eq!(owner.financial_runtime().state_value().unwrap(), state);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decodes
    );
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        native
    );
    let after = owner.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    verify_recovery(owner, &t, archive, pin, tip);
}
