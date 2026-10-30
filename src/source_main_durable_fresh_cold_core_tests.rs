// Parent-Core only. Reconstruct ONE real prefix from original raw foundation.
use super::*;
fn cold_writer(
    t: &Temp,
) -> (
    SourceMainNativeDurableFreshInputV1,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
) {
    let (raw, documents) = setup(t, "same-held-live-validity");
    let completed = raw.index.capture_verified_history_pin().unwrap();
    let (archive, pin) = raw.checkpoint().unwrap();
    let escrow = t.root.join("paired-fresh-cold-escrow");
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
    let root = t.root.join("paired-fresh-cold-control");
    std::fs::create_dir(&root).unwrap();
    let mut owner = SourceMainNativeDurableFreshInputV1::adopt(
        source,
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    action(&mut owner, feed()); // genuine NoFrame rejection before source Start
    source_action(&mut owner, NativeMainActiveServiceActionV1::Pause); // genuine controller WrongAction, no child ACK
    source_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 81,
            documents,
        },
    );
    for _ in 0..24 {
        if owner
            .source
            .inner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .is_some()
        {
            return (owner, archive, pin, completed);
        }
        source_action(
            &mut owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let active = owner.source.inner.active_service().unwrap();
        if active.pending_inline_write().is_some() {
            return (owner, archive, pin, completed);
        }
        if active.pending_write().is_some() {
            source_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("genuine source failed to reach actual inline await")
}
fn cold_verify(
    mut owner: SourceMainNativeDurableFreshInputV1,
    t: &Temp,
    archive: Vec<u8>,
    pin: NativeMainArchivePin,
    completed: SourceIndexHistoryPinV1,
) {
    let financial = owner.financial_state().unwrap();
    let source = owner.source.inner.active_service().unwrap();
    let effects = source.effects().to_vec();
    let receipts = source.history().to_vec();
    let kernel = source.kernel_value().unwrap();
    let feeds = owner.feeds().to_vec();
    let head = owner.head();
    let actions = owner.history().len();
    let post = owner.index().capture_verified_history_pin().unwrap();
    let expected = owner.capture_cold_pin().unwrap();
    let index = {
        let ActiveWalOwner::Active(same) = owner.source.inner.owner.take().unwrap() else {
            panic!("same actual financial owner")
        };
        same.owner.index
    };
    drop(owner);
    assert!(
        crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1::open(
            &t.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &expected.source
        )
        .is_err(),
        "old public financial continuity remains strict"
    );
    let pair = crate::source_main_active_service_wal::VerifiedDurableFreshPairedHistoryV1::open(
        &expected.outer.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &expected,
    )
    .unwrap();
    let io = index.storage_io_counters().unwrap();
    let prefix = NativeMainColdPrefixV1::reexecute_with_paired_fresh_views(
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
        &pair,
    )
    .unwrap();
    let cold =
        NativeMainColdDurableFreshInputV1::reexecute(prefix, pair, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), actions);
    assert_eq!(cold.acknowledged_head(), head);
    assert_eq!(cold.feeds(), feeds);
    assert_eq!(cold.financial_state().unwrap(), financial);
    assert_eq!(cold.original_source_receipts(), receipts);
    assert_eq!(cold.source.state.effects(), effects);
    assert_eq!(
        cold.source.state.frame.as_ref().unwrap().kernel_value(),
        kernel
    );
    assert_eq!(cold.index().capture_verified_history_pin().unwrap(), post);
    let after = cold.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
}
#[test]
fn paired_fresh_cold_reexecutes_raw_feeds_three_plans_and_real_api_rejections() {
    let t = temp();
    let (mut owner, archive, pin, completed) = cold_writer(&t);
    action(&mut owner, feed());
    for at in 0..3 {
        source_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
        if at < 2 {
            source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
            action(
                &mut owner,
                NativeMainDurableFreshActionV1::Feed(
                    NativeMainFreshRpcInputV1::new(obj(&[("height", V::number(13.))]), None)
                        .unwrap(),
                ),
            );
        }
    }
    source_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    source_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    action(&mut owner, feed()); // real AlreadyExited rejection, no fake feed
    cold_verify(owner, &t, archive, pin, completed);
}
#[test]
fn paired_fresh_cold_keeps_actual_before_replacement_and_feed_without_native_or_source_retry() {
    let t = temp();
    let (mut owner, archive, pin, completed) = cold_writer(&t);
    action(&mut owner, feed());
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
    action(&mut owner, feed());
    source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    cold_verify(owner, &t, archive, pin, completed);
}
#[test]
fn paired_fresh_cold_keeps_second_lost_return_post_and_never_reappends() {
    let t = temp();
    let (mut owner, archive, pin, completed) = cold_writer(&t);
    action(&mut owner, feed());
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
    active_mut(&mut owner.source.inner)
        .owner
        .index
        .test_fail_after_sync();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainDurableFreshErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_inline_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    assert_eq!(
        owner.advance().unwrap(),
        NativeMainDurableFreshPhaseV1::Idle
    );
    action(&mut owner, feed());
    source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    cold_verify(owner, &t, archive, pin, completed);
}

#[test]
fn paired_fresh_cold_updates_documents_while_same_decoded_financial_validity_stays_held() {
    let t = temp();
    let (mut owner, archive, pin, completed) = cold_writer(&t);
    for _ in 0..3 {
        source_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
        source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    }
    let active = owner.source.inner.active_service().unwrap();
    let pending = active
        .pending_write()
        .expect("genuine decoded financial validity await") as *const _;
    let effects = active.effects().to_vec();
    let current = owner.index().capture_verified_history_pin().unwrap();
    action(
        &mut owner,
        NativeMainDurableFreshActionV1::Feed(
            NativeMainFreshRpcInputV1::new(
                obj(&[("height", V::number(12.))]),
                Some(V::Array(vec![])),
            )
            .unwrap(),
        ),
    );
    assert_eq!(
        owner
            .source
            .inner
            .active_service()
            .unwrap()
            .pending_write()
            .unwrap() as *const _,
        pending
    );
    assert_eq!(
        owner.source.inner.active_service().unwrap().effects(),
        effects
    );
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        current
    );
    source_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    source_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    source_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    cold_verify(owner, &t, archive, pin, completed);
}

