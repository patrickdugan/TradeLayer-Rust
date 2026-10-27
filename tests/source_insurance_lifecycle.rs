//! Genuine unchanged insurance/Tally modules supply expected observations only.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn text(v: V) -> String {
    let V::String(v) = v else {
        panic!("string expected")
    };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("array expected")
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
fn observed(v: DependencyReturn) -> V {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn assert_value(actual: &V, expected: &V, label: &str) {
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
                for (i, ((ak, av), (bk, bv))) in a.iter().zip(b).enumerate() {
                    if ak != bk {
                        return format!("{path} field{i} {ak} != {bk}");
                    };
                    if av != bv {
                        return diff(av, bv, format!("{path}.{ak}"));
                    }
                }
                format!("{path} lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{path}: {a:?} != {b:?}"),
        }
    }
    if actual != expected {
        panic!("{label}: {}", diff(actual, expected, "$".into()))
    }
}
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,BN,types,logic,Orderbook,effects');
// This is the declared DB adapter, not a source method. Node uses the genuine
// RegExp meaning; Rust explicitly bounds constructed patterns to literal IDs.
code=code.replace("if(op==='$ne')","if(op==='$regex')return new RegExp(wanted).test(actual);if(op==='$ne')");
code+='\nmodule.exports={environment,tag,BN};\n';const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag,BN}=driver.exports;let e;
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),insurance:e.load('insurance').instances,tally:e.load('tally').addresses};}
function emit(name,command,outcome){const tagged=tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','rpc-clock','uuid'].includes(x.kind))});
 // The ordinary observer uses BN.toString(), which hides negative zero. Keep
 // the actual input constructor sign in command arguments; returned BN text
 // remains the literal source diagnostic.
 if(command.args){const args=tagged[1].find(([k])=>k==='command')[1][1].find(([k])=>k==='args')[1][1];command.args.forEach((v,i)=>{if(BN.isBigNumber(v)&&v.isZero()&&v.isNegative())args[i]=['bignumber','-0'];});}
 console.log(JSON.stringify(norm(tagged)));}
