// Complete original Main owner-prefix observations. Rust execution pending Root.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::source_rich_value::{
    self as rich, Atom, Graph, Intrinsic, Key, Kind, Limits, Object, Property, PropertyValue, Realm,
};
use tradelayer_core::validity_source::SourceValue as V;
const OBSERVER: &str = include_str!("../scripts/rich_owner_prefix_observer.cjs");
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

fn cases(lock: bool) -> Vec<V> {
    let evidence = std::env::var("TL_RICH_OWNER_EVIDENCE_ROOT")
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
    assert_eq!(all.get("runtimePolicy"), s("RichThrownV14"));
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
    assert_eq!(
        p.get("io"),
        obj(&[]),
        "legacy IO holds no duplicate raw error/IO owner"
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

fn u(v: &str) -> Vec<u16> {
    v.encode_utf16().collect()
}
fn data(name: &str, value: Atom) -> Property {
    Property {
        key: Key::Utf16(u(name)),
        enumerable: true,
        configurable: true,
        value: PropertyValue::Data {
            value,
            writable: true,
        },
    }
}
fn client(graph: &mut Graph) -> Atom {
    let id = graph.objects.len() as u32;
    graph.objects.push(Object {
        kind: Kind::Ordinary,
        prototype: Atom::Intrinsic(Intrinsic {
            realm: Realm::HostIo,
            name: u("Object.prototype"),
        }),
        properties: vec![
            data("chain", Atom::Utf16(u("LTC"))),
            data("test", Atom::Boolean(false)),
        ],
    });
    Atom::Reference(id)
}
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap(), 16).unwrap())
        .collect()
}
fn wire(hex: &str) -> Graph {
    rich::wire::decode_external(
        &bytes(hex),
        &rich::intrinsics::capabilities(),
        Limits::default(),
    )
    .unwrap()
    .graph()
    .clone()
}
fn invoke(
    replay: &mut SourceContractReplay,
    target: &str,
    args: Graph,
) -> tradelayer_core::source_contract_replay::SourceContractReceipt {
    replay
        .apply(C::InvokeRichV1(rich::protocol::InvokeRichV1 {
            target: target.into(),
            arguments: args,
        }))
        .unwrap()
}
fn empty() -> Graph {
    Graph {
        roots: vec![],
        objects: vec![],
        symbols: vec![],
    }
}
fn effects(runtime: &SourceContractRuntime, values: Vec<V>) -> V {
    let snapshots = runtime.rich_effect_snapshots().unwrap();
    V::Array(
        values
            .into_iter()
            .map(|value| match text(value.get("kind")).as_str() {
                "source-log" => obj(&[
                    ("kind", s("source-log")),
                    ("level", value.get("level")),
                    (
                        "wire",
                        s(
                            snapshots[text(value.get("richSnapshot")).parse::<usize>().unwrap()]
                                .iter()
                                .map(|x| format!("{x:02x}"))
                                .collect::<String>(),
                        ),
                    ),
                ]),
                "rpc-input" => obj(&[
                    ("kind", s("rpc-input")),
                    ("method", value.get("method")),
                    ("ordinal", value.get("ordinal")),
                    ("transport", value.get("transport")),
                    (
                        "wire",
                        s(
                            snapshots[text(value.get("richSnapshot")).parse::<usize>().unwrap()]
                                .iter()
                                .map(|x| format!("{x:02x}"))
                                .collect::<String>(),
                        ),
                    ),
                ]),
                "database" | "db" => value,
                other => panic!("unexpected effect {other}: {value:?}"),
            })
            .collect(),
    )
}
#[test]
fn rich_main_same_owner_partial_writes_raw_rethrow_and_swallowed_rejection() {
    let source = cases(false);
    assert_eq!(
        source,
        cases(true),
        "complete both-numeric original outcomes/state/effects"
    );
    assert_eq!(source.len(), 20);
    let rows = include_str!("../fixtures/rich_owner_prefix.tsv")
        .lines()
        .map(|x| x.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 10);
    for (pair, wire_row) in source.chunks_exact(2).zip(rows) {
        let (reset, call) = (&pair[0], &pair[1]);
        let name = text(call.get("name"));
        assert_eq!(name, wire_row[0]);
        let mut replay = SourceContractReplay::with_policy(P::RichThrownV14).unwrap();
        replay.apply(C::SetHeight(V::number(12.))).unwrap();
        for input in imports(reset) {
            let receipt = replay.apply(input).unwrap();
            assert!(!matches!(receipt.outcome, O::LocalError(_)));
        }
        let mut inputs = empty();
        let metadata = client(&mut inputs);
        inputs.roots = vec![metadata, Atom::Utf16(u("LTC"))];
        assert!(matches!(
            invoke(&mut replay, "SourceMain.configureRichOwnerInputs", inputs).outcome,
            O::RichDependencyV1(_)
        ));
        let mut config = wire(wire_row[1]);
        let facts = config.roots[0].clone();
        let metadata = client(&mut config);
        config.roots = vec![metadata, facts, Atom::Binary64(10f64.to_bits())];
        assert!(matches!(
            invoke(&mut replay, "SourceMain.configureRichIO", config).outcome,
            O::RichDependencyV1(_)
        ));
        production_state(replay.runtime(), reset);
        let command = call.get("command");
        let mut arguments = empty();
        arguments.roots = array(command.get("args"))
            .into_iter()
            .map(|v| match v {
                V::Number(x) => Atom::Binary64(x),
                other => panic!("non-numeric input {other:?}"),
            })
            .collect();
        let receipt = invoke(&mut replay, &text(command.get("target")), arguments);
        let O::RichDependencyV1(result) = receipt.outcome else {
            panic!("{name}: {:?}", receipt.outcome)
        };
        compare(
            &effects(replay.runtime(), receipt.effects),
            &call.get("effects"),
            &format!("{name} all ordered DB/rawRPC/rawlog effects"),
        );
        assert_eq!(
            rich::wire::encode(&result.graph).unwrap(),
            bytes(wire_row[2]),
            "{name} complete raw return/rethrow graph and identity"
        );
        assert_eq!(
            result.transport,
            if wire_row[3] == "PendingThrow" {
                rich::protocol::Transport::PendingThrow
            } else {
                rich::protocol::Transport::Pending
            },
            "{name} original Promise outcome"
        );
        production_state(replay.runtime(), call);
        let checkpoint = replay.checkpoint().unwrap();
        let restored = SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
        assert_eq!(
            restored, replay,
            "{name} same actual index/Core owner replay"
        );
        assert_eq!(
            restored.runtime().rich_state_material().unwrap(),
            replay.runtime().rich_state_material().unwrap()
        );
    }
}
