// Child of the actual durable-service tests: literal source reset/native/RPC
// inputs and actual owner/WAL APIs; observations are assertions only.
use super::*;
use crate::source_compact_index_v2::{CompactIndexError, SourceIndexHistoryPinV1};
use crate::source_main_active_service_wal::VerifiedActiveServiceAckHistoryV1;

fn cold_temp() -> Temp {
    let parent = std::env::temp_dir().canonicalize().unwrap();
    let root = parent.join(format!(
        "cold-source-active-reexecution-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    Temp { root, parent }
}
fn cold_open(
    temp: &Temp,
    current: &SourceIndexHistoryPinV1,
    archive: &[u8],
    pin: NativeMainArchivePin,
    tip: SourceIndexHistoryPinV1,
) -> NativeMainColdPrefixV1 {
    let index = SourceCompactIndex::open_verified_history_owner(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        current,
    )
    .unwrap();
    NativeMainColdPrefixV1::reexecute(
        archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: pin,
                current: tip,
                commands: vec![],
            },
            actual_post: current.clone(),
        },
        index,
    )
    .unwrap()
}
fn cold_io(index: &SourceCompactIndex) -> (u64, u64, u64) {
    let io = index.storage_io_counters().unwrap();
    (
        io.confirmed_append_bytes,
        io.copied_checkpoint_bytes,
        io.published_compactions,
    )
}
#[test]
fn cold_reexecutes_entire_actual_start_pause_ack_history_on_one_original_post_owner() {
    let temp = cold_temp();
    let (owner, docs) = setup(&temp, "detached-live-rpc-failure");
    let (archive, pin) = owner.checkpoint().unwrap();
    let tip = owner.index().capture_verified_history_pin().unwrap();
    let mut live = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut live, docs);
    run_action(&mut live, NativeMainActiveServiceActionV1::Pause);
    let state = live.financial_runtime().state_value().unwrap();
    let receipts = live.active_service().unwrap().history().to_vec();
    let current = live.index().capture_verified_history_pin().unwrap();
    let wal = live.wal_pin().unwrap();
    let head = live.head();
    drop(live);
    let transcript = VerifiedActiveServiceAckHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &wal,
    )
    .unwrap();
    let prefix = cold_open(&temp, &current, &archive, pin, tip);
    let io = cold_io(prefix.index());
    let cold = NativeMainColdAcknowledgedServiceV1::reexecute(prefix, transcript, 64 * 1024 * 1024)
        .unwrap();
    assert_eq!(cold.financial_runtime().state_value().unwrap(), state);
    assert_eq!(cold.original_action_receipts(), receipts.as_slice());
    assert_eq!(cold.verified_actions(), 2);
    assert_eq!(cold.acknowledged_wal_head(), head);
    assert_eq!(
        cold.index().capture_verified_history_pin().unwrap(),
        current
    );
    assert_eq!(cold_io(cold.index()), io);
}
#[test]
fn cold_reexecutes_successful_native_append_resolve_same_held_pause_timer_and_shutdown() {
    for pause in [false, true] {
        let temp = cold_temp();
        let (owner, docs) = setup(&temp, "detached-live-rpc-failure");
        let (archive, pin) = owner.checkpoint().unwrap();
        let tip = owner.index().capture_verified_history_pin().unwrap();
        let mut live = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
            owner,
            &temp.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
            64 * 1024 * 1024,
        )
        .unwrap();
        wal_start(&mut live, docs);
        for _ in 0..12 {
            run_action(
                &mut live,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            );
            if live.active_service().unwrap().pending_write().is_some() {
                break;
            }
        }
        assert!(live.active_service().unwrap().pending_write().is_some());
        assert!(
            live.active_service().unwrap().caller_result().is_none(),
            "genuine catchup await before outer return"
        );
        if pause {
            run_action(&mut live, NativeMainActiveServiceActionV1::Pause);
        }
        run_action(&mut live, NativeMainActiveServiceActionV1::AppendPending);
        run_action(&mut live, NativeMainActiveServiceActionV1::ResolvePost);
        if let Some(id) = live.active_service().unwrap().next_timer() {
            run_action(
                &mut live,
                NativeMainActiveServiceActionV1::FireTimer { id, generation: 71 },
            );
            run_action(
                &mut live,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            );
        }
        run_action(&mut live, NativeMainActiveServiceActionV1::Shutdown(vec![]));
        let state = live.financial_runtime().state_value().unwrap();
        let receipts = live.active_service().unwrap().history().to_vec();
        let current = live.index().capture_verified_history_pin().unwrap();
        assert_ne!(tip, current);
        let wal = live.wal_pin().unwrap();
        let head = live.head();
        let count = live.history().len();
        drop(live);
        let transcript = VerifiedActiveServiceAckHistoryV1::open(
            &temp.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &wal,
        )
        .unwrap();
        assert!(transcript
            .entries()
            .iter()
            .any(|entry| entry.native_mutations().is_some()));
        let prefix = cold_open(&temp, &current, &archive, pin, tip);
        let io = cold_io(prefix.index());
        let mut cold =
            NativeMainColdAcknowledgedServiceV1::reexecute(prefix, transcript, 64 * 1024 * 1024)
                .unwrap();
        assert_eq!(cold.financial_runtime().state_value().unwrap(), state);
        assert_eq!(cold.original_action_receipts(), receipts.as_slice());
        assert_eq!(cold.verified_actions(), count);
        assert_eq!(cold.acknowledged_wal_head(), head);
        assert_eq!(cold.actual_post(), &current);
        assert_eq!(
            cold.index().capture_verified_history_pin().unwrap(),
            current
        );
        assert_eq!(cold_io(cold.index()), io);
        let fresh = obj(&[
            ("_id", s("raw-cold-guard-probe")),
            ("value", s("new raw input")),
        ]);
        assert!(matches!(
            cold.index.import_row(&current.metadata, fresh),
            Err(CompactIndexError::ColdWriteRefused)
        ));
        assert_eq!(cold_io(cold.index()), io);
        assert!(SourceCompactIndex::open_verified_history_owner(
            &temp.root,
            Namespace::new("LTC", false, "txIndex").unwrap(),
            &current
        )
        .is_err());
    }
}
#[test]
fn cold_wrong_raw_observation_limit_retains_actual_executed_source_prefix_and_both_locks() {
    let temp = cold_temp();
    let (owner, docs) = setup(&temp, "detached-live-rpc-failure");
    let (archive, pin) = owner.checkpoint().unwrap();
    let tip = owner.index().capture_verified_history_pin().unwrap();
    let mut live = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut live, docs);
    let actual_start_state = live.financial_runtime().state_value().unwrap();
    let wal = live.wal_pin().unwrap();
    let current = live.index().capture_verified_history_pin().unwrap();
    drop(live);
    let transcript = VerifiedActiveServiceAckHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &wal,
    )
    .unwrap();
    let prefix = cold_open(&temp, &current, &archive, pin, tip);
    let io = cold_io(prefix.index());
    let rejected =
        NativeMainColdAcknowledgedServiceV1::reexecute(prefix, transcript, 32 * 1024 * 1024)
            .unwrap_err();
    assert_eq!(rejected.cause, "actual source Accepted mismatch at 0");
    assert_eq!(rejected.verified_actions(), 0);
    assert_eq!(
        rejected.financial_runtime().state_value().unwrap(),
        actual_start_state,
        "actual executed bootstrap prefix remains retained"
    );
    assert!(rejected.owner.state.started);
    assert!(rejected.owner.state.frame.is_some());
    assert_eq!(
        rejected.index().capture_verified_history_pin().unwrap(),
        current
    );
    assert_eq!(cold_io(rejected.index()), io);
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &current
    )
    .is_err());
    assert!(VerifiedActiveServiceAckHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &wal
    )
    .is_err());
}
