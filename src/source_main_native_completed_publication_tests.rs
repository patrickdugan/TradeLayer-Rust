use super::*;
use crate::source_contract_replay::source_durable::SourceDurableFilesystem;
use crate::source_contract_replay::{SourceContractCommand, SourceContractCommandOutcome};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "moved-completed-publication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        if let Ok(root) = self.0.canonicalize() {
            assert_eq!(root.parent(), Some(parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn invoke(financial: &mut SourceContractReplay, target: &str, arguments: Vec<V>) {
    let receipt = financial
        .apply(SourceContractCommand::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        }))
        .unwrap();
    assert!(matches!(
        receipt.outcome,
        SourceContractCommandOutcome::Dependency(
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
        )
    ));
}
fn row() -> V {
    obj(&[
        (
            "_id",
            s("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8"),
        ),
        (
            "value",
            obj(&[
                (
                    "txId",
                    s("ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8"),
                ),
                ("marker", s("tl")),
                ("payload", s("00,production-code")),
                (
                    "sender",
                    obj(&[
                        ("senderAddress", s("MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB")),
                        ("amount", V::number(0.)),
                    ]),
                ),
                ("reference", V::Array(vec![])),
            ]),
        ),
    ])
}
fn owner_mode(temp: &Temp, include_row: bool) -> SourceMainNativeSessionV13 {
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    invoke(
        &mut financial,
        "SourceActivation.configureClient",
        vec![obj(&[("chain", s("LTC")), ("test", V::Bool(false))])],
    );
    invoke(&mut financial, "Activation.init", vec![s("LTC")]);
    invoke(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
            ),
            (
                "io",
                obj(&[
                    ("height", V::number(10.)),
                    (
                        "networkInfo",
                        obj(&[
                            ("networkactive", V::Bool(true)),
                            ("connections", V::number(2.)),
                        ]),
                    ),
                    ("blocks", V::Array(vec![])),
                ]),
            ),
            ("genesisBlock", V::number(10.)),
        ])],
    );
    financial
        .apply(SourceContractCommand::SetHeight(V::number(10.)))
        .unwrap();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index =
        SourceCompactIndex::create(&temp.0, namespace.clone(), "moved raw row command").unwrap();
    if include_row {
        index.import_row(&index.snapshot().unwrap(), row()).unwrap();
    }
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace,
        metadata: index.snapshot().unwrap(),
    };
    SourceMainNativeSessionV13::adopt(NativeMainOwners::adopt(financial, index, &expected).unwrap())
        .unwrap()
}

