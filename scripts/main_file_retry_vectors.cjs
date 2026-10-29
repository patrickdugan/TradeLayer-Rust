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
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
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
replace('return {ammInstances,ammId,instanceId,adapters','return {database,ammInstances,ammId,instanceId,adapters');
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
let productionIOState,persistenceClass,virtualFSState,factoryFailure,actualTxUtils,pauseView,fileIOState;const records=[];let e,m,Main,TxIndex,marginInstances,contextKeys,startup,unhandled=[],marginReturnedObjects,settlementObserverDepth;
process.on('unhandledRejection',error=>{if(e){const value={name:error?.name||'Error',message:error?.message||String(error)};unhandled.push(value);e.effects.push({kind:'unhandled-rejection',thrown:value});}});
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function classBytes(name,sourceName){const full=source[sourceName],start=full.indexOf('class '+name+' {'),end=full.lastIndexOf('module.exports = '+name);assert.ok(start>=0&&end>start);const bytes=full.slice(start,end).trimEnd();inventory.classes[name]={sha256:sha(bytes),bytes:Buffer.byteLength(bytes),fullClass:true};return bytes;}
function compileClass(name,sourceName,scope){const fn=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes(name,sourceName)+'\nreturn '+(name==='Main'?'[Main,()=>pause]':name)+';})',e.context,{filename:'unchanged-class/'+sourceName+'.js',timeout:3000});const value=fn(...Object.values(scope));if(name==='Main'){pauseView=value[1];return value[0];}return value;}
function unavailable(target){return function(...args){const error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});e.effects.push({kind:'unavailable',target,args:e.copy(args),thrown:{name:error.name,message:error.message}});throw error;};}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);inventory.methods[label]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
function state(){const namesBefore=[...e.tables.keys()],sourceState=e.state();assert.deepEqual([...e.tables.keys()],namesBefore,'audit snapshots must not materialize collections');const A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');assert.ok(e.context.tallyMap===undefined||e.context.tallyMap===T,'actual optional Tally binding must reference the existing original class');return{fileIO:fileIOState,mainPause:pauseView(),source:sourceState,...(marginReturnedObjects?{marginIdentity:marginIdentity()}:{}),ammInstances:e.ammInstances.map((value,id)=>({id:id+1,...value})),propertyRegistryBound:!!e.context.PropertyRegistry,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,classParseBlock:TxIndex.parseBlock,instance:TxIndex.instance,clientIsMainClient:TxIndex.client===m.client},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,vesting:e.load('vesting').instance?{...e.load('vesting').instance}:undefined,vestingStatic:{adminAddress:e.load('vesting').adminAddress,sponsorAddress:e.load('vesting').sponsorAddress,expiryInterval:e.load('vesting').expiryInterval,hedgeLeverage:e.load('vesting').hedgeLeverage},consensusVector:e.load('consensus').consensusVector,insuranceInstances:e.load('insurance').instances,unhandledRejections:unhandled,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache,spotRpcDocumentsBound:e.context.spotRpcDocumentsBound,tallyMapBound:e.context.tallyMap===undefined?undefined:e.context.tallyMap===T},startup,productionIO:productionIOState,persistence:persistenceClass?.instance?{...persistenceClass.instance}:null,virtualFS:virtualFSState};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment({clientProfile:options.clientProfile});factoryFailure=null;const originalFactory=e.storage.getDatabase;e.storage.getDatabase=function(...args){const name=args[0];if(factoryFailure?.collection===name){if(factoryFailure.skip>0)factoryFailure.skip--;else{const error=Error(factoryFailure.message);factoryFailure=null;e.effects.push({kind:'database',name,method:'getDatabase',args:e.copy(args),thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return Reflect.apply(originalFactory,this,args);};marginInstances=[];contextKeys=new Set();startup=[];unhandled=[];marginReturnedObjects=options.trackMarginIdentity?[]:undefined;settlementObserverDepth=0;if(options.empty)for(const table of e.tables.values())table.splice(0,Infinity);if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
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
 function io(method,args){const ordinal=(ioCounts[method]||0)+1;ioCounts[method]=ordinal;const fail=(ioFacts.failures||[]).find(f=>f.method===method&&f.ordinal===ordinal);if(fail){const error=Object.assign(Error(fail.message),{code:fail.code,name:fail.name||'Error'});e.effects.push({kind:'rpc-input',method,args:e.copy(args),ordinal,thrown:{name:error.name,message:error.message,code:error.code}});throw error;}let returned;if(method==='Client.getBlockCount')returned=ioFacts.height;else if(method==='Client.getNetworkInfo')returned=ioFacts.networkInfo;else if(method==='Client.getBlockHash')returned=(ioFacts.blocks||[]).find(b=>b.height===args[0])?.hash;else if(method==='Client.getBlock')returned=(ioFacts.blocks||[]).find(b=>b.hash===args[0])?.block;else if(method==='Client.getChain')returned=options.clientProfile?.chain;else if(method==='Client.getTest')returned=options.clientProfile?.test;else throw Error('unmapped recorded IO '+method);e.effects.push({kind:'rpc-input',method,args:e.copy(args),ordinal,returned:e.copy(returned)});return structuredClone(returned);}
 class RecordedClient{constructor(){this.chain=options.clientProfile?.chain;this.test=options.clientProfile?.test;}async getBlockCount(...args){return io('Client.getBlockCount',args);}async getNetworkInfo(...args){return io('Client.getNetworkInfo',args);}async getBlockHash(...args){return io('Client.getBlockHash',args);}async getBlock(...args){return io('Client.getBlock',args);}async getChain(...args){return io('Client.getChain',args);}async getTest(...args){return io('Client.getTest',args);}}
 const client=new RecordedClient();actualTxUtils=undefined;const ClientWrapper={async getInstance(...args){e.effects.push({kind:'rpc-input',method:'Client.getInstance',args:e.copy(args),returned:{chain:client.chain,test:client.test}});return client;}};TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:new Proxy({},{get(_t,k){return unavailable('TxUtils.'+String(k));}}),transparentIndex:[],process:e.context.process});

 const ioPath=path.posix,dirs=new Set(['/tl-data',...(options.directoryInputs||[])]),files=new Map((options.fileInputs||[]).map(row=>[row.path,row.bytes]));virtualFSState={directories:[],files:[]};
 const fileCounts={};fileIOState=options.fileIOInputs?{facts:structuredClone(options.fileIOInputs),counts:fileCounts}:undefined;const updateFS=()=>{virtualFSState.directories=[...dirs];virtualFSState.files=[...files];};updateFS();
 const ioError=(code,file)=>Object.assign(Error(code+': '+file),{code});
 async function fsCall(method,args,fn){try{if(fileIOState){const ordinal=(fileCounts[method]||0)+1;fileCounts[method]=ordinal;const failure=(fileIOState.facts.failures||[]).find(x=>x.method===method&&x.ordinal===ordinal);if(failure)throw Object.assign(Error(failure.message),{code:failure.code});}const value=fn();updateFS();e.effects.push({kind:'filesystem',method,args:e.copy(args),returned:e.copy(value)});return value;}catch(error){e.effects.push({kind:'filesystem',method,args:e.copy(args),thrown:{name:error.name,message:error.message,code:error.code}});throw error;}}
 const virtualFS={promises:{
  mkdir(file,opts){return fsCall('mkdir',[file,opts],()=>{const parts=file.split('/');let item='';for(const part of parts){if(!part)continue;item+='/'+part;dirs.add(item);}return undefined;});},
  readdir(file,opts){return fsCall('readdir',[file,...(opts===undefined?[]:[opts])],()=>{if(!dirs.has(file))throw ioError('ENOENT',file);const prefix=file+'/';const names=[];for(const key of [...dirs,...files.keys()]){if(key.startsWith(prefix)){const rest=key.slice(prefix.length);if(rest&&!rest.includes('/')&&!names.includes(rest))names.push(rest);}}return names;}).then(names=>opts?.withFileTypes?names.map(name=>({name,isDirectory(){return dirs.has(ioPath.join(file,name));}})):names);},
  readFile(file,encoding){return fsCall('readFile',[file,encoding],()=>{if(!files.has(file))throw ioError('ENOENT',file);return files.get(file);});},
  writeFile(file,value){return fsCall('writeFile',[file,value],()=>{files.set(file,String(value));return undefined;});},
  copyFile(src,dst){return fsCall('copyFile',[src,dst],()=>{if(!files.has(src))throw ioError('ENOENT',src);files.set(dst,files.get(src));return undefined;});},
  unlink(file){return fsCall('unlink',[file],()=>{if(!files.delete(file))throw ioError('ENOENT',file);return undefined;});},
  rm(file,opts){return fsCall('rm',[file,opts],()=>{for(const key of [...files.keys()])if(key===file||key.startsWith(file+'/'))files.delete(key);for(const key of [...dirs])if(key===file||key.startsWith(file+'/'))dirs.delete(key);return undefined;});},
  stat(file){return fsCall('stat',[file],()=>{if(!dirs.has(file)&&!files.has(file))throw ioError('ENOENT',file);return{mtimeMs:(fileIOState?.facts.mtimes||[]).find(x=>x.path===file)?.millis??1700000000000,directory:dirs.has(file)};}).then(v=>({mtimeMs:v.mtimeMs,isDirectory(){return v.directory;}}));},
 }};
 const localClock=makeDrivenClock();driverClock=localClock;const virtualTimer=(callback,ms)=>localClock.schedule(callback,ms);e.context.setTimeout=virtualTimer;e.context.clearTimeout=handle=>localClock.cancel(handle);e.context.process.env.TL_NEDB_ROOT='/tl-data';
 e.adapters.fs=virtualFS;e.adapters.path=ioPath;Object.assign(e.adapters['./client'],ClientWrapper);persistenceClass=e.load('persistence');
 const rawTx=fs.readFileSync(path.join(repo,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(sha(rawTx),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');const depRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(repo,'../../../../tradelayer.js'),external=Module.createRequire(path.join(depRoot,'package.json')),txModule={exports:{}};const txWrapper=vm.runInContext('(function(exports,require,module){\n'+rawTx+'\n})',e.context,{filename:'pinned/txUtils.js'});txWrapper(txModule.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?ClientWrapper:(()=>{throw Error('Unmapped TxUtils require '+key)})(),txModule);actualTxUtils=txModule.exports;
 if(options.persistenceRows)e.table('persistence').push(...structuredClone(options.persistenceRows));
 const genesisText=source.main.slice(source.main.indexOf('const GENESIS_BLOCK_HEIGHTS ='),source.main.indexOf('\nclass Main {'));
 const genesisConfig=vm.runInContext(genesisText+'\nGENESIS_BLOCK_HEIGHTS',e.context);

 Main=compileClass('Main','main',{client,db:e.storage,TxIndex,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:e.load('vesting'),Persistence:persistenceClass,GENESIS_BLOCK_HEIGHTS:genesisConfig,initialize:unavailable('init.initialize'),console:e.context.console,COIN:100000000,pause:false,setTimeout:virtualTimer,process:e.context.process,TxUtils:actualTxUtils});
 for(const[name,receiver]of [['Main',Main.prototype],['TxIndex',TxIndex]])for(const method of Object.getOwnPropertyNames(receiver)){const fn=receiver[method];if(typeof fn==='function')inventory.methods[name+'.'+method]={sha256:sha(fn.toString()),async:Object.prototype.toString.call(fn)==='[object AsyncFunction]'};}
 for(const [receiver,method,label]of [[e.load('tally'),'flushQueuedFeeAccruals','Tally.flushQueuedFeeAccruals'],[e.logic,'settleLiquidityRewards','Logic.settleLiquidityRewards'],[e.Orderbook,'processQueuedOnChainOrdersForBlock','Orderbook.processQueuedOnChainOrdersForBlock'],[e.Orderbook,'processQueuedChannelTradesForBlock','Orderbook.processQueuedChannelTradesForBlock'],[e.load('channels'),'processWithdrawals','Channels.processWithdrawals'],[e.load('clearing'),'clearingFunction','Clearing.clearingFunction']])observe(receiver,method,label);
 if(options.observeTypes)observe(e.types,'decodePayload','Types.decodePayload');
 const AMM=e.load('amm');for(const method of ['addCapital','redeemCapital','quoteContractOrders']){const original=AMM.prototype[method];inventory.methods['AMM.'+method]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};AMM.prototype[method]=function(...args){e.effects.push({kind:'source-function',target:'AMM.'+method,instance:e.ammId(this),args:e.copy(args)});return Reflect.apply(original,this,args);};}
 observe(Main,'getInstance','Main.getInstance');m=new Main();if(options.mainGenesis!==undefined)m.genesisBlock=options.mainGenesis;if(options.volumeRows)e.table('volumeIndex').push(...structuredClone(options.volumeRows));m.txIndex.then(value=>startup.push({returned:value}),error=>startup.push({thrown:{name:error.name,message:error.message}}));await flush();emit(name,{reset:{activationRows:e.copy(e.table('activations')),indexRows:structuredClone(importedRows),heightRow:options.maxHeight??10,...(Object.keys(options).length?{options}: {})},profile:'callable-class'},{returned:{samePrototype:true,sameSingleton:true},startup,metadataInit});}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure){if(failure.op==='getDatabase')factoryFailure={...failure};else e.setFailure(failure);}let transport='Immediate',outcome;try{const parts=method.split('.'),target=parts.length===1?m:parts[0]==='MainStatic'?Main:parts[0]==='PersistenceStatic'?persistenceClass:await persistenceClass.getInstance();const key=parts.at(-1);const value=Reflect.apply(target[key],target,args);if(value&&typeof value.then==='function'){transport='Pending';try{const returned=await value;outcome={returned:method.startsWith('Persistence')&&returned===persistenceClass.instance?{$persistenceInstance:1}:returned};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:method.includes('.')?method.replace(/^MainStatic\./,'Main.').replace(/^PersistenceStatic\./,'Persistence.'):'Main.'+method,args,failure},{transport,outcome});}
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


