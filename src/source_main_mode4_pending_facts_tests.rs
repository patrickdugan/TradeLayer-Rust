use super::*;
use crate::source_compact_index_v2::{MainIndexReadViewV1, NativeHistoricalValidityPlaybackV1};
#[test]
fn actual_future_append_metadata_reproduces_complete_live_pending_fields_without_plan_or_io() {
    let t = Temp::new();
    let mut index = metadata(&t, false);
    let mut f = fixture(&t, &index);
    f.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let source = view
        .prepare_historical_inline_database("updateAsync", arguments())
        .unwrap();
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments())
        .unwrap();
    let mut expected = vec![];
    live.visit_canonical_fields(&mut |path, value| expected.push((path.to_owned(), value.clone())))
        .unwrap();
    let p = f.begin(2, Action::AppendPending, &index, Some(&live));
    index.append_inline_database(&live, None).unwrap();
    f.finish(p, &index);
    let pin = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    drop(f);
    let reader = VerifiedInlineActiveServiceAckHistoryV1::open(&t.wal, wal_ns(), &pin).unwrap();
    assert_eq!(
        reader.original_observation_bytes().unwrap(),
        b"opaque raw constructor observation"
    );
    assert!(reader
        .entries()
        .iter()
        .all(|e| !reader.entry_has_native_recovery(e).unwrap()));
    let post = index.capture_verified_history_pin().unwrap();
    let io = index.storage_io_counters().unwrap();
    let mut pending = reader
        .prepare_pending_inline(&mut index, 1, source)
        .unwrap();
    assert_eq!(pending.append_sequence(), Some(2));
    assert!(!pending.source().append_verified());
    let mut actual = vec![];
    pending
        .visit_live_fields(&mut |p, v| actual.push((p.to_owned(), v.clone())))
        .unwrap();
    assert_eq!(actual, expected);
    let cap = index.begin_verified_cold_execution(&post).unwrap();
    let views = vec![
        MainIndexReadViewV1::from_verified(
            reader.checked_original_view(&mut index, &before).unwrap(),
        ),
        MainIndexReadViewV1::from_verified(
            reader.checked_original_view(&mut index, &post).unwrap(),
        ),
    ];
    let mut playback = NativeHistoricalValidityPlaybackV1::open_verified_views(
        &mut index,
        &cap,
        &[before.clone(), post.clone()],
        views,
    )
    .unwrap();
    reader
        .verify_inline_operation(
            playback.scoped_cold_index(&cap).unwrap(),
            2,
            pending.source_mut(),
        )
        .unwrap();
    playback
        .install_verified_inline_post(pending.source())
        .unwrap();
    assert_eq!(playback.source_history_pin(), &post);
    assert!(playback.complete());
    drop(playback);
    assert_eq!(
        pending.source_mut().resolve_recorded_post().unwrap(),
        index.resolve_inline_database_post(&live).unwrap()
    );
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
}
#[test]
fn never_appended_old_pending_metadata_is_not_recovered_from_expected_observation() {
    let t = Temp::new();
    let mut index = metadata(&t, false);
    let mut f = fixture(&t, &index);
    f.start(&index);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let source = view
        .prepare_historical_inline_database("updateAsync", arguments())
        .unwrap();
    let pin = f.store.capture_inline_cold_pin(&meta_ns()).unwrap();
    drop(f);
    let reader = VerifiedInlineActiveServiceAckHistoryV1::open(&t.wal, wal_ns(), &pin).unwrap();
    assert!(reader
        .prepare_pending_inline(&mut index, 1, source)
        .is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    // An actual no-match update has no predicted physical plan. Its complete
    // live fields derive only from raw method/args and the actual metadata.
    let args = vec![
        obj(&[("_id", s("absent"))]),
        obj(&[("$set", obj(&[("value", V::number(7.))]))]),
        obj(&[("upsert", V::Bool(false))]),
    ];
    let source = view
        .prepare_historical_inline_database("updateAsync", args.clone())
        .unwrap();
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", args)
        .unwrap();
    let mut pending = reader
        .prepare_pending_inline(&mut index, 1, source)
        .unwrap();
    assert_eq!(pending.append_sequence(), None);
    let (mut expected, mut actual) = (vec![], vec![]);
    live.visit_canonical_fields(&mut |p, v| expected.push((p.to_owned(), v.clone())))
        .unwrap();
    pending
        .visit_live_fields(&mut |p, v| actual.push((p.to_owned(), v.clone())))
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        pending.source_mut().resolve_recorded_post().unwrap(),
        index.resolve_inline_database_post(&live).unwrap()
    );
}
