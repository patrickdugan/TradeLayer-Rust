// Actual-owner candidate tests. Parent must compile these with the new bridge.
// Every raw row/RPC document and expected observation comes from the unchanged
// configured-service source corpus. Expected state is never imported.
use super::*;
use crate::source_contract_replay::{SourceContractCommand, SourceContractCommandOutcome};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_SERVICE_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-retained-caller-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn arr(v: V) -> Vec<V> {
    let V::Array(rows) = v else {
        panic!("array: {v:?}")
    };
    rows
}
include!("source_main_retained_caller_test_transport.rs");
fn invoke(financial: &mut SourceContractReplay, target: &str, args: Vec<V>) {
    let result = financial
        .apply(SourceContractCommand::Invoke(DependencyCall {
            target: target.into(),
            arguments: args,
        }))
        .unwrap();
    assert!(
        matches!(
            result.outcome,
            SourceContractCommandOutcome::Dependency(
                DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
            )
        ),
        "{target}: {:?}",
        result.outcome
    );
}
fn setup(temp: &Temp, mode: &str) -> (SourceMainNativeSessionV13, V) {
    let input = record(mode, "input");
    let reset = untag(raw_get(input, "rawReset"));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    // Reproduce DECLARED reference fixture construction followed by raw
    // reset(empty:true). No expected business table/state is read or imported.
    assert_eq!(reset.get("empty"), V::Bool(true));
    let constructor = untag(raw_get(input, "rawHarnessConstructorInput"));
    assert_eq!(
        constructor.get("profile"),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    for raw_collection in arr(constructor.get("collections")) {
        let V::String(collection) = raw_collection.get("collection") else {
            panic!("raw collection name")
        };
        financial
            .apply(SourceContractCommand::Import {
                collection,
                rows: vec![],
            })
            .unwrap();
    }
    let client = reset.get("clientProfile");
    let client = obj(&[("chain", client.get("chain")), ("test", client.get("test"))]);
    invoke(
        &mut financial,
        "SourceActivation.configureClient",
        vec![client.clone()],
    );
    invoke(&mut financial, "Activation.init", vec![reset.get("init")]);
    invoke(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            ("client", client),
            ("io", reset.get("orchestration")),
            ("genesisBlock", reset.get("mainGenesis")),
        ])],
    );
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
    financial
        .apply(SourceContractCommand::SetHeight(
            reset.get("clientProfile").get("height"),
        ))
        .unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual configured service raw inputs",
    )
    .unwrap();
    for row in arr(untag(raw_get(input, "rawNativeRows"))) {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    let docs = untag(raw_get(input, "rawRPCDocuments"));
    let docs = if docs == V::Undefined {
        V::Array(vec![])
    } else {
        V::Array(vec![
            obj(&[
                ("id", docs.get("id")),
                ("raw", docs.get("raw")),
                ("document", docs.get("document")),
            ]),
            obj(&[
                ("id", docs.get("parent").get("txid")),
                ("document", docs.get("parent")),
            ]),
        ])
    };
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    (
        SourceMainNativeSessionV13::adopt(
            NativeMainOwners::adopt(financial, index, &expected).unwrap(),
        )
        .unwrap(),
        docs,
    )
}

