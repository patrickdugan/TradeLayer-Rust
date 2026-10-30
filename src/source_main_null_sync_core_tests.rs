// Actual private Core tests: Parent owns compilation/execution.
use super::*;

fn source_record(case: &str, point: &str) -> &'static str {
    include_str!("../fixtures/null_sync_first10.jsonl")
        .lines()
        .find(|line| {
            json_string(raw_get(line, "case")) == case
                && if point == "input" {
                    json_string(raw_get(line, "kind")) == "input"
                } else {
                    json_string(raw_get(line, "kind")) == "observation"
                        && json_string(raw_get(line, "point")) == point
                }
        })
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
        V::Array(vec![]),
    )
}

fn begin(temp: &Temp, case: &str) -> SourceMainNativeNullSyncV1 {
    let (owner, documents) = setup_null(temp, case);
    let expected = owner.expectation().unwrap();
    let profile = untag(raw_get(source_record(case, "input"), "rawBoundaryProfile"));
    owner
        .into_retained_null_sync(
            &expected,
            501,
            71,
            documents,
            NativeMainNullIoProfileV1 {
                hold_child_read: profile.get("holdRead") == V::Bool(true),
                hold_parent_tip: profile.get("holdParentTip") == V::Bool(true),
            },
        )
        .unwrap()
}
fn selected_effects(events: &[V], kind: &str) -> Vec<V> {
    events
        .iter()
        .filter(|e| e.get("kind") == s(kind) && e.get("method") != s("Client.getInstance"))
        .cloned()
        .collect()
}
fn compare_source(owner: &SourceMainNativeNullSyncV1, case: &str, point: &str) {
    let source = source_record(case, point);
    let caller = untag(raw_get(source, "caller"));
    match caller.get("state") {
        V::String(state) if state == "Pending" => assert!(
            owner.caller_result().is_none(),
            "{case}/{point}: actual parent still pending"
        ),
        V::String(state) if state == "Fulfilled" => {
            assert_eq!(caller.get("returned"), V::Array(vec![s("undefined")]));
            assert_eq!(
                owner.caller_result(),
                Some(&DependencyReturn::Pending(V::Undefined)),
                "{case}/{point}: actual parent fulfillment"
            );
        }
        other => panic!("source parent outcome {other:?}"),
    }
    let mut rows = vec![];
    owner
        .index()
        .visit_rows(|r| {
            rows.push(r.clone());
            Ok(true)
        })
        .unwrap();
    assert_eq!(
        V::Array(rows),
        untag(raw_get(source, "rows")),
        "{case}/{point}: COMPLETE native rows"
    );
    let source_effects = arr(untag(raw_get(source, "effects")));
    for kind in ["rpc-input", "unhandled-rejection"] {
        assert_eq!(
            selected_effects(owner.effects(), kind),
            selected_effects(&source_effects, kind),
            "{case}/{point}: ordered {kind}"
        );
    }
    assert_eq!(
        owner
            .financial_state()
            .unwrap()
            .get("mainProduction")
            .get("index")
            .get("classParseBlock"),
        untag(raw_get(source, "index")).get("parseBlock"),
        "{case}/{point}: actual shared TxIndex parse cursor"
    );
}
fn run(owner: &mut SourceMainNativeNullSyncV1, allow_writes: bool) {
    for _ in 0..256 {
        let stop = owner.advance(4096);
        match stop {
            NativeMainNullStopV1::Stopped(error) => {
                panic!("actual null-source local stop {error:?}")
            }
            NativeMainNullStopV1::PendingNativeWrites if allow_writes => {
                let pending = owner.pending_operations();
                assert!(!pending.is_empty());
                for operation in pending {
                    assert_eq!(
                        owner.append_pending(&operation, None).unwrap(),
                        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
                    );
                    assert_eq!(
                        owner.resolve_post(&operation).unwrap(),
                        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
                    );
                }
            }
            _ => return,
        }
    }
    panic!("bounded source controls did not quiesce")
}
#[test]
fn actual_null_profile_matches_all_first_ten_parent_child_interleavings_and_raw_source_observations(
) {
    let mut count = 0;
    for input in include_str!("../fixtures/null_sync_first10.jsonl")
        .lines()
        .filter(|line| json_string(raw_get(line, "kind")) == "input")
    {
        let case = json_string(raw_get(input, "case"));
        let temp = Temp::new();
        let mut owner = begin(&temp, &case);
        let profile = untag(raw_get(input, "rawBoundaryProfile"));
        run(&mut owner, profile.get("holdInsert") != V::Bool(true));
        compare_source(&owner, &case, "initial-reached-source-boundary");
        if profile.get("holdRead") == V::Bool(true) {
            owner.release_detached_read().unwrap();
            run(&mut owner, true);
            compare_source(&owner, &case, "same-detached-child-database-resumed");
        }
        if profile.get("holdInsert") == V::Bool(true) {
            run(&mut owner, true);
            compare_source(&owner, &case, "same-detached-child-database-resumed");
        }
        if profile.get("holdParentTip") == V::Bool(true) {
            owner.release_parent_tip().unwrap();
            run(&mut owner, true);
            compare_source(&owner, &case, "same-parent-tip-resumed");
        }
        run(&mut owner, true);
        compare_source(&owner, &case, "finite-reached-source-jobs-drained");
        assert!(
            owner.detached_result().is_some(),
            "{case}: actual detached promise settled"
        );
        count += 1;
    }
    assert_eq!(count, 10);
}
#[test]
fn fulfilled_parent_keeps_real_detached_read_and_one_actual_locked_index() {
    let temp = Temp::new();
    let mut owner = begin(&temp, "first10/01-child-read-held-parent-returns");
    run(&mut owner, false);
    assert_eq!(
        owner.caller_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
    assert!(owner.detached_result().is_none());
    let before = owner.index().capture_verified_history_pin().unwrap();
    let effects = owner.effects().to_vec();
    run(&mut owner, false);
    assert_eq!(owner.effects(), effects);
    assert_eq!(
        owner.index().capture_verified_history_pin().unwrap(),
        before
    );
    assert!(SourceCompactIndex::open_verified_history_owner(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &before
    )
    .is_err());
    owner.release_detached_read().unwrap();
    assert!(owner.release_detached_read().is_err());
    run(&mut owner, true);
    assert_eq!(
        owner.detached_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
}
#[test]
fn detached_native_write_requires_its_exact_actual_owner_and_separate_post_resolution() {
    let ta = Temp::new();
    let tb = Temp::new();
    let case = "first10/03-child-initializer-write-held";
    let mut a = begin(&ta, case);
    let mut b = begin(&tb, case);
    run(&mut a, false);
    run(&mut b, false);
    let operations = a.pending_operations();
    assert_eq!(operations.len(), 1);
    let operation = operations[0].clone();
    assert_eq!(operation.job(), NativeMainNullJobV1::DetachedInit);
    assert_eq!(operation.value().get("role"), s("Detached"));
    let before = b.index().capture_verified_history_pin().unwrap();
    let effects = b.effects().to_vec();
    assert_eq!(
        b.append_pending(&operation, None),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(b.effects(), effects);
    assert_eq!(b.index().capture_verified_history_pin().unwrap(), before);
    assert_eq!(
        a.append_pending(&operation, None).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::AwaitNativeWrite
    );
    let post = a.index().capture_verified_history_pin().unwrap();
    run(&mut a, false);
    assert!(a.detached_result().is_none());
    assert_eq!(a.index().capture_verified_history_pin().unwrap(), post);
    assert_eq!(
        a.resolve_post(&operation).unwrap(),
        crate::source_contract_runtime::NativeMainServiceStopV1::Quiescent
    );
    assert_eq!(
        a.resolve_post(&operation),
        Err(SourceRuleError::InvalidInput)
    );
    run(&mut a, true);
    assert_eq!(
        a.detached_result(),
        Some(&DependencyReturn::Pending(V::Undefined))
    );
}
#[test]
fn old_numeric_and_explicit_primitive_entries_still_refuse_null_before_source() {
    for coercive in [false, true] {
        let temp = Temp::new();
        let (owner, documents) = setup_null(&temp, "first10/01-child-read-held-parent-returns");
        let expected = owner.expectation().unwrap();
        let result = if coercive {
            owner
                .into_retained_coercive_sync(&expected, V::Null, 501, 71, documents)
                .map(|_| ())
        } else {
            owner
                .into_retained_main_caller(
                    &expected,
                    NativeMainRetainedCallerV1::SyncIndex { max: V::Null },
                    501,
                    71,
                    documents,
                )
                .map(|_| ())
        };
        let failure = match result {
            Ok(()) => panic!("old profile widened"),
            Err(failure) => failure,
        };
        assert_eq!(failure.into_owner().expectation().unwrap(), expected);
    }
}
