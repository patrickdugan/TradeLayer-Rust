//! Actual type30 stake/fraud/relay source graph over the existing sole Core stores.
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
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array")
    };
    value
}
fn text(value: V) -> String {
    let V::String(value) = value else {
        panic!("string")
    };
    value
}
fn returned(value: DependencyReturn) -> V {
    let (transport, outcome) = match value {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
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
const REFERENCE: &str = r###"
'use strict';
// Audit only: unchanged pinned business modules; real shared in-memory DB calls.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=__dirname,root=process.env.PROOF_REPO||path.resolve(stage,'../../outputs/TradeLayer-Rust'),flags=process.argv.slice(2),lock=flags.includes('--lock');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],code=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(code.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
const extraPins={procedural:['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'],bitvmCache:['legacy_bitvm_cache_source.txt','053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca'],bitvmRisk:['legacy_bitvm_risk_source.txt','14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66'],bitvmBundle:['legacy_bitvm_bundle_source.txt','e63a2f2e09e195a03ab2e3acb3fb6f582917c628a4c08774911c4ba6558903fe']};
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`Object.assign(pins,${JSON.stringify(extraPins)});\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache','bitvmBundle','bitvmRisk'","'vaults'");
replace('return {types,logic,Orderbook,effects','return {context,rawArtifacts,storage,tables,sources,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
replace('const adapters={','const rawArtifacts=new Map();\n  const adapters={');
replace("'fs':forbidden('fs')","'fs':{...fs,readFileSync(p,encoding){if(!rawArtifacts.has(String(p)))return fs.readFileSync(p,encoding);const raw=Buffer.from(rawArtifacts.get(String(p)));effects.push({kind:'artifact-read',path:String(p),encoding,rawHex:raw.toString('hex')});return encoding?raw.toString(encoding):raw;}}");
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',records=[];let e,lastParams,configuredArtifacts={};
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),O=e.load('oracle'),C=e.load('contractRegistry'),P=e.load('property');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,oracleWatermarks:O.lastOracleUpdateBlock,contractMod:C.modFlag,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,environment:{...e.context.process.env},artifacts:configuredArtifacts,lastParams,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result,state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation','artifact-read'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function');target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
async function configure(name,environment){e.effects.length=0;e.context.process.env={...environment};emit(name,{environment},{outcome:{returned:undefined}});}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;configuredArtifacts={};for(const method of ['processStakeFraudProof','applyProceduralOracleSettlement'])observe(e.logic,method,'Logic.'+method);for(const method of ['recordStake','getStake','applyFraudProof','relayTradeLayerState'])observe(e.load('oracle'),method,'Oracle.'+method);for(const method of ['parseRelayBlob','validateRelayBundle'])observe(e.load('dlcOracleBridge'),method,'Dlc.'+method);for(const method of ['getInstance','load','getPropertyData','getPropertyIndex'])observe(e.load('property'),method,'Property.'+method);for(const method of ['save','grantTokens','redeemTokens'])observe(e.load('property').prototype,method,'Property.'+method);for(const [module,field,label,methods]of [['procedural','ProceduralRegistry','Procedural',['transitionContract','_base']],['bitvmCache','BitvmCacheRegistry','BitvmCache',['_base','_challengeBlocks','buildCacheId','get','open','challenge','finalize','resolve']],['bitvmRisk',null,'BitvmRisk',['onCacheOpen','onEscrowRelease','onSweep']]]){const object=field?e.load(module)[field]:e.load(module);for(const method of methods)observe(object,method,label+'.'+method);}await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [module,method]=target.split('.'),receiver=module==='Logic'?e.logic:module==='Oracle'?e.load('oracle'):module==='Tally'?e.load('tally'):module==='Property'?e.load('property'):e.load('dlcOracleBridge');let transport='Immediate',outcome;try{let value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure,unsupported},{transport,outcome});}
async function raw(name,payload,sender=A,failure,inactive=false){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,30,'tl',payload,sender,null,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(30,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
const amount=v=>(v*1e8).toString(36),stake=(value=1)=>({action:0,oracleId:1,stakedPropertyId:7,amount:value}),fraud=(value=0.5)=>({action:1,oracleId:1,stakedPropertyId:7,amount:value,accusedAddress:A,evidenceHash:'evidence'}),relay=(extra={})=>({action:2,oracleId:1,relayType:0,stateHash:'state',...extra});
(async()=>{
for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 await raw(entry+'-raw-stake','0,1,7,'+amount(1));
 await call(entry+'-direct-stake','Logic.processStakeFraudProof',[A,stake(0.25),11]);
 await call(entry+'-getStake','Oracle.getStake',[1,A]);
 await raw(entry+'-raw-fraud','1,1,7,'+amount(0.5)+','+A+',evidence',B);
 await call(entry+'-direct-fraud','Logic.processStakeFraudProof',[B,fraud(0.25),12]);
 await call(entry+'-absent-fraud','Logic.processStakeFraudProof',[B,{...fraud(1),accusedAddress:'missing'},12]);
 await raw(entry+'-raw-relay','2,1,0,0,,,0,state');
 await call(entry+'-direct-relay','Logic.processStakeFraudProof',[A,relay({relayType:2}),13]);
 await call(entry+'-backup-relay','Logic.processStakeFraudProof',[B,relay({relayType:3}),13]);
 await raw(entry+'-raw-outsider','2,1,0,0,,,0,state','outsider');
 await raw(entry+'-raw-invalid-action','3,1,7,1');
 await raw(entry+'-raw-invalid-oracle','0,999,7,'+amount(1));
 await raw(entry+'-raw-missing-property','0,1,99,'+amount(1));
 await raw(entry+'-raw-zero-stake','0,1,7,0');
 await raw(entry+'-raw-negative-stake','0,1,7,-'+amount(1));
 await raw(entry+'-raw-insufficient','0,1,7,'+amount(2000));
 await raw(entry+'-raw-empty-fraud','1,1,7,1');
 await raw(entry+'-raw-empty-relay','2,1,0,0');
 await raw(entry+'-raw-invalid-relay-bundle','2,1,0,0,,,0,state,,,b64:'+Buffer.from('{}').toString('base64'));
 await call(entry+'-null-params','Logic.processStakeFraudProof',[A,null,14]);
 await call(entry+'-missing-amount','Logic.processStakeFraudProof',[A,{action:0,oracleId:1,stakedPropertyId:7},14]);
 await call(entry+'-unknown-action','Logic.processStakeFraudProof',[A,{action:3},14]);
 await call(entry+'-nan-action-falls-back-to-stake','Logic.processStakeFraudProof',[A,{...stake(0.1),action:NaN},14]);
 await call(entry+'-negative-fraud-increases-source-stake','Logic.processStakeFraudProof',[B,fraud(-0.25),15]);
 await call(entry+'-empty-settlement-helper','Logic.applyProceduralOracleSettlement',[A,{},15]);
}
for(const [name,target,args,collection,op,skip]of[
 ['stake-database','Logic.processStakeFraudProof',[A,stake(1),10],'oracleData','getDatabase',0],
 ['stake-read','Logic.processStakeFraudProof',[A,stake(1),10],'oracleData','findOne',0],
 ['stake-write','Logic.processStakeFraudProof',[A,stake(1),10],'oracleData','update',0],
 ['fraud-first-write','Logic.processStakeFraudProof',[B,fraud(0.5),11],'oracleData','update',0],
 ['fraud-second-write','Logic.processStakeFraudProof',[B,fraud(0.5),11],'oracleData','update',1],
 ['relay-write','Logic.processStakeFraudProof',[A,relay(),11],'oracleData','update',0],
]){await reset('failure-'+name);if(name.startsWith('fraud'))await call(name+'-setup','Logic.processStakeFraudProof',[A,stake(1),10]);const failure={collection,op,skip,message:name};await call(name+'-failed-prefix',target,args,failure);await call(name+'-retry',target,args);}
await reset('oracle-direct');
await call('direct-recordStake-string','Oracle.recordStake',['feed','address',7,'0.123456789',1]);
await call('direct-recordStake-sum','Oracle.recordStake',['feed','address',7,'0.000000019',2]);
await call('direct-getStake','Oracle.getStake',['feed','address']);
await call('direct-fraud-slice-and-round','Oracle.applyFraudProof',['feed','address','claim',0.023456789,'abcdefghijklmnopqrstuvwxyz012345',3]);
await call('direct-fraud-no-record','Oracle.applyFraudProof',['feed','missing','claim',1,'evidence',4]);
await call('direct-relay-no-settlement','Oracle.relayTradeLayerState',[1,A,0,'state','dlc',5]);
await call('direct-signature-record','Oracle.relayTradeLayerState',[1,A,0,'state','dlc',6,JSON.stringify({signatureHex:'aabb'})]);
await call('direct-signature-repeat','Oracle.relayTradeLayerState',[1,A,1,'state','dlc',7,JSON.stringify({signatureHex:'AABB'})]);
await call('direct-signature-replay-error','Oracle.relayTradeLayerState',[1,A,0,'changed','dlc',8,JSON.stringify({signatureHex:'aabb'})]);
await call('direct-dlc-parse-empty','Dlc.parseRelayBlob',['']);
await call('direct-dlc-parse-malformed','Dlc.parseRelayBlob',['{']);
await call('direct-dlc-validate-missing','Dlc.validateRelayBundle',[null,'state']);
await reset('unsupported-domains');
await call('unsupported-procedural-transition','Logic.processStakeFraudProof',[A,relay({dlcRef:'next',settlementState:'OPEN'}),10],undefined,false);
await call('unsupported-settlement-object','Logic.processStakeFraudProof',[A,relay({relayBlob:JSON.stringify({settlement:{mode:'redemption',propertyId:7,amount:1}})}),11],undefined,false);

const settled=(mode,fields={})=>relay({relayBlob:JSON.stringify({settlement:{mode,propertyId:7,amount:1,...fields}})});
const settle=async(name,mode,fields={},block=20,sender=A,failure)=>call(name,'Logic.processStakeFraudProof',[sender,settled(mode,fields),block],failure);
await reset('ordinary-settlement');
await call('property-static-index-initializes-same-cache','Property.getPropertyIndex',[]);
await settle('payout-property-error-before-cache-coercion','payout',{propertyId:0,cacheId:{toString:1}});
for(const [mode,fields]of [['redemption',{}],['redeem',{amount:.000000019}],['rollover',{nextPropertyId:3,toAddress:B}],['roll',{propertyId:999,nextPropertyId:3}],['roll',{nextPropertyId:999}],['redemption',{amount:2000000}],['redemption',{amount:0}],['rollover',{nextPropertyId:0}],['redemption',{propertyId:0}],['unknown',{}],['pnl_sweep',{toAddress:B}],['pnlsweep',{amount:0}],['sweep',{amount:-1,toAddress:B}],['sweep',{amount:10000}],['bitvm_challenge',{}],['bitvm_payout',{}],['bitvm_resolve',{}],['resolve',{cacheId:'missing',resolverAddress:B}],['challenge',{cacheId:'missing'}],['payout',{cacheId:'missing'}],['resolve',{cacheId:'missing'}]]) await settle('ordinary-'+records.length,mode,fields);
for(const verdict of ['uphold','reject','payout']){
 await reset('cache-lifecycle-'+verdict);
 await settle('cache-open-'+verdict,'cache',{cacheId:verdict,amount:10,toAddress:B,cacheBondAmount:2,challengeBlocks:2});
 if(verdict==='payout'){
  await settle('cache-early-payout','payout',{cacheId:verdict,amount:10,toAddress:B},21);
  await settle('cache-valid-payout','payout',{cacheId:verdict,amount:10,toAddress:B},22);
  await settle('cache-double-payout','payout',{cacheId:verdict,amount:10,toAddress:B},23);
 }else{
  await settle('cache-challenge-'+verdict,'challenge',{cacheId:verdict,challengeBondAmount:3,evidenceHash:'e'},21,B);
  await settle('cache-blocked-payout-'+verdict,'payout',{cacheId:verdict,amount:10,toAddress:B},23);
  await settle('cache-resolve-'+verdict,'resolve',{cacheId:verdict,verdict},24);
  await settle('cache-after-resolve-payout-'+verdict,'payout',{cacheId:verdict,amount:10,toAddress:B},25);
 }
}
await reset('procedural-transition-roll');
await call('procedural-transition-autoRoll','Logic.processStakeFraudProof',[A,relay({dlcRef:'old',settlementState:'settled',autoRoll:true,nextDlcRef:'new'}),30]);
await call('procedural-transition-replace','Logic.processStakeFraudProof',[A,relay({dlcRef:'old',settlementState:'closed'}),31]);
for(const [name,mode,fields,collection,op,skip]of[
 ['redeem-get','redeem',{},'propertyList','findOne',0],['redeem-load','redeem',{},'propertyList','findOne',1],['redeem-save','redeem',{},'propertyList','update',0],['redeem-tally-save','redeem',{},'tallyMap','update',0],['rollover-second-read','rollover',{nextPropertyId:3},'propertyList','findOne',2],['cache-risk-first','cache',{cacheId:'failure'},'procedural','update',0],['cache-risk-second','cache',{cacheId:'failure'},'procedural','update',1],['cache-record','cache',{cacheId:'failure'},'procedural','update',3],['cache-second-tally','cache',{cacheId:'failure'},'tallyMap','update',1]
]){await reset('failure-settlement-'+name);await settle(name+'-failure',mode,fields,30,A,{collection,op,skip,message:name});await settle(name+'-retry',mode,fields,31);}
await reset('risk-environment');await configure('sweep-cap',{TL_BITVM_MAX_SWEEP_PER_WINDOW:'3'});await settle('sweep-with-counter','sweep',{amount:2,toAddress:B});await settle('sweep-window-cap-reject','sweep',{amount:2,toAddress:B});await configure('cache-cap',{TL_BITVM_MAX_PENDING_ESCROW:'3'});await settle('cache-global-cap','cache',{amount:4});
await reset('state-root-environment');await configure('enable-state-root',{TL_ORACLE_REQUIRE_STATE_ROOT:'1'});await settle('state-root-missing-fields','redeem');
const payload=Buffer.from(JSON.stringify({stateRoot:'root'})),payloadHash=crypto.createHash('sha256').update(payload).digest('hex');
await call('state-root-valid-redemption','Logic.processStakeFraudProof',[A,relay({relayBlob:JSON.stringify({payloadHash,balancePayloadB64:payload.toString('base64'),settlement:{mode:'redeem',propertyId:7,amount:1,stateRoot:'root'}})}),40]);
await call('state-root-transition-mismatch','Logic.processStakeFraudProof',[A,relay({relayBlob:JSON.stringify({payloadHash,balancePayloadB64:payload.toString('base64'),settlement:{mode:'redeem',propertyId:7,amount:1,transitionHash:'bad'}})}),41]);
await configure('binohash-prior-source-guards',{TL_ORACLE_REQUIRE_STATE_ROOT:'1',TL_ORACLE_STATE_COMMIT_SCHEME:'binohash'});
for(const [name,payloadDoc]of [['missing-root',{stateRoot:'root'}],['different-root',{stateRoot:'root',binoRoot:'different'}]]){const bytes=Buffer.from(JSON.stringify(payloadDoc));await call('binohash-'+name,'Logic.processStakeFraudProof',[A,relay({relayBlob:JSON.stringify({payloadHash:crypto.createHash('sha256').update(bytes).digest('hex'),balancePayloadB64:bytes.toString('base64'),settlement:{mode:'redeem',propertyId:7,amount:1}})}),42]);}
await reset('bundle-missing');await configure('bundle-required',{TL_BITVM_REQUIRE_BUNDLE:'1'});await settle('bundle-no-hash','cache',{cacheId:'missing-bundle'});
const artifactPath=path.resolve(root,'../../work/proof-artifacts/bundle.json');
const bundleDoc={selectedBucketPct:12,sourceHashes:{source:'hash'}},bundleHash=e.load('bitvmBundle').computeBundleHash(bundleDoc);bundleDoc.bundleHash=bundleHash;
const artifactText=JSON.stringify(bundleDoc),artifacts={[artifactPath]:Buffer.from(artifactText).toString('hex')};e.rawArtifacts.set(artifactPath,Buffer.from(artifactText));configuredArtifacts=artifacts;e.effects.length=0;emit('artifact-input',{artifacts},{outcome:{returned:undefined}});
await settle('bundle-valid-cache','cache',{cacheId:'bundle-valid',bundleHash,bundlePath:artifactPath});
await settle('bundle-mismatch-claim','cache',{cacheId:'bundle-mismatch',bundleHash:'ff',bundlePath:artifactPath});
const invalidArtifact=JSON.stringify({...bundleDoc,selectedBucketPct:13});e.rawArtifacts.set(artifactPath,Buffer.from(invalidArtifact));configuredArtifacts={[artifactPath]:Buffer.from(invalidArtifact).toString('hex')};e.effects.length=0;emit('artifact-invalid-content-input',{artifacts:configuredArtifacts},{outcome:{returned:undefined}});
await settle('bundle-invalid-content','cache',{cacheId:'bundle-invalid-content',bundleHash,bundlePath:artifactPath});
await call('bundle-missing-artifact-explicit-local-boundary','Logic.processStakeFraudProof',[A,settled('cache',{cacheId:'missing-artifact',bundleHash,bundlePath:artifactPath+'.missing'}),42],undefined,true);

for(const record of records)console.log(JSON.stringify(tag(record)));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1});
"###;
fn cases(lock: bool) -> Vec<V> {
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .args([
            "-e",
            REFERENCE,
            "proof-reference",
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
fn command(value: &V) -> C {
    for (field, target) in [
        ("environment", "SourceEnvironment.configure"),
        ("artifacts", "SourceArtifacts.configure"),
    ] {
        if value.get(field) != V::Undefined {
            return C::Invoke(DependencyCall {
                target: target.into(),
                arguments: vec![value.get(field)],
            });
        }
    }
    if let V::Array(args) = value.get("raw") {
        C::ExecuteContract(args)
    } else {
        C::Invoke(DependencyCall {
            target: text(value.get("target")),
            arguments: array(value.get("args")),
        })
    }
}
fn actual(runtime: &mut SourceContractRuntime, case: &V) -> V {
    let c = case.get("command");
    if c.get("environment") != V::Undefined || c.get("artifacts") != V::Undefined {
        let C::Invoke(call) = command(&c) else {
            panic!("boundary configure")
        };
        assert_eq!(
            runtime.invoke_dependency(&call).unwrap(),
            DependencyReturn::Immediate(V::Undefined)
        );
        return obj(&[("outcome", obj(&[("returned", V::Undefined)]))]);
    }
    if let Some(f) = fault(&c) {
        runtime.fail_storage(f).unwrap();
    }
    if let V::Array(args) = c.get("raw") {
        let result = match runtime.execute_contract_unverified(&args).unwrap() {
            SourceOutcome::Returned(value) => value,
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
        let name = text(table.get("name"));
        known.insert(name.clone());
        assert_eq!(
            V::Array(runtime.collection(&name).to_vec()),
            table.get("rows"),
            "{name} complete shared rows"
        );
    }
    let state = runtime.state_value().unwrap();
    for table in array(state.get("tables")) {
        if !known.contains(&text(table.get("name"))) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} no invented rows"
            );
        }
    }
    let proof = state.get("proofLifecycle");
    assert_eq!(
        proof.get("propertyInitialized"),
        expected.get("propertyInitialized"),
        "{name} property constructor"
    );
    assert_eq!(
        proof.get("environment"),
        expected.get("environment"),
        "{name} original environment input"
    );
    assert_eq!(
        proof.get("artifacts"),
        expected.get("artifacts"),
        "{name} raw artifact input"
    );
    let global = state.get("globalClearing");
    for (key, wanted) in [
        ("tallyStaticInitialized", "tallyInitialized"),
        ("tallyMod", "tallyMod"),
        ("contractModRaw", "contractMod"),
        ("propertyCache", "propertyCache"),
    ] {
        assert_eq!(global.get(key), expected.get(wanted), "{name} {key}");
    }
    assert_eq!(
        runtime.tally_cache(),
        expected.get("tally"),
        "{name} sole Tally"
    );
    for (key, wanted) in [
        ("oracleSourceCache", "oracleCache"),
        ("oracleSourceInitialized", "oracleInitialized"),
        ("oracleSourceWatermarks", "oracleWatermarks"),
    ] {
        assert_eq!(state.get(key), expected.get(wanted), "{name} {key}");
    }
    assert_eq!(
        runtime.margins().state_value().unwrap().get("instances"),
        V::Array(vec![]),
        "{name} no duplicate Margin state"
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
    V::Array(values.into_iter().filter_map(|v|{
    if matches!(v.get("kind"),V::String(k)if["database","db","uuid","random","rpc-clock","artifact-read"].contains(&k.as_str())){Some(v)}
    else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if["Logic.processStakeFraudProof","Logic.applyProceduralOracleSettlement","Oracle.recordStake","Oracle.getStake","Oracle.applyFraudProof","Oracle.relayTradeLayerState","Dlc.parseRelayBlob","Dlc.validateRelayBundle","Property.getInstance","Property.load","Property.getPropertyData","Property.getPropertyIndex","Property.save","Property.grantTokens","Property.redeemTokens","Procedural.transitionContract","Procedural._base","BitvmCache._base","BitvmCache._challengeBlocks","BitvmCache.buildCacheId","BitvmCache.get","BitvmCache.open","BitvmCache.challenge","BitvmCache.finalize","BitvmCache.resolve","BitvmRisk.onCacheOpen","BitvmRisk.onEscrowRelease","BitvmRisk.onSweep"].contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
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
    for table in array(case.get("state").get("tables")) {
        runtime
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
}
#[test]
fn unchanged_type30_graph_both_genuine_numeric_versions() {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true));
    let mut runtime = SourceContractRuntime::with_policy(P::ProofLifecycleV6);
    let (mut resets, mut supported, mut unsupported) = (0, 0, 0);
    for case in &baseline {
        let c = case.get("command");
        let name = text(case.get("name"));
        if let V::Object(_) = c.get("reset") {
            resets += 1;
            runtime = SourceContractRuntime::with_policy(P::ProofLifecycleV6);
            setup(&mut runtime, case);
        } else if c.get("unsupported") == V::Bool(true) {
            unsupported += 1;
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: text(c.get("target")),
                    arguments: array(c.get("args"))
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime,before,"{name} explicit local admission rollback; source effects are not claimed equivalent");
            continue;
        } else {
            supported += 1;
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
            "{name} full ordered genuine effects"
        );
    }
    assert_eq!(
        (baseline.len(), resets, supported, unsupported),
        (234, 29, 204, 1)
    );
}
#[test]
fn real_failure_prefixes_restore_then_continue_with_no_conserving_repair() {
    let records = cases(false);
    for name in records.iter().filter_map(|case| {
        let name = text(case.get("name"));
        if case.get("command").get("reset") != V::Undefined
            && (name.starts_with("failure-")
                || name.starts_with("cache-lifecycle-")
                || name == "procedural-transition-roll"
                || name.ends_with("environment")
                || name == "bundle-missing")
        {
            Some(name)
        } else {
            None
        }
    }) {
        let start = records
            .iter()
            .position(|r| r.get("name") == s(&name))
            .unwrap();
        let mut replay = SourceContractReplay::with_policy(P::ProofLifecycleV6).unwrap();
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
            if c.get("unsupported") == V::Bool(true) {
                continue;
            }
            if let Some(f) = fault(&c) {
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay.apply(command(&c)).unwrap();
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "actual source command has a source receipt"
            );
            assert_state(replay.runtime(), case);
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(&bytes[..5], b"TLCR\x06");
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(restored, replay);
            assert_eq!(restored.checkpoint().unwrap(), bytes);
            replay = restored;
        }
    }
}
