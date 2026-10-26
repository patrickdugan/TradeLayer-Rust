//! Complete pinned CommonJS modules, actual shared rows and clearing caches.
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
// Expose the storage adapter only. All pinned source module bytes and actual
// registry/clearing methods remain unchanged in their common live realm.
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');
code+='\nmodule.exports={environment,tag};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag}=driver.exports;let e;
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const c=e.load('clearing');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),oracleMarks:c.latestOracleMarkById,nativeMarks:c.latestNativeMarkById,blockTrades:c.blockTrades};}
function emit(name,command,outcome){console.log(JSON.stringify(norm(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>x.kind==='database'||x.kind==='db')}))));}
async function reset(name,native=false){e=environment({});e.load('clearing');await flush();const rows=e.table('contractList');rows[0].data.native=native;e.effects.length=0;emit(name,{reset:true},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(failure){const original=e.storage.getDatabase;let skip=failure.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===failure.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(failure.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args,failure){e.effects.length=0;if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}
 let outcome;try{const[namespace,method]=target.split('.');outcome={returned:await e.load(namespace==='Clearing'?'clearing':'contractRegistry')[method](...args)}}catch(error){outcome={thrown:{name:error.name,message:error.message}}};await flush();emit(name,{target,args,failure},outcome);
}
const probe=(name,height,failure)=>call(name,'Clearing.isPriceUpdatedForBlockHeight',[1,height],failure);
(async()=>{
 await reset('oracle-lexical-reset');
 await imported('oracle-string-heights','oracleData',[{_id:'first',oracleId:1,blockHeight:'9',data:{price:9}},{_id:'second',oracleId:1,blockHeight:'10',data:{price:10}}]);
 await probe('prime-max-is-lexical',99);await probe('existing-cache-remains',100);
 await imported('current-block','oracleData',[{_id:'now',oracleId:1,blockHeight:100,data:{price:10}}]);await probe('current-updates-from-lexical-cache',100);await probe('same-block-repeat-sees-mutable-cache',100);
 await imported('current-string-price','oracleData',[{_id:'now',oracleId:1,blockHeight:100,data:{price:'10'}}]);await probe('strict-price-type-change',100);
 await reset('oracle-mixed-height-reset');
 await imported('oracle-mixed-heights','oracleData',[{_id:'first',oracleId:1,blockHeight:9,data:{price:9}},{_id:'second',oracleId:1,blockHeight:'10',data:{price:10}},{_id:'third',oracleId:1,blockHeight:false,data:{price:1}}]);await probe('mixed-height-comparison-numeric',99);
 await probe('caught-price-read-failure',99,{collection:'oracleData',op:'find',message:'oracle-read'});
 await probe('factory-before-catch-throws',99,{collection:'oracleData',op:'getDatabase',message:'oracle-factory'});
 for(const op of ['findOne','getDatabase']){
  await reset('native-'+op+'-reset',true);
  await imported('native-contract-id-fallback-'+op,'volumeIndex',[{_id:1,value:{price:88,blockHeight:10}}]);
  await probe('native-info-'+op+'-failure-falls-back',10,{collection:'contractList',op,skip:1,message:'pair-info-'+op});
  await probe('native-cache-repeated-'+op,10);
 }
 await reset('native-pair-priority-reset',true);
 await imported('native-pair-and-id','volumeIndex',[{_id:'7-7',value:{price:77,blockHeight:10}},{_id:1,value:{price:88,blockHeight:10}}]);await probe('native-pair-priority',10);
 for(const native of [0,null,'',undefined]){
  await reset('strict-native-'+String(native),native);
  // Passing undefined to a default parameter produces false; explicitly import
  // the missing-field representation for that source row instead.
  if(native===undefined){const row=structuredClone(e.table('contractList')[0]);delete row.data.native;await imported('missing-native-field','contractList',[row]);}
  await imported('non-oracle-contract-doc-'+String(native),'volumeIndex',[{_id:1,value:{price:66,blockHeight:10}}]);await probe('native-must-be-exact-false-'+String(native),10);
 }
 await reset('missing-contract-reset');await imported('missing-contract','contractList',[]);await call('missing-is-oracle-returns-null','Contract.isOracleContract',[1]);await call('missing-oracle-id-throws','Contract.getOracleId',[1]);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;

#[test]
fn genuine_clearing_cache_relations_and_native_lookup_exception_boundaries() {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut runtime = SourceContractRuntime::new();
    let mut count = 0;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let case = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
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
            }
            observed(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: text(command.get("target")),
                        arguments: array(command.get("args")),
                    })
                    .unwrap_or_else(|e| panic!("{name}: local error {e:?}")),
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
                    &format!("{name} unexpected {collection}"),
                )
            }
        }
        for field in ["oracleMarks", "nativeMarks", "blockTrades"] {
            assert_value(
                &state.get(field),
                &expected.get(field),
                &format!("{name} complete {field}"),
            )
        }
        let effects = runtime
            .take_effects()
            .into_iter()
            .filter(|e| e.get("kind") == s("db") || e.get("kind") == s("database"))
            .collect();
        assert_value(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered storage effects"),
        );
        count += 1;
    }
    assert_eq!(count, 42);
}
