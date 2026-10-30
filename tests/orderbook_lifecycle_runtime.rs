#[path = "orderbook_lifecycle_transport.rs"]
mod wire;
use tradelayer_core::contract_queue_source_owner::OrderbookHelperEntryV1;
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceCall, SourceOutcome, SourceRuleError, SourceValue as V,
};
use wire::{assert_value, items, records, source_outcome, source_rows, text};

fn returned(value: DependencyReturn) -> V {
    match value {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v,
        other => panic!("{other:?}"),
    }
}
fn source(value: DependencyReturn) -> SourceOutcome {
    match value {
        DependencyReturn::Pending(v) => SourceOutcome::Returned(v),
        DependencyReturn::PendingThrow(e) => SourceOutcome::Thrown(e),
        other => panic!("async source transport: {other:?}"),
    }
}
fn setup(record: &V) -> (SourceContractRuntime, u64, OrderbookHelperEntryV1) {
    let input = record.get("rawInput");
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12);
    for table in items(input.get("rawHarnessConstructorInput")) {
        runtime
            .import_collection(&text(table.get("name")), items(table.get("rows")))
            .unwrap();
    }
    runtime
        .import_collection("orderBooks", items(input.get("initialRows")))
        .unwrap();
    let handle = returned(
        runtime
            .invoke_dependency(&DependencyCall {
                target: "Orderbook.newInstance".into(),
                arguments: items(input.get("constructor")),
            })
            .unwrap(),
    );
    let V::Number(bits) = handle.get("orderbookInstance") else {
        panic!("actual constructor handle")
    };
    let id = f64::from_bits(bits) as u64;
    let entry = if input.get("environmentInput").get("preload") == V::String("orderbook".into()) {
        OrderbookHelperEntryV1::OrderbookFirst
    } else {
        OrderbookHelperEntryV1::MainSubgraph
    };
    if input.get("precedingCancel") != V::Undefined {
        runtime
            .invoke_orderbook_lifecycle_helper(
                id,
                entry,
                &SourceCall {
                    method: "cancelContractOrdersForSize".into(),
                    arguments: items(input.get("precedingCancel")),
                },
            )
            .unwrap();
    }
    let failure = input.get("failure");
    if failure != V::Undefined || input.get("factoryFailure") == V::Bool(true) {
        runtime
            .fail_storage(StorageFailure {
                collection: "orderBooks".into(),
                operation: if failure == V::Undefined {
                    "getDatabase".into()
                } else {
                    format!("{}Async", text(failure.get("op")))
                },
                skip: 0,
                message: if failure == V::Undefined {
                    "actual factory failure".into()
                } else {
                    text(failure.get("message"))
                },
            })
            .unwrap();
    }
    runtime.take_effects();
    (runtime, id, entry)
}
#[test]
fn actual_same_core_queue_registry_and_tally_match_all_original_helper_scenes() {
    let mut count = 0;
    for record in records() {
        let (mut runtime, id, entry) = setup(&record);
        let input = record.get("rawInput");
        let tally = runtime.tally_cache();
        let actual = source(
            runtime
                .invoke_orderbook_lifecycle_helper(
                    id,
                    entry,
                    &SourceCall {
                        method: text(input.get("method")),
                        arguments: items(input.get("args")),
                    },
                )
                .unwrap(),
        );
        assert_eq!(
            actual,
            source_outcome(&record),
            "{}",
            text(record.get("name"))
        );
        assert_value(
            &V::Array(runtime.collection("orderBooks").to_vec()),
            &V::Array(source_rows(&record, "orderBooks")),
            "persisted book rows",
        );
        let mut instance = items(record.get("after").get("instances"))[0].clone();
        if let V::Object(fields) = &mut instance {
            fields.retain(|(key, _)| key != "id");
        }
        assert_value(
            &runtime.queue().instance_value(id).unwrap(),
            &instance,
            "same receiver cache",
        );
        for table in items(record.get("after").get("tables")) {
            assert_value(
                &V::Array(runtime.collection(&text(table.get("name"))).to_vec()),
                &table.get("rows"),
                "complete persisted source table",
            );
        }
        let effects = runtime.take_effects();
        let selected = effects
            .into_iter()
            .filter(|e| matches!(e.get("kind"),V::String(ref k)if k=="database"||k=="db"))
            .collect::<Vec<_>>();
        assert_value(
            &V::Array(selected),
            &record.get("effects"),
            "complete ordered database effects",
        );
        assert_eq!(
            runtime.tally_cache(),
            tally,
            "source stops at unbound globals before any tally mutation"
        );
        assert_eq!(
            record.get("after").get("tally"),
            record.get("before").get("tally")
        );
        let state = runtime.state_value().unwrap();
        let expected = record.get("after").get("contract").get("contractSeries");
        assert_value(
            &state.get("globalClearing").get("contractSeries"),
            &if expected == V::Undefined {
                V::Array(vec![])
            } else {
                expected
            },
            "actual registry cache",
        );
        count += 1;
    }
    assert_eq!(count, 35);
}
#[test]
fn new_explicit_entry_keeps_legacy_default_and_invoke_admission_unchanged() {
    let call = SourceCall {
        method: "getAddressOrders".into(),
        arguments: vec![V::String("alice".into()), V::Bool(false)],
    };
    let mut default = SourceContractRuntime::new();
    let before = default.state_value().unwrap();
    assert_eq!(
        default.invoke_orderbook_lifecycle_helper(1, OrderbookHelperEntryV1::MainSubgraph, &call),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(default.state_value().unwrap(), before);
    let record = records().remove(0);
    let (mut runtime, id, _) = setup(&record);
    let before = runtime.state_value().unwrap();
    assert!(runtime
        .invoke_dependency(&DependencyCall {
            target: "Orderbook.getAddressOrders".into(),
            arguments: vec![
                V::Object(vec![("orderbookInstance".into(), V::number(id as f64))]),
                V::String("alice".into()),
                V::Bool(false)
            ]
        })
        .is_err());
    assert_eq!(runtime.state_value().unwrap(), before);
}