async function firstServiceTen(){
 const blocks=[1,2,3,4,5].map(height=>({height,hash:'h'+height,block:{hash:'h'+height,previousblockhash:'h'+(height-1),tx:[]}}));
 await reset('service/first10/recorded-empty-block-and-filesystem-inputs',{empty:true,clientProfile:{chain:'LTC',test:false,height:5},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:5,networkInfo:{networkactive:true,connections:2},blocks}});
 await call('service/first10/01-network-status-real-source-conditions','checkNetworkStatus');
 await call('service/first10/02-fetch-real-raw-empty-block','fetchWithRetry',[1]);
 await call('service/first10/03-block-begin-creates-real-persistence-header','blockHandlerBegin',[blocks[0].block,1]);
 await call('service/first10/04-process-empty-block-through-all-three-phases','processBlock',[blocks[1].block,2]);
 await call('service/first10/05-initialize-real-metadata-selects-source-LTC-genesis','initialize');
 await call('service/first10/06-mismatch-confirms-canonical-then-records-when-no-restoration','blockHandlerBegin',[{...blocks[2].block,previousblockhash:'wrong'},3]);
 await call('service/first10/07-fetch-after-real-initialization','fetchWithRetry',[4]);
 await call('service/first10/08-process-next-empty-block-with-source-withdrawal-and-end','processBlock',[blocks[3].block,4]);
 await call('service/first10/09-native-unset-persistence-handle-reorg-error','handleReorg',[4]);
 await call('service/first10/10-real-network-status-again','checkNetworkStatus');
 assert.equal(records.filter(r=>r.command[1].some(([k])=>k==='target')).length,10);
}

async function expandService(){
 const blocks=[1,2,3,4,5].map(height=>({height,hash:'canonical'+height,block:{hash:'canonical'+height,previousblockhash:'canonical'+(height-1),tx:[]}}));
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:5},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:5,networkInfo:{networkactive:true,connections:2},blocks}};
 for(const[name,networkInfo,failure]of[
  ['inactive',{networkactive:false,connections:2}],['zero-connections',{networkactive:true,connections:0}],['missing-connections',{networkactive:true}],
  ['refused',undefined,{method:'Client.getNetworkInfo',ordinal:1,message:'recorded refused',code:'ECONNREFUSED'}],
  ['timeout',undefined,{method:'Client.getNetworkInfo',ordinal:1,message:'recorded timeout',code:'ETIMEDOUT'}],
  ['unexpected',undefined,{method:'Client.getNetworkInfo',ordinal:1,message:'recorded corrupt RPC',code:'EBADMSG'}]
 ]){await reset('service/network/'+name+'/input',{...base,orchestration:{...base.orchestration,networkInfo,failures:failure?[failure]:[]}});await call('service/network/'+name,'checkNetworkStatus');}
 for(const[name,failures,retries]of[
  ['retry-once',[{method:'Client.getBlockHash',ordinal:1,message:'recorded first timeout',code:'ETIMEDOUT'}],2],
  ['retry-exhausted',[1,2,3].map(ordinal=>({method:'Client.getBlockHash',ordinal,message:'recorded repeated timeout',code:'ETIMEDOUT'})),2],
  ['non-timeout-propagates',[{method:'Client.getBlock',ordinal:1,message:'recorded block read refused',code:'ECONNREFUSED'}],3],
  ['negative-retries-no-call',[], -1]
 ]){await reset('service/fetch/'+name+'/input',{...base,orchestration:{...base.orchestration,failures}});await call('service/fetch/'+name,'fetchWithRetry',[3,retries,7]);}
 for(const method of ['blockHandlerBegin','processBlock']){
  await reset('service/reorg-empty-snapshots/'+method+'/input',{...base,persistenceRows:[{_id:'block-2',height:2,hash:'old2',prevHash:'old1',createdAt:1700000000000}]});
  await call('service/reorg-empty-snapshots/'+method,method,[blocks[2].block,3]);
  assert.deepEqual(JSON.parse(JSON.stringify(records.at(-1).summary.outcome.returned)),method==='processBlock'?{reOrg:true,restore:0}:0);
 }
 for(const[collection,op]of[['persistence','findOne'],['persistence','update'],['txIndex','getDatabase']]){
  await reset('service/block-failure/'+collection+'-'+op+'/input',base);await call('service/block-failure/'+collection+'-'+op,'processBlock',[blocks[1].block,2],{collection,op,message:'block-'+collection+'-'+op});
 }
}

