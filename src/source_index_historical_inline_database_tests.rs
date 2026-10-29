use super::*;
fn escrow_directory(temp: &Temp) -> PathBuf {
    let directory = temp.root.join("historical-inline-escrow");
    std::fs::create_dir(&directory).unwrap();
    directory
}
fn assert_transition(initial: Vec<V>, method: &str, arguments: Vec<V>) -> DependencyReturn {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    for row in initial {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let read = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let live = index
        .prepare_inline_database(&before.metadata, method, arguments.clone())
        .unwrap();
    let mut historical = read
        .prepare_historical_inline_database(method, arguments.clone())
        .unwrap();
    assert_eq!(historical.method(), method);
    assert_eq!(historical.arguments(), &arguments);
    assert_eq!(historical.original_read(), &before);
    assert_eq!(
        historical.durable_read_context(),
        live.durable_read_context()
    );
    assert_eq!(historical.before_state(), live.before_state());
    assert_eq!(historical.after_state(), live.after_state());
    assert_eq!(
        historical.requires_native_write(),
        live.append_plan().is_some()
    );
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    if let Some(append) = live.append_plan() {
        let expected = crate::source_main_native_intent_store::native_prepared_mutation_bytes(
            append.mutations(),
        )
        .unwrap();
        assert_eq!(historical.exact_prepared_mutations(), expected);
        assert!(historical.resolve_recorded_post().is_err());
        index.append_inline_database(&live, None).unwrap();
        let post = index.capture_verified_history_pin().unwrap();
        let post_view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap();
        historical
            .verify_recorded_native_append(&post_view, &expected)
            .unwrap();
        assert!(historical.append_verified());
    } else {
        assert!(historical
            .verify_recorded_native_append(&read, &[])
            .is_err());
        index.append_inline_database(&live, None).unwrap();
        assert_eq!(index.capture_verified_history_pin().unwrap(), before);
    }
    let expected = index.resolve_inline_database_post(&live).unwrap();
    assert_eq!(historical.resolve_recorded_post().unwrap(), expected);
    assert!(historical.resolved());
    assert!(historical.resolve_recorded_post().is_err());
    expected
}
#[test]
fn historical_raw_insert_update_queries_coercions_results_and_tlnm_match_live_source_body() {
    let raw = object(&[("_id", text("genesisBlock")), ("value", V::Promise)]);
    assert_eq!(
        assert_transition(vec![], "insertAsync", vec![raw]),
        DependencyReturn::Pending(object(&[
            ("_id", text("genesisBlock")),
            ("value", V::object())
        ]))
    );
    let existing = object(&[("_id", text("MaxHeight")), ("value", V::number(10.))]);
    assert!(matches!(
        assert_transition(
            vec![existing.clone()],
            "insertAsync",
            vec![existing.clone()]
        ),
        DependencyReturn::PendingThrow(_)
    ));
    assert_eq!(
        assert_transition(
            vec![],
            "updateAsync",
            vec![object(&[("_id", text("missing"))])]
        ),
        DependencyReturn::Pending(V::number(0.))
    );
    for value in [
        V::Undefined,
        V::number(f64::NAN),
        V::number(-0.0),
        V::number(f64::INFINITY),
        V::Array(vec![
            V::Undefined,
            V::number(f64::NEG_INFINITY),
            text("raw"),
        ]),
        object(&[("first", V::Undefined), ("second", V::number(-0.0))]),
    ] {
        assert_eq!(
            assert_transition(
                vec![existing.clone()],
                "updateAsync",
                update("MaxHeight", value)
            ),
            DependencyReturn::Pending(V::number(1.))
        );
    }
    assert_eq!(
        assert_transition(vec![], "updateAsync", update("indexExists", V::Bool(true))),
        DependencyReturn::Pending(V::number(1.))
    );
    assert_eq!(
        assert_transition(
            vec![existing.clone()],
            "updateAsync",
            vec![
                object(&[("_id", text("MaxHeight"))]),
                object(&[("replacement", text("raw"))])
            ],
        ),
        DependencyReturn::Pending(V::number(1.))
    );
    assert!(matches!(
        assert_transition(
            vec![existing],
            "updateAsync",
            vec![
                object(&[("_id", text("MaxHeight"))]),
                object(&[("_id", text("changed"))])
            ],
        ),
        DependencyReturn::PendingThrow(_)
    ));
}
#[test]
fn historical_inline_uses_actual_pruned_copy_pages_for_exact_split_and_postproof() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    for n in 0..64 {
        let row = object(&[
            ("_id", text(&format!("tx-{n:06}-raw"))),
            ("value", V::number(n as f64)),
        ]);
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let arguments = vec![object(&[
        ("_id", text("tx-000031-z")),
        ("value", text("actual source raw row")),
    ])];
    let live = index
        .prepare_inline_database(&before.metadata, "insertAsync", arguments.clone())
        .unwrap();
    assert_eq!(live.append_plan().unwrap().mutations().len(), 3);
    let expected = crate::source_main_native_intent_store::native_prepared_mutation_bytes(
        live.append_plan().unwrap().mutations(),
    )
    .unwrap();
    let context = live.durable_read_context();
    index.append_inline_database(&live, None).unwrap();
    let expected_return = index.resolve_inline_database_post(&live).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let permit = index
        .retain_verified_history_generation(
            &escrow_directory(&temp),
            &[before.clone(), post.clone()],
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let raw = permit
        .independently_retained_pin()
        .canonical_scalar_bytes()
        .unwrap();
    let escrow = SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(
        index.native_namespace(),
        &raw,
    )
    .unwrap();
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
    drop(live);
    index.journal.prune_retention().unwrap();
    let old_directory = before
        .storage
        .canonical_catalog
        .join(format!("g-{:020}", before.storage.publication.generation));
    assert!(!old_directory.exists());
    drop(index);
    drop(permit);
    let actual_before = VerifiedOriginalMetadataViewV1::open_copy(&escrow, &before).unwrap();
    let actual_post = VerifiedOriginalMetadataViewV1::open_copy(&escrow, &post).unwrap();
    let mut historical = actual_before
        .prepare_historical_inline_database("insertAsync", arguments)
        .unwrap();
    assert_eq!(historical.durable_read_context(), context);
    assert_eq!(historical.exact_prepared_mutations(), expected);
    assert!(!historical.append_verified());
    assert!(historical.resolve_recorded_post().is_err());
    historical
        .verify_recorded_native_append(&actual_post, &expected)
        .unwrap();
    assert_eq!(historical.resolve_recorded_post().unwrap(), expected_return);
    assert_eq!(actual_before.visit_rows(|_| Ok(true)).unwrap().rows, 64);
    assert_eq!(actual_post.visit_rows(|_| Ok(true)).unwrap().rows, 65);
    assert_ne!(
        actual_before.copied_file().unwrap(),
        actual_before.original().storage.canonical_catalog
    );
}
#[test]
fn historical_inline_wrong_mutation_foreign_post_and_late_extra_state_refuse_atomically() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let args = update("MaxHeight", V::number(11.));
    let live = index
        .prepare_inline_database(&before.metadata, "updateAsync", args.clone())
        .unwrap();
    let raw = crate::source_main_native_intent_store::native_prepared_mutation_bytes(
        live.append_plan().unwrap().mutations(),
    )
    .unwrap();
    let mut historical = view
        .prepare_historical_inline_database("updateAsync", args.clone())
        .unwrap();
    index.append_inline_database(&live, None).unwrap();
    let post = index.capture_verified_history_pin().unwrap();
    let post_view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &post).unwrap();
    let mut wrong = raw.clone();
    wrong[0] ^= 1;
    assert!(historical
        .verify_recorded_native_append(&post_view, &wrong)
        .is_err());
    assert!(!historical.append_verified());
    let foreign_temp = Temp::new();
    let mut foreign = new_index(&foreign_temp);
    let p = foreign
        .prepare_inline_database(&foreign.snapshot().unwrap(), "updateAsync", args)
        .unwrap();
    foreign.append_inline_database(&p, None).unwrap();
    let foreign_pin = foreign.capture_verified_history_pin().unwrap();
    let foreign_view =
        VerifiedOriginalMetadataViewV1::open_original(&mut foreign, &foreign_pin).unwrap();
    assert_eq!(
        historical.after_state(),
        &prepared_metadata_state_entries(foreign_view.raw_entries(), &[]).unwrap()
    );
    assert!(historical
        .verify_recorded_native_append(&foreign_view, &raw)
        .is_err());
    index
        .put(object(&[
            ("_id", text("unexpected")),
            ("value", V::Bool(true)),
        ]))
        .unwrap();
    let late = index.capture_verified_history_pin().unwrap();
    let late_view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &late).unwrap();
    assert!(historical
        .verify_recorded_native_append(&late_view, &raw)
        .is_err());
    assert!(!historical.append_verified());
    assert!(historical.resolve_recorded_post().is_err());
    historical
        .verify_recorded_native_append(&post_view, &raw)
        .unwrap();
    assert_eq!(
        historical.resolve_recorded_post().unwrap(),
        DependencyReturn::Pending(V::number(1.))
    );
    assert!(historical
        .verify_recorded_native_append(&post_view, &raw)
        .is_err());
    assert_eq!(index.capture_verified_history_pin().unwrap(), late);
}
#[test]
fn historical_inline_local_domains_match_live_and_visitors_retain_raw_inputs_and_copied_identity() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    for (method, args) in [
        ("findAsync", vec![]),
        (
            "updateAsync",
            vec![
                object(&[("_id", text("x"))]),
                object(&[("$inc", object(&[("value", V::number(1.))]))]),
                object(&[("upsert", V::Bool(true))]),
            ],
        ),
        (
            "updateAsync",
            vec![object(&[("other", text("x"))]), V::object()],
        ),
        ("insertAsync", vec![V::Undefined]),
        ("insertAsync", vec![V::Null; 4]),
    ] {
        let live = index
            .prepare_inline_database(&before.metadata, method, args.clone())
            .err()
            .expect("same live local refusal");
        let historical = view
            .prepare_historical_inline_database(method, args)
            .err()
            .expect("same historical local refusal");
        assert_eq!(format!("{live:?}"), format!("{historical:?}"));
    }
    let permit = index
        .retain_verified_history_generation(
            &escrow_directory(&temp),
            std::slice::from_ref(&before),
            crate::compact_journal::GenerationEscrowDurabilityV1::FileSyncOnly,
        )
        .unwrap();
    let copy =
        VerifiedOriginalMetadataViewV1::open_copy(permit.independently_retained_pin(), &before)
            .unwrap();
    let args = update(
        "MaxHeight",
        V::Array(vec![V::Undefined, V::number(f64::NAN), V::number(-0.0)]),
    );
    let historical = copy
        .prepare_historical_inline_database("updateAsync", args.clone())
        .unwrap();
    let mut fields = vec![];
    historical
        .visit_canonical_fields(&mut |k, v| {
            fields.push((k.to_owned(), v.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields
        .iter()
        .any(|(k, v)| k == "copiedGeneration" && v == &V::Bool(true)));
    assert!(fields.iter().any(|(k, _)| k == "copiedDescriptor/chunk/0"));
    assert!(fields
        .iter()
        .any(|(k, _)| k == "originalRead/storage/publication/generation"));
    assert!(fields.iter().any(|(k, v)| k == "mutations/chunk/0"
        && v == &prepared_text(prepared_hex(historical.exact_prepared_mutations()))));
    assert_eq!(
        historical.durable_read_context().get("arguments"),
        V::Array(args)
    );
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}
#[test]
fn historical_inline_scalar_visitor_streams_large_actual_pages_losslessly() {
    let temp = Temp::new();
    let mut index = new_index(&temp);
    for at in 0..20 {
        index
            .import_row(
                &index.snapshot().unwrap(),
                object(&[
                    ("_id", text(&format!("tx-{at:06}-raw"))),
                    ("value", text(&"x".repeat(60_000))),
                ]),
            )
            .unwrap();
    }
    let before = index.capture_verified_history_pin().unwrap();
    let view = VerifiedOriginalMetadataViewV1::open_original(&mut index, &before).unwrap();
    let historical = view
        .prepare_historical_inline_database("updateAsync", update("tx-000000-raw", text("changed")))
        .unwrap();
    assert!(historical.exact_prepared_mutations().len() > crate::validity_source::MAX_BYTES);
    let mut recovered = vec![];
    historical
        .visit_canonical_fields(&mut |path, value| {
            value.validate()?;
            if path.starts_with("mutations/chunk/") {
                let V::String(hex) = value else {
                    panic!("raw byte scalar");
                };
                recovered.extend(source_escrow_decode_hex(hex).unwrap());
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(recovered, historical.exact_prepared_mutations());
    assert_eq!(index.capture_verified_history_pin().unwrap(), before);
}

#[path = "source_index_historical_inline_review_tests.rs"]
mod independent_review_tests;