fn corrupt_fixture(
    t: &Temp,
) -> (
    SourceMainNativeDurableFreshInputV1,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
) {
    let (mut owner, archive, pin, completed) = cold_writer(t);
    action(&mut owner, feed());
    source_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    action(
        &mut owner,
        NativeMainDurableFreshActionV1::Feed(
            NativeMainFreshRpcInputV1::new(obj(&[("height", V::number(13.))]), None).unwrap(),
        ),
    );
    source_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    (owner, archive, pin, completed)
}
fn corrupt_and_check(
    kind: crate::source_main_active_service_wal::DurableFreshCorruptionV1,
    financial_gate: bool,
) {
    let t = temp();
    let (mut owner, archive, pin, completed) = corrupt_fixture(&t);
    let expected = owner.capture_cold_pin().unwrap();
    let post = owner.index().capture_verified_history_pin().unwrap();
    let index = {
        let ActiveWalOwner::Active(same) = owner.source.inner.owner.take().unwrap() else {
            panic!("same actual original owner")
        };
        same.owner.index
    };
    drop(owner);
    let changed =
        crate::source_main_active_service_wal::test_rewrite_durable_fresh_outer(&expected, kind)
            .unwrap();
    let counters = index.storage_io_counters().unwrap();
    if !financial_gate {
        assert!(
            crate::source_main_active_service_wal::VerifiedDurableFreshPairedHistoryV1::open(
                &changed.outer.root,
                Namespace::new("LTC", false, "consensus").unwrap(),
                &t.root,
                Namespace::new("LTC", false, "consensus").unwrap(),
                &changed
            )
            .is_err()
        );
        assert_eq!(index.capture_verified_history_pin().unwrap(), post);
        assert_eq!(
            index.storage_io_counters().unwrap().confirmed_append_bytes,
            counters.confirmed_append_bytes
        );
        return;
    }
    // Complete repaired CRC/head/feed chains are deliberately re-pinned here.
    // Metadata integrity alone must NOT become financial/source validation.
    let paired = crate::source_main_active_service_wal::VerifiedDurableFreshPairedHistoryV1::open(
        &changed.outer.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &changed,
    )
    .unwrap();
    let tampered_at = paired.entries().iter().position(|entry| {
        let NativeMainDurableFreshActionV1::Feed(input) = entry.action() else {
            return false;
        };
        input.io().get("height") == V::number(999.)
            && matches!(input.documents(), Some(V::Array(rows)) if rows.iter().any(|row| row.get("id") == s("negative-only-substituted-document")))
    }).expect("derive substituted boundary from actual repaired raw feed");
    let genuine_source_prefix = paired.entries()[tampered_at]
        .before_receiver()
        .source_actions();
    assert!(
        tampered_at > 2 && genuine_source_prefix > 0,
        "tampered feed follows genuine source construction/actions"
    );
    let tampered_raw = paired.entries()[tampered_at].action().clone();
    let prefix = NativeMainColdPrefixV1::reexecute_with_paired_fresh_views(
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
        &paired,
    )
    .unwrap();
    let rejected = NativeMainColdDurableFreshInputV1::reexecute(prefix, paired, 64 * 1024 * 1024)
        .err()
        .expect("actual feed computation rejects substituted documents despite repaired metadata");
    assert_eq!(
        rejected.cause,
        format!("fresh source After/outcome mismatch at {tampered_at}")
    );
    assert_eq!(
        rejected.owner.raw.next, tampered_at,
        "must reach substituted feed, not reject an earlier API call"
    );
    assert_eq!(
        rejected.owner.source.original_action_receipts().len() as u64,
        genuine_source_prefix
    );
    assert!(
        !rejected.owner.source.state.effects().is_empty(),
        "genuine source prefix executed"
    );
    let NativeMainDurableFreshActionV1::Feed(tampered_input) = tampered_raw else {
        unreachable!()
    };
    assert_eq!(
        rejected
            .owner
            .raw
            .feeds
            .last()
            .expect("actual tampered feed was executed")
            .raw,
        tampered_input
    );
    assert_eq!(
        rejected.index().capture_verified_history_pin().unwrap(),
        post
    );
    let after = rejected.index().storage_io_counters().unwrap();
    assert_eq!(
        after.confirmed_append_bytes,
        counters.confirmed_append_bytes
    );
    assert_eq!(
        after.copied_checkpoint_bytes,
        counters.copied_checkpoint_bytes
    );
    assert_eq!(after.published_compactions, counters.published_compactions);
}
#[test]
fn paired_fresh_missing_raw_feed_blob_refuses_without_repair() {
    corrupt_and_check(
        crate::source_main_active_service_wal::DurableFreshCorruptionV1::Missing,
        false,
    );
}
#[test]
fn paired_fresh_reordered_actual_crc_commits_refuse_even_with_new_trace_pin() {
    corrupt_and_check(
        crate::source_main_active_service_wal::DurableFreshCorruptionV1::Reorder,
        false,
    );
}
#[test]
fn paired_fresh_substituted_feed_with_all_metadata_chains_repaired_requires_real_source_recompute()
{
    corrupt_and_check(
        crate::source_main_active_service_wal::DurableFreshCorruptionV1::Substitute,
        true,
    );
}

