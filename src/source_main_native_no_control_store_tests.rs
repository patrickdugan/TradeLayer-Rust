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
            "moved-no-control-store-{}-{}",
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
fn owner_with_inputs(
    temp: &Temp,
    service: bool,
    compacting: bool,
    second: bool,
    fatal: bool,
) -> SourceMainNativeSessionV13 {
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
                        "failures",
                        V::Array(vec![obj(&[
                            ("method", s("Client.getBlockCount")),
                            ("ordinal", V::number(1.)),
                            ("name", s("SourceBoundaryUnavailable")),
                            ("message", s("Native Main control has no live RPC service")),
                        ])]),
                    ),
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
    if service {
        invoke(
            &mut financial,
            "SourceMain.configureService",
            vec![obj(&[
                ("fileInputs", V::Array(vec![])),
                ("directoryInputs", V::Array(vec![])),
                (
                    "fileIOInputs",
                    obj(&[("failures", V::Array(vec![])), ("mtimes", V::Array(vec![]))]),
                ),
            ])],
        );
    }
    if fatal {
        financial
            .apply(SourceContractCommand::FailStorage(
                crate::source_contract_runtime::StorageFailure {
                    collection: "txIndex".into(),
                    operation: "updateAsync".into(),
                    skip: 0,
                    message: "recorded-primary-validity-rejection".into(),
                },
            ))
            .unwrap();
    }
    financial
        .apply(SourceContractCommand::SetHeight(V::number(10.)))
        .unwrap();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index =
        SourceCompactIndex::create(&temp.0, namespace.clone(), "moved raw row command").unwrap();
    index.import_row(&index.snapshot().unwrap(), row()).unwrap();
    if second {
        index
            .import_row(&index.snapshot().unwrap(), second_raw_row())
            .unwrap();
    }
    if compacting {
        index.native_main_test_compaction_fixture().unwrap();
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
    owner_with_service(temp, false)
}

