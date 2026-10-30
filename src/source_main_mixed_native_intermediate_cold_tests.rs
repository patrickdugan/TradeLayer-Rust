// Child of actual mixed cold controls. Raw source inputs and pre-Start tuning
// select the real storage operation; no financial/native result is supplied.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
#[derive(Clone, Copy)]
struct IntermediateInputs {
    offset: Option<u64>,
    retained_ancestors: usize,
    fault: Option<&'static str>,
}
fn primed_ordinary(
    t: &Temp,
    inputs: IntermediateInputs,
) -> (
    SourceMainNativeMixedFactServiceV1,
    Vec<u8>,
    NativeMainArchivePin,
    SourceIndexHistoryPinV1,
    u64,
    u64,
) {
    let (mut raw, documents, prime) = setup_primed(t, inputs.offset);
    let target = prime
        .checked_add(inputs.offset.unwrap_or(64 * 1024 * 1024))
        .unwrap();
    assert_eq!(inputs.retained_ancestors, 0);
    let declared = obj(&[
        ("profile", s("TL-MIXED-COLD-INTERMEDIATE-RAW-INPUT-1")),
        ("sourceCase", s("same-held-live-validity")),
        ("targetLogBytes", s(&target.to_string())),
        (
            "retainedAncestors",
            s(&inputs.retained_ancestors.to_string()),
        ),
        ("faultPoint", inputs.fault.map_or(V::Null, s)),
        ("generation", s("71")),
    ]);
    std::fs::write(
        t.root.join("raw-intermediate-inputs.json"),
        declared.diagnostic_json().unwrap(),
    )
    .unwrap();
    // The actual fault is armed BEFORE Start. Earlier writes stay below the
    // declared limit; only the real compaction invokes this point.
    if let Some(fault) = inputs.fault {
        raw.index.native_main_test_compaction_fault(fault);
    }
    let tip = raw.index().capture_verified_history_pin().unwrap();
    let (archive, pin) = raw.checkpoint().unwrap();
    let escrow = t.root.join("mixed-cold-intermediate-generation-escrow");
    std::fs::create_dir(&escrow).unwrap();
    let mut owner = SourceMainNativeMixedFactServiceV1::create(
        raw,
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
        escrow.canonicalize().unwrap(),
        crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
    )
    .unwrap();
    mixed_actor_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents,
        },
    );
    for _ in 0..40 {
        if let Some(write) = owner.active_service().unwrap().pending_write() {
            assert_eq!(write.append_plan().configuration().target_log_bytes, target);
            assert_eq!(
                write.append_plan().configuration().retained_ancestors,
                inputs.retained_ancestors
            );
            return (owner, archive, pin, tip, prime, target);
        }
        if owner
            .active_service()
            .unwrap()
            .pending_inline_write()
            .is_some()
        {
            mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
            mixed_actor_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
        } else {
            mixed_actor_action(
                &mut owner,
                NativeMainActiveServiceActionV1::Advance { limit: 4096 },
            );
        }
    }
    panic!("actual raw source did not reach an ordinary validity await")
}
#[test]
fn mixed_cold_actual_ordinary_intermediate_copy_retry_post_rebuilds_one_financial_owner_without_native_io(
) {
    // Calibrate operational native byte length only in an independent source
    // run. No observed financial/validity/returned value enters the fresh owner.
    let calibration = temp();
    let (mut probe, _, _, _, prime, _) = primed_ordinary(
        &calibration,
        IntermediateInputs {
            offset: None,
            retained_ancestors: 0,
            fault: None,
        },
    );
    let at = probe.index().footprint().unwrap().journal_file_bytes;
    assert!(probe
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .append_plan()
        .intermediate()
        .is_none());
    mixed_actor_action(&mut probe, NativeMainActiveServiceActionV1::AppendPending);
    let bytes = probe
        .index()
        .footprint()
        .unwrap()
        .journal_file_bytes
        .checked_sub(at)
        .unwrap();
    assert!(bytes > 1);
    let offset = at
        .checked_sub(prime)
        .unwrap()
        .checked_add(bytes - 1)
        .unwrap();
    drop(probe);
    let inputs = IntermediateInputs {
        offset: Some(offset),
        retained_ancestors: 0,
        fault: Some("after-current-sync"),
    };
    let live_decode_before = crate::source_contract_runtime::native_main_test_row_decode_total();
    let t = temp();
    let (mut owner, archive, pin, tip, real_prime, target) = primed_ordinary(&t, inputs);
    assert_eq!(prime, real_prime);
    let live_decode_delta = owner
        .financial_runtime()
        .native_main_test_row_decode_count()
        .checked_sub(live_decode_before)
        .unwrap();
    assert_eq!(live_decode_delta, 1);
    assert!(!owner.financial_runtime().collection("tallyMap").is_empty());
    let source_finance = owner.financial_runtime().state_value().unwrap();
    let source_effects = owner.active_service().unwrap().effects().to_vec();
    let decode = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let before = owner.index().capture_verified_history_pin().unwrap();
    let query = owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .query()
        .clone();
    let arguments = owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .update_arguments()
        .to_vec();
    let original_context = owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .durable_read_context();
    let mutations = crate::source_main_native_intent_store::native_prepared_mutation_bytes(
        owner
            .active_service()
            .unwrap()
            .pending_write()
            .unwrap()
            .append_plan()
            .mutations(),
    )
    .unwrap();
    assert!(
        owner
            .active_service()
            .unwrap()
            .pending_write()
            .unwrap()
            .append_plan()
            .intermediate()
            .is_some(),
        "actual ordinary native append must choose compaction under declared tuning"
    );
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    let expected_copy = owner
        .actual()
        .inner
        .pending
        .as_ref()
        .unwrap()
        .escrow_permit
        .as_ref()
        .unwrap()
        .independently_retained_pin()
        .clone();
    assert_eq!(expected_copy.points[0], before);
    assert_eq!(
        owner.financial_runtime().state_value().unwrap(),
        source_finance
    );
    assert_eq!(owner.active_service().unwrap().effects(), source_effects);
    let verified = owner
        .into_no_index()
        .unwrap()
        .try_reopen(
            &t.root,
            Candidate::Intermediate,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    assert_eq!(verified.candidate(), Candidate::Intermediate);
    let mut owner = verified.prepare_verified_replacement().unwrap();
    let middle = owner.index().capture_verified_history_pin().unwrap();
    assert_eq!(middle.metadata.sequence, before.metadata.sequence);
    assert_ne!(middle.metadata.publication, before.metadata.publication);
    assert_eq!(middle.storage.content, before.storage.content);
    let replacement = owner
        .actual()
        .inner
        .pending
        .as_ref()
        .unwrap()
        .native_recovery
        .as_ref()
        .unwrap()
        .replacement
        .as_ref()
        .unwrap();
    assert_eq!(replacement.query(), &query);
    assert_eq!(replacement.update_arguments(), arguments);
    assert_eq!(
        replacement.append_plan().configuration().target_log_bytes,
        target
    );
    assert_eq!(
        replacement.append_plan().configuration().retained_ancestors,
        0
    );
    assert!(replacement.append_plan().intermediate().is_none());
    assert_eq!(
        crate::source_main_native_intent_store::native_prepared_mutation_bytes(
            replacement.append_plan().mutations()
        )
        .unwrap(),
        mutations
    );
    let replacement_context = replacement.durable_read_context();
    assert_ne!(replacement_context, original_context);
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(
        owner.financial_runtime().state_value().unwrap(),
        source_finance
    );
    assert_eq!(owner.active_service().unwrap().effects(), source_effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decode
    );
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .pending_write()
            .unwrap()
            .durable_read_context(),
        replacement_context
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
    let post = owner.index().capture_verified_history_pin().unwrap();
    assert_eq!(post.metadata.sequence, before.metadata.sequence + 1);
    assert_eq!(post.metadata.publication, middle.metadata.publication);
    let state = owner.financial_runtime().state_value().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let receipts = owner.active_service().unwrap().history().to_vec();
    let kernel = owner.active_service().unwrap().kernel_value();
    let source_head = owner.active_service().unwrap().head();
    let count = owner.acknowledged_actions();
    let wal_head = owner.head();
    let captured = owner.capture_cold_history_pin().unwrap();
    let mut actual = owner.inner.unwrap();
    let ActiveWalOwner::Active(active) = actual.inner.owner.take().unwrap() else {
        panic!("actual source owner")
    };
    let mut index = active.owner.index;
    drop(actual);
    let mut reader = VerifiedMixedNativeHistoryV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &captured,
    )
    .unwrap();
    let window = reader.read_window(0).unwrap();
    let access = reader.window_access(&window).unwrap();
    let original_view = access.checked_original_view(&mut index, &post).unwrap();
    assert!(original_view.copied_file().is_none());
    let copied_origin = access.checked_original_view(&mut index, &tip).unwrap();
    assert!(copied_origin.copied_file().is_some());
    assert_eq!(copied_origin.original(), &tip);
    let copied_before = access.checked_original_view(&mut index, &before).unwrap();
    assert!(copied_before.copied_file().is_some());
    assert_eq!(copied_before.original(), &before);
    let cold_decode_before = crate::source_contract_runtime::native_main_test_row_decode_total();
    let io = index.storage_io_counters().unwrap();
    let prefix = NativeMainColdPrefixV1::reexecute_with_mixed_fact_views(
        &archive,
        &NativeMainColdPrefixExpectationV1 {
            completed: NativeMainHistoricalRowsExpectationV1 {
                archive: pin,
                current: tip,
                commands: vec![],
            },
            actual_post: post.clone(),
        },
        index,
        &mut reader,
    )
    .unwrap();
    let cold =
        NativeMainColdMixedFactServiceV1::reexecute(prefix, reader, 64 * 1024 * 1024).unwrap();
    assert_eq!(cold.verified_actions(), count);
    assert_eq!(cold.acknowledged_wal_head(), wal_head);
    assert_eq!(cold.state.head, source_head);
    assert_eq!(cold.financial_runtime().state_value().unwrap(), state);
    assert_eq!(cold.state.effects(), effects);
    assert_eq!(cold.state.history, receipts);
    assert_eq!(
        cold.state.frame.as_ref().unwrap().kernel_value(),
        kernel.unwrap()
    );
    assert_eq!(cold.actual_post(), &post);
    let source = cold
        .state
        .frame
        .as_ref()
        .unwrap()
        .mixed_historical_validity()
        .unwrap();
    assert_eq!(source.query(), &query);
    assert_eq!(source.update_arguments(), arguments);
    assert_eq!(source.exact_prepared_mutations(), mutations);
    assert_eq!(source.durable_read_context(), replacement_context);
    assert!(source.append_verified());
    assert!(!source.resolved());
    assert_eq!(
        cold.financial_runtime()
            .native_main_test_row_decode_count()
            .checked_sub(cold_decode_before)
            .unwrap(),
        live_decode_delta
    );
    let after = cold.index().storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    assert_eq!(cold.index().capture_verified_history_pin().unwrap(), post);
}
