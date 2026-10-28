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
const OBSERVER: &str = r#"
const path=require('node:path'),cp=require('node:child_process');
const root=process.env.PROOF_REPO,stage=process.env.TL_MAIN_EVIDENCE_ROOT||root;
const args=[path.join(stage,'scripts/main_phase_vectors.cjs'),'--print','--check'];if(process.argv.includes('--lock'))args.push('--lock');
const raw=cp.execFileSync(process.execPath,args,{env:{...process.env,PROOF_REPO:root},maxBuffer:16*1024*1024,encoding:'utf8'});
const U=['undefined'],A=x=>['array',x],S=x=>['string',x],B=x=>['boolean',x],O=x=>['object',Object.entries(x)];
function get(v,key){return v?.[0]==='object'?(v[1].find(([k])=>k===key)?.[1]||U):U;}
function fallback(v,x){return v[0]==='undefined'?x:v;}
function norm(v){const t=v[0];if(t==='map')return A(v[1].map(([k,x])=>A([norm(k),norm(x)])));if(t==='set')return A(v[1].map(norm));if(t==='array')return A(v[1].map(norm));if(t==='object')return ['object',v[1].map(([k,x])=>[k,norm(x)])];if(t==='regexp'){return v[1]==='^tx'&&v[2]===''?S('^tx'):O({'$regexp':S(v[1]),'$flags':S(v[2])});}if(t==='function'||t==='cycle')throw Error('unadmitted identity transport');return v;}
for(const line of raw.trim().split('\n')){const r=JSON.parse(line),c=r.command;
 if(get(c,'profile')[1]==='full-import')continue;
 const st=r.state,src=get(st,'source'),t=get(st,'tally'),p=get(st,'property'),k=get(st,'contract'),ki=get(k,'instance'),o=get(src,'oracle'),oi=get(o,'instance'),cl=get(src,'clearing'),ch=get(st,'channels');
 const state=O({tables:get(src,'tables'),books:get(src,'instances'),margins:get(st,'marginInstances'),tally:fallback(get(t,'addresses'),A([])),tallyInitialized:get(t,'initialized'),tallyMod:get(t,'modFlag'),contractMod:get(k,'modFlag'),propertyCache:fallback(get(p,'propertyIndex'),A([])),propertyInitialized:B(p[0]!=='undefined'),contractSeries:fallback(get(ki,'contractSeries'),A([])),registryInitialized:B(ki[0]!=='undefined'),seriesInitialized:B(get(ki,'contractSeries')[0]!=='undefined'),activation:get(st,'activation'),channels:get(ch,'registry'),pendingWithdrawals:get(ch,'pending'),contexts:get(st,'clearingContexts'),blockTrades:get(cl,'blockTrades'),deleverageTrades:get(cl,'deleverageTrades'),liquidationRecords:get(cl,'liquidationRecords'),oracleMarks:get(cl,'latestOracleMarkById'),nativeMarks:get(cl,'latestNativeMarkById'),oracleCache:fallback(get(oi,'oracles'),A([])),oracleInitialized:B(oi[0]!=='null'&&oi[0]!=='undefined'),oracleWatermarks:get(o,'lastOracleUpdateBlock'),insurance:get(src,'insurance'),globals:get(st,'globals')});
 const effects=A(r.effects[1].filter(v=>{const kind=get(v,'kind')[1];return ['database','db','uuid','random','rpc-clock','unhandled-rejection'].includes(kind)||(kind==='source-function'&&get(v,'target')[1]!=='Types.decodePayload');}));
 console.log(JSON.stringify(norm(O({name:S(r.name),command:c,result:r.result,state,effects}))));
}
"#;
fn cases(lock: bool) -> Vec<V> {
    let out = std::process::Command::new("node")
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
    let kind=v.get("kind");if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","unhandled-rejection"].contains(&k.as_str())){Some(v)}else if kind==s("main-phase"){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}else{None}
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
#[test]
fn genuine_main_admitted_transactions_mid_and_journal_both_numeric_versions() {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true));
    assert_eq!(
        baseline.len(),
        38,
        "39 original observations minus full import boundary"
    );
    let mut runtime = SourceContractRuntime::with_policy(P::MainCallableV8);
    let (mut resets, mut admitted, mut refused) = (0, 0, 0);
    for case in &baseline {
        let c = case.get("command");
        if c.get("reset") != V::Undefined {
            resets += 1;
            runtime = SourceContractRuntime::with_policy(P::MainCallableV8);
            for command in imports(case) {
                if let C::Import { collection, rows } = command {
                    runtime.import_collection(&collection, rows).unwrap();
                }
            }
            runtime.take_effects();
            state(&runtime, case);
            continue;
        }
        let target = text(c.get("target"));
        if matches!(
            target.as_str(),
            "Main.constructConsensusFromIndex" | "Main.blockHandlerBegin"
        ) {
            refused += 1;
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&call(&target, array(c.get("args")))),
                Err(SourceRuleError::UnknownDependency)
            );
            assert!(runtime == before, "unavailable phase locally rolls back");
            continue;
        }
        if let Some(f) = failure(&c) {
            runtime.fail_storage(f).unwrap();
        }
        compare(
            &result(
                runtime
                    .invoke_dependency(&call(&target, array(c.get("args"))))
                    .unwrap(),
            ),
            &case.get("result"),
            &format!("{} exact transport/outcome", text(case.get("name"))),
        );
        state(&runtime, case);
        compare(
            &effects(runtime.take_effects()),
            &case.get("effects"),
            &format!("{} ordered effects", text(case.get("name"))),
        );
        admitted += 1;
    }
    assert_eq!((resets, admitted, refused), (13, 18, 7));
}
#[test]
fn main_replay_profiles_failures_retries_and_refusals_are_bound() {
    let baseline = cases(false);
    let mut replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
    let mut admitted = 0;
    for case in &baseline {
        let c = case.get("command");
        if c.get("reset") != V::Undefined {
            replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
            for i in imports(case) {
                replay.apply(i).unwrap();
            }
            continue;
        }
        let target = text(c.get("target"));
        if matches!(
            target.as_str(),
            "Main.constructConsensusFromIndex" | "Main.blockHandlerBegin"
        ) {
            continue;
        }
        if let Some(f) = failure(&c) {
            replay.apply(C::FailStorage(f)).unwrap();
        }
        let receipt = replay
            .apply(C::Invoke(call(&target, array(c.get("args")))))
            .unwrap();
        let O::Dependency(v) = receipt.outcome else {
            panic!("Main must execute, not record a supplied verdict")
        };
        compare(&result(v), &case.get("result"), "replayed genuine outcome");
        state(replay.runtime(), case);
        let bytes = replay.checkpoint().unwrap();
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert!(
            restored == replay,
            "whole replay state including aliases/counters/profile"
        );
        assert!(
            restored.checkpoint().unwrap() == bytes,
            "canonical checkpoint"
        );
        let mut wrong = replay.head();
        wrong.commitment[0] ^= 1;
        assert!(SourceContractReplay::from_checkpoint(&bytes, wrong).is_err());
        let mut altered = bytes;
        let at = altered.len() / 2;
        altered[at] ^= 1;
        assert!(SourceContractReplay::from_checkpoint(&altered, replay.head()).is_err());
        admitted += 1;
    }
    assert_eq!(admitted, 18);
    let mut runtime = SourceContractRuntime::with_policy(P::MainCallableV8);
    assert_eq!(
        runtime
            .invoke_dependency(&call(
                "SourceMain.configure",
                vec![obj(&[("activationProfile", s("declared-answer"))])]
            ))
            .unwrap(),
        DependencyReturn::Immediate(V::Undefined)
    );
    runtime.set_activation_answer(false);
    assert_eq!(
        runtime
            .invoke_dependency(&call("Activation.isTxTypeActive", vec![n(18.)]))
            .unwrap(),
        DependencyReturn::Pending(V::Bool(false))
    );
    let before = runtime.clone();
    assert_eq!(
        runtime.invoke_dependency(&call(
            "SourceMain.configure",
            vec![obj(&[("activationProfile", s("source-registry"))])]
        )),
        Err(SourceRuleError::InvalidInput)
    );
    assert!(runtime == before);
    let mut profile_replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
    profile_replay
        .apply(C::Invoke(call(
            "SourceMain.configure",
            vec![obj(&[("activationProfile", s("declared-answer"))])],
        )))
        .unwrap();
    profile_replay
        .apply(C::Invoke(call("Activation.isTxTypeActive", vec![n(18.)])))
        .unwrap();
    let locked = profile_replay.runtime().clone();
    let rejected = profile_replay
        .apply(C::Invoke(call(
            "SourceMain.configure",
            vec![obj(&[("activationProfile", s("source-registry"))])],
        )))
        .unwrap();
    assert_eq!(
        rejected.outcome,
        O::LocalError(SourceRuleError::InvalidInput)
    );
    assert!(
        profile_replay.runtime() == &locked,
        "rejected switch has no runtime effects"
    );
    let bytes = profile_replay.checkpoint().unwrap();
    assert!(
        SourceContractReplay::from_checkpoint(&bytes, profile_replay.head()).unwrap()
            == profile_replay,
        "entry profile and rejected switch are replay bound"
    );
    let mut registry = SourceContractRuntime::with_policy(P::MainCallableV8);
    registry.set_activation_answer(true);
    assert_eq!(
        registry
            .invoke_dependency(&call("Activation.isTxTypeActive", vec![n(18.)]))
            .unwrap(),
        DependencyReturn::Pending(V::Bool(false)),
        "default empty actual registry, independent of old declared answer"
    );
    assert_eq!(
        registry
            .state_value()
            .unwrap()
            .get("mainCallable")
            .get("activationLocked"),
        V::Bool(true)
    );
    let before = registry.clone();
    assert_eq!(
        registry.invoke_dependency(&call(
            "Main.blockHandlerMid",
            vec![V::Array(vec![]), n(f64::INFINITY)]
        )),
        Err(SourceRuleError::InvalidInput)
    );
    assert!(
        registry == before,
        "local height boundary restores full state and effects"
    );
    let before = runtime.clone();
    assert_eq!(
        runtime.invoke_dependency(&call(
            "Main.blockHandlerMid",
            vec![V::Array(vec![]), n(10.)]
        )),
        Err(SourceRuleError::InvalidInput)
    );
    assert!(runtime == before);
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
    ] {
        let mut old = SourceContractRuntime::with_policy(policy);
        let before = old.clone();
        assert_eq!(
            old.invoke_dependency(&call(
                "Main.blockHandlerMid",
                vec![V::Array(vec![]), n(10.)]
            )),
            Err(SourceRuleError::UnknownDependency)
        );
        assert!(old == before);
    }
}
