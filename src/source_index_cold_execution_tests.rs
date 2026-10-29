use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "cold-index-capability-{}-{}",
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
fn row(id: &str) -> V {
    V::Object(vec![
        ("_id".into(), text(id)),
        (
            "value".into(),
            V::Object(vec![("payload".into(), text("0,raw"))]),
        ),
    ])
}
fn prepared(index: &SourceCompactIndex) -> NativePreparedValidityWriteV1 {
    let selected = index
        .prepare_validity_read(
            &index.snapshot().unwrap(),
            text("input"),
            V::number(0.),
            V::Bool(true),
            text("computed source argument"),
        )
        .unwrap();
    let NativePreparedValidityQueryV1::Selected(selected) = selected else {
        panic!("raw row selected")
    };
    let NativePreparedValidityOperationV1::Write(write) = selected
        .prepare_write(index, &index.snapshot().unwrap())
        .unwrap()
    else {
        panic!("real prepared plan")
    };
    *write
}
#[test]
fn cold_capability_blocks_metadata_io_before_source_put_and_prepared_commit() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-input"))
        .unwrap();
    let expected = index.capture_verified_history_pin().unwrap();
    let cap = index.begin_verified_cold_execution(&expected).unwrap();
    let write = prepared(&index);
    let before = index.storage_io_counters().unwrap().confirmed_append_bytes;
    assert!(matches!(
        index.commit_prepared_validity(&write),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(matches!(
        index.import_row(&index.snapshot().unwrap(), row("tx-2-other")),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(matches!(
        index.native_database(
            "updateAsync",
            &[
                V::Object(vec![("_id".into(), text("tx-1-input"))]),
                V::Object(vec![(
                    "$set".into(),
                    V::Object(vec![("reason".into(), text("unexpected source write"))])
                )])
            ]
        ),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), expected);
    assert_eq!(
        index.storage_io_counters().unwrap().confirmed_append_bytes,
        before
    );
    assert!(index.begin_verified_cold_execution(&expected).is_err());
    index.finish_verified_cold_execution(&cap).unwrap();
    index.commit_prepared_validity(&write).unwrap();
    assert_eq!(
        index.snapshot().unwrap().sequence,
        expected.metadata.sequence + 1,
        "guard refusal did not attempt or consume actual append token"
    );
    assert!(
        index.finish_verified_cold_execution(&cap).is_err(),
        "stale capability cannot close a different scope"
    );
}
#[test]
fn cold_capability_foreign_scope_cannot_enable_writes_and_bad_expectation_stays_healthy() {
    let first = Temp::new();
    let second = Temp::new();
    let mut a = SourceCompactIndex::create(&first.0, ns(), "first raw owner").unwrap();
    let mut b = SourceCompactIndex::create(&second.0, ns(), "second raw owner").unwrap();
    let current = a.capture_verified_history_pin().unwrap();
    let mut wrong = current.clone();
    wrong.storage.content.chain_sha256d[0] ^= 1;
    assert!(a.begin_verified_cold_execution(&wrong).is_err());
    a.import_row(&a.snapshot().unwrap(), row("tx-1-input"))
        .unwrap();
    let a_pin = a.capture_verified_history_pin().unwrap();
    let b_pin = b.capture_verified_history_pin().unwrap();
    let a_cap = a.begin_verified_cold_execution(&a_pin).unwrap();
    let b_cap = b.begin_verified_cold_execution(&b_pin).unwrap();
    assert!(a.finish_verified_cold_execution(&b_cap).is_err());
    assert!(b.finish_verified_cold_execution(&a_cap).is_err());
    assert!(matches!(
        a.import_row(&a.snapshot().unwrap(), row("tx-2-other")),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(a.capture_verified_history_pin().unwrap(), a_pin);
    assert_eq!(b.capture_verified_history_pin().unwrap(), b_pin);
    a.finish_verified_cold_execution(&a_cap).unwrap();
    b.finish_verified_cold_execution(&b_cap).unwrap();
}
#[test]
fn cold_capability_refuses_compaction_policy_and_publication_before_any_disk_change() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.0, ns(), "actual raw origin").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row("tx-1-input"))
        .unwrap();
    let expected = index.capture_verified_history_pin().unwrap();
    let cap = index.begin_verified_cold_execution(&expected).unwrap();
    let before = index.storage_io_counters().unwrap();
    assert!(matches!(
        index.compact_metadata(&expected.metadata),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(matches!(
        index.configure_compaction(crate::compact_journal::CompactionConfig::default()),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), expected);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, before.confirmed_append_bytes);
    assert_eq!(
        after.copied_checkpoint_bytes,
        before.copied_checkpoint_bytes
    );
    assert_eq!(after.published_compactions, before.published_compactions);
    index.finish_verified_cold_execution(&cap).unwrap();
}
