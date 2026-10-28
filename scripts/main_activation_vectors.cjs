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
// Audit snapshots must not create empty storage collections. The original
// driver built views with its creating table() helper after copying tables,
// mutating the NEXT source prestate without a business operation or DB effect.
replace("const parseRows=name=>table(name).map(row=>({id:row._id,value:JSON.parse(row.value??row.data)}));", "const parseRows=name=>(tables.get(name)||[]).map(row=>{const value=row.value??row.data;return{id:row._id,value:typeof value==='string'?JSON.parse(value):value};});");
replace("views:{books:table('orderBooks').filter", "views:{books:(tables.get('orderBooks')||[]).filter");
const mod=new Module(reference,module);mod.filename=reference;mod.paths=Module._nodeModulePaths(path.dirname(reference));mod._compile(code,reference);
const inventory={sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',pins,referenceDriverSha256:sha(fs.readFileSync(reference)),classes:{},methods:{},boundary:{fullMainImport:'declared unavailable ./reOrg.js; no imported module success supplied',callable:'byte-exact full Main and TxIndex classes; real constructors/prototypes; module startup IIFEs not executed',activation:'unchanged complete module, real persisted registry; init/RPC not supplied',storage:'shared explicit source fixture DB adapter; anchored /^tx/ RegExp is genuine JS test; production NeDB equivalence not claimed',rpc:'all reached ClientWrapper/TxUtils calls unavailable; no height/client success answer',observation:'nonmutating audit table views; original return plus finite drained turns; no generic event-loop equivalence'}};
function tag(value,seen=new Set()){
 if(value===undefined)return['undefined'];if(value===null)return['null'];if(typeof value==='string'||typeof value==='boolean')return[typeof value,value];
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return['number',Number.isNaN(value)?'NaN':value===Infinity?'Infinity':value===-Infinity?'-Infinity':b.toString('hex')];}
 if(typeof value==='function')return['function',value.name];if(Object.prototype.toString.call(value)==='[object RegExp]')return['regexp',value.source,value.flags];
 if(Object.prototype.toString.call(value)==='[object Promise]')return['promise'];if(value?.constructor?.name==='BigNumber')return['bignumber',value.valueOf()];
 if(seen.has(value))return['cycle'];seen.add(value);let result;
 if(Array.isArray(value))result=['array',value.map(v=>tag(v,seen))];else if(Object.prototype.toString.call(value)==='[object Map]')result=['map',[...value].map(([k,v])=>[tag(k,seen),tag(v,seen)])];else if(Object.prototype.toString.call(value)==='[object Set]')result=['set',[...value].map(v=>tag(v,seen))];else result=['object',Object.entries(value).map(([k,v])=>[k,tag(v,seen)])];seen.delete(value);return result;
}
const records=[];let e,m,Main,TxIndex,marginInstances,contextKeys,startup,unhandled=[];
process.on('unhandledRejection',error=>{if(e){const value={name:error?.name||'Error',message:error?.message||String(error)};unhandled.push(value);e.effects.push({kind:'unhandled-rejection',thrown:value});}});
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function classBytes(name,sourceName){const full=source[sourceName],start=full.indexOf('class '+name+' {'),end=full.lastIndexOf('module.exports = '+name);assert.ok(start>=0&&end>start);const bytes=full.slice(start,end).trimEnd();inventory.classes[name]={sha256:sha(bytes),bytes:Buffer.byteLength(bytes),fullClass:true};return bytes;}
function compileClass(name,sourceName,scope){const fn=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes(name,sourceName)+'\nreturn '+name+';})',e.context,{filename:'unchanged-class/'+sourceName+'.js',timeout:3000});return fn(...Object.values(scope));}
function unavailable(target){return function(...args){const error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});e.effects.push({kind:'unavailable',target,args:e.copy(args),thrown:{name:error.name,message:error.message}});throw error;};}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);inventory.methods[label]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
function state(){const tableNamesBefore=[...e.tables.keys()],sourceState=e.state();assert.deepEqual([...e.tables.keys()],tableNamesBefore,'audit snapshots must not create storage collections');const A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');return{source:sourceState,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,instance:TxIndex.instance},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,vesting:e.load('vesting').instance?{...e.load('vesting').instance}:undefined,vestingStatic:{adminAddress:e.load('vesting').adminAddress,sponsorAddress:e.load('vesting').sponsorAddress,expiryInterval:e.load('vesting').expiryInterval,hedgeLeverage:e.load('vesting').hedgeLeverage},consensusVector:e.load('consensus').consensusVector,insuranceInstances:e.load('insurance').instances,unhandledRejections:unhandled,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache},startup};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment({clientProfile:options.clientProfile});marginInstances=[];contextKeys=new Set();startup=[];unhandled=[];if(options.empty)for(const table of e.tables.values())table.splice(0,Infinity);if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
 // Observe every real receiver that reaches the original loader, including
 // the fresh constructor used by _pruneInstaLiqOrdersFromFreshBook. The
 // original method, arguments, promise, return and effects remain unchanged.
 const originalBookLoad=e.Orderbook.prototype.loadOrderBook;e.Orderbook.prototype.loadOrderBook=function(...args){e.instanceId(this);return Reflect.apply(originalBookLoad,this,args);};
 if(options.balancedSupply){const properties=JSON.parse(e.table('propertyList')[0].value);for(const[id,value]of properties)value.totalInCirculation=id===7?2000:0;e.table('propertyList')[0].value=JSON.stringify(properties);}
 const MM=e.load('marginMap'),originalLoad=MM.loadMarginMap;MM.loadMarginMap=function(...args){const result=Reflect.apply(originalLoad,this,args);result.then(v=>{if(v?.margins&&!marginInstances.includes(v))marginInstances.push(v);},()=>{});return result;};
 const clearing=e.load('clearing'),originalInit=clearing.initPositionCache,originalFlush=clearing.flushPositionCache;clearing.initPositionCache=function(...args){const key=Reflect.apply(originalInit,this,args);contextKeys.add(key);return key;};clearing.flushPositionCache=function(...args){const value=Reflect.apply(originalFlush,this,args);contextKeys.delete(args[0]);return value;};
 const activation=e.load('activation').getInstance();let metadataInit;if(options.init){try{metadataInit={returned:await activation.init(options.init)};}catch(error){metadataInit={thrown:{name:error.name,message:error.message}};}}
 const importedRows=options.rows??rows;e.table('txIndex').push(...structuredClone(importedRows),{_id:'MaxHeight',value:options.maxHeight??10});if(options.processed)e.table('consensus').push(...importedRows.map(row=>({_id:row.value.txId,processed:true,params:{valid:true,block:Number(row._id.split('-')[1])}})));
 const ClientWrapper={getInstance:unavailable('ClientWrapper.getInstance')};TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:new Proxy({},{get(_t,k){return unavailable('TxUtils.'+String(k));}}),transparentIndex:[],process:e.context.process});
 const client={getBlockCount:unavailable('client.getBlockCount'),getNetworkInfo:unavailable('client.getNetworkInfo')};
 Main=compileClass('Main','main',{client,db:e.storage,TxIndex,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:e.load('vesting'),Persistence:new Proxy({},{get(_t,k){return unavailable('Persistence.'+String(k));}}),console:e.context.console,COIN:100000000,pause:false,setTimeout,process:e.context.process});
 for(const[name,receiver]of [['Main',Main.prototype],['TxIndex',TxIndex]])for(const method of Object.getOwnPropertyNames(receiver)){const fn=receiver[method];if(typeof fn==='function')inventory.methods[name+'.'+method]={sha256:sha(fn.toString()),async:Object.prototype.toString.call(fn)==='[object AsyncFunction]'};}
 for(const [receiver,method,label]of [[e.load('tally'),'flushQueuedFeeAccruals','Tally.flushQueuedFeeAccruals'],[e.logic,'settleLiquidityRewards','Logic.settleLiquidityRewards'],[e.Orderbook,'processQueuedOnChainOrdersForBlock','Orderbook.processQueuedOnChainOrdersForBlock'],[e.Orderbook,'processQueuedChannelTradesForBlock','Orderbook.processQueuedChannelTradesForBlock'],[e.load('channels'),'processWithdrawals','Channels.processWithdrawals'],[e.load('clearing'),'clearingFunction','Clearing.clearingFunction']])observe(receiver,method,label);
 if(options.observeTypes)observe(e.types,'decodePayload','Types.decodePayload');
 m=new Main();m.txIndex.then(value=>startup.push({returned:value}),error=>startup.push({thrown:{name:error.name,message:error.message}}));await flush();emit(name,{reset:{activationRows:e.copy(e.table('activations')),indexRows:structuredClone(importedRows),heightRow:options.maxHeight??10,...(Object.keys(options).length?{options}: {})},profile:'callable-class'},{returned:{samePrototype:true,sameSingleton:true},startup,metadataInit});}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure)e.setFailure(failure);let transport='Immediate',outcome;try{const value=Reflect.apply(m[method],m,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:'Main.'+method,args,failure},{transport,outcome});}
