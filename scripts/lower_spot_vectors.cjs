#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.env.SPOT_REPO||path.resolve(__dirname,'..'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
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
replace('return {types,logic,Orderbook,effects','return {instanceId,instances,adapters,context,storage,tables,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);

const txids={'raw5-queued':'a'.repeat(64),'raw5-counter':'b'.repeat(64),'direct-stop-post':'c'.repeat(64)};const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('channels');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,channels:C.channelsRegistry,withdrawals:C.pendingWithdrawals,orderbookInstances:e.copy(e.state().instances),...(records.length>=11?{feeAccrualBuffer:T._feeAccrualBuffer,feeCache:T.feeCache,clearing:e.copy(e.state().clearing),boundTally:!!e.context.tallyMap}:{}),lastParams,unhandled,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','rpc','activation'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
function bindRpc(){
 const source=fs.readFileSync(path.join(root,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(root,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const client={async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'Client.getRawTransaction',args,returned:structuredClone(value)});return value;},async getBlock(...args){const value=structuredClone(e.table('sourceRpcBlocks').find(row=>row.hash===args[0])?.block);e.effects.push({kind:'rpc',method:'Client.getBlock',args,returned:structuredClone(value)});return value;}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+source+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve(client)}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 for(const key of ['getRawTransaction','getBlockHeight'])e.adapters['./txUtils'][key]=function(...args){e.effects.push({kind:'source-function',target:'TxUtils.'+key,args:structuredClone(args)});return Reflect.apply(own.exports[key],own.exports,args);};
}
async function reset(name,options={}){
 e=driver.exports.environment(options);lastParams=undefined;unhandled=[];
 if(options.bindTally)e.context.tallyMap=e.load('tally');
 const rows=JSON.parse(e.table('tallyMap')[0].data);const shape=rows[0][1][7];for(const row of rows){row[1][1]=structuredClone(shape);if(options.reserved){for(const value of Object.values(row[1])){value.reserved=options.reserved;value.amount+=options.reserved;}}}if(!rows.some(row=>row[0]===B))rows.push([B,{1:structuredClone(shape),7:structuredClone(shape)}]);e.table('tallyMap')[0].data=JSON.stringify(rows);
 e.table('sourceRpcTransactions').push(...['raw5-queued','raw5-counter','direct-stop-post'].map(name=>({txid:txids[name],transaction:{blockhash:'spot-block',vin:[],vout:[]}})));
 e.table('sourceRpcBlocks').push({hash:'spot-block',block:{height:9}});bindRpc();
 for(const method of ['onChainTokenToToken','cancelOrder'])observe(e.logic,method,'Logic.'+method);
 observe(e.Orderbook,'queueOnChainTokenOrder','Orderbook.queueOnChainTokenOrder');
 for(const method of ['cancelAllTokenOrders','cancelTokenOrderByTxid','cancelTokenBuyOrdersByPrice','cancelTokenSellOrdersByPrice','cancelAllContractOrders','cancelContractOrderByTxid','cancelOrdersByCriteria','saveOrderBook','loadOrderBook','getOrdersForAddress','addTokenOrder','matchTokenOrders','processTokenMatches']){
  const original=e.Orderbook.prototype[method];if(typeof original!=='function')continue;e.Orderbook.prototype[method]=function(...args){const id=e.instanceId(this);e.effects.push({kind:'source-function',target:'Orderbook.instance.'+method,instance:id,args:structuredClone(args)});return Reflect.apply(original,this,args);};
 }
 await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});
}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,type,payload,sender=A,reference=null,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[txids[name]||name,type,'tl',payload,sender,reference,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure){e.effects.length=0;if(failure)fail(failure);const [ns,method]=target.split('.'),receiver=ns==='Logic'?e.logic:ns==='Orderbook'?e.Orderbook:e.load('tally');let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure},{transport,outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.tables.set(collection,e.copy(rows));emit(name,{import:{collection,rows}},{outcome:{returned:undefined}});}
async function instanceCall(name,key,method,args=[]){e.effects.length=0;const command={instance:{key,method,args:structuredClone(args)}},receiver=new e.Orderbook(key);e.instanceId(receiver);let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,command,{transport,outcome});}
const amount=v=>(v*1e8).toString(36),send=(v,p=7)=>'0;'+B+';'+p.toString(36)+';'+v;
(async()=>{
 await reset('first-ten');
 await raw('raw5-queued',5,'7,1,'+amount(1)+','+amount(1)+',0,0');
 await raw('raw5-counter',5,'1,7,'+amount(1)+','+amount(1)+',0,0',B);
 await call('actual-token-queue-drain','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
 await raw('raw6-no-sender-transport',6,'0,7,1,0,,buy');
 await call('direct-cancel-empty-all','Logic.cancelOrder',[A,false,7,1,true,{},10]);
 await call('direct-cancel-plural-missing','Logic.cancelOrder',[A,false,7,1,false,{txid:'a'.repeat(64)},10]);
 await call('direct-contract-cancel-empty','Logic.cancelOrder',[A,true,1,null,true,{},10]);
 await call('direct-cancel-null-params','Logic.cancelOrder',[A,false,7,1,false,null,10]);
 await call('direct-stop-post-original-rpc','Logic.onChainTokenToToken',[A,7,1,1,2,txids['direct-stop-post'],10,true,true]);
 await call('actual-stop-token-drain','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
 if(!flags.includes('--proof10')){
  await reset('bound-source-financial-realm',{bindTally:true});
  await raw('raw5-queued',5,'7,1,'+amount(1)+','+amount(1)+',0,0');
  await raw('raw5-counter',5,'1,7,'+amount(1)+','+amount(1)+',0,0',B);
  await call('bound-actual-token-queue-drain','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
  await call('bound-repeat-token-queue-drain','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
  await reset('bound-source-legacy-enriched-orders',{bindTally:true,reserved:10});
  await call('legacy-enriched-buy-queue','Orderbook.queueOnChainTokenOrder',['7-1',A,{offeredPropertyId:7,desiredPropertyId:1,amountOffered:1,amountExpected:1,blockTime:9,sender:A,senderAddress:A,stop:false,post:false},10,'d'.repeat(64)]);
  await call('legacy-enriched-sell-queue','Orderbook.queueOnChainTokenOrder',['1-7',B,{offeredPropertyId:1,desiredPropertyId:7,amountOffered:1,amountExpected:1,blockTime:9,sender:B,senderAddress:B,stop:false,post:false},10,'e'.repeat(64)]);
  await call('bound-legacy-source-financial-drain','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
  await reset('bound-source-self-trade',{bindTally:true});
  await raw('raw5-queued',5,'7,1,'+amount(1)+','+amount(1)+',0,0');
  await raw('raw5-counter',5,'1,7,'+amount(1)+','+amount(1)+',0,0');
  await call('bound-same-block-self-match-limit','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
  await reset('raw5-guard-refusals');
  await raw('zero-token-order',5,'7,1,0,'+amount(1)+',0,0');
  await raw('same-property-order',5,'7,7,'+amount(1)+','+amount(1)+',0,0');
  await raw('vesting-property-order',5,'3,7,'+amount(1)+','+amount(1)+',0,0');
  await raw('insufficient-token-order',5,'7,1,'+amount(2000)+','+amount(1)+',0,0');
  await call('direct-rpc-document-missing','Logic.onChainTokenToToken',[A,7,1,1,1,'missing',10,false,false]);
  await reset('resting-book-cancel-prefixes',{bindTally:true});
  await raw('raw5-queued',5,'7,1,'+amount(2)+','+amount(1)+',0,0');
  await call('create-real-resting-order','Orderbook.processQueuedOnChainOrdersForBlock',[10]);
  await raw('raw6-live-book-filter-transport',6,'0,7,1,0,,buy');
  await call('cancel-all-live-real-book','Logic.cancelOrder',[A,false,7,1,true,{},10]);
  await call('cancel-price-live-real-book','Logic.cancelOrder',[A,false,7,1,false,{price:2},10]);
  await reset('cancellation-invalid-flags');
  for(const flag of [null,undefined,'other',2])await call('cancel-flag-'+String(flag),'Logic.cancelOrder',[A,flag,7,1,true,{},10]);
  await reset('direct-source-token-matcher');
  await instanceCall('actual-source-price-rounding','1-7','calculatePrice',[1,6]);
  await instanceCall('actual-source-lexical-pair-key','1-7','normalizeOrderBookKey',['10','2']);
  const buy=(extra={})=>({offeredPropertyId:7,desiredPropertyId:1,amountOffered:4,amountExpected:2,blockTime:9,sender:A,price:2,post:false,...extra});
  const sell=(extra={})=>({offeredPropertyId:1,desiredPropertyId:7,amountOffered:2,amountExpected:4,blockTime:9,sender:B,price:2,post:false,...extra});
  await instanceCall('actual-source-direct-full-match','1-7','matchTokenOrders',[{buy:[buy()],sell:[sell()]}]);
  await instanceCall('actual-source-buy-post-bump','1-7','matchTokenOrders',[{buy:[buy({blockTime:10,post:true})],sell:[sell()]}]);
  await instanceCall('actual-source-sell-post-string-plus-bump','1-7','matchTokenOrders',[{buy:[buy()],sell:[sell({blockTime:10,post:true})]}]);
  await instanceCall('actual-source-older-maker-self-removal','1-7','matchTokenOrders',[{buy:[buy({blockTime:10})],sell:[sell({sender:A})]}]);
  await instanceCall('actual-source-property-mismatch-break','1-7','matchTokenOrders',[{buy:[buy()],sell:[sell({desiredPropertyId:8})]}]);
  await reset('direct-source-cancel-branches',{bindTally:true});
  const book=()=>({buy:[buy({txid:'one',initMargin:1}),buy({sender:B,txid:'two',initMargin:3})],sell:[sell({sender:A,txid:'one',initMargin:5})]});
  const persist=book=>[{_id:'1-7',value:JSON.stringify(book)}];
  await seed('cancel-criteria-input','orderBooks',persist(book()));
  await instanceCall('actual-source-no-txid-price-address-noop','1-7','cancelOrdersByCriteria',['outsider','1-7',{address:A,price:2,buy:true},true]);
  await instanceCall('actual-source-txid-buy-then-address-branch','1-7','cancelOrdersByCriteria',['outsider','1-7',{txid:'one',buy:true,address:B},true]);
  await seed('cancel-sell-bug-input','orderBooks',persist(book()));
  await instanceCall('actual-source-sell-txid-splices-buy','1-7','cancelOrdersByCriteria',['outsider','1-7',{txid:'one',buy:false},true]);
  await instanceCall('actual-source-singular-token-txid-unbound-order','1-7','cancelTokenOrderByTxid',[A,7,1,'one',10]);
  const amm=book();amm.buy.push(buy({sender:'amm',txid:'virtual'}));amm.sell.push(sell({sender:'amm',txid:'virtual'}));
  await seed('cancel-amm-input','orderBooks',persist(amm));
  await instanceCall('actual-source-amm-both-sides-no-refund','1-7','cancelOrdersByCriteria',['outsider','1-7',{},true,true]);

  await reset('direct-source-financial-return-alias',{bindTally:true,reserved:10});
  await instanceCall('direct-source-add-legacy-buy','1-7','addTokenOrder',[buy({senderAddress:A}),10,'f'.repeat(64)]);
  await instanceCall('direct-source-add-legacy-sell-keeps-order-roles','1-7','addTokenOrder',[sell({senderAddress:B}),10,'g'.repeat(64)]);
  assert.equal(records.at(-1).result.outcome.returned.matches[0].buyOrder.orderRole,'split');
  assert.equal(records.at(-1).result.outcome.returned.matches[0].sellOrder.orderRole,'split');
  await reset('direct-source-cancel-throw-keeps-live-prefix');
  await seed('cancel-partial-throw-input','orderBooks',persist({buy:[null,buy({sender:'amm'})],sell:[]}));
  await instanceCall('actual-source-splice-before-null-row-throw','1-7','cancelOrdersByCriteria',['outsider','1-7',{},true,true]);
  assert.equal(records.at(-1).result.outcome.thrown.message,"Cannot read properties of null (reading 'sender')");
  assert.deepEqual(records.at(-1).state.orderbookInstances[0].orderBooks['1-7'].buy,[null]);

 }

})().then(()=>{const output=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n';const suffix=flags.includes('--all-receivers')?'_receivers_v2':'';const destination=path.resolve(__dirname,'../fixtures/'+(flags.includes('--proof10')?'lower_spot_proof10':'lower_spot_source')+suffix+'.jsonl');if(flags.includes('--check'))assert.equal(fs.readFileSync(destination,'utf8'),output);if(flags.includes('--stdout')){process.stdout.write(output);return;}if(!flags.includes('--check'))fs.writeFileSync(destination,output);console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target||r.command.instance).length,version:lock?'9.1.2':'9.3.1',sha256:crypto.createHash('sha256').update(output).digest('hex'),summary:records.map(r=>({name:r.name,result:r.result,unhandled:r.state.unhandled}))},null,2));}).catch(error=>{console.error(error);process.exitCode=1;});
