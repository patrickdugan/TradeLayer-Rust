// Independent private Core ownership probe, NOT executed by the reviewer.
use super::*;
fn source_record(_case: &str, _kind: &str) -> &'static str {
    include_str!("../fixtures/null_live_held.jsonl")
        .lines()
        .find(|line| json_string(raw_get(line, "kind")) == "input")
        .unwrap()
}
fn setup_null(temp: &Temp, case: &str) -> (SourceMainNativeSessionV13, V) {
    let input = source_record(case, "input");
    let reset = untag(raw_get(input, "rawReset"));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let constructor = untag(raw_get(input, "rawHarnessConstructorInput"));
    assert_eq!(
        constructor.get("profile"),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    assert_eq!(reset.get("empty"), V::Bool(true));
    for collection in arr(constructor.get("collections")) {
        let V::String(name) = collection.get("collection") else {
            panic!("collection")
        };
        financial
            .apply(SourceContractCommand::Import {
                collection: name,
                rows: vec![],
            })
            .unwrap();
    }
    let profile = reset.get("clientProfile");
    let client = obj(&[
        ("chain", profile.get("chain")),
        ("test", profile.get("test")),
    ]);
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
        .apply(SourceContractCommand::SetHeight(profile.get("height")))
        .unwrap();
    let failure = untag(raw_get(input, "rawBoundaryProfile")).get("dbFailure");
    if let V::Object(_) = failure {
        let operation = match failure.get("op") {
            V::String(name) => match name.as_str() {
                "find" => "findAsync",
                "findOne" => "findOneAsync",
                "insert" => "insertAsync",
                _ => panic!("raw IO method"),
            },
            _ => panic!("raw IO method"),
        };
        let V::String(message) = failure.get("message") else {
            panic!("raw IO rejection")
        };
        financial
            .native_main_runtime_mut()
            .fail_storage(crate::source_contract_runtime::StorageFailure {
                collection: "txIndex".into(),
                operation: operation.into(),
                skip: 0,
                message,
            })
            .unwrap();
    }
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual configured service raw inputs",
    )
    .unwrap();
    for row in arr(reset.get("rows")) {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    // Exact DECLARED harness reset input: empty rows plus its MaxHeight seed.
    index
        .import_row(
            &index.snapshot().unwrap(),
            obj(&[("_id", s("MaxHeight")), ("value", reset.get("maxHeight"))]),
        )
        .unwrap();
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
        {
            let raw = untag(raw_get(input, "rawRPCDocuments"));
            V::Array(vec![
                obj(&[
                    ("id", raw.get("id")),
                    ("raw", raw.get("raw")),
                    ("document", raw.get("document")),
                ]),
                obj(&[
                    ("id", raw.get("parent").get("txid")),
                    ("document", raw.get("parent")),
                ]),
            ])
        },
    )
}

#[test]
fn completed_null_callers_do_not_close_a_header_still_lent_to_a_held_live_financial_block() {
    let temp = Temp::new();
    let (owner, documents) = setup_null(&temp, "live");
    let expected = owner.expectation().unwrap();
    let mut owner = owner
        .into_retained_null_sync(
            &expected,
            501,
            71,
            documents,
            NativeMainNullIoProfileV1::default(),
        )
        .unwrap();
    let input = source_record("live", "input");
    let raw = untag(raw_get(input, "rawRPCDocuments"));
    let target = s(&format!(
        "tx-11-{}",
        match raw.get("id") {
            V::String(id) => id,
            _ => panic!("raw input txid"),
        }
    ));
    let mut reached = false;
    for _ in 0..512 {
        let stop = owner.advance(4096);
        if let NativeMainNullStopV1::Stopped(error) = stop {
            panic!("actual null source stopped before held-live comparison: {error:?}");
        }
        let operations = owner.pending_operations();
        for operation in operations {
            let value = operation.value();
            let arguments = value
                .get("operation")
                .get("context")
                .get("logical")
                .get("updateArguments");
            let live = matches!(arguments,V::Array(ref args) if args.first().is_some_and(|query|query.get("_id")==target));
            if live {
                assert_eq!(operation.job(), NativeMainNullJobV1::DetachedInit);
                assert_eq!(
                    owner.caller_result(),
                    Some(&DependencyReturn::Pending(V::Undefined))
                );
                assert_eq!(
                    owner.detached_result(),
                    Some(&DependencyReturn::Pending(V::Undefined))
                );
                let mut fields = vec![];
                owner
                    .visit_canonical_fields(&mut |path, value| {
                        fields.push((path.to_owned(), value.clone()));
                        Ok(())
                    })
                    .unwrap();
                let field = |path: &str| {
                    fields
                        .iter()
                        .find(|(p, _)| p == path)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(V::Undefined)
                };
                assert_eq!(
                    field("job/DetachedInit/service/block/present"),
                    V::Bool(true)
                );
                let owns_block_header =
                    field("job/DetachedInit/service/block/invocation/present") == V::Bool(true);
                assert!(field("headerClosed")==V::Bool(false)||owns_block_header,"actual live financial block has no independent header; its borrowed source queue/header must remain retained. Fields: {fields:?}");
                reached = true;
                break;
            }
            assert_eq!(
                owner.append_pending(&operation, None).unwrap(),
                crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
            );
            assert_eq!(
                owner.resolve_post(&operation).unwrap(),
                crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
            );
        }
        if reached {
            break;
        }
    }
    assert!(
        reached,
        "actual same-owner source never reached the original held live validity write"
    );
}

#[path = "source_main_null_header_continuation_tests.rs"]
mod retained_header_continuation;
