//! Finite original Main/Persistence phases over recorded data and IO.
use crate::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use crate::source_contract_runtime::{
    SourceContractRuntime as ActualRuntime, SourceContractRuntimePolicy as P,
};
use crate::validity_source::{DependencyCall, SourceValue as V};
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
const OBSERVER: &str = include_str!("../scripts/main_file_retry_observer.cjs");
fn cases(lock: bool) -> Vec<V> {
    let evidence = std::env::var("TL_MAIN_FILE_RETRY_EVIDENCE_ROOT")
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
    let mut frames = Vec::new();
    let mut current = V::Undefined;
    for line in std::str::from_utf8(&out.stdout).unwrap().lines() {
        let op = array(
            crate::property_catalog::parse_diagnostic_value(line)
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
            t.parse::<crate::decimal::Decimal>()
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
fn state(runtime: &ObservedRuntime<'_>, case: &V) {
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
  if kind==s("source-function") && matches!(v.get("target"),V::String(ref target)if target.starts_with("Persistence.")){return None;}
  if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","rpc","rpc-input","source-file","artifact-read","unhandled-rejection","timer","timer-scheduled","timer-fired","timer-cancelled","filesystem","source-function"].contains(&k.as_str())){return Some(v);}
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

fn lower_state(runtime: &ObservedRuntime<'_>, case: &V) {
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
fn activation_state(runtime: &ObservedRuntime<'_>, case: &V) {
    lower_state(runtime, case);
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = runtime.state_value().unwrap();
    compare(
        &all.get("tables"),
        &expected.get("tables"),
        &format!("{name} complete persisted table inventory/order"),
    );
    for (field, actual, wanted) in [
        (
            "vesting",
            all.get("activationLifecycle").get("vesting"),
            expected.get("vesting"),
        ),
        (
            "consensusVector",
            all.get("activationLifecycle").get("consensusVector"),
            expected.get("consensusVector"),
        ),
        (
            "unhandledRejections",
            all.get("globalClearing").get("unhandledRejections"),
            expected.get("unhandledRejections"),
        ),
    ] {
        compare(
            &actual,
            &wanted,
            &format!("{name} actual bootstrap receiver/history {field}: wanted {wanted:?}; actual {actual:?}"),
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
fn mixed_state(runtime: &ObservedRuntime<'_>, case: &V) {
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

fn service_state(runtime: &ObservedRuntime<'_>, case: &V) {
    mixed_state(runtime, case);
    let all = runtime.state_value().unwrap();
    let p = all.get("mainProduction");
    let service = p.get("service");
    compare(
        &service.get("fileIO"),
        &case.get("state").get("fileIO"),
        "exact recorded filesystem facts and operation counters",
    );
    compare(
        &service.get("pause"),
        &case.get("state").get("mainPause"),
        "actual Main lexical pause",
    );
    let expected = case.get("state");
    let name = text(case.get("name"));
    let index = p.get("index");
    assert_eq!(
        index.get("classClient"),
        obj(&[("$rpcClient", V::number(1.))])
    );
    assert_eq!(
        expected.get("productionIndex").get("clientIsMainClient"),
        V::Bool(true)
    );
    assert_eq!(
        p.get("rpcBindings"),
        V::Array(vec![V::Array(vec![
            index.get("classClient"),
            p.get("client")
        ])])
    );
    compare(
        &p.get("client"),
        &expected.get("productionIndex").get("classClient"),
        &format!("{name} exact original RPC client metadata"),
    );
    compare(
        &index.get("classParseBlock"),
        &expected.get("productionIndex").get("classParseBlock"),
        &format!("{name} actual static parse height"),
    );
    let ix = array(index.get("instances"));
    assert_eq!(ix.len(), 1);
    assert_eq!(ix[0].get("id"), V::number(1.));
    compare(
        &obj(&[
            ("client", ix[0].get("client")),
            ("parseBlock", ix[0].get("parseBlock")),
        ]),
        &expected.get("productionIndex").get("instance"),
        &format!("{name} actual index singleton"),
    );
    let mut receiver = obj(&[
        ("client", p.get("client")),
        ("txIndex", V::Promise),
        ("getBlockCountAsync", obj(&[("function", s(""))])),
        ("getNetworkInfoAsync", obj(&[("function", s(""))])),
        ("genesisBlock", p.get("genesisBlock")),
        ("parseBlock", p.get("parseBlock")),
    ]);
    if let V::Object(extra) = service.get("extraOwn") {
        for (k, v) in extra {
            receiver.insert(&k, v).unwrap();
        }
    }
    compare(
        &receiver,
        &expected.get("productionReceiver"),
        &format!("{name} complete original Main receiver"),
    );
    compare(
        &service.get("persistence"),
        &expected.get("servicePersistence"),
        &format!("{name} exact original Persistence singleton"),
    );
    compare(
        &obj(&[
            ("directories", service.get("directories")),
            ("files", service.get("files")),
        ]),
        &expected.get("serviceFileSystem"),
        &format!("{name} complete ordered recorded filesystem"),
    );
    compare(
        &p.get("io"),
        &expected.get("productionIO").get("facts"),
        &format!("{name} recorded IO facts"),
    );
    let counts = V::Object(
        array(p.get("counts"))
            .into_iter()
            .map(|p| {
                let p = array(p);
                (text(p[0].clone()), p[1].clone())
            })
            .collect(),
    );
    compare(
        &counts,
        &expected.get("productionIO").get("counts"),
        &format!("{name} exact RPC counters"),
    );
}
fn reset_service(case: &V) -> SourceContractReplay {
    let name = text(case.get("name"));
    let options = case.get("command").get("reset").get("options");
    let profile = options.get("clientProfile");
    let mut replay = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    checked_input(
        &mut replay,
        C::Invoke(call(
            "SourceActivation.configureClient",
            vec![if profile.get("chain") == V::Undefined {
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
    let files = if options.get("fileInputs") == V::Undefined {
        V::Array(vec![])
    } else {
        options.get("fileInputs")
    };
    checked_input(
        &mut replay,
        C::Invoke(call(
            "SourceMain.configureService",
            vec![obj(&[
                ("fileInputs", files),
                ("fileIOInputs", options.get("fileIOInputs")),
                (
                    "directoryInputs",
                    if options.get("directoryInputs") == V::Undefined {
                        V::Array(vec![])
                    } else {
                        options.get("directoryInputs")
                    },
                ),
            ])],
        )),
        &name,
    );
    replay
}

use crate::source_compact_index_v2::SourceCompactIndex;
use crate::source_main_step_kernel::{
    MainStepCommandV1 as Step, MainStepKernelV1, MainStepSourceResult, MainStepStop,
};
use crate::storage::Namespace;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_STEP_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "file-step-{}-{}",
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

// TEST observer only: read actual native metadata for the source table view.
// No row is installed in Core or used as a dependency/financial result.
struct ObservedRuntime<'a> {
    actual: &'a ActualRuntime,
    rows: Vec<V>,
}
impl std::ops::Deref for ObservedRuntime<'_> {
    type Target = ActualRuntime;
    fn deref(&self) -> &Self::Target {
        self.actual
    }
}
impl ObservedRuntime<'_> {
    fn collection(&self, name: &str) -> &[V] {
        if name == "txIndex" {
            &self.rows
        } else {
            self.actual.collection(name)
        }
    }
    fn state_value(&self) -> Result<V, crate::validity_source::SourceRuleError> {
        let mut all = self.actual.state_value()?;
        if let V::Object(fields) = &mut all {
            if let Some((_, V::Array(tables))) = fields.iter_mut().find(|(key, _)| key == "tables")
            {
                for table in tables {
                    if table.get("name") == s("txIndex") {
                        let V::Object(fields) = table else { panic!() };
                        let (_, rows) = fields.iter_mut().find(|(key, _)| key == "rows").unwrap();
                        *rows = V::Array(self.rows.clone());
                    }
                }
            }
        }
        Ok(all)
    }
}
struct FileDriver {
    // Construction journal is private test scaffolding, never checkpointed
    // or advertised as the head of these unjournaled continuation prefixes.
    actual: SourceContractReplay,
    index: SourceCompactIndex,
    kernel: MainStepKernelV1,
    jobs: Vec<(u64, V)>,
    timers: Vec<V>,
    logical_time: V,
    _temp: Temp,
}
impl FileDriver {
    fn reset(case: &V) -> Self {
        let mut actual = reset_service(case);
        let rows = actual.runtime().collection("txIndex").to_vec();
        checked_input(
            &mut actual,
            C::Import {
                collection: "txIndex".into(),
                rows: vec![],
            },
            "move metadata to authoritative native owner",
        );
        let temp = Temp::new();
        let mut index = SourceCompactIndex::create(
            &temp.root,
            Namespace::new("LTC", false, "txIndex").unwrap(),
            "actual-file-continuation-differential",
        )
        .unwrap();
        for row in rows {
            index.import_row(&index.snapshot().unwrap(), row).unwrap();
        }
        actual
            .native_main_runtime_mut()
            .native_main_bind(obj(&[("profile", s("private-Core-test-only-actual-loan"))]))
            .unwrap();
        Self {
            actual,
            index,
            kernel: MainStepKernelV1::with_file_retries(1),
            jobs: vec![],
            timers: vec![],
            logical_time: V::number(0.),
            _temp: temp,
        }
    }
    fn observe(&self) -> ObservedRuntime<'_> {
        assert!(
            self.actual.runtime().collection("txIndex").is_empty(),
            "Core metadata mirror must remain empty"
        );
        let pin = self.index.snapshot().unwrap();
        let mut cursor = self.index.read_generation(&pin).unwrap().cursor();
        let mut rows = vec![];
        while let Some(row) = cursor.next_row().unwrap() {
            rows.push(row);
        }
        assert!(cursor.receipt().complete);
        ObservedRuntime {
            actual: self.actual.runtime(),
            rows,
        }
    }
    fn step(&mut self, case: &V) {
        let name = text(case.get("name"));
        let controller = case.get("command").get("controller");
        let kind = text(controller.get("kind"));
        let mut source_call = None;
        let command = match kind.as_str() {
            "StartSourceCall" => {
                let target = text(controller.get("target"));
                let args = decode_embedded_tag(controller.get("args"));
                let source_target = target.strip_prefix("Main.").unwrap_or(&target).to_string();
                source_call = Some((source_target, args.clone()));
                Step::Start {
                    method: target,
                    arguments: array(args),
                }
            }
            "FireNextTimer" | "FireTimer" => {
                let id = uint(controller.get("id"));
                assert_eq!(
                    self.kernel.next_timer(),
                    Some(id),
                    "{name} exact source callback order"
                );
                Step::FireTimer { id, generation: 1 }
            }
            "ClearTimer" => Step::ClearTimer {
                id: uint(controller.get("id")),
                generation: 1,
            },
            "RecordedIOFactsUpdate" => {
                Step::UpdateRecordedIo(decode_embedded_tag(controller.get("changes")))
            }
            _ => panic!("unowned source controller {kind}"),
        };
        let evaluated = self
            .actual
            .native_main_runtime_mut()
            .native_main_step_apply(&mut self.index, &mut self.kernel, command);
        assert_eq!(evaluated.storage_stop, None, "{name}");
        assert_eq!(
            evaluated.consumed_rows, 0,
            "{name}: no TL rows in source corpus"
        );
        assert!(
            evaluated.validity.is_empty(),
            "{name}: no source validity writes"
        );
        let evaluated = evaluated.evaluation.unwrap();
        assert_eq!(
            evaluated.stop,
            MainStepStop::Quiescent,
            "{name}: {evaluated:?}"
        );
        if let Some((target, args)) = source_call {
            self.jobs.push((
                evaluated.started.unwrap(),
                obj(&[
                    ("id", V::number((self.jobs.len() + 1) as f64)),
                    ("target", s(target)),
                    ("args", args),
                    ("status", s("Pending")),
                    ("value", V::Undefined),
                    ("error", V::Undefined),
                ]),
            ));
        }
        for (id, result) in &evaluated.completed {
            if let Some((_, job)) = self.jobs.iter_mut().find(|(key, _)| key == id) {
                match result {
                    MainStepSourceResult::Fulfilled(value) => {
                        job.insert("status", s("Fulfilled")).unwrap();
                        job.insert("value", value.clone()).unwrap();
                    }
                    MainStepSourceResult::Thrown(error) => {
                        job.insert("status", s("Rejected")).unwrap();
                        job.insert(
                            "error",
                            obj(&[
                                ("name", s(&error.exception.name)),
                                ("message", s(&error.exception.message)),
                            ]),
                        )
                        .unwrap();
                    }
                }
            }
        }
        for effect in &evaluated.effects {
            match text_or_empty(effect.get("kind")).as_str() {
                "timer-scheduled" => self.timers.push(obj(&[
                    ("id", effect.get("id")),
                    ("delay", effect.get("delay")),
                    ("due", effect.get("due")),
                    ("state", s("Scheduled")),
                ])),
                "timer-fired" | "timer-cancelled" => {
                    let timer = self
                        .timers
                        .iter_mut()
                        .find(|timer| timer.get("id") == effect.get("id"))
                        .unwrap();
                    if effect.get("kind") == s("timer-fired") {
                        timer.insert("state", s("Fired")).unwrap();
                        self.logical_time = effect.get("logicalTime");
                    } else {
                        timer.insert("state", s("Cancelled")).unwrap();
                    }
                }
                _ => (),
            }
        }
        compare(
            &effects(evaluated.effects),
            &case.get("effects"),
            &format!("{name} complete source IO/DB/phase/timer order"),
        );
        compare(
            &V::Array(self.jobs.iter().map(|(_, job)| job.clone()).collect()),
            &case.get("scheduler").get("jobs"),
            &format!("{name} every caller pending/completed/error"),
        );
        compare(
            &V::Array(self.timers.clone()),
            &case.get("scheduler").get("timers"),
            &format!("{name} source timer history"),
        );
        compare(
            &self.logical_time,
            &case.get("scheduler").get("logicalTime"),
            &format!("{name} logical clock"),
        );
        service_state(&self.observe(), case);
        let mut paths = std::collections::BTreeSet::new();
        self.kernel
            .visit_canonical_fields(&mut |path, value| {
                assert!(
                    paths.insert(path.to_string()),
                    "duplicate canonical path {path}"
                );
                value.validate()
            })
            .unwrap();
    }
}
fn text_or_empty(value: V) -> String {
    if let V::String(value) = value {
        value
    } else {
        String::new()
    }
}
fn uint(value: V) -> u64 {
    if let V::Number(bits) = value {
        let value = f64::from_bits(bits);
        assert!(value >= 0. && value.fract() == 0.);
        value as u64
    } else {
        panic!("numeric source timer ID")
    }
}
fn decode_embedded_tag(value: V) -> V {
    let text = crate::source_contract_values::json(&value).unwrap();
    V::from_diagnostic_json(&text).unwrap()
}
#[test]
fn unchanged_source_file_continuations_all_parent_prefixes_both_numeric_profiles() {
    let frames = cases(false);
    assert_eq!(frames, cases(true), "both genuine numeric profiles");
    assert_eq!(
        frames.len(),
        59,
        "child termination separately verified by original generator"
    );
    let mut driver = None;
    let mut calls = 0;
    let mut controls = 0;
    for case in &frames {
        if case.get("command").get("reset") != V::Undefined {
            let next = FileDriver::reset(case);
            service_state(&next.observe(), case);
            driver = Some(next);
        } else {
            if case.get("command").get("controller").get("kind") == s("StartSourceCall") {
                calls += 1;
            }
            controls += 1;
            driver.as_mut().unwrap().step(case);
        }
    }
    assert_eq!(calls, 21);
    assert!(controls > calls);
}
