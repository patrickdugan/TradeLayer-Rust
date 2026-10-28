//! Genuine unchanged raw10/AMMPool evidence under the future explicit policy.
//! The existing Core is the only DB, registry cache and spendable Tally owner.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
};
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
fn runtime() -> SourceContractRuntime {
    SourceContractRuntime::with_policy(P::AmmLifecycleV11)
}
fn records(lock: bool) -> Vec<V> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage = std::env::var("TL_AMM_EVIDENCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.to_path_buf());
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", root)
        .arg(stage.join("scripts/lower_amm_vectors.cjs"))
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
fn value(value: DependencyReturn) -> V {
    match value {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v,
        other => panic!("{other:?}"),
    }
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
fn setup(r: &mut SourceContractRuntime, case: &V) {
    for command in setup_commands(case) {
        match command {
            C::Invoke(call) => {
                value(r.invoke_dependency(&call).unwrap());
            }
            C::Import { collection, rows } => r.import_collection(&collection, rows).unwrap(),
            _ => unreachable!(),
        }
    }
    r.take_effects();
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
fn actual(r: &mut SourceContractRuntime, c: &V) -> V {
    if let Some(f) = failure(c) {
        r.fail_storage(f).unwrap();
    }
    match command(c) {
        C::ExecuteContract(args) => {
            let outcome = match r
                .execute_contract_unverified(&args)
                .unwrap_or_else(|e| panic!("{args:?}: {e:?}"))
            {
                SourceOutcome::Returned(v) => v,
                SourceOutcome::Thrown(e) => obj(&[
                    (
                        "params",
                        r.last_contract_parameters()
                            .cloned()
                            .unwrap_or(V::Undefined),
                    ),
                    (
                        "thrown",
                        obj(&[("name", s(e.name)), ("message", s(e.message))]),
                    ),
                ]),
            };
            obj(&[("outcome", outcome)])
        }
        C::Invoke(call) => projected(
            r.invoke_dependency(&call)
                .unwrap_or_else(|e| panic!("{call:?}: {e:?}")),
        ),
        _ => unreachable!(),
    }
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
                if target.starts_with("AMM.") {
                    let handle = arguments.remove(0);
                    fields.push(("instance", handle.get("sourceAmmInstance")));
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
#[test]
fn actual_raw10_and_same_owner_capital_source_prefixes_both_numeric_versions() {
    let records = records(false);
    assert_eq!(records, self::records(true));
    let (mut r, mut resets, mut calls) = (runtime(), 0, 0);
    for case in &records {
        let c = case.get("command");
        let name = text(case.get("name"));
        if c.get("reset") != V::Undefined {
            resets += 1;
            r = runtime();
            setup(&mut r, case);
        } else {
            calls += 1;
            compare(
                &actual(&mut r, &c),
                &case.get("result"),
                &format!("{name} exact transport/outcome"),
            );
        }
        state(&r, case);
        compare(
            &effects(r.take_effects()),
            &case.get("effects"),
            &format!("{name} full DB and observed source boundaries"),
        );
    }
    assert_eq!((records.len(), resets, calls), (40, 8, 32));
}
#[test]
fn amm_failure_prefixes_checkpoint_restore_and_retry_without_a_shadow_ledger() {
    let records = records(false);
    let mut scenes = 0;
    for (start, reset) in records
        .iter()
        .enumerate()
        .filter(|(_, case)| case.get("command").get("reset") != V::Undefined)
    {
        scenes += 1;
        let mut replay = SourceContractReplay::with_policy(P::AmmLifecycleV11).unwrap();
        for command in setup_commands(reset) {
            replay.apply(command).unwrap();
        }
        for case in records
            .iter()
            .skip(start + 1)
            .take_while(|case| case.get("command").get("reset") == V::Undefined)
        {
            let c = case.get("command");
            if let Some(f) = failure(&c) {
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay.apply(command(&c)).unwrap();
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "{}: {:?}",
                text(case.get("name")),
                receipt.outcome
            );
            state(replay.runtime(), case);
            compare(
                &effects(receipt.effects),
                &case.get("effects"),
                &text(case.get("name")),
            );
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(&bytes[..5], b"TLCR\x0b");
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(restored.runtime(), replay.runtime());
            replay = restored;
        }
    }
    assert_eq!(scenes, 8);
}
#[test]
fn previous_policies_do_not_admit_raw10_or_the_new_class_cache_commands() {
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
        let mut r = SourceContractRuntime::with_policy(policy);
        let before = r.clone();
        assert!(r
            .invoke_dependency(&dep(
                "SourceAMM.construct",
                vec![n(0.), n(100.), n(10.), n(1.)]
            ))
            .is_err());
        assert_eq!(r, before);
        let args = vec![
            s("not-admitted"),
            n(10.),
            s("tl"),
            s("0,1,1,2,7,3,,,0,"),
            s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT"),
            V::Null,
            n(0.),
            n(0.),
            n(10.),
        ];
        assert!(r.execute_contract_unverified(&args).is_err());
        assert_eq!(r, before);
    }
}
