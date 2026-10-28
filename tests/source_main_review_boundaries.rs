//! Callable Main evidence only: daemon import/startup/RPC are not supplied.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
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
const OBSERVER: &str = include_str!("../scripts/main_orchestration_review_observer.cjs");
fn cases(lock: bool) -> Vec<V> {
    let evidence = std::env::var("TL_MAIN_ORCHESTRATION_REVIEW_EVIDENCE_ROOT")
        .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
    let out = std::process::Command::new("node")
        .env("TL_MAIN_EVIDENCE_ROOT", evidence)
        .env("TL_MAIN_REVIEW_GROUP", "main")
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
    let mut frames = Vec::new();
    let mut current = V::Undefined;
    for line in std::str::from_utf8(&out.stdout).unwrap().lines() {
        let op = array(
            tradelayer_core::property_catalog::parse_diagnostic_value(line)
                .expect("bounded observation transport chunk"),
        );
        if op[0] == s("done") {
            assert_ne!(current, V::Undefined);
            frames.push(std::mem::replace(&mut current, V::Undefined));
        } else {
            assert_eq!(op[0], s("put"));
            assert_eq!(op.len(), 3);
            install_observation_part(&mut current, &array(op[1].clone()), op[2].clone());
        }
    }
    assert_eq!(
        current,
        V::Undefined,
        "every complete original observation terminates"
    );
    frames
}
// Reassemble the TEST observation only. Its combined state+effects can
// exceed a single SourceValue input bound. Actual commands retain every normal
// public engine validation/size limit; no observation field/order is omitted.
fn install_observation_part(current: &mut V, path: &[V], value: V) {
    if path.is_empty() {
        *current = value;
        return;
    }
    match (&path[0], current) {
        (V::String(key), V::Object(fields)) => {
            if let Some((_, next)) = fields.iter_mut().find(|(k, _)| k == key) {
                install_observation_part(next, &path[1..], value);
            } else {
                assert_eq!(path.len(), 1, "parent container must already exist");
                fields.push((key.clone(), value));
            }
        }
        (V::Number(bits), V::Array(items)) => {
            let number = f64::from_bits(*bits);
            assert!(number >= 0. && number.fract() == 0.);
            let index = number as usize;
            if index == items.len() {
                assert_eq!(path.len(), 1);
                items.push(value);
            } else {
                assert!(index < items.len());
                install_observation_part(&mut items[index], &path[1..], value);
            }
        }
        other => panic!("invalid observation transport path {other:?}"),
    }
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
  let kind=v.get("kind");
  if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","rpc","rpc-input","source-file","artifact-read","unhandled-rejection"].contains(&k.as_str())){return Some(v);}
  if kind==s("main-phase"){return Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]));}
  if kind==s("dependency"){
   let target=v.get("target");
   if matches!(&target,V::String(t)if ["TxUtils.getRawTransaction","TxUtils.getBlockHeight","TxUtils.getAddressTypeUniversal","TxUtils.extractPubkeyByType"].contains(&t.as_str())){return Some(obj(&[("kind",s("source-function")),("target",target),("args",v.get("args"))]));}
   if matches!(&target,V::String(t)if ["AMM.addCapital","AMM.redeemCapital","AMM.quoteContractOrders"].contains(&t.as_str())){let mut args=array(v.get("args"));let receiver=args.remove(0);return Some(obj(&[("kind",s("source-function")),("target",target),("instance",receiver.get("sourceAmmInstance")),("args",V::Array(args))]));}
  }
  None
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
    assert_eq!(all.get("runtimePolicy"), s("MainOrchestrationV12"));
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
    assert_eq!(&bytes[..5], b"TLCR\x0c");
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

fn mixed_state(runtime: &SourceContractRuntime, case: &V) {
    activation_state(runtime, case);
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = runtime.state_value().unwrap();
    assert_eq!(
        expected.get("sourceReceiverEvidence"),
        obj(&[
            ("samePrototype", V::Bool(true)),
            ("sameSingleton", V::Bool(true))
        ]),
        "{name} actual Main receiver/class"
    );
    let amm = all.get("ammLifecycle");
    for key in ["instances", "propertyAmms", "propertyRegistryBound"] {
        let source = match key {
            "instances" => "ammInstances",
            other => other,
        };
        compare(
            &normalized(amm.get(key)),
            &normalized(expected.get(source)),
            &format!("{name} actual AMM {key}"),
        );
    }
    if expected.get("marginIdentity") != V::Undefined {
        let identity = runtime.margins().state_value().unwrap();
        let instances = V::Array(
            array(identity.get("instances"))
                .into_iter()
                .enumerate()
                .map(|(i, value)| {
                    let fields = runtime.margins().instance_value(i as u64 + 1).unwrap();
                    obj(&[
                        ("seriesId", value.get("seriesId")),
                        ("margins", value.get("margins")),
                        ("expiryIndex", fields.get("expiryIndex")),
                        ("tickerExpiry", fields.get("tickerExpiry")),
                        ("optionOI", fields.get("optionOI")),
                    ])
                })
                .collect(),
        );
        compare(
            &normalized(obj(&[
                ("instances", instances),
                ("objects", identity.get("objects")),
            ])),
            &normalized(expected.get("marginIdentity")),
            &format!("{name} actual position objects and shared references"),
        );
    }
    assert_eq!(
        all.get("mainCallable").get("activationProfile"),
        s("source-registry")
    );
}

