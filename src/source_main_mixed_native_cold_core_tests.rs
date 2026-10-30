// Child of actual-fact controller tests: raw genuine source inputs and actual
// financial/native owners only. Before/Accepted never initialize this actor.
use super::*;
use crate::source_main_active_service_wal::VerifiedMixedNativeHistoryV1;
fn mixed_actor_action(
    owner: &mut SourceMainNativeMixedFactServiceV1,
    action: NativeMainActiveServiceActionV1,
) {
    owner.prepare_action(action).unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
}
fn mixed_actor_setup(
    t: &Temp,
) -> (
    SourceMainNativeMixedFactServiceV1,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
) {
    let (raw, documents) = setup(t, "same-held-live-validity");
    let tip = raw.index().capture_verified_history_pin().unwrap();
    let (archive, pin) = raw.checkpoint().unwrap();
    let escrow = t.root.join("mixed-cold-actor-generation-escrow");
    std::fs::create_dir(&escrow).unwrap();
    let mut owner = SourceMainNativeMixedFactServiceV1::create(
        raw,
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        escrow.canonicalize().unwrap(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents,
        },
    );
    for _ in 0..40 {
        if owner.active_service().unwrap().pending_write().is_some() {
            return (owner, archive, pin, tip);
        }
        if owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .is_some()
        {
            mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        } else {
            mixed_actor_action(
                &mut owner,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            );
        }
    }
    panic!("genuine source ordinary await absent")
}
fn mixed_cold(
    mut owner: SourceMainNativeMixedFactServiceV1,
    t: &Temp,
    archive: Vec<u8>,
    pin: NativeMainArchivePin,
    tip: SourceIndexHistoryPinV1,
) -> Result<NativeMainColdMixedFactServiceV1, Box<RejectedNativeMainColdMixedFactServiceV1>> {
    let captured = owner.capture_cold_history_pin().unwrap();
    let mut actual = owner.inner.unwrap();
    let ActiveWalOwner::Active(active) = actual.inner.owner.take().unwrap() else {
        panic!("actual financial owner")
    };
    let index = active.owner.index;
    drop(actual);
    let reader = VerifiedMixedNativeHistoryV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &captured,
    )
    .unwrap();
    let prefix = NativeMainColdPrefixV1::reexecute(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: pin,
                current: tip,
                commands: vec![],
            },
            actual_post: captured.native_post().clone(),
        },
        index,
    )
    .unwrap();
    NativeMainColdMixedFactServiceV1::reexecute(prefix, reader, 64 * 1024 * 1024)
}
#[test]
fn mixed_cold_actual_inline_then_ordinary_success_keeps_finance_source_head_and_unresolved_post() {
    let t = temp();
    let (mut owner, archive, pin, tip) = mixed_actor_setup(&t);
    assert!(!owner.financial_runtime().collection("tallyMap").is_empty());
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let state = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let kernel = owner.active_service().unwrap().kernel_value();
    let head = owner.active_service().unwrap().head();
    let count = owner.acknowledged_actions();
    let post = owner.index().capture_verified_history_pin().unwrap();
    let io = owner.index().storage_io_counters().unwrap();
    let cold = mixed_cold(owner, &t, archive, pin, tip).unwrap();
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
    assert!(cold
        .state
        .frame
        .as_ref()
        .unwrap()
        .mixed_historical_validity()
        .unwrap()
        .append_verified());
    assert!(!cold
        .state
        .frame
        .as_ref()
        .unwrap()
        .mixed_historical_validity()
        .unwrap()
        .resolved());
}
#[test]
fn mixed_cold_ordinary_separate_resolve_and_segment_keep_same_actual_source_receipts() {
    let t = temp();
    let (mut owner, archive, pin, tip) = mixed_actor_setup(&t);
    owner.prepare_checkpoint().unwrap();
    owner.attempt_checkpoint().unwrap();
    owner.finish_checkpoint().unwrap();
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    let state = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let head = owner.active_service().unwrap().head();
    let count = owner.acknowledged_actions();
    let cold = mixed_cold(owner, &t, archive, pin, tip).unwrap();
    assert_eq!(cold.verified_actions(), count);
    assert_eq!(cold.verified_segments(), 1);
    assert_eq!(cold.state.head, head);
    assert_eq!(cold.financial_runtime().state_value().unwrap(), state);
    assert_eq!(cold.state.effects(), effects);
}
#[test]
fn mixed_cold_unwritten_final_ordinary_await_is_refused_with_real_financial_prefix_retained() {
    let t = temp();
    let live_decode_before = crate::source_contract_runtime::native_main_test_row_decode_total();
    let (owner, archive, pin, tip) = mixed_actor_setup(&t);
    let state = owner.financial_runtime().state_value().unwrap();
    let post = owner.index().capture_verified_history_pin().unwrap();
    // These are independently captured live comparisons, never cold inputs.
    let count = owner.acknowledged_actions();
    assert!(count > 0);
    assert!(owner.active_service().unwrap().pending_write().is_some());
    let effects = owner.active_service().unwrap().effects().to_vec();
    let live_decodes = owner
        .financial_runtime()
        .native_main_test_row_decode_count()
        .checked_sub(live_decode_before)
        .expect("live decode counter must be monotonic");
    assert_eq!(
        live_decodes, 1,
        "this raw source scene contains one actual Main0 row"
    );
    let cold_decode_before = crate::source_contract_runtime::native_main_test_row_decode_total();
    let rejected = match mixed_cold(owner, &t, archive, pin, tip) {
        Err(rejected) => rejected,
        Ok(_) => panic!("unwritten native Post must not be manufactured"),
    };
    assert_eq!(rejected.financial_runtime().state_value().unwrap(), state);
    let mut endpoint_fields = Vec::new();
    if let Some(frame) = rejected.owner.state.frame.as_ref() {
        frame
            .visit_canonical_fields(&mut |path, value| {
                if path.ends_with("/row/pc")
                    || path.ends_with("/row/validityStarted")
                    || path.ends_with("/row/ambiguous")
                    || path.ends_with("/row/write/profile")
                {
                    endpoint_fields.push((path.to_owned(), value.clone()));
                }
                Ok(())
            })
            .unwrap();
    }
    let historical_endpoint = rejected
        .owner
        .state
        .frame
        .as_ref()
        .and_then(|frame| frame.mixed_historical_validity())
        .map(|awaiting| (awaiting.append_verified(), awaiting.resolved()));
    assert_eq!(rejected.verified_actions(), count - 1,
        "cause={} acknowledgedInput={} sourceActions={} historicalEndpoint={historical_endpoint:?} row={endpoint_fields:?}",
        rejected.cause, count, rejected.owner.state.head.actions);
    assert_eq!(rejected.owner.state.effects(), effects);
    assert_eq!(
        rejected
            .financial_runtime()
            .native_main_test_row_decode_count()
            .checked_sub(cold_decode_before)
            .expect("cold decode counter must be monotonic"),
        live_decodes
    );
    assert_eq!(
        rejected.cause,
        format!("actual mixed source verification failed at {count}: StaleSnapshot")
    );
    let mut fields = vec![];
    rejected
        .owner
        .state
        .frame
        .as_ref()
        .unwrap()
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    let pc = fields
        .iter()
        .find_map(|(path, value)| {
            (path.ends_with("/row/pc") && *value == s("PrimaryValidity"))
                .then(|| path.strip_suffix("pc").unwrap().to_owned())
        })
        .expect("actual source reached the final ordinary validity PC");
    assert!(fields.contains(&(format!("{pc}validityStarted"), V::Bool(true))));
    assert!(fields.contains(&(format!("{pc}ambiguous"), s("StaleSnapshot"))));
    assert!(fields.contains(&(format!("{pc}write"), V::Null)));
    assert!(rejected
        .owner
        .state
        .frame
        .as_ref()
        .unwrap()
        .mixed_historical_validity()
        .is_none());
    assert_eq!(
        rejected.index().capture_verified_history_pin().unwrap(),
        post
    );
    assert!(
        rejected.cause.contains("verification")
            || rejected.cause.contains("Before")
            || rejected.cause.contains("Accepted"),
        "{}",
        rejected.cause
    );
}

#[path = "source_main_mixed_native_recovery_cold_core_tests.rs"]
mod mixed_native_recovery_cold_core_tests;

#[path = "source_main_mixed_native_intermediate_cold_tests.rs"]
mod mixed_native_intermediate_cold_tests;
