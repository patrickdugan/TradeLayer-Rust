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
            "moved-row-control-{}-{}",
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
fn owner_with_service(temp: &Temp, service: bool) -> SourceMainNativeSessionV13 {
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
    financial
        .apply(SourceContractCommand::SetHeight(V::number(10.)))
        .unwrap();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index =
        SourceCompactIndex::create(&temp.0, namespace.clone(), "moved raw row command").unwrap();
    index.import_row(&index.snapshot().unwrap(), row()).unwrap();
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
fn controller(
    financial_temp: &Temp,
    control_temp: &Temp,
    catchup: bool,
) -> SourceMainNativeRowSessionV1 {
    let owner = owner(financial_temp);
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
#[test]
fn actual_synced_native_post_reopens_same_row_owner_then_acks_original_await_once() {
    let financial_temp = Temp::new();
    let control_temp = Temp::new();
    let mut controller = controller(&financial_temp, &control_temp, false);
    let decoded_before = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(
        controller
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded_before + 1
    );
    let financial = controller
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let effects = controller.effects().to_vec();
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let control_pin = controller.store_pin().unwrap();
    let post = controller
        .pending_write()
        .unwrap()
        .append_plan()
        .after()
        .clone();
    controller.owner.index.test_fail_after_sync();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    assert_eq!(
        controller
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        financial
    );
    let loan = controller.into_no_index().unwrap();
    assert_eq!(
        loan.financial_runtime().native_main_state_value().unwrap(),
        financial
    );
    assert_eq!(loan.effects(), effects);
    assert_eq!(loan.store_pin().unwrap(), control_pin);
    let mut no_native_owner = false;
    loan.visit_canonical_fields(&mut |path, value| {
        assert!(!matches!(value, V::Array(_) | V::Object(_)));
        if path == "nativeOwnerPresent" {
            no_native_owner = value == &V::Bool(false);
        }
        Ok(())
    })
    .unwrap();
    assert!(no_native_owner);
    let mut recovered = loan
        .try_reopen(
            &financial_temp.0,
            NativePreparedValidityCandidateV1::Post,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    assert_eq!(
        recovered.reopened_observation().unwrap().candidate(),
        NativePreparedValidityCandidateV1::Post
    );
    assert_eq!(
        recovered.index().snapshot().unwrap().sequence,
        post.logical_sequence
    );
    assert_eq!(
        recovered.store_pin().unwrap(),
        control_pin,
        "reopen alone never acknowledges source"
    );
    assert!(recovered.result().is_none());
    assert_eq!(
        recovered.resolve_pending_post(),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(
        recovered.result(),
        Some(&Ok(DependencyReturn::Pending(V::number(0.))))
    );
    assert_eq!(
        recovered.index().snapshot().unwrap().sequence,
        post.logical_sequence
    );
    assert_eq!(recovered.effects(), effects);
    assert_eq!(marks(recovered.effects()), 1);
    assert_eq!(
        recovered
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded_before + 1
    );
    assert_eq!(recovered.store.state().unwrap().get("kind"), s("Idle"));
}
#[test]
fn failed_candidate_open_returns_same_no_index_owner_and_does_not_repair_unknown_tail() {
    let financial_temp = Temp::new();
    let control_temp = Temp::new();
    let mut controller = controller(&financial_temp, &control_temp, false);
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
    let financial = controller
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let effects = controller.effects().to_vec();
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
    let loan = controller.into_no_index().unwrap();
    let files = tree(&financial_temp.0);
    let rejected = match loan.try_reopen(
        &financial_temp.0,
        NativePreparedValidityCandidateV1::Post,
        Recovery::DiscardIncompleteTail,
    ) {
        Ok(_) => panic!("Before tail is not the independently prepared Post"),
        Err(rejected) => rejected,
    };
    assert_eq!(
        rejected
            .owner()
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        financial
    );
    assert_eq!(rejected.owner().effects(), effects);
    assert_eq!(
        tree(&financial_temp.0),
        files,
        "wrong candidate must refuse before repair"
    );
    let mut recovered = rejected
        .into_owner()
        .try_reopen(
            &financial_temp.0,
            NativePreparedValidityCandidateV1::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    assert!(
        recovered
            .reopened_observation()
            .unwrap()
            .recovery_report()
            .discarded_bytes
            > 0
    );
    assert!(
        matches!(
            recovered.commit_pending(),
            NativeMainRowStopV1::StorageStopped(_)
        ),
        "Before cannot reuse old Journal owner-token append"
    );
    assert_eq!(
        recovered
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        financial
    );
    assert_eq!(recovered.effects(), effects);
    assert_eq!(
        recovered
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded
    );
    assert!(recovered.result().is_none());
}
#[test]
fn actual_catchup_after_sync_ambiguity_keeps_cursor_and_runs_six_suffix_calls_once() {
    let financial_temp = Temp::new();
    let control_temp = Temp::new();
    let mut controller = controller(&financial_temp, &control_temp, true);
    let decoded = controller
        .financial_runtime()
        .native_main_test_row_decode_count();
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(controller.consumed_rows(), 1);
    assert!(controller.grouping().unwrap().complete);
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    controller.owner.index.test_fail_after_sync();
    assert!(matches!(
        controller.commit_pending(),
        NativeMainRowStopV1::StorageStopped(_)
    ));
    let loan = controller.into_no_index().unwrap();
    let mut recovered = loan
        .try_reopen(
            &financial_temp.0,
            NativePreparedValidityCandidateV1::Post,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    let post = recovered.index().snapshot().unwrap();
    assert_eq!(
        recovered.resolve_pending_post(),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(
        recovered.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: "SourceBoundaryUnavailable".into(),
                message: "Native Main control has no live RPC service".into()
            }
        )))
    );
    assert_eq!(recovered.index().snapshot().unwrap(), post);
    assert_eq!(
        recovered
            .effects()
            .iter()
            .filter(|effect| effect.get("kind") == s("main-phase"))
            .count(),
        6
    );
    assert_eq!(marks(recovered.effects()), 1);
    assert_eq!(
        recovered
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded + 1
    );
    assert_eq!(recovered.consumed_rows(), 1);
    assert_eq!(recovered.validity().len(), 1);
    assert_eq!(recovered.store.state().unwrap().get("kind"), s("Idle"));
}
#[test]
fn loan_refusal_before_native_intent_retains_whole_live_controller() {
    let financial_temp = Temp::new();
    let control_temp = Temp::new();
    let mut controller = controller(&financial_temp, &control_temp, false);
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    let financial = controller
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let effects = controller.effects().to_vec();
    let pin = controller.store_pin().unwrap();
    let rejected = match controller.into_no_index() {
        Ok(_) => panic!("no unresolved durable native intent yet"),
        Err(rejected) => rejected,
    };
    let mut retained = rejected.into_session();
    assert_eq!(
        retained
            .financial_runtime()
            .native_main_state_value()
            .unwrap(),
        financial
    );
    assert_eq!(retained.effects(), effects);
    assert_eq!(retained.store_pin().unwrap(), pin);
    assert_eq!(retained.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(retained.effects(), effects);
}
