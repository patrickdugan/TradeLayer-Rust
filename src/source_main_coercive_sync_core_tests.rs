// Private actual-Core gates. Root must compile these; standalone primitive
// comparisons are not substituted for this financial/owner execution.
use super::*;
fn source_record(case: &str, kind: &str) -> &'static str {
    include_str!("../fixtures/coercive_sync_vectors.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "case")) == case && json_string(raw_get(line, "kind")) == kind
        })
        .unwrap()
}
fn source_final(case: &str) -> &'static str {
    include_str!("../fixtures/coercive_sync_vectors.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "case")) == case
                && json_string(raw_get(line, "kind")) == "observation"
                && json_string(raw_get(line, "point")) == "finite-source-jobs-drained"
        })
        .unwrap()
}
fn setup_source(temp: &Temp, case: &str) -> (SourceMainNativeSessionV13, V, V) {
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
    if input.contains("\"rawStorageFailure\":") {
        let failure = untag(raw_get(input, "rawStorageFailure"));
        assert_eq!(failure.get("op"), s("find"));
        let V::String(message) = failure.get("message") else {
            panic!("message")
        };
        financial
            .native_main_runtime_mut()
            .fail_storage(crate::source_contract_runtime::StorageFailure {
                collection: "txIndex".into(),
                operation: "findAsync".into(),
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
        untag(raw_get(input, "rawMax")),
        V::Array(vec![]),
    )
}
fn begin_source(temp: &Temp, case: &str) -> SourceMainNativeCoerciveSyncV1 {
    let (owner, max, documents) = setup_source(temp, case);
    let expected = owner.expectation().unwrap();
    owner
        .into_retained_coercive_sync(&expected, max, 501, 71, documents)
        .unwrap()
}
enum Held {
    Caller(Box<SourceMainNativeCoerciveSyncV1>),
    Consensus(Box<SourceMainNativeCoerciveConsensusV1>),
}
impl Held {
    fn caller_result(&self) -> Option<&DependencyReturn> {
        match self {
            Self::Caller(c) => c.caller_result(),
            Self::Consensus(c) => c.caller_result(),
        }
    }
    fn effects(&self) -> &[V] {
        match self {
            Self::Caller(c) => c.effects(),
            Self::Consensus(c) => c.effects(),
        }
    }
    fn state(&self) -> V {
        match self {
            Self::Caller(c) => c.financial_state().unwrap(),
            Self::Consensus(c) => c.financial_state().unwrap(),
        }
    }
    fn index(&self) -> &SourceCompactIndex {
        match self {
            Self::Caller(c) => &c.caller.owner.index,
            Self::Consensus(c) => &c.consensus.caller.owner.index,
        }
    }
}
fn drive(mut held: Held, case: &str) -> Held {
    for _ in 0..256 {
        match &mut held {
            Held::Caller(c) => match c.advance() {
                NativeMainRetainedCallerStopV1::ConsensusReady => {
                    let Held::Caller(c) = held else {
                        unreachable!()
                    };
                    held = Held::Consensus(Box::new(c.into_consensus().unwrap()));
                }
                NativeMainRetainedCallerStopV1::Complete => return held,
                NativeMainRetainedCallerStopV1::CallerReturned => {}
                NativeMainRetainedCallerStopV1::Native(
                    crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite,
                ) => {
                    let selector = c.pending_descriptor().unwrap();
                    assert_eq!(
                        c.append_pending(&selector, None),
                        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite,
                        "{case}"
                    );
                    assert_eq!(
                        c.resolve_post(&selector),
                        crate::source_contract_runtime::NativeMainRowStopV1::Complete,
                        "{case}"
                    );
                }
                other => panic!("{case}: {other:?}"),
            },
            Held::Consensus(c) => {
                let stop = c.advance_consensus(64).unwrap();
                if c.caller_result().is_some() {
                    return held;
                }
                let operations = c.pending_operations();
                if let Some(operation) = operations
                    .iter()
                    .find(|operation| operation.value().get("context") != V::Null)
                {
                    c.append_pending(operation, None).unwrap();
                    c.resolve_post(operation).unwrap();
                } else if let Some(operation) = operations.iter().find(|operation| {
                    operation.kind() == NativeMainRetainedConsensusOperationKindV1::CallerInline
                }) {
                    c.advance_detached(operation).unwrap();
                } else {
                    panic!("{case}: unresolved {stop:?}")
                }
            }
        }
    }
    panic!("bounded source fixture {case}")
}
fn expected_result(observation: &str) -> DependencyReturn {
    let result = raw_get(observation, "result");
    match json_string(raw_get(result, "transport")).as_str() {
        "Pending" => DependencyReturn::Pending(untag(raw_get(result, "returned"))),
        "PendingThrow" => {
            let value = untag(raw_get(result, "thrown"));
            let (V::String(name), V::String(message)) = (value.get("name"), value.get("message"))
            else {
                panic!("thrown")
            };
            DependencyReturn::PendingThrow(crate::validity_source::SourceException {
                name,
                message,
            })
        }
        other => panic!("unexpected transport {other}"),
    }
}
fn rpc(events: &[V]) -> Vec<V> {
    events
        .iter()
        .filter(|value| {
            value.get("kind") == s("rpc-input") && value.get("method") != s("Client.getInstance")
        })
        .cloned()
        .collect()
}
#[test]
fn actual_coercive_profile_matches_all_source_results_rows_raw_rpc_and_parse_height() {
    let mut count = 0;
    for input in include_str!("../fixtures/coercive_sync_vectors.jsonl")
        .lines()
        .filter(|line| json_string(raw_get(line, "kind")) == "input")
    {
        let case = json_string(raw_get(input, "case"));
        let temp = Temp::new();
        let held = drive(Held::Caller(Box::new(begin_source(&temp, &case))), &case);
        let expected = source_final(&case);
        assert_eq!(
            held.caller_result(),
            Some(&expected_result(expected)),
            "{case}: caller result"
        );
        let mut actual_rows = vec![];
        held.index()
            .visit_rows(|row| {
                actual_rows.push(row.clone());
                Ok(true)
            })
            .unwrap();
        assert_eq!(
            V::Array(actual_rows),
            untag(raw_get(expected, "rows")),
            "{case}: complete native rows"
        );
        assert_eq!(
            rpc(held.effects()),
            rpc(&arr(untag(raw_get(expected, "effects")))),
            "{case}: exact program RPC records"
        );
        assert_eq!(
            held.state()
                .get("mainProduction")
                .get("index")
                .get("classParseBlock"),
            untag(raw_get(expected, "index")).get("parseBlock"),
            "{case}: original raw height type"
        );
        count += 1;
    }
    assert_eq!(count, 26);
}
#[test]
fn explicit_entry_refuses_null_objects_and_legacy_entry_stays_numeric_before_source() {
    for value in [
        V::Null,
        V::Object(vec![]),
        V::Array(vec![]),
        V::Promise,
        V::BigNumber("10".into()),
    ] {
        let temp = Temp::new();
        let (owner, _, documents) = setup_source(&temp, "first10/01-number-equal");
        let before = owner.expectation().unwrap();
        let failure = match owner.into_retained_coercive_sync(&before, value, 501, 71, documents) {
            Ok(_) => panic!("outside primitive profile"),
            Err(error) => error,
        };
        let owner = failure.into_owner();
        assert_eq!(owner.expectation().unwrap(), before);
    }
    let temp = Temp::new();
    let (owner, _, documents) = setup_source(&temp, "first10/01-number-equal");
    let before = owner.expectation().unwrap();
    let failure = match owner.into_retained_main_caller(
        &before,
        NativeMainRetainedCallerV1::SyncIndex { max: s("10") },
        501,
        71,
        documents,
    ) {
        Ok(_) => panic!("old entry widened"),
        Err(error) => error,
    };
    assert_eq!(failure.into_owner().expectation().unwrap(), before);
}
#[test]
fn original_string_start_keeps_real_detached_save_and_separate_source_resolution() {
    let temp = Temp::new();
    let mut caller = begin_source(&temp, "extended/11-string-extract-real-empty-blocks");
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::CallerReturned
    );
    assert_eq!(
        caller.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(caller.pending_write().is_none());
    let selector = caller.pending_descriptor().unwrap();
    assert_eq!(selector.value().get("role"), s("Detached"));
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    assert_eq!(
        caller.pending_write().unwrap().arguments()[0].get("_id"),
        s("MaxHeight")
    );
    assert_eq!(
        caller.append_pending(&selector, None),
        crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
    );
    let post = caller.caller.owner.index.snapshot().unwrap();
    let effects = caller.effects().to_vec();
    assert_eq!(
        caller.advance(),
        NativeMainRetainedCallerStopV1::Native(
            crate::source_contract_runtime::NativeMainRowStopV1::AwaitNativeWrite
        )
    );
    assert_eq!(caller.effects(), effects);
    assert_eq!(caller.caller.owner.index.snapshot().unwrap(), post);
    assert_eq!(
        caller.resolve_post(&selector),
        crate::source_contract_runtime::NativeMainRowStopV1::Complete
    );
    let held = drive(Held::Caller(Box::new(caller)), "string detached suffix");
    assert_eq!(
        held.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
}
#[test]
fn old_numeric_frame_keeps_its_original_observation_namespace() {
    let temp = Temp::new();
    let (owner, _, documents) = setup_source(&temp, "first10/01-number-equal");
    let expected = owner.expectation().unwrap();
    let caller = owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::SyncIndex {
                max: V::number(10.),
            },
            501,
            71,
            documents,
        )
        .unwrap();
    let mut fields = vec![];
    caller
        .visit_canonical_fields(&mut |path, value| {
            fields.push((path.to_owned(), value.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.iter().all(|(path, _)| !path.contains("coercive")));
}
