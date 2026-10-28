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
const OBSERVER: &str = r#"
const path=require('node:path'),cp=require('node:child_process');
const root=process.env.PROOF_REPO,stage=process.env.TL_MAIN_EVIDENCE_ROOT||root;
const args=[path.join(stage,'scripts/main_spot_vectors.cjs'),'--print','--check','--all-receivers'];if(process.argv.includes('--lock'))args.push('--lock');
const raw=cp.execFileSync(process.execPath,args,{env:{...process.env,PROOF_REPO:root},maxBuffer:16*1024*1024,encoding:'utf8'});
const U=['undefined'],A=x=>['array',x],S=x=>['string',x],B=x=>['boolean',x],O=x=>['object',Object.entries(x)];
function get(v,key){return v?.[0]==='object'?(v[1].find(([k])=>k===key)?.[1]||U):U;}
function fallback(v,x){return v[0]==='undefined'?x:v;}
function norm(v){const t=v[0];if(t==='map')return A(v[1].map(([k,x])=>A([norm(k),norm(x)])));if(t==='set')return A(v[1].map(norm));if(t==='array')return A(v[1].map(norm));if(t==='object')return ['object',v[1].map(([k,x])=>[k,norm(x)])];if(t==='regexp'){return v[1]==='^tx'&&v[2]===''?S('^tx'):O({'$regexp':S(v[1]),'$flags':S(v[2])});}if(t==='function'||t==='cycle')throw Error('unadmitted identity transport');return v;}
for(const line of raw.trim().split('\n')){const r=JSON.parse(line),c=r.command;
 if(get(c,'profile')[1]==='full-import')continue;
 const st=r.state,src=get(st,'source'),t=get(st,'tally'),p=get(st,'property'),k=get(st,'contract'),ki=get(k,'instance'),o=get(src,'oracle'),oi=get(o,'instance'),cl=get(src,'clearing'),ch=get(st,'channels');
 const state=O({tables:get(src,'tables'),books:get(src,'instances'),margins:get(st,'marginInstances'),tally:fallback(get(t,'addresses'),A([])),tallyInitialized:get(t,'initialized'),tallyMod:get(t,'modFlag'),contractMod:get(k,'modFlag'),propertyCache:fallback(get(p,'propertyIndex'),A([])),propertyInitialized:B(p[0]!=='undefined'),contractSeries:fallback(get(ki,'contractSeries'),A([])),registryInitialized:B(ki[0]!=='undefined'),seriesInitialized:B(get(ki,'contractSeries')[0]!=='undefined'),activation:get(st,'activation'),channels:get(ch,'registry'),pendingWithdrawals:get(ch,'pending'),contexts:get(st,'clearingContexts'),blockTrades:get(cl,'blockTrades'),deleverageTrades:get(cl,'deleverageTrades'),liquidationRecords:get(cl,'liquidationRecords'),oracleMarks:get(cl,'latestOracleMarkById'),nativeMarks:get(cl,'latestNativeMarkById'),oracleCache:fallback(get(oi,'oracles'),A([])),oracleInitialized:B(oi[0]!=='null'&&oi[0]!=='undefined'),oracleWatermarks:get(o,'lastOracleUpdateBlock'),insurance:get(src,'insurance'),globals:get(st,'globals'),clearlist:get(st,'clearlist'),tallyInstance:fallback(get(t,'instance'),U),feeBuffer:fallback(get(get(src,'tally'),'feeAccrualBuffer'),A([])),feeBufferInitialized:B(get(get(src,'tally'),'feeAccrualBuffer')[0]!=='undefined')});
 const effects=A(r.effects[1].filter(v=>{const kind=get(v,'kind')[1];return ['database','db','uuid','random','rpc-clock','rpc','unhandled-rejection'].includes(kind)||(kind==='source-function'&&get(v,'target')[1]!=='Types.decodePayload');}));
 console.log(JSON.stringify(norm(O({name:S(r.name),domain:S(r.domain),command:c,result:r.result,state,effects}))));
}
"#;
fn cases(lock: bool) -> Vec<V> {
    let evidence = std::env::var("TL_MAIN_SPOT_EVIDENCE_ROOT")
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
    let kind=v.get("kind");if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","rpc","unhandled-rejection"].contains(&k.as_str())){Some(v)}else if kind==s("main-phase")||(kind==s("dependency")&&matches!(v.get("target"),V::String(t)if ["TxUtils.getRawTransaction","TxUtils.getBlockHeight"].contains(&t.as_str()))){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}else{None}
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
    assert_eq!(all.get("runtimePolicy"), s("SpotLifecycleV9"));
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
        &V::Bool(all.get("globals").get("sourceSpotRpcDocuments") == V::Bool(true)),
        &expected.get("globals").get("spotRpcDocumentsBound"),
        &format!("{name} explicit original RPC document profile"),
    );
    compare(
        &V::Bool(all.get("globals").get("tallyMap") != V::Undefined),
        &expected.get("globals").get("tallyMapBound"),
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
#[test]
fn original_main_spot_actual_registry_queue_fee_prefixes_and_full_replay() {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true), "both genuine numeric dependencies");
    let mut replay = SourceContractReplay::with_policy(P::SpotLifecycleV9).unwrap();
    let (mut resets, mut calls, mut unavailable, mut inputs) = (0, 0, 0, 0);
    let mut after_unavailable = false;
    let mut kinds = std::collections::BTreeSet::new();
    for case in &baseline {
        let command = case.get("command");
        let name = text(case.get("name"));
        if command.get("reset") != V::Undefined {
            resets += 1;
            replay = SourceContractReplay::with_policy(P::SpotLifecycleV9).unwrap();
            for input in imports(case) {
                replay.apply(input).unwrap();
            }
            if command.get("reset").get("options").get("bindRpc") == V::Bool(true) {
                replay
                    .apply(C::Invoke(call("SourceGlobal.bindSpotRpcDocuments", vec![])))
                    .unwrap();
            }
            if command.get("reset").get("options").get("bindTally") == V::Bool(true) {
                replay
                    .apply(C::Invoke(call("SourceGlobal.bindTally", vec![])))
                    .unwrap();
            }
            after_unavailable = false;
        } else if command.get("import") != V::Undefined {
            inputs += 1;
            let input = command.get("import");
            replay
                .apply(C::Import {
                    collection: text(input.get("collection")),
                    rows: array(input.get("rows")),
                })
                .unwrap();
        } else if command.get("binding") == s("rpcTransactions") {
            inputs += 1;
            replay
                .apply(C::Invoke(call("SourceGlobal.bindRpcTransactions", vec![])))
                .unwrap();
        } else {
            let target = text(command.get("target"));
            let arguments = array(command.get("args"));
            if case.get("domain") == s("unavailable-capability") {
                unavailable += 1;
                let before = replay.runtime().clone();
                assert_eq!(
                    replay
                        .apply(C::Invoke(call(&target, arguments)))
                        .unwrap()
                        .outcome,
                    O::LocalError(SourceRuleError::UnknownDependency),
                    "{name} explicit local unavailable capability"
                );
                assert_eq!(
                    replay.runtime(),
                    &before,
                    "{name} local refusal preserves the whole owner"
                );
                after_unavailable = true;
                continue;
            }
            assert!(!after_unavailable, "a source-unavailable prefix cannot be injected as the next Rust prestate; isolate the experiment with a constructor reset");
            calls += 1;
            if let Some(failure) = failure(&command) {
                replay.apply(C::FailStorage(failure)).unwrap();
            }
            if matches!(
                target.as_str(),
                "Main.processTxSet" | "Main.processTx" | "Main.blockHandlerMid"
            ) {
                if let Some(V::Array(rows)) = arguments.first() {
                    for row in rows {
                        let value = if target == "Main.processTxSet" {
                            row.get("value")
                        } else {
                            row.clone()
                        };
                        if let V::String(payload) = value.get("payload") {
                            if let Some(kind) = payload.chars().next().and_then(|c| c.to_digit(36))
                            {
                                kinds.insert(kind);
                            }
                        }
                    }
                }
            }
            let receipt = replay.apply(C::Invoke(call(&target, arguments))).unwrap();
            let value = match receipt.outcome {
                O::Dependency(value) => value,
                other => {
                    panic!("{name} must execute original Main, actual receipt outcome: {other:?}")
                }
            };
            compare(
                &result(value),
                &case.get("result"),
                &format!("{name} actual source outcome/transport"),
            );
            lower_state(replay.runtime(), case);
            compare(
                &effects(receipt.effects),
                &case.get("effects"),
                &format!("{name} complete ordered effects"),
            );
        }
        if command.get("target") == V::Undefined {
            lower_state(replay.runtime(), case);
        }
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(&bytes[..5], b"TLCR\x09");
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert_eq!(
            restored, replay,
            "{name} complete source-history reconstruction"
        );
        assert_eq!(
            restored.checkpoint().unwrap(),
            bytes,
            "{name} exact canonical re-encoding"
        );
    }
    assert_eq!(
        (baseline.len(), resets, calls, unavailable, inputs),
        (27, 6, 20, 1, 0),
        "genuine Main+spot source domains"
    );
    assert!(
        kinds.is_superset(&[5, 6].into_iter().collect()),
        "actual Main calls cover both newly admitted spot kinds"
    );
}

#[test]
fn prior_v8_main_keeps_its_guard_and_no_spot_job_diagnostic() {
    let baseline = cases(false);
    let mut replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
    for input in imports(&baseline[0]) {
        replay.apply(input).unwrap();
    }
    let before = replay.runtime().clone();
    let command = baseline
        .iter()
        .find(|c| c.get("command").get("target") == s("Main.processTxSet"))
        .unwrap()
        .get("command");
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
    assert_eq!(
        replay.runtime(),
        &before,
        "V9 admission cannot widen the existing V8 Main graph"
    );
    assert_eq!(
        replay.runtime().state_value().unwrap().get("spotLifecycle"),
        V::Undefined
    );
    let checkpoint = replay.checkpoint().unwrap();
    assert_eq!(&checkpoint[..5], b"TLCR\x08");
    assert_eq!(
        SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap(),
        replay
    );
}
