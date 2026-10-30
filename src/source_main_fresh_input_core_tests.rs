// Actual-owner controls. Parent owns Core compilation. Raw setup is taken
// from the genuine source input/catalog, never observed financial state.
use super::*;
use crate::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O,
};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
include!("source_main_fresh_input_test_transport.rs");
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
            "actual-fresh-rpc-input-{}-{}",
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
fn list(value: V) -> Vec<V> {
    let V::Array(rows) = value else {
        panic!("raw array")
    };
    rows
}
fn source(mode: &str, point: &str) -> &'static str {
    include_str!("../fixtures/native_main_service_tail_observer2.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "mode")) == mode
                && json_string(raw_get(line, "point")) == point
        })
        .unwrap()
}
fn caller_source() -> &'static str {
    include_str!("../fixtures/native_main_inline_callers.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "case")) == "initialize-final-first/fulfill"
                && json_string(raw_get(line, "kind")) == "input"
        })
        .unwrap()
}
fn fresh(mode: &str) -> NativeMainFreshRpcInputV1 {
    let fixture = if mode == "incoming-fresh-tip" {
        include_str!("../fixtures/main_fresh_input_service_chain.jsonl")
    } else {
        include_str!("../fixtures/main_fresh_input.jsonl")
    };
    let line = fixture
        .lines()
        .find(|line| {
            json_string(raw_get(line, "mode")) == mode
                && json_string(raw_get(line, "point")) == "raw-facts-fed-same-await-still-held"
        })
        .unwrap();
    let raw = untag(raw_get(line, "rawControl")).get("raw");
    let documents = raw.get("documents");
    NativeMainFreshRpcInputV1::new(
        if raw.get("io") == V::Undefined {
            obj(&[])
        } else {
            raw.get("io")
        },
        if documents == V::Undefined {
            None
        } else {
            Some(documents)
        },
    )
    .unwrap()
}
fn invoke(financial: &mut SourceContractReplay, target: &str, arguments: Vec<V>) {
    let receipt = financial
        .apply(C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        }))
        .unwrap();
    assert!(
        matches!(receipt.outcome, O::Dependency(_)),
        "{target}: {:?}",
        receipt.outcome
    );
}
fn setup(temp: &Temp, input: &str, native_key: &str) -> (SourceMainNativeSessionV13, V) {
    let reset = untag(raw_get(input, "rawReset"));
    assert_eq!(reset.get("empty"), V::Bool(true));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let constructor = untag(raw_get(input, "rawHarnessConstructorInput"));
    assert_eq!(
        constructor.get("profile"),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    for row in list(constructor.get("collections")) {
        let V::String(collection) = row.get("collection") else {
            panic!("raw collection")
        };
        financial
            .apply(C::Import {
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
        .apply(C::SetHeight(reset.get("clientProfile").get("height")))
        .unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual fresh rpc input source",
    )
    .unwrap();
    for row in list(untag(raw_get(input, native_key))) {
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
fn start(temp: &Temp, mode: &str) -> SourceMainNativeFreshInputServiceV1 {
    let (owner, docs) = setup(temp, source(mode, "input"), "nativeInput");
    let expected = owner.expectation().unwrap();
    owner
        .into_inline_active_service(&expected, V::number(10.), 71, docs, 64 * 1024 * 1024)
        .unwrap()
        .into_fresh_input_service()
        .unwrap()
}
fn apply(
    owner: &mut SourceMainNativeFreshInputServiceV1,
    action: NativeMainActiveServiceActionV1,
) -> NativeMainActiveServiceActionResultV1 {
    owner.apply(action).unwrap().result.clone()
}
fn drive(
    owner: &mut SourceMainNativeFreshInputServiceV1,
    hold_live_validity: bool,
) -> crate::source_contract_runtime::NativeMainServiceStopV1 {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    for _ in 0..128 {
        let NativeMainActiveServiceActionResultV1::Stop(stop) = apply(
            owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        ) else {
            panic!("actual source stop")
        };
        if stop != Stop::AwaitNativeWrite {
            return stop;
        }
        if hold_live_validity
            && owner
                .active
                .frame
                .as_ref()
                .unwrap()
                .fresh_input_test_live_request()
                .is_some()
            && owner.pending_write().is_some()
        {
            return stop;
        }
        apply(owner, NativeMainActiveServiceActionV1::AppendPending);
        apply(owner, NativeMainActiveServiceActionV1::ResolvePost);
    }
    panic!("bounded actual source")
}
fn fields(owner: &SourceMainNativeFreshInputServiceV1) -> Vec<(String, V)> {
    let mut result = vec![];
    owner
        .visit_canonical_fields(&mut |p, v| {
            result.push((p.into(), v.clone()));
            Ok(())
        })
        .unwrap();
    result
}
#[test]
fn fresh_tip_and_block_resume_same_live_timer_preserving_rpc_ordinals_and_owner() {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let t = Temp::new();
    let mut owner = start(&t, "live-iteration-pause-boundary");
    assert_eq!(drive(&mut owner, false), Stop::PendingTimer);
    let timer = owner.next_timer().unwrap();
    let kernel = owner.active.kernel_value();
    let prefix = owner.effects().to_vec();
    let before = owner.active.owner.index.snapshot().unwrap();
    let state = owner.financial_state().unwrap();
    owner.feed(fresh("incoming-fresh-tip")).unwrap();
    assert_eq!(owner.next_timer(), Some(timer));
    assert_eq!(owner.active.kernel_value(), kernel);
    assert_eq!(owner.effects(), prefix);
    assert_eq!(owner.active.owner.index.snapshot().unwrap(), before);
    let updated = owner.financial_state().unwrap();
    assert_eq!(updated.get("tables"), state.get("tables"));
    assert_eq!(
        updated.get("mainProduction").get("counts"),
        state.get("mainProduction").get("counts")
    );
    assert_eq!(
        updated.get("mainProduction").get("client"),
        state.get("mainProduction").get("client")
    );
    apply(
        &mut owner,
        NativeMainActiveServiceActionV1::FireTimer {
            id: timer,
            generation: 71,
        },
    );
    assert_eq!(drive(&mut owner, false), Stop::PendingTimer);
    assert!(owner
        .active
        .financial_runtime()
        .collection("persistence")
        .iter()
        .any(|row| row.get("_id") == s("block-12") && row.get("hash") == s("fresh12")));
    assert!(owner
        .active
        .financial_runtime()
        .collection("consensus")
        .iter()
        .any(|row| row.get("_id") == s("TrackHeight") && row.get("value") == V::number(12.)));
    let counts = owner
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("rpc-input") && e.get("method") == s("Client.getBlockCount"))
        .map(|e| e.get("ordinal"))
        .collect::<Vec<_>>();
    let ordinals = counts
        .iter()
        .map(|value| {
            let V::Number(bits) = value else {
                panic!("actual RPC ordinal")
            };
            f64::from_bits(*bits)
        })
        .collect::<Vec<_>>();
    assert!(
        ordinals.windows(2).all(|pair| pair[1] == pair[0] + 1.),
        "exact continuing RPC ordinals"
    );
    assert_eq!(owner.feeds().len(), 1);
    assert_eq!(owner.profile(), NATIVE_MAIN_FRESH_INPUT_SERVICE_PROFILE);
}
#[test]
fn fresh_documents_cannot_change_or_reexecute_actual_financial_validity_await() {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let t = Temp::new();
    let mut owner = start(&t, "same-held-live-validity");
    assert_eq!(drive(&mut owner, true), Stop::AwaitNativeWrite);
    let plan = owner.pending_write().unwrap() as *const _;
    let context = owner.active.pending_native_context();
    let state = owner.financial_state().unwrap();
    let prefix = owner.effects().to_vec();
    let physical = owner.active.owner.index.snapshot().unwrap();
    let captured = owner
        .active
        .frame
        .as_ref()
        .unwrap()
        .fresh_input_test_live_request()
        .unwrap();
    assert!(!owner
        .active
        .financial_runtime()
        .collection("tallyMap")
        .is_empty());
    owner.feed(fresh("validity-held-fresh-facts")).unwrap();
    assert_eq!(owner.pending_write().unwrap() as *const _, plan);
    assert_eq!(owner.active.pending_native_context(), context);
    assert_eq!(owner.active.owner.index.snapshot().unwrap(), physical);
    assert_eq!(
        owner
            .active
            .frame
            .as_ref()
            .unwrap()
            .fresh_input_test_live_request()
            .unwrap(),
        captured
    );
    assert_eq!(
        owner.financial_state().unwrap().get("tables"),
        state.get("tables")
    );
    assert_eq!(
        owner
            .financial_state()
            .unwrap()
            .get("mainProduction")
            .get("counts"),
        state.get("mainProduction").get("counts")
    );
    assert_eq!(
        owner
            .active
            .frame
            .as_ref()
            .unwrap()
            .fresh_input_test_documents(),
        &V::Array(vec![])
    );
    assert_eq!(owner.effects(), prefix);
    for _ in 0..3 {
        assert_eq!(drive(&mut owner, true), Stop::AwaitNativeWrite);
        assert_eq!(owner.pending_write().unwrap() as *const _, plan);
        assert_eq!(owner.effects(), prefix);
    }
    apply(&mut owner, NativeMainActiveServiceActionV1::Pause);
    apply(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(drive(&mut owner, true), Stop::AwaitNativeWrite);
    apply(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(drive(&mut owner, false), Stop::PendingTimer);
    assert!(owner.effects().iter().rev().any(
        |e| e.get("method") == s("Client.getBlockCount") && e.get("returned") == V::number(12.)
    ));
    assert!(owner.effects().starts_with(&prefix));
}
#[test]
fn invalid_or_observation_pending_or_terminal_feed_does_not_change_source_or_feed_history() {
    let t = Temp::new();
    let mut owner = start(&t, "live-iteration-pause-boundary");
    drive(&mut owner, false);
    let original = fields(&owner);
    let state = owner.financial_state().unwrap();
    assert!(NativeMainFreshRpcInputV1::new(obj(&[("processed", V::Bool(true))]), None).is_err());
    assert_eq!(fields(&owner), original);
    assert_eq!(owner.financial_state().unwrap(), state);
    owner.active.observation_limit = 1;
    assert!(owner.apply(NativeMainActiveServiceActionV1::Pause).is_err());
    assert!(owner.active.observation_pending());
    let held = owner.financial_state().unwrap();
    let events = owner.effects().to_vec();
    assert!(matches!(
        owner.feed(fresh("incoming-fresh-tip")),
        Err(NativeMainActiveServiceErrorV1::ObservationPending)
    ));
    assert!(owner.feeds().is_empty());
    assert_eq!(owner.financial_state().unwrap(), held);
    assert_eq!(owner.effects(), events);
    owner.active.observation_limit = 64 * 1024 * 1024;
    owner.retry_source_observation().unwrap();
    apply(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    let terminal = fields(&owner);
    let held = owner.financial_state().unwrap();
    assert!(matches!(
        owner.feed(fresh("incoming-fresh-tip")),
        Err(NativeMainActiveServiceErrorV1::AlreadyExited(0))
    ));
    assert_eq!(fields(&owner), terminal);
    assert_eq!(owner.financial_state().unwrap(), held);
}
#[test]
fn retained_caller_feed_keeps_same_detached_plan_actual_route_and_borrowed_invocation() {
    use crate::source_contract_runtime::NativeMainRowStopV1 as Row;
    let t = Temp::new();
    let (owner, documents) = setup(&t, caller_source(), "rawNativeRows");
    let expected = owner.expectation().unwrap();
    let mut caller = owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::Initialize,
            501,
            71,
            documents,
        )
        .unwrap();
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::PendingTimer
    );
    caller.fire_timer(caller.next_timer().unwrap(), 71).unwrap();
    for _ in 0..128 {
        match caller.advance() {
            NativeMainRetainedCallerStopV1::ConsensusReady => break,
            NativeMainRetainedCallerStopV1::Native(Row::AwaitNativeWrite) => {
                let p = caller.pending_descriptor().unwrap();
                assert_eq!(caller.append_pending(&p, None), Row::AwaitNativeWrite);
                assert_eq!(caller.resolve_post(&p), Row::Complete);
            }
            other => panic!("actual startup {other:?}"),
        }
    }
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::Native(Row::AwaitNativeWrite)
    );
    let consensus = caller.into_retained_consensus().unwrap();
    let pending = consensus.pending_operations();
    let route = consensus.route.value().clone();
    let before = consensus.caller.owner.index.snapshot().unwrap();
    let state = consensus.financial_state().unwrap();
    let effects = consensus.effects().to_vec();
    let mut owner = consensus.into_fresh_input_caller();
    owner.feed(fresh("validity-held-fresh-facts")).unwrap();
    assert_eq!(owner.pending_operations(), pending);
    assert_eq!(owner.consensus.route.value(), &route);
    assert_eq!(
        owner.consensus.caller.owner.index.snapshot().unwrap(),
        before
    );
    assert_eq!(
        owner.financial_state().unwrap().get("tables"),
        state.get("tables")
    );
    assert_eq!(owner.effects(), effects);
    assert_eq!(owner.consensus.caller.documents, V::Array(vec![]));
    assert_eq!(
        owner.consensus.service.fresh_input_test_documents(),
        &V::Array(vec![])
    );
    let p = pending
        .iter()
        .find(|p| p.kind() == NativeMainRetainedConsensusOperationKindV1::CallerInline)
        .unwrap();
    assert_eq!(
        owner.append_pending(p, None).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    );
    assert_eq!(
        owner.resolve_post(p).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
    );
    assert_eq!(owner.feeds().len(), 1);
    assert_eq!(owner.profile(), NATIVE_MAIN_FRESH_INPUT_CALLER_PROFILE);
}
