use tradelayer_core::source_main_native_session::NativeStepHistoryAdmissionV1;
fn kernel_fields_exact(session: &SourceMainNativeStepSessionV1) -> Vec<(String, V)> {
    let mut fields = vec![];
    session
        .kernel()
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.into(), value.clone()));
            Ok(())
        })
        .unwrap();
    fields
}
#[test]
fn byte_window_parks_and_publishes_proper_prefixes_without_reexecuting_any_source_call() {
    let temp = Temp::new();
    let mut writer = writer_new(&temp);
    let bootstrap = writer.bootstrap().clone();
    let mut store = SegmentStore::new();
    writer_start(&mut writer, "Main.setPause", vec![]);
    let mut parked = 0;
    let mut partial = false;
    for _ in 0..34 {
        let command = Command::Start {
            method: "Main.syncIfNecessary".into(),
            arguments: vec![V::String("0".repeat(60_000))],
        };
        loop {
            let expected = writer.expectation().unwrap();
            match writer.apply_step(&expected, command.clone()) {
                Ok(receipt) => {
                    assert!(receipt.evaluation.is_ok());
                    break;
                }
                Err(NativeStepSessionError::Capacity) => {
                    assert_eq!(
                        writer.expectation().unwrap(),
                        expected,
                        "byte parking precedes source work"
                    );
                    assert!(matches!(
                        writer.admission().unwrap(),
                        NativeStepHistoryAdmissionV1::PublishRetained { .. }
                    ));
                    let retained = writer.session().receipts().count();
                    let published = writer_publish(&mut writer, &mut store);
                    partial |= writer.session().receipts().count() > 0;
                    eprintln!(
                        "byte park: retained={retained}; sealedSource={}; remainder={}",
                        published.head.source.commands,
                        writer.session().receipts().count()
                    );
                    parked += 1;
                }
                Err(error) => panic!("unexpected actual source refusal: {error:?}"),
            }
        }
    }
    let current = writer.expectation().unwrap().current;
    assert_eq!(current.head.commands, 35);
    assert_eq!(writer.session().kernel().jobs().count(), 34);
    assert!(writer
        .session()
        .kernel()
        .jobs()
        .all(|(_, s)| matches!(s, MainStepJobStatus::WaitingTimer(_))));
    let financial = writer.session().financial_runtime().state_value().unwrap();
    let kernel = kernel_fields_exact(writer.session());
    while writer.session().receipts().count() > 0 {
        let expected = writer.expectation().unwrap();
        let before_count = writer.session().receipts().count();
        let mut failed_bytes = None;
        let failed = writer.publish_segment(&expected, |segment| {
            failed_bytes = Some((segment.bytes().to_vec(), segment.publication().clone()));
            Err("stored prefix; ack unavailable".into())
        });
        assert!(matches!(
            failed,
            Err(NativeStepSessionError::PublicationBlocked(_))
        ));
        assert_eq!(writer.expectation().unwrap(), expected);
        assert_eq!(writer.session().receipts().count(), before_count);
        let failed_bytes = failed_bytes.unwrap();
        let published = writer
            .publish_segment(&expected, |segment| {
                assert_eq!(segment.bytes(), failed_bytes.0.as_slice());
                assert_eq!(segment.publication(), &failed_bytes.1);
                assert!(segment.bytes().len() <= 64 * 1024 * 1024);
                assert!(segment.commands() > 0);
                store.insert(
                    segment.publication().head.last_segment_sha256,
                    segment.bytes().to_vec(),
                );
                Ok(())
            })
            .unwrap();
        let after_count = writer.session().receipts().count();
        assert!(after_count < before_count);
        partial |= after_count > 0;
        assert_eq!(writer.expectation().unwrap().current, current);
        assert_eq!(
            writer.session().financial_runtime().state_value().unwrap(),
            financial
        );
        assert_eq!(kernel_fields_exact(writer.session()), kernel);
        assert_eq!(writer.expectation().unwrap().sealed, published);
        assert!(writer.session().checkpoint().is_err());
    }
    assert!(
        partial,
        "source scene must exercise an actual proper-prefix publication"
    );
    eprintln!(
        "sourceCalls=35; pendingTimers=34; byteParks={parked}; segments={}",
        store.len()
    );
    let publication = writer.expectation().unwrap().sealed;
    assert_eq!(publication.head.source, current.head);
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let pin = physical(&fresh);
    let replay = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| Ok(store[&digest].clone()),
        fresh,
        &pin,
        NativeStepProfileV1::Immediate,
    )
    .unwrap();
    assert_eq!(replay.session().head().unwrap(), current.head);
    assert_eq!(kernel_fields_exact(replay.session()), kernel);
    assert_eq!(
        replay.session().financial_runtime().state_value().unwrap(),
        financial
    );
}
