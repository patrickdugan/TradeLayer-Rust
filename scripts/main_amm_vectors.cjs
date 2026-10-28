#!/usr/bin/env node
'use strict';
// Audit-only callable extraction. Original Main/TxIndex class bytes and receivers
// are retained; import/startup/RPC boundaries are observations, never successes.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),Module=require('node:module'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||(fs.existsSync(path.join(stage,'scripts/legacy_numeric_oracle.cjs'))?stage:path.join(stage,'../../outputs/TradeLayer-Rust')));
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
replace('return {types,logic,Orderbook,effects','return {ammInstances,ammId,instanceId,adapters,context,storage,tables,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
// Audit views may read absent tables but must not create collection inputs.
replace("const parseRows=name=>table(name).map", "const parseRows=name=>(tables.get(name)||[]).map");
replace("views:{books:table('orderBooks').filter", "views:{books:(tables.get('orderBooks')||[]).filter");
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
const records=[];let e,m,Main,TxIndex,marginInstances,contextKeys,startup,unhandled=[];
process.on('unhandledRejection',error=>{if(e){const thrown={name:error?.name||'Error',message:error?.message||String(error)};unhandled.push(thrown);e.effects.push({kind:'unhandled-rejection',thrown});}});
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function classBytes(name,sourceName){const full=source[sourceName],start=full.indexOf('class '+name+' {'),end=full.lastIndexOf('module.exports = '+name);assert.ok(start>=0&&end>start);const bytes=full.slice(start,end).trimEnd();inventory.classes[name]={sha256:sha(bytes),bytes:Buffer.byteLength(bytes),fullClass:true};return bytes;}
function compileClass(name,sourceName,scope){const fn=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes(name,sourceName)+'\nreturn '+name+';})',e.context,{filename:'unchanged-class/'+sourceName+'.js',timeout:3000});return fn(...Object.values(scope));}
function unavailable(target){return function(...args){const error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});e.effects.push({kind:'unavailable',target,args:e.copy(args),thrown:{name:error.name,message:error.message}});throw error;};}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);inventory.methods[label]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
function state(){const tableNamesBefore=[...e.tables.keys()],sourceState=e.state();assert.deepEqual([...e.tables.keys()],tableNamesBefore,'audit snapshots must not create storage collections');const A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');return{source:sourceState,ammInstances:e.ammInstances.map((value,id)=>({id:id+1,...value})),propertyRegistryBound:!!e.context.PropertyRegistry,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,instance:TxIndex.instance},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache,tallyMapBound:!!e.context.tallyMap,spotRpcDocumentsBound:!!e.context.spotRpcDocumentsBound},unhandledRejections:unhandled,startup};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment({});marginInstances=[];contextKeys=new Set();startup=[];unhandled=[];seedAmmInputs(options);if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
 // Observe every real receiver that reaches the original loader, including
 // the fresh constructor used by _pruneInstaLiqOrdersFromFreshBook. The
 // original method, arguments, promise, return and effects remain unchanged.
 const originalBookLoad=e.Orderbook.prototype.loadOrderBook;e.Orderbook.prototype.loadOrderBook=function(...args){e.instanceId(this);return Reflect.apply(originalBookLoad,this,args);};
 if(options.balancedSupply){const properties=JSON.parse(e.table('propertyList')[0].value);for(const[id,value]of properties)value.totalInCirculation=id===7?2000:0;e.table('propertyList')[0].value=JSON.stringify(properties);}
 const MM=e.load('marginMap'),originalLoad=MM.loadMarginMap;MM.loadMarginMap=function(...args){const result=Reflect.apply(originalLoad,this,args);result.then(v=>{if(v?.margins&&!marginInstances.includes(v))marginInstances.push(v);},()=>{});return result;};
 const clearing=e.load('clearing'),originalInit=clearing.initPositionCache,originalFlush=clearing.flushPositionCache;clearing.initPositionCache=function(...args){const key=Reflect.apply(originalInit,this,args);contextKeys.add(key);return key;};clearing.flushPositionCache=function(...args){const value=Reflect.apply(originalFlush,this,args);contextKeys.delete(args[0]);return value;};
 const activation=e.load('activation').getInstance();const registry=activation.initializeTxRegistry();for(const type of (options.activeTypes||[18])){registry[type].active=!options.inactive;registry[type].activationBlock=options.activationBlock??1;}e.table('activations').push({_id:'activationsList',value:JSON.stringify(registry)});const importedRows=options.rows??rows;e.table('txIndex').push(...structuredClone(importedRows),{_id:'MaxHeight',value:options.maxHeight??10});if(options.processed)e.table('consensus').push(...importedRows.map(row=>({_id:row.value.txId,processed:true,params:{valid:true,block:Number(row._id.split('-')[1])}})));
 const ClientWrapper={getInstance:unavailable('ClientWrapper.getInstance')};TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:new Proxy({},{get(_t,k){return unavailable('TxUtils.'+String(k));}}),transparentIndex:[],process:e.context.process});
 const client={getBlockCount:unavailable('client.getBlockCount'),getNetworkInfo:unavailable('client.getNetworkInfo')};
 Main=compileClass('Main','main',{client,db:e.storage,TxIndex,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:new Proxy({},{get(_t,k){return unavailable('Vesting.'+String(k));}}),Persistence:new Proxy({},{get(_t,k){return unavailable('Persistence.'+String(k));}}),console:e.context.console,COIN:100000000,pause:false,setTimeout,process:e.context.process});
 for(const[name,receiver]of [['Main',Main.prototype],['TxIndex',TxIndex]])for(const method of Object.getOwnPropertyNames(receiver)){const fn=receiver[method];if(typeof fn==='function')inventory.methods[name+'.'+method]={sha256:sha(fn.toString()),async:Object.prototype.toString.call(fn)==='[object AsyncFunction]'};}
 for(const [receiver,method,label]of [[e.load('tally'),'flushQueuedFeeAccruals','Tally.flushQueuedFeeAccruals'],[e.logic,'settleLiquidityRewards','Logic.settleLiquidityRewards'],[e.Orderbook,'processQueuedOnChainOrdersForBlock','Orderbook.processQueuedOnChainOrdersForBlock'],[e.Orderbook,'processQueuedChannelTradesForBlock','Orderbook.processQueuedChannelTradesForBlock'],[e.load('channels'),'processWithdrawals','Channels.processWithdrawals'],[e.load('clearing'),'clearingFunction','Clearing.clearingFunction']])observe(receiver,method,label);
 if(options.observeTypes)observe(e.types,'decodePayload','Types.decodePayload');
 for(const[owner,method,label]of [[e.logic,'AMMPool','Logic.AMMPool'],[e.load('property'),'getAMM','Property.getAMM'],[e.load('contractRegistry'),'getAMM','Contract.getAMM'],[e.load('contractRegistry'),'getContractInfo','Contract.getContractInfo'],[e.load('tally'),'updateBalance','Tally.updateBalance']])observe(owner,method,label);
 const AMM=e.load('amm');for(const method of ['addCapital','redeemCapital','quoteContractOrders']){const original=AMM.prototype[method];inventory.methods['AMM.'+method]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};AMM.prototype[method]=function(...args){e.effects.push({kind:'source-function',target:'AMM.'+method,instance:e.ammId(this),args:e.copy(args)});return Reflect.apply(original,this,args);};}
 m=new Main();m.txIndex.then(value=>startup.push({returned:value}),error=>startup.push({thrown:{name:error.name,message:error.message}}));await flush();emit(name,{reset:{activationRows:e.copy(e.table('activations')),indexRows:structuredClone(importedRows),heightRow:options.maxHeight??10,...(Object.keys(options).length?{options}: {})},profile:'callable-class'},{returned:{samePrototype:true,sameSingleton:true},startup});}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure)e.setFailure(failure);let transport='Immediate',outcome;try{const value=Reflect.apply(m[method],m,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:'Main.'+method,args,failure},{transport,outcome});}