async function reset(name,scene={}){e=environment(scene);await flush();e.effects.length=0;emit(name,{reset:true,entry:scene.preload==='tally'?'tally-first':'main-subgraph'},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(failure){const original=e.storage.getDatabase;let skip=failure.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===failure.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(failure.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,method,args=[],key=null,failure){e.effects.length=0;e.setHeight(77);if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}
 let outcome;try{const Fund=e.load('insurance');const value=key===null?await Fund[method](...args):await Fund.instances.get(key)[method](...args);outcome={returned:method==='getInstance'?{sourceInsuranceInstance:`${args[0]}${args[1]?'-oracle':''}`}:value};}catch(error){outcome={thrown:{name:error.name,message:error.message}}}await flush();
 emit(name,{method,args,key,failure},outcome);
}
const instance=(name,id,oracle=false,failure)=>call(name,'getInstance',[id,oracle],null,failure);
const seeded=(key,balances,extra={})=>({_id:`seed-${key}`,key,value:{balances,contractSeriesId:key,hedgeRatio:0,...extra}});
const balance=(propertyId,amountAvailable)=>({propertyId,amountAvailable});
(async()=>{
 await reset('map-order-reset');await instance('map-two-first',2);await instance('map-one-second',1);await instance('map-existing-two-keeps-place',2);
 await reset('basic-reset');await instance('base-instance',1);await instance('same-key-string-instance','1');await instance('truthy-oracle-original-value',1,'false');await instance('null-id',null);await instance('undefined-id',undefined);
 await call('deposit-string','deposit',[7,'100.000000009',10],'1');await call('base-snapshot','getSnapshot',[],'1');await call('oracle-getSnapshot-wrong-unsuffixed','getSnapshot',[],'1-oracle');
 await call('strict-property-number-misses','withdraw',[20,7,11],'1');await call('lexical-string-insufficient','withdraw',['20','7',11],'1');await call('numeric-withdraw','withdraw',[20,'7',11],'1');await call('negative-withdraw-increases','withdraw',[-5,'7',12],'1');
 await call('undefined-event-time','recordEvent',['custom',{keep:[false,null]}],'1');await call('default-snapshot','saveSnapshot',[],'1');await call('explicit-snapshot-still-queries-clock','saveSnapshot',[{balances:[balance(9,3)],custom:true,block:123}],'1');await call('get-explicit-snapshot','getSnapshot',[],'1');await call('reload-external','loadFromSnapshot',[],'1');
 for(const amount of [NaN,Infinity,-Infinity,'bad',true,[],0,-1])await call('ignored-deposit-'+String(amount),'deposit',[9,amount,13],'1');
 await call('tiny-round-down','deposit',[9,'0.000000019',13],'1');

 for(const [name,loss]of [['ordinary-number',3],['ordinary-string','3'],['null',null],['undefined',undefined]])await call('loss-must-have-abs-'+name,'calcPayout',[loss,20],'1');
 await call('positive-payout','calcPayout',[new BN(1),20],'1');await call('negative-loss-sign-retained','calcPayout',[new BN('-0.5'),21],'1');await call('cap-payout','calcPayout',[new BN(99),22],'1');await call('payout-history','getPayouts',[1,20,22],'1');await call('payout-history-reversed','getPayouts',[1,22,20],'1');
 await call('property-total','getTotalBalanceForProperty',[9]);await call('summary','updateInsuranceSummary');await call('legacy-fund-balance-empty','getInsuranceFundBalance',[9]);
 await imported('legacy-fund-balance-row','insurance',[{_id:9,amount:'1.234567899'}]);await call('legacy-fund-balance','getInsuranceFundBalance',[9]);

 for(const [name,balances,loss]of [
  ['cross-property',[balance(7,2),balance('8','3')],new BN(4)],
  ['negative-total-deficit',[balance(7,-20),balance(8,5)],new BN(1)],
  ['nan-balance',[balance(7,'bad'),balance(8,5)],new BN(1)],
  ['nan-loss',[balance(7,5)],new BN(NaN)],
  ['infinite-balance',[balance(7,'Infinity')],new BN(2)],
  ['infinite-loss',[balance(7,5)],new BN(Infinity)],
  ['negative-zero',[balance(7,5)],new BN('-0')],
  ['rounding',[balance(7,'1.000000015')],new BN('0.000000005')]
 ]){await reset(name+'-reset');await imported(name+'-seed','insurance',[seeded('1',balances)]);await instance(name+'-instance',1);await call(name+'-payout','calcPayout',[loss,30],'1');await call(name+'-history','getPayouts',[1,0,100],'1');}

 for(const [method,collection,op,skip]of [['withdraw','insurance','insert',0],['withdraw','insurance','update',0],['calcPayout','insurance','insert',0],['calcPayout','insurance','insert',1],['calcPayout','insurance','update',0]]){
  const name=method+'-'+op+'-'+skip;await reset('failure-'+name);await imported('seed-'+name,'insurance',[seeded('1',[balance('7',10)])]);await instance('instance-'+name,1);
  const args=method==='withdraw'?[2,'7',40]:[new BN(2),40];await call('failed-'+name,method,args,'1',{collection,op,skip,message:'fail-'+name});await call('retry-'+name,method,args,'1');
 }
 await reset('load-failure-reset');await instance('failed-load',1,false,{collection:'insurance',op:'findOne',message:'initial-load'});await instance('retry-load',1);await call('deposit-load-failure','deposit',[7,1,50],'1',{collection:'insurance',op:'findOne',message:'deposit-load'});await call('deposit-load-retry','deposit',[7,1,50],'1');
 await reset('summary-reset');await imported('summary-all-rows','insurance',[seeded('1',[balance(7,'1.000000005'),balance('7',2),balance(2,-1),balance(3,'-0.000000001')]),seeded('2',[balance('7','0x10')]),{key:'event',value:{type:'event'}},{_id:7,amount:'123'}]);await call('total-merges-string-number','getTotalBalanceForProperty',[7]);await call('summary-rounding-and-order','updateInsuranceSummary');await call('total-reads-all-documents','getTotalBalanceForProperty',['7']);
 await imported('prototype-property','insurance',[seeded('1',[balance('toString',1)])]);await call('source-object-prototype-defect','updateInsuranceSummary');

 for(const [name,balances,oracle]of [['numeric',[balance(7,10),balance(8,20)],false],['strings',[balance(7,'10'),balance(8,'20')],false],['oracle-wrong-base',[balance(7,'10'),balance(8,'20')],true],['zero-number',[balance(7,0)],false],['zero-string',[balance(7,'0')],false]]){
  await reset('liquidate-'+name);await imported('liquidate-seed-'+name,'insurance',[seeded('1',balances,{block:55}),seeded('1-oracle',[balance(9,999)],{block:56})]);await call('liquidate-'+name,'liquidate',['admin',1,oracle]);await call('liquidate-retry-'+name,'liquidate',['admin',1,oracle]);
 }
 await reset('missing-snapshot-reset');await call('liquidate-caught-missing-snapshot','liquidate',['admin',1,false]);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn reference() -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR")])
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
                .unwrap_or_else(|e| panic!("diagnostic {e:?}"))
        })
        .collect()
}
#[test]
fn full_source_insurance_lifecycle_shared_rows_cache_tally_and_failure_retries() {
    let mut runtime = SourceContractRuntime::new();
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        let actual = if command.get("reset") == V::Bool(true) {
            runtime = SourceContractRuntime::new();
            runtime.set_entry(&text(command.get("entry"))).unwrap();
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap()
            }
            obj(&[("returned", V::Undefined)])
        } else if matches!(command.get("seed"), V::Object(_)) {
            let seed = command.get("seed");
            runtime
                .import_collection(&text(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
            obj(&[("returned", V::Undefined)])
        } else {
            runtime.set_height(V::number(77.)).unwrap();
            let failure = command.get("failure");
            if matches!(failure, V::Object(_)) {
                let op = text(failure.get("op"));
                runtime
                    .fail_storage(StorageFailure {
                        collection: text(failure.get("collection")),
                        operation: if op == "getDatabase" {
                            op
                        } else {
                            format!("{op}Async")
                        },
                        skip: if let V::Number(bits) = failure.get("skip") {
                            f64::from_bits(bits) as usize
                        } else {
                            0
                        },
                        message: text(failure.get("message")),
                    })
                    .unwrap()
            };
            let mut arguments = array(command.get("args"));
            if !matches!(command.get("key"), V::Null) {
                arguments.insert(0, obj(&[("sourceInsuranceInstance", command.get("key"))]))
            };
            observed(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: format!("Insurance.{}", text(command.get("method"))),
                        arguments,
                    })
                    .unwrap_or_else(|e| panic!("{name}: {e:?}")),
            )
        };
        assert_value(&actual, &case.get("outcome"), &format!("{name} outcome"));
        let expected = case.get("state");
        let mut names = std::collections::BTreeSet::new();
        for table in array(expected.get("tables")) {
            let collection = text(table.get("name"));
            names.insert(collection.clone());
            assert_value(
                &V::Array(runtime.collection(&collection).to_vec()),
                &table.get("rows"),
                &format!("{name} complete {collection}"),
            )
        }
        let state = runtime.state_value().unwrap();
        for table in array(state.get("tables")) {
            let collection = text(table.get("name"));
            if !names.contains(&collection) {
                assert_value(
                    &table.get("rows"),
                    &V::Array(vec![]),
                    &format!("{name} extra {collection}"),
                )
            }
        }
        let V::Object(instances) = state.get("insurance") else {
            panic!()
        };
        let cache = V::Array(
            instances
                .into_iter()
                .map(|(k, v)| V::Array(vec![s(&k), v]))
                .collect(),
        );
        assert_value(
            &cache,
            &expected.get("insurance"),
            &format!("{name} complete singleton cache"),
        );
        if !matches!(expected.get("tally"), V::Undefined) {
            assert_value(
                &runtime.tally_cache(),
                &expected.get("tally"),
                &format!("{name} actual Tally cache"),
            );
        }
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","rpc-clock","uuid"].contains(&k.as_str()))).collect();
        assert_value(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered actual effects"),
        );
        runtime = runtime.clone();
        count += 1;
    }
    assert_eq!(count, 144, "genuine lifecycle observations");
}

