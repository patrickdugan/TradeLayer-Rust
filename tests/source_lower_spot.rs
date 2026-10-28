//! Future explicit spot policy. The parent owns policy/replay promotion.
//! Original JS executes first; inputs are full documents, never supplied verdicts.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
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
    let V::String(v) = v else {
        panic!("expected string: {v:?}")
    };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("expected array: {v:?}")
    };
    v
}
fn dep(target: &str, arguments: Vec<V>) -> DependencyCall {
    DependencyCall {
        target: target.into(),
        arguments,
    }
}
fn runtime() -> SourceContractRuntime {
    SourceContractRuntime::with_policy(P::SpotLifecycleV9)
}
fn records(lock: bool) -> Vec<V> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage = std::env::var("TL_SPOT_EVIDENCE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.to_path_buf());
    let output = std::process::Command::new("node")
        .env("SPOT_REPO", root)
        .arg(stage.join("scripts/lower_spot_vectors.cjs"))
        .args([
            "--stdout",
            "--check",
            "--all-receivers",
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
fn command(c: &V) -> C {
    if c.get("import") != V::Undefined {
        let i = c.get("import");
        C::Import {
            collection: text(i.get("collection")),
            rows: array(i.get("rows")),
        }
    } else if let V::Array(args) = c.get("raw") {
        C::ExecuteContract(args)
    } else {
        C::Invoke(dep(&text(c.get("target")), array(c.get("args"))))
    }
}
fn actual(r: &mut SourceContractRuntime, c: &V) -> V {
    if c.get("import") != V::Undefined {
        let i = c.get("import");
        r.import_collection(&text(i.get("collection")), array(i.get("rows")))
            .unwrap();
        obj(&[("outcome", obj(&[("returned", V::Undefined)]))])
    } else if c.get("instance") != V::Undefined {
        let i = c.get("instance");
        let receiver = match r
            .invoke_dependency(&dep("Orderbook.newInstance", vec![i.get("key")]))
            .unwrap()
        {
            DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v,
            other => panic!("construct receiver: {other:?}"),
        };
        let mut args = vec![receiver];
        args.extend(array(i.get("args")));
        projected(
            r.invoke_dependency(&dep(&format!("Orderbook.{}", text(i.get("method"))), args))
                .unwrap(),
        )
    } else if let V::Array(args) = c.get("raw") {
        let outcome = match r
            .execute_contract_unverified(&args)
            .unwrap_or_else(|e| panic!("raw invocation {args:?} produced local error {e:?}"))
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
    } else {
        projected(
            r.invoke_dependency(&dep(&text(c.get("target")), array(c.get("args"))))
                .unwrap(),
        )
    }
}
fn setup_commands(case: &V) -> Vec<C> {
    let mut commands = vec![
        C::Invoke(dep(
            "SourceMain.configure",
            vec![obj(&[("activationProfile", s("declared-answer"))])],
        )),
        C::Invoke(dep("SourceGlobal.bindSpotRpcDocuments", vec![])),
    ];
    if case.get("command").get("reset").get("bindTally") == V::Bool(true) {
        commands.push(C::Invoke(dep("SourceGlobal.bindTally", vec![])));
    }
    commands.extend(
        array(case.get("state").get("tables"))
            .into_iter()
            .map(|t| C::Import {
                collection: text(t.get("name")),
                rows: array(t.get("rows")),
            }),
    );
    commands
}
fn setup(r: &mut SourceContractRuntime, case: &V) {
    for command in setup_commands(case) {
        match command {
            C::Invoke(d) => {
                r.invoke_dependency(&d).unwrap();
            }
            C::Import { collection, rows } => r.import_collection(&collection, rows).unwrap(),
            _ => unreachable!(),
        }
    }
    r.take_effects();
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
fn state(r: &SourceContractRuntime, case: &V) {
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = r.state_value().unwrap();
    assert_eq!(all.get("runtimePolicy"), s("SpotLifecycleV9"));
    let g = all.get("globalClearing");
    let mut known = std::collections::BTreeSet::new();
    for t in array(expected.get("tables")) {
        let key = text(t.get("name"));
        known.insert(key.clone());
        compare(
            &V::Array(r.collection(&key).to_vec()),
            &t.get("rows"),
            &format!("{name} full {key}"),
        );
    }
    for t in array(all.get("tables")) {
        if !known.contains(&text(t.get("name"))) {
            compare(
                &t.get("rows"),
                &V::Array(vec![]),
                &format!("{name} no invented rows"),
            );
        }
    }
    for (actual, wanted, label) in [
        (r.tally_cache(), expected.get("tally"), "sole Tally"),
        (
            g.get("tallyStaticInitialized"),
            expected.get("tallyInitialized"),
            "static initialization",
        ),
        (g.get("tallyMod"), expected.get("tallyMod"), "mod flag"),
        (
            g.get("propertyCache"),
            expected.get("propertyCache"),
            "sole Property cache",
        ),
        (
            all.get("proofLifecycle").get("propertyInitialized"),
            expected.get("propertyInitialized"),
            "property initialization",
        ),
        (
            all.get("sendCommit").get("tallyInstance"),
            expected.get("tallyInstance"),
            "actual instance cache",
        ),
        (
            g.get("channels").get("registry"),
            expected.get("channels"),
            "channels unchanged",
        ),
        (
            g.get("channels").get("pendingWithdrawals"),
            expected.get("withdrawals"),
            "withdrawals unchanged",
        ),
        (
            g.get("unhandledRejections"),
            expected.get("unhandled"),
            "unhandled errors",
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
    let books = V::Array(
        array(all.get("queue").get("instances"))
            .into_iter()
            .map(|entry| {
                let entry = array(entry);
                let mut fields = vec![("id".to_string(), entry[0].clone())];
                let V::Object(values) = entry[1].clone() else {
                    panic!("instance fields")
                };
                for (key, value) in values {
                    fields.push((
                        key.clone(),
                        if key == "tickSize" {
                            assert_eq!(value, V::BigNumber("0.00000001".into()));
                            V::BigNumber("1e-8".into())
                        } else {
                            value
                        },
                    ));
                }
                V::Object(fields)
            })
            .collect(),
    );
    compare(
        &books,
        &expected.get("orderbookInstances"),
        &format!("{name} every live receiver/book alias"),
    );
    if expected.get("boundTally") != V::Undefined {
        compare(
            &V::Bool(all.get("globals").get("tallyMap") != V::Undefined),
            &expected.get("boundTally"),
            &format!("{name} actual realm binding"),
        );
        let cl = expected.get("clearing");
        for (key, wanted) in [
            ("blockTrades", "blockTrades"),
            ("oracleMarks", "latestOracleMarkById"),
            ("nativeMarks", "latestNativeMarkById"),
        ] {
            compare(
                &all.get(key),
                &cl.get(wanted),
                &format!("{name} clearing {key}"),
            );
        }
        for (key, wanted) in [
            ("deleverageTrades", "deleverageTrades"),
            ("liquidationRecords", "liquidationRecords"),
        ] {
            compare(
                &g.get(key),
                &cl.get(wanted),
                &format!("{name} clearing {key}"),
            );
        }
        if expected.get("feeAccrualBuffer") != V::Undefined {
            compare(
                &all.get("feeBuffer"),
                &expected.get("feeAccrualBuffer"),
                &format!("{name} actual fee buffer"),
            );
        }
        assert_eq!(
            expected.get("feeCache"),
            V::Undefined,
            "this corpus uses actual persisted fee rows, not a fabricated static cache"
        );
    }
}
const OBSERVED_INSTANCE: &[&str] = &[
    "cancelAllTokenOrders",
    "cancelTokenOrderByTxid",
    "cancelTokenBuyOrdersByPrice",
    "cancelTokenSellOrdersByPrice",
    "cancelAllContractOrders",
    "cancelContractOrderByTxid",
    "cancelOrdersByCriteria",
    "saveOrderBook",
    "getOrdersForAddress",
    "addTokenOrder",
    "matchTokenOrders",
    "processTokenMatches",
];
// The borrowed queue owner has no independent load-method observer. Its full
// ordered factory/read/write traces remain compared; no DB effects are omitted.
fn reference_effects(v: V) -> V {
    V::Array(
        array(v)
            .into_iter()
            .filter(|e| e.get("target") != s("Orderbook.instance.loadOrderBook"))
            .collect(),
    )
}
fn effects(values: Vec<V>) -> V {
    V::Array(values.into_iter().filter_map(|v|{
    let kind=v.get("kind");
    if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","rpc"].contains(&k.as_str())){return Some(v);}
    if kind!=s("dependency"){return None;}
    let V::String(target)=v.get("target")else{return None;};
    if target.starts_with("Activation."){return Some(obj(&[("kind",s("activation")),("method",s(target.strip_prefix("Activation.").unwrap())),("args",v.get("args")),("returned",v.get("returned"))]));}
    if ["Logic.onChainTokenToToken","Logic.cancelOrder","TxUtils.getRawTransaction","TxUtils.getBlockHeight","Orderbook.queueOnChainTokenOrder"].contains(&target.as_str()){
        return Some(obj(&[("kind",s("source-function")),("target",s(target)),("args",v.get("args"))]));
    }
    if let Some(method)=target.strip_prefix("Orderbook."){
        if OBSERVED_INSTANCE.contains(&method){let args=array(v.get("args"));let receiver=args.first()?.get("orderbookInstance");if receiver==V::Undefined{return None;}return Some(obj(&[("kind",s("source-function")),("target",s(format!("Orderbook.instance.{method}"))),("instance",receiver),("args",V::Array(args[1..].to_vec()))]));}
    }None
}).collect())
}
#[test]
fn actual_raw5_raw6_and_literal_queue_financial_prefixes_both_numeric_versions() {
    let records = records(false);
    assert_eq!(records, self::records(true));
    let (mut resets, mut imports, mut raw, mut direct) = (0, 0, 0, 0);
    let mut r = runtime();
    for case in &records {
        let c = case.get("command");
        let name = text(case.get("name"));
        if c.get("reset") != V::Undefined {
            resets += 1;
            r = runtime();
            setup(&mut r, case);
        } else {
            if c.get("import") != V::Undefined {
                imports += 1;
            } else if c.get("raw") != V::Undefined {
                raw += 1;
            } else {
                direct += 1;
            }
            compare(
                &actual(&mut r, &c),
                &case.get("result"),
                &format!("{name} exact transport/outcome"),
            );
        }
        state(&r, case);
        compare(
            &effects(r.take_effects()),
            &reference_effects(case.get("effects")),
            &format!("{name} ordered complete DB/RPC and observed boundaries"),
        );
    }
    assert_eq!(
        (records.len(), resets, imports, raw + direct),
        (64, 11, 4, 49)
    );
}
#[test]
fn source_failures_and_successful_enriched_legacy_orders_restore_then_continue() {
    let records = records(false);
    let mut scenes = 0;
    for (start, case) in records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.get("command").get("reset") != V::Undefined)
    {
        scenes += 1;
        let mut replay = SourceContractReplay::with_policy(P::SpotLifecycleV9).unwrap();
        for c in setup_commands(case) {
            replay.apply(c).unwrap();
        }
        for case in records[start + 1..]
            .iter()
            .take_while(|r| r.get("command").get("reset") == V::Undefined)
        {
            let c = case.get("command");
            let receipt = if c.get("instance") != V::Undefined {
                let i = c.get("instance");
                let created = replay
                    .apply(C::Invoke(dep("Orderbook.newInstance", vec![i.get("key")])))
                    .unwrap();
                let receiver = match created.outcome {
                    O::Dependency(
                        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v),
                    ) => v,
                    other => panic!("construct replay receiver: {other:?}"),
                };
                let mut args = vec![receiver];
                args.extend(array(i.get("args")));
                replay
                    .apply(C::Invoke(dep(
                        &format!("Orderbook.{}", text(i.get("method"))),
                        args,
                    )))
                    .unwrap()
            } else {
                replay.apply(command(&c)).unwrap()
            };
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "{} actual receipt {:?}",
                text(case.get("name")),
                receipt.outcome
            );
            state(replay.runtime(), case);
            let checkpoint = replay.checkpoint().unwrap();
            assert_eq!(&checkpoint[..5], b"TLCR\x09");
            let restored =
                SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
            assert_eq!(restored, replay);
            assert_eq!(restored.checkpoint().unwrap(), checkpoint);
            replay = restored;
        }
    }
    assert_eq!(scenes, 11);
}
#[test]
fn prior_policies_do_not_silently_acquire_literal_spot_mutation() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
        P::MainCallableV8,
    ] {
        let mut r = SourceContractRuntime::with_policy(policy);
        for target in [
            "SourceGlobal.bindSpotRpcDocuments",
            "Logic.onChainTokenToToken",
            "Logic.cancelOrder",
            "Orderbook.addTokenOrder",
            "Orderbook.matchTokenOrders",
            "Orderbook.cancelAllTokenOrders",
        ] {
            let before = r.clone();
            assert!(matches!(
                r.invoke_dependency(&dep(target, vec![])),
                Err(SourceRuleError::UnknownDependency | SourceRuleError::UnknownMethod)
            ));
            assert_eq!(r, before);
        }
        for kind in [5., 6.] {
            let before = r.clone();
            assert_eq!(
                r.execute_contract_unverified(&[
                    s("old"),
                    V::number(kind),
                    s("tl"),
                    s(""),
                    s("sender"),
                    V::Null,
                    V::number(0.),
                    V::number(0.),
                    V::number(10.)
                ]),
                Err(SourceRuleError::InvalidInput)
            );
            assert_eq!(r, before);
        }
    }
}
