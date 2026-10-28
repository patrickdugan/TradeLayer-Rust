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
const records=[];let e,m,Main,TxIndex,marginInstances,contextKeys,startup;
process.on('unhandledRejection',error=>{if(e)e.effects.push({kind:'unhandled-rejection',thrown:{name:error?.name||'Error',message:error?.message||String(error)}});});
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function classBytes(name,sourceName){const full=source[sourceName],start=full.indexOf('class '+name+' {'),end=full.lastIndexOf('module.exports = '+name);assert.ok(start>=0&&end>start);const bytes=full.slice(start,end).trimEnd();inventory.classes[name]={sha256:sha(bytes),bytes:Buffer.byteLength(bytes),fullClass:true};return bytes;}
function compileClass(name,sourceName,scope){const fn=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes(name,sourceName)+'\nreturn '+name+';})',e.context,{filename:'unchanged-class/'+sourceName+'.js',timeout:3000});return fn(...Object.values(scope));}
function unavailable(target){return function(...args){const error=Object.assign(Error('Unavailable external boundary: '+target),{name:'SourceBoundaryUnavailable'});e.effects.push({kind:'unavailable',target,args:e.copy(args),thrown:{name:error.name,message:error.message}});throw error;};}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);inventory.methods[label]={sha256:sha(original.toString()),async:Object.prototype.toString.call(original)==='[object AsyncFunction]'};target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
function state(){const sourceState=e.state(),A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');return{source:sourceState,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,instance:TxIndex.instance},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache},startup};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment({});marginInstances=[];contextKeys=new Set();startup=[];if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
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
 m=new Main();m.txIndex.then(value=>startup.push({returned:value}),error=>startup.push({thrown:{name:error.name,message:error.message}}));await flush();emit(name,{reset:{activationRows:e.copy(e.table('activations')),indexRows:structuredClone(importedRows),heightRow:options.maxHeight??10,...(Object.keys(options).length?{options}: {})},profile:'callable-class'},{returned:{samePrototype:true,sameSingleton:true},startup});}
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