function importProbe(){const effects=[],module={exports:{}};let result;try{vm.runInNewContext(source.main,{module,exports:module.exports,require(name){effects.push({require:name});if(name==='events'||name==='util')return require(name);throw Object.assign(Error('Unavailable full-import dependency: '+name),{name:'SourceBoundaryUnavailable'});},console:{log(){},warn(){},error(){}},process:{exit:unavailable('process.exit')}},{filename:'unchanged-full-main-import.js',timeout:1000});result={returned:module.exports};}catch(error){result={thrown:{name:error.name,message:error.message}};}records.push({name:'proof/01-full-main-import-declared-boundary',command:tag({profile:'full-import'}),result:tag(result),state:tag({exported:module.exports}),effects:tag(effects),summary:result});}
function phases(){return e.effects.filter(x=>x.kind==='source-function').map(x=>x.target).filter(x=>x!=='Types.decodePayload');}
const postPhases=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Clearing.clearingFunction'];

const BN=numeric.loadBigNumber();
function bindRpc(){
 const source=fs.readFileSync(path.join(repo,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(repo,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+source+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve({})}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 // This is the pre-existing explicit raw-RPC boundary profile. Original raw
 // transaction documents drive unchanged pure classifier/pubkey methods.
 Object.assign(e.adapters['./txUtils'],{getAddressTypeUniversal:own.exports.getAddressTypeUniversal,extractPubkeyByType:own.exports.extractPubkeyByType,async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'TxUtils.getRawTransaction',args,returned:structuredClone(value)});return value;}});
}



