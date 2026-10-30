use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
#[test]
fn actual_fact_lost_native_return_reopens_only_post_keeps_real_stop_and_same_source_pc() {
    let t = temp();
    let mut owner = fact_setup(&t);
    let effects = owner.inner.active_service().unwrap().effects().to_vec();
    let financial = owner.inner.financial_runtime().state_value().unwrap();
    active_mut(&mut owner.inner)
        .owner
        .index
        .test_fail_after_sync();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let io = owner.inner.wal.record().unwrap().get("actualNativeIoFact");
    let no_index = owner.into_no_inline_index().unwrap();
    assert_eq!(no_index.effects(), effects);
    let no_index = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::RejectIncompleteTail)
        .err()
        .expect("wrong actual Before refuses")
        .into_owner();
    assert_eq!(no_index.effects(), effects);
    let post = no_index
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap();
    assert_eq!(post.effects(), effects);
    let mut owner = post.finish_verified_post().unwrap();
    assert!(owner.fact_barrier_pending());
    let sequence = owner.inner.index().snapshot().unwrap().sequence;
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.inner.index().snapshot().unwrap().sequence, sequence);
    assert_eq!(
        owner.inner.wal.record().unwrap().get("actualNativeIoFact"),
        io
    );
    assert_eq!(
        owner.inner.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
    assert!(matches!(
        owner
            .inner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .result,
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::StorageStopped(_))
    ));
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(
        owner
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

fn failed_before() -> (Temp, SourceMainNativeActualFactNoIndexV1, Vec<V>, V) {
    let t = temp();
    let mut owner = fact_setup(&t);
    let effects = owner.inner.active_service().unwrap().effects().to_vec();
    let financial = owner.inner.financial_runtime().state_value().unwrap();
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
    (t, owner.into_no_inline_index().unwrap(), effects, financial)
}
#[test]
fn actual_fact_before_replacement_syncs_mapping_and_keeps_both_real_native_results() {
    let (t, no_index, effects, financial) = failed_before();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent
    );
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(
        owner.inner.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
    let record = owner.inner.wal.record().unwrap();
    let chain = fact_parse(&fact_blob(&owner, record.get("actualNativeIoChain")));
    assert_eq!(chain.get("attempt"), s("2"));
    assert_ne!(chain.get("previous"), V::Null);
    let first = fact_parse(&fact_blob(&owner, chain.get("previous")));
    assert_eq!(first.get("attempt"), s("1"));
    let old_io = fact_parse(&fact_blob(&owner, first.get("actualIoFact")));
    let new_io = fact_parse(&fact_blob(&owner, chain.get("actualIoFact")));
    assert_eq!(old_io.get("actualOutcome").get("kind"), s("NativeIoError"));
    assert_eq!(
        new_io.get("actualOutcome").get("kind"),
        s("NativeAppendReturned")
    );
    assert!(matches!(
        owner
            .inner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .result,
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::StorageStopped(_))
    ));
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn replacement_second_lost_return_retains_latest_typed_io_and_post_never_appends_again() {
    let (t, no_index, effects, financial) = failed_before();
    let mut owner = no_index
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
    assert_eq!(
        owner.inner.wal.record().unwrap().get("stage"),
        s("NativeIoObserved")
    );
    assert_eq!(
        owner.inner.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
    let no_index = owner.into_no_inline_index().unwrap();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    let sequence = owner.inner.index().snapshot().unwrap().sequence;
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.inner.index().snapshot().unwrap().sequence, sequence);
    let chain = fact_parse(&fact_blob(
        &owner,
        owner.inner.wal.record().unwrap().get("actualNativeIoChain"),
    ));
    assert_eq!(chain.get("attempt"), s("2"));
    let io = fact_parse(&fact_blob(&owner, chain.get("actualIoFact")));
    assert_eq!(io.get("actualOutcome").get("kind"), s("NativeIoError"));
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn lost_supersession_intent_return_never_installs_or_appends_before_actual_post_resolution() {
    let (t, no_index, effects, financial) = failed_before();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    let before = owner.inner.index().capture_verified_history_pin().unwrap();
    owner.inner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(
        owner.inner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert!(
        !owner
            .inner
            .pending
            .as_ref()
            .unwrap()
            .native_recovery
            .as_ref()
            .unwrap()
            .installed
    );
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(
        owner.inner.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
}
fn primed_fact_owner(t: &Temp, offset: Option<u64>) -> (SourceMainNativeActualFactServiceV1, u64) {
    let (raw, documents, prime) = setup_primed(t, offset);
    let escrow = t.root.join("actual-fact-recovery-generation-escrow");
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
            return (owner, prime);
        }
        fact_action(
            &mut owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let active = owner.inner.active_service().unwrap();
        if active.pending_inline_write().is_some() {
            return (owner, prime);
        }
        if active.pending_write().is_some() {
            fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("raw source never reached real inline await")
}
#[test]
fn actual_fact_compaction_intermediate_has_synced_real_escrow_before_native_only_retry() {
    // Calibrate operational byte length only using an independent actual run.
    // No financial/validation output is supplied to the fresh real owner.
    let calibration = temp();
    let (mut probe, prime) = primed_fact_owner(&calibration, None);
    let at = probe.inner.index().footprint().unwrap().journal_file_bytes;
    fact_action(&mut probe, NativeMainActiveServiceActionV1::AppendPending);
    let bytes = probe.inner.index().footprint().unwrap().journal_file_bytes - at;
    assert!(bytes > 1);
    let offset = at - prime + bytes - 1;
    drop(probe);
    let t = temp();
    let (mut owner, actual_prime) = primed_fact_owner(&t, Some(offset));
    assert_eq!(actual_prime, prime);
    assert!(owner
        .inner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap()
        .append_plan()
        .unwrap()
        .intermediate()
        .is_some());
    let effects = owner.inner.active_service().unwrap().effects().to_vec();
    active_mut(&mut owner.inner)
        .owner
        .index
        .native_main_test_compaction_fault("after-current-sync");
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    assert!(owner.inner.pending.as_ref().unwrap().escrow_chain.is_some());
    let no_index = owner.into_no_inline_index().unwrap();
    let mut owner = no_index
        .try_reopen(
            &t.root,
            Candidate::Intermediate,
            Recovery::RejectIncompleteTail,
        )
        .unwrap()
        .prepare_verified_replacement()
        .unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
    assert_ne!(
        owner.inner.wal.record().unwrap().get("generationEscrow"),
        V::Undefined
    );
    fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn actual_fact_foreign_and_corrupt_prefix_reopen_keeps_the_same_no_index_owner() {
    let t = temp();
    let mut owner = fact_setup(&t);
    let effects = owner.inner.active_service().unwrap().effects().to_vec();
    let pin = owner
        .inner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap()
        .verified_reopen_pin(Candidate::Before)
        .unwrap();
    let file = pin.namespace.path(
        &pin.canonical_catalog
            .join(format!("g-{:020}", pin.publication.generation)),
    );
    let prefix = std::fs::metadata(&file).unwrap().len();
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
    let no_index = owner.into_no_inline_index().unwrap();
    let foreign = temp();
    let index = SourceCompactIndex::create(
        &foreign.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual foreign native data",
    )
    .unwrap();
    drop(index);
    let no_index = no_index
        .try_reopen(
            &foreign.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .err()
        .expect("foreign physical owner refuses")
        .into_owner();
    assert_eq!(no_index.effects(), effects);
    let file = file.canonicalize().unwrap();
    assert!(file.starts_with(t.root.canonicalize().unwrap()));
    let mut raw = std::fs::read(&file).unwrap();
    let at = usize::try_from(prefix / 2).unwrap();
    assert!(at < raw.len());
    raw[at] ^= 1;
    std::fs::write(&file, &raw).unwrap();
    let no_index = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .err()
        .expect("corrupt actual complete prefix refuses")
        .into_owner();
    assert_eq!(no_index.effects(), effects);
    assert_eq!(std::fs::read(&file).unwrap(), raw);
}
