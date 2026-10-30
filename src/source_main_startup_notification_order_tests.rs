use crate::source_main_step_kernel::startup_source_promise_queue::{
    ActualSettlement, Notification, Realm, StartupPromiseQueue,
};
use crate::source_main_step_kernel::MainStepLegacyThrow;
use crate::validity_source::{SourceException, SourceValue as V};

#[test]
fn unchanged_default_canonical_stream_preserves_exact_field_order() {
    // Ordered comparison with the actual pre-mount committed Kernel capture.
    let kernel = crate::source_main_step_kernel::MainStepKernelV1::new(9);
    assert_eq!(
        super::default_kernel_observation(&kernel),
        include_str!("../fixtures/startup_kernel_default_v1.txt")
    );
}

#[test]
fn reaction_capacity_stop_retains_inflight_adoption_and_original_pending_payload() {
    let mut queue = StartupPromiseQueue::new(9).unwrap();
    let source = queue.create(Realm::SourceBusiness).unwrap();
    let destination = queue.create(Realm::SourceBusiness).unwrap();
    for continuation in 1..=4096 {
        queue
            .await_actual(Realm::SourceBusiness, source, continuation)
            .unwrap();
    }
    queue.adopt(destination, source).unwrap();
    assert_eq!(
        queue.next_delivery(),
        Err(crate::validity_source::SourceRuleError::Capacity)
    );
    let mut fields = std::collections::BTreeMap::new();
    queue
        .visit_fields(&mut |path, value| {
            fields.insert(path.to_owned(), value.clone());
            Ok(())
        })
        .unwrap();
    assert_eq!(fields.get("inflight/present"), Some(&V::Bool(true)));
    assert_eq!(
        fields.get("inflight/kind"),
        Some(&V::String("Assimilate".into()))
    );
    assert_eq!(fields.get("active/present"), Some(&V::Bool(false)));
    assert_eq!(
        fields.get("promises/0/reactions/count"),
        Some(&V::String("4096".into()))
    );
    assert_eq!(fields.get("promises/0/settled"), Some(&V::Bool(false)));
    assert_eq!(fields.get("promises/1/settled"), Some(&V::Bool(false)));
    assert_eq!(
        fields.get("promises/1/resolverLocked"),
        Some(&V::Bool(true))
    );
    assert_eq!(
        queue.end_turn(),
        Err(crate::validity_source::SourceRuleError::Capacity)
    );
}

fn rejection(queue: &mut StartupPromiseQueue, message: &str) -> ActualSettlement {
    ActualSettlement::Rejected(
        queue
            .actual_rejection(&MainStepLegacyThrow {
                exception: SourceException {
                    name: "Error".into(),
                    message: message.into(),
                },
                code: V::Undefined,
            })
            .unwrap(),
    )
}

#[test]
fn native_unhandled_notifications_follow_rejection_order_not_creation_order() {
    let mut queue = StartupPromiseQueue::new(9).unwrap();
    let first = queue.create(Realm::SourceBusiness).unwrap();
    let second = queue.create(Realm::SourceBusiness).unwrap();
    let second_error = rejection(&mut queue, "rejected-first");
    let first_error = rejection(&mut queue, "rejected-second");
    queue.resolve_actual(second, &second_error).unwrap();
    queue.resolve_actual(first, &first_error).unwrap();
    let actual = queue.end_turn().unwrap();
    let promises: Vec<_> = actual
        .iter()
        .map(|event| match event {
            Notification::Unhandled { promise, .. } => *promise,
            Notification::HandledLater { .. } => panic!("no handlers attached"),
        })
        .collect();
    assert_eq!(promises, vec![second, first]);
}

#[test]
fn native_late_handled_notification_precedes_new_unhandled_rejection() {
    let mut queue = StartupPromiseQueue::new(9).unwrap();
    let old = queue.create(Realm::SourceBusiness).unwrap();
    let old_error = rejection(&mut queue, "old-error");
    queue.resolve_actual(old, &old_error).unwrap();
    assert!(
        matches!(&queue.end_turn().unwrap()[..], [Notification::Unhandled { promise, .. }] if *promise == old)
    );
    queue.await_actual(Realm::SourceBusiness, old, 1).unwrap();
    let delivery = queue.next_delivery().unwrap().unwrap();
    queue.acknowledge(delivery.ticket).unwrap();
    assert!(queue.next_delivery().unwrap().is_none());
    let new = queue.create(Realm::SourceBusiness).unwrap();
    let new_error = rejection(&mut queue, "new-error");
    queue.resolve_actual(new, &new_error).unwrap();
    let actual = queue.end_turn().unwrap();
    assert!(
        matches!(&actual[..], [Notification::HandledLater { promise: first, .. }, Notification::Unhandled { promise: second, .. }] if *first == old && *second == new),
        "actual: {actual:?}"
    );
}
