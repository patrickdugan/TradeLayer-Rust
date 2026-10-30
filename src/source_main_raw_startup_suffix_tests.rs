// WIP private-Core proposal: uncompiled. Source provides RAW inputs only.
fn raw_suffix_input() -> &'static str {
    include_str!("../fixtures/startup_init_suffix.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "kind")) == "input"
                && json_string(raw_get(line, "case"))
                    == "suffix/11-after-real300-entire-empty-initialize-returns"
        })
        .unwrap()
}
fn prefix_case_through_real300(temp: &Temp, case: &str) -> (SourceMainRawStartupV1, V) {
    let raw = include_str!("../fixtures/startup_init_suffix.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "kind")) == "input"
                && json_string(raw_get(line, "case")) == case
        })
        .unwrap();
    let (financial, index, expected, inputs) = setup(temp, raw);
    let production_io = untag(raw_get(raw, "rawReset")).get("orchestration");
    let mut startup =
        SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    startup.apply(StartupCommandV1::ConstructMain).unwrap();
    startup.apply(StartupCommandV1::EnterInitialize).unwrap();
    drain(&mut startup);
    for record in include_str!("../fixtures/startup_init_suffix.jsonl")
        .lines()
        .filter(|line| {
            json_string(raw_get(line, "kind")) == "observation"
                && json_string(raw_get(line, "case")) == case
        })
    {
        let control = untag(raw_get(record, "control"));
        match control.get("kind") {
            V::String(kind) if kind == "FireOriginalTimer" => {
                let (id, delay, _) = startup.next_timer().unwrap();
                assert_eq!(control.get("delay"), V::number(delay as f64));
                startup
                    .apply(StartupCommandV1::FireTimer { id, generation: 9 })
                    .unwrap();
                drain(&mut startup);
                if delay == 300 {
                    break;
                }
            }
            V::String(kind) if kind == "ReleaseOriginalAutoDbClient" => {
                startup
                    .apply(StartupCommandV1::SettleClient { token: 1 })
                    .unwrap();
                drain(&mut startup);
            }
            _ => (),
        }
    }
    assert_eq!(startup.stop(), StartupStopV1::InitializeSuffixReady);
    assert!(startup.next_timer().is_none());
    (startup, production_io)
}
#[test]
fn actual_index_suffix_does_not_rerun_prefix_and_holds_real_insert_await() {
    use crate::source_contract_runtime::{
        NativeMainRetainedCallerStopV1 as Stop, NativeMainRowStopV1 as Row,
    };
    for case in [
        "suffix/11-after-real300-entire-empty-initialize-returns",
        "suffix/12-held-db-then-real300-entire-empty-initialize-returns",
        "suffix/13-empty-own-chain-entire-empty-initialize-returns",
    ] {
        let temp = Temp::new();
        let (startup, io) = prefix_case_through_real300(&temp, case);
        let native_before = startup.index.snapshot().unwrap();
        let before_tally = startup.financial.runtime().collection("tallyMap").to_vec();
        let previous_timers = startup
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("timer-scheduled"))
            .count();
        let previous_clients = startup
            .effects()
            .iter()
            .filter(|e| {
                e.get("kind") == s("rpc-input") && e.get("method") == s("Client.getInstance")
            })
            .count();
        let mut suffix = startup
            .into_initialize_index_suffix(700, V::Array(vec![]), io)
            .unwrap();
        let mut inserts = 0;
        for _ in 0..128 {
            match suffix.advance() {
                Stop::Native(Row::AwaitNativeWrite) => {
                    let descriptor = suffix.pending_descriptor().expect("exact held source call");
                    let before = suffix.startup.index.snapshot().unwrap();
                    assert_eq!(suffix.caller_result(), None);
                    assert_eq!(
                        suffix.append_pending(&descriptor, None),
                        Row::AwaitNativeWrite
                    );
                    assert_ne!(suffix.startup.index.snapshot().unwrap(), before);
                    assert_eq!(
                        suffix.caller_result(),
                        None,
                        "physical insert is not JS await fulfillment"
                    );
                    assert_eq!(suffix.resolve_post(&descriptor), Row::Complete);
                    inserts += 1;
                }
                Stop::ConsensusReady => break,
                other => panic!("source suffix {other:?}"),
            }
        }
        assert_eq!(suffix.advance(), Stop::ConsensusReady);
        assert_eq!(inserts, 1);
        assert_eq!(
            suffix.startup.index.get("genesisBlock").unwrap(),
            Some(obj(&[
                ("_id", s("genesisBlock")),
                ("value", V::number(2819000.))
            ]))
        );
        assert_eq!(
            suffix
                .startup
                .index
                .get("MaxHeight")
                .unwrap()
                .unwrap()
                .get("value"),
            V::number(0.)
        );
        assert_ne!(suffix.startup.index.snapshot().unwrap(), native_before);
        assert_eq!(
            suffix.startup.financial.runtime().collection("tallyMap"),
            before_tally.as_slice()
        );
        assert_eq!(
            suffix
                .effects()
                .iter()
                .filter(|e| e.get("kind") == s("timer-scheduled"))
                .count(),
            previous_timers,
            "no constructor/1500/500/300 reexecution"
        );
        assert_eq!(
            suffix
                .effects()
                .iter()
                .filter(|e| e.get("kind") == s("rpc-input")
                    && e.get("method") == s("Client.getInstance"))
                .count(),
            previous_clients + 1,
            "same singleton getInstance still executes literal init(true) once"
        );
        assert_eq!(
            suffix.caller_result(),
            None,
            "actual consensus remains unexecuted, not fake Main fulfillment"
        );
        let mut returned = false;
        for _ in 0..128 {
            let (stop, caller_returned) = suffix
                .advance_consensus(1)
                .expect("actual original consensus body");
            if caller_returned {
                returned = true;
                break;
            }
            assert!(
                matches!(
                    stop,
                    crate::source_contract_runtime::NativeMainServiceStopV1::DrainBound
                        | crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
                ),
                "{stop:?}"
            );
        }
        assert!(
            returned,
            "must execute real body through caller fulfillment, not stop at ConsensusReady"
        );
        assert_eq!(
            suffix.caller_result(),
            Some(&DependencyReturn::Pending(V::Undefined))
        );
        let source = include_str!("../fixtures/startup_init_suffix.jsonl")
            .lines()
            .find(|line| {
                json_string(raw_get(line, "case")) == case
                    && json_string(raw_get(line, "kind")) == "observation"
                    && json_string(raw_get(line, "point"))
                        == "same-original-initialize-suffix-fulfilled"
            })
            .unwrap();
        assert_eq!(
            untag(raw_get(source, "caller")).get("state"),
            s("Fulfilled")
        );
        assert_eq!(
            suffix.startup.financial.runtime().collection("tallyMap"),
            before_tally.as_slice()
        );
        assert_eq!(
            suffix
                .startup
                .financial_state()
                .unwrap()
                .get("borrowedQueue"),
            V::Null,
            "one original header closes only after real complete body"
        );
        let effects = suffix.effects().to_vec();
        suffix.advance_consensus(1).unwrap();
        assert_eq!(suffix.effects(), effects);
        assert!(SourceCompactIndex::open(
            &temp.root,
            suffix.startup.origin.namespace.clone(),
            &suffix.startup.index.snapshot().unwrap()
        )
        .is_err());
    }
}
#[test]
fn premature_handoff_retains_whole_pending_startup_owner_without_source_calls() {
    let raw = raw_suffix_input();
    let temp = Temp::new();
    let (financial, index, expected, inputs) = setup(&temp, raw);
    let mut startup =
        SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    startup.apply(StartupCommandV1::ConstructMain).unwrap();
    startup.apply(StartupCommandV1::EnterInitialize).unwrap();
    drain(&mut startup);
    let before_fields = fields(&startup);
    let before_state = startup.financial_state().unwrap();
    let effects = startup.effects().to_vec();
    let rejected = startup
        .into_initialize_index_suffix(700, V::Array(vec![]), obj(&[]))
        .err()
        .expect("real1500 remains held");
    assert_eq!(rejected.error, SourceRuleError::InvalidInput);
    let startup = rejected.into_startup();
    assert_eq!(fields(&startup), before_fields);
    assert_eq!(startup.financial_state().unwrap(), before_state);
    assert_eq!(startup.effects(), effects);
    assert_eq!(startup.next_timer().map(|(_, delay, _)| delay), Some(1500));
}
#[test]
fn warm_database_short_circuit_preserves_actual_null_client_gettest_throw() {
    use crate::source_contract_runtime::NativeMainRetainedCallerStopV1 as Stop;
    for label in [
        "warm/16-null-main-client-throws-getTest-after-db-short-circuit",
        "warm/17-undefined-main-client-throws-getTest-after-db-short-circuit",
    ] {
        let raw = include_str!("../fixtures/startup_init_warm_client.jsonl")
            .lines()
            .find(|line| {
                json_string(raw_get(line, "kind")) == "input"
                    && json_string(raw_get(line, "case")) == label
            })
            .unwrap();
        let source = include_str!("../fixtures/startup_init_warm_client.jsonl")
            .lines()
            .find(|line| {
                json_string(raw_get(line, "kind")) == "observation"
                    && json_string(raw_get(line, "case")) == label
                    && json_string(raw_get(line, "point")) == "same-1500ms-resolved"
            })
            .unwrap();
        let temp = Temp::new();
        let (financial, index, expected, inputs) = setup(&temp, raw);
        let io = untag(raw_get(raw, "rawReset")).get("orchestration");
        let mut startup =
            SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
        startup.apply(StartupCommandV1::ConstructMain).unwrap();
        startup
            .apply(StartupCommandV1::SettleClient { token: 1 })
            .unwrap();
        drain(&mut startup);
        startup.apply(StartupCommandV1::EnterInitialize).unwrap();
        drain(&mut startup);
        let (id, delay, _) = startup.next_timer().unwrap();
        assert_eq!(delay, 1500);
        startup
            .apply(StartupCommandV1::FireTimer { id, generation: 9 })
            .unwrap();
        drain(&mut startup);
        assert_eq!(
            startup.stop(),
            StartupStopV1::InitializeSuffixReady,
            "warm DB avoids earlier chain property access"
        );
        let test_calls = startup
            .effects()
            .iter()
            .filter(|e| e.get("method") == s("Client.getTest"))
            .count();
        let mut suffix = startup
            .into_initialize_index_suffix(700, V::Array(vec![]), io)
            .unwrap();
        assert_eq!(
            suffix.advance(),
            Stop::CallerReturned,
            "native throw is not a local Backend refusal"
        );
        let expected_error = untag(raw_get(source, "caller")).get("error");
        let Some(DependencyReturn::PendingThrow(actual)) = suffix.caller_result() else {
            panic!("same native caller rejection")
        };
        assert_eq!(s(&actual.name), expected_error.get("name"));
        assert_eq!(s(&actual.message), expected_error.get("message"));
        assert_eq!(
            suffix
                .effects()
                .iter()
                .filter(|e| e.get("method") == s("Client.getTest"))
                .count(),
            test_calls,
            "property read throws before RPC execution"
        );
        assert_eq!(suffix.startup.index.snapshot().unwrap(), expected.metadata);
        assert!(suffix.pending_descriptor().is_none());
    }
}
