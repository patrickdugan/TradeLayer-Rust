use super::*;
use crate::source_contract_replay::{SourceContractCommand, SourceContractCommandOutcome};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "cold-source-reexecution-{}-{}",
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
fn owner_with_failures(temp: &Temp, failures: usize) -> SourceMainNativeSessionV13 {
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
    for ordinal in 0..failures {
        financial
            .apply(SourceContractCommand::FailStorage(
                crate::source_contract_runtime::StorageFailure {
                    collection: "txIndex".into(),
                    operation: "updateAsync".into(),
                    skip: 0,
                    message: format!("raw recorded rejection {ordinal}"),
                },
            ))
            .unwrap();
    }
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

use crate::source_compact_index_v2::SourceIndexHistoryPinV1;

fn owner(temp: &Temp) -> SourceMainNativeSessionV13 {
    owner_with_failures(temp, 0)
}

fn record_one(
    owner: &mut SourceMainNativeSessionV13,
    command: NativeMainSessionCommand,
) -> Vec<SourceIndexHistoryPinV1> {
    let before = owner.index.capture_verified_history_pin().unwrap();
    let expected = owner.expectation().unwrap();
    let receipt = owner.apply(&expected, command).unwrap();
    assert!(matches!(
        receipt.status,
        NativeMainSessionStatus::Completed(_)
    ));
    let post = owner.index.capture_verified_history_pin().unwrap();
    if before == post {
        vec![before]
    } else {
        assert_eq!(post.metadata.sequence, before.metadata.sequence + 1);
        vec![before, post]
    }
}
fn call(live: bool) -> NativeMainSessionCommand {
    NativeMainSessionCommand::ProcessRows {
        rows: V::Array(vec![if live { row().get("value") } else { row() }]),
        height: V::number(10.),
        live,
    }
}
#[test]
fn historical_actual_source_rebuilds_finance_from_raw_foundation_and_original_file_history() {
    let temp = Temp::new();
    let mut owner = owner(&temp);
    let first = record_one(&mut owner, call(false));
    // A second raw call must execute the real processed check and skip. It is
    // never told that the first command was processed or that validity is true.
    let second = record_one(&mut owner, call(false));
    assert_eq!(second.len(), 1);
    let state = owner.financial_runtime().state_value().unwrap();
    let (archive, pin) = owner.checkpoint().unwrap();
    let current = owner.index.capture_verified_history_pin().unwrap();
    let expected = NativeMainHistoricalRowsExpectationV1 {
        archive: pin.clone(),
        current: current.clone(),
        commands: vec![first, second],
    };
    drop(owner);
    let index = SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &current,
    )
    .unwrap();
    let io = index.storage_io_counters().unwrap().confirmed_append_bytes;
    let replay =
        SourceMainNativeSessionV13::reexecute_historical_rows(&archive, &expected, index).unwrap();
    assert_eq!(replay.financial_runtime().state_value().unwrap(), state);
    assert_eq!(replay.head(), pin.head);
    assert_eq!(replay.checkpoint().unwrap(), (archive, pin));
    assert_eq!(
        replay.index.capture_verified_history_pin().unwrap(),
        current
    );
    assert_eq!(
        replay
            .index
            .storage_io_counters()
            .unwrap()
            .confirmed_append_bytes,
        io
    );
    assert_eq!(replay.history[0].validity.len(), 1);
    assert!(replay.history[1].validity.is_empty());
}
#[test]
fn historical_actual_live_source_rebuilds_same_true_result_without_past_append() {
    let temp = Temp::new();
    let mut owner = owner(&temp);
    let pins = record_one(&mut owner, call(true));
    assert_eq!(
        owner.history[0].status,
        NativeMainSessionStatus::Completed(DependencyReturn::Pending(V::Bool(true)))
    );
    let state = owner.financial_runtime().state_value().unwrap();
    let (archive, pin) = owner.checkpoint().unwrap();
    let current = owner.index.capture_verified_history_pin().unwrap();
    let expected = NativeMainHistoricalRowsExpectationV1 {
        archive: pin,
        current: current.clone(),
        commands: vec![pins],
    };
    drop(owner);
    let index = SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &current,
    )
    .unwrap();
    let replay =
        SourceMainNativeSessionV13::reexecute_historical_rows(&archive, &expected, index).unwrap();
    assert_eq!(replay.financial_runtime().state_value().unwrap(), state);
    assert_eq!(replay.checkpoint().unwrap().0, archive);
}
#[test]
fn historical_bad_boundary_retains_current_owner_without_financial_state_import_or_head_escape() {
    let temp = Temp::new();
    let mut owner = owner(&temp);
    let mut pins = record_one(&mut owner, call(false));
    let (archive, pin) = owner.checkpoint().unwrap();
    let current = owner.index.capture_verified_history_pin().unwrap();
    pins[0].rows.chain_sha256d[0] ^= 1;
    let expected = NativeMainHistoricalRowsExpectationV1 {
        archive: pin,
        current: current.clone(),
        commands: vec![pins],
    };
    drop(owner);
    let index = SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &current,
    )
    .unwrap();
    let rejected =
        SourceMainNativeSessionV13::reexecute_historical_rows(&archive, &expected, index)
            .unwrap_err();
    assert!(rejected.financial_runtime().is_none());
    assert!(rejected.actual_prefix().is_empty());
    assert_eq!(
        rejected.index().capture_verified_history_pin().unwrap(),
        current
    );
    assert!(
        SourceCompactIndex::open_verified_history_owner(
            &temp.0,
            Namespace::new("LTC", false, "txIndex").unwrap(),
            &current
        )
        .is_err(),
        "same real writer lock remains held"
    );
}

