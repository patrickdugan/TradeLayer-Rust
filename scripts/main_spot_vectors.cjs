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
if(flags.includes('--all-receivers'))replace('    return module.exports;\n  }\n  // Imported persisted state',`    if(name==='orderbook'){
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
function state(){const sourceState=e.state(),A=e.load('activation').instance,C=e.load('channels'),R=e.load('contractRegistry'),T=e.load('tally'),P=e.load('property'),clearing=e.load('clearing');return{source:sourceState,receiver:{own:Object.fromEntries(Object.entries(m).map(([k,v])=>[k,typeof v==='function'?{function:v.name}:v])),samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m},index:{classClient:TxIndex.client,instance:TxIndex.instance},activation:A?{...A}:null,tally:{addresses:T.addresses,initialized:!!T.addresses,modFlag:T.modFlag,instance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined},contract:{instance:R.instance?{...R.instance}:undefined,modFlag:R.modFlag},property:P.instance?{...P.instance}:undefined,clearlist:{clearlistsMap:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,merkleXpubMap:e.load('clearlist').merkleXpubMap},channels:{registry:C.channelsRegistry,pending:C.pendingWithdrawals},marginInstances:marginInstances.map(i=>({...i})),clearingContexts:[...contextKeys].map(key=>[key,clearing.getPositionsFromCache(key)]),globals:{liquidityQueue:e.logic._liquidityRewardQueue,liquidityVolume:e.logic._liquidityRewardVolumeCache,tallyMapBound:!!e.context.tallyMap,spotRpcDocumentsBound:!!e.context.spotRpcDocumentsBound},startup};}
function emit(name,command,result){const current=state();assert.deepEqual(current.source.unsupported,[],name+' hidden unowned financial dependency');const domain=command.target?(e.effects.some(x=>x.kind==='unavailable')?'unavailable-capability':'source-call'):command.reset?'constructor-and-input':'explicit-input';records.push({name,domain,command:tag(command),result:tag(result),state:tag(current),effects:tag(e.effects),summary:result});}
function input(id,kind,body){return{_id:'tx-10-'+id,value:{txId:id,marker:'tl',payload:kind+body,sender:{senderAddress:'1BoatSLRHtKNngkdXEeobR76b53LETtpyT',amount:0},reference:[]}};}
const rawBody=[1,(100*1e8).toString(36),'1','0','0','0','0','0'].join(','),rows=[input('index32','w','[]'),input('live18','i',rawBody),input('marker18','i',rawBody)];
const live=row=>row.value;
async function reset(name='proof/02-original-class-constructor-and-unavailable-index-startup',options={}){e=mod.exports.environment({});marginInstances=[];contextKeys=new Set();startup=[];seedSpotInputs(options);if(options.rpcTransactions)e.table('sourceRpcTransactions').push(...structuredClone(options.rpcTransactions));if(options.bindRpc)bindRpc();
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
 const raw=fs.readFileSync(path.join(repo,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(sha(raw),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(repo,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const client={async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'Client.getRawTransaction',args,returned:structuredClone(value)});return value;},async getBlock(...args){const value=structuredClone(e.table('sourceRpcBlocks').find(row=>row.hash===args[0])?.block);e.effects.push({kind:'rpc',method:'Client.getBlock',args,returned:structuredClone(value)});return value;}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+raw+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve(client)}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 for(const key of ['getRawTransaction','getBlockHeight','getAddressTypeUniversal','extractPubkeyByType'])e.adapters['./txUtils'][key]=function(...args){e.effects.push({kind:'source-function',target:'TxUtils.'+key,args:e.copy(args)});return Reflect.apply(own.exports[key],own.exports,args);};
 e.context.spotRpcDocumentsBound=true;
}



const spotA='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',spotB='1BitcoinEaterAddressDontSendf59kuE',units=v=>(v*1e8).toString(36);
inventory.inputLabels={};
function spotRow(label,type,body,sender=spotA){const id=sha('main-spot:'+label);inventory.inputLabels[id]={label,type};const row=input(id,type.toString(36),body);row.value.sender.senderAddress=sender;return row;}
const spotRows={
 buy:spotRow('vanilla-buy',5,'7,1,'+units(1)+','+units(1)+',0,0'),
 sell:spotRow('vanilla-sell',5,'1,7,'+units(1)+','+units(1)+',0,0',spotB),
 cancelTx:spotRow('cancel-plural',6,'0,7,1,0,'+'a'.repeat(64)+',buy'),
 cancelProbe:spotRow('cancel-noop-trigger',6,'0,7,1,0,,buy'),
 cancelAll:spotRow('cancel-all',6,'0,7,1,1,,buy'),
 zero:spotRow('zero',5,'7,1,0,'+units(1)+',0,0'),
 stopPost:spotRow('stop-post',5,'7,1,'+units(1)+','+units(2)+',1,1'),
 same:spotRow('same-property',5,'7,7,'+units(1)+','+units(1)+',0,0'),
};
function documents(rows){return rows.map(row=>({txid:row.value.txId,transaction:{blockhash:'spot-block',vin:[],vout:[]}}));}
function seedSpotInputs(options){
 const rows=JSON.parse(e.table('tallyMap')[0].data),reserve=options.reserved||0;
 for(const[,properties]of rows){properties[1]=structuredClone(properties[7]);for(const value of Object.values(properties)){value.reserved=reserve;value.amount+=reserve;}}
 e.table('tallyMap')[0].data=JSON.stringify(rows);
 const properties=JSON.parse(e.table('propertyList')[0].value);for(const[id,value]of properties)value.totalInCirculation=[1,7].includes(id)?2*(1000+reserve):0;e.table('propertyList')[0].value=JSON.stringify(properties);
 e.table('sourceRpcBlocks').push({hash:'spot-block',block:{height:9}});
 if(options.bindTally)e.context.tallyMap=e.load('tally');
 if(options.historicalBook){
  const buy={offeredPropertyId:7,desiredPropertyId:1,amountOffered:1,amountExpected:1,blockTime:9,sender:spotA,senderAddress:spotA,stop:false,post:false,price:1,txid:'history-buy'};
  const sell={offeredPropertyId:1,desiredPropertyId:7,amountOffered:1,amountExpected:1,blockTime:9,sender:spotB,senderAddress:spotB,stop:false,post:false};
  e.table('orderBooks').push({_id:'1-7',value:JSON.stringify({buy:[buy],sell:[]})},{_id:'activePairs',pairs:['1-7']},{_id:'queue-1-7',orders:[{kind:'token',orderBookKey:'1-7',sender:spotB,blockHeight:10,txid:'e'.repeat(64),order:sell}]});
 }
}
function latest(){return records.at(-1).summary;}
async function main(){
 inventory.boundary.activation='unchanged original Activation registry loaded from persisted rows; raw5/6 active at block1; no supplied Main validity answer';
 inventory.boundary.rpc='explicit document capability executes unchanged original TxUtils getRawTransaction/getBlockHeight over input RPC transaction/block documents; no authentication or financial verdict; original constructor startup client boundary remains unavailable';
 inventory.boundary.console='ordinary console methods return undefined; source log argument evaluation remains original';
 await reset('spot/proof-original-main-prestate',{observeTypes:true,activeTypes:[5,6],rows:Object.values(spotRows),bindRpc:true,rpcTransactions:documents(Object.values(spotRows))});
 await call('spot/01-indexed-vanilla-buy-and-counter-enqueue','processTxSet',[[spotRows.buy,spotRows.sell],10]);
 await call('spot/02-live-processed-orders-skip','processTx',[[live(spotRows.buy),live(spotRows.sell)],10]);
 await call('spot/03-live-noop-cancel-triggers-vanilla-match-failure','blockHandlerMid',[[live(spotRows.cancelProbe)],10]);
 assert.equal(latest().outcome.thrown.message,'tallyMap is not defined');
 assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),postPhases.slice(0,3));
 await call('spot/04-processed-cancel-retry-only-withdraws','blockHandlerMid',[[live(spotRows.cancelProbe)],10]);
 assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),['Channels.processWithdrawals']);
 await call('spot/05-indexed-cancel-all-literal-noop-and-wrong-save','processTxSet',[[spotRows.cancelAll],10]);
 await call('spot/06-indexed-cancel-all-retry-skips','processTxSet',[[spotRows.cancelAll],10]);
 assert.equal(latest().outcome.returned,1);
 await call('spot/07-live-txid-cancel-rejected-by-actual-validator','processTx',[[live(spotRows.cancelTx)],10]);
 await call('spot/08-indexed-stop-post-original-txutils','processTxSet',[[spotRows.stopPost],10]);
 await call('spot/09-indexed-zero-and-same-property-rejected','processTxSet',[[spotRows.zero,spotRows.same],10]);
 await call('spot/10-empty-live-call-only-withdraws','blockHandlerMid',[[],10]);
 assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),['Channels.processWithdrawals']);
 const proof=records.map(r=>JSON.stringify(r)).join('\n')+'\n';
 assert.equal(records.filter(r=>r.command[1].some(([key])=>key==='target')).length,10,'execute first ten genuine Main calls before extending');
 if(!proofOnly){
  const one=spotRow('unmatched-live-order',5,'7,1,'+units(1)+','+units(1)+',0,0');
  const cancel=spotRow('unmatched-live-cancel-all',6,'0,7,1,1,,buy');
  await reset('spot/unmatched-all-six-phases-prestate',{observeTypes:true,activeTypes:[5,6],rows:[one,cancel],bindRpc:true,rpcTransactions:documents([one])});
  await call('spot/live-unmatched-order-runs-all-six-phases','blockHandlerMid',[[live(one)],10]);
  assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),[...postPhases,'Channels.processWithdrawals']);
  await call('spot/live-cancel-all-retains-source-book-all-six-phases','blockHandlerMid',[[live(cancel)],10]);
  assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),[...postPhases,'Channels.processWithdrawals']);
  const retry=spotRow('bound-vanilla-retry-trigger',6,'0,7,1,0,,buy');
  await reset('spot/bound-vanilla-match-failure-prestate',{observeTypes:true,activeTypes:[5,6],rows:[spotRows.buy,spotRows.sell,retry],bindRpc:true,bindTally:true,rpcTransactions:documents([spotRows.buy,spotRows.sell])});
  await call('spot/live-bound-vanilla-match-reaches-negative-reserve','blockHandlerMid',[[live(spotRows.buy),live(spotRows.sell)],10]);
  assert.match(latest().outcome.thrown.message,/Reserved balance cannot go negative/);
  await call('spot/indexed-bound-vanilla-processed-retry-skips','processTxSet',[[spotRows.buy,spotRows.sell],10]);assert.equal(latest().outcome.returned,2);
  await call('spot/live-new-cancel-retries-real-failed-queue-prefix','blockHandlerMid',[[live(retry)],11]);
  const history=spotRow('historical-order-trigger',6,'0,7,1,0,,buy'),flush=spotRow('historical-fee-flush-trigger',6,'0,7,1,0,,buy');
  await reset('spot/historical-enriched-input-prestate',{observeTypes:true,activeTypes:[5,6],rows:[history,flush],bindRpc:true,bindTally:true,reserved:10,historicalBook:true});
  await call('spot/live-historical-enriched-settlement-all-six-phases','blockHandlerMid',[[live(history)],10]);
  assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),[...postPhases,'Channels.processWithdrawals']);
  assert.ok(e.table('tradeHistory').some(row=>row.type==='token'),'real historical source match must write its trade');
  assert.equal(e.load('tally')._feeAccrualBuffer.has('10'),true,'real settlement creates the block10 fee buffer');
  await call('spot/live-historical-processed-retry-does-not-flush-fees','blockHandlerMid',[[live(history)],10]);
  assert.deepEqual(phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals'),['Channels.processWithdrawals']);
  await call('spot/live-new-same-block-cancel-flushes-real-fees','blockHandlerMid',[[live(flush)],10]);
  inventory.feeScene={result:e.copy(latest()),remainingBlocks:[...e.load('tally')._feeAccrualBuffer.keys()],feeRows:e.copy(e.table('feeCache')),phases:phases().filter(x=>postPhases.includes(x)||x==='Channels.processWithdrawals')};
  const failed=spotRow('cancel-book-write-failure',6,'0,7,1,1,,buy');
  await reset('spot/cancel-write-failure-prestate',{observeTypes:true,activeTypes:[5,6],rows:[failed],bindRpc:true});
  await call('spot/live-cancel-write-failure-still-runs-source-phases','blockHandlerMid',[[live(failed)],10],{collection:'orderBooks',op:'update',message:'cancel-book-write'});
  assert.match(e.table('consensus').find(row=>row._id===failed.value.txId).params.reason,/Logic error: cancel-book-write/);
  await call('spot/indexed-cancel-write-failure-processed-retry-skips','processTxSet',[[failed],10]);assert.equal(latest().outcome.returned,1);
  const unavailableRow=spotRow('unbound-original-rpc',5,'7,1,'+units(1)+','+units(1)+',0,0');
  await reset('spot/unavailable-rpc-isolated-prestate',{observeTypes:true,activeTypes:[5,6],rows:[unavailableRow]});
  await call('spot/indexed-unbound-rpc-is-recorded-unavailable','processTxSet',[[unavailableRow],10]);
  assert.match(e.table('consensus').find(row=>row._id===unavailableRow.value.txId).params.reason,/Unavailable external boundary: TxUtils.getRawTransaction/);
 }
 const payload=records.map(r=>JSON.stringify(r)).join('\n')+'\n';
 inventory.observations=records.length;inventory.calls=records.filter(r=>r.command[1].some(([key])=>key==='target')).length;inventory.fixtureSha256=sha(payload);inventory.proof10Sha256=sha(proof);
 inventory.domains=Object.fromEntries(['source-call','unavailable-capability','constructor-and-input','explicit-input'].map(domain=>[domain,records.filter(r=>r.domain===domain).length]));
 const outputs=proofOnly?[['main_spot_proof10.jsonl',proof]]:[['main_spot_proof10.jsonl',proof],['main_spot_vectors.jsonl',payload],['main_spot_inventory.json',JSON.stringify(inventory,null,2)+'\n']];
 fs.mkdirSync(path.join(stage,'fixtures'),{recursive:true});for(const[name,value]of outputs){const observedName=flags.includes('--all-receivers')?name.replace(/\.(jsonl|json)$/,'_receivers_v2.$1'):name;const file=path.join(stage,'fixtures',observedName);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),value);else fs.writeFileSync(file,value);}
 if(flags.includes('--print')){process.stdout.write(payload);return;}
 console.log(JSON.stringify({observations:records.length,calls:inventory.calls,version:flags.includes('--lock')?'9.1.2':'9.3.1',fixtureSha256:sha(payload),summary:records.filter(r=>r.command[1].some(([key])=>key==='target')).map(r=>({name:r.name,result:r.summary}))},null,2));
}
main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
