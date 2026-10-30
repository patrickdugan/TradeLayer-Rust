// Actual private-Core tests. This proposal has not been Core compiled by author.
use super::*;
use crate::source_contract_replay::{SourceContractCommand, SourceContractCommandOutcome};
use crate::storage::Namespace;
use crate::validity_source::{DependencyCall, DependencyReturn};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
include!("source_main_raw_startup_test_transport.rs");
const FIXTURE: &str = include_str!("../fixtures/startup_init_first10.jsonl");
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_SERVICE_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-raw-startup-prefix-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
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
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone()))
            .collect(),
    )
}
fn arr(value: V) -> Vec<V> {
    let V::Array(rows) = value else {
        panic!("raw array")
    };
    rows
}
fn invoke(replay: &mut SourceContractReplay, target: &str, args: Vec<V>) {
    let receipt = replay
        .apply(SourceContractCommand::Invoke(DependencyCall {
            target: target.into(),
            arguments: args,
        }))
        .unwrap();
    assert!(
        matches!(
            receipt.outcome,
            SourceContractCommandOutcome::Dependency(
                DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
            )
        ),
        "{target}: {:?}",
        receipt.outcome
    );
}
fn setup(
    temp: &Temp,
    raw: &str,
) -> (
    SourceContractReplay,
    SourceCompactIndex,
    NativeMainExpectation,
    NativeStartupInputsV1,
) {
    let reset = untag(raw_get(raw, "rawReset"));
    let raw_input = untag(raw_get(raw, "rawStartupInputs"));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    let constructor = untag(raw_get(raw, "rawConstructorInput"));
    assert_eq!(
        constructor.get("profile"),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    for collection in arr(constructor.get("collections")) {
        let V::String(collection) = collection.get("collection") else {
            panic!("collection")
        };
        financial
            .apply(SourceContractCommand::Import {
                collection,
                rows: vec![],
            })
            .unwrap();
    }
    let profile = reset.get("clientProfile");
    invoke(
        &mut financial,
        "SourceActivation.configureClient",
        vec![obj(&[
            ("chain", profile.get("chain")),
            ("test", profile.get("test")),
        ])],
    );
    invoke(&mut financial, "Activation.init", vec![reset.get("init")]);
    financial
        .apply(SourceContractCommand::SetHeight(profile.get("height")))
        .unwrap();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        namespace.clone(),
        "actual source callable cold DB inputs",
    )
    .unwrap();
    for category in arr(untag(raw_get(raw, "rawCategories"))) {
        if category.get("name") == s("txIndex") {
            for row in arr(category.get("rows")) {
                index.import_row(&index.snapshot().unwrap(), row).unwrap();
            }
        }
    }
    let main_client = match raw_input.get("mainClient") {
        V::String(kind) if kind == "null" => V::Null,
        V::String(kind) if kind == "undefined" => V::Undefined,
        _ => obj(&[
            ("chain", raw_input.get("ownChain")),
            ("test", V::Bool(false)),
        ]),
    };
    let error = |message: &str, code: &str| NativeStartupIoErrorV1 {
        name: "Error".into(),
        message: message.into(),
        code: s(code),
    };
    let inputs = NativeStartupInputsV1 {
        chain: "LTC".into(),
        test: false,
        main_client,
        sdk_client_fields: obj(&[
            ("chain", raw_input.get("ownChain")),
            ("test", V::Bool(false)),
        ]),
        automatic_client_error: (raw_input.get("autoReject") == V::Bool(true))
            .then(|| error("recorded auto DB client rejection", "EIO")),
        global_client_error: (raw_input.get("rejectInitClient") == V::Bool(true))
            .then(|| error("recorded actual global initialize client rejection", "EIO")),
        datastore_error: match raw_input.get("categoryFailure") {
            V::String(category) => Some(NativeStartupDatastoreErrorV1 {
                error: error(
                    &format!("recorded datastore constructor rejection {category}"),
                    "EACCES",
                ),
                category,
            }),
            _ => None,
        },
    };
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace,
        metadata: index.snapshot().unwrap(),
    };
    (financial, index, expected, inputs)
}
fn fields(owner: &SourceMainRawStartupV1) -> BTreeMap<String, V> {
    let mut fields = BTreeMap::new();
    owner
        .visit_canonical_fields(&mut |path, value| {
            assert!(
                fields.insert(path.into(), value.clone()).is_none(),
                "{path}"
            );
            Ok(())
        })
        .unwrap();
    fields
}
fn drain(owner: &mut SourceMainRawStartupV1) {
    owner
        .apply(StartupCommandV1::Drain { limit: 1024 })
        .unwrap();
    owner.apply(StartupCommandV1::EndTurn).unwrap();
}
fn assert_source_prefix(owner: &SourceMainRawStartupV1, raw: &str) {
    let source = untag(raw_get(raw, "database"));
    let fields = fields(owner);
    for key in ["initialized", "initializing", "path", "test"] {
        assert_eq!(
            fields.get(&format!("database/{key}")),
            Some(&source.get(key)),
            "{} {} {key}",
            json_string(raw_get(raw, "case")),
            json_string(raw_get(raw, "point"))
        );
    }
    let categories = arr(source.get("categories"));
    assert_eq!(
        fields.get("database/categories/count"),
        Some(&s(categories.len().to_string()))
    );
    for (i, category) in categories.iter().enumerate() {
        assert_eq!(
            fields.get(&format!("database/categories/{i}")),
            Some(category)
        );
    }
    if let StartupStopV1::Thrown(error) = owner.stop() {
        let caller = untag(raw_get(raw, "caller"));
        assert_eq!(caller.get("state"), s("Rejected"));
        assert_eq!(caller.get("error").get("name"), s(error.exception.name));
        assert_eq!(
            caller.get("error").get("message"),
            s(error.exception.message)
        );
        assert_eq!(caller.get("error").get("code"), error.code);
    }
}
#[test]
fn actual_core_first_ten_cold_callable_prefixes_retain_db_waits_and_failures() {
    assert_eq!(
        FIXTURE,
        include_str!("../fixtures/startup_init_first10_locked.jsonl")
    );
    let mut current = None;
    let mut counts = (0, 0);
    for raw in FIXTURE.lines() {
        if json_string(raw_get(raw, "kind")) == "input" {
            let temp = Temp::new();
            let (financial, index, expected, inputs) = setup(&temp, raw);
            let before = financial.runtime().collection("tallyMap").to_vec();
            let mut owner =
                SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
            owner.apply(StartupCommandV1::ConstructMain).unwrap();
            if untag(raw_get(raw, "rawStartupInputs")).get("warm") == V::Bool(true) {
                owner
                    .apply(StartupCommandV1::SettleClient { token: 1 })
                    .unwrap();
                drain(&mut owner);
            }
            owner.apply(StartupCommandV1::EnterInitialize).unwrap();
            drain(&mut owner);
            current = Some((owner, temp, before, expected));
            counts.0 += 1;
            continue;
        }
        let (owner, temp, before, expected) = current.as_mut().unwrap();
        let control = untag(raw_get(raw, "control"));
        match control.get("kind") {
            V::String(kind) if kind == "FireOriginalTimer" => {
                let (id, delay, _) = owner.next_timer().unwrap();
                assert_eq!(control.get("delay"), V::number(delay as f64));
                owner
                    .apply(StartupCommandV1::FireTimer { id, generation: 9 })
                    .unwrap();
                drain(owner);
            }
            V::String(kind)
                if kind == "ReleaseOriginalAutoDbClient"
                    || kind == "RejectOriginalAutoDbClient" =>
            {
                owner
                    .apply(StartupCommandV1::SettleClient { token: 1 })
                    .unwrap();
                drain(owner);
            }
            V::String(kind)
                if [
                    "EnterActualMainInitialize",
                    "RetainOriginalMainDelay",
                    "DrainReachedFiniteJobsOnly",
                ]
                .contains(&kind.as_str()) => {}
            other => panic!("actual control {other:?}"),
        }
        assert_source_prefix(owner, raw);
        counts.1 += 1;
        assert_eq!(
            owner.financial.runtime().collection("tallyMap"),
            before.as_slice()
        );
        assert_eq!(owner.index.snapshot().unwrap(), expected.metadata);
        assert!(
            SourceCompactIndex::open(&temp.root, expected.namespace.clone(), &expected.metadata)
                .is_err(),
            "actual native writer remains held"
        );
    }
    assert_eq!(counts, (10, 38));
}
#[test]
fn actual_same_index_txutils_prelude_precedes_db_import_and_constructor_static_init() {
    let raw = FIXTURE.lines().next().unwrap();
    let temp = Temp::new();
    let (financial, index, expected, inputs) = setup(&temp, raw);
    let foundation = financial.checkpoint().unwrap();
    let mut owner = SourceMainRawStartupV1::begin(financial, index, &expected, inputs, 9).unwrap();
    assert_eq!(owner.foundation_checkpoint(), foundation);
    assert_eq!(owner.foundation_head(), expected.financial_head);
    assert_eq!(owner.origin(), &expected);
    assert_eq!(owner.raw_inputs().chain, "LTC");
    assert_eq!(owner.profile(), PROFILE);
    let calls = |owner: &SourceMainRawStartupV1| {
        owner
            .effects()
            .iter()
            .filter(|e| {
                e.get("kind") == s("rpc-input") && e.get("method") == s("Client.getInstance")
            })
            .map(|e| e.get("args"))
            .collect::<Vec<_>>()
    };
    assert_eq!(calls(&owner), vec![V::Array(vec![]), V::Array(vec![])]);
    owner.apply(StartupCommandV1::ConstructMain).unwrap();
    assert_eq!(
        calls(&owner),
        vec![
            V::Array(vec![]),
            V::Array(vec![]),
            V::Array(vec![V::Bool(true)])
        ]
    );
    let index_state = owner
        .financial_state()
        .unwrap()
        .get("runtime")
        .get("mainProduction")
        .get("index");
    assert_eq!(index_state.get("moduleLoaded"), V::Bool(true));
    assert_eq!(
        index_state.get("classClient"),
        index_state.get("txUtilsClient")
    );
    assert_eq!(owner.raw_commands(), &[StartupCommandV1::ConstructMain]);
}
#[test]
fn configured_owner_and_wrong_pin_refuse_before_any_mutation() {
    let raw = FIXTURE.lines().next().unwrap();
    let temp = Temp::new();
    let (mut financial, index, expected, inputs) = setup(&temp, raw);
    invoke(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
            ),
            ("io", obj(&[])),
        ])],
    );
    let mut current = expected;
    current.financial_head = financial.head();
    let before = financial.runtime().state_value().unwrap();
    let bytes = financial.checkpoint().unwrap();
    let rejected = SourceMainRawStartupV1::begin(financial, index, &current, inputs.clone(), 9)
        .err()
        .expect("already configured owner");
    assert_eq!(rejected.error, SourceRuleError::UnknownDependency);
    let (financial, index) = rejected.into_owners();
    assert_eq!(financial.runtime().state_value().unwrap(), before);
    assert_eq!(financial.checkpoint().unwrap(), bytes);
    assert_eq!(index.snapshot().unwrap(), current.metadata);
    let temp = Temp::new();
    let (financial, index, expected, _) = setup(&temp, raw);
    let before = financial.checkpoint().unwrap();
    let mut wrong = expected;
    wrong.financial_head.commands += 1;
    let rejected = SourceMainRawStartupV1::begin(financial, index, &wrong, inputs, 9)
        .err()
        .expect("stale pin");
    assert_eq!(rejected.error, SourceRuleError::InvalidInput);
    assert_eq!(rejected.into_owners().0.checkpoint().unwrap(), before);
}
#[test]
fn raw_typed_startup_never_adds_an_old_invoke_namespace() {
    for policy in [
        SourceContractRuntimePolicy::GlobalClearingV2,
        SourceContractRuntimePolicy::MainOrchestrationV12,
    ] {
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        let before = replay.runtime().state_value().unwrap();
        let receipt = replay
            .apply(SourceContractCommand::Invoke(DependencyCall {
                target: "SourceMainRawStartup.step".into(),
                arguments: vec![],
            }))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            SourceContractCommandOutcome::LocalError(SourceRuleError::UnknownDependency)
        );
        assert_eq!(replay.runtime().state_value().unwrap(), before);
        let checkpoint = replay.checkpoint().unwrap();
        assert_eq!(
            SourceContractReplay::from_checkpoint(&checkpoint, replay.head())
                .unwrap()
                .checkpoint()
                .unwrap(),
            checkpoint
        );
    }
}

mod startup_retry_review {
    use super::*;
    include!("source_main_startup_retry_tests.rs");
}

mod startup_sdk_review {
    use super::*;
    include!("source_main_startup_sdk_tests.rs");
}

mod raw_startup_empty_suffix {
    use super::*;
    include!("source_main_raw_startup_suffix_tests.rs");
}

include!("source_main_raw_startup_live_tests.rs");

include!("source_main_startup_nested_tests.rs");
