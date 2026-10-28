//! Unchanged pinned raw3 UTXO-trade ingress over the sole actual Core stores.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as Replay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
fn n(v: f64) -> V {
    V::number(v)
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
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
const REFERENCE: &str = include_str!("../scripts/lower_utxo_trade_vectors.cjs");

fn source(lock: bool) -> Vec<V> {
    let output = std::process::Command::new("node")
        .args([
            "-e",
            REFERENCE.trim_start_matches("#!/usr/bin/env node"),
            "--",
            "--stdout",
        ])
        .env("UTXO_TRADE_REPO", env!("CARGO_MANIFEST_DIR"))
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
fn thrown(e: &tradelayer_core::validity_source::SourceException) -> V {
    obj(&[("name", s(&e.name)), ("message", s(&e.message))])
}
fn result(o: &O, raw: bool, params: V) -> V {
    if raw {
        return obj(&[(
            "outcome",
            match o {
                O::Source(SourceOutcome::Returned(v)) => v.clone(),
                O::Source(SourceOutcome::Thrown(e)) => {
                    obj(&[("params", params), ("thrown", thrown(e))])
                }
                _ => panic!("raw {o:?}"),
            },
        )]);
    }
    let (transport, outcome) = match o {
        O::Dependency(DependencyReturn::Immediate(v)) => {
            ("Immediate", obj(&[("returned", v.clone())]))
        }
        O::Dependency(DependencyReturn::Pending(v)) => ("Pending", obj(&[("returned", v.clone())])),
        O::Dependency(DependencyReturn::Throw(e)) => ("Throw", obj(&[("thrown", thrown(e))])),
        O::Dependency(DependencyReturn::PendingThrow(e)) => {
            ("PendingThrow", obj(&[("thrown", thrown(e))]))
        }
        _ => panic!("direct {o:?}"),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
fn genuine_lower_utxo_trade_cases(lock: bool) {
    let mut replay = Replay::with_policy(P::MainCallableV8).unwrap();
    let (mut resets, mut seeds, mut calls, mut refusals, mut bindings) = (0, 0, 0, 0, 0);
    for case in source(lock) {
        let name = txt(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            resets += 1;
            replay = Replay::with_policy(P::MainCallableV8).unwrap();
            let options = command.get("reset");
            replay
                .apply(invoke(
                    "SourceMain.configure",
                    vec![obj(&[("activationProfile", s("source-registry"))])],
                ))
                .unwrap();
            if let V::String(entry) = options.get("entry") {
                replay.apply(C::SetEntry(entry)).unwrap();
            }

            for table in arr(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: txt(table.get("name")),
                        rows: arr(table.get("rows")),
                    })
                    .unwrap();
            }
            if options.get("rpc") == V::Bool(true) {
                replay
                    .apply(invoke("SourceGlobal.bindRpcTransactions", vec![]))
                    .unwrap();
            }
            continue;
        }
        if command.get("bindTally") == V::Bool(true) {
            bindings += 1;
            replay
                .apply(invoke("SourceGlobal.bindTally", vec![]))
                .unwrap();
            continue;
        }
        if matches!(command.get("seed"), V::Object(_)) {
            seeds += 1;
            let seed = command.get("seed");
            replay
                .apply(C::Import {
                    collection: txt(seed.get("collection")),
                    rows: arr(seed.get("rows")),
                })
                .unwrap();
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
        let before = replay.runtime().state_value().unwrap();
        let receipt = replay
            .apply(if raw {
                C::ExecuteContract(arr(command.get("raw")))
            } else {
                invoke(&txt(command.get("target")), arr(command.get("args")))
            })
            .unwrap();
        if command.get("unsupported") == V::Bool(true) {
            assert_eq!(
                receipt.outcome,
                O::LocalError(SourceRuleError::UnknownDependency),
                "{name} explicit unbound RPC domain"
            );
            assert_eq!(
                replay.runtime().state_value().unwrap(),
                before,
                "{name} local admission rollback"
            );
            assert!(receipt.effects.is_empty());
            refusals += 1;
            continue;
        }
        calls += 1;
        assert!(
            !matches!(receipt.outcome, O::LocalError(_)),
            "{name} {:?} effects={:?}",
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
            "{name} exact source outcome"
        );
        let state = replay.runtime().state_value().unwrap();
        let expected = case.get("state");
        for table in arr(expected.get("tables")) {
            assert_eq!(
                V::Array(
                    replay
                        .runtime()
                        .collection(&txt(table.get("name")))
                        .to_vec()
                ),
                table.get("rows"),
                "{name} persisted {:?}",
                table.get("name")
            );
        }
        for (actual, wanted) in [
            (state.get("tally"), "tally"),
            (
                state.get("globalClearing").get("tallyStaticInitialized"),
                "tallyInitialized",
            ),
            (state.get("globalClearing").get("tallyMod"), "tallyMod"),
            (
                state.get("globalClearing").get("propertyCache"),
                "propertyCache",
            ),
            (
                state.get("proofLifecycle").get("propertyInitialized"),
                "propertyInitialized",
            ),
            (
                state.get("globalClearing").get("unhandledRejections"),
                "unhandled",
            ),
        ] {
            assert_eq!(actual, expected.get(wanted), "{name} live source {wanted}");
        }
        assert_eq!(
            state.get("mainCallable").get("activation"),
            expected.get("activation"),
            "{name} real Activation receiver"
        );
        let channels = state.get("globalClearing").get("channels");
        assert_eq!(
            channels.get("registry"),
            expected.get("channels"),
            "{name} actual Channels receiver cache"
        );
        assert_eq!(
            channels.get("pendingWithdrawals"),
            expected.get("withdrawals"),
            "{name} actual withdrawals"
        );
        assert_eq!(
            state.get("sendCommit").get("tallyInstance"),
            expected.get("tallyInstance"),
            "{name} distinct empty instance cache"
        );
        assert_eq!(
            state.get("sendCommit").get("pendingRead"),
            V::Bool(false),
            "{name} declared deferred read drained at public boundary"
        );
        let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","rpc","activation"].contains(&t.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else{None}
        }).collect::<Vec<_>>();
        assert_eq!(
            V::Array(effects),
            V::Array(
                arr(case.get("effects"))
                    .into_iter()
                    .filter(|v| v.get("kind") != s("source-function")
                        || matches!(v.get("target"),V::String(t)if t.starts_with("Logic.")))
                    .collect()
            ),
            "{name} complete ordered DB/environment/observed-source calls"
        );
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(
            Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay,
            "{name} full real replay/cache/failure prefix"
        );
    }
    assert_eq!((resets, seeds, calls, refusals, bindings), (8, 0, 34, 0, 0));
}

#[test]
fn lower_utxo_trade_original_source_default() {
    genuine_lower_utxo_trade_cases(false);
}
#[test]
fn lower_utxo_trade_original_source_locked_bignumber() {
    genuine_lower_utxo_trade_cases(true);
}
#[test]
fn lower_utxo_trade_raw_admission_preserves_older_policy_boundaries() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        assert_eq!(
            runtime.execute_contract_unverified(&[
                s("raw3-older"),
                n(3.),
                s("tl"),
                s("7,3b2ozk,1,rs,1,0,0,"),
                s("actor"),
                V::Array(vec![]),
                n(0.),
                n(0.),
                n(10.)
            ]),
            Err(SourceRuleError::InvalidInput)
        );
        assert_eq!(
            runtime, before,
            "{policy:?} local refusal cannot change state"
        );
    }
}
