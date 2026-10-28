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
(async()=>{
 await reset('quote-first-ten-input',{poolInput:[5,100,2,1]});
 await quote('quote-valid-original-number',1,[100,10]);
 await quote('quote-zero-mid-empty',1,[0,10]);
 await quote('quote-infinite-mid-empty',1,[Infinity,10]);
 await quote('quote-string-mid-block-one-bp',1,['100','10',1]);
 await quote('quote-nonnumeric-spread-preserves-nan-prices',1,[1.234567895,10,'bad']);
 await call('size-source-bottom-distance','AMMStatic.calculateOrderSize',[0.001,0.1,100]);
 await call('redemption-source-raw-collateral-formula','AMMStatic.calculateRedemptionValue',[2,true,{collateralId:7,price:10,tokens:100},12]);
 await call('rebalance-zero-position-real-registry','AMMStatic.updateOrdersForAllContractAMMs',[10]);
 await call('generate-token-branch-retains-tdz-error','AMMStatic.generateOrders',[2,0.1,10,1,7,false,true]);
 await call('generate-token-orders-retains-caught-missing-txid','AMMStatic.generateTokenOrders',[1,7,100,1,2,10]);
 assert.equal(records.filter(row=>row.command.quote||row.command.target).length,10,'first execute ten genuine original calls before extension');
 const proof=records.map(row=>JSON.stringify(tag(row))).join('\n')+'\n';
 assert.equal(sha(proof),'ec741a48d1ef24b1fbf1934e8c4c6149e77fc527e6d17bbc8a39bc6712e3ddb7');
 if(!flags.includes('--proof10')){
  await quote('quote-binary-rounding-mid',1,[2.675,10]);
  await quote('quote-negative-zero-bid',1,[1e-12,10,20000]);
  await quote('quote-infinite-spread-prices',1,[100,10,Infinity]);
  await quote('quote-nonnumeric-block-keeps-nan-time-and-txid',1,[100,'bad',-1]);
  await quote('quote-huge-mid-toFixed-exponential-branch',1,[1e21,10]);
  await quote('quote-object-number-coercion-source-typeerror',1,[{toString:1},10]);
  await call('size-source-second-distance','AMMStatic.calculateOrderSize',[0.002,0.1,'100']);
  await call('size-source-third-distance','AMMStatic.calculateOrderSize',[0.003,0.1,100]);
  await call('size-source-top-distance','AMMStatic.calculateOrderSize',[0.1,0.1,100]);
  await call('redemption-string-tokens-retains-plus-concatenation','AMMStatic.calculateRedemptionValue',[2,true,{collateralId:7,price:10,tokens:'100'},12]);
  await call('redemption-token-branch-ignores-pool-data','AMMStatic.calculateRedemptionValue',[2,false,undefined,'10']);
  await call('redemption-null-pool-source-typeerror','AMMStatic.calculateRedemptionValue',[2,true,null,12]);
  await call('generate-default-class-type-is-source-invalid','AMMStatic.generateOrders',[2,0.1,10,1,7,false,false]);
  await call('generate-zero-token-orders-still-constructs-real-book','AMMStatic.generateTokenOrders',[1,7,100,0,2,10]);
  await reset('quotes-string-metadata-input',{poolInput:['5',100,'2','1']});await quote('quote-string-metadata-native-number-result',1,[100,10]);
  await reset('quotes-nan-position-input',{poolInput:[NaN,100,2,1]});await quote('quote-nan-position-empty',1,[100,10]);
  await reset('quotes-negative-position-input',{poolInput:[-1,100,2,1]});await quote('quote-negative-position-empty',1,[100,10]);
  const book={buy:[{sender:'amm',amount:1,price:1,txid:'old-amm-buy'},{sender:A,amount:1,price:1,txid:'real-user'}],sell:[{sender:'amm',amount:1,price:3,txid:'old-amm-sell'}]},books=[{_id:'1',value:JSON.stringify(book)}],pool={position:5,maxPosition:100,maxQuoteSize:2,contractType:1,lpAddresses:{},ammOrders:[]};
  await reset('positive-real-rebalance-input',{contractPool:pool,books});
  await call('rebalance-positive-real-oracle-and-book','AMMStatic.updateOrdersForAllContractAMMs',[10]);
  await call('rebalance-next-block-replaces-only-virtual-quotes','AMMStatic.updateOrdersForAllContractAMMs',[11]);
  await reset('rebalance-first-save-failure-input',{contractPool:pool,books});
  await call('rebalance-first-save-failure-prefix','AMMStatic.updateOrdersForAllContractAMMs',[10],{collection:'orderBooks',op:'update',message:'quote-cancel-save'});
  await call('rebalance-first-save-failure-retry','AMMStatic.updateOrdersForAllContractAMMs',[10]);
  await reset('rebalance-final-save-failure-input',{contractPool:pool,books});
  await call('rebalance-final-save-failure-prefix','AMMStatic.updateOrdersForAllContractAMMs',[10],{collection:'orderBooks',op:'update',skip:1,message:'quote-final-save'});
  await call('rebalance-final-save-failure-retry','AMMStatic.updateOrdersForAllContractAMMs',[11]);
  await reset('rebalance-no-pool-input',{noAmmPool:true});await call('rebalance-missing-pool-is-caught-skip','AMMStatic.updateOrdersForAllContractAMMs',[10]);
  await reset('rebalance-registry-read-failure-input',{contractPool:pool});await call('rebalance-load-failure-is-source-swallowed','AMMStatic.updateOrdersForAllContractAMMs',[10],{collection:'contractList',op:'find',message:'quote-registry-read'});
  await reset('rebalance-corrupt-number-metadata-input',{contractPool:{...pool,position:{toString:1}}});await call('rebalance-getter-number-typeerror-is-caught-skip','AMMStatic.updateOrdersForAllContractAMMs',[10]);
  await reset('registry-constructor-source-input');
  await call('registry-create-retains-real-new-amm-receiver','Contract.createContractSeries',[A,{native:true,underlyingOracleId:0,onChainData:[[1,0]],notionalPropertyId:0,notionalValue:0.001,collateralPropertyId:7,leverage:5,expiryPeriod:0,series:1,inverse:true,fee:false,whitelist:0},10]);
  await call('registry-create-typeerror-still-retains-constructor-prefix','Contract.createContractSeries',[A,null,11]);
 }

 if(!flags.includes('--proof10')){
  await reset('number-hint-big-number-source-input',{poolInput:[5,100,2,new BN('-0')]});
  await quote('quote-big-number-negative-zero-contract-and-block',1,[100,new BN('-0')]);
  await call('size-big-number-negative-zero-result','AMMStatic.calculateOrderSize',[0.001,0.1,new BN('-0')]);
  await call('redemption-big-number-negative-zero-result','AMMStatic.calculateRedemptionValue',[new BN('-0'),false,undefined,10]);
  await quote('quote-big-number-positive-control',1,[new BN('100'),new BN('10'),new BN('1')]);
  await quote('quote-big-number-negative-zero-spread-is-not-fallback40',1,[100,10,new BN('-0')]);
  await call('redemption-big-number-default-primitive-hint','AMMStatic.calculateRedemptionValue',[2,true,{collateralId:7,price:10,tokens:new BN('-0')},12]);
 }
 const payload=records.map(row=>JSON.stringify(tag(row))).join('\n')+'\n';
 fs.mkdirSync(path.join(stage,'fixtures'),{recursive:true});
 const outputs=flags.includes('--proof10')?[['amm_quotes_proof10.jsonl',proof]]:[['amm_quotes_proof10.jsonl',proof],['amm_quote_number_vectors.jsonl',payload]];
 for(const[name,value]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),value);else fs.writeFileSync(file,value);}
 if(flags.includes('--print')){process.stdout.write(payload);return;}
 console.log(JSON.stringify({frames:records.length,calls:records.filter(row=>row.command.quote||row.command.target).length,version:flags.includes('--lock')?'9.1.2':'9.3.1',sha256:sha(payload),summary:records.map(row=>({name:row.name,result:tag(row.result)}))},null,2));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
