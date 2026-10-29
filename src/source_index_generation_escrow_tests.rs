use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    source: PathBuf,
    escrow: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("NATIVE_ESCROW_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-source-index-escrow-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("source");
        let escrow = root.join("escrow");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&escrow).unwrap();
        Self {
            root,
            source: source.canonicalize().unwrap(),
            escrow: escrow.canonicalize().unwrap(),
        }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), self.root.parent());
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn ns() -> Namespace {
    Namespace::new("LTC", false, "txIndex").unwrap()
}
fn row(n: u64) -> V {
    V::Object(vec![
        ("_id".into(), V::String(format!("tx-{n:06}-raw"))),
        (
            "value".into(),
            V::Object(vec![(
                "payload".into(),
                V::String("literal raw input".into()),
            )]),
        ),
    ])
}
#[test]
fn source_escrow_full_row_pin_stream_survives_original_generation_prune() {
    let temp = Temp::new();
    let mut index =
        SourceCompactIndex::create(&temp.source, ns(), "actual raw metadata escrow").unwrap();
    for n in 0..1100 {
        index
            .import_row(&index.snapshot().unwrap(), row(n))
            .unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin().clone();
    index
        .journal
        .configure(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let plan = index.journal.prepare_compaction().unwrap();
    index
        .journal
        .publish_compaction_with_escrow(&plan, &permit.storage)
        .unwrap();
    drop(plan);
    index.journal.prune_retention().unwrap();
    drop(index);
    drop(permit);
    let snapshot = pin.open_historical(&before).unwrap();
    let mut count = 0;
    let receipt = snapshot
        .visit_rows(|actual| {
            assert_eq!(actual, &row(count));
            count += 1;
            Ok(true)
        })
        .unwrap();
    assert_eq!(count, 1100);
    assert_eq!(receipt, before.rows);
    assert_ne!(
        snapshot.copied_file(),
        ns().path(&before.storage.canonical_catalog).as_path()
    );
    let mut wrong = before.clone();
    wrong.rows.chain_sha256d[0] ^= 1;
    let mut forged = pin.clone();
    forged.points = vec![wrong.clone()];
    assert!(forged.open_historical(&wrong).is_err());
}
#[test]
fn source_escrow_rejects_partial_row_pin_and_extra_page_expectation() {
    let temp = Temp::new();
    let mut index = SourceCompactIndex::create(&temp.source, ns(), "actual raw metadata").unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row(1))
        .unwrap();
    let point = index.capture_verified_history_pin().unwrap();
    let mut partial = point.clone();
    partial.rows.complete = false;
    assert!(index
        .retain_verified_history_generation(
            &temp.escrow,
            &[partial],
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly
        )
        .is_err());
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&point),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let mut pin = permit.independently_retained_pin().clone();
    let mut bad = point.clone();
    bad.metadata.context.push('x');
    bad.rows.snapshot = bad.metadata.clone();
    pin.points = vec![bad.clone()];
    assert!(pin.open_historical(&bad).is_err());
}

#[path = "source_index_independent_escrow_tests.rs"]
mod independent_prefix_tests;

#[path = "source_index_inline_escrow_guard_tests.rs"]
mod inline_escrow_guard_tests;