#[test]
fn paired_fresh_cold_created_child_with_no_source_actions_replays_real_feed_and_wrong_action_rejections(
) {
    let t = temp();
    let (raw, _) = setup(&t, "same-held-live-validity");
    let completed = raw.index.capture_verified_history_pin().unwrap();
    let (archive, pin) = raw.checkpoint().unwrap();
    let escrow = t.root.join("paired-created-source-escrow");
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
    let root = t.root.join("paired-created-control");
    std::fs::create_dir(&root).unwrap();
    let mut owner = SourceMainNativeDurableFreshInputV1::adopt(
        source,
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    action(&mut owner, feed());
    source_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    let financial = owner.financial_state().unwrap();
    let head = owner.head();
    let expected = owner.capture_cold_pin().unwrap();
    assert_eq!(
        expected.source_entry,
        crate::source_main_active_service_wal::DurableFreshSourceEntryV1::Created
    );
    let index = {
        let ActiveWalOwner::Ready(same) = owner.source.inner.owner.take().unwrap() else {
            panic!("same original never-started financial owner")
        };
        same.index
    };
    drop(owner);
    assert!(
        crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1::open(
            &t.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &expected.source
        )
        .is_err()
    );
    let pair = crate::source_main_active_service_wal::VerifiedDurableFreshPairedHistoryV1::open(
        &expected.outer.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &expected,
    )
    .unwrap();
    let prefix = NativeMainColdPrefixV1::reexecute_with_paired_fresh_views(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: pin,
                current: completed.clone(),
                commands: vec![],
            },
            actual_post: completed.clone(),
        },
        index,
        &pair,
    )
    .unwrap();
    let cold =
        NativeMainColdDurableFreshInputV1::reexecute(prefix, pair, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), 2);
    assert_eq!(cold.acknowledged_head(), head);
    assert!(cold.feeds().is_empty());
    assert!(cold.original_source_receipts().is_empty());
    assert_eq!(cold.financial_state().unwrap(), financial);
    assert_eq!(
        cold.index().capture_verified_history_pin().unwrap(),
        completed
    );
}
