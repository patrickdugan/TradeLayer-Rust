use super::*;
fn fixture() -> (Temp, SourceCompactIndex) {
    let temp = Temp::new();
    let index =
        SourceCompactIndex::create(&temp.source, ns(), "actual old metadata view raw inputs")
            .unwrap();
    (temp, index)
}
#[test]
fn source_escrow_scalar_bytes_match_actual_producer_and_reject_misaligned_points() {
    let (temp, mut index) = fixture();
    index
        .import_row(&index.snapshot().unwrap(), row(1))
        .unwrap();
    let first = index.capture_verified_history_pin().unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row(2))
        .unwrap();
    let unlisted = index.capture_verified_history_pin().unwrap();
    index
        .import_row(&index.snapshot().unwrap(), row(3))
        .unwrap();
    let last = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            &[first.clone(), last.clone()],
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin();
    let raw = pin.canonical_scalar_bytes().unwrap();
    let decoded =
        SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(&ns(), &raw).unwrap();
    assert_eq!(&decoded, pin);
    assert!(!decoded.points.contains(&unlisted));
    let actual_unlisted = VerifiedOriginalMetadataViewV1::open_copy(&decoded, &unlisted).unwrap();
    assert_eq!(
        actual_unlisted.visit_rows(|_| Ok(true)).unwrap(),
        unlisted.rows
    );
    assert_eq!(actual_unlisted.get("tx-000002-raw").unwrap(), Some(row(2)));
    assert!(actual_unlisted.get("tx-000003-raw").unwrap().is_none());
    let mut forged = pin.clone();
    forged.points.reverse();
    let wrong = forged.canonical_scalar_bytes().unwrap();
    assert!(SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(&ns(), &wrong).is_err());
    let mut extra = raw.clone();
    extra.push(0);
    assert!(SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(&ns(), &extra).is_err());
    assert!(
        SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(
            &ns(),
            &vec![0; MAX_SOURCE_ESCROW_SCALAR_BYTES + 1]
        )
        .is_err()
    );
    assert!(
        SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(
            &Namespace::new("BTC", false, "txIndex").unwrap(),
            &raw
        )
        .is_err()
    );
}
#[test]
fn copied_old_view_survives_real_original_prune_and_retains_distinct_identities_beyond_1024() {
    let (temp, mut index) = fixture();
    for n in 0..1100 {
        index
            .import_row(&index.snapshot().unwrap(), row(n))
            .unwrap();
    }
    let old = index.capture_verified_history_pin().unwrap();
    let before = VerifiedOriginalMetadataViewV1::open_original(&mut index, &old).unwrap();
    assert!(before.copied_file().is_none());
    assert_eq!(before.get("tx-000017-raw").unwrap(), Some(row(17)));
    let mut changed = row(17);
    changed.insert("changed", V::Bool(true)).unwrap();
    index.put(changed).unwrap();
    let current = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            &[old.clone(), current.clone()],
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let raw = permit
        .independently_retained_pin()
        .canonical_scalar_bytes()
        .unwrap();
    let pin = SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(&ns(), &raw).unwrap();
    let copy = VerifiedOriginalMetadataViewV1::open_copy(&pin, &old).unwrap();
    assert_eq!(copy.original(), &old);
    assert_eq!(copy.get("tx-000017-raw").unwrap(), Some(row(17)));
    assert!(copy.copied_file().is_some());
    assert_eq!(
        copy.copied_generation().unwrap().original_current,
        current.storage
    );
    assert_eq!(
        copy.visit_rows(|_| Ok(true)).unwrap(),
        before.visit_rows(|_| Ok(true)).unwrap()
    );
    drop(before);
    drop(copy);
    index
        .journal
        .configure(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let compaction = index.journal.prepare_compaction().unwrap();
    index
        .journal
        .publish_compaction_with_escrow(&compaction, &permit.storage)
        .unwrap();
    drop(compaction);
    index.journal.prune_retention().unwrap();
    assert!(index.read_verified_history_generation(&old).is_err());
    let original_dir = old
        .storage
        .canonical_catalog
        .join(format!("g-{:020}", old.storage.publication.generation));
    assert!(
        !original_dir.exists(),
        "actual original generation really pruned"
    );
    drop(index);
    drop(permit);
    let view = VerifiedOriginalMetadataViewV1::open_copy(&pin, &old).unwrap();
    let mut count = 0;
    let receipt = view
        .visit_rows(|r| {
            assert_eq!(r, &row(count));
            count += 1;
            Ok(true)
        })
        .unwrap();
    assert_eq!(count, 1100);
    assert_eq!(receipt, old.rows);
    assert_eq!(view.original(), &old);
    assert_ne!(view.copied_file().unwrap(), ns().path(&original_dir));
    assert!(view.get("absent").unwrap().is_none());
}
#[test]
fn copied_view_requires_actual_manifest_and_independent_row_receipt() {
    let (temp, mut index) = fixture();
    index
        .import_row(&index.snapshot().unwrap(), row(1))
        .unwrap();
    let point = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&point),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin().clone();
    let mut wrong = point.clone();
    wrong.rows.chain_sha256d[0] ^= 1;
    assert!(VerifiedOriginalMetadataViewV1::open_copy(&pin, &wrong).is_err());
    let manifest = pin.storage.copied_generation.join("GENINFO");
    let raw = std::fs::read(&manifest).unwrap();
    let mut broken = raw.clone();
    broken[0] ^= 1;
    std::fs::write(&manifest, &broken).unwrap();
    assert!(VerifiedOriginalMetadataViewV1::open_copy(&pin, &point).is_err());
    std::fs::write(&manifest, raw).unwrap();
    assert_eq!(
        VerifiedOriginalMetadataViewV1::open_copy(&pin, &point)
            .unwrap()
            .get("tx-000001-raw")
            .unwrap(),
        Some(row(1))
    );
}
