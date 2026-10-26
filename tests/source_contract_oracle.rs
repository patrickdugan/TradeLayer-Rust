//! Execute the unchanged complete oracle.js against a bounded shared DB adapter.
//! Reference outcomes are observations, never runtime dependency answers.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};

fn s(value: &str) -> V {
    V::String(value.into())
}
fn array(value: V) -> Vec<V> {
    if let V::Array(a) = value {
        a
    } else {
        panic!("array expected")
    }
}
fn text(value: V) -> String {
    if let V::String(a) = value {
        a
    } else {
        panic!("string expected")
    }
}
fn object(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn decode(hex: &str) -> V {
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    tradelayer_core::property_catalog::parse_diagnostic_value(std::str::from_utf8(&bytes).unwrap())
        .unwrap()
}
fn assert_value(actual: &V, expected: &V, label: &str) {
    fn difference(a: &V, b: &V, path: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return difference(a, b, format!("{path}[{i}]"));
                    }
                }
                format!("{path} lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for (i, ((ak, av), (bk, bv))) in a.iter().zip(b).enumerate() {
                    if ak != bk {
                        return format!("{path} field {i}: {ak} != {bk}");
                    };
                    if av != bv {
                        return difference(av, bv, format!("{path}.{ak}"));
                    }
                }
                format!("{path} lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{path}: {a:?} != {b:?}"),
        }
    }
    if actual != expected {
        panic!("{label}: {}", difference(actual, expected, "$".into()))
    }
}

const REFERENCE: &str = r#"
const fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto');
const {loadBigNumber,readPinnedSource}=require('./scripts/legacy_numeric_oracle.cjs');
const source=readPinnedSource('fixtures/legacy_oracle_source.txt','70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d');
function diagnostic(x){
 if(x===undefined)return['undefined'];if(x===null)return['null'];if(typeof x==='boolean')return['boolean',x];if(typeof x==='string')return['string',x];
 if(typeof x==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(x);return['number',Number.isNaN(x)?'NaN':x===Infinity?'Infinity':x===-Infinity?'-Infinity':b.toString('hex')];}
 if(Array.isArray(x))return['array',Array.from(x,diagnostic)];
 return['object',Object.keys(x).map(k=>[k,diagnostic(x[k])])];
}
const hex=x=>Buffer.from(JSON.stringify(diagnostic(x))).toString('hex'),clone=x=>structuredClone(x);
let Oracle,tables,failures,effects;
function failed(name,op){const i=failures.findIndex(f=>f.collection===name&&f.operation===op);if(i>=0){if(failures[i].skip>0)failures[i].skip--;else throw Error(failures.splice(i,1)[0].message);}}
const matches=(row,q)=>Object.entries(q).every(([k,v])=>row[k]===v);
async function dbcall(name,method,args,run){const event={kind:'db',collection:name,op:method.replace(/Async$/,''),method,args:clone(args)};
 try{failed(name,method);const result=run();event.returned=clone(result);return clone(result);}catch(e){event.thrown={name:e.name,message:e.message};throw e;}finally{effects.push(event);}}
const db={async getDatabase(name){tables[name]??=[];const event={kind:'database',name,method:'getDatabase',args:[name]};
 try{failed(name,'getDatabase');event.returned={database:name};return{
  findAsync:async(...args)=>dbcall(name,'findAsync',args,()=>tables[name].filter(r=>matches(r,args[0]))),
  findOneAsync:async(...args)=>dbcall(name,'findOneAsync',args,()=>tables[name].find(r=>matches(r,args[0]))??null),
  updateAsync:async(...args)=>dbcall(name,'updateAsync',args,()=>{const[q,u,o]=args,i=tables[name].findIndex(r=>matches(r,q));if(i<0&&!o?.upsert)return 0;if(i<0)tables[name].push(clone(u));else tables[name][i]=clone(u);return 1;})
 };}catch(e){event.thrown={name:e.name,message:e.message};throw e;}finally{effects.push(event);}}};
