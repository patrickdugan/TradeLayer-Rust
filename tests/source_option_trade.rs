//! Whole unchanged type27/Logic/Margin graph under genuine numeric dependencies.
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn text(v: V) -> String {
    let V::String(v) = v else {
        panic!("string {v:?}")
    };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("array {v:?}")
    };
    v
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn observed(v: DependencyReturn) -> V {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn source(v: SourceOutcome) -> V {
    match v {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm');
const root=process.argv[1],version=process.argv[2],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN;
if(version==='9.1.2'){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine9.1.2');const module={exports:{}};vm.runInNewContext(source.toString('utf8'),{module,exports:module.exports},{timeout:1000});BN=module.exports;numeric.loadBigNumber=()=>BN;}else{if(version!=='9.3.1')throw Error('version');BN=numeric.loadBigNumber();}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='option-channel',TK='1-100-C-100';let e,instances,lastParams;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),O=e.load('oracle'),Ch=e.load('channels'),Con=e.load('contractRegistry');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,contractMod:Con.modFlag,instances:instances.map(v=>({seriesId:v.seriesId,margins:[...v.margins],expiryIndex:[...v.expiryIndex],tickerExpiry:[...v.tickerExpiry],optionOI:[...v.optionOI]})),channels:[...Ch.channelsRegistry],oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,lastParams};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock'].includes(v.kind))})));}
async function reset(name,options={}){e=m.exports.environment(options.entry==='tally-first'?{preload:'tally',funds:options.funds??1000}:options.entry==='volume-first'?{preload:'volumeIndex',funds:options.funds??1000}:{entry:options.entry??'main-subgraph',funds:options.funds??1000});instances=[];lastParams=undefined;
 const M=e.load('marginMap'),load=M.loadMarginMap;M.loadMarginMap=function(...args){const promise=load.apply(this,args);promise.then(value=>{if(value&&value.margins&&!instances.includes(value))instances.push(value);},()=>{});return promise;};
 e.table('channels').splice(0,Infinity,{_id:C,data:{participants:{A,B},A:{7:options.channelFunds??1000},B:{7:options.channelFunds??1000}}});if(options.noSeries)e.table('contractList').splice(0);
 await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],extra={}){e.effects.length=0;if(extra.failure)fail(extra.failure);const original=structuredClone(args);let outcome;try{const[ns,method]=target.split('.');let owner,actual=args;if(ns==='Logic')owner=e.logic;else if(ns==='Margin'&&method!=='getInstance'){owner=instances[args[0].sourceMarginInstance-1];actual=args.slice(1);}else owner=e.load({Margin:'marginMap',Validity:'validity',Tally:'tally',Channels:'channels'}[ns]);let returned=await owner[method](...actual);if(ns==='Margin'&&method==='getInstance'&&returned?.margins)returned={sourceMarginInstance:instances.indexOf(returned)+1};outcome={returned};}catch(error){outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args:original,...extra},outcome);}
