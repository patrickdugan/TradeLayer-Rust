// Nested in actual durable tests; all setup values are raw source fixture inputs.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
include!("source_main_inline_wal_primed_setup.rs");
fn temp() -> Temp {
    let parent = std::env::var_os("MAIN_SERVICE_TMP")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    let root = parent.join(format!(
        "actual-inline-service-wal-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    Temp { root, parent }
}
fn wrap(
    raw: SourceMainNativeSessionV13,
    docs: V,
    temp: &Temp,
) -> SourceMainNativeDurableActiveServiceV1 {
    let escrow = temp.root.join("inline-generation-escrow");
    std::fs::create_dir(&escrow).unwrap();
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_inline_service_escrow(
        raw,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        escrow.canonicalize().unwrap(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    owner
}
fn first_inline(owner: &mut SourceMainNativeDurableActiveServiceV1) {
    for _ in 0..24 {
        if owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .is_some()
        {
            return;
        }
        run_action(
            owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let active = owner.active_service().unwrap();
        if active.pending_inline_write().is_some() {
            return;
        }
        if active.pending_write().is_some() {
            run_action(owner, NativeMainActiveServiceActionV1::AppendPending);
            run_action(owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("actual source did not reach inline native await")
}
fn active_mut(
    owner: &mut SourceMainNativeDurableActiveServiceV1,
) -> &mut SourceMainNativeActiveServiceV1 {
    let Some(ActiveWalOwner::Active(active)) = &mut owner.owner else {
        panic!("real active owner")
    };
    active
}
fn ordinary(temp: &Temp) -> SourceMainNativeDurableActiveServiceV1 {
    let (raw, docs) = setup(temp, "same-held-live-validity");
    let mut owner = wrap(raw, docs, temp);
    first_inline(&mut owner);
    owner
}
fn fail_native(owner: &mut SourceMainNativeDurableActiveServiceV1, post: bool) {
    if post {
        active_mut(owner).owner.index.test_fail_after_sync();
    } else {
        active_mut(owner)
            .owner
            .index
            .test_fail_after_partial_write();
    }
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeRecoveryRequired
    );
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .is_err());
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Shutdown(vec![]))
        .is_err());
}
fn rpc_count(owner: &SourceMainNativeDurableActiveServiceV1) -> usize {
    owner
        .active_service()
        .unwrap()
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("rpc-data"))
        .count()
}
#[test]
fn inline_wal_mode_is_immutable_and_ordinary_native_write_copies_nothing() {
    let t = temp();
    let mut owner = ordinary(&t);
    assert_eq!(owner.profile(), NATIVE_MAIN_INLINE_ACTIVE_WAL_PROFILE);
    let origin = owner.wal.origin_context().unwrap().get("origin");
    assert_eq!(
        origin.get("profile"),
        s(NATIVE_MAIN_INLINE_ACTIVE_WAL_PROFILE)
    );
    assert_eq!(
        origin.get("requiredInlineServiceProfile"),
        s(crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE)
    );
    let archive = t.root.join("inline-generation-escrow");
    assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = rpc_count(&owner);
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(rpc_count(&owner), count);
    let entry = owner
        .read_checked_history_entry(owner.history().last().unwrap())
        .unwrap();
    assert_eq!(
        entry.get("profile"),
        s(NATIVE_MAIN_INLINE_ACTIVE_WAL_PROFILE)
    );
    assert_eq!(entry.get("generationEscrow"), V::Undefined);
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
    assert_eq!(rpc_count(&owner), count);
    let wal_pin = owner.wal.pin().unwrap();
    drop(owner);
    assert!(
        crate::source_main_active_service_wal::VerifiedActiveServiceAckHistoryV1::open(
            &t.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &wal_pin,
        )
        .is_err(),
        "old ordinary HISTORY reader must not admit inline Mode4"
    );
}
#[test]
fn inline_raw_intent_lost_ack_never_reexecutes_source_or_native_append() {
    let t = temp();
    let mut owner = ordinary(&t);
    let before = owner.index().capture_verified_history_pin().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Intent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    owner.advance().unwrap();
    assert_eq!(
        owner.index().snapshot().unwrap().sequence,
        before.metadata.sequence + 1
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert!(owner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .is_some());
}
#[test]
fn inline_actual_lost_native_return_reopens_post_keeps_failure_and_resolves_same_source_pc() {
    let t = temp();
    let mut owner = ordinary(&t);
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = rpc_count(&owner);
    let context = owner
        .active_service()
        .unwrap()
        .pending_native_context()
        .unwrap();
    fail_native(&mut owner, true);
    let no_index = owner.into_no_inline_index().unwrap();
    assert_eq!(no_index.effects(), effects);
    let no_index = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::RejectIncompleteTail)
        .err()
        .expect("wrong actual candidate")
        .into_owner();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap();
    owner.advance().unwrap();
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(rpc_count(&owner), count);
    assert_eq!(owner.acknowledged_append, Some(context));
    assert!(matches!(
        owner
            .active_service()
            .unwrap()
            .history()
            .last()
            .unwrap()
            .result,
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::StorageStopped(_))
    ));
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(rpc_count(&owner), count);
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
}
#[test]
fn inline_actual_partial_before_syncs_replacement_and_never_repeats_the_parent_action() {
    let t = temp();
    let mut owner = ordinary(&t);
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = rpc_count(&owner);
    fail_native(&mut owner, false);
    let no_index = owner.into_no_inline_index().unwrap();
    let mut owner = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap();
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent
    );
    let before = owner.index().capture_verified_history_pin().unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent,
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    owner.advance().unwrap();
    assert_eq!(owner.phase(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(rpc_count(&owner), count);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(rpc_count(&owner), count);
}
#[test]
fn inline_real_compaction_midpoint_has_prior_synced_escrow_and_same_source_continuation() {
    // Independent real source execution calibrates only native byte lengths; no
    // financial/validity result is supplied to the separately raw-configured owner.
    let calibration = temp();
    let (raw, docs, prime) = setup_primed(&calibration, None);
    let mut probe = wrap(raw, docs, &calibration);
    first_inline(&mut probe);
    let at = probe.index().footprint().unwrap().journal_file_bytes;
    run_action(&mut probe, NativeMainActiveServiceActionV1::AppendPending);
    let bytes = probe.index().footprint().unwrap().journal_file_bytes - at;
    assert!(bytes > 1);
    let offset = at - prime + bytes - 1;
    drop(probe);
    let t = temp();
    let (raw, docs, real_prime) = setup_primed(&t, Some(offset));
    assert_eq!(real_prime, prime);
    let mut owner = wrap(raw, docs, &t);
    first_inline(&mut owner);
    assert!(
        owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .unwrap()
            .append_plan()
            .unwrap()
            .intermediate()
            .is_some(),
        "actual intended live operation must compact"
    );
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = rpc_count(&owner);
    let before = owner.index().capture_verified_history_pin().unwrap();
    active_mut(&mut owner)
        .owner
        .index
        .native_main_test_compaction_fault("after-current-sync");
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::EscrowBoundIntent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert!(owner.pending.as_ref().unwrap().escrow_permit.is_some());
    assert!(owner.pending.as_ref().unwrap().escrow_chain.is_some());
    assert!(matches!(
        owner.resolve_control_write(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let no_index = owner.into_no_inline_index().unwrap();
    let mut owner = no_index
        .try_reopen(
            &t.root,
            Candidate::Intermediate,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    owner.advance().unwrap();
    assert_eq!(owner.phase(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(rpc_count(&owner), count);
    let entry = owner
        .read_checked_history_entry(owner.history().last().unwrap())
        .unwrap();
    assert_ne!(entry.get("generationEscrow"), V::Undefined);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(rpc_count(&owner), count);
}
#[test]
fn inline_foreign_and_corrupt_native_files_leave_the_same_financial_owner_parked() {
    let t = temp();
    let mut owner = ordinary(&t);
    let count = rpc_count(&owner);
    let pin = owner
        .active_service()
        .unwrap()
        .pending_inline_write()
        .unwrap()
        .verified_reopen_pin(Candidate::Before)
        .unwrap();
    let original_file = pin.namespace.path(
        &pin.canonical_catalog
            .join(format!("g-{:020}", pin.publication.generation)),
    );
    let complete_prefix_bytes = std::fs::metadata(&original_file).unwrap().len();
    fail_native(&mut owner, false);
    let no_index = owner.into_no_inline_index().unwrap();
    let foreign = temp();
    let index = SourceCompactIndex::create(
        &foreign.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual foreign metadata",
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
        .expect("foreign actual root")
        .into_owner();
    let file = original_file.canonicalize().unwrap();
    assert!(file.starts_with(t.root.canonicalize().unwrap()));
    let mut bytes = std::fs::read(&file).unwrap();
    let at = usize::try_from(complete_prefix_bytes / 2).unwrap();
    assert!(at < usize::try_from(complete_prefix_bytes).unwrap());
    bytes[at] ^= 1;
    std::fs::write(&file, &bytes).unwrap();
    let no_index = no_index
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .err()
        .expect("corrupt committed prefix must not be repaired")
        .into_owner();
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert_eq!(
        no_index
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("rpc-data"))
            .count(),
        count
    );
}

#[path = "source_main_mode4_closure_core_tests.rs"]
mod mode4_closure_core_tests;

#[path = "source_main_mode4_prefix_provider_tests.rs"]
mod source_mode4_prefix_tests;

#[path = "source_main_actual_fact_controller_tests.rs"]
mod actual_fact_controller_tests;
