// Actual-Core continuation regression. No financial backend or poststate input.
use super::*;

fn fields(owner: &SourceMainNativeNullSyncV1) -> Vec<(String, V)> {
    let mut fields = vec![];
    owner
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    fields
}
fn field(values: &[(String, V)], path: &str) -> V {
    values
        .iter()
        .find(|(key, _)| key == path)
        .map_or(V::Undefined, |(_, value)| value.clone())
}
fn begin_live() -> (Temp, SourceMainNativeNullSyncV1, V) {
    let temp = Temp::new();
    let (owner, documents) = setup_null(&temp, "live");
    let expected = owner.expectation().unwrap();
    let owner = owner
        .into_retained_null_sync(
            &expected,
            501,
            71,
            documents,
            NativeMainNullIoProfileV1::default(),
        )
        .unwrap();
    let input = source_record("live", "input");
    let raw = untag(raw_get(input, "rawRPCDocuments"));
    let V::String(id) = raw.get("id") else {
        panic!("raw live transaction id")
    };
    (temp, owner, s(&format!("tx-11-{id}")))
}
fn reach_live(owner: &mut SourceMainNativeNullSyncV1, target: &V) -> NativeMainNullPendingV1 {
    for _ in 0..512 {
        let stop = owner.advance(4096);
        assert!(
            !matches!(stop, NativeMainNullStopV1::Stopped(_)),
            "actual source stop before live validity: {stop:?}"
        );
        for operation in owner.pending_operations() {
            let arguments = operation
                .value()
                .get("operation")
                .get("context")
                .get("logical")
                .get("updateArguments");
            if matches!(arguments,V::Array(ref args) if args.first().is_some_and(|query|query.get("_id")==*target))
            {
                return operation;
            }
            assert_eq!(
                owner.append_pending(&operation, None).unwrap(),
                crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
            );
            assert_eq!(
                owner.resolve_post(&operation).unwrap(),
                crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
            );
        }
    }
    panic!("the same source owner did not reach the actual live validity await")
}

#[test]
fn exact_held_validity_retains_borrowed_header_until_real_block_finishes_then_closes_once_with_timer_pending(
) {
    let (temp, mut owner, target) = begin_live();
    let operation = reach_live(&mut owner, &target);
    assert_eq!(operation.job(), NativeMainNullJobV1::DetachedInit);
    assert_eq!(
        owner.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert_eq!(
        owner.detached_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    let held = fields(&owner);
    assert_eq!(field(&held, "headerClosed"), V::Bool(false));
    assert_eq!(
        field(&held, "job/DetachedInit/service/block/present"),
        V::Bool(true)
    );
    assert_eq!(
        field(&held, "job/DetachedInit/service/block/invocation/present"),
        V::Bool(false)
    );
    assert_eq!(
        field(&held, "job/DetachedInit/service/borrowedInvocation"),
        s("TL-MAIN-RETAINED-CALLER-CONSENSUS-13-1")
    );
    let before = owner.index().capture_verified_history_pin().unwrap();
    let before_effects = owner.effects().to_vec();
    assert_eq!(
        owner.advance(4096),
        NativeMainNullStopV1::PendingNativeWrites
    );
    assert_eq!(owner.effects(), before_effects);
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &before
    )
    .is_err());

    assert_eq!(
        owner.append_pending(&operation, None).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    );
    let post = owner.index().capture_verified_history_pin().unwrap();
    assert_ne!(post, before);
    // A physical Post is still the SAME unresolved source await. Draining does
    // not reexecute it, close the header or manufacture source fulfillment.
    let after_append_effects = owner.effects().to_vec();
    assert_eq!(
        owner.advance(4096),
        NativeMainNullStopV1::PendingNativeWrites
    );
    assert_eq!(field(&fields(&owner), "headerClosed"), V::Bool(false));
    assert_eq!(owner.effects(), after_append_effects);
    assert_eq!(owner.index().capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        owner.resolve_post(&operation).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
    );
    assert_eq!(
        owner.resolve_post(&operation),
        Err(SourceRuleError::InvalidInput)
    );

    let mut reached_timer = false;
    for _ in 0..512 {
        let stop = owner.advance(1);
        assert!(
            !matches!(stop, NativeMainNullStopV1::Stopped(_)),
            "actual resumed source stopped: {stop:?}"
        );
        let current = fields(&owner);
        let has_block = field(&current, "job/DetachedInit/service/block/present") == V::Bool(true);
        let has_completion =
            field(&current, "job/DetachedInit/service/completedBlock/present") == V::Bool(true);
        if has_block || has_completion {
            assert_eq!(
                field(&current, "headerClosed"),
                V::Bool(false),
                "a real borrowed block/completion still owns work"
            );
        }
        for pending in owner.pending_operations() {
            assert_ne!(
                pending, operation,
                "the resolved source await cannot be repeated"
            );
            assert_eq!(
                owner.append_pending(&pending, None).unwrap(),
                crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
            );
            assert_eq!(
                owner.resolve_post(&pending).unwrap(),
                crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
            );
        }
        if stop == NativeMainNullStopV1::PendingTimer {
            assert!(!has_block && !has_completion);
            assert_eq!(field(&current, "headerClosed"), V::Bool(true));
            assert_eq!(
                field(&current, "job/DetachedInit/service/borrowedInvocation"),
                V::Undefined
            );
            reached_timer = true;
            break;
        }
    }
    assert!(
        reached_timer,
        "the actual finished block must hand off and close without waiting for its future timer"
    );
    let exact = owner.index().capture_verified_history_pin().unwrap();
    let final_effects = owner.effects().to_vec();
    let final_financial = owner.financial_state().unwrap();
    for _ in 0..3 {
        assert_eq!(owner.advance(4096), NativeMainNullStopV1::PendingTimer);
        assert_eq!(field(&fields(&owner), "headerClosed"), V::Bool(true));
        assert_eq!(owner.index().capture_verified_history_pin().unwrap(), exact);
        assert_eq!(owner.effects(), final_effects);
        assert_eq!(owner.financial_state().unwrap(), final_financial);
    }
}