function importProbe(){const effects=[],module={exports:{}};let result;try{vm.runInNewContext(source.main,{module,exports:module.exports,require(name){effects.push({require:name});if(name==='events'||name==='util')return require(name);throw Object.assign(Error('Unavailable full-import dependency: '+name),{name:'SourceBoundaryUnavailable'});},console:{log(){},warn(){},error(){}},process:{exit:unavailable('process.exit')}},{filename:'unchanged-full-main-import.js',timeout:1000});result={returned:module.exports};}catch(error){result={thrown:{name:error.name,message:error.message}};}records.push({name:'proof/01-full-main-import-declared-boundary',command:tag({profile:'full-import'}),result:tag(result),state:tag({exported:module.exports}),effects:tag(effects),summary:result});}
function phases(){return e.effects.filter(x=>x.kind==='source-function').map(x=>x.target).filter(x=>x!=='Types.decodePayload');}
const postPhases=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Clearing.clearingFunction'];

const ammA='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',ammB='1BitcoinEaterAddressDontSendf59kuE';
inventory.inputLabels={};
function ammRow(label,body,sender=ammA){const id=sha('main-amm:'+label),row=input(id,'a',body);inventory.inputLabels[id]={label,type:10};row.value.sender.senderAddress=sender;return row;}
const ammRows={
 property:ammRow('property-unbound','0,0,1,2,7,3,,,0,'),
 add:ammRow('contract-add','0,1,1,2,7,3,,,0,'),
 redeem:ammRow('contract-redeem','1,1,1,1,7,3,,,0,'),
 liveAdd:ammRow('contract-live-add','0,1,1,3,7,3,,,0,'),
 zero:ammRow('zero-first','0,1,1,0,7,3,,,0,'),
 liveRedeem:ammRow('contract-live-redeem','1,1,1,1,7,3,,,0,'),
 failure:ammRow('contract-metadata-write-failure','0,1,1,2,7,3,,,0,'),
};
function seedAmmInputs(options){
 const rows=JSON.parse(e.table('tallyMap')[0].data);for(const[,properties]of rows)properties[1]=structuredClone(properties[7]);e.table('tallyMap')[0].data=JSON.stringify(rows);
 const properties=JSON.parse(e.table('propertyList')[0].value);for(const[id,value]of properties)value.totalInCirculation=[1,7].includes(id)?2000:0;e.table('propertyList')[0].value=JSON.stringify(properties);
 e.table('contractList')[0].data.ammPool=structuredClone(options.contractPool??{position:0,maxPosition:100,maxQuoteSize:10,contractType:1,lpAddresses:{},ammOrders:[]});
 if(options.propertyRegistry)e.context.PropertyRegistry=e.load('property');
 if(options.propertyPool){const AMM=e.load('amm');e.load('property').getInstance().ammIndex.set('1-7',new AMM(0,100,10,0));}
}
function latest(){return records.at(-1).summary;}
function observedPhases(){return phases().filter(name=>postPhases.includes(name)||name==='Channels.processWithdrawals');}
async function main(){
 inventory.boundary.activation='complete unchanged original Activation, actual persisted 36-entry registry with raw10 active at block1; no validity result supplied';
 inventory.boundary.observation='nonmutating audit table views; ordinary console undefined, genuine RegExp, every real Orderbook and AMMPool constructor tracked with original prototype/constructor; unhandled rejections retained';
 inventory.boundary.amm='unchanged full original AMM/Property/Contract/Tally bodies; only input rows, genuine constructor cache inputs and explicit PropertyRegistry class binding';
 await reset('amm/proof-original-main-prestate',{observeTypes:true,activeTypes:[10],rows:Object.values(ammRows)});
 await call('amm/01-indexed-property-missing-registry','processTxSet',[[ammRows.property],10]);
 await call('amm/02-indexed-contract-add-real-tally','processTxSet',[[ammRows.add],10]);
 await call('amm/03-indexed-contract-redeem-real-tally','processTxSet',[[ammRows.redeem],10]);
 await call('amm/04-indexed-processed-add-retry-skips','processTxSet',[[ammRows.add],10]);
 await call('amm/05-live-contract-add-real-tally','processTx',[[live(ammRows.liveAdd)],10]);
 await call('amm/06-indexed-zero-rejected','processTxSet',[[ammRows.zero],10]);
 await call('amm/07-live-contract-redeem-and-postphases','blockHandlerMid',[[live(ammRows.liveRedeem)],10]);
 await call('amm/08-live-processed-redeem-retry','blockHandlerMid',[[live(ammRows.liveRedeem)],10]);
 await call('amm/09-indexed-contract-metadata-write-failure','processTxSet',[[ammRows.failure],10],{collection:'contractList',op:'update',message:'amm-main-persist'});
 await call('amm/10-empty-live-only-withdraws','blockHandlerMid',[[],10]);
 assert.equal(records.filter(record=>record.command[1].some(([key])=>key==='target')).length,10,'first execute ten genuine Main calls before extension');
 const proof=records.map(record=>JSON.stringify(record)).join('\n')+'\n';
 assert.equal(sha(proof),'3d860b92ecdadcae535670353dcc64074fc94c2361ce3d9b22f0bc04bd6ecc80','first-ten original calls under nonmutating observer frozen before expansion');
 if(!proofOnly){
  const add=ammRow('roundtrip-contract-add','0,1,1,2,7,3,,,0,'),redeem=ammRow('roundtrip-contract-redeem','1,1,1,2,7,3,,,0,'),max=ammRow('roundtrip-contract-over-max','0,1,1,rt,7,3,,,0,');
  await reset('amm/contract-balanced-roundtrip-input',{observeTypes:true,activeTypes:[10],rows:[add,redeem,max]});
  await call('amm/live-contract-roundtrip-all-six-phases','blockHandlerMid',[[live(add),live(redeem)],10]);
  assert.equal(latest().outcome.returned,10);assert.deepEqual(observedPhases(),[...postPhases,'Channels.processWithdrawals']);
  await call('amm/live-contract-roundtrip-processed-retry','blockHandlerMid',[[live(add),live(redeem)],10]);assert.deepEqual(observedPhases(),['Channels.processWithdrawals']);
  await call('amm/indexed-contract-roundtrip-skips-both','processTxSet',[[add,redeem],10]);assert.equal(latest().outcome.returned,2);
  await call('amm/live-contract-over-max-no-financial-change','blockHandlerMid',[[live(max)],11]);assert.equal(latest().outcome.returned,11);

  const pa=ammRow('property-roundtrip-add','0,0,1,2,7,3,,,0,'),pr=ammRow('property-roundtrip-redeem','1,0,1,2,7,3,,,0,'),reverse=ammRow('property-reverse-pair','0,0,7,2,1,3,,,0,'),pa2=ammRow('property-overredeem-add','0,0,1,2,7,3,,,0,'),over=ammRow('property-second-overredeem','1,0,1,1,7,2s,,,0,');
  await reset('amm/property-genuine-cache-input',{observeTypes:true,activeTypes:[10],rows:[pa,pr,reverse,pa2,over],propertyRegistry:true,propertyPool:true});
  await call('amm/live-property-roundtrip-all-six-phases','blockHandlerMid',[[live(pa),live(pr)],10]);
  assert.equal(latest().outcome.returned,10);assert.deepEqual(observedPhases(),[...postPhases,'Channels.processWithdrawals']);
  await call('amm/indexed-property-reverse-pair-missing-initializer','processTxSet',[[reverse],10]);
  await call('amm/live-second-asset-overredemption-supply-failure','blockHandlerMid',[[live(pa2),live(over)],11]);assert.match(latest().outcome.thrown.message,/Supply mismatch for Property 1/);
  await call('amm/live-overredemption-processed-retry-only-withdrawals','blockHandlerMid',[[live(pa2),live(over)],11]);assert.deepEqual(observedPhases(),['Channels.processWithdrawals']);

  const bound=ammRow('bound-missing-property-pool','0,0,1,2,7,3,,,0,');
  await reset('amm/bound-class-without-pool-input',{observeTypes:true,activeTypes:[10],rows:[bound],propertyRegistry:true});
  await call('amm/indexed-bound-property-missing-initializer','processTxSet',[[bound],10]);assert.match(e.table('consensus').find(row=>row._id===bound.value.txId).params.reason,/initializeAMM is not defined/);

  const fail=ammRow('property-first-delta-failure','0,0,1,2,7,3,,,0,'),fresh=ammRow('property-fresh-retry','0,0,1,2,7,3,,,0,');
  await reset('amm/property-live-delta-failure-input',{observeTypes:true,activeTypes:[10],rows:[fail,fresh],propertyRegistry:true,propertyPool:true});
  await call('amm/live-property-delta-failure-retains-pool-prefix','blockHandlerMid',[[live(fail)],10],{collection:'tallyMapDelta',op:'insert',message:'amm-main-delta'});
  assert.equal(latest().outcome.returned,10);assert.equal(e.ammInstances[0].position,2);
  await call('amm/live-property-delta-processed-retry-only-withdrawals','blockHandlerMid',[[live(fail)],10]);assert.deepEqual(observedPhases(),['Channels.processWithdrawals']);
  await call('amm/live-property-fresh-retry-uses-live-pool-prefix','blockHandlerMid',[[live(fresh)],11]);assert.equal(e.ammInstances[0].position,4);assert.match(latest().outcome.thrown.message,/Supply mismatch/);

  const cf=ammRow('contract-live-persist-failure','0,1,1,2,7,3,,,0,'),cr=ammRow('contract-live-fresh-retry','0,1,1,2,7,3,,,0,');
  await reset('amm/contract-live-persist-failure-input',{observeTypes:true,activeTypes:[10],rows:[cf,cr]});
  await call('amm/live-contract-write-failure-retains-real-debit','blockHandlerMid',[[live(cf)],10],{collection:'contractList',op:'update',message:'amm-main-live-persist'});assert.match(latest().outcome.thrown.message,/Supply mismatch/);
  await call('amm/live-contract-write-failure-processed-retry','blockHandlerMid',[[live(cf)],10]);assert.deepEqual(observedPhases(),['Channels.processWithdrawals']);
  await call('amm/live-contract-fresh-retry-rehydrates-old-pool-and-debits-again','blockHandlerMid',[[live(cr)],11]);assert.match(latest().outcome.thrown.message,/Supply mismatch/);assert.equal(e.table('contractList')[0].data.ammPool.position,2);
  await call('amm/indexed-contract-fresh-retry-now-skips','processTxSet',[[cr],11]);assert.equal(latest().outcome.returned,1);

  const inactive=ammRow('inactive-main-contract','0,1,1,2,7,3,,,0,');
  await reset('amm/inactive-genuine-registry-input',{observeTypes:true,activeTypes:[10],rows:[inactive],inactive:true});
  await call('amm/live-inactive-raw10-only-withdraws','blockHandlerMid',[[live(inactive)],10]);assert.equal(latest().outcome.returned,10);assert.deepEqual(observedPhases(),['Channels.processWithdrawals']);

  await reset('amm/unavailable-main-rpc-isolated-input',{observeTypes:true,activeTypes:[10],rows:[]});
  await call('amm/original-main-rpc-unsupported-boundary','getBlockCountAsync',[]);assert.equal(records.at(-1).domain,'unavailable-capability');
 }
 const payload=records.map(record=>JSON.stringify(record)).join('\n')+'\n';
 inventory.observations=records.length;inventory.calls=records.filter(record=>record.command[1].some(([key])=>key==='target')).length;inventory.fixtureSha256=sha(payload);inventory.proof10Sha256=sha(proof);
 inventory.domains=Object.fromEntries(['source-call','unavailable-capability','constructor-and-input','explicit-input'].map(domain=>[domain,records.filter(record=>record.domain===domain).length]));
 inventory.postphaseCoverage=Object.fromEntries([...postPhases,'Channels.processWithdrawals'].map(target=>[target,records.filter(record=>JSON.stringify(record.effects).includes(target)).length]));
 fs.mkdirSync(path.join(stage,'fixtures'),{recursive:true});
 const outputs=proofOnly?[['main_amm_proof10.jsonl',proof]]:[['main_amm_proof10.jsonl',proof],['main_amm_vectors.jsonl',payload],['main_amm_inventory.json',JSON.stringify(inventory,null,2)+'\n']];
 for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}
 if(flags.includes('--print')){process.stdout.write(payload);return;}
 console.log(JSON.stringify({observations:records.length,calls:inventory.calls,version:flags.includes('--lock')?'9.1.2':'9.3.1',fixtureSha256:sha(payload),postphaseCoverage:inventory.postphaseCoverage,summary:records.filter(record=>record.command[1].some(([key])=>key==='target')).map(record=>({name:record.name,result:record.summary}))},null,2));
}
main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
