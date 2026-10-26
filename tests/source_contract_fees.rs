//! Actual complete CommonJS source composition supplies expectations only.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn array(v: V) -> Vec<V> {
    let V::Array(a) = v else {
        panic!("array: {v:?}")
    };
    a
}
fn text(v: V) -> String {
    let V::String(a) = v else {
        panic!("string: {v:?}")
    };
    a
}
fn num(v: V) -> usize {
    let V::Number(a) = v else {
        panic!("number: {v:?}")
    };
    f64::from_bits(a) as usize
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
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),{createRequire}=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs'),code=fs.readFileSync(file,'utf8').replace(/^#![^\n]*\n/,'');
const prefix=code.slice(0,code.indexOf('const scenes=['));if(prefix.length<1000)throw Error('composed oracle inventory changed');
const module={exports:{}};new Function('require','module','exports','__dirname','__filename',prefix+'\nmodule.exports={environment,tag};')(createRequire(file),module,module.exports,path.dirname(file),file);
const {environment,tag}=module.exports;let e;
function view(){const state=e.state();return{tables:state.tables,feeBuffer:state.tally.feeAccrualBuffer?[...state.tally.feeAccrualBuffer].map(([block,entries])=>[block,[...entries]]):[],insurance:[...state.insurance]}}
async function action(name,command){let outcome={returned:undefined};if(command.reset){e=environment(command.reset)}else{e.setHeight(command.height??10);e.effects.length=0;try{if(command.failure)e.setFailure(command.failure);else if(command.seed)e.table(command.seed.collection).splice(0,Infinity,...structuredClone(command.seed.rows));else outcome={returned:await e.load('tally')[command.method](...command.args)}}catch(error){outcome={thrown:{name:error.name,message:error.message}}}}
 console.log(JSON.stringify(tag({name,command,outcome,state:view(),effects:e.effects.filter(v=>['database','db','rpc-clock'].includes(v.kind))})));}
async function call(name,method,args,extra={}){return action(name,{method,args,...extra})}
(async()=>{
for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 const scene=entry==='tally-first'?{preload:'tally'}:entry==='volume-first'?{preload:'volumeIndex'}:{entry};
 await action('reset-'+entry,{reset:scene,entry});
 await call(entry+'-queue-first','queueFeeAccrual',[7,0.00025,1,10]);await call(entry+'-queue-second','queueFeeAccrual',[7,0.00025,1,10]);
 await call(entry+'-flush','flushQueuedFeeAccruals',[10]);await call(entry+'-retry','flushQueuedFeeAccruals',[10]);
}
await action('reset-tally-branches',{reset:{preload:'tally'},entry:'tally-first'});
for(let i=0;i<3;i++)await call('spot-dust-'+i,'accrueFee',[7,0.000000015,null,10]);
await call('property-one-odd-sats','accrueFee',[1,0.00000003,null,10]);
await call('oracle-one-sat-zero-deposit','accrueFee',[7,0.00000001,2,10]);
await call('oracle-odd-sats','accrueFee',[7,0.00000003,2,10]);
await call('contract-dust-first','accrueFee',[7,0.000000015,2,10]);await call('contract-dust-rollup','accrueFee',[7,0.000000015,2,10]);
await action('native-definition',{seed:{collection:'contractList',rows:e.table('contractList').map(row=>({...row,data:{...row.data,native:true}}))}});
await call('native-value-route','accrueFee',[7,0.00000003,1,10]);
await call('zero-exits-after-database','accrueFee',[7,0,1,10]);await call('negative-exits-after-database','accrueFee',[7,-1,1,10]);
await action('consensus-heights',{seed:{collection:'consensus',rows:[{_id:'TrackHeight',value:0},{_id:'MaxProcessedHeight',value:11}]}});
await call('resolve-falls-through-zero','queueFeeAccrual',[7,0.0001,1]);await call('flush-resolved-height','flushQueuedFeeAccruals',[]);
await call('update-sixth-block-override','updateFeeCache',[7,0.0001,1,10,'ignored',12]);
await call('queue-ignores-sixth-block','queueFeeAccrual',[7,0.0002,1,10,'ignored',12]);await call('flush-all-numeric-block-sort','flushAllQueuedFeeAccruals',[]);
for(const [collection,op,skip]of [['insurance','insert',0],['insurance','update',0],['feeCache','update',0],['contractList','findOne',0]]){
 await action('arm-'+collection+'-'+op,{failure:{collection,op,skip,message:'fee-failure-'+collection+'-'+op}});
 await call('partial-'+collection+'-'+op,'accrueFee',[7,0.00025,2,13],{height:13});
}
await call('queue-retryable-insurance-prefix','queueFeeAccrual',[7,0.0002,2,14],{height:14});
await action('arm-retryable-fee-write',{failure:{collection:'feeCache',op:'update',message:'fee-write-after-insurance'}});
await call('failed-buffer-retained','flushQueuedFeeAccruals',[14],{height:14});
await call('retry-repeats-source-insurance','flushQueuedFeeAccruals',[14],{height:14});
await action('arm-dust-read',{failure:{collection:'feeCache',op:'get',message:'dust-read'}});
await call('caught-dust-read','accrueFee',[7,0.000000015,null,15],{height:15});
await action('arm-dust-save',{failure:{collection:'feeCache',op:'put',message:'dust-save'}});
await call('dust-prefix-before-put-throw','accrueFee',[7,0.000000015,null,15],{height:15});
})().catch(error=>{console.error(error);process.exitCode=1});
"###;

