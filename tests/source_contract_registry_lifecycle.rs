//! Complete unchanged CommonJS definitions; source outcomes never answer reads.
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
};
fn s(value: &str) -> V {
    V::String(value.into())
}
fn text(value: V) -> String {
    let V::String(v) = value else {
        panic!("string {value:?}")
    };
    v
}
fn array(value: V) -> Vec<V> {
    let V::Array(v) = value else {
        panic!("array {value:?}")
    };
    v
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn outcome(value: DependencyReturn) -> V {
    match value {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn source_outcome(value: SourceOutcome) -> V {
    match value {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm');
const root=process.argv[1],version=process.argv[2],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));
let BN;
if(version==='9.1.2'){
 const archive=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8'),packed=archive.match(/const BIGNUMBER_GZIP="([^"]+)"/)[1];
 const source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));
 if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine9.1.2 hash');
 const module={exports:{}};vm.runInNewContext(source.toString('utf8'),{module,exports:module.exports},{timeout:1000});BN=module.exports;numeric.loadBigNumber=()=>BN;
}else{if(version!=='9.3.1')throw Error('test version');BN=numeric.loadBigNumber();}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';
const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT';let e,lastParams;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const C=e.load('contractRegistry'),O=e.load('oracle'),T=e.load('tally');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),registryInitialized:!!C.instance,seriesInitialized:!!C.instance?.contractSeries,series:C.instance?.contractSeries??new Map(),oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,oracleWatermarks:O.lastOracleUpdateBlock,tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,lastParams};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock','activation'].includes(v.kind)).map(v=>v.kind==='activation'?{kind:'dependency',target:'Activation.'+v.method,args:v.args,returned:v.returned}:v)})));}
async function reset(name,options={}){e=m.exports.environment(options.entry==='tally-first'?{preload:'tally'}:options.entry==='volume-first'?{preload:'volumeIndex'}:{entry:options.entry??'main-subgraph'});lastParams=undefined;
 if(options.empty)e.table('contractList').splice(0);if(options.nested)e.table('oracleList')[0].name={ticker:'NESTED'};
 await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args=[],extra={}){e.effects.length=0;if(extra.failure)fail(extra.failure);const original=structuredClone(args);let outcome;
 try{const[ns,method]=target.split('.'),names={Contract:'contractRegistry',Property:'property',Volume:'volumeIndex',VolumeIndex:'volumeIndex',Oracle:'oracle',Logic:null};const owner=ns==='Logic'?e.logic:e.load(names[ns]);const actual=args.map((v,i)=>extra.maps?.includes(i)?new Map(v):v);outcome={returned:await owner[method](...actual)};}catch(error){outcome={thrown:{name:error.name,message:error.message}};}
 await flush();emit(name,{target,args:original,...extra},outcome);}
async function raw(name,type,payload,extra={}){const args=[name,type,'tl',payload,A,null,0,0,10];e.effects.length=0;lastParams=undefined;if(extra.failure)fail(extra.failure);let outcome;
 try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={returned:{params:lastParams,returned}};}catch(error){outcome={thrown:{name:error.name,message:error.message}};}
 await flush();emit(name,{raw:args,...extra},outcome);}
