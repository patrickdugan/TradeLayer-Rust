//! Whole unchanged Oracle/Logic/Types source over the same Core collections.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
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
const REFERENCE: &str = r###"'use strict';
// Audit only: unchanged pinned business modules; real shared in-memory DB calls.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=__dirname,root=process.env.PROOF_REPO||path.resolve(stage,'../../outputs/TradeLayer-Rust'),flags=process.argv.slice(2),lock=flags.includes('--lock');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],code=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(code.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('return {types,logic,Orderbook,effects','return {context,storage,tables,sources,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',records=[];let e,lastParams;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),O=e.load('oracle'),P=e.load('property');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,oracleWatermarks:O.lastOracleUpdateBlock,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,insurance:e.load('insurance').instances,lastParams,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind)&&!['Oracle.getInstance','Oracle.load','Oracle.getOraclePrice','Insurance.getInstance'].includes(v.target))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;for(const method of ['createOracle','publishOracleData','closeOracle','updateAdmin'])observe(e.logic,method,'Logic.'+method);for(const method of ['getInstance','load','getNextId','createOracle','getOracleInfo','isAdmin','updateAdmin','closeOracle','publishData','getOraclePrice'])observe(e.load('oracle'),method,'Oracle.'+method);observe(e.load('tally'),'updateBalance','Tally.updateBalance');for(const method of ['liquidate','getInstance'])observe(e.load('insurance'),method,'Insurance.'+method);await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [module,method]=target.split('.'),receiver=module==='Logic'?e.logic:module==='Oracle'?e.load('oracle'):module==='Tally'?e.load('tally'):e.load('insurance');let transport='Immediate',outcome;try{let value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}if(target==='Oracle.getInstance'&&outcome?.returned)outcome.returned={oracles:outcome.returned.oracles};await flush();emit(name,{target,args,failure,...(unsupported?{unsupported:true}:{})},{transport,outcome});}
async function raw(name,type,payload,sender=A,failure,block=10){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,null,0,0,block];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.tables.set(collection,e.copy(rows));emit(name,{import:{collection,rows}},{outcome:{returned:undefined}});}
(async()=>{
await reset('first10-reset');
await raw('raw13-create',13,'FEED,url,'+B+',1;2,2');
await raw('raw14-publish',14,'2,'+(100*1e4).toString(36)+',a,b,c');
await raw('raw8-new-admin',8,C+',0,1,0,2,0');
await raw('raw14-new-admin',14,'2,'+(101*1e4).toString(36),C,undefined,11);
await raw('raw15-wrong-validator',15,'2',B);
await call('direct-create-scalar','Logic.createOracle',[A,'DIRECT','u',B,[1,2],3,null,12]);
await call('direct-close-missing','Logic.closeOracle',[999,12]);
await call('direct-close-existing','Logic.closeOracle',[2,12]);
await call('publish-prefix-read-failure','Logic.publishOracleData',[2,200,undefined,undefined,undefined,13],{collection:'oracleData',op:'find',message:'price-read'});

for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 await raw(entry+'-empty-create',13,'');
 await raw(entry+'-duplicate-ticker-create',13,'TEST,u,,1;2,0');
 await raw(entry+'-lag-nan-create',13,'FEED,,,,-');
 await raw(entry+'-valid-publish',14,'1,'+(1010000).toString(36));
 await raw(entry+'-same-height-publish',14,'1,'+(5000000).toString(36));
 await raw(entry+'-outsider-publish',14,'1,'+(1000000).toString(36),'outsider',undefined,11);
 await raw(entry+'-backup-publish',14,'1,'+(1010000).toString(36),B,undefined,11);
 await raw(entry+'-missing-feed',14,'999,1');
 await raw(entry+'-zero-price',14,'1,0');
 await raw(entry+'-nan-price',14,'1,-');
 await raw(entry+'-nan-high',14,'1,1,-');
 await raw(entry+'-positive-high',14,'1,1,10,20,30');
 await raw(entry+'-backup-update',8,C+',0,1,0,1,1');
 await raw(entry+'-unauthorized-update',8,C+',0,1,0,1,0','outsider');
 await raw(entry+'-missing-admin-target',8,C+',0,1,0,99,0');
 await raw(entry+'-no-admin-target',8,C+',0,0,0,1,0');
 await raw(entry+'-token-and-oracle-targets',8,C+',0,1,1,1,0');
 await raw(entry+'-raw15-negative',15,'-1');
 await call(entry+'-close-one','Logic.closeOracle',[1,12]);
 await raw(entry+'-publish-closed',14,'1,2',A,undefined,12);
}
await reset('inactive',{inactive:true});await raw('inactive-create',13,'NO');await raw('inactive-publish',14,'1,1');await raw('inactive-admin',8,C+',0,1,0,1,0');
await reset('direct-shapes');
for(const [name,args]of [['scalar',['name',A]],['undefined',[]],['null',[null,A]],['zero',[0,A]],['array',[[],A]],['config',[{name:'n',ticker:'t',lag:'2',clearlists:[1,'x'],backupAddress:B,url:'u'},A]],['fallbacks',[{name:0,ticker:false,lag:null,clearlists:{},adminAddress:'',backupAddress:false},A]],['nan-lag',[{ticker:'N',lag:'invalid'}]],['infinite-lag',[{ticker:'I',lag:Infinity}]],['array-lag',[{ticker:'A',lag:[]}]],['throw-lag',[{ticker:'bad',lag:{valueOf:1,toString:1}}]]])await call('create-'+name,'Oracle.createOracle',args);
await call('read-next-id','Oracle.getNextId');await call('read-instance','Oracle.getInstance');
await call('undefined-admin-missing-feed','Oracle.isAdmin',[undefined,999]);await call('null-admin-missing-feed','Oracle.isAdmin',[null,999]);await call('existing-primary','Oracle.isAdmin',[A,1]);await call('existing-backup','Oracle.isAdmin',[B,1]);
await call('set-backup-direct','Oracle.updateAdmin',[1,'new-backup',[]]);await call('set-primary-undefined','Oracle.updateAdmin',[1,undefined,false]);await call('logic-token-receiver-error','Logic.updateAdmin',[false,true,true,1,C,false,10]);await call('logic-no-target','Logic.updateAdmin',[false,false,false,1,C,false,10]);
await call('missing-direct-update','Oracle.updateAdmin',[999,C,false]);await call('missing-direct-close','Oracle.closeOracle',[999]);
for(const [name,target,args,collection,op,skip]of[
 ['create-next-factory','Logic.createOracle',[A,'new'],'oracleList','getDatabase',0],['create-next-find','Logic.createOracle',[A,'new'],'oracleList','find',0],['create-insert-factory','Logic.createOracle',[A,'new'],'oracleList','getDatabase',1],['create-insert','Logic.createOracle',[A,'new'],'oracleList','insert',0],
 ['admin-factory','Oracle.updateAdmin',[1,C,false],'oracleList','getDatabase',0],['admin-read','Oracle.updateAdmin',[1,C,false],'oracleList','findOne',0],['admin-write','Oracle.updateAdmin',[1,C,false],'oracleList','update',0],
 ['admin-validity-read','raw',[],'oracleList','findOne',0],
 ['close-factory','Logic.closeOracle',[1,10],'oracleList','getDatabase',0],['close-read','Logic.closeOracle',[1,10],'oracleList','findOne',0],['close-write','Logic.closeOracle',[1,10],'oracleList','update',0],['close-insurance-factory','Logic.closeOracle',[1,10],'insurance','getDatabase',0],['close-insurance-load','Logic.closeOracle',[1,10],'insurance','findOne',0],
 ['publish-read','Logic.publishOracleData',[1,110,undefined,undefined,undefined,11],'oracleData','find',0],['publish-write','Logic.publishOracleData',[1,110,undefined,undefined,undefined,11],'oracleData','update',0]
]){await reset('failure-reset-'+name);const failure={collection,op,skip,message:name};if(target==='raw'){await raw(name+'-failed',14,'1,1',A,failure);await raw(name+'-retry',14,'1,1');}else{await call(name+'-failed',target,args,failure);await call(name+'-retry',target,args);}}
await reset('id-fallback-cache');
await seed('id-source-rows','oracleList',[{_id:'oracle-4foo'},{_id:'oracle-0x10'},{_id:'oracle-2-9',id:null},{_id:'oracle-x',id:'bad'}]);await call('load-id-source','Oracle.load');await seed('id-clear-db','oracleList',[]);await call('next-cache-parseInt','Oracle.getNextId');await call('create-cache-max','Oracle.createOracle',['cache',A]);
await reset('id-numeric-rows');await seed('numeric-id-import','oracleList',[{_id:'oracle-2',id:2.5},{_id:'oracle-100',id:'10.5'},{_id:'oracle-infinite',id:Infinity},{_id:'oracle-nan',id:NaN}]);await call('numeric-next','Oracle.getNextId');await call('numeric-create','Oracle.createOracle',[{ticker:'FRACTION'},A]);
await reset('bad-cache-key');await seed('numeric-key-rows','oracleList',[{_id:4}]);await call('load-bad-key','Oracle.load');await seed('clear-bad-key-db','oracleList',[]);await call('bad-cache-key-source-error','Oracle.getNextId');await seed('positive-id-avoids-cache','oracleList',[{_id:'x',id:5}]);await call('positive-id-returns-before-bad-key','Oracle.getNextId');
await reset('nested-legacy');await seed('nested-oracle-import','oracleList',[{_id:'oracle-1',name:{adminAddress:A,backupAddress:B,ticker:'legacy'}}]);await raw('nested-publish',14,'1,1');await raw('nested-backup-admin',8,C+',0,1,0,1,0',B);await call('nested-close-primary-undefined','Logic.closeOracle',[1,15]);


for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('closed-insurance-prefix-'+entry,{entry});
 await seed('closed-insurance-snapshot-'+entry,'insurance',[{_id:'snapshot',key:'true',value:{contractSeriesId:true,balances:[{propertyId:7,amountAvailable:20}],hedgeRatio:0.5,block:10}}]);
 await call('closed-insurance-tally-prefix-'+entry,'Logic.closeOracle',[1,10]);
 await call('closed-insurance-repeat-'+entry,'Logic.closeOracle',[1,11]);
}
await reset('post-close-write-error');await seed('post-close-snapshot','insurance',[{_id:'snapshot',key:'true',value:{contractSeriesId:true,balances:[{propertyId:7,amountAvailable:20}],hedgeRatio:0.5,block:10}}]);await call('insurance-tally-save-caught','Oracle.closeOracle',[1],{collection:'tallyMap',op:'update',message:'insurance-tally-save'});await call('insurance-tally-save-retry','Oracle.closeOracle',[1]);

