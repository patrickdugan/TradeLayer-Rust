//! Final unchanged Logic methods, actual Margin suffix, and raw17 source failure.
use tradelayer_core::source_contract_replay::{SourceContractCommand as C, SourceContractReplay};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
};
fn s(value: &str) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array")
    };
    value
}
fn text(value: V) -> String {
    let V::String(value) = value else {
        panic!("string")
    };
    value
}
fn returned(value: DependencyReturn) -> V {
    let (transport, outcome) = match value {
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
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm');
const root=process.argv[1],version=process.argv[2],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN;
if(version==='9.1.2'){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine9.1.2');const loaded={exports:{}};vm.runInNewContext(source.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}else{if(version!=='9.3.1')throw Error('version');BN=numeric.loadBigNumber();}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');const end=code.indexOf('const scenes=[');if(end<0)throw Error('source environment changed');code=code.slice(0,end);if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('source inventory changed');code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',TK='1-100-C-100';let e,instances,unhandled,lastParams;
process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),O=e.load('oracle'),C=e.load('contractRegistry'),Ch=e.load('channels');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),instances:instances.map(v=>({seriesId:v.seriesId,margins:[...v.margins],expiryIndex:[...v.expiryIndex],tickerExpiry:[...v.tickerExpiry],optionOI:[...v.optionOI]})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,contractMod:C.modFlag,channels:[...Ch.channelsRegistry],unhandled,lastParams};}
function emit(name,command,result){console.log(JSON.stringify(tag({name,command,result,state:state(),effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind))})));}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',inactive:options.inactive}:options.entry==='volume-first'?{preload:'volumeIndex',inactive:options.inactive}:{entry:options.entry??'main-subgraph',inactive:options.inactive});instances=[];unhandled=[];lastParams=undefined;const M=e.load('marginMap');Object.defineProperty(M.prototype,'seriesId',{configurable:true,set(value){Object.defineProperty(this,'seriesId',{value,writable:true,enumerable:true,configurable:true});instances.push(this);}});for(const method of ['exerciseDerivative','deliverContract','handleOptionExercise']){const original=e.logic[method];e.logic[method]=function(...args){e.effects.push({kind:'source-function',target:'Logic.'+method,args:structuredClone(args)});return Reflect.apply(original,this,args);};}e.table('marginMaps').splice(0,Infinity,...['1',TK].map(seriesId=>({_id:JSON.stringify({seriesId}),value:JSON.stringify([[A,{address:A,contracts:5,margin:10,avgPrice:100,options:{[TK]:{contracts:2,avgPrice:4,margin:3}}}]])})));await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,method,args=[],failure,bare=false){e.effects.length=0;if(failure)fail(failure);let transport,outcome;try{let value=Reflect.apply(e.logic[method],bare?undefined:e.logic,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else{transport='Immediate';outcome={returned:value};}}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:'Logic.'+method,args,failure,nodeOnly:bare},{transport,outcome});}
async function raw(name,payload){e.effects.length=0;lastParams=undefined;const args=[name,17,'tl',payload,A,null,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(17,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args},{outcome});}
const params=(contractId,amount=2)=>({contractId,amount});
(async()=>{
for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 for(const[name,args]of [['omitted',[]],['null',[A,null,10]],['undefined',[A,undefined,10]],['number-params',[A,2,10]],['string-params',[A,'bad',10]],['array-params',[A,[],10]],['empty-params',[A,{},10]],['null-id',[A,params(null),10]],['number-id',[A,params(1),10]],['array-id',[A,params([]),10]],['object-id',[A,params({match:'bad'}),10]],['old-five-arguments',['1',2,{},A,10]]])await call(entry+'-'+name,'exerciseDerivative',args);
 for(const contract of ['1','',TK,'1-P-','1-p-','C--','1-2-3'])await call(entry+'-contract-'+contract,'exerciseDerivative',[A,params(contract),10]);
 await call(entry+'-zero-amount-has-no-old-required-guard','exerciseDerivative',[A,params('1',0),10]);await call(entry+'-missing-amount-is-forwarded','exerciseDerivative',[A,{contractId:'1'},10]);
 await call(entry+'-retry-fresh-instance','exerciseDerivative',[A,params('1'),11]);
 await call(entry+'-delivery-placeholder','deliverContract',['1',2,10]);await call(entry+'-option-placeholder','handleOptionExercise',[TK,7,8,2,10]);
 await raw(entry+'-raw17','1,2');
}
for(const op of ['getDatabase','findOne']){await reset('failure-'+op);const failure={collection:'marginMaps',op,message:'derivative-'+op};await call('failure-'+op+'-prefix','exerciseDerivative',[A,params('1'),10],failure);await call('failure-'+op+'-retry','exerciseDerivative',[A,params('1'),11]);}
await reset('nonarray-document');e.table('marginMaps')[0].value='{}';e.effects.length=0;emit('seed-nonarray-document',{seed:{collection:'marginMaps',rows:structuredClone(e.table('marginMaps'))}},{outcome:{returned:undefined}});await call('nonarray-is-empty-live-instance','exerciseDerivative',[A,params('1'),10]);
await reset('malformed-json');const saved=e.table('marginMaps')[0].value;e.table('marginMaps')[0].value='{';e.effects.length=0;emit('seed-malformed-json',{seed:{collection:'marginMaps',rows:structuredClone(e.table('marginMaps'))}},{outcome:{returned:undefined}});await call('json-failure-retains-constructor-before-catch','exerciseDerivative',[A,params('1'),10]);e.table('marginMaps')[0].value=saved;e.effects.length=0;emit('repair-malformed-json',{seed:{collection:'marginMaps',rows:structuredClone(e.table('marginMaps'))}},{outcome:{returned:undefined}});await call('json-retry-loads-second-instance','exerciseDerivative',[A,params('1'),11]);
await reset('arg-eval-before-margin');await call('argument-error-leaves-factory-failure-pending','exerciseDerivative',[A,params(1),10],{collection:'marginMaps',op:'getDatabase',message:'after-argument-error'});await call('next-real-margin-call-rejects-independently','exerciseDerivative',[A,params('1'),11]);await call('argument-error-sequence-retry','exerciseDerivative',[A,params('1'),12]);
await reset('inactive-raw',{inactive:true});await raw('inactive-still-reference-error','0,0');await raw('malformed-still-reference-error','bad,bad');
await reset('bare-probes');await call('bare-delivery-global-this','exerciseDerivative',[A,params('1'),10],undefined,true);await call('bare-option-global-this','exerciseDerivative',[A,params(TK),10],undefined,true);
})().catch(error=>{console.error(error);process.exitCode=1;});
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
fn failure(command: &V) -> Option<StorageFailure> {
    let value = command.get("failure");
    if value == V::Undefined {
        return None;
    }
    let op = text(value.get("op"));
    Some(StorageFailure {
        collection: text(value.get("collection")),
        operation: if op == "getDatabase" {
            op
        } else {
            format!("{op}Async")
        },
        skip: 0,
        message: text(value.get("message")),
    })
}
fn assert_state(runtime: &SourceContractRuntime, case: &V) {
    let state = case.get("state");
    let name = text(case.get("name"));
    let mut known = std::collections::BTreeSet::new();
    for table in array(state.get("tables")) {
        let collection = text(table.get("name"));
        known.insert(collection.clone());
        assert_eq!(
            V::Array(runtime.collection(&collection).to_vec()),
            table.get("rows"),
            "{name} {collection}"
        );
    }
    let actual = runtime.state_value().unwrap();
    for table in array(actual.get("tables")) {
        if !known.contains(&text(table.get("name"))) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} unexpected rows"
            );
        }
    }
    let global = actual.get("globalClearing");
    for (key, expected) in [
        ("tallyStaticInitialized", "tallyInitialized"),
        ("tallyMod", "tallyMod"),
        ("contractModRaw", "contractMod"),
        ("unhandledRejections", "unhandled"),
    ] {
        assert_eq!(global.get(key), state.get(expected), "{name} {key}");
    }
    assert_eq!(
        runtime.tally_cache(),
        state.get("tally"),
        "{name} sole Tally"
    );
    assert_eq!(
        global.get("channels").get("registry"),
        state.get("channels"),
        "{name} Channels"
    );
    assert_eq!(
        actual.get("oracleSourceCache"),
        state.get("oracleCache"),
        "{name} Oracle cache"
    );
    assert_eq!(
        actual.get("oracleSourceInitialized"),
        state.get("oracleInitialized"),
        "{name} Oracle flag"
    );
    let expected = array(state.get("instances"));
    let actual_instances = array(runtime.margins().state_value().unwrap().get("instances"));
    assert_eq!(
        actual_instances.len(),
        expected.len(),
        "{name} actual fresh instances"
    );
    for (index, value) in expected.iter().enumerate() {
        assert_eq!(
            &runtime.margins().instance_value(index as u64 + 1).unwrap(),
            value,
            "{name} full instance{index}"
        );
    }
    assert_eq!(
        runtime
            .last_contract_parameters()
            .cloned()
            .unwrap_or(V::Undefined),
        state.get("lastParams"),
        "{name} raw params"
    );
}
fn apply(runtime: &mut SourceContractRuntime, case: &V) -> V {
    let command = case.get("command");
    if let Some(f) = failure(&command) {
        runtime.fail_storage(f).unwrap();
    }
    if let V::Array(args) = command.get("raw") {
        let outcome = match runtime.execute_contract_unverified(&args).unwrap() {
            SourceOutcome::Returned(v) => v,
            SourceOutcome::Thrown(e) => obj(&[
                (
                    "params",
                    runtime
                        .last_contract_parameters()
                        .cloned()
                        .unwrap_or(V::Undefined),
                ),
                (
                    "thrown",
                    obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                ),
            ]),
        };
        obj(&[("outcome", outcome)])
    } else {
        returned(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: text(command.get("target")),
                    arguments: array(command.get("args")),
                })
                .unwrap(),
        )
    }
}
fn projected_effects(effects: Vec<V>) -> V {
    V::Array(effects.into_iter().filter_map(|v|{
        if matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str())){Some(v)}
        else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if ["Logic.exerciseDerivative","Logic.deliverContract","Logic.handleOptionExercise"].contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
        else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",v.get("args")),("returned",v.get("returned"))]))}
        else{None}
    }).collect())
}
#[test]
fn unchanged_final_exercise_graph_both_genuine_numeric_versions() {
    let archived = cases("9.3.1");
    assert_eq!(archived, cases("9.1.2"));
    let mut runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
    let mut calls = 0;
    let mut bare = 0;
    for case in &archived {
        let command = case.get("command");
        let name = text(case.get("name"));
        if command.get("nodeOnly") == V::Bool(true) {
            bare += 1;
            assert_eq!(case.get("result").get("transport"), s("Throw"));
            assert_eq!(
                case.get("result").get("outcome").get("thrown").get("name"),
                s("TypeError")
            );
            assert_eq!(
                case.get("result")
                    .get("outcome")
                    .get("thrown")
                    .get("message"),
                s(if name == "bare-delivery-global-this" {
                    "this.deliverContract is not a function"
                } else {
                    "this.handleOptionExercise is not a function"
                })
            );
            continue;
        }
        if let V::Object(_) = command.get("reset") {
            runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
            let options = command.get("reset");
            runtime
                .set_entry(if let V::String(ref v) = options.get("entry") {
                    v
                } else {
                    "main-subgraph"
                })
                .unwrap();
            runtime.set_activation_answer(options.get("inactive") != V::Bool(true));
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
        } else if let V::Object(_) = command.get("seed") {
            let seed = command.get("seed");
            runtime
                .import_collection(&text(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
        } else {
            assert_eq!(
                apply(&mut runtime, case),
                case.get("result"),
                "{name} exact source outcome/transport"
            );
            calls += 1;
        }
        assert_state(&runtime, case);
        assert_eq!(
            projected_effects(runtime.take_effects()),
            case.get("effects"),
            "{name} full ordered source function/DB effects"
        );
    }
    assert_eq!(bare, 2);
    assert_eq!(calls, 112);
    assert_eq!(archived.len(), 128);
}
#[test]
fn unawaited_real_margin_suffix_and_rejections_restore_then_continue() {
    let cases = cases("9.3.1");
    for label in [
        "failure-getDatabase",
        "failure-findOne",
        "malformed-json",
        "arg-eval-before-margin",
    ] {
        let start = cases
            .iter()
            .position(|v| v.get("name") == s(label))
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
            .take_while(|v| v.get("command").get("reset") == V::Undefined)
        {
            let command = case.get("command");
            if let V::Object(_) = command.get("seed") {
                let seed = command.get("seed");
                replay
                    .apply(C::Import {
                        collection: text(seed.get("collection")),
                        rows: array(seed.get("rows")),
                    })
                    .unwrap();
            } else {
                if let Some(f) = failure(&command) {
                    replay.apply(C::FailStorage(f)).unwrap();
                }
                replay
                    .apply(C::Invoke(DependencyCall {
                        target: text(command.get("target")),
                        arguments: array(command.get("args")),
                    }))
                    .unwrap();
            }
            assert_state(replay.runtime(), case);
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(restored, replay);
            assert_eq!(restored.checkpoint().unwrap(), bytes);
            replay = restored;
        }
    }
}
