use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("OUTER_BOOTSTRAP_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "outer-bootstrap-native-read-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn raw(id: &str, value: V) -> V {
    prepared_obj(vec![("_id", prepared_text(id)), ("value", value)])
}
fn seeded_index(temp: &Temp) -> SourceCompactIndex {
    let mut index =
        SourceCompactIndex::create(&temp.0, ns(), "raw actual outer bootstrap inputs").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), raw("MaxHeight", V::number(10.)))
        .unwrap();
    index
}
fn put(index: &mut SourceCompactIndex, id: &str, value: V) {
    index
        .native_database(
            "updateAsync",
            &[
                prepared_obj(vec![("_id", prepared_text(id))]),
                prepared_obj(vec![("$set", prepared_obj(vec![("value", value)]))]),
                prepared_obj(vec![("upsert", V::Bool(true))]),
            ],
        )
        .unwrap();
}
fn fields(query: &NativeMainStartupIndexReadV1) -> BTreeMap<String, V> {
    let mut result = BTreeMap::new();
    query
        .visit_canonical_fields(&mut |key, value| {
            assert!(result.insert(key.into(), value.clone()).is_none(), "{key}");
            Ok(())
        })
        .unwrap();
    result
}
#[test]
fn native_startup_request_samples_only_at_actual_executor_and_delivers_same_held_result() {
    let temp = Temp::new();
    let mut index = seeded_index(&temp);
    let mut query = index
        .startup_source_index_read_request(NativeMainStartupIndexReadKindV1::FindMax)
        .unwrap();
    assert!(!query.sampled());
    assert!(query.actual_view().is_none());
    put(&mut index, "MaxHeight", V::number(11.));
    index
        .startup_source_index_read_sample_actual(&mut query)
        .unwrap();
    let retained = query.actual_rows().unwrap().clone();
    assert!(
        matches!(query.actual_returned(),Some(DependencyReturn::Pending(row)) if row.get("value")==V::number(11.))
    );
    put(&mut index, "MaxHeight", V::number(12.));
    index
        .startup_source_index_read_deliver_callback(&mut query)
        .unwrap();
    assert_eq!(query.actual_rows(), Some(&retained));
    assert!(
        matches!(query.actual_returned(),Some(DependencyReturn::Pending(row)) if row.get("value")==V::number(11.))
    );
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(12.)
    );
    let before = fields(&query);
    assert!(index
        .startup_source_index_read_sample_actual(&mut query)
        .is_err());
    assert!(index
        .startup_source_index_read_deliver_callback(&mut query)
        .is_err());
    assert_eq!(fields(&query), before);
    query.consume_callback().unwrap();
    assert!(query.consume_callback().is_err());
}
#[test]
fn native_startup_fixed_read_retains_original_rows_while_same_owner_adds_new_height() {
    let temp = Temp::new();
    let mut index = seeded_index(&temp);
    put(
        &mut index,
        "tx-10-original",
        prepared_obj(vec![("payload", prepared_text(""))]),
    );
    let mut query = index
        .startup_source_index_read_request(NativeMainStartupIndexReadKindV1::Fixed)
        .unwrap();
    put(
        &mut index,
        "tx-11-before-executor",
        prepared_obj(vec![("payload", prepared_text(""))]),
    );
    index
        .startup_source_index_read_sample_actual(&mut query)
        .unwrap();
    let held = fields(&query);
    let held_rows = query.actual_rows().unwrap().clone();
    put(
        &mut index,
        "tx-12-after-executor",
        prepared_obj(vec![("payload", prepared_text(""))]),
    );
    index
        .startup_source_index_read_deliver_callback(&mut query)
        .unwrap();
    let read = query.actual_view().unwrap();
    let MainIndexPlanningOutcome::Ready(plan) = read.plan_main_groups().unwrap() else {
        panic!("actual grouping")
    };
    assert_eq!(plan.max_index_block(), V::number(11.));
    assert_eq!(plan.receipt(), &held_rows);
    assert_eq!(held.get("read/rows/count"), Some(&prepared_text("3")));
    assert!(read.copied_file().is_none());
    assert_ne!(read.pin(), &index.snapshot().unwrap());
    assert_eq!(
        query.actual_returned(),
        None,
        "a fixed row view is never a fabricated SourceValue array"
    );
}
#[test]
fn native_startup_native_read_cannot_move_to_foreign_or_reopened_writer() {
    let ta = Temp::new();
    let tb = Temp::new();
    let index = seeded_index(&ta);
    let other = seeded_index(&tb);
    let mut query = index
        .startup_source_index_read_request(NativeMainStartupIndexReadKindV1::FindMax)
        .unwrap();
    assert!(other
        .startup_source_index_read_sample_actual(&mut query)
        .is_err());
    assert!(!query.sampled());
    index
        .startup_source_index_read_sample_actual(&mut query)
        .unwrap();
    assert!(other
        .startup_source_index_read_deliver_callback(&mut query)
        .is_err());
    assert!(!query.callback_delivered());
    let expected = index.snapshot().unwrap();
    drop(index);
    let reopened = SourceCompactIndex::open(&ta.0, ns(), &expected).unwrap();
    assert!(reopened
        .startup_source_index_read_deliver_callback(&mut query)
        .is_err());
    assert!(!query.callback_delivered());
    assert!(
        query.actual_view().is_some(),
        "wrong-owner attempt retains original actual read"
    );
}
#[test]
fn native_startup_fixed_read_streams_more_than_source_aggregate_capacity() {
    let temp = Temp::new();
    let mut index = seeded_index(&temp);
    for at in 0..1030 {
        index
            .import_row(
                &index.snapshot().unwrap(),
                raw(
                    &format!("tx-10-{at:04}"),
                    prepared_obj(vec![("payload", prepared_text(""))]),
                ),
            )
            .unwrap();
    }
    let mut query = index
        .startup_source_index_read_request(NativeMainStartupIndexReadKindV1::Fixed)
        .unwrap();
    index
        .startup_source_index_read_sample_actual(&mut query)
        .unwrap();
    index
        .startup_source_index_read_deliver_callback(&mut query)
        .unwrap();
    assert_eq!(query.actual_rows().unwrap().rows, 1031);
    assert!(query.actual_rows().unwrap().complete);
    assert_eq!(
        fields(&query).get("read/rows/count"),
        Some(&prepared_text("1031"))
    );
    let MainIndexPlanningOutcome::Ready(plan) =
        query.actual_view().unwrap().plan_main_groups().unwrap()
    else {
        panic!("actual complete plan")
    };
    assert_eq!(plan.receipt().rows, 1031);
}
#[test]
fn native_startup_outer_scope_binds_original_live_writer_without_pinning_an_early_sequence() {
    let ta = Temp::new();
    let tb = Temp::new();
    let mut index = seeded_index(&ta);
    let other = seeded_index(&tb);
    let scope = index.capture_startup_source_index_scope().unwrap();
    put(&mut index, "MaxHeight", V::number(18.));
    index.require_startup_source_index_scope(&scope).unwrap();
    assert!(other.require_startup_source_index_scope(&scope).is_err());
    let mut fields: BTreeMap<String, V> = BTreeMap::new();
    scope
        .visit_canonical_fields(&mut |path, value| {
            assert!(fields.insert(path.into(), value.clone()).is_none());
            Ok(())
        })
        .unwrap();
    assert!(matches!(fields.get("physicalIdentity"),Some(V::String(value)) if !value.is_empty()));
    let expected = index.snapshot().unwrap();
    drop(index);
    let reopened = SourceCompactIndex::open(&ta.0, ns(), &expected).unwrap();
    assert!(reopened.require_startup_source_index_scope(&scope).is_err());
}
