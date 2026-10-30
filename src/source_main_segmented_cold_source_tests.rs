// Child of existing actual cold-source tests. Reuse raw reset/native/RPC
// constructors and the real c05 cold_open; observations never initialize source.
use super::*;
use crate::source_main_active_service_wal::VerifiedSegmentedActiveHistoryV1;
fn segmented_live(
    temp: &Temp,
) -> (
    SourceMainNativeSegmentedActiveServiceV1,
    V,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
) {
    let (owner, docs) = setup(temp, "detached-live-rpc-failure");
    let (archive, pin) = owner.checkpoint().unwrap();
    let tip = owner.index().capture_verified_history_pin().unwrap();
    let live = SourceMainNativeSegmentedActiveServiceV1::create(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    (live, docs, archive, pin, tip)
}
fn segmented_action(
    live: &mut SourceMainNativeSegmentedActiveServiceV1,
    action: NativeMainActiveServiceActionV1,
) {
    live.prepare_action(action).unwrap();
    assert_eq!(live.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
}
fn seal_segment(live: &mut SourceMainNativeSegmentedActiveServiceV1) {
    live.prepare_checkpoint().unwrap();
    live.attempt_checkpoint().unwrap();
    live.finish_checkpoint().unwrap();
}
#[test]
fn cold_more_than4096_raw_actions_reexecute_one_owner_across_exact_segment_boundaries() {
    let temp = cold_temp();
    let (mut live, docs, archive, pin, tip) = segmented_live(&temp);
    segmented_action(
        &mut live,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents: docs,
        },
    );
    for _ in 0..4097 {
        if live.history_window().len() == 256 {
            seal_segment(&mut live)
        }
        segmented_action(&mut live, NativeMainActiveServiceActionV1::Pause);
    }
    let expected = live.financial_runtime().state_value().unwrap();
    let source_head = live.active_service().unwrap().head();
    let current = live.index().capture_verified_history_pin().unwrap();
    let captured = live.capture_acknowledged_history().unwrap();
    let head = live.head();
    drop(live);
    let history = VerifiedSegmentedActiveHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &captured,
    )
    .unwrap();
    let prefix = cold_open(&temp, &current, &archive, pin, tip);
    let io = cold_io(prefix.index());
    let cold =
        NativeMainColdSegmentedServiceV1::reexecute(prefix, history, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), 4098);
    assert_eq!(cold.verified_segments(), 16);
    assert_eq!(cold.acknowledged_wal_head(), head);
    assert_eq!(cold.state.head, source_head);
    assert_eq!(cold.financial_runtime().state_value().unwrap(), expected);
    assert_eq!(cold_io(cold.index()), io);
    assert_eq!(
        cold.index().capture_verified_history_pin().unwrap(),
        current
    );
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &current
    )
    .is_err());
}
#[test]
fn cold_nonempty_financial_native_prefix_crosses_checkpoints_around_same_append_resolve() {
    let temp = cold_temp();
    let (mut live, docs, archive, pin, tip) = segmented_live(&temp);
    segmented_action(
        &mut live,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents: docs,
        },
    );
    for _ in 0..12 {
        segmented_action(
            &mut live,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        if live.active_service().unwrap().pending_write().is_some() {
            break;
        }
    }
    assert!(live.active_service().unwrap().pending_write().is_some());
    assert!(!live.financial_runtime().collection("tallyMap").is_empty());
    seal_segment(&mut live);
    segmented_action(&mut live, NativeMainActiveServiceActionV1::AppendPending);
    seal_segment(&mut live);
    segmented_action(&mut live, NativeMainActiveServiceActionV1::ResolvePost);
    segmented_action(&mut live, NativeMainActiveServiceActionV1::Shutdown(vec![]));
    let expected = live.financial_runtime().state_value().unwrap();
    let current = live.index().capture_verified_history_pin().unwrap();
    assert_ne!(current, tip);
    let count = live.acknowledged_actions();
    let head = live.head();
    let captured = live.capture_acknowledged_history().unwrap();
    drop(live);
    let history = VerifiedSegmentedActiveHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &captured,
    )
    .unwrap();
    let prefix = cold_open(&temp, &current, &archive, pin, tip);
    let io = cold_io(prefix.index());
    let mut cold =
        NativeMainColdSegmentedServiceV1::reexecute(prefix, history, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), count);
    assert_eq!(cold.verified_segments(), 2);
    assert_eq!(cold.acknowledged_wal_head(), head);
    assert_eq!(cold.financial_runtime().state_value().unwrap(), expected);
    assert_eq!(cold_io(cold.index()), io);
    let raw = obj(&[
        ("_id", s("new-raw-cold-segment-guard-probe")),
        ("value", s("fresh raw input")),
    ]);
    assert!(matches!(
        cold.index.import_row(&current.metadata, raw),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(
        cold.index().capture_verified_history_pin().unwrap(),
        current
    );
    assert_eq!(cold_io(cold.index()), io);
}
#[test]
fn segmented_ordinary_reader_refuses_real_native_ambiguity_without_promoting_comparison_text() {
    use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
    let temp = cold_temp();
    let (mut live, docs, _, _, _) = segmented_live(&temp);
    segmented_action(
        &mut live,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents: docs,
        },
    );
    for _ in 0..12 {
        segmented_action(
            &mut live,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        if live.active_service().unwrap().pending_write().is_some() {
            break;
        }
    }
    seal_segment(&mut live);
    let Some(ActiveWalOwner::Active(service)) = live.controller.owner.as_mut() else {
        panic!("actual source owner")
    };
    service.owner.index.test_fail_after_sync();
    live.prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        live.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let no_index = match live.into_no_native_index() {
        Ok(o) => o,
        Err(e) => panic!("{e:?}"),
    };
    let mut live = no_index
        .try_reopen(
            &temp.root,
            Candidate::Post,
            crate::storage::Recovery::RejectIncompleteTail,
        )
        .unwrap();
    live.advance().unwrap();
    let captured = live.capture_acknowledged_history().unwrap();
    drop(live);
    assert!(VerifiedSegmentedActiveHistoryV1::open(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &captured
    )
    .is_err());
}
