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
            "actual-caller-start-wal-{}-{}",
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

fn controller(temp: &Temp) -> SourceMainNativeDurableCallerStartV1 {
    let (owner, documents) = setup(temp, "initialize-final-first/fulfill");
    let expected = owner.expectation().unwrap();
    let walroot = temp.root.join("wal");
    std::fs::create_dir(&walroot).unwrap();
    SourceMainNativeDurableCallerStartV1::create(
        owner,
        &expected,
        &walroot,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        crate::source_main_retained_caller_action::RetainedMainCallerActionV1::Start {
            caller:
                crate::source_main_retained_caller_action::RetainedMainCallerStartV1::Initialize,
            scope: 501,
            generation: 71,
            documents,
        },
        64 * 1024 * 1024,
    )
    .unwrap()
}
fn actual_index(c: &SourceMainNativeDurableCallerStartV1) -> &SourceCompactIndex {
    match c.owner.as_ref().unwrap() {
        RetainedCallerStartOwnerV1::Ready(owner) => &owner.index,
        RetainedCallerStartOwnerV1::Caller(caller) => &caller.owner.index,
    }
}
#[test]
fn actual_constructor_does_not_run_before_its_synced_start_intent() {
    let temp = Temp::new();
    let mut c = controller(&temp);
    assert!(!c.constructor_dispatched());
    assert!(c.actual_route().is_none());
    let before = actual_index(&c).snapshot().unwrap();
    let effects = c.effects().to_vec();
    assert_eq!(
        c.wal.origin_context().unwrap().get("origin").get("profile"),
        s(RETAINED_CALLER_START_WAL_PROFILE)
    );
    assert_eq!(
        c.origin().get("rawStart"),
        CallerStartBlob::of(&c.raw).unwrap().value()
    );
    c.barrier_fault = Some((
        RetainedCallerStartWalPhaseV1::Intent,
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
    ));
    assert!(c.resume().is_err());
    assert!(!c.constructor_dispatched());
    assert!(c.actual_route().is_none());
    assert_eq!(actual_index(&c).snapshot().unwrap(), before);
    assert_eq!(c.effects(), effects);
    assert_eq!(
        c.recover_wal().unwrap(),
        crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(
        c.resume().unwrap(),
        RetainedCallerStartWalPhaseV1::StartAcknowledged
    );
    assert!(c.constructor_dispatched());
    assert!(c.actual_route().is_some());
    assert_eq!(
        c.actual_route()
            .unwrap()
            .get("retainedInvocation")
            .get("actualGeneration"),
        s("71")
    );
    assert_eq!(actual_index(&c).snapshot().unwrap(),before,"constructor performs only its actual module/header prefix, not the later native source write");
    assert!(c.effects().len() > effects.len());
    assert_eq!(c.wal.record().unwrap().get("stage"), s("Ack"));
}
#[test]
fn real_accepted_or_ack_lost_return_rebinds_wal_without_a_second_constructor() {
    for phase in [
        RetainedCallerStartWalPhaseV1::Accepted,
        RetainedCallerStartWalPhaseV1::Ack,
    ] {
        let temp = Temp::new();
        let mut c = controller(&temp);
        c.barrier_fault = Some((
            phase,
            crate::source_main_active_service_wal::TestFault::AfterCommit,
        ));
        assert!(c.resume().is_err());
        assert_eq!(c.phase(), phase);
        assert!(c.constructor_dispatched());
        let effects = c.effects().to_vec();
        let route = c.actual_route().cloned();
        let native = actual_index(&c).snapshot().unwrap();
        assert_eq!(
            c.recover_wal().unwrap(),
            crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post
        );
        assert_eq!(
            c.resume().unwrap(),
            RetainedCallerStartWalPhaseV1::StartAcknowledged
        );
        assert_eq!(c.effects(), effects);
        assert_eq!(c.actual_route().cloned(), route);
        assert_eq!(actual_index(&c).snapshot().unwrap(), native);
        assert_eq!(c.wal.record().unwrap().get("stage"), s("Ack"));
        assert_eq!(
            c.resume().unwrap(),
            RetainedCallerStartWalPhaseV1::StartAcknowledged
        );
        assert_eq!(c.effects(), effects);
    }
}
#[test]
fn real_accepted_before_state_retry_keeps_same_started_source_and_observation() {
    let temp = Temp::new();
    let mut c = controller(&temp);
    c.barrier_fault = Some((
        RetainedCallerStartWalPhaseV1::Accepted,
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
    ));
    assert!(c.resume().is_err());
    let effects = c.effects().to_vec();
    let route = c.actual_route().cloned();
    let post = c.post.clone();
    assert!(c.constructor_dispatched());
    assert_eq!(
        c.recover_wal().unwrap(),
        crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Before
    );
    assert_eq!(
        c.resume().unwrap(),
        RetainedCallerStartWalPhaseV1::StartAcknowledged
    );
    assert_eq!(c.effects(), effects);
    assert_eq!(c.actual_route().cloned(), route);
    assert_eq!(c.post, post);
    assert!(
        matches!(
            c.owner.as_ref(),
            Some(RetainedCallerStartOwnerV1::Caller(_))
        ),
        "same actual caller/frame stays privately owned"
    );
}

#[test]
fn real_synced_intent_lost_return_does_not_dispatch_source_until_exact_post_rebind() {
    let temp = Temp::new();
    let mut c = controller(&temp);
    let before = actual_index(&c).snapshot().unwrap();
    let effects = c.effects().to_vec();
    c.barrier_fault = Some((
        RetainedCallerStartWalPhaseV1::Intent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(c.resume().is_err());
    assert!(!c.constructor_dispatched());
    assert!(c.actual_route().is_none());
    assert_eq!(c.effects(), effects);
    assert_eq!(actual_index(&c).snapshot().unwrap(), before);
    assert_eq!(
        c.recover_wal().unwrap(),
        crate::source_main_active_service_wal::ActiveServiceWalResolvedV1::Post
    );
    assert_eq!(c.phase(), RetainedCallerStartWalPhaseV1::Execute);
    assert!(!c.constructor_dispatched());
    assert_eq!(
        c.resume().unwrap(),
        RetainedCallerStartWalPhaseV1::StartAcknowledged
    );
    assert!(c.constructor_dispatched());
    assert!(c.actual_route().is_some());
    let accepted_effects = c.effects().to_vec();
    assert_eq!(
        c.resume().unwrap(),
        RetainedCallerStartWalPhaseV1::StartAcknowledged
    );
    assert_eq!(c.effects(), accepted_effects);
}
