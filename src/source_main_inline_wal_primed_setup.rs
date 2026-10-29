// Raw construction copied from the current tested compaction fixture, with operational configuration before owner adoption.
fn setup_primed(temp: &Temp, offset: Option<u64>) -> (SourceMainNativeSessionV13, V, u64) {
    let mode = "same-held-live-validity";
    let input = record(mode, "input");
    let reset = untag(raw_get(input, "rawReset"));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    // Reproduce DECLARED reference fixture construction followed by raw
    // reset(empty:true). No expected business table/state is read or imported.
    assert_eq!(reset.get("empty"), V::Bool(true));
    let constructor = untag(raw_get(input, "rawHarnessConstructorInput"));
    assert_eq!(
        constructor.get("profile"),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    for raw_collection in arr(constructor.get("collections")) {
        let V::String(collection) = raw_collection.get("collection") else {
            panic!("raw collection name")
        };
        financial
            .apply(SourceContractCommand::Import {
                collection,
                rows: vec![],
            })
            .unwrap();
    }
    let client = reset.get("clientProfile");
    let client = obj(&[("chain", client.get("chain")), ("test", client.get("test"))]);
    invoke(
        &mut financial,
        "SourceActivation.configureClient",
        vec![client.clone()],
    );
    invoke(&mut financial, "Activation.init", vec![reset.get("init")]);
    invoke(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            ("client", client),
            ("io", reset.get("orchestration")),
            ("genesisBlock", reset.get("mainGenesis")),
        ])],
    );
    invoke(
        &mut financial,
        "SourceMain.configureService",
        vec![obj(&[
            ("fileInputs", V::Array(vec![])),
            ("directoryInputs", V::Array(vec![])),
            (
                "fileIOInputs",
                obj(&[("failures", V::Array(vec![])), ("mtimes", V::Array(vec![]))]),
            ),
        ])],
    );
    financial
        .apply(SourceContractCommand::SetHeight(
            reset.get("clientProfile").get("height"),
        ))
        .unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual configured service raw inputs",
    )
    .unwrap();
    for row in arr(untag(raw_get(input, "nativeInput"))) {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    index.native_main_test_compaction_fixture().unwrap();
    let primed = index.footprint().unwrap().journal_file_bytes;
    index
        .configure_compaction(crate::compact_journal::CompactionConfig {
            target_log_bytes: primed
                .checked_add(offset.unwrap_or(64 * 1024 * 1024))
                .unwrap(),
            retained_ancestors: 0,
        })
        .unwrap();
    let docs = untag(raw_get(input, "rawRPCDocuments"));
    let docs = if docs == V::Undefined {
        V::Array(vec![])
    } else {
        V::Array(vec![
            obj(&[
                ("id", docs.get("id")),
                ("raw", docs.get("raw")),
                ("document", docs.get("document")),
            ]),
            obj(&[
                ("id", docs.get("parent").get("txid")),
                ("document", docs.get("parent")),
            ]),
        ])
    };
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    (
        SourceMainNativeSessionV13::adopt(
            NativeMainOwners::adopt(financial, index, &expected).unwrap(),
        )
        .unwrap(),
        docs,
        primed,
    )
}
