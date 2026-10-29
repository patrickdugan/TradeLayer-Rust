use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::source_compact_index_v2::SourceCompactIndex;
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::source_main_native_owners::{NativeMainExpectation, NativeMainOwners};
use tradelayer_core::source_main_native_session::{
    NativeMainReexecutionOwner, NativePhysicalBinding,
};
use tradelayer_core::source_main_native_session::{
    NativeMainSessionCommand as N, NativeMainSessionError, NativeMainSessionStatus as S,
    SourceMainNativeSessionV13,
};
use tradelayer_core::storage::Namespace;
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = PathBuf::from(std::env::var("TL_NATIVE_MAIN_TMP").unwrap())
            .canonicalize()
            .unwrap();
        let root = parent.join(format!(
            "native-main-integrated-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(v: V) -> String {
    if let V::String(t) = v {
        t
    } else {
        panic!("text")
    }
}
#[derive(Default)]
struct Case {
    header: Option<V>,
    input: Vec<V>,
    output: Vec<V>,
    financial: BTreeMap<String, Vec<V>>,
    direct_financial: BTreeMap<String, Vec<V>>,
}
fn cases() -> BTreeMap<String, Case> {
    let mut cases = BTreeMap::<String, Case>::new();
    for line in include_str!("../fixtures/native_main_controls.jsonl").lines() {
        let frame = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        let case = cases.entry(text(frame.get("name"))).or_default();
        match text(frame.get("kind")).as_str() {
            "case" => case.header = Some(frame),
            "input-row" => case.input.push(frame.get("row")),
            "output-row" => case.output.push(frame.get("row")),
            "financial-row" => case
                .financial
                .entry(text(frame.get("collection")))
                .or_default()
                .push(frame.get("row")),
            "direct-financial-row" => case
                .direct_financial
                .entry(text(frame.get("collection")))
                .or_default()
                .push(frame.get("row")),
            _ => panic!("case frame"),
        }
    }
    cases
}
fn invoke(replay: &mut SourceContractReplay, target: &str, arguments: Vec<V>) {
    let receipt = replay
        .apply(C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        }))
        .unwrap();
    assert!(
        !matches!(
            receipt.outcome,
            O::LocalError(_)
                | O::Dependency(DependencyReturn::PendingThrow(_) | DependencyReturn::Throw(_))
        ),
        "foundation {target}: {:?}",
        receipt.outcome
    );
}
fn financial() -> SourceContractReplay {
    let mut financial = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    invoke(
        &mut financial,
        "SourceActivation.configureClient",
        vec![obj(&[("chain", s("LTC")), ("test", V::Bool(false))])],
    );
    financial.apply(C::SetHeight(V::number(10.))).unwrap();
    invoke(&mut financial, "Activation.init", vec![s("LTC")]);
    invoke(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
            ),
            (
                "io",
                obj(&[
                    ("height", V::number(10.)),
                    (
                        "failures",
                        V::Array(vec![obj(&[
                            ("method", s("Client.getBlockCount")),
                            ("ordinal", V::number(1.)),
                            ("name", s("SourceBoundaryUnavailable")),
                            ("message", s("Native Main control has no live RPC service")),
                        ])]),
                    ),
                ]),
            ),
            ("genesisBlock", V::number(10.)),
        ])],
    );
    financial
}
fn session(temp: &Temp, case: &Case) -> SourceMainNativeSessionV13 {
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        namespace.clone(),
        "native13/genuine-source-origin",
    )
    .unwrap();
    for row in &case.input {
        let pin = index.snapshot().unwrap();
        index.import_row(&pin, row.clone()).unwrap();
    }
    let mut financial = financial();
    if case.header.as_ref().unwrap().get("name")
        == s("native-update-error-retains-real-main0-prefix")
    {
        financial
            .apply(C::FailStorage(
                tradelayer_core::source_contract_runtime::StorageFailure {
                    collection: "txIndex".into(),
                    operation: "updateAsync".into(),
                    skip: 0,
                    message: "native-source-update-failure".into(),
                },
            ))
            .unwrap();
    }
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace,
        metadata: index.snapshot().unwrap(),
    };
    SourceMainNativeSessionV13::adopt(NativeMainOwners::adopt(financial, index, &expected).unwrap())
        .unwrap()
}
fn raw_index(temp: &Temp, case: &Case) -> SourceCompactIndex {
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "native13/genuine-source-origin",
    )
    .unwrap();
    for row in &case.input {
        let pin = index.snapshot().unwrap();
        index.import_row(&pin, row.clone()).unwrap();
    }
    index
}
fn physical(index: &SourceCompactIndex) -> NativePhysicalBinding {
    let (persistent_identity, canonical_catalog) = index.native_store_context().unwrap();
    NativePhysicalBinding {
        persistent_identity,
        canonical_catalog,
        metadata: index.snapshot().unwrap(),
    }
}
#[test]
fn actual_main_native_loan_handles_main0_and_complete_1100_row_reads() {
    let cases = cases();
    let case = &cases["full-catchup-handles-main0"];
    let temp = Temp::new();
    let mut session = session(&temp, case);
    let origin = session
        .index()
        .read_generation(&session.index().snapshot().unwrap())
        .unwrap();
    let expected = session.expectation().unwrap();
    let receipt = session
        .apply(
            &expected,
            N::ConstructConsensusFromIndex {
                start: V::number(10.),
            },
        )
        .unwrap()
        .clone();
    assert_eq!(
        receipt.status,
        S::Completed(DependencyReturn::PendingThrow(
            tradelayer_core::validity_source::SourceException {
                name: "SourceBoundaryUnavailable".into(),
                message: "Native Main control has no live RPC service".into()
            }
        )),
        "actual source terminal boundary"
    );
    let grouping = receipt.grouping.as_ref().unwrap();
    assert!(grouping.complete);
    assert_eq!(grouping.rows, 1101);
    assert_eq!(receipt.validity.len(), 1);
    let validity = &receipt.validity[0];
    assert!(validity.query.complete);
    assert_eq!(validity.query.visited_rows, 1101);
    assert_eq!(validity.query.matched_rows, 1100);
    assert!(session.financial_runtime().collection("txIndex").is_empty());
    for collection in ["propertyList", "tallyMap", "activations", "consensus"] {
        let actual = session.financial_runtime().collection(collection);
        let expected = case
            .financial
            .get(collection)
            .map_or(&[][..], Vec::as_slice);
        assert_eq!(actual.len(), expected.len(), "financial count {collection}");
        for (a, b) in actual.iter().zip(expected) {
            assert_eq!(a, b, "actual Main-owned financial collection {collection}");
        }
    }
    let mut current = session
        .index()
        .read_generation(&session.index().snapshot().unwrap())
        .unwrap()
        .cursor();
    let mut count = 0;
    while let Some(row) = current.next_row().unwrap() {
        assert_eq!(row, case.output[count], "native output row {count}");
        count += 1;
    }
    assert_eq!(count, case.output.len());
    assert!(current.receipt().complete);
    let mut old = origin.cursor();
    while let Some(row) = old.next_row().unwrap() {
        assert_eq!(row.get("valid"), V::Undefined, "pinned original read");
    }
    assert_ne!(session.head().commitment, expected.session.commitment);
    assert_eq!(session.foundation_head().commands, 4);
    assert!(
        session
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("nativeMainBinding")
            != V::Undefined
    );
    assert!(format!("{:?}", session.financial_runtime()).contains("native_main_binding"));
    assert_eq!(
        session.require_authenticated_admission(),
        Err(tradelayer_core::validity_source::SourceRuleError::UnauthenticatedInputAndState)
    );
}
#[test]
fn actual_native_grouping_finishes_before_malformed_row_rejects_all_financial_processing() {
    let cases = cases();
    let case = &cases["late-malformed-prevents-earlier-main0"];
    let temp = Temp::new();
    let mut session = session(&temp, case);
    let expected = session.expectation().unwrap();
    let receipt = session
        .apply(
            &expected,
            N::ConstructConsensusFromIndex {
                start: V::number(10.),
            },
        )
        .unwrap()
        .clone();
    assert!(
        matches!(receipt.status,S::Completed(DependencyReturn::PendingThrow(ref e)) if e.name=="TypeError"&&e.message=="payload.slice is not a function")
    );
    assert!(receipt.grouping.as_ref().unwrap().complete);
    assert_eq!(receipt.grouping.as_ref().unwrap().rows, 1102);
    assert!(receipt.validity.is_empty());
    assert_eq!(receipt.before, receipt.after.unwrap());
    assert!(session
        .financial_runtime()
        .collection("propertyList")
        .is_empty());
    assert!(session
        .financial_runtime()
        .collection("consensus")
        .is_empty());
}
#[test]
fn wrong_native_expectations_refuse_before_any_processing_and_escape_is_suspended() {
    let cases = cases();
    let temp = Temp::new();
    let mut session = session(&temp, &cases["full-catchup-handles-main0"]);
    let expected = session.expectation().unwrap();
    let command = N::ConstructConsensusFromIndex {
        start: V::number(10.),
    };
    let mut wrong = expected.clone();
    wrong.physical.canonical_catalog.push("foreign");
    assert_eq!(
        session.apply(&wrong, command.clone()).unwrap_err(),
        NativeMainSessionError::WrongPhysicalBinding
    );
    let mut wrong = expected.clone();
    wrong.session.commitment[0] ^= 1;
    assert_eq!(
        session.apply(&wrong, command).unwrap_err(),
        NativeMainSessionError::WrongHead
    );
    assert_eq!(session.expectation().unwrap(), expected);
    let receipt = session
        .apply(
            &expected,
            N::Invoke(DependencyCall {
                target: "Main.constructConsensusFromIndex".into(),
                arguments: vec![V::number(10.)],
            }),
        )
        .unwrap();
    assert!(matches!(receipt.status, S::Suspended { .. }));
    assert!(session
        .financial_runtime()
        .collection("propertyList")
        .is_empty());
    assert!(matches!(
        session.apply(
            &session.expectation().unwrap(),
            N::Invoke(DependencyCall {
                target: "Main.loadMaxProcessedHeight".into(),
                arguments: vec![]
            })
        ),
        Err(NativeMainSessionError::Halted)
    ));
}
#[test]
fn native13_reexecutes_actual_commands_into_fresh_owner_and_fences_old_reader() {
    let all = cases();
    let case = &all["full-catchup-handles-main0"];
    let original_temp = Temp::new();
    let mut original = session(&original_temp, case);
    let expected = original.expectation().unwrap();
    original
        .apply(
            &expected,
            N::ConstructConsensusFromIndex {
                start: V::number(10.),
            },
        )
        .unwrap();
    let (bytes, pin) = original.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLCR\x0d");
    assert!(
        SourceContractReplay::from_checkpoint(&bytes, pin.foundation).is_err(),
        "legacy parser must reject native13 session envelope"
    );
    let replay_temp = Temp::new();
    let raw = raw_index(&replay_temp, case);
    let expected_raw = physical(&raw);
    assert_ne!(expected_raw.canonical_catalog, pin.origin.canonical_catalog);
    let replay = SourceMainNativeSessionV13::reexecute(&bytes, &pin, raw, &expected_raw).unwrap();
    assert_eq!(replay.head(), original.head());
    assert_ne!(
        replay.expectation().unwrap().physical.canonical_catalog,
        pin.current.canonical_catalog
    );
    assert_eq!(
        replay.history()[0].financial_commitment,
        original.history()[0].financial_commitment
    );
    assert_eq!(
        replay.history()[0]
            .after_rows
            .as_ref()
            .unwrap()
            .chain_sha256d,
        original.history()[0]
            .after_rows
            .as_ref()
            .unwrap()
            .chain_sha256d
    );
    for collection in ["propertyList", "tallyMap", "activations", "consensus"] {
        assert_eq!(
            replay.financial_runtime().collection(collection),
            original.financial_runtime().collection(collection)
        );
    }
    let rejected_temp = Temp::new();
    let raw = raw_index(&rejected_temp, case);
    let expected_raw = physical(&raw);
    let mut corrupt = bytes;
    corrupt[7] ^= 1;
    let rejected =
        SourceMainNativeSessionV13::reexecute(&corrupt, &pin, raw, &expected_raw).unwrap_err();
    assert_eq!(rejected.error, NativeMainSessionError::WrongHead);
    let NativeMainReexecutionOwner::RawIndex(index) = rejected.owner else {
        panic!("archive rejection must precede financial/native execution")
    };
    assert_eq!(physical(&index), expected_raw);
}
#[test]
fn native_source_update_throw_runs_fatal_fallback_and_retains_prior_real_financial_writes() {
    let all = cases();
    let case = &all["native-update-error-retains-real-main0-prefix"];
    let temp = Temp::new();
    let mut session = session(&temp, case);
    let before = session.expectation().unwrap();
    let receipt = session
        .apply(
            &before,
            N::ConstructConsensusFromIndex {
                start: V::number(10.),
            },
        )
        .unwrap()
        .clone();
    assert!(
        matches!(receipt.status,S::Completed(DependencyReturn::PendingThrow(ref e))if e.name=="SourceBoundaryUnavailable")
    );
    assert_eq!(receipt.validity.len(), 2);
    for evaluation in &receipt.validity {
        assert!(evaluation.query.complete);
        assert_eq!(evaluation.query.matched_rows, 1100);
        assert_eq!(evaluation.query.visited_rows, 1101);
    }
    assert!(
        matches!(receipt.validity[0].update.as_ref().unwrap().returned,DependencyReturn::PendingThrow(ref e) if e.message=="native-source-update-failure")
    );
    assert_eq!(receipt.validity[0].post_state, before.physical.metadata);
    assert!(receipt.validity[1].post_state.sequence > before.physical.metadata.sequence);
    assert!(!session
        .financial_runtime()
        .collection("propertyList")
        .is_empty());
    assert!(!session
        .financial_runtime()
        .collection("tallyMap")
        .is_empty());
    for collection in ["propertyList", "tallyMap", "activations", "consensus"] {
        assert_eq!(
            session.financial_runtime().collection(collection),
            case.financial
                .get(collection)
                .map_or(&[][..], Vec::as_slice),
            "real source failure-prefix {collection}"
        );
    }
    let native = session
        .index()
        .get("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8")
        .unwrap()
        .unwrap();
    assert_eq!(native.get("valid"), V::Bool(false));
}
#[test]
fn native13_reexecutes_only_with_explicit_recorded_and_replacement_physical_bindings() {
    let all = cases();
    let case = &all["full-catchup-handles-main0"];
    let original_temp = Temp::new();
    let original = session(&original_temp, case);
    let (bytes, pin) = original.checkpoint().unwrap();
    let raw_temp = Temp::new();
    let index = raw_index(&raw_temp, case);
    let expected_raw = physical(&index);
    let mut wrong_raw = expected_raw.clone();
    wrong_raw.canonical_catalog.push("foreign");
    let rejected =
        SourceMainNativeSessionV13::reexecute(&bytes, &pin, index, &wrong_raw).unwrap_err();
    assert_eq!(rejected.error, NativeMainSessionError::WrongPhysicalBinding);
    let NativeMainReexecutionOwner::RawIndex(index) = rejected.owner else {
        panic!("wrong replacement pin must reject before execution")
    };
    assert_eq!(physical(&index), expected_raw);
    let mut wrong_recorded = pin.clone();
    wrong_recorded.origin.canonical_catalog.push("foreign");
    let rejected =
        SourceMainNativeSessionV13::reexecute(&bytes, &wrong_recorded, *index, &expected_raw)
            .unwrap_err();
    assert_eq!(rejected.error, NativeMainSessionError::WrongPhysicalBinding);
    let NativeMainReexecutionOwner::RawIndex(mut index) = rejected.owner else {
        panic!("wrong recorded pin must reject before execution")
    };
    assert_eq!(physical(&index), expected_raw);
    let changed = index
        .discard_rows(&expected_raw.metadata, &[], "wrong-origin-context")
        .unwrap();
    assert_eq!(changed.context, "wrong-origin-context");
    let expected_changed = physical(&index);
    let rejected =
        SourceMainNativeSessionV13::reexecute(&bytes, &pin, *index, &expected_changed).unwrap_err();
    assert_eq!(rejected.error, NativeMainSessionError::WrongPhysicalBinding);
    let NativeMainReexecutionOwner::RawIndex(index) = rejected.owner else {
        panic!("wrong origin context must reject before execution")
    };
    assert_eq!(physical(&index), expected_changed);
    assert_eq!(
        index
            .get("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8")
            .unwrap()
            .unwrap()
            .get("valid"),
        V::Undefined
    );
}
#[test]
fn direct_native_process_rows_preserves_real_source_set_and_live_returns_finance_and_metadata() {
    let all = cases();
    for (name, live) in [
        ("direct-main0-and-full-grouping", false),
        ("direct-live-main0-native-validity", true),
    ] {
        let case = &all[name];
        let temp = Temp::new();
        let mut session = session(&temp, case);
        let native = case
            .input
            .iter()
            .find(|row| {
                row.get("_id")
                    == s("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8")
            })
            .unwrap()
            .clone();
        let rows = V::Array(vec![if live { native.get("value") } else { native }]);
        let expected = session.expectation().unwrap();
        let receipt = session
            .apply(
                &expected,
                N::ProcessRows {
                    rows,
                    height: V::number(10.),
                    live,
                },
            )
            .unwrap()
            .clone();
        assert_eq!(
            receipt.status,
            S::Completed(DependencyReturn::Pending(
                case.header.as_ref().unwrap().get("directResult")
            )),
            "original caller-array return {name}"
        );
        assert!(receipt.grouping.is_none());
        assert_eq!(receipt.validity.len(), 1);
        assert!(receipt.validity[0].query.complete);
        assert_eq!(receipt.validity[0].query.matched_rows, 1100);
        assert_eq!(receipt.validity[0].query.visited_rows, 1101);
        for collection in ["propertyList", "tallyMap", "activations", "consensus"] {
            assert_eq!(
                session.financial_runtime().collection(collection),
                case.direct_financial
                    .get(collection)
                    .map_or(&[][..], Vec::as_slice),
                "actual direct source {name} financial rows {collection}"
            );
        }
        assert!(session.financial_runtime().collection("txIndex").is_empty());
        assert_eq!(
            session
                .index()
                .get("tx-10-ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8")
                .unwrap()
                .unwrap()
                .get("valid"),
            V::Bool(true)
        );
        let (bytes, pin) = session.checkpoint().unwrap();
        let fresh_temp = Temp::new();
        let fresh = raw_index(&fresh_temp, case);
        let origin = physical(&fresh);
        let replay = SourceMainNativeSessionV13::reexecute(&bytes, &pin, fresh, &origin).unwrap();
        assert_eq!(replay.head(), session.head());
        assert_eq!(
            replay.history()[0].financial_commitment,
            receipt.financial_commitment
        );
    }
}
