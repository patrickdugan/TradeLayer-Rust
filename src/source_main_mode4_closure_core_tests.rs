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
