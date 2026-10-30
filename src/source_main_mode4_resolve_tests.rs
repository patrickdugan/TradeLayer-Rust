// Parent-Core-only extension of fd64. No production code or expected financial
// data is imported. Complete live results are assertions against raw replay.
use super::*;

#[test]
fn mode4_resolves_same_source_twice_and_recomputes_three_successive_native_plans() {
    let t = temp();
    let (raw, docs) = setup(&t, "same-held-live-validity");
    let completed = raw.index.capture_verified_history_pin().unwrap();
    let (archive, archive_pin) = raw.checkpoint().unwrap();
    let mut owner = wrap(raw, docs, &t);
    first_inline(&mut owner);
    assert!(
        owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .unwrap()
            .arguments()[0]
            .get("_id")
            != s("MaxHeight")
    );
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .unwrap()
            .arguments()[0]
            .get("_id"),
        s("MaxHeight")
    );
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .unwrap()
            .arguments()[0]
            .get("_id"),
        s("indexExists")
    );
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    // Source return remains held after real third metadata Post; no automatic
    // source completion, next live row or new invocation is manufactured.
    let expected_state = owner
        .active_service()
        .unwrap()
        .financial_runtime()
        .state_value()
        .unwrap();
    let expected_effects = owner.active_service().unwrap().effects().to_vec();
    let expected_receipts = owner.active_service().unwrap().history().to_vec();
    let count = owner.history().len();
    let independent = owner.capture_inline_cold_pin().unwrap();
    let post = owner.index().capture_verified_history_pin().unwrap();
    let ActiveWalOwner::Active(same) = owner.owner.take().unwrap() else {
        panic!("actual same owner")
    };
    let index = same.owner.index;
    drop(owner);
    let reader =
        crate::source_main_active_service_wal::VerifiedInlineActiveServiceAckHistoryV1::open(
            &t.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &independent,
        )
        .unwrap();
    let io = index.storage_io_counters().unwrap();
    let prefix = NativeMainColdPrefixV1::reexecute_with_mode4_views(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: archive_pin,
                current: completed,
                commands: vec![],
            },
            actual_post: post.clone(),
        },
        index,
        &reader,
    )
    .unwrap();
    let cold = NativeMainColdMode4ServiceV1::reexecute(prefix, reader, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), count);
    assert_eq!(cold.verified_inline_operations(), 3);
    assert_eq!(
        cold.financial_runtime().state_value().unwrap(),
        expected_state
    );
    assert_eq!(cold.state.effects(), expected_effects);
    assert_eq!(cold.original_action_receipts(), expected_receipts);
    assert!(cold
        .state
        .frame
        .as_ref()
        .unwrap()
        .pending_mode4_historical_context()
        .is_some());
    assert_eq!(cold.actual_post(), &post);
    let after = cold.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
}
