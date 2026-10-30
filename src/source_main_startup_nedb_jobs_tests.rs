use crate::source_main_step_kernel::{
    startup_source_nedb_jobs::{Collection, DeliveryKind, QueryKind, StartupNedbQueue},
    startup_source_promise_queue::{ActualPayloadBinding, ActualPayloadRef, StartupPromiseQueue},
    MainStepKernelV1,
};
use crate::validity_source::{SourceRuleError, SourceValue as V};
use std::collections::BTreeMap;
fn reserve(queue: &mut StartupPromiseQueue, ordinal: u64) -> ActualPayloadRef {
    let job = queue.allocate_source_job("Main.readHeight").unwrap();
    queue
        .reserve_actual_payload(ActualPayloadBinding {
            job,
            method_scope: "Main.loadTrackHeight",
            source_site: "findOneAsync",
            lane: "Main",
            await_ordinal: ordinal,
        })
        .unwrap()
}
fn fields(queue: &StartupNedbQueue) -> BTreeMap<String, V> {
    let mut fields = BTreeMap::new();
    queue
        .visit_fields(&mut |p, v| {
            assert!(fields.insert(p.into(), v.clone()).is_none());
            Ok(())
        })
        .unwrap();
    fields
}
#[test]
fn actual_collection_workers_interleave_but_same_database_waits_for_callback_queue_next() {
    let mut kernel = MainStepKernelV1::new(8);
    kernel.startup_install_source_promises().unwrap();
    let (promises, db) = kernel.startup_source_parts().unwrap();
    let a1 = reserve(promises, 1);
    let a2 = reserve(promises, 2);
    let b1 = reserve(promises, 3);
    let first = db
        .enqueue_actual_request(
            promises,
            a1,
            Collection::Consensus,
            QueryKind::FindOneStringId,
        )
        .unwrap();
    let second = db
        .enqueue_actual_request(
            promises,
            a2,
            Collection::Consensus,
            QueryKind::FindOneStringId,
        )
        .unwrap();
    let third = db
        .enqueue_actual_request(
            promises,
            b1,
            Collection::TallyMap,
            QueryKind::FindOneStringId,
        )
        .unwrap();
    let mut events = vec![];
    while let Some(delivery) = db.next_delivery().unwrap() {
        let name = if delivery.request == first {
            "A1"
        } else if delivery.request == second {
            "A2"
        } else {
            assert_eq!(delivery.request, third);
            "B1"
        };
        events.push(format!(
            "{}:{name}",
            if delivery.kind == DeliveryKind::SampleCandidates {
                "sample"
            } else {
                "callback"
            }
        ));
        assert_eq!(
            db.binding(delivery.request).unwrap().source_site,
            "findOneAsync"
        );
        db.acknowledge_actual(delivery).unwrap();
    }
    assert_eq!(
        events,
        vec![
            "sample:A1",
            "sample:B1",
            "callback:A1",
            "callback:B1",
            "sample:A2",
            "callback:A2"
        ]
    );
    let fixture = V::from_json(
        &crate::issuance_intent::JsonValue::parse(include_str!(
            "../fixtures/startup_nedb_executor_reference.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        V::Array(events.into_iter().map(V::String).collect()),
        fixture.get("independentWorkers")
    );
}
#[test]
fn callback_queues_worker_next_before_source_handler_submits_new_database_requests() {
    let mut promises = StartupPromiseQueue::new(8).unwrap();
    let mut db = StartupNedbQueue::new(8).unwrap();
    let a1 = reserve(&mut promises, 1);
    let a2 = reserve(&mut promises, 2);
    let b1 = reserve(&mut promises, 3);
    let first = db
        .enqueue_actual_request(
            &promises,
            a1,
            Collection::Consensus,
            QueryKind::FindOneStringId,
        )
        .unwrap();
    let second = db
        .enqueue_actual_request(
            &promises,
            a2,
            Collection::Consensus,
            QueryKind::FindOneStringId,
        )
        .unwrap();
    let third = db
        .enqueue_actual_request(
            &promises,
            b1,
            Collection::TallyMap,
            QueryKind::FindOneStringId,
        )
        .unwrap();
    let mut names = vec![(first, "A1"), (second, "A2"), (third, "B1")];
    let mut events = vec![];
    while let Some(delivery) = db.next_delivery().unwrap() {
        let name = names
            .iter()
            .find(|(key, _)| key == &delivery.request)
            .unwrap()
            .1;
        events.push(format!(
            "{}:{name}",
            if delivery.kind == DeliveryKind::SampleCandidates {
                "sample"
            } else {
                "callback"
            }
        ));
        if delivery.request == first && delivery.kind == DeliveryKind::DeliverCallback {
            let a3 = reserve(&mut promises, 4);
            let c1 = reserve(&mut promises, 5);
            names.push((
                db.enqueue_actual_request(
                    &promises,
                    a3,
                    Collection::Consensus,
                    QueryKind::FindOneStringId,
                )
                .unwrap(),
                "A3",
            ));
            names.push((
                db.enqueue_actual_request(&promises, c1, Collection::TxIndex, QueryKind::FindAll)
                    .unwrap(),
                "C1",
            ));
        }
        db.acknowledge_actual(delivery).unwrap();
    }
    let fixture = V::from_json(
        &crate::issuance_intent::JsonValue::parse(include_str!(
            "../fixtures/startup_nedb_executor_reference.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        V::Array(events.into_iter().map(V::String).collect()),
        fixture.get("callbackEnqueues")
    );
}
#[test]
fn actual_sample_and_callback_tickets_cannot_be_replayed_or_cross_owner_delivered() {
    let mut promises = StartupPromiseQueue::new(8).unwrap();
    let payload = reserve(&mut promises, 1);
    let mut db = StartupNedbQueue::new(8).unwrap();
    let mut foreign = StartupNedbQueue::new(8).unwrap();
    db.enqueue_actual_request(
        &promises,
        payload,
        Collection::Consensus,
        QueryKind::FindOneStringId,
    )
    .unwrap();
    let delivery = db.next_delivery().unwrap().unwrap();
    let before = fields(&foreign);
    assert_eq!(
        foreign.acknowledge_actual(delivery),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&foreign), before);
    db.acknowledge_actual(delivery).unwrap();
    let before = fields(&db);
    assert_eq!(
        db.acknowledge_actual(delivery),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&db), before);
    let callback = db.next_delivery().unwrap().unwrap();
    assert_eq!(callback.kind, DeliveryKind::DeliverCallback);
    db.acknowledge_actual(callback).unwrap();
    assert!(db.next_delivery().unwrap().is_none());
}
#[test]
fn metadata_namespace_substitution_and_duplicate_or_completed_payload_refuse_before_mutation() {
    let mut promises = StartupPromiseQueue::new(8).unwrap();
    let payload = reserve(&mut promises, 1);
    let mut db = StartupNedbQueue::new(8).unwrap();
    db.enqueue_actual_request(
        &promises,
        payload,
        Collection::Consensus,
        QueryKind::FindOneStringId,
    )
    .unwrap();
    let before = fields(&db);
    assert_eq!(
        db.enqueue_actual_request(
            &promises,
            payload,
            Collection::Consensus,
            QueryKind::FindOneStringId
        ),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&db), before);
    let mut other = StartupPromiseQueue::new(8).unwrap();
    let other_payload = reserve(&mut other, 1);
    assert_eq!(
        db.enqueue_actual_request(
            &other,
            other_payload,
            Collection::Consensus,
            QueryKind::FindOneStringId
        ),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&db), before);
    let ready = reserve(&mut promises, 2);
    promises.mark_actual_payload_ready(ready).unwrap();
    assert_eq!(
        db.enqueue_actual_request(
            &promises,
            ready,
            Collection::Consensus,
            QueryKind::FindOneStringId
        ),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&db), before);
}
#[test]
fn owner_stop_retains_selected_real_job_and_denies_any_more_sampling_or_callbacks() {
    let mut promises = StartupPromiseQueue::new(8).unwrap();
    let payload = reserve(&mut promises, 1);
    let mut db = StartupNedbQueue::new(8).unwrap();
    db.enqueue_actual_request(
        &promises,
        payload,
        Collection::Consensus,
        QueryKind::FindOneStringId,
    )
    .unwrap();
    let delivery = db.next_delivery().unwrap().unwrap();
    let before = fields(&db);
    db.stop_after_owner_prefix(SourceRuleError::Capacity);
    assert_eq!(db.next_delivery(), Err(SourceRuleError::Capacity));
    assert_eq!(
        db.acknowledge_actual(delivery),
        Err(SourceRuleError::Capacity)
    );
    let after = fields(&db);
    assert_eq!(before.get("active"), after.get("active"));
    assert_eq!(
        before.get("requests/0/phase"),
        after.get("requests/0/phase")
    );
}