async function liveFiniteService(){
 const blocks=[0,1,2,3,4,5].map(height=>({height,hash:'canonical'+height,block:{hash:'canonical'+height,previousblockhash:'canonical'+(height-1),tx:[]}}));
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:5},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:5,networkInfo:{networkactive:true,connections:2},blocks}};
 await reset('service/recovery-already-up/input',base);await call('service/recovery-already-up/loads-both-progress-markers','enterRecoveryMode',[3,4]);
 await reset('service/recovery-timeout-then-up/input',{...base,orchestration:{...base.orchestration,failures:[{method:'Client.getNetworkInfo',ordinal:1,message:'recorded transient outage',code:'ETIMEDOUT'}]}});await call('service/recovery-timeout-then-up/real-retry-timer-and-progress-reads','enterRecoveryMode',[3,4]);
 for(const track of[5,3]){await reset('service/live-finite-'+track+'/input',{...base,orchestration:{...base.orchestration,failures:[{method:'Client.getBlockCount',ordinal:2,message:'recorded end of live IO capture',code:'EIO'}]}});await call('service/live-finite-'+track+'/original-live-loop-until-recorded-IO-error','processIncomingBlocks',[5-track,track,5]);}
 await reset('service/sync-unawaited-live/input',{...base,orchestration:{...base.orchestration,failures:[{method:'Client.getBlockCount',ordinal:2,message:'recorded spawned live error',code:'EIO'}]}});await call('service/sync-unawaited-live/drained-original-rejection','syncIfNecessary');
}

async function originalStartupModules(){
 await reset('startup/original-modules/shared-source-input',{empty:true,clientProfile:{chain:'LTC',test:false,height:5},rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:5,networkInfo:{networkactive:true,connections:2},blocks:[]}});
 const readSource=(name,file)=>numeric.readPinnedSource(path.join(repo,'fixtures',file),{db:'b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38',init:'55c7dd6da798a58dc57d34ad75be357f9811574db5e6f384620055f966447cc0'}[name]);
 const originalDb=readSource('db','legacy_db_source.txt'),originalInit=readSource('init','legacy_init_source.txt');
 class DatastoreIO{
  constructor(options){this.category=path.posix.basename(options.filename,'.db');e.effects.push({kind:'datastore-open',filename:options.filename,autoload:options.autoload,category:this.category});}
 }
 for(const[short,method]of[['find','findAsync'],['findOne','findOneAsync'],['insert','insertAsync'],['update','updateAsync'],['remove','removeAsync']])DatastoreIO.prototype[short]=function(...args){const callback=args.pop();e.database(this.category)[method](...args).then(value=>callback(null,value),error=>callback(error));};
 DatastoreIO.prototype.count=function(query,callback){e.database(this.category).findAsync(query).then(rows=>callback(null,rows.length),error=>callback(error));};
 const instantiate=(text,name,required)=>{const module={exports:{}};vm.runInContext('(function(require,module,exports,__dirname){\n'+text+'\n})',e.context,{filename:'unchanged-full-module/'+name+'.js'})(required,module,module.exports,'/tl-source');return module.exports;};
 e.effects.length=0;
 const actualDb=instantiate(originalDb,'db',key=>key==='nedb'?DatastoreIO:key==='path'?path.posix:key==='util'?require('node:util'):key==='./client'?e.adapters['./client']:(()=>{throw Error('unmapped DB dependency '+key)})());
 const actualFactory=actualDb.getDatabase.bind(actualDb);actualDb.getDatabase=function(...args){const name=args[0];e.effects.push({kind:'database',name,method:'getDatabase',args:e.copy(args),returned:{database:name}});return actualFactory(...args);};
 // Existing module references and the newly imported Main share one IO row store.
 e.storage.getDatabase=(...args)=>actualDb.getDatabase(...args);e.adapters['./db']=actualDb;
 const actualInit=instantiate(originalInit,'init',key=>key==='./client'?e.adapters['./client']:key==='./db'?actualDb:(()=>{throw Error('unmapped init dependency '+key)})());
 e.adapters['./init']=actualInit;e.adapters['./txIndex']=TxIndex;e.adapters['./persistence']=persistenceClass;e.adapters.events=require('node:events');
 Main=e.load('main');await flush();
 inventory.startupModules={mainSHA256:sha(source.main),initSHA256:sha(originalInit),dbSHA256:sha(originalDb),actualDatastoreCategories:Object.keys(actualDb.databases),databaseMetadata:{initialized:actualDb.initialized,initializing:actualDb.initializing,path:actualDb.path,test:actualDb.test}};
 records.push({name:'startup/unchanged-main-init-db-module-IIFEs',domain:'source-only-startup',command:tag({moduleImport:'main',originalInit:true,originalDb:true}),result:tag({exportedClass:Main.name}),state:tag({source:e.state(),database:inventory.startupModules.databaseMetadata,mainClass:{instance:Main.instance,isInitializing:Main.isInitializing,persistenceInitialized:Main.persistenceInitialized},activation:e.load('activation').instance}),effects:tag(e.effects)});
 e.effects.length=0;m=await Main.getInstance();await flush();emit('startup/original-static-Main-getInstance-and-Persistence-initialization',{target:'Main.getInstance',static:true,args:[]},{transport:'Pending',outcome:{returned:{samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m}}});
 await call('startup/original-initialize-after-real-import','initialize');
}

