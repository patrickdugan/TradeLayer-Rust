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
const reference=path.join(repo,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(reference,'utf8').replace(/\r\n/g,'\n');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'reference anchor '+before);code=code.replace(before,after);}
replace('const pins = {','const pins = '+JSON.stringify({activation:pins.activation,vaults:pins.vaults}).slice(0,-1)+',');
const activationStart=code.indexOf("'./db':storage,'./activation':"),activationEnd=code.indexOf("    './txUtils':",activationStart);assert.ok(activationStart>=0&&activationEnd>activationStart);code=code.slice(0,activationStart)+"'./db':storage, './client':forbidden('ClientWrapper'),\n"+code.slice(activationEnd);
const txutilsStart=code.indexOf("    './txUtils':"),txutilsEnd=code.indexOf("'node-fetch':",txutilsStart);assert.ok(txutilsEnd>txutilsStart);code=code.slice(0,txutilsStart)+"    './txUtils':new Proxy({},{get(targetObject,key){if(Object.hasOwn(targetObject,key))return targetObject[key];return function(...args){const target='TxUtils.'+String(key),error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});effects.push({kind:'unavailable',target,args:copy(args),thrown:{name:error.name,message:error.message}});throw error;};}}),"+code.slice(txutilsEnd);
replace("'scaling','vaults','procedural'","'scaling','procedural'");
replace("if(op==='$gte')return actual>=wanted;","if(op==='$regex'){assert.equal(Object.prototype.toString.call(wanted),'[object RegExp]');return wanted.test(actual);}if(op==='$gte')return actual>=wanted;");
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:copy(values)});}");
replace('return {types,logic,Orderbook,effects','return {instanceId,adapters,context,storage,tables,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';

replace('const sources = Object.fromEntries(',`Object.assign(pins,{
 vesting:['native_genesis/source/vesting.js','4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b'],
 main:['native_genesis/source/main.js','006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009'],
 persistence:['legacy_persistence_source.txt','628c5b3cfd919fd518aedc2b757cdc256f7ccc36c447090f30aebd8a677ce2e1'],
 reOrg:['legacy_reorg_source.txt','3e4cabb0dc560fd8c601b72e05f0e698ce02adc646eaef241189fd192d906a82'],
 txIndex:['legacy_txindex_source_txIndex.txt','095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51'],
 procedural:['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e']});\nconst sources = Object.fromEntries(`);
replace("['vesting','txIndex'","['txIndex'");replace("'scaling','procedural'","'scaling'");
replace("'./client':forbidden('ClientWrapper')",`'./client':{async getInstance(...args){effects.push({kind:'rpc-input',target:'Client.getInstance',args});if(!scene.clientProfile){const error=Object.assign(Error('Unavailable explicit Client metadata profile'),{name:'SourceBoundaryUnavailable'});effects.push({kind:'unavailable',target:'Client.getInstance',args,thrown:{name:error.name,message:error.message}});throw error;}return{async getChain(...args){effects.push({kind:'rpc-input',target:'Client.getChain',args,returned:scene.clientProfile.chain});return scene.clientProfile.chain;},async getTest(...args){effects.push({kind:'rpc-input',target:'Client.getTest',args,returned:scene.clientProfile.test});return scene.clientProfile.test;}};}}`);
replace("'fs':forbidden('fs')",`'fs':{existsSync(file){const name=path.basename(file,'.js'),returned=Object.hasOwn(sources,name);effects.push({kind:'source-file',method:'existsSync',name,returned});return returned;},promises:{async readFile(file,encoding){const name=path.basename(file,'.js');if(!Object.hasOwn(sources,name))throw Error('Unavailable archived source file '+name);effects.push({kind:'source-file',method:'readFile',name,sha256:crypto.createHash('sha256').update(sources[name]).digest('hex')});return sources[name];}}}`);
replace("'./txUtils':new Proxy({},{get(targetObject,key)",`'./txUtils':new Proxy({async getBlockCount(...args){const returned=scene.clientProfile?.height??10;effects.push({kind:'rpc-clock',method:'TxUtils.getBlockCount',args,returned});return returned;}},{get(targetObject,key)`);
// Observation only: $TLinit is a heterogeneous Tally row without JSON data.
// Audit-only state views cannot create the next call's persisted inputs.
replace("const parseRows=name=>table(name).map(row=>({id:row._id,value:JSON.parse(row.value??row.data)}));", "const parseRows=name=>(tables.get(name)||[]).map(row=>{const value=row.value??row.data;return{id:row._id,value:typeof value==='string'?JSON.parse(value):value};});");
replace("views:{books:table('orderBooks').filter", "views:{books:(tables.get('orderBooks')||[]).filter");
replace('    return module.exports;\n  }\n  // Imported persisted state',`    if(name==='orderbook'){
      const SourceConstructor=module.exports;
      const observed=new Proxy(SourceConstructor,{construct(target,args,newTarget){
        const receiver=Reflect.construct(target,args,newTarget===observed?target:newTarget);
        assert.equal(Object.getPrototypeOf(receiver),target.prototype,'retain actual source prototype');
        assert.equal(receiver.constructor,target,'retain actual source constructor');
        instanceId(receiver);return receiver;
      }});
      module.exports=observed;
    }
    return module.exports;
  }
  // Imported persisted state`);
replace('  let serial=0,','  const ammInstances=[];function ammId(value){let id=ammInstances.indexOf(value);if(id<0){id=ammInstances.length;ammInstances.push(value);}return id+1;}\n  let serial=0,');
replace('    return module.exports;\n  }\n  // Imported persisted state',`    if(name==='amm'){
 const SourceConstructor=module.exports;const observed=new Proxy(SourceConstructor,{construct(target,args,newTarget){const receiver=Reflect.construct(target,args,newTarget===observed?target:newTarget);assert.equal(Object.getPrototypeOf(receiver),target.prototype);assert.equal(receiver.constructor,target);ammId(receiver);return receiver;}});module.exports=observed;
 }
    return module.exports;
  }
  // Imported persisted state`);
replace('return {instanceId,adapters','return {ammInstances,ammId,instanceId,adapters');
const mod=new Module(reference,module);mod.filename=reference;mod.paths=Module._nodeModulePaths(path.dirname(reference));mod._compile(code,reference);
const inventory={sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',pins,referenceDriverSha256:sha(fs.readFileSync(reference,'utf8').replace(/\r\n/g,'\n')),classes:{},methods:{},boundary:{fullMainImport:'declared unavailable ./reOrg.js; no imported module success supplied',callable:'byte-exact full Main and TxIndex classes; real constructors/prototypes; module startup IIFEs not executed',activation:'unchanged complete module, real persisted registry; init/RPC not supplied',storage:'shared explicit source fixture DB adapter; anchored /^tx/ RegExp is genuine JS test; production NeDB equivalence not claimed',rpc:'all reached ClientWrapper/TxUtils calls unavailable; no height/client success answer',observation:'original return plus finite drained turns; no generic event-loop equivalence'}};
function tag(value,seen=new Set()){
 if(value===undefined)return['undefined'];if(value===null)return['null'];if(typeof value==='string'||typeof value==='boolean')return[typeof value,value];
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return['number',Number.isNaN(value)?'NaN':value===Infinity?'Infinity':value===-Infinity?'-Infinity':b.toString('hex')];}
 if(typeof value==='function')return['function',value.name];if(Object.prototype.toString.call(value)==='[object RegExp]')return['regexp',value.source,value.flags];
 if(Object.prototype.toString.call(value)==='[object Promise]')return['promise'];if(value?.constructor?.name==='BigNumber')return['bignumber',value.valueOf()];
 if(seen.has(value))return['cycle'];seen.add(value);let result;
 if(Array.isArray(value))result=['array',value.map(v=>tag(v,seen))];else if(Object.prototype.toString.call(value)==='[object Map]')result=['map',[...value].map(([k,v])=>[tag(k,seen),tag(v,seen)])];else if(Object.prototype.toString.call(value)==='[object Set]')result=['set',[...value].map(v=>tag(v,seen))];else result=['object',Object.entries(value).map(([k,v])=>[k,tag(v,seen)])];seen.delete(value);return result;
}
let productionIOState,factoryFailure;const records=[];let e,m,Main,TxIndex,marginInstances,contextKeys,startup,unhandled=[],marginReturnedObjects,settlementObserverDepth;
process.on('unhandledRejection',error=>{if(e){const value={name:error?.name||'Error',message:error?.message||String(error)};unhandled.push(value);e.effects.push({kind:'unhandled-rejection',thrown:value});}});
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function classBytes(name,sourceName){const full=source[sourceName],start=full.indexOf('class '+name+' {'),end=full.lastIndexOf('module.exports = '+name);assert.ok(start>=0&&end>start);const bytes=full.slice(start,end).trimEnd();inventory.classes[name]={sha256:sha(bytes),bytes:Buffer.byteLength(bytes),fullClass:true};return bytes;}
function compileClass(name,sourceName,scope){const fn=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes(name,sourceName)+'\nreturn '+name+';})',e.context,{filename:'unchanged-class/'+sourceName+'.js',timeout:3000});return fn(...Object.values(scope));}
function unavailable(target){return function(...args){const error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});e.effects.push({kind:'unavailable',target,args:e.copy(args),thrown:{name:error.name,message:error.message}});throw error;};}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);inventory.methods[label]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
function state(){const namesBefore=[...e.tables.keys()],sourceState=e.state();assert.deepEqual([...e.tables.keys()],namesBefore,'audit snapshots must not materialize collections');const A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');assert.ok(e.context.tallyMap===undefined||e.context.tallyMap===T,'actual optional Tally binding must reference the existing original class');return{source:sourceState,...(marginReturnedObjects?{marginIdentity:marginIdentity()}:{}),ammInstances:e.ammInstances.map((value,id)=>({id:id+1,...value})),propertyRegistryBound:!!e.context.PropertyRegistry,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,classParseBlock:TxIndex.parseBlock,instance:TxIndex.instance,clientIsMainClient:TxIndex.client===m.client},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,vesting:e.load('vesting').instance?{...e.load('vesting').instance}:undefined,vestingStatic:{adminAddress:e.load('vesting').adminAddress,sponsorAddress:e.load('vesting').sponsorAddress,expiryInterval:e.load('vesting').expiryInterval,hedgeLeverage:e.load('vesting').hedgeLeverage},consensusVector:e.load('consensus').consensusVector,insuranceInstances:e.load('insurance').instances,unhandledRejections:unhandled,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache,spotRpcDocumentsBound:e.context.spotRpcDocumentsBound,tallyMapBound:e.context.tallyMap===undefined?undefined:e.context.tallyMap===T},startup,productionIO:productionIOState};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),rawFactGraph:richGraph(productionIOState?.facts),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment({clientProfile:options.clientProfile});resetRichGraph();factoryFailure=null;const originalFactory=e.storage.getDatabase;e.storage.getDatabase=function(...args){const name=args[0];if(factoryFailure?.collection===name){if(factoryFailure.skip>0)factoryFailure.skip--;else{const error=Error(factoryFailure.message);factoryFailure=null;e.effects.push({kind:'database',name,method:'getDatabase',args:e.copy(args),thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return Reflect.apply(originalFactory,this,args);};marginInstances=[];contextKeys=new Set();startup=[];unhandled=[];marginReturnedObjects=options.trackMarginIdentity?[]:undefined;settlementObserverDepth=0;if(options.empty)for(const table of e.tables.values())table.splice(0,Infinity);if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
 // Observe every real receiver that reaches the original loader, including
 // the fresh constructor used by _pruneInstaLiqOrdersFromFreshBook. The
 // original method, arguments, promise, return and effects remain unchanged.
 if(options.bindTally)e.context.tallyMap=e.load('tally');if(options.propertyRegistry)e.context.PropertyRegistry=e.load('property');if(options.rpcBlocks)e.table('sourceRpcBlocks').push(...structuredClone(options.rpcBlocks));
 const originalBookLoad=e.Orderbook.prototype.loadOrderBook;e.Orderbook.prototype.loadOrderBook=function(...args){e.instanceId(this);return Reflect.apply(originalBookLoad,this,args);};
 if(options.balancedSupply){const properties=JSON.parse(e.table('propertyList')[0].value);for(const[id,value]of properties)value.totalInCirculation=id===7?2000:0;e.table('propertyList')[0].value=JSON.stringify(properties);}
 const MM=e.load('marginMap'),originalLoad=MM.loadMarginMap;MM.loadMarginMap=function(...args){const result=Reflect.apply(originalLoad,this,args);result.then(v=>{if(v?.margins&&!marginInstances.includes(v))marginInstances.push(v);},()=>{});return result;};
 if(options.trackMarginIdentity){
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
 }
 const clearing=e.load('clearing'),originalInit=clearing.initPositionCache,originalFlush=clearing.flushPositionCache;clearing.initPositionCache=function(...args){const key=Reflect.apply(originalInit,this,args);contextKeys.add(key);return key;};clearing.flushPositionCache=function(...args){const value=Reflect.apply(originalFlush,this,args);contextKeys.delete(args[0]);return value;};
 const activation=e.load('activation').getInstance();let metadataInit;if(options.init){try{metadataInit={returned:await activation.init(options.init)};}catch(error){metadataInit={thrown:{name:error.name,message:error.message}};}}
 const importedRows=options.rows??rows;e.table('txIndex').push(...structuredClone(importedRows),{_id:'MaxHeight',value:options.maxHeight??10});if(options.processed)e.table('consensus').push(...importedRows.map(row=>({_id:row.value.txId,processed:true,params:{valid:true,block:Number(row._id.split('-')[1])}})));
 const ioFacts=structuredClone(options.orchestration||{});const ioCounts={};productionIOState={facts:ioFacts,counts:ioCounts};
 function io(method,args){const ordinal=(ioCounts[method]||0)+1;ioCounts[method]=ordinal;const fail=(ioFacts.failures||[]).find(f=>f.method===method&&f.ordinal===ordinal);if(fail){let error;if(Object.hasOwn(fail,'rawThrow'))error=fail.rawThrow;else if(fail.rawKind==='intrinsic-function-prototype')error=fail.realm==='HostIO'?Function.prototype:vm.runInContext('Function.prototype',e.context);else if(fail.rawKind==='symbol')error=Symbol(fail.description);else if(fail.rawKind==='cycle'){error={kind:'cycle'};error.self=error;}else if(fail.rawKind==='bigint')error=BigInt(fail.decimal);else if(fail.rawKind==='null-prototype')error=Object.assign(Object.create(null),fail.properties||{});else if(fail.rawKind==='self-error'){error=Error('self');error.name=error;}else{error=Object.assign(Error(fail.message),{code:fail.code,name:fail.name||'Error'});if(Object.hasOwn(fail,'errorMessage'))error.message=fail.errorMessage;if(Object.hasOwn(fail,'errorName'))error.name=fail.errorName;}lastIOThrown=error;e.effects.push({kind:'rpc-input',method,args:e.copy(args),ordinal,richThrown:richGraph(error)});throw error;}let returned;if(method==='Client.getBlockCount')returned=ioFacts.height;else if(method==='Client.getNetworkInfo')returned=ioFacts.networkInfo;else if(method==='Client.getBlockHash')returned=(ioFacts.blocks||[]).find(b=>b.height===args[0])?.hash;else if(method==='Client.getBlock')returned=(ioFacts.blocks||[]).find(b=>b.hash===args[0])?.block;else if(method==='Client.getChain')returned=options.clientProfile?.chain;else if(method==='Client.getTest')returned=options.clientProfile?.test;else throw Error('unmapped recorded IO '+method);e.effects.push({kind:'rpc-input',method,args:e.copy(args),ordinal,returned:e.copy(returned),richReturned:richGraph(returned)});return structuredClone(returned);}
 class RecordedClient{constructor(){this.chain=options.clientProfile?.chain;this.test=options.clientProfile?.test;}async getBlockCount(...args){return io('Client.getBlockCount',args);}async getNetworkInfo(...args){return io('Client.getNetworkInfo',args);}async getBlockHash(...args){return io('Client.getBlockHash',args);}async getBlock(...args){return io('Client.getBlock',args);}async getChain(...args){return io('Client.getChain',args);}async getTest(...args){return io('Client.getTest',args);}}
 const client=new RecordedClient();const ClientWrapper={async getInstance(...args){e.effects.push({kind:'rpc-input',method:'Client.getInstance',args:e.copy(args),returned:{chain:client.chain,test:client.test}});return client;}};TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:new Proxy({},{get(_t,k){return unavailable('TxUtils.'+String(k));}}),transparentIndex:[],process:e.context.process});
 Main=compileClass('Main','main',{client,db:e.storage,TxIndex,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:e.load('vesting'),Persistence:new Proxy({},{get(_t,k){return unavailable('Persistence.'+String(k));}}),console:e.context.console,COIN:100000000,pause:false,setTimeout,process:e.context.process});
 for(const[name,receiver]of [['Main',Main.prototype],['TxIndex',TxIndex]])for(const method of Object.getOwnPropertyNames(receiver)){const fn=receiver[method];if(typeof fn==='function')inventory.methods[name+'.'+method]={sha256:sha(fn.toString()),async:Object.prototype.toString.call(fn)==='[object AsyncFunction]'};}
 for(const [receiver,method,label]of [[e.load('tally'),'flushQueuedFeeAccruals','Tally.flushQueuedFeeAccruals'],[e.logic,'settleLiquidityRewards','Logic.settleLiquidityRewards'],[e.Orderbook,'processQueuedOnChainOrdersForBlock','Orderbook.processQueuedOnChainOrdersForBlock'],[e.Orderbook,'processQueuedChannelTradesForBlock','Orderbook.processQueuedChannelTradesForBlock'],[e.load('channels'),'processWithdrawals','Channels.processWithdrawals'],[e.load('clearing'),'clearingFunction','Clearing.clearingFunction']])observe(receiver,method,label);
 if(options.observeTypes)observe(e.types,'decodePayload','Types.decodePayload');
 const AMM=e.load('amm');for(const method of ['addCapital','redeemCapital','quoteContractOrders']){const original=AMM.prototype[method];inventory.methods['AMM.'+method]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};AMM.prototype[method]=function(...args){e.effects.push({kind:'source-function',target:'AMM.'+method,instance:e.ammId(this),args:e.copy(args)});return Reflect.apply(original,this,args);};}
 m=new Main();if(options.mainGenesis!==undefined)m.genesisBlock=options.mainGenesis;if(options.volumeRows)e.table('volumeIndex').push(...structuredClone(options.volumeRows));m.txIndex.then(value=>startup.push({returned:value}),error=>startup.push({thrown:{name:error.name,message:error.message}}));await flush();emit(name,{reset:{activationRows:e.copy(e.table('activations')),indexRows:structuredClone(importedRows),heightRow:options.maxHeight??10,...(Object.keys(options).length?{options}: {})},profile:'callable-class'},{returned:{samePrototype:true,sameSingleton:true},startup,metadataInit});}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure){if(failure.op==='getDatabase')factoryFailure={...failure};else e.setFailure(failure);}let transport='Immediate',outcome;try{const value=Reflect.apply(m[method],m,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();if(failure)assert.ok(e.effects.some(event=>event.thrown?.message===failure.message),'declared IO failure must actually be consumed '+name);emit(name,{target:'Main.'+method,args,failure},{transport,outcome});}
function importProbe(){const effects=[],module={exports:{}};let result;try{vm.runInNewContext(source.main,{module,exports:module.exports,require(name){effects.push({require:name});if(name==='events'||name==='util')return require(name);throw Object.assign(Error('Unavailable full-import dependency: '+name),{name:'SourceBoundaryUnavailable'});},console:{log(){},warn(){},error(){}},process:{exit:unavailable('process.exit')}},{filename:'unchanged-full-main-import.js',timeout:1000});result={returned:module.exports};}catch(error){result={thrown:{name:error.name,message:error.message}};}records.push({name:'proof/01-full-main-import-declared-boundary',command:tag({profile:'full-import'}),result:tag(result),state:tag({exported:module.exports}),effects:tag(effects),summary:result});}
function phases(){return e.effects.filter(x=>x.kind==='source-function').map(x=>x.target).filter(x=>x!=='Types.decodePayload');}
const postPhases=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Clearing.clearingFunction'];

function marginIdentity(){const objects=[...marginReturnedObjects];return{instances:marginInstances.map(m=>({seriesId:m.seriesId,margins:[...m.margins].map(([key,value])=>{let id=objects.indexOf(value);if(id<0){id=objects.length;objects.push(value);}return[key,id];}),expiryIndex:m.expiryIndex,tickerExpiry:m.tickerExpiry,optionOI:m.optionOI})),objects};}
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




const mixedAdmin='MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB',mixedOther='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw';
const units=value=>(value*1e8).toString(36),enabled=[0,1,2,4,5,6,7,8,9,10,11,12,13,14,15,16,18,21,26,32];
inventory.inputLabels={};inventory.findings=[];
function row(label,type,body,sender=mixedAdmin){const id=sha('main-mixed:'+label);inventory.inputLabels[id]={label,type};const item=input(id,type.toString(36),body);item.value.sender.senderAddress=sender;return item;}
const bodyIssue=(ticker,managed)=>[1000..toString(36),ticker,'',+managed,mixedOther,0,0,''].join(',');
function sceneRows(prefix='first10'){
 const make=(label,...args)=>row(prefix+'/'+label,...args);
 return{
  activate:make('activate',0,enabled.join(';')+',mixed-code'),
  fixed:make('fixed',1,bodyIssue('MIXFIX',false)),managed:make('managed',1,bodyIssue('MIXMNG',true)),
  list:make('clearlist',7,mixedOther+',MIXLIST,url,description'),oracle:make('oracle',13,'MIXOR,url,'+mixedOther+',,1'),
  sendFixed:make('send-fixed',2,'0;'+mixedOther+';5;4'),sendNative:make('send-native',2,'0;'+mixedOther+';1;4'),
  grant:make('grant',11,'6,'+units(2)+','+mixedOther),redeem:make('redeem',12,'6,'+units(1)),
  buy:make('spot-buy',5,'5,1,'+units(1)+','+units(1)+',0,0'),sell:make('spot-sell',5,'1,5,'+units(1)+','+units(1)+',0,0',mixedOther),
  cancel:make('cancel',6,'0,5,1,0,,buy'),
  publish:make('publish',14,['1',(10*1e4).toString(36),'c','8','a'].join(',')),
  contract:make('contract',16,'1,0,1:1,1,2,1,10,o,3,0,0'),
  ammAdd:make('amm-add',10,'0,1,1,1,5,1,,,0,'),ammRedeem:make('amm-redeem',10,'1,1,1,1,5,1,,,0,'),
  attest:make('attest',9,'0,4,'+mixedOther+',ca'),close:make('close',15,'1'),
  pay:make('pay',26,'5,1,2'),withdraw:make('withdraw',21,'false,1,300000000,0,'+mixedAdmin),
  publishNew:make('publish-generic',32,'[]'),
 };
}
function rpcDocuments(rows){return rows.map(item=>({txid:item.value.txId,transaction:{blockhash:'mixed-block',vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}}));}
const mixedCall=call;
call=async function(name,...args){const start=e.logs.length;await mixedCall(name,...args);const last=records.at(-1);inventory.findings.push({name,domain:last.domain,phases:phases().filter(value=>postPhases.includes(value)||value==='Channels.processWithdrawals'),diagnostics:e.logs.slice(start).filter(value=>value.level!=='log'),processed:(e.tables.get('consensus')||[]).filter(value=>value.processed).map(value=>({label:inventory.inputLabels[value._id]?.label,id:value._id,valid:value.params.valid,reason:value.params.reason}))});};
async function resetMixed(name,rs,options={}){await reset(name,{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:Object.values(rs),bindRpc:true,rpcTransactions:rpcDocuments(Object.values(rs)),rpcBlocks:[{hash:'mixed-block',block:{height:9}}],bindTally:true,propertyRegistry:true,...options});}
async function firstTen(){
 const r=sceneRows();await resetMixed('mixed/first10/shared-owner-native-genesis-input',r);
 await call('mixed/first10/01-actual-activation-bootstrap','processTxSet',[[r.activate],10]);
 await call('mixed/first10/02-properties-list-oracle-same-owners','processTxSet',[[r.fixed,r.managed,r.list,r.oracle],10]);
 await call('mixed/first10/03-funded-sends-and-managed-grant','processTxSet',[[r.sendFixed,r.sendNative,r.grant],10]);
 await call('mixed/first10/04-spot-pair-enqueue-from-real-balances','processTxSet',[[r.buy,r.sell],10]);
 await call('mixed/first10/05-cancel-real-shared-postphases','blockHandlerMid',[[live(r.cancel)],10]);
 await call('mixed/first10/06-oracle-contract-and-amm-roundtrip','processTxSet',[[r.publish,r.contract,r.ammAdd,r.ammRedeem],10]);
 await call('mixed/first10/07-managed-attestation-and-oracle-error','blockHandlerMid',[[live(r.redeem),live(r.attest),live(r.close)],11]);
 await call('mixed/first10/08-processed-mixed-retry-skips','processTxSet',[[r.activate,r.fixed,r.buy,r.ammAdd,r.ammRedeem],11]);
 await call('mixed/first10/09-upper-distribution-withdraw-and-publish','processTxSet',[[r.pay,r.withdraw,r.publishNew],11]);
 await call('mixed/first10/10-empty-live-reached-withdrawals','blockHandlerMid',[[],11]);
 assert.equal(records.filter(value=>value.command[1].some(([key])=>key==='target')).length,10,'first ten actual Main calls precede expansion');
}
function extendedRows(prefix){
 const r=sceneRows(prefix),make=(label,...args)=>row(prefix+'/'+label,...args);
 r.country=make('country-self-cert',9,'0,0,'+mixedOther+',CA',mixedOther);
 r.nonNative=make('oracle-contract',16,'0,1,,1,2,1,10,o,3,0,0');
 r.extra=make('activate-extra',0,'3;4;8;20;22;23;30,mixed-extra');
 r.sendNew=make('send-new',2,'0;'+mixedOther+';5;1');
 r.ammAddNew=make('amm-add-new',10,'0,1,1,1,5,1,,,0,');r.ammRedeemNew=make('amm-redeem-new',10,'1,1,1,1,5,1,,,0,');
 r.cancelNew=make('cancel-new',6,'0,5,1,0,,buy');
 r.buyNew=make('buy-new',5,'5,1,'+units(1)+','+units(1)+',0,0');
 r.utxo=make('utxo-no-reference',3,'5,'+units(1)+',1,rs,1,0,0,');
 r.commit=make('commit',4,'5,'+units(1)+',LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789,1,0,,');
 r.propertyAmm=make('property-missing-initializer',10,'0,0,1,1,5,1,,,0,');
 r.update=make('oracle-admin',8,mixedOther+',0,1,0,1,0');
 r.stake=make('stake',30,'0,1,5,'+units(1));
 r.tokenChannel=make('token-channel',20,'5,1,'+units(1)+','+units(1)+',1,k,1,0,0');
 r.contractBuy=make('contract-buy',18,'2,'+units(10)+',1,0,0,0,0,0');
 r.contractSell=make('contract-sell',18,'2,'+units(10)+',1,1,0,0,0,0',mixedOther);
 return r;
}
function bootstrapAndAssets(r){return[r.activate,r.fixed,r.managed,r.list,r.oracle,r.country,r.sendFixed,r.sendNative,r.grant];}
async function expandMixed(){
 for(const order of ['contracts-first','amm-first']){
  const r=extendedRows('derivatives/'+order);await resetMixed('mixed/derivatives/'+order+'/input',r,{trackMarginIdentity:true});
  await call('mixed/derivatives/'+order+'/genuine-bootstrap-assets-and-oracle-contract','processTxSet',[[...bootstrapAndAssets(r),r.nonNative,r.publish],10]);
  const contractRows=[live(r.contractBuy),live(r.contractSell)],capitalRows=[live(r.ammAdd),live(r.ammRedeem)];
  await call('mixed/derivatives/'+order+'/shared-capital-contract-settlement-and-postphases','blockHandlerMid',[order==='contracts-first'?[...contractRows,...capitalRows,live(r.cancel)]:[...capitalRows,...contractRows,live(r.cancel)],10]);
  await call('mixed/derivatives/'+order+'/processed-derivative-and-amm-retry','blockHandlerMid',[[...contractRows,...capitalRows],11]);
  await call('mixed/derivatives/'+order+'/new-lower-call-after-actual-settlement-prefix','blockHandlerMid',[[live(r.sendNew),live(r.cancelNew)],11]);
 }
 for(const order of ['dependencies-first','assets-swapped','activation-late']){
  const r=extendedRows('ordering/'+order);await resetMixed('mixed/ordering/'+order+'/input',r);
  const base=bootstrapAndAssets(r);
  if(order==='assets-swapped')[base[1],base[2]]=[base[2],base[1]];
  if(order==='activation-late'){base.splice(0,1);base.splice(2,0,r.activate);}
  const batch=[...base,r.extra,r.nonNative,r.ammAdd,r.ammRedeem,r.buy,r.sell];
  await call('mixed/ordering/'+order+'/one-indexed-shared-batch','processTxSet',[batch,10]);
  await call('mixed/ordering/'+order+'/varied-live-roundtrip-and-lower-upper','blockHandlerMid',[[live(r.ammAddNew),live(r.ammRedeemNew),live(r.redeem),live(r.attest),live(r.pay),live(r.cancel)],10]);
  await call('mixed/ordering/'+order+'/reversed-processed-batch','processTxSet',[[...batch].reverse(),11]);
  await call('mixed/ordering/'+order+'/fresh-live-after-reached-source-prefix','blockHandlerMid',[[live(r.sendNew),live(r.cancelNew),live(r.propertyAmm)],11]);
  await call('mixed/ordering/'+order+'/actual-input-rpc-commit-and-utxo','processTxSet',[[r.commit,r.utxo,r.stake,r.tokenChannel,r.update],11]);
 }
 for(const [label,failure]of[
  ['delta-second',{collection:'tallyMapDelta',op:'insert',skip:1,message:'mixed-delta-second'}],
  ['contract-save',{collection:'contractList',op:'update',message:'mixed-contract-save'}],
  ['orderbook-save',{collection:'orderBooks',op:'update',skip:1,message:'mixed-orderbook-save'}],
  ['processed-marker',{collection:'consensus',op:'update',skip:1,message:'mixed-processed-marker'}],
  ['index-validity',{collection:'txIndex',op:'update',skip:1,message:'mixed-index-validity'}],
 ]){
  const r=extendedRows('failure/'+label);await resetMixed('mixed/failure/'+label+'/input',r);
  await call('mixed/failure/'+label+'/genuine-bootstrap-and-assets','processTxSet',[bootstrapAndAssets(r),10]);
  const batch=[live(r.ammAdd),live(r.sendNew),live(r.buy),live(r.ammRedeem),live(r.publish),live(r.cancel)];
  await call('mixed/failure/'+label+'/mixed-live-prefix','blockHandlerMid',[batch,10],failure);
  assert.ok(e.effects.some(value=>value.thrown?.message===failure.message),'injected actual storage failure must be consumed '+label);
  await call('mixed/failure/'+label+'/same-transactions-processed-retry','blockHandlerMid',[batch,10]);
  await call('mixed/failure/'+label+'/new-transactions-retain-real-owner-prefix','blockHandlerMid',[[live(r.ammAddNew),live(r.ammRedeemNew),live(r.cancelNew),live(r.pay)],11]);
 }
 for(const [label,failure]of[
  ['insurance-snapshot',{collection:'insurance',op:'update',message:'mixed-bootstrap-insurance'}],
  ['activation-save',{collection:'activations',op:'update',skip:1,message:'mixed-bootstrap-registry'}],
 ]){
  const r=extendedRows('bootstrap-failure/'+label);await resetMixed('mixed/bootstrap-failure/'+label+'/input',r);
  await call('mixed/bootstrap-failure/'+label+'/bootstrap-and-dependent-assets','processTxSet',[bootstrapAndAssets(r),10],failure);
  assert.ok(e.effects.some(value=>value.thrown?.message===failure.message),'bootstrap failure must be reached '+label);
  await call('mixed/bootstrap-failure/'+label+'/live-dependent-roundtrip-phases','blockHandlerMid',[[live(r.ammAdd),live(r.ammRedeem),live(r.sendNew),live(r.cancel)],10]);
  await call('mixed/bootstrap-failure/'+label+'/same-bootstrap-replay-skips','processTxSet',[bootstrapAndAssets(r),11]);
 }
 // Unbound RPC and Client experiments are isolated resets. Their observed
 // source prefix is never imported into a later available comparison.
 const unavailable=extendedRows('unavailable/rpc');await resetMixed('mixed/unavailable/rpc/input',unavailable,{bindRpc:false});
 await call('mixed/unavailable/rpc/bootstrap-and-assets','processTxSet',[bootstrapAndAssets(unavailable),10]);
 await call('mixed/unavailable/rpc/original-spot-rpc-unavailable','processTxSet',[[unavailable.buy],10]);
 assert.equal(records.at(-1).domain,'unavailable-capability');
 const missing=row('unavailable/client',0,'0,mixed-missing',undefined);missing.value.sender.senderAddress=undefined;
 await resetMixed('mixed/unavailable/client/input',{missing},{clientProfile:undefined,init:undefined});
 await call('mixed/unavailable/client/original-native-client-unavailable','processTxSet',[[missing],10]);
 assert.equal(records.at(-1).domain,'unavailable-capability');
 const final=extendedRows('after-unavailable');await resetMixed('mixed/after-unavailable/fresh-source-input',final);
 await call('mixed/after-unavailable/genuine-bootstrap-no-injected-prefix','processTxSet',[bootstrapAndAssets(final),10]);
 await call('mixed/after-unavailable/live-roundtrip-real-six-phases','blockHandlerMid',[[live(final.ammAdd),live(final.ammRedeem),live(final.sendNew),live(final.cancel)],11]);
}

async function firstProductionTen(){
 const opts={empty:true,clientProfile:{chain:'LTC',test:false,height:12},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:12,blocks:[10,11,12].map(height=>({height,hash:'h'+height,block:{hash:'h'+height,previousblockhash:'h'+(height-1),tx:[]}}))},volumeRows:[{_id:'0-1',value:{blockHeight:11,ltcVolume:2,rawVolume:3}}]};
 await reset('production/first10/recorded-client-and-source-genesis',opts);
 await call('production/first10/01-end-empty-block','blockHandlerEnd',['h10',10]);
 await call('production/first10/02-end-positive-object-does-not-pass-source-number-guard','blockHandlerEnd',['h11',11]);
 assert.equal(e.effects.some(x=>x.target==='Vesting.updateVesting'),false);
 await call('production/first10/03-current-height-is-recorded-IO','getCurrentBlockHeight');assert.equal(records.at(-1).summary.outcome.returned,12);
 await call('production/first10/04-lag-falls-back-to-null-max-height','checkBlockLag');assert.equal(JSON.stringify(records.at(-1).summary.outcome.returned),JSON.stringify({lag:12,chainTip:12,maxTrack:null}));
 await call('production/first10/05-save-track','saveTrackHeight',[10]);
 await call('production/first10/06-lag-uses-persisted-track','checkBlockLag');assert.equal(JSON.stringify(records.at(-1).summary.outcome.returned),JSON.stringify({lag:2,chainTip:12,maxTrack:10}));
 await call('production/first10/07-index-ahead-of-tip','syncIndex',[20]);
 await call('production/first10/08-fresh-index-extracts-real-empty-block-inputs','initOrLoadTxIndex');assert.equal(e.tables.get('txIndex').find(x=>x._id==='MaxHeight').value,12);assert.equal(e.tables.get('txIndex').find(x=>x._id==='indexExists').value,true);
 await call('production/first10/09-check-sync-retains-class-instance-parse-quirk','checkSync');assert.equal(JSON.stringify(records.at(-1).summary.outcome.returned),JSON.stringify({consensus:0,txIndex:0}));
 await call('production/first10/10-no-indexed-TL-rows-returns-before-live-loop','constructConsensusFromIndex',[10]);
 assert.equal(records.filter(r=>r.command[1].some(([k])=>k==='target')).length,10);
}

