#!/usr/bin/env node
'use strict';
// Audit-only callable extraction. Original Main/TxIndex class bytes and receivers
// are retained; import/startup/RPC boundaries are observations, never successes.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),Module=require('node:module'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);
const flags=process.argv.slice(2),numeric=require(path.join(repo,'scripts/legacy_numeric_oracle.cjs')),proofOnly=flags.includes('--proof10');
if(flags.includes('--lock')){const packed=fs.readFileSync(path.join(repo,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(sha(raw),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');numeric.loadBigNumber=()=>{const m={exports:{}};vm.runInNewContext(raw.toString(),{module:m,exports:m.exports},{timeout:1000});return m.exports;};}
function sha(value){return crypto.createHash('sha256').update(value).digest('hex');}
const pins={
 main:['native_genesis/source/main.js','006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009'],
 txIndex:['legacy_txindex_source_txIndex.txt','095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51'],
 activation:['native_genesis/source/activation.js','268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e'],
 vaults:['legacy_vaults_source.txt','3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983'],
};
const source=Object.fromEntries(Object.entries(pins).map(([k,[file,hash]])=>[k,numeric.readPinnedSource(path.join(repo,'fixtures',file),hash)]));
const reference=path.join(repo,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(reference,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'reference anchor '+before);code=code.replace(before,after);}
replace('const pins = {','const pins = '+JSON.stringify({activation:pins.activation,vaults:pins.vaults}).slice(0,-1)+',');
const activationStart=code.indexOf("'./db':storage,'./activation':"),activationEnd=code.indexOf("    './txUtils':",activationStart);assert.ok(activationStart>=0&&activationEnd>activationStart);code=code.slice(0,activationStart)+"'./db':storage, './client':forbidden('ClientWrapper'),\n"+code.slice(activationEnd);
const txutilsStart=code.indexOf("    './txUtils':"),txutilsEnd=code.indexOf("'node-fetch':",txutilsStart);assert.ok(txutilsEnd>txutilsStart);code=code.slice(0,txutilsStart)+"    './txUtils':new Proxy({},{get(targetObject,key){if(Object.hasOwn(targetObject,key))return targetObject[key];return function(...args){const target='TxUtils.'+String(key),error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});effects.push({kind:'unavailable',target,args:copy(args),thrown:{name:error.name,message:error.message}});throw error;};}}),"+code.slice(txutilsEnd);
replace("'scaling','vaults','procedural','bitvmCache','bitvmBundle','bitvmRisk',","");
const extraPins={scaling:['legacy_scaling_source.txt','ea2103b70dd051d39e44c9201a1eed2107b8110ac9803a0b825b92942799ca61'],procedural:['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'],bitvmCache:['legacy_bitvm_cache_source.txt','053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca'],bitvmRisk:['legacy_bitvm_risk_source.txt','14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66'],bitvmBundle:['legacy_bitvm_bundle_source.txt','e63a2f2e09e195a03ab2e3acb3fb6f582917c628a4c08774911c4ba6558903fe']};
replace('const sources = Object.fromEntries(',`Object.assign(pins,${JSON.stringify(extraPins)});\nconst sources = Object.fromEntries(`);
replace('const adapters={','const rawArtifacts=new Map();\n  const adapters={');
replace("'path':path","'path':path.posix");
replace('else throw Error(`Unsupported storage update ${op}`);',"else if(op==='$push'){const previous=field(rows[i],k)??[];if(!Array.isArray(previous))throw Error('Cannot push to non-array');setField(rows[i],k,[...previous,v]);}else throw Error(`Unsupported storage update ${op}`);");
replace("'fs':forbidden('fs')",`'fs':{readFileSync(p,encoding){const key=String(p);if(!rawArtifacts.has(key)){const error=Object.assign(Error('Unavailable artifact input: '+key),{name:'SourceBoundaryUnavailable'});effects.push({kind:'unavailable',target:'fs.readFileSync',args:[key,encoding],thrown:{name:error.name,message:error.message}});throw error;}const raw=Buffer.from(rawArtifacts.get(key));effects.push({kind:'artifact-read',path:key,encoding,rawHex:raw.toString('hex')});return encoding?raw.toString(encoding):raw;}}`);
replace("if(op==='$gte')return actual>=wanted;","if(op==='$regex'){assert.equal(Object.prototype.toString.call(wanted),'[object RegExp]');return wanted.test(actual);}if(op==='$gte')return actual>=wanted;");
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {rawArtifacts,instanceId,adapters,context,storage,tables,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
replace("effects.push({kind:'database',name,method:'getDatabase',args:[...arguments],returned:{database:name}});",`const event={kind:'database',name,method:'getDatabase',args:[...arguments],returned:{database:name}};effects.push(event);
    if(failure&&failure.collection===name&&failure.op==='getDatabase'){
      if(failure.skip>0)failure.skip--;else{const message=failure.message;failure=null;delete event.returned;event.thrown={name:'Error',message};throw Error(message);}
    }`);
const mod=new Module(reference,module);mod.filename=reference;mod.paths=Module._nodeModulePaths(path.dirname(reference));mod._compile(code,reference);
const inventory={sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',pins,referenceDriverSha256:sha(fs.readFileSync(reference)),classes:{},methods:{},boundary:{fullMainImport:'declared unavailable ./reOrg.js; no imported module success supplied',callable:'byte-exact full Main and TxIndex classes; real constructors/prototypes; module startup IIFEs not executed',activation:'unchanged complete module, real persisted registry; init/RPC not supplied',storage:'shared explicit source fixture DB adapter; anchored /^tx/ RegExp is genuine JS test; production NeDB equivalence not claimed',rpc:'all reached ClientWrapper/TxUtils calls unavailable; no height/client success answer',observation:'original return plus finite drained turns; no generic event-loop equivalence'}};
function tag(value,seen=new Set()){
 if(value===undefined)return['undefined'];if(value===null)return['null'];if(typeof value==='string'||typeof value==='boolean')return[typeof value,value];
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return['number',Number.isNaN(value)?'NaN':value===Infinity?'Infinity':value===-Infinity?'-Infinity':b.toString('hex')];}
 if(typeof value==='function')return['function',value.name];if(Object.prototype.toString.call(value)==='[object RegExp]')return['regexp',value.source,value.flags];
 if(Object.prototype.toString.call(value)==='[object Promise]')return['promise'];if(value?.constructor?.name==='BigNumber')return['bignumber',value.valueOf()];
 if(seen.has(value))return['cycle'];seen.add(value);let result;
 if(Array.isArray(value))result=['array',value.map(v=>tag(v,seen))];else if(Object.prototype.toString.call(value)==='[object Map]')result=['map',[...value].map(([k,v])=>[tag(k,seen),tag(v,seen)])];else if(Object.prototype.toString.call(value)==='[object Set]')result=['set',[...value].map(v=>tag(v,seen))];else result=['object',Object.entries(value).map(([k,v])=>[k,tag(v,seen)])];seen.delete(value);return result;
}
const records=[];let e,m,Main,TxIndex,marginInstances,contextKeys,startup,marginReturnedObjects,settlementObserverDepth;
process.on('unhandledRejection',error=>{if(e)e.effects.push({kind:'unhandled-rejection',thrown:{name:error?.name||'Error',message:error?.message||String(error)}});});
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function classBytes(name,sourceName){const full=source[sourceName],start=full.indexOf('class '+name+' {'),end=full.lastIndexOf('module.exports = '+name);assert.ok(start>=0&&end>start);const bytes=full.slice(start,end).trimEnd();inventory.classes[name]={sha256:sha(bytes),bytes:Buffer.byteLength(bytes),fullClass:true};return bytes;}
function compileClass(name,sourceName,scope){const fn=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes(name,sourceName)+'\nreturn '+name+';})',e.context,{filename:'unchanged-class/'+sourceName+'.js',timeout:3000});return fn(...Object.values(scope));}
function unavailable(target){return function(...args){const error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});e.effects.push({kind:'unavailable',target,args:e.copy(args),thrown:{name:error.name,message:error.message}});throw error;};}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);inventory.methods[label]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
function state(){const sourceState=e.state(),A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');return{source:sourceState,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,instance:TxIndex.instance},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginIdentity:marginIdentity(),marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),vaults:{vaults:e.load('vaults').vaults,syntheticTokens:e.load('vaults').syntheticTokens},proofBoundary:{environment:{...e.context.process.env},artifacts:Object.fromEntries([...e.rawArtifacts].map(([p,b])=>[p,Buffer.from(b).toString('hex')]))},globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache,tallyMapBound:!!e.context.tallyMap,spotRpcDocumentsBound:!!e.context.spotRpcDocumentsBound},startup};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment(options.entry?{entry:options.entry}:{});marginInstances=[];contextKeys=new Set();startup=[];marginReturnedObjects=[];settlementObserverDepth=0;seedUpperInputs(options);if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
 // Observe every real receiver that reaches the original loader, including
 // the fresh constructor used by _pruneInstaLiqOrdersFromFreshBook. The
 // original method, arguments, promise, return and effects remain unchanged.
 const originalBookLoad=e.Orderbook.prototype.loadOrderBook;e.Orderbook.prototype.loadOrderBook=function(...args){e.instanceId(this);return Reflect.apply(originalBookLoad,this,args);};
 if(options.balancedSupply){const properties=JSON.parse(e.table('propertyList')[0].value);for(const[id,value]of properties)value.totalInCirculation=id===7?2000:0;e.table('propertyList')[0].value=JSON.stringify(properties);}
 const MM=e.load('marginMap'),originalLoad=MM.loadMarginMap;MM.loadMarginMap=function(...args){const result=Reflect.apply(originalLoad,this,args);result.then(v=>{if(v?.margins&&!marginInstances.includes(v))marginInstances.push(v);},()=>{});return result;};

 // Retain actual returned position objects during the source settlement body,
 // including the seller temporary that its omitted argument leaves uncached.
 // Forward original promises/receivers unchanged; no result is substituted.
 const originalPosition=MM.prototype.getPositionForAddress;
 inventory.positionReturnObservation={scope:'actual getPositionForAddress references during original processContractMatches; cached and uncached retained',sourceGetterSha256:sha(originalPosition.toString()),businessBodiesChanged:false,promisesForwardedUnchanged:true};
 MM.prototype.getPositionForAddress=function(...args){
  const track=settlementObserverDepth>0,result=Reflect.apply(originalPosition,this,args);
  if(track)result.then(value=>{if(value&&typeof value==='object'&&!marginReturnedObjects.includes(value))marginReturnedObjects.push(value);},()=>{});
  return result;
 };
 const originalSettlement=e.Orderbook.prototype.processContractMatches;
 e.Orderbook.prototype.processContractMatches=function(...args){
  settlementObserverDepth++;
  try{const result=Reflect.apply(originalSettlement,this,args);result.then(()=>{settlementObserverDepth--;},()=>{settlementObserverDepth--;});return result;}
  catch(error){settlementObserverDepth--;throw error;}
 };
 const clearing=e.load('clearing'),originalInit=clearing.initPositionCache,originalFlush=clearing.flushPositionCache;clearing.initPositionCache=function(...args){const key=Reflect.apply(originalInit,this,args);contextKeys.add(key);return key;};clearing.flushPositionCache=function(...args){const value=Reflect.apply(originalFlush,this,args);contextKeys.delete(args[0]);return value;};
 const activation=e.load('activation').getInstance();const registry=activation.initializeTxRegistry();for(const type of (options.activeTypes||[18])){registry[type].active=!options.inactive;registry[type].activationBlock=options.activationBlock??1;}e.table('activations').push({_id:'activationsList',value:JSON.stringify(registry)});const importedRows=options.rows??rows;e.table('txIndex').push(...structuredClone(importedRows),{_id:'MaxHeight',value:options.maxHeight??10});if(options.processed)e.table('consensus').push(...importedRows.map(row=>({_id:row.value.txId,processed:true,params:{valid:true,block:Number(row._id.split('-')[1])}})));
 const ClientWrapper={getInstance:unavailable('ClientWrapper.getInstance')};TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:new Proxy({},{get(_t,k){return unavailable('TxUtils.'+String(k));}}),transparentIndex:[],process:e.context.process});
 const client={getBlockCount:unavailable('client.getBlockCount'),getNetworkInfo:unavailable('client.getNetworkInfo')};
 Main=compileClass('Main','main',{client,db:e.storage,TxIndex,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:new Proxy({},{get(_t,k){return unavailable('Vesting.'+String(k));}}),Persistence:new Proxy({},{get(_t,k){return unavailable('Persistence.'+String(k));}}),console:e.context.console,COIN:100000000,pause:false,setTimeout,process:e.context.process});
 for(const[name,receiver]of [['Main',Main.prototype],['TxIndex',TxIndex]])for(const method of Object.getOwnPropertyNames(receiver)){const fn=receiver[method];if(typeof fn==='function')inventory.methods[name+'.'+method]={sha256:sha(fn.toString()),async:Object.prototype.toString.call(fn)==='[object AsyncFunction]'};}
 for(const [receiver,method,label]of [[e.load('tally'),'flushQueuedFeeAccruals','Tally.flushQueuedFeeAccruals'],[e.logic,'settleLiquidityRewards','Logic.settleLiquidityRewards'],[e.Orderbook,'processQueuedOnChainOrdersForBlock','Orderbook.processQueuedOnChainOrdersForBlock'],[e.Orderbook,'processQueuedChannelTradesForBlock','Orderbook.processQueuedChannelTradesForBlock'],[e.load('channels'),'processWithdrawals','Channels.processWithdrawals'],[e.load('clearing'),'clearingFunction','Clearing.clearingFunction']])observe(receiver,method,label);
 if(options.observeTypes)observe(e.types,'decodePayload','Types.decodePayload');
 m=new Main();m.txIndex.then(value=>startup.push({returned:value}),error=>startup.push({thrown:{name:error.name,message:error.message}}));await flush();emit(name,{reset:{activationRows:e.copy(e.table('activations')),indexRows:structuredClone(importedRows),heightRow:options.maxHeight??10,...(Object.keys(options).length?{options}: {})},profile:'callable-class'},{returned:{samePrototype:true,sameSingleton:true},startup});}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure)e.setFailure(failure);let transport='Immediate',outcome;try{const value=Reflect.apply(m[method],m,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:'Main.'+method,args,failure},{transport,outcome});}
