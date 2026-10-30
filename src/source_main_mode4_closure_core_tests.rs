// Nested in the existing actual-source Mode4 controller tests. Private test-only
// moves retain the same financial/index/frame owner while releasing its WAL
// writer to the distinct readonly verifier; no production escape is added.
use super::*;
use crate::source_compact_index_v2::{
    NativeHistoricalInlineDatabaseV1, VerifiedOriginalMetadataViewV1,
};
use crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1;
fn actual_source_operation(
    owner: &mut SourceMainNativeDurableActiveServiceV1,
) -> NativeHistoricalInlineDatabaseV1 {
    let write = owner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap();
    let method = write.method().to_owned();
    let arguments = write.arguments().to_vec();
    let before = owner.index().capture_verified_history_pin().unwrap();
    let active = active_mut(owner);
    let view =
        VerifiedOriginalMetadataViewV1::open_original(&mut active.owner.index, &before).unwrap();
    view.prepare_historical_inline_database(&method, arguments)
        .unwrap()
}
fn verify_same_actual_owner(
    mut owner: SourceMainNativeDurableActiveServiceV1,
    t: &Temp,
    mut source: NativeHistoricalInlineDatabaseV1,
) {
    let independent = owner.capture_inline_cold_pin().unwrap();
    let sequence = owner.history().last().unwrap().sequence;
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = rpc_count(&owner);
    let context = owner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap()
        .durable_read_context();
    let ActiveWalOwner::Active(mut same) = owner.owner.take().expect("actual retained owner")
    else {
        panic!("actual active source");
    };
    drop(owner);
    let reader = VerifiedInlineActiveServiceAckHistoryV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &independent,
    )
    .unwrap();
    let pin = same.owner.index.capture_verified_history_pin().unwrap();
    let closure = reader
        .verify_inline_operation(&mut same.owner.index, sequence, &mut source)
        .unwrap();
    assert_eq!(closure.sequence(), sequence);
    assert!(source.append_verified());
    assert!(!source.resolved());
    assert_eq!(same.effects(), effects);
    assert_eq!(
        same.pending_inline_write().unwrap().durable_read_context(),
        context
    );
    assert_eq!(
        same.owner.index.capture_verified_history_pin().unwrap(),
        pin
    );
    let rpc = same
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("rpc-data"))
        .count();
    assert_eq!(rpc, count);
    assert!(same
        .frame
        .as_ref()
        .unwrap()
        .inline_pending_write()
        .is_some());
}
#[test]
fn mode4_actual_source_ack_closure_keeps_finance_and_native_return_distinct() {
    let t = temp();
    let mut owner = ordinary(&t);
    let source = actual_source_operation(&mut owner);
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    verify_same_actual_owner(owner, &t, source);
}
#[test]
fn mode4_actual_source_lost_native_post_closure_retains_same_source_prefix() {
    let t = temp();
    let mut owner = ordinary(&t);
    let source = actual_source_operation(&mut owner);
    fail_native(&mut owner, true);
    let no_index = owner.into_no_inline_index().unwrap();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap();
    owner.advance().unwrap();
    verify_same_actual_owner(owner, &t, source);
}
#[test]
fn mode4_actual_source_before_replacement_closure_preserves_same_source_await() {
    let t = temp();
    let mut owner = ordinary(&t);
    let source = actual_source_operation(&mut owner);
    fail_native(&mut owner, false);
    let no_index = owner.into_no_inline_index().unwrap();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap();
    owner.advance().unwrap();
    verify_same_actual_owner(owner, &t, source);
}