async function expandProduction(){
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:12},init:'LTC',rows:[],maxHeight:12,mainGenesis:10,orchestration:{height:12,blocks:[]}};
 for(const[op,skip]of [['getDatabase',0],['find',0],['find',1],['find',2],['find',3]]){await reset('production/end-failure/'+op+'-'+skip+'/input',base);await call('production/end-failure/'+op+'-'+skip,'blockHandlerEnd',['h12',12],{collection:'volumeIndex',op,skip,message:'volume-'+op+'-'+skip});}
 for(const spec of [
  {name:'new-index',opts:{maxHeight:0,orchestration:{height:9,blocks:[]}},failure:{collection:'txIndex',op:'insert',message:'genesis-insert'}},
  {name:'existing-index-read',failure:{collection:'txIndex',op:'findOne',message:'max-read'}},
  {name:'fetch-tip-swallowed',opts:{orchestration:{height:12,blocks:[],failures:[{method:'Client.getBlockCount',ordinal:1,message:'recorded node timeout',code:'ETIMEDOUT'}]}}},
 ]){await reset('production/index-failure/'+spec.name+'/input',{...base,...spec.opts});await call('production/index-failure/'+spec.name,'initOrLoadTxIndex',[],spec.failure);}
 const indexed=input('production32','w','[]');
 for(const[col,op]of [['consensus','getDatabase'],['tallyMap','getDatabase'],['tallyMap','findOne']]){
  await reset('production/catchup-failure/'+col+'-'+op+'/input',{...base,rows:[indexed],processed:true,orchestration:{height:12,failures:[{method:'Client.getBlockCount',ordinal:1,message:'recorded offline after catchup',code:'ECONNREFUSED'}]}});
  await call('production/catchup-failure/'+col+'-'+op,'constructConsensusFromIndex',[10],{collection:col,op,message:'catchup-'+col+'-'+op});
 }

 for(const spec of [
  {name:'new-invalid32',rows:[indexed]},
  {name:'processed32',rows:[indexed],processed:true},
  {name:'missing-payload-still-creates-height-bucket',rows:[{_id:'tx-10-missing',value:{txId:'missing'}}]},
  {name:'malformed-height-key-is-NaN-and-returns',rows:[{...indexed,_id:'tx-bad-production32'}]},
  {name:'partial-processed-funding-priority',rows:['M','K','m','k','4'].map((p,i)=>input('prio'+i,p,'body')),processed:true},
  {name:'periodic10000-postphase-even-processed',rows:[{...indexed,_id:'tx-10000-production32'}],processed:true,start:10000},
  {name:'positive-volume-source-vesting-calls',rows:[indexed],volumeRows:[{_id:'0-1',value:{blockHeight:10,ltcVolume:2000,rawVolume:2000}}]},
  {name:'start-beyond-last-still-writes-start-minus-one',rows:[indexed],start:20},
 ]){await reset('production/catchup/'+spec.name+'/input',{...base,...spec,orchestration:{height:12,failures:[{method:'Client.getBlockCount',ordinal:1,message:'recorded offline after catchup',code:'ECONNREFUSED'}]}});await call('production/catchup/'+spec.name,'constructConsensusFromIndex',[spec.start??10]);}
 await reset('production/construct-or-load-empty/input',base);await call('production/construct-or-load-empty/full-original-wrapper','constructOrLoadConsensus');
 await reset('production/finite-safe-boundary/input',{...base,rows:[{_id:'tx-9007199254740991-empty',value:{txId:'empty',payload:''}}],maxHeight:Number.MAX_SAFE_INTEGER,orchestration:{height:12,failures:[{method:'Client.getBlockCount',ordinal:1,message:'recorded safe-boundary completion',code:'EIO'}]}});await call('production/finite-safe-boundary/original-next-safe-boundary-exits','constructConsensusFromIndex',[Number.MAX_SAFE_INTEGER]);
 for(const[name,id]of[['FEFF','tx-\uFEFF10-empty'],['0085','tx-\u008510-empty'],['hex-prefix','tx-0x10-empty']]){await reset('production/parseInt/'+name+'/input',{...base,rows:[{_id:id,value:{txId:'empty',payload:''}}],orchestration:{height:12,failures:[{method:'Client.getBlockCount',ordinal:1,message:'recorded parseInt completion',code:'EIO'}]}});await call('production/parseInt/'+name+'/original-default-radix-and-whitespace','constructConsensusFromIndex',[10]);}
 for(const method of['syncIndex','initOrLoadTxIndex']){await reset('production/returned-rejection/'+method+'/input',base);await call('production/returned-rejection/'+method+'/returned-promise-rejection-escapes-catch',method,method==='syncIndex'?[12]:[],{collection:'consensus',op:'getDatabase',message:'recorded returned catchup rejection'});assert.equal(records.at(-1).summary.transport,'PendingThrow');}
 await reset('production/signed-zero-sync/input',base);await call('production/signed-zero-sync/original-numeric-zero-branch','syncIndex',[-0]);
 const native=row('production/native-bootstrap',0,'0,production-code');
 await reset('production/native-vesting/input',{...base,rows:[native,indexed],volumeRows:[{_id:'0-1',value:{blockHeight:10,ltcVolume:2000,rawVolume:2000}}],orchestration:{height:12,failures:[{method:'Client.getBlockCount',ordinal:1,message:'recorded offline after catchup',code:'ECONNREFUSED'}]}});
 await call('production/native-vesting/genuine-Main0-creates-property-and-balances','processTxSet',[[native],10]);
 await call('production/native-vesting/catchup-uses-original-vesting-with-async-DB-quirk','constructConsensusFromIndex',[10]);
}
async function main(){
 inventory.boundary.rpc='recorded Client metadata, block count, network info and raw block documents only; no financial/validation/verifier answers';inventory.boundary.callable='unchanged full Main and TxIndex class bytes and genuine constructors; startup IIFEs remain explicitly outside this callable proof';
 inventory.boundary.production='first ten original Main methods execute startup-index/catchup/end seams; no production RPC authentication or durable publication claim';
 await firstProductionTen();
 const proof=records.map(r=>JSON.stringify(r)).join('\n')+'\n';if(!proofOnly)await expandProduction();const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';inventory.observations=records.length;inventory.calls=records.filter(r=>r.command[1].some(([k])=>k==='target')).length;inventory.fixtureSha256=sha(payload);inventory.proof10Sha256=sha(proof);
 const outputs=proofOnly?[['main_production_proof10.jsonl',proof]]:[['main_production_proof10.jsonl',proof],['main_production_vectors.jsonl',payload],['main_production_inventory.json',JSON.stringify(inventory,null,2)+'\n']];
 for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}
 console.log(JSON.stringify({frames:records.length,actualMainCalls:inventory.calls,fixtureSha256:sha(payload),proof10Sha256:sha(proof),firstTenComplete:true}));
}

