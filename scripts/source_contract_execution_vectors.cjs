#!/usr/bin/env node
'use strict';
// Genuine archived CommonJS composition. No validator, placement, match,
// collateral movement or settlement result is supplied by this driver.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto');
const {createRequire} = require('node:module');
const {loadBigNumber, readPinnedSource} = require('./legacy_numeric_oracle.cjs');
const root = path.resolve(__dirname, '..'), BN = loadBigNumber();
const pins = {
  types:['legacy_types_source_types.txt','a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1'],
  txDecoder:['legacy_types_source_txDecoder.txt','a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076'],
  txEncoder:['legacy_types_source_txEncoder.txt','c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42'],
  base94:['legacy_base94_source.txt','0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb'],
  base256:['legacy_base256_source.txt','11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc'],
  converter:['legacy_base_converter_source.txt','a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62'],
  validity:['legacy_validity_source.txt','ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00'],
  property:['reference_fidelity/property.js','ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1'],
  clearlist:['legacy_clearlist_audit_reference.txt','fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1'],
  tally:['native_genesis/source/tally.js','cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef'],
  logic:['native_genesis/source/logic.js','f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d'],
  oracle:['legacy_oracle_source.txt','70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d'],
  dlcOracleBridge:['legacy_oracle_bridge_source.txt','954a7fbf30b66e52b587a2d7108e080c41da812acff77ccd457c6e547cb1587f'],
  orderbook:['legacy_orderbook_source.txt','c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98'],
  contractRegistry:['legacy_contract_registry_source.txt','d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121'],
  marginMap:['legacy_margin_map_source.txt','ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636'],
  clearing:['legacy_clearing_source.txt','612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528'],
  iou:['legacy_iou_source.txt','7ad5e87e4d72cf57c640c8c15564a3da21dfb4f2e692097ac8aab8215018cf96'],
  tradeHistoryManager:['legacy_trade_history_source.txt','670035f8153abda542c732deee0cf995fcd1e1e7c7b9f50464fce6ab1b5c1569'],
  options:['legacy_options_source.txt','02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703'],
  volumeIndex:['legacy_volume_index_source.txt','d4d4329254125ce8e4217d22b0ba57a1581c6279df999af60281b0365dffb08c'],
  channels:['legacy_channels_source.txt','b3fa22beab9a9fdb63246daa12582f86591a4217863240321f88fdff0160ec79'],
  consensus:['legacy_consensus_source.txt','af4c2fe7c37680be018dcb0df9f2cf8b348baac8aec1fe2335a220ce27dc0230'],
  amm:['legacy_amm_source.txt','bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1'],
  insurance:['insurance_owner_review/insurance.js','5abbe7934ca4c2f4180ed910989ae8f90d752bf9e17cb8944b8b49e33d03c39c'],
};
const sources = Object.fromEntries(Object.entries(pins).map(([key,[file,sha]]) =>
  [key,readPinnedSource(path.join(root,'fixtures',file),sha)]));
const args = process.argv.slice(2), moduleFlag = args.indexOf('--module-root');
if (moduleFlag >= 0 && !args[moduleFlag+1]) throw Error('--module-root requires a path');
const moduleRoot = moduleFlag >= 0 ? path.resolve(args[moduleFlag+1])
  : process.env.TRADELAYER_JS_ROOT || path.resolve(root,'../../../../tradelayer.js');
