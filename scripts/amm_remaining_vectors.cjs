#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),root=process.env.PROOF_REPO||stage,flags=process.argv.slice(2),numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
if(flags.includes('--lock')){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(sha(raw),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const m={exports:{}};vm.runInNewContext(raw.toString(),{module:m,exports:m.exports});BN=m.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(a,b){assert.equal(code.split(a).length,2,'observer anchor '+a);code=code.replace(a,b);}
replace('const pins = {',"const pins = {activation:['native_genesis/source/activation.js','268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e'],");
const start=code.indexOf("'./db':storage,'./activation':"),end=code.indexOf("    './txUtils':",start);assert.ok(start>=0&&end>start);code=code.slice(0,start)+"'./db':storage,'./client':forbidden('ClientWrapper'),\n"+code.slice(end);
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('  let serial=0,','  const ammInstances=[];function ammId(v){let i=ammInstances.indexOf(v);if(i<0){i=ammInstances.length;ammInstances.push(v);}return i+1;}\n  let serial=0,');
replace('    return module.exports;\n  }\n  // Imported persisted state',`    if(name==='amm'){
      const SourceConstructor=module.exports;const observed=new Proxy(SourceConstructor,{construct(target,args,newTarget){const receiver=Reflect.construct(target,args,newTarget===observed?target:newTarget);assert.equal(Object.getPrototypeOf(receiver),target.prototype);assert.equal(receiver.constructor,target);ammId(receiver);return receiver;}});module.exports=observed;
    }
    return module.exports;
  }
  // Imported persisted state`);
replace('    return module.exports;\n  }\n  // Imported persisted state',`    if(name==='orderbook'){
 const SourceConstructor=module.exports;const observed=new Proxy(SourceConstructor,{construct(target,args,newTarget){const receiver=Reflect.construct(target,args,newTarget===observed?target:newTarget);assert.equal(Object.getPrototypeOf(receiver),target.prototype);assert.equal(receiver.constructor,target);instanceId(receiver);return receiver;}});module.exports=observed;
 }
    return module.exports;
  }
  // Imported persisted state`);
replace('return {types,logic,Orderbook,effects','return {ammInstances,ammId,instanceId,context,tables,storage,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
replace("const parseRows=name=>table(name).map", "const parseRows=name=>(tables.get(name)||[]).map");
replace("views:{books:table('orderBooks').filter", "views:{books:(tables.get('orderBooks')||[]).filter");
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',records=[];let e,lastParams;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.valueOf()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,x])=>[k,tag(x)])];}
function copied(v){return Object.prototype.toString.call(v)==='[object Error]'?{name:v.name,message:v.message}:e.copy(v);}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('contractRegistry'),A=e.load('activation');return{books:e.state().instances,tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyInitialized:!!P.instance,propertyCache:P.instance?.propertyIndex??new Map(),propertyAmms:P.instance?.ammIndex??new Map(),propertyRegistryBound:!!e.context.PropertyRegistry,contractInitialized:!!C.instance,contractSeries:C.instance?.contractSeries??new Map(),activation:A.instance?{...A.instance}:undefined,ammInstances:e.ammInstances.map((value,index)=>({id:index+1,...value})),lastParams,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned dependency');records.push({name,command:copied(command),result:copied(result),state:snapshot,effects:e.copy(e.effects.filter(v=>['database','db','uuid','random','rpc-clock','source-function'].includes(v.kind)))});}
function observe(owner,method,label,instance=false){const original=owner[method];assert.equal(typeof original,'function',label);owner[method]=function(...args){const event={kind:'source-function',target:label,...(instance?{instance:e.ammId(this)}:{}),args:copied(args)};e.effects.push(event);let result;try{result=Reflect.apply(original,this,args);}catch(error){event.thrown={name:error.name,message:error.message};throw error;}if(result&&typeof result.then==='function')result.then(value=>{event.returned=copied(value);},error=>{event.thrown={name:error.name,message:error.message};});else event.returned=copied(result);return result;};}
async function reset(name,options={}){
 e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally'}:options.entry==='volume-first'?{preload:'volumeIndex'}:options);lastParams=undefined;
 const rows=JSON.parse(e.table('tallyMap')[0].data);for(const[,properties]of rows)properties[1]=structuredClone(properties[7]);e.table('tallyMap')[0].data=JSON.stringify(rows);
 e.table('contractList')[0].data.ammPool=options.contractPool??{position:0,maxPosition:100,maxQuoteSize:10,contractType:1,lpAddresses:{},ammOrders:[]};
 const activation=e.load('activation').getInstance(),registry=activation.initializeTxRegistry();registry[10].active=!options.inactive;registry[10].activationBlock=1;e.table('activations').push({_id:'activationsList',value:JSON.stringify(registry)});
 const Pool=e.load('amm');if(options.propertyRegistry)e.context.PropertyRegistry=e.load('property');
 if(options.propertyPool){const pool=new Pool(0,100,10,0);e.load('property').getInstance().ammIndex.set('1-7',pool);}
 if(options.poolInput)new Pool(...options.poolInput);
 if(options.noAmmPool)delete e.table('contractList')[0].data.ammPool;
 if(options.books)e.table('orderBooks').splice(0,Infinity,...structuredClone(options.books));
 observe(e.logic,'AMMPool','Logic.AMMPool');observe(e.load('contractRegistry'),'getAMM','Contract.getAMM');observe(e.load('contractRegistry'),'getContractInfo','Contract.getContractInfo');observe(e.load('property'),'getAMM','Property.getAMM');observe(e.load('tally'),'updateBalance','Tally.updateBalance');observe(e.load('activation').prototype,'isTxTypeActive','Activation.isTxTypeActive');
 for(const method of ['addCapital','redeemCapital','quoteContractOrders'])observe(Pool.prototype,method,'AMM.'+method,true);
 for(const method of ['calculateOrderSize','calculateRedemptionValue','generateOrders','generateTokenOrders','updateOrdersForAllContractAMMs'])observe(Pool,method,'AMMStatic.'+method);
 for(const method of ['loadOrderBook','cancelOrdersByCriteria','insertOrder','saveOrderBook']){const original=e.Orderbook.prototype[method];e.Orderbook.prototype[method]=function(...args){const event={kind:'source-function',target:'Orderbook.'+method,instance:e.instanceId(this),args:copied(args)};e.effects.push(event);let result;try{result=Reflect.apply(original,this,args);}catch(error){event.thrown={name:error.name,message:error.message};throw error;}if(result&&typeof result.then==='function')result.then(value=>event.returned=copied(value),error=>event.thrown={name:error.name,message:error.message});else event.returned=copied(result);return result;};}
 await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});
}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,payload,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,10,'tl',payload,A,null,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(10,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure){e.effects.length=0;const input=copied(args);if(failure)fail(failure);const owners={Logic:e.logic,Property:e.load('property'),Contract:e.load('contractRegistry'),AMMStatic:e.load('amm'),Clearing:e.load('clearing')},[ns,method]=target.split('.');let transport='Immediate',outcome;try{const value=Reflect.apply(owners[ns][method],owners[ns],args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args:input,failure},{transport,outcome});}
async function poolCall(name,id,method,args=[],failure){e.effects.length=0;if(failure)fail(failure);const input=copied(args);let transport='Pending',outcome;try{outcome={returned:await Reflect.apply(e.ammInstances[id-1][method],e.ammInstances[id-1],args)};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{pool:{id,method,args:input},failure},{transport,outcome});}

async function quote(name,id,args){e.effects.length=0;let transport='Immediate',outcome;try{const value=e.ammInstances[id-1].quoteContractOrders(...args);outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{quote:{id,args}},{transport,outcome});}
const STATIC_FIELDS=['position','maxPosition','maxQuoteSize','contractType','lpShares','lpAddresses','ammOrders','strategyBlob'];
const METHODS=['insertCapital','redeemCapital','requestLiquidity','runOptionStrategy','buildStraddle','buildIronFly','buildCalendar','calculateTotalPosition','getLPAddresses','getAMMOrdersAndPositions','generateOrdersForInverse','generateOrdersForLinear'];
const inventory={sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',sourceSha256:sha(fs.readFileSync(path.join(root,'fixtures/legacy_amm_source.txt'))),methods:{},staticFields:STATIC_FIELDS,profile:'ordinary own class data only; no method/prototype replacement; constructors do not initialize class fields'};
let staticTallyBound=false;
const sourceState=state;state=function(){const names=[...e.tables.keys()],v=sourceState();assert.deepEqual([...e.tables.keys()],names,'audit snapshot never creates collections');const Pool=e.load('amm');return{...v,tokenOrderErrors:e.logs.filter(x=>x.level==='error'&&String(x.values[0]).startsWith('Error placing orders for pair ')).map(x=>String(x.values[0])),statics:Object.fromEntries(Object.keys(Pool).filter(key=>STATIC_FIELDS.includes(key)).map(key=>[key,Pool[key]])),staticTallyBound};};
const originalReset=reset;reset=async function(name,options={}){staticTallyBound=false;await originalReset(name,options);const Pool=e.load('amm');for(const method of METHODS){const descriptor=Object.getOwnPropertyDescriptor(Pool,method);assert.equal(typeof descriptor.value,'function');inventory.methods[method]={sha256:sha(descriptor.value.toString()),async:Object.prototype.toString.call(descriptor.value)==='[object AsyncFunction]'};observe(Pool,method,'AMMStatic.'+method);}for(const key of STATIC_FIELDS)assert.equal(Object.getOwnPropertyDescriptor(Pool,key),undefined,'instance constructor does not initialize static '+key);};
async function configure(name,fields){for(const[key,value]of Object.entries(fields)){assert.ok(STATIC_FIELDS.includes(key));Object.defineProperty(e.load('amm'),key,{value:copied(value),writable:true,enumerable:true,configurable:true});}e.effects.length=0;await flush();emit(name,{configureStatic:copied(fields)},{transport:'Immediate',outcome:{returned:undefined}});}
async function bindStaticTally(name){e.context.TallyMap=e.load('tally');staticTallyBound=true;e.effects.length=0;emit(name,{bindStaticTally:true},{transport:'Immediate',outcome:{returned:undefined}});}
const originalEmit=emit;emit=function(...args){originalEmit(...args);records.at(-1).consoleObservations=structuredClone(e.logs);};
(async()=>{
 await reset('remaining/first-ten-default-class');
 await call('remaining/default-state','AMMStatic.getAMMOrdersAndPositions');
 await call('remaining/default-no-strategy','AMMStatic.runOptionStrategy',[100,0.2,10]);
 await call('remaining/default-straddle','AMMStatic.buildStraddle',[100,10]);
 await call('remaining/default-iron-fly','AMMStatic.buildIronFly',[100,'10']);
 await call('remaining/default-calendar','AMMStatic.buildCalendar',[100,10]);
 await call('remaining/default-lp-addresses-error','AMMStatic.getLPAddresses');
 await call('remaining/default-total-position-error','AMMStatic.calculateTotalPosition');
 await call('remaining/default-inverse-nan-prefix','AMMStatic.generateOrdersForInverse',[100,0.1,10]);
 await call('remaining/default-linear-nan-prefix','AMMStatic.generateOrdersForLinear',[100,0.1,10]);
 await call('remaining/default-request-liquidity-error-prefix','AMMStatic.requestLiquidity',[{size:2,side:'buy'}]);
 assert.equal(records.filter(r=>r.command.target).length,10,'ten actual original calls before extending');
 const proof=records.map(r=>JSON.stringify(tag(r))).join('\n')+'\n';
 if(!flags.includes('--proof10')){
  await reset('remaining/default-lp-prefix');
  await call('remaining/default-insert-prefix','AMMStatic.insertCapital',[A,1,2,true,7,3,10]);
  await call('remaining/default-redeem-prefix','AMMStatic.redeemCapital',[A,1,2,true,7,3,10]);
  await reset('remaining/class-data-profile');
  await configure('remaining/input-class-fields',{position:10,maxPosition:100,maxQuoteSize:5,lpShares:{[A]:2,[B]:3},lpAddresses:{[A]:2,[B]:3},ammOrders:[],strategyBlob:'{"type":"straddle"}',contractType:'inverse'});
  await call('remaining/buy-liquidity','AMMStatic.requestLiquidity',[{size:3,side:'buy',id:'first'}]);
  await call('remaining/sell-liquidity','AMMStatic.requestLiquidity',[{size:4,side:'sell',id:'second'}]);
  await call('remaining/maximum-liquidity-null','AMMStatic.requestLiquidity',[{size:1000,side:'buy'}]);
  await call('remaining/string-size-native-number','AMMStatic.requestLiquidity',[{size:'2',side:'buy'}]);
  await call('remaining/class-orders-and-position','AMMStatic.getAMMOrdersAndPositions');
  await call('remaining/lp-addresses','AMMStatic.getLPAddresses');
  await call('remaining/total-position','AMMStatic.calculateTotalPosition');
  await call('remaining/single-position','AMMStatic.calculateTotalPosition',[A]);
  await call('remaining/missing-position-zero','AMMStatic.calculateTotalPosition',['absent']);
  await call('remaining/straddle-strategy','AMMStatic.runOptionStrategy',[100,0.2,10]);
  await configure('remaining/iron-fly-data',{strategyBlob:'{"type":"ironFly"}'});
  await call('remaining/iron-fly-strategy','AMMStatic.runOptionStrategy',[100,0.2,'10']);
  await configure('remaining/calendar-data',{strategyBlob:'{"type":"calendar"}'});
  await call('remaining/calendar-strategy','AMMStatic.runOptionStrategy',[100,0.2,10]);
  await configure('remaining/invalid-strategy-data',{strategyBlob:'{'});
  await call('remaining/invalid-strategy-caught','AMMStatic.runOptionStrategy',[100,0.2,10]);
  await configure('remaining/null-strategy-data',{strategyBlob:'null'});
  await call('remaining/null-strategy-typeerror','AMMStatic.runOptionStrategy',[100,0.2,10]);
  await configure('remaining/unknown-strategy-data',{strategyBlob:'{"type":"other"}'});
  await call('remaining/unknown-strategy-empty','AMMStatic.runOptionStrategy',[100,0.2,10]);
  await call('remaining/inverse-mutates-class-position','AMMStatic.generateOrdersForInverse',[100,0.1,10]);
  await configure('remaining/linear-data',{contractType:'linear',position:-2,maxQuoteSize:5});
  await call('remaining/linear-mutates-negative-position','AMMStatic.generateOrdersForLinear',[100,0.1,10]);
  await call('remaining/linear-dispatch','AMMStatic.generateOrders',[100,0.1,10,1,7,false,false]);
  await configure('remaining/inverse-data',{contractType:'inverse',position:'10'});
  await call('remaining/inverse-native-string-plus','AMMStatic.generateOrders',[100,0.1,10,1,7,true,false]);
  await configure('remaining/share-string-data',{lpShares:{[A]:'2',[B]:3}});
  await call('remaining/share-reduce-native-plus','AMMStatic.calculateTotalPosition');
  await reset('remaining/unbound-static-tally');
  await configure('remaining/unbound-capital-data',{position:10,maxPosition:100,lpShares:{[A]:5}});
  await call('remaining/unbound-tally-insert-prefix','AMMStatic.insertCapital',[A,1,2,true,7,3,10]);
  await call('remaining/unbound-tally-redeem-prefix','AMMStatic.redeemCapital',[A,1,1,true,7,3,10]);
  await reset('remaining/bound-static-tally');
  await configure('remaining/bound-capital-data',{position:10,maxPosition:100,lpShares:{[A]:5}});
  await bindStaticTally('remaining/actual-existing-tally-realm-input');
  await call('remaining/bound-contract-insert','AMMStatic.insertCapital',[A,1,2,true,7,3,10]);
  await call('remaining/bound-contract-redeem','AMMStatic.redeemCapital',[A,1,1,true,7,3,10]);
  await call('remaining/bound-token-insert','AMMStatic.insertCapital',[A,1,2,false,7,3,10]);
  await call('remaining/bound-token-redeem','AMMStatic.redeemCapital',[A,1,1,false,7,2,10]);
  await call('remaining/insufficient-share-rejection','AMMStatic.redeemCapital',[A,1,999,true,7,2,10]);
  await call('remaining/max-position-rejection','AMMStatic.insertCapital',[A,1,999,true,7,2,10]);
  await call('remaining/bound-zero-share-deletion','AMMStatic.redeemCapital',[A,1,7,true,7,2,10]);
  await reset('remaining/default-free-txid');
  for(const totalOrders of [0,-1,0.5,1,1.5,'2',undefined])await call('remaining/unbound-txid-orders-'+String(totalOrders),'AMMStatic.generateTokenOrders',[1,7,100,totalOrders,2,10]);
  await call('remaining/free-txid-native-coercion-error','AMMStatic.generateTokenOrders',[1,7,100,1,{toString:1},10]);
 }
 if(!flags.includes('--proof10')){
  await reset('remaining/order-exact-original-source');
  await configure('remaining/order-class-input',{position:10,maxPosition:100,lpShares:{[A]:5}});
  await call('remaining/order-max-check-before-address-coercion','AMMStatic.insertCapital',[{toString:1},1,999,true,7,3,10]);
  await call('remaining/order-position-write-before-address-coercion','AMMStatic.insertCapital',[{toString:1},1,2,true,7,3,10]);
  await call('remaining/order-lp-id-coercion-before-unbound-tally','AMMStatic.insertCapital',[A,{toString:1},2,true,7,3,10]);
  await bindStaticTally('remaining/order-actual-tally-binding');
  await call('remaining/order-contract-insert-ignores-id2','AMMStatic.insertCapital',[A,1,2,true,{toString:1},{toString:1},10]);
  await call('remaining/order-contract-redeem-ignores-id2','AMMStatic.redeemCapital',[A,1,1,true,{toString:1},{toString:1},10]);
  await call('remaining/order-token-insert-retains-first-two-tally-prefix','AMMStatic.insertCapital',[A,1,2,false,7,{toString:1},10]);
  await call('remaining/order-token-redeem-retains-first-two-tally-prefix','AMMStatic.redeemCapital',[A,1,1,false,7,{toString:1},10]);
 }
 const payload=records.map(r=>JSON.stringify(tag(r))).join('\n')+'\n';
 inventory.frames=records.length;inventory.actualCalls=records.filter(r=>r.command.target).length;inventory.resetFrames=records.filter(r=>r.command.reset).length;inventory.explicitDataFrames=records.filter(r=>r.command.configureStatic||r.command.bindStaticTally).length;inventory.fullSHA256=sha(payload);inventory.proofSHA256=sha(proof);inventory.numeric='genuine9.3.1-and9.1.2-byte-equal';
 const outputs=flags.includes('--proof10')?[['amm_remaining_proof10.jsonl',proof]]:[['amm_remaining_proof10.jsonl',proof],['amm_remaining_vectors.jsonl',payload],['amm_remaining_inventory.json',JSON.stringify(inventory,null,2)+'\n']];for(const[name,value]of outputs){const p=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(p,'utf8'),value);else fs.writeFileSync(p,value);}
 if(flags.includes('--print')){process.stdout.write(payload);return;}console.log(JSON.stringify(inventory,null,2));
})().catch(e=>{console.error(e.stack||e);process.exitCode=1;});