fn actual_compacting_inline(t: &Temp) -> SourceMainNativeDurableActiveServiceV1 {
    let calibration = temp();
    let (raw, docs, prime) = setup_primed(&calibration, None);
    let mut probe = wrap(raw, docs, &calibration);
    first_inline(&mut probe);
    let at = probe.index().footprint().unwrap().journal_file_bytes;
    run_action(&mut probe, NativeMainActiveServiceActionV1::AppendPending);
    let bytes = probe.index().footprint().unwrap().journal_file_bytes - at;
    let offset = at - prime + bytes - 1;
    drop(probe);
    let (raw, docs, real_prime) = setup_primed(t, Some(offset));
    assert_eq!(real_prime, prime);
    let mut owner = wrap(raw, docs, t);
    first_inline(&mut owner);
    assert!(owner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap()
        .append_plan()
        .unwrap()
        .intermediate()
        .is_some());
    owner
}
fn actual_rejected_reappend(recovery: Option<Candidate>, pause: bool) {
    let t = temp();
    let mut owner = if matches!(recovery, Some(Candidate::Intermediate)) {
        actual_compacting_inline(&t)
    } else {
        ordinary(&t)
    };
    let mut first_source = actual_source_operation(&mut owner);
    let mut rejected_source = actual_source_operation(&mut owner);
    if let Some(candidate) = recovery {
        if candidate == Candidate::Intermediate {
            active_mut(&mut owner)
                .owner
                .index
                .native_main_test_compaction_fault("after-current-sync");
            owner
                .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
                .unwrap();
            assert!(matches!(
                owner.advance(),
                Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
            ));
        } else {
            fail_native(&mut owner, candidate == Candidate::Post);
        }
        let without = owner.into_no_inline_index().unwrap();
        owner = without
            .try_reopen(
                &t.root,
                candidate,
                if candidate == Candidate::Before {
                    Recovery::DiscardIncompleteTail
                } else {
                    Recovery::RejectIncompleteTail
                },
            )
            .unwrap();
        owner.advance().unwrap();
    } else {
        run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    }
    let first_sequence = owner.history().last().unwrap().sequence;
    if pause {
        run_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    }
    let source_before = owner.observation().unwrap();
    let native_before = owner.index().capture_verified_history_pin().unwrap();
    let effects_before = owner.active_service().unwrap().effects().to_vec();
    let calls_before = rpc_count(&owner);
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let rejected = owner.history().last().unwrap();
    let rejected_sequence = rejected.sequence;
    let accepted = owner.read_history_blob(&rejected.accepted).unwrap();
    let fields_len =
        usize::try_from(u64::from_be_bytes(accepted[6..14].try_into().unwrap())).unwrap();
    assert_eq!(&accepted[14..14 + fields_len], source_before);
    assert_eq!(
        &accepted[22 + fields_len..],
        active_wal_bytes(&obj(&[("apiRejection", s("WriteAlreadyAttempted"))])).unwrap()
    );
    assert_eq!(owner.observation().unwrap(), source_before);
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        native_before
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects_before);
    assert_eq!(rpc_count(&owner), calls_before);
    let independent = owner.capture_inline_cold_pin().unwrap();
    let ActiveWalOwner::Active(mut same) = owner.owner.take().unwrap() else {
        panic!("same actual active owner");
    };
    drop(owner);
    let reader = VerifiedInlineActiveServiceAckHistoryV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &independent,
    )
    .unwrap();
    reader
        .verify_inline_operation(&mut same.owner.index, first_sequence, &mut first_source)
        .unwrap();
    assert!(first_source.append_verified());
    assert!(!first_source.resolved());
    assert!(reader
        .verify_inline_operation(
            &mut same.owner.index,
            rejected_sequence,
            &mut rejected_source
        )
        .is_err());
    assert!(!rejected_source.append_verified());
    assert!(!rejected_source.resolved());
    assert_eq!(same.effects(), effects_before);
    assert_eq!(
        same.owner.index.capture_verified_history_pin().unwrap(),
        native_before
    );
    assert!(same.pending_inline_write().is_some());
}

#[test]
fn mode4_actual_rejected_second_append_preserves_ack_history_without_second_publication() {
    actual_rejected_reappend(None, false);
}
#[test]
fn mode4_actual_rejected_append_after_pause_keeps_original_ack_relation() {
    actual_rejected_reappend(None, true);
}
#[test]
fn mode4_actual_rejected_append_after_before_recovery_keeps_replacement_relation() {
    actual_rejected_reappend(Some(Candidate::Before), false);
}
#[test]
fn mode4_actual_rejected_append_after_lost_post_keeps_retained_source_failure() {
    actual_rejected_reappend(Some(Candidate::Post), false);
}
#[test]
fn mode4_actual_rejected_append_after_intermediate_recovery_keeps_real_escrow() {
    actual_rejected_reappend(Some(Candidate::Intermediate), false);
}
