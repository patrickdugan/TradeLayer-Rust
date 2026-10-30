// Parent-Core gates: actual source/financial owners only, no dummy model or
// expected state/return imported. Expectations below are comparisons only.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
fn actor_setup(
    t: &Temp,
) -> (
    SourceMainNativeActualFactServiceV1,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
) {
    let (raw, documents) = setup(t, "same-held-live-validity");
    let completed = raw.index.capture_verified_history_pin().unwrap();
    let (archive, pin) = raw.checkpoint().unwrap();
    let escrow = t.root.join("actual-fact-actor-generation-escrow");
    std::fs::create_dir(&escrow).unwrap();
    let mut owner = SourceMainNativeActualFactServiceV1::create(
        raw,
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        escrow.canonicalize().unwrap(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    fact_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents,
        },
    );
    for _ in 0..24 {
        if owner
            .inner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .is_some()
        {
            return (owner, archive, pin, completed);
        }
        fact_action(
            &mut owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let active = owner.inner.active_service().unwrap();
        if active.pending_inline_write().is_some() {
            return (owner, archive, pin, completed);
        }
        if active.pending_write().is_some() {
            fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("genuine raw source did not reach live inline await")
}
fn verify_actor(
    mut owner: SourceMainNativeActualFactServiceV1,
    t: &Temp,
    archive: Vec<u8>,
    pin: NativeMainArchivePin,
    completed: SourceIndexHistoryPinV1,
    expected_inline: usize,
) {
    let expected_state = owner.inner.financial_runtime().state_value().unwrap();
    let active = owner.inner.active_service().unwrap();
    let expected_effects = active.effects().to_vec();
    let expected_receipts = active.history().to_vec();
    let expected_kernel = active.kernel_value();
    let count = owner.history().len();
    let post = owner.inner.index().capture_verified_history_pin().unwrap();
    let namespace = owner.inner.index().native_namespace().clone();
    let independent = owner
        .inner
        .wal
        .capture_actual_source_fact_cold_pin(&namespace)
        .unwrap();
    let index = {
        let ActiveWalOwner::Active(same) = owner.inner.owner.take().unwrap() else {
            panic!("actual source owner")
        };
        // Drop every remaining live financial/frame field before reconstructing
        // the single cold owner; retain only the actual final index here.
        same.owner.index
    };
    drop(owner);
    let reader = crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &independent,
    )
    .unwrap();
    let io = index.storage_io_counters().unwrap();
    let prefix = NativeMainColdPrefixV1::reexecute_with_actual_fact_views(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: pin,
                current: completed,
                commands: vec![],
            },
            actual_post: post.clone(),
        },
        index,
        &reader,
    )
    .unwrap();
    let cold =
        NativeMainColdActualFactServiceV1::reexecute(prefix, reader, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), count);
    assert_eq!(cold.verified_inline_operations(), expected_inline);
    assert_eq!(
        cold.financial_runtime().state_value().unwrap(),
        expected_state
    );
    assert_eq!(cold.state.effects(), expected_effects);
    assert_eq!(cold.original_action_receipts(), expected_receipts);
    assert_eq!(
        cold.state.frame.as_ref().unwrap().kernel_value(),
        expected_kernel.unwrap()
    );
    assert_eq!(cold.actual_post(), &post);
    let after = cold.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
}
#[test]
fn actual_fact_actor_never_appended_plan_pause_shutdown_uses_produced_facts_not_observed_state() {
    let t = temp();
    let (mut owner, archive, pin, completed) = actor_setup(&t);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    fact_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![s("literal source terminal boundary")]),
    );
    verify_actor(owner, &t, archive, pin, completed, 0);
}
#[test]
fn actual_fact_actor_three_native_plans_and_resolves_recompute_complete_finance_and_receipts() {
    let t = temp();
    let (mut owner, archive, pin, completed) = actor_setup(&t);
    for i in 0..3 {
        fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
        if i < 2 {
            fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    verify_actor(owner, &t, archive, pin, completed, 3);
}
#[test]
fn actual_fact_actor_before_native_replacement_uses_real_io_chain_without_source_retry() {
    let t = temp();
    let (mut owner, archive, pin, completed) = actor_setup(&t);
    active_mut(&mut owner.inner)
        .owner
        .index
        .test_fail_after_partial_write();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_inline_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    verify_actor(owner, &t, archive, pin, completed, 1);
}
#[test]
fn actual_fact_actor_second_lost_return_post_recovery_does_not_replay_native_or_financial_prefix() {
    let t = temp();
    let (mut owner, archive, pin, completed) = actor_setup(&t);
    active_mut(&mut owner.inner)
        .owner
        .index
        .test_fail_after_partial_write();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_inline_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    active_mut(&mut owner.inner)
        .owner
        .index
        .test_fail_after_sync();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_inline_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    verify_actor(owner, &t, archive, pin, completed, 1);
}

#[test]
fn actual_fact_actor_real_reappend_api_rejection_preserves_prior_native_and_source_observations() {
    let t = temp();
    let (mut owner, archive, pin, completed) = actor_setup(&t);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let point = owner.inner.index().capture_verified_history_pin().unwrap();
    let receipts = owner.inner.active_service().unwrap().history().to_vec();
    fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(
        owner.inner.index().capture_verified_history_pin().unwrap(),
        point
    );
    assert_eq!(owner.inner.active_service().unwrap().history(), receipts);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    verify_actor(owner, &t, archive, pin, completed, 1);
}
