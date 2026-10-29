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
            "actual-inline-service-{}-{}",
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
include!("source_main_native_service_test_transport.rs");
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
    for row in arr(untag(raw_get(input, "nativeInput"))) {
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
fn start(temp: &Temp, mode: &str) -> SourceMainNativeActiveServiceV1 {
    let (owner, docs) = setup(temp, mode);
    let expected = owner.expectation().unwrap();
    let service = owner
        .into_inline_active_service(&expected, V::number(10.), 71, docs, 64 * 1024 * 1024)
        .unwrap();
    assert!(!service.observation_pending());
    assert_eq!(
        service
            .financial_runtime()
            .native_main_service_client_scope_value(),
        V::Null
    );
    let original_init = untag(raw_get(record(mode, "input"), "rawTxUtilsInitialization"));
    assert_eq!(
        original_init.get("sourceSHA256"),
        s("38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3")
    );
    assert_eq!(
        V::Array(service.bootstrap_effects().to_vec()),
        original_init.get("effects"),
        "actual same-index module initialization is independently source-observed and retained"
    );
    service
}
fn apply(
    service: &mut SourceMainNativeActiveServiceV1,
    action: NativeMainActiveServiceActionV1,
) -> NativeMainActiveServiceActionResultV1 {
    let expected = service.expectation().unwrap();
    let result = service.apply(&expected, action).unwrap().result.clone();
    assert_eq!(
        service
            .financial_runtime()
            .native_main_service_client_scope_value(),
        V::Null,
        "client capability never escapes a running step/park/terminal boundary"
    );
    result
}
fn report_service_boundary(
    service: &SourceMainNativeActiveServiceV1,
    stop: &NativeMainServiceStopV1,
) {
    if !matches!(
        stop,
        NativeMainServiceStopV1::Quiescent
            | NativeMainServiceStopV1::SourceStopped(_)
            | NativeMainServiceStopV1::KernelStopped(_)
    ) {
        return;
    }
    eprintln!(
        "SERVICE_BOUNDARY stop={stop:?} caller={:?} kernel={:?}",
        service.caller_result(),
        service.kernel_value()
    );
    for effect in service.effects().iter().filter(|effect| matches!(effect.get("kind"),V::String(ref kind) if ["rpc-input", "rpc-data", "rpc-clock", "unhandled-rejection"].contains(&kind.as_str()))) {
        eprintln!("SERVICE_BOUNDARY_ACTUAL {effect:?}");
    }
    for effect in service.effects().iter().rev().take(12).rev() {
        eprintln!("SERVICE_BOUNDARY_SUFFIX {effect:?}");
    }
}
fn drive(
    service: &mut SourceMainNativeActiveServiceV1,
    stop_at_live: bool,
) -> NativeMainServiceStopV1 {
    for _ in 0..24 {
        let NativeMainActiveServiceActionResultV1::Stop(stop) = apply(
            service,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        ) else {
            panic!("advance")
        };
        match stop {
            NativeMainServiceStopV1::AwaitNativeWrite => {
                if stop_at_live
                    && service.caller_result().is_some()
                    && service.pending_inline_write().is_none()
                {
                    report_service_boundary(service, &stop);
                    return stop;
                }
                assert_eq!(
                    apply(service, NativeMainActiveServiceActionV1::AppendPending),
                    NativeMainActiveServiceActionResultV1::Stop(
                        NativeMainServiceStopV1::AwaitNativeWrite
                    )
                );
                let result = apply(service, NativeMainActiveServiceActionV1::ResolvePost);
                if let NativeMainActiveServiceActionResultV1::Stop(stop) = result {
                    if matches!(
                        stop,
                        NativeMainServiceStopV1::PendingTimer | NativeMainServiceStopV1::Quiescent
                    ) {
                        report_service_boundary(service, &stop);
                        return stop;
                    }
                }
            }
            NativeMainServiceStopV1::DrainBound => (),
            stop => {
                report_service_boundary(service, &stop);
                return stop;
            }
        }
    }
    panic!("bounded service scene failed to reach original source boundary")
}
fn source_tables(record: &str) -> V {
    untag(tagged_field(
        tagged_field(raw_get(record, "sourceState"), "source"),
        "tables",
    ))
}
fn chosen_effects(effects: Vec<V>, native: bool) -> Vec<V> {
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
                && phases.iter().any(|p| effect.get("target") == s(p))
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
fn compare(service: &SourceMainNativeActiveServiceV1, mode: &str, point: &str) {
    let original = record(mode, point);
    assert_eq!(
        service
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("tables"),
        source_tables(original),
        "all original financial table inventories and rows at {mode}/{point}"
    );
    let mut actual_rows = vec![];
    service
        .index()
        .visit_rows(|row| {
            actual_rows.push(row.clone());
            Ok(true)
        })
        .unwrap();
    assert_eq!(
        V::Array(actual_rows),
        untag(raw_get(original, "nativeRows")),
        "complete real native metadata at {mode}/{point}"
    );
    assert_eq!(chosen_effects(service.invocation_effects().to_vec(),true),chosen_effects(tagged_items(raw_get(original,"effects")).into_iter().map(untag).collect(),false),"ordered financial DB / RPC / timers / real phase calls at {mode}/{point}; native query receipts and source-only instrumentation remain separately scoped");
    assert_eq!(
        service.caller_result(),
        Some(&Ok(DependencyReturn::Pending(V::Undefined)))
    );
}
#[test]
fn source_empty_live_block_then_same_wait_and_pause() {
    let temp = Temp::new();
    let mode = "live-iteration-pause-boundary";
    let mut service = start(&temp, mode);
    assert_eq!(
        drive(&mut service, false),
        NativeMainServiceStopV1::PendingTimer
    );
    compare(
        &service,
        mode,
        "catchup-returned-live-empty-block11-timer-pending",
    );
    assert_eq!(
        apply(&mut service, NativeMainActiveServiceActionV1::Pause),
        NativeMainActiveServiceActionResultV1::Pause(Ok(DependencyReturn::Immediate(V::Bool(
            true
        ))))
    );
    compare(&service, mode, "pause-with-original-10s-wait-still-pending");
    let id = service.next_timer().unwrap();
    apply(
        &mut service,
        NativeMainActiveServiceActionV1::FireTimer { id, generation: 71 },
    );
    assert_eq!(
        drive(&mut service, false),
        NativeMainServiceStopV1::PendingTimer
    );
    compare(
        &service,
        mode,
        "same-live-wait-fired-now-original-recursive-sync-paused",
    );
}
#[test]
fn detached_live_failure_never_rejects_completed_outer_catchup() {
    let temp = Temp::new();
    let mode = "detached-live-rpc-failure";
    let mut service = start(&temp, mode);
    assert_eq!(
        drive(&mut service, false),
        NativeMainServiceStopV1::Quiescent
    );
    assert_eq!(service.next_timer(), None);
    compare(
        &service,
        mode,
        "catchup-returned-detached-live-await-rejected",
    );
    assert!(
        service
            .effects()
            .iter()
            .any(|e| e.get("kind") == s("unhandled-rejection")),
        "actual discarded incoming rejection is retained"
    );
}
#[test]
fn source_live_financial_prefix_pause_same_native_write_and_return() {
    let temp = Temp::new();
    let mode = "same-held-live-validity";
    let mut service = start(&temp, mode);
    assert_eq!(
        drive(&mut service, true),
        NativeMainServiceStopV1::AwaitNativeWrite
    );
    compare(
        &service,
        mode,
        "catchup-returned-live-original-validity-promise-held",
    );
    let held = service.pending_write().unwrap().durable_read_context();
    let prefix = service.effects().to_vec();
    apply(&mut service, NativeMainActiveServiceActionV1::Pause);
    assert_eq!(
        service.pending_write().unwrap().durable_read_context(),
        held
    );
    compare(
        &service,
        mode,
        "pause-does-not-resolve-original-live-validity",
    );
    assert_eq!(
        apply(
            &mut service,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 }
        ),
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::AwaitNativeWrite)
    );
    assert_eq!(service.effects()[..prefix.len()], prefix);
    assert_eq!(
        apply(&mut service, NativeMainActiveServiceActionV1::AppendPending),
        NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::AwaitNativeWrite)
    );
    let expected = service.expectation().unwrap();
    assert_eq!(
        service
            .apply(&expected, NativeMainActiveServiceActionV1::AppendPending)
            .unwrap_err(),
        NativeMainActiveServiceErrorV1::WriteAlreadyAttempted
    );
    apply(&mut service, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(
        drive(&mut service, false),
        NativeMainServiceStopV1::PendingTimer
    );
    compare(
        &service,
        mode,
        "same-held-live-promise-resumed-service-now-pause-wait",
    );
}
#[test]
fn failed_observation_retries_only_observation_and_legacy_head_does_not_escape() {
    let temp = Temp::new();
    let (owner, docs) = setup(&temp, "detached-live-rpc-failure");
    let expected = owner.expectation().unwrap();
    let mut service = owner
        .into_inline_active_service(&expected, V::number(10.), 71, docs, 1)
        .unwrap();
    assert!(service.observation_pending());
    let before = service.effects().to_vec();
    assert!(service.retry_observation().is_err());
    assert_eq!(service.effects(), before);
    assert_eq!(
        service.expectation(),
        Err(NativeMainActiveServiceErrorV1::ObservationPending)
    );
    assert_eq!(
        service.owner.head(),
        expected.session,
        "frozen legacy head stays private; new session has no completed-owner release API"
    );
}

