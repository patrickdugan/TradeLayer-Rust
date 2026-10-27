//! Unchanged pinned lower Property/Logic graph over the sole actual Core stores.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as Replay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
fn n(v: f64) -> V {
    V::number(v)
}
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn arr(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("array {v:?}")
    }
}
fn txt(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("string {v:?}")
    }
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
const REFERENCE: &str = r####"
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.env.PROPERTY_REPO||path.resolve(__dirname,'../../../outputs/TradeLayer-Rust'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {context,storage,tables,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',B='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,lastParams,unhandled,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result,state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;unhandled=[];if(Object.hasOwn(options,'propertyRows'))e.table('propertyList').splice(0,Infinity,...structuredClone(options.propertyRows));for(const method of ['tokenIssue','grantManagedToken','redeemManagedToken'])observe(e.logic,method,'Logic.'+method);for(const method of ['getInstance','load','getPropertyData','getPropertyIndex','isManagedAndAdmin','doesTickerExist'])observe(e.load('property'),method,'Property.'+method);for(const method of ['createToken','getNextPropertyId','addProperty','save','grantTokens','redeemTokens'])observe(e.load('property').prototype,method,'Property.'+method);const P=e.load('procedural').ProceduralRegistry;for(const method of ['_base','getTemplate','getContract','ensureIssuanceContext','ensureRedemptionContext','_normalizeHash','upsertTemplate','upsertContract','transitionContract'])observe(P,method,'Procedural.'+method);await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,type,payload,sender=A,reference=null,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,reference,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);let receiver;const [ns,method]=target.split('.');if(ns==='Logic')receiver=e.logic;else if(ns==='Procedural')receiver=e.load('procedural').ProceduralRegistry;else if(ns==='PropertyInstance')receiver=e.load('property').instance;else receiver=e.load('property');let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:target.replace('PropertyInstance.','Property.'),args,failure,unsupported},{transport,outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{outcome:{returned:undefined}});}
const amount=v=>(v*1e8).toString(36),issue=(ticker,managed=false,extra={})=>[1000..toString(36),ticker,extra.whitelists||'',+managed,B,+!!extra.nft,0,extra.proceduralType??''].join(',');
(async()=>{
await reset('first-ten');
await raw('raw1-fixed-issuance',1,issue('FIXED'));
await raw('raw11-fixed-admin-refused',11,'8,'+amount(1)+','+A);
await raw('raw1-managed-issuance',1,issue('MNG',true));
await raw('raw11-managed-reference-recipient',11,'9,'+amount(2)+',',A,{address:B});
await raw('raw11-managed-defaults-sender',11,'9,'+amount(3)+',');
await raw('raw12-managed-redemption',12,'9,'+amount(2));
await raw('raw12-outsider-refused',12,'9,'+amount(1),B);
await raw('raw1-duplicate-ticker-refused',1,issue('MNG',true));
await raw('raw11-zero-amount-refused',11,'9,0,'+B);
await raw('raw1-procedural-issuance',1,issue('PROC',true,{proceduralType:'2'}));
if(!flags.includes('--proof10')){
await call('procedural-grant-missing-context','Logic.grantManagedToken',[10,1,B,A,11]);
await raw('procedural-raw-grant-missing-context',11,'a,'+amount(1)+','+B);
await call('procedural-template-real-write','Procedural.upsertTemplate',['template-A',{templateHash:'ABCD'}]);
await call('procedural-contract-real-write','Procedural.upsertContract',['contract-A','template-A','OPEN']);
await raw('procedural-raw-grant-success',11,['a',amount(2),B,'','template-A','contract-A','OPEN',' abcd '].join(','));
await raw('procedural-raw-redeem-open-invalid',12,['a',amount(1),'template-A','contract-A','SETTLED'].join(','),B);
await call('procedural-contract-real-settled','Procedural.transitionContract',['contract-A','SETTLED']);
await raw('procedural-raw-nonadmin-redeem-success',12,['a',amount(1),'template-A','contract-A','SETTLED'].join(','),B);
await call('procedural-direct-wrong-hash','Logic.grantManagedToken',[10,1,B,A,12,'wrong','template-A','contract-A','SETTLED']);
for(const entry of ['types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});await raw(entry+'-raw-managed-issue',1,issue('MNG',true));await raw(entry+'-grant',11,'8,'+amount(1)+','+B);await raw(entry+'-redeem',12,'8,'+amount(1));
}
await reset('direct-issue-fields');
await call('direct-fixed-issue','Logic.tokenIssue',[A,1.123456789,'DIRECT','unused-url',4,false,B,false,12]);
await call('direct-duplicate-returns-error-id-message','Logic.tokenIssue',[A,2,'DIRECT','',0,false,'',false,12]);
await call('direct-nft-managed-type-priority','Logic.tokenIssue',[A,2,'NFT','',0,true,B,true,12]);
await call('direct-procedural-overrides-nft','Logic.tokenIssue',[A,2,'PRNFT','',0,true,B,true,12,0]);
await call('direct-create-negative-initial-tally-error-caught','Logic.tokenIssue',[A,-1,'NEG','',0,false,'',false,12]);
await call('direct-invalid-total-nan-caught','Logic.tokenIssue',[A,'NaN','NAN','',0,false,'',false,12]);
await reset('raw1-encoder-multiple-whitelist');
const encoded=e.load('txEncoder').encodeTokenIssue({initialAmount:1000,ticker:'SHIFT',whitelists:[1,2],managed:true,backupAddress:B,nft:false,coloredCoinHybrid:false});
await raw('raw1-actual-encoder-comma-whitelists-shifts-fields',1,encoded.slice(3));
await raw('raw1-semicolon-whitelists-exact-decode',1,issue('LIST',true,{whitelists:'1;2'}));
await raw('raw1-lowercase-s-rejected',1,issue('sBAD',false));
await raw('raw1-invalid-initial-integer',1,'0,EMPTY,,0,,0,0,');
for(const failure of [{collection:'propertyList',op:'update'},{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'},{collection:'propertyList',op:'findOne',skip:1},{collection:'propertyList',op:'getDatabase',skip:1}]){
 const label=failure.collection+'-'+failure.op+'-'+(failure.skip??0);await reset('issue-failure-'+label);await raw('issue-first-'+label,1,issue('FAIL',true),A,null,{...failure,message:label});await raw('issue-retry-'+label,1,issue('FAIL',true));await raw('issue-next-after-'+label,1,issue('NEXT',true));
}
for(const skip of [0,1,2,3]){
 await reset('grant-read-failure-'+skip);await raw('managed-issue-before-grant-'+skip,1,issue('MNG',true));await call('grant-read-prefix-'+skip,'Logic.grantManagedToken',[8,1,B,A,12],{collection:'propertyList',op:'findOne',skip,message:'grant-read-'+skip});await call('grant-read-retry-'+skip,'Logic.grantManagedToken',[8,1,B,A,12]);
}
for(const failure of [{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'},{collection:'propertyList',op:'update'}]){
 const label=failure.collection+'-'+failure.op;await reset('grant-write-failure-'+label);await raw('managed-issue-for-'+label,1,issue('MNG',true));await call('grant-write-prefix-'+label,'Logic.grantManagedToken',[8,1,B,A,12],{...failure,message:label});await call('grant-write-retry-'+label,'Logic.grantManagedToken',[8,1,B,A,12]);await call('redeem-owned-prefix-'+label,'Logic.redeemManagedToken',[8,1001,A,13]);
}
await reset('cache-duplicate-direct');
await call('load-cached-index','Property.getPropertyIndex',[]);
await call('duplicate-cached-ticker-real-error-return','PropertyInstance.createToken',['P1',1,'Fixed',0,A,B]);
await seed('string-key-ticker-index','propertyList',[{_id:'propertyIndex',value:JSON.stringify([['KEY',{ticker:'OTHER',totalInCirculation:1,type:1,issuer:A}]])}]);
await call('load-string-key-index','Property.load',[]);
await call('property-type-own-tostring-rejects-first','PropertyInstance.addProperty',[99,'OBJECTTYPE',1,{toString:1},0,A,'',{}]);
await call('property-type-array-primitive-real-write','PropertyInstance.addProperty',[99,'ARRAYTYPE',1,['Fixed'],0,A,'',{}]);
await call('cache-has-ticker-constructor-error-caught','Logic.tokenIssue',[A,1,'KEY','',0,false,B,false,13]);
await call('nonnumeric-key-next-id-source-nan','PropertyInstance.getNextPropertyId',[]);
await call('nonnumeric-key-issuance-source-nan-id','Logic.tokenIssue',[A,1,'NANID','',0,false,B,false,13]);
for(const [label,rows]of [['missing',[]],['false-value',[{_id:'propertyIndex',value:false}]],['zero-value',[{_id:'propertyIndex',value:0}]],['null-value',[{_id:'propertyIndex',value:null}]],['json-null',[{_id:'propertyIndex',value:'null'}]],['bad-pair',[{_id:'propertyIndex',value:'[null]'}]]]){
 await reset('property-index-'+label,{propertyRows:rows});
 await call('ticker-read-'+label,'Property.doesTickerExist',['NEW']);
 await call('managed-read-'+label,'Property.isManagedAndAdmin',[1,A]);
 await call('load-index-'+label,'Property.load',[]);
 await call('direct-create-index-'+label,'Logic.tokenIssue',[A,1,'NEW','',0,false,B,false,13]);
}
await reset('uncaught-malformed-json-domain',{propertyRows:[{_id:'propertyIndex',value:'bad'}]});
await call('ticker-uncaught-malformed-json-local-refusal','Property.doesTickerExist',['NEW'],undefined,true);

}

})().then(()=>{const output=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n',destination=path.resolve(__dirname,flags.includes('--proof10')?'../fixtures/lower_property_proof10.jsonl':'../fixtures/lower_property_source.jsonl');if(flags.includes('--stdout')){process.stdout.write(output);return;}if(flags.includes('--check'))assert.equal(fs.readFileSync(destination,'utf8'),output);else fs.writeFileSync(destination,output);console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target).length,version:lock?'9.1.2':'9.3.1',summary:records.map(r=>({name:r.name,result:r.result,properties:[...r.state.propertyCache],unhandled:r.state.unhandled}))},null,2));}).catch(error=>{console.error(error);process.exitCode=1;});
"####;

fn source() -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, "--", "--stdout"])
        .env("PROPERTY_REPO", env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn thrown(e: &tradelayer_core::validity_source::SourceException) -> V {
    obj(&[("name", s(&e.name)), ("message", s(&e.message))])
}
fn result(o: &O, raw: bool, params: V) -> V {
    if raw {
        return obj(&[(
            "outcome",
            match o {
                O::Source(SourceOutcome::Returned(v)) => v.clone(),
                O::Source(SourceOutcome::Thrown(e)) => {
                    obj(&[("params", params), ("thrown", thrown(e))])
                }
                _ => panic!("raw {o:?}"),
            },
        )]);
    }
    let (transport, outcome) = match o {
        O::Dependency(DependencyReturn::Immediate(v)) => {
            ("Immediate", obj(&[("returned", v.clone())]))
        }
        O::Dependency(DependencyReturn::Pending(v)) => ("Pending", obj(&[("returned", v.clone())])),
        O::Dependency(DependencyReturn::Throw(e)) => ("Throw", obj(&[("thrown", thrown(e))])),
        O::Dependency(DependencyReturn::PendingThrow(e)) => {
            ("PendingThrow", obj(&[("thrown", thrown(e))]))
        }
        _ => panic!("direct {o:?}"),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
#[test]
fn genuine_lower_property_issuance_grants_redemptions_and_prefixes_replay() {
    let mut replay = Replay::with_policy(P::LowerLifecycleV7).unwrap();
    let (mut resets, mut seeds, mut calls, mut refusals) = (0, 0, 0, 0);
    for case in source() {
        let name = txt(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            resets += 1;
            replay = Replay::with_policy(P::LowerLifecycleV7).unwrap();
            let options = command.get("reset");
            if let V::String(entry) = options.get("entry") {
                replay.apply(C::SetEntry(entry)).unwrap();
            }
            if options.get("inactive") == V::Bool(true) {
                replay.apply(C::SetActivation(false)).unwrap();
            }
            for table in arr(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: txt(table.get("name")),
                        rows: arr(table.get("rows")),
                    })
                    .unwrap();
            }
            continue;
        }
        if matches!(command.get("seed"), V::Object(_)) {
            seeds += 1;
            let seed = command.get("seed");
            replay
                .apply(C::Import {
                    collection: txt(seed.get("collection")),
                    rows: arr(seed.get("rows")),
                })
                .unwrap();
            continue;
        }
        if matches!(command.get("failure"), V::Object(_)) {
            let f = command.get("failure");
            let op = txt(f.get("op"));
            replay
                .apply(C::FailStorage(StorageFailure {
                    collection: txt(f.get("collection")),
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
                    message: txt(f.get("message")),
                }))
                .unwrap();
        }
        let raw = matches!(command.get("raw"), V::Array(_));
        let before = replay.runtime().state_value().unwrap();
        let receipt = replay
            .apply(if raw {
                C::ExecuteContract(arr(command.get("raw")))
            } else {
                invoke(&txt(command.get("target")), arr(command.get("args")))
            })
            .unwrap();
        if command.get("unsupported") == V::Bool(true) {
            assert_eq!(
                receipt.outcome,
                O::LocalError(SourceRuleError::UnsupportedNumericDomain),
                "{name} explicit native JSON SyntaxError domain"
            );
            assert_eq!(
                replay.runtime().state_value().unwrap(),
                before,
                "{name} local admission rollback"
            );
            assert!(receipt.effects.is_empty());
            refusals += 1;
            continue;
        }
        calls += 1;
        assert!(
            !matches!(receipt.outcome, O::LocalError(_)),
            "{name} {:?} effects={:?}",
            receipt.outcome,
            receipt.effects
        );
        let params = replay
            .runtime()
            .last_contract_parameters()
            .cloned()
            .unwrap_or(V::Undefined);
        assert_eq!(
            result(&receipt.outcome, raw, params),
            case.get("result"),
            "{name} exact source outcome"
        );
        let state = replay.runtime().state_value().unwrap();
        let expected = case.get("state");
        for table in arr(expected.get("tables")) {
            assert_eq!(
                V::Array(
                    replay
                        .runtime()
                        .collection(&txt(table.get("name")))
                        .to_vec()
                ),
                table.get("rows"),
                "{name} persisted {:?}",
                table.get("name")
            );
        }
        for (actual, wanted) in [
            (state.get("tally"), "tally"),
            (
                state.get("globalClearing").get("tallyStaticInitialized"),
                "tallyInitialized",
            ),
            (state.get("globalClearing").get("tallyMod"), "tallyMod"),
            (
                state.get("globalClearing").get("propertyCache"),
                "propertyCache",
            ),
            (
                state.get("proofLifecycle").get("propertyInitialized"),
                "propertyInitialized",
            ),
            (
                state.get("globalClearing").get("unhandledRejections"),
                "unhandled",
            ),
        ] {
            assert_eq!(actual, expected.get(wanted), "{name} live source {wanted}");
        }
        let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","activation"].contains(&t.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")||t.starts_with("Property.")||t.starts_with("Procedural.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else if e.get("kind")==s("dependency")&&e.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",e.get("args")),("returned",e.get("returned"))]))}
            else{None}
        }).collect::<Vec<_>>();
        assert_eq!(
            V::Array(effects),
            case.get("effects"),
            "{name} complete ordered DB/environment/observed-source calls"
        );
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(
            Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay,
            "{name} full real replay/cache/failure prefix"
        );
    }
    assert_eq!((resets, seeds, calls, refusals), (26, 1, 109, 1));
}
#[test]
fn lower_property_new_routes_and_raws_stay_unavailable_to_all_older_policies() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        for method in ["tokenIssue", "grantManagedToken", "redeemManagedToken"] {
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: format!("Logic.{method}"),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
        for kind in [1., 11., 12.] {
            let before = runtime.clone();
            assert_eq!(
                runtime.execute_contract_unverified(&[
                    s("original-tx"),
                    n(kind),
                    s("tl"),
                    s(""),
                    s("actor"),
                    V::Null,
                    n(0.),
                    n(0.),
                    n(10.)
                ]),
                Err(SourceRuleError::InvalidInput)
            );
            assert_eq!(runtime, before);
        }
    }
}
#[test]
fn lower_property_shared_cache_capacity_is_a_local_atomic_refusal() {
    let mut runtime = SourceContractRuntime::with_policy(P::LowerLifecycleV7);
    let pairs = V::Array(
        (1..=1024)
            .map(|id| {
                V::Array(vec![
                    n(f64::from(id)),
                    obj(&[
                        ("ticker", s(&format!("P{id}"))),
                        ("totalInCirculation", n(1.)),
                        ("type", n(1.)),
                        ("issuer", s("actor")),
                    ]),
                ])
            })
            .collect(),
    );
    // Literal Property.load admits an already-parsed array too. This reaches
    // map capacity without raising the separate JSON parser byte/node bounds.
    runtime
        .import_collection(
            "propertyList",
            vec![obj(&[("_id", s("propertyIndex")), ("value", pairs)])],
        )
        .unwrap();
    runtime
        .invoke_dependency(&DependencyCall {
            target: "Property.load".into(),
            arguments: vec![],
        })
        .unwrap();
    runtime.take_effects();
    let before = runtime.clone();
    assert_eq!(
        runtime.invoke_dependency(&DependencyCall {
            target: "Logic.tokenIssue".into(),
            arguments: vec![
                s("actor"),
                n(1.),
                s("NEW"),
                s(""),
                n(0.),
                V::Bool(false),
                s(""),
                V::Bool(false),
                n(10.)
            ]
        }),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(runtime, before);
}