async function errorLabelsMain(){
 const samples=[['numeric-name',{name:5,message:'recorded failure'}],['boolean-name',{name:true,message:'recorded failure'}],['object-name',{name:{kind:'rpc'},message:'recorded failure'}],['array-name',{name:['Error'],message:'recorded failure'}]];
 for(const[label,fields]of samples){await reset('error-label/'+label+'/input',{empty:true,clientProfile:{chain:'LTC',test:false,height:12},init:'LTC',rows:[],maxHeight:12,mainGenesis:10,orchestration:{height:12,failures:[{method:'Client.getBlockCount',ordinal:1,...fields}]}});await call('error-label/'+label,'getCurrentBlockHeight');if(proofOnly&&records.length===8)break;}
 const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n',proof=records.slice(0,8).map(r=>JSON.stringify(r)).join('\n')+'\n';const outputs=proofOnly?[['main_rich_throw_proof4.jsonl',proof]]:[['main_rich_throw_proof4.jsonl',proof],['main_rich_throw_vectors.jsonl',payload]];
 for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}console.log(JSON.stringify({observations:records.length,actualMainCalls:records.length/2,fixtureSHA256:sha(payload),proofSHA256:sha(proof)}));
}
let richIds,richObjects,richSymbols,richSymbolIds,richIntrinsics,lastIOThrown;
const utf16=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
function resetRichGraph(){richIds=new WeakMap();richObjects=[];richSymbols=[];richSymbolIds=new Map();richIntrinsics=new Map();lastIOThrown=undefined;
 const names=['Object','Array','Function','Error','TypeError','RangeError','SyntaxError','ReferenceError','URIError','EvalError','AggregateError','Promise'];
 for(const realm of ['HostIO','SourceVM']){const builtins=realm==='HostIO'?globalThis:vm.runInContext('({'+names.join(',')+'})',e.context);for(const name of names){const ctor=builtins[name];if(ctor){richIntrinsics.set(ctor,{realm,name});richIntrinsics.set(ctor.prototype,{realm,name:name+'.prototype'});}}}
 registerNativeBindings();
}