function reset(){tables={};failures=[];effects=[];const module={exports:{}};vm.runInNewContext(source,{module,exports:module.exports,console:{log(){},error(){},warn(){}},require(name){if(name==='./db')return db;if(name==='bignumber.js')return loadBigNumber();if(name==='crypto')return crypto;if(name==='./dlcOracleBridge.js'||name==='./insurance.js')return{};throw Error('unexpected import '+name);}},{filename:'pinned/oracle.js',timeout:2000});Oracle=module.exports;}
function state(){return{oracleList:tables.oracleList??[],oracleData:tables.oracleData??[],sourceOracle:{instance:Oracle.instance?{oracles:Array.from(Oracle.instance.oracles)}:null,lastOracleUpdateBlock:Array.from(Oracle.lastOracleUpdateBlock)}};}
let scene='';
async function action(label,command){effects=[];let outcome;try{let value;
 if(command.op==='reset')reset();else if(command.op==='import')tables[command.collection]=clone(command.rows);else if(command.op==='failure')failures.push(clone(command.failure));else value=await Oracle[command.method](...command.arguments);outcome={returned:value};
 }catch(e){outcome={thrown:{name:e.name,message:e.message}};}console.log([scene+'/'+label,hex(command),hex(outcome),hex(state()),hex(effects)].join('\t'));}
const start=async name=>{scene=name;await action('reset',{op:'reset'});};
const imported=(label,collection,rows)=>action(label,{op:'import',collection,rows});
const call=(label,method,...args)=>action(label,{op:'call',method,arguments:args});
const publish=(label,id,price,height,high='HIGH',low={raw:'LOW'},close=['CLOSE'])=>call(label,'publishData',id,price,high,low,close,height);
const fail=(label,collection,operation,skip=0)=>action(label,{op:'failure',failure:{collection,operation,skip,message:'injected '+label}});
const row=(id,price,blockHeight,key='row')=>({_id:key,oracleId:id,data:{price},blockHeight});
(async()=>{
 await start('metadata-and-cache');
 await imported('metadata','oracleList',[{_id:'oracle-1',id:1,name:{type:'commodity',ticker:'FUND',adminAddress:'ADMIN',backupAddress:'BACKUP'},type:{legacy:true},adminAddress:'TOP',backupAddress:'TOPBACK',clearlists:[1,'2'],whitelists:[9],lag:-2,extra:{preserved:[false,null]}}]);
 await imported('last-price','oracleData',[row(1,100,9)]);
 await publish('publish',1,102,10);await call('info-cache','getOracleInfo',1);
 await publish('same-block',1,999,10);await publish('older-block',1,999,9);
 await publish('clamp-up-price-only',1,'1000',11);await publish('clamp-down-price-only',1,'1',12);
 await imported('external-metadata','oracleList',[{_id:'oracle-1',name:'external',adminAddress:'NEW'},{_id:'oracle-8',name:'eighth'}]);
 await call('stale-info','getOracleInfo',1);await call('merge-load','load');await call('fresh-info','getOracleInfo',1);
 await imported('empty-db','oracleList',[]);await call('load-never-clears','load');await call('stale-eighth','getOracleInfo',8);
 await publish('missing-metadata-fallback',2,77,1);await call('fallback-info','getOracleInfo',2);

 await start('initialization-and-failure-prefixes');
 await fail('load-database','oracleList','getDatabase');await call('caught-load','load');
 await fail('info-load','oracleList','findAsync');await call('info-initializes-before-load','getOracleInfo',5);
 for(const [label,collection,operation,skip]of [['price-database','oracleData','getDatabase',0],['price-find','oracleData','findAsync',0],['metadata-database','oracleList','getDatabase',0],['metadata-find','oracleList','findOneAsync',0],['save-database','oracleData','getDatabase',1],['save-update','oracleData','updateAsync',0]]){
  await start('failure-'+label);await imported('prior-price','oracleData',[row(1,100,1)]);await fail(label,collection,operation,skip);
  await publish('failed',1,200,2);await call('visible-cache','getOracleInfo',1);await publish('retry-skipped',1,150,2);await publish('higher-can-proceed',1,150,3);
 }

 await start('typed-keys-and-watermark-relations');
 await publish('number-id',1,100,10);await publish('string-id','1',200,10);await call('typed-number-price','getOraclePrice',1);await call('typed-string-price','getOraclePrice','1');
 await publish('negative-zero',-0,10,1);await publish('positive-zero-same-key',0,999,1);
 await publish('nan-id',NaN,12,1);await publish('nan-id-same-key',NaN,99,1);
 await publish('lexical-height',2,20,'9');await publish('lexical-skip',2,99,'10');await publish('numeric-height',2,30,10);
 await publish('undefined-height',3,40,undefined);await publish('undefined-height-repeated',3,41,undefined);await publish('nan-height',3,42,NaN);await publish('nan-height-repeated',3,43,NaN);

 for(const [label,prior,incoming]of [['one-number',1,999],['one-string','1',999],['one-array',[1],999],['hex','0x64','0x65'],['array-price',['100'],['102']],['null-price',null,3],['empty-string','',3],['boolean-price',false,3],['nan-new',100,NaN],['infinite-new',100,Infinity],['negative-zero',-0,-0],['round-half-up','1.0001',99],['scientific','1e-7',1]]){
  await start('circuit-'+label);await imported('last','oracleData',[row(1,prior,1)]);await publish('publish',1,incoming,2);await call('price','getOraclePrice',1);
 }
 await start('latest-row-relations');
 await imported('rows','oracleData',[row(1,'10','10','first'),row(1,'9','9','second'),row(1,'tie','9','invalid'),row(1,'8','9','tie'),row(1,'0x64',1,'hex'),row(1,Infinity,100,'infinite'),row(1,[],0,'empty-array')]);
 await call('lexical-latest','getOraclePrice',1);await publish('latest-controls-circuit',1,99,11);
 await imported('number-coercions','oracleData',[row(1,'\ufeff0b1100100\u00a0',1,'binary'),row(1,'0o145',2,'octal'),row(1,'bad',999,'bad')]);
 await call('octal-price','getOraclePrice',1);await publish('octal-circuit',1,104,12);
})().catch(e=>{console.error(e);process.exitCode=1;});
"#;