fn begin(
    temp: &Temp,
    case: &str,
    command: NativeMainRetainedCallerV1,
) -> SourceMainNativeRetainedCallerV1 {
    let (owner, documents) = setup(temp, case);
    let expected = owner.expectation().unwrap();
    owner
        .into_retained_main_caller(&expected, command, 501, 71, documents)
        .unwrap()
}
fn drive_to_boundary(c: &mut SourceMainNativeRetainedCallerV1) -> NativeMainRetainedCallerStopV1 {
    for _ in 0..128 {
        match c.advance() {
            NativeMainRetainedCallerStopV1::Native(
                crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite,
            ) => {
                let descriptor = c.pending_descriptor().unwrap();
                assert_eq!(descriptor.value().get("role"), s("Awaited"));
                assert_eq!(
                    c.append_pending(&descriptor, None),
                    crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
                );
                assert_eq!(
                    c.resolve_post(&descriptor),
                    crate::source_contract_runtime::NativeMainRowStopV1::Complete
                );
            }
            other => return other,
        }
    }
    panic!("bounded actual caller")
}
#[test]
fn actual_sync_caller_returns_before_detached_save_and_applied_write_keeps_same_source_await() {
    let temp = Temp::new();
    let mut c = begin(
        &temp,
        "sync-final-first/fulfill",
        NativeMainRetainedCallerV1::SyncIndex {
            max: V::number(10.),
        },
    );
    assert_eq!(
        drive_to_boundary(&mut c),
        NativeMainRetainedCallerStopV1::CallerReturned
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(
        c.pending_write().is_none(),
        "caller result before detached native IO preparation"
    );
    let descriptor = c.pending_descriptor().unwrap();
    assert_eq!(descriptor.value().get("role"), s("Detached"));
    assert_eq!(
        c.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    assert_eq!(
        c.pending_write().unwrap().arguments()[0].get("_id"),
        s("MaxHeight")
    );
    assert_eq!(
        c.append_pending(&descriptor, None),
        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
    );
    let effects = c.effects().to_vec();
    let actual_post = c.owner.index.snapshot().unwrap();
    assert_eq!(
        c.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    assert_eq!(c.pending_descriptor(), Some(descriptor.clone()));
    assert_eq!(c.effects(), effects);
    assert_eq!(c.owner.index.snapshot().unwrap(), actual_post);
    assert_eq!(
        c.resolve_post(&descriptor),
        crate::source_contract_runtime::NativeMainRowStopV1::Complete
    );
    assert_eq!(
        c.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    let second = c.pending_descriptor().unwrap();
    assert_ne!(second, descriptor);
    assert_eq!(second.value().get("operationOrdinal"), s("1"));
    assert_eq!(
        c.resolve_post(&descriptor),
        crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
            SourceRuleError::InvalidInput
        )
    );
    assert_eq!(
        c.append_pending(&second, None),
        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        c.resolve_post(&second),
        crate::source_contract_runtime::NativeMainRowStopV1::Complete
    );
    assert_eq!(c.advance(), NativeMainRetainedCallerStopV1::Complete);
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(c.effects().len() > effects.len());
    let mut names = vec![];
    c.visit_canonical_fields(&mut |p, _| {
        names.push(p.to_owned());
        Ok(())
    })
    .unwrap();
    assert!(names.contains(&"caller/final/present".to_owned()));
}
#[test]
fn actual_init_or_load_preserves_static_initializer_and_awaited_extractor_source_catches() {
    let temp = Temp::new();
    let mut c = begin(
        &temp,
        "init-final-first/fulfill",
        NativeMainRetainedCallerV1::InitOrLoad,
    );
    assert_eq!(
        drive_to_boundary(&mut c),
        NativeMainRetainedCallerStopV1::CallerReturned
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(
        c.pending_descriptor().unwrap().value().get("role"),
        s("Detached")
    );
    // A declared source DB rejection is admitted through the actual Core failure
    // path at this still-unattempted detached await, never a supplied result.
    c.owner
        .financial
        .native_main_runtime_mut()
        .fail_storage(crate::source_contract_runtime::StorageFailure {
            collection: "txIndex".into(),
            operation: "updateAsync".into(),
            skip: 0,
            message: "recorded-final-save-rejection".into(),
        })
        .unwrap();
    assert_eq!(c.advance(), NativeMainRetainedCallerStopV1::Complete);
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(c
        .effects()
        .iter()
        .any(|effect| effect.get("kind") == s("unhandled-rejection")
            && effect.get("thrown").get("message") == s("recorded-final-save-rejection")));
}
#[test]
fn actual_initialize_retains_real_delay_instance_genesis_and_consensus_boundary() {
    let temp = Temp::new();
    let mut c = begin(
        &temp,
        "initialize-final-first/fulfill",
        NativeMainRetainedCallerV1::Initialize,
    );
    assert_eq!(c.advance(), NativeMainRetainedCallerStopV1::PendingTimer);
    assert!(c.caller_result().is_none());
    let timer = c.next_timer().unwrap();
    assert!(c.fire_timer(timer, 70).is_err());
    c.fire_timer(timer, 71).unwrap();
    assert_eq!(
        drive_to_boundary(&mut c),
        NativeMainRetainedCallerStopV1::ConsensusReady
    );
    assert!(
        c.caller_result().is_none(),
        "consensus has not executed and Initialize has not fulfilled"
    );
    let mut fields = vec![];
    c.visit_canonical_fields(&mut |p, v| {
        fields.push((p.to_owned(), v.clone()));
        Ok(())
    })
    .unwrap();
    assert!(fields.contains(&("caller/genesis".into(), V::number(2819000.))));
    assert_eq!(
        c.pending_descriptor().unwrap().value().get("role"),
        s("Detached")
    );
    assert!(
        fields.contains(&("caller/invocation/present".into(), V::Bool(true))),
        "same borrowed invocation is retained"
    );
}

#[test]
fn actual_static_initialization_failure_escapes_but_instance_failure_is_caught() {
    let temp = Temp::new();
    let (mut owner, documents) = setup(&temp, "initialize-final-first/fulfill");
    owner
        .financial
        .native_main_runtime_mut()
        .fail_storage(crate::source_contract_runtime::StorageFailure {
            collection: "txIndex".into(),
            operation: "insertAsync".into(),
            skip: 0,
            message: "recorded-genesis-insert-rejection".into(),
        })
        .unwrap();
    let expected = owner.expectation().unwrap();
    let mut c = owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::InitOrLoad,
            501,
            71,
            documents,
        )
        .unwrap();
    assert_eq!(c.advance(), NativeMainRetainedCallerStopV1::Complete);
    assert!(
        matches!(c.caller_result(),Some(DependencyReturn::PendingThrow(error)) if error.message=="recorded-genesis-insert-rejection")
    );
    let temp = Temp::new();
    let mut c = begin(
        &temp,
        "initialize-final-first/fulfill",
        NativeMainRetainedCallerV1::Initialize,
    );
    c.owner
        .financial
        .native_main_runtime_mut()
        .fail_storage(crate::source_contract_runtime::StorageFailure {
            collection: "txIndex".into(),
            operation: "insertAsync".into(),
            skip: 0,
            message: "recorded-instance-genesis-insert-rejection".into(),
        })
        .unwrap();
    assert_eq!(c.advance(), NativeMainRetainedCallerStopV1::PendingTimer);
    c.fire_timer(c.next_timer().unwrap(), 71).unwrap();
    assert_eq!(
        drive_to_boundary(&mut c),
        NativeMainRetainedCallerStopV1::ConsensusReady
    );
    assert!(c.caller_result().is_none());
    assert!(c
        .effects()
        .iter()
        .any(|effect| effect.get("collection") == s("txIndex")
            && effect.get("thrown").get("message")
                == s("recorded-instance-genesis-insert-rejection")));
}

#[path = "source_main_retained_caller_review_tests.rs"]
mod independent_review;

#[path = "source_main_coercive_sync_core_tests.rs"]
mod coercive_sync_review;

#[path = "source_main_null_sync_core_tests.rs"]
mod null_sync_review;

#[path = "source_main_null_live_prefix_review_tests.rs"]
mod null_live_prefix_review;