fn owner(temp: &Temp) -> SourceMainNativeSessionV13 {
    owner_mode(temp, true)
}
fn control(temp: &Temp) -> NativeIntentStoreV1 {
    NativeIntentStoreV1::create(
        &temp.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("profile", s("actual completed native publication"))]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap()
}
fn begin(
    owner: SourceMainNativeSessionV13,
    store: NativeIntentStoreV1,
    command: NativeMainSessionCommand,
) -> SourceMainNativeRowSessionV1 {
    let expected = owner.expectation().unwrap();
    let pin = store.pin().unwrap();
    owner
        .into_row_session(store, &expected, &pin, command, 8 * 1024 * 1024)
        .unwrap()
}
fn finish(controller: &mut SourceMainNativeRowSessionV1) {
    let mut stop = controller.advance();
    for _ in 0..20 {
        match stop {
            NativeMainRowStopV1::Complete => return,
            NativeMainRowStopV1::AwaitNativeWrite => stop = controller.commit_pending(),
            other => panic!("actual command stopped: {other:?}"),
        }
    }
    panic!("actual bounded command did not finish")
}
#[test]
fn successive_actual_row_commands_publish_legacy_logical_heads_and_reexecute_from_raw_origin() {
    let actual_temp = Temp::new();
    let reference_temp = Temp::new();
    let control_temp = Temp::new();
    let raw_temp = Temp::new();
    let mut actual = owner(&actual_temp);
    let mut reference = owner(&reference_temp);
    let mut store = control(&control_temp);
    assert_eq!(actual.head(), reference.head());
    let commands = [
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![]),
            height: V::number(10.),
            live: false,
        },
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![]),
            height: V::number(11.),
            live: true,
        },
    ];
    for command in commands {
        let original = actual.expectation().unwrap();
        let old_head = actual.head();
        let expected = reference
            .apply(&reference.expectation().unwrap(), command.clone())
            .unwrap()
            .clone();
        let mut controller = begin(actual, store, command);
        assert!(controller.completed_native_head().is_none());
        finish(&mut controller);
        let published = controller.completed_native_receipt().unwrap();
        assert_eq!(published.before, original.physical);
        assert_eq!(published.sequence, old_head.commands);
        assert_eq!(published.previous_head, old_head.commitment);
        assert_eq!(
            logical_receipt_bytes(published).unwrap(),
            logical_receipt_bytes(&expected).unwrap()
        );
        assert_eq!(
            controller.completed_native_head().unwrap(),
            reference.head()
        );
        assert_eq!(published.effects, expected.effects);
        assert_eq!(controller.effects(), published.effects);
        assert!(controller
            .owner
            .financial
            .runtime()
            .native_main_row_effects()
            .is_empty());
        let head = controller.completed_native_head().unwrap();
        let history = controller.owner.history.len();
        assert_eq!(controller.commit_pending(), NativeMainRowStopV1::Complete);
        assert_eq!(controller.owner.history.len(), history);
        assert_eq!(controller.completed_native_head().unwrap(), head);
        (actual, store) = controller.into_completed_owners().unwrap();
        assert_eq!(actual.head(), reference.head());
        assert_eq!(actual.history().len(), history);
        assert!(actual
            .financial
            .runtime()
            .native_main_row_effects()
            .is_empty());
    }
    let (bytes, pin) = actual.checkpoint().unwrap();
    let mut raw = SourceCompactIndex::create(
        &raw_temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "moved raw row command",
    )
    .unwrap();
    raw.import_row(&raw.snapshot().unwrap(), row()).unwrap();
    let raw_pin = physical(&raw).unwrap();
    let reconstructed = SourceMainNativeSessionV13::reexecute(&bytes, &pin, raw, &raw_pin).unwrap();
    assert_eq!(reconstructed.head(), actual.head());
    assert_eq!(reconstructed.history().len(), 4);
    assert_eq!(
        reconstructed
            .financial
            .native_main_financial_commitment()
            .unwrap(),
        actual.financial.native_main_financial_commitment().unwrap()
    );
    let _retained_store = store;
}
#[test]
fn unfinished_release_returns_whole_owner_without_draining_or_exposing_an_old_head() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let head = actual.head();
    let store = control(&control_temp);
    let controller = begin(
        actual,
        store,
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
    );
    let mut controller = match controller.into_completed_owners() {
        Err(rejected) => rejected.into_session(),
        Ok(_) => panic!("unfinished owner was released"),
    };
    assert_eq!(controller.owner.head, head);
    assert!(controller.completed_native_head().is_none());
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    let financial = controller
        .owner
        .financial
        .native_main_financial_commitment()
        .unwrap();
    let effects = controller.effects().to_vec();
    let pin = controller.store_pin().unwrap();
    let mut retained = match controller.into_completed_owners() {
        Err(rejected) => rejected.into_session(),
        Ok(_) => panic!("unfinished owner was released"),
    };
    assert_eq!(retained.owner.head, head);
    assert_eq!(retained.effects(), effects);
    assert_eq!(
        retained
            .owner
            .financial
            .native_main_financial_commitment()
            .unwrap(),
        financial
    );
    assert_eq!(retained.store_pin().unwrap(), pin);
    assert!(retained.completed_native_head().is_none());
    assert_eq!(retained.commit_pending(), NativeMainRowStopV1::Complete);
    assert_eq!(
        retained.completed_native_head().unwrap().commands,
        head.commands + 1
    );
}
#[test]
fn recovered_native_post_publishes_original_before_once_without_new_source_or_effect_drain() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let before = actual.expectation().unwrap();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
    );
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let effects = controller.effects().to_vec();
    controller.owner.index.test_fail_after_sync();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(controller.owner.head, before.session);
    assert_eq!(controller.effects(), effects);
    let loan = controller.into_no_index().unwrap();
    let mut recovered = loan
        .try_reopen(
            &source.0,
            crate::source_compact_index_v2::NativePreparedValidityCandidateV1::Post,
            crate::storage::Recovery::RejectIncompleteTail,
        )
        .unwrap();
    assert_eq!(
        recovered.resolve_pending_post(),
        NativeMainRowStopV1::Complete
    );
    let receipt = recovered.completed_native_receipt().unwrap();
    assert_eq!(receipt.before, before.physical);
    assert_eq!(receipt.effects, effects);
    assert_eq!(receipt.sequence, before.session.commands);
    assert!(recovered
        .owner
        .financial
        .runtime()
        .native_main_row_effects()
        .is_empty());
    let (owner, _store) = recovered.into_completed_owners().unwrap();
    assert_eq!(owner.history().len(), 1);
}

