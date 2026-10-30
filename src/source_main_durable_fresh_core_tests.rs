// Parent-Core only: all owners/metadata/native failures are real existing pieces.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::source_main_active_service_wal::TestFault as F;
use crate::storage::Recovery;
fn coordinator(t: &Temp) -> SourceMainNativeDurableFreshInputV1 {
    let source = fact_setup(t);
    let root = t.root.join("durable-fresh-outer-control");
    std::fs::create_dir(&root).unwrap();
    SourceMainNativeDurableFreshInputV1::adopt(
        source,
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap()
}
fn feed() -> NativeMainDurableFreshActionV1 {
    NativeMainDurableFreshActionV1::Feed(
        NativeMainFreshRpcInputV1::new(obj(&[("height", V::number(12.))]), None).unwrap(),
    )
}
fn action(owner: &mut SourceMainNativeDurableFreshInputV1, action: NativeMainDurableFreshActionV1) {
    let count = owner.history().len();
    owner.prepare_action(action).unwrap();
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.history().len(), count + 1);
}
fn source_action(
    owner: &mut SourceMainNativeDurableFreshInputV1,
    source: NativeMainActiveServiceActionV1,
) {
    action(owner, NativeMainDurableFreshActionV1::SourceAction(source));
}
fn context(owner: &SourceMainNativeDurableFreshInputV1) -> V {
    owner
        .source
        .inner
        .active_service()
        .unwrap()
        .pending_native_context()
        .unwrap()
}
#[test]
fn durable_fresh_intent_before_keeps_same_plan_and_feed_publishes_only_after_real_sync() {
    let t = temp();
    let mut owner = coordinator(&t);
    let before = context(&owner);
    let physical = owner.index().capture_verified_history_pin().unwrap();
    let tables = owner.financial_state().unwrap().get("tables");
    let effects = owner
        .source
        .inner
        .active_service()
        .unwrap()
        .effects()
        .to_vec();
    let plan = owner
        .source
        .inner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap() as *const _;
    owner.control.barrier_fault = Some((NativeMainDurableFreshPhaseV1::Intent, F::BeforeCommit));
    owner.prepare_action(feed()).unwrap();
    assert!(owner.advance().is_err());
    assert!(owner.feeds().is_empty());
    assert_eq!(context(&owner), before);
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        physical
    );
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.feeds().len(), 1);
    assert_eq!(context(&owner), before);
    assert_eq!(
        owner
            .source
            .inner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .unwrap() as *const _,
        plan
    );
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        physical
    );
    assert_eq!(owner.financial_state().unwrap().get("tables"), tables);
    assert_eq!(
        owner.source.inner.active_service().unwrap().effects(),
        effects
    );
}
#[test]
fn durable_fresh_accepted_return_lost_resolves_only_control_without_second_feed() {
    let t = temp();
    let mut owner = coordinator(&t);
    owner.control.barrier_fault = Some((NativeMainDurableFreshPhaseV1::Accepted, F::AfterCommit));
    owner.prepare_action(feed()).unwrap();
    assert!(owner.advance().is_err());
    assert_eq!(owner.feeds().len(), 1);
    let snapshot = owner.financial_state().unwrap();
    let head = owner.control.feed_head;
    assert!(matches!(
        owner.prepare_action(feed()),
        Err(NativeMainDurableFreshErrorV1::Busy)
    ));
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.feeds().len(), 1);
    assert_eq!(owner.control.feed_head, head);
    assert_eq!(owner.financial_state().unwrap(), snapshot);
    assert_eq!(owner.history().len(), 1);
    let pin = owner.wal_pin().unwrap();
    let outer = pin.root.clone();
    drop(owner);
    assert!(
        crate::source_main_active_service_wal::VerifiedActiveServiceAckHistoryV1::open(
            &outer,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &pin
        )
        .is_err()
    );
}
#[test]
fn durable_fresh_post_recovery_retains_feed_and_actual_source_await_without_reappend() {
    let t = temp();
    let mut owner = coordinator(&t);
    action(&mut owner, feed());
    let feeds = owner.feeds().to_vec();
    let before = owner
        .source
        .inner
        .active_service()
        .unwrap()
        .effects()
        .to_vec();
    active_mut(&mut owner.source.inner)
        .owner
        .index
        .test_fail_after_sync();
    owner
        .prepare_action(NativeMainDurableFreshActionV1::SourceAction(
            NativeMainActiveServiceActionV1::AppendPending,
        ))
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainDurableFreshErrorV1::NativeRecoveryRequired)
    ));
    assert!(matches!(
        owner.prepare_action(feed()),
        Err(NativeMainDurableFreshErrorV1::Busy)
    ));
    let no_index = owner.into_no_inline_index().unwrap();
    assert_eq!(no_index.feeds(), feeds);
    assert_eq!(no_index.effects(), before);
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    let post = owner.index().capture_verified_history_pin().unwrap();
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.index().capture_verified_history_pin().unwrap(), post);
    assert_eq!(owner.feeds(), feeds);
    assert_eq!(
        owner.source.inner.active_service().unwrap().effects(),
        before
    );
    source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(owner.feeds(), feeds);
    assert_eq!(
        owner
            .source
            .inner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .unwrap()
            .arguments()[0]
            .get("_id"),
        s("MaxHeight")
    );
}
#[test]
fn durable_fresh_before_replacement_moves_same_feed_route_and_retries_only_native() {
    let t = temp();
    let mut owner = coordinator(&t);
    action(&mut owner, feed());
    let feeds = owner.feeds().to_vec();
    let tables = owner.financial_state().unwrap().get("tables");
    let before = owner
        .source
        .inner
        .active_service()
        .unwrap()
        .effects()
        .to_vec();
    active_mut(&mut owner.source.inner)
        .owner
        .index
        .test_fail_after_partial_write();
    owner
        .prepare_action(NativeMainDurableFreshActionV1::SourceAction(
            NativeMainActiveServiceActionV1::AppendPending,
        ))
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainDurableFreshErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_inline_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.feeds(), feeds);
    assert_eq!(owner.financial_state().unwrap().get("tables"), tables);
    assert_eq!(
        owner.source.inner.active_service().unwrap().effects(),
        before
    );
    source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(owner.feeds(), feeds);
}
#[test]
fn durable_fresh_source_inner_control_return_lost_never_reexecutes_pause() {
    let t = temp();
    let mut owner = coordinator(&t);
    let count = owner.source.history().len();
    owner.source.inner.barrier_fault = Some((NativeMainActiveWalPhaseV1::Intent, F::AfterCommit));
    owner
        .prepare_action(NativeMainDurableFreshActionV1::SourceAction(
            NativeMainActiveServiceActionV1::Pause,
        ))
        .unwrap();
    assert!(owner.advance().is_err());
    assert_eq!(owner.phase(), NativeMainDurableFreshPhaseV1::Execute);
    assert_eq!(
        owner.resolve_source_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.source.history().len(), count + 1);
    assert_eq!(
        owner
            .source
            .inner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .action,
        NativeMainActiveServiceActionV1::Pause
    );
}
#[test]
fn durable_fresh_observation_failure_retains_published_feed_and_never_republishes() {
    let t = temp();
    let mut owner = coordinator(&t);
    owner.prepare_action(feed()).unwrap();
    owner.control.observation_limit = 1;
    assert!(matches!(
        owner.advance(),
        Err(NativeMainDurableFreshErrorV1::Capacity)
    ));
    assert_eq!(owner.phase(), NativeMainDurableFreshPhaseV1::Observe);
    let actual = owner.financial_state().unwrap();
    let head = owner.control.feed_head;
    assert_eq!(owner.feeds().len(), 1);
    assert!(matches!(
        owner.advance(),
        Err(NativeMainDurableFreshErrorV1::Capacity)
    ));
    assert_eq!(owner.control.feed_head, head);
    assert_eq!(owner.financial_state().unwrap(), actual);
    owner.control.observation_limit = 64 * 1024 * 1024;
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.feeds().len(), 1);
}

