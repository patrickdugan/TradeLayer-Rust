// Private actual-Core tests proposed for Root. Author has not Core-compiled them.
const NONEMPTY_SOURCE: &str = include_str!("../fixtures/startup_init_nonempty_default_a.jsonl");
fn nonempty_record(case: &str, kind: &str, point: Option<&str>) -> &'static str {
    NONEMPTY_SOURCE
        .lines()
        .find(|raw| {
            json_string(raw_get(raw, "case")) == case
                && json_string(raw_get(raw, "kind")) == kind
                && point.is_none_or(|p| json_string(raw_get(raw, "point")) == p)
        })
        .expect("executed original-source record")
}
fn nonempty_through_index(temp: &Temp, case: &str) -> SourceMainRawStartupIndexSuffixV1 {
    use crate::source_contract_runtime::{
        NativeMainRetainedCallerStopV1 as Stop, NativeMainRowStopV1 as Row,
    };
    let input = nonempty_record(case, "input", None);
    let (financial, index, expected, inputs) = setup(temp, input);
    let production_io = untag(raw_get(input, "rawReset")).get("orchestration");
    let mut startup =
        SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    startup.apply(StartupCommandV1::ConstructMain).unwrap();
    startup.apply(StartupCommandV1::EnterInitialize).unwrap();
    drain(&mut startup);
    for raw in NONEMPTY_SOURCE.lines().filter(|raw| {
        json_string(raw_get(raw, "kind")) == "observation"
            && json_string(raw_get(raw, "case")) == case
    }) {
        let control = untag(raw_get(raw, "control"));
        match control.get("kind") {
            V::String(kind) if kind == "FireOriginalTimer" => {
                let (id, delay, _) = startup.next_timer().unwrap();
                assert_eq!(V::number(delay as f64), control.get("delay"));
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
    let mut suffix = startup
        .into_initialize_index_suffix(1700, V::Array(vec![]), production_io)
        .unwrap();
    for _ in 0..128 {
        match suffix.advance() {
            Stop::ConsensusReady => return suffix,
            Stop::Native(Row::AwaitNativeWrite) => {
                let descriptor = suffix.pending_descriptor().unwrap();
                assert_eq!(
                    suffix.append_pending(&descriptor, None),
                    Row::AwaitNativeWrite
                );
                assert_eq!(suffix.resolve_post(&descriptor), Row::Complete);
            }
            other => panic!("actual startup index body: {other:?}"),
        }
    }
    panic!("bounded actual startup index body")
}
fn original_nonempty_table(raw: &str, category: &str) -> Vec<V> {
    let tables = json_parts(raw_get(raw_get(raw, "financial"), "tables"));
    let table = tables
        .into_iter()
        .find(|table| json_string(raw_get(table, "category")) == category)
        .unwrap();
    arr(untag(raw_get(table, "rows")))
}
fn sort_original_table(mut rows: Vec<V>) -> Vec<V> {
    // The source observer explicitly uses original find({}).sort({_id:1}).
    rows.sort_by_key(|row| match row.get("_id") {
        V::String(id) => id,
        other => panic!("source _id {other:?}"),
    });
    rows
}
fn advance_until_nonempty_await(
    live: &mut SourceMainRawStartupLiveInitializeV1,
) -> crate::source_contract_runtime::NativeMainRetainedConsensusPendingV1 {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    for _ in 0..256 {
        let (stop, returned) = live.advance(1).unwrap();
        assert!(
            !returned,
            "source caller must not bypass its actual row validity await"
        );
        match stop {
            Stop::AwaitNativeWrite => {
                let pending = live.pending_operations();
                assert_eq!(
                    pending.len(),
                    1,
                    "choose exact actual operation, never first-of-many"
                );
                return pending.into_iter().next().unwrap();
            }
            Stop::DrainBound | Stop::Quiescent => (),
            other => panic!("real source catchup prefix: {other:?}"),
        }
    }
    panic!("bounded actual source financial prefix")
}
#[test]
fn nonempty_initialize_executes_real_finance_and_retains_exact_native_await() {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let case = "first10/01-native-bootstrap-before500";
    let temp = Temp::new();
    let suffix = nonempty_through_index(&temp, case);
    assert_eq!(
        suffix.startup.financial.runtime().tally_cache(),
        V::Array(vec![])
    );
    let mut live = suffix.into_live_initialize().unwrap();
    let operation = advance_until_nonempty_await(&mut live);
    assert_eq!(live.caller_result(), None);
    assert_ne!(
        live.suffix.startup.financial.runtime().tally_cache(),
        V::Array(vec![])
    );
    let before = live.suffix.startup.index.snapshot().unwrap();
    assert_eq!(
        live.resolve_post(&operation),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        live.append_pending(&operation, None).unwrap(),
        Stop::AwaitNativeWrite
    );
    let after = live.suffix.startup.index.snapshot().unwrap();
    assert_ne!(before, after);
    let financial = live.financial_state().unwrap();
    let effects = live.effects().to_vec();
    assert_eq!(
        live.caller_result(),
        None,
        "native publication is not JS fulfillment"
    );
    assert_eq!(live.advance(1), Err(SourceRuleError::InvalidInput));
    assert_eq!(
        live.append_pending(&operation, None),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(live.financial_state().unwrap(), financial);
    assert_eq!(live.effects(), effects);
    assert_eq!(live.resolve_post(&operation).unwrap(), Stop::Quiescent);
    assert_eq!(
        live.resolve_post(&operation),
        Err(SourceRuleError::InvalidInput)
    );
    let mut returned = false;
    for _ in 0..256 {
        let (stop, caller_returned) = live.advance(1).unwrap();
        if caller_returned {
            returned = true;
            break;
        }
        assert!(
            matches!(stop, Stop::DrainBound | Stop::Quiescent),
            "{stop:?}"
        );
    }
    assert!(returned);
    assert_eq!(
        live.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    let source = nonempty_record(
        case,
        "observation",
        Some("real-nonempty-initialize-caller-and-live-prefix"),
    );
    assert_eq!(
        untag(raw_get(source, "caller")).get("state"),
        s("Fulfilled")
    );
    // These are complete selected financial tables, not supplied execution data.
    // Random NeDB contractList/insurance IDs and global hash receipts remain a
    // distinct captured-external-entropy replay gate; they are NOT normalized.
    for category in ["propertyList", "tallyMap", "activations"] {
        assert_eq!(
            sort_original_table(
                live.suffix
                    .startup
                    .financial
                    .runtime()
                    .collection(category)
                    .to_vec()
            ),
            original_nonempty_table(source, category),
            "actual complete source table {category}"
        );
    }
    let mut native_rows = vec![];
    live.suffix
        .startup
        .index
        .visit_rows(|row| {
            native_rows.push(row.clone());
            Ok(true)
        })
        .unwrap();
    assert_eq!(
        sort_original_table(native_rows),
        original_nonempty_table(source, "txIndex")
    );
    assert_eq!(live.suffix.startup.index.snapshot().unwrap(), after);
    assert!(
        SourceCompactIndex::open(
            &temp.root,
            live.suffix.startup.origin.namespace.clone(),
            &after
        )
        .is_err(),
        "same one actual index lock remains owned"
    );
}
#[test]
fn nonempty_live_timer_clock_does_not_reset_after_real_startup_waits() {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let temp = Temp::new();
    let mut live = literal_nonempty_ready(&temp);
    let operation = literal_until_financial_await(&mut live);
    live.append_pending(&operation, None).unwrap();
    live.resolve_post(&operation).unwrap();
    for _ in 0..512 {
        let (stop, _) = live.advance(1).unwrap();
        if stop == Stop::PendingTimer {
            break;
        }
        assert!(
            matches!(stop, Stop::DrainBound | Stop::Quiescent),
            "{stop:?}"
        );
    }
    let fields = live.live.service.kernel_value();
    assert_eq!(fields.get("logicalTime"), V::number(2300.));
    assert_eq!(live.next_timer(), Some(4));
    let timers = arr(fields.get("timers"));
    assert_eq!(timers.len(), 2, "two actual discarded source incoming jobs");
    assert_eq!(timers[0].get("id"), s("4"));
    assert_eq!(timers[1].get("id"), s("5"));
    for timer in &timers {
        assert_eq!(timer.get("delay"), V::number(10000.));
        assert_eq!(timer.get("due"), V::number(12300.));
    }
    let before = live.financial_state().unwrap();
    let effects = live.effects().to_vec();
    assert_eq!(
        live.fire_timer(4, 10),
        Err(crate::source_main_step_kernel::MainStepError::WrongGeneration)
    );
    assert_eq!(live.financial_state().unwrap(), before);
    assert_eq!(live.effects(), effects);
    assert_eq!(live.next_timer(), Some(4));
}
#[test]
fn nonempty_actual_route_rejects_equal_logical_operation_from_other_owner() {
    let ta = Temp::new();
    let tb = Temp::new();
    let mut a = nonempty_through_index(&ta, "first10/01-native-bootstrap-before500")
        .into_live_initialize()
        .unwrap();
    let mut b = nonempty_through_index(&tb, "first10/01-native-bootstrap-before500")
        .into_live_initialize()
        .unwrap();
    let pa = advance_until_nonempty_await(&mut a);
    let pb = advance_until_nonempty_await(&mut b);
    assert_ne!(pa, pb, "same logical args are not an actual receiver route");
    let before = b.financial_state().unwrap();
    let effects = b.effects().to_vec();
    let pin = b.suffix.startup.index.snapshot().unwrap();
    assert_eq!(
        b.append_pending(&pa, None),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(b.resolve_post(&pa), Err(SourceRuleError::InvalidInput));
    assert_eq!(b.financial_state().unwrap(), before);
    assert_eq!(b.effects(), effects);
    assert_eq!(b.suffix.startup.index.snapshot().unwrap(), pin);
    assert_eq!(b.pending_operations(), vec![pb]);
}
