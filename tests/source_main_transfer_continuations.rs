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
const args=[path.join(stage,'scripts/main_transfer_continuations_vectors.cjs'),'--print','--check'];if(process.argv.includes('--lock'))args.push('--lock');
const raw=cp.execFileSync(process.execPath,args,{env:{...process.env,PROOF_REPO:root},maxBuffer:16*1024*1024,encoding:'utf8'});
const U=['undefined'],A=x=>['array',x],S=x=>['string',x],B=x=>['boolean',x],O=x=>['object',Object.entries(x)];
function get(v,key){return v?.[0]==='object'?(v[1].find(([k])=>k===key)?.[1]||U):U;}
function fallback(v,x){return v[0]==='undefined'?x:v;}
function norm(v){const t=v[0];if(t==='map')return A(v[1].map(([k,x])=>A([norm(k),norm(x)])));if(t==='set')return A(v[1].map(norm));if(t==='array')return A(v[1].map(norm));if(t==='object')return ['object',v[1].map(([k,x])=>[k,norm(x)])];if(t==='regexp'){return v[1]==='^tx'&&v[2]===''?S('^tx'):O({'$regexp':S(v[1]),'$flags':S(v[2])});}if(t==='function'||t==='cycle')throw Error('unadmitted identity transport');return v;}
for(const line of raw.trim().split('\n')){const r=JSON.parse(line),c=r.command;
 if(get(c,'profile')[1]==='full-import')continue;
 const st=r.state,src=get(st,'source'),t=get(st,'tally'),p=get(st,'property'),k=get(st,'contract'),ki=get(k,'instance'),o=get(src,'oracle'),oi=get(o,'instance'),cl=get(src,'clearing'),ch=get(st,'channels');
 const state=O({sourceReceiverEvidence:O({samePrototype:get(get(st,'receiver'),'samePrototype'),sameSingleton:get(get(st,'receiver'),'sameSingleton')}),tables:get(src,'tables'),books:get(src,'instances'),margins:get(st,'marginInstances'),marginIdentity:get(st,'marginIdentity'),vaults:get(st,'vaults'),proofBoundary:get(st,'proofBoundary'),tally:fallback(get(t,'addresses'),A([])),tallyInitialized:get(t,'initialized'),tallyMod:get(t,'modFlag'),contractMod:get(k,'modFlag'),propertyCache:fallback(get(p,'propertyIndex'),A([])),propertyInitialized:B(p[0]!=='undefined'),contractSeries:fallback(get(ki,'contractSeries'),A([])),registryInitialized:B(ki[0]!=='undefined'),seriesInitialized:B(get(ki,'contractSeries')[0]!=='undefined'),activation:get(st,'activation'),channels:get(ch,'registry'),pendingWithdrawals:get(ch,'pending'),contexts:get(st,'clearingContexts'),blockTrades:get(cl,'blockTrades'),deleverageTrades:get(cl,'deleverageTrades'),liquidationRecords:get(cl,'liquidationRecords'),oracleMarks:get(cl,'latestOracleMarkById'),nativeMarks:get(cl,'latestNativeMarkById'),oracleCache:fallback(get(oi,'oracles'),A([])),oracleInitialized:B(oi[0]!=='null'&&oi[0]!=='undefined'),oracleWatermarks:get(o,'lastOracleUpdateBlock'),insurance:get(src,'insurance'),globals:get(st,'globals'),clearlist:get(st,'clearlist'),tallyInstance:fallback(get(t,'instance'),U),feeBuffer:fallback(get(get(src,'tally'),'feeAccrualBuffer'),A([])),feeBufferInitialized:B(get(get(src,'tally'),'feeAccrualBuffer')[0]!=='undefined')});
 const effects=A(r.effects[1].filter(v=>{const kind=get(v,'kind')[1];return ['database','db','uuid','random','rpc-clock','rpc','artifact-read','unhandled-rejection'].includes(kind)||(kind==='source-function'&&get(v,'target')[1]!=='Types.decodePayload');}));
 console.log(JSON.stringify(norm(O({name:S(r.name),domain:S(r.domain),command:c,result:r.result,state,effects}))));
}
"#;
fn cases(lock: bool) -> Vec<V> {
    let evidence = std::env::var("TL_MAIN_UPPER_EVIDENCE_ROOT")
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
    if a != b && label.contains("ordered") {
        if let (V::Array(actual), V::Array(expected)) = (a, b) {
            let at = actual
                .iter()
                .zip(expected)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.len().min(expected.len()));
            for index in at.saturating_sub(3)..(at + 7).min(actual.len().max(expected.len())) {
                eprintln!(
                    "effect {index}: actual={:?} expected={:?}",
                    actual.get(index),
                    expected.get(index)
                );
            }
        }
    }
    assert!(a == b, "{label}: {}", diff(a, b, String::new()));
}
fn normalized(v: V) -> V {
    match v {
        V::BigNumber(t) if t == "-0" => V::BigNumber(t),
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
    let kind=v.get("kind");if matches!(&kind,V::String(k)if ["database","db","uuid","random","rpc-clock","rpc","artifact-read","unhandled-rejection"].contains(&k.as_str())){Some(v)}else if kind==s("main-phase")||(kind==s("dependency")&&matches!(v.get("target"),V::String(t)if ["TxUtils.getRawTransaction","TxUtils.getBlockHeight","TxUtils.getAddressTypeUniversal","TxUtils.extractPubkeyByType"].contains(&t.as_str()))){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}else{None}
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

fn upper_state(runtime: &SourceContractRuntime, case: &V) {
    lower_state(runtime, case);
    let name = text(case.get("name"));
    let expected = case.get("state");
    let all = runtime.state_value().unwrap();
    assert_eq!(expected.get("sourceReceiverEvidence"),obj(&[("samePrototype",V::Bool(true)),("sameSingleton",V::Bool(true))]),"{name} corpus must keep real original Main receivers; no daemon/client success is inferred");
    for key in ["vaults", "syntheticTokens"] {
        compare(
            &normalized(all.get("syntheticLifecycle").get(key)),
            &normalized(expected.get("vaults").get(key)),
            &format!("{name} genuine synthetic {key}"),
        );
    }
    for key in ["environment", "artifacts"] {
        compare(
            &all.get("proofLifecycle").get(key),
            &expected.get("proofBoundary").get(key),
            &format!("{name} declared source proof {key}"),
        );
    }
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
    assert_eq!(
        all.get("mainCallable").get("activationProfile"),
        s("source-registry"),
        "{name} real Activation profile"
    );
    assert_eq!(
        all.get("mainCallable").get("pendingHeight"),
        V::Null,
        "{name} actual public boundary drains height continuation"
    );
}
fn checked_input(replay: &mut SourceContractReplay, command: C, name: &str) {
    let receipt = replay.apply(command).unwrap();
    assert!(
        !matches!(receipt.outcome, O::LocalError(_)),
        "{name} declared input admission failed: {:?}",
        receipt.outcome
    );
}
#[test]
fn original_main_transfer_continuations_varied_rows_tally_failures_and_replay() {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true), "both genuine numeric constructors");
    let mut replay = SourceContractReplay::with_policy(P::SpotLifecycleV9).unwrap();
    let (mut resets, mut calls, mut unavailable) = (0, 0, 0);
    let mut after_unavailable = false;
    let mut kinds = std::collections::BTreeSet::new();
    for case in &baseline {
        let command = case.get("command");
        let name = text(case.get("name"));
        if command.get("reset") != V::Undefined {
            resets += 1;
            replay = SourceContractReplay::with_policy(P::SpotLifecycleV9).unwrap();
            let options = command.get("reset").get("options");
            if let V::String(entry) = options.get("entry") {
                checked_input(&mut replay, C::SetEntry(entry), &name);
            }
            for input in imports(case) {
                checked_input(&mut replay, input, &name);
            }
            for (key, target) in [
                ("environment", "SourceEnvironment.configure"),
                ("artifacts", "SourceArtifacts.configure"),
            ] {
                if options.get(key) != V::Undefined {
                    checked_input(
                        &mut replay,
                        C::Invoke(call(target, vec![options.get(key)])),
                        &name,
                    );
                }
            }
            if options.get("bindRpc") == V::Bool(true) {
                checked_input(
                    &mut replay,
                    C::Invoke(call("SourceGlobal.bindSpotRpcDocuments", vec![])),
                    &name,
                );
            }
            if options.get("bindTally") == V::Bool(true) {
                checked_input(
                    &mut replay,
                    C::Invoke(call("SourceGlobal.bindTally", vec![])),
                    &name,
                );
            }
            after_unavailable = false;
        } else {
            let target = text(command.get("target"));
            let arguments = array(command.get("args"));
            if case.get("domain") == s("unavailable-capability") {
                unavailable += 1;
                let before = replay.runtime().clone();
                let receipt = replay.apply(C::Invoke(call(&target, arguments))).unwrap();
                assert_eq!(
                    receipt.outcome,
                    O::LocalError(SourceRuleError::UnknownDependency),
                    "{name} explicit capability boundary"
                );
                assert!(
                    receipt.effects.is_empty(),
                    "{name} local unavailable dependency rolls back its effects"
                );
                assert_eq!(
                    replay.runtime(),
                    &before,
                    "{name} local refusal keeps full owner"
                );
                let checkpoint = replay.checkpoint().unwrap();
                let restored =
                    SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
                assert_eq!(
                    restored, replay,
                    "{name} explicit local refusal remains a bound replay receipt"
                );
                assert_eq!(restored.checkpoint().unwrap(), checkpoint);
                after_unavailable = true;
                continue;
            }
            assert!(
                !after_unavailable,
                "no unavailable source prefix may become a later Rust prestate"
            );
            calls += 1;
            if let Some(f) = failure(&command) {
                checked_input(&mut replay, C::FailStorage(f), &name);
            }
            if let Some(V::Array(rows)) = arguments.first() {
                for row in rows {
                    let value = if target == "Main.processTxSet" {
                        row.get("value")
                    } else {
                        row.clone()
                    };
                    if let V::String(payload) = value.get("payload") {
                        if let Some(kind) = payload.chars().next().and_then(|c| c.to_digit(36)) {
                            kinds.insert(kind);
                        }
                    }
                }
            }
            let receipt = replay.apply(C::Invoke(call(&target, arguments))).unwrap();
            let O::Dependency(value) = receipt.outcome else {
                panic!("{name} must execute original Main, not record supplied financial/verifier output")
            };
            compare(
                &result(value),
                &case.get("result"),
                &format!("{name} exact source result/transport"),
            );
            upper_state(replay.runtime(), case);
            compare(
                &effects(receipt.effects),
                &case.get("effects"),
                &format!("{name} exact ordered DB/RPC/artifact/phase effects"),
            );
        }
        if command.get("target") == V::Undefined {
            upper_state(replay.runtime(), case);
        }
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(&bytes[..5], b"TLCR\x09");
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert_eq!(
            restored, replay,
            "{name} full real source history/receivers/profiles"
        );
        assert_eq!(
            restored.checkpoint().unwrap(),
            bytes,
            "{name} exact canonical re-encoding"
        );
    }
    assert_eq!(
        (baseline.len(), resets, calls, unavailable),
        (102, 34, 68, 0)
    );
    assert_eq!(kinds, [22].into_iter().collect());
}
