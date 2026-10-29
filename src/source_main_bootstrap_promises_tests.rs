fn v2_error(state: &OrderedBootstrap, reference: V) -> V {
    let id = ob_usize(reference).unwrap();
    let error = state.error_value(id).unwrap();
    let mut fields = normalized_error(error);
    fields.push(("group", n(id as f64)));
    obj(&fields)
}
fn v2_projection(replay: &SourceContractReplay) -> V {
    let mut actual = projection(replay);
    let state = state(replay);
    let mut trace = vec![];
    for event in &state.events {
        match event.get("kind") {
            V::String(kind) if kind == "client-request" => trace.push(obj(&[
                ("kind", s("client-request")),
                ("ordinal", n(ob_usize(event.get("ordinal")).unwrap() as f64)),
                ("args", event.get("args")),
            ])),
            V::String(kind) if kind == "caller-result" => {
                let result = event.get("result");
                let result = if result.get("status") == s("Fulfilled") {
                    result
                } else {
                    let mut fields = vec![("status", s("Rejected"))];
                    fields.extend(normalized_error(
                        state
                            .error_value(ob_usize(result.get("errorRef")).unwrap())
                            .unwrap(),
                    ));
                    obj(&fields)
                };
                trace.push(obj(&[("kind", s("caller-result")), ("result", result)]));
            }
            V::String(kind) if kind == "caller-catch" => trace.push(obj(&[
                ("kind", s("caller-catch")),
                ("handler", event.get("handler")),
                ("error", v2_error(&state, event.get("errorRef"))),
            ])),
            V::String(kind) if kind == "unhandled-rejection" => trace.push(obj(&[
                ("kind", s("unhandled-rejection")),
                ("isMainIndexPromise", event.get("isMainIndexPromise")),
                ("error", v2_error(&state, event.get("errorRef"))),
            ])),
            V::String(kind) if kind == "rejection-handled" => trace.push(obj(&[
                ("kind", s("rejection-handled")),
                ("isMainIndexPromise", event.get("isMainIndexPromise")),
            ])),
            _ => (),
        }
    }
    actual.insert("visibleTrace", V::Array(trace)).unwrap();
    actual
}
fn v2_frames(name: &str) -> Vec<V> {
    include_str!("../fixtures/ordered_bootstrap_promises_controls.jsonl")
        .lines()
        .map(|line| crate::property_catalog::parse_diagnostic_value(line).unwrap())
        .filter(|frame| frame.get("case") == s(name))
        .collect()
}
fn v2_apply_frame(replay: &mut SourceContractReplay, frame: &V) {
    if frame.get("rawConfiguration") != V::Null {
        let receipt = invoke(
            replay,
            "SourceMainBootstrap.configure",
            frame.get("rawConfiguration"),
        );
        assert!(
            matches!(
                receipt.outcome,
                SourceContractCommandOutcome::Dependency(DependencyReturn::Immediate(_))
            ),
            "configure {:?}: {:?}",
            frame.get("case"),
            receipt.outcome
        );
    }
    for control in values(frame.get("rawControls")) {
        let receipt = invoke(replay, "SourceMainBootstrap.step", control);
        assert!(
            matches!(
                receipt.outcome,
                SourceContractCommandOutcome::Dependency(DependencyReturn::Immediate(_))
            ),
            "{:?}/{:?}: {:?}",
            frame.get("case"),
            frame.get("point"),
            receipt.outcome
        );
    }
    assert_eq!(
        v2_projection(replay),
        frame.get("expectedProjection"),
        "{:?}/{:?}",
        frame.get("case"),
        frame.get("point")
    );
    if !replay.runtime().ordered_bootstrap_closed_for_adoption() {
        assert_eq!(
            replay.checkpoint(),
            Err(SourceContractReplayError::InvalidCheckpoint)
        );
    }
}
fn run_v2_case(name: &str) -> SourceContractReplay {
    let frames = v2_frames(name);
    assert!(!frames.is_empty());
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    for frame in frames {
        v2_apply_frame(&mut replay, &frame);
    }
    assert!(
        replay.runtime().ordered_bootstrap_closed_for_adoption(),
        "finite source jobs not closed: {name}"
    );
    let head = replay.head();
    let raw = replay.checkpoint().unwrap();
    let restored = SourceContractReplay::from_checkpoint(&raw, head).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), raw);
    assert_eq!(
        restored.runtime().state_value().unwrap(),
        replay.runtime().state_value().unwrap()
    );
    replay
}
macro_rules! promise_source_case {
    ($test:ident,$case:literal) => {
        #[test]
        fn $test() {
            run_v2_case($case);
        }
    };
}
promise_source_case!(
    promise_01_early_immediate_catch,
    "static-immediate-reject-handler-before-first-turn"
);
promise_source_case!(
    promise_02_deferred_early_catch,
    "static-deferred-handler-before-reject"
);
promise_source_case!(
    promise_03_reject_then_same_turn_catch,
    "static-deferred-reject-then-handler-same-turn"
);
promise_source_case!(
    promise_04_late_catch_does_not_renotify,
    "static-notified-late-handler-repeat-turns"
);
promise_source_case!(
    promise_05_competing_immediate_rejections,
    "both-immediate-reject-one-turn"
);
promise_source_case!(
    promise_06_same_turn_import_first,
    "both-deferred-reject-one-turn-import-first"
);
promise_source_case!(
    promise_07_same_turn_static_first,
    "both-deferred-reject-one-turn-static-first"
);
promise_source_case!(
    promise_08_import_success_static_rejection,
    "same-turn-import-resolve-static-reject"
);
promise_source_case!(
    promise_09_static_success_import_rejection,
    "same-turn-static-resolve-import-reject"
);
promise_source_case!(
    promise_10_real_host_await_checkpoints,
    "both-deferred-resolve-static-first-checkpoints"
);
promise_source_case!(
    promise_11_shared_error_identity,
    "same-error-object-rejects-two-source-promises"
);
promise_source_case!(
    promise_12_multiple_catches_one_handled_notification,
    "two-late-handlers-one-rejection-handled-notification"
);
#[test]
fn v2_assigned_clients_do_not_release_pending_outer_promises_or_native_owners() {
    let frames = v2_frames("both-deferred-resolve-static-first-checkpoints");
    let split = frames
        .iter()
        .position(|frame| frame.get("point") == s("microtask-checkpoint-2"))
        .unwrap();
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    for frame in &frames[..=split] {
        v2_apply_frame(&mut replay, frame);
    }
    let index_state = state_index(&replay);
    assert_eq!(
        index_state.get("txUtilsClient"),
        obj(&[("$rpcClient", n(1.))])
    );
    assert_eq!(
        index_state.get("classClient"),
        obj(&[("$rpcClient", n(1.))])
    );
    assert_eq!(state(&replay).import_status, "AwaitOuter");
    assert_eq!(state(&replay).index_status, "AwaitOuter");
    assert!(!replay.runtime().ordered_bootstrap_closed_for_adoption());
    let temp = Temp::new();
    let index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "raw v2 startup origin",
    )
    .unwrap();
    let (mut replay, index) = try_adopt_pending(replay, index);
    for frame in &frames[split + 1..] {
        v2_apply_frame(&mut replay, frame);
    }
    let expected = NativeMainExpectation {
        financial_head: replay.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    let owners = NativeMainOwners::adopt(replay, index, &expected).unwrap();
    assert_eq!(owners.financial().head(), expected.financial_head);
}
#[test]
fn v2_input_aliases_are_prevalidated_and_profiles_cannot_rewind_each_other() {
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let shared =
        v2_frames("same-error-object-rejects-two-source-promises")[0].get("rawConfiguration");
    let mut bad = shared.clone();
    bad.insert("rejectionObjects", V::Array(vec![])).unwrap();
    let before = replay.runtime().state_value().unwrap();
    assert_eq!(
        invoke(&mut replay, "SourceMainBootstrap.configure", bad).outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::InvalidInput)
    );
    assert_eq!(replay.runtime().state_value().unwrap(), before);
    let mut bad = shared.clone();
    bad.insert("expectedBalance", n(999.)).unwrap();
    assert_eq!(
        invoke(&mut replay, "SourceMainBootstrap.configure", bad).outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::InvalidInput)
    );
    assert_eq!(replay.runtime().state_value().unwrap(), before);
    invoke(
        &mut replay,
        "SourceMainBootstrap.configure",
        configuration(),
    );
    let old = replay.runtime().state_value().unwrap();
    assert_eq!(
        invoke(&mut replay, "SourceMainBootstrap.configure", shared).outcome,
        SourceContractCommandOutcome::LocalError(SourceRuleError::UnknownDependency)
    );
    assert_eq!(replay.runtime().state_value().unwrap(), old);
}
#[test]
fn v2_closed_profile_retains_the_same_legacy_witness_refusal() {
    use crate::source_contract_replay::{
        SourceTransitionDescriptor, SourceTransitionError, SourceTransitionExpectation,
        SourceTransitionWitness,
    };
    let replay = run_v2_case("static-immediate-reject-handler-before-first-turn");
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
}
