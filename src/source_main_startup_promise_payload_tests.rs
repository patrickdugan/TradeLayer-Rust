use crate::source_main_step_kernel::startup_source_promise_queue::{
    ActualPayloadBinding, ActualSettlement, Realm, StartupPromiseQueue,
};
use crate::validity_source::{SourceRuleError, SourceValue as V};
use std::collections::BTreeMap;
fn fields(queue: &StartupPromiseQueue) -> BTreeMap<String, V> {
    let mut fields = BTreeMap::new();
    queue
        .visit_fields(&mut |path, value| {
            assert!(fields.insert(path.to_owned(), value.clone()).is_none());
            Ok(())
        })
        .unwrap();
    fields
}
fn actual_binding(queue: &mut StartupPromiseQueue) -> ActualPayloadBinding {
    let job = queue
        .allocate_source_job("Main.constructConsensusFromIndex")
        .unwrap();
    assert_eq!(job.serial(), 1);
    ActualPayloadBinding {
        job,
        method_scope: "TxIndex.findMaxIndexedBlock",
        source_site: "FindMaxQuery",
        lane: "Main",
        await_ordinal: 2,
    }
}
#[test]
fn real_frame_payload_reservation_cannot_settle_before_actual_capture() {
    let mut queue = StartupPromiseQueue::new(8).unwrap();
    let binding = actual_binding(&mut queue);
    let payload = queue.reserve_actual_payload(binding).unwrap();
    let promise = queue.create(Realm::RecordedHostIo).unwrap();
    let before = fields(&queue);
    assert_eq!(
        queue.resolve_actual(promise, &ActualSettlement::Payload(payload)),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        fields(&queue),
        before,
        "refusal cannot lock the real resolver"
    );
    assert_eq!(queue.actual_payload_binding(payload).unwrap(), &binding);
    assert!(!queue.actual_payload_is_ready(payload).unwrap());
    queue.mark_actual_payload_ready(payload).unwrap();
    assert!(queue
        .resolve_actual(promise, &ActualSettlement::Payload(payload))
        .unwrap());
    assert_eq!(
        queue.mark_actual_payload_ready(payload),
        Err(SourceRuleError::InvalidInput)
    );
}
#[test]
fn source_route_aliases_keep_the_exact_actual_payload_through_foreign_realm_adoption() {
    let mut queue = StartupPromiseQueue::new(8).unwrap();
    let binding = actual_binding(&mut queue);
    let payload = queue.reserve_actual_payload(binding).unwrap();
    let host = queue.create(Realm::RecordedHostIo).unwrap();
    let alias = queue.create(Realm::SourceBusiness).unwrap();
    queue.adopt(alias, host).unwrap();
    let first = queue.reserve_source_continuation(binding).unwrap();
    let second_binding = ActualPayloadBinding {
        source_site: "MainAfterFindMax",
        ..binding
    };
    let second = queue.reserve_source_continuation(second_binding).unwrap();
    assert_eq!(first.serial(), 1);
    assert_eq!(second.serial(), 2);
    assert_eq!(
        queue.source_continuation_binding(second).unwrap(),
        &second_binding
    );
    queue
        .await_routed(Realm::SourceBusiness, alias, first)
        .unwrap();
    queue
        .await_routed(Realm::SourceBusiness, host, second)
        .unwrap();
    queue.mark_actual_payload_ready(payload).unwrap();
    queue
        .resolve_actual(host, &ActualSettlement::Payload(payload))
        .unwrap();
    let mut delivered = vec![];
    while let Some(delivery) = queue.next_delivery().unwrap() {
        assert_eq!(delivery.settlement, ActualSettlement::Payload(payload));
        delivered.push(delivery.continuation);
        queue.acknowledge(delivery.ticket).unwrap();
    }
    assert_eq!(delivered, vec![first.serial(), second.serial()]);
    assert_eq!(queue.actual_payload_binding(payload).unwrap(), &binding);
}
#[test]
fn foreign_jobs_payloads_and_routes_refuse_before_any_metadata_mutation() {
    let mut owner = StartupPromiseQueue::new(8).unwrap();
    let binding = actual_binding(&mut owner);
    let payload = owner.reserve_actual_payload(binding).unwrap();
    owner.mark_actual_payload_ready(payload).unwrap();
    let route = owner.reserve_source_continuation(binding).unwrap();
    let mut foreign = StartupPromiseQueue::new(8).unwrap();
    let promise = foreign.create(Realm::SourceBusiness).unwrap();
    let before = fields(&foreign);
    assert_eq!(
        foreign.reserve_actual_payload(binding),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.reserve_source_continuation(binding),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.mark_actual_payload_ready(payload),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.resolve_actual(promise, &ActualSettlement::Payload(payload)),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.await_routed(Realm::SourceBusiness, promise, route),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&foreign), before);
}
#[test]
fn same_site_ordinal_cannot_replace_a_retained_frame_payload_or_hide_it_after_stop() {
    let mut queue = StartupPromiseQueue::new(8).unwrap();
    let binding = actual_binding(&mut queue);
    let payload = queue.reserve_actual_payload(binding).unwrap();
    assert_eq!(
        queue.reserve_actual_payload(binding),
        Err(SourceRuleError::InvalidInput)
    );
    queue.mark_actual_payload_ready(payload).unwrap();
    let before = fields(&queue);
    assert_eq!(
        before.get("payloads/0/sourceSite"),
        Some(&V::String("FindMaxQuery".into()))
    );
    assert_eq!(before.get("payloads/0/ready"), Some(&V::Bool(true)));
    queue.stop_after_owner_prefix(SourceRuleError::Capacity);
    assert_eq!(
        queue.allocate_source_job("Main.incomingLoop"),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(queue.actual_payload_binding(payload).unwrap(), &binding);
    assert!(queue.actual_payload_is_ready(payload).unwrap());
    let after = fields(&queue);
    for (path, value) in before
        .iter()
        .filter(|(path, _)| path.starts_with("payloads/"))
    {
        assert_eq!(after.get(path), Some(value));
    }
}
