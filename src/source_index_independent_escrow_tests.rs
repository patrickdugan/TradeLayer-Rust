use super::*;
#[test]
fn source_independent_escrow_reader_verifies_original_row_receipt_and_actual_raw_pages() {
    let temp = Temp::new();
    let mut index =
        SourceCompactIndex::create(&temp.source, ns(), "actual raw metadata independent escrow")
            .unwrap();
    let mut points = vec![];
    for n in 0..132 {
        index
            .import_row(&index.snapshot().unwrap(), row(n))
            .unwrap();
        points.push(index.capture_verified_history_pin().unwrap());
    }
    let last = points.last().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(last),
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
    assert!(pin.open_historical(&points[0]).is_err());
    let snapshot = pin.open_independently_pinned_history(&points[0]).unwrap();
    assert!(snapshot.raw_get(DIRECTORY).is_some());
    assert_eq!(snapshot.raw_entries().count(), 2);
    assert_eq!(
        snapshot
            .visit_rows(|actual| {
                assert_eq!(actual, &row(0));
                Ok(true)
            })
            .unwrap(),
        points[0].rows
    );
    assert_eq!(snapshot.original(), &points[0]);
    let mut wrong = points[0].clone();
    wrong.rows.chain_sha256d[0] ^= 1;
    assert!(pin.open_independently_pinned_history(&wrong).is_err());
}
