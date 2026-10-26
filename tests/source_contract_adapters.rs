//! Expectations execute unchanged pinned financial modules in the composed
//! CommonJS realm. Only storage/clock/UUID/random remain declared boundaries.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};

fn text(v: V) -> String {
    let V::String(v) = v else {
        panic!("string: {v:?}")
    };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("array: {v:?}")
    };
    v
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
fn observed(v: DependencyReturn) -> V {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[
                ("name", V::String(e.name)),
                ("message", V::String(e.message)),
            ]),
        )]),
    }
}

const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
// Expose the existing declared storage boundary and its persisted rows. No
// financial module or source function is edited, stubbed or supplied a result.
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');
code+='\nmodule.exports={environment,tag};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag}=driver.exports;let e;let count=0;
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:[...(e.load('tally').addresses??new Map())]};}
function emit(name,command,outcome){console.log(JSON.stringify(norm(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))}))));count++;}
async function reset(name,seed){e=environment({});if(seed)seed(e);await flush();e.effects.length=0;emit(name,{reset:true},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(failure){const original=e.storage.getDatabase;let skip=failure.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===failure.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(failure.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args,extra={}){
 e.effects.length=0;if(extra.failure){if(extra.failure.op==='getDatabase')factoryFailure(extra.failure);else e.setFailure(extra.failure);}
 const namespace=target.split('.')[0],method=target.slice(namespace.length+1),modules={Tally:'tally',Contract:'contractRegistry',PnlIou:'iou',VolumeIndex:'volumeIndex'};
 let outcome;try{outcome={returned:await e.load(modules[namespace])[method](...args)}}catch(error){outcome={thrown:{name:error.name,message:error.message}}}
 await flush();emit(name,{target,args,...extra},outcome);
}
(async()=>{
 await reset('tally-cached-load-reset');let actor=JSON.parse(e.table('tallyMap')[0].data)[0][0];
 await call('tally-prime-cache','Tally.getTally',[actor,7]);
 await imported('tally-import-malformed','tallyMap',[{_id:'tallyMap',data:'{bad'}]);
 await call('tally-malformed-load-return-is-undefined','Tally.loadFromDB',[]);
 await call('tally-malformed-get-retains-cached-balance','Tally.getTally',[actor,7]);
 await call('tally-malformed-still-sufficient','Tally.hasSufficientBalance',[actor,7,100]);
 await imported('tally-import-invalid-map-entry','tallyMap',[{_id:'tallyMap',data:'[null]'}]);
 await call('tally-destructure-failure-return-is-undefined','Tally.loadFromDB',[]);
 await call('tally-destructure-failure-keeps-cache','Tally.getTally',[actor,7]);
 await imported('tally-valid-empty-map','tallyMap',[{_id:'tallyMap',data:'[]'}]);
 await call('tally-valid-load-returns-actual-empty-map','Tally.loadFromDB',[]);
 await call('tally-empty-get-is-zero','Tally.getTally',[actor,7]);

 const volumeRows=[{_id:'globalCumulativeVolume',value:{globalCumulativeVolume:2000,block:8}},{_id:'contractCumulativeVolume',value:{contractCumulativeVolume:4000,block:8}},{_id:'ltcPairCumulativeVolume',value:{ltcPairTotalVolume:8000,block:8}}];
 await reset('reward-failures-reset',e=>e.table('volumeIndex').push(...structuredClone(volumeRows)));
 for(let skip=0;skip<3;skip++)await call('reward-cumulative-read-failure-'+skip,'VolumeIndex.baselineLiquidityReward',[1,.01,7,{block:9}],{failure:{collection:'volumeIndex',op:'find',skip,message:'cumulative-read-'+skip}});
 await call('reward-cumulative-factory-failure','VolumeIndex.baselineLiquidityReward',[1,.01,7,{block:9}],{failure:{collection:'volumeIndex',op:'getDatabase',message:'cumulative-factory'}});
 await imported('reward-import-unsorted-history','volumeIndex',[
  {_id:'globalCumulativeVolume',value:{globalCumulativeVolume:1,block:1}},
  {_id:'globalCumulativeVolume',value:{globalCumulativeVolume:1000000,block:8}},
  {_id:'contractCumulativeVolume',value:{contractCumulativeVolume:0,block:2}},
  {_id:'ltcPairCumulativeVolume',value:{ltcPairTotalVolume:0,block:2}},
  {_id:'ltcPairCumulativeVolume',value:{ltcPairTotalVolume:100,block:7}}
 ]);
 await call('reward-cumulative-latest-row-sorting','VolumeIndex.calculateLiquidityReward',[1,7,{block:9,feePaid:.01}]);

 await reset('iou-thirds-reset');
 await call('iou-third-claim-strings-and-audit','PnlIou.addIouClaims',[1,7,10,'buyer','seller',1,2,.1]);
 await call('iou-third-existing-claim-strings-and-audit','PnlIou.addIouClaims',[1,7,10,'buyer','seller',1,2,.1]);

 await reset('tally-coercion-reset');actor=JSON.parse(e.table('tallyMap')[0].data)[0][0];
 await imported('tally-string-balances','tallyMap',[{_id:'tallyMap',data:JSON.stringify([[actor,{7:{amount:27,available:'9',reserved:'9',margin:'9',vesting:0,channelBalance:0}}]])}]);
 await call('sufficiency-two-strings-is-lexicographic','Tally.hasSufficientBalance',[actor,7,'10']);
 await call('sufficiency-string-and-number-is-numeric','Tally.hasSufficientBalance',[actor,7,10]);
 await call('sufficiency-lexicographic-failure-shortfall-is-numeric','Tally.hasSufficientBalance',[actor,7,'99']);
 await call('reserve-two-strings-is-lexicographic','Tally.hasSufficientReserve',[actor,7,'10']);
 await call('margin-two-strings-is-lexicographic','Tally.hasSufficientMargin',[actor,7,'10']);
 await reset('tally-hex-delta-reset');actor=JSON.parse(e.table('tallyMap')[0].data)[0][0];
 await call('tally-hex-string-delta-is-accepted','Tally.updateBalance',[actor,7,'0x10',0,0,0,'hex-change',10,'hex-txid']);

 await reset('boolean-notional-domain-reset',e=>{e.table('contractList')[0].data.notionalValue=true;});
 await call('boolean-notional-is-source-nan-local-unsupported','Contract.getInitialMargin',[1,100],{unsupported:true});
})().catch(error=>{console.error(error);process.exitCode=1});
"###;

fn cases() -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
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
        .map(|line| {
            tradelayer_core::property_catalog::parse_diagnostic_value(line)
                .unwrap_or_else(|e| panic!("source adapter diagnostic {e:?}: {line}"))
        })
        .collect()
}

fn assert_state(runtime: &SourceContractRuntime, expected: &V, name: &str) {
    let mut names = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let table_name = text(table.get("name"));
        names.insert(table_name.clone());
        assert_eq!(
            V::Array(runtime.collection(&table_name).to_vec()),
            table.get("rows"),
            "{name}: complete persisted {table_name}"
        );
    }
    for table in array(runtime.state_value().unwrap().get("tables")) {
        let table_name = text(table.get("name"));
        if !names.contains(&table_name) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name}: extra table {table_name}"
            );
        }
    }
    assert_eq!(
        runtime.tally_cache(),
        expected.get("tally"),
        "{name}: Tally cache"
    );
}

