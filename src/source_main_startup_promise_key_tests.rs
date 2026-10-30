use crate::source_main_step_kernel::startup_source_promise_queue::{
    ActualPayloadBinding, ActualSettlement, Realm, StartupPromiseQueue,
};
use crate::validity_source::{SourceRuleError, SourceValue as V};
use std::collections::BTreeMap;
#[test]
fn all_opaque_key_visitors_validate_same_owner_and_stream_every_identity_component() {
    let mut queue = StartupPromiseQueue::new(8).unwrap();
    let job = queue.allocate_source_job("Main.loadTrackHeight").unwrap();
    let binding = ActualPayloadBinding {
        job,
        method_scope: "Main.loadTrackHeight",
        source_site: "query",
        lane: "Main",
        await_ordinal: 1,
    };
    let payload = queue.reserve_actual_payload(binding).unwrap();
    let route = queue.reserve_source_continuation(binding).unwrap();
    let promise = queue.create(Realm::SourceBusiness).unwrap();
    let mut fields = BTreeMap::new();
    let mut emit = |p: &str, v: &V| {
        assert!(fields.insert(p.to_string(), v.clone()).is_none());
        Ok(())
    };
    queue
        .visit_promise_key("promise", promise, &mut emit)
        .unwrap();
    queue
        .visit_payload_key("payload", payload, &mut emit)
        .unwrap();
    queue.visit_source_job_key("job", job, &mut emit).unwrap();
    queue
        .visit_continuation_key("route", route, &mut emit)
        .unwrap();
    assert_eq!(fields.len(), 12);
    for prefix in ["promise", "payload", "job", "route"] {
        assert_eq!(
            fields.get(&format!("{prefix}/generation")),
            Some(&V::String("8".into()))
        );
        assert_eq!(
            fields.get(&format!("{prefix}/owner")),
            fields.get("promise/owner")
        );
    }
    let foreign = StartupPromiseQueue::new(8).unwrap();
    let mut visited = false;
    let mut deny = |_: &str, _: &V| {
        visited = true;
        Ok(())
    };
    assert_eq!(
        foreign.visit_promise_key("x", promise, &mut deny),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.visit_payload_key("x", payload, &mut deny),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.visit_source_job_key("x", job, &mut deny),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        foreign.visit_continuation_key("x", route, &mut deny),
        Err(SourceRuleError::InvalidInput)
    );
    assert!(!visited, "foreign references emit no partial fields");
}
#[test]
fn selected_source_delivery_is_exact_and_readonly_until_same_ticket_acknowledges_it() {
    let mut queue = StartupPromiseQueue::new(8).unwrap();
    let promise = queue.create(Realm::SourceBusiness).unwrap();
    queue
        .resolve_actual(promise, &ActualSettlement::Fulfilled(V::Null))
        .unwrap();
    queue
        .await_actual(Realm::SourceBusiness, promise, 1)
        .unwrap();
    assert!(queue.actual_delivery().is_none());
    let delivery = queue.next_delivery().unwrap().unwrap();
    assert_eq!(queue.actual_delivery(), Some(&delivery));
    let mut calls = 0;
    assert_eq!(
        queue.visit_promise_key("p", promise, &mut |_, _| {
            calls += 1;
            Err(SourceRuleError::Backend)
        }),
        Err(SourceRuleError::Backend)
    );
    assert_eq!(calls, 1);
    assert_eq!(queue.actual_delivery(), Some(&delivery));
    queue.acknowledge(delivery.ticket).unwrap();
    assert!(queue.actual_delivery().is_none());
}