async function servicePeerControls(){
 const blocks=[0,1,2,3].map(height=>({height,hash:'canonical'+height,block:{hash:'canonical'+height,previousblockhash:'canonical'+(height-1),tx:[]}}));const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:3,networkInfo:{networkactive:true,connections:2},blocks}};
 for(const[name,chain]of[['FEFF','\uFEFFLTC'],['0085','\u0085LTC']]){await reset('service/review/chain-'+name+'/input',{...base,clientProfile:{chain,test:false,height:3}});await call('service/review/chain-'+name,'blockHandlerBegin',[blocks[1].block,1]);}
 await reset('service/review/missing-chain/input',{...base,clientProfile:{test:false,height:3},init:undefined});await call('service/review/missing-chain/first-partial-instance-throw','blockHandlerBegin',[blocks[1].block,1]);await call('service/review/missing-chain/retry-reuses-original-partial-instance','blockHandlerBegin',[blocks[1].block,1]);
 await reset('service/review/immediate-snapshot-file/input',{...base,persistenceRows:[{_id:'block-2',height:2,hash:'old2',prevHash:'old1',createdAt:1700000000000}],fileInputs:[{path:'/tl-data/ltc-main-snapshots/junk',bytes:'ordinary file'}]});await call('service/review/immediate-snapshot-file/original-nondirectory-is-observed-then-filtered','processBlock',[blocks[3].block,3]);
 for(const[name,retries,failures]of[['NaN',NaN,[]],['negative-Infinity',-Infinity,[]],['Infinity-first-success',Infinity,[]],['null-single-timeout',null,[{method:'Client.getBlockHash',ordinal:1,code:'ETIMEDOUT',message:'retry null'}]],['string-single-timeout','0',[{method:'Client.getBlockHash',ordinal:1,code:'ETIMEDOUT',message:'retry string'}]],['constructor-error-has-no-timeout-code',3,[{method:'Client.getBlockHash',ordinal:1,code:'ETIMEDOUT',message:{toString:0}}]]]){await reset('service/review/retry-'+name+'/input',{...base,orchestration:{...base.orchestration,failures}});await call('service/review/retry-'+name,'fetchWithRetry',[1,retries,7]);}
}
async function main(){
 if(flags.includes('--startup-modules')){await originalStartupModules();const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';const file=path.join(stage,'fixtures/main_startup_modules.jsonl');if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),payload);else fs.writeFileSync(file,payload);console.log(JSON.stringify({startupObservations:records.length,fixtureSha256:sha(payload),originalModules:inventory.startupModules}));return;}
 inventory.boundary.callable='unchanged full Main class plus unchanged complete Persistence module; actual recorded IO adapter, original asynchronous control flow';
 inventory.boundary.rpc='recorded client/network/block facts and errors only; virtual timer and POSIX filesystem data map; no financial/validation result';
 inventory.boundary.production='class-callable startup/block-begin/end/processBlock proof; full module startup IIFEs and actual durable rollback are separate pending boundaries';
 await firstServiceTen();const proof=records.map(r=>JSON.stringify(r)).join('\n')+'\n';if(!proofOnly){await expandService();await liveFiniteService();await servicePeerControls();}const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';inventory.observations=records.length;inventory.calls=records.filter(r=>r.command[1].some(([k])=>k==='target')).length;inventory.fixtureSha256=sha(payload);inventory.proof10Sha256=sha(proof);
 const outputs=proofOnly?[['main_file_io_proof10.jsonl',proof]]:[['main_file_io_proof10.jsonl',proof],['main_file_io_vectors.jsonl',payload],['main_file_io_inventory.json',JSON.stringify(inventory,null,2)+'\n']];
 for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}
 console.log(JSON.stringify({frames:records.length,actualMainCalls:inventory.calls,fixtureSha256:sha(payload),proof10Sha256:sha(proof),firstTenComplete:true}));
}
async function firstSnapshotTen(){
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3001},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,fileInputs:[{path:'/tl-data/ltc-main/tally.db',bytes:'recorded tally bytes\n'},{path:'/tl-data/ltc-main/consensus.db',bytes:'recorded consensus bytes\n'},{path:'/tl-data/ltc-main/readme.txt',bytes:'retain ordinary file'}],orchestration:{height:3001,blocks:[]}};
 await reset('snapshot/first10/recorded-files-same-open-database-input',base);
 await call('snapshot/first10/01-original-delay','delay',[7]);
 await call('snapshot/first10/02-original-pause-toggle-on','setPause');
 await call('snapshot/first10/03-original-pause-toggle-off','setPause');
 await call('snapshot/first10/04-static-genesis-really-reaches-warm-singleton','MainStatic.getGenesisBlock');
 await call('snapshot/first10/05-scheduled-checkpoint-from-real-Main-begin','blockHandlerBegin',[{hash:'h1000',previousblockhash:'h999',tx:[]},1000]);
 await call('snapshot/first10/06-read-generated-snapshot-metadata','Persistence.listSnapshots');
 await call('snapshot/first10/07-second-snapshot-through-main-three-phases','processBlock',[{hash:'h2000',previousblockhash:'h1999',tx:[]},2000]);
 await call('snapshot/first10/08-third-snapshot-prunes-only-oldest','processBlock',[{hash:'h3000',previousblockhash:'h2999',tx:[]},3000]);
 const stored=virtualFSState.files.find(([key])=>key.startsWith('/tl-data/ltc-main-snapshots/2000-')&&key.endsWith('/meta.json'));assert.ok(stored);const snapshot={...JSON.parse(stored[1]),dir:stored[0].split('/').at(-2)};
 await call('snapshot/first10/09-restore-generated-nonempty-file-snapshot','Persistence.restoreSnapshot',[snapshot]);
 await call('snapshot/first10/10-actual-TxUtils-lacks-static-latest-method','MainStatic.getLastBlock');
 assert.equal(records.filter(r=>r.command[1].some(([k])=>k==='target')).length,10);
}
async function expandSnapshots(){
 const block={hash:'new3',previousblockhash:'canonical2',tx:[]};
 const meta={blockHeight:1,consensusHash:'recorded-source-file-label',createdAt:'2023-11-14T22:13:20.000Z',files:['tally.db']};
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:3,blocks:[{height:1,hash:'canonical1',block:{hash:'canonical1',tx:[]}},{height:2,hash:'canonical2',block:{hash:'canonical2',tx:[]}}]},persistenceRows:[{_id:'block-1',height:1,hash:'canonical1'},{_id:'block-2',height:2,hash:'old2'}],directoryInputs:['/tl-data/ltc-main-snapshots/one'],fileInputs:[{path:'/tl-data/ltc-main/tally.db',bytes:'current-file-not-a-Tally-verdict'},{path:'/tl-data/ltc-main/keep.txt',bytes:'ordinary-current-file'},{path:'/tl-data/ltc-main-snapshots/one/meta.json',bytes:JSON.stringify(meta)},{path:'/tl-data/ltc-main-snapshots/one/tally.db',bytes:'historical-file-does-not-reload-live-Tally'}]};
 for(const method of ['blockHandlerBegin','processBlock']){await reset('snapshot/reorg/'+method+'/input',base);await call('snapshot/reorg/'+method+'/actual-nonempty-snapshot-restoration',method,[block,3]);assert.equal(records.at(-1).summary.outcome.returned?.restore??records.at(-1).summary.outcome.returned,1);}
 for(const [name,failure]of [['consensus-hash-read',{collection:'tallyMap',op:'find',message:'checkpoint-hash-read'}],['store-hash-swallowed',{collection:'consensus',op:'update',message:'checkpoint-store'}],['max-height-after-files',{collection:'consensus',op:'update',skip:1,message:'checkpoint-max'}],['old-header-cleanup',{collection:'persistence',op:'remove',message:'checkpoint-cleanup'}]]){await reset('snapshot/failure/'+name+'/input',{...base,persistenceRows:[]});await call('snapshot/failure/'+name+'/original-partial-artifact-and-row-prefix','blockHandlerBegin',[{hash:'checkpoint1000',previousblockhash:'none',tx:[]},1000],failure);assert.ok(e.effects.some(x=>x.thrown?.message===failure.message),name+' failure reached');}
 for(const failure of [undefined,{collection:'consensus',op:'update',message:'unawaited-restored-track-write'},{collection:'consensus',op:'getDatabase',message:'unawaited-restored-track-factory'}]){await reset('snapshot/restore/'+(failure?.op||'ordinary')+'/input',base);await call('snapshot/restore/'+(failure?.op||'ordinary')+'/same-open-row-owners','Persistence.restoreSnapshot',[{...meta,dir:'one'}],failure);}
 await reset('snapshot/restore/missing-file/input',{...base,fileInputs:base.fileInputs.filter(x=>!x.path.endsWith('/one/tally.db'))});await call('snapshot/restore/missing-file/keeps-source-nonatomic-copy-delete-prefix','Persistence.restoreSnapshot',[{...meta,dir:'one'}]);
 await reset('snapshot/list/invalid-metadata/input',{...base,directoryInputs:[...base.directoryInputs,'/tl-data/ltc-main-snapshots/bad','/tl-data/ltc-main-snapshots/null'],fileInputs:[...base.fileInputs,{path:'/tl-data/ltc-main-snapshots/bad/meta.json',bytes:'not json'},{path:'/tl-data/ltc-main-snapshots/null/meta.json',bytes:'null'}]});await call('snapshot/list/invalid-metadata/skips-only-source-caught-errors','Persistence.listSnapshots');
}
async function malformedSnapshotMeta(){
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,directoryInputs:['/tl-data/ltc-main-snapshots/one'],fileInputs:[{path:'/tl-data/ltc-main/tally.db',bytes:'current source file'},{path:'/tl-data/ltc-main/keep.txt',bytes:'retain ordinary file'},{path:'/tl-data/ltc-main-snapshots/one/tally.db',bytes:'recorded historical file'}],orchestration:{height:3,blocks:[]}};
 for(const[name,meta]of [['missing-files',{blockHeight:1,dir:'one'}],['null-files',{blockHeight:1,dir:'one',files:null}],['false-files',{blockHeight:1,dir:'one',files:false}],['number-files',{blockHeight:1,dir:'one',files:1}],['object-files',{blockHeight:1,dir:'one',files:{}}],['string-files',{blockHeight:1,dir:'one',files:'ab'}],['empty-string-files',{blockHeight:1,dir:'one',files:''}],['numeric-file-element',{blockHeight:1,dir:'one',files:[1]}],['null-file-element',{blockHeight:1,dir:'one',files:[null]}],['numeric-dir',{blockHeight:1,dir:1,files:[]}],['object-dir',{blockHeight:1,dir:{},files:[]}],['null-meta',null]]){await reset('snapshot/meta/'+name+'/input',base);await call('snapshot/meta/'+name+'/literal-native-failure-and-file-prefix','Persistence.restoreSnapshot',[meta]);}
}
async function snapshotMain(){
 await firstSnapshotTen();const proof=records.map(r=>JSON.stringify(r)).join('\n')+'\n';if(!proofOnly){await expandSnapshots();await malformedSnapshotMeta();}const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';
 for(const[name,value]of (proofOnly?[['main_file_io_proof10.jsonl',proof]]:(proofOnly?[['main_file_io_proof10.jsonl',proof]]:[['main_file_io_proof10.jsonl',proof],['main_file_io_vectors.jsonl',payload]]))){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}
 console.log(JSON.stringify({frames:records.length,calls:records.filter(r=>r.command[1].some(([k])=>k==='target')).length,sha256:sha(payload)}));
}
async function firstFileTen(){
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,fileInputs:[{path:'/tl-data/ltc-main/a.db',bytes:'recorded a bytes'},{path:'/tl-data/ltc-main/note.txt',bytes:'ordinary retain'}],fileIOInputs:{failures:[{method:'copyFile',ordinal:1,code:'EBUSY',message:'recorded busy copy'},{method:'unlink',ordinal:1,code:'EPERM',message:'recorded locked delete'}],mtimes:[]},orchestration:{height:3,blocks:[{height:3,hash:'h3',block:{hash:'h3',tx:[]}}]}};
 await reset('file-io/first10/shared-rows-and-recorded-IO-inputs',base);
 await call('file-io/first10/01-safe-copy-busy-50ms-then-success','Persistence.safeCopy',['/tl-data/ltc-main/a.db','/tl-data/ltc-main/b.db']);
 await call('file-io/first10/02-safe-delete-permission-50ms-then-success','Persistence.safeDelete',['/tl-data/ltc-main/b.db']);
 await call('file-io/first10/03-safe-delete-missing-is-success','Persistence.safeDelete',['/tl-data/ltc-main/missing.db']);
 await call('file-io/first10/04-safe-copy-missing-propagates','Persistence.safeCopy',['/tl-data/ltc-main/missing.db','/tl-data/ltc-main/c.db']);
 await call('file-io/first10/05-direct-update-max-returns-height','Persistence.updateMaxProcessed',[5]);
 await call('file-io/first10/06-direct-record-header','Persistence.recordBlockHeader',[3,'h3','h2']);
 await call('file-io/first10/07-direct-read-header','Persistence.getBlockHeader',[3]);
 await call('file-io/first10/08-direct-last-known-header','Persistence.getLastKnownBlock');
 await call('file-io/first10/09-direct-prune-empty-snapshots','Persistence.pruneOldSnapshots',[0]);
 await call('file-io/first10/10-direct-detect-equal-chain-tip','Persistence.detectAndHandleReorg');
 assert.equal(records.length,11);assert.equal(records[5].summary.outcome.returned,5);assert.equal(records[10].summary.outcome.returned,null);
}
async function expandFileIo(){
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,fileInputs:[{path:'/tl-data/ltc-main/a.db',bytes:'a'},{path:'/tl-data/ltc-main/b.db',bytes:'b'},{path:'/tl-data/ltc-main/note.txt',bytes:'keep'}],fileIOInputs:{failures:[],mtimes:[]},orchestration:{height:3,blocks:[]}};
 for(const[method,op]of[['safeCopy','copyFile'],['safeDelete','unlink']])for(const[code,count]of[['EBUSY',5],['EPERM',5],['EBUSY',4],['EIO',1]]){
  const failures=Array.from({length:count},(_,i)=>({method:op,ordinal:i+1,code,message:'recorded '+code+' '+(i+1)}));const name='file-io/retries/'+method+'/'+code+'-'+count;
  await reset(name+'/input',{...base,fileIOInputs:{failures,mtimes:[]}});await call(name+'/actual-five-attempt-source-loop','Persistence.'+method,method==='safeCopy'?['/tl-data/ltc-main/a.db','/tl-data/ltc-main/c.db']:['/tl-data/ltc-main/a.db']);
  assert.equal(e.effects.filter(x=>x.kind==='timer').length,code==='EIO'?0:count);
 }
 for(const[stage,failure]of[['copy-second',{method:'copyFile',ordinal:2,code:'EIO',message:'copy second'}],['metadata-write',{method:'writeFile',ordinal:1,code:'EPERM',message:'meta write'}],['snapshot-list',{method:'readdir',ordinal:2,code:'EIO',message:'snapshot list'}]]){
  const name='file-io/checkpoint/'+stage;await reset(name+'/input',{...base,fileIOInputs:{failures:[failure],mtimes:[]}});await call(name+'/actual-partial-header-file-row-prefix','blockHandlerBegin',[{hash:'h1000',previousblockhash:'h999',tx:[]},1000]);assert.ok(e.effects.some(x=>x.thrown?.message===failure.message));
 }
 const backupDirs=['old','new','middle'].map(x=>'/tl-data/ltc-main-backups/backup-'+x),mtimes=backupDirs.map((path,i)=>({path,millis:[1,9,5][i]}));
 const restore={...base,directoryInputs:[...backupDirs,'/tl-data/ltc-main-snapshots/one'],fileInputs:[...base.fileInputs,...backupDirs.map((dir,i)=>({path:dir+'/old.db',bytes:'old'+i})),{path:'/tl-data/ltc-main-snapshots/one/a.db',bytes:'historical a'}],fileIOInputs:{failures:[],mtimes}};
 for(const[name,failures]of[['actual-newest-mtime',[]],['failed-stat-nonfatal',[{method:'stat',ordinal:1,code:'EIO',message:'stat old'}]],['failed-old-prune-nonfatal',[{method:'rm',ordinal:1,code:'EPERM',message:'prune middle'}]],['backup-copy-partial',[{method:'copyFile',ordinal:2,code:'EIO',message:'backup copy'}]],['delete-five-permissions',Array.from({length:5},(_,i)=>({method:'unlink',ordinal:i+1,code:'EPERM',message:'locked '+i}))]]){await reset('file-io/restore/'+name+'/input',{...restore,fileIOInputs:{failures,mtimes}});await call('file-io/restore/'+name+'/same-open-rows-and-source-IO-order','Persistence.restoreSnapshot',[{blockHeight:1,dir:'one',files:['a.db']}]);}
 for(const[method,args,failure]of[['updateMaxProcessed',[7],{collection:'consensus',op:'update',message:'direct max write'}],['saveTrackHeight',[9],undefined],['cleanupPersistence',[3000],undefined],['getLastKnownBlock',[],undefined],['checkForReorgForNewBlock',[1,'h0'],undefined]]){await reset('file-io/direct/'+method+'/input',{...base,persistenceRows:[{_id:'block-1999',height:1999,hash:'old'},{_id:'block-2000',height:2000,hash:'edge'},{_id:'block-3000',height:3000,hash:'keep'}]});await call('file-io/direct/'+method+'/original-return-and-live-rows','Persistence.'+method,args,failure);}
}
async function directPersistenceLifecycle(){
 const base={empty:true,clientProfile:{chain:'LTC',test:false,height:3},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,fileIOInputs:{failures:[],mtimes:[]},orchestration:{height:3,blocks:[]}};
 await reset('file-io/lifecycle/options-and-warm-singleton/input',base);
 await call('file-io/lifecycle/01-real-static-getter-constructor-options','PersistenceStatic.getInstance',[{network:'LTC',test:false,snapshotInterval:0}]);
 await call('file-io/lifecycle/02-warm-getter-ignores-different-options','PersistenceStatic.getInstance',[{network:'DOGE',test:true,snapshotInterval:1}]);
 await call('file-io/lifecycle/03-direct-reinit-uses-constructor-raw-fields','Persistence.init',[{network:'DOGE',test:true}]);
 await call('file-io/lifecycle/04-disabled-interval-returns-before-hash','Persistence.maybeCheckpoint',[1000]);
 await reset('file-io/lifecycle/default-reinit-native-failure/input',{...base,orchestration:{...base.orchestration,failures:[{method:'Client.getChain',ordinal:2,message:'recorded re-init chain failure',code:'EIO'}]}});
 await call('file-io/lifecycle/05-default-static-getter-discovers-network','PersistenceStatic.getInstance');
 await call('file-io/lifecycle/06-direct-reinit-keeps-partial-and-old-paths','Persistence.init');
 await call('file-io/lifecycle/07-warm-getter-keeps-same-instance-after-failure','PersistenceStatic.getInstance',[null]);
 await call('file-io/lifecycle/08-next-direct-reinit-really-retries','Persistence.init');
 await reset('file-io/lifecycle/null-options-before-constructor/input',base);
 await call('file-io/lifecycle/09-null-options-native-read-before-instance','PersistenceStatic.getInstance',[null]);
 await call('file-io/lifecycle/10-default-retry-creates-first-real-instance','PersistenceStatic.getInstance');
}
async function fileMain(){await firstFileTen();const proof=records.map(x=>JSON.stringify(x)).join('\n')+'\n';if(!proofOnly){await expandFileIo();await directPersistenceLifecycle();}const payload=records.map(x=>JSON.stringify(x)).join('\n')+'\n';for(const[name,bytes]of (proofOnly?[['main_file_io_proof10.jsonl',proof]]:[['main_file_io_proof10.jsonl',proof],['main_file_io_vectors.jsonl',payload]])){const f=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(f,'utf8'),bytes);else fs.writeFileSync(f,bytes);}console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command[1].some(([k])=>k==='target')).length,sha256:sha(payload)}));}
let driverClock,driverCalls=[],driverJobs=[],driverActions=0,driverTopCalls=0;
const resolutionSignals=new WeakSet();
const stopResolutionHook=require('node:v8').promiseHooks.onSettled(promise=>resolutionSignals.add(promise));
function makeDrivenClock(){
 const events=e.effects;let now=0,next=1;const timers=[];
 return{
  schedule(callback,delay){assert.equal(typeof callback,'function');assert.equal(typeof delay,'number');assert.ok(Number.isFinite(delay)&&delay>=0);const timer={id:next++,delay,due:now+delay,state:'Scheduled',callback};timers.push(timer);events.push({kind:'timer-scheduled',id:timer.id,delay,due:timer.due});return timer.id;},
  cancel(id){const timer=timers.find(row=>row.id===id);if(timer?.state==='Scheduled'){timer.state='Cancelled';events.push({kind:'timer-cancelled',id});}return undefined;},
  next(){return timers.filter(row=>row.state==='Scheduled').sort((a,b)=>a.due-b.due||a.id-b.id)[0]?.id;},
  fire(id){const timer=timers.find(row=>row.id===id);if(!timer||timer.state!=='Scheduled')return{refused:'TimerNotScheduled'};const first=this.next();if(id!==first)return{refused:'TimerNotNext'};now=Math.max(now,timer.due);timer.state='Fired';events.push({kind:'timer-fired',id,logicalTime:now});timer.callback();return{fired:id};},
  snapshot(){return{profile:'explicit-callbacks/no-implicit-Date-advance/v1',logicalTime:now,timers:timers.map(({id,delay,due,state})=>({id,delay,due,state}))};}
 };
}
function observeMainInvocations(){
 for(const method of ['safeCopy','safeDelete','createSnapshot','restoreSnapshot','maybeCheckpoint']){
  const original=persistenceClass.prototype[method];assert.equal(typeof original,'function');
  inventory.methods['Persistence.'+method]={sha256:sha(original.toString())};
  persistenceClass.prototype[method]=function(...args){
   const call={id:driverCalls.length+1,method:'Persistence.'+method,args:tag(args),samePersistence:this===persistenceClass.instance,entered:true};driverCalls.push(call);
   try{const result=Reflect.apply(original,this,args);call.returned=true;if(result&&typeof result.then==='function')call.promise=result;else call.immediate=tag(result);return result;}
   catch(error){call.synchronousThrow={name:error.name,message:error.message};throw error;}
  };
 }

 for(const method of ['syncIfNecessary','processIncomingBlocks','enterRecoveryMode','setPause','saveTrackHeight','loadTrackHeight','loadMaxProcessedHeight','checkBlockLag','checkNetworkStatus','fetchWithRetry','processBlock','delay','shutdown']){
  const original=Main.prototype[method];assert.equal(typeof original,'function');
  Main.prototype[method]=function(...args){
   const call={id:driverCalls.length+1,method,args:tag(args),sameReceiver:this===m,entered:true};driverCalls.push(call);
   try{const result=Reflect.apply(original,this,args);call.returned=true;if(result&&typeof result.then==='function'){call.promise=result;}else call.immediate=tag(result);return result;}
   catch(error){call.synchronousThrow={name:error.name,message:error.message};throw error;}
  };
 }
}
function schedulerState(){
 return{actions:driverActions,topLevelSourceCalls:driverTopCalls,clock:driverClock.snapshot(),jobs:driverJobs.map(({id,target,args,status,value,error})=>({id,target,args,status,...(status==='Fulfilled'?{value}:{}),...(status==='Rejected'?{error}:{})})),invocations:driverCalls.map(({promise,...call})=>({...call,...(promise?{resolutionSignaled:resolutionSignals.has(promise),outcomeObservation:'not inspected; no internal .then handler attached'}:{})}))};
}
async function checkpointControl(name,command,result={}){
 await flush();driverActions++;
 emit(name,{controller:command},{control:result});
 records.at(-1).scheduler=schedulerState();
 e.effects.length=0;
}
async function sourceStart(name,target,args=[]){
 driverTopCalls++;
 const job={id:driverJobs.length+1,target,args:tag(args),status:'Entered'};driverJobs.push(job);
 try{
  const parts=target.split('.'),receiver=parts[0]==='Persistence'?await persistenceClass.getInstance():m,key=parts.at(-1);const value=Reflect.apply(receiver[key],receiver,args);
  if(value&&typeof value.then==='function'){
   job.status='Pending';
   // This is the actual external caller's observer, not an observer installed
   // on source-internal discarded promises. It does not change their handling.
   value.then(result=>{job.status='Fulfilled';job.value=tag(result);},error=>{job.status='Rejected';job.error={name:error.name,message:error.message};});
  }else{job.status='Fulfilled';job.value=tag(value);}
 }catch(error){job.status='Thrown';job.error={name:error.name,message:error.message};}
 await checkpointControl(name,{kind:'StartSourceCall',target:target.includes('.')?target:'Main.'+target,args:tag(args)},{started:job.id});
 return job;
}
async function fireNext(name){const id=driverClock.next();assert.notEqual(id,undefined,'explicit pending timer');const result=driverClock.fire(id);assert.deepEqual(result,{fired:id});await checkpointControl(name,{kind:'FireNextTimer',id},result);return id;}
async function updateRecordedIoFacts(name,changes){
 Object.assign(productionIOState.facts,structuredClone(changes));
 await checkpointControl(name,{kind:'RecordedIOFactsUpdate',changes:tag(changes)},{updated:true});
}
const schedulerBase={empty:true,clientProfile:{chain:'LTC',test:false,height:0},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,orchestration:{height:0,networkInfo:{networkactive:true,connections:2},blocks:[]}};
async function schedulerReset(label,changes={}){
 driverCalls=[];driverJobs=[];
 await reset(label,{...structuredClone(schedulerBase),...changes});
 observeMainInvocations();records.at(-1).scheduler=schedulerState();e.effects.length=0;
}
function trackHeight(){return e.tables.get('consensus')?.find(row=>row._id==='TrackHeight')?.value;}
async function firstSchedulerTen(){
 await schedulerReset('scheduler/first10/pause/input');
 await sourceStart('scheduler/first10/01-pause','setPause');assert.equal(pauseView(),true);
 const sync=await sourceStart('scheduler/first10/02-sync-is-pending','syncIfNecessary');assert.equal(sync.status,'Pending');assert.equal(driverClock.snapshot().timers[0].delay,1000);
 await fireNext('scheduler/first10/paused-timer-rearms-without-completing');assert.equal(sync.status,'Pending');
 await sourceStart('scheduler/first10/03-unpause-is-not-timer-fire','setPause');assert.equal(sync.status,'Pending');
 await fireNext('scheduler/first10/resume-sync-fulfills-but-unawaited-incoming-waits');assert.equal(sync.status,'Fulfilled');assert.equal(driverClock.snapshot().timers.at(-1).delay,10000);
 await sourceStart('scheduler/first10/04-shared-height-write-during-incoming-wait','saveTrackHeight',[5]);assert.equal(trackHeight(),5);
 await sourceStart('scheduler/first10/05-shared-height-read-during-incoming-wait','checkBlockLag');
 await schedulerReset('scheduler/first10/recovery-and-incoming/input',{orchestration:{height:0,networkInfo:{networkactive:false,connections:0},blocks:[]}});
 const recovery=await sourceStart('scheduler/first10/06-recovery-is-pending','enterRecoveryMode',[0,1]);assert.equal(recovery.status,'Pending');
 await sourceStart('scheduler/first10/07-concurrent-source-write','saveTrackHeight',[7]);
 const incoming=await sourceStart('scheduler/first10/08-concurrent-incoming-is-pending','processIncomingBlocks',[0,7,0]);assert.equal(incoming.status,'Pending');
 await sourceStart('scheduler/first10/09-pause-does-not-resolve-existing-waits','setPause');assert.equal(recovery.status,'Pending');assert.equal(incoming.status,'Pending');
 await updateRecordedIoFacts('scheduler/first10/network-data-becomes-active',{networkInfo:{networkactive:true,connections:2}});
 await fireNext('scheduler/first10/recovery-resumes-first-and-reads-current-same-owner-height');assert.equal(recovery.status,'Fulfilled');assert.deepEqual(recovery.value,tag(7));assert.equal(incoming.status,'Pending');
 await sourceStart('scheduler/first10/10-same-owner-height-read','loadTrackHeight');assert.equal(driverTopCalls,10);
}
async function expandScheduler(){
 const live={orchestration:{height:1,networkInfo:{networkactive:true,connections:2},blocks:[{height:1,hash:'h1',block:{hash:'h1',previousblockhash:'h0',tx:[]}}]}};
 await schedulerReset('scheduler/live/source-processing-before-pause/input',live);
 const incoming=await sourceStart('scheduler/live/real-empty-block-before-timer','processIncomingBlocks',[1,0,1]);assert.equal(incoming.status,'Pending');assert.equal(trackHeight(),1);
 await sourceStart('scheduler/live/concurrent-height9-write','saveTrackHeight',[9]);assert.equal(trackHeight(),9);
 await sourceStart('scheduler/live/pause-while10s-is-pending','setPause');
 await fireNext('scheduler/live/old-chain-tip-write-overwrites-concurrent9-before-next-pause-check');assert.equal(trackHeight(),1);assert.equal(incoming.status,'Pending');assert.equal(driverClock.snapshot().timers.at(-1).delay,1000);
 await sourceStart('scheduler/live/unpause-does-not-resolve-the1s-promise','setPause');assert.equal(incoming.status,'Pending');
 await fireNext('scheduler/live/outer-incoming-fulfills-after-recursive-sync-spawns-new-incoming');assert.equal(incoming.status,'Fulfilled');
 const id=driverClock.next();assert.notEqual(id,undefined);driverClock.cancel(id);
 await checkpointControl('scheduler/live/clearing-a-timer-leaves-source-await-unresolved',{kind:'ClearTimer',id});
 const before=JSON.stringify(schedulerState()),refused=driverClock.fire(id);assert.deepEqual(refused,{refused:'TimerNotScheduled'});assert.equal(JSON.stringify(schedulerState()),before);
 await checkpointControl('scheduler/live/refired-cancelled-handle-is-controller-refusal',{kind:'FireTimer',id},refused);

 await schedulerReset('scheduler/recovery/rejection/input',{orchestration:{height:0,networkInfo:{networkactive:false,connections:0},blocks:[]}});
 const recovering=await sourceStart('scheduler/recovery/pending-before-recorded-IO-rejection','enterRecoveryMode',[0,1]);
 await updateRecordedIoFacts('scheduler/recovery/recorded-next-network-failure',{failures:[{method:'Client.getNetworkInfo',ordinal:2,message:'recorded recovery failure',code:'EIO'}]});
 await fireNext('scheduler/recovery/source-rejects-after-actual-wait');assert.equal(recovering.status,'Rejected');assert.equal(recovering.error.message,'recorded recovery failure');

 await schedulerReset('scheduler/discarded/source-promise/input');
 const sync=await sourceStart('scheduler/discarded/sync-fulfills-with-live-job-still-pending','syncIfNecessary');assert.equal(sync.status,'Fulfilled');
 await updateRecordedIoFacts('scheduler/discarded/recorded-background-count-failure',{failures:[{method:'Client.getBlockCount',ordinal:3,message:'recorded detached source failure',code:'EIO'}]});
 await fireNext('scheduler/discarded/actual-unhandled-rejection-is-preserved');assert.ok(unhandled.some(row=>row.message==='recorded detached source failure'));assert.equal(sync.status,'Fulfilled');

 await schedulerReset('scheduler/cancel/no-source-completion/input',{orchestration:{height:0,networkInfo:{networkactive:false,connections:0},blocks:[]}});
 const pending=await sourceStart('scheduler/cancel/pending-recovery','enterRecoveryMode',[0,1]);const timer=driverClock.next();driverClock.cancel(timer);
 await checkpointControl('scheduler/cancel/host-clears-callback-without-fabricating-source-return',{kind:'ClearTimer',id:timer});assert.equal(pending.status,'Pending');
}
async function shutdownChild(){
 await schedulerReset('scheduler/exit/child/input');
 const pending=await sourceStart('scheduler/exit/child/delay-stays-pending','delay',[1000]);assert.equal(pending.status,'Pending');
 let afterSourceExit=false;e.logs.length=0;
 e.context.process.exit=code=>{
  const terminal={kind:'ActualSourceProcessExit',code,afterSourceExit,logs:e.logs.map(row=>({level:row.level,values:row.values})),scheduler:schedulerState(),state:tag(state())};
  fs.writeFileSync(1,JSON.stringify(terminal)+'\n');process.exit(code);
 };
 await sourceStart('scheduler/exit/child/unconditional-shutdown','shutdown');
 afterSourceExit=true;fs.writeFileSync(1,'UNEXPECTED_RETURN_FROM_SOURCE_SHUTDOWN\n');
}
async function drivenMain(){
 if(flags.includes('--child-shutdown'))return shutdownChild();
 await firstSchedulerTen();const proof=records.map(row=>JSON.stringify(row)).join('\n')+'\n';
 if(!proofOnly){
  await expandScheduler();
  const child=require('node:child_process').spawnSync(process.execPath,[__filename,'--child-shutdown',...(flags.includes('--lock')?['--lock']:[])],{encoding:'utf8',maxBuffer:16*1024*1024});
  assert.equal(child.status,0,child.stderr);assert.equal(child.signal,null);const terminal=JSON.parse(child.stdout.trim());assert.equal(terminal.kind,'ActualSourceProcessExit');assert.equal(terminal.afterSourceExit,false);assert.equal(terminal.scheduler.jobs[0].status,'Pending');assert.equal(terminal.scheduler.jobs[1].status,'Entered');assert.equal(terminal.scheduler.clock.timers[0].state,'Scheduled');
  assert.deepEqual(terminal.logs.map(row=>row.values),[['Saving state to database...'],['Shutdown completed.']]);
  records.push({name:'scheduler/exit/actual-child-termination-with-pending-delay',child:terminal});
 }
 const payload=records.map(row=>JSON.stringify(row)).join('\n')+'\n';
 for(const[name,bytes]of proofOnly?[['main_driven_scheduler_proof10.jsonl',proof]]:[['main_driven_scheduler_proof10.jsonl',proof],['main_driven_scheduler_vectors.jsonl',payload]]){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),bytes);else fs.writeFileSync(file,bytes);}
 stopResolutionHook();console.log(JSON.stringify({observations:records.length,topLevelSourceCalls:driverTopCalls,fullSHA256:sha(payload),proofSHA256:sha(proof),sourcePromiseResolutionObservation:'V8 promiseHooks; no internal rejection handlers',pendingIsNotSourceReturn:true}));
}