function richAtom(value){
 if(value===undefined)return{tag:'Undefined'};if(value===null)return{tag:'Null'};if(typeof value==='boolean')return{tag:'Boolean',value};if(typeof value==='string')return{tag:'UTF16',units:utf16(value)};
 if(typeof value==='number'){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return{tag:'Binary64',bits:bytes.toString('hex')};}
 if(typeof value==='bigint')return{tag:'BigInt',decimal:value.toString()};
 if(typeof value==='symbol'){if(!richSymbolIds.has(value)){const id=richSymbols.length;richSymbolIds.set(value,id);richSymbols.push({id,description:value.description===undefined?null:utf16(value.description),globalKey:Symbol.keyFor(value)??null});}return{tag:'SymbolRef',id:richSymbolIds.get(value)};}
 const intrinsic=richIntrinsics.get(value);if(intrinsic)return{tag:'Intrinsic',...intrinsic};
 if(!richIds.has(value)){richIds.set(value,richObjects.length);richObjects.push(value);}return{tag:'HeapRef',id:richIds.get(value)};
}
function richGraph(root){const atom=richAtom(root),nodes=[];for(let i=0;i<richObjects.length;i++){
 const value=richObjects[i],descriptors=Object.getOwnPropertyDescriptors(value),properties=[];for(const key of Reflect.ownKeys(descriptors)){const d=descriptors[key],base={key:typeof key==='symbol'?richAtom(key):{tag:'UTF16',units:utf16(key)},enumerable:d.enumerable,configurable:d.configurable};properties.push(Object.hasOwn(d,'value')?{...base,kind:'Data',writable:d.writable,value:richAtom(d.value)}:{...base,kind:'Accessor',get:richAtom(d.get),set:richAtom(d.set)});}
 const kind=typeof value==='function'?'Callable':Array.isArray(value)?'Array':require('node:util').types.isNativeError(value)?'NativeError':'Ordinary';const node={id:i,kind,prototype:richAtom(Object.getPrototypeOf(value)),properties};if(kind==='Callable')node.sourceSha256=sha(Function.prototype.toString.call(value));nodes.push(node);
 }return{schema:'RichValueGraph/v1',root:atom,objects:nodes,symbols:structuredClone(richSymbols)};}