#[test]
fn actual_failed_withdrawal_and_payout_retry_replay_from_independently_retained_heads() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy;
    let policy = SourceContractRuntimePolicy::GlobalClearingV2;
    let mut replay = SourceContractReplay::with_policy(policy).unwrap();
    let mut active = false;
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if command.get("reset") == V::Bool(true) {
            active = matches!(
                name.as_str(),
                "failure-withdraw-update-0" | "failure-calcPayout-insert-1"
            );
            if !active {
                continue;
            };
            replay = SourceContractReplay::with_policy(policy).unwrap();
            for table in array(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
        } else {
            if !active {
                continue;
            }
            let actual = if matches!(command.get("seed"), V::Object(_)) {
                let seed = command.get("seed");
                replay
                    .apply(C::Import {
                        collection: text(seed.get("collection")),
                        rows: array(seed.get("rows")),
                    })
                    .unwrap()
                    .outcome
            } else {
                replay.apply(C::SetHeight(V::number(77.))).unwrap();
                let f = command.get("failure");
                if matches!(f, V::Object(_)) {
                    let op = text(f.get("op"));
                    replay
                        .apply(C::FailStorage(StorageFailure {
                            collection: text(f.get("collection")),
                            operation: if op == "getDatabase" {
                                op
                            } else {
                                format!("{op}Async")
                            },
                            skip: if let V::Number(bits) = f.get("skip") {
                                f64::from_bits(bits) as usize
                            } else {
                                0
                            },
                            message: text(f.get("message")),
                        }))
                        .unwrap();
                }
                let mut args = array(command.get("args"));
                if !matches!(command.get("key"), V::Null) {
                    args.insert(0, obj(&[("sourceInsuranceInstance", command.get("key"))]))
                }
                replay
                    .apply(C::Invoke(DependencyCall {
                        target: format!("Insurance.{}", text(command.get("method"))),
                        arguments: args,
                    }))
                    .unwrap()
                    .outcome
            };
            let value = match actual {
                O::Void => obj(&[("returned", V::Undefined)]),
                O::Dependency(value) => observed(value),
                other => panic!("{name}: unexpected journal outcome {other:?}"),
            };
            assert_value(
                &value,
                &case.get("outcome"),
                &format!("{name} replayed source outcome"),
            );
        }
        let before = replay.runtime().state_value().unwrap();
        let retained_head = replay.head();
        let bytes = replay.checkpoint().unwrap();
        replay = SourceContractReplay::from_checkpoint(&bytes, retained_head).unwrap();
        assert_value(
            &replay.runtime().state_value().unwrap(),
            &before,
            &format!("{name} complete current owner after raw command replay"),
        );
        assert_eq!(replay.runtime().policy(), policy);
        assert!(
            replay.checkpoint().unwrap() == bytes,
            "{name} canonical bytes"
        );
        count += 1;
    }
    assert_eq!(count, 10);
}