const fileBase={empty:true,clientProfile:{chain:'LTC',test:false,height:1000},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,
 fileInputs:[{path:'/tl-data/ltc-main/a.db',bytes:'live a'},{path:'/tl-data/ltc-main/b.db',bytes:'live b'},{path:'/tl-data/ltc-main/note.txt',bytes:'keep note'},{path:'/tl-data/new-a',bytes:'replacement a'}],
 fileIOInputs:{failures:[],mtimes:[]},orchestration:{height:1000,networkInfo:{networkactive:true,connections:2},blocks:[]}};
const emptyBlock={hash:'h1000',previousblockhash:'h999',tx:[]};
async function fileReset(label,changes={}){await schedulerReset(label,{...structuredClone(fileBase),...changes});}
function fileAt(path){return virtualFSState.files.find(row=>row[0]===path)?.[1];}
function sourceRow(collection,id){return e.tables.get(collection)?.find(row=>row._id===id);}
function snapshotDbFiles(){return virtualFSState.files.filter(([p])=>p.startsWith('/tl-data/ltc-main-snapshots/')&&p.endsWith('.db'));}
function failure(method,ordinal,code){return{method,ordinal,code,message:'recorded '+method+' '+code+' '+ordinal};}
async function drainFifty(label,count,job){for(let at=0;at<count;at++){assert.equal(job.status,'Pending');const timer=driverClock.snapshot().timers.find(row=>row.id===driverClock.next());assert.equal(timer.delay,50);await fireNext(label+'/'+(at+1));}}

