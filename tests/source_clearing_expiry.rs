//! Full unchanged CommonJS execution supplies option expiry expectations only.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};

fn s(value: &str) -> V {
    V::String(value.into())
}
fn array(value: V) -> Vec<V> {
    let V::Array(a) = value else {
        panic!("array {value:?}")
    };
    a
}
fn text(value: V) -> String {
    let V::String(s) = value else {
        panic!("string {value:?}")
    };
    s
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

const EXPECTED_ACTIONS: usize = 94;
const EXPECTED_CALLS: usize = 77;
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('composed environment changed');
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';
const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();let e,instances,contextKeys;
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='funding-third';
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return['array',[...v].map(([k,v])=>['array',[tag(k),tag(v)]])];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function balance(value){return{amount:value,available:value,reserved:0,margin:0,vesting:0,channelBalance:0};}
function seed(options={}){
 const meta=e.table('contractList')[0].data;
 meta.contracts={unexpired:[{id:'future-30',expirationBlock:30},{id:20,expirationBlock:20},{id:999,expirationBlock:20},{id:9,expirationBlock:9}]};
 const tally=JSON.parse(e.table('tallyMap')[0].data);
 for(const[,properties]of tally)Object.assign(properties[7],{amount:1000,available:900,margin:options.shortfall?0:100});
 e.table('tallyMap')[0].data=JSON.stringify(tally);
 const p=(address,contracts,opts)=>({address,contracts,margin:10,unrealizedPNL:0,avgPrice:100,lastMark:100,options:opts,extra:{retained:true}});
 let positions=[[A,p(A,1,{
 '1-10-C-90':{contracts:2,margin:4},'1-10-P-110':{contracts:1,margin:3},
 '1-10-C-130':{contracts:1,margin:2},'1-20-C-80':{contracts:1,margin:1},
 '1-10-C':{contracts:0,margin:5},bad:{contracts:1,margin:8}})],
 [B,p(B,-1,{'1-10-C-90':{contracts:-2,margin:4},'1-10-P-110':{contracts:-1,margin:3},'1-10-P-90':{contracts:-1,margin:2}})]];
 if(options.onlyOtm)positions=[[A,p(A,1,{'1-10-C-130':{contracts:1,margin:2}})]];
 if(options.empty)positions=[];
 if(options.zero)positions=[[A,p(A,1,{'1-10-C-90':{contracts:0,margin:9}})]];
 if(options.stringSeries)meta.contracts.unexpired=[{id:'string-future',expirationBlock:20}];
 e.table('marginMaps').push({_id:JSON.stringify({seriesId:options.stringSeries?'1':1}),value:JSON.stringify(positions)});
}
function state(){return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:[...(e.load('tally').addresses??new Map())],margins:instances.map(i=>({seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]})),contexts:contextKeys.map(key=>[key,e.load('clearing').getPositionsFromCache(key)])};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))})));}
async function reset(name,options={}){e=m.exports.environment(options.entry==='tally-first'?{preload:'tally'}:options.entry==='volume-first'?{preload:'volumeIndex'}:{entry:options.entry??'main-subgraph'});instances=[];contextKeys=[];
 const Margin=e.load('marginMap'),original=Margin.loadMarginMap;Margin.loadMarginMap=async function(...args){const result=await original.apply(this,args);if(result&&result.margins&&!instances.includes(result))instances.push(result);return result;};
 seed(options);await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args,extra={}){e.effects.length=0;if(extra.failure){if(extra.failure.op==='getDatabase')factoryFailure(extra.failure);else e.setFailure(extra.failure);}let result;const original=args.slice();
 if(extra.object)args[0]=e.load('clearing').getPositionsFromCache(extra.object.ctx)[extra.object.index];
 try{const ns=target.split('.')[0],method=target.slice(ns.length+1);const value=await e.load(ns==='Clearing'?'clearing':'contractRegistry')[method](...args);result={returned:value};if(method==='initPositionCache'&&!contextKeys.includes(value))contextKeys.push(value);}catch(error){result={thrown:{name:error.name,message:error.message}}}
 await flush();emit(name,{target,args:original,...extra},result);}
