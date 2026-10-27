//! Complete unchanged MarginMap class: constructor Maps/Sets and real replay.
use tradelayer_core::source_contract_replay::{SourceContractCommand as C, SourceContractReplay};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};
fn s(v: &str) -> V {
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
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else { panic!("array") };
    v
}
fn text(v: V) -> String {
    let V::String(v) = v else { panic!("string") };
    v
}
fn observed(result: DependencyReturn) -> V {
    let (transport, outcome) = match result {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
        DependencyReturn::Throw(e) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
        DependencyReturn::PendingThrow(e) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
fn compare(actual: &V, expected: &V, label: &str) {
    fn first(a: &V, b: &V, path: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return first(a, b, format!("{path}[{i}]"));
                    }
                }
                format!("{path} lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{path} fields {ak} != {bk}");
                    }
                    if av != bv {
                        return first(av, bv, format!("{path}.{ak}"));
                    }
                }
                format!("{path} field lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{path}: {a:?} != {b:?}"),
        }
    }
    if actual != expected {
        panic!("{label}: {}", first(actual, expected, "$".into()));
    }
}
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),Module=require('node:module'),crypto=require('node:crypto');
const root=process.argv[1],version=process.argv[2],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN;
if(version==='9.1.2'){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine9.1.2');const module={exports:{}};vm.runInNewContext(source.toString('utf8'),{module,exports:module.exports},{timeout:1000});BN=module.exports;numeric.loadBigNumber=()=>BN;}else{if(version!=='9.3.1')throw Error('version');BN=numeric.loadBigNumber();}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,getFailure(){return failure},types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
let e,instances;const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',TK='1-100-C-100',h={sourceMarginInstance:1};
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(['[object Map]','[object Set]'].includes(Object.prototype.toString.call(v)))return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const f=e.getFailure();return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),instances:instances.map(v=>({seriesId:v.seriesId,margins:[...v.margins],expiryIndex:[...v.expiryIndex],tickerExpiry:[...v.tickerExpiry],optionOI:[...v.optionOI]})),failure:f?[{collection:f.collection,operation:f.op+'Async',skip:f.skip,message:f.message}]:[]};}
function emit(name,command,result){console.log(JSON.stringify(tag({name,command,result,state:state(),effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock'].includes(v.kind))})));}
async function reset(name){e=m.exports.environment({});instances=[];const M=e.load('marginMap'),load=M.loadMarginMap;M.loadMarginMap=function(...args){const p=load.apply(this,args);p.then(v=>{if(v&&v.margins&&!instances.includes(v))instances.push(v);},()=>{});return p;};await flush();e.effects.length=0;emit(name,{reset:true},{outcome:{returned:undefined}});}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure)e.setFailure(failure);let transport,outcome;try{const M=e.load('marginMap'),receiver=method==='getInstance'?M:instances[args[0].sourceMarginInstance-1],values=method==='getInstance'?args:args.slice(1);let v=receiver[method](...values);if(v&&typeof v.then==='function'){transport='Pending';try{v=await v;outcome={returned:v};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else{transport='Immediate';outcome={returned:v};}if(method==='getInstance'&&outcome.returned?.margins)outcome.returned={sourceMarginInstance:instances.indexOf(outcome.returned)+1};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{method,args,failure},{transport,outcome});}
const touch=(name,before,after,ticker,expiry)=>call(name,'_touchExpiryIndex',[h,before,after,ticker,{expiryBlock:expiry}]);
(async()=>{
await reset('open-count-close');await call('instance','getInstance',[1]);await touch('first-open',0,1,TK,100);await touch('second-position-increments-no-expiry-reassignment',0,2,TK,999);await call('too-early','getExpiringTickersUpTo',[h,99]);await call('at-expiry','getExpiringTickersUpTo',[h,100]);await touch('one-position-closes',1,0,TK,100);await call('still-indexed','getExpiringTickersUpTo',[h,100]);await touch('last-position-closes',-1,0,TK,100);await call('all-empty-after-last-close','getExpiringTickersUpTo',[h,Infinity]);await touch('close-missing-does-not-create-index',1,0,'missing',100);
await reset('map-set-insertion-order');await call('ordered-instance','getInstance',[1]);await touch('open-date200-first',0,1,'A',200);await touch('open-date100-second',0,1,'B',100);await touch('same-date-set-order',0,1,'C',200);await touch('string-date-distinct-map-key',0,1,'D','100');await call('filter-with-map-order','getExpiringTickersUpTo',[h,150]);await call('full-map-and-set-order','getExpiringTickersUpTo',[h,Infinity]);await touch('delete-first-set-member',1,0,'A',200);await touch('reinsert-goes-to-set-end',0,1,'A',200);await touch('delete-other-set-member',1,0,'C',200);await touch('remove-empty-date-map-entry',1,0,'A',200);await touch('recreate-date-goes-to-map-end',0,1,'A',200);await call('reinserted-map-order','getExpiringTickersUpTo',[h,Infinity]);await call('numeric-cleanup-removes-both-key-types','cleanupExpiredTickersUpTo',[h,100]);await call('remaining-unexpired','getExpiringTickersUpTo',[h,Infinity]);await call('clear-final-index','cleanupExpiredTickersUpTo',[h,Infinity]);
await reset('coercions');await call('coercion-instance','getInstance',[1]);await touch('undefined-to-hex-number',undefined,'0x2',undefined,true);await touch('nan-is-zero',NaN,-1,null,'10');await touch('negative-zero-to-boolean',-0,true,-0,Infinity);await touch('ordinary-object-number-nan',{},1,NaN,'not-a-number');await touch('array-empty-string-zero-no-transition',[], '', 'empty',10);await touch('flip-without-zero-no-index-change',-1,1,null,'10');await touch('boolean-to-zero-closes-normalized-zero',true,0,0,Infinity);for(const expiry of [undefined,null,0,-0,NaN,'',false])await touch('falsey-expiry-noop-'+String(expiry),0,1,'guard',expiry);await touch('array-zero-to-number', [0], '1','array-zero',15);await call('hex-height','getExpiringTickersUpTo',[h,'0x10']);await call('cleanup-coercion','cleanupExpiredTickersUpTo',[h,'0x10']);await call('nan-date-never-expires','getExpiringTickersUpTo',[h,Infinity]);await touch('nan-after-closes-undefined-ticker',1,'NaN',undefined,true);
await reset('actual-wrong-arguments-and-restart');await call('real-option-instance','getInstance',[1]);await call('actual-apply-retains-wrong-this-noop','applyOptionTrade',[h,A,TK,1,2,10,0]);await call('literal-extra-this-still-noop','_touchExpiryIndex',[h,h,0,1,TK,{expiryBlock:100}]);await touch('manually-index-real-position',0,1,TK,100);await call('save-position-only','saveMarginMap',[h,10]);await call('fresh-receiver-does-not-infer-indexes','getInstance',[1]);await call('fresh-receiver-index-empty','getExpiringTickersUpTo',[{sourceMarginInstance:2},Infinity]);await call('prior-live-receiver-remains-indexed','getExpiringTickersUpTo',[h,Infinity]);
await reset('retained-failure-and-retry');await call('failure-instance','getInstance',[1]);await call('pure-index-does-not-consume-real-db-failure','_touchExpiryIndex',[h,0,1,TK,{expiryBlock:100}],{collection:'marginMaps',op:'update',message:'index-save'});await call('read-does-not-consume-db-failure','getExpiringTickersUpTo',[h,100]);await call('real-save-failure-retains-indexes','saveMarginMap',[h,10]);await call('real-save-retry','saveMarginMap',[h,10]);await call('cleanup-retained-index','cleanupExpiredTickersUpTo',[h,100]);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn cases(version: &str) -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR"), version])
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
fn failure(v: &V) -> StorageFailure {
    StorageFailure {
        collection: text(v.get("collection")),
        operation: format!("{}Async", text(v.get("op"))),
        skip: 0,
        message: text(v.get("message")),
    }
}
fn assert_state(runtime: &SourceContractRuntime, case: &V) {
    let name = text(case.get("name"));
    let expected = case.get("state");
    let names = array(expected.get("tables"))
        .into_iter()
        .map(|table| {
            let name = text(table.get("name"));
            compare(
                &V::Array(runtime.collection(&name).to_vec()),
                &table.get("rows"),
                &format!("{name} complete persisted rows"),
            );
            name
        })
        .collect::<std::collections::BTreeSet<_>>();
    let state = runtime.state_value().unwrap();
    for table in array(state.get("tables")) {
        let name = text(table.get("name"));
        if !names.contains(&name) {
            assert!(array(table.get("rows")).is_empty(), "{name} extra rows");
        }
    }
    let expected_instances = array(expected.get("instances"));
    for (index, expected) in expected_instances.iter().enumerate() {
        compare(
            &runtime.margins().instance_value(index as u64 + 1).unwrap(),
            expected,
            &format!("{name} full live instance{index}"),
        );
    }
    assert_eq!(
        array(state.get("margins").get("instances")).len(),
        expected_instances.len(),
        "{name} extra owner instance"
    );
    compare(
        &state.get("continuation").get("failures"),
        &expected.get("failure"),
        &format!("{name} actual pending failure schedule"),
    );
    let owner = runtime.margins();
    let bytes = owner.checkpoint().unwrap();
    assert!(
        tradelayer_core::margin_map_source_owner::MarginMapSourceOwner::from_checkpoint(&bytes)
            .unwrap()
            == *owner,
        "{name} complete index checkpoint"
    );
}
fn import(runtime: &mut SourceContractRuntime, case: &V) {
    for table in array(case.get("state").get("tables")) {
        runtime
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
}
fn apply(runtime: &mut SourceContractRuntime, case: &V) {
    let command = case.get("command");
    if matches!(command.get("failure"), V::Object(_)) {
        runtime
            .fail_storage(failure(&command.get("failure")))
            .unwrap();
    }
    let result = runtime
        .invoke_dependency(&DependencyCall {
            target: format!("Margin.{}", text(command.get("method"))),
            arguments: array(command.get("args")),
        })
        .unwrap_or_else(|e| panic!("{} local {e:?}", text(case.get("name"))));
    compare(
        &observed(result),
        &case.get("result"),
        &format!("{} actual transport/outcome", text(case.get("name"))),
    );
}
#[test]
fn genuine_constructor_owned_indexes_match_maps_sets_coercions_and_restarts() {
    let cases = cases("9.3.1");
    assert_eq!(
        cases,
        cases_for_lock(),
        "tested genuine numeric versions diverged"
    );
    let mut runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
    let (mut resets, mut calls) = (0, 0);
    for case in cases {
        if case.get("command").get("reset") == V::Bool(true) {
            runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
            import(&mut runtime, &case);
            resets += 1;
        } else {
            apply(&mut runtime, &case);
            calls += 1;
        }
        assert_state(&runtime, &case);
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{} exact ordered effects", text(case.get("name"))),
        );
    }
    assert_eq!((resets, calls), (5, 60));
}
fn cases_for_lock() -> Vec<V> {
    cases("9.1.2")
}
#[test]
fn populated_index_history_and_failures_replay_without_changing_older_policies() {
    let cases = cases("9.3.1");
    for selected in [
        "map-set-insertion-order",
        "actual-wrong-arguments-and-restart",
        "retained-failure-and-retry",
    ] {
        let start = cases
            .iter()
            .position(|v| v.get("name") == s(selected))
            .unwrap();
        let mut replay = SourceContractReplay::with_policy(P::OptionLifecycleV4).unwrap();
        for table in array(cases[start].get("state").get("tables")) {
            replay
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        for case in cases[start + 1..]
            .iter()
            .take_while(|v| v.get("command").get("reset") != V::Bool(true))
        {
            let command = case.get("command");
            if matches!(command.get("failure"), V::Object(_)) {
                replay
                    .apply(C::FailStorage(failure(&command.get("failure"))))
                    .unwrap();
            }
            replay
                .apply(C::Invoke(DependencyCall {
                    target: format!("Margin.{}", text(command.get("method"))),
                    arguments: array(command.get("args")),
                }))
                .unwrap();
            assert_state(replay.runtime(), case);
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert!(
                restored == replay,
                "{} whole replay continuation",
                text(case.get("name"))
            );
            replay = restored;
        }
    }
    for policy in [P::OnchainV1, P::GlobalClearingV2, P::SourceLifecycleV3] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        assert_eq!(
            runtime.invoke_dependency(&DependencyCall {
                target: "Margin._touchExpiryIndex".into(),
                arguments: vec![]
            }),
            Err(if policy == P::OnchainV1 {
                SourceRuleError::UnknownDependency
            } else {
                SourceRuleError::UnknownMethod
            })
        );
        assert!(runtime == before, "old policy changed admission/state");
    }

    // The optional constructor Maps are canonical ordered data. They may not
    // encode duplicate SameValueZero keys, duplicate Set members, partial
    // groups, or an alternative explicit-empty encoding of the legacy state.
    let instance = |expiry: V, ticker: V, oi: V| {
        obj(&[
            ("seriesId", V::number(1.0)),
            ("margins", V::Array(vec![])),
            ("expiryIndex", expiry),
            ("tickerExpiry", ticker),
            ("optionOI", oi),
        ])
    };
    let pair = |key: V, value: V| V::Array(vec![key, value]);
    let expiry = V::Array(vec![pair(V::number(100.0), V::Array(vec![s("A"), s("B")]))]);
    let tickers = V::Array(vec![
        pair(s("A"), V::number(100.0)),
        pair(s("B"), V::number(100.0)),
    ]);
    let oi = V::Array(vec![
        pair(s("A"), V::number(2.0)),
        pair(s("B"), V::number(1.0)),
    ]);
    let state = |value: V| {
        obj(&[
            ("instances", V::Array(vec![value])),
            ("objects", V::Array(vec![])),
        ])
    };
    use tradelayer_core::margin_map_source_owner::MarginMapSourceOwner;
    let valid = state(instance(expiry.clone(), tickers.clone(), oi.clone()));
    let owner = MarginMapSourceOwner::from_state_value(&valid).unwrap();
    assert!(
        MarginMapSourceOwner::from_checkpoint(&owner.checkpoint().unwrap()).unwrap() == owner,
        "populated canonical state must roundtrip"
    );
    let duplicates = V::Array(vec![
        pair(V::number(100.0), V::Array(vec![s("A")])),
        pair(V::number(100.0), V::Array(vec![s("B")])),
    ]);
    let duplicate_set = V::Array(vec![pair(V::number(100.0), V::Array(vec![s("A"), s("A")]))]);
    let negative_zero_key = V::Array(vec![pair(V::number(-0.0), V::Array(vec![s("A")]))]);
    let negative_zero_member = V::Array(vec![pair(
        V::number(100.0),
        V::Array(vec![V::number(-0.0)]),
    )]);
    let object_key = V::Array(vec![pair(obj(&[]), V::Array(vec![s("A")]))]);
    let duplicate_tickers = V::Array(vec![
        pair(s("A"), V::number(100.0)),
        pair(s("A"), V::number(100.0)),
    ]);
    for invalid in [
        instance(duplicates, tickers.clone(), oi.clone()),
        instance(duplicate_set, tickers.clone(), oi.clone()),
        instance(negative_zero_key, tickers.clone(), oi.clone()),
        instance(negative_zero_member, tickers.clone(), oi.clone()),
        instance(object_key, tickers.clone(), oi.clone()),
        instance(expiry.clone(), duplicate_tickers.clone(), oi.clone()),
        instance(expiry.clone(), tickers.clone(), duplicate_tickers),
        instance(expiry.clone(), V::Undefined, oi.clone()),
        instance(V::Null, tickers.clone(), oi.clone()),
        instance(V::Array(vec![]), V::Array(vec![]), V::Array(vec![])),
    ] {
        assert!(
            MarginMapSourceOwner::from_state_value(&state(invalid)).is_err(),
            "noncanonical optional Map/Set state was accepted"
        );
    }

    // Object-identity Map keys lie outside this portable owner domain. A local
    // refusal restores all state/effects rather than becoming a JS throw or
    // leaving the optionOI prefix written before the unsupported expiry key.
    let mut runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
    runtime
        .invoke_dependency(&DependencyCall {
            target: "Margin.getInstance".into(),
            arguments: vec![V::number(1.0)],
        })
        .unwrap();
    let before = runtime.clone();
    assert_eq!(
        runtime.invoke_dependency(&DependencyCall {
            target: "Margin._touchExpiryIndex".into(),
            arguments: vec![
                obj(&[("sourceMarginInstance", V::number(1.0))]),
                V::number(0.0),
                V::number(1.0),
                s("local-boundary"),
                obj(&[("expiryBlock", obj(&[]))]),
            ],
        }),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert!(
        runtime == before,
        "local index-domain failure leaked a prefix"
    );
}
