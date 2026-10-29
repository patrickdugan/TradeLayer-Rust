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
#[test]
fn moved_controller_binds_catchup_row_await_and_publishes_after_durable_ack() {
    let financial_temp = Temp::new();
    let control_temp = Temp::new();
    let owner = owner(&financial_temp);
    let expected = owner.expectation().unwrap();
    let store = NativeIntentStoreV1::create(
        &control_temp.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[("profile", s("actual moved catchup control"))]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let pin = store.pin().unwrap();
    let mut controller = owner
        .into_row_session(
            store,
            &expected,
            &pin,
            NativeMainSessionCommand::ConstructConsensusFromIndex {
                start: V::number(10.),
            },
            8 * 1024 * 1024,
        )
        .unwrap();
    assert_eq!(
        controller.store.state().unwrap().get("kind"),
        s("CommandIntent")
    );
    assert_eq!(controller.advance(), NativeMainRowStopV1::AwaitNativeWrite);
    assert_eq!(controller.consumed_rows(), 1);
    assert!(controller.grouping().unwrap().complete);
    assert!(controller.result().is_none());
    let before = controller.index().snapshot().unwrap();
    assert_eq!(
        controller.prepare_pending(),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        controller.store.state().unwrap().get("kind"),
        s("NativeIntent")
    );
    assert_eq!(controller.index().snapshot().unwrap(), before);
    assert_eq!(controller.commit_pending(), NativeMainRowStopV1::Complete);
    assert_eq!(controller.store.state().unwrap().get("kind"), s("Idle"));
    assert_eq!(
        controller.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: "SourceBoundaryUnavailable".into(),
                message: "Native Main control has no live RPC service".into()
            }
        )))
    );
    assert_eq!(controller.validity().len(), 1);
    assert_eq!(
        controller
            .effects()
            .iter()
            .filter(|effect| effect.get("kind") == s("main-phase"))
            .count(),
        6
    );
    let post = controller.index().snapshot().unwrap();
    assert_eq!(controller.commit_pending(), NativeMainRowStopV1::Complete);
    assert_eq!(controller.index().snapshot().unwrap(), post);
}

#[test]
fn configured_service_refuses_before_durable_command_intent_or_source_work() {
    let financial_temp = Temp::new();
    let control_temp = Temp::new();
    let owner = owner_with_service(&financial_temp, true);
    let expected = owner.expectation().unwrap();
    let financial_before = owner.financial.runtime().native_main_state_value().unwrap();
    let store = NativeIntentStoreV1::create(
        &control_temp.0,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[(
            "profile",
            s("finite profile excludes service before intent"),
        )]),
        V::Null,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    let pin = store.pin().unwrap();
    let store_before = store.state().unwrap();
    let rejection = match owner.into_row_session(
        store,
        &expected,
        &pin,
        NativeMainSessionCommand::ConstructConsensusFromIndex {
            start: V::number(10.),
        },
        8 * 1024 * 1024,
    ) {
        Ok(_) => panic!("configured service must refuse before starting this finite profile"),
        Err(rejection) => rejection,
    };
    let (owner, store) = rejection.into_owners();
    assert_eq!(owner.expectation().unwrap(), expected);
    assert_eq!(
        owner.financial.runtime().native_main_state_value().unwrap(),
        financial_before
    );
    assert_eq!(store.pin().unwrap(), pin);
    assert_eq!(store.state().unwrap(), store_before);
    assert_eq!(store.state().unwrap().get("kind"), s("Idle"));
}
