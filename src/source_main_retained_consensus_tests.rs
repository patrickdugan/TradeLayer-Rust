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
            "actual-retained-consensus-{}-{}",
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

fn ready(temp: &Temp) -> SourceMainNativeRetainedCallerV1 {
    let (owner, documents) = setup(temp, "initialize-final-first/fulfill");
    let expected = owner.expectation().unwrap();
    let mut c = owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::Initialize,
            501,
            71,
            documents,
        )
        .unwrap();
    assert_eq!(c.advance(), NativeMainRetainedCallerStopV1::PendingTimer);
    c.fire_timer(c.next_timer().unwrap(), 71).unwrap();
    for _ in 0..128 {
        match c.advance() {
            NativeMainRetainedCallerStopV1::ConsensusReady => return c,
            NativeMainRetainedCallerStopV1::Native(
                crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite,
            ) => {
                let p = c.pending_descriptor().unwrap();
                assert_eq!(
                    c.append_pending(&p, None),
                    crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
                );
                assert_eq!(
                    c.resolve_post(&p),
                    crate::source_contract_runtime::NativeMainRowStopV1::Complete
                );
            }
            other => panic!("actual initialization prefix: {other:?}"),
        }
    }
    panic!("bounded actual initializer")
}
fn caller_operation(
    c: &SourceMainNativeRetainedConsensusV1,
) -> NativeMainRetainedConsensusPendingV1 {
    c.pending_operations()
        .into_iter()
        .find(|p| p.kind() == NativeMainRetainedConsensusOperationKindV1::CallerInline)
        .unwrap()
}
fn drain_detached(c: &mut SourceMainNativeRetainedConsensusV1) {
    for _ in 0..16 {
        let Some(p) = c
            .pending_operations()
            .into_iter()
            .find(|p| p.kind() == NativeMainRetainedConsensusOperationKindV1::CallerInline)
        else {
            return;
        };
        match c.advance_detached(&p).unwrap() {
            NativeMainRetainedConsensusStopV1::Detached(
                NativeMainRetainedCallerStopV1::Native(
                    crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite,
                ),
            ) => {
                let p = caller_operation(c);
                assert_eq!(
                    c.append_pending(&p, None).unwrap(),
                    crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
                );
                assert_eq!(
                    c.resolve_post(&p).unwrap(),
                    crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
                );
            }
            NativeMainRetainedConsensusStopV1::Detached(
                NativeMainRetainedCallerStopV1::ConsensusReady,
            ) => (),
            other => panic!("actual detached source continuation: {other:?}"),
        }
    }
    panic!("bounded actual detached save")
}
#[test]
fn real_initialize_consensus_fulfills_while_same_final_save_is_held_before_apply() {
    let temp = Temp::new();
    let mut caller = ready(&temp);
    assert!(caller.caller_result().is_none());
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    let before = caller.owner.index.snapshot().unwrap();
    let prefix = caller.effects().to_vec();
    let mut c = caller.into_retained_consensus().unwrap();
    assert_eq!(
        c.advance_consensus(4096).unwrap(),
        NativeMainRetainedConsensusStopV1::CallerReturned
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(c.caller.owner.index.snapshot().unwrap(), before);
    assert!(c.effects().starts_with(&prefix));
    assert!(
        c.effects()
            .iter()
            .any(|e| e.get("collection") == s("consensus") && e.get("kind") == s("db")),
        "real consensus body executed"
    );
    let mut fields = vec![];
    c.visit_canonical_fields(&mut |p, v| {
        fields.push((p.to_owned(), v.clone()));
        Ok(())
    })
    .unwrap();
    assert!(fields.contains(&("caller/caller/invocation/present".into(), V::Bool(true))));
    assert!(fields.contains(&(
        "service/borrowedInvocation".into(),
        s("TL-MAIN-RETAINED-CALLER-CONSENSUS-13-1"),
    )));
    assert!(!fields
        .iter()
        .any(|(path, _)| path.starts_with("service/catchup/invocation/")));
    let p = caller_operation(&c);
    assert_eq!(p.value().get("role"), s("Detached"));
    assert!(
        c.advance_consensus(1).is_err(),
        "later discarded dispatch waits for the retained native operation in this admission"
    );
    assert_eq!(
        c.append_pending(&p, None).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    );
    assert!(matches!(
        c.append_pending(&p, None),
        Err(NativeMainRetainedConsensusErrorV1::WriteAlreadyAttempted)
    ));
    assert_eq!(
        c.resolve_post(&p).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
    );
    assert!(matches!(
        c.resolve_post(&p),
        Err(NativeMainRetainedConsensusErrorV1::NoAttemptedWrite)
    ));
    drain_detached(&mut c);
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(
        c.caller.frame.is_some(),
        "same financial owner/invocation stays private after caller fulfillment"
    );
}
#[test]
fn real_applied_final_save_return_stays_held_through_actual_initialize_fulfillment() {
    let temp = Temp::new();
    let mut caller = ready(&temp);
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    let descriptor = caller.pending_descriptor().unwrap();
    assert_eq!(
        caller.append_pending(&descriptor, None),
        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
    );
    let post = caller.owner.index.snapshot().unwrap();
    let mut c = caller.into_retained_consensus().unwrap();
    let original = caller_operation(&c);
    assert_eq!(
        c.advance_consensus(4096).unwrap(),
        NativeMainRetainedConsensusStopV1::CallerReturned
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(c.caller.owner.index.snapshot().unwrap(), post);
    assert_eq!(caller_operation(&c), original);
    assert_eq!(
        c.resolve_post(&original).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
    );
    drain_detached(&mut c);
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
}
#[test]
fn real_detached_rejection_is_observed_after_actual_initialize_result_without_early_queue_leave() {
    let temp = Temp::new();
    let caller = ready(&temp);
    let mut c = caller.into_retained_consensus().unwrap();
    assert_eq!(
        c.advance_consensus(4096).unwrap(),
        NativeMainRetainedConsensusStopV1::CallerReturned
    );
    c.caller
        .owner
        .financial
        .native_main_runtime_mut()
        .fail_storage(crate::source_contract_runtime::StorageFailure {
            collection: "txIndex".into(),
            operation: "updateAsync".into(),
            skip: 0,
            message: "recorded-detached-after-consensus".into(),
        })
        .unwrap();
    let p = caller_operation(&c);
    assert_eq!(
        c.advance_detached(&p).unwrap(),
        NativeMainRetainedConsensusStopV1::Detached(NativeMainRetainedCallerStopV1::ConsensusReady)
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(
        c.effects()
            .iter()
            .filter(|e| e.get("kind") == s("unhandled-rejection")
                && e.get("thrown").get("message") == s("recorded-detached-after-consensus"))
            .count(),
        1
    );
    let mut fields = vec![];
    c.visit_canonical_fields(&mut |p, v| {
        fields.push((p.to_owned(), v.clone()));
        Ok(())
    })
    .unwrap();
    assert!(fields.contains(&("caller/caller/invocation/present".into(), V::Bool(false))));
    assert!(c
        .pending_operations()
        .iter()
        .all(|p| p.kind() != NativeMainRetainedConsensusOperationKindV1::CallerInline));
}

fn raw_financial_row() -> V {
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
fn setup_financial_row(temp: &Temp, mode: &str) -> (SourceMainNativeSessionV13, V) {
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
    for row in arr(untag(raw_get(input, "rawNativeRows")))
        .into_iter()
        .chain(std::iter::once(raw_financial_row()))
    {
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

#[test]
fn actual_equal_tip_catchup_executes_financial_row_once_and_resolves_selected_native_await() {
    let temp = Temp::new();
    let (owner, documents) = setup_financial_row(&temp, "sync-final-first/fulfill");
    let expected = owner.expectation().unwrap();
    let mut caller = owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::SyncIndex {
                max: V::number(11.),
            },
            501,
            71,
            documents,
        )
        .unwrap();
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::ConsensusReady
    );
    assert!(caller.caller_result().is_none());
    let mut c = caller.into_retained_consensus().unwrap();
    assert_eq!(
        c.advance_consensus(4096).unwrap(),
        NativeMainRetainedConsensusStopV1::Service(
            crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
        )
    );
    let operation = c
        .pending_operations()
        .into_iter()
        .find(|p| p.kind() == NativeMainRetainedConsensusOperationKindV1::ServiceValidity)
        .unwrap();
    assert_eq!(operation.value().get("role"), s("Awaited"));
    assert!(
        c.effects()
            .iter()
            .any(|e| e.get("collection") == s("tallyMap") && e.get("method") == s("updateAsync")),
        "actual Main0 financial source prefix executed"
    );
    assert!(
        !c.caller
            .owner
            .financial
            .runtime()
            .collection("propertyList")
            .is_empty(),
        "actual Main0 property initialization executed before its native await"
    );
    let prefix = c.effects().to_vec();
    let financial = c.financial_state().unwrap();
    assert_eq!(
        c.append_pending(&operation, None).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    );
    assert_eq!(
        c.advance_consensus(1).unwrap(),
        NativeMainRetainedConsensusStopV1::Service(
            crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
        )
    );
    assert_eq!(c.effects(), prefix);
    assert_eq!(c.financial_state().unwrap(), financial);
    assert_eq!(c.pending_operations(), vec![operation.clone()]);
    assert!(matches!(
        c.resolve_post(&operation).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
            | crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    ));
    assert_eq!(
        c.advance_consensus(4096).unwrap(),
        NativeMainRetainedConsensusStopV1::CallerReturned
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
}

#[test]
fn new_actual_origin_route_refuses_equal_logical_selectors_from_another_real_owner() {
    let ta = Temp::new();
    let tb = Temp::new();
    let a = ready(&ta);
    let b = ready(&tb);
    assert_eq!(
        a.pending_descriptor(),
        b.pending_descriptor(),
        "old profile remains a receiver-local logical selector"
    );
    let mut a = a.into_retained_consensus().unwrap();
    let mut b = b.into_retained_consensus().unwrap();
    let pa = caller_operation(&a);
    let pb = caller_operation(&b);
    assert_ne!(pa, pb);
    assert_eq!(pa.value().get("caller"), pb.value().get("caller"));
    assert_ne!(pa.value().get("route"), pb.value().get("route"));
    let before = b.caller.owner.index.snapshot().unwrap();
    let effects = b.effects().to_vec();
    assert!(matches!(
        b.advance_detached(&pa),
        Err(NativeMainRetainedConsensusErrorV1::InvalidInput)
    ));
    assert!(matches!(
        b.append_pending(&pa, None),
        Err(NativeMainRetainedConsensusErrorV1::InvalidInput)
    ));
    assert_eq!(b.caller.owner.index.snapshot().unwrap(), before);
    assert_eq!(b.effects(), effects);
    assert_eq!(caller_operation(&b), pb);
    assert_eq!(pa.value().get("route").get("declaredGeneration"), s("71"));
    assert_eq!(
        pa.value()
            .get("route")
            .get("retainedInvocation")
            .get("actualGeneration"),
        s("71")
    );
    assert_eq!(
        a.advance_detached(&pa).unwrap(),
        NativeMainRetainedConsensusStopV1::Detached(NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        ))
    );
    let prepared = caller_operation(&a);
    let origin = prepared.value().get("route");
    assert_eq!(
        a.append_pending(&prepared, None).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    );
    assert_eq!(
        caller_operation(&a).value().get("route"),
        origin,
        "actual physical publication does not reconstruct the original route"
    );
    assert_eq!(
        a.resolve_post(&prepared).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
    );
    assert_eq!(
        caller_operation(&a).value().get("route"),
        origin,
        "same origin survives source resolution into the ready job"
    );
}