async function firstFileRetryTen(){
 await fileReset('file-retry/first10/copy/input',{fileIOInputs:{failures:[failure('copyFile',1,'EBUSY')],mtimes:[]}});
 const copy=await sourceStart('file-retry/first10/01-copy-pending','Persistence.safeCopy',['/tl-data/ltc-main/a.db','/tl-data/copied.db']);assert.equal(copy.status,'Pending');assert.equal(fileAt('/tl-data/copied.db'),undefined);
 await sourceStart('file-retry/first10/02-concurrent-pause','setPause');
 await sourceStart('file-retry/first10/03-concurrent-height','saveTrackHeight',[9]);
 await drainFifty('file-retry/first10/copy-resumes',1,copy);assert.equal(copy.status,'Fulfilled');assert.equal(fileAt('/tl-data/copied.db'),'live a');
 await sourceStart('file-retry/first10/04-height-survives-copy','loadTrackHeight');assert.equal(trackHeight(),9);
 await fileReset('file-retry/first10/delete-missing/input');
 const missing=await sourceStart('file-retry/first10/05-delete-enoent-no-timer','Persistence.safeDelete',['/tl-data/ltc-main/absent.db']);assert.equal(missing.status,'Fulfilled');assert.equal(driverClock.next(),undefined);
 await fileReset('file-retry/first10/block/input',{fileIOInputs:{failures:[failure('copyFile',2,'EPERM')],mtimes:[]}});
 const block=await sourceStart('file-retry/first10/06-real-block-snapshot-pending','processBlock',[emptyBlock,1000]);assert.equal(block.status,'Pending');assert.equal(snapshotDbFiles().length,1);assert.ok(sourceRow('persistence','block-1000'));assert.ok(sourceRow('consensus','block-1000'));assert.equal(sourceRow('consensus','MaxProcessedHeight'),undefined);
 await sourceStart('file-retry/first10/07-pause-during-checkpoint','setPause');
 await sourceStart('file-retry/first10/08-height-during-checkpoint','saveTrackHeight',[11]);
 await drainFifty('file-retry/first10/block-resumes',1,block);assert.equal(block.status,'Fulfilled');assert.equal(snapshotDbFiles().length,2);assert.equal(trackHeight(),11);assert.equal(sourceRow('consensus','MaxProcessedHeight').value,1000);
 await fileReset('file-retry/first10/copy-exhausted/input',{fileIOInputs:{failures:Array.from({length:5},(_,at)=>failure('copyFile',at+1,'EBUSY')),mtimes:[]}});
 const exhausted=await sourceStart('file-retry/first10/09-copy-exhausted','Persistence.safeCopy',['/tl-data/ltc-main/a.db','/tl-data/never.db']);await drainFifty('file-retry/first10/five-copy-waits-including-after-fifth-attempt',5,exhausted);assert.equal(exhausted.status,'Rejected');assert.equal(exhausted.error.message,'safeCopy failed: /tl-data/ltc-main/a.db → /tl-data/never.db');assert.equal(fileIOState.counts.copyFile,5);assert.equal(fileAt('/tl-data/never.db'),undefined);
 await fileReset('file-retry/first10/delete-exhausted/input',{fileIOInputs:{failures:Array.from({length:5},(_,at)=>failure('unlink',at+1,'EPERM')),mtimes:[]}});
 const deleted=await sourceStart('file-retry/first10/10-delete-exhausted','Persistence.safeDelete',['/tl-data/ltc-main/a.db']);await drainFifty('file-retry/first10/five-delete-waits-including-after-fifth-attempt',5,deleted);assert.equal(deleted.status,'Rejected');assert.equal(deleted.error.message,'safeDelete failed: /tl-data/ltc-main/a.db');assert.equal(fileAt('/tl-data/ltc-main/a.db'),'live a');assert.equal(driverTopCalls,10);
}
function restoreInputs(failures=[]){return{...structuredClone(fileBase),
 directoryInputs:['/tl-data/ltc-main-snapshots/one'],
 persistenceRows:[{_id:'block-1',height:1,hash:'canonical1'},{_id:'block-2',height:2,hash:'old2'}],
 fileInputs:[...structuredClone(fileBase.fileInputs),{path:'/tl-data/ltc-main-snapshots/one/a.db',bytes:'historical a'},{path:'/tl-data/ltc-main-snapshots/one/meta.json',bytes:JSON.stringify({blockHeight:1,consensusHash:'recorded historical hash',createdAt:'recorded time',files:['a.db']})}],
 fileIOInputs:{failures,mtimes:[]},
 orchestration:{height:3,networkInfo:{networkactive:true,connections:2},blocks:[{height:1,hash:'canonical1',block:{hash:'canonical1',tx:[]}},{height:2,hash:'canonical2',block:{hash:'canonical2',tx:[]}}]}};}
