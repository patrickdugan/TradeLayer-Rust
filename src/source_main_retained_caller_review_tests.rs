// Additive private-Core controls. These exercise the existing receiver-local
// selector contract; they do not promote selectors into native capabilities.
use super::*;

fn observed(c: &SourceMainNativeRetainedCallerV1) -> Vec<(String, V)> {
    let mut fields = Vec::new();
    c.visit_canonical_fields(&mut |path, value| {
        fields.push((path.to_owned(), value.clone()));
        Ok(())
    })
    .unwrap();
    fields
}

fn started(temp: &Temp, case: &str, scope: u64) -> SourceMainNativeRetainedCallerV1 {
    let (owner, documents) = setup(temp, case);
    let expected = owner.expectation().unwrap();
    owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::SyncIndex {
                max: V::number(10.),
            },
            scope,
            71,
            documents,
        )
        .unwrap()
}

fn detached_write(c: &mut SourceMainNativeRetainedCallerV1) {
    assert_eq!(
        drive_to_boundary(c),
        NativeMainRetainedCallerStopV1::CallerReturned
    );
    assert_eq!(
        c.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(c.pending_write().is_none());
    assert_eq!(
        c.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    assert_eq!(
        c.pending_descriptor().unwrap().value().get("role"),
        s("Detached")
    );
}

#[test]
fn equal_receiver_local_selectors_still_append_only_the_receivers_own_actual_plan() {
    let a_temp = Temp::new();
    let b_temp = Temp::new();
    let mut a = started(&a_temp, "sync-final-first/fulfill", 501);
    let mut b = started(&b_temp, "sync-final-first/fulfill", 501);
    detached_write(&mut a);
    detached_write(&mut b);
    let selector_from_a = a.pending_descriptor().unwrap();
    assert_eq!(Some(selector_from_a.clone()), b.pending_descriptor());
    assert_ne!(
        a.owner.index.native_store_context().unwrap(),
        b.owner.index.native_store_context().unwrap()
    );
    let a_before = observed(&a);
    let b_before = b.owner.index.snapshot().unwrap();
    let b_financial = b.financial_state().unwrap();
    assert_eq!(
        b.append_pending(&selector_from_a, None),
        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(observed(&a), a_before);
    assert_ne!(b.owner.index.snapshot().unwrap(), b_before);
    assert_eq!(b.financial_state().unwrap(), b_financial);
    assert_eq!(
        b.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(
        b.resolve_post(&selector_from_a),
        crate::source_contract_runtime::NativeMainRowStopV1::Complete
    );
    assert_eq!(observed(&a), a_before);
    assert!(a.pending_write().is_some());
}

#[test]
fn different_scope_selector_refuses_before_actual_append_or_source_resume() {
    let a_temp = Temp::new();
    let b_temp = Temp::new();
    let mut a = started(&a_temp, "sync-final-first/fulfill", 501);
    let mut b = started(&b_temp, "sync-final-first/fulfill", 502);
    detached_write(&mut a);
    detached_write(&mut b);
    let foreign = a.pending_descriptor().unwrap();
    let actual = b.pending_descriptor().unwrap();
    assert_ne!(foreign, actual);
    let before = observed(&b);
    for operation in [false, true] {
        let result = if operation {
            b.resolve_post(&foreign)
        } else {
            b.append_pending(&foreign, None)
        };
        assert_eq!(
            result,
            crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
                SourceRuleError::InvalidInput
            )
        );
        assert_eq!(observed(&b), before);
    }
    assert_eq!(
        b.append_pending(&actual, None),
        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        b.resolve_post(&actual),
        crate::source_contract_runtime::NativeMainRowStopV1::Complete
    );
}

#[test]
fn actual_awaited_selector_cannot_route_the_detached_child_at_the_same_caller_scope() {
    let a_temp = Temp::new();
    let b_temp = Temp::new();
    let mut a = started(&a_temp, "sync-save-second/fulfill", 501);
    let mut b = started(&b_temp, "sync-final-first/fulfill", 501);
    assert_eq!(
        a.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    let awaited = a.pending_descriptor().unwrap();
    assert_eq!(awaited.value().get("role"), s("Awaited"));
    assert!(a.caller_result().is_none());
    detached_write(&mut b);
    assert_ne!(Some(awaited.clone()), b.pending_descriptor());
    let before = observed(&b);
    assert_eq!(
        b.append_pending(&awaited, None),
        crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
            SourceRuleError::InvalidInput
        )
    );
    assert_eq!(observed(&b), before);
    assert_eq!(
        b.resolve_post(&awaited),
        crate::source_contract_runtime::NativeMainRowStopV1::SourceStopped(
            SourceRuleError::InvalidInput
        )
    );
    assert_eq!(observed(&b), before);
    assert!(a.pending_write().is_some());
    assert!(a.caller_result().is_none());
}