#[test]
fn literal_shutdown_keeps_live_await_unresolved_and_forbids_native_write() {
    let temp = Temp::new();
    let mut service = start(&temp, "same-held-live-validity");
    assert_eq!(
        drive(&mut service, true),
        NativeMainServiceStopV1::AwaitNativeWrite
    );
    let actual = service.index().snapshot().unwrap();
    let write = service.pending_write().unwrap().durable_read_context();
    let before = service.effects().len();
    assert_eq!(
        apply(
            &mut service,
            NativeMainActiveServiceActionV1::Shutdown(vec![])
        ),
        NativeMainActiveServiceActionResultV1::Control(Ok(()))
    );
    assert_eq!(service.index().snapshot().unwrap(), actual);
    assert_eq!(
        service.pending_write().unwrap().durable_read_context(),
        write
    );
    let effects = &service.effects()[before..];
    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0].get("kind"), s("console"));
    assert_eq!(
        effects[0].get("values"),
        V::Array(vec![s("Saving state to database...")])
    );
    assert_eq!(
        effects[1].get("values"),
        V::Array(vec![s("Shutdown completed.")])
    );
    assert_eq!(
        effects[2],
        obj(&[("kind", s("process-exit")), ("code", V::number(0.))])
    );
    let expected = service.expectation().unwrap();
    for action in [
        NativeMainActiveServiceActionV1::AppendPending,
        NativeMainActiveServiceActionV1::ResolvePost,
        NativeMainActiveServiceActionV1::Advance { limit: 1 },
        NativeMainActiveServiceActionV1::Pause,
    ] {
        assert_eq!(
            service.apply(&expected, action).unwrap_err(),
            NativeMainActiveServiceErrorV1::AlreadyExited(0)
        );
    }
    assert_eq!(service.index().snapshot().unwrap(), actual);
}

