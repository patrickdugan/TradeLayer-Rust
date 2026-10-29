// Child of actual cold-row reexecution tests. These calls run original source
// PCs from raw foundation commands; observations are assertions only.
use super::*;
use crate::source_contract_runtime::NativeMainRowStopV1;
fn cold_row_phase_control(failures: usize, live: bool, wrong_mutation: bool) {
    let temp = Temp::new();
    let mut original = owner_with_failures(&temp, failures);
    let foundation = original.foundation.clone();
    let foundation_head = original.foundation_head;
    let binding = original.binding.clone();
    let before = original.index.capture_verified_history_pin().unwrap();
    let rows = V::Array(vec![if live { row().get("value") } else { row() }]);
    let mut frame = original
        .financial
        .native_main_runtime_mut()
        .native_main_rows_begin(rows.clone(), V::number(10.), live)
        .unwrap();
    assert_eq!(
        original
            .financial
            .native_main_runtime_mut()
            .native_main_rows_advance(&original.index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let mut expected_fields = vec![];
    frame
        .visit_canonical_fields(&mut |p, v| {
            expected_fields.push((p.to_owned(), v.clone()));
            Ok(())
        })
        .unwrap();
    let expected_prefix = original
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let pending = frame.pending_write().unwrap();
    let raw=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(pending.append_plan(),pending.durable_read_context()).unwrap().exact_prepared_mutations().to_vec();
    assert_eq!(
        original
            .financial
            .native_main_runtime_mut()
            .native_main_rows_append(&mut original.index, &mut frame),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    let post = original.index.capture_verified_history_pin().unwrap();
    assert_eq!(
        original
            .financial
            .native_main_runtime_mut()
            .native_main_rows_resolve_post(&original.index, &mut frame),
        NativeMainRowStopV1::Complete
    );
    let expected_result = frame.result().unwrap().clone();
    let expected_state = original
        .financial_runtime()
        .native_main_state_value()
        .unwrap();
    let expected_effects = original
        .financial_runtime()
        .native_main_row_effects()
        .to_vec();
    let expected_validity = frame.validity().to_vec();
    drop(frame);
    drop(original);
    // Old financial owner and its index are gone. Reexecute raw foundation into
    // ONE new financial owner; independently cold-open SAME real actual Post.
    let mut financial =
        SourceContractReplay::from_checkpoint(&foundation, foundation_head).unwrap();
    financial
        .native_main_runtime_mut()
        .native_main_bind(binding)
        .unwrap();
    let mut index = SourceCompactIndex::open_verified_history_owner(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &post,
    )
    .unwrap();
    let cap = index.begin_verified_cold_execution(&post).unwrap();
    let io = index.storage_io_counters().unwrap().confirmed_append_bytes;
    let mut history = crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1::open(
        &mut index,
        &[before, post.clone()],
    )
    .unwrap();
    let runtime = financial.native_main_runtime_mut();
    let mut cold = runtime
        .native_main_rows_begin(rows, V::number(10.), live)
        .unwrap();
    assert_eq!(
        runtime.native_main_rows_advance_historical(&mut history, &mut cold),
        NativeMainRowStopV1::AwaitNativeWrite
    );
    assert_eq!(runtime.native_main_state_value().unwrap(), expected_prefix);
    let mut fields = vec![];
    cold.visit_canonical_fields(&mut |p, v| {
        fields.push((p.to_owned(), v.clone()));
        Ok(())
    })
    .unwrap();
    assert_eq!(fields, expected_fields);
    if wrong_mutation {
        let mut wrong = raw.clone();
        wrong.push(0);
        let decodes = runtime.native_main_test_row_decode_count();
        assert!(matches!(
            runtime.native_main_rows_verify_historical_append(&mut history, &mut cold, &wrong),
            NativeMainRowStopV1::StorageStopped(_)
        ));
        assert_eq!(runtime.native_main_state_value().unwrap(), expected_prefix);
        assert!(matches!(
            runtime.native_main_rows_advance_historical(&mut history, &mut cold),
            NativeMainRowStopV1::StorageStopped(_)
        ));
        assert_eq!(runtime.native_main_test_row_decode_count(), decodes);
        assert_eq!(history.consumed_transitions(), 0);
    } else {
        assert_eq!(
            runtime.native_main_rows_verify_historical_append(&mut history, &mut cold, &raw),
            NativeMainRowStopV1::AwaitNativeWrite
        );
        assert_eq!(
            runtime.native_main_state_value().unwrap(),
            expected_prefix,
            "verified past native write cannot resolve source Promise or drain financial queue"
        );
        assert_eq!(
            runtime.native_main_rows_resolve_historical_post(&mut history, &mut cold),
            NativeMainRowStopV1::Complete
        );
        assert_eq!(cold.result(), Some(&expected_result));
        assert_eq!(cold.validity(), expected_validity.as_slice());
        assert_eq!(runtime.native_main_state_value().unwrap(), expected_state);
        assert_eq!(
            runtime.native_main_row_effects(),
            expected_effects.as_slice()
        );
        assert!(history.complete());
    }
    drop(history);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        index.storage_io_counters().unwrap().confirmed_append_bytes,
        io
    );
    index.finish_verified_cold_execution(&cap).unwrap();
}
#[test]
fn actual_historical_row_pc_keeps_same_primary_native_await_until_separate_source_resolve() {
    for live in [false, true] {
        cold_row_phase_control(0, live, false);
    }
}
#[test]
fn actual_historical_fatal_row_pc_uses_raw_failure_once_then_same_second_validity_await() {
    cold_row_phase_control(1, false, false);
}
#[test]
fn actual_historical_row_pc_wrong_wal_mutation_retains_financial_prefix_without_reentry() {
    cold_row_phase_control(0, true, true);
}
