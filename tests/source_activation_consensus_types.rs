//! Original Consensus catch/type behavior, using the existing V10 Core.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
fn s(value: impl Into<String>) -> V {
    V::String(value.into())
}
fn obj(values: &[(&str, V)]) -> V {
    V::Object(
        values
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(value: V) -> String {
    let V::String(value) = value else {
        panic!("string {value:?}")
    };
    value
}
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array {value:?}")
    };
    value
}
fn cases(lock: bool) -> Vec<V> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage = std::env::var("TL_CONSENSUS_TYPES_EVIDENCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.to_path_buf());
    let out = std::process::Command::new("node")
        .env("ACTIVATION_REPO", root)
        .arg(stage.join("scripts/v10_consensus_type_vectors.cjs"))
        .args([
            "--check",
            "--print",
            if lock { "--lock" } else { "--baseline" },
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::str::from_utf8(&out.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn projected(value: DependencyReturn) -> V {
    let (transport, outcome) = match value {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
        DependencyReturn::Throw(e) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(e.name)), ("message", s(e.message))]),
            )]),
        ),
        DependencyReturn::PendingThrow(e) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(e.name)), ("message", s(e.message))]),
            )]),
        ),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
fn compare(a: &V, b: &V, label: &str) {
    fn diff(a: &V, b: &V, path: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return diff(a, b, format!("{path}[{i}]"));
                    }
                }
                format!("{path} lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{path} keys {ak} != {bk}");
                    }
                    if av != bv {
                        return diff(av, bv, format!("{path}.{ak}"));
                    }
                }
                format!("{path} field counts {} != {}", a.len(), b.len())
            }
            _ => format!("{path}: {a:?} != {b:?}"),
        }
    }
    assert!(a == b, "{label}: {}", diff(a, b, String::new()));
}
#[test]
fn original_consensus_raw_types_relational_order_and_swallowed_errors_replay() {
    let cases = cases(false);
    assert_eq!(cases, self::cases(true));
    assert_eq!(cases.len(), 24);
    let mut selections = 0;
    for case in cases {
        let name = text(case.get("name"));
        let mut replay = SourceContractReplay::with_policy(P::ActivationLifecycleV10).unwrap();
        let expected = case.get("state");
        for table in array(case.get("command").get("collections")) {
            replay
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        let receipt = replay
            .apply(C::Invoke(DependencyCall {
                target: "Consensus.pushLatestActivationToConsensusVector".into(),
                arguments: vec![],
            }))
            .unwrap();
        let value = match receipt.outcome {
            O::Dependency(value) => value,
            other => panic!("{name}: {other:?}"),
        };
        compare(
            &projected(value),
            &case.get("result"),
            &format!("{name} actual catch/transport"),
        );
        for table in array(expected.get("tables")) {
            compare(
                &V::Array(
                    replay
                        .runtime()
                        .collection(&text(table.get("name")))
                        .to_vec(),
                ),
                &table.get("rows"),
                &format!("{name} all table rows/order"),
            );
        }
        let state = replay.runtime().state_value().unwrap();
        compare(
            &state.get("activationLifecycle").get("consensusVector"),
            &expected.get("vector"),
            &format!("{name} literal Consensus vector"),
        );
        compare(
            &state.get("globalClearing").get("unhandledRejections"),
            &expected.get("unhandled"),
            &format!("{name} source catch stays fulfilled"),
        );
        assert_eq!(
            replay.runtime().tally_cache(),
            V::Array(vec![]),
            "{name} no invented balances"
        );
        let effects = V::Array(
            receipt
                .effects
                .into_iter()
                .filter(|value| {
                    [s("database"), s("db"), s("source-file")].contains(&value.get("kind"))
                })
                .collect(),
        );
        compare(
            &effects,
            &case.get("effects"),
            &format!("{name} exact ordered original DB/file effects"),
        );
        if !array(expected.get("vector")).is_empty() {
            selections += 1;
        }
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(&bytes[..5], b"TLCR\x0a");
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), bytes);
    }
    assert_eq!(selections, 10, "all actual typed selections are exercised");
}
