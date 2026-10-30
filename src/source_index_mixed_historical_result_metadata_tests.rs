// Child of actual mixed-validity metadata tests. Query/update arguments here
// are metadata API inputs, not supplied Main/financial outputs.
use super::*;
#[test]
fn mixed_historical_known_error_requires_actual_post_and_one_exact_resolve_marker() {
    let t = Temp::new();
    let mut index = make_index(&t);
    let write = plan(&index);
    let before = index.capture_verified_history_pin().unwrap();
    index.commit_prepared_validity(&write).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let before_view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let post_view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap();
    let capability = index.begin_verified_cold_execution(&post).unwrap();
    let io = index.storage_io_counters().unwrap();
    let before_pin = before.clone();
    let post_pin = post.clone();
    let mut playback = NativeHistoricalValidityPlaybackV1::open_verified_views(
        &mut index,
        &capability,
        &[before, post],
        vec![
            MainIndexReadViewV1::from_verified(before_view),
            MainIndexReadViewV1::from_verified(post_view),
        ],
    )
    .unwrap();
    let NativeHistoricalValidityOperationV1::Await(mut awaiting) = playback
        .prepare_source_update(
            text("id"),
            V::number(1.),
            V::Bool(true),
            V::Undefined,
            &mut |_| Ok(None),
        )
        .unwrap()
    else {
        panic!("actual selected metadata await")
    };
    assert_eq!(
        awaiting.durable_read_context(),
        write.durable_read_context()
    );
    assert!(awaiting
        .retain_mixed_recorded_error("private typed-error marker".into())
        .is_err());
    let actual_before = VerifiedOriginalMetadataViewV1::open_original(
        playback.scoped_cold_index(&capability).unwrap(),
        &before_pin,
    )
    .unwrap();
    let actual_post = VerifiedOriginalMetadataViewV1::open_original(
        playback.scoped_cold_index(&capability).unwrap(),
        &post_pin,
    )
    .unwrap();
    let mutations = awaiting.exact_prepared_mutations().to_vec();
    assert!(awaiting
        .verify_independent_mixed_post(&actual_before, &mutations)
        .is_err());
    assert!(awaiting
        .install_mixed_write_context(&actual_post, &actual_post, &mutations)
        .is_err());
    awaiting
        .install_mixed_write_context(&actual_before, &actual_post, &mutations)
        .unwrap();
    assert_eq!(
        awaiting.durable_read_context(),
        write.durable_read_context()
    );
    let raw = awaiting.exact_prepared_mutations().to_vec();
    playback
        .verify_recorded_native_append(&mut awaiting, &raw)
        .unwrap();
    awaiting
        .retain_mixed_recorded_error("private typed-error marker".into())
        .unwrap();
    assert!(!awaiting.take_mixed_recorded_error("wrong marker"));
    assert!(awaiting.matches_mixed_recorded_error("private typed-error marker"));
    assert!(awaiting.take_mixed_recorded_error("private typed-error marker"));
    assert!(!awaiting.take_mixed_recorded_error("private typed-error marker"));
    playback
        .resolve_recorded_native_post(&mut awaiting)
        .unwrap();
    assert!(playback.complete());
    drop(playback);
    let after = index.storage_io_counters().unwrap();
    assert_eq!(after.confirmed_append_bytes, io.confirmed_append_bytes);
    assert_eq!(after.copied_checkpoint_bytes, io.copied_checkpoint_bytes);
    assert_eq!(after.published_compactions, io.published_compactions);
}