#[test]
fn historical_empty_native_history_reexecutes_only_raw_foundation() {
    let temp = Temp::new();
    let owner = owner(&temp);
    let state = owner.financial_runtime().state_value().unwrap();
    let (archive, pin) = owner.checkpoint().unwrap();
    let current = owner.index.capture_verified_history_pin().unwrap();
    let expected = NativeMainHistoricalRowsExpectationV1 {
        archive: pin.clone(),
        current: current.clone(),
        commands: vec![],
    };
    drop(owner);
    let index = SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &current,
    )
    .unwrap();
    let replay =
        SourceMainNativeSessionV13::reexecute_historical_rows(&archive, &expected, index).unwrap();
    assert_eq!(replay.financial_runtime().state_value().unwrap(), state);
    assert_eq!(replay.checkpoint().unwrap(), (archive, pin));
}

#[test]
fn historical_actual_failure_facts_reexecute_fatal_validity_without_supplied_verdicts() {
    for failures in [1, 2] {
        let temp = Temp::new();
        let mut owner = owner_with_failures(&temp, failures);
        let pins = record_one(&mut owner, call(false));
        assert_eq!(owner.history[0].validity.len(), 2);
        let state = owner.financial_runtime().state_value().unwrap();
        let (archive, pin) = owner.checkpoint().unwrap();
        let current = owner.index.capture_verified_history_pin().unwrap();
        let expected = NativeMainHistoricalRowsExpectationV1 {
            archive: pin.clone(),
            current: current.clone(),
            commands: vec![pins],
        };
        drop(owner);
        let index = SourceCompactIndex::open_verified_history_owner(
            &temp.0,
            Namespace::new("LTC", false, "txIndex").unwrap(),
            &current,
        )
        .unwrap();
        let replay =
            SourceMainNativeSessionV13::reexecute_historical_rows(&archive, &expected, index)
                .unwrap();
        assert_eq!(replay.financial_runtime().state_value().unwrap(), state);
        assert_eq!(replay.checkpoint().unwrap(), (archive, pin));
        assert_eq!(replay.history[0].validity.len(), 2);
        assert_eq!(
            replay.index.capture_verified_history_pin().unwrap(),
            current
        );
    }
}

#[path = "source_main_native_historical_row_bridge_tests.rs"]
mod historical_row_bridge_tests;

#[path = "source_main_native_cold_prefix_tests.rs"]
mod cold_prefix_tests;
