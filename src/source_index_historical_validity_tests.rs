use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "historical-validity-playback-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
fn row(id: &str) -> V {
    V::Object(vec![
        ("_id".into(), text(id)),
        (
            "value".into(),
            V::Object(vec![
                ("txId".into(), text("shared")),
                ("payload".into(), text("0,raw")),
            ]),
        ),
    ])
}
fn args() -> [V; 4] {
    [
        text("shared"),
        V::number(0.),
        V::Bool(true),
        text("source result"),
    ]
}
fn actual(index: &mut SourceCompactIndex, a: &[V; 4]) -> StreamedValidityEvaluation {
    index
        .native_stream_upsert_validity(
            &index.snapshot().unwrap(),
            a[0].clone(),
            a[1].clone(),
            a[2].clone(),
            a[3].clone(),
            &mut |_| Ok(None),
        )
        .unwrap()
}
fn replay(
    p: &mut NativeHistoricalValidityPlaybackV1<'_>,
    a: &[V; 4],
) -> Result<StreamedValidityEvaluation> {
    p.verify_source_update(
        a[0].clone(),
        a[1].clone(),
        a[2].clone(),
        a[3].clone(),
        &mut |_| Ok(None),
    )
}
#[test]
fn historical_source_validity_cold_post_replays_first_suffix_without_append() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "raw source").unwrap();
    for id in ["tx-1-shared", "tx-2-shared"] {
        index
            .import_row(&index.snapshot().unwrap(), row(id))
            .unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let original = actual(&mut index, &args());
    let post = index.capture_verified_history_pin().unwrap();
    assert!(index.get("tx-2-shared").unwrap().unwrap().get("valid") == V::Undefined);
    drop(index);
    let mut index = SourceCompactIndex::open_verified_history_owner(&temp.0, ns(), &post).unwrap();
    let io = index.storage_io_counters().unwrap();
    let mut p =
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before, post.clone()]).unwrap();
    assert_eq!(replay(&mut p, &args()).unwrap(), original);
    assert!(p.complete());
    assert_eq!(p.consumed_transitions(), 1);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    let after_io = index.storage_io_counters().unwrap();
    assert_eq!(after_io.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after_io.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after_io.published_compactions, io.published_compactions);
}
#[test]
fn historical_source_validity_wrong_computed_fields_refuse_without_consumption() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "raw source").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-shared"))
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let original = actual(&mut index, &args());
    let post = index.capture_verified_history_pin().unwrap();
    let mut p =
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before, post.clone()]).unwrap();
    let mut wrong = args();
    wrong[2] = V::Bool(false);
    assert!(replay(&mut p, &wrong).is_err());
    assert_eq!(p.consumed_transitions(), 0);
    assert_eq!(replay(&mut p, &args()).unwrap(), original);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
}
#[test]
fn historical_source_validity_two_batches_no_match_and_source_rejection_order() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "raw source").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-shared"))
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let one = actual(&mut index, &args());
    let mid = index.capture_verified_history_pin().unwrap();
    let mut second = args();
    second[3] = text("second real source result");
    let two = actual(&mut index, &second);
    let post = index.capture_verified_history_pin().unwrap();
    let mut p =
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before, mid, post.clone()]).unwrap();
    let missing = p
        .verify_source_update(
            text("absent"),
            V::number(1.),
            V::Bool(false),
            text("unused"),
            &mut |_| panic!("no match must not invoke update hook"),
        )
        .unwrap();
    assert!(missing.update.is_none());
    assert_eq!(p.consumed_transitions(), 0);
    let err = SourceException {
        name: "Error".into(),
        message: "recorded IO failure".into(),
    };
    let rejected = p
        .verify_source_update(
            text("shared"),
            V::number(0.),
            V::Bool(true),
            text("source result"),
            &mut |_| Ok(Some(DependencyReturn::PendingThrow(err.clone()))),
        )
        .unwrap();
    assert_eq!(
        rejected.outcome,
        crate::validity_source::SourceOutcome::Thrown(err)
    );
    assert_eq!(p.consumed_transitions(), 0);
    assert_eq!(replay(&mut p, &args()).unwrap(), one);
    assert_eq!(replay(&mut p, &second).unwrap(), two);
    assert!(p.complete());
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
}
#[test]
fn historical_source_validity_extra_metadata_write_is_not_a_source_transition() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "raw source").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-shared"))
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-2-extra"))
        .unwrap();
    let wrong = index.capture_verified_history_pin().unwrap();
    let mut p =
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before.clone(), wrong.clone()])
            .unwrap();
    assert!(replay(&mut p, &args()).is_err());
    assert_eq!(p.consumed_transitions(), 0);
    actual(&mut index, &args());
    let post = index.capture_verified_history_pin().unwrap();
    assert!(NativeHistoricalValidityPlaybackV1::open(&mut index, &[before, post.clone()]).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
}
