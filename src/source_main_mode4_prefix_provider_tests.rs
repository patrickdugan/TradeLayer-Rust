// These are Parent-Core gates; the standalone metadata suite cannot execute
// private Runtime source reconstruction and is not presented as such evidence.
use super::*;
#[test]
fn mode4_prefix_rebuild_routes_origin_endpoint_and_playback_through_checked_views() {
    let t = temp();
    let (raw, docs) = setup(&t, "same-held-live-validity");
    let original_financial = raw.financial_runtime().state_value().unwrap();
    let completed = raw.index.capture_verified_history_pin().unwrap();
    let (archive, archive_pin) = raw.checkpoint().unwrap();
    assert_eq!(archive_pin.head.commands, 0);
    let mut owner = wrap(raw, docs, &t);
    first_inline(&mut owner);
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let final_post = owner.index().capture_verified_history_pin().unwrap();
    let independent = owner.capture_inline_cold_pin().unwrap();
    let ActiveWalOwner::Active(same) = owner.owner.take().unwrap() else {
        panic!("same real financial owner")
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
                current: completed.clone(),
                commands: vec![],
            },
            actual_post: final_post.clone(),
        },
        index,
        &reader,
    )
    .unwrap();
    assert_eq!(
        prefix.financial_runtime().state_value().unwrap(),
        original_financial
    );
    assert_eq!(prefix.completed_tip(), &completed);
    assert_eq!(prefix.actual_post(), &final_post);
    assert!(prefix.completed_prefix().is_empty());
    let after = prefix.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    assert!(SourceCompactIndex::open_verified_history_owner(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &final_post
    )
    .is_err());
}

#[test]
fn mode4_raw_actions_recompute_financial_source_through_first_real_inline_append() {
    let t = temp();
    let (raw, docs) = setup(&t, "same-held-live-validity");
    let completed = raw.index.capture_verified_history_pin().unwrap();
    let (archive, archive_pin) = raw.checkpoint().unwrap();
    let mut owner = wrap(raw, docs, &t);
    first_inline(&mut owner);
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let expected_state = owner
        .active_service()
        .unwrap()
        .financial_runtime()
        .state_value()
        .unwrap();
    let expected_effects = owner.active_service().unwrap().effects().to_vec();
    let expected_receipts = owner.active_service().unwrap().history().to_vec();
    let expected_actions = owner.history().len();
    let independent = owner.capture_inline_cold_pin().unwrap();
    let post = owner.index().capture_verified_history_pin().unwrap();
    let ActiveWalOwner::Active(same) = owner.owner.take().unwrap() else {
        panic!("actual source owner")
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
    assert_eq!(cold.verified_actions(), expected_actions);
    assert_eq!(cold.verified_inline_operations(), 1);
    assert_eq!(
        cold.financial_runtime().state_value().unwrap(),
        expected_state
    );
    assert_eq!(cold.state.effects(), expected_effects);
    assert_eq!(cold.original_action_receipts(), expected_receipts);
    assert_eq!(cold.actual_post(), &post);
    assert!(cold
        .state
        .frame
        .as_ref()
        .unwrap()
        .pending_mode4_historical_context()
        .is_some());
    let after = cold.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    assert!(SourceCompactIndex::open_verified_history_owner(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &post
    )
    .is_err());
}

#[path = "source_main_mode4_resolve_tests.rs"]
mod mode4_resolve_followon_tests;
