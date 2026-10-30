use super::*;
fn setup_read() -> (Temp, SourceCompactIndex) {
    let temp = Temp::new();
    let index =
        SourceCompactIndex::create(&temp.source, ns(), "actual old-view Main read raw inputs")
            .unwrap();
    (temp, index)
}
fn all_rows(read: &MainIndexReadViewV1) -> (Vec<V>, RowStreamReceipt) {
    let mut cursor = read.cursor();
    let mut rows = vec![];
    while let Some(row) = cursor.next_row().unwrap() {
        rows.push(row);
    }
    (rows, cursor.receipt())
}
#[test]
fn shared_original_main_read_keeps_exact_rows_grouping_and_fixed_snapshot_during_same_index_writes()
{
    let (_temp, mut index) = setup_read();
    for n in 0..40 {
        index
            .import_row(&index.snapshot().unwrap(), row(n))
            .unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let snapshot = index.read_generation(&before.metadata).unwrap();
    let read = MainIndexReadViewV1::from_original(snapshot.clone());
    let (other, receipt) = all_rows(&read);
    assert_eq!(other, (0..40).map(row).collect::<Vec<_>>());
    assert_eq!(receipt, before.rows);
    assert!(read.copied_file().is_none());
    let original = snapshot.plan_main_groups().unwrap();
    let MainIndexPlanningOutcome::Ready(original) = original else {
        panic!("raw source grouping");
    };
    let MainIndexPlanningOutcome::Ready(shared) = read.plan_main_groups().unwrap() else {
        panic!("shared source grouping");
    };
    assert_eq!(original.receipt(), shared.receipt());
    assert_eq!(original.max_index_block(), shared.max_index_block());
    let mut changed = row(17);
    changed.insert("changed", V::Bool(true)).unwrap();
    index.put(changed.clone()).unwrap();
    let mut old = shared.phase(17, false);
    assert_eq!(old.next_row().unwrap(), Some(row(17)));
    assert!(old.next_row().unwrap().is_none());
    let fresh = MainIndexReadViewV1::from_original(
        index.read_generation(&index.snapshot().unwrap()).unwrap(),
    );
    assert_eq!(
        read.source_database_read(
            "findOneAsync",
            &[V::Object(vec![(
                "_id".into(),
                V::String("tx-000017-raw".into())
            )])]
        )
        .unwrap(),
        DependencyReturn::Pending(row(17))
    );
    assert_eq!(
        fresh
            .source_database_read(
                "findOneAsync",
                &[V::Object(vec![(
                    "_id".into(),
                    V::String("tx-000017-raw".into())
                )])]
            )
            .unwrap(),
        DependencyReturn::Pending(changed)
    );
    assert_eq!(all_rows(&read).1, before.rows);
}
#[test]
fn copied_main_read_streams_all1100_actual_rows_after_original_prune_with_copy_identity_separate() {
    let (temp, mut index) = setup_read();
    for n in 0..1100 {
        index
            .import_row(&index.snapshot().unwrap(), row(n))
            .unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let original =
        MainIndexReadViewV1::from_original(index.read_generation(&before.metadata).unwrap());
    assert!(matches!(
        original.source_database_read("findAsync", &[V::object()]),
        Err(CompactIndexError::Limit)
    ));
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let expected = permit.independently_retained_pin().clone();
    drop(original);
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let compact = index.journal.prepare_compaction().unwrap();
    index
        .journal
        .publish_compaction_with_escrow(&compact, &permit.storage)
        .unwrap();
    drop(compact);
    index.journal.prune_retention().unwrap();
    let old = before
        .storage
        .canonical_catalog
        .join(format!("g-{:020}", before.storage.publication.generation));
    assert!(!old.exists());
    drop(index);
    drop(permit);
    let actual = VerifiedOriginalMetadataViewV1::open_copy(&expected, &before).unwrap();
    let read = MainIndexReadViewV1::from_verified(actual);
    assert!(read.copied_file().is_some());
    assert_eq!(read.original_history(), Some(&before));
    assert_eq!(read.pin(), &before.metadata);
    let (rows, receipt) = all_rows(&read);
    assert_eq!(rows, (0..1100).map(row).collect::<Vec<_>>());
    assert_eq!(receipt, before.rows);
    assert_eq!(
        prepared_storage_receipt(&read.complete_metadata_state().unwrap()),
        before.storage.content
    );
    assert!(matches!(
        read.source_database_read("findAsync", &[V::object()]),
        Err(CompactIndexError::Limit)
    ));
    let MainIndexPlanningOutcome::Ready(plan) = read.plan_main_groups().unwrap() else {
        panic!("actual copied source grouping");
    };
    assert_eq!(plan.receipt(), &before.rows);
    assert_eq!(plan.max_index_block(), V::number(1099.));
    for height in [0, 17, 1099] {
        let mut cursor = plan.phase(height, false);
        assert_eq!(cursor.next_row().unwrap(), Some(row(height)));
        assert!(cursor.next_row().unwrap().is_none());
    }
}
#[test]
fn generic_read_only_database_matches_original_coercions_and_never_exposes_metadata_write() {
    let (_temp, mut index) = setup_read();
    index
        .import_row(&index.snapshot().unwrap(), row(1))
        .unwrap();
    let pin = index.capture_verified_history_pin().unwrap();
    let snapshot = index.read_generation(&pin.metadata).unwrap();
    let read = MainIndexReadViewV1::from_original(snapshot.clone());
    for (method, args) in [
        (
            "findOneAsync",
            vec![V::Object(vec![("_id".into(), V::String("missing".into()))])],
        ),
        ("findAsync", vec![V::object()]),
        ("findOneAsync", vec![V::object()]),
    ] {
        assert_eq!(
            read.source_database_read(method, &args).unwrap(),
            snapshot.source_database_read(method, &args).unwrap()
        );
    }
    assert!(matches!(
        read.source_database_read("updateAsync", &[]),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), pin);
}
#[test]
fn copied_validity_playback_recomputes_actual_transition_on_same_guarded_final_index_without_append(
) {
    let (temp, mut index) = setup_read();
    index
        .import_row(&index.snapshot().unwrap(), row(10))
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let args = [
        V::String("raw".into()),
        V::number(1.),
        V::Bool(true),
        V::Undefined,
    ];
    let original = index
        .native_stream_upsert_validity(
            &index.snapshot().unwrap(),
            args[0].clone(),
            args[1].clone(),
            args[2].clone(),
            args[3].clone(),
            &mut |_| Ok(None),
        )
        .unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &temp.escrow,
            &[before.clone(), post.clone()],
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let expected = permit.independently_retained_pin().clone();
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 0,
        })
        .unwrap();
    let compact = index.journal.prepare_compaction().unwrap();
    index
        .journal
        .publish_compaction_with_escrow(&compact, &permit.storage)
        .unwrap();
    drop(compact);
    index.journal.prune_retention().unwrap();
    let final_post = index.capture_verified_history_pin().unwrap();
    assert_ne!(final_post.storage.publication, post.storage.publication);
    assert!(
        NativeHistoricalValidityPlaybackV1::open(&mut index, &[before.clone(), post.clone()])
            .is_err()
    );
    let first = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_copy(&expected, &before).unwrap(),
    );
    let next = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_copy(&expected, &post).unwrap(),
    );
    let capability = index.begin_verified_cold_execution(&final_post).unwrap();
    let io = index.storage_io_counters().unwrap();
    let mut playback = NativeHistoricalValidityPlaybackV1::open_verified_views(
        &mut index,
        &capability,
        &[before, post.clone()],
        vec![first, next],
    )
    .unwrap();
    let actual = playback
        .verify_source_update(
            args[0].clone(),
            args[1].clone(),
            args[2].clone(),
            args[3].clone(),
            &mut |_| Ok(None),
        )
        .unwrap();
    assert_eq!(actual, original);
    assert!(playback.complete());
    assert_eq!(playback.current(), &post.metadata);
    drop(playback);
    assert_eq!(index.capture_verified_history_pin().unwrap(), final_post);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(io.confirmed_append_bytes, after.confirmed_append_bytes);
    assert_eq!(io.copied_checkpoint_bytes, after.copied_checkpoint_bytes);
    assert_eq!(io.published_compactions, after.published_compactions);
    assert!(matches!(
        index.native_database(
            "updateAsync",
            &[
                V::Object(vec![("_id".into(), V::String("new-native-write".into()))]),
                V::Object(vec![(
                    "$set".into(),
                    V::Object(vec![("value".into(), V::Bool(true))])
                )]),
                V::Object(vec![("upsert".into(), V::Bool(true))]),
            ]
        ),
        Err(CompactIndexError::ColdWriteRefused)
    ));
}

#[path = "source_index_mode4_review_tests.rs"]
mod reviewer_controls;
