#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.env.UTXO_TRADE_REPO||path.resolve(__dirname,'../../../outputs/TradeLayer-Rust'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {adapters,context,storage,tables,unsupported,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const activationStart=code.indexOf("'./db':storage,'./activation':"),activationEnd=code.indexOf("    './txUtils':",activationStart);assert.ok(activationStart>=0&&activationEnd>activationStart);code=code.slice(0,activationStart)+"'./db':storage, './client':forbidden('ClientWrapper'),\n"+code.slice(activationEnd);
replace('const sources = Object.fromEntries(',`pins.activation=['native_genesis/source/activation.js','268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e'];\nconst sources = Object.fromEntries(`);
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);

const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('channels');return{activation:{...e.load('activation').instance},tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,channels:C.channelsRegistry,withdrawals:C.pendingWithdrawals,lastParams,unhandled,unsupported:e.unsupported.slice()};}
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
 const activation=e.load('activation').getInstance(),registry=activation.initializeTxRegistry();registry[3].active=!options.inactive;registry[3].activationBlock=1;e.table('activations').push({_id:'activationsList',value:JSON.stringify(registry)});
 if(options.tallyRows)e.table('tallyMap').splice(0,Infinity,...structuredClone(options.tallyRows));
 if(options.rpc){e.table('sourceRpcTransactions').push({txid:'raw4-first-commit',transaction:{vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}});bindRpc();}
 for(const method of ['tradeTokenForUTXO'])observe(e.logic,method,'Logic.'+method);
 for(const method of ['getInstance','getTally','loadFromDB','updateBalance','updateChannelBalance','getAddressBalances','hasSufficientChannel'])observe(e.load('tally'),method,'Tally.'+method);
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

const refs=[{vout:0,address:A},{vout:1,address:B}];
const payload=(quantity='2',property='7',sats='rs',tag='')=>[property,quantity==='!'?'!':amount(Number(quantity)),'1',sats,'1','0','0',tag].join(',');
(async()=>{
 await reset('initial-ten');
 await raw('raw3-no-references',3,payload(),A,[]);
 await raw('raw3-null-references',3,payload(),A,null);
 await raw('raw3-object-references',3,payload(),A,{});
 await raw('raw3-nonmatching-vouts',3,payload(),A,[{vout:8,address:A}]);
 await raw('raw3-zero-amount',3,payload('0'),A,refs);
 await raw('raw3-positive-omitted-outputs',3,payload(),A,refs);
 await raw('raw3-negative-amount',3,payload('-1'),A,refs);
 await raw('raw3-nan-amount',3,payload('!'),A,refs);
 await raw('raw3-string-withdraw-tag',3,payload('2','7','rs','0'),A,refs);
 await call('direct-utxo-trade-missing-channel','Logic.tradeTokenForUTXO',[A,B,7,2,true,1000,B,1000,500,1,null,10,'direct-missing-channel']);
 if(!flags.includes('--proof10')){
  await raw('raw3-vout-string-is-not-number',3,payload(),A,[{vout:'0',address:A},{vout:'1',address:B}]);
  await raw('raw3-payment-reference-only',3,payload(),A,[refs[0]]);
  await raw('raw3-token-reference-only',3,payload(),A,[refs[1]]);
  await raw('raw3-null-reference-row',3,payload(),A,[null]);
  await raw('raw3-reversed-references',3,payload(),A,[refs[1],refs[0]]);
  await raw('raw3-first-duplicate-reference-wins',3,payload('0'),A,[refs[0],{vout:0,address:B},refs[1]]);
  await raw('raw3-empty-payload-defaults',3,'',A,[refs[0]]);
  await raw('raw3-unknown-property-zero',3,payload('0','rr'),A,refs);
  await raw('raw3-unknown-property-positive',3,payload('2','rr'),A,refs);
  await raw('raw3-invalid-sats',3,payload('2','7','!'),A,refs);
  await raw('raw3-decimal-output-index',3,'7,'+amount(2)+',1,rs,10,0,0,',A,[refs[0],{vout:10,address:B}]);
  await raw('raw3-noninteger-token-index-defaults',3,'7,'+amount(2)+',1,rs,x,0,0,',A,refs);
  await reset('raw3-inactive',{inactive:true});
  await raw('raw3-inactive-zero',3,payload('0'),A,refs);
  await raw('raw3-inactive-positive-source-continues',3,payload(),A,refs);
  await reset('raw3-channel-backed',{tallyRows:[{_id:'tallyMap',data:JSON.stringify([[A,{7:{amount:10,available:0,reserved:0,margin:0,vesting:0,channelBalance:10}}]])}]});
  await raw('raw3-available-channel-still-omits-outputs',3,payload(),A,refs);
  await reset('raw3-missing-tally',{tallyRows:[]});
  await raw('raw3-absent-channel-undefined-shortfall',3,payload(),A,refs);
  for(const f of [{collection:'tallyMap',op:'getDatabase'},{collection:'tallyMap',op:'findOne'},{collection:'propertyList',op:'getDatabase'},{collection:'propertyList',op:'findOne'}]){
   const label=f.collection+'-'+f.op;await reset('raw3-failure-'+label);await raw('raw3-failed-read-'+label,3,payload(),A,refs,{...f,message:label});await raw('raw3-failed-read-retry-'+label,3,payload(),A,refs);
  }
 }
})().then(()=>{
 const output=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n';
 if(flags.includes('--stdout')){process.stdout.write(output);return;}
 const destination=path.resolve(__dirname,flags.includes('--proof10')?'../fixtures/lower_utxo_trade_proof10.jsonl':'../fixtures/lower_utxo_trade_source.jsonl');
 if(flags.includes('--check'))assert.equal(fs.readFileSync(destination,'utf8'),output);else fs.writeFileSync(destination,output);
 console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target).length,version:lock?'9.1.2':'9.3.1',summary:records.map(r=>({name:r.name,result:r.result}))},null,2));
}).catch(error=>{console.error(error);process.exitCode=1;});
