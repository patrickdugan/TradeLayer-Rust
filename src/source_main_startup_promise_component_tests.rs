use crate::source_main_step_kernel::startup_source_promise_queue as promise;
use crate::{source_main_step_kernel, validity_source};
use promise::*;
use source_main_step_kernel::MainStepLegacyThrow;
use std::collections::BTreeMap;
use validity_source::{SourceException, SourceRuleError, SourceValue as V};
fn q() -> StartupPromiseQueue {
    StartupPromiseQueue::new(9).unwrap()
}
fn yes(value: f64) -> ActualSettlement {
    ActualSettlement::Fulfilled(V::number(value))
}
fn drain(q: &mut StartupPromiseQueue) -> Vec<(u64, ActualSettlement)> {
    let mut out = vec![];
    while let Some(delivery) = q.next_delivery().unwrap() {
        out.push((delivery.continuation, delivery.settlement.clone()));
        q.acknowledge(delivery.ticket).unwrap();
    }
    out
}
fn fields(q: &StartupPromiseQueue) -> BTreeMap<String, V> {
    let mut out = BTreeMap::new();
    q.visit_fields(&mut |path, v| {
        assert!(out.insert(path.into(), v.clone()).is_none(), "{path}");
        Ok(())
    })
    .unwrap();
    out
}
fn reference(case: &str) -> V {
    include_str!("../fixtures/startup_promise_reference.jsonl")
        .lines()
        .map(|line| V::from_json(&crate::issuance_intent::JsonValue::parse(line).unwrap()).unwrap())
        .find(|value| value.get("case") == V::String(case.into()))
        .unwrap()
        .get("observations")
}
fn observations(values: Vec<(u64, ActualSettlement)>) -> V {
    V::Array(
        values
            .into_iter()
            .map(|(continuation, settlement)| {
                let mut fields = vec![("continuation".into(), V::number(continuation as f64))];
                match settlement {
                    ActualSettlement::Payload(_) => panic!(
                        "native scalar-only reference case produced an opaque backend payload"
                    ),
                    ActualSettlement::Fulfilled(value) => fields.extend([
                        ("kind".into(), V::String("Fulfilled".into())),
                        ("value".into(), value),
                    ]),
                    ActualSettlement::Rejected(error) => fields.extend([
                        ("kind".into(), V::String("Rejected".into())),
                        (
                            "name".into(),
                            V::String(error.thrown().exception.name.clone()),
                        ),
                        (
                            "message".into(),
                            V::String(error.thrown().exception.message.clone()),
                        ),
                        (
                            "code".into(),
                            match &error.thrown().code {
                                V::Undefined => V::Array(vec![V::String("undefined".into())]),
                                code => V::Array(vec![V::String("string".into()), code.clone()]),
                            },
                        ),
                        ("identity".into(), V::number(error.identity() as f64)),
                    ]),
                }
                V::Object(fields)
            })
            .collect(),
    )
}
#[test]
fn foreign_await_queues_assimilation_instead_of_a_fixed_number_of_ticks() {
    let mut q = q();
    let h = q.create(Realm::RecordedHostIo).unwrap();
    let s = q.create(Realm::SourceBusiness).unwrap();
    q.resolve_actual(h, &yes(1.)).unwrap();
    q.resolve_actual(s, &yes(2.)).unwrap();
    let wrapper = q.await_actual(Realm::SourceBusiness, h, 1).unwrap();
    assert_ne!(wrapper, h);
    assert_eq!(q.await_actual(Realm::SourceBusiness, s, 2).unwrap(), s);
    let actual = drain(&mut q);
    assert_eq!(actual, vec![(2, yes(2.)), (1, yes(1.))]);
    assert_eq!(observations(actual), reference("foreign-first"));
    assert!(q.end_turn().unwrap().is_empty());
}
#[test]
fn literal_return_adoption_reactions_interleave_with_ready_source_await() {
    let mut q = q();
    let root = q.create(Realm::SourceBusiness).unwrap();
    let a = q.create(Realm::SourceBusiness).unwrap();
    let b = q.create(Realm::SourceBusiness).unwrap();
    let c = q.create(Realm::SourceBusiness).unwrap();
    q.resolve_actual(root, &yes(7.)).unwrap();
    q.adopt(a, root).unwrap();
    q.adopt(b, a).unwrap();
    q.adopt(c, b).unwrap();
    q.await_actual(Realm::SourceBusiness, c, 3).unwrap();
    q.await_actual(Realm::SourceBusiness, root, 1).unwrap();
    let actual = drain(&mut q);
    assert_eq!(actual, vec![(1, yes(7.)), (3, yes(7.))]);
    assert_eq!(observations(actual), reference("return-adoption-chain"));
}
#[test]
fn adoption_locks_original_resolver_before_pending_child_fulfills() {
    let mut q = q();
    let child = q.create(Realm::RecordedHostIo).unwrap();
    let outer = q.create(Realm::SourceBusiness).unwrap();
    q.adopt(outer, child).unwrap();
    assert!(!q.resolve_actual(outer, &yes(99.)).unwrap());
    q.await_actual(Realm::SourceBusiness, outer, 1).unwrap();
    assert!(drain(&mut q).is_empty());
    q.resolve_actual(child, &yes(7.)).unwrap();
    let actual = drain(&mut q);
    assert_eq!(actual, vec![(1, yes(7.))]);
    assert_eq!(
        observations(actual),
        reference("adoption-first-resolution-wins")
    );
}
#[test]
fn real_rejection_identity_alias_and_late_handler_notifications_are_retained() {
    let mut q = q();
    let p = q.create(Realm::SourceBusiness).unwrap();
    let error = q
        .actual_rejection(&MainStepLegacyThrow {
            exception: SourceException {
                name: "Error".into(),
                message: "actual IO rejection".into(),
            },
            code: V::String("EIO".into()),
        })
        .unwrap();
    let rejected = ActualSettlement::Rejected(error.clone());
    q.resolve_actual(p, &rejected.clone()).unwrap();
    let first = q.end_turn().unwrap();
    assert!(matches!(&first[..],[Notification::Unhandled{rejection,..}] if rejection==&error));
    q.await_actual(Realm::SourceBusiness, p, 1).unwrap();
    q.await_actual(Realm::SourceBusiness, p, 2).unwrap();
    assert_eq!(drain(&mut q), vec![(1, rejected.clone()), (2, rejected)]);
    let later = q.end_turn().unwrap();
    assert!(matches!(&later[..],[Notification::HandledLater{rejection,..}] if rejection==&error));
    assert!(q.end_turn().unwrap().is_empty());
}
#[test]
fn same_generation_foreign_queue_keys_and_errors_refuse_before_any_metadata_mutation() {
    let mut a = q();
    let mut b = q();
    let pa = a.create(Realm::SourceBusiness).unwrap();
    let pb = b.create(Realm::SourceBusiness).unwrap();
    let error = a
        .actual_rejection(&MainStepLegacyThrow {
            exception: SourceException {
                name: "Error".into(),
                message: "foreign".into(),
            },
            code: V::Undefined,
        })
        .unwrap();
    let before = fields(&b);
    assert_eq!(
        b.resolve_actual(pa, &yes(1.)),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(b.adopt(pb, pa), Err(SourceRuleError::InvalidInput));
    assert_eq!(
        b.await_actual(Realm::SourceBusiness, pa, 1),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        b.resolve_actual(pb, &ActualSettlement::Rejected(error)),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&b), before);
}
#[test]
fn actual_delivered_result_stays_owned_through_source_local_stop_and_wrong_ack() {
    let mut q = q();
    let p = q.create(Realm::SourceBusiness).unwrap();
    q.resolve_actual(
        p,
        &ActualSettlement::Fulfilled(V::Object(vec![(
            "original".into(),
            V::String("actual-result".into()),
        )])),
    )
    .unwrap();
    q.await_actual(Realm::SourceBusiness, p, 1).unwrap();
    let delivery = q.next_delivery().unwrap().unwrap();
    let before = fields(&q);
    let mut other = StartupPromiseQueue::new(9).unwrap();
    let foreign = other.create(Realm::SourceBusiness).unwrap();
    other.resolve_actual(foreign, &yes(8.)).unwrap();
    other
        .await_actual(Realm::SourceBusiness, foreign, 1)
        .unwrap();
    let foreign_delivery = other.next_delivery().unwrap().unwrap();
    assert_eq!(
        q.acknowledge(foreign_delivery.ticket),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(fields(&q), before);
    assert_eq!(q.next_delivery(), Err(SourceRuleError::InvalidInput));
    q.stop_after_owner_prefix(SourceRuleError::Capacity);
    assert_eq!(
        q.acknowledge(delivery.ticket),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(q.next_delivery(), Err(SourceRuleError::Capacity));
    let retained = fields(&q);
    assert_eq!(retained.get("active/present"), Some(&V::Bool(true)));
    assert_eq!(
        retained.get("active/settlement/value/0/value"),
        Some(&V::String("actual-result".into()))
    );
}
#[test]
fn capacity_is_a_retained_metadata_stop_and_does_not_synthesize_rejection() {
    let mut q = q();
    for _ in 0..1024 {
        q.create(Realm::SourceBusiness).unwrap();
    }
    assert_eq!(
        q.create(Realm::RecordedHostIo),
        Err(SourceRuleError::Capacity)
    );
    let state = fields(&q);
    assert_eq!(state.get("promises/count"), Some(&V::String("1024".into())));
    assert_eq!(state.get("ready/count"), Some(&V::String("0".into())));
    assert_eq!(q.next_delivery(), Err(SourceRuleError::Capacity));
    assert_eq!(q.end_turn(), Err(SourceRuleError::Capacity));
}
#[test]
fn competing_native_ready_values_match_independent_node_order() {
    let mut q = q();
    let source = q.create(Realm::SourceBusiness).unwrap();
    let host = q.create(Realm::RecordedHostIo).unwrap();
    q.await_actual(Realm::SourceBusiness, source, 1).unwrap();
    q.await_actual(Realm::SourceBusiness, host, 2).unwrap();
    q.resolve_actual(host, &yes(5.)).unwrap();
    q.resolve_actual(source, &yes(9.)).unwrap();
    assert_eq!(
        observations(drain(&mut q)),
        reference("competing-ready-same-and-foreign")
    );
}
#[test]
fn two_catches_and_shared_actual_error_identity_match_node_without_unhandled_notification() {
    let mut q = q();
    let source = q.create(Realm::SourceBusiness).unwrap();
    let error = q
        .actual_rejection(&MainStepLegacyThrow {
            exception: SourceException {
                name: "Error".into(),
                message: "actual IO rejection".into(),
            },
            code: V::String("EIO".into()),
        })
        .unwrap();
    q.resolve_actual(source, &ActualSettlement::Rejected(error))
        .unwrap();
    q.await_actual(Realm::SourceBusiness, source, 1).unwrap();
    q.await_actual(Realm::SourceBusiness, source, 2).unwrap();
    assert_eq!(
        observations(drain(&mut q)),
        reference("two-catches-same-error")
    );
    assert!(q.end_turn().unwrap().is_empty());
}
#[test]
fn late_handler_native_notifications_and_source_callback_match_node() {
    let mut q = q();
    let source = q.create(Realm::SourceBusiness).unwrap();
    let error = q
        .actual_rejection(&MainStepLegacyThrow {
            exception: SourceException {
                name: "Error".into(),
                message: "actual IO rejection".into(),
            },
            code: V::String("EIO".into()),
        })
        .unwrap();
    q.resolve_actual(source, &ActualSettlement::Rejected(error))
        .unwrap();
    assert!(matches!(
        &q.end_turn().unwrap()[..],
        [Notification::Unhandled { .. }]
    ));
    q.await_actual(Realm::SourceBusiness, source, 1).unwrap();
    let V::Array(mut actual) = observations(drain(&mut q)) else {
        unreachable!()
    };
    actual.insert(
        0,
        V::Object(vec![("notification".into(), V::String("Unhandled".into()))]),
    );
    assert!(matches!(
        &q.end_turn().unwrap()[..],
        [Notification::HandledLater { .. }]
    ));
    actual.push(V::Object(vec![(
        "notification".into(),
        V::String("HandledLater".into()),
    )]));
    assert_eq!(V::Array(actual), reference("late-catch-notifications"));
}
#[test]
fn self_cycle_is_actual_native_type_error_not_an_arbitrary_fulfilled_promise_value() {
    let mut q = q();
    let source = q.create(Realm::SourceBusiness).unwrap();
    q.adopt(source, source).unwrap();
    q.await_actual(Realm::SourceBusiness, source, 1).unwrap();
    assert_eq!(
        observations(drain(&mut q)),
        reference("self-cycle-native-type-error")
    );
    assert!(q.end_turn().unwrap().is_empty());
}
