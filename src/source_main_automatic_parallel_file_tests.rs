#[test]
fn automatic_external_file_timer_runs_while_same_financial_native_await_is_held() {
    use crate::source_contract_runtime::NativeMainServiceStopV1 as Stop;
    let t = Temp::new();
    let mut c = start(&t, "parallel");
    assert_eq!(drive(&mut c, false), Stop::PendingTimer);
    compare(&c, "parallel", "same-file-child-held");
    pause_height(&mut c, "parallel");
    assert_eq!(file_count(&c, "copyFile"), 3);
    compare(&c, "parallel", "pause-height-same-file-child");
    fire(&mut c);
    assert_eq!(drive(&mut c, true), Stop::AwaitNativeWrite);
    compare(
        &c,
        "parallel",
        "same-native-validity-held-after-real-finance",
    );
    let plan = c.pending_write().unwrap() as *const _;
    let before = c.active.owner.index.snapshot().unwrap();
    let financial = c.financial_state().unwrap().get("tables");
    let external_timer = c
        .next_timer()
        .expect("actual second file timer remains held");
    fire(&mut c);
    assert_eq!(advance(&mut c), Stop::AwaitNativeWrite);
    assert_eq!(c.pending_write().unwrap() as *const _, plan);
    assert_eq!(c.active.owner.index.snapshot().unwrap(), before);
    assert_eq!(c.financial_state().unwrap().get("tables"), financial);
    assert_eq!(
        file_count(&c, "copyFile"),
        5,
        "the real independent retry must run before native resolution"
    );
    compare(
        &c,
        "parallel",
        "external-file-timer-fired-while-native-validity-held",
    );
    assert!(c.active.frame.as_ref().unwrap().automatic_file_test_provenance_valid(),"every borrowed span is exactly an actual trace's effects, separate from direct Kernel events");
    apply(&mut c, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(advance(&mut c), Stop::AwaitNativeWrite);
    apply(&mut c, NativeMainActiveServiceActionV1::ResolvePost);
    assert_eq!(drive(&mut c, false), Stop::PendingTimer);
    compare(
        &c,
        "parallel",
        "same-native-promise-resolved-parent-recursive-wait",
    );
    assert_eq!(
        c.effects()
            .iter()
            .filter(|e| e.get("kind") == s("timer-fired")
                && e.get("id") == V::number(external_timer as f64))
            .count(),
        1,
        "one actual Kernel timer firing is never re-emitted as a borrowed source effect"
    );
    assert_eq!(
        c.effects()
            .iter()
            .filter(|e| e.get("kind") == s("timer-scheduled"))
            .map(|e| e.get("delay"))
            .collect::<Vec<_>>(),
        vec![V::number(50.), V::number(50.), V::number(1000.)]
    );
}
