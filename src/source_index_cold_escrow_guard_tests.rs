use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "cold-await-escrow-guard-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
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
#[test]
fn cold_scope_blocks_escrow_copy_and_guarded_native_commit_before_io() {
    let temp = Temp::new();
    let escrow = Temp::new();
    let mut index = SourceCompactIndex::create(
        &temp.0,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual raw context",
    )
    .unwrap();
    let row = V::Object(vec![
        ("_id".into(), V::String("tx-1-raw".into())),
        (
            "value".into(),
            V::Object(vec![("payload".into(), V::String("0,raw".into()))]),
        ),
    ]);
    index.import_row(&index.snapshot().unwrap(), row).unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let NativePreparedValidityQueryV1::Selected(selected) = index
        .prepare_validity_read(
            &before.metadata,
            V::String("raw".into()),
            V::number(0.),
            V::Bool(true),
            V::Undefined,
        )
        .unwrap()
    else {
        panic!("real source selection")
    };
    let NativePreparedValidityOperationV1::Write(write) =
        selected.prepare_write(&index, &before.metadata).unwrap()
    else {
        panic!("real prepared plan")
    };
    let cap = index.begin_verified_cold_execution(&before).unwrap();
    let io = index.storage_io_counters().unwrap();
    assert!(matches!(
        index.retain_verified_history_generation(
            &escrow.0,
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly
        ),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(
        std::fs::read_dir(&escrow.0).unwrap().count(),
        0,
        "cold replay cannot create escrow files as a side effect"
    );
    assert!(matches!(
        index.commit_prepared_validity_with_escrow(&write, None),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    index.finish_verified_cold_execution(&cap).unwrap();
    index.commit_prepared_validity(&write).unwrap();
    assert_eq!(
        index.snapshot().unwrap().sequence,
        before.metadata.sequence + 1
    );
}
