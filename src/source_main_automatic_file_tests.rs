// Parent must compile these against the actual Core. Only genuine raw reset,
// constructor catalog, RPC documents and bootstrap calls initialize the owner.
// Complete original observations are comparison-only, never imported.
use super::*;
use crate::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O,
};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::source_main_step_kernel::{MainStepCommandV1 as Command, MainStepSourceResult};
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
include!("source_main_automatic_file_test_transport.rs");
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
            "actual-automatic-file-{}-{}",
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
fn source(mode: &str, point: &str) -> &'static str {
    let fixture = match mode {
        "financial" => include_str!("../fixtures/automatic_financial_source.jsonl"),
        "parallel" => include_str!("../fixtures/automatic_parallel_file_source.jsonl"),
        "empty" => include_str!("../fixtures/automatic_empty_service_source.jsonl"),
        "exhausted" => include_str!("../fixtures/automatic_exhausted_service_source.jsonl"),
        "restore" => include_str!("../fixtures/automatic_restore_service_source.jsonl"),
        _ => panic!("source mode"),
    };
    fixture
        .lines()
        .find(|line| json_string(raw_get(line, "point")) == point)
        .unwrap()
}
fn list(value: V) -> Vec<V> {
    let V::Array(values) = value else {
        panic!("raw array")
    };
    values
}
fn invoke(financial: &mut SourceContractReplay, target: &str, arguments: Vec<V>) {
    let receipt = financial
        .apply(C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        }))
        .unwrap();
    assert!(
        !matches!(receipt.outcome, O::LocalError(_)),
        "{target}: {:?}",
        receipt.outcome
    );
}
fn setup(temp: &Temp, mode: &str) -> (SourceMainNativeSessionV13, V) {
    let input = source(mode, "input");
    let reset = untag(raw_get(input, "rawReset"));
    assert_eq!(reset.get("empty"), V::Bool(true));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let constructor = raw_get(input, "rawHarnessConstructorInput");
    assert_eq!(
        untag(tagged_field(constructor, "profile")),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    for raw in tagged_items(tagged_field(constructor, "collections")) {
        let V::String(collection) = untag(tagged_field(raw, "collection")) else {
            panic!("raw constructor collection")
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
            ("fileInputs", reset.get("fileInputs")),
            ("fileIOInputs", reset.get("fileIOInputs")),
            (
                "directoryInputs",
                if reset.get("directoryInputs") == V::Undefined {
                    V::Array(vec![])
                } else {
                    reset.get("directoryInputs")
                },
            ),
        ])],
    );
    if let V::Array(rows) = reset.get("persistenceRows") {
        financial
            .apply(C::Import {
                collection: "persistence".into(),
                rows,
            })
            .unwrap();
    }
    financial
        .apply(C::SetHeight(reset.get("clientProfile").get("height")))
        .unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual automatic native file source",
    )
    .unwrap();
    for row in list(untag(raw_get(input, "nativeInput"))) {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    let mut session = SourceMainNativeSessionV13::adopt(
        NativeMainOwners::adopt(financial, index, &expected).unwrap(),
    )
    .unwrap();
    for command in list(untag(raw_get(input, "rawBootstrapCommands"))) {
        assert_eq!(command.get("target"), s("Main.processTxSet"));
        let arguments = list(command.get("args"));
        let expected = session.expectation().unwrap();
        let receipt = session
            .apply(
                &expected,
                NativeMainSessionCommand::ProcessRows {
                    rows: arguments[0].clone(),
                    height: arguments[1].clone(),
                    live: false,
                },
            )
            .unwrap();
        assert!(
            matches!(receipt.status, NativeMainSessionStatus::Completed(_)),
            "real raw bootstrap refused: {:?}",
            receipt.status
        );
    }
    let raw = untag(raw_get(input, "rawRPCDocuments"));
    let documents = if raw == V::Undefined {
        V::Array(vec![])
    } else {
        V::Array(vec![
            obj(&[
                ("id", raw.get("id")),
                ("raw", raw.get("raw")),
                ("document", raw.get("document")),
            ]),
            obj(&[
                ("id", raw.get("parent").get("txid")),
                ("document", raw.get("parent")),
            ]),
        ])
    };
    (session, documents)
}
fn start(temp: &Temp, mode: &str) -> SourceMainNativeAutomaticFileServiceV1 {
    let (session, documents) = setup(temp, mode);
    let expected = session.expectation().unwrap();
    let c = session
        .into_automatic_file_active_service(
            &expected,
            list(untag(raw_get(source(mode, "input"), "command")).get("args"))[0].clone(),
            91,
            documents,
            64 * 1024 * 1024,
        )
        .unwrap();
    assert!(!c.active.observation_pending());
    assert!(c.active.source_start_error().is_none());
    c
}
fn apply(
    c: &mut SourceMainNativeAutomaticFileServiceV1,
    action: NativeMainActiveServiceActionV1,
) -> NativeMainActiveServiceActionResultV1 {
    c.apply(action).unwrap().result.clone()
}
fn advance(
    c: &mut SourceMainNativeAutomaticFileServiceV1,
) -> crate::source_contract_runtime::NativeMainServiceStopV1 {
    let NativeMainActiveServiceActionResultV1::Stop(stop) =
        apply(c, NativeMainActiveServiceActionV1::Advance { limit: 4096 })
    else {
        panic!("actual advance")
    };
    stop
}
fn drive(
    c: &mut SourceMainNativeAutomaticFileServiceV1,
    financial_await: bool,
) -> crate::source_contract_runtime::NativeMainServiceStopV1 {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    for _ in 0..64 {
        let stop = advance(c);
        if stop == Stop::AwaitNativeWrite {
            if financial_await && c.pending_write().is_some() {
                return stop;
            }
            assert_eq!(
                apply(c, NativeMainActiveServiceActionV1::AppendPending),
                NativeMainActiveServiceActionResultV1::Stop(Stop::AwaitNativeWrite)
            );
            apply(c, NativeMainActiveServiceActionV1::ResolvePost);
        } else {
            return stop;
        }
    }
    panic!("bounded automatic source drive")
}
fn file_count(c: &SourceMainNativeAutomaticFileServiceV1, method: &str) -> usize {
    c.effects()
        .iter()
        .filter(|e| e.get("kind") == s("filesystem") && e.get("method") == s(method))
        .count()
}
fn fields(c: &SourceMainNativeAutomaticFileServiceV1) -> Vec<(String, V)> {
    let mut fields = vec![];
    c.visit_canonical_fields(&mut |p, v| {
        fields.push((p.to_owned(), v.clone()));
        Ok(())
    })
    .unwrap();
    fields
}
fn ordered(effects: Vec<V>, native: bool) -> Vec<V> {
    let phases = [
        "Tally.flushQueuedFeeAccruals",
        "Logic.settleLiquidityRewards",
        "Orderbook.processQueuedOnChainOrdersForBlock",
        "Orderbook.processQueuedChannelTradesForBlock",
        "Channels.processWithdrawals",
        "Clearing.clearingFunction",
    ];
    effects
        .into_iter()
        .filter_map(|effect| {
            let kind = effect.get("kind");
            if [s("database"), s("db")].contains(&kind) {
                if effect.get("collection") == s("txIndex") || effect.get("name") == s("txIndex") {
                    return None;
                }
                return Some(effect);
            }
            if [
                s("rpc-input"),
                s("rpc-data"),
                s("rpc-clock"),
                s("filesystem"),
                s("timer-scheduled"),
                s("timer-fired"),
                s("timer-cancelled"),
                s("process-exit"),
            ]
            .contains(&kind)
            {
                return Some(effect);
            }
            if ((native && kind == s("main-phase")) || (!native && kind == s("source-function")))
                && phases
                    .iter()
                    .any(|target| effect.get("target") == s(target))
            {
                return Some(obj(&[
                    ("kind", s("SourcePhaseEntry")),
                    ("target", effect.get("target")),
                    ("args", effect.get("args")),
                ]));
            }
            None
        })
        .collect()
}
fn compare(c: &SourceMainNativeAutomaticFileServiceV1, mode: &str, point: &str) {
    let observed = source(mode, point);
    let state = raw_get(observed, "sourceState");
    assert_eq!(
        c.financial_state().unwrap().get("tables"),
        untag(tagged_field(tagged_field(state, "source"), "tables")),
        "all named financial tables/rows {mode}/{point}"
    );
    let mut rows = vec![];
    c.active
        .owner
        .index
        .visit_rows(|row| {
            rows.push(row.clone());
            Ok(true)
        })
        .unwrap();
    assert_eq!(
        V::Array(rows),
        untag(raw_get(observed, "nativeRows")),
        "all native rows {mode}/{point}"
    );
    let actual = c
        .financial_state()
        .unwrap()
        .get("mainProduction")
        .get("service");
    assert_eq!(
        actual.get("fileIO"),
        untag(tagged_field(state, "fileIO")),
        "all actual file facts/counters {mode}/{point}"
    );
    let files = tagged_field(state, "virtualFS");
    assert_eq!(
        actual.get("files"),
        untag(tagged_field(files, "files")),
        "complete actual file rows/bytes {mode}/{point}"
    );
    assert_eq!(
        actual.get("directories"),
        untag(tagged_field(files, "directories")),
        "complete actual directory order {mode}/{point}"
    );
    assert_eq!(
        ordered(c.active.invocation_effects().to_vec(), true),
        ordered(
            tagged_items(raw_get(observed, "effects"))
                .into_iter()
                .map(untag)
                .collect(),
            false
        ),
        "complete ordered financial DB/RPC/file/timer/phase events {mode}/{point}"
    );
    assert_eq!(
        c.caller_result(),
        Some(&Ok(DependencyReturn::Pending(V::Undefined)))
    );
}
fn pause_height(c: &mut SourceMainNativeAutomaticFileServiceV1, mode: &str) {
    let commands = list(untag(raw_get(
        source(mode, "input"),
        "rawInterleavingCommands",
    )));
    for command in commands {
        let V::String(method) = command.get("target") else {
            panic!("raw target")
        };
        c.enqueue_control(Command::Start {
            method,
            arguments: list(command.get("args")),
        })
        .unwrap();
        assert_eq!(
            advance(c),
            crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
        );
    }
}
fn fire(c: &mut SourceMainNativeAutomaticFileServiceV1) {
    let id = c.next_timer().unwrap();
    assert!(matches!(
        apply(
            c,
            NativeMainActiveServiceActionV1::FireTimer { id, generation: 91 }
        ),
        NativeMainActiveServiceActionResultV1::Control(Ok(()))
    ));
}
#[test]
fn automatic_empty_checkpoint_uses_same_file_child_and_recursive_source_parent() {
    let t = Temp::new();
    let mut c = start(&t, "empty");
    assert_eq!(
        drive(&mut c, false),
        crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
    );
    assert_eq!(file_count(&c, "copyFile"), 2);
    assert!(fields(&c).contains(&(
        "active/frame/automaticFile/prefix/invocation/present".into(),
        V::Bool(true)
    )));
    compare(&c, "empty", "same-file-child-held");
    let timer = c.next_timer();
    let before = c.active.owner.index.snapshot().unwrap();
    let native_factory_before = c
        .effects()
        .iter()
        .filter(|e| {
            e.get("kind") == s("database")
                && e.get("name") == s("txIndex")
                && e.get("method") == s("getDatabase")
        })
        .count();
    pause_height(&mut c, "empty");
    assert_eq!(c.next_timer(), timer);
    compare(&c, "empty", "pause-height-same-file-child");
    fire(&mut c);
    assert_eq!(
        drive(&mut c, false),
        crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
    );
    assert_eq!(file_count(&c, "copyFile"), 3);
    assert_eq!(
        c.active.owner.index.snapshot().unwrap(),
        before,
        "original empty processBlockData performs no native write"
    );
    assert_eq!(c.effects().iter().filter(|e| e.get("kind")==s("database") && e.get("name")==s("txIndex") && e.get("method")==s("getDatabase")).count(),native_factory_before+1,"actual original empty processBlockData still acquires its native database once after files");
    compare(&c, "empty", "same-empty-file-parent-recursive-wait");
    let scheduled = c
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("timer-scheduled"))
        .map(|e| e.get("delay"))
        .collect::<Vec<_>>();
    assert_eq!(scheduled, vec![V::number(50.), V::number(1000.)]);
    assert!(!fields(&c).contains(&(
        "active/frame/automaticFile/prefix/invocation/present".into(),
        V::Bool(true)
    )));
}
#[test]
fn automatic_nonempty_real_financial_prefix_holds_same_native_await_without_replay() {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let t = Temp::new();
    let mut c = start(&t, "financial");
    assert_eq!(drive(&mut c, false), Stop::PendingTimer);
    compare(&c, "financial", "same-file-child-held");
    let invocation_start = fields(&c)
        .into_iter()
        .find(|(p, _)| p == "active/frame/automaticFile/prefix/effectStart")
        .expect("actual original whole invocation start")
        .1;
    pause_height(&mut c, "financial");
    fire(&mut c);
    assert_eq!(drive(&mut c, true), Stop::AwaitNativeWrite);
    assert!(c.pending_inline_write().is_none());
    assert!(c.pending_write().is_some());
    let retained = fields(&c);
    assert!(retained.contains(&(
        "active/frame/block/automaticFileInvocationEffectStart".into(),
        invocation_start.clone()
    )));
    assert_ne!(
        retained
            .iter()
            .find(|(p, _)| p == "active/frame/block/effectStart")
            .expect("actual native tail start")
            .1,
        invocation_start,
        "native tail effects are a distinct suffix; original invocation prefix remains retained"
    );
    assert!(!c
        .active
        .financial_runtime()
        .collection("propertyList")
        .is_empty());
    assert!(!c
        .active
        .financial_runtime()
        .collection("tallyMap")
        .is_empty());
    compare(
        &c,
        "financial",
        "same-native-validity-held-after-real-finance",
    );
    let state = c.financial_state().unwrap();
    let prefix = c.effects().to_vec();
    let before = c.active.owner.index.snapshot().unwrap();
    let plan = c.pending_write().unwrap() as *const _;
    for _ in 0..3 {
        assert_eq!(advance(&mut c), Stop::AwaitNativeWrite);
        assert_eq!(c.pending_write().unwrap() as *const _, plan);
        assert_eq!(c.effects(), prefix);
        assert_eq!(c.financial_state().unwrap(), state);
        assert_eq!(c.active.owner.index.snapshot().unwrap(), before);
    }
    assert_eq!(
        apply(&mut c, NativeMainActiveServiceActionV1::AppendPending),
        NativeMainActiveServiceActionResultV1::Stop(Stop::AwaitNativeWrite)
    );
    let post = c.active.owner.index.snapshot().unwrap();
    assert_eq!(post.sequence, before.sequence + 1);
    assert!(matches!(
        c.apply(NativeMainActiveServiceActionV1::AppendPending),
        Err(NativeMainActiveServiceErrorV1::WriteAlreadyAttempted)
    ));
    assert_eq!(advance(&mut c), Stop::AwaitNativeWrite);
    assert_eq!(c.effects(), prefix);
    assert_eq!(c.financial_state().unwrap(), state);
    assert_eq!(c.active.owner.index.snapshot().unwrap(), post);
    apply(&mut c, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(drive(&mut c, false), Stop::PendingTimer);
    compare(
        &c,
        "financial",
        "same-native-promise-resolved-parent-recursive-wait",
    );
    assert_eq!(file_count(&c, "copyFile"), 3);
    assert!(c.effects().starts_with(&prefix));
}
#[test]
fn automatic_second_copy_rejection_has_all_five_waits_and_no_native_tail() {
    let t = Temp::new();
    let mut c = start(&t, "exhausted");
    assert_eq!(
        drive(&mut c, false),
        crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
    );
    let native = c.active.owner.index.snapshot().unwrap();
    pause_height(&mut c, "exhausted");
    for at in 0..5 {
        fire(&mut c);
        let stop = drive(&mut c, false);
        if at < 4 {
            assert_eq!(
                stop,
                crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
            );
        } else {
            assert_eq!(
                stop,
                crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
            );
        }
    }
    assert_eq!(file_count(&c, "copyFile"), 6);
    assert_eq!(c.active.owner.index.snapshot().unwrap(), native);
    assert!(c.pending_write().is_none());
    assert!(c.pending_inline_write().is_none());
    assert_eq!(
        c.effects()
            .iter()
            .filter(|e| e.get("kind") == s("timer-scheduled") && e.get("delay") == V::number(50.))
            .count(),
        5
    );
    compare(
        &c,
        "exhausted",
        "same-file-child-exhausted-incoming-rejected",
    );
    assert!(c.active.frame.as_ref().unwrap().automatic_file_test_completed().any(|(_,r)|matches!(r,MainStepSourceResult::Thrown(error)if error.exception.message.starts_with("safeCopy failed:"))));
    assert_eq!(
        c.effects()
            .iter()
            .filter(|e| e.get("kind") == s("unhandled-rejection"))
            .count(),
        1
    );
}
#[test]
fn automatic_shutdown_during_partial_checkpoint_keeps_child_unresolved() {
    let t = Temp::new();
    let mut c = start(&t, "empty");
    drive(&mut c, false);
    let timer = c.next_timer().unwrap();
    let rows = c.active.owner.index.snapshot().unwrap();
    let prefix = c.effects().to_vec();
    assert_eq!(
        apply(&mut c, NativeMainActiveServiceActionV1::Shutdown(vec![])),
        NativeMainActiveServiceActionResultV1::Control(Ok(()))
    );
    assert!(c.effects().starts_with(&prefix));
    assert_eq!(c.next_timer(), Some(timer));
    assert_eq!(c.active.owner.index.snapshot().unwrap(), rows);
    assert!(fields(&c).contains(&(
        "active/frame/automaticFile/prefix/invocation/present".into(),
        V::Bool(true)
    )));
    assert!(matches!(
        c.apply(NativeMainActiveServiceActionV1::FireTimer {
            id: timer,
            generation: 91
        }),
        Err(NativeMainActiveServiceErrorV1::AlreadyExited(0))
    ));
    let exited_fields = fields(&c);
    assert!(c
        .enqueue_control(Command::Start {
            method: "Main.saveTrackHeight".into(),
            arguments: vec![V::number(22.)]
        })
        .is_err());
    assert_eq!(
        fields(&c),
        exited_fields,
        "refused control cannot change terminal source/operation metadata"
    );
}

#[test]
fn automatic_restore_returns_real_reorg_child_without_native_or_balance_rollback() {
    let t = Temp::new();
    let mut c = start(&t, "restore");
    assert_eq!(
        drive(&mut c, false),
        crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
    );
    compare(&c, "restore", "same-file-child-held");
    let before = c.active.owner.index.snapshot().unwrap();
    let balances = c.active.financial_runtime().collection("tallyMap").to_vec();
    let prefix = c.effects().to_vec();
    pause_height(&mut c, "restore");
    fire(&mut c);
    assert_eq!(
        drive(&mut c, false),
        crate::source_contract_runtime::NativeMainServiceStopV1::PendingTimer
    );
    compare(&c, "restore", "same-restore-file-parent-recursive-wait");
    assert_eq!(c.active.owner.index.snapshot().unwrap(), before);
    assert_eq!(
        c.active.financial_runtime().collection("tallyMap"),
        balances
    );
    assert!(c.pending_write().is_none());
    assert!(c.pending_inline_write().is_none());
    assert!(c.effects().starts_with(&prefix));
    assert!(c.active.frame.as_ref().unwrap().automatic_file_test_completed().any(|(_,r)|matches!(r,MainStepSourceResult::Fulfilled(value)if value.get("reOrg")==V::Bool(true)&&value.get("restore")==V::number(1.))));
}

include!("source_main_automatic_parallel_file_tests.rs");
