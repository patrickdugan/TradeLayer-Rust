use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let path = parent.join(format!(
            "historical-native-source-read-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        if let Ok(path) = self.0.canonicalize() {
            assert_eq!(path.parent(), Some(parent.as_path()));
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn txt(s: &str) -> V {
    V::String(s.into())
}
fn height(index: &mut SourceCompactIndex, value: f64) {
    index
        .native_database(
            "updateAsync",
            &[
                obj(&[("_id", txt("MaxHeight"))]),
                obj(&[("$set", obj(&[("value", V::number(value))]))]),
                obj(&[("upsert", V::Bool(true))]),
            ],
        )
        .unwrap();
}
#[test]
fn actual_historical_source_queries_return_original_pages_not_current_post_and_refuse_writes() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "source read control",
    )
    .unwrap();
    height(&mut index, 10.);
    let before = index.capture_verified_history_pin().unwrap();
    let queries = [
        obj(&[("_id", txt("MaxHeight"))]),
        obj(&[("_id", txt("absent"))]),
        V::Object(vec![]),
    ];
    let mut expected = vec![];
    for query in &queries {
        for method in ["findOneAsync", "findAsync"] {
            expected.push(
                index
                    .native_database(method, std::slice::from_ref(query))
                    .unwrap(),
            );
        }
    }
    height(&mut index, 11.);
    let post = index.capture_verified_history_pin().unwrap();
    let appended = index.storage_io_counters().unwrap().confirmed_append_bytes;
    let history =
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before.clone(), post.clone()])
            .unwrap();
    assert_eq!(history.source_history_pin(), &before);
    let mut actual = vec![];
    for query in &queries {
        for method in ["findOneAsync", "findAsync"] {
            actual.push(
                history
                    .source_read_view()
                    .source_database_read(method, std::slice::from_ref(query))
                    .unwrap(),
            );
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(history.source_actual_rows().unwrap(), before.rows);
    assert!(matches!(
        history
            .source_read_view()
            .source_database_read("updateAsync", &[]),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    drop(history);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        index.storage_io_counters().unwrap().confirmed_append_bytes,
        appended
    );
}
#[test]
fn kernel_actual_index_loan_requires_same_opaque_cold_guard_and_cannot_append() {
    let temp = Temp::new();
    let other_temp = Temp::new();
    let ns = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index =
        SourceCompactIndex::create(&temp.0, ns.clone(), "same actual kernel owner").unwrap();
    height(&mut index, 10.);
    let post = index.capture_verified_history_pin().unwrap();
    let capability = index.begin_verified_cold_execution(&post).unwrap();
    let mut other = SourceCompactIndex::create(&other_temp.0, ns, "foreign kernel owner").unwrap();
    let other_pin = other.capture_verified_history_pin().unwrap();
    let foreign = other.begin_verified_cold_execution(&other_pin).unwrap();
    let appended = index.storage_io_counters().unwrap().confirmed_append_bytes;
    let mut history =
        NativeHistoricalValidityPlaybackV1::open(&mut index, std::slice::from_ref(&post)).unwrap();
    assert!(matches!(
        history.scoped_cold_index(&foreign),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    let actual = history.scoped_cold_index(&capability).unwrap();
    assert!(matches!(
        actual.native_database(
            "updateAsync",
            &[
                obj(&[("_id", txt("fresh-raw-row"))]),
                obj(&[("_id", txt("fresh-raw-row")), ("value", V::number(1.))]),
                obj(&[("upsert", V::Bool(true))])
            ]
        ),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(history.source_actual_rows().unwrap(), post.rows);
    drop(history);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        index.storage_io_counters().unwrap().confirmed_append_bytes,
        appended
    );
    index.finish_verified_cold_execution(&capability).unwrap();
    other.finish_verified_cold_execution(&foreign).unwrap();
}