async function richCall(name,method,args=[]){e.effects.length=0;const logs=e.logs.length;let transport='Immediate',outcome,raw;
 try{const receiver=method.startsWith('TxIndex.')?TxIndex:m,key=method.split('.').at(-1);const value=Reflect.apply(receiver[key],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{raw=await value;outcome={returned:richGraph(raw)};}catch(thrown){transport='PendingThrow';raw=thrown;outcome={thrown:richGraph(thrown),sameAsIO:Object.is(thrown,lastIOThrown)};}}else{raw=value;outcome={returned:richGraph(value)};}}
 catch(thrown){transport='Throw';raw=thrown;outcome={thrown:richGraph(thrown),sameAsIO:Object.is(thrown,lastIOThrown)};}
 await flush();records.push({name,command:tag({target:method.includes('.')?method:'Main.'+method,args}),result:{transport,outcome},state:tag(state()),effects:tag(e.effects),logging:{recordedCalls:e.logs.length-logs,adapter:'raw arguments; no display coercion'},finalGraph:richGraph(raw)});
}
async function richScenario(label,method,fields,args=[],extra={}){
 const rpcMethod=method==='checkNetworkStatus'?'Client.getNetworkInfo':'Client.getBlockCount';
 await reset('rich/'+label+'/input',{empty:true,clientProfile:{chain:'LTC',test:false,height:12},init:'LTC',rows:[],maxHeight:12,mainGenesis:10,...extra,orchestration:{height:12,networkInfo:{networkactive:true,connections:2},failures:[{method:rpcMethod,ordinal:1,...fields}]}});
 await richCall('rich/'+label,method,args);assert.ok(e.effects.some(x=>x.richThrown),'actual recorded IO throw reached '+label);
}
async function firstRichTen(){
 const samples=[['number-name',{name:5,message:'recorded'}],['boolean-name',{name:true,message:'recorded'}],['object-name',{name:{label:'rpc'},message:'recorded'}],['array-name',{name:['Error'],message:'recorded'}],['number-message',{errorMessage:5}],['object-message',{errorMessage:{kind:'message'}}],['raw-null',{rawThrow:null}],['raw-undefined',{rawThrow:undefined}],['raw-zero',{rawThrow:0}],['raw-object',{rawThrow:{code:'EIO',message:['raw'],name:{kind:'raw'}}}]];
 for(const[label,fields]of samples)await richScenario(label,'getCurrentBlockHeight',fields);
 assert.equal(records.length,20);
}
async function expandRich(){
 for(const[label,fields]of [['raw-string',{rawThrow:'boom'}],['raw-array',{rawThrow:['EIO',{message:1}]}],['raw-false',{rawThrow:false}],['raw-negative-zero',{rawThrow:-0}],['raw-nan',{rawThrow:NaN}],['raw-cycle',{rawKind:'cycle'}],['raw-symbol',{rawKind:'symbol',description:'source-symbol'}],['falsy-native-name',{errorName:false,message:'recorded'}]])await richScenario(label,'getCurrentBlockHeight',fields);
 for(const[label,fields]of [['null-code-read-replacement',{rawThrow:null}],['undefined-code-read-replacement',{rawThrow:undefined}],['number-rethrow',{rawThrow:7}],['caught-timeout-object',{rawThrow:{code:'ETIMEDOUT',message:['raw','timeout']}}],['timeout-message-coercion-replacement',{code:'ETIMEDOUT',errorMessage:{toString:0}}],['raw-logger-must-not-coerce-name',{name:{toString:0},message:'recorded',code:'EIO'}]])await richScenario(label,'checkNetworkStatus',fields);
 for(const[label,fields]of [['wrap-array-name',{name:['Error'],message:'recorded'}],['wrap-string-name',{name:'Error',message:'recorded'}],['wrap-name-coercion-replacement',{name:{toString:0},message:'recorded'}],['wrap-symbol-replacement',{rawKind:'symbol',description:'wrapped-symbol'}],['wrap-null',{rawThrow:null}],['wrap-native-message-object',{errorMessage:{kind:'message'}}]])await richScenario(label,'TxIndex.fetchChainTip',fields);
 await richScenario('own-undefined-message','getCurrentBlockHeight',{errorMessage:undefined});await richScenario('raw-bigint','getCurrentBlockHeight',{rawKind:'bigint',decimal:'9007199254740993'});await richScenario('raw-null-prototype','getCurrentBlockHeight',{rawKind:'null-prototype',properties:{code:'EIO',message:['raw']}});await richScenario('wrap-null-prototype-replacement','TxIndex.fetchChainTip',{rawKind:'null-prototype',properties:{}});await richScenario('raw-self-referencing-error','getCurrentBlockHeight',{rawKind:'self-error'});
 const shared={label:'shared-error-field'};await richScenario('shared-name-message-identity','getCurrentBlockHeight',{name:shared,errorMessage:shared,message:'constructor'});assert.ok(Object.is(Object.getOwnPropertyDescriptor(lastIOThrown,'name').value,Object.getOwnPropertyDescriptor(lastIOThrown,'message').value),'actual raw fields share one object');
 const empty={_id:'tx-10-empty',value:{txId:'empty',payload:''}};
 await richScenario('partial-height-write-before-raw-rejection','constructConsensusFromIndex',{name:{kind:'raw-name'},message:'post-height failure'},[10],{rows:[empty],maxHeight:10});assert.equal((e.tables.get('consensus')||[]).find(x=>x._id==='MaxProcessedHeight')?.value,10,'actual height write survives raw rejection');
 await richScenario('partial-genesis-before-swallowed-wrap','initOrLoadTxIndex',{name:['Error'],message:'after-genesis'},[],{maxHeight:0});assert.equal((e.tables.get('txIndex')||[]).find(x=>x._id==='genesisBlock')?.value,10,'actual genesis insert survives swallowed wrap');
}
async function richMain(){
 await firstRichTen();const proof=records.map(x=>JSON.stringify(x)).join('\n')+'\n';if(!proofOnly){await expandRich();await expandRuntimeControls();}const payload=records.map(x=>JSON.stringify(x)).join('\n')+'\n';
 const outputs=proofOnly?[['rich_main_v14_proof10.jsonl',proof]]:[['rich_main_v14_proof10.jsonl',proof],['rich_main_v14_vectors.jsonl',payload]];
 for(const[name,value]of outputs){const f=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(f,'utf8'),value);else fs.writeFileSync(f,value);}console.log(JSON.stringify({observations:records.length,actualOriginalCalls:records.length/2,sha256:sha(payload),proofSha256:sha(proof),node:process.version}));
}