#[test]
fn actual_empty_index_catchup_completion_publishes_the_existing_grouping_and_chain() {
    let actual_temp = Temp::new();
    let reference_temp = Temp::new();
    let control_temp = Temp::new();
    let actual = owner_mode(&actual_temp, false);
    let mut reference = owner_mode(&reference_temp, false);
    let command = NativeMainSessionCommand::ConstructConsensusFromIndex {
        start: V::number(10.),
    };
    let expected = reference
        .apply(&reference.expectation().unwrap(), command.clone())
        .unwrap()
        .clone();
    assert!(matches!(
        expected.status,
        NativeMainSessionStatus::Completed(_)
    ));
    let mut controller = begin(actual, control(&control_temp), command);
    finish(&mut controller);
    let published = controller.completed_native_receipt().unwrap();
    assert_eq!(
        logical_receipt_bytes(published).unwrap(),
        logical_receipt_bytes(&expected).unwrap()
    );
    assert!(published.grouping.as_ref().unwrap().complete);
    assert_eq!(published.grouping.as_ref().unwrap().rows, 0);
    let (actual, _store) = controller.into_completed_owners().unwrap();
    assert_eq!(actual.head(), reference.head());
    assert_eq!(actual.history().len(), 1);
}
#[test]
fn actual_live_tail_local_refusal_keeps_history_private_and_all_effects_retained() {
    let actual_temp = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&actual_temp);
    let head = actual.head();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ConstructConsensusFromIndex {
            start: V::number(10.),
        },
    );
    let mut stop = controller.advance();
    for _ in 0..10 {
        if stop == NativeMainRowStopV1::AwaitNativeWrite {
            stop = controller.commit_pending()
        } else {
            break;
        }
    }
    assert_eq!(
        stop,
        NativeMainRowStopV1::SourceStopped(SourceRuleError::UnknownDependency)
    );
    assert!(controller.completed_native_head().is_none());
    assert_eq!(controller.owner.head, head);
    assert!(controller.owner.history.is_empty());
    let effects = controller.effects().to_vec();
    assert!(!effects.is_empty());
    let rejected = match controller.into_completed_owners() {
        Err(rejected) => rejected,
        Ok(_) => panic!("live tail Local was released"),
    };
    let retained = rejected.into_session();
    assert_eq!(retained.effects(), effects);
    assert_eq!(retained.owner.head, head);
}
#[test]
fn actual_source_effect_capacity_never_publishes_or_drains_a_local_prefix() {
    let actual_temp = Temp::new();
    let control_temp = Temp::new();
    let mut actual = owner(&actual_temp);
    actual
        .financial
        .native_main_runtime_mut()
        .native_main_completed_test_exhaust_effects();
    let head = actual.head();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![]),
            height: V::number(10.),
            live: false,
        },
    );
    assert_eq!(
        controller.advance(),
        NativeMainRowStopV1::SourceStopped(SourceRuleError::Capacity)
    );
    assert_eq!(controller.effects().len(), 65536);
    assert_eq!(controller.owner.head, head);
    assert!(controller.completed_native_receipt().is_none());
    let rejected = match controller.into_completed_owners() {
        Err(rejected) => rejected,
        Ok(_) => panic!("Local capacity prefix was released"),
    };
    let retained = rejected.into_session();
    assert_eq!(retained.effects().len(), 65536);
    assert!(retained.owner.history.is_empty());
}

