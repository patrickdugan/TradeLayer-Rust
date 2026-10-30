// Nested under existing inline-WAL raw-fixture tests. Parent Core gate only.
use super::*;
fn fact_action(
    owner: &mut SourceMainNativeActualFactServiceV1,
    action: NativeMainActiveServiceActionV1,
) {
    let count = owner.history().len();
    owner.prepare_action(action).unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert!(!owner.fact_barrier_pending());
    assert_eq!(owner.history().len(), count + 1);
}
fn fact_setup(t: &Temp) -> SourceMainNativeActualFactServiceV1 {
    let (raw, documents) = setup(t, "same-held-live-validity");
    let escrow = t.root.join("actual-fact-generation-escrow");
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
            return owner;
        }
        fact_action(
            &mut owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let active = owner.inner.active_service().unwrap();
        if active.pending_inline_write().is_some() {
            return owner;
        }
        if active.pending_write().is_some() {
            fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("genuine raw source input did not reach inline await")
}
fn fact_parse(raw: &[u8]) -> V {
    crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(raw).unwrap()).unwrap()
}
fn fact_blob(owner: &SourceMainNativeActualFactServiceV1, value: V) -> Vec<u8> {
    let V::String(size) = value.get("bytes") else {
        panic!("blob size")
    };
    let V::String(hash) = value.get("sha256") else {
        panic!("blob hash")
    };
    let mut sha256 = [0; 32];
    for (i, pair) in hash.as_bytes().chunks_exact(2).enumerate() {
        sha256[i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    owner
        .read_history_blob(&ActiveServiceWalBlobV1 {
            bytes: size.parse().unwrap(),
            sha256,
        })
        .unwrap()
}
#[test]
fn actual_source_fact_writer_tracks_three_successive_native_plans_without_resuming_during_append() {
    let t = temp();
    let mut owner = fact_setup(&t);
    let mut contexts = vec![];
    for number in 0..3 {
        let active = owner.inner.active_service().unwrap();
        let context = active
            .pending_inline_write()
            .expect("real current plan")
            .durable_read_context();
        let financial = active.financial_runtime().state_value().unwrap();
        let effects = active.effects().to_vec();
        contexts.push(context.clone());
        fact_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
        let record = owner.inner.wal.record().unwrap();
        assert_eq!(record.get("profile"), s(ACTUAL_SOURCE_FACT_WAL_PROFILE));
        assert_eq!(record.get("actualNativeReadContext"), context);
        let raw = fact_blob(&owner, record.get("actualNativeIoFact"));
        let io = fact_parse(&raw);
        assert_eq!(
            io.get("actualOutcome").get("kind"),
            s("NativeAppendReturned")
        );
        assert_ne!(io.get("verifiedPostHex"), V::Null);
        let pending = fact_parse(&fact_blob(&owner, record.get("actualNativeIoPendingFact")));
        assert_eq!(pending.get("nativeReadContext"), context);
        assert_eq!(
            io.get("actualPendingFactSHA256"),
            s(&hex(&sha256(&fact_blob(
                &owner,
                record.get("actualNativeIoPendingFact")
            ))))
        );
        let active = owner.inner.active_service().unwrap();
        assert_eq!(active.financial_runtime().state_value().unwrap(), financial);
        assert_eq!(active.effects(), effects);
        if number < 2 {
            fact_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    assert!(contexts.windows(2).all(|pair| pair[0] != pair[1]));
}
#[test]
fn actual_source_native_lost_return_is_durable_before_recovery_and_cannot_escape_via_pause() {
    let t = temp();
    let mut owner = fact_setup(&t);
    let active = owner.inner.active_service().unwrap();
    let financial = active.financial_runtime().state_value().unwrap();
    let effects = active.effects().to_vec();
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
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeRecoveryRequired
    );
    assert!(!owner.fact_barrier_pending());
    let record = owner.inner.wal.record().unwrap();
    assert_eq!(record.get("stage"), s("NativeIoObserved"));
    let io = fact_parse(&fact_blob(&owner, record.get("actualNativeIoFact")));
    assert_eq!(io.get("actualOutcome").get("kind"), s("NativeIoError"));
    assert_eq!(io.get("verifiedPostHex"), V::Null);
    let display = io.get("actualOutcome").get("exactSourceDisplay");
    assert!(matches!(display, V::String(_)));
    let active = owner.inner.active_service().unwrap();
    assert_eq!(active.financial_runtime().state_value().unwrap(), financial);
    assert_eq!(active.effects(), effects);
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .is_err());
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Shutdown(vec![]))
        .is_err());
}
#[test]
fn lost_fact_barrier_return_resolves_only_actual_control_plan_and_does_not_reappend() {
    let t = temp();
    let mut owner = fact_setup(&t);
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.inner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Observe,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert!(owner.fact_barrier_pending());
    let sequence = owner.inner.index().snapshot().unwrap().sequence;
    let financial = owner.inner.financial_runtime().state_value().unwrap();
    let effects = owner.inner.active_service().unwrap().effects().to_vec();
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .is_err());
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.inner.index().snapshot().unwrap().sequence, sequence);
    assert_eq!(
        owner.inner.financial_runtime().state_value().unwrap(),
        financial
    );
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
}
#[test]
fn exact_before_fact_barrier_rearms_control_only_without_repeating_executed_pause() {
    let t = temp();
    let mut owner = fact_setup(&t);
    owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .unwrap();
    owner.inner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Observe,
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
    ));
    assert!(owner.advance().is_err());
    assert!(owner.fact_barrier_pending());
    assert!(owner.inner.pending.as_ref().unwrap().executed);
    let actual = owner.inner.active_service().unwrap();
    let kernel = actual.kernel_value();
    let effects = actual.effects().to_vec();
    let financial = actual.financial_runtime().state_value().unwrap();
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    let actual = owner.inner.active_service().unwrap();
    assert_eq!(actual.kernel_value(), kernel);
    assert_eq!(actual.effects(), effects);
    assert_eq!(actual.financial_runtime().state_value().unwrap(), financial);
}
#[test]
fn lost_intent_post_resolution_syncs_new_source_facts_before_returning() {
    let t = temp();
    let mut owner = fact_setup(&t);
    owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .unwrap();
    owner.inner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Intent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert!(!owner.inner.pending.as_ref().unwrap().executed);
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert!(owner.inner.pending.as_ref().unwrap().executed);
    assert!(!owner.fact_barrier_pending());
    assert_eq!(
        owner.inner.wal.record().unwrap().get("stage"),
        s("SourcePendingPlanObserved")
    );
    let effects = owner.inner.active_service().unwrap().effects().to_vec();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.inner.active_service().unwrap().effects(), effects);
}
#[test]
fn facts_origin_is_explicit_and_old_mode4_reader_refuses_it() {
    let t = temp();
    let mut owner = fact_setup(&t);
    let origin = owner.inner.wal.origin_context().unwrap().get("origin");
    assert_eq!(origin.get("profile"), s(ACTUAL_SOURCE_FACT_WAL_PROFILE));
    assert_eq!(
        origin.get("actualNativeFactScope"),
        s(ACTUAL_SOURCE_FACT_SCOPE)
    );
    assert!(owner.inner.capture_inline_cold_pin().is_err());
    let pin = owner.wal_pin().unwrap();
    drop(owner);
    assert!(
        crate::source_main_active_service_wal::VerifiedActiveServiceAckHistoryV1::open(
            &t.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &pin,
        )
        .is_err()
    );
}

#[path = "source_main_actual_fact_recovery_tests.rs"]
mod actual_fact_native_recovery_tests;

#[path = "source_main_actual_fact_trace_core_tests.rs"]
mod actual_fact_trace_core_tests;
