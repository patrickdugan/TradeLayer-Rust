use super::*;
use crate::storage::Namespace;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-row-continuation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        if let Ok(root) = self.0.canonicalize() {
            assert_eq!(root.parent(), Some(parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn invoke(runtime: &mut SourceContractRuntime, target: &str, arguments: Vec<V>) {
    let returned = runtime
        .invoke_dependency(&DependencyCall {
            target: target.into(),
            arguments,
        })
        .unwrap();
    assert!(
        matches!(
            returned,
            DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
        ),
        "{target}: {returned:?}"
    );
    runtime.take_effects();
}
fn row() -> V {
    // cb61 genuine NeDB await oracle Main0 raw transaction, not params/state.
    obj(&[
        (
            "_id",
            s("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8"),
        ),
        (
            "value",
            obj(&[
                (
                    "txId",
                    s("ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8"),
                ),
                ("marker", s("tl")),
                ("payload", s("00,production-code")),
                (
                    "sender",
                    obj(&[
                        ("senderAddress", s("MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB")),
                        ("amount", n(0.)),
                    ]),
                ),
                ("reference", V::Array(vec![])),
            ]),
        ),
    ])
}
fn setup(temp: &Temp) -> (SourceContractRuntime, SourceCompactIndex) {
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12);
    invoke(
        &mut runtime,
        "SourceActivation.configureClient",
        vec![obj(&[("chain", s("LTC")), ("test", V::Bool(false))])],
    );
    invoke(&mut runtime, "Activation.init", vec![s("LTC")]);
    invoke(
        &mut runtime,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
            ),
            (
                "io",
                obj(&[
                    ("height", n(10.)),
                    (
                        "networkInfo",
                        obj(&[("networkactive", V::Bool(true)), ("connections", n(2.))]),
                    ),
                    ("blocks", V::Array(vec![])),
                ]),
            ),
            ("genesisBlock", n(10.)),
        ])],
    );
    runtime.set_height(n(10.)).unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual row raw source commands",
    )
    .unwrap();
    index.import_row(&index.snapshot().unwrap(), row()).unwrap();
    runtime
        .native_main_bind(obj(&[("profile", s("actual-row-test"))]))
        .unwrap();
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.set(0));
    (runtime, index)
}
fn marks(runtime: &SourceContractRuntime) -> usize {
    runtime
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("collection") == s("consensus")
                && effect.get("method") == s("updateAsync")
                && arg(&array(&effect.get("args")).unwrap(), 0).get("_id")
                    == row().get("value").get("txId")
        })
        .count()
}
fn decodes(_runtime: &SourceContractRuntime) -> usize {
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.get())
}
#[test]
fn actual_financial_prefix_stays_borrowed_and_resumes_without_decode_or_mark() {
    let temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert!(
        runtime.core.queue_context.is_some(),
        "SAME queue remains borrowed at source await"
    );
    assert_eq!(marks(&runtime), 1);
    let decoded = decodes(&runtime);
    assert_eq!(decoded, 1);
    assert!(runtime
        .collection("consensus")
        .iter()
        .any(|row| row.get("processed") == V::Bool(true)));
    assert!(
        !runtime.collection("propertyList").is_empty(),
        "actual activation financial prefix already executed"
    );
    let prefix = runtime.collection("consensus").to_vec();
    let before = index.snapshot().unwrap();
    let effects = runtime.core.effects.clone();
    let mut fields = vec![];
    frame
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.contains(&("row/pc".into(), s("PrimaryValidity"))));
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(
        runtime.core.effects, effects,
        "await inspection does not drain or execute"
    );
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(frame.result(), Some(&Ok(DependencyReturn::Pending(n(0.)))));
    assert_eq!(marks(&runtime), 1);
    assert_eq!(decodes(&runtime), decoded);
    assert_eq!(runtime.collection("consensus"), prefix);
    assert!(runtime.core.queue_context.is_none());
    assert_eq!(frame.consumed_rows(), 1);
    assert_eq!(frame.validity().len(), 1);
    let retry = runtime.native_main_rows(&mut index, V::Array(vec![row()]), n(10.), false);
    assert_eq!(
        retry.result,
        Ok(DependencyReturn::Pending(n(1.))),
        "whole invocation retry changes source skip count"
    );
}
#[test]
fn exact_native_post_resolves_the_stored_await_without_second_append() {
    let temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    // Actual native commit has happened but its original source await is parked.
    index
        .commit_prepared_validity(frame.pending_write().unwrap())
        .unwrap();
    let post = index.snapshot().unwrap();
    let effects = runtime.core.effects.clone();
    assert_eq!(
        runtime.native_main_rows_resolve_post(&index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(index.snapshot().unwrap(), post);
    assert_eq!(runtime.core.effects, effects);
    assert_eq!(frame.result(), Some(&Ok(DependencyReturn::Pending(n(0.)))));
    assert_eq!(marks(&runtime), 1);
}
#[test]
fn known_update_throw_enters_literal_fatal_mark_and_second_validity_await() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for runtime in [&mut runtime, &mut legacy] {
        runtime
            .fail_storage(StorageFailure {
                collection: "txIndex".into(),
                operation: "updateAsync".into(),
                skip: 0,
                message: "controlled-real-source-validity-rejection".into(),
            })
            .unwrap();
    }
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(marks(&runtime), 2);
    assert_eq!(frame.validity().len(), 1);
    let set = frame.pending_write().unwrap().update_arguments()[1].get("$set");
    assert_eq!(set.get("valid"), V::Bool(false));
    assert_eq!(
        set.get("reason"),
        s("Fatal tx processing error: controlled-real-source-validity-rejection")
    );
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected = legacy.native_main_rows(&mut legacy_index, V::Array(vec![row()]), n(10.), false);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(frame.validity().len(), expected.validity.len());
    assert_eq!(runtime.core.effects, legacy.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        legacy.native_main_state_value().unwrap()
    );
}
#[test]
fn prefix_capacity_preserves_unchanged_legacy_stop_and_borrowed_queue() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        owner.core.effects = vec![V::Undefined; 65_535];
    }
    let before = index.snapshot().unwrap();
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::SourceStopped(SourceRuleError::Capacity)
    );
    let actual_decodes = decodes(&runtime);
    let expected = legacy.native_main_rows(&mut legacy_index, V::Array(vec![row()]), n(10.), false);
    assert_eq!(expected.result, Err(SourceRuleError::Capacity));
    assert_eq!(decodes(&legacy) - actual_decodes, actual_decodes);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        legacy.native_main_state_value().unwrap()
    );
    assert_eq!(runtime.core.effects, legacy.core.effects);
    assert!(
        runtime.core.queue_context.is_some(),
        "the original native Local stop precedes queue leave"
    );
    assert!(runtime.core.effects_exhausted);
    assert_eq!(runtime.core.effects.len(), 65_536);
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(frame.result(), None);
    assert!(frame.pending_write().is_none());
    let stopped = runtime.native_main_state_value().unwrap();
    let effects = runtime.core.effects.clone();
    let decode_count = decodes(&runtime);
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::SourceStopped(SourceRuleError::Capacity)
    );
    assert_eq!(runtime.native_main_state_value().unwrap(), stopped);
    assert_eq!(runtime.core.effects, effects);
    assert_eq!(
        decodes(&runtime),
        decode_count,
        "a retained prefix stop never decodes again"
    );
}
#[test]
fn successful_empty_source_segment_reaches_actual_finish_capacity_without_drain() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        owner.core.effects = vec![V::Undefined; 65_536];
        owner.core.emit(V::Undefined); // Actual emitter reaches its original bound.
    }
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected = legacy.native_main_rows(&mut legacy_index, V::Array(vec![]), n(10.), false);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(frame.result(), Some(&Err(SourceRuleError::Capacity)));
    assert_eq!(runtime.core.effects.len(), 65_536);
    assert_eq!(runtime.core.effects, legacy.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        legacy.native_main_state_value().unwrap()
    );
    assert!(
        runtime.core.queue_context.is_none(),
        "the successful empty source body reaches real finish/queue leave"
    );
    assert_eq!(frame.consumed_rows(), 0);
    assert!(frame.validity().is_empty());
}

