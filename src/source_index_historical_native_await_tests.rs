use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "historical-native-await-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        assert_eq!(
            path.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        let _ = std::fs::remove_dir_all(path);
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn text(s: &str) -> V {
    V::String(s.into())
}
fn row() -> V {
    V::Object(vec![
        ("_id".into(), text("tx-1-input")),
        (
            "value".into(),
            V::Object(vec![("payload".into(), text("0,raw"))]),
        ),
    ])
}
fn actual(
    temp: &Temp,
) -> (
    SourceCompactIndex,
    Vec<SourceIndexHistoryPinV1>,
    V,
    Vec<u8>,
    StreamedValidityEvaluation,
) {
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    index.import_row(&index.snapshot().unwrap(), row()).unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &before.metadata,
            text("input"),
            V::number(0.),
            V::Bool(true),
            text("actual computed reason"),
        )
        .unwrap()
    else {
        panic!("raw first suffix")
    };
    let NativePreparedValidityOperationV1::Write(write) =
        selected.prepare_write(&index, &before.metadata).unwrap()
    else {
        panic!("actual prepared native plan")
    };
    let context = write.durable_read_context();
    let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(),context.clone()).unwrap();
    let raw = candidates.exact_prepared_mutations().to_vec();
    index.commit_prepared_validity(&write).unwrap();
    let result = index.resolve_prepared_validity_post(&write).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    drop(index);
    (
        SourceCompactIndex::open_verified_history_owner(&temp.0, ns(), &post).unwrap(),
        vec![before, post],
        context,
        raw,
        result,
    )
}
fn prepare(
    playback: &mut NativeHistoricalValidityPlaybackV1<'_>,
) -> NativeHistoricalValidityAwaitV1 {
    let NativeHistoricalValidityOperationV1::Await(awaiting) = playback
        .prepare_source_update(
            text("input"),
            V::number(0.),
            V::Bool(true),
            text("actual computed reason"),
            &mut |_| Ok(None),
        )
        .unwrap()
    else {
        panic!("actual held await")
    };
    *awaiting
}
#[test]
fn historical_native_await_append_and_resolve_are_three_distinct_same_scope_phases() {
    let temp = Temp::new();
    let (mut index, pins, context, raw, expected) = actual(&temp);
    let current = index.capture_verified_history_pin().unwrap();
    let cap = index.begin_verified_cold_execution(&current).unwrap();
    let io = index.storage_io_counters().unwrap().confirmed_append_bytes;
    let mut playback = NativeHistoricalValidityPlaybackV1::open(&mut index, &pins).unwrap();
    let mut awaiting = prepare(&mut playback);
    let mut observed = vec![];
    awaiting
        .visit_canonical_fields(&mut |path, value| {
            assert!(!matches!(value, V::Object(_) | V::Array(_)));
            observed.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(observed
        .iter()
        .any(|(path, value)| path == "phase" && *value == text("Prepared")));
    let mut playback_fields = vec![];
    playback
        .visit_canonical_fields(&mut |path, value| {
            playback_fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(playback_fields
        .iter()
        .any(|(path, _)| path.starts_with("originalRead/")));
    assert!(playback_fields
        .iter()
        .any(|(path, _)| path.starts_with("actualCurrent/")));
    assert_eq!(awaiting.durable_read_context(), context);
    assert_eq!(awaiting.exact_prepared_mutations(), raw);
    assert_eq!(playback.current(), &pins[0].metadata);
    assert!(!awaiting.append_verified());
    assert!(!awaiting.resolved());
    assert!(playback
        .resolve_recorded_native_post(&mut awaiting)
        .is_err());
    assert!(playback
        .prepare_source_update(
            text("input"),
            V::number(0.),
            V::Bool(true),
            text("duplicate source execution"),
            &mut |_| panic!("a held await cannot consume another source callback")
        )
        .is_err());
    let mut wrong = raw.clone();
    wrong.push(0);
    assert!(playback
        .verify_recorded_native_append(&mut awaiting, &wrong)
        .is_err());
    assert_eq!(playback.current(), &pins[0].metadata);
    playback
        .verify_recorded_native_append(&mut awaiting, &raw)
        .unwrap();
    assert_eq!(playback.current(), &pins[1].metadata);
    assert!(awaiting.append_verified());
    assert!(!awaiting.resolved());
    assert!(
        !playback.complete(),
        "native Post alone cannot resolve source Promise"
    );
    assert!(playback
        .verify_recorded_native_append(&mut awaiting, &raw)
        .is_err());
    assert_eq!(
        playback
            .resolve_recorded_native_post(&mut awaiting)
            .unwrap(),
        expected
    );
    assert!(awaiting.resolved());
    assert!(playback.complete());
    assert!(playback
        .resolve_recorded_native_post(&mut awaiting)
        .is_err());
    drop(playback);
    assert_eq!(index.capture_verified_history_pin().unwrap(), current);
    assert_eq!(
        index.storage_io_counters().unwrap().confirmed_append_bytes,
        io
    );
    index.finish_verified_cold_execution(&cap).unwrap();
}
#[test]
fn historical_native_await_foreign_playback_cannot_resolve_or_advance_any_prefix() {
    let one = Temp::new();
    let two = Temp::new();
    let (mut first, first_pins, _, raw, _) = actual(&one);
    let (mut second, second_pins, _, _, _) = actual(&two);
    let mut a = NativeHistoricalValidityPlaybackV1::open(&mut first, &first_pins).unwrap();
    let mut b = NativeHistoricalValidityPlaybackV1::open(&mut second, &second_pins).unwrap();
    let mut awaiting = prepare(&mut a);
    assert!(b
        .verify_recorded_native_append(&mut awaiting, &raw)
        .is_err());
    assert!(b.resolve_recorded_native_post(&mut awaiting).is_err());
    assert_eq!(b.consumed_transitions(), 0);
    assert_eq!(a.consumed_transitions(), 0);
    a.verify_recorded_native_append(&mut awaiting, &raw)
        .unwrap();
    assert!(b.resolve_recorded_native_post(&mut awaiting).is_err());
    a.resolve_recorded_native_post(&mut awaiting).unwrap();
    assert!(a.complete());
    let mut next = prepare(&mut b);
    b.verify_recorded_native_append(&mut next, &raw).unwrap();
    b.resolve_recorded_native_post(&mut next).unwrap();
    assert!(b.complete());
}
#[test]
fn historical_native_await_wrong_computed_mutations_cannot_use_actual_post_as_answer() {
    let temp = Temp::new();
    let (mut index, pins, _, raw, _) = actual(&temp);
    let current = index.capture_verified_history_pin().unwrap();
    let mut playback = NativeHistoricalValidityPlaybackV1::open(&mut index, &pins).unwrap();
    let NativeHistoricalValidityOperationV1::Await(mut awaiting) = playback
        .prepare_source_update(
            text("input"),
            V::number(0.),
            V::Bool(false),
            text("wrong computed verdict"),
            &mut |_| Ok(None),
        )
        .unwrap()
    else {
        panic!("computed real plan")
    };
    assert!(playback
        .verify_recorded_native_append(&mut awaiting, &raw)
        .is_err());
    let derived = awaiting.exact_prepared_mutations().to_vec();
    assert!(
        playback
            .verify_recorded_native_append(&mut awaiting, &derived)
            .is_err(),
        "even self-matching bytes cannot replace independently verified historical full Post"
    );
    assert_eq!(playback.consumed_transitions(), 0);
    assert!(!awaiting.append_verified());
    drop(playback);
    assert_eq!(index.capture_verified_history_pin().unwrap(), current);
}
