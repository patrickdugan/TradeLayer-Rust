// Actual-Core proposals. Independent source projections are outputs only;
// Runtime receives only explicit raw configuration and caller/IO controls.
use super::*;
use crate::source_contract_replay::{
    SourceContractCommand, SourceContractCommandOutcome, SourceContractReplay,
    SourceContractReplayError,
};
fn invoke(
    replay: &mut SourceContractReplay,
    target: &str,
    input: V,
) -> crate::source_contract_replay::SourceContractReceipt {
    replay
        .apply(SourceContractCommand::Invoke(DependencyCall {
            target: target.into(),
            arguments: vec![input],
        }))
        .unwrap()
}
fn values(value: V) -> Vec<V> {
    let V::Array(v) = value else {
        panic!("array required: {value:?}")
    };
    v
}
fn state(replay: &SourceContractReplay) -> OrderedBootstrap {
    replay.runtime().core.ordered_bootstrap_state().unwrap()
}
fn state_index(replay: &SourceContractReplay) -> V {
    replay
        .runtime()
        .core
        .main_production
        .as_ref()
        .unwrap()
        .index
        .state_value()
        .unwrap()
}
fn normalized_error(value: &V) -> Vec<(&'static str, V)> {
    let mut fields = vec![
        ("name", value.get("name")),
        ("message", value.get("message")),
    ];
    if value.get("code") != V::Undefined {
        fields.push(("code", value.get("code")));
    }
    fields
}
fn projection(replay: &SourceContractReplay) -> V {
    let state = state(replay);
    let owner = state_index(replay);
    let requests = state
        .events
        .iter()
        .filter(|e| e.get("kind") == s("client-request"))
        .map(|e| {
            obj(&[
                ("ordinal", n(ob_usize(e.get("ordinal")).unwrap() as f64)),
                ("args", e.get("args")),
            ])
        })
        .collect();
    let notifications = state
        .events
        .iter()
        .filter(|e| e.get("kind") == s("unhandled-rejection"))
        .map(|event| {
            let id = ob_usize(event.get("errorRef")).unwrap();
            let error = state.error_value(id).unwrap();
            let settled = state
                .events
                .iter()
                .find(|e| {
                    e.get("kind") == s("promise-settled")
                        && e.get("promise") == event.get("promise")
                })
                .unwrap();
            assert_eq!(
                settled.get("errorRef"),
                event.get("errorRef"),
                "original rejected object must retain its identity"
            );
            obj(&[
                ("code", error.get("code")),
                ("name", error.get("name")),
                ("message", error.get("message")),
                ("isMainIndexPromise", event.get("isMainIndexPromise")),
            ])
        })
        .collect::<Vec<_>>();
    assert_eq!(
        replay.runtime().core.source_unhandled_rejections.len(),
        notifications.len(),
        "same global notification history"
    );
    let callers = state
        .callers
        .iter()
        .map(|caller| {
            if caller.get("status") == s("Fulfilled") {
                return caller.clone();
            }
            let error = state
                .error_value(ob_usize(caller.get("errorRef")).unwrap())
                .unwrap();
            let mut fields = vec![("status", s("Rejected"))];
            fields.extend(normalized_error(error));
            obj(&fields)
        })
        .collect();
    obj(&[
        (
            "importClientOwn",
            V::Bool(owner.get("txUtilsClient") != V::Undefined),
        ),
        (
            "importClientBound",
            V::Bool(owner.get("txUtilsClient") == obj(&[("$rpcClient", n(1.))])),
        ),
        (
            "classClientOwn",
            V::Bool(owner.get("classClient") != V::Undefined),
        ),
        (
            "classClientBound",
            V::Bool(owner.get("classClient") == obj(&[("$rpcClient", n(1.))])),
        ),
        ("mainConstructed", V::Bool(state.constructed)),
        ("indexSingleton", V::Bool(owner.get("singleton") != V::Null)),
        ("notifications", V::Array(notifications)),
        (
            "rejectionHandled",
            n(state
                .events
                .iter()
                .filter(|e| e.get("kind") == s("rejection-handled"))
                .count() as f64),
        ),
        ("requests", V::Array(requests)),
        ("callers", V::Array(callers)),
    ])
}
fn run_source_case(name: &str) {
    let fixture = include_str!("../fixtures/ordered_bootstrap_controls.jsonl");
    let frames = fixture
        .lines()
        .map(|line| crate::property_catalog::parse_diagnostic_value(line).unwrap())
        .filter(|row| row.get("case") == s(name))
        .collect::<Vec<_>>();
    assert!(!frames.is_empty(), "source case absent: {name}");
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    for frame in frames {
        let point = frame.get("point");
        if frame.get("rawConfiguration") != V::Null {
            let receipt = invoke(
                &mut replay,
                "SourceMainBootstrap.configure",
                frame.get("rawConfiguration"),
            );
            assert!(
                matches!(
                    receipt.outcome,
                    SourceContractCommandOutcome::Dependency(DependencyReturn::Immediate(_))
                ),
                "{name} configure: {:?}",
                receipt.outcome
            );
        }
        for command in values(frame.get("rawControls")) {
            let receipt = invoke(&mut replay, "SourceMainBootstrap.step", command);
            assert!(
                matches!(
                    receipt.outcome,
                    SourceContractCommandOutcome::Dependency(DependencyReturn::Immediate(_))
                ),
                "{name} {point:?}: {:?}",
                receipt.outcome
            );
        }
        assert_eq!(
            projection(&replay),
            frame.get("expectedProjection"),
            "{name} {point:?}"
        );
        if !replay.runtime().ordered_bootstrap_closed_for_adoption() {
            assert!(
                matches!(
                    replay.checkpoint(),
                    Err(SourceContractReplayError::InvalidCheckpoint)
                ),
                "pending prefix must not expose a legacy checkpoint"
            );
        }
    }
    assert!(
        replay.runtime().ordered_bootstrap_closed_for_adoption(),
        "{name} finite source jobs/notifications did not close"
    );
    let head = replay.head();
    let bytes = replay.checkpoint().unwrap();
    let restored = SourceContractReplay::from_checkpoint(&bytes, head).unwrap();
    assert_eq!(restored.head(), head);
    assert_eq!(restored.checkpoint().unwrap(), bytes);
    assert_eq!(
        restored.runtime().state_value().unwrap(),
        replay.runtime().state_value().unwrap()
    );
}
macro_rules! source_case {
    ($test:ident,$id:literal) => {
        #[test]
        fn $test() {
            run_source_case($id)
        }
    };
}
source_case!(
    source_01_immediate_success,
    "immediate-success-import-before-static"
);
source_case!(
    source_02_import_resolves_before_main,
    "deferred-import-resolves-before-main"
);
source_case!(
    source_03_import_resolves_after_main,
    "deferred-import-resolves-after-main"
);
source_case!(
    source_04_import_rejects_before_main,
    "deferred-import-rejects-before-main"
);
source_case!(
    source_05_import_rejects_after_main,
    "deferred-import-rejects-after-main"
);
source_case!(
    source_06_import_immediate_reject_before_main,
    "immediate-import-rejects-before-main"
);
source_case!(
    source_07_import_immediate_reject_same_turn,
    "immediate-import-rejects-same-turn-main"
);
source_case!(
    source_08_static_deferred_reject,
    "deferred-static-rejects-after-main"
);
source_case!(
    source_09_static_immediate_reject,
    "immediate-static-rejects-same-turn-main"
);
source_case!(
    source_10_two_failures_import_first,
    "both-deferred-reject-import-first"
);
source_case!(
    source_11_two_failures_static_first,
    "both-deferred-reject-index-first"
);
source_case!(
    source_12_raw_after_import_reject,
    "rejected-import-raw-call-preserves-original-init-reference-error"
);
source_case!(
    source_13_height_rpc_rejection,
    "main-height-rpc-rejection-is-separate-from-bootstrap"
);
source_case!(
    source_14_actual_late_handler,
    "static-rejection-explicit-late-handler"
);
fn configuration() -> V {
    obj(&[
        ("profile", s(ORDERED_BOOTSTRAP_PROFILE)),
        (
            "production",
            obj(&[
                (
                    "client",
                    obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
                ),
                ("io", obj(&[("height", n(11.))])),
            ]),
        ),
        (
            "clientCalls",
            V::Array(vec![
                obj(&[("mode", s("resolve"))]),
                obj(&[("mode", s("resolve"))]),
            ]),
        ),
    ])
}
fn command(action: &str) -> V {
    obj(&[
        ("profile", s(ORDERED_BOOTSTRAP_PROFILE)),
        ("action", s(action)),
    ])
}
#[test]
fn invalid_namespace_and_already_executed_old_setup_refuse_without_rewinding() {
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let before = replay.runtime().state_value().unwrap();
    let mut bad = configuration();
    set(&mut bad, "profile", s("old-V12-is-not-this-namespace")).unwrap();
    assert!(matches!(
        invoke(&mut replay, "SourceMainBootstrap.configure", bad).outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::InvalidInput)
    ));
    assert_eq!(replay.runtime().state_value().unwrap(), before);
    let old = configuration().get("production");
    let first = invoke(&mut replay, "SourceMain.configureProduction", old);
    assert!(matches!(
        first.outcome,
        SourceContractCommandOutcome::Dependency(_)
    ));
    let before = replay.runtime().state_value().unwrap();
    let raw = replay.checkpoint().unwrap();
    assert!(matches!(
        invoke(
            &mut replay,
            "SourceMainBootstrap.configure",
            configuration()
        )
        .outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::InvalidInput)
    ));
    assert_eq!(replay.runtime().state_value().unwrap(), before);
    assert!(replay.runtime().ordered_bootstrap_closed_for_adoption());
    assert_ne!(
        replay.checkpoint().unwrap(),
        raw,
        "failed command is journaled without changing actual old state"
    );
}
#[test]
fn unresolved_steps_fence_ordinary_calls_and_unknown_full_main_import() {
    let source = V::from_json(
        &crate::issuance_intent::JsonValue::parse(include_str!(
            "../fixtures/ordered_bootstrap_full_main_boundary.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        source.get("imports"),
        V::Array(vec![s("events"), s("util"), s("./reOrg.js")])
    );
    assert_eq!(
        source.get("result").get("thrown").get("name"),
        s("SourceBoundaryUnavailable")
    );
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    invoke(
        &mut replay,
        "SourceMainBootstrap.configure",
        configuration(),
    );
    invoke(
        &mut replay,
        "SourceMainBootstrap.step",
        command("ImportTxUtils"),
    );
    let before = replay.runtime().state_value().unwrap();
    let receipt = replay
        .apply(SourceContractCommand::Invoke(DependencyCall {
            target: "Main.getCurrentBlockHeight".into(),
            arguments: vec![],
        }))
        .unwrap();
    assert!(matches!(
        receipt.outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::UnknownDependency)
    ));
    assert_eq!(replay.runtime().state_value().unwrap(), before);
    let receipt = invoke(
        &mut replay,
        "SourceMainBootstrap.step",
        command("ImportFullMainAndIIFEs"),
    );
    assert!(matches!(
        receipt.outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::UnknownDependency)
    ));
    assert_eq!(replay.runtime().state_value().unwrap(), before);
}
#[test]
fn default_v2_refuses_new_namespace_and_retains_its_exact_state() {
    let mut replay = SourceContractReplay::new().unwrap();
    let before = replay.runtime().state_value().unwrap();
    let receipt = invoke(
        &mut replay,
        "SourceMainBootstrap.configure",
        configuration(),
    );
    assert!(matches!(
        receipt.outcome,
        SourceContractCommandOutcome::LocalError(_)
    ));
    assert_eq!(replay.runtime().state_value().unwrap(), before);
}