fn fail(
    runtime: &mut SourceContractRuntime,
    collection: &str,
    operation: &str,
    skip: usize,
    message: &str,
) {
    runtime
        .fail_storage(StorageFailure {
            collection: collection.into(),
            operation: operation.into(),
            skip,
            message: message.into(),
        })
        .unwrap();
}
fn same_legacy(runtime: &SourceContractRuntime, legacy: &SourceContractRuntime) {
    assert_eq!(runtime.core.effects, legacy.core.effects);
    assert_eq!(
        runtime.native_main_state_value().unwrap(),
        legacy.native_main_state_value().unwrap()
    );
}
#[test]
fn actual_primary_find_throw_marks_fatal_then_queries_once_for_second_await() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        fail(
            owner,
            "txIndex",
            "findAsync",
            0,
            "actual primary find rejection",
        );
    }
    let before = index.snapshot().unwrap();
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(marks(&runtime), 2);
    assert_eq!(decodes(&runtime), 1);
    assert!(
        frame.validity().is_empty(),
        "source find throws before native complete-query receipt"
    );
    let update = frame.pending_write().unwrap().update_arguments()[1].get("$set");
    assert_eq!(update.get("valid"), V::Bool(false));
    assert_eq!(
        update.get("reason"),
        s("Fatal tx processing error: actual primary find rejection")
    );
    assert_eq!(index.snapshot().unwrap(), before);
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected = legacy.native_main_rows(&mut legacy_index, V::Array(vec![row()]), n(10.), false);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(frame.result(), Some(&Ok(DependencyReturn::Pending(n(0.)))));
    assert_eq!(frame.validity().len(), 1);
    same_legacy(&runtime, &legacy);
}
#[test]
fn actual_fatal_mark_throw_escapes_without_reentering_row_catch() {
    let probe_temp = Temp::new();
    let (mut probe, probe_index) = setup(&probe_temp);
    let mut probe_frame = probe
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        probe.native_main_rows_advance(&probe_index, &mut probe_frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let successful_prefix_updates = probe
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("collection") == s("consensus") && effect.get("method") == s("updateAsync")
        })
        .count();
    assert!(successful_prefix_updates > 0);
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        fail(
            owner,
            "txIndex",
            "findAsync",
            0,
            "actual primary read rejection",
        );
        fail(
            owner,
            "consensus",
            "updateAsync",
            successful_prefix_updates,
            "actual fatal mark rejection",
        );
    }
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(
        marks(&runtime),
        2,
        "the second attempted source mark throws"
    );
    assert!(frame.validity().is_empty());
    let expected = legacy.native_main_rows(&mut legacy_index, V::Array(vec![row()]), n(10.), false);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: "Error".into(),
                message: "actual fatal mark rejection".into()
            }
        )))
    );
    same_legacy(&runtime, &legacy);
}
#[test]
fn actual_second_validity_throw_escapes_fatal_branch_without_third_mark() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        fail(
            owner,
            "txIndex",
            "updateAsync",
            0,
            "actual primary update rejection",
        );
        fail(
            owner,
            "txIndex",
            "updateAsync",
            0,
            "actual fatal update rejection",
        );
    }
    let before = index.snapshot().unwrap();
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(marks(&runtime), 2);
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(frame.validity().len(), 2);
    assert_eq!(
        index.snapshot().unwrap(),
        before,
        "both genuine source rejections precede native append"
    );
    let expected = legacy.native_main_rows(&mut legacy_index, V::Array(vec![row()]), n(10.), false);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::PendingThrow(
            crate::validity_source::SourceException {
                name: "Error".into(),
                message: "actual fatal update rejection".into()
            }
        )))
    );
    same_legacy(&runtime, &legacy);
}
#[test]
fn actual_mixed_already_processed_and_new_row_retains_original_skip_count() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    let mut fresh = row();
    set(&mut fresh, "_id", s("tx-10-second-raw-command")).unwrap();
    let mut value = fresh.get("value");
    set(&mut value, "txId", s("second-raw-command")).unwrap();
    set(&mut fresh, "value", value).unwrap();
    for (owner, metadata) in [(&mut runtime, &mut index), (&mut legacy, &mut legacy_index)] {
        metadata
            .import_row(&metadata.snapshot().unwrap(), fresh.clone())
            .unwrap();
        assert_eq!(
            owner
                .native_main_rows(metadata, V::Array(vec![row()]), n(10.), false)
                .result,
            Ok(DependencyReturn::Pending(n(0.)))
        );
        owner.take_effects(); // Only after a genuinely completed prior invocation.
    }
    NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT.with(|count| count.set(0));
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![row(), fresh.clone()]), n(10.), false)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(frame.consumed_rows(), 2);
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(
        frame.pending_write().unwrap().update_arguments()[0].get("_id"),
        fresh.get("_id")
    );
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected = legacy.native_main_rows(
        &mut legacy_index,
        V::Array(vec![row(), fresh]),
        n(10.),
        false,
    );
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(frame.result(), Some(&Ok(DependencyReturn::Pending(n(1.)))));
    assert_eq!(frame.validity().len(), 1);
    same_legacy(&runtime, &legacy);
}
#[test]
fn actual_live_logic_rejection_preserves_processed_any_before_failed_logic() {
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        fail(
            owner,
            "tallyMap",
            "findOneAsync",
            0,
            "recorded-tally-init-read-rejection",
        );
    }
    let live_row = row().get("value");
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![live_row.clone()]), n(10.), true)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(marks(&runtime), 1);
    let update = frame.pending_write().unwrap().update_arguments()[1].get("$set");
    assert_eq!(update.get("valid"), V::Bool(false));
    assert_eq!(
        update.get("reason"),
        s(" Logic error: recorded-tally-init-read-rejection")
    );
    let mut processed_any = V::Undefined;
    frame
        .visit_canonical_fields(&mut |path, value| {
            if path == "processedAny" {
                processed_any = value.clone();
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(processed_any, V::Bool(true));
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected =
        legacy.native_main_rows(&mut legacy_index, V::Array(vec![live_row]), n(10.), true);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::Pending(V::Bool(true))))
    );
    same_legacy(&runtime, &legacy);
}