#[test]
fn unchanged_oracle_module_matches_shared_runtime_rows_cache_watermarks_and_db_failures() {
    let reference = std::process::Command::new("node")
        .args(["-e", REFERENCE])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let mut runtime = SourceContractRuntime::new();
    let mut count = 0;
    for line in std::str::from_utf8(&reference.stdout).unwrap().lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 5);
        let label = fields[0];
        let command = decode(fields[1]);
        let expected = decode(fields[2]);
        let actual = match text(command.get("op")).as_str() {
            "reset" => {
                runtime = SourceContractRuntime::new();
                object(&[("returned", V::Undefined)])
            }
            "import" => {
                runtime
                    .import_collection(&text(command.get("collection")), array(command.get("rows")))
                    .unwrap();
                object(&[("returned", V::Undefined)])
            }
            "failure" => {
                let f = command.get("failure");
                let V::Number(skip) = f.get("skip") else {
                    panic!()
                };
                runtime
                    .fail_storage(StorageFailure {
                        collection: text(f.get("collection")),
                        operation: text(f.get("operation")),
                        skip: f64::from_bits(skip) as usize,
                        message: text(f.get("message")),
                    })
                    .unwrap();
                object(&[("returned", V::Undefined)])
            }
            "call" => {
                let value = runtime
                    .invoke_dependency(&DependencyCall {
                        target: format!("Oracle.{}", text(command.get("method"))),
                        arguments: array(command.get("arguments")),
                    })
                    .unwrap_or_else(|e| panic!("{label}: local error {e:?}"));
                match value {
                    DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                        object(&[("returned", v)])
                    }
                    DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => object(&[(
                        "thrown",
                        object(&[("name", s(&e.name)), ("message", s(&e.message))]),
                    )]),
                }
            }
            _ => panic!("unexpected command"),
        };
        assert_value(&actual, &expected, &format!("{label} outcome"));
        let state = decode(fields[3]);
        for collection in ["oracleList", "oracleData"] {
            assert_value(
                &V::Array(runtime.collection(collection).to_vec()),
                &state.get(collection),
                &format!("{label} {collection}"),
            );
        }
        assert_value(
            &runtime.state_value().unwrap().get("sourceOracle"),
            &state.get("sourceOracle"),
            &format!("{label} singleton/cache/watermarks"),
        );
        let effects = runtime
            .take_effects()
            .into_iter()
            .filter(|e| matches!(e.get("kind"),V::String(k)if k=="db"||k=="database"))
            .collect();
        assert_value(
            &V::Array(effects),
            &decode(fields[4]),
            &format!("{label} ordered actual DB effects"),
        );
        // Subsequent actions continue from a complete clone, including pending
        // failures and typed map keys. Durable codec coverage belongs to runtime.
        let cloned = runtime.clone();
        assert_value(
            &cloned.state_value().unwrap(),
            &runtime.state_value().unwrap(),
            label,
        );
        runtime = cloned;
        count += 1;
    }
    assert_eq!(count, 140, "adversarial action count");
}
