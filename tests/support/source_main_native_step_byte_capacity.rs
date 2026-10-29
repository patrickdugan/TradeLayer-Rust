// Additive actual-Core regression. No metadata adapter or second owner.
fn byte_cap_complete_kernel_fields(session: &SourceMainNativeStepSessionV1) -> Vec<(String, V)> {
    let mut fields = Vec::new();
    session
        .kernel()
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.to_string(), value.clone()));
            Ok(())
        })
        .unwrap();
    fields
}

fn byte_cap_publish_complete_retained_window(
    writer: &mut NativeStepHistoryWriterV1,
    store: &mut SegmentStore,
) {
    let fixed_head = writer.session().head().unwrap();
    let fixed_financial = writer.session().financial_runtime().state_value().unwrap();
    let fixed_kernel = byte_cap_complete_kernel_fields(writer.session());
    let fixed_physical = writer.session().expectation().unwrap().physical;
    while writer.expectation().unwrap().sealed.head.source != fixed_head {
        let before = writer.expectation().unwrap();
        // Full proof observations only; no Runtime or financial owner is cloned.
        let proof_before: Vec<_> = writer
            .session()
            .receipts()
            .map(|receipt| {
                (
                    receipt.clone(),
                    writer
                        .session()
                        .command_field_frames(usize::try_from(receipt.sequence).unwrap())
                        .unwrap()
                        .to_vec(),
                )
            })
            .collect();
        assert!(!proof_before.is_empty());
        let result = writer.publish_segment(&before, |segment| {
            if segment.bytes().len() > 64 * 1024 * 1024 {
                return Err("rotation must preserve the declared64MiB segment bound".into());
            }
            let digest = segment.publication().head.last_segment_sha256;
            if let Some(bytes) = store.get(&digest) {
                assert_eq!(bytes.as_slice(), segment.bytes());
            } else {
                store.insert(digest, segment.bytes().to_vec());
            }
            Ok(())
        });
        match result {
            Ok(publication) => {
                assert_eq!(publication.head.segments, before.sealed.head.segments + 1);
                assert!(publication.head.source.commands > before.sealed.head.source.commands);
                assert!(publication.head.source.commands <= fixed_head.commands);
                assert!(writer.session().receipts().count() < proof_before.len());
            }
            Err(error) => {
                // Current f5651 reaches Capacity after accepting all35 calls.
                // Verify the complete retained prefix before failing progress.
                assert_eq!(writer.expectation().unwrap(), before);
                assert_eq!(writer.session().head().unwrap(), fixed_head);
                assert_eq!(
                    writer.session().financial_runtime().state_value().unwrap(),
                    fixed_financial
                );
                assert_eq!(
                    byte_cap_complete_kernel_fields(writer.session()),
                    fixed_kernel
                );
                assert_eq!(
                    writer.session().expectation().unwrap().physical,
                    fixed_physical
                );
                let proof_after: Vec<_> = writer
                    .session()
                    .receipts()
                    .map(|receipt| {
                        (
                            receipt.clone(),
                            writer
                                .session()
                                .command_field_frames(usize::try_from(receipt.sequence).unwrap())
                                .unwrap()
                                .to_vec(),
                        )
                    })
                    .collect();
                assert_eq!(
                    proof_after, proof_before,
                    "failed rotation retains every receipt and raw command frame"
                );
                panic!("valid accepted source prefix must make bounded publication progress; retained_commands={} current_sequence={} error={error:?}", proof_after.len(), fixed_head.commands);
            }
        }
        assert_eq!(writer.session().head().unwrap(), fixed_head);
        assert_eq!(
            writer.session().financial_runtime().state_value().unwrap(),
            fixed_financial
        );
        assert_eq!(
            byte_cap_complete_kernel_fields(writer.session()),
            fixed_kernel
        );
        assert_eq!(
            writer.session().expectation().unwrap().physical,
            fixed_physical
        );
    }
    assert_eq!(writer.session().receipts().count(), 0);
}

#[test]
fn accepted_large_paused_source_prefix_publishes_bounded_segments_and_reexecutes_same_state() {
    let temp = Temp::new();
    let mut writer = writer_new(&temp);
    let bootstrap = writer.bootstrap().clone();
    let owner = writer.session().financial_runtime() as *const _;
    let mut store = SegmentStore::new();
    writer_start(&mut writer, "Main.setPause", vec![]);
    let restore = V::String("0".repeat(60_000));
    for _ in 0..34 {
        let command = Command::Start {
            method: "Main.syncIfNecessary".into(),
            arguments: vec![restore.clone()],
        };
        loop {
            let before = writer.expectation().unwrap();
            let financial_before = writer.session().financial_runtime().state_value().unwrap();
            let kernel_before = byte_cap_complete_kernel_fields(writer.session());
            match writer.apply_step(&before, command.clone()) {
                Ok(receipt) => {
                    assert_eq!(
                        receipt.evaluation.as_ref().unwrap().stop,
                        MainStepStop::Quiescent
                    );
                    assert_eq!(receipt.command, command);
                    break;
                }
                Err(NativeStepSessionError::Capacity) => {
                    assert_eq!(
                        writer.expectation().unwrap(),
                        before,
                        "typed byte-window parking precedes execution"
                    );
                    assert_eq!(
                        writer.session().financial_runtime().state_value().unwrap(),
                        financial_before
                    );
                    assert_eq!(
                        byte_cap_complete_kernel_fields(writer.session()),
                        kernel_before
                    );
                    assert!(
                        writer.session().receipts().next().is_some(),
                        "empty window must admit this supported command"
                    );
                    byte_cap_publish_complete_retained_window(&mut writer, &mut store);
                }
                Err(error) => panic!("supported original source call refused: {error:?}"),
            }
        }
        assert!(
            std::ptr::eq(owner, writer.session().financial_runtime()),
            "one actual financial Runtime through all calls"
        );
    }
    assert_eq!(writer.session().head().unwrap().commands, 35);
    assert_eq!(writer.session().kernel().jobs().count(), 34);
    assert!(writer
        .session()
        .kernel()
        .jobs()
        .all(|(_, status)| matches!(status, MainStepJobStatus::WaitingTimer(_))));
    byte_cap_publish_complete_retained_window(&mut writer, &mut store);
    assert!(
        store.len() >= 2,
        "the full actual proof exceeds one64MiB segment"
    );
    let publication = writer.expectation().unwrap().sealed;
    assert_eq!(publication.head.source.commands, 35);
    assert!(
        writer.session().checkpoint().is_err(),
        "rotated windows are never ordinary full archives"
    );
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let origin_pin = physical(&fresh);
    let replay = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| {
            store
                .get(&digest)
                .cloned()
                .ok_or("missing independently retained segment".into())
        },
        fresh,
        &origin_pin,
        NativeStepProfileV1::Immediate,
    )
    .unwrap();
    assert_eq!(replay.verified_publication(), &publication);
    assert_eq!(
        replay.session().head().unwrap(),
        writer.session().head().unwrap()
    );
    assert_eq!(
        replay.session().financial_runtime().state_value().unwrap(),
        writer.session().financial_runtime().state_value().unwrap()
    );
    assert_eq!(
        byte_cap_complete_kernel_fields(replay.session()),
        byte_cap_complete_kernel_fields(writer.session())
    );
    assert_eq!(replay.session().receipts().count(), 0);
    assert_ne!(
        replay
            .session()
            .expectation()
            .unwrap()
            .physical
            .canonical_catalog,
        publication.physical.canonical_catalog
    );
    assert!(replay.session().checkpoint().is_err());
}
