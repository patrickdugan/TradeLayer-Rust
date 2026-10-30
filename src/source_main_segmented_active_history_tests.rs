// Child of actual durable source tests: raw source fixtures/setup are reused.
// No processed/valid/settlement/financial values are supplied as inputs.
use super::*;
fn segmented(temp: &Temp, mode: &str) -> (SourceMainNativeSegmentedActiveServiceV1, V) {
    let (owner, docs) = setup(temp, mode);
    let owner = SourceMainNativeSegmentedActiveServiceV1::create(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    (owner, docs)
}
fn action(
    owner: &mut SourceMainNativeSegmentedActiveServiceV1,
    action: NativeMainActiveServiceActionV1,
) {
    let sequence = owner.acknowledged_actions() + 1;
    owner.prepare_action(action).unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.acknowledged_actions(), sequence);
    assert_eq!(owner.history_window().last().unwrap().sequence, sequence);
}
fn checkpoint(owner: &mut SourceMainNativeSegmentedActiveServiceV1) {
    let source_head = owner.active_service().map(|s| s.head());
    let financial = owner.financial_runtime() as *const SourceContractRuntime;
    let index = owner.index() as *const SourceCompactIndex;
    let metadata = owner.index().snapshot().unwrap();
    let native = owner
        .active_service()
        .and_then(|s| s.pending_write())
        .map(|p| p as *const _);
    let context = owner
        .active_service()
        .and_then(|s| s.pending_native_context());
    let effects = owner.active_service().map(|s| s.effects().to_vec());
    let count = owner.acknowledged_actions();
    let head = owner.head();
    owner.prepare_checkpoint().unwrap();
    assert!(matches!(
        owner.prepare_action(NativeMainActiveServiceActionV1::Pause),
        Err(NativeMainActiveWalErrorV1::Busy)
    ));
    owner.attempt_checkpoint().unwrap();
    owner.finish_checkpoint().unwrap();
    assert_eq!(owner.acknowledged_actions(), count);
    assert_eq!(owner.head(), head);
    assert!(owner.history_window().is_empty());
    assert_eq!(owner.active_service().map(|s| s.head()), source_head);
    assert_eq!(owner.financial_runtime() as *const _, financial);
    assert_eq!(owner.index() as *const _, index);
    assert_eq!(owner.index().snapshot().unwrap(), metadata);
    assert_eq!(
        owner
            .active_service()
            .and_then(|s| s.pending_write())
            .map(|p| p as *const _),
        native
    );
    assert_eq!(
        owner
            .active_service()
            .and_then(|s| s.pending_native_context()),
        context
    );
    assert_eq!(
        owner.active_service().map(|s| s.effects().to_vec()),
        effects
    );
}
fn start(owner: &mut SourceMainNativeSegmentedActiveServiceV1, documents: V) {
    action(
        owner,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents,
        },
    );
}
fn hold_native(owner: &mut SourceMainNativeSegmentedActiveServiceV1) {
    for _ in 0..12 {
        action(
            owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let s = owner.active_service().unwrap();
        if s.pending_write().is_some() {
            if s.caller_result().is_some() {
                return;
            }
            action(owner, NativeMainActiveServiceActionV1::AppendPending);
            action(owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("actual source did not reach held native await")
}
#[test]
fn more_than4096_real_pause_actions_preserve_one_owner_source_and_physical_heads() {
    let temp = Temp::new();
    let (mut owner, docs) = segmented(&temp, "detached-live-rpc-failure");
    start(&mut owner, docs);
    let financial = owner.financial_runtime() as *const SourceContractRuntime;
    let index = owner.index() as *const SourceCompactIndex;
    let mut raw_roots = Vec::new();
    for n in 0..4097 {
        if owner.history_window().len() == ACTIVE_ACK_SEGMENT_WINDOW {
            let before = owner.observation().unwrap();
            assert!(matches!(
                owner.prepare_action(NativeMainActiveServiceActionV1::Pause),
                Err(NativeMainActiveWalErrorV1::Capacity)
            ));
            assert_eq!(owner.observation().unwrap(), before);
            raw_roots.extend(
                owner
                    .history_window()
                    .iter()
                    .map(|r| (r.sequence, r.action.clone(), r.entry.clone(), r.head)),
            );
            checkpoint(&mut owner);
        }
        action(&mut owner, NativeMainActiveServiceActionV1::Pause);
        assert_eq!(owner.financial_runtime() as *const _, financial);
        assert_eq!(owner.index() as *const _, index);
        assert_eq!(owner.active_service().unwrap().head().actions, n + 2);
    }
    raw_roots.extend(
        owner
            .history_window()
            .iter()
            .map(|r| (r.sequence, r.action.clone(), r.entry.clone(), r.head)),
    );
    assert_eq!(raw_roots.len(), 4098);
    let mut previous_entry = V::Null;
    let mut previous_head = sha256(
        &active_wal_bytes(&owner.controller.wal.origin_context().unwrap().get("origin")).unwrap(),
    );
    for (at, (sequence, action, entry, head)) in raw_roots.into_iter().enumerate() {
        assert_eq!(sequence, at as u64 + 1);
        let raw = owner.read_history_blob(&action).unwrap();
        let decoded = NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).unwrap();
        assert!(if at == 0 {
            matches!(decoded, NativeMainActiveServiceActionV1::Start { .. })
        } else {
            decoded == NativeMainActiveServiceActionV1::Pause
        });
        let raw = owner.read_history_blob(&entry).unwrap();
        let value = V::from_diagnostic_json(std::str::from_utf8(&raw).unwrap()).unwrap();
        assert_eq!(value.get("previousEntry"), previous_entry);
        assert_eq!(value.get("previousHead"), s(&hex(&previous_head)));
        let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
        chain.extend_from_slice(&previous_head);
        chain.extend_from_slice(&sequence.to_be_bytes());
        chain.extend_from_slice(&entry.sha256);
        assert_eq!(sha256(&chain), head);
        previous_head = head;
        previous_entry = entry.value();
    }
    assert_eq!(previous_head, owner.head());
    assert!(owner.archived().unwrap().actions >= 4096);
    assert_eq!(owner.active_service().unwrap().head().actions, 4098);
    assert!(owner.active_service().unwrap().history().len() <= 256);
}
#[test]
fn segment_checkpoint_preserves_actual_pending_native_plan_through_apply_and_distinct_resolve() {
    let temp = Temp::new();
    let (mut owner, docs) = segmented(&temp, "same-held-live-validity");
    start(&mut owner, docs);
    hold_native(&mut owner);
    let context = owner.active_service().unwrap().pending_native_context();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let before = owner.index().snapshot().unwrap();
    checkpoint(&mut owner);
    assert_eq!(
        owner.active_service().unwrap().pending_native_context(),
        context
    );
    action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let post = owner.index().snapshot().unwrap();
    assert_ne!(post, before);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner.active_service().unwrap().pending_native_context(),
        context
    );
    let held = owner.active_service().unwrap().pending_write().unwrap() as *const _;
    checkpoint(&mut owner);
    assert_eq!(
        owner.active_service().unwrap().pending_write().unwrap() as *const _,
        held
    );
    assert_eq!(owner.index().snapshot().unwrap(), post);
    action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(owner.index().snapshot().unwrap(), post);
    assert!(owner.active_service().unwrap().pending_write().is_none());
}
#[test]
fn metadata_checkpoint_before_and_lost_return_hold_source_then_same_metadata_only_reconcile() {
    for fault in [
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ] {
        let temp = Temp::new();
        let (mut owner, docs) = segmented(&temp, "same-held-live-validity");
        start(&mut owner, docs);
        hold_native(&mut owner);
        let observation = owner.observation().unwrap();
        let head = owner.head();
        let count = owner.acknowledged_actions();
        let meta = owner.index().snapshot().unwrap();
        owner.prepare_checkpoint().unwrap();
        owner.controller.wal.set_test_fault(fault);
        assert!(owner.attempt_checkpoint().is_err());
        assert_eq!(owner.observation().unwrap(), observation);
        assert!(matches!(
            owner.prepare_action(NativeMainActiveServiceActionV1::AppendPending),
            Err(NativeMainActiveWalErrorV1::Busy)
        ));
        let resolution = owner.resolve_checkpoint().unwrap();
        if fault == crate::source_main_active_service_wal::TestFault::BeforeCommit {
            assert_eq!(resolution, ActiveServiceWalResolvedV1::Before);
            owner.attempt_checkpoint().unwrap();
        } else {
            assert_eq!(resolution, ActiveServiceWalResolvedV1::Post)
        }
        owner.finish_checkpoint().unwrap();
        assert_eq!(owner.acknowledged_actions(), count);
        assert_eq!(owner.head(), head);
        assert_eq!(owner.index().snapshot().unwrap(), meta);
        assert!(owner.active_service().unwrap().pending_write().is_some());
    }
}
#[test]
fn archived_segment_and_globals_survive_same_owner_actual_native_post_lost_return() {
    use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
    let temp = Temp::new();
    let (mut owner, docs) = segmented(&temp, "same-held-live-validity");
    start(&mut owner, docs);
    hold_native(&mut owner);
    checkpoint(&mut owner);
    let archived = owner.archived().unwrap().clone();
    let decode_count = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let context = owner.active_service().unwrap().pending_native_context();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let Some(ActiveWalOwner::Active(service)) = owner.controller.owner.as_mut() else {
        panic!("same actual active owner")
    };
    service.owner.index.test_fail_after_sync();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let global = owner.acknowledged_actions();
    let head = owner.head();
    let no_index = match owner.into_no_native_index() {
        Ok(o) => o,
        Err(e) => panic!("{e:?}"),
    };
    assert_eq!(no_index.archived(), Some(&archived));
    assert_eq!(no_index.effects(), effects);
    assert_eq!(
        no_index
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode_count
    );
    let mut owner = no_index
        .try_reopen(
            &temp.root,
            Candidate::Post,
            crate::storage::Recovery::RejectIncompleteTail,
        )
        .unwrap();
    assert_eq!(owner.archived(), Some(&archived));
    assert_eq!(owner.acknowledged_actions(), global);
    assert_eq!(owner.head(), head);
    assert_eq!(
        owner.active_service().unwrap().pending_native_context(),
        context
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.acknowledged_actions(), global + 1);
    let post = owner.index().snapshot().unwrap();
    checkpoint(&mut owner);
    action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(owner.index().snapshot().unwrap(), post);
}
