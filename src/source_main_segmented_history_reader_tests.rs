use super::*;
fn comparison_fixture_store(t: &Temp) -> ActiveServiceWalV1 {
    let archive = b"metadata fixture, not a financial archive".to_vec();
    let boundary = b"metadata fixture, not a native history pin".to_vec();
    let observation = b"opaque comparison fixture only".to_vec();
    let origin = obj(&[
        ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
        (
            "archive",
            ActiveServiceWalBlobV1::of(&archive).unwrap().value(),
        ),
        (
            "observation",
            ActiveServiceWalBlobV1::of(&observation).unwrap().value(),
        ),
        (
            "requiredVerifiedHistory",
            ActiveServiceWalBlobV1::of(&boundary).unwrap().value(),
        ),
        (
            "underlyingSourceHistoryProfile",
            s(ENHANCED_HISTORY_PROFILE),
        ),
        ("acknowledgedWindowLimit", s("256")),
    ]);
    let store = ActiveServiceWalV1::create(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        origin,
        SourceDurableFilesystem::FileSyncOnly,
    )
    .unwrap();
    // These exact same content-addressed blobs are stored at the first real
    // generic metadata publication; no source constructor/output is involved.
    for value in [archive, boundary, observation] {
        store.write_blob(&value).unwrap();
    }
    store
}
fn fixture_action(
    store: &mut ActiveServiceWalV1,
    previous: Option<&ActiveHistorySegmentCursorV1>,
    entries: &mut Vec<ActiveServiceWalBlobV1>,
    skip_accepted: bool,
    no_op_append: bool,
) {
    let sequence = previous.map_or(0, |c| c.actions) + entries.len() as u64 + 1;
    let prior = entries
        .last()
        .cloned()
        .or_else(|| previous.map(|c| c.last_entry.clone()));
    let prior_head = if let Some(p) = &prior {
        let v = parse(&store.read_blob(p).unwrap()).unwrap();
        let mut c = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
        c.extend_from_slice(&segment_hash(&v.get("previousHead")).unwrap());
        c.extend_from_slice(&segment_count(&v.get("sequence")).unwrap().to_be_bytes());
        c.extend_from_slice(&p.sha256);
        sha256(&c)
    } else {
        sha256(&bytes(&store.origin_context().unwrap().get("origin")).unwrap())
    };
    let action = if sequence == 1 {
        crate::source_main_native_session::NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents: V::Array(vec![]),
        }
    } else if no_op_append {
        crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending
    } else {
        crate::source_main_native_session::NativeMainActiveServiceActionV1::Pause
    };
    let action = action.canonical_action_bytes().unwrap();
    let before = b"opaque Before comparison".to_vec();
    let fields = b"opaque Accepted comparison";
    let diagnostic = bytes(&obj(&[("apiRejection", V::Null)])).unwrap();
    let mut accepted = b"TLMAP\x01".to_vec();
    accepted.extend_from_slice(&(fields.len() as u64).to_be_bytes());
    accepted.extend_from_slice(fields);
    accepted.extend_from_slice(&(diagnostic.len() as u64).to_be_bytes());
    accepted.extend_from_slice(&diagnostic);
    let boundary = b"metadata fixture, not a native history pin".to_vec();
    let source = obj(&[
        ("actions", s(sequence.to_string())),
        ("commitment", s(hex(&sha256(&sequence.to_be_bytes())))),
    ]);
    let segment = previous.map_or(V::Null, |c| c.segment.value());
    let mut entry = obj(&[
        ("profile", s(SEGMENTED_ACTIVE_HISTORY_PROFILE)),
        ("sequence", s(sequence.to_string())),
        (
            "previousEntry",
            prior
                .as_ref()
                .map_or(V::Null, ActiveServiceWalBlobV1::value),
        ),
        ("previousHead", s(hex(&prior_head))),
        (
            "action",
            ActiveServiceWalBlobV1::of(&action).unwrap().value(),
        ),
        (
            "before",
            ActiveServiceWalBlobV1::of(&before).unwrap().value(),
        ),
        ("nativeCandidates", V::Null),
        ("nativeMutations", V::Null),
        (
            "accepted",
            ActiveServiceWalBlobV1::of(&accepted).unwrap().value(),
        ),
        (
            "verifiedHistoryBefore",
            ActiveServiceWalBlobV1::of(&boundary).unwrap().value(),
        ),
        (
            "verifiedHistoryPost",
            ActiveServiceWalBlobV1::of(&boundary).unwrap().value(),
        ),
        ("sourceHead", source.clone()),
        ("historySegment", segment.clone()),
    ]);
    let raw = bytes(&entry).unwrap();
    let pin = ActiveServiceWalBlobV1::of(&raw).unwrap();
    let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
    chain.extend_from_slice(&prior_head);
    chain.extend_from_slice(&sequence.to_be_bytes());
    chain.extend_from_slice(&pin.sha256);
    let head = sha256(&chain);
    for stage in ["Intent", "Accepted", "Ack"] {
        if stage == "Accepted" && skip_accepted {
            continue;
        }
        entry.insert("stage", s(stage)).unwrap();
        entry
            .insert(
                "origin",
                store.origin_context().unwrap().get("origin").get("archive"),
            )
            .unwrap();
        entry
            .insert(
                "head",
                if stage == "Intent" {
                    V::Null
                } else {
                    s(hex(&head))
                },
            )
            .unwrap();
        entry
            .insert(
                "entry",
                if stage == "Intent" {
                    V::Null
                } else {
                    pin.value()
                },
            )
            .unwrap();
        if stage == "Intent" {
            entry.insert("accepted", V::Null).unwrap();
            entry
                .insert(
                    "sourceHead",
                    prior.as_ref().map_or(V::Null, |p| {
                        parse(&store.read_blob(p).unwrap())
                            .unwrap()
                            .get("sourceHead")
                    }),
                )
                .unwrap();
            let V::Object(ref mut values) = entry else {
                unreachable!()
            };
            values.retain(|(k, _)| k != "verifiedHistoryPost");
        } else {
            entry
                .insert(
                    "accepted",
                    ActiveServiceWalBlobV1::of(&accepted).unwrap().value(),
                )
                .unwrap();
            entry.insert("sourceHead", source.clone()).unwrap();
            entry
                .insert(
                    "verifiedHistoryPost",
                    ActiveServiceWalBlobV1::of(&boundary).unwrap().value(),
                )
                .unwrap();
        }
        // Record fields intentionally exclude previousEntry; its independent actual
        // entry binds it, as in the production recorder.
        let V::Object(ref mut values) = entry else {
            unreachable!()
        };
        values.retain(|(k, _)| k != "previousEntry");
        let mut plan = store
            .prepare(
                &store.pin().unwrap(),
                entry.clone(),
                vec![
                    action.clone(),
                    before.clone(),
                    accepted.clone(),
                    boundary.clone(),
                    raw.clone(),
                ],
                vec![ActiveServiceWalBlobV1::of(&raw).unwrap()],
            )
            .unwrap();
        store.attempt(&mut plan).unwrap();
    }
    entries.push(pin);
}
#[test]
fn actual_segmented_reader_preserves_all_ordered_raw_windows_and_actual_lock() {
    let t = Temp::new();
    let mut store = comparison_fixture_store(&t);
    let mut prior = None;
    for _ in 0..2 {
        let mut entries = vec![];
        for _ in 0..3 {
            fixture_action(&mut store, prior.as_ref(), &mut entries, false, false)
        }
        let mut p = store
            .prepare_acknowledged_history_checkpoint(
                &store.pin().unwrap(),
                prior.as_ref(),
                &entries,
            )
            .unwrap();
        store
            .attempt_acknowledged_history_checkpoint(&mut p)
            .unwrap();
        prior = Some(
            store
                .finish_acknowledged_history_checkpoint(&mut p)
                .unwrap()
                .cursor()
                .clone(),
        );
    }
    let mut tail = vec![];
    for _ in 0..2 {
        fixture_action(&mut store, prior.as_ref(), &mut tail, false, false)
    }
    let record = store.record().unwrap();
    let captured = store
        .capture_segmented_ack_history(
            8,
            segment_hash(&record.get("head")).unwrap(),
            &record.get("sourceHead"),
        )
        .unwrap();
    drop(store);
    let mut reader = VerifiedSegmentedActiveHistoryV1::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &captured,
    )
    .unwrap();
    let mut total = 0;
    assert!(Journal::open(
        &t.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        Recovery::RejectIncompleteTail
    )
    .is_err());
    for w in 0..reader.window_count() {
        let window = reader.read_window(w).unwrap();
        for entry in window.entries() {
            total += 1;
            assert_eq!(entry.sequence, total);
            assert!(entry.raw_action().starts_with(b"TLMASA\x01"));
        }
    }
    assert_eq!(total, 8);
    assert_eq!(reader.actions(), 8);
    assert_eq!(reader.head(), captured.head());
}
#[test]
fn reader_refuses_missing_accepted_barrier_and_untyped_same_metadata_append() {
    for bad_append in [false, true] {
        let t = Temp::new();
        let mut store = comparison_fixture_store(&t);
        let mut entries = vec![];
        fixture_action(&mut store, None, &mut entries, false, false);
        fixture_action(&mut store, None, &mut entries, !bad_append, bad_append);
        let record = store.record().unwrap();
        let captured = store
            .capture_segmented_ack_history(
                2,
                segment_hash(&record.get("head")).unwrap(),
                &record.get("sourceHead"),
            )
            .unwrap();
        drop(store);
        assert!(VerifiedSegmentedActiveHistoryV1::open(
            &t.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &captured
        )
        .is_err());
    }
}
