//! Callable Main evidence only: daemon import/startup/RPC are not supplied.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};
fn s(v: impl Into<String>) -> V {
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
    if let V::String(v) = v {
        v
    } else {
        panic!("expected string")
    }
}
fn array(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("expected array")
    }
}
// Observer projection only. The pinned original class/module methods execute
// in the checked-in generator; no returned validity/financial verdict is input.
const OBSERVER: &str = include_str!("../scripts/main_activation_observer.cjs");
fn cases(lock: bool) -> Vec<V> {
    let evidence = std::env::var("TL_MAIN_ACTIVATION_EVIDENCE_ROOT")
        .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
    let out = std::process::Command::new("node")
        .env("TL_MAIN_EVIDENCE_ROOT", evidence)
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .args([
            "-e",
            OBSERVER,
            "main-observer",
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
fn call(target: &str, args: Vec<V>) -> DependencyCall {
    DependencyCall {
        target: target.into(),
        arguments: args,
    }
}
fn result(value: DependencyReturn) -> V {
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
fn failure(c: &V) -> Option<StorageFailure> {
    let f = c.get("failure");
    if f == V::Undefined {
        return None;
    }
    let op = text(f.get("op"));
    Some(StorageFailure {
        collection: text(f.get("collection")),
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
        message: text(f.get("message")),
    })
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
                format!("{path} array lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{path} fields {ak} != {bk}");
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
fn normalized(v: V) -> V {
    match v {
        V::BigNumber(t) => V::BigNumber(
            t.parse::<tradelayer_core::decimal::Decimal>()
                .map(|d| d.to_string())
                .unwrap_or(t),
        ),
        V::Array(a) => V::Array(a.into_iter().map(normalized).collect()),
        V::Object(o) => V::Object(o.into_iter().map(|(k, v)| (k, normalized(v))).collect()),
        _ => v,
    }
}
// Optional capability diagnostics use false for an absent binding. Retain
// every present value, so a true or unexpected nonempty value cannot disappear.
fn observed_rpc_profile(value: V) -> V {
    match value {
        V::Undefined => V::Bool(false),
        present => present,
    }
}
fn state(runtime: &SourceContractRuntime, case: &V) {
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = runtime.state_value().unwrap();
    let g = all.get("globalClearing");
    let mut known = std::collections::BTreeSet::new();
    for t in array(expected.get("tables")) {
        let k = text(t.get("name"));
        known.insert(k.clone());
        compare(
            &V::Array(runtime.collection(&k).to_vec()),
            &t.get("rows"),
            &format!("{name} full {k}"),
        );
    }
    for t in array(all.get("tables")) {
        if !known.contains(&text(t.get("name"))) {
            compare(
                &t.get("rows"),
                &V::Array(vec![]),
                &format!("{name} extra rows"),
            );
        }
    }
    for (actual, wanted) in [
        (runtime.tally_cache(), expected.get("tally")),
        (
            g.get("tallyStaticInitialized"),
            expected.get("tallyInitialized"),
        ),
        (g.get("tallyMod"), expected.get("tallyMod")),
        (g.get("contractModRaw"), expected.get("contractMod")),
        (g.get("propertyCache"), expected.get("propertyCache")),
        (
            all.get("proofLifecycle").get("propertyInitialized"),
            expected.get("propertyInitialized"),
        ),
        (g.get("contractSeries"), expected.get("contractSeries")),
        (
            all.get("registryLifecycle").get("registryInitialized"),
            expected.get("registryInitialized"),
        ),
        (
            all.get("registryLifecycle").get("seriesInitialized"),
            expected.get("seriesInitialized"),
        ),
        (
            all.get("mainCallable").get("activation"),
            expected.get("activation"),
        ),
        (g.get("channels").get("registry"), expected.get("channels")),
        (
            g.get("channels").get("pendingWithdrawals"),
            expected.get("pendingWithdrawals"),
        ),
        (all.get("oracleSourceCache"), expected.get("oracleCache")),
        (
            all.get("oracleSourceInitialized"),
            expected.get("oracleInitialized"),
        ),
        (
            all.get("oracleSourceWatermarks"),
            expected.get("oracleWatermarks"),
        ),
    ] {
        compare(&actual, &wanted, &format!("{name} source caches"));
    }
    for key in ["blockTrades", "oracleMarks", "nativeMarks"] {
        compare(&all.get(key), &expected.get(key), &format!("{name} {key}"));
    }
    for key in ["deleverageTrades", "liquidationRecords"] {
        compare(&g.get(key), &expected.get(key), &format!("{name} {key}"));
    }
    let V::Object(ins) = all.get("insurance") else {
        panic!()
    };
    compare(
        &V::Array(
            ins.into_iter()
                .map(|(k, v)| V::Array(vec![s(k), v]))
                .collect(),
        ),
        &expected.get("insurance"),
        &format!("{name} insurance"),
    );
    let margins = array(expected.get("margins"));
    assert_eq!(
        array(runtime.margins().state_value().unwrap().get("instances")).len(),
        margins.len(),
        "{name} margin receiver count"
    );
    for (i, m) in margins.iter().enumerate() {
        compare(
            &runtime.margins().instance_value(i as u64 + 1).unwrap(),
            m,
            &format!("{name} margin{i}"),
        );
    }
    let books = array(expected.get("books"));
    assert_eq!(
        array(runtime.queue().state_value().get("instances")).len(),
        books.len(),
        "{name} book receiver count"
    );
    for b in books {
        let V::Number(id) = b.get("id") else { panic!() };
        let V::Object(fields) = b else { panic!() };
        let wanted = V::Object(fields.into_iter().filter(|(k, _)| k != "id").collect());
        compare(
            &normalized(
                runtime
                    .queue()
                    .instance_value(f64::from_bits(id) as u64)
                    .unwrap(),
            ),
            &normalized(wanted),
            &format!("{name} whole queue receiver"),
        );
    }
    let objects = array(all.get("margins").get("objects"));
    let contexts = V::Array(
        array(g.get("contexts"))
            .into_iter()
            .map(|p| {
                let p = array(p);
                V::Array(vec![
                    p[0].clone(),
                    V::Array(
                        array(p[1].clone())
                            .into_iter()
                            .map(|id| {
                                let V::Number(id) = id else { panic!() };
                                objects[f64::from_bits(id) as usize].clone()
                            })
                            .collect(),
                    ),
                ])
            })
            .collect(),
    );
    compare(
        &contexts,
        &expected.get("contexts"),
        &format!("{name} clearing aliases"),
    );
    compare(
        &all.get("liquidityRewards").get("queue"),
        &expected.get("globals").get("liquidityQueue"),
        &format!("{name} reward queue"),
    );
    compare(
        &all.get("liquidityRewards").get("volumeCache"),
        &expected.get("globals").get("liquidityVolume"),
        &format!("{name} reward volume"),
    );
}
fn effects(values: Vec<V>) -> V {
    V::Array(values.into_iter().filter_map(|v|{
    let kind=v.get("kind");if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","rpc","rpc-input","source-file","unhandled-rejection"].contains(&k.as_str())){Some(v)}else if kind==s("main-phase")||(kind==s("dependency")&&matches!(v.get("target"),V::String(t)if ["TxUtils.getRawTransaction","TxUtils.getBlockHeight"].contains(&t.as_str()))){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}else{None}
}).collect())
}
fn imports(case: &V) -> Vec<C> {
    array(case.get("state").get("tables"))
        .into_iter()
        .map(|t| C::Import {
            collection: text(t.get("name")),
            rows: array(t.get("rows")),
        })
        .collect()
}

fn lower_state(runtime: &SourceContractRuntime, case: &V) {
    state(runtime, case);
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = runtime.state_value().unwrap();
    for key in ["clearlistsMap", "banlist", "xpubCache", "merkleXpubMap"] {
        compare(
            &all.get("clearlistLifecycle").get(key),
            &expected.get("clearlist").get(key),
            &format!("{name} actual ClearList {key}"),
        );
    }
    assert_eq!(all.get("runtimePolicy"), s("ActivationLifecycleV10"));
    assert_eq!(all.get("spotLifecycle").get("entry"), s("main-subgraph"));
    assert_eq!(all.get("spotLifecycle").get("mainActive"), V::Bool(false));
    assert_eq!(all.get("spotLifecycle").get("pendingOrdersRead"), V::Null);
    compare(
        &all.get("spotLifecycle").get("feeBufferInitialized"),
        &expected.get("feeBufferInitialized"),
        &format!("{name} source fee Map initialization"),
    );
    compare(
        &normalized(all.get("feeBuffer")),
        &normalized(expected.get("feeBuffer")),
        &format!("{name} actual queued fees"),
    );
    compare(
        &observed_rpc_profile(all.get("globals").get("sourceSpotRpcDocuments")),
        &observed_rpc_profile(expected.get("globals").get("spotRpcDocumentsBound")),
        &format!("{name} explicit original RPC document profile"),
    );
    compare(
        &V::Bool(all.get("globals").get("tallyMap") != V::Undefined),
        &observed_rpc_profile(expected.get("globals").get("tallyMapBound")),
        &format!("{name} actual Tally realm binding"),
    );
    let instance = expected.get("tallyInstance");
    let instance = if instance == V::Null {
        V::Undefined
    } else {
        instance
    };
    compare(
        &all.get("sendCommit").get("tallyInstance"),
        &instance,
        &format!("{name} actual Tally singleton receiver"),
    );
    assert_eq!(
        all.get("sendCommit").get("pendingRead"),
        V::Bool(false),
        "{name} stable observed completion drains source read job"
    );
}

fn checked_input(replay: &mut SourceContractReplay, command: C, name: &str) {
    let receipt = replay.apply(command).unwrap();
    assert!(
        !matches!(receipt.outcome, O::LocalError(_)),
        "{name} input refused: {:?}",
        receipt.outcome
    );
}
fn activation_state(runtime: &SourceContractRuntime, case: &V) {
    lower_state(runtime, case);
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = runtime.state_value().unwrap();
    compare(
        &all.get("tables"),
        &expected.get("tables"),
        &format!("{name} complete persisted table inventory/order"),
    );
    for (actual, wanted) in [
        (
            all.get("activationLifecycle").get("vesting"),
            expected.get("vesting"),
        ),
        (
            all.get("activationLifecycle").get("consensusVector"),
            expected.get("consensusVector"),
        ),
        (
            all.get("globalClearing").get("unhandledRejections"),
            expected.get("unhandledRejections"),
        ),
    ] {
        compare(
            &actual,
            &wanted,
            &format!("{name} actual bootstrap receiver/history"),
        );
    }
    assert_eq!(
        expected.get("insuranceInstances"),
        V::Array(vec![]),
        "{name} source transient bootstrap insurance constructor stays outside the singleton arena"
    );
    // The original singleton getter stores these on its receiver; the class
    // static reads observed by Main remain undefined, not invented parameters.
    for key in [
        "adminAddress",
        "sponsorAddress",
        "expiryInterval",
        "hedgeLeverage",
    ] {
        assert_eq!(
            expected.get("vestingStatic").get(key),
            V::Undefined,
            "{name} genuine static Vesting {key}"
        );
    }
    assert_eq!(
        all.get("mainCallable").get("pendingHeight"),
        V::Null,
        "{name} height continuation drained"
    );
    assert_eq!(
        all.get("spotLifecycle").get("pendingOrdersRead"),
        V::Null,
        "{name} actual Main queue receiver read drained"
    );
    assert_eq!(
        all.get("spotLifecycle").get("pendingTokenThresholdReads"),
        V::Array(vec![]),
        "{name} inherited threshold jobs drained"
    );
    assert_eq!(
        all.get("spotLifecycle").get("pendingWithdrawalSave"),
        V::Null,
        "{name} inherited withdrawal save drained"
    );
    let activation = all.get("activationLifecycle");
    assert_eq!(
        activation.get("pendingInsurance"),
        V::Null,
        "{name} insurance job drained"
    );
    assert_eq!(
        activation.get("pendingClearlists"),
        V::Array(vec![]),
        "{name} ClearList jobs drained"
    );
    assert_eq!(
        activation.get("nativeErrors"),
        V::Array(vec![]),
        "{name} native Error receiver scope drained"
    );
    let awaits = activation.get("awaitState");
    for key in ["active", "running"] {
        assert_eq!(awaits.get(key), V::Bool(false), "{name} {key}");
    }
    for key in ["clock", "serial", "childDepth"] {
        assert_eq!(awaits.get(key), V::number(0.), "{name} {key}");
    }
}
fn checkpoint(replay: &SourceContractReplay, name: &str) {
    let bytes = replay.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLCR\x0a");
    let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
    assert_eq!(
        &restored, replay,
        "{name} complete actual source-history reconstruction"
    );
    assert_eq!(
        restored.checkpoint().unwrap(),
        bytes,
        "{name} canonical exact re-encoding"
    );
}
fn genuine_activation_main(lock: bool) {
    let baseline = cases(lock);
    assert_eq!(baseline.len(), 105);
    let mut replay = SourceContractReplay::with_policy(P::ActivationLifecycleV10).unwrap();
    let (mut resets, mut main_calls, mut supports, mut unavailable) = (0, 0, 0, 0);
    let mut after_unavailable = false;
    for case in &baseline {
        let command = case.get("command");
        let name = text(case.get("name"));
        if command.get("reset") != V::Undefined {
            resets += 1;
            replay = SourceContractReplay::with_policy(P::ActivationLifecycleV10).unwrap();
            let options = command.get("reset").get("options");
            let profile = options.get("clientProfile");
            checked_input(
                &mut replay,
                C::Invoke(call(
                    "SourceActivation.configureClient",
                    vec![if profile == V::Undefined {
                        V::Null
                    } else {
                        obj(&[
                            ("chain", profile.get("chain")),
                            ("test", profile.get("test")),
                        ])
                    }],
                )),
                &name,
            );
            if profile.get("height") != V::Undefined {
                checked_input(&mut replay, C::SetHeight(profile.get("height")), &name);
            }
            for input in imports(case) {
                checked_input(&mut replay, input, &name);
            }
            if options.get("init") != V::Undefined {
                let receipt = replay
                    .apply(C::Invoke(call(
                        "Activation.init",
                        vec![options.get("init")],
                    )))
                    .unwrap();
                assert_eq!(
                    receipt.outcome,
                    O::Dependency(DependencyReturn::Pending(V::Undefined)),
                    "{name} genuine metadata init"
                );
            }
            after_unavailable = false;
            activation_state(replay.runtime(), case);
            checkpoint(&replay, &name);
            continue;
        }
        let target = text(command.get("target"));
        let arguments = array(command.get("args"));
        if case.get("domain") == s("unavailable-capability") {
            unavailable += 1;
            let before = replay.runtime().clone();
            let receipt = replay.apply(C::Invoke(call(&target, arguments))).unwrap();
            assert_eq!(
                receipt.outcome,
                O::LocalError(SourceRuleError::UnknownDependency),
                "{name} missing Client stays an explicit local capability boundary"
            );
            assert!(receipt.effects.is_empty());
            assert_eq!(
                replay.runtime(),
                &before,
                "{name} excluded source prefix must not become Rust prestate"
            );
            after_unavailable = true;
            checkpoint(&replay, &name);
            continue;
        }
        assert!(
            !after_unavailable,
            "no available call may inherit an unavailable adapter prefix"
        );
        if target == "Property.createToken" {
            supports += 1;
            assert_eq!(command.get("sourceSupport"), V::Bool(true));
        } else {
            assert!(target.starts_with("Main."));
            main_calls += 1;
        }
        if let Some(f) = failure(&command) {
            checked_input(&mut replay, C::FailStorage(f), &name);
        }
        let receipt = replay.apply(C::Invoke(call(&target, arguments))).unwrap();
        let O::Dependency(value) = receipt.outcome else {
            panic!(
                "{name} must execute owned original source: {:?}",
                receipt.outcome
            )
        };
        compare(
            &result(value),
            &case.get("result"),
            &format!("{name} source outcome/transport"),
        );
        activation_state(replay.runtime(), case);
        compare(
            &effects(receipt.effects),
            &case.get("effects"),
            &format!("{name} complete ordered DB/RPC/file/postphase/unhandled effects"),
        );
        checkpoint(&replay, &name);
    }
    assert_eq!((resets, main_calls, supports, unavailable), (29, 43, 32, 1));
}
#[test]
fn original_main_raw0_bootstrap_all_native_subsets_failures_retries_default() {
    genuine_activation_main(false);
}
#[test]
fn original_main_raw0_bootstrap_all_native_subsets_failures_retries_locked() {
    genuine_activation_main(true);
}
#[test]
fn v8_v9_main_still_refuse_raw0_atomically() {
    let baseline = cases(false);
    let first = baseline
        .iter()
        .find(|r| r.get("command").get("target") == s("Main.processTxSet"))
        .unwrap();
    for (policy, version) in [(P::MainCallableV8, 8), (P::SpotLifecycleV9, 9)] {
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        for input in imports(&baseline[0]) {
            checked_input(&mut replay, input, "old-policy prestate");
        }
        let before = replay.runtime().clone();
        let command = first.get("command");
        let receipt = replay
            .apply(C::Invoke(call(
                "Main.processTxSet",
                array(command.get("args")),
            )))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::InvalidInput)
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(
            replay.runtime(),
            &before,
            "older Main refusal keeps every receiver and table"
        );
        assert_eq!(
            replay
                .runtime()
                .state_value()
                .unwrap()
                .get("activationLifecycle"),
            V::Undefined
        );
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(bytes[4], version);
        assert_eq!(
            SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay
        );
    }
}