fn owner_with_service(temp: &Temp, service: bool) -> SourceMainNativeSessionV13 {
    owner_with_fixture(temp, service, false)
}
fn controller(
    financial_temp: &Temp,
    control_temp: &Temp,
    catchup: bool,
) -> SourceMainNativeRowSessionV1 {
    controller_from_owner(owner(financial_temp), control_temp, catchup)
}
fn controller_from_owner(
    owner: SourceMainNativeSessionV13,
    control_temp: &Temp,
    catchup: bool,
) -> SourceMainNativeRowSessionV1 {
    let expected = owner.expectation().unwrap();
    let store = NativeIntentStoreV1::create(
        &control_temp.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("profile", s("actual same-owner no-index control"))]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let pin = store.pin().unwrap();
    let command = if catchup {
        NativeMainSessionCommand::ConstructConsensusFromIndex {
            start: V::number(10.),
        }
    } else {
        NativeMainSessionCommand::ProcessRows {
            rows: V::Array(vec![row()]),
            height: V::number(10.),
            live: false,
        }
    };
    owner
        .into_row_session(store, &expected, &pin, command, 8 * 1024 * 1024)
        .unwrap()
}
fn marks(effects: &[V]) -> usize {
    effects
        .iter()
        .filter(|effect| {
            effect.get("collection") == s("consensus")
                && effect.get("method") == s("updateAsync")
                && match effect.get("args") {
                    V::Array(ref args) => args
                        .get(1)
                        .is_some_and(|update| update.get("$set").get("processed") == V::Bool(true)),
                    _ => false,
                }
        })
        .count()
}
fn tree(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn owner_with_fixture(temp: &Temp, service: bool, compacting: bool) -> SourceMainNativeSessionV13 {
    owner_with_inputs(temp, service, compacting, false, false)
}
fn control_namespace() -> Namespace {
    Namespace::new("LTC", false, "consensus").unwrap()
}
fn second_raw_row() -> V {
    let V::Object(mut outer) = row() else {
        unreachable!()
    };
    *outer.iter_mut().find(|(key, _)| key == "_id").unwrap() =
        ("_id".into(), s("tx-10-second-no-control"));
    let (_, V::Object(value)) = outer.iter_mut().find(|(key, _)| key == "value").unwrap() else {
        unreachable!()
    };
    *value.iter_mut().find(|(key, _)| key == "txId").unwrap() =
        ("txId".into(), s("second-no-control"));
    V::Object(outer)
}
fn two_row_controller(financial: &Temp, control: &Temp) -> SourceMainNativeRowSessionV1 {
    let owner = owner_with_inputs(financial, false, false, true, false);
    let expected = owner.expectation().unwrap();
    let store = NativeIntentStoreV1::create(
        &control.0,
        control_namespace(),
        V::Null,
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let pin = store.pin().unwrap();
    owner
        .into_row_session(
            store,
            &expected,
            &pin,
            NativeMainSessionCommand::ProcessRows {
                rows: V::Array(vec![row(), second_raw_row()]),
                height: V::number(10.),
                live: false,
            },
            8 * 1024 * 1024,
        )
        .unwrap()
}
fn repair_native_before(
    controller: SourceMainNativeRowSessionV1,
    financial: &Temp,
) -> SourceMainNativeRowSessionV1 {
    controller
        .into_no_index()
        .unwrap()
        .try_reopen(
            &financial.0,
            NativePreparedValidityCandidateV1::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap()
}
fn reopen_control_post(
    controller: SourceMainNativeRowSessionV1,
    control: &Temp,
) -> SourceMainNativeRowSessionV1 {
    controller
        .into_no_control_store()
        .unwrap()
        .try_reopen_synced_post(
            &control.0,
            control_namespace(),
            SourceDurableFilesystem::FileSyncOnly,
        )
        .unwrap()
}
fn complete_n0(controller: &SourceMainNativeRowSessionV1, decodes: usize, mark_count: usize) {
    assert_eq!(
        controller.result(),
        Some(&Ok(DependencyReturn::Pending(V::number(0.))))
    );
    assert_eq!(
        controller
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decodes
    );
    assert_eq!(marks(controller.effects()), mark_count);
    assert_eq!(controller.store.state().unwrap().get("kind"), s("Idle"));
}

#[test]
fn healthy_ack_before_blob_error_keeps_store_and_retries_exact_sealed_operation() {
    let financial = Temp::new();
    let control = Temp::new();
    let mut controller = controller(&financial, &control, false);
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller
        .owner
        .index
        .commit_prepared_validity(controller.frame.as_ref().unwrap().pending_write().unwrap())
        .unwrap();
    assert_eq!(
        controller.prepare_prefix_ack(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let prepared = controller.retained_prefix_ack().unwrap();
    let before = prepared.independently_expected_before().clone();
    let expected = prepared.independently_expected_after().clone();
    let blocked_blob = control.0.join("native13-control-blobs").join(format!(
        "{}.tlc13",
        hex(&prepared.acknowledged_prefix().sha256)
    ));
    std::fs::create_dir(&blocked_blob).unwrap();
    assert!(matches!(
        controller.sync_prepared_prefix_ack(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(controller.store_pin().unwrap(), before);
    let rejection = match controller.into_no_control_store() {
        Ok(_) => panic!("healthy Store must remain live"),
        Err(error) => error,
    };
    let mut controller = rejection.into_session();
    assert_eq!(
        controller
            .retained_prefix_ack()
            .unwrap()
            .independently_expected_after(),
        &expected
    );
    std::fs::remove_dir(&blocked_blob).unwrap();
    assert_eq!(
        controller.sync_prepared_prefix_ack(),
        NativeMainRowStopV1::Complete
    );
    complete_n0(&controller, decoded + 1, 1);
}

#[test]
fn torn_control_ack_post_is_refused_without_repair_and_returns_same_owner() {
    let financial = Temp::new();
    let control = Temp::new();
    let mut controller = controller(&financial, &control, false);
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller
        .store
        .native_main_test_control_fail_after_partial_write();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let state = controller
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let effects = controller.effects().to_vec();
    let native_post = controller.index().snapshot().unwrap();
    let absent = controller.into_no_control_store().unwrap();
    let files = tree(&control.0);
    let rejected = match absent.try_reopen_synced_post(
        &control.0,
        control_namespace(),
        SourceDurableFilesystem::FileSyncOnly,
    ) {
        Ok(_) => panic!("torn ACK is not known Post"),
        Err(error) => error,
    };
    assert_eq!(tree(&control.0), files);
    assert_eq!(
        rejected
            .owner()
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        state
    );
    assert_eq!(rejected.owner().effects(), effects);
    assert_eq!(rejected.owner().index().snapshot().unwrap(), native_post);
}

#[test]
fn superseding_intent_synced_post_rebinds_before_installing_actual_replacement() {
    let financial = Temp::new();
    let control = Temp::new();
    let mut controller = controller(&financial, &control, false);
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
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
    let mut controller = repair_native_before(controller, &financial);
    assert_eq!(
        controller.prepare_superseding(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let source_old = controller.pending_write().unwrap().durable_read_context();
    let state = controller
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let effects = controller.effects().to_vec();
    let native_before = controller.index().snapshot().unwrap();
    let expected = controller
        .retained_superseding_intent()
        .unwrap()
        .independently_expected_after()
        .clone();
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.sync_prepared_superseding(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(
        controller.pending_write().unwrap().durable_read_context(),
        source_old
    );
    let mut recovered = reopen_control_post(controller, &control);
    assert_eq!(recovered.store_pin().unwrap(), expected);
    assert_eq!(
        recovered
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        state
    );
    assert_eq!(recovered.effects(), effects);
    assert_eq!(recovered.index().snapshot().unwrap(), native_before);
    assert_eq!(
        recovered.resume_recovered_control_post(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert!(recovered.superseding_active);
    assert_eq!(
        recovered.index().snapshot().unwrap(),
        native_before,
        "installing metadata plan cannot append"
    );
    assert_eq!(recovered.commit_pending(), NativeMainRowStopV1::Complete);
    complete_n0(&recovered, decoded + 1, 1);
}

#[test]
fn superseded_first_row_then_ordinary_second_intent_and_ack_recovery_share_one_command() {
    let financial = Temp::new();
    let control = Temp::new();
    let mut controller = two_row_controller(&financial, &control);
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
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
    let mut controller = repair_native_before(controller, &financial);
    assert_eq!(
        controller.prepare_superseding(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.sync_prepared_superseding(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.commit_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert!(
        !controller.superseding_active,
        "first ACK resets supersession before next actual source row"
    );
    assert_eq!(controller.validity().len(), 1);
    assert_eq!(
        controller
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded + 2
    );
    assert_eq!(marks(controller.effects()), 2);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.store.state().unwrap().get("kind"),
        s("NativeIntent")
    );
    assert!(!controller.superseding_active);
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let second_post = controller.index().snapshot().unwrap();
    let mut recovered = reopen_control_post(controller, &control);
    assert_eq!(
        recovered.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(recovered.index().snapshot().unwrap(), second_post);
    assert_eq!(recovered.validity().len(), 2);
    complete_n0(&recovered, decoded + 2, 2);
}

#[test]
fn fatal_validity_await_recovers_ack_without_third_mark_or_decode() {
    let financial = Temp::new();
    let control = Temp::new();
    let owner = owner_with_inputs(&financial, false, false, false, true);
    let mut controller = controller_from_owner(owner, &control, false);
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(marks(controller.effects()), 2);
    let update = controller.pending_write().unwrap().update_arguments()[1].get("$set");
    assert_eq!(update.get("valid"), V::Bool(false));
    assert_eq!(
        update.get("reason"),
        s("Fatal tx processing error: recorded-primary-validity-rejection")
    );
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller.store.native_main_test_control_fail_after_sync();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let post = controller.index().snapshot().unwrap();
    let mut recovered = reopen_control_post(controller, &control);
    assert_eq!(
        recovered.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(recovered.index().snapshot().unwrap(), post);
    complete_n0(&recovered, decoded + 1, 2);
}

#[test]
fn actual_ack_synced_post_rebinds_same_permit_then_resolves_original_await_once() {
    let financial = Temp::new();
    let control = Temp::new();
    let mut controller = controller(&financial, &control, false);
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
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
    let expected = controller
        .retained_prefix_ack()
        .unwrap()
        .independently_expected_after()
        .clone();
    let state = controller
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let effects = controller.effects().to_vec();
    let native_post = controller.index().snapshot().unwrap();
    let mut recovered = reopen_control_post(controller, &control);
    assert_eq!(recovered.store_pin().unwrap(), expected);
    assert_eq!(
        recovered
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        state
    );
    assert_eq!(recovered.effects(), effects);
    assert!(recovered.result().is_none());
    assert_eq!(
        recovered.resume_recovered_control_post(),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(recovered.index().snapshot().unwrap(), native_post);
    complete_n0(&recovered, decoded + 1, 1);
}
