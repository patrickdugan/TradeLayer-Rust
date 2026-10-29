use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "cold-index-discard-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        assert_eq!(
            root.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
#[test]
fn cold_capability_discard_rows_and_context_only_discard_refuse_before_io() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "original raw context",
    )
    .unwrap();
    let row = V::Object(vec![
        ("_id".into(), V::String("tx-1-raw".into())),
        (
            "value".into(),
            V::Object(vec![("payload".into(), V::String("0,raw".into()))]),
        ),
    ]);
    index
        .import_row(&index.snapshot().unwrap(), row.clone())
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let io = index.storage_io_counters().unwrap();
    let cap = index.begin_verified_cold_execution(&before).unwrap();
    let mut wrong = before.metadata.clone();
    wrong.sequence += 1;
    for (pin, ids) in [
        (&before.metadata, vec!["tx-1-raw".to_owned()]),
        (&before.metadata, vec![]),
        (&wrong, vec![]),
    ] {
        assert!(matches!(
            index.discard_rows(pin, &ids, "forbidden changed context"),
            Err(CompactIndexError::ColdWriteRefused)
        ));
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
        assert_eq!(index.snapshot().unwrap().context, "original raw context");
        assert_eq!(index.len().unwrap(), 1);
        assert_eq!(index.get("tx-1-raw").unwrap(), Some(row.clone()));
        let after = index.storage_io_counters().unwrap();
        assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
        assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
        assert_eq!(after.published_compactions, io.published_compactions);
    }
    index.finish_verified_cold_execution(&cap).unwrap();
    let post = index
        .discard_rows(
            &before.metadata,
            &["tx-1-raw".to_owned()],
            "explicit live context",
        )
        .unwrap();
    assert_eq!(post.sequence, before.metadata.sequence + 1);
    assert_eq!(post.context, "explicit live context");
    assert!(index.is_empty().unwrap());
}
