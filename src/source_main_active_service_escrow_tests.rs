// Child of actual recovery tests, sharing its raw fixture construction helpers.
use super::*;
fn escrow_temp() -> Temp {
    let parent = std::env::temp_dir().canonicalize().unwrap();
    let root = parent.join(format!(
        "actual-active-service-escrow-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    Temp { root, parent }
}
fn escrow_owner(
    temp: &Temp,
    compacting: bool,
) -> (SourceMainNativeDurableActiveServiceV1, PathBuf) {
    let (raw, docs) = if compacting {
        setup_compacting(temp, "same-held-live-validity")
    } else {
        setup(temp, "same-held-live-validity")
    };
    let archive = temp.root.join("generation-escrow");
    std::fs::create_dir(&archive).unwrap();
    let archive = archive.canonicalize().unwrap();
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_generation_escrow(
        raw,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        archive.clone(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    run_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
    );
    assert!(owner.active_service().unwrap().pending_write().is_some());
    assert!(
        owner.active_service().unwrap().caller_result().is_none(),
        "first real catchup await selected, before fixture compaction is consumed"
    );
    (owner, archive)
}
#[test]
fn enhanced_ordinary_native_append_records_mode_but_creates_no_archive() {
    let temp = escrow_temp();
    let (mut owner, archive) = escrow_owner(&temp, false);
    assert_eq!(owner.profile(), ACTIVE_ESCROW_WAL_PROFILE);
    let ctx = owner.wal.origin_context().unwrap();
    assert_eq!(
        ctx.get("origin").get("profile"),
        s(ACTIVE_ESCROW_WAL_PROFILE)
    );
    assert_eq!(
        ctx.get("origin").get("generationEscrow").get("root"),
        s(&archive.to_string_lossy())
    );
    assert!(owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .append_plan()
        .intermediate()
        .is_none());
    let effects = owner.active_service().unwrap().effects().to_vec();
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    let entry = owner
        .read_checked_history_entry(owner.history().last().unwrap())
        .unwrap();
    assert_eq!(entry.get("generationEscrow"), V::Undefined);
}
#[test]
fn actual_escrow_permit_and_wal_binding_precede_compaction_native_io() {
    let temp = escrow_temp();
    let (mut owner, archive) = escrow_owner(&temp, true);
    assert!(owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .append_plan()
        .intermediate()
        .is_some());
    let before = owner.index().snapshot().unwrap();
    let original = owner.index().capture_verified_history_pin().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let decoded = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::EscrowBoundIntent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(owner.phase(), NativeMainActiveWalPhaseV1::EscrowBoundIntent);
    assert_eq!(owner.index().snapshot().unwrap(), before);
    let pending = owner.pending.as_ref().unwrap();
    assert!(!pending.executed);
    let pin = pending
        .escrow_permit
        .as_ref()
        .expect("actual sealed permit")
        .independently_retained_pin()
        .clone();
    let fact = pending.plan.as_ref().unwrap().record().unwrap();
    assert_eq!(fact.get("stage"), s("EscrowBoundIntent"));
    assert_ne!(fact.get("generationEscrow"), V::Undefined);
    assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 1);
    assert_eq!(
        pin.open_independently_pinned_history(&original)
            .unwrap()
            .original(),
        &original
    );
    owner.resolve_control_write().unwrap();
    owner.advance().unwrap();
    assert_ne!(
        owner.index().snapshot().unwrap().publication,
        before.publication
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded
    );
    let entry = owner
        .read_checked_history_entry(owner.history().last().unwrap())
        .unwrap();
    assert_ne!(entry.get("generationEscrow"), V::Undefined);
    assert!(owner.acknowledged_append.is_some());
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn lost_before_escrow_binding_rearms_only_wal_and_preserves_actual_permit() {
    let temp = escrow_temp();
    let (mut owner, archive) = escrow_owner(&temp, true);
    let before = owner.index().snapshot().unwrap();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::EscrowBoundIntent,
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
    ));
    assert!(owner.advance().is_err());
    let chain = owner
        .pending
        .as_ref()
        .unwrap()
        .escrow_chain
        .clone()
        .unwrap();
    let descriptor = owner
        .pending
        .as_ref()
        .unwrap()
        .escrow_permit
        .as_ref()
        .unwrap()
        .independently_retained_pin()
        .storage
        .canonical_descriptor_bytes()
        .unwrap();
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(owner.index().snapshot().unwrap(), before);
    assert_eq!(
        owner.pending.as_ref().unwrap().escrow_chain.as_deref(),
        Some(chain.as_slice())
    );
    assert_eq!(
        owner
            .pending
            .as_ref()
            .unwrap()
            .escrow_permit
            .as_ref()
            .unwrap()
            .independently_retained_pin()
            .storage
            .canonical_descriptor_bytes()
            .unwrap(),
        descriptor
    );
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Shutdown(vec![]))
        .is_err());
    owner.advance().unwrap();
    assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 1);
}
#[test]
fn escrow_compaction_midpoint_recovery_preserves_original_archive_chain_and_source_prefix() {
    let temp = escrow_temp();
    let (mut owner, _) = escrow_owner(&temp, true);
    let original = owner.index().capture_verified_history_pin().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    active_mut(&mut owner)
        .owner
        .index
        .native_main_test_compaction_fault("after-current-sync");
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(owner.advance().is_err());
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeRecoveryRequired
    );
    let chain = owner
        .pending
        .as_ref()
        .unwrap()
        .escrow_chain
        .clone()
        .unwrap();
    let retained = owner
        .pending
        .as_ref()
        .unwrap()
        .escrow_permit
        .as_ref()
        .unwrap()
        .independently_retained_pin()
        .clone();
    let no_index = owner.into_no_native_index().unwrap();
    let mut owner = no_index
        .try_reopen(
            &temp.root,
            Candidate::Intermediate,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    owner.advance().unwrap();
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        count
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    let entry = owner
        .read_checked_history_entry(owner.history().last().unwrap())
        .unwrap();
    assert_ne!(entry.get("generationEscrow"), V::Undefined);
    assert_eq!(
        retained
            .open_independently_pinned_history(&original)
            .unwrap()
            .original(),
        &original
    );
    let raw = owner
        .read_history_blob(&ActiveServiceWalBlobV1::of(&chain).unwrap())
        .unwrap();
    assert_eq!(raw, chain);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn corrupt_synced_escrow_blocks_prune_and_keeps_original_action_owner_held() {
    let temp = escrow_temp();
    let (mut owner, _) = escrow_owner(&temp, true);
    let before = owner.index().snapshot().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::EscrowBoundIntent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    let copy = owner
        .pending
        .as_ref()
        .unwrap()
        .escrow_permit
        .as_ref()
        .unwrap()
        .independently_retained_pin()
        .storage
        .copied_generation
        .join("RECORD");
    let mut bytes = std::fs::read(&copy).unwrap();
    bytes[0] ^= 1;
    std::fs::write(copy, bytes).unwrap();
    assert!(matches!(
        owner.resolve_control_write(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    assert_eq!(owner.index().snapshot().unwrap(), before);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .is_err());
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Shutdown(vec![]))
        .is_err());
}
