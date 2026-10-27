//! Full pinned IOU and Margin modules execute against the same live DB adapter.
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
fn assert_value(a: &V, b: &V, label: &str) {
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
                for (i, ((ak, av), (bk, bv))) in a.iter().zip(b).enumerate() {
                    if ak != bk {
                        return format!("{p} field{i}: {ak} != {bk}");
                    };
                    if av != bv {
                        return diff(av, bv, format!("{p}.{ak}"));
                    }
                }
                format!("{p} lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    if a != b {
        panic!("{label}: {}", diff(a, b, "$".into()))
    }
}
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,BN,types,logic,Orderbook,effects');code+='\nmodule.exports={environment,tag,BN};\n';const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag,BN}=driver.exports;let e,instances;
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),marginInstances:instances,tally:e.load('tally').addresses};}
function emit(name,command,outcome){console.log(JSON.stringify(norm(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','random'].includes(x.kind))}))));}
async function reset(name){e=environment({});instances=[];const Margin=e.load('marginMap'),original=Margin.getInstance;Margin.getInstance=async function(...args){const instance=await original.apply(this,args);if(instance)instances.push(instance);return instance;};await flush();e.effects.length=0;emit(name,{reset:true},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}let outcome;try{const value=await e.load('iou')[method](...args);outcome={returned:method==='_db'?{sourceDatabase:'iou'}:value};}catch(error){outcome={thrown:{name:error.name,message:error.message}}};await flush();emit(name,{method,args,failure},outcome);}
const bucket=(loss=1,extra={})=>({_id:'1:7',contractId:1,propertyId:7,amount:'-10',blockStartAmount:'-11',blockDelta:'-2',blockLosses:loss,blockProfits:'3',lastBlock:10,...extra});
const claims=(values={buyer:1,seller:2})=>({_id:'claims:1:7',contractId:1,propertyId:7,claims:values,lastBlock:10});
async function seed(name,loss=1,values={buyer:1,seller:2},extra={}){await imported(name,'iou',[bucket(loss,extra),claims(values)]);}
(async()=>{
 await reset('helpers-reset');await call('key-preserves-original-text','key',['01','07']);await call('claim-key-number-coercion','claimKey',['0x1','07']);await call('actual-db-handle','_db');await call('missing-doc','getDoc',[1,7]);await call('missing-value','get',[1,7]);await call('missing-bucket','getBucket',[1,7]);await call('missing-claims','getClaimMap',[1,7]);
 await seed('helper-docs');await call('full-doc','getDoc',[1,7]);await call('full-bucket-big-number-fields','getBucket',[1,7]);await call('full-value','get',[1,7]);await call('block-reduction','blockReductionTowardZero',[bucket(1,{blockStartAmount:'-12',amount:'-10'})]);await call('no-block-reduction','blockReductionTowardZero',[bucket(1,{blockStartAmount:1,amount:2})]);
 await call('plain-payout','payOutstandingIous',[1,7,.1,10]);await call('repeated-source-payout','payOutstandingIous',[1,7,.1,10]);await call('read-bucket-still-not-debited','getDoc',[1,7]);await call('payout-does-not-credit-tally','getClaimMap',[1,7]);
 await call('save-full-claim-doc','saveClaimMap',[claims({z:'2',a:'3'})]);await call('stale-block-skips','payOutstandingIous',[1,7,1,11]);await call('zero-skips-before-db','payOutstandingIous',[1,7,0,10]);await call('negative-skips-before-db','payOutstandingIous',[1,7,-1,10]);
 await seed('numeric-address-entries',3,{'10':'1','2':'2',buyer:'3'});await call('numeric-address-order','payOutstandingIous',[1,7,1,10]);
 await seed('string-numeric-block',1,{buyer:1},{lastBlock:'10'});await call('last-block-number-coercion','payOutstandingIous',['1','7',1,10]);
 await seed('negative-losses',-1);await call('negative-losses-empty','payOutstandingIous',[1,7,1,10]);await seed('empty-claims',1,{});await call('empty-claims-empty','payOutstandingIous',[1,7,1,10]);
 await seed('nonpositive-claims',1,{a:0,b:-1,c:'bad'});await call('nonpositive-claims-empty','payOutstandingIous',[1,7,1,10]);
 await seed('nan-delta',1,{buyer:1});await call('nan-big-number-propagates','payOutstandingIous',[1,7,new BN(NaN),10]);
 for(const [name,collection,op,skip]of [['bucket-factory','iou','getDatabase',0],['bucket-read','iou','findOne',0],['claim-read','iou','findOne',1],['audit-insert','iou','insert',0],['item-audit','iou','insert',1],['claim-save','iou','update',0],['margin-factory','marginMaps','getDatabase',0],['margin-read-caught','marginMaps','findOne',0]]){
  await reset('failure-'+name);await seed('seed-'+name);await call('failed-'+name,'payOutstandingIous',[1,7,.1,10],{collection,op,skip,message:'failure-'+name});await call('retry-'+name,'payOutstandingIous',[1,7,.1,10]);
 }
 await reset('legacy-helper-reset');await seed('legacy-helper-docs');await call('positive-delta-routes-add-loss','addDelta',[1,7,.1,10]);await call('negative-delta-routes-add-profit','addDelta',[1,7,-.2,10]);await call('zero-delta-null','addDelta',[1,7,0,10]);await call('losers-plain-mark-type-error','applyToLosers',[1,1,10,7]);await call('losers-retains-source-negative-route','applyToLosers',[1,new BN(.3),10,7]);
 await call('audit-full-field-override','audit',[{block:10,contractId:1,propertyId:7,_id:'chosen',type:'custom',ts:12,event:'test'}]);await call('audit-insert-caught','audit',[{block:10,contractId:1,propertyId:7,event:'catch'}],{collection:'iou',op:'insert',message:'audit-insert'});await call('audit-null-fails-after-factory','audit',[null]);
 await call('getDoc-read-caught','getDoc',[1,7],{collection:'iou',op:'findOne',message:'doc-read'});await call('getDoc-factory-not-caught','getDoc',[1,7],{collection:'iou',op:'getDatabase',message:'doc-factory'});await call('get-read-caught','get',[1,7],{collection:'iou',op:'findOne',message:'value-read'});
 await imported('total-docs','iou',[bucket(1,{amount:'2'}),{_id:'2:7',propertyId:'7',amount:'3'},{_id:'a',type:'audit',propertyId:7,amount:'100'},{_id:'b',propertyId:8,amount:'100'}]);await call('total-all-nonaudit-buckets','getTotalForProperty',['7']);await call('total-read-caught-zero-big-number','getTotalForProperty',[7],{collection:'iou',op:'find',message:'total-read'});await call('delete-one-bucket','delete',[1,7]);await call('delete-property-strict-number','deleteForProperty',[7]);await call('total-string-property-remains','getTotalForProperty',[7]);
 await imported('negative-total-doc','iou',[bucket(1,{amount:'-2'})]);await call('negative-total-number-zero','getTotalForProperty',[7]);await imported('nan-total-doc','iou',[bucket(1,{amount:'bad'})]);await call('nan-total-number-zero','getTotalForProperty',[7]);
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
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
#[test]
fn genuine_iou_allocation_helpers_rows_audits_live_margin_reads_and_retries() {
    let mut runtime = SourceContractRuntime::new();
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        let actual = if command.get("reset") == V::Bool(true) {
            runtime = SourceContractRuntime::new();
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
            let f = command.get("failure");
            if matches!(f, V::Object(_)) {
                let op = text(f.get("op"));
                runtime
                    .fail_storage(StorageFailure {
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
                    })
                    .unwrap()
            };
            observed(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: format!("PnlIou.{}", text(command.get("method"))),
                        arguments: array(command.get("args")),
                    })
                    .unwrap_or_else(|e| panic!("{name} local error {e:?}")),
            )
        };
        assert_value(
            &actual,
            &case.get("outcome"),
            &format!("{name} complete outcome"),
        );
        let expected = case.get("state");
        let mut names = std::collections::BTreeSet::new();
        for table in array(expected.get("tables")) {
            let collection = text(table.get("name"));
            names.insert(collection.clone());
            assert_value(
                &V::Array(runtime.collection(&collection).to_vec()),
                &table.get("rows"),
                &format!("{name} full {collection}"),
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
        if !matches!(expected.get("tally"), V::Undefined) {
            assert_value(
                &runtime.tally_cache(),
                &expected.get("tally"),
                &format!("{name} Tally"),
            )
        };
        let expected_instances = array(expected.get("marginInstances"));
        let actual_instances = array(runtime.margins().state_value().unwrap().get("instances"));
        assert_eq!(
            actual_instances.len(),
            expected_instances.len(),
            "{name} actual Margin instances"
        );
        for (i, expected) in expected_instances.iter().enumerate() {
            assert_value(
                &runtime.margins().instance_value(i as u64 + 1).unwrap(),
                expected,
                &format!("{name} full Margin instance{i}"),
            )
        }
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","random"].contains(&k.as_str()))).collect();
        assert_value(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} actual ordered DB/audit effects"),
        );
        count += 1;
    }
    assert_eq!(count, 89, "genuine IOU observations");
}

#[test]
fn iou_audit_prefix_and_failed_claim_save_replay_before_retry() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy;
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::GlobalClearingV2).unwrap();
    let mut active = false;
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if command.get("reset") == V::Bool(true) {
            active = matches!(
                name.as_str(),
                "failure-claim-save" | "failure-margin-factory"
            );
            if !active {
                continue;
            };
            replay =
                SourceContractReplay::with_policy(SourceContractRuntimePolicy::GlobalClearingV2)
                    .unwrap();
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
            };
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
                };
                replay
                    .apply(C::Invoke(DependencyCall {
                        target: format!("PnlIou.{}", text(command.get("method"))),
                        arguments: array(command.get("args")),
                    }))
                    .unwrap()
                    .outcome
            };
            let value = match actual {
                O::Void => obj(&[("returned", V::Undefined)]),
                O::Dependency(v) => observed(v),
                other => panic!("{name}: {other:?}"),
            };
            assert_value(
                &value,
                &case.get("outcome"),
                &format!("{name} journal outcome"),
            );
        }
        let before = replay.runtime().state_value().unwrap();
        let retained_head = replay.head();
        let bytes = replay.checkpoint().unwrap();
        replay = SourceContractReplay::from_checkpoint(&bytes, retained_head).unwrap();
        assert_value(
            &replay.runtime().state_value().unwrap(),
            &before,
            &format!("{name} full actual state after replay"),
        );
        assert!(
            replay.checkpoint().unwrap() == bytes,
            "{name} canonical bytes"
        );
        count += 1;
    }
    assert_eq!(count, 8);
}