(async()=>{
 for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
  await reset('expiry-'+entry,{entry});
  await call(entry+'-before-expiry','Clearing.settleOptionExpiries',[1,9,100,144,'tx-before']);
  await call(entry+'-full-itm-otm','Clearing.settleOptionExpiries',[1,10,100,144,'tx-expire']);
  await call(entry+'-retry-no-duplicate-custody','Clearing.settleOptionExpiries',[1,10,100,144,'tx-retry']);
  await call(entry+'-later-expiry','Clearing.settleOptionExpiries',[1,20,100,144,'tx-later']);
 }
 await reset('expiry-zero-qty',{zero:true});await call('zero-qty-disappears-with-held-margin','Clearing.settleOptionExpiries',[1,10,100]);
 await reset('expiry-no-positions',{empty:true});await call('no-options-returns-before-save','Clearing.settleOptionExpiries',[1,10,100]);
 await reset('expiry-string-series',{stringSeries:true});await call('string-series-retains-document-key','Clearing.settleOptionExpiries',['1',10,100,144,'tx-string']);
 for(const[collection,op,skip,onlyOtm]of [['marginMaps','getDatabase',0,false],['marginMaps','findOne',0,false],['contractList','findOne',0,false],['marginMapDelta','insert',0,false],['marginMapDelta','insert',1,false],['tallyMapDelta','insert',0,true],['marginMaps','update',0,false]]){
  await reset('expiry-failure-'+collection+'-'+op+'-'+skip,{onlyOtm});
  await call('retained-'+collection+'-'+op+'-'+skip,'Clearing.settleOptionExpiries',[1,10,100,144,'tx-failed'],{failure:{collection,op,skip,message:'expiry-'+collection+'-'+op+'-'+skip}});
  await call('retry-'+collection+'-'+op+'-'+skip,'Clearing.settleOptionExpiries',[1,10,100,144,'tx-retry']);
 }
 await reset('expiry-insufficient-margin',{onlyOtm:true,shortfall:true});await call('actual-tally-rejection-preserves-option','Clearing.settleOptionExpiries',[1,10,100,144,'tx-shortfall']);
 await reset('expiry-missing-definition');await imported('remove-definition','contractList',[]);await call('missing-definition-retains-loaded-instance','Clearing.settleOptionExpiries',[1,10,100]);
 for(const[series,info,expiry]of [[1,null,10],['0x10',{},10],[1,{contracts:{unexpired:'bad'}},10],[1,{contracts:{unexpired:[{id:'first',expirationBlock:'20'},{id:'tie',expirationBlock:20},{id:'early',expirationBlock:5},null]}},10],[1,{contracts:{unexpired:[{expirationBlock:20},{id:'later',expirationBlock:30}]}},10],[1,{contracts:{unexpired:[{id:5,expirationBlock:20}]}},Infinity]])await call('resolve-'+String(series)+'-'+String(expiry),'Clearing.resolveOptionExerciseTarget',[series,info,expiry]);
 for(const type of ['Call','Put','other'])for(const qty of [0,-0,2,-2,NaN,Infinity,-Infinity,'0x10'])await call('exercise-'+type+'-'+String(qty),'Clearing.computeExerciseContracts',[{type},qty]);
 await call('exercise-zero-before-null-meta','Clearing.computeExerciseContracts',[null,0]);await call('exercise-null-meta-throws','Clearing.computeExerciseContracts',[null,1]);
 const p=(contracts,avgPrice=100)=>({address:'helper',contracts,margin:10,avgPrice,unrealizedPNL:0,lastMark:undefined,extra:{keep:true}});
 await call('init-direct-helper-objects','Clearing.initPositionCache',[1,99,[p(2),p(-2),p(1),p(-1),p(0),null,p('bad')]]);
 for(const[index,delta,price,margin]of [[0,1,110,2],[0,-1,120,0],[0,-4,90,3],[1,3,105,1],[2,-1,90,2],[3,1,100,1],[4,1,100,3],[5,0,100,0],[6,NaN,100,1]])await call('assign-direct-'+index+'-'+delta,'Clearing.applyAssignedUnderlyingPosition',[null,delta,price,margin],{object:{ctx:'1:99',index}});
})().catch(error=>{console.error(error);process.exitCode=1});

"###;
fn compare_value(actual: &V, expected: &V, name: &str) {
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
                format!("{p}: lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{p}: fields {ak} != {bk}");
                    };
                    if av != bv {
                        return diff(av, bv, format!("{p}.{ak}"));
                    }
                }
                format!("{p}: field lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    panic!("{name}: {}", diff(actual, expected, "$".into()));
}

