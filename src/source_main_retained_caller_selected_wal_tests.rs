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
            "actual-selected-caller-wal-{}-{}",
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
fn setup_compacting(temp: &Temp, mode: &str) -> (SourceMainNativeSessionV13, V) {
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
    // Actual metadata-only byte fixture, before adoption; no financial state or source outcome.
    index.native_main_test_compaction_fixture().unwrap();
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
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::source_main_active_service_wal::{ActiveServiceWalResolvedV1 as Resolved, TestFault};
use crate::source_main_retained_caller_action::{
    RetainedMainCallerActionV1 as Action, RetainedMainCallerStartV1 as Start,
    RetainedMainCallerTimerLaneV1 as Lane,
};
use crate::storage::Recovery;

fn wrap(
    temp: &Temp,
    owner: SourceMainNativeSessionV13,
    documents: V,
    caller: Start,
) -> SourceMainNativeDurableSelectedCallerV1 {
    let expected = owner.expectation().unwrap();
    let wal = temp.root.join("caller-selected-wal");
    let escrow = temp.root.join("caller-selected-escrow");
    std::fs::create_dir(&wal).unwrap();
    std::fs::create_dir(&escrow).unwrap();
    let mut c = SourceMainNativeDurableSelectedCallerV1::create(
        owner,
        &expected,
        &wal,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        Action::Start {
            caller,
            scope: 501,
            generation: 71,
            documents,
        },
        (
            64 * 1024 * 1024,
            escrow,
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        ),
    )
    .unwrap();
    assert_eq!(c.resume().unwrap(), RetainedCallerSelectedWalPhaseV1::Idle);
    c
}
fn controller(temp: &Temp) -> SourceMainNativeDurableSelectedCallerV1 {
    let (owner, docs) = setup(temp, "initialize-final-first/fulfill");
    wrap(temp, owner, docs, Start::Initialize)
}
fn actual(c: &SourceMainNativeDurableSelectedCallerV1) -> &RetainedCallerSelectedOwnerV1 {
    c.owner.as_ref().unwrap()
}
fn index(c: &SourceMainNativeDurableSelectedCallerV1) -> &SourceCompactIndex {
    &actual(c).session().index
}
fn effects(c: &SourceMainNativeDurableSelectedCallerV1) -> &[V] {
    actual(c)
        .session()
        .financial
        .runtime()
        .native_main_row_effects()
}
fn state(c: &SourceMainNativeDurableSelectedCallerV1) -> V {
    actual(c)
        .session()
        .financial
        .runtime()
        .state_value()
        .unwrap()
}
fn action(c: &mut SourceMainNativeDurableSelectedCallerV1, a: Action) {
    c.begin(a).unwrap();
    assert_eq!(c.resume().unwrap(), RetainedCallerSelectedWalPhaseV1::Idle);
}
fn caller_pc(c: &SourceMainNativeDurableSelectedCallerV1) -> V {
    let mut pc = V::Undefined;
    if let RetainedCallerSelectedOwnerV1::Caller(caller) = actual(c) {
        caller
            .visit_canonical_fields(&mut |p, v| {
                if p == "caller/pc" {
                    pc = v.clone();
                }
                Ok(())
            })
            .unwrap();
    }
    pc
}
fn timer(c: &SourceMainNativeDurableSelectedCallerV1) -> u64 {
    let RetainedCallerSelectedOwnerV1::Caller(caller) = actual(c) else {
        panic!("caller delay")
    };
    caller.next_timer().unwrap()
}
fn start_delay(c: &mut SourceMainNativeDurableSelectedCallerV1) {
    action(c, Action::AdvanceCaller);
    let id = timer(c);
    action(
        c,
        Action::FireTimer {
            lane: Lane::CallerDelay,
            id,
            generation: 71,
        },
    );
}
fn first_native(
    c: &mut SourceMainNativeDurableSelectedCallerV1,
) -> NativeMainRetainedConsensusPendingV1 {
    for _ in 0..128 {
        let ops = c.pending_operations();
        if let Some(p) = ops.iter().find(|p| p.value().get("context") != V::Null) {
            return p.clone();
        }
        if let Some(p) = ops.iter().find(|p| p.value().get("role") == s("Detached")) {
            action(
                c,
                Action::AdvanceDetached {
                    selector: p.value(),
                },
            );
        } else {
            action(c, Action::AdvanceCaller);
        }
    }
    panic!("bounded actual caller native await")
}
fn append_resolve(
    c: &mut SourceMainNativeDurableSelectedCallerV1,
    p: &NativeMainRetainedConsensusPendingV1,
) {
    action(
        c,
        Action::AppendSelected {
            selector: p.value(),
        },
    );
    action(
        c,
        Action::ResolveSelectedPost {
            selector: p.value(),
        },
    );
}
fn consensus_ready(c: &mut SourceMainNativeDurableSelectedCallerV1) {
    for _ in 0..128 {
        if caller_pc(c) == s("Consensus") {
            return;
        }
        if let Some(p) = c
            .pending_operations()
            .into_iter()
            .find(|p| p.value().get("context") != V::Null)
        {
            append_resolve(c, &p);
        } else {
            action(c, Action::AdvanceCaller);
        }
    }
    panic!("bounded actual initializer handoff")
}
fn fail_native(
    c: &mut SourceMainNativeDurableSelectedCallerV1,
    p: &NativeMainRetainedConsensusPendingV1,
    post: bool,
) {
    let index = &mut c.owner.as_mut().unwrap().session_mut().index;
    if post {
        index.test_fail_after_sync();
    } else {
        index.test_fail_after_partial_write();
    }
    c.begin(Action::AppendSelected {
        selector: p.value(),
    })
    .unwrap();
    assert!(matches!(
        c.resume(),
        Err(RetainedCallerSelectedWalErrorV1::NativeRecoveryRequired)
    ));
    assert!(c.pending.as_ref().unwrap().executed);
    assert!(matches!(
        c.begin(Action::Pause),
        Err(RetainedCallerSelectedWalErrorV1::Busy)
    ));
}

#[test]
fn selected_raw_intent_before_and_post_recovery_dispatches_the_actual_action_once() {
    for fault in [TestFault::BeforeCommit, TestFault::AfterCommit] {
        let temp = Temp::new();
        let mut c = controller(&temp);
        assert_eq!(c.history().len(), 1);
        assert_eq!(
            c.origin().get("profile"),
            s(RETAINED_CALLER_SELECTED_WAL_PROFILE)
        );
        assert_eq!(c.origin().get("admittedActions"), s("SelectedActions"));
        let prefix = effects(&c).to_vec();
        let before = index(&c).snapshot().unwrap();
        c.begin(Action::AdvanceCaller).unwrap();
        c.barrier_fault = Some((RetainedCallerSelectedWalPhaseV1::Intent, fault));
        assert!(c.resume().is_err());
        assert!(!c.pending.as_ref().unwrap().executed);
        assert_eq!(effects(&c), prefix);
        assert_eq!(index(&c).snapshot().unwrap(), before);
        assert_eq!(
            c.recover_wal().unwrap(),
            if fault == TestFault::BeforeCommit {
                Resolved::Before
            } else {
                Resolved::Post
            }
        );
        c.resume().unwrap();
        assert_eq!(c.history().len(), 2);
        assert_eq!(caller_pc(&c), s("Delay"));
        assert!(timer(&c) > 0);
        let entry = c.read_acknowledged_entry(&c.history()[1]).unwrap();
        assert_eq!(
            crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&entry).unwrap())
                .unwrap()
                .get("previousHead"),
            s(&hex(&c.history()[0].head))
        );
        assert_eq!(index(&c).snapshot().unwrap(), before);
    }
}
#[test]
fn selected_accepted_and_ack_lost_return_keep_same_timer_frame_effects_and_native_bytes() {
    for phase in [
        RetainedCallerSelectedWalPhaseV1::Accepted,
        RetainedCallerSelectedWalPhaseV1::Ack,
    ] {
        let temp = Temp::new();
        let mut c = controller(&temp);
        c.begin(Action::AdvanceCaller).unwrap();
        c.barrier_fault = Some((phase, TestFault::AfterCommit));
        assert!(c.resume().is_err());
        assert!(c.pending.as_ref().unwrap().executed);
        let prefix = effects(&c).to_vec();
        let before = index(&c).snapshot().unwrap();
        let id = timer(&c);
        assert_eq!(c.history().len(), 1);
        assert_eq!(c.recover_wal().unwrap(), Resolved::Post);
        c.resume().unwrap();
        assert_eq!(timer(&c), id);
        assert_eq!(effects(&c), prefix);
        assert_eq!(index(&c).snapshot().unwrap(), before);
        assert_eq!(c.history().len(), 2);
        assert_eq!(c.phase(), RetainedCallerSelectedWalPhaseV1::Idle);
    }
}
#[test]
fn selected_origin_route_refuses_another_real_owners_equal_logical_operation_before_intent() {
    let a = Temp::new();
    let b = Temp::new();
    let mut ca = controller(&a);
    let mut cb = controller(&b);
    start_delay(&mut ca);
    start_delay(&mut cb);
    let pa = first_native(&mut ca);
    let pb = first_native(&mut cb);
    assert_eq!(pa.value().get("caller"), pb.value().get("caller"));
    assert_ne!(pa.value().get("route"), pb.value().get("route"));
    let before = index(&cb).snapshot().unwrap();
    let prefix = effects(&cb).to_vec();
    let wal = cb.wal_pin().unwrap();
    assert!(matches!(
        cb.begin(Action::AppendSelected {
            selector: pa.value()
        }),
        Err(RetainedCallerSelectedWalErrorV1::WrongSelector)
    ));
    assert_eq!(cb.wal_pin().unwrap(), wal);
    assert_eq!(index(&cb).snapshot().unwrap(), before);
    assert_eq!(effects(&cb), prefix);
    let mut wrong_role = pb.value();
    wrong_role.insert("role", s("Detached")).unwrap();
    assert!(matches!(
        cb.begin(Action::AppendSelected {
            selector: wrong_role
        }),
        Err(RetainedCallerSelectedWalErrorV1::WrongSelector)
    ));
    assert_eq!(cb.pending_operations(), vec![pb]);
}
#[test]
fn selected_inline_lost_native_return_reopens_exact_post_without_source_or_append_retry() {
    let temp = Temp::new();
    let mut c = controller(&temp);
    start_delay(&mut c);
    let p = first_native(&mut c);
    let prefix = effects(&c).to_vec();
    let financial = state(&c);
    let route = c.route.as_ref().unwrap().clone();
    fail_native(&mut c, &p, true);
    let no_index = c.into_no_native_index().unwrap();
    assert_eq!(no_index.effects(), prefix);
    let no_index = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::RejectIncompleteTail,
        )
        .err()
        .expect("wrong candidate")
        .into_owner();
    let mut c = no_index
        .try_reopen_post(&temp.root, Recovery::RejectIncompleteTail)
        .unwrap();
    assert_eq!(c.phase(), RetainedCallerSelectedWalPhaseV1::Observe);
    let post = index(&c).snapshot().unwrap();
    c.resume().unwrap();
    assert_eq!(state(&c), financial);
    assert_eq!(effects(&c), prefix);
    assert_eq!(c.route.as_ref(), Some(&route));
    assert_eq!(c.acknowledged_append, Some(p.value()));
    assert_eq!(index(&c).snapshot().unwrap(), post);
    action(
        &mut c,
        Action::ResolveSelectedPost {
            selector: p.value(),
        },
    );
    assert_eq!(index(&c).snapshot().unwrap(), post);
    assert!(c.acknowledged_append.is_none());
    let second = first_native(&mut c);
    assert_ne!(second.value(), p.value());
    append_resolve(&mut c, &second);
}
#[test]
fn selected_inline_before_supersession_is_durable_before_native_only_retry_and_next_write() {
    let temp = Temp::new();
    let mut c = controller(&temp);
    start_delay(&mut c);
    let p = first_native(&mut c);
    let prefix = effects(&c).to_vec();
    let financial = state(&c);
    fail_native(&mut c, &p, false);
    let no_index = c.into_no_native_index().unwrap();
    let mut c = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    assert_eq!(
        c.phase(),
        RetainedCallerSelectedWalPhaseV1::NativeSupersessionIntent
    );
    let before = index(&c).snapshot().unwrap();
    c.barrier_fault = Some((
        RetainedCallerSelectedWalPhaseV1::NativeSupersessionIntent,
        TestFault::AfterCommit,
    ));
    assert!(c.resume().is_err());
    assert_eq!(index(&c).snapshot().unwrap(), before);
    assert_eq!(effects(&c), prefix);
    assert!(c.pending.as_ref().unwrap().replacement.is_some());
    assert_eq!(c.recover_wal().unwrap(), Resolved::Post);
    c.resume().unwrap();
    assert_eq!(state(&c), financial);
    assert_eq!(effects(&c), prefix);
    let replacement = c.pending_operations().into_iter().next().unwrap();
    assert_eq!(replacement.value().get("caller"), p.value().get("caller"));
    assert_eq!(c.acknowledged_append, Some(replacement.value()));
    action(
        &mut c,
        Action::ResolveSelectedPost {
            selector: replacement.value(),
        },
    );
    let second = first_native(&mut c);
    assert_ne!(second.value().get("caller"), p.value().get("caller"));
    append_resolve(&mut c, &second);
    assert_eq!(c.phase(), RetainedCallerSelectedWalPhaseV1::Idle);
    assert!(c.pending.is_none());
    assert!(c.acknowledged_append.is_none());
}
#[test]
fn selected_real_detached_final_save_stays_separate_through_initialize_fulfillment() {
    let temp = Temp::new();
    let mut c = controller(&temp);
    start_delay(&mut c);
    consensus_ready(&mut c);
    let detached = c.pending_operations().into_iter().next().unwrap();
    assert_eq!(detached.value().get("role"), s("Detached"));
    action(
        &mut c,
        Action::AdvanceDetached {
            selector: detached.value(),
        },
    );
    let save = c.pending_operations().into_iter().next().unwrap();
    assert_ne!(save.value().get("context"), V::Null);
    let before = index(&c).snapshot().unwrap();
    action(&mut c, Action::EnterConsensus);
    action(&mut c, Action::AdvanceConsensus { limit: 4096 });
    let RetainedCallerSelectedOwnerV1::Consensus(consensus) = actual(&c) else {
        panic!("same consensus owner")
    };
    assert_eq!(
        consensus.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(index(&c).snapshot().unwrap(), before);
    assert!(c.pending_operations().contains(&save));
    append_resolve(&mut c, &save);
    let second = c.pending_operations().into_iter().next().unwrap();
    action(
        &mut c,
        Action::AdvanceDetached {
            selector: second.value(),
        },
    );
    let second = c
        .pending_operations()
        .into_iter()
        .find(|p| p.value().get("context") != V::Null)
        .unwrap();
    assert_ne!(
        second.value().get("operationOrdinal"),
        save.value().get("operationOrdinal")
    );
    append_resolve(&mut c, &second);
    assert!(
        matches!(actual(&c), RetainedCallerSelectedOwnerV1::Consensus(_)),
        "caller fulfillment never releases its old head/session"
    );
}
#[test]
fn selected_service_validity_post_recovery_retains_the_real_financial_prefix() {
    let temp = Temp::new();
    let (owner, docs) = setup_financial_row(&temp, "sync-final-first/fulfill");
    let mut c = wrap(
        &temp,
        owner,
        docs,
        Start::SyncIndex {
            max: V::number(11.),
        },
    );
    action(&mut c, Action::AdvanceCaller);
    action(&mut c, Action::EnterConsensus);
    action(&mut c, Action::AdvanceConsensus { limit: 4096 });
    let p = c
        .pending_operations()
        .into_iter()
        .find(|p| p.kind() == NativeMainRetainedConsensusOperationKindV1::ServiceValidity)
        .unwrap();
    assert!(effects(&c)
        .iter()
        .any(|e| e.get("collection") == s("tallyMap") && e.get("method") == s("updateAsync")));
    let prefix = effects(&c).to_vec();
    let financial = state(&c);
    fail_native(&mut c, &p, true);
    let no_index = c.into_no_native_index().unwrap();
    let mut c = no_index
        .try_reopen_post(&temp.root, Recovery::RejectIncompleteTail)
        .unwrap();
    c.resume().unwrap();
    assert_eq!(state(&c), financial);
    assert_eq!(effects(&c), prefix);
    assert_eq!(c.pending_operations(), vec![p.clone()]);
    action(
        &mut c,
        Action::ResolveSelectedPost {
            selector: p.value(),
        },
    );
    action(&mut c, Action::AdvanceConsensus { limit: 4096 });
    let RetainedCallerSelectedOwnerV1::Consensus(consensus) = actual(&c) else {
        panic!("actual consensus")
    };
    assert_eq!(
        consensus.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(effects(&c).starts_with(&prefix));
}
#[test]
fn selected_service_validity_before_uses_actual_supersession_without_repeating_main_logic() {
    let temp = Temp::new();
    let (owner, docs) = setup_financial_row(&temp, "sync-final-first/fulfill");
    let mut c = wrap(
        &temp,
        owner,
        docs,
        Start::SyncIndex {
            max: V::number(11.),
        },
    );
    action(&mut c, Action::AdvanceCaller);
    action(&mut c, Action::EnterConsensus);
    action(&mut c, Action::AdvanceConsensus { limit: 4096 });
    let p = c
        .pending_operations()
        .into_iter()
        .find(|p| p.kind() == NativeMainRetainedConsensusOperationKindV1::ServiceValidity)
        .unwrap();
    let prefix = effects(&c).to_vec();
    let financial = state(&c);
    fail_native(&mut c, &p, false);
    let no_index = c.into_no_native_index().unwrap();
    let mut c = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    c.barrier_fault = Some((
        RetainedCallerSelectedWalPhaseV1::NativeSupersessionIntent,
        TestFault::BeforeCommit,
    ));
    assert!(c.resume().is_err());
    assert_eq!(effects(&c), prefix);
    assert_eq!(state(&c), financial);
    assert_eq!(c.recover_wal().unwrap(), Resolved::Before);
    c.resume().unwrap();
    assert_eq!(effects(&c), prefix);
    assert_eq!(state(&c), financial);
    let replacement = c.pending_operations().into_iter().next().unwrap();
    assert_eq!(replacement.kind(), p.kind());
    assert_eq!(
        replacement.value().get("sourceJob"),
        p.value().get("sourceJob")
    );
    assert_eq!(
        replacement.value().get("operationOrdinal"),
        p.value().get("operationOrdinal")
    );
    action(
        &mut c,
        Action::ResolveSelectedPost {
            selector: replacement.value(),
        },
    );
    action(&mut c, Action::AdvanceConsensus { limit: 4096 });
    let RetainedCallerSelectedOwnerV1::Consensus(consensus) = actual(&c) else {
        panic!("actual consensus")
    };
    assert_eq!(
        consensus.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
}
#[test]
fn selected_compacting_inline_retains_actual_generation_before_intermediate_publication() {
    let temp = Temp::new();
    let (owner, docs) = setup_compacting(&temp, "initialize-final-first/fulfill");
    let mut c = wrap(&temp, owner, docs, Start::Initialize);
    start_delay(&mut c);
    let p = first_native(&mut c);
    assert!(
        actual(&c)
            .native(&p)
            .unwrap()
            .append()
            .unwrap()
            .intermediate()
            .is_some(),
        "actual selected native write must compact"
    );
    let prefix = effects(&c).to_vec();
    let financial = state(&c);
    let before = index(&c).capture_verified_history_pin().unwrap();
    c.owner
        .as_mut()
        .unwrap()
        .session_mut()
        .index
        .native_main_test_compaction_fault("after-current-sync");
    c.begin(Action::AppendSelected {
        selector: p.value(),
    })
    .unwrap();
    c.barrier_fault = Some((
        RetainedCallerSelectedWalPhaseV1::EscrowBoundIntent,
        TestFault::AfterCommit,
    ));
    assert!(c.resume().is_err());
    assert!(!c.pending.as_ref().unwrap().executed);
    assert!(c.pending.as_ref().unwrap().escrow_permit.is_some());
    assert!(c.pending.as_ref().unwrap().escrow_chain.is_some());
    assert_eq!(index(&c).capture_verified_history_pin().unwrap(), before);
    assert_eq!(effects(&c), prefix);
    assert_eq!(c.recover_wal().unwrap(), Resolved::Post);
    assert!(matches!(
        c.resume(),
        Err(RetainedCallerSelectedWalErrorV1::NativeRecoveryRequired)
    ));
    let no_index = c.into_no_native_index().unwrap();
    let mut c = no_index
        .try_reopen(
            &temp.root,
            Candidate::Intermediate,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    c.resume().unwrap();
    assert_eq!(state(&c), financial);
    assert_eq!(effects(&c), prefix);
    let entry = c
        .read_acknowledged_entry(c.history().last().unwrap())
        .unwrap();
    let entry =
        crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&entry).unwrap())
            .unwrap();
    assert_ne!(entry.get("generationEscrow"), V::Null);
    assert_ne!(entry.get("nativeRecovery"), V::Null);
    let replacement = c.pending_operations().into_iter().next().unwrap();
    action(
        &mut c,
        Action::ResolveSelectedPost {
            selector: replacement.value(),
        },
    );
}

#[path = "independent_selected_core_tests.rs"]
mod independent_selected_recovery_review;