const external = createRequire(path.join(moduleRoot,'package.json'));
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT', B='1BitcoinEaterAddressDontSendf59kuE';
const fixedTime=1700000000000;
function tag(value, seen=new Set()) {
  if (value===undefined) return ['undefined'];
  if (value===null) return ['null'];
  if (typeof value==='number') {const buf=Buffer.alloc(8);buf.writeDoubleBE(value);return ['number',Number.isNaN(value)?'NaN':value===Infinity?'Infinity':value===-Infinity?'-Infinity':buf.toString('hex')];}
  if (typeof value==='string'||typeof value==='boolean') return [typeof value,value];
  if (typeof value==='bigint') return ['bigint',String(value)];
  if (BN.isBigNumber(value)) return ['bignumber',value.toString()];
  if (typeof value==='function') return ['function',value.name];
  if (seen.has(value)) return ['cycle'];
  seen.add(value);
  let result;
  if (Array.isArray(value)) result=['array',value.map(x=>tag(x,seen))];
  else if (Object.prototype.toString.call(value)==='[object Map]') result=['map',Array.from(value,([k,v])=>[tag(k,seen),tag(v,seen)])];
  else if (value instanceof Promise) result=['promise'];
  else result=['object',Object.entries(value).map(([k,v])=>[k,tag(v,seen)])];
  seen.delete(value); return result;
}
const hex = value => Buffer.from(JSON.stringify(tag(value)),'utf8').toString('hex');
const balance=available=>({amount:available,available,reserved:0,margin:0,vesting:0,channelBalance:0});
const body=(amount=1,sell=false,flags={}) => [1,Math.round((flags.price??100)*1e8).toString(36),Number(amount).toString(36),+sell,+!!flags.insurance,+!!flags.reduce,+!!flags.post,+!!flags.stop].join(',');
function copy(value,seen=new Map()){
  if(value===null||typeof value!=='object')return value;
  if(BN.isBigNumber(value))return new BN(value);
  if(seen.has(value))return seen.get(value);
  if(Object.prototype.toString.call(value)==='[object Map]'){const result=new Map();seen.set(value,result);for(const[k,v]of value)result.set(copy(k,seen),copy(v,seen));return result;}
  const result=Array.isArray(value)?[]:{};seen.set(value,result);
  for(const[k,v]of Object.entries(value))result[k]=copy(v,seen);return result;
}
function environment(scene) {
  const tables=new Map(), modules=new Map(), databaseHandles=new Map(), effects=[], instances=[], logs=[], unsupported=[];
  let serial=0, dbSerial=0, failure=null,chainHeight=10,randomState=1;
  const table=name=>{if(!tables.has(name)) tables.set(name,[]);return tables.get(name);};
  const clone=value=>structuredClone(value);
  const field=(obj,key)=>key.split('.').reduce((v,k)=>v?.[k],obj);
  const matches=(doc,query)=>Object.entries(query).every(([key,value])=>{
    if(key==='$or')return value.some(q=>matches(doc,q));
    if(key==='$and')return value.every(q=>matches(doc,q));
    const actual=field(doc,key);
    if(value&&typeof value==='object'&&!Array.isArray(value)) return Object.entries(value).every(([op,wanted])=>{
      if(op==='$gte')return actual>=wanted;if(op==='$lte')return actual<=wanted;
      if(op==='$gt')return actual>wanted;if(op==='$lt')return actual<wanted;
      if(op==='$ne')return actual!==wanted;if(op==='$in')return wanted.includes(actual);
      if(op==='$exists')return (actual!==undefined)===wanted;
      throw Error(`Unsupported storage query ${op}`);
    });
    return actual===value;
  });
  function dbCall(name,op,params,run) {
    const call={kind:'db',collection:name,op,method:['findOne','find','update','insert','remove'].includes(op)?`${op}Async`:op,args:clone(params)};effects.push(call);
    if(failure&&failure.collection===name&&failure.op===op) {
      if(failure.skip>0)failure.skip--;else {const message=failure.message;failure=null;call.thrown={name:'Error',message};throw Error(message);}
    }
    try {const result=run();call.returned=clone(result);return result;}catch(e){call.thrown={name:e.name,message:e.message};throw e;}
  }
  function setField(doc,key,value){const parts=key.split('.');let target=doc;for(const part of parts.slice(0,-1))target=target[part]??={};target[parts.at(-1)]=clone(value);}
  const database=name=>({
    async findOneAsync(query){return dbCall(name,'findOne',[query],()=>clone(table(name).find(row=>matches(row,query))||null));},
    async findAsync(query){return dbCall(name,'find',[query],()=>clone(table(name).filter(row=>matches(row,query))));},
    async updateAsync(query,update,options={}){return dbCall(name,'update',[...arguments],()=>{
      let rows=table(name), indexes=rows.flatMap((row,i)=>matches(row,query)?[i]:[]);
      if(!options.multi)indexes=indexes.slice(0,1);
      if(indexes.length===0&&options.upsert){rows.push({...clone(query),_id:query._id??`db-${++dbSerial}`});indexes=[rows.length-1];}
      for(const i of indexes){if(Object.keys(update).some(k=>k.startsWith('$'))){
        for(const [op,values]of Object.entries(update))for(const[k,v]of Object.entries(values)){
          if(op==='$set')setField(rows[i],k,v);else if(op==='$inc')setField(rows[i],k,(field(rows[i],k)||0)+v);else throw Error(`Unsupported storage update ${op}`);
        }
      }else rows[i]={...clone(update),_id:update._id??rows[i]._id};}
      return indexes.length;
    });},
    async insertAsync(doc){return dbCall(name,'insert',[doc],()=>{const value=clone(doc);value._id??=`db-${++dbSerial}`;if(table(name).some(row=>row._id===value._id))throw Error('uniqueViolated');table(name).push(value);return clone(value);});},
    async removeAsync(query,options={}){return dbCall(name,'remove',[...arguments],()=>{const rows=table(name);let removed=0;for(let i=rows.length-1;i>=0;i--)if(matches(rows[i],query)&&(options.multi||removed===0)){rows.splice(i,1);removed++;}return removed;});},
    async loadDatabase(){return dbCall(name,'loadDatabase',[],()=>undefined);},
    async get(key){return dbCall(name,'get',[key],()=>clone(table(name).find(row=>row._id===key)||null));},
    async put(key,value){return dbCall(name,'put',[key,value],()=>{const rows=table(name),i=rows.findIndex(row=>row._id===key);const row={_id:key,...clone(value)};if(i<0)rows.push(row);else rows[i]=row;return undefined;});},
  });
  // The pinned db.js factory is async; preserving that also preserves source
  // failures in methods that incorrectly dereference its promise directly.
  const storage={async getDatabase(name){
    effects.push({kind:'database',name,method:'getDatabase',args:[...arguments],returned:{database:name}});
    if(!databaseHandles.has(name))databaseHandles.set(name,database(name));
    return databaseHandles.get(name);
  }};
  const forbidden=name=>new Proxy(function unownedBoundary(){unsupported.push(name);throw Error(`Unowned boundary: ${name}`);},{get(_t,key){if(key==='then')return undefined;return forbidden(`${name}.${String(key)}`);}});
  class Clock extends Date {constructor(...values){super(...(values.length?values:[fixedTime]));}static now(){return fixedTime;}}
  const deterministicMath=Object.create(Math);
  deterministicMath.random=()=>{randomState=(Math.imul(randomState,1664525)+1013904223)>>>0;const value=randomState/4294967296;effects.push({kind:'random',returned:value});return value;};
  const context=vm.createContext({Buffer,Date:Clock,Math:deterministicMath,process:{env:{}},console:Object.fromEntries(['log','warn','error'].map(level=>[level,(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})])),setTimeout,clearTimeout});
  const adapters={
    'bignumber.js':BN,'crypto':crypto,'path':path,'util':require('node:util'),'fs':forbidden('fs'),
    'uuid':{v4(){const value=`source-contract-${++serial}`;effects.push({kind:'uuid',value});return value;}},
    './db':storage,'./activation':{getInstance(){return {async isTxTypeActive(type){effects.push({kind:'activation',method:'isTxTypeActive',args:[type],returned:!scene.inactive});return !scene.inactive;},getAdmin(){effects.push({kind:'activation',method:'getAdmin',args:[],returned:'admin'});return 'admin';},areActivationsAboveThreshold(){return false;}};}},
    './txUtils':new Proxy({async getBlockCount(...values){effects.push({kind:'rpc-clock',method:'TxUtils.getBlockCount',args:copy(values),returned:chainHeight});return chainHeight;}},{get(target,key){return Object.hasOwn(target,key)?target[key]:forbidden(`TxUtils.${String(key)}`);}}),'node-fetch':forbidden('fetch'),'litecoin':forbidden('Litecoin'),
  };
  for(const name of ['bitcoinjs-lib','tiny-secp256k1','bip32'])adapters[name]=external(name);
  adapters.bech32={bech32:forbidden('bech32')};
  const unowned=['vesting','txIndex','scaling','vaults','procedural','bitvmCache','bitvmBundle','bitvmRisk','experimental/binohash/binohashAdapter'];
  for(const name of unowned)adapters[`./${name}`]=forbidden(name);
  function keyOf(key){return key.startsWith('./')?key.replace(/\.js$/,''):key;}
  function load(name){
    if(modules.has(name))return modules.get(name).exports;
    if(!sources[name])throw Error(`No pinned source ${name}`);
    const module={exports:{}};modules.set(name,module);effects.push({kind:'module',name});
    const wrapper=vm.runInContext(`(function(exports, require, module, __filename, __dirname){\n${sources[name]}\n})`,context,{filename:`pinned/${name}.js`,timeout:3000});
    wrapper(module.exports,key=>{
      const normalized=keyOf(key);
      if(normalized==='bigint-base-converter')return load('converter');
      if(Object.hasOwn(adapters,normalized))return adapters[normalized];
      if(normalized.startsWith('./')&&sources[normalized.slice(2)])return load(normalized.slice(2));
      throw Error(`Unmapped source import ${name}: ${key}`);
    },module,`pinned/${name}.js`,'pinned');
    return module.exports;
  }
  // Imported persisted state is input, not a supplied method result.
  const meta={id:1,ticker:'TEST',issuer:A,native:false,underlyingOracleId:1,onChainData:[],notionalPropertyId:0,notionalValue:scene.notional??1,collateralPropertyId:7,leverage:10,inverse:!!scene.inverse,whitelist:0};
  if(scene.basicReward)meta.issuerAddress=A;
  table('contractList').push({_id:'contract-1',id:1,type:'contractSeries',data:meta});
  table('propertyList').push({_id:'propertyIndex',value:JSON.stringify([1,3,7].map(id=>[id,{ticker:`P${id}`,totalInCirculation:1000000,type:1,whitelistId:0,issuer:A}]))});
  table('tallyMap').push({_id:'tallyMap',data:JSON.stringify([[A,{7:balance(scene.funds??1000)}],[B,{7:balance(scene.funds??1000)}]])});
  table('oracleList').push({_id:'oracle-1',ticker:'TEST',adminAddress:A,backupAddress:B});
  if(!scene.noOracleData)table('oracleData').push({_id:'oracle-1-9',type:'oracle',oracleId:1,blockHeight:9,data:{price:100,high:100,low:100,close:100}});
  for(const address of [A,B])table('attestations').push({_id:`country-${address}`,data:{address,listId:0,status:'active',data:'CA',blockHeight:1}});
  if(scene.initialEntries){
    const quantity=scene.initialContracts??1,margin=10*quantity;
    const balances=JSON.parse(table('tallyMap')[0].data);
    for(const[,properties]of balances){properties[7].available-=margin;properties[7].margin=margin;}
    table('tallyMap')[0].data=JSON.stringify(balances);
    table('marginMaps').push({_id:JSON.stringify({seriesId:1}),value:JSON.stringify([[A,{contracts:quantity,margin,unrealizedPNL:0,address:A,avgPrice:scene.initialEntries[0]}],[B,{contracts:-quantity,margin,unrealizedPNL:0,address:B,avgPrice:scene.initialEntries[1]}]])});
  }
  if(scene.basicReward)table('attestations').push({_id:'basic-reward-issuer',data:{address:A,listId:1,status:'active',data:'issuer-reward',blockHeight:1}});
  // main.js imports Validity before Tally/Margin/Clearing/Contracts/Volume,
  // and finally Types/Logic. Daemon startup and unrelated imported services
  // are explicit boundaries, so this is that reachable pinned subgraph order.
  const entry=scene.entry||'main-subgraph';
  effects.push({kind:'entry',entry,preload:scene.preload});
  if(scene.preload)load(scene.preload);
  if(entry==='main-subgraph')for(const name of ['validity','tally','marginMap','clearing','property','contractRegistry','volumeIndex','oracle','orderbook','txEncoder'])load(name);
  const types=load('types'),logic=load('logic'),Orderbook=load('orderbook');
  function observe(target,method,instance=false){const original=target[method];if(typeof original!=='function')throw Error(`Missing observed method ${method}`);target[method]=async function(...values){
    const call={kind:'source',target:instance?'Orderbook.instance':'Orderbook',method,args:values.map(value=>value instanceof Orderbook?{orderbookInstance:instanceId(value)}:copy(value))};
    if(instance)call.instance=instanceId(this);effects.push(call);
    try{const result=await original.apply(this,values);call.returned=result instanceof Orderbook?{orderbookInstance:instanceId(result)}:copy(result);return result;}catch(error){call.thrown={name:error.name,message:error.message};throw error;}
  };}
  function instanceId(instance){let index=instances.indexOf(instance);if(index<0){index=instances.length;instances.push(instance);}return index+1;}
  for(const method of ['getOrderbookInstance','queueOnChainContractOrder','processQueuedOnChainOrdersForBlock'])observe(Orderbook,method);
  for(const method of ['addContractOrder','processContractMatches'])observe(Orderbook.prototype,method,true);
  // These observers return the exact original value/promise. They record call
  // arguments only, preserving sync methods and un-awaited source writes.
  for(const [name,method,prototype]of [
    ['contractRegistry','moveCollateralToReserve'],['contractRegistry','moveCollateralToMargin'],
    ['marginMap','setInitialMargin',true],['marginMap','updateContractBalancesWithMatch',true],
    ['marginMap','settlePNL',true],['marginMap','realizePnl',true],
    ['clearing','recordTrade'],['clearing','isPriceUpdatedForBlockHeight'],['volumeIndex','saveVolumeDataById'],
    ['iou','addIouClaims'],['iou','addProfit'],['iou','addLoss'],
    ['tally','updateFeeCache'],['tally','accrueFee'],['insurance','deposit',true],
  ]){
    const target=prototype?load(name).prototype:load(name),original=target[method];
    assert.equal(typeof original,'function',`${name}.${method}`);
    target[method]=function(...values){effects.push({kind:'source',target:name,method,args:copy(values)});return original.apply(this,values);};
  }
  function state(){
    const Clearing=load('clearing'),Oracle=load('oracle'),Tally=load('tally');
    const parseRows=name=>table(name).map(row=>({id:row._id,value:JSON.parse(row.value??row.data)}));
    return {loadedModules:[...modules.keys()],tables:Array.from(tables,([name,rows])=>({name,rows:clone(rows)})),instances:instances.map((value,index)=>({id:index+1,...value})),
      views:{books:table('orderBooks').filter(row=>row.value!==undefined).map(row=>({id:row._id,value:JSON.parse(row.value)})),tally:parseRows('tallyMap'),marginMaps:parseRows('marginMaps')},
      clearing:{blockTrades:Clearing.blockTrades,deleverageTrades:Clearing.deleverageTrades,liquidationRecords:Clearing.liquidationRecords,latestOracleMarkById:Clearing.latestOracleMarkById,latestNativeMarkById:Clearing.latestNativeMarkById},
      oracle:{instance:Oracle.instance?{...Oracle.instance}:null,lastOracleUpdateBlock:Oracle.lastOracleUpdateBlock},insurance:load('insurance').instances,
      tally:{instance:Tally.instance?{...Tally.instance}:null,addresses:Tally.addresses,feeAccrualBuffer:Tally._feeAccrualBuffer},unsupported:unsupported.slice()};
  }
  return {types,logic,Orderbook,effects,logs,state,table,load,setHeight(height){chainHeight=height;},setFailure(value){failure={skip:0,...value};}};
}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
const scenes=[
  {name:'ordinary-unmatched',actions:[{raw:body(),sender:A},{drain:10}]},
  {name:'ordinary-matched',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'types-entry-matched',entry:'types-first',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'volume-preload-matched',preload:'volumeIndex',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'raw-reduce-shifts-liquidation',actions:[{raw:body(1,false,{reduce:true}),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'raw-post-shifts-reduce',actions:[{raw:body(1,false,{post:true}),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'raw-stop-shifts-post',actions:[{raw:body(1,false,{stop:true}),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'insurance-ignored-placement',actions:[{raw:body(1,false,{insurance:true}),sender:A},{drain:10}]},
  {name:'zero-quantity',actions:[{raw:body(0),sender:A},{drain:10}]},
  {name:'negative-quantity',actions:[{raw:body(-1),sender:A},{drain:10}]},
  {name:'oversubscribed-queue',funds:10,actions:[{raw:body(),sender:A},{raw:body(),sender:A},{drain:10}]},
  {name:'validator-shrinks-quantity',funds:15,actions:[{raw:body(2),sender:A},{drain:10}]},
  {name:'validator-rejects-zero-after-shrink',funds:5,actions:[{raw:body(),sender:A},{drain:10}]},
  {name:'inactive-raw',inactive:true,actions:[{raw:body(),sender:A},{drain:10}]},
  {name:'future-height-queue',actions:[{raw:body(),sender:A,block:11},{drain:10},{drain:11}]},
  {name:'partial-fill',actions:[{raw:body(3),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'self-cross',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:A},{drain:10}]},
  {name:'close-positions',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10},{raw:body(1,true,{price:102}),sender:A,block:11},{raw:body(1,false,{price:102}),sender:B,block:11},{drain:11}]},
  {name:'flip-positions',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10},{raw:body(2,true,{price:102}),sender:A,block:11},{raw:body(2,false,{price:102}),sender:B,block:11},{drain:11}]},
  {name:'inverse-matched',inverse:true,actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'nonunit-notional-matched',notional:2,actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'late-volume-write-failure-retry',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{failure:{collection:'volumeIndex',op:'update',message:'injected-volume-write'}},{drain:10},{drain:10}]},
  {name:'late-history-write-failure',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{failure:{collection:'tradeHistory',op:'update',message:'injected-history-write'}},{drain:10}]},
  {name:'queue-write-failure-prefix',actions:[{failure:{collection:'orderBooks',op:'update',skip:1,message:'injected-active-pairs-write'}},{raw:body(),sender:A},{drain:10}]},
  {name:'source-zero-rounded-volume-prefix',notional:1e-8,actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
  {name:'same-block-mutable-mark-cache',actions:[{markProbe:{contractId:1,height:9}},{publish:{price:102,height:10}},{raw:body(2,false,{price:102}),sender:A},{raw:body(1,true,{price:102}),sender:B},{raw:body(1,true,{price:102}),sender:B},{drain:10}]},
  {name:'ordinary-matched-quantity-two',actions:[{raw:body(2),sender:A},{raw:body(2,true),sender:B},{drain:10}]},
  {name:'matched-fee-flush',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10},{flushFees:10}]},
  {name:'tally-preload-matched-fee-flush',preload:'tally',actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10},{flushFees:10}]},
  {name:'imported-partial-close-no-mark-iou-profit',noOracleData:true,initialContracts:2,initialEntries:[0.9,1.1],actions:[{raw:body(1,true,{price:1}),sender:A},{raw:body(1,false,{price:1}),sender:B},{drain:10}]},
  {name:'imported-partial-close-no-mark-iou-loss',noOracleData:true,initialContracts:2,initialEntries:[1.1,0.9],actions:[{raw:body(1,true,{price:1}),sender:A},{raw:body(1,false,{price:1}),sender:B},{drain:10}]},
  {name:'imported-issuerAddress-basic-reward',basicReward:true,actions:[{raw:body(),sender:A},{raw:body(1,true),sender:B},{drain:10}]},
];
function compactSummary(state,sourceOrders){
  const tables=new Map(state.tables.map(table=>[table.name,table.rows]));
  const balances=new Map(state.views.tally[0].value),positions=new Map(state.views.marginMaps[0]?.value||[]);
  let buffered=new BN(0),value=new BN(0),stash=new BN(0);
  for(const byBlock of state.tally.feeAccrualBuffer?.values()||[])for(const entry of byBlock.values())buffered=buffered.plus(entry.amount);
  for(const row of tables.get('feeCache')||[])if(row._id==='7-1'){value=value.plus(row.value||0);stash=stash.plus(row.stash||0);}
  const queued=tables.get('orderBooks')?.find(row=>row._id==='queue-1')?.orders||[];
  return {actors:[['A',A],['B',B]].map(([name,address])=>({name,address,...balances.get(address)?.[7],contracts:positions.get(address)?.contracts??0,positionMargin:positions.get(address)?.margin??0})),orders:sourceOrders,
    queuedTxids:queued.map(entry=>entry.txid),fees:{buffered:buffered.toFixed(),persistedValue:value.toFixed(),persistedStash:stash.toFixed(),bufferedPlusPersisted:buffered.plus(value).plus(stash).toFixed()},
    tradeHistoryRows:tables.get('tradeHistory')?.length||0};
}
function verifyScene(scene,observations){
  const last=observations.at(-1),lastState=last.state;
  const rows=(state,name)=>state.tables.find(t=>t.name===name)?.rows||[];
  const doc=(state,name,id)=>rows(state,name).find(row=>row._id===id);
  const queue=state=>doc(state,'orderBooks','queue-1')?.orders||[];
  const book=state=>state.views.books.find(row=>row.id==='1')?.value;
  const balances=state=>new Map(state.views.tally[0].value);
  const position=(state,address)=>new Map(state.views.marginMaps[0]?.value||[]).get(address);
  for(let i=0;i<observations.length;i++){
    const {outcome,state}=observations[i];assert.deepEqual(state.unsupported,[],`${scene.name}: unowned runtime boundary`);
    if(scene.actions[i].raw!==undefined){
      assert.equal(outcome.params.valid,!['inactive-raw','validator-rejects-zero-after-shrink'].includes(scene.name),`${scene.name}: actual validator verdict`);
    }
  }
  const thrown=observations.filter(o=>o.outcome.thrown);
  assert.equal(thrown.length,['late-volume-write-failure-retry','late-history-write-failure','queue-write-failure-prefix','source-zero-rounded-volume-prefix','volume-preload-matched','matched-fee-flush'].includes(scene.name)?1:0,`${scene.name}: actual thrown count ${JSON.stringify(thrown.map(row=>row.outcome.thrown))}`);
  if(scene.name==='ordinary-unmatched'){
    assert.equal(queue(observations[0].state).length,1);assert.equal(balances(observations[0].state).get(A)[7].reserved,0);
    assert.equal(book(lastState).buy[0].stop,10);assert.equal(book(lastState).buy[0].amount,1);
    assert.equal(balances(lastState).get(A)[7].reserved,10);assert.equal(queue(lastState).length,0);
  }
  if(['ordinary-matched','types-entry-matched'].includes(scene.name)){
    assert.equal(position(lastState,A).contracts,1);assert.equal(position(lastState,B).contracts,-1);
    assert.equal(position(lastState,A).margin,10);assert.equal(balances(lastState).get(A)[7].available,989.99975);
    assert.equal(rows(lastState,'tradeHistory').length,1);assert.equal(doc(lastState,'volumeIndex',1).value.ltcVolume,0.1);
    const sellerMove=last.effects.find(effect=>effect.kind==='source'&&effect.target==='contractRegistry'&&effect.method==='moveCollateralToMargin'&&effect.args[0]===B);
    assert.equal(sellerMove.args.length,13);assert.equal(sellerMove.args[12].contracts,0);
    const bookWrite=last.effects.findLastIndex(effect=>effect.kind==='db'&&effect.collection==='orderBooks'&&effect.op==='update'&&effect.args[0]._id==='1');
    const moveMargin=last.effects.findIndex(effect=>effect.kind==='source'&&effect.target==='contractRegistry'&&effect.method==='moveCollateralToMargin');
    assert(bookWrite>=0&&bookWrite<moveMargin,'source saves matched book before financial writes');
  }
  if(scene.name==='raw-reduce-shifts-liquidation'){
    assert.equal(queue(observations[0].state)[0].params.isLiq,true);
    assert.equal(balances(lastState).get(A)[7].amount,899.99975);assert.equal(balances(lastState).get(A)[7].reserved,10);
    assert.equal(position(lastState,A).margin,0);assert.equal(position(lastState,A).unrealizedPNL,-100);
  }
  if(scene.name==='raw-post-shifts-reduce'){assert.equal(queue(observations[0].state)[0].params.reduce,true);assert.equal(queue(observations[0].state)[0].params.post,false);}
  if(scene.name==='raw-stop-shifts-post'){assert.equal(queue(observations[0].state)[0].params.post,true);assert.equal(balances(lastState).get(A)[7].amount,1000.00025);assert.equal(balances(lastState).get(B)[7].amount,999.9995);}
  if(scene.name==='insurance-ignored-placement'){assert.equal(queue(observations[0].state)[0].params.insurance,true);assert.equal(Object.hasOwn(book(lastState).buy[0],'insurance'),false);}
  if(scene.name==='zero-quantity'){assert.equal(observations[0].outcome.params.amount,0);assert.equal(book(lastState),undefined);assert.equal(queue(lastState).length,0);}
  if(scene.name==='negative-quantity'){assert.equal(book(lastState).buy[0].amount,-1);assert.equal(balances(lastState).get(A)[7].reserved,10);}
  if(scene.name==='oversubscribed-queue'){assert.equal(queue(observations[1].state).length,2);assert.equal(book(lastState).buy.length,1);assert.equal(balances(lastState).get(A)[7].available,0);assert.equal(queue(lastState).length,0);}
  if(scene.name==='validator-shrinks-quantity')assert.equal(observations[0].outcome.params.amount,1);
  if(scene.name==='validator-rejects-zero-after-shrink')assert.equal(observations[0].outcome.params.amount,0);
  if(scene.name==='future-height-queue'){assert.equal(queue(observations[1].state).length,1);assert.equal(balances(observations[1].state).get(A)[7].reserved,0);assert.equal(book(lastState).buy[0].blockTime,11);}
  if(scene.name==='partial-fill'){assert.equal(book(lastState).buy[0].amount,2);assert.equal(book(lastState).buy[0].initMargin,'20.00000000');assert.equal(balances(lastState).get(A)[7].reserved,20);}
  if(scene.name==='self-cross'){assert.equal(book(lastState).buy.length,0);assert.equal(book(lastState).sell.length,1);assert.equal(balances(lastState).get(A)[7].reserved,20);assert.equal(rows(lastState,'tradeHistory').length,0);}
  if(scene.name==='close-positions'){assert.equal(position(lastState,A).contracts,0);assert.equal(position(lastState,B).contracts,0);assert.equal(balances(lastState).get(A)[7].margin,0);assert.equal(position(lastState,A).realizedPNL,2);}
  if(scene.name==='flip-positions'){assert.equal(position(lastState,A).contracts,-1);assert.equal(position(lastState,B).contracts,1);assert.equal(balances(lastState).get(A)[7].reserved,10.2);assert.equal(balances(lastState).get(A)[7].margin,20.2);assert.equal(position(lastState,A).margin,10);}
  if(scene.name==='inverse-matched')assert.equal(position(lastState,A).margin,0.001);
  if(scene.name==='nonunit-notional-matched')assert.equal(position(lastState,A).margin,20);
  if(scene.name==='late-volume-write-failure-retry'){
    const prefix=observations[3].state;assert.equal(queue(prefix).length,2);assert.equal(position(prefix,A).contracts,1);assert.equal(rows(prefix,'tradeHistory').length,1);
    assert.equal(position(lastState,A).contracts,2);assert.equal(position(lastState,B).contracts,-2);assert.equal(queue(lastState).length,0);
  }
  if(scene.name==='late-history-write-failure'){assert.equal(position(lastState,A).contracts,1);assert.equal(rows(lastState,'tradeHistory').length,0);assert.equal(queue(lastState).length,2);}
  if(scene.name==='queue-write-failure-prefix'){assert.equal(queue(lastState).length,1);assert.equal(doc(lastState,'orderBooks','activePairs'),undefined);assert.equal(balances(lastState).get(A)[7].reserved,0);}
  if(scene.name==='source-zero-rounded-volume-prefix'){assert.equal(last.outcome.thrown.message,'');assert.equal(position(lastState,A).contracts,1);assert.equal(rows(lastState,'tradeHistory').length,1);assert.equal(queue(lastState).length,2);}
  if(scene.name==='same-block-mutable-mark-cache'){
    assert.equal(observations[0].outcome.returned.thisPrice,100);
    const settle=last.effects.filter(effect=>effect.kind==='source'&&effect.target==='marginMap'&&effect.method==='settlePNL');
    assert.deepEqual(settle.map(effect=>effect.args[2]),[100,100,102,102]);
    assert.equal(position(lastState,A).contracts,2);assert.equal(balances(lastState).get(A)[7].amount,997.9995);
  }
  if(scene.name==='volume-preload-matched'){assert.equal(last.outcome.thrown.message,'VolumeIndex.getContractUnitLTCValue is not a function');assert.equal(position(lastState,A).contracts,1);assert.equal(queue(lastState).length,2);}
  if(scene.name==='ordinary-matched-quantity-two'){
    assert.equal(position(lastState,A).contracts,2);assert.equal(position(lastState,B).contracts,-2);
    assert.equal(balances(lastState).get(A)[7].available,979.9995);assert.equal(balances(lastState).get(B)[7].margin,20);
    assert.equal(compactSummary(lastState,[]).fees.buffered,'0.001');
  }
  if(scene.name==='matched-fee-flush'){
    assert.equal(last.outcome.thrown.message,'ContractRegistry.isNativeContract is not a function');
    assert.equal(compactSummary(lastState,[]).fees.buffered,'0.0005');
    assert.equal(rows(lastState,'insurance').length,0);
  }
  if(scene.name==='tally-preload-matched-fee-flush'){
    assert.equal(compactSummary(lastState,[]).fees.buffered,'0');assert.equal(compactSummary(lastState,[]).fees.persistedStash,'0.00025');
    assert.equal(rows(lastState,'insurance').find(row=>row.key==='1-oracle').value.balances[0].amountAvailable,'0.00025');
  }
  if(scene.name==='imported-partial-close-no-mark-iou-profit'){
    assert.equal(doc(lastState,'iou','claims:1:7').claims[A],'0.1');assert.equal(doc(lastState,'iou','claims:1:7').claims[B],'0.1');
    assert.equal(doc(lastState,'iou','1:7').amount,'-0.2');assert.equal(doc(lastState,'iou','1:7').blockProfits,'0.2');
    assert.equal(rows(lastState,'iou').filter(row=>row.type==='audit').length,2);
  }
  if(scene.name==='imported-partial-close-no-mark-iou-loss'){
    assert.equal(doc(lastState,'iou','1:7').amount,'0.2');assert.equal(doc(lastState,'iou','1:7').blockLosses,'0.2');
    assert.equal(doc(lastState,'iou','claims:1:7'),undefined);
  }
  if(scene.name==='imported-issuerAddress-basic-reward'){
    assert.equal(balances(lastState).get(A)[3].available,0.00000875);assert.equal(balances(lastState).get(B)[3].available,0.00000875);
    assert.equal(rows(lastState,'tallyMapDelta').filter(row=>row.data.type==='baselineLiquidityReward').length,2);
  }
}
async function main(){
  const rows=[],summary=[];
  for(const scene of scenes){
    const e=environment(scene), initial=e.state(),observations=[],sourceOrders=[];
    rows.push(['V',`${scene.name}/initial`,hex({op:'reset',scene}),hex({returned:undefined}),hex(initial),hex(e.effects.splice(0)),JSON.stringify(compactSummary(initial,sourceOrders))].join('\t'));
    for(let i=0;i<scene.actions.length;i++){
      const action=scene.actions[i], txid=(i+1).toString(16).padStart(64,'0');let outcome,params;
      e.setHeight(action.block??action.drain??action.publish?.height??action.markProbe?.height??action.flushFees??10);
      try{
        if(action.raw!==undefined){params=await e.types.decodePayload(txid,18,'tl',action.raw,action.sender,null,0,0,action.block??10);let returned;if(params.valid===true)returned=await e.logic.typeSwitch(18,params);outcome={params,returned};}
        else if(action.drain!==undefined)outcome={returned:await e.Orderbook.processQueuedOnChainOrdersForBlock(action.drain)};
        else if(action.failure!==undefined){e.setFailure(action.failure);outcome={returned:undefined};}
        else if(action.markProbe!==undefined)outcome={returned:await e.load('clearing').isPriceUpdatedForBlockHeight(action.markProbe.contractId,action.markProbe.height)};
        else if(action.publish!==undefined){const{price,height}=action.publish;outcome={returned:await e.load('oracle').publishData(1,price,price,price,price,height)};}
        else if(action.flushFees!==undefined)outcome={returned:await e.load('tally').flushQueuedFeeAccruals(action.flushFees)};
        else throw Error('Unknown command');
      }catch(error){outcome={params,thrown:{name:error.name,message:error.message}};}
      await flush();
      const sourceLogs=e.logs.splice(0).filter(x=>x.level!=='log');
      outcome.sourceLogs=sourceLogs;
      const snapshot=e.state();
      const effects=e.effects.splice(0);
      if(params)sourceOrders.push({txid,sender:action.sender,block:action.block??10,valid:params.valid,amount:params.amount,price:params.price,decodedFlags:{sell:params.sell,insurance:params.insurance,reduce:params.reduce,post:params.post,stop:params.stop},queuedFlags:effects.find(effect=>effect.kind==='source'&&effect.method==='queueOnChainContractOrder')?.args[2]});
      rows.push(['V',`${scene.name}/${i+1}`,hex({...action,txid,...(action.raw===undefined?{}:{wire:`tli${action.raw}`,type:18,marker:'tl'})}),hex(outcome),hex(snapshot),hex(effects),JSON.stringify(compactSummary(snapshot,sourceOrders))].join('\t'));
      observations.push({outcome,state:copy(snapshot),effects:copy(effects)});
      summary.push({scene:scene.name,action:i+1,outcome,views:snapshot.views,unsupported:snapshot.unsupported});
    }
    verifyScene(scene,observations);
  }
  const output='# Actual pinned source dca0888b827c5bc28fa9189bc9e1d507f5c5670c; scripts/source_contract_execution_vectors.cjs\n'
    +'# V\tscene/action\tcommand diagnostic JSON hex\toutcome diagnostic JSON hex\tstate diagnostic JSON hex\tordered effects diagnostic JSON hex\tcompact summary JSON\n'+rows.join('\n')+'\n';
  const target=path.join(root,'fixtures/source_contract_execution_vectors.tsv');
  if(args.includes('--write'))fs.writeFileSync(target,output);else assert.equal(fs.readFileSync(target,'utf8').replace(/\r\n/g,'\n'),output);
  if(args.includes('--summary'))console.log(JSON.stringify(summary,null,2));
  console.log(`PASS ${scenes.length} actual composed contract scenes / ${rows.length} observations`);
}
main().catch(error=>{console.error(error);process.exitCode=1;});