#[test]
fn next_command_refuses_stale_head_and_retains_newly_published_owner_and_store() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let stale = actual.expectation().unwrap();
    let command = NativeMainSessionCommand::ProcessRows {
        rows: V::Array(vec![]),
        height: V::number(10.),
        live: false,
    };
    let mut controller = begin(actual, control(&control_temp), command.clone());
    finish(&mut controller);
    let published = controller.completed_native_head().unwrap();
    let (actual, store) = controller.into_completed_owners().unwrap();
    let pin = store.pin().unwrap();
    let rejected =
        match actual.into_row_session(store, &stale, &pin, command.clone(), 8 * 1024 * 1024) {
            Err(rejected) => rejected,
            Ok(_) => panic!("stale native head was admitted"),
        };
    let (actual, store) = rejected.into_owners();
    assert_eq!(actual.head(), published);
    assert_eq!(store.pin().unwrap(), pin);
    assert!(actual
        .financial
        .runtime()
        .native_main_row_effects()
        .is_empty());
    let mut next = begin(actual, store, command);
    finish(&mut next);
    assert_eq!(
        next.completed_native_head().unwrap().commands,
        published.commands + 1
    );
}

#[test]
fn prefix_ack_control_post_recovery_preserves_publication_start_and_publishes_once() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let original = actual.expectation().unwrap();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
    );
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let effects = controller.effects().to_vec();
    let absent = controller.into_no_control_store().unwrap();
    assert_eq!(absent.publication.start.head, original.session);
    assert_eq!(absent.publication.start.before, original.physical);
    let mut recovered = absent
        .try_reopen_synced_post(
            &control_temp.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
    assert!(recovered.completed_native_head().is_none());
    assert_eq!(recovered.effects(), effects);
    assert_eq!(
        recovered.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    let receipt = recovered.completed_native_receipt().unwrap();
    assert_eq!(receipt.before, original.physical);
    assert_eq!(receipt.previous_head, original.session.commitment);
    assert_eq!(receipt.sequence, original.session.commands);
    assert_eq!(receipt.effects, effects);
    assert_eq!(recovered.owner.history.len(), 1);
    assert!(recovered
        .owner
        .financial
        .runtime()
        .native_main_row_effects()
        .is_empty());
    let head = recovered.completed_native_head().unwrap();
    assert_eq!(recovered.commit_pending(), NativeMainRowStopV1::Complete);
    assert_eq!(recovered.completed_native_head().unwrap(), head);
    let (_owner, _store) = recovered.into_completed_owners().unwrap();
}

#[test]
fn superseding_intent_control_recovery_keeps_original_before_and_completed_receipt() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let original = actual.expectation().unwrap();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
    );
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller.owner.index.test_fail_after_partial_write();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let mut controller = controller
        .into_no_index()
        .unwrap()
        .try_reopen(
            &source.0,
            crate::source_compact_index_v2::NativePreparedValidityCandidateV1::Before,
            crate::storage::Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    assert_eq!(
        controller.prepare_superseding(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.sync_prepared_superseding(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let absent = controller.into_no_control_store().unwrap();
    assert_eq!(absent.publication.start.head, original.session);
    assert_eq!(absent.publication.start.before, original.physical);
    let mut recovered = absent
        .try_reopen_synced_post(
            &control_temp.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
    assert_eq!(
        recovered.resume_recovered_control_post(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert!(recovered.completed_native_head().is_none());
    assert_eq!(recovered.commit_pending(), NativeMainRowStopV1::Complete);
    let receipt = recovered.completed_native_receipt().unwrap();
    assert_eq!(receipt.before, original.physical);
    assert_eq!(receipt.previous_head, original.session.commitment);
    assert_eq!(receipt.sequence, original.session.commands);
    assert_eq!(recovered.owner.history.len(), 1);
    assert!(recovered
        .owner
        .financial
        .runtime()
        .native_main_row_effects()
        .is_empty());
    let (_owner, _store) = recovered.into_completed_owners().unwrap();
}

#[test]
fn accepted_sync_lost_ack_retains_real_completed_source_prefix_without_publication_or_replay() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let original = actual.expectation().unwrap();
    let source_head = actual.financial.head();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![]),
            height: V::number(10.),
            live: false,
        },
    );
    let native_before = controller.index().snapshot().unwrap();
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.advance(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert!(matches!(
        controller.pc,
        NativeMainRowControllerPcV1::FinalAcceptedAttempted
    ));
    assert_eq!(
        controller.frame.as_ref().unwrap().result(),
        Some(&Ok(DependencyReturn::Pending(V::number(0.))))
    );
    assert!(controller
        .publication
        .prepared
        .as_ref()
        .unwrap()
        .after_control
        .is_some());
    assert!(controller
        .publication
        .prepared
        .as_ref()
        .unwrap()
        .accepted_blob
        .is_some());
    assert!(controller.store_pin().is_err());
    assert_eq!(controller.owner.head, original.session);
    assert!(controller.owner.history.is_empty());
    assert_eq!(controller.owner.financial.head(), source_head);
    assert_eq!(controller.index().snapshot().unwrap(), native_before);
    let financial = controller
        .owner
        .financial
        .native_main_financial_commitment()
        .unwrap();
    let effects = controller.effects().to_vec();
    assert_eq!(
        financial,
        controller
            .publication
            .prepared
            .as_ref()
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .financial_commitment
    );
    assert_eq!(
        effects,
        controller
            .publication
            .prepared
            .as_ref()
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .effects
    );
    assert!(controller.completed_native_head().is_none());
    assert!(controller.completed_native_receipt().is_none());
    let stopped = controller.advance();
    assert!(matches!(stopped, NativeMainRowStopV1::StorageStopped(_)));
    assert_eq!(controller.advance(), stopped);
    assert_eq!(controller.effects(), effects);
    assert_eq!(
        controller
            .owner
            .financial
            .native_main_financial_commitment()
            .unwrap(),
        financial
    );
    let rejected = match controller.into_completed_owners() {
        Err(rejected) => rejected,
        Ok(_) => panic!("ambiguous Accepted was released"),
    };
    let retained = rejected.into_session();
    assert_eq!(retained.owner.head, original.session);
    assert!(retained.owner.history.is_empty());
    assert_eq!(retained.effects(), effects);
    assert_eq!(retained.owner.financial.head(), source_head);
}

#[test]
fn actual_ack_with_missing_accepted_blob_refuses_guard_before_any_effect_drain() {
    let source = Temp::new();
    let control_temp = Temp::new();
    let actual = owner(&source);
    let original = actual.expectation().unwrap();
    let mut controller = begin(
        actual,
        control(&control_temp),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
    );
    NATIVE_PUBLICATION_REMOVE_ACCEPTED_TEST.with(|flag| {
        assert!(!flag.replace(true));
    });
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(controller.store.state().unwrap().get("kind"), s("Idle"));
    assert!(matches!(
        controller.pc,
        NativeMainRowControllerPcV1::Blocked(_)
    ));
    assert_eq!(
        controller.frame.as_ref().unwrap().result(),
        Some(&Ok(DependencyReturn::Pending(V::number(0.))))
    );
    assert_eq!(controller.owner.head, original.session);
    assert!(controller.owner.history.is_empty());
    assert!(!controller.publication.installed);
    let prepared = controller.publication.prepared.as_ref().unwrap();
    let receipt = prepared.receipt.as_ref().unwrap();
    let effects = controller.effects().to_vec();
    assert!(!effects.is_empty());
    assert_eq!(receipt.effects, effects);
    let financial = controller
        .owner
        .financial
        .native_main_financial_commitment()
        .unwrap();
    assert_eq!(receipt.financial_commitment, financial);
    let before = controller.store_pin().unwrap();
    let stopped = controller.advance();
    assert!(matches!(stopped, NativeMainRowStopV1::StorageStopped(_)));
    assert_eq!(controller.effects(), effects);
    assert_eq!(controller.store_pin().unwrap(), before);
    let rejected = match controller.into_completed_owners() {
        Err(rejected) => rejected,
        Ok(_) => panic!("unverifiable accepted record was released"),
    };
    let retained = rejected.into_session();
    assert_eq!(retained.owner.head, original.session);
    assert_eq!(retained.effects(), effects);
    assert_eq!(
        retained
            .owner
            .financial
            .native_main_financial_commitment()
            .unwrap(),
        financial
    );
}
