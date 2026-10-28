//! Unchanged pinned send/commit graph over the sole actual Core stores.
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
const root=process.env.SEND_COMMIT_REPO||path.resolve(__dirname,'../../../outputs/TradeLayer-Rust'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {adapters,context,storage,tables,unsupported,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);

const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('channels');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,channels:C.channelsRegistry,withdrawals:C.pendingWithdrawals,lastParams,unhandled,unsupported:e.unsupported.slice()};}
function emit(name,command,result){const snapshot=e.copy(state());if(!command.unsupported)assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','rpc','activation'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
function bindRpc(){
 const source=fs.readFileSync(path.join(root,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(root,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+source+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve({})}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 // This is the pre-existing explicit raw-RPC boundary profile. Original raw
 // transaction documents drive unchanged pure classifier/pubkey methods.
 Object.assign(e.adapters['./txUtils'],{getAddressTypeUniversal:own.exports.getAddressTypeUniversal,extractPubkeyByType:own.exports.extractPubkeyByType,async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'TxUtils.getRawTransaction',args,returned:structuredClone(value)});return value;}});
}
async function reset(name,options={}){
 e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;unhandled=[];
 if(options.tallyRows)e.table('tallyMap').splice(0,Infinity,...structuredClone(options.tallyRows));
 if(options.rpc){e.table('sourceRpcTransactions').push({txid:'raw4-first-commit',transaction:{vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}});bindRpc();}
 for(const method of ['sendToken','sendSingle','sendAll','processSend','vestingSend','calculateVestingMovement','roundToEightDecimals','commitToken'])observe(e.logic,method,'Logic.'+method);
 for(const method of ['getInstance','getTally','loadFromDB','updateBalance','updateChannelBalance','getAddressBalances'])observe(e.load('tally'),method,'Tally.'+method);
 observe(e.load('tally').prototype,'getAddressBalances','Tally.instance.getAddressBalances');
 for(const method of ['getChannel','loadChannelsRegistry','recordCommitToChannel','handleChannelPubkey','predictColumnForAddress','assignColumnBasedOnAddress','resolveColumn','recordChannelDelta','saveChannelsRegistry','recordParticipantChange'])observe(e.load('channels'),method,'Channels.'+method);
 await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});
}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,type,payload,sender=A,reference=null,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,reference,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [ns,method]=target.split('.'),receiver=ns==='Logic'?e.logic:e.load('tally');let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure,unsupported},{transport,outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{outcome:{returned:undefined}});}
async function bindTally(name){e.effects.length=0;e.context.tallyMap=e.load('tally');emit(name,{bindTally:true},{outcome:{returned:undefined}});}
const balance=(available,vesting=0)=>({amount:available+vesting,available,reserved:0,margin:0,vesting,channelBalance:0}),tallyRows=(available=1000,vesting=0,id=7)=>[{_id:'tallyMap',data:JSON.stringify([[A,{[id]:balance(available,vesting)}]])}];
const amount=v=>(v*1e8).toString(36),send=(v,p=7)=>'0;'+B+';'+p.toString(36)+';'+v;
(async()=>{
 await reset('first10',{rpc:true});
 await raw('raw2-funded-scalar-send',2,send('2'));
 await raw('raw2-decimal-send',2,send(amount(.12345678)+'~'));
 await raw('raw2-negative-send-invalid',2,send('-1'));
 await raw('raw2-insufficient-send-invalid',2,send((2000).toString(36)));
 await raw('raw2-sendall-rejected-before-logic',2,'1;'+B);
 await call('direct-sendall-shadowed-boolean','Logic.sendToken',[true,A,B,7,1,10]);
 await call('direct-multisend-unbound-helper','Logic.sendToken',[false,A,[B],[7],[1],10]);
 await call('direct-sendall-separate-empty-instance','Logic.sendAll',[A,B]);
 await raw('raw4-zero-commit-invalid',4,'7,0,'+B+',0,,,');
 await raw('raw4-first-commit',4,'7,'+amount(2)+','+B+',1,0,,');
if(!flags.includes('--proof10')){
for(const entry of ['types-first','tally-first','volume-first']){
 await reset('send-entry-'+entry,{entry});await raw('send-entry-raw-'+entry,2,send('1'));await call('send-entry-instance-'+entry,'Logic.sendAll',[A,B]);
}
await reset('direct-shapes');
await call('empty-array-send-noop','Logic.sendToken',[false,A,[],[],[],10]);
await call('mismatched-array-send-source-error','Logic.sendToken',[false,A,[B],[7],[1,2],10]);
await call('null-recipient-array-length-source-error','Logic.sendToken',[false,A,null,[7],[1],10]);
await call('null-property-send-noop','Logic.sendToken',[false,A,B,null,1,10]);
await call('default-unbound-process-send','Logic.processSend',[A,B,7,1,10]);
await bindTally('bind-original-static-tally');
await call('bound-static-tally-missing-method','Logic.processSend',[A,B,7,1,10]);
for(const quantity of [0,-0,'0',-2,1001]){
 const label=Object.is(quantity,-0)?'minuszero':String(quantity)+(typeof quantity==='string'?'-string':'');await reset('direct-send-'+label);
 await call('direct-send-outcome-'+label,'Logic.sendSingle',[A,B,7,quantity,10]);
 await call('direct-send-retry-'+label,'Logic.sendSingle',[A,B,7,1,11]);
}
for(const f of [{collection:'tallyMap',op:'getDatabase',skip:0},{collection:'tallyMap',op:'getDatabase',skip:1},{collection:'tallyMap',op:'getDatabase',skip:2},{collection:'tallyMap',op:'findOne',skip:0},{collection:'tallyMap',op:'findOne',skip:1},{collection:'tallyMap',op:'findOne',skip:2},{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'}]){
 const label=f.collection+'-'+f.op+'-'+(f.skip??0);await reset('send-failure-'+label);await call('send-failure-prefix-'+label,'Logic.sendSingle',[A,B,7,1,10],{...f,message:label});await call('send-failure-retry-'+label,'Logic.sendSingle',[A,B,7,1,11]);
}
for(const [label,rows]of [['absent',[]],['false-data',[{_id:'tallyMap',data:false}]],['malformed',[{_id:'tallyMap',data:'bad'}]]]){
 await reset('send-cache-'+label);await call('preload-'+label,'Tally.getTally',[A,7]);await seed('replace-tally-'+label,'tallyMap',rows);await call('send-cache-prefix-'+label,'Logic.sendSingle',[A,B,7,1,10]);
}
await reset('static-tally-address-balances');await call('static-funded-balances','Tally.getAddressBalances',[A]);await call('static-missing-balances','Tally.getAddressBalances',[B]);
await seed('unknown-property-tally','tallyMap',tallyRows(3,0,999));await call('static-unknown-property-ticker-null-error','Tally.getAddressBalances',[A]);
await reset('vesting-literal-id',{tallyRows:tallyRows(10,5,3)});
await call('vesting-subwhole-rejected','Logic.vestingSend',[A,B,3,.5,10]);
await call('vesting-original-fractional-amount','Logic.sendToken',[false,A,B,3,1.5,10]);
await call('vesting-whole-follow','Logic.vestingSend',[A,B,3,2,11]);
for(const f of [{collection:'tallyMapDelta',op:'insert',skip:1},{collection:'tallyMapDelta',op:'insert',skip:2},{collection:'tallyMap',op:'update',skip:2}]){
 const label=f.collection+'-'+f.op+'-'+f.skip;await reset('vesting-failure-'+label,{tallyRows:tallyRows(10,5,3)});await call('vesting-failure-prefix-'+label,'Logic.vestingSend',[A,B,3,2,10],{...f,message:label});await call('vesting-failure-retry-'+label,'Logic.vestingSend',[A,B,3,2,11]);
}
await reset('pure-vesting-math');
for(const [name,args]of [['ordinary',[3,{available:10,vesting:7}]],['thirds',[1,{available:3,vesting:9}]],['zero-denominator',[1,{available:0,vesting:2}]],['missing-values',[1,{}]]])await call('movement-'+name,'Logic.calculateVestingMovement',args);
for(const [name,value]of [['positive',1.123456789],['negative',-1.123456789],['string','2.000000019']])await call('round-floor-'+name,'Logic.roundToEightDecimals',[value]);
for(const f of [{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMap',op:'update'},{collection:'channelDelta',op:'insert'},{collection:'channels',op:'update'}]){
 const label=f.collection+'-'+f.op;await reset('commit-failure-'+label,{rpc:true});await call('commit-failure-prefix-'+label,'Logic.commitToken',[A,B,7,2,true,[0],10,'raw4-first-commit'],{...f,message:label});await call('commit-failure-retry-'+label,'Logic.commitToken',[A,B,7,2,true,[0],11,'raw4-first-commit']);
}
await reset('commit-bound-missing-rawdoc',{rpc:true});await call('commit-bound-missing-rawdoc-prefix','Logic.commitToken',[A,B,7,2,false,[],10,'original-missing-id']);
await reset('commit-default-unbound');await call('commit-default-unbound-local-refusal','Logic.commitToken',[A,B,7,2,false,[],10,'original-unbound-id'],undefined,true);
}
})().then(()=>{const output=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n';const destination=path.resolve(__dirname,flags.includes('--proof10')?'../fixtures/lower_send_commit_proof10.jsonl':'../fixtures/lower_send_commit_source.jsonl');if(flags.includes('--stdout')){process.stdout.write(output);return;}if(flags.includes('--check'))assert.equal(fs.readFileSync(destination,'utf8'),output);else fs.writeFileSync(destination,output);console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target).length,version:lock?'9.1.2':'9.3.1',summary:records.map(r=>({name:r.name,result:r.result,tallyInstance:r.state.tallyInstance,channels:[...r.state.channels],unhandled:r.state.unhandled}))},null,2));}).catch(error=>{console.error(error);process.exitCode=1;});
"####;

fn source(lock: bool) -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, "--", "--stdout"])
        .env("SEND_COMMIT_REPO", env!("CARGO_MANIFEST_DIR"))
        .env("TL_SOURCE_BN_VERSION", if lock { "9.1.2" } else { "9.3.1" })
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
fn genuine_lower_send_commit_cases(lock: bool) {
    let mut replay = Replay::with_policy(P::MainCallableV8).unwrap();
    let (mut resets, mut seeds, mut calls, mut refusals, mut bindings) = (0, 0, 0, 0, 0);
    for case in source(lock) {
        let name = txt(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            resets += 1;
            replay = Replay::with_policy(P::MainCallableV8).unwrap();
            let options = command.get("reset");
            replay
                .apply(invoke(
                    "SourceMain.configure",
                    vec![obj(&[("activationProfile", s("declared-answer"))])],
                ))
                .unwrap();
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
            if options.get("rpc") == V::Bool(true) {
                replay
                    .apply(invoke("SourceGlobal.bindRpcTransactions", vec![]))
                    .unwrap();
            }
            continue;
        }
        if command.get("bindTally") == V::Bool(true) {
            bindings += 1;
            replay
                .apply(invoke("SourceGlobal.bindTally", vec![]))
                .unwrap();
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
                O::LocalError(SourceRuleError::UnknownDependency),
                "{name} explicit unbound RPC domain"
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
        let channels = state.get("globalClearing").get("channels");
        assert_eq!(
            channels.get("registry"),
            expected.get("channels"),
            "{name} actual Channels receiver cache"
        );
        assert_eq!(
            channels.get("pendingWithdrawals"),
            expected.get("withdrawals"),
            "{name} actual withdrawals"
        );
        assert_eq!(
            state.get("sendCommit").get("tallyInstance"),
            expected.get("tallyInstance"),
            "{name} distinct empty instance cache"
        );
        assert_eq!(
            state.get("sendCommit").get("pendingRead"),
            V::Bool(false),
            "{name} declared deferred read drained at public boundary"
        );
        let effects=receipt.effects.iter().filter_map(|e|{
            if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","rpc","activation"].contains(&t.as_str())){Some(e.clone())}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}
            else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if ["Activation.isTxTypeActive","Activation.getAdmin"].contains(&t.as_str())){Some(obj(&[("kind",s("activation")),("method",if e.get("target")==s("Activation.getAdmin"){s("getAdmin")}else{s("isTxTypeActive")}), ("args",e.get("args")),("returned",e.get("returned"))]))}
            else{None}
        }).collect::<Vec<_>>();
        assert_eq!(
            V::Array(effects),
            V::Array(
                arr(case.get("effects"))
                    .into_iter()
                    .filter(|v| v.get("kind") != s("source-function")
                        || matches!(v.get("target"),V::String(t)if t.starts_with("Logic.")))
                    .collect()
            ),
            "{name} complete ordered DB/environment/observed-source calls"
        );
        let bytes = replay.checkpoint().unwrap();
        assert_eq!(
            Replay::from_checkpoint(&bytes, replay.head()).unwrap(),
            replay,
            "{name} full real replay/cache/failure prefix"
        );
    }
    assert_eq!(
        (resets, seeds, calls, refusals, bindings),
        (33, 4, 82, 1, 1)
    );
}
#[test]
fn lower_send_commit_new_routes_and_raws_refuse_older_policies() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        for target in [
            "Logic.sendToken",
            "Logic.sendSingle",
            "Logic.sendAll",
            "Logic.processSend",
            "Logic.vestingSend",
            "Logic.calculateVestingMovement",
            "Logic.roundToEightDecimals",
            "Logic.commitToken",
            "Tally.getInstance",
            "Tally.getAddressBalances",
            "Tally.instanceGetAddressBalances",
        ] {
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: target.into(),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency),
                "{policy:?} {target}"
            );
            assert_eq!(runtime, before);
        }
        for kind in [2., 4.] {
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
fn lower_send_receiver_and_actor_domains_are_local_before_effects() {
    let mut runtime = SourceContractRuntime::with_policy(P::MainCallableV8);
    for (target, arguments) in [
        (
            "Tally.instanceGetAddressBalances",
            vec![obj(&[("sourceTallyInstance", V::Bool(true))]), s("actor")],
        ),
        (
            "Logic.sendSingle",
            vec![obj(&[]), s("receiver"), n(7.), n(1.), n(10.)],
        ),
    ] {
        let before = runtime.clone();
        assert!(runtime
            .invoke_dependency(&DependencyCall {
                target: target.into(),
                arguments
            })
            .is_err());
        assert_eq!(runtime, before);
    }
}

#[test]
fn genuine_lower_send_commit_archived_number_dependency() {
    genuine_lower_send_commit_cases(false);
}
#[test]
fn genuine_lower_send_commit_locked_number_dependency() {
    genuine_lower_send_commit_cases(true);
}