#[test]
fn unchanged_source_expiry_graph_drives_actual_options_margin_tally_and_failed_prefixes() {
    let result = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut runtime = SourceContractRuntime::new();
    let mut actions = 0;
    let mut calls = 0;
    for line in std::str::from_utf8(&result.stdout).unwrap().lines() {
        let case = tradelayer_core::property_catalog::parse_diagnostic_value(line)
            .unwrap_or_else(|e| panic!("diagnostic {e:?}: {}", &line[..line.len().min(200)]));
        let name = text(case.get("name"));
        let command = case.get("command");
        if !matches!(command.get("reset"), V::Undefined) {
            runtime = SourceContractRuntime::new();
            let entry = command.get("reset").get("entry");
            runtime
                .set_entry(if let V::String(ref s) = entry {
                    s
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
            if !matches!(command.get("failure"), V::Undefined) {
                let f = command.get("failure");
                let op = text(f.get("op"));
                runtime
                    .fail_storage(StorageFailure {
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
                    .unwrap();
            }
            let actual = runtime
                .invoke_dependency(&DependencyCall {
                    target: text(command.get("target")),
                    arguments: {
                        let mut args = array(command.get("args"));
                        let reference = command.get("object");
                        if reference != V::Undefined {
                            let contexts = array(
                                runtime
                                    .state_value()
                                    .unwrap()
                                    .get("globalClearing")
                                    .get("contexts"),
                            );
                            let pair = array(
                                contexts
                                    .iter()
                                    .find(|pair| array((*pair).clone())[0] == reference.get("ctx"))
                                    .unwrap()
                                    .clone(),
                            );
                            let ids = array(pair[1].clone());
                            let index = if let V::Number(bits) = reference.get("index") {
                                f64::from_bits(bits) as usize
                            } else {
                                0
                            };
                            args[0] = obj(&[("sourcePositionObject", ids[index].clone())]);
                        }
                        args
                    },
                })
                .unwrap_or_else(|e| panic!("{name}: local {e:?}"));
            compare_value(
                &outcome(actual),
                &case.get("outcome"),
                &format!("{name} outcome"),
            );
            calls += 1;
        }
        let mut names = std::collections::BTreeSet::new();
        for table in array(case.get("state").get("tables")) {
            let collection = text(table.get("name"));
            names.insert(collection.clone());
            compare_value(
                &V::Array(runtime.collection(&collection).to_vec()),
                &table.get("rows"),
                &format!("{name} persisted {collection}"),
            );
        }
        for table in array(runtime.state_value().unwrap().get("tables")) {
            let collection = text(table.get("name"));
            if !names.contains(&collection) {
                assert_eq!(
                    table.get("rows"),
                    V::Array(vec![]),
                    "{name}: unexpected {collection}"
                );
            }
        }
        compare_value(
            &runtime.tally_cache(),
            &case.get("state").get("tally"),
            &format!("{name} Tally cache"),
        );
        let expected_instances = array(case.get("state").get("margins"));
        let actual_instances = array(runtime.margins().state_value().unwrap().get("instances"));
        assert_eq!(
            actual_instances.len(),
            expected_instances.len(),
            "{name} loaded Margin instances"
        );
        for (i, expected) in expected_instances.iter().enumerate() {
            compare_value(
                &runtime.margins().instance_value(i as u64 + 1).unwrap(),
                expected,
                &format!("{name} Margin instance{}", i + 1),
            );
        }
        let contexts = array(
            runtime
                .state_value()
                .unwrap()
                .get("globalClearing")
                .get("contexts"),
        );
        let values = contexts
            .into_iter()
            .map(|pair| {
                let pair = array(pair);
                let ids = array(pair[1].clone());
                V::Array(vec![
                    pair[0].clone(),
                    V::Array(
                        ids.into_iter()
                            .map(|id| {
                                let V::Number(bits) = id else { panic!("ID") };
                                runtime
                                    .margins()
                                    .inspect_position_object(f64::from_bits(bits) as usize)
                                    .unwrap()
                            })
                            .collect(),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        compare_value(
            &V::Array(values),
            &case.get("state").get("contexts"),
            &format!("{name} actual object references"),
        );
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect::<Vec<_>>();
        compare_value(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered effects"),
        );
        actions += 1;
    }
    assert_eq!(actions, EXPECTED_ACTIONS, "expiry source inventory changed");
    assert_eq!(calls, EXPECTED_CALLS, "expiry calls changed");
}

#[test]
fn complete_replay_retains_assignment_then_failed_expiry_delta_and_continues() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    let output = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let cases = std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect::<Vec<_>>();
    let start = cases
        .iter()
        .position(|case| case.get("name") == s("expiry-failure-marginMapDelta-insert-1"))
        .unwrap();
    let mut replay = SourceContractReplay::new().unwrap();
    for table in array(cases[start].get("state").get("tables")) {
        replay
            .apply(C::Import {
                collection: text(table.get("name")),
                rows: array(table.get("rows")),
            })
            .unwrap();
    }
    for (index, case) in cases[start + 1..start + 3].iter().enumerate() {
        let command = case.get("command");
        if index == 0 {
            replay
                .apply(C::FailStorage(StorageFailure {
                    collection: "marginMapDelta".into(),
                    operation: "insertAsync".into(),
                    skip: 1,
                    message: "expiry-marginMapDelta-insert-1".into(),
                }))
                .unwrap();
        }
        let receipt = replay
            .apply(C::Invoke(DependencyCall {
                target: text(command.get("target")),
                arguments: array(command.get("args")),
            }))
            .unwrap();
        if index == 0 {
            assert!(matches!(
                receipt.outcome,
                O::Dependency(DependencyReturn::PendingThrow(_))
            ));
            let live = replay
                .runtime()
                .margins()
                .inspect_position(1, &s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT"))
                .unwrap();
            assert_eq!(live.get("contracts"), V::number(3.0));
            assert_eq!(live.get("margin"), V::number(14.0));
            assert_eq!(live.get("options").get("1-10-C-90"), V::Undefined);
        }
        for table in array(case.get("state").get("tables")) {
            compare_value(
                &V::Array(
                    replay
                        .runtime()
                        .collection(&text(table.get("name")))
                        .to_vec(),
                ),
                &table.get("rows"),
                "expiry restart genuine documents",
            );
        }
        let checkpoint = replay.checkpoint().unwrap();
        let restored = SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), checkpoint);
        replay = restored;
    }
}
