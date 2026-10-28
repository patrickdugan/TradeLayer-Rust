//! Genuine unchanged raw10/AMMPool evidence under the future explicit policy.
//! The existing Core is the only DB, registry cache and spendable Tally owner.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
fn s(value: impl Into<String>) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
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
fn dep(target: &str, arguments: Vec<V>) -> DependencyCall {
    DependencyCall {
        target: target.into(),
        arguments,
    }
}
fn records(lock: bool) -> Vec<V> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage = std::env::var("TL_AMM_QUOTE_EVIDENCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.to_path_buf());
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", root)
        .arg(stage.join("scripts/amm_quote_vectors.cjs"))
        .args([
            "--check",
            "--print",
            if lock { "--lock" } else { "--baseline" },
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::str::from_utf8(&output.stdout)
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
fn setup_commands(case: &V) -> Vec<C> {
    let options = case.get("command").get("reset");
    let mut commands = vec![C::Invoke(dep(
        "SourceMain.configure",
        vec![obj(&[("activationProfile", s("source-registry"))])],
    ))];
    commands.extend(
        array(case.get("state").get("tables"))
            .into_iter()
            .map(|table| C::Import {
                collection: text(table.get("name")),
                rows: array(table.get("rows")),
            }),
    );
    if options.get("propertyRegistry") == V::Bool(true) {
        commands.push(C::Invoke(dep("SourceGlobal.bindPropertyRegistry", vec![])));
    }
    if options.get("propertyPool") == V::Bool(true) {
        commands.push(C::Invoke(dep(
            "SourceAMM.construct",
            vec![n(0.), n(100.), n(10.), n(0.)],
        )));
        commands.push(C::Invoke(dep(
            "SourceAMM.attachPropertyPool",
            vec![n(1.), n(7.), obj(&[("sourceAmmInstance", n(1.))])],
        )));
    }
    if let V::Array(input) = options.get("poolInput") {
        commands.push(C::Invoke(dep("SourceAMM.construct", input)));
    }
    commands
}
fn failure(c: &V) -> Option<StorageFailure> {
    let f = c.get("failure");
    if matches!(f, V::Undefined | V::Null) {
        return None;
    }
    let operation = text(f.get("op"));
    Some(StorageFailure {
        collection: text(f.get("collection")),
        operation: if operation == "getDatabase" {
            operation
        } else {
            format!("{operation}Async")
        },
        skip: match f.get("skip") {
            V::Number(bits) => f64::from_bits(bits) as usize,
            _ => 0,
        },
        message: text(f.get("message")),
    })
}
fn command(c: &V) -> C {
    if c.get("quote") != V::Undefined {
        let q = c.get("quote");
        let mut arguments = vec![obj(&[("sourceAmmInstance", q.get("id"))])];
        arguments.extend(array(q.get("args")));
        return C::Invoke(dep("AMM.quoteContractOrders", arguments));
    }
    if let V::Array(raw) = c.get("raw") {
        return C::ExecuteContract(raw);
    }
    let p = c.get("pool");
    if p != V::Undefined {
        let mut arguments = vec![obj(&[("sourceAmmInstance", p.get("id"))])];
        arguments.extend(array(p.get("args")));
        return C::Invoke(dep(&format!("AMM.{}", text(p.get("method"))), arguments));
    }
    C::Invoke(dep(&text(c.get("target")), array(c.get("args"))))
}
const OBSERVED: &[&str] = &[
    "Activation.isTxTypeActive",
    "Logic.AMMPool",
    "Property.getAMM",
    "Contract.getAMM",
    "Contract.getContractInfo",
    "Tally.updateBalance",
    "AMM.addCapital",
    "AMM.redeemCapital",
    "AMM.quoteContractOrders",
    "AMMStatic.calculateOrderSize",
    "AMMStatic.calculateRedemptionValue",
    "AMMStatic.generateOrders",
    "AMMStatic.generateTokenOrders",
    "AMMStatic.updateOrdersForAllContractAMMs",
    "Orderbook.loadOrderBook",
    "Orderbook.cancelOrdersByCriteria",
    "Orderbook.insertOrder",
    "Orderbook.saveOrderBook",
];
fn effects(values: Vec<V>) -> V {
    V::Array(
        values
            .into_iter()
            .filter_map(|event| {
                if [
                    s("database"),
                    s("db"),
                    s("uuid"),
                    s("random"),
                    s("rpc-clock"),
                ]
                .contains(&event.get("kind"))
                {
                    return Some(event);
                }
                if event.get("kind") != s("dependency") {
                    return None;
                }
                let V::String(target) = event.get("target") else {
                    return None;
                };
                if !OBSERVED.contains(&target.as_str()) {
                    return None;
                }
                let mut arguments = array(event.get("args"));
                let mut fields = vec![("kind", s("source-function")), ("target", s(&target))];
                if target.starts_with("AMM.") || target.starts_with("Orderbook.") {
                    let handle = arguments.remove(0);
                    fields.push((
                        "instance",
                        if target.starts_with("AMM.") {
                            handle.get("sourceAmmInstance")
                        } else {
                            handle.get("orderbookInstance")
                        },
                    ));
                }
                fields.push(("args", V::Array(arguments)));
                if let V::Object(own) = &event {
                    for (key, v) in own {
                        if key == "returned" || key == "thrown" {
                            fields.push((key.as_str(), v.clone()));
                        }
                    }
                }
                Some(obj(&fields))
            })
            .collect(),
    )
}
fn state(r: &SourceContractRuntime, case: &V) {
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = r.state_value().unwrap();
    assert_eq!(all.get("runtimePolicy"), s("AmmLifecycleV11"));
    let mut known = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let key = text(table.get("name"));
        known.insert(key.clone());
        compare(
            &V::Array(r.collection(&key).to_vec()),
            &table.get("rows"),
            &format!("{name} full {key}"),
        );
    }
    for table in array(all.get("tables")) {
        if !known.contains(&text(table.get("name"))) {
            compare(
                &table.get("rows"),
                &V::Array(vec![]),
                &format!("{name} no invented table rows"),
            );
        }
    }
    let global = all.get("globalClearing");
    let amm = all.get("ammLifecycle");
    for (actual, wanted, label) in [
        (r.tally_cache(), expected.get("tally"), "sole static Tally"),
        (
            global.get("tallyStaticInitialized"),
            expected.get("tallyInitialized"),
            "static initialized",
        ),
        (global.get("tallyMod"), expected.get("tallyMod"), "mod flag"),
        (
            all.get("sendCommit").get("tallyInstance"),
            expected.get("tallyInstance"),
            "distinct genuine singleton",
        ),
        (
            global.get("propertyCache"),
            expected.get("propertyCache"),
            "single Property index",
        ),
        (
            all.get("proofLifecycle").get("propertyInitialized"),
            expected.get("propertyInitialized"),
            "Property initialized",
        ),
        (
            all.get("registryLifecycle").get("registryInitialized"),
            expected.get("contractInitialized"),
            "Contract initialized",
        ),
        (
            global.get("contractSeries"),
            expected.get("contractSeries"),
            "Contract series cache",
        ),
        (
            amm.get("instances"),
            expected.get("ammInstances"),
            "every actual AMMPool receiver",
        ),
        (
            amm.get("propertyAmms"),
            expected.get("propertyAmms"),
            "actual Property ammIndex and aliases",
        ),
        (
            amm.get("propertyRegistryBound"),
            expected.get("propertyRegistryBound"),
            "realm Registry binding",
        ),
        (
            all.get("mainCallable").get("activation"),
            expected.get("activation"),
            "genuine activation registry instance",
        ),
        (
            r.last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined),
            expected.get("lastParams"),
            "raw params",
        ),
    ] {
        compare(&actual, &wanted, &format!("{name} {label}"));
    }
    assert_eq!(expected.get("unsupported"), V::Array(vec![]));
}

fn normalized(value: V) -> V {
    match value {
        V::BigNumber(value) => V::BigNumber(
            value
                .parse::<tradelayer_core::decimal::Decimal>()
                .map(|value| value.to_string())
                .unwrap_or(value),
        ),
        V::Array(values) => V::Array(values.into_iter().map(normalized).collect()),
        V::Object(values) => V::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalized(value)))
                .collect(),
        ),
        other => other,
    }
}
fn quote_state(runtime: &SourceContractRuntime, case: &V) {
    state(runtime, case);
    let name = text(case.get("name"));
    let books = array(case.get("state").get("books"));
    assert_eq!(
        array(runtime.queue().state_value().get("instances")).len(),
        books.len(),
        "{name} every actual book receiver"
    );
    for book in books {
        let V::Number(id) = book.get("id") else {
            panic!("book id")
        };
        let V::Object(fields) = book else {
            panic!("book own fields")
        };
        let wanted = V::Object(fields.into_iter().filter(|(key, _)| key != "id").collect());
        compare(
            &normalized(
                runtime
                    .queue()
                    .instance_value(f64::from_bits(id) as u64)
                    .unwrap(),
            ),
            &normalized(wanted),
            &format!("{name} actual same-owner book alias"),
        );
    }
}
#[test]
fn source_v11_quotes_registry_coordinator_failure_prefixes_and_replay() {
    let records = records(false);
    assert_eq!(records, self::records(true));
    let mut replay = SourceContractReplay::with_policy(P::AmmLifecycleV11).unwrap();
    let (mut resets, mut calls) = (0, 0);
    for case in &records {
        let c = case.get("command");
        let name = text(case.get("name"));
        if c.get("reset") != V::Undefined {
            resets += 1;
            replay = SourceContractReplay::with_policy(P::AmmLifecycleV11).unwrap();
            for input in setup_commands(case) {
                replay.apply(input).unwrap();
            }
        } else {
            calls += 1;
            if let Some(f) = failure(&c) {
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay.apply(command(&c)).unwrap();
            let value = match receipt.outcome {
                O::Dependency(value) => value,
                other => panic!("{name}: {other:?}"),
            };
            compare(
                &projected(value),
                &case.get("result"),
                &format!("{name} source transport/outcome"),
            );
            compare(
                &effects(receipt.effects),
                &case.get("effects"),
                &format!("{name} exact ordered source/DB effects"),
            );
        }
        quote_state(replay.runtime(), case);
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(&bytes[..5], b"TLCR\x0b");
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), bytes);
        replay = restored;
    }
    assert_eq!((records.len(), resets, calls), (49, 11, 38));
}
#[test]
fn quote_static_helpers_do_not_change_earlier_policy_admission() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
        P::MainCallableV8,
        P::SpotLifecycleV9,
        P::ActivationLifecycleV10,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        assert!(runtime
            .invoke_dependency(&dep(
                "AMMStatic.calculateOrderSize",
                vec![V::number(0.001), V::number(0.1), V::number(100.)]
            ))
            .is_err());
        assert_eq!(runtime, before);
    }
}