const lowerA='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',lowerB='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw';
const units=n=>(n*1e8).toString(36),issue=(ticker,managed=false)=>[1000..toString(36),ticker,'',+managed,lowerB,0,0,''].join(',');
inventory.inputLabels={};
function lowerRow(label,type,body,sender=lowerA){const id=sha('main-lower:'+label);inventory.inputLabels[id]={label,type};const row=input(id,type.toString(36),body);row.value.sender.senderAddress=sender;return row;}
const lowerRows={
 fixed:lowerRow('fixed',1,issue('MFIX')),
 managed:lowerRow('managed',1,issue('MMNG',true)),
 grant:lowerRow('grant',11,'9,'+units(2)+','+lowerB),
 redeem:lowerRow('redeem',12,'9,'+units(1)),
 send:lowerRow('send',2,'0;'+lowerB+';8;2'),
 commit:lowerRow('commit',4,'8,'+units(2)+','+lowerB+',1,0,,'),
 list:lowerRow('list',7,lowerB+',MAINLIST,url,description'),
 attest:lowerRow('attest',9,'0,1,'+lowerB+',ca'),
 listAdmin:lowerRow('list-admin',8,lowerB+',1,0,0,1,0'),
 oracle:lowerRow('oracle',13,'MAINOR,url,'+lowerB+',,1'),
 publish:lowerRow('publish',14,['2',(10*1e4).toString(36),(12).toString(36),'8','a'].join(',')),
 oracleAdmin:lowerRow('oracle-admin',8,lowerB+',0,1,0,2,0'),
 close:lowerRow('close',15,'2'),
 postphase:lowerRow('postphase-send',2,'0;'+lowerB+';8;1'),
};
async function importRows(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));await flush();emit(name,{import:{collection,rows}},{returned:undefined});}
async function bindOriginalRpc(name){e.effects.length=0;bindRpc();await flush();emit(name,{binding:'rpcTransactions'},{returned:undefined});}
async function main(){
 inventory.boundary.activation='unchanged complete Activation registry loaded from imported source activationsList; all exercised types genuinely active at block1; no declared-answer Main profile';
 inventory.boundary.rpc='unbound TxUtils/Client/Persistence dependencies are explicitly unavailable and excluded from Rust parity. The explicit bound profile reuses original pure TxUtils classifier/extractor over imported raw transaction documents, without a pubkey/validity/authorization verdict. No production startup or network success is inferred.';
 inventory.boundary.console='ordinary console methods return undefined; log arguments remain recorded';
 const ordinaryRpc=[{txid:lowerRows.commit.value.txId,transaction:{vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}}];
 await reset('lower/registry-and-shared-source-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[1,2,4,7,8,9,11,12,13,14,15,18],rows:Object.values(lowerRows),bindRpc:true,rpcTransactions:ordinaryRpc});
 await call('lower/01-indexed-fixed-and-managed-issue','processTxSet',[[lowerRows.fixed,lowerRows.managed],10]);
 await call('lower/02-live-grant-and-redemption','processTx',[[live(lowerRows.grant),live(lowerRows.redeem)],10]);
 await call('lower/03-indexed-funded-scalar-send','processTxSet',[[lowerRows.send],10]);
 await call('lower/04-indexed-commit-original-bound-rpc','processTxSet',[[lowerRows.commit],10]);
 await call('lower/05-indexed-create-clearlist','processTxSet',[[lowerRows.list],10]);
 await call('lower/06-live-attestation-and-clearlist-admin','processTx',[[live(lowerRows.attest),live(lowerRows.listAdmin)],10]);
 await call('lower/07-indexed-oracle-create-and-publish','processTxSet',[[lowerRows.oracle,lowerRows.publish],10]);
 await call('lower/08-indexed-oracle-admin-and-source-close-error','processTxSet',[[lowerRows.oracleAdmin,lowerRows.close],10]);
 await call('lower/09-live-processed-lower-rows-skip','processTx',[[...Object.values(lowerRows).slice(0,13).map(live)],10]);
 await call('lower/10-live-send-through-real-postphases','blockHandlerMid',[[live(lowerRows.postphase)],10]);
 assert.deepEqual(phases(),[...postPhases,'Channels.processWithdrawals']);
 const processed=e.table('consensus').filter(r=>r.processed===true);
 for(const key of ['fixed','managed','grant','redeem','send','list','attest','oracle','publish','oracleAdmin','postphase'])assert.equal(processed.find(r=>r._id===lowerRows[key].value.txId).params.valid,true,key+' actual source acceptance');
 assert.equal(processed.find(r=>r._id===lowerRows.commit.value.txId).params.valid,true,'bound commit actual source acceptance');
 assert.match(processed.find(r=>r._id===lowerRows.close.value.txId).params.reason,/Cannot use 'in' operator to search for 'high' in 2/);
 if(!proofOnly){
  const oldSender='1BoatSLRHtKNngkdXEeobR76b53LETtpyT';
  const indexedUnbound=lowerRow('indexed-commit-default-rpc',4,'7,'+units(2)+','+lowerB+',1,0,,',oldSender);
  await reset('indexed/commit-unavailable-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[4,18],rows:[indexedUnbound]});
  await call('indexed/commit-unavailable-rpc-prefix-isolated','processTxSet',[[indexedUnbound],10]);
  assert.match(e.table('consensus').find(r=>r._id===indexedUnbound.value.txId).params.reason,/Unavailable external boundary: TxUtils.getRawTransaction/);
  const ordinarySend=label=>lowerRow(label,2,'0;'+lowerB+';7;1',oldSender);
  const future=ordinarySend('future-activation-block');
  await reset('activation/future-block-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[2],activationBlock:20,rows:[future]});
  await call('activation/future-block-is-not-an-extra-validity-rule','processTxSet',[[future],10]);
  assert.equal(e.table('consensus').find(r=>r._id===future.value.txId).params.valid,true);
  const inactive=ordinarySend('inactive-real-registry');
  await reset('activation/inactive-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[2],inactive:true,rows:[inactive]});
  await call('activation/inactive-is-processed-no-financial-postphases','blockHandlerMid',[[live(inactive)],10]);assert.deepEqual(phases(),['Channels.processWithdrawals']);assert.equal(e.table('consensus').find(r=>r._id===inactive.value.txId).params.valid,false);
  for(const[scenario,failure]of [
   ['send-delta-failure',{collection:'tallyMapDelta',op:'insert',message:'lower-send-delta'}],
   ['processed-write-failure',{collection:'consensus',op:'update',message:'lower-processed-write'}],
   ['index-validity-failure',{collection:'txIndex',op:'update',message:'lower-index-validity'}],
  ]){
   const row=ordinarySend(scenario);await reset('failure/'+scenario+'/prestate',{balancedSupply:true,observeTypes:true,activeTypes:[2],rows:[row]});
   await call('failure/'+scenario+'/retained-prefix','processTx',[[live(row)],10],failure);
   assert.equal(e.table('consensus').find(r=>r._id===row.value.txId).params.valid,false);
   await call('failure/'+scenario+'/processed-retry-skips','processTx',[[live(row)],10]);assert.equal(records.at(-1).summary.outcome.returned,false);
  }
  const failedCommit=lowerRow('live-commit-default-rpc',4,'7,'+units(2)+','+lowerB+',1,0,,',oldSender);
  await reset('live/commit-unavailable-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[4,18],rows:[failedCommit]});
  await call('live/commit-logic-error-still-runs-real-financial-phases','blockHandlerMid',[[live(failedCommit)],10]);assert.deepEqual(phases(),[...postPhases,'Channels.processWithdrawals']);assert.match(e.table('consensus').find(r=>r._id===failedCommit.value.txId).params.reason,/Unavailable external boundary/);
  const buy=lowerRow('live-contract-buy',18,[1,(100*1e8).toString(36),'1','1','0','0','0','0'].join(','),oldSender);
  const sell=lowerRow('live-contract-sell',18,[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),'1BitcoinEaterAddressDontSendf59kuE');
  await reset('live/matched-contract-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[18],rows:[buy,sell]});
  await call('live/matched-contracts-through-real-postphases','blockHandlerMid',[[live(buy),live(sell)],10]);
  assert.deepEqual(phases(),postPhases);
  assert.match(records.at(-1).summary.outcome.thrown.message,/Supply mismatch for Property 7, diff -0\.00050000/);
  assert.ok(e.table('marginMaps').some(row=>JSON.parse(row.value).some(pair=>pair[1].contracts===1)),'real matched positions must exist');
  const bound=lowerRow('bound-rpc-commit',4,'7,'+units(2)+','+lowerB+',1,0,,',oldSender);
  await reset('bound-rpc/commit-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[4,18],rows:[bound]});
  await importRows('bound-rpc/original-transaction-document','sourceRpcTransactions',[{txid:bound.value.txId,transaction:{vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}}]);
  await bindOriginalRpc('bound-rpc/original-pure-txutils-profile');
  await call('bound-rpc/real-channel-commit-and-processed-marker','processTxSet',[[bound],10]);assert.equal(e.table('consensus').find(r=>r._id===bound.value.txId).params.valid,true);
  await call('bound-rpc/empty-live-call-only-withdraws','blockHandlerMid',[[],10]);assert.deepEqual(phases(),['Channels.processWithdrawals']);
  const utxoBody='7,'+units(2)+',1,rs,1,0,0,';
  const utxoMissing=lowerRow('utxo-missing-references',3,utxoBody,oldSender),utxoPositive=lowerRow('utxo-positive-matching-references',3,utxoBody,oldSender);
  utxoPositive.value.reference=[{vout:0,address:oldSender},{vout:1,address:lowerB}];
  await reset('raw3/original-main-prestate',{balancedSupply:true,observeTypes:true,activeTypes:[3,18],rows:[utxoMissing,utxoPositive]});
  await call('raw3/indexed-no-reference-has-no-valid-flag','processTxSet',[[utxoMissing],10]);
  assert.equal(e.table('consensus').find(r=>r._id===utxoMissing.value.txId).processed,true);
  assert.equal(Object.hasOwn(e.table('consensus').find(r=>r._id===utxoMissing.value.txId).params,'valid'),false);
  await call('raw3/live-matching-reference-original-outputs-error','blockHandlerMid',[[live(utxoPositive)],10]);
  assert.equal(e.table('consensus').find(r=>r._id===utxoPositive.value.txId).params.valid,false);
  assert.equal(e.table('consensus').find(r=>r._id===utxoPositive.value.txId).params.reason,"Fatal tx processing error: Cannot read properties of undefined (reading 'length')");
  assert.deepEqual(phases(),['Channels.processWithdrawals']);
 }
 const proof=records.slice(0,11).map(r=>JSON.stringify(r)).join('\n')+'\n',payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';
 inventory.observations=records.length;inventory.calls=records.filter(r=>r.command?.[0]==='object'&&r.command[1].some(([k])=>k==='target')).length;inventory.fixtureSha256=sha(payload);inventory.proof10Sha256=sha(proof);inventory.domains=Object.fromEntries(['source-call','unavailable-capability','constructor-and-input','explicit-input'].map(domain=>[domain,records.filter(r=>r.domain===domain).length]));
 const outputs=proofOnly?[['main_lower_proof10.jsonl',proof]]:[['main_lower_proof10.jsonl',proof],['main_lower_vectors.jsonl',payload],['main_lower_inventory.json',JSON.stringify(inventory,null,2)+'\n']];
 fs.mkdirSync(path.join(stage,'fixtures'),{recursive:true});
 for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),value);else fs.writeFileSync(file,value);}
 const summary={observations:records.length,methods:inventory.calls,domains:inventory.domains,fixtureSha256:sha(payload),version:flags.includes('--lock')?'9.1.2':'9.3.1',calls:records.filter(r=>r.command?.[0]==='object'&&r.command[1].some(([k])=>k==='target')).map(r=>({name:r.name,domain:r.domain,result:r.summary})),processed:e.table('consensus').filter(r=>r.processed===true).map(r=>({label:inventory.inputLabels[r._id]?.label,id:r._id,valid:r.params.valid,reason:r.params.reason})),unavailable:records.flatMap(r=>{const v=r.effects;return v?.[0]==='array'?v[1].filter(x=>JSON.stringify(x).includes('unavailable')):[]}).length};
 if(flags.includes('--print'))process.stdout.write(payload);else console.log(JSON.stringify(summary,null,2));
}
main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
