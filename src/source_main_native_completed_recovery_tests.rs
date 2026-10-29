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
            "moved-completed-barrier-recovery-{}-{}",
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
// This helper runs actual private source/metadata callbacks to their returned
// boundary. It merely pauses before final control IO; no result is supplied.
fn returned_source(
    source: &Temp,
    storage: &Temp,
    include_row: bool,
    explicit: bool,
) -> SourceMainNativeRowSessionV1 {
    let mut controller = begin(
        owner_mode(source, include_row),
        control(storage),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(if include_row { vec![row()] } else { vec![] }),
            height: V::number(10.),
            live: false,
        },
    );
    let stop = controller.frame.as_mut().unwrap().advance(
        controller.owner.financial.native_main_runtime_mut(),
        &mut controller.owner.index,
    );
    if include_row {
        assert_eq!(stop, NativeMainRowStopV1::AwaitNativeWrite);
        assert_eq!(
            controller.prepare_pending(),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        if explicit {
            controller.owner.index.test_fail_after_partial_write();
            assert!(matches!(
                controller.commit_pending(),
                NativeMainRowStopV1::StorageStopped(_)
            ));
            controller = controller
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
            assert_eq!(
                controller.sync_prepared_superseding(),
                NativeMainRowStopV1::AwaitNativeWrite
            );
        }
        assert!(matches!(
            controller.pc,
            NativeMainRowControllerPcV1::NativeIntent
        ));
        controller.pc = NativeMainRowControllerPcV1::NativeAppendAttempted;
        assert_eq!(
            controller.frame.as_mut().unwrap().append(
                controller.owner.financial.native_main_runtime_mut(),
                &mut controller.owner.index
            ),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        controller.pc = NativeMainRowControllerPcV1::NativePost;
        assert_eq!(
            controller.prepare_prefix_ack(),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        controller
            .store
            .sync_prepared_native_prefix_ack(
                &mut controller.permit,
                controller.prefix_ack.as_ref().unwrap(),
            )
            .unwrap();
        controller.superseding_active = false;
        controller.pc = NativeMainRowControllerPcV1::Source;
        assert_eq!(
            controller.frame.as_mut().unwrap().resolve_post(
                controller.owner.financial.native_main_runtime_mut(),
                &mut controller.owner.index
            ),
            NativeMainRowStopV1::Complete
        );
    } else {
        assert_eq!(stop, NativeMainRowStopV1::Complete);
    }
    assert!(matches!(
        controller.frame.as_ref().unwrap().result(),
        Some(Ok(_))
    ));
    assert!(controller.frame.as_ref().unwrap().pending_write().is_none());
    assert!(controller.owner.history.is_empty());
    controller
}
fn reopen_final(
    controller: SourceMainNativeRowSessionV1,
    storage: &Temp,
) -> SourceMainNativeRowSessionV1 {
    controller
        .into_no_control_store()
        .unwrap()
        .try_reopen_synced_post(
            &storage.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap()
}
fn retained_head(controller: &SourceMainNativeRowSessionV1) {
    assert_eq!(controller.owner.head, controller.publication.start.head);
    assert_eq!(
        controller.owner.history.len(),
        controller.publication.start.history_len
    );
    assert!(controller.completed_native_head().is_none());
    assert!(controller.completed_native_receipt().is_none());
    assert!(!controller.publication.installed);
}
fn assert_once(controller: &mut SourceMainNativeRowSessionV1, effects: &[V]) {
    assert_eq!(controller.advance(), NativeMainRowStopV1::Complete);
    let receipt = controller.completed_native_receipt().unwrap();
    assert_eq!(receipt.effects, effects);
    assert_eq!(receipt.before, controller.publication.start.before);
    assert_eq!(
        receipt.previous_head,
        controller.publication.start.head.commitment
    );
    assert_eq!(
        controller.owner.history.len(),
        controller.publication.start.history_len + 1
    );
    assert!(controller
        .owner
        .financial
        .runtime()
        .native_main_row_effects()
        .is_empty());
    let head = controller.completed_native_head().unwrap();
    assert_eq!(controller.advance(), NativeMainRowStopV1::Complete);
    assert_eq!(controller.completed_native_head().unwrap(), head);
    assert_eq!(controller.effects(), effects);
}
#[test]
fn real_empty_source_acceptance_lost_ack_rebinds_same_permit_without_source_reexecution() {
    let source = Temp::new();
    let storage = Temp::new();
    let mut controller = returned_source(&source, &storage, false, false);
    let financial = controller
        .owner
        .financial
        .native_main_financial_commitment()
        .unwrap();
    let effects = controller.effects().to_vec();
    assert_eq!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let accepted = controller
        .publication
        .prepared
        .as_ref()
        .unwrap()
        .accepted_blob
        .clone();
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    retained_head(&controller);
    let stopped = controller.advance();
    assert!(matches!(stopped, NativeMainRowStopV1::StorageStopped(_)));
    assert_eq!(controller.advance(), stopped);
    assert_eq!(controller.effects(), effects);
    let mut controller = reopen_final(controller, &storage);
    assert!(matches!(
        controller.pc,
        NativeMainRowControllerPcV1::FinalAcceptedSynced
    ));
    retained_head(&controller);
    assert_eq!(
        controller
            .owner
            .financial
            .native_main_financial_commitment()
            .unwrap(),
        financial
    );
    assert_eq!(
        controller
            .publication
            .prepared
            .as_ref()
            .unwrap()
            .accepted_blob,
        accepted
    );
    assert_eq!(
        controller.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_once(&mut controller, &effects);
}
#[test]
fn real_row_final_command_ack_lost_ack_retains_effects_and_publishes_exactly_once() {
    let source = Temp::new();
    let storage = Temp::new();
    let mut controller = returned_source(&source, &storage, true, false);
    let effects = controller.effects().to_vec();
    assert!(!effects.is_empty());
    assert_eq!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.prepare_completed_acknowledgement(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let after = controller
        .retained_completed_acknowledgement()
        .unwrap()
        .independently_expected_after()
        .clone();
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.sync_prepared_completed_acknowledgement(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    retained_head(&controller);
    assert_eq!(controller.effects(), effects);
    let absent = controller.into_no_control_store().unwrap();
    assert_eq!(absent.independently_expected_control_post(), &after);
    let mut controller = absent
        .try_reopen_synced_post(
            &storage.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
    assert!(matches!(
        controller.pc,
        NativeMainRowControllerPcV1::FinalCommandAckSynced
    ));
    assert_eq!(controller.store.state().unwrap().get("kind"), s("Idle"));
    retained_head(&controller);
    assert_eq!(controller.effects(), effects);
    assert_eq!(
        controller.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_once(&mut controller, &effects);
}
fn block_blob(
    storage: &Temp,
    blob: &crate::source_main_native_intent_store::NativeIntentBlobV1,
) -> PathBuf {
    let path = storage
        .0
        .join("native13-control-blobs")
        .join(format!("{}.tlc13", hex(&blob.sha256)));
    std::fs::create_dir(&path).unwrap();
    path
}
#[test]
fn healthy_before_acceptance_failure_requires_same_store_exact_prepared_retry() {
    let source = Temp::new();
    let storage = Temp::new();
    let mut controller = returned_source(&source, &storage, false, false);
    let effects = controller.effects().to_vec();
    assert_eq!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let op = controller.retained_completed_acceptance().unwrap();
    let before = op.independently_expected_before().clone();
    let path = block_blob(&storage, op.accepted_blob());
    assert!(matches!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(controller.store.pin().unwrap(), before);
    retained_head(&controller);
    let rejected = match controller.into_no_control_store() {
        Err(e) => e,
        Ok(_) => panic!("healthy Before released"),
    };
    let mut controller = rejected.into_session();
    std::fs::remove_dir(&path).unwrap();
    assert_eq!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_once(&mut controller, &effects);
}
#[test]
fn healthy_before_final_ack_blob_failure_preserves_prepared_context_and_retries_once() {
    let source = Temp::new();
    let storage = Temp::new();
    let mut controller = returned_source(&source, &storage, true, false);
    let effects = controller.effects().to_vec();
    assert_eq!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.prepare_completed_acknowledgement(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let op = controller.retained_completed_acknowledgement().unwrap();
    let before = op.independently_expected_before().clone();
    let path = block_blob(&storage, op.acknowledgement());
    assert!(matches!(
        controller.sync_prepared_completed_acknowledgement(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(controller.store.pin().unwrap(), before);
    let rejected = match controller.into_no_control_store() {
        Err(e) => e,
        Ok(_) => panic!("healthy Before released"),
    };
    let mut controller = rejected.into_session();
    retained_head(&controller);
    std::fs::remove_dir(&path).unwrap();
    assert_eq!(
        controller.sync_prepared_completed_acknowledgement(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    retained_head(&controller);
    assert_once(&mut controller, &effects);
}
#[test]
fn torn_final_barriers_refuse_reopen_without_repair_and_retain_all_owners() {
    for command_ack in [false, true] {
        let source = Temp::new();
        let storage = Temp::new();
        let mut controller = returned_source(&source, &storage, true, false);
        assert_eq!(
            controller.prepare_completed_acceptance(),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        if command_ack {
            assert_eq!(
                controller.sync_prepared_completed_acceptance(),
                NativeMainRowStopV1::AwaitNativeWrite
            );
            assert_eq!(
                controller.prepare_completed_acknowledgement(),
                NativeMainRowStopV1::AwaitNativeWrite
            );
        }
        let effects = controller.effects().to_vec();
        let original = controller.publication.start.head.clone();
        controller
            .store
            .native_main_test_control_fail_after_partial_write();
        let stop = if command_ack {
            controller.sync_prepared_completed_acknowledgement()
        } else {
            controller.sync_prepared_completed_acceptance()
        };
        assert!(matches!(stop, NativeMainRowStopV1::StorageStopped(_)));
        let absent = controller.into_no_control_store().unwrap();
        let pin = absent.independently_expected_control_post().clone();
        let paths = control_journal_files(&storage.0);
        let rejected = match absent.try_reopen_synced_post(
            &storage.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        ) {
            Err(e) => e,
            Ok(_) => panic!("torn control accepted"),
        };
        let absent = rejected.into_owner();
        assert_eq!(absent.independently_expected_control_post(), &pin);
        assert_eq!(absent.owner.head, original);
        assert_eq!(absent.effects(), effects);
        assert_eq!(control_journal_files(&storage.0), paths);
    }
}
fn control_journal_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, path: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out)
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    std::fs::read(path).unwrap(),
                ))
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}
#[test]
fn final_recovery_foreign_directory_and_namespace_return_same_no_store_owner() {
    let source = Temp::new();
    let storage = Temp::new();
    let foreign = Temp::new();
    let foreign_store = control(&foreign);
    drop(foreign_store);
    let mut controller = returned_source(&source, &storage, false, false);
    assert_eq!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let effects = controller.effects().to_vec();
    let absent = controller.into_no_control_store().unwrap();
    let expected = absent.independently_expected_control_post().clone();
    let rejected = match absent.try_reopen_synced_post(
        &foreign.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        SourceDurableFilesystem::FileSyncOnly,
    ) {
        Err(e) => e,
        Ok(_) => panic!("foreign root admitted"),
    };
    let absent = rejected.into_owner();
    assert_eq!(absent.independently_expected_control_post(), &expected);
    assert_eq!(absent.effects(), effects);
    let rejected = match absent.try_reopen_synced_post(
        &storage.0,
        Namespace::new("BTC", false, "consensus").unwrap(),
        SourceDurableFilesystem::FileSyncOnly,
    ) {
        Err(e) => e,
        Ok(_) => panic!("foreign namespace admitted"),
    };
    let mut controller = rejected
        .into_owner()
        .try_reopen_synced_post(
            &storage.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
    assert_eq!(
        controller.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_once(&mut controller, &effects);
}
#[test]
fn original_and_superseding_rows_survive_both_final_lost_acks_with_identical_logical_receipts() {
    for explicit in [false, true] {
        let source = Temp::new();
        let storage = Temp::new();
        let reference_temp = Temp::new();
        let mut reference = owner(&reference_temp);
        let mut controller = returned_source(&source, &storage, true, explicit);
        let expected = reference
            .apply(
                &reference.expectation().unwrap(),
                controller.command.clone(),
            )
            .unwrap()
            .clone();
        let effects = controller.effects().to_vec();
        assert_eq!(
            controller.prepare_completed_acceptance(),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        controller.store.native_main_test_control_fail_after_sync();
        assert!(matches!(
            controller.sync_prepared_completed_acceptance(),
            NativeMainRowStopV1::StorageStopped(_)
        ));
        let mut controller = reopen_final(controller, &storage);
        retained_head(&controller);
        assert_eq!(
            controller.prepare_completed_acknowledgement(),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        controller.store.native_main_test_control_fail_after_sync();
        assert!(matches!(
            controller.sync_prepared_completed_acknowledgement(),
            NativeMainRowStopV1::StorageStopped(_)
        ));
        let mut controller = reopen_final(controller, &storage);
        retained_head(&controller);
        assert_eq!(
            controller.resume_recovered_control_post(),
            NativeMainRowStopV1::Complete
        );
        assert_once(&mut controller, &effects);
        assert_eq!(
            logical_receipt_bytes(controller.completed_native_receipt().unwrap()).unwrap(),
            logical_receipt_bytes(&expected).unwrap()
        );
        let (mut owner, store) = controller.into_completed_owners().unwrap();
        let next = NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        };
        let next_expected = reference
            .apply(&reference.expectation().unwrap(), next.clone())
            .unwrap()
            .clone();
        let mut next_controller = begin(owner, store, next);
        finish(&mut next_controller);
        assert_eq!(
            logical_receipt_bytes(next_controller.completed_native_receipt().unwrap()).unwrap(),
            logical_receipt_bytes(&next_expected).unwrap()
        );
        (owner, _) = next_controller.into_completed_owners().unwrap();
        assert_eq!(owner.history.len(), 2);
        assert_eq!(owner.head, reference.head());
    }
}
#[test]
fn pending_source_cannot_prepare_final_barriers_or_escape_under_old_head() {
    let source = Temp::new();
    let storage = Temp::new();
    let mut controller = begin(
        owner(&source),
        control(&storage),
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        },
    );
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert!(matches!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert!(controller.retained_completed_acceptance().is_none());
    retained_head(&controller);
    let rejected = match controller.into_completed_owners() {
        Err(e) => e,
        Ok(_) => panic!("unfinished source escaped"),
    };
    assert!(rejected
        .into_session()
        .frame
        .as_ref()
        .unwrap()
        .pending_write()
        .is_some());
}
#[test]
fn missing_actual_accepted_blob_prevents_rebind_without_losing_retained_owner_or_sealed_bytes() {
    let source = Temp::new();
    let storage = Temp::new();
    let mut controller = returned_source(&source, &storage, true, false);
    assert_eq!(
        controller.prepare_completed_acceptance(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let operation = controller.retained_completed_acceptance().unwrap();
    let blob = operation.accepted_blob().clone();
    let raw = operation.accepted_bytes().to_vec();
    let path = storage
        .0
        .join("native13-control-blobs")
        .join(format!("{}.tlc13", hex(&blob.sha256)));
    let effects = controller.effects().to_vec();
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.sync_prepared_completed_acceptance(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let absent = controller.into_no_control_store().unwrap();
    std::fs::remove_file(&path).unwrap();
    let rejected = match absent.try_reopen_synced_post(
        &storage.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        SourceDurableFilesystem::FileSyncOnly,
    ) {
        Err(e) => e,
        Ok(_) => panic!("missing sealed accepted bytes admitted"),
    };
    let absent = rejected.into_owner();
    assert_eq!(absent.effects(), effects);
    assert!(!absent.publication.installed);
    assert!(absent.owner.history.is_empty());
    std::fs::write(&path, &raw).unwrap();
    let mut controller = absent
        .try_reopen_synced_post(
            &storage.0,
            Namespace::new("LTC", false, "consensus").unwrap(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap();
    assert_eq!(
        controller.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_once(&mut controller, &effects);
}
