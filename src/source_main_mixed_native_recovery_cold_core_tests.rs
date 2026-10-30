// Child of the frozen first-success actor controls. Actual source owners,
// retained native IO/results and real file recovery only; comparisons below
// never initialize financial/source state.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
fn failed_before(
    t: &Temp,
) -> (
    SourceMainNativeMixedFactServiceV1,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
) {
    let (mut owner, archive, pin, tip) = mixed_actor_setup(t);
    active_mut(&mut owner.actual_mut().inner)
        .owner
        .index
        .test_fail_after_partial_write();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let owner = owner
        .into_no_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    (owner, archive, pin, tip)
}
fn verify_recovery(
    owner: SourceMainNativeMixedFactServiceV1,
    t: &Temp,
    archive: Vec<u8>,
    pin: NativeMainArchivePin,
    tip: SourceIndexHistoryPinV1,
) {
    let state = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let head = owner.active_service().unwrap().head();
    let kernel = owner.active_service().unwrap().kernel_value();
    let count = owner.acknowledged_actions();
    let post = owner.index().capture_verified_history_pin().unwrap();
    let io = owner.index().storage_io_counters().unwrap();
    let cold = mixed_cold(owner, t, archive, pin, tip).unwrap();
    assert_eq!(cold.verified_actions(), count);
    assert_eq!(cold.state.head, head);
    assert_eq!(cold.financial_runtime().state_value().unwrap(), state);
    assert_eq!(cold.state.effects(), effects);
    assert_eq!(
        cold.state.frame.as_ref().unwrap().kernel_value(),
        kernel.unwrap()
    );
    assert_eq!(cold.actual_post(), &post);
    let after = cold.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
}
#[test]
fn mixed_cold_before_native_only_retry_preserves_first_source_error_and_cleared_row_error() {
    let t = temp();
    let (mut owner, archive, pin, tip) = failed_before(&t);
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert!(matches!(
        owner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .result,
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::StorageStopped(_))
    ));
    verify_recovery(owner, &t, archive, pin, tip);
}
#[test]
fn mixed_cold_original_lost_return_post_keeps_known_error_through_advance_pause_and_real_resolve() {
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
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
    );
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    verify_recovery(owner, &t, archive, pin, tip);
}
#[test]
fn mixed_cold_second_native_lost_return_post_never_delivers_second_error_to_source() {
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
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    verify_recovery(owner, &t, archive, pin, tip);
}

#[path = "source_main_mixed_native_review_core_tests.rs"]
mod mixed_native_review_core_tests;