#[test]
fn durable_fresh_outer_start_intent_is_synced_before_constructor_and_lost_return_never_restarts_it()
{
    let t = temp();
    let (raw, documents) = setup(&t, "same-held-live-validity");
    let escrow = t.root.join("durable-fresh-origin-escrow");
    std::fs::create_dir(&escrow).unwrap();
    let source = SourceMainNativeActualFactServiceV1::create(
        raw,
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        escrow.canonicalize().unwrap(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    let root = t.root.join("durable-fresh-before-start-control");
    std::fs::create_dir(&root).unwrap();
    let mut owner = SourceMainNativeDurableFreshInputV1::adopt(
        source,
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    let before = owner.financial_state().unwrap();
    owner.control.barrier_fault = Some((NativeMainDurableFreshPhaseV1::Intent, F::AfterCommit));
    owner
        .prepare_action(NativeMainDurableFreshActionV1::SourceAction(
            NativeMainActiveServiceActionV1::Start {
                start: V::number(10.),
                generation: 92,
                documents,
            },
        ))
        .unwrap();
    assert!(owner.advance().is_err());
    assert_eq!(owner.source.history().len(), 0);
    assert!(owner.source.inner.active_service().is_none());
    assert_eq!(owner.financial_state().unwrap(), before);
    assert!(matches!(
        owner
            .control
            .wal
            .read_blob(&ActiveServiceWalBlobV1::of(&owner.control.origin_archive).unwrap()),
        Err(ActiveServiceWalErrorV1::Ambiguous)
    ));
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(owner.source.history().len(), 0);
    assert!(owner.source.inner.active_service().is_none());
    assert_eq!(owner.financial_state().unwrap(), before);
    assert_eq!(
        owner
            .control
            .wal
            .read_blob(&ActiveServiceWalBlobV1::of(&owner.control.origin_archive).unwrap())
            .unwrap(),
        owner.source.inner.origin_archive
    );
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    assert_eq!(owner.source.history().len(), 1);
    assert_eq!(owner.history().len(), 1);
    assert!(owner.source.inner.active_service().is_some());
}

#[path = "source_main_durable_fresh_cold_core_tests.rs"]
mod durable_fresh_cold_core_tests;
