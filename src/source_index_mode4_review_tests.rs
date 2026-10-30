use super::*;

#[test]
fn reviewer_verified_playback_refuses_foreign_capability_and_equal_rows_foreign_view_before_io() {
    let (_a, mut a) = setup_read();
    let (_b, mut b) = setup_read();
    for index in [&mut a, &mut b] {
        index
            .import_row(&index.snapshot().unwrap(), row(7))
            .unwrap();
    }
    let pa = a.capture_verified_history_pin().unwrap();
    let pb = b.capture_verified_history_pin().unwrap();
    assert_eq!(pa.rows.rows, pb.rows.rows);
    assert_eq!(pa.rows.chain_sha256d, pb.rows.chain_sha256d);
    // Full row receipts also bind their distinct actual catalog snapshots.
    assert_ne!(pa.rows, pb.rows);
    assert_eq!(pa.storage.content, pb.storage.content);
    assert_ne!(
        pa.storage.persistent_identity,
        pb.storage.persistent_identity
    );
    let va = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_original(&mut a, &pa).unwrap(),
    );
    let vb = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_original(&mut b, &pb).unwrap(),
    );
    let ca = a.begin_verified_cold_execution(&pa).unwrap();
    let cb = b.begin_verified_cold_execution(&pb).unwrap();
    let io = b.storage_io_counters().unwrap();
    assert!(matches!(
        NativeHistoricalValidityPlaybackV1::open_verified_views(
            &mut b,
            &ca,
            std::slice::from_ref(&pb),
            vec![vb.clone()]
        ),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(matches!(
        NativeHistoricalValidityPlaybackV1::open_verified_views(
            &mut b,
            &cb,
            std::slice::from_ref(&pb),
            vec![va]
        ),
        Err(CompactIndexError::WrongContext)
    ));
    assert_eq!(b.capture_verified_history_pin().unwrap(), pb);
    let after = b.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
    let playback = NativeHistoricalValidityPlaybackV1::open_verified_views(
        &mut b,
        &cb,
        std::slice::from_ref(&pb),
        vec![vb],
    )
    .unwrap();
    assert!(playback.complete());
    drop(playback);
    assert_eq!(b.capture_verified_history_pin().unwrap(), pb);
}

#[test]
fn reviewer_original_unbranded_and_reordered_views_do_not_gain_verified_admission() {
    let (_temp, mut index) = setup_read();
    index
        .import_row(&index.snapshot().unwrap(), row(3))
        .unwrap();
    let before = index.capture_verified_history_pin().unwrap();
    let first = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap(),
    );
    let unbranded =
        MainIndexReadViewV1::from_original(index.read_generation(&before.metadata).unwrap());
    index
        .import_row(&index.snapshot().unwrap(), row(4))
        .unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let last = MainIndexReadViewV1::from_verified(
        VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap(),
    );
    let cap = index.begin_verified_cold_execution(&post).unwrap();
    let io = index.storage_io_counters().unwrap();
    assert!(matches!(
        NativeHistoricalValidityPlaybackV1::open_verified_views(
            &mut index,
            &cap,
            std::slice::from_ref(&before),
            vec![unbranded]
        ),
        Err(CompactIndexError::WrongContext)
    ));
    assert!(matches!(
        NativeHistoricalValidityPlaybackV1::open_verified_views(
            &mut index,
            &cap,
            &[before.clone(), post.clone()],
            vec![last.clone(), first.clone()]
        ),
        Err(CompactIndexError::WrongContext)
    ));
    let playback = NativeHistoricalValidityPlaybackV1::open_verified_views(
        &mut index,
        &cap,
        &[before, post.clone()],
        vec![first, last],
    )
    .unwrap();
    assert!(!playback.complete());
    drop(playback);
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
}