const native=(period=24,count=3,inverse=false)=>[A,true,1,[[1,7]],1,2,7,10,period,count,inverse,false,10];
const oracle=(period=24,count=3)=>[A,false,1,[],0,2,7,10,period,count,false,false,10];
const params={native:true,underlyingOracleId:0,onChainData:[[1,7]],notionalPropertyId:1,notionalValue:2,collateralPropertyId:7,leverage:10,expiryPeriod:24,series:3,inverse:false,fee:false};
(async()=>{
 for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
  await reset('definition-'+entry,{entry});await call(entry+'-native-dated','Logic.createContractSeries',native());
  await call(entry+'-native-child-resolves-parent','Contract.getContractInfo',['2-34']);await call(entry+'-series-query','Contract.getContractSeries',[2]);await call(entry+'-series-string-id-is-strict','Contract.getContractSeries',['2']);
  await call(entry+'-hydrated-amm','Contract.getAMM',[2]);await call(entry+'-native-type','Contract.getContractType',[2]);await call(entry+'-collateral-filter','Contract.getAllContractsForCollateral',['unused',7]);
  await call(entry+'-flat-oracle-first-throw','Logic.createContractSeries',oracle());
  await call(entry+'-load-actual-cache','Contract.loadContractSeries',[]);await call(entry+'-forward-match-ignores-notional','Contract.isDuplicateNativeContract',[7,[1,7],999]);
  await call(entry+'-reverse-match-requires-notional','Contract.isDuplicateNativeContract',[7,[7,1],999]);await call(entry+'-validator-nested-pair-does-not-match','Contract.isDuplicateNativeContract',[7,[[1,7]],1]);
  await raw(entry+'-raw-native-keeps-decoder-empty-pairs',16,'1,0,1:7,1,2,7,10,o,3,0,0');await raw(entry+'-raw-oracle-keeps-flat-ticker-throw',16,'0,1,,0,2,7,10,o,3,0,0');
  await reset('nested-'+entry,{entry,nested:true});await raw(entry+'-raw-oracle-dated',16,'0,1,,0,2,7,10,o,3,0,0');await call(entry+'-oracle-perpetual','Logic.createContractSeries',oracle(0,2));
  await call(entry+'-native-inverse-second-collateral-series','Logic.createContractSeries',native(12,2,true));await call(entry+'-inverse-native-selection','Contract.lookupInverseNativeByNotionalPid',[7]);
  await call(entry+'-dated-tickers-pass-perpetual-filter','Contract.getAllPerpContracts',[]);await call(entry+'-constructor-contract-list-stays-empty','Contract.isValidSeriesId',[2]);await call(entry+'-save-constructor-empty-maps','Contract.saveAllData',[]);
 }
 await reset('empty-duplicate-cache',{empty:true});await call('cache-empty-map','Contract.isDuplicateNativeContract',[7,[1,7],1]);await call('create-does-not-refresh-empty-cache','Logic.createContractSeries',native());await call('stale-empty-map-duplicate-false','Contract.isDuplicateNativeContract',[7,[1,7],1]);await call('explicit-load-refreshes-map','Contract.loadContractSeries',[]);await call('refreshed-duplicate-true','Contract.isDuplicateNativeContract',[7,[1,7],1]);
 for(const[collection,op,skip]of [['contractList','getDatabase',0],['contractList','find',0],['propertyList','findOne',0],['contractList','getDatabase',1],['contractList','update',0],['contractList','update',1]]){
  await reset('create-failure-'+collection+'-'+op+'-'+skip);await call('create-prefix-'+collection+'-'+op+'-'+skip,'Contract.createContractSeries',[A,params,10],{failure:{collection,op,skip,message:'definition-'+collection+'-'+op+'-'+skip}});await call('create-retry-'+collection+'-'+op+'-'+skip,'Contract.createContractSeries',[A,params,10]);
 }
 await reset('duplicate-load-failure',{empty:true});await call('duplicate-catches-failed-load','Contract.isDuplicateNativeContract',[7,[1,7],1],{failure:{collection:'contractList',op:'find',message:'duplicate-read'}});await call('duplicate-retries-uninitialized-cache','Contract.isDuplicateNativeContract',[7,[1,7],1]);
 await reset('identifier-and-generation-domains',{empty:true});
 for(const[name,map]of [['mixed',[[1,{}],['0x10',{}],['12tail',{}],['bad',{}],[-4,{}]]],['negative-zero',[[-0,{}],[0,{}]]],['large',[[9007199254740992,{}]]]])await call('next-map-'+name,'Contract.getNextIdFromMap',[map],{maps:[0]});
 for(const[period,count,id,block]of [[0,3,1,10],[-2,3,1,10],[2,0,1,10],[2,-1,1,10],['0x10',2,'S','12tail'],[null,2,1,10],[3,2.2,1,10],[2,2,9007199254740992,9007199254740992]])await call('generated-'+String(period)+'-'+String(count)+'-'+String(id),'Contract.generateContracts',[period,count,id,block]);
 await imported('malformed-db-id-poisons-effective-next-id','contractList',[{_id:'malformed-id-row',id:'bad',type:'contractSeries',data:{id:'bad'}}]);await call('effective-next-id-is-db-method-not-shadowed-singleton','Contract.getNextId',[]);await call('create-map-id-ignores-nan-key','Contract.createContractSeries',[A,{...params,expiryPeriod:0,series:2},10]);
 await call('absent-contract-type-source-error','Contract.getContractType',[999]);await call('absent-amm-source-error','Contract.getAMM',[999]);
 await reset('fresh-property-reads');await call('property-number-key','Property.getPropertyData',['7']);await imported('changed-property-json','propertyList',[{_id:'propertyIndex',value:JSON.stringify([[7,{ticker:'NEW'}],['s-7-1',{ticker:'SYNTH'}],[0,{ticker:'ZERO'}]])}]);
 for(const id of [7,'s-7-1',false,'absent',null])await call('property-id-'+String(id),'Property.getPropertyData',[id]);
 await imported('malformed-property-json','propertyList',[{_id:'propertyIndex',value:'{invalid'}]);await call('property-catches-malformed-json','Property.getPropertyData',[7]);
 for(const op of ['getDatabase','findOne'])await call('property-catches-'+op,'Property.getPropertyData',[7],{failure:{collection:'propertyList',op,message:'property-'+op}});
 await reset('literal-last-price');await imported('latest-volume-is-not-height-filtered','volumeIndex',[{_id:'1-7',value:{price:'3',blockHeight:100}},{_id:'9-7',value:{price:'0.000000003',blockHeight:100}}]);
 for(const[pair,height]of [['1-7',1],['7-1',1],['7-9',1],['1-7-extra',1],['nohyphen',10],[undefined,10],['missing-pair',10]])await call('last-price-'+String(pair),'Volume.getLastPrice',[pair,height]);
 for(const op of ['getDatabase','findOne'])await call('last-price-catches-'+op,'Volume.getLastPrice',['1-7',1],{failure:{collection:'volumeIndex',op,message:'volume-'+op}});
 await reset('native-ltc-special-case');await call('native-ltc-ticker-after-property-read','Contract.createContractSeries',[A,{...params,notionalPropertyId:0},10]);
 await reset('direct-save-map-order',{empty:true});await imported('other-type-id-collision','contractList',[{_id:'old-kind',id:1,type:'other',data:{old:true}}]);await call('save-id-query-overwrites-other-type','Contract.saveDataToDb',[[[1,{id:1,ticker:'A'}],[2,{id:2,ticker:'B'}]],'contractSeries'],{maps:[0]});
 await call('save-partial-write-retained','Contract.saveDataToDb',[[[1,{id:1,ticker:'CHANGED'}],[3,{id:3,ticker:'C'}]],'contractSeries'],{maps:[0],failure:{collection:'contractList',op:'update',skip:1,message:'save-second-row'}});
 await call('save-partial-write-retry','Contract.saveDataToDb',[[[1,{id:1,ticker:'CHANGED'}],[3,{id:3,ticker:'C'}]],'contractSeries'],{maps:[0]});
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn compare(actual: &V, expected: &V, label: &str) {
    if actual == expected {
        return;
    }
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
                format!("{p} field counts {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    panic!("{label}: {}", diff(actual, expected, "$".into()));
}
fn cases(version: &str) -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR"), version])
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
fn install_failure(runtime: &mut SourceContractRuntime, command: &V) {
    let f = command.get("failure");
    if matches!(f, V::Undefined) {
        return;
    }
    let op = text(f.get("op"));
    runtime
        .fail_storage(StorageFailure {
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
        .unwrap();
}
#[test]
fn unchanged_registry_lifecycle_complete_graph_both_genuine_bignumber_versions() {
    let archived = cases("9.3.1");
    let locked = cases("9.1.2");
    compare(
        &V::Array(archived.clone()),
        &V::Array(locked),
        "independent tested9.1.2 equality",
    );
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3);
    let mut calls = 0;
    for case in &archived {
        let name = text(case.get("name"));
        let command = case.get("command");
        if !matches!(command.get("reset"), V::Undefined) {
            runtime =
                SourceContractRuntime::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3);
            let entry = command.get("reset").get("entry");
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
        } else if !matches!(command.get("seed"), V::Undefined) {
            let seed = command.get("seed");
            runtime
                .import_collection(&text(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
        } else {
            install_failure(&mut runtime, &command);
            let actual = if !matches!(command.get("raw"), V::Undefined) {
                source_outcome(
                    runtime
                        .execute_contract_unverified(&array(command.get("raw")))
                        .unwrap_or_else(|e| panic!("{name} local {e:?}")),
                )
            } else {
                outcome(
                    runtime
                        .invoke_dependency(&DependencyCall {
                            target: text(command.get("target")),
                            arguments: array(command.get("args")),
                        })
                        .unwrap_or_else(|e| panic!("{name} local {e:?}")),
                )
            };
            compare(&actual, &case.get("outcome"), &format!("{name} outcome"));
            calls += 1;
        }
        let expected = case.get("state");
        let actual = runtime.state_value().unwrap();
        let mut collections = std::collections::BTreeSet::new();
        for table in array(expected.get("tables")) {
            let collection = text(table.get("name"));
            collections.insert(collection.clone());
            compare(
                &V::Array(runtime.collection(&collection).to_vec()),
                &table.get("rows"),
                &format!("{name} persisted {collection}"),
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
        assert!(
            runtime.collection("orderBooks").is_empty(),
            "{name} invented orderbook rows"
        );
        compare(
            &runtime.tally_cache(),
            &expected.get("tally"),
            &format!("{name} sole Tally cache"),
        );
        compare(
            &actual.get("globalClearing").get("tallyStaticInitialized"),
            &expected.get("tallyInitialized"),
            &format!("{name} Tally initialized"),
        );
        for field in ["registryInitialized", "seriesInitialized"] {
            compare(
                &actual.get("registryLifecycle").get(field),
                &expected.get(field),
                &format!("{name} {field}"),
            );
        }
        compare(
            &actual.get("globalClearing").get("contractSeries"),
            &expected.get("series"),
            &format!("{name} actual series cache"),
        );
        compare(
            &actual.get("oracleSourceCache"),
            &expected.get("oracleCache"),
            &format!("{name} Oracle cache"),
        );
        compare(
            &actual.get("oracleSourceInitialized"),
            &expected.get("oracleInitialized"),
            &format!("{name} Oracle initialized"),
        );
        compare(
            &actual.get("oracleSourceWatermarks"),
            &expected.get("oracleWatermarks"),
            &format!("{name} Oracle watermarks"),
        );
        compare(
            &runtime
                .last_contract_parameters()
                .cloned()
                .unwrap_or(V::Undefined),
            &expected.get("lastParams"),
            &format!("{name} raw parameters"),
        );
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str())) || matches!(v.get("target"),V::String(target)if ["Activation.isTxTypeActive","Activation.getAdmin"].contains(&target.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered storage/dependency effects"),
        );
    }
    assert_eq!(archived.len(), 166);
    assert_eq!(calls, 140);
}

#[test]
fn actual_definition_partial_writes_replay_then_retry() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    let cases = cases("9.3.1");
    for (start_name, length) in [
        ("create-failure-contractList-update-1", 3),
        ("direct-save-map-order", 5),
    ] {
        let start = cases
            .iter()
            .position(|v| v.get("name") == s(start_name))
            .unwrap();
        let mut replay =
            SourceContractReplay::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3)
                .unwrap();
        for table in array(cases[start].get("state").get("tables")) {
            replay
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        let mut thrown = 0;
        for case in &cases[start + 1..start + length] {
            let command = case.get("command");
            if !matches!(command.get("seed"), V::Undefined) {
                let seed = command.get("seed");
                replay
                    .apply(C::Import {
                        collection: text(seed.get("collection")),
                        rows: array(seed.get("rows")),
                    })
                    .unwrap();
            } else {
                let failure = command.get("failure");
                if !matches!(failure, V::Undefined) {
                    replay
                        .apply(C::FailStorage(StorageFailure {
                            collection: text(failure.get("collection")),
                            operation: format!("{}Async", text(failure.get("op"))),
                            skip: match failure.get("skip") {
                                V::Number(v) => f64::from_bits(v) as usize,
                                _ => 0,
                            },
                            message: text(failure.get("message")),
                        }))
                        .unwrap();
                }
                let receipt = replay
                    .apply(C::Invoke(DependencyCall {
                        target: text(command.get("target")),
                        arguments: array(command.get("args")),
                    }))
                    .unwrap();
                let O::Dependency(actual) = receipt.outcome else {
                    panic!("dependency receipt")
                };
                if matches!(
                    actual,
                    DependencyReturn::PendingThrow(_) | DependencyReturn::Throw(_)
                ) {
                    thrown += 1;
                }
                compare(
                    &outcome(actual),
                    &case.get("outcome"),
                    "genuine definition receipt",
                );
            }
            for table in array(case.get("state").get("tables")) {
                compare(
                    &V::Array(
                        replay
                            .runtime()
                            .collection(&text(table.get("name")))
                            .to_vec(),
                    ),
                    &table.get("rows"),
                    "genuine partial-write/retry documents",
                );
            }
            let checkpoint = replay.checkpoint().unwrap();
            let restored =
                SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
            assert_eq!(restored, replay);
            assert_eq!(restored.checkpoint().unwrap(), checkpoint);
            replay = restored;
        }
        assert_eq!(thrown, 1);
    }
}
