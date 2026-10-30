use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;

fn operation(temp: &Temp) -> SourceMainNativeValidityFactOperationV1 {
    let (active, old_wal) = actual_pending(temp);
    drop(old_wal);
    let path = temp.root.join("ordinary-operation-wal");
    std::fs::create_dir(&path).unwrap();
    let wal = FactWal::create(
        &path,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[
            (
                "profile",
                fact_value(
                    crate::source_main_active_service_wal::ORDINARY_VALIDITY_OPERATION_WAL_PROFILE,
                ),
            ),
            (
                "requiredNativeFactProfile",
                fact_value(ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
            ),
            (
                "actualNativeFactScope",
                fact_value(ACTUAL_VALIDITY_FACT_SCOPE),
            ),
            (
                "operationSourceScope",
                fact_value("RetainedOrdinaryValidityAction-1"),
            ),
        ]),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    SourceMainNativeValidityFactOperationV1::adopt(active, wal).unwrap()
}
fn issue(operation: &mut SourceMainNativeValidityFactOperationV1) {
    operation.prepare_intent().unwrap();
    operation.sync_intent().unwrap();
    operation.attempt_native().unwrap();
    operation.prepare_fact().unwrap();
    operation.sync_fact().unwrap();
}
fn accept(operation: &mut SourceMainNativeValidityFactOperationV1) {
    operation.prepare_accepted().unwrap();
    operation.sync_accepted().unwrap();
    operation.prepare_ack().unwrap();
    operation.sync_ack().unwrap();
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::Acknowledged
    );
}
#[test]
fn actual_ordinary_result_delivers_after_fact_then_accepts_once_without_resuming_source() {
    let temp = Temp::new();
    let mut operation = operation(&temp);
    let effects = operation.effects().to_vec();
    let financial = operation.financial_runtime().state_value().unwrap();
    let decode = operation
        .financial_runtime()
        .native_main_test_row_decode_count();
    let context = operation.pending_native_context();
    let before_head = operation.source_head().clone();
    let before = operation.index().snapshot().unwrap();
    assert!(operation.deliver_actual_result().is_err());
    assert!(operation.prepare_accepted().is_err());
    issue(&mut operation);
    assert_ne!(operation.index().snapshot().unwrap(), before);
    assert!(operation.source_result().is_none());
    assert_eq!(operation.source_head(), &before_head);
    operation.deliver_actual_result().unwrap();
    assert!(matches!(
        operation.source_result(),
        Some(NativeMainActiveServiceActionResultV1::Stop(
            NativeMainServiceStopV1::AwaitNativeWrite
        ))
    ));
    assert!(operation.deliver_actual_result().is_err());
    assert!(operation.attempt_native().is_err());
    accept(&mut operation);
    assert_eq!(operation.source_head().actions, before_head.actions + 1);
    assert_eq!(operation.native_attempts(), 1);
    assert_eq!(operation.pending_native_context(), context);
    assert_eq!(operation.effects(), effects);
    assert_eq!(
        operation.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(
        operation
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode
    );
    assert!(operation.prepare_intent().is_err());
    assert!(operation.prepare_accepted().is_err());
    assert!(operation.prepare_ack().is_err());
    assert_eq!(
        operation.boundary.wal.record().unwrap().get("stage"),
        fact_value("Ack")
    );
}
fn failed(temp: &Temp, lost_return: bool) -> (SourceMainNativeValidityFactOperationV1, Vec<V>, V) {
    let mut operation = operation(temp);
    let effects = operation.effects().to_vec();
    let financial = operation.financial_runtime().state_value().unwrap();
    if lost_return {
        operation.boundary.owner.owner.index.test_fail_after_sync();
    } else {
        operation
            .boundary
            .owner
            .owner
            .index
            .test_fail_after_partial_write();
    }
    issue(&mut operation);
    operation.deliver_actual_result().unwrap();
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::NativeRecoveryRequired
    );
    assert!(matches!(
        operation.source_result(),
        Some(NativeMainActiveServiceActionResultV1::Stop(
            NativeMainServiceStopV1::StorageStopped(_)
        ))
    ));
    assert!(operation.prepare_accepted().is_err());
    (operation, effects, financial)
}
#[test]
fn actual_post_reopen_keeps_first_error_and_never_reappends_before_accepted_ack() {
    let temp = Temp::new();
    let (operation, effects, financial) = failed(&temp, true);
    let original = operation.source_result().unwrap().clone();
    let decode = operation
        .financial_runtime()
        .native_main_test_row_decode_count();
    let no_index = operation.into_no_index().unwrap();
    let no_index = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::RejectIncompleteTail,
        )
        .err()
        .expect("wrong candidate retains whole no-index owner")
        .into_owner();
    assert_eq!(no_index.effects(), effects);
    let mut operation = no_index
        .try_reopen(&temp.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap();
    let post = operation.index().capture_verified_history_pin().unwrap();
    assert!(operation.attempt_replacement().is_err());
    operation.prepare_recovery_intent().unwrap();
    operation.sync_recovery_intent().unwrap();
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::ResultDelivered
    );
    assert!(operation.attempt_replacement().is_err());
    accept(&mut operation);
    assert_eq!(operation.native_attempts(), 1);
    assert_eq!(
        operation.index().capture_verified_history_pin().unwrap(),
        post
    );
    assert_eq!(operation.source_result(), Some(&original));
    assert_eq!(operation.effects(), effects);
    assert_eq!(
        operation.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(
        operation
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode
    );
}
#[test]
fn actual_before_mapping_sync_precedes_single_native_only_retry_and_preserves_source_error() {
    let temp = Temp::new();
    let (operation, effects, financial) = failed(&temp, false);
    let first = operation.source_result().unwrap().clone();
    let decode = operation
        .financial_runtime()
        .native_main_test_row_decode_count();
    let mut operation = operation
        .into_no_index()
        .unwrap()
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    let before = operation.index().capture_verified_history_pin().unwrap();
    assert!(operation.attempt_replacement().is_err());
    operation.prepare_recovery_intent().unwrap();
    operation
        .boundary
        .wal
        .set_test_fault(crate::source_main_active_service_wal::TestFault::AfterCommit);
    assert!(operation.sync_recovery_intent().is_err());
    assert_eq!(
        operation.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert!(!operation.state.recovery.as_ref().unwrap().installed);
    assert!(operation.attempt_replacement().is_err());
    assert_eq!(
        operation.resolve_control().unwrap(),
        crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::ReplacementIntentSynced
    );
    operation.attempt_replacement().unwrap();
    assert!(operation.attempt_replacement().is_err());
    operation.prepare_fact().unwrap();
    operation.sync_fact().unwrap();
    assert_eq!(operation.native_attempts(), 2);
    assert!(operation
        .boundary
        .attempt
        .as_ref()
        .unwrap()
        .actual_result()
        .is_ok());
    assert!(operation.state.prior_attempts[0].actual_result().is_err());
    assert_eq!(operation.source_result(), Some(&first));
    accept(&mut operation);
    assert_eq!(operation.source_result(), Some(&first));
    assert_eq!(operation.effects(), effects);
    assert_eq!(
        operation.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(
        operation
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode
    );
}
#[test]
fn second_real_lost_return_then_post_keeps_both_typed_results_without_source_success() {
    let temp = Temp::new();
    let (operation, effects, financial) = failed(&temp, false);
    let first = operation.source_result().unwrap().clone();
    let mut operation = operation
        .into_no_index()
        .unwrap()
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    operation.prepare_recovery_intent().unwrap();
    operation.sync_recovery_intent().unwrap();
    operation.boundary.owner.owner.index.test_fail_after_sync();
    operation.attempt_replacement().unwrap();
    operation.prepare_fact().unwrap();
    operation.sync_fact().unwrap();
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::NativeRecoveryRequired
    );
    assert_eq!(operation.source_result(), Some(&first));
    let mut operation = operation
        .into_no_index()
        .unwrap()
        .try_reopen(&temp.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap();
    let post = operation.index().capture_verified_history_pin().unwrap();
    operation.prepare_recovery_intent().unwrap();
    operation.sync_recovery_intent().unwrap();
    accept(&mut operation);
    assert_eq!(operation.native_attempts(), 2);
    assert_eq!(
        operation.index().capture_verified_history_pin().unwrap(),
        post
    );
    assert_eq!(operation.source_result(), Some(&first));
    assert_eq!(operation.effects(), effects);
    assert_eq!(
        operation.financial_runtime().state_value().unwrap(),
        financial
    );
}
#[test]
fn accepted_and_ack_lost_returns_reconcile_only_exact_control_post_without_reobservation() {
    let temp = Temp::new();
    let mut operation = operation(&temp);
    issue(&mut operation);
    operation.deliver_actual_result().unwrap();
    let effects = operation.effects().to_vec();
    operation.prepare_accepted().unwrap();
    let source_head = operation.source_head().clone();
    let native = operation.index().capture_verified_history_pin().unwrap();
    operation
        .boundary
        .wal
        .set_test_fault(crate::source_main_active_service_wal::TestFault::AfterCommit);
    assert!(operation.sync_accepted().is_err());
    assert!(operation.attempt_native().is_err());
    assert!(operation.prepare_accepted().is_err());
    assert_eq!(
        operation.resolve_control().unwrap(),
        crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::AcceptedSynced
    );
    operation.prepare_ack().unwrap();
    operation
        .boundary
        .wal
        .set_test_fault(crate::source_main_active_service_wal::TestFault::AfterCommit);
    assert!(operation.sync_ack().is_err());
    assert_eq!(
        operation.resolve_control().unwrap(),
        crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        operation.phase(),
        NativeValidityOperationPhaseV1::Acknowledged
    );
    assert_eq!(operation.source_head(), &source_head);
    assert_eq!(operation.native_attempts(), 1);
    assert_eq!(operation.effects(), effects);
    assert_eq!(
        operation.index().capture_verified_history_pin().unwrap(),
        native
    );
}
