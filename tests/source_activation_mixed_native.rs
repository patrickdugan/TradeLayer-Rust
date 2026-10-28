//! Original raw0 composition over the existing Core owners, under V10 only.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as Replay,
};
use tradelayer_core::source_contract_runtime::{SourceContractRuntimePolicy as P, StorageFailure};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
const REFERENCE: &str = include_str!("../scripts/mixed_activation_vectors.cjs");
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn arr(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("array {v:?}")
    }
}
fn txt(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("string {v:?}")
    }
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
fn source(lock: bool) -> Vec<V> {
    let code = REFERENCE
        .strip_prefix("#!/usr/bin/env node\n")
        .unwrap_or(REFERENCE);
    let output = std::process::Command::new("node")
        .args(["-e", code, "--", "--stdout"])
        .env("ACTIVATION_MIXED_REPO", env!("CARGO_MANIFEST_DIR"))
        .env("TL_SOURCE_BN_VERSION", if lock { "9.1.2" } else { "9.3.1" })
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn error(e: &tradelayer_core::validity_source::SourceException) -> V {
    obj(&[("name", s(&e.name)), ("message", s(&e.message))])
}
fn result(outcome: &O, raw: bool, params: V) -> V {
    if raw {
        return obj(&[(
            "outcome",
            match outcome {
                O::Source(SourceOutcome::Returned(v)) => v.clone(),
                O::Source(SourceOutcome::Thrown(e)) => {
                    obj(&[("params", params), ("thrown", error(e))])
                }
                _ => panic!("raw {outcome:?}"),
            },
        )]);
    }
    let (transport, outcome) = match outcome {
        O::Dependency(DependencyReturn::Immediate(v)) => {
            ("Immediate", obj(&[("returned", v.clone())]))
        }
        O::Dependency(DependencyReturn::Pending(v)) => ("Pending", obj(&[("returned", v.clone())])),
        O::Dependency(DependencyReturn::Throw(e)) => ("Throw", obj(&[("thrown", error(e))])),
        O::Dependency(DependencyReturn::PendingThrow(e)) => {
            ("PendingThrow", obj(&[("thrown", error(e))]))
        }
        _ => panic!("direct {outcome:?}"),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
fn checkpoint(replay: &Replay, name: &str) {
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(
        Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
        *replay,
        "{name} actual owner checkpoint/replay"
    );
}
fn genuine(lock: bool) {
    let mut replay = Replay::with_policy(P::ActivationLifecycleV10).unwrap();
    let (mut resets, mut calls, mut unavailable) = (0, 0, 0);
    for case in source(lock) {
        let name = txt(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            resets += 1;
            replay = Replay::with_policy(P::ActivationLifecycleV10).unwrap();
            let options = command.get("reset");
            let profile = options.get("clientProfile");
            replay
                .apply(invoke(
                    "SourceActivation.configureClient",
                    vec![if profile == V::Undefined {
                        V::Null
                    } else {
                        profile
                    }],
                ))
                .unwrap();
            for table in arr(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: txt(table.get("name")),
                        rows: arr(table.get("rows")),
                    })
                    .unwrap();
            }
            if options.get("init") != V::Undefined {
                let receipt = replay
                    .apply(invoke("Activation.init", vec![options.get("init")]))
                    .unwrap();
                assert_eq!(
                    receipt.outcome,
                    O::Dependency(DependencyReturn::Pending(V::Undefined)),
                    "{name} original metadata initialization"
                );
            }
            checkpoint(&replay, &name);
            continue;
        }
        if matches!(command.get("failure"), V::Object(_)) {
            let f = command.get("failure");
            let op = txt(f.get("op"));
            replay
                .apply(C::FailStorage(StorageFailure {
                    collection: txt(f.get("collection")),
                    operation: if op == "getDatabase" {
                        op
                    } else {
                        format!("{op}Async")
                    },
                    skip: if let V::Number(b) = f.get("skip") {
                        f64::from_bits(b) as usize
                    } else {
                        0
                    },
                    message: txt(f.get("message")),
                }))
                .unwrap();
        }
        let raw = matches!(command.get("raw"), V::Array(_));
        let before = replay.runtime().clone();
        let receipt = replay
            .apply(if raw {
                C::ExecuteContract(arr(command.get("raw")))
            } else {
                invoke(&txt(command.get("target")), arr(command.get("args")))
            })
            .unwrap();
        if name.starts_with("missing-client-") {
            unavailable += 1;
            assert_eq!(
                receipt.outcome,
                O::LocalError(SourceRuleError::UnknownDependency),
                "{name} unavailable capability stays local"
            );
            assert_eq!(
                replay.runtime(),
                &before,
                "{name} excluded source poststate cannot enter Core"
            );
            assert!(receipt.effects.is_empty());
            checkpoint(&replay, &name);
            continue;
        }
        calls += 1;
        assert!(
            !matches!(receipt.outcome, O::LocalError(_)),
            "{name} {:?} {:?}",
            receipt.outcome,
            receipt.effects
        );
        let params = replay
            .runtime()
            .last_contract_parameters()
            .cloned()
            .unwrap_or(V::Undefined);
        assert_eq!(
            result(&receipt.outcome, raw, params),
            case.get("result"),
            "{name} exact source result"
        );
        let actual = replay.runtime().state_value().unwrap();
        let expected = case.get("state");
        assert_eq!(
            actual.get("tables"),
            expected.get("tables"),
            "{name} complete persisted collection inventory and order"
        );
        for table in arr(expected.get("tables")) {
            assert_eq!(
                V::Array(
                    replay
                        .runtime()
                        .collection(&txt(table.get("name")))
                        .to_vec()
                ),
                table.get("rows"),
                "{name} table {:?}",
                table.get("name")
            );
        }
        for (value, key) in [
            (actual.get("mainCallable").get("activation"), "activation"),
            (actual.get("activationLifecycle").get("vesting"), "vesting"),
            (
                actual.get("activationLifecycle").get("consensusVector"),
                "consensusVector",
            ),
            (actual.get("tally"), "tally"),
            (
                actual.get("globalClearing").get("tallyStaticInitialized"),
                "tallyInitialized",
            ),
            (actual.get("globalClearing").get("tallyMod"), "tallyMod"),
            (
                actual.get("sendCommit").get("tallyInstance"),
                "tallyInstance",
            ),
            (
                actual.get("globalClearing").get("propertyCache"),
                "propertyCache",
            ),
            (
                actual.get("proofLifecycle").get("propertyInitialized"),
                "propertyInitialized",
            ),
            (
                actual.get("globalClearing").get("unhandledRejections"),
                "unhandled",
            ),
            (
                actual.get("globalClearing").get("channels").get("registry"),
                "channels",
            ),
            (
                actual
                    .get("globalClearing")
                    .get("channels")
                    .get("pendingWithdrawals"),
                "withdrawals",
            ),
        ] {
            assert_eq!(value, expected.get(key), "{name} actual receiver {key}");
        }
        let await_state = actual.get("activationLifecycle").get("awaitState");
        assert_eq!(
            await_state.get("active"),
            V::Bool(false),
            "{name} initializer scope exited"
        );
        assert_eq!(await_state.get("running"), V::Bool(false));
        for key in ["clock", "serial", "childDepth"] {
            assert_eq!(await_state.get(key), n(0.), "{name} drained {key}");
        }
        let catalogs = actual.get("clearlistLifecycle");
        let original = expected.get("clearlist");
        for (key, source) in [
            ("clearlistsMap", "lists"),
            ("banlist", "banlist"),
            ("xpubCache", "xpubCache"),
            ("merkleXpubMap", "roots"),
        ] {
            assert_eq!(
                catalogs.get(key),
                original.get(source),
                "{name} clearlist {key}"
            );
        }
        assert_eq!(
            actual.get("insurance"),
            obj(&[]),
            "{name} transient constructor must not enter instances"
        );
        assert_eq!(expected.get("insuranceInstances"), V::Array(vec![]));
        assert_eq!(
            actual.get("activationLifecycle").get("pendingInsurance"),
            V::Null,
            "{name} continuation drained"
        );
        assert_eq!(
            actual.get("activationLifecycle").get("pendingClearlists"),
            V::Array(vec![])
        );
        assert_eq!(
            actual.get("activationLifecycle").get("nativeErrors"),
            V::Array(vec![])
        );
        let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(kind)if["database","db","uuid","random","rpc-clock","rpc","rpc-input","source-file"].contains(&kind.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&e.get("target")==s("Logic.activateTradeLayer"){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else{None}
        }).collect();
        let expected_effects = arr(case.get("effects"))
            .into_iter()
            .filter(|e| {
                e.get("kind") != s("source-function")
                    || e.get("target") == s("Logic.activateTradeLayer")
            })
            .collect();
        assert_eq!(
            V::Array(effects),
            V::Array(expected_effects),
            "{name} complete ordered original DB/RPC/file effects"
        );
        checkpoint(&replay, &name);
    }
    assert_eq!((resets, calls, unavailable), (16, 48, 0));
}
#[test]
fn activation_v10_all_mixed_native_subsets_default() {
    genuine(false);
}
#[test]
fn activation_v10_all_mixed_native_subsets_locked() {
    genuine(true);
}
