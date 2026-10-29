use super::*;

fn fields(plan: &NativeHistoricalInlineDatabaseV1) -> Vec<(String, V)> {
    let mut fields = vec![];
    plan.visit_canonical_fields(&mut |path, value| {
        fields.push((path.into(), value.clone()));
        Ok(())
    })
    .unwrap();
    fields
}

#[test]
fn same_sequence_same_directory_extra_page_change_refuses_before_phase_or_io_changes() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    for (id, value) in [("MaxHeight", 10.), ("unselected", 7.)] {
        index
            .import_row(
                &index.snapshot().unwrap(),
                object(&[("_id", text(id)), ("value", V::number(value))]),
            )
            .unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let old = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let arguments = update("MaxHeight", V::number(11.));
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", arguments.clone())
        .unwrap();
    let mut historical = old
        .prepare_historical_inline_database("updateAsync", arguments)
        .unwrap();
    let expected = historical.exact_prepared_mutations().to_vec();
    let mut altered = live.append_plan().unwrap().mutations().to_vec();
    let mut changed = 0;
    for mutation in &mut altered {
        if let Mutation::Put { key, value } = mutation {
            if key != DIRECTORY {
                let mut rows = decode_page(value).unwrap();
                for row in &mut rows {
                    if row.get("_id") == text("unselected") {
                        row.insert("value", V::number(8.)).unwrap();
                        changed += 1;
                    }
                }
                *value = encode_page(&rows).unwrap();
            }
        }
    }
    assert_eq!(changed, 1);
    // Actual native writer encodes/syncs one valid-CRC alternative operation.
    // It retains the expected selected change but also changes an unrelated row.
    index.journal.commit(altered).unwrap();
    index.directory = live.put.as_ref().unwrap().directory.clone();
    let actual_post = index.capture_verified_history_pin().unwrap();
    let actual_view =
        VerifiedOriginalMetadataViewV1::open_original(&mut index, &actual_post).unwrap();
    assert_eq!(actual_post.metadata.sequence, before.metadata.sequence + 1);
    assert_eq!(
        actual_view.directory().encode().unwrap(),
        historical.directory.encode().unwrap()
    );
    let actual_state = prepared_metadata_state_entries(actual_view.raw_entries(), &[]).unwrap();
    assert_eq!(actual_state.profile, historical.after_state().profile);
    assert_eq!(actual_state.keys, historical.after_state().keys);
    assert_eq!(actual_state.bytes, historical.after_state().bytes);
    assert_eq!(actual_state.complete, historical.after_state().complete);
    assert_ne!(
        actual_state.chain_sha256d,
        historical.after_state().chain_sha256d
    );
    let before_fields = fields(&historical);
    assert!(matches!(
        historical.verify_recorded_native_append(&actual_view, &expected),
        Err(CompactIndexError::WrongContext)
    ));
    assert_eq!(fields(&historical), before_fields);
    assert!(!historical.append_verified());
    assert!(historical.resolve_recorded_post().is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), actual_post);
}

#[test]
fn resolving_metadata_transition_or_no_write_result_never_releases_live_cold_guard() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    let before = index.capture_verified_history_pin().unwrap();
    let old = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let args = update("MaxHeight", V::number(11.));
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", args.clone())
        .unwrap();
    let mut historical = old
        .prepare_historical_inline_database("updateAsync", args)
        .unwrap();
    let raw = historical.exact_prepared_mutations().to_vec();
    index.append_inline_database(&live, None).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let actual = VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap();
    let capability = index.begin_verified_cold_execution(&post).unwrap();
    historical
        .verify_recorded_native_append(&actual, &raw)
        .unwrap();
    assert_eq!(
        historical.resolve_recorded_post().unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    assert!(historical.resolve_recorded_post().is_err());
    let mut no_write = actual
        .prepare_historical_inline_database(
            "updateAsync",
            vec![object(&[("_id", text("missing"))])],
        )
        .unwrap();
    assert!(!no_write.requires_native_write());
    let original = fields(&no_write);
    assert!(no_write
        .verify_recorded_native_append(&actual, &[])
        .is_err());
    assert_eq!(fields(&no_write), original);
    assert_eq!(
        no_write.resolve_recorded_post().unwrap(),
        DependencyReturn::Pending(V::number(0.))
    );
    assert!(no_write.resolve_recorded_post().is_err());
    let future = index
        .prepare_inline_database(
            &post.metadata,
            "updateAsync",
            update("MaxHeight", V::number(12.)),
        )
        .unwrap();
    assert!(matches!(
        index.append_inline_database(&future, None),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert!(matches!(
        index.put(object(&[("_id", text("fresh"))])),
        Err(CompactIndexError::ColdWriteRefused)
    ));
    assert_eq!(index.capture_verified_history_pin().unwrap(), post);
    index.finish_verified_cold_execution(&capability).unwrap();
    index.append_inline_database(&future, None).unwrap();
    assert_eq!(
        index.get("MaxHeight").unwrap().unwrap().get("value"),
        V::number(12.)
    );
}
