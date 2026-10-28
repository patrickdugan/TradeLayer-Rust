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
replace('return {types,logic,Orderbook,effects','return {ammInstances,ammId,context,tables,storage,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',records=[];let e,lastParams;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.valueOf()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,x])=>[k,tag(x)])];}
function copied(v){return Object.prototype.toString.call(v)==='[object Error]'?{name:v.name,message:v.message}:e.copy(v);}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('contractRegistry'),A=e.load('activation');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyInitialized:!!P.instance,propertyCache:P.instance?.propertyIndex??new Map(),propertyAmms:P.instance?.ammIndex??new Map(),propertyRegistryBound:!!e.context.PropertyRegistry,contractInitialized:!!C.instance,contractSeries:C.instance?.contractSeries??new Map(),activation:A.instance?{...A.instance}:undefined,ammInstances:e.ammInstances.map((value,index)=>({id:index+1,...value})),lastParams,unsupported:e.state().unsupported};}
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
 observe(e.logic,'AMMPool','Logic.AMMPool');observe(e.load('contractRegistry'),'getAMM','Contract.getAMM');observe(e.load('contractRegistry'),'getContractInfo','Contract.getContractInfo');observe(e.load('property'),'getAMM','Property.getAMM');observe(e.load('tally'),'updateBalance','Tally.updateBalance');observe(e.load('activation').prototype,'isTxTypeActive','Activation.isTxTypeActive');
 for(const method of ['addCapital','redeemCapital'])observe(Pool.prototype,method,'AMM.'+method,true);
 await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});
}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,payload,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,10,'tl',payload,A,null,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(10,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure){e.effects.length=0;if(failure)fail(failure);const owners={Logic:e.logic,Property:e.load('property'),Contract:e.load('contractRegistry')},[ns,method]=target.split('.');let transport='Immediate',outcome;try{const value=Reflect.apply(owners[ns][method],owners[ns],args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure},{transport,outcome});}
async function poolCall(name,id,method,args=[],failure){e.effects.length=0;if(failure)fail(failure);const input=copied(args);let transport='Pending',outcome;try{outcome={returned:await Reflect.apply(e.ammInstances[id-1][method],e.ammInstances[id-1],args)};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{pool:{id,method,args:input},failure},{transport,outcome});}
(async()=>{
 await reset('first-ten-original-amm-prestate');
 await raw('raw10-property-amm-missing-registry','0,0,1,2,7,3,,,0,');
 await raw('raw10-contract-add-real-tally','0,1,1,2,7,3,,,0,');
 await raw('raw10-contract-redeem-real-tally','1,1,1,1,7,3,,,0,');
 await call('direct-contract-invalid-zero','Logic.AMMPool',[A,10,false,true,1,0,7,3]);
 await call('direct-contract-over-max','Logic.AMMPool',[A,10,false,true,1,1000,7,3]);
 await raw('raw10-missing-contract-rejected','0,1,2r,2,7,3,,,0,');
 await call('direct-missing-contract','Logic.AMMPool',[A,10,false,true,99,2,7,3]);
 await call('direct-property-missing-registry','Property.getAMM',[1,7]);
 await call('direct-contract-persist-failure','Logic.AMMPool',[A,10,false,true,1,2,7,3],{collection:'contractList',op:'update',message:'amm-contract-persist'});
 await call('direct-contract-persist-retry','Logic.AMMPool',[A,10,false,true,1,2,7,3]);
 assert.equal(records.filter(r=>r.command.raw||r.command.target).length,10,'first execute ten actual calls before extension');
 const proof=records.map(r=>JSON.stringify(tag(r))).join('\n')+'\n';
 if(!flags.includes('--proof10')){
  await reset('property-registry-bound-no-pool',{propertyRegistry:true});
  await raw('raw10-bound-property-initializer-missing','0,0,1,2,7,3,,,0,');
  await reset('real-property-pool-input',{propertyRegistry:true,propertyPool:true});
  await raw('raw10-property-add-real-assets-no-lp-credit','0,0,1,2,7,3,,,0,');
  await raw('raw10-property-redeem-real-assets-no-lp-burn','1,0,1,1,7,1,,,0,');
  await raw('raw10-property-second-asset-over-redemption','1,0,1,1,7,100,,,0,');
  await call('property-reverse-pair-not-normalized','Property.getAMM',[7,1]);
  await poolCall('direct-property-invalid-second-mutates-pool-first',1,'addCapital',[A,1,2,false,7,0,10]);
  await poolCall('direct-property-invalid-first-no-new-mutation',1,'addCapital',[A,1,-1,false,7,3,10]);
  await reset('property-tally-failure-prefix',{propertyRegistry:true,propertyPool:true});
  await raw('property-add-delta-insert-failure','0,0,1,2,7,3,,,0,',{collection:'tallyMapDelta',op:'insert',message:'amm-first-delta'});
  await raw('property-add-delta-insert-retry','0,0,1,2,7,3,,,0,');
  await reset('fractional-native-number-contract-pool');
  await call('contract-native-number-first-tenth','Logic.AMMPool',[A,10,false,true,1,0.1,7,3]);
  await call('contract-native-number-second-two-tenths','Logic.AMMPool',[A,10,false,true,1,0.2,7,3]);
  await call('contract-native-number-redeem-tenth','Logic.AMMPool',[A,10,true,true,1,0.1,7,3]);
  await call('contract-other-owner-insufficient-share','Logic.AMMPool',[B,10,true,true,1,1,7,3]);
  await reset('invalid-raw10-source-guards');
  for(const[name,payload]of [['zero-first','0,1,1,0,7,3,,,0,'],['zero-second','0,0,1,2,7,0,,,0,'],['unknown-property','0,0,2r,2,7,3,,,0,'],['bad-strategy-json','0,1,1,2,7,3,,,0,{bad']])await raw(name,payload);
  await reset('inactive-raw10-real-registry',{inactive:true});await raw('inactive-contract-pool','0,1,1,2,7,3,,,0,');
  await reset('literal-amm-db-promise-defects',{poolInput:[0,100,10,1]});
  await poolCall('createAMM-dereferences-factory-promise',1,'createAMM',[{contractId:1}]);
  await poolCall('saveAMMState-mutates-argument-before-promise-error',1,'saveAMMState',[1,{position:2}]);
  await poolCall('loadAMMState-dereferences-factory-promise',1,'loadAMMState',[1]);
  await poolCall('loadAMMRegistry-dereferences-factory-promise',1,'loadAMMRegistry',[1]);
 }
 const payload=records.map(r=>JSON.stringify(tag(r))).join('\n')+'\n';fs.mkdirSync(path.join(stage,'fixtures'),{recursive:true});
 const outputs=flags.includes('--proof10')?[['lower_amm_proof10.jsonl',proof]]:[['lower_amm_proof10.jsonl',proof],['lower_amm_vectors.jsonl',payload]];
 for(const[name,data]of outputs){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),data);else fs.writeFileSync(file,data);}
 if(flags.includes('--print')){process.stdout.write(payload);return;}
 console.log(JSON.stringify({observations:records.length,calls:records.filter(r=>r.command.raw||r.command.target||r.command.pool).length,version:flags.includes('--lock')?'9.1.2':'9.3.1',sha256:sha(payload),proof10Sha256:sha(proof),summary:records.map(r=>({name:r.name,result:r.result}))},null,2));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