fn production_state(runtime: &SourceContractRuntime, case: &V) {
    mixed_state(runtime, case);
    let all = runtime.state_value().unwrap();
    let p = all.get("mainProduction");
    let source = case.get("state");
    let name = text(case.get("name"));
    let idx = p.get("index");
    compare(
        &p.get("client"),
        &source.get("productionIndex").get("classClient"),
        &format!("{name} actual TxIndex static client"),
    );
    compare(
        &idx.get("classParseBlock"),
        &source.get("productionIndex").get("classParseBlock"),
        &format!("{name} actual TxIndex static parse height"),
    );
    assert_eq!(
        idx.get("classClient"),
        obj(&[("$rpcClient", V::number(1.))]),
        "exact owned RPC reference"
    );
    assert_eq!(
        source.get("productionIndex").get("clientIsMainClient"),
        V::Bool(true),
        "genuine original client alias"
    );
    assert_eq!(
        p.get("rpcBindings"),
        V::Array(vec![V::Array(vec![
            idx.get("classClient"),
            p.get("client")
        ])]),
        "registered handle resolves to unchanged source client metadata"
    );
    let instances = array(idx.get("instances"));
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].get("id"), V::number(1.));
    compare(
        &obj(&[
            ("client", instances[0].get("client")),
            ("parseBlock", instances[0].get("parseBlock")),
        ]),
        &source.get("productionIndex").get("instance"),
        &format!("{name} actual sole TxIndex singleton receiver"),
    );
    compare(
        &obj(&[
            ("client", p.get("client")),
            ("txIndex", V::Promise),
            ("getBlockCountAsync", obj(&[("function", s(""))])),
            ("getNetworkInfoAsync", obj(&[("function", s(""))])),
            ("genesisBlock", p.get("genesisBlock")),
            ("parseBlock", p.get("parseBlock")),
        ]),
        &source.get("productionReceiver"),
        &format!("{name} complete original Main receiver own fields"),
    );
    compare(
        &p.get("io"),
        &source.get("productionIO").get("facts"),
        &format!("{name} exact data-only recorded IO facts"),
    );
    let counts = V::Object(
        array(p.get("counts"))
            .into_iter()
            .map(|pair| {
                let pair = array(pair);
                (text(pair[0].clone()), pair[1].clone())
            })
            .collect(),
    );
    compare(
        &counts,
        &source.get("productionIO").get("counts"),
        &format!("{name} actual RPC suspension counters"),
    );
}
fn reset_production(case: &V) -> SourceContractReplay {
    let name = text(case.get("name"));
    let c = case.get("command");
    let options = c.get("reset").get("options");
    let profile = options.get("clientProfile");
    let mut replay = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    checked_input(
        &mut replay,
        C::Invoke(call(
            "SourceActivation.configureClient",
            vec![obj(&[
                ("chain", profile.get("chain")),
                ("test", profile.get("test")),
            ])],
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
        checked_input(
            &mut replay,
            C::Invoke(call("Activation.init", vec![options.get("init")])),
            &name,
        );
    }
    checked_input(
        &mut replay,
        C::Invoke(call(
            "SourceMain.configureProduction",
            vec![obj(&[
                (
                    "client",
                    obj(&[
                        ("chain", profile.get("chain")),
                        ("test", profile.get("test")),
                    ]),
                ),
                ("io", options.get("orchestration")),
                ("genesisBlock", options.get("mainGenesis")),
            ])],
        )),
        &name,
    );
    replay
}
fn genuine_production(first_ten: bool) {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true), "both genuine numeric profiles");
    assert_eq!(baseline.len(), 20);
    let mut replay = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    let (mut resets, mut calls) = (0, 0);
    for case in &baseline {
        if first_ten && calls == 10 {
            break;
        }
        let name = text(case.get("name"));
        let c = case.get("command");
        if c.get("reset") != V::Undefined {
            resets += 1;
            replay = reset_production(case);
            production_state(replay.runtime(), case);
            checkpoint(&replay, &name);
            continue;
        }
        if first_ten && calls == 10 {
            break;
        }
        calls += 1;
        if let Some(f) = failure(&c) {
            checked_input(&mut replay, C::FailStorage(f), &name);
        }
        let receipt = replay
            .apply(C::Invoke(call(
                &text(c.get("target")),
                array(c.get("args")),
            )))
            .unwrap();
        let O::Dependency(value) = receipt.outcome else {
            panic!("{name} {:?}", receipt.outcome)
        };
        compare(
            &result(value),
            &case.get("result"),
            &format!("{name} exact original outcome/transport"),
        );
        compare(
            &effects(receipt.effects),
            &case.get("effects"),
            &format!("{name} complete original ordered effects"),
        );
        production_state(replay.runtime(), case);
        checkpoint(&replay, &name);
    }
    if first_ten {
        assert_eq!(calls, 10);
    } else {
        assert_eq!((resets, calls), (10, 10));
    }
}

#[test]
fn source_main_review_boundaries_full_original_state_effects_and_replay() {
    genuine_production(false);
}