#[test]
fn genuine_entry_variants_fee_buffer_dust_insurance_and_retained_failures() {
    let output = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut runtime = SourceContractRuntime::new();
    let mut count = 0;
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        let row = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        let name = text(row.get("name"));
        let command = row.get("command");
        let state = row.get("state");
        if !matches!(command.get("reset"), V::Undefined) {
            runtime = SourceContractRuntime::new();
            runtime.set_entry(&text(command.get("entry"))).unwrap();
            for table in array(state.get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap()
            }
            runtime.take_effects();
            count += 1;
            continue;
        }
        runtime
            .set_height(if matches!(command.get("height"), V::Undefined) {
                V::number(10.0)
            } else {
                command.get("height")
            })
            .unwrap();
        runtime.take_effects();
        let failure = command.get("failure");
        let seed = command.get("seed");
        let actual = if !matches!(failure, V::Undefined) {
            let op = text(failure.get("op"));
            let operation = if matches!(op.as_str(), "get" | "put") {
                op
            } else {
                format!("{op}Async")
            };
            runtime
                .fail_storage(StorageFailure {
                    collection: text(failure.get("collection")),
                    operation,
                    skip: if matches!(failure.get("skip"), V::Undefined) {
                        0
                    } else {
                        num(failure.get("skip"))
                    },
                    message: text(failure.get("message")),
                })
                .unwrap();
            obj(&[("returned", V::Undefined)])
        } else if !matches!(seed, V::Undefined) {
            runtime
                .import_collection(&text(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
            obj(&[("returned", V::Undefined)])
        } else {
            observed(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: format!("Tally.{}", text(command.get("method"))),
                        arguments: array(command.get("args")),
                    })
                    .unwrap_or_else(|e| panic!("{name} unavailable: {e:?}")),
            )
        };
        assert_eq!(actual, row.get("outcome"), "{name} source outcome");
        for table in array(state.get("tables")) {
            let collection = text(table.get("name"));
            assert_eq!(
                V::Array(runtime.collection(&collection).to_vec()),
                table.get("rows"),
                "{name} complete {collection}"
            );
        }
        let owned = runtime.state_value().unwrap();
        assert_eq!(
            owned.get("feeBuffer"),
            state.get("feeBuffer"),
            "{name} buffered amounts and identities"
        );
        let V::Object(insurance) = owned.get("insurance") else {
            panic!("live Insurance map")
        };
        assert_eq!(
            V::Array(
                insurance
                    .into_iter()
                    .map(|(key, value)| V::Array(vec![s(&key), value]))
                    .collect()
            ),
            state.get("insurance"),
            "{name} live Insurance instance cache"
        );
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if matches!(k.as_str(),"database"|"db"|"rpc-clock"))).collect::<Vec<_>>();
        assert_eq!(
            V::Array(effects),
            row.get("effects"),
            "{name} actual database/RPC boundary order"
        );
        count += 1;
    }
    assert_eq!(count, 55, "genuine fee corpus must remain complete");
}
