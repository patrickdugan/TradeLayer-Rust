use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let path = parent.join(format!(
            "cold-history-index-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize() {
            assert_eq!(
                path.parent(),
                Some(std::env::temp_dir().canonicalize().unwrap().as_path())
            );
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn text(s: &str) -> V {
    V::String(s.into())
}
fn row(id: &str, payload: &str) -> V {
    V::Object(vec![
        ("_id".into(), text(id)),
        (
            "value".into(),
            V::Object(vec![("payload".into(), text(payload))]),
        ),
    ])
}
fn rows(snapshot: &SourceIndexReadSnapshot) -> Vec<V> {
    let mut cursor = snapshot.cursor();
    let mut output = vec![];
    while let Some(row) = cursor.next_row().unwrap() {
        output.push(row);
    }
    assert!(cursor.receipt().complete);
    output
}
#[test]
fn cold_verified_post_owner_reads_real_raw_origin_pages_and_keeps_post_authoritative() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    let original = row("tx-1-input", "00,raw-first");
    index
        .import_row(&index.snapshot().unwrap(), original.clone())
        .unwrap();
    let origin = index.capture_verified_history_pin().unwrap();
    let mut fields = vec![];
    origin
        .visit_canonical_fields(&mut |path, value| {
            assert!(!matches!(value, V::Object(_) | V::Array(_)));
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields
        .iter()
        .any(|(path, _)| path == "storage/content/chain"));
    assert!(fields.iter().any(|(path, _)| path == "rows/chain"));
    index
        .native_database(
            "updateAsync",
            &[
                V::Object(vec![("_id".into(), text("tx-1-input"))]),
                V::Object(vec![(
                    "$set".into(),
                    V::Object(vec![(
                        "value".into(),
                        row("tx-1-input", "00,raw-second").get("value"),
                    )]),
                )]),
                V::Object(vec![("upsert".into(), V::Bool(false))]),
            ],
        )
        .unwrap();
    index
        .import_row(
            &index.snapshot().unwrap(),
            row("tx-2-other", "00,other-input"),
        )
        .unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    drop(index);
    let mut index = SourceCompactIndex::open_verified_history_owner(&temp.0, ns(), &post).unwrap();
    let history = index.read_verified_history_generation(&origin).unwrap();
    assert_eq!(rows(&history), vec![original]);
    assert_eq!(history.pin(), &origin.metadata);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        index.get("tx-1-input").unwrap(),
        Some(row("tx-1-input", "00,raw-second"))
    );
    index
        .import_row(&index.snapshot().unwrap(), row("tx-3-new", "00,new-input"))
        .unwrap();
    assert_eq!(
        rows(&history).len(),
        1,
        "later actual writes cannot alter the verified history view"
    );
}
#[test]
fn wrong_row_or_complete_kv_history_fact_refuses_without_replacing_current_owner() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-input", "00,raw"))
        .unwrap();
    let origin = index.capture_verified_history_pin().unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-2-other", "00,other"))
        .unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let mut wrong = origin.clone();
    wrong.rows.chain_sha256d[0] ^= 1;
    assert!(index.read_verified_history_generation(&wrong).is_err());
    wrong = origin.clone();
    wrong.storage.content.chain_sha256d[0] ^= 1;
    assert!(index.read_verified_history_generation(&wrong).is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    drop(index);
    let mut wrong = post.clone();
    wrong.storage.content.chain_sha256d[0] ^= 1;
    assert!(SourceCompactIndex::open_verified_history_owner(&temp.0, ns(), &wrong).is_err());
    SourceCompactIndex::open_verified_history_owner(&temp.0, ns(), &post).unwrap();
}
#[test]
fn compacted_or_pruned_history_is_explicitly_unavailable_even_if_row_values_match() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-input", "00,raw"))
        .unwrap();
    let origin = index.capture_verified_history_pin().unwrap();
    let held = index.read_verified_history_generation(&origin).unwrap();
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    index.compact_metadata(&index.snapshot().unwrap()).unwrap();
    let current = index.capture_verified_history_pin().unwrap();
    assert_ne!(current.metadata.publication, origin.metadata.publication);
    assert_eq!(
        rows(&index.read_verified_history_generation(&current).unwrap()),
        vec![row("tx-1-input", "00,raw")]
    );
    assert!(index.read_verified_history_generation(&origin).is_err());
    assert_eq!(rows(&held), vec![row("tx-1-input", "00,raw")]);
    assert_eq!(index.capture_verified_history_pin().unwrap(), current);
    drop(held);
    index.journal.prune_retention().unwrap();
    drop(index);
    let mut index =
        SourceCompactIndex::open_verified_history_owner(&temp.0, ns(), &current).unwrap();
    assert_eq!(
        rows(&index.read_verified_history_generation(&current).unwrap()),
        vec![row("tx-1-input", "00,raw")]
    );
    assert!(index.read_verified_history_generation(&origin).is_err());
}