async function expandRuntimeControls(){
 const cycle={kind:'height-cycle'};cycle.self=cycle;
 for(const[label,method,io]of[
  ['height-number','getCurrentBlockHeight',{height:12}],
  ['height-object','getCurrentBlockHeight',{height:{v:1,child:{v:2}}}],
  ['height-array','getCurrentBlockHeight',{height:[1,{v:2}]}],
  ['height-cycle','getCurrentBlockHeight',{height:cycle}],
  ['network-active','checkNetworkStatus',{networkInfo:{networkactive:true,connections:2}}],
  ['network-inactive','checkNetworkStatus',{networkInfo:{networkactive:false,connections:3}}],
  ['network-zero','checkNetworkStatus',{networkInfo:{networkactive:true,connections:0}}],
  ['network-object-connection','checkNetworkStatus',{networkInfo:{networkactive:true,connections:{peer:'recorded'}}}],
 ]){await reset('rich-positive/'+label+'/input',{empty:true,clientProfile:{chain:'LTC',test:false,height:12},init:'LTC',rows:[],maxHeight:12,mainGenesis:10,orchestration:{height:12,networkInfo:{networkactive:true,connections:2},failures:[],...io}});await richCall('rich-positive/'+label,method);}
 for(const realm of ['HostIO','SourceVM'])await richScenario('function-prototype/'+realm,'TxIndex.fetchChainTip',{rawKind:'intrinsic-function-prototype',realm});
}
function registerNativeBindings(){
 for(const realm of ['HostIO','SourceVM']){
  const b=realm==='HostIO'?globalThis:vm.runInContext('({Object,Array,Error,Symbol,BigInt})',e.context);
  for(const name of ['Symbol','BigInt']){richIntrinsics.set(b[name],{realm,name});richIntrinsics.set(b[name].prototype,{realm,name:name+'.prototype'});}
  for(const[obj,name]of[[b.Object.prototype,'Object.prototype'],[b.Array.prototype,'Array.prototype'],[b.Error.prototype,'Error.prototype']])for(const field of ['toString','valueOf','join'])if(Object.hasOwn(obj,field))richIntrinsics.set(obj[field],{realm,name:name+'.'+field});
  const stack=Object.getOwnPropertyDescriptor(new b.Error(),'stack');richIntrinsics.set(stack.get,{realm,name:'Error.stack.get'});richIntrinsics.set(stack.set,{realm,name:'Error.stack.set'});
 }
}

richMain().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
