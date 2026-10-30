// Child of the existing actual-inline source fixture tests. Uses raw genuine
// source construction, one moved financial owner and actual native files.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
fn mixed(t: &Temp) -> SourceMainNativeMixedFactServiceV1 {
    let (owner, documents) = setup(t, "same-held-live-validity");
    let root = t.root.join("mixed-actual-generation-escrow");
    std::fs::create_dir(&root).unwrap();
    let mut service = SourceMainNativeMixedFactServiceV1::create(
        owner,
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        root.canonicalize().unwrap(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    action(
        &mut service,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents,
        },
    );
    service
}
fn action(service: &mut SourceMainNativeMixedFactServiceV1, raw: NativeMainActiveServiceActionV1) {
    let count = service.acknowledged_actions();
    service.prepare_action(raw).unwrap();
    assert_eq!(service.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert!(!service.fact_barrier_pending());
    assert_eq!(service.acknowledged_actions(), count + 1);
}
fn ordinary(t: &Temp) -> SourceMainNativeMixedFactServiceV1 {
    let mut service = mixed(t);
    for _ in 0..32 {
        let active = service.active_service().unwrap();
        if active.pending_write().is_some() {
            return service;
        }
        if active.pending_inline_write().is_some() {
            action(&mut service, NativeMainActiveServiceActionV1::AppendPending);
            action(&mut service, NativeMainActiveServiceActionV1::ResolvePost);
        } else {
            action(
                &mut service,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            );
        }
    }
    panic!("real source ordinary await not reached")
}
fn finish_failed_before(t: &Temp) -> SourceMainNativeMixedFactServiceV1 {
    let mut owner = ordinary(t);
    active_mut(&mut owner.actual_mut().inner)
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
    owner
        .into_no_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Before, Recovery::DiscardIncompleteTail)
        .unwrap()
        .prepare_verified_replacement()
        .unwrap()
}
#[test]
fn mixed_real_source_records_both_plan_kinds_and_no_append_reenters_finance() {
    let t = temp();
    let mut owner = mixed(&t);
    let mut kinds = vec![];
    for _ in 0..48 {
        let active = owner.active_service().unwrap();
        if active.pending_inline_write().is_some() || active.pending_write().is_some() {
            let financial = active.financial_runtime().state_value().unwrap();
            let effects = active.effects().to_vec();
            let decode = active
                .financial_runtime()
                .native_main_test_row_decode_count();
            action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            let record = owner.actual().inner.wal.record().unwrap();
            assert_eq!(record.get("profile"), s(MIXED_SOURCE_FACT_WAL_PROFILE));
            kinds.push(record.get("mixedOperationKind"));
            let active = owner.active_service().unwrap();
            assert_eq!(active.financial_runtime().state_value().unwrap(), financial);
            assert_eq!(active.effects(), effects);
            assert_eq!(
                active
                    .financial_runtime()
                    .native_main_test_row_decode_count(),
                decode
            );
            if kinds.contains(&s("InlineDatabase")) && kinds.contains(&s("OrdinaryValidity")) {
                break;
            }
            action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        } else {
            action(
                &mut owner,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            );
        }
    }
    assert!(kinds.contains(&s("InlineDatabase")));
    assert!(kinds.contains(&s("OrdinaryValidity")));
    let native = owner.index().capture_verified_history_pin().unwrap();
    let financial = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let record = owner.actual().inner.wal.record().unwrap();
    assert_eq!(record.get("mixedOperationKind"), s("SourceAction"));
    assert_eq!(record.get("ordinaryNativeIoFact"), V::Undefined);
    assert_eq!(record.get("actualNativeIoFact"), V::Undefined);
    let raw = fact_blob(owner.actual(), record.get("accepted"));
    let fields = u64::from_be_bytes(raw[6..14].try_into().unwrap()) as usize;
    let diagnostic = fact_parse(&raw[14 + fields + 8..]);
    assert_eq!(diagnostic.get("apiRejection"), s("WriteAlreadyAttempted"));
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        native
    );
    assert_eq!(owner.financial_runtime().state_value().unwrap(), financial);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
}
#[test]
fn mixed_ordinary_before_retry_pins_mapping_before_one_native_call_and_keeps_first_error() {
    let t = temp();
    let mut owner = finish_failed_before(&t);
    let financial = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let decode = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let before = owner.index().capture_verified_history_pin().unwrap();
    owner.actual_mut().inner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert!(
        !owner
            .actual()
            .inner
            .pending
            .as_ref()
            .unwrap()
            .native_recovery
            .as_ref()
            .unwrap()
            .installed
    );
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.financial_runtime().state_value().unwrap(), financial);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode
    );
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
    let slots = owner.actual().inner.wal.record().unwrap();
    assert_ne!(slots.get("ordinaryNativeIoChain"), V::Undefined);
    action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    let rejected = owner.actual().inner.wal.record().unwrap();
    assert_eq!(rejected.get("ordinaryNativeIoFact"), V::Undefined);
    let raw = fact_blob(owner.actual(), rejected.get("accepted"));
    let fields = u64::from_be_bytes(raw[6..14].try_into().unwrap()) as usize;
    assert_eq!(
        fact_parse(&raw[14 + fields + 8..]).get("apiRejection"),
        s("WriteAlreadyAttempted")
    );
    action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn mixed_ordinary_second_lost_return_then_post_preserves_both_actual_errors() {
    let t = temp();
    let mut owner = finish_failed_before(&t);
    let effects = owner.active_service().unwrap().effects().to_vec();
    let financial = owner.financial_runtime().state_value().unwrap();
    active_mut(&mut owner.actual_mut().inner)
        .owner
        .index
        .test_fail_after_sync();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let mut owner = owner
        .into_no_index()
        .unwrap()
        .try_reopen(&t.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap()
        .finish_verified_post()
        .unwrap();
    let post = owner.index().capture_verified_history_pin().unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.index().capture_verified_history_pin().unwrap(), post);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(owner.financial_runtime().state_value().unwrap(), financial);
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
    action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn mixed_checkpoint_keeps_actual_ordinary_plan_source_head_effects_and_global_action_chain() {
    let t = temp();
    let mut owner = ordinary(&t);
    let context = owner.active_service().unwrap().pending_native_context();
    let financial = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    for _ in 0..4 {
        action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    }
    let actions = owner.acknowledged_actions();
    let head = owner.head();
    let source_head = owner.active_service().unwrap().head();
    let native = owner.index().capture_verified_history_pin().unwrap();
    owner.prepare_checkpoint().unwrap();
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .is_err());
    owner.attempt_checkpoint().unwrap();
    let archived = owner.finish_checkpoint().unwrap();
    assert_eq!(archived.actions, actions);
    assert!(owner.history_window().is_empty());
    assert_eq!(owner.head(), head);
    assert_eq!(owner.active_service().unwrap().head(), source_head);
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        native
    );
    assert_eq!(
        owner.active_service().unwrap().pending_native_context(),
        context
    );
    assert_eq!(owner.financial_runtime().state_value().unwrap(), financial);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(owner.acknowledged_actions(), actions + 1);
}
#[test]
fn mixed_typed_native_fact_lost_ack_blocks_source_delivery_and_reappend_until_exact_post() {
    let t = temp();
    let mut owner = ordinary(&t);
    let financial = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let decode = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    owner.actual_mut().inner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Observe,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(owner.advance().is_err());
    assert!(owner.fact_barrier_pending());
    assert!(!owner.active_service().unwrap().observation_pending());
    assert!(owner
        .actual()
        .inner
        .pending
        .as_ref()
        .and_then(|p| p.mixed_ordinary_slots.as_ref())
        .unwrap()
        .attempt
        .is_some());
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .is_err());
    let native = owner.index().capture_verified_history_pin().unwrap();
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        native
    );
    assert_eq!(owner.financial_runtime().state_value().unwrap(), financial);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode
    );
}
