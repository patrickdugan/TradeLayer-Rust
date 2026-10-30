// Actual-Core proposal. Source inputs are raw; comparisons never execute as
// source answers. Parent owns Core build/execution and previous081 negatives.
fn literal_nonempty_ready(temp: &Temp) -> SourceMainRawStartupLiteralInitializeV1 {
    use crate::source_contract_runtime::{
        NativeMainRetainedCallerStopV1 as Stop, NativeMainRowStopV1 as Row,
    };
    let case = "first10/01-native-bootstrap-before500";
    let raw = nonempty_record(case, "input", None);
    let (financial, index, expected, inputs) = setup(temp, raw);
    let io = untag(raw_get(raw, "rawReset")).get("orchestration");
    let mut startup =
        SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    startup.apply(StartupCommandV1::ConstructMain).unwrap();
    startup.apply(StartupCommandV1::EnterInitialize).unwrap();
    drain(&mut startup);
    for record in NONEMPTY_SOURCE.lines().filter(|raw| {
        json_string(raw_get(raw, "kind")) == "observation"
            && json_string(raw_get(raw, "case")) == case
    }) {
        let control = untag(raw_get(record, "control"));
        match control.get("kind") {
            V::String(kind) if kind == "FireOriginalTimer" => {
                let (id, delay, _) = startup.next_timer().unwrap();
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
    let mut suffix = startup
        .into_initialize_index_suffix(3700, V::Array(vec![]), io)
        .unwrap()
        .into_literal_source()
        .unwrap();
    for _ in 0..128 {
        match suffix.advance() {
            Stop::ConsensusReady => return suffix.into_live_initialize().unwrap(),
            Stop::Native(Row::AwaitNativeWrite) => {
                let operation = suffix.pending_descriptor().unwrap();
                assert_eq!(
                    suffix.append_pending(&operation, None),
                    Row::AwaitNativeWrite
                );
                assert_eq!(suffix.resolve_post(&operation), Row::Complete);
            }
            other => panic!("literal source index suffix {other:?}"),
        }
    }
    panic!("bounded literal suffix")
}
fn literal_fields(owner: &SourceMainRawStartupLiteralInitializeV1) -> BTreeMap<String, V> {
    let mut fields = BTreeMap::new();
    owner
        .visit_canonical_fields(&mut |path, value| {
            assert!(
                fields.insert(path.into(), value.clone()).is_none(),
                "{path}"
            );
            Ok(())
        })
        .unwrap();
    fields
}
fn literal_until_financial_await(
    owner: &mut SourceMainRawStartupLiteralInitializeV1,
) -> crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1 {
    for _ in 0..512 {
        let (stop, caller) = owner.advance(1).unwrap();
        assert!(!caller);
        if stop == crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite {
            let pending = owner.pending_operations();
            assert_eq!(pending.len(), 1);
            return pending.into_iter().next().unwrap();
        }
    }
    panic!("real native financial source await")
}
fn literal_pump(owner: &mut SourceMainRawStartupLiteralInitializeV1) -> bool {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let mut first_completed = false;
    let mut returned = false;
    for _ in 0..2048 {
        let (stop, caller) = owner.advance(1).unwrap();
        returned |= caller;
        if literal_fields(owner).get("literalStartup/nested/pc") == Some(&s("DrainFirstIncoming")) {
            first_completed = true;
            assert_eq!(
                owner.caller_result(),
                None,
                "inner returned Promise must not fulfill outer Initialize"
            );
            assert_ne!(
                owner.financial_state().unwrap().get("borrowedQueue"),
                V::Null,
                "one original header/queue remains borrowed between actual calls"
            );
        }
        match stop {
            Stop::AwaitNativeWrite => {
                let pending = owner.pending_operations();
                assert_eq!(pending.len(), 1, "exact independently retained operation");
                let operation = pending.into_iter().next().unwrap();
                assert_eq!(owner.caller_result(), None);
                assert_eq!(
                    owner.append_pending(&operation, None).unwrap(),
                    Stop::AwaitNativeWrite
                );
                assert_eq!(owner.resolve_post(&operation).unwrap(), Stop::Quiescent);
            }
            Stop::PendingTimer if returned => break,
            Stop::PendingTimer | Stop::DrainBound | Stop::Quiescent => (),
            other => panic!("literal nested consensus {other:?}"),
        }
    }
    assert!(returned, "actual outer caller eventually fulfills");
    first_completed
}
#[test]
fn literal_equal_tip_runs_inner_then_outer_consensus_without_double_issuing_finance() {
    let temp = Temp::new();
    let mut owner = literal_nonempty_ready(&temp);
    assert!(
        literal_pump(&mut owner),
        "actual inner call completed separately"
    );
    assert_eq!(
        owner.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    let source = nonempty_record(
        "first10/01-native-bootstrap-before500",
        "observation",
        Some("real-nonempty-initialize-caller-and-live-prefix"),
    );
    for category in ["propertyList", "tallyMap", "activations"] {
        assert_eq!(
            sort_original_table(
                owner
                    .live
                    .suffix
                    .startup
                    .financial
                    .runtime()
                    .collection(category)
                    .to_vec()
            ),
            original_nonempty_table(source, category),
            "complete actual source {category}"
        );
    }
    let fields = literal_fields(&owner);
    assert_eq!(
        fields.get("literalStartup/nested/completedFirst/present"),
        Some(&V::Bool(true))
    );
    let source_effects = arr(untag(raw_get(source, "effects")));
    for method in ["Client.getBlockCount", "Client.getChain", "Client.getTest"] {
        let expected = source_effects
            .iter()
            .filter(|e| e.get("kind") == s("rpc-input") && e.get("method") == s(method))
            .count();
        let actual = owner
            .effects()
            .iter()
            .filter(|e| e.get("kind") == s("rpc-input") && e.get("method") == s(method))
            .count();
        assert_eq!(actual, expected, "complete RPC call count {method}");
    }
    assert_eq!(
        owner.financial_state().unwrap().get("borrowedQueue"),
        V::Null
    );
    let current = owner.live.suffix.startup.index.snapshot().unwrap();
    assert!(SourceCompactIndex::open(
        &temp.root,
        owner.live.suffix.startup.origin.namespace.clone(),
        &current
    )
    .is_err());
}
#[test]
fn literal_two_discarded_incoming_jobs_keep_real_clock_and_financial_header_lifetime() {
    let temp = Temp::new();
    let mut owner = literal_nonempty_ready(&temp);
    assert!(literal_pump(&mut owner));
    let kernel = owner.live.service.kernel_value();
    let timers = arr(kernel.get("timers"));
    assert_eq!(
        timers.len(),
        2,
        "unchanged source produces TWO incoming jobs"
    );
    assert_eq!(timers[0].get("id"), s("4"));
    assert_eq!(timers[1].get("id"), s("5"));
    for timer in timers {
        assert_eq!(timer.get("delay"), V::number(10000.));
        assert_eq!(timer.get("due"), V::number(12300.));
    }
    assert_eq!(kernel.get("logicalTime"), V::number(2300.));
    assert_eq!(owner.next_timer(), Some(4));
    let effects = owner.effects().to_vec();
    let before = owner.financial_state().unwrap();
    assert_eq!(
        owner.fire_timer(4, 10),
        Err(crate::source_main_step_kernel::MainStepError::WrongGeneration)
    );
    assert_eq!(owner.effects(), effects);
    assert_eq!(owner.financial_state().unwrap(), before);
}
#[test]
fn literal_actual_pending_routes_remain_owner_bound_and_cannot_resolve_before_append() {
    let ta = Temp::new();
    let tb = Temp::new();
    let mut a = literal_nonempty_ready(&ta);
    let mut b = literal_nonempty_ready(&tb);
    let pa = literal_until_financial_await(&mut a);
    let pb = literal_until_financial_await(&mut b);
    assert_ne!(pa, pb);
    let before = b.financial_state().unwrap();
    let effects = b.effects().to_vec();
    let pin = b.live.suffix.startup.index.snapshot().unwrap();
    assert_eq!(
        b.append_pending(&pa, None),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(b.resolve_post(&pb), Err(SourceRuleError::InvalidInput));
    assert_eq!(b.effects(), effects);
    assert_eq!(b.financial_state().unwrap(), before);
    assert_eq!(b.live.suffix.startup.index.snapshot().unwrap(), pin);
}