const reorgBlock={hash:'h3',previousblockhash:'canonical2',tx:[]};
async function expandFileRetries(){
 await fileReset('file-retry/block-copy-exhausted/input',{fileIOInputs:{failures:Array.from({length:5},(_,at)=>failure('copyFile',at+1,'EBUSY')),mtimes:[]}});
 const copyExhausted=await sourceStart('file-retry/block-copy-exhausted/block-pending','processBlock',[emptyBlock,1000]);await drainFifty('file-retry/block-copy-exhausted/all-five-real-waits',5,copyExhausted);assert.equal(copyExhausted.status,'Rejected');assert.ok(copyExhausted.error.message.startsWith('safeCopy failed: /tl-data/ltc-main/a.db → /tl-data/ltc-main-snapshots/1000-'));assert.equal(snapshotDbFiles().length,0);assert.ok(sourceRow('persistence','block-1000'));assert.ok(sourceRow('consensus','block-1000'));assert.equal(sourceRow('consensus','MaxProcessedHeight'),undefined);
 await fileReset('file-retry/nonbusy/input',{fileIOInputs:{failures:[failure('copyFile',1,'EIO')],mtimes:[]}});
 const failed=await sourceStart('file-retry/nonbusy/immediate-original-error','Persistence.safeCopy',['/tl-data/ltc-main/a.db','/tl-data/failed.db']);assert.equal(failed.status,'Rejected');assert.equal(failed.error.message,'recorded copyFile EIO 1');assert.equal(driverClock.next(),undefined);
 await fileReset('file-retry/enoent-after-permission/input',{fileIOInputs:{failures:[failure('unlink',1,'EPERM'),failure('unlink',2,'ENOENT')],mtimes:[]}});
 const missing=await sourceStart('file-retry/enoent-after-permission/delete','Persistence.safeDelete',['/tl-data/ltc-main/a.db']);await drainFifty('file-retry/enoent-after-permission/resume',1,missing);assert.equal(missing.status,'Fulfilled');assert.equal(fileAt('/tl-data/ltc-main/a.db'),'live a');
 await fileReset('file-retry/snapshot-prefix/input',{fileIOInputs:{failures:[failure('copyFile',2,'EBUSY')],mtimes:[]}});
 const block=await sourceStart('file-retry/snapshot-prefix/before-concurrent-file-copy','processBlock',[emptyBlock,1000]);assert.equal(block.status,'Pending');assert.deepEqual(snapshotDbFiles().map(([,b])=>b),['live a']);
 const mutation=await sourceStart('file-retry/snapshot-prefix/concurrent-real-file-copy','Persistence.safeCopy',['/tl-data/new-a','/tl-data/ltc-main/a.db']);assert.equal(mutation.status,'Fulfilled');assert.equal(fileAt('/tl-data/ltc-main/a.db'),'replacement a');
 await drainFifty('file-retry/snapshot-prefix/resume-does-not-recopy-first-file',1,block);assert.equal(block.status,'Fulfilled');assert.deepEqual(snapshotDbFiles().map(([,b])=>b),['live a','live b']);
 await fileReset('file-retry/concurrent-delete/input',{fileIOInputs:{failures:[failure('copyFile',2,'EBUSY')],mtimes:[]}});
 const deletedSource=await sourceStart('file-retry/concurrent-delete/block-pending','processBlock',[emptyBlock,1000]);assert.equal(deletedSource.status,'Pending');
 const concurrentDelete=await sourceStart('file-retry/concurrent-delete/actual-source-unlink','Persistence.safeDelete',['/tl-data/ltc-main/b.db']);assert.equal(concurrentDelete.status,'Fulfilled');
 await drainFifty('file-retry/concurrent-delete/retry-observes-real-missing-file',1,deletedSource);assert.equal(deletedSource.status,'Rejected');assert.equal(deletedSource.error.message,'ENOENT: /tl-data/ltc-main/b.db');assert.deepEqual(snapshotDbFiles().map(([,b])=>b),['live a']);assert.equal(sourceRow('consensus','MaxProcessedHeight'),undefined);

 await fileReset('file-retry/main-restore/input',restoreInputs([failure('unlink',1,'EBUSY')]));
 const restored=await sourceStart('file-retry/main-restore/delete-pending-after-real-backup','processBlock',[reorgBlock,3]);assert.equal(restored.status,'Pending');assert.ok(virtualFSState.files.some(([p,b])=>p.includes('/backup-')&&p.endsWith('/a.db')&&b==='live a'));assert.equal(fileAt('/tl-data/ltc-main/a.db'),'live a');
 await sourceStart('file-retry/main-restore/concurrent-height','saveTrackHeight',[9]);await sourceStart('file-retry/main-restore/concurrent-pause','setPause');
 await drainFifty('file-retry/main-restore/resume-real-delete-and-restore',1,restored);assert.equal(restored.status,'Fulfilled');assert.equal(fileAt('/tl-data/ltc-main/a.db'),'historical a');assert.equal(fileAt('/tl-data/ltc-main/b.db'),undefined);assert.equal(fileAt('/tl-data/ltc-main/note.txt'),'keep note');assert.equal(trackHeight(),1);assert.equal(sourceRow('persistence','block-3'),undefined);assert.deepEqual(restored.value,tag({reOrg:true,restore:1}));
 await fileReset('file-retry/restore-exhausted/input',restoreInputs(Array.from({length:5},(_,at)=>failure('unlink',at+1,'EPERM'))));
 const exhausted=await sourceStart('file-retry/restore-exhausted/block-is-pending','processBlock',[reorgBlock,3]);await drainFifty('file-retry/restore-exhausted/five-delete-waits',5,exhausted);assert.equal(exhausted.status,'Rejected');assert.equal(fileAt('/tl-data/ltc-main/a.db'),'live a');assert.ok(virtualFSState.files.some(([p,b])=>p.includes('/backup-')&&p.endsWith('/b.db')&&b==='live b'));
}
async function shutdownFileChild(){
 await fileReset('file-retry/exit/input',{fileIOInputs:{failures:[failure('copyFile',2,'EBUSY')],mtimes:[]}});
 const pending=await sourceStart('file-retry/exit/block-pending-after-partial-copy','processBlock',[emptyBlock,1000]);assert.equal(pending.status,'Pending');assert.equal(snapshotDbFiles().length,1);
 e.logs.length=0;let afterSourceExit=false;
 e.context.process.exit=code=>{fs.writeFileSync(1,JSON.stringify({kind:'ActualSourceProcessExit',code,afterSourceExit,logs:e.logs.map(r=>({level:r.level,values:r.values})),scheduler:schedulerState(),state:tag(state())})+'\n');process.exit(code);};
 await sourceStart('file-retry/exit/source-shutdown','shutdown');afterSourceExit=true;throw Error('unreachable after actual source exit');
}
async function fileRetryMain(){
 if(flags.includes('--child-file-shutdown'))return shutdownFileChild();
 await firstFileRetryTen();const proof=records.map(r=>JSON.stringify(r)).join('\n')+'\n';
 if(!proofOnly){await expandFileRetries();const child=require('node:child_process').spawnSync(process.execPath,[__filename,'--child-file-shutdown',...(flags.includes('--lock')?['--lock']:[])],{encoding:'utf8',maxBuffer:32*1024*1024});assert.equal(child.status,0,child.stderr);const terminal=JSON.parse(child.stdout.trim());assert.equal(terminal.afterSourceExit,false);assert.equal(terminal.scheduler.jobs[0].status,'Pending');assert.equal(terminal.scheduler.clock.timers[0].delay,50);assert.deepEqual(terminal.logs.map(r=>r.values),[['Saving state to database...'],['Shutdown completed.']]);records.push({name:'file-retry/exit/actual-child-pending-block',child:terminal});}
 const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';for(const[name,bytes]of proofOnly?[['main_file_retry_proof10.jsonl',proof]]:[['main_file_retry_proof10.jsonl',proof],['main_file_retry_vectors.jsonl',payload]]){const p=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(p,'utf8'),bytes);else fs.writeFileSync(p,bytes);}
 stopResolutionHook();console.log(JSON.stringify({observations:records.length,parentCalls:driverTopCalls,fullSHA256:sha(payload),proofSHA256:sha(proof),node:process.version,v8:process.versions.v8}));
}
fileRetryMain().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
