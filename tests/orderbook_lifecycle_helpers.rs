#[path = "orderbook_lifecycle_transport.rs"]
mod wire;
use tradelayer_core::contract_queue_source_owner::{
    ContractQueueSourceOwner, OrderbookHelperEntryV1, QueueDbFailure,
};
use tradelayer_core::validity_source::{
    SourceCall, SourceException, SourceRuleError, SourceValue as V,
};
use wire::{assert_value, items, records, source_outcome, source_rows, text};

fn owner(record: &V) -> (ContractQueueSourceOwner, u64) {
    let mut queue = ContractQueueSourceOwner::new();
    let input = record.get("rawInput");
    queue.import_rows(items(input.get("initialRows"))).unwrap();
    let id = queue
        .create_instance(items(input.get("constructor"))[0].clone(), None)
        .unwrap();
    if input.get("precedingCancel") != V::Undefined {
        queue
            .evaluate_orderbook_helper_instance(
                id,
                OrderbookHelperEntryV1::MainSubgraph,
                &SourceCall {
                    method: "cancelContractOrdersForSize".into(),
                    arguments: items(input.get("precedingCancel")),
                },
                &mut |_, _| panic!("unreached dependency"),
            )
            .unwrap();
        queue.take_db_calls();
        queue.take_detailed_db_calls().unwrap();
    }
    let failure = input.get("failure");
    if failure != V::Undefined || input.get("factoryFailure") == V::Bool(true) {
        queue
            .fail_database_call(QueueDbFailure {
                operation: if failure == V::Undefined {
                    "getDatabase".into()
                } else {
                    format!("{}Async", text(failure.get("op")))
                },
                skip: 0,
                exception: SourceException {
                    name: "Error".into(),
                    message: if failure == V::Undefined {
                        "actual factory failure".into()
                    } else {
                        text(failure.get("message"))
                    },
                },
            })
            .unwrap();
    }
    (queue, id)
}
fn compare(record: &V) {
    let (mut queue, id) = owner(record);
    let input = record.get("rawInput");
    let call = SourceCall {
        method: text(input.get("method")),
        arguments: items(input.get("args")),
    };
    let outcome = queue
        .evaluate_orderbook_helper_instance(
            id,
            OrderbookHelperEntryV1::MainSubgraph,
            &call,
            &mut |_, _| {
                panic!("default circular/unbound graph has no admitted external margin result")
            },
        )
        .unwrap();
    assert_eq!(
        outcome,
        source_outcome(record),
        "{}",
        text(record.get("name"))
    );
    assert_value(
        &V::Array(queue.rows().to_vec()),
        &V::Array(source_rows(record, "orderBooks")),
        "rows",
    );
    let mut instance = items(record.get("after").get("instances"))[0].clone();
    if let V::Object(fields) = &mut instance {
        fields.retain(|(key, _)| key != "id");
    }
    assert_value(&queue.instance_value(id).unwrap(), &instance, "instance");
    assert_value(
        &V::Array(queue.take_detailed_db_calls().unwrap()),
        &record.get("effects"),
        "complete DB effects",
    );
    let bytes = queue.checkpoint().unwrap();
    let restored = ContractQueueSourceOwner::from_checkpoint(&bytes).unwrap();
    assert_eq!(restored.state_value(), queue.state_value());
}
#[test]
fn actual_default_entry_helpers_match_complete_source_rows_cache_outcomes_and_db_effects() {
    let mut count = 0;
    for record in records()
        .iter()
        .filter(|r| r.get("rawInput").get("environmentInput").get("preload") == V::Undefined)
    {
        compare(record);
        count += 1;
    }
    assert_eq!(count, 31);
}
#[test]
fn first_ten_actual_original_calls_remain_a_separate_exact_proof() {
    let full = include_str!("../fixtures/orderbook_lifecycle_helpers.jsonl");
    let proof = include_str!("../fixtures/orderbook_lifecycle_helpers_proof10.jsonl");
    assert_eq!(
        proof,
        full.lines().take(10).collect::<Vec<_>>().join("\n") + "\n"
    );
    for record in records().iter().take(10) {
        compare(record);
    }
}
#[test]
fn original_method_admission_and_local_refusal_rollback_stay_exact() {
    let record = records()
        .into_iter()
        .find(|r| {
            r.get("name")
                == V::String("orderbook-first/flip-entry-real-margin-before-unbound-tally".into())
        })
        .unwrap();
    let (mut queue, id) = owner(&record);
    let before = queue.state_value();
    let call = SourceCall {
        method: "adjustOrdersForAddress".into(),
        arguments: items(record.get("rawInput").get("args")),
    };
    assert_eq!(
        queue.evaluate_instance(id, &call),
        Err(SourceRuleError::UnknownMethod)
    );
    assert_eq!(
        queue.evaluate_orderbook_helper_instance(
            id,
            OrderbookHelperEntryV1::OrderbookFirst,
            &call,
            &mut |_, _| Err(SourceRuleError::UnknownDependency)
        ),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(queue.state_value(), before);
}