#[test]
fn genuine_source_adapter_catches_rounding_coercion_and_ordered_storage_effects() {
    let mut runtime = SourceContractRuntime::new();
    let mut actions = 0;
    for case in cases() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if command.get("unsupported") == V::Bool(true) {
            continue;
        }
        if command.get("reset") == V::Bool(true) {
            runtime = SourceContractRuntime::new();
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
            if let V::Object(_) = command.get("failure") {
                let f = command.get("failure");
                let operation = text(f.get("op"));
                let skip = match f.get("skip") {
                    V::Number(bits) => f64::from_bits(bits) as usize,
                    _ => 0,
                };
                runtime
                    .fail_storage(StorageFailure {
                        collection: text(f.get("collection")),
                        operation: if operation == "getDatabase" {
                            operation
                        } else {
                            format!("{operation}Async")
                        },
                        skip,
                        message: text(f.get("message")),
                    })
                    .unwrap();
            }
            let actual = runtime
                .invoke_dependency(&DependencyCall {
                    target: text(command.get("target")),
                    arguments: array(command.get("args")),
                })
                .unwrap_or_else(|error| panic!("{name}: unexpected local error {error:?}"));
            assert_eq!(
                observed(actual),
                case.get("outcome"),
                "{name}: complete source outcome"
            );
            actions += 1;
        }
        assert_state(&runtime, &case.get("state"), &name);
        let effects = runtime.take_effects().into_iter().filter(|v| matches!(v.get("kind"), V::String(k) if ["database", "db", "uuid", "random", "rpc-clock"].contains(&k.as_str()))).collect::<Vec<_>>();
        assert_eq!(
            V::Array(effects),
            case.get("effects"),
            "{name}: ordered source effects"
        );
    }
    assert_eq!(actions, 21);
}

#[test]
fn genuine_source_boolean_notional_nan_is_an_explicit_local_domain_error_with_rollback() {
    let cases = cases();
    let reset = cases
        .iter()
        .find(|v| v.get("name") == V::String("boolean-notional-domain-reset".into()))
        .unwrap();
    let case = cases
        .iter()
        .find(|v| v.get("command").get("unsupported") == V::Bool(true))
        .unwrap();
    let returned = case.get("outcome").get("returned");
    let V::Number(bits) = returned else {
        panic!("genuine source must return Number NaN")
    };
    assert!(f64::from_bits(bits).is_nan());
    let mut runtime = SourceContractRuntime::new();
    for table in array(reset.get("state").get("tables")) {
        runtime
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
    let before = runtime.clone();
    let command = case.get("command");
    assert_eq!(
        runtime.invoke_dependency(&DependencyCall {
            target: text(command.get("target")),
            arguments: array(command.get("args"))
        }),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert_eq!(
        runtime, before,
        "local numeric-domain rejection rolls back adapter state/effects"
    );
}