async function raw(name,payload,extra={}){e.effects.length=0;lastParams=undefined;if(extra.failure)fail(extra.failure);const args=[name,27,'tl',payload,C,null,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(27,lastParams);outcome={returned:{params:lastParams,returned}};}catch(error){outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,...extra},outcome);}
const p=(extra={})=>({contractId:TK,price:2,amount:1,columnAIsSeller:true,block:10,...extra});
const maprow=positions=>[{_id:JSON.stringify({seriesId:1}),value:JSON.stringify(positions)}];
(async()=>{
 for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
  await reset('ordinary-'+entry,{entry});await call(entry+'-ordinary-option','Logic.processOptionTrade',[C,p(),'ordinary']);await call(entry+'-option-reduce','Logic.processOptionTrade',[C,p({columnAIsSeller:false,price:3}),'reduce']);await call(entry+'-option-flip-open','Logic.processOptionTrade',[C,p({amount:1}),'flip-open']);await call(entry+'-option-flips-both-sides','Logic.processOptionTrade',[C,p({columnAIsSeller:false,price:4,amount:3}),'flip']);
  await reset('raw-'+entry,{entry});await raw(entry+'-actual-raw-option',TK+',2,1,1,0,0');await raw(entry+'-raw-option-combo',TK+',5,1,1,0,0,1-100-C-120,2,1');
 }
 for(const[comboTicker,comboPrice]of [['1-100-P-80',1],['1-100-C-120',3]]){await reset('option-package-'+comboTicker+'-'+comboPrice);await call('real-option-package','Logic.processOptionTrade',[C,p({comboTicker,comboPrice,comboAmount:1}),'package']);}
 await reset('primitive-and-invalid-parameters');for(const value of [undefined,null,5,'bad',{},p({contractId:'bad'}),p({contractId:'Infinity-100-C-100'}),p({comboTicker:'1-PERP',comboAmount:1}),p({contractId:'1-5-C-100'})])await call('malformed-'+String(value?.contractId??value),'Logic.processOptionTrade',[C,value,'invalid']);
 await raw('raw-invalid-primary','bad,2,1,1,0,0');await raw('raw-invalid-combo',TK+',2,1,1,0,0,1-PERP,1,1');
 await reset('source-channel-credit-does-not-create-tally-funds',{funds:0});await call('actual-premium-debit-fails-after-validity','Logic.processOptionTrade',[C,p(),'unfunded']);
 await reset('missing-option-series',{noSeries:true});await call('actual-missing-series-invalid','Logic.processOptionTrade',[C,p(),'missing']);
 await reset('missing-channel');await imported('delete-actual-channel','channels',[]);await call('actual-missing-channel-invalid','Logic.processOptionTrade',[C,p(),'missing-channel']);
 for(const[collection,op,skip]of [['channels','getDatabase',0],['channels','find',0],['tallyMap','findOne',0],['marginMaps','getDatabase',0],['marginMaps','findOne',0],['marginMaps','findOne',1],['tallyMapDelta','insert',0],['tallyMapDelta','insert',1],['tallyMapDelta','insert',2],['marginMapDelta','getDatabase',0],['marginMapDelta','findOne',0],['marginMapDelta','insert',0],['marginMapDelta','insert',1],['marginMaps','update',0]]){
  const label=collection+'-'+op+'-'+skip;await reset('option-failure-'+label);await call('source-prefix-'+label,'Logic.processOptionTrade',[C,p(),'failure'],{failure:{collection,op,skip,message:'option-'+label}});await call('source-retry-'+label,'Logic.processOptionTrade',[C,p(),'retry']);
 }
 await reset('raw-second-validation-failure');await raw('raw-revalidates-and-first-instance-not-reused',TK+',2,1,1,0,0',{failure:{collection:'marginMaps',op:'findOne',skip:1,message:'second-validation'}});
 await reset('raw-execution-receiver-failure');await raw('raw-premiums-before-undefined-receiver',TK+',2,1,1,0,0',{failure:{collection:'marginMaps',op:'findOne',skip:2,message:'execution-receiver'}});
 await reset('direct-option-position-math');await call('real-margin-instance','Margin.getInstance',[1]);const h={sourceMarginInstance:1};
 for(const[quantity,price,credit]of [[4,10,5],[-1,20,0],[-5,30,-2],[2,15,1],[0,99,0],[null,null,null]])await call('actual-option-position-'+quantity+'-'+price,'Margin.applyOptionTrade',[h,A,TK,quantity,price,10,credit]);
 await call('persist-direct-option-position','Margin.saveMarginMap',[h,10]);await call('actual-wrong-touch-argument-order','Margin._touchExpiryIndex',[h,h,0,1,TK,{expiryBlock:100}]);
 await reset('existing-options-string-malformed');await imported('import-real-malformed-position','marginMaps',maprow([[A,{address:A,contracts:0,margin:0,options:'bad'}]]));await call('premium-margin-before-malformed-options-throw','Logic.processOptionTrade',[C,p(),'malformed-options']);
 await reset('expiry-unwind-policy');await imported('import-long-wing-and-open-short','marginMaps',maprow([[A,{address:A,contracts:0,margin:0,options:{[TK]:{contracts:1,avgPrice:1,margin:0},'1-100-C-120':{contracts:-1,avgPrice:1,margin:10}}}]]));await call('source-forbids-single-leg-unwind','Logic.processOptionTrade',[C,p(),'unwind']);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn compare(a: &V, b: &V, label: &str) {
    fn diff(a: &V, b: &V, p: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return diff(a, b, format!("{p}[{i}]"));
                    }
                }
                format!("{p} lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{p} fields {ak} != {bk}");
                    }
                    if av != bv {
                        return diff(av, bv, format!("{p}.{ak}"));
                    }
                }
                format!("{p} fields {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    if a != b {
        panic!("{label}: {}", diff(a, b, "$".into()));
    }
}
fn cases(version: &str) -> Vec<V> {
    let r = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR"), version])
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    std::str::from_utf8(&r.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn failure(command: &V) -> Option<StorageFailure> {
    let f = command.get("failure");
    if matches!(f, V::Undefined) {
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
        skip: match f.get("skip") {
            V::Number(v) => f64::from_bits(v) as usize,
            _ => 0,
        },
        message: text(f.get("message")),
    })
}
#[test]
fn complete_unchanged_option_graph_both_genuine_numeric_versions() {
    let cases = cases("9.3.1");
    compare(
        &V::Array(cases.clone()),
        &V::Array(self::cases("9.1.2")),
        "genuine9.1.2 independently tested equality",
    );
    let mut runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
    let mut calls = 0;
    for case in &cases {
        let name = text(case.get("name"));
        let c = case.get("command");
        if !matches!(c.get("reset"), V::Undefined) {
            runtime = SourceContractRuntime::with_policy(P::OptionLifecycleV4);
            let entry = c.get("reset").get("entry");
            runtime
                .set_entry(if let V::String(ref v) = entry {
                    v
                } else {
                    "main-subgraph"
                })
                .unwrap();
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
        } else if !matches!(c.get("seed"), V::Undefined) {
            let seed = c.get("seed");
            runtime
                .import_collection(&text(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
        } else {
            if let Some(f) = failure(&c) {
                runtime.fail_storage(f).unwrap();
            }
            let actual = if !matches!(c.get("raw"), V::Undefined) {
                source(
                    runtime
                        .execute_contract_unverified(&array(c.get("raw")))
                        .unwrap_or_else(|e| panic!("{name} local {e:?}")),
                )
            } else {
                let returned = runtime
                    .invoke_dependency(&DependencyCall {
                        target: text(c.get("target")),
                        arguments: array(c.get("args")),
                    })
                    .unwrap_or_else(|e| panic!("{name} local {e:?}"));
                if c.get("target") == s("Margin._touchExpiryIndex") {
                    assert!(
                        matches!(
                            returned,
                            DependencyReturn::Immediate(_) | DependencyReturn::Throw(_)
                        ),
                        "{name} source sync method"
                    );
                } else {
                    assert!(
                        matches!(
                            returned,
                            DependencyReturn::Pending(_) | DependencyReturn::PendingThrow(_)
                        ),
                        "{name} source async method"
                    );
                }
                observed(returned)
            };
            compare(&actual, &case.get("outcome"), &format!("{name} outcome"));
            calls += 1;
        }
        let state = case.get("state");
        let actual = runtime.state_value().unwrap();
        let mut collections = std::collections::BTreeSet::new();
        for table in array(state.get("tables")) {
            let name = text(table.get("name"));
            collections.insert(name.clone());
            compare(
                &V::Array(runtime.collection(&name).to_vec()),
                &table.get("rows"),
                &format!("{name} persisted {name}"),
            );
        }
        for table in array(actual.get("tables")) {
            if !collections.contains(&text(table.get("name"))) {
                assert_eq!(
                    table.get("rows"),
                    V::Array(vec![]),
                    "{name} unexpected collection"
                );
            }
        }
        compare(
            &runtime.tally_cache(),
            &state.get("tally"),
            &format!("{name} soleTallycache"),
        );
        let global = actual.get("globalClearing");
        compare(
            &global.get("tallyStaticInitialized"),
            &state.get("tallyInitialized"),
            &format!("{name} Tallyinitialized"),
        );
        compare(
            &global.get("tallyMod"),
            &state.get("tallyMod"),
            &format!("{name} Tallyflag"),
        );
        compare(
            &global.get("contractModRaw"),
            &state.get("contractMod"),
            &format!("{name} Contractflag"),
        );
        compare(
            &global.get("channels").get("registry"),
            &state.get("channels"),
            &format!("{name} realChannelscache"),
        );
        let expected = array(state.get("instances"));
        let actual_instances = array(runtime.margins().state_value().unwrap().get("instances"));
        assert_eq!(
            actual_instances.len(),
            expected.len(),
            "{name} realfreshMargininstances"
        );
        for (i, expected) in expected.iter().enumerate() {
            compare(
                &runtime.margins().instance_value(i as u64 + 1).unwrap(),
                expected,
                &format!("{name} Margininstance{i}"),
            );
        }
        compare(
            &actual.get("oracleSourceCache"),
            &state.get("oracleCache"),
            &format!("{name} Oraclecache"),
        );
        compare(
            &actual.get("oracleSourceInitialized"),
            &state.get("oracleInitialized"),
            &format!("{name} Oracleinitialized"),
        );
        compare(
            &runtime
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined),
            &state.get("lastParams"),
            &format!("{name} actualrawparams"),
        );
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} orderedactualDB/UUIDeffects"),
        );
    }
    assert_eq!(cases.len(), 117);
    assert_eq!(calls, 81);
}
#[test]
fn option_premium_margin_and_position_prefix_restore_then_actual_retry() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    let cases = cases("9.3.1");
    for label in [
        "option-failure-marginMaps-findOne-1",
        "option-failure-marginMapDelta-insert-1",
        "option-failure-marginMaps-update-0",
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
        let mut thrown = 0;
        for case in &cases[start + 1..start + 3] {
            let c = case.get("command");
            if let Some(f) = failure(&c) {
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay
                .apply(C::Invoke(DependencyCall {
                    target: text(c.get("target")),
                    arguments: array(c.get("args")),
                }))
                .unwrap();
            let O::Dependency(outcome) = receipt.outcome else {
                panic!("dependency receipt")
            };
            if matches!(
                outcome,
                DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)
            ) {
                thrown += 1;
            }
            compare(
                &observed(outcome),
                &case.get("outcome"),
                "actualsourceprefix receipt",
            );
            for table in array(case.get("state").get("tables")) {
                compare(
                    &V::Array(
                        replay
                            .runtime()
                            .collection(&text(table.get("name")))
                            .to_vec(),
                    ),
                    &table.get("rows"),
                    "actualpremium/margin/position prefix documents",
                );
            }
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(restored, replay);
            assert_eq!(restored.checkpoint().unwrap(), bytes);
            replay = restored;
        }
        assert_eq!(thrown, 1);
    }
}
