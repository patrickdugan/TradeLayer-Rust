use super::*;
#[test]
fn independent_escrow_reader_verifies_unlisted_wal_points_beyond_capture_cap() {
    let temp = Temp::new();
    let mut j = store(&temp);
    let mut points = vec![];
    for n in 0..140u64 {
        put(&mut j, "raw", &n.to_be_bytes());
        points.push(j.verified_prefix_pin().unwrap());
    }
    let last = points.last().unwrap();
    let permit = j
        .retain_verified_generation(
            &temp.escrow,
            std::slice::from_ref(last),
            GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let pin = permit.independently_retained_pin().clone();
    j.configure(CompactionConfig {
        target_log_bytes: 65536,
        retained_ancestors: 0,
    })
    .unwrap();
    let plan = j.prepare_compaction().unwrap();
    j.publish_compaction_with_escrow(&plan, &permit).unwrap();
    drop(plan);
    j.prune_retention().unwrap();
    drop(j);
    drop(permit);
    for n in [0usize, 70, 138] {
        assert!(
            pin.open_historical(&points[n]).is_err(),
            "old whitelist admission stays exact"
        );
        let snapshot = pin.open_independently_pinned_prefix(&points[n]).unwrap();
        assert_eq!(snapshot.get("raw"), Some(&((n as u64).to_be_bytes())[..]));
        assert_eq!(snapshot.original, points[n]);
        assert_ne!(
            snapshot.copied_file,
            ns().path(&gen_path(
                &points[n].canonical_catalog,
                points[n].publication.generation
            ))
        );
    }
    let mut wrong = points[0].clone();
    wrong.persistent_identity.push('x');
    assert!(pin.open_independently_pinned_prefix(&wrong).is_err());
    let mut wrong = points[0].clone();
    wrong.content.chain_sha256d[0] ^= 1;
    assert!(pin.open_independently_pinned_prefix(&wrong).is_err());
}