function importProbe(){const effects=[],module={exports:{}};let result;try{vm.runInNewContext(source.main,{module,exports:module.exports,require(name){effects.push({require:name});if(name==='events'||name==='util')return require(name);throw Object.assign(Error('Unavailable full-import dependency: '+name),{name:'SourceBoundaryUnavailable'});},console:{log(){},warn(){},error(){}},process:{exit:unavailable('process.exit')}},{filename:'unchanged-full-main-import.js',timeout:1000});result={returned:module.exports};}catch(error){result={thrown:{name:error.name,message:error.message}};}records.push({name:'proof/01-full-main-import-declared-boundary',command:tag({profile:'full-import'}),result:tag(result),state:tag({exported:module.exports}),effects:tag(effects),summary:result});}
function phases(){return e.effects.filter(x=>x.kind==='source-function').map(x=>x.target).filter(x=>x!=='Types.decodePayload');}
const postPhases=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Clearing.clearingFunction'];

const BN=numeric.loadBigNumber();
function bindRpc(){
 const raw=fs.readFileSync(path.join(repo,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(sha(raw),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(repo,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const client={async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'Client.getRawTransaction',args,returned:structuredClone(value)});return value;},async getBlock(...args){const value=structuredClone(e.table('sourceRpcBlocks').find(row=>row.hash===args[0])?.block);e.effects.push({kind:'rpc',method:'Client.getBlock',args,returned:structuredClone(value)});return value;}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+raw+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve(client)}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 for(const key of ['getRawTransaction','getBlockHeight','getAddressTypeUniversal','extractPubkeyByType'])e.adapters['./txUtils'][key]=function(...args){e.effects.push({kind:'source-function',target:'TxUtils.'+key,args:e.copy(args)});return Reflect.apply(own.exports[key],own.exports,args);};
 e.context.spotRpcDocumentsBound=true;
}



const upperA='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',upperB='1BitcoinEaterAddressDontSendf59kuE',upperC='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',units=n=>(n*1e8).toString(36);
function marginIdentity(){const objects=[...marginReturnedObjects];return{instances:marginInstances.map(m=>({seriesId:m.seriesId,margins:[...m.margins].map(([key,value])=>{let id=objects.indexOf(value);if(id<0){id=objects.length;objects.push(value);}return[key,id];}),expiryIndex:m.expiryIndex,tickerExpiry:m.tickerExpiry,optionOI:m.optionOI})),objects};}
const kinds=[16,17,19,20,21,22,23,24,25,26,27,28,29,30,31,33,34,35];
inventory.inputLabels={};inventory.ownedModulePins=extraPins;
function upperRow(label,type,body,sender=upperA){const id=sha('main-upper:'+label);inventory.inputLabels[id]={label,type};const row=input(id,type.toString(36),body);row.value.sender.senderAddress=sender;return row;}
function seedUpperInputs(options){
 e.table('sourceRpcBlocks').push({hash:'upper-block',block:{height:9}});
 for(const {collection,rows} of options.collections||[])e.table(collection).splice(0,Infinity,...structuredClone(rows));
 if(options.contractPatch)Object.assign(e.table('contractList')[0].data,structuredClone(options.contractPatch));
 if(options.nestedOracle)e.table('oracleList')[0].name={ticker:'UPPER'};
 if(options.environment)e.context.process.env={...options.environment};
 for(const [p,hex] of Object.entries(options.artifacts||{}))e.rawArtifacts.set(p,Buffer.from(hex,'hex'));
 if(options.bindTally)e.context.tallyMap=e.load('tally');
}
function docs(rows){return rows.map(row=>({txid:row.value.txId,transaction:{blockhash:'upper-block',vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}}));}
const bodies={
 16:'1,0,1:7,1,2,7,10,o,3,0,0',17:'1,2',
 19:'1,'+units(100)+',1,1,k,0,1',20:'7,1,'+units(2)+','+units(4)+',1,k,1,0,0',
 21:'false,7,300000000,0,'+upperC,22:'7,'+units(2)+',1,'+upperC+',0,',
 23:'trade,settle,0,0,0,0,0,0',24:'7,1,'+units(2),25:'7,1,'+units(1),26:'1,7,2',
 27:'1-100-C-100,2,1,1,0,0',28:'7,1,2,1,k,u',29:'7,a,1,2,1,k,2,u',
 30:'0,1,7,'+units(1),31:'0,0,0,0,0,,0,0',33:'0,7,1,address',34:'7,a,address',35:'hash,method,params'
};
function latest(){return records.at(-1).summary;}
function encodedRow(label,type,method,params,sender=upperA){return upperRow(label,type,e.load('txEncoder')[method](params).slice(3),sender);}
function channelInputs(){
 const bal=(available=0,channelBalance=0)=>({amount:available+channelBalance,available,reserved:0,margin:0,vesting:0,channelBalance});
 const channel={channel:upperC,participants:{A:upperA,B:upperB},A:{7:100,1:100},B:{7:100,1:100},commits:[],clearLists:{A:[],B:[]},payEnabled:{A:false,B:false}};
 const tally=[[upperA,{7:bal(1000),1:bal()}],[upperB,{7:bal(1000),1:bal()}],[upperC,{7:bal(0,200),1:bal(0,200)}]];
 const properties=[1,3,7].map(id=>[id,{ticker:'P'+id,totalInCirculation:id===7?2200:id===1?200:0,type:1,whitelistId:0,issuer:upperA}]);
 const tracking={_id:upperC,trades:[{txid:'prior',block:2,status:'pending',params:{price:100,amount:2}}],keepAlives:[],settlements:[],closes:[],netSettles:[],kingSettlements:[]};
 return [{collection:'channels',rows:[{_id:upperC,data:channel}]},{collection:'tallyMap',rows:[{_id:'tallyMap',data:JSON.stringify(tally)}]},{collection:'propertyList',rows:[{_id:'propertyIndex',value:JSON.stringify(properties)}]},{collection:'scaling',rows:[tracking]}];
}
function nativeInputs(){
 const collections=channelInputs().filter(v=>['propertyList','tallyMap'].includes(v.collection));
 const tally=JSON.parse(collections.find(v=>v.collection==='tallyMap').rows[0].data).filter(([a])=>a!==upperC);
 tally[0][1][7].available=999;tally[0][1][7].margin=1;
 collections.find(v=>v.collection==='tallyMap').rows[0].data=JSON.stringify(tally);
 collections.push({collection:'marginMaps',rows:[{_id:JSON.stringify({seriesId:1}),value:JSON.stringify([[upperA,{contracts:-10,margin:1,unrealizedPNL:0,address:upperA,avgPrice:100}]])}]},{collection:'volumeIndex',rows:[{_id:'7-7',value:{price:100,blockHeight:10}}]});
 return collections;
}
function signedRelayBody(label,settlement){
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(repo,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),secp=external('tiny-secp256k1');
 const key=Buffer.alloc(32);key[31]=1;
 const canonical={eventId:'main-upper-'+label,outcome:'test',outcomeIndex:0,stateHash:'state',timestamp:1700000000};
 const messageHash=crypto.createHash('sha256').update(JSON.stringify(canonical)).digest();
 const document={...canonical,oraclePubkeyHex:Buffer.from(secp.pointFromScalar(key,true)).toString('hex'),signatureHex:Buffer.from(secp.sign(messageHash,key)).toString('hex'),settlement};
 return e.load('txEncoder').encodeStakeFraudProof({action:2,oracleId:1,relayType:0,stateHash:'state',relayBlob:'b64:'+Buffer.from(JSON.stringify(document)).toString('base64')}).slice(3);
}
function processed(row){return e.table('consensus').find(v=>v._id===row.value.txId);}
function checkProcessed(row){assert.equal(processed(row)?.processed,true,'original Main must record reached transaction '+inventory.inputLabels[row.value.txId].label);}
async function resetRows(name,rows,options={}){await reset(name,{balancedSupply:true,observeTypes:true,activeTypes:kinds,rows,bindRpc:true,rpcTransactions:docs(rows),...options});}


async function main(){
 inventory.boundary.scope='original Main/raw21 un-awaited queue save, explicit one-shot DB faults, exact retries; no Rust parity claim';
 const scenes=['processTxSet','blockHandlerMid'].flatMap(method=>[['baseline-'+method,method],...['getDatabase','update'].map(op=>[op+'-'+method,method,{collection:'withdrawQueue',op,message:'withdrawal-'+op}])]);
 for(const[label,method,failure]of scenes){
  const row=upperRow('withdrawal-schedule-'+label,21,'0,7,'+units(3)+',1,'+upperC);
  await resetRows('withdrawal/'+label+'/input',[row],{balancedSupply:false,collections:channelInputs()});
  await call('withdrawal/'+label+'/original-main',method,[[method==='processTxSet'?row:live(row)],10],failure);checkProcessed(row);
  assert.equal(records.at(-1).domain,'source-call');assert.equal(processed(row).params.valid,true,'un-awaited save failure does not invalidate source Logic');
  if(failure){assert.ok(e.effects.some(event=>event.thrown?.message===failure.message),'actual source fault was consumed');assert.ok(e.effects.some(event=>event.kind==='unhandled-rejection'&&event.thrown?.message===failure.message),'original rejection remains observable');}
  await call('withdrawal/'+label+'/processed-retry','processTxSet',[[row],10]);assert.equal(latest().outcome.returned,1);
 }
 assert.equal(records.length,18);assert.equal(records.filter(r=>r.domain==='source-call').length,12);
 const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';inventory.frames=18;inventory.calls=12;inventory.resets=6;inventory.unavailable=0;inventory.fixtureSha256=sha(payload);inventory.rustParityClaim=false;
 for(const[name,value]of [['main_withdrawal_schedule_vectors.jsonl',payload],['main_withdrawal_schedule_inventory.json',JSON.stringify(inventory,null,2)+'\n']]){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),value);else fs.writeFileSync(file,value);}
 if(flags.includes('--print'))process.stdout.write(payload);else console.log(JSON.stringify({frames:18,calls:12,resets:6,sha256:sha(payload),version:flags.includes('--lock')?'9.1.2':'9.3.1'}));
}
main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