use crate::source_compact_index_v2::SourceCompactIndex;
use crate::source_main_native_owners::{
    NativeMainExpectation, NativeMainOwnerError, NativeMainOwners,
};
use crate::storage::Namespace;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static BOOTSTRAP_TEMP: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let path = parent.join(format!(
            "actual-ordered-bootstrap-{}-{}",
            std::process::id(),
            BOOTSTRAP_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize() {
            assert_eq!(
                path.parent(),
                Some(std::env::temp_dir().canonicalize().unwrap().as_path())
            );
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn try_adopt_pending(
    financial: SourceContractReplay,
    index: SourceCompactIndex,
) -> (SourceContractReplay, SourceCompactIndex) {
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    let rejected = NativeMainOwners::adopt(financial, index, &expected).unwrap_err();
    assert_eq!(rejected.error, NativeMainOwnerError::InitializationPending);
    let (financial, index) = rejected.into_owners();
    assert_eq!(financial.head(), expected.financial_head);
    assert_eq!(index.snapshot().unwrap(), expected.metadata);
    (financial, index)
}
#[test]
fn pending_and_notification_pending_profiles_retain_same_owners_until_actual_end_turn() {
    let temp = Temp::new();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index =
        SourceCompactIndex::create(&temp.0, namespace, "raw empty metadata origin").unwrap();
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let mut config = configuration();
    set(
        &mut config,
        "clientCalls",
        V::Array(vec![
            obj(&[
                ("mode", s("deferred")),
                (
                    "rejection",
                    obj(&[
                        ("name", s("Error")),
                        ("message", s("actual explicit client failure")),
                    ]),
                ),
            ]),
            obj(&[("mode", s("resolve"))]),
        ]),
    )
    .unwrap();
    invoke(&mut replay, "SourceMainBootstrap.configure", config);
    invoke(
        &mut replay,
        "SourceMainBootstrap.step",
        command("ImportTxUtils"),
    );
    (replay, index) = try_adopt_pending(replay, index);
    invoke(
        &mut replay,
        "SourceMainBootstrap.step",
        command("ConstructMain"),
    );
    invoke(
        &mut replay,
        "SourceMainBootstrap.step",
        command("RejectImport"),
    );
    invoke(
        &mut replay,
        "SourceMainBootstrap.step",
        command("DrainMicrotasks"),
    );
    assert!(!replay.runtime().ordered_bootstrap_closed_for_adoption());
    assert!(replay.checkpoint().is_err());
    (replay, index) = try_adopt_pending(replay, index);
    invoke(&mut replay, "SourceMainBootstrap.step", command("EndTurn"));
    assert!(replay.runtime().ordered_bootstrap_closed_for_adoption());
    let raw = replay.checkpoint().unwrap();
    let head = replay.head();
    assert_eq!(
        SourceContractReplay::from_checkpoint(&raw, head)
            .unwrap()
            .runtime()
            .state_value()
            .unwrap(),
        replay.runtime().state_value().unwrap()
    );
    let expected = NativeMainExpectation {
        financial_head: head,
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    let owners = NativeMainOwners::adopt(replay, index, &expected).unwrap();
    assert_eq!(owners.financial().head(), head);
}
#[test]
fn successful_closed_profile_hands_same_initialized_index_to_existing_service_without_duplicate_import(
) {
    let temp = Temp::new();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let index = SourceCompactIndex::create(&temp.0, namespace.clone(), "raw empty metadata origin")
        .unwrap();
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    invoke(
        &mut replay,
        "SourceMainBootstrap.configure",
        configuration(),
    );
    for action in [
        "ImportTxUtils",
        "ConstructMain",
        "CallMainHeight",
        "EndTurn",
    ] {
        invoke(&mut replay, "SourceMainBootstrap.step", command(action));
    }
    let before = state_index(&replay);
    let raw = replay.checkpoint().unwrap();
    let head = replay.head();
    assert_eq!(
        SourceContractReplay::from_checkpoint(&raw, head)
            .unwrap()
            .head(),
        head
    );
    let expected = NativeMainExpectation {
        financial_head: head,
        namespace,
        metadata: index.snapshot().unwrap(),
    };
    let owners = NativeMainOwners::adopt(replay, index, &expected).unwrap();
    let native =
        crate::source_main_native_session::SourceMainNativeSessionV13::adopt(owners).unwrap();
    let expected = native.expectation().unwrap();
    let active = native
        .into_active_service(&expected, n(10.), 71, V::Array(vec![]), 8 * 1024 * 1024)
        .unwrap();
    assert!(active.source_start_error().is_none());
    assert!(
        active.bootstrap_effects().is_empty(),
        "already imported SAME index must not make another getInstance[] request"
    );
    let actual = active
        .financial_runtime()
        .core
        .main_production
        .as_ref()
        .unwrap()
        .index
        .state_value()
        .unwrap();
    assert_eq!(actual, before);
}
#[test]
fn old_conditional_witness_descriptor_refuses_the_new_closed_initializer_boundary() {
    use crate::source_contract_replay::{
        SourceTransitionDescriptor, SourceTransitionError, SourceTransitionExpectation,
        SourceTransitionWitness,
    };
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    invoke(
        &mut replay,
        "SourceMainBootstrap.configure",
        configuration(),
    );
    for action in [
        "ImportTxUtils",
        "ConstructMain",
        "CallMainHeight",
        "EndTurn",
    ] {
        invoke(&mut replay, "SourceMainBootstrap.step", command(action));
    }
    let descriptor = SourceTransitionDescriptor::reference();
    let expected = SourceTransitionExpectation {
        policy: SourceContractRuntimePolicy::MainOrchestrationV12,
        descriptor: &descriptor,
        pre_head: replay.head(),
        commands_commitment: None,
        post_head: None,
    };
    assert_eq!(
        SourceTransitionWitness::produce(&replay, &[], &expected),
        Err(SourceTransitionError::UnsupportedCommand)
    );
    let old = SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
        .unwrap();
    let expected = SourceTransitionExpectation {
        pre_head: old.head(),
        ..expected
    };
    assert!(SourceTransitionWitness::produce(&old, &[], &expected).is_ok());
}
#[test]
fn real_old_provider_bytes_preserve_earlier_setup_and_unknown_actions_never_silently_change_meaning(
) {
    use crate::source_contract_replay::SourceContractReplayHead;
    let pin = |name: &str| {
        let row =
            include_str!("../fixtures/ordered_bootstrap_reference/old_v12_bootstrap_heads.tsv")
                .lines()
                .find(|row| row.starts_with(&format!("{name}\t")))
                .unwrap();
        let fields = row.split('\t').collect::<Vec<_>>();
        let bytes = fields[2]
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        SourceContractReplayHead {
            commands: fields[1].parse().unwrap(),
            commitment: bytes.try_into().unwrap(),
        }
    };
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    assert_eq!(replay.head(), pin("initial"));
    assert_eq!(
        replay.checkpoint().unwrap(),
        include_bytes!("../fixtures/ordered_bootstrap_reference/old_v12_initial.tlcr")
    );
    invoke(
        &mut replay,
        "SourceMain.configureProduction",
        configuration().get("production"),
    );
    assert_eq!(replay.head(), pin("configured"));
    assert_eq!(
        replay.checkpoint().unwrap(),
        include_bytes!("../fixtures/ordered_bootstrap_reference/old_v12_configured.tlcr")
    );
    // Captured by the actual immutable8bcc executable: the old interpreter
    // refused this unfamiliar namespace. Current support cannot relabel that
    // old recorded refusal as successful startup during checkpoint replay.
    assert_eq!(
        SourceContractReplay::from_checkpoint(
            include_bytes!(
                "../fixtures/ordered_bootstrap_reference/old_v12_unknown_bootstrap.tlcr"
            ),
            pin("unknownBootstrap")
        ),
        Err(SourceContractReplayError::ReplayMismatch)
    );
}

include!("source_main_bootstrap_old_dispatch_tests.rs");