fn first_live_inline_await(service: &mut SourceMainNativeActiveServiceV1) {
    for _ in 0..24 {
        if service.pending_inline_write().is_some() {
            return;
        }
        let result = apply(
            service,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        match result {
            NativeMainActiveServiceActionResultV1::Stop(
                NativeMainServiceStopV1::AwaitNativeWrite,
            ) => {
                if service.pending_inline_write().is_some() {
                    return;
                }
                apply(service, NativeMainActiveServiceActionV1::AppendPending);
                apply(service, NativeMainActiveServiceActionV1::ResolvePost);
            }
            NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::DrainBound) => (),
            result => panic!("genuine held-inline scene stopped early: {result:?}"),
        }
    }
    panic!("no actual inline source await")
}
#[test]
fn actual_inline_transaction_and_two_height_awaits_keep_same_frame_and_source_pending() {
    let temp = Temp::new();
    let mut service = start(&temp, "same-held-live-validity");
    assert_eq!(
        service.profile(),
        crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE
    );
    first_live_inline_await(&mut service);
    assert!(
        service.caller_result().is_some(),
        "outer catchup really completed before incoming source block"
    );
    let write = service.pending_inline_write().unwrap();
    let query = write.arguments()[0].get("_id");
    let V::String(id) = query.clone() else {
        panic!("actual selected source key")
    };
    assert!(id.starts_with("tx-11-"));
    assert!(service.index().get(&id).unwrap().is_none());
    let old_height = service.index().get("MaxHeight").unwrap();
    let old_flag = service.index().get("indexExists").unwrap();
    let financial = service.financial_runtime().state_value().unwrap();
    let rpc_before = service
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("rpc-data"))
        .count();
    apply(&mut service, NativeMainActiveServiceActionV1::AppendPending);
    assert!(service.index().get(&id).unwrap().is_some());
    assert_eq!(service.index().get("MaxHeight").unwrap(), old_height);
    assert_eq!(service.index().get("indexExists").unwrap(), old_flag);
    assert_eq!(
        service.financial_runtime().state_value().unwrap(),
        financial,
        "actual native bytes do not settle the source Promise"
    );
    assert!(service
        .apply(
            &service.expectation().unwrap(),
            NativeMainActiveServiceActionV1::AppendPending
        )
        .is_err());
    apply(&mut service, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(
        service.pending_inline_write().unwrap().arguments()[0].get("_id"),
        s("MaxHeight")
    );
    apply(&mut service, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(
        service
            .index()
            .get("MaxHeight")
            .unwrap()
            .unwrap()
            .get("value"),
        V::number(11.)
    );
    assert_eq!(service.index().get("indexExists").unwrap(), old_flag);
    apply(&mut service, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(
        service.pending_inline_write().unwrap().arguments()[0].get("_id"),
        s("indexExists")
    );
    apply(&mut service, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(
        service
            .index()
            .get("indexExists")
            .unwrap()
            .unwrap()
            .get("value"),
        V::Bool(true)
    );
    assert_eq!(
        service
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("rpc-data"))
            .count(),
        rpc_before,
        "same source decode/sender prefix is not replayed on any native await"
    );
    apply(&mut service, NativeMainActiveServiceActionV1::ResolvePost);
    assert!(service.pending_inline_write().is_none());
    assert!(
        service.pending_write().is_some(),
        "same incoming block advances into its real financial validity await"
    );
}
#[test]
fn actual_shutdown_at_inline_source_await_never_applies_held_native_plan() {
    let temp = Temp::new();
    let mut service = start(&temp, "same-held-live-validity");
    first_live_inline_await(&mut service);
    let before = service.index().capture_verified_history_pin().unwrap();
    let context = service.pending_native_context().unwrap();
    apply(
        &mut service,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    assert_eq!(service.pending_native_context(), Some(context));
    assert_eq!(
        service.index().capture_verified_history_pin().unwrap(),
        before
    );
    let expected = service.expectation().unwrap();
    assert!(matches!(
        service
            .apply(&expected, NativeMainActiveServiceActionV1::AppendPending)
            .err(),
        Some(NativeMainActiveServiceErrorV1::AlreadyExited(_))
    ));
    assert_eq!(
        service.index().capture_verified_history_pin().unwrap(),
        before
    );
}