#[test]
fn actual_activation_save_failure_is_source_swallowed_and_retains_valid_live_result() {
    // Genuine original source4ad25241/e5023b52: Activation.saveActivationsList
    // catches this dependency rejection, so Logic fulfills and valid stays true.
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        fail(
            owner,
            "activations",
            "updateAsync",
            0,
            "recorded-activation-save-rejection",
        );
    }
    let live_row = row().get("value");
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![live_row.clone()]), n(10.), true)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(marks(&runtime), 1);
    let update = frame.pending_write().unwrap().update_arguments()[1].get("$set");
    assert_eq!(update.get("valid"), V::Bool(true));
    assert_eq!(update.get("reason"), V::Undefined);
    assert!(runtime
        .core
        .effects
        .iter()
        .any(|effect| effect.get("collection") == s("activations")
            && effect.get("method") == s("updateAsync")
            && effect.get("thrown").get("message") == s("recorded-activation-save-rejection")));
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected =
        legacy.native_main_rows(&mut legacy_index, V::Array(vec![live_row]), n(10.), true);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::Pending(V::Bool(true))))
    );
    same_legacy(&runtime, &legacy);
}

#[test]
fn actual_live_logic_rejection_keeps_five_financial_writes_before_failed_flag() {
    // Genuine original source4ad25241/e5023b52: sixth tally update writes $TLinit;
    // five balance saves precede it. Main already set processedAny=true.
    let temp = Temp::new();
    let control_temp = Temp::new();
    let (mut runtime, mut index) = setup(&temp);
    let (mut legacy, mut legacy_index) = setup(&control_temp);
    for owner in [&mut runtime, &mut legacy] {
        fail(
            owner,
            "tallyMap",
            "updateAsync",
            5,
            "recorded-tally-init-flag-rejection",
        );
    }
    let live_row = row().get("value");
    let mut frame = runtime
        .native_main_rows_begin(V::Array(vec![live_row.clone()]), n(10.), true)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance(&index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(decodes(&runtime), 1);
    assert_eq!(marks(&runtime), 1);
    let updates = runtime
        .core
        .effects
        .iter()
        .filter(|effect| {
            effect.get("collection") == s("tallyMap") && effect.get("method") == s("updateAsync")
        })
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 6);
    for effect in &updates[..5] {
        assert_eq!(
            arg(&array(&effect.get("args")).unwrap(), 0).get("_id"),
            s("tallyMap")
        );
        assert_eq!(effect.get("thrown"), V::Undefined);
    }
    assert_eq!(
        arg(&array(&updates[5].get("args")).unwrap(), 0).get("_id"),
        s("$TLinit")
    );
    assert_eq!(
        updates[5].get("thrown").get("message"),
        s("recorded-tally-init-flag-rejection")
    );
    assert!(!runtime.collection("propertyList").is_empty());
    assert!(runtime
        .collection("tallyMap")
        .iter()
        .any(|row| row.get("_id") == s("tallyMap")));
    assert!(!runtime
        .collection("tallyMap")
        .iter()
        .any(|row| row.get("_id") == s("$TLinit")));
    let financial_prefix = runtime.collection("tallyMap").to_vec();
    let update = frame.pending_write().unwrap().update_arguments()[1].get("$set");
    assert_eq!(update.get("valid"), V::Bool(false));
    assert_eq!(
        update.get("reason"),
        s(" Logic error: recorded-tally-init-flag-rejection")
    );
    assert_eq!(
        runtime.native_main_rows_commit(&mut index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    assert_eq!(runtime.collection("tallyMap"), financial_prefix);
    let expected =
        legacy.native_main_rows(&mut legacy_index, V::Array(vec![live_row]), n(10.), true);
    assert_eq!(frame.result(), Some(&expected.result));
    assert_eq!(
        frame.result(),
        Some(&Ok(DependencyReturn::Pending(V::Bool(true))))
    );
    same_legacy(&runtime, &legacy);
}
