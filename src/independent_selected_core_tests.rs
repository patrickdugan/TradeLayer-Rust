// Parent-Core gate only. This review does not compile or substitute a Core.
// The financial owner is built from the author's unchanged raw-input setup.
use super::*;

#[test]
fn independent_second_native_lost_return_retains_original_action_and_held_source_until_explicit_resolve(
) {
    let temp = Temp::new();
    let mut c = controller(&temp);
    start_delay(&mut c);
    let selected = first_native(&mut c);
    let source_effects = effects(&c).to_vec();
    let financial = state(&c);
    let acknowledged_before = c.history().len();
    fail_native(&mut c, &selected, false);
    let original_result = c.pending.as_ref().unwrap().result.clone();
    assert!(original_result.is_some());
    let original_raw = c.pending.as_ref().unwrap().raw.clone();
    let no_index = c.into_no_native_index().unwrap();
    let mut c = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    // A SECOND actual native attempt is applied, but its return is lost. This
    // must stay on native recovery, never repeat the original source action.
    c.owner
        .as_mut()
        .unwrap()
        .session_mut()
        .index
        .test_fail_after_sync();
    let stopped = c.resume();
    assert!(stopped.is_err(), "{stopped:?}");
    assert_eq!(
        c.phase(),
        RetainedCallerSelectedWalPhaseV1::NativeRecoveryRequired
    );
    assert_eq!(effects(&c), source_effects);
    assert_eq!(state(&c), financial);
    assert_eq!(c.pending.as_ref().unwrap().result, original_result);
    assert_eq!(c.pending.as_ref().unwrap().raw, original_raw);
    assert!(c.pending.as_ref().unwrap().executed);
    assert_eq!(c.history().len(), acknowledged_before);
    assert!(matches!(
        c.begin(Action::Pause),
        Err(RetainedCallerSelectedWalErrorV1::Busy)
    ));
    let no_index = c.into_no_native_index().unwrap();
    let mut c = no_index
        .try_reopen_post(&temp.root, Recovery::RejectIncompleteTail)
        .unwrap();
    let post = index(&c).capture_verified_history_pin().unwrap();
    let io = index(&c).storage_io_counters().unwrap();
    let proof = c.pending.as_ref().unwrap().native_recovery.clone().unwrap();
    let decoded =
        crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&proof).unwrap())
            .unwrap();
    assert_eq!(decoded.get("candidate"), s("Post"));
    assert_ne!(decoded.get("previous"), V::Null);
    c.resume().unwrap();
    assert_eq!(effects(&c), source_effects);
    assert_eq!(state(&c), financial);
    assert_eq!(index(&c).capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        index(&c)
            .storage_io_counters()
            .unwrap()
            .confirmed_append_bytes,
        io.confirmed_append_bytes
    );
    assert_eq!(c.history().len(), acknowledged_before + 1);
    let current = c.pending_operations().into_iter().next().unwrap();
    assert_eq!(c.acknowledged_append, Some(current.value()));
    // Only this distinct, ACK-authorized source action releases the same await.
    action(
        &mut c,
        Action::ResolveSelectedPost {
            selector: current.value(),
        },
    );
    assert!(c.acknowledged_append.is_none());
    let next = first_native(&mut c);
    assert_ne!(next.value(), selected.value());
}