await reset('composite-actor-boundary');
await call('create-bignumber-admin','Oracle.createOracle',[{ticker:'BNADMIN',adminAddress:new BN(7)}]);
await call('bignumber-actor-actual-false-local-identity-boundary','Oracle.isAdmin',[new BN(7),2],undefined,true);
for(const record of records)console.log(JSON.stringify(tag(record)));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1});
"###;
fn text(value: V) -> String {
    if let V::String(value) = value {
        value
    } else {
        panic!("expected source string: {value:?}")
    }
}
fn array(value: V) -> Vec<V> {
    if let V::Array(value) = value {
        value
    } else {
        panic!("expected source array: {value:?}")
    }
}
fn cases(lock: bool) -> Vec<V> {
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .args([
            "-e",
            REFERENCE,
            "lower-oracle-reference",
            if lock { "--lock" } else { "--baseline" },
        ])
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
fn returned(value: DependencyReturn) -> V {
    let (transport, outcome) = match value {
        DependencyReturn::Immediate(value) => ("Immediate", obj(&[("returned", value)])),
        DependencyReturn::Pending(value) => ("Pending", obj(&[("returned", value)])),
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
fn fault(command: &V) -> Option<tradelayer_core::source_contract_runtime::StorageFailure> {
    let f = command.get("failure");
    if f == V::Undefined {
        return None;
    }
    let op = text(f.get("op"));
    Some(tradelayer_core::source_contract_runtime::StorageFailure {
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
}
fn command(c: &V) -> C {
    if let V::Object(_) = c.get("import") {
        let i = c.get("import");
        return C::Import {
            collection: text(i.get("collection")),
            rows: array(i.get("rows")),
        };
    }
    if let V::Array(args) = c.get("raw") {
        C::ExecuteContract(args)
    } else {
        C::Invoke(DependencyCall {
            target: text(c.get("target")),
            arguments: array(c.get("args")),
        })
    }
}
fn actual(runtime: &mut SourceContractRuntime, case: &V) -> V {
    let c = case.get("command");
    if let V::Object(_) = c.get("import") {
        let i = c.get("import");
        runtime
            .import_collection(&text(i.get("collection")), array(i.get("rows")))
            .unwrap();
        return obj(&[("outcome", obj(&[("returned", V::Undefined)]))]);
    }
    if let Some(f) = fault(&c) {
        runtime.fail_storage(f).unwrap();
    }
    if let V::Array(args) = c.get("raw") {
        let result = match runtime.execute_contract_unverified(&args).unwrap() {
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
        obj(&[("outcome", result)])
    } else {
        returned(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: text(c.get("target")),
                    arguments: array(c.get("args")),
                })
                .unwrap(),
        )
    }
}
fn assert_state(runtime: &SourceContractRuntime, case: &V) {
    let expected = case.get("state");
    let name = text(case.get("name"));
    let mut known = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let collection = text(table.get("name"));
        known.insert(collection.clone());
        assert_eq!(
            V::Array(runtime.collection(&collection).to_vec()),
            table.get("rows"),
            "{name} full rows {collection}"
        );
    }
    let state = runtime.state_value().unwrap();
    assert_eq!(state.get("runtimePolicy"), s("LowerLifecycleV7"));
    for table in array(state.get("tables")) {
        if !known.contains(&text(table.get("name"))) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} no invented rows"
            );
        }
    }
    for (key, wanted) in [
        ("oracleSourceCache", "oracleCache"),
        ("oracleSourceInitialized", "oracleInitialized"),
        ("oracleSourceWatermarks", "oracleWatermarks"),
    ] {
        assert_eq!(state.get(key), expected.get(wanted), "{name} {key}");
    }
    // The existing Core stores the actual string-keyed Insurance Map as object
    // fields. Project its exact entries; compare every populated instance too.
    let V::Object(insurance) = state.get("insurance") else {
        panic!("insurance Map projection")
    };
    assert_eq!(
        V::Array(
            insurance
                .into_iter()
                .map(|(key, value)| V::Array(vec![V::String(key), value]))
                .collect()
        ),
        expected.get("insurance"),
        "{name} complete Insurance Map entries"
    );
    let global = state.get("globalClearing");
    for (key, wanted) in [
        ("tallyStaticInitialized", "tallyInitialized"),
        ("tallyMod", "tallyMod"),
        ("propertyCache", "propertyCache"),
    ] {
        assert_eq!(global.get(key), expected.get(wanted), "{name} {key}");
    }
    assert_eq!(
        state.get("proofLifecycle").get("propertyInitialized"),
        expected.get("propertyInitialized"),
        "{name} shared property constructor"
    );
    assert_eq!(
        runtime.tally_cache(),
        expected.get("tally"),
        "{name} sole Tally"
    );
    assert_eq!(
        runtime
            .last_contract_parameters()
            .cloned()
            .unwrap_or(V::Undefined),
        expected.get("lastParams"),
        "{name} original raw parameters"
    );
}
fn effects(values: Vec<V>) -> V {
    const TARGETS: &[&str] = &[
        "Logic.createOracle",
        "Logic.publishOracleData",
        "Logic.closeOracle",
        "Logic.updateAdmin",
        "Oracle.createOracle",
        "Oracle.getNextId",
        "Oracle.isAdmin",
        "Oracle.updateAdmin",
        "Oracle.closeOracle",
        "Oracle.publishData",
        "Oracle.getOracleInfo",
        "Insurance.liquidate",
        "Tally.updateBalance",
    ];
    V::Array(values.into_iter().filter_map(|v|{
        if matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str())) {Some(v)}
        else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if TARGETS.contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
        else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",v.get("args")),("returned",v.get("returned"))]))}
        else{None}
    }).collect())
}
fn setup(runtime: &mut SourceContractRuntime, case: &V) {
    let options = case.get("command").get("reset");
    runtime
        .set_entry(&if options.get("entry") == V::Undefined {
            "main-subgraph".into()
        } else {
            text(options.get("entry"))
        })
        .unwrap();
    runtime.set_activation_answer(options.get("inactive") != V::Bool(true));
    for table in array(case.get("state").get("tables")) {
        runtime
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
}
#[test]
fn unchanged_lower_oracle_graph_both_genuine_numeric_versions() {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true));
    let mut runtime = SourceContractRuntime::with_policy(P::LowerLifecycleV7);
    let (mut resets, mut imports, mut raw, mut direct, mut unsupported) = (0, 0, 0, 0, 0);
    for case in &baseline {
        let c = case.get("command");
        let name = text(case.get("name"));
        if c.get("reset") != V::Undefined {
            resets += 1;
            runtime = SourceContractRuntime::with_policy(P::LowerLifecycleV7);
            setup(&mut runtime, case);
        } else if c.get("unsupported") == V::Bool(true) {
            unsupported += 1;
            assert_eq!(
                case.get("result").get("outcome").get("returned"),
                V::Bool(false),
                "real DB copy rejects composite actor identity"
            );
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: text(c.get("target")),
                    arguments: array(c.get("args"))
                }),
                Err(SourceRuleError::UnsupportedNumericDomain)
            );
            assert_eq!(
                runtime, before,
                "explicit local actor domain rollback; no JS-equivalence claim for refused call"
            );
            continue;
        } else {
            if c.get("import") != V::Undefined {
                imports += 1;
            } else if c.get("raw") != V::Undefined {
                raw += 1;
            } else {
                direct += 1;
            }
            assert_eq!(
                actual(&mut runtime, case),
                case.get("result"),
                "{name} exact outcome/transport"
            );
        }
        assert_state(&runtime, case);
        assert_eq!(
            effects(runtime.take_effects()),
            case.get("effects"),
            "{name} complete ordered DB and observed source calls"
        );
    }
    assert_eq!(
        (baseline.len(), resets, imports, raw, direct, unsupported),
        (212, 32, 12, 88, 79, 1)
    );
}
#[test]
fn source_closed_admin_and_publication_prefixes_replay_then_continue() {
    let records = cases(false);
    let mut runs = 0;
    for start in records.iter().enumerate().filter_map(|(i, r)| {
        let name = text(r.get("name"));
        (r.get("command").get("reset") != V::Undefined
            && (name.starts_with("failure-reset-")
                || name.starts_with("closed-insurance-prefix-")
                || [
                    "first10-reset",
                    "id-fallback-cache",
                    "bad-cache-key",
                    "nested-legacy",
                    "post-close-write-error",
                ]
                .contains(&name.as_str())))
        .then_some(i)
    }) {
        runs += 1;
        let mut replay = SourceContractReplay::with_policy(P::LowerLifecycleV7).unwrap();
        let options = records[start].get("command").get("reset");
        if options.get("entry") != V::Undefined {
            replay
                .apply(C::SetEntry(text(options.get("entry"))))
                .unwrap();
        }
        for table in array(records[start].get("state").get("tables")) {
            replay
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        for case in records[start + 1..]
            .iter()
            .take_while(|r| r.get("command").get("reset") == V::Undefined)
        {
            let c = case.get("command");
            if let Some(f) = fault(&c) {
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay.apply(command(&c)).unwrap();
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "actual source receipt"
            );
            assert_state(replay.runtime(), case);
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(&bytes[..5], b"TLCR\x07");
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(restored, replay);
            assert_eq!(restored.checkpoint().unwrap(), bytes);
            replay = restored;
        }
    }
    assert_eq!(runs, 24);
}
#[test]
fn earlier_graphs_refuse_lower_oracle_and_local_owner_rejections_rollback() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        for target in [
            "Logic.createOracle",
            "Logic.publishOracleData",
            "Logic.closeOracle",
            "Logic.updateAdmin",
            "Oracle.getInstance",
            "Oracle.getNextId",
            "Oracle.createOracle",
            "Oracle.isAdmin",
            "Oracle.updateAdmin",
            "Oracle.closeOracle",
        ] {
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: target.into(),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
        for kind in [8., 13., 14., 15.] {
            let before = runtime.clone();
            assert_eq!(
                runtime.execute_contract_unverified(&[
                    s("old-refusal"),
                    V::number(kind),
                    s("tl"),
                    s(""),
                    s("sender"),
                    V::Null,
                    V::number(0.),
                    V::number(0.),
                    V::number(10.)
                ]),
                Err(SourceRuleError::InvalidInput)
            );
            assert_eq!(runtime, before);
        }
    }
    let mut runtime = SourceContractRuntime::with_policy(P::LowerLifecycleV7);
    let before = runtime.clone();
    assert_eq!(
        runtime.invoke_dependency(&DependencyCall {
            target: "Logic.updateAdmin".into(),
            arguments: vec![
                V::Bool(true),
                V::Bool(false),
                V::Bool(true),
                V::number(1.),
                s("new"),
                V::Bool(false),
                V::number(10.)
            ]
        }),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(
        runtime, before,
        "uncomposed ClearList write remains a local admission failure"
    );
}