const admin='MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB';inventory.inputLabels={};
function activationRow(label,types,actor=admin,hash='main-code'){
 const id=sha('main-activation:'+label);inventory.inputLabels[id]={label,kind:0};
 const row=input(id,'0',types+','+hash);row.value.sender.senderAddress=actor;return row;
}
const rows0={one:activationRow('one','1'),batch:activationRow('batch','2;3'),unauthorized:activationRow('unauthorized','4','claimed-other'),outbound:activationRow('outbound','36'),boot:activationRow('boot','0'),repeat:activationRow('repeat','0',admin,'repeat-hash'),last:activationRow('last','35')};
async function sourceProperty(name,args){
 e.effects.length=0;let outcome;try{outcome={returned:await e.load('property').getInstance().createToken(...args)}}catch(error){outcome={thrown:{name:error.name,message:error.message}};}
 await flush();emit(name,{target:'Property.createToken',args,sourceSupport:true},{transport:'Pending',outcome});
}
const originalMainCall=call;inventory.findings=[];
call=async function(name,...args){const start=e.logs.length;await originalMainCall(name,...args);inventory.findings.push({name,phases:phases(),diagnostics:e.logs.slice(start).filter(row=>row.level!=='log')});};
async function main(){
 inventory.boundary.activation='complete actual Activation/Vesting/Consensus modules; actual metadata init and registry writes; no supplied activation/hash/supply/validity results';
 inventory.boundary.rpc='explicit chain/test/clock metadata; virtual read-only archived file inputs; Main/TxIndex constructor startup remains separately unavailable; unsupported method calls are isolated audit experiments';
 await reset('first10/indexed-registry-prestate',{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:Object.values(rows0)});
 await call('first10/01-indexed-real-registry-activation','processTxSet',[[rows0.one],10]);
 await call('first10/02-live-activation-batch','processTx',[[live(rows0.batch)],10]);
 await call('first10/03-indexed-nonadmin','processTxSet',[[rows0.unauthorized],10]);
 await call('first10/04-indexed-out-of-bounds','processTxSet',[[rows0.outbound],10]);
 await call('first10/05-indexed-real-bootstrap','processTxSet',[[rows0.boot],10]);
 await call('first10/06-indexed-bootstrap-repeat','processTxSet',[[rows0.repeat],10]);
 await call('first10/07-live-final-registry-id','processTx',[[live(rows0.last)],10]);
 await call('first10/08-live-processed-retry','processTx',[[live(rows0.one)],10]);
 const liveBoot=activationRow('live-boot','0');
 await reset('first10/live-native-prestate',{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:[liveBoot]});
 await call('first10/09-live-bootstrap-real-postphases','blockHandlerMid',[[live(liveBoot)],10]);
 await reset('first10/empty-live-prestate',{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:[]});
 await call('first10/10-empty-live-withdrawal-phase','blockHandlerMid',[[],10]);
 if(!proofOnly){
  const specs=[['TLITE',500000,'Fixed'],['TLITEVEST',250000,'Vesting'],['TLITEI',1500000,'Vesting'],['TLITEIVEST',1500001,'Native']];
  for(let mask=0;mask<16;mask++){
   const label=mask.toString(2).padStart(4,'0'),row=activationRow('mixed-'+label,'0');
   await reset('mixed/'+label+'/prestate',{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:[row]});
   for(let i=0;i<4;i++)if(mask&(1<<i))await sourceProperty('mixed/'+label+'/actual-seed-'+specs[i][0],[...specs[i],0]);
   await call('mixed/'+label+'/indexed-bootstrap','processTxSet',[[row],10]);
   assert.equal(e.load('activation').instance.txRegistry[0].active,true);
   assert.equal(e.table('tallyMap').find(row=>row._id==='$TLinit').initialized,true);
   assert.equal(e.table('consensus').find(entry=>entry._id===row.value.txId).params.valid,true);
  }
  for(const[scenario,failure]of[
   ['delta-second',{collection:'tallyMapDelta',op:'insert',skip:1,message:'main-native-delta-second'}],
   ['flag-write',{collection:'tallyMap',op:'update',skip:5,message:'main-native-flag'}],
   ['insurance-snapshot',{collection:'insurance',op:'update',message:'main-native-insurance'}],
   ['registry-save',{collection:'activations',op:'update',skip:1,message:'main-native-registry-save'}],
  ]){
   const first=activationRow('failure-'+scenario+'-first','0'),retry=activationRow('failure-'+scenario+'-new-tx','0',admin,'retry-hash');
   await reset('failure/'+scenario+'/prestate',{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:[first,retry]});
   await call('failure/'+scenario+'/actual-prefix-and-journal','processTxSet',[[first],10],failure);
   await call('failure/'+scenario+'/same-tx-is-processed','processTxSet',[[first],10]);
   assert.equal(records.at(-1).summary.outcome.returned,1);
   await call('failure/'+scenario+'/new-tx-real-retry','processTxSet',[[retry],10]);
  }
  for(const chain of ['BTC','DOGE']){
   const actor=chain==='BTC'?'bc1qktknrnx2jcchjht9anz0uy8ae02xryxq2vxeem':'DLSfu9qvEggkeXAgCAwBBw5BVLvMCtkewz',row=activationRow('chain-'+chain,'0',actor);
   await reset('chain/'+chain+'/prestate',{empty:true,clientProfile:{chain,test:false,height:10},init:chain,observeTypes:true,rows:[row]});
   await call('chain/'+chain+'/indexed-original-bootstrap','processTxSet',[[row],10]);
  }
  const activeLive=activationRow('live-native-and-contracts','0;18');
  await reset('live/active-native-clearing-prestate',{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',observeTypes:true,rows:[activeLive]});
  await call('live/native-and-contract-activation-real-postphases','blockHandlerMid',[[live(activeLive)],10]);
  for(const chain of ['BTC','DOGE']){
   const actor=chain==='BTC'?'bc1qktknrnx2jcchjht9anz0uy8ae02xryxq2vxeem':'DLSfu9qvEggkeXAgCAwBBw5BVLvMCtkewz',row=activationRow('live-chain-'+chain,'0;18',actor);
   await reset('live-chain/'+chain+'/prestate',{empty:true,clientProfile:{chain,test:false,height:10},init:chain,observeTypes:true,rows:[row]});
   await call('live-chain/'+chain+'/original-native-postphases','blockHandlerMid',[[live(row)],10]);
  }
  const unavailable=activationRow('unavailable-client','0',undefined);
  unavailable.value.sender.senderAddress=undefined;
  await reset('unavailable/client-isolated-prestate',{empty:true,observeTypes:true,rows:[unavailable]});
  await call('unavailable/missing-client-is-an-adapter-capability','processTxSet',[[unavailable],10]);
 }
 const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';
 const calls=records.filter(r=>r.command?.[0]==='object'&&r.command[1].some(([k,v])=>k==='target'&&v[1].startsWith('Main.')));
 inventory.observations=records.length;inventory.calls=calls.length;inventory.fixtureSha256=sha(payload);inventory.domains=Object.fromEntries(['source-call','unavailable-capability','constructor-and-input','explicit-input'].map(d=>[d,records.filter(r=>r.domain===d).length]));
 const proof=records.slice(0,13).map(r=>JSON.stringify(r)).join('\n')+'\n';
 const outputs=proofOnly?[['main_activation_proof10.jsonl',proof]]:[['main_activation_proof10.jsonl',proof],['main_activation_vectors.jsonl',payload],['main_activation_inventory.json',JSON.stringify(inventory,null,2)+'\n']];
 fs.mkdirSync(path.join(stage,'fixtures'),{recursive:true});for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}
 console.log(JSON.stringify({observations:records.length,actualMainCalls:calls.length,domains:inventory.domains,fixtureSha256:sha(payload),calls:calls.map(r=>({name:r.name,domain:r.domain,result:r.summary})),classes:inventory.classes},null,2));
}
main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
