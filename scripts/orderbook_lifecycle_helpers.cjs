#!/usr/bin/env node
'use strict';
// Complete pinned business modules execute unchanged; adapters provide data IO.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const crypto=require('node:crypto'),vm=require('node:vm'),Module=require('node:module'),zlib=require('node:zlib');
const flags=process.argv.slice(2),option=(key,fallback)=>{const i=flags.indexOf(key);return i<0?fallback:flags[i+1];};
const root=path.resolve(option('--repo',path.join(__dirname,'..'))),out=path.resolve(option('--out',path.join(root,'fixtures')));
const sha=raw=>crypto.createHash('sha256').update(raw).digest('hex');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(flags.includes('--lock')){
 const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1];
 const raw=zlib.gunzipSync(Buffer.from(packed,'base64'));assert.equal(sha(raw),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');
 const m={exports:{}};vm.runInNewContext(raw.toString(),{module:m,exports:m.exports});BN=m.exports;numeric.loadBigNumber=()=>BN;
}
const sourcePath=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(sourcePath,'utf8').replace(/\r\n/g,'\n');
const harnessSHA=sha(code);assert.equal(harnessSHA,'cb7af611bc3224057036fcbdb3ec6f13d1984b8402c306b99c5a1f55fb144c64');code=code.slice(0,code.indexOf('const scenes=['));
const anchor='return {types,logic,Orderbook,effects';assert.equal(code.split(anchor).length,2);
const rawAnchor='  // main.js imports Validity before Tally/Margin/Clearing/Contracts/Volume,';
assert.equal(code.split(rawAnchor).length,2);code=code.replace(rawAnchor,'  const rawHarnessConstructorInput=[...tables].map(([name,rows])=>({name,rows:structuredClone(rows)}));\n'+rawAnchor);
code=code.replace(anchor,'return {rawHarnessConstructorInput,context,tables,storage,instances,instanceId,copy,unsupported,types,logic,Orderbook,effects');
code+='\nmodule.exports={environment};';
const driver=new Module(sourcePath,module);driver.filename=sourcePath;driver.paths=Module._nodeModulePaths(path.dirname(sourcePath));driver._compile(code,sourcePath);
function tag(v){
 if(v===undefined)return['undefined'];if(v===null)return['null'];
 if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];
 if(typeof v==='number'){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':bytes.toString('hex')];}
 if(BN.isBigNumber(v))return['bignumber',v.valueOf()];
 if(Array.isArray(v))return['array',v.map(tag)];
 if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);
 return['object',Object.entries(v).map(([key,x])=>[key,tag(x)])];
}
const order=(txid,sender,price,amount,extra={})=>({txid,sender,price,amount,...extra});
const book={buy:[order('b-low','alice',1,2),order('b-other','bob',8,3),order('b-high','alice',10,4)],sell:[order('s-high','alice',12,3),order('s-low','alice',2,4),order('s-other','bob',5,8)]};
const scenes=[];const add=(name,method,args,document=book,extra={})=>scenes.push({name,method,args,document,...extra});
// First ten actual helper calls precede all extension scenes.
add('first10/get-empty','getAddressOrders',['alice',false],undefined,{missing:true});
add('first10/get-buy','getAddressOrders',['alice',false]);
add('first10/get-sell-truthy','getAddressOrders',['alice','yes']);
add('first10/cancel-buy-descending','cancelContractOrdersForSize',['alice',999,20,false,5]);
add('first10/reload-original-after-cancel','getAddressOrders',['alice',false],book,{precedingCancel:['alice',999,20,false,5]});
add('first10/cancel-sell-ascending','cancelContractOrdersForSize',['alice',999,20,true,5]);
add('first10/cancel-zero','cancelContractOrdersForSize',['alice',1,20,false,0]);
add('first10/cancel-negative','cancelContractOrdersForSize',['alice',1,20,false,-2]);
add('first10/get-malformed-side','getAddressOrders',['alice',false],{buy:null,sell:[]});
add('first10/adjust-ordinary-book-source-error','adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts:0,avgPrice:100}]);
add('adjust-nested-empty-source-error','adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts:0,avgPrice:100}],{orderBooks:{'1':{buy:[],sell:[]}}});
add('adjust-add-empty-before-source-error','adjustOrdersForAddress',['alice',2,{propertyId:7,block:20},{contracts:0,avgPrice:100}],{orderBooks:{}});
for(const [name,initialReduce,contracts]of [['flip-reduce-unbound-tally',false,-1],['same-reduce-real-margin',true,-1],['flip-entry-real-margin-before-unbound-tally',true,1]])
 add('adjust-'+name,'adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts,avgPrice:100}],{orderBooks:{'1':{buy:[order('nested','alice',10,2,{initialReduce,initMargin:3,marginUsed:1})],sell:[]}}});
add('adjust-missing-contract-after-flag-prefix','adjustOrdersForAddress',['alice',99,{propertyId:7,block:20},{contracts:1,avgPrice:100}],{orderBooks:{'99':{buy:[order('missing','alice',10,2,{initialReduce:true,initMargin:3})],sell:[]}}});
for(const [name,initialReduce,contracts]of [['same-reduce-real-margin',true,-1],['flip-entry-real-margin-before-unbound-tally',true,1]])
 add('orderbook-first/'+name,'adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts,avgPrice:100}],{orderBooks:{'1':{buy:[order('nested','alice',10,2,{initialReduce,initMargin:3,marginUsed:1})],sell:[]}}},{orderbookFirst:true});
add('orderbook-first/missing-contract-after-prefix','adjustOrdersForAddress',['alice',99,{propertyId:7,block:20},{contracts:1,avgPrice:100}],{orderBooks:{'99':{buy:[order('missing','alice',10,2,{initialReduce:true,initMargin:3})],sell:[]}}},{orderbookFirst:true});
add('cancel-nan-price','cancelContractOrdersForSize',['alice',1,20,false,3],{buy:[order('a','alice',1,2),order('b','alice','bad',2),order('c','alice',9,2),order('d','alice',2,2)],sell:[]});
add('cancel-malformed-after-partial','cancelContractOrdersForSize',['alice',1,20,false,100],{buy:[order('a','alice',4,2),null],sell:[]});
add('get-null-row','getAddressOrders',['alice',false],{buy:[order('a','alice',1,2),null],sell:[]});
add('cancel-string-amounts','cancelContractOrdersForSize',['alice',1,20,false,'0.3'],{buy:[order('a','alice',4,'0.1'),order('b','alice',3,'0.3')],sell:[]});
add('cancel-size-NaN','cancelContractOrdersForSize',['alice',1,20,false,'invalid']);
add('cancel-size-Infinity','cancelContractOrdersForSize',['alice',1,20,true,'Infinity']);
add('load-find-caught','getAddressOrders',['alice',false],book,{failure:{collection:'orderBooks',op:'findOne',message:'actual find failure'}});
add('load-factory-escapes','getAddressOrders',['alice',false],book,{factoryFailure:true});
add('cancel-price-object-coercion','cancelContractOrdersForSize',['alice',1,20,false,1],{buy:[order('a','alice',{toString:0},2),order('b','alice',2,2)],sell:[]});
add('cancel-amount-object-coercion','cancelContractOrdersForSize',['alice',1,20,false,1],{buy:[order('a','alice',2,{toString:0})],sell:[]});
add('cancel-size-object-coercion-after-sort','cancelContractOrdersForSize',['alice',1,20,false,{toString:0}]);
add('adjust-address-object-coercion','adjustOrdersForAddress',[{toString:0},1,{propertyId:7,block:20},{contracts:0,avgPrice:100}],{orderBooks:{'1':{buy:[],sell:[]}}});
add('adjust-old-flag-object-coercion','adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts:-1,avgPrice:100}],{orderBooks:{'1':{buy:[order('nested','alice',10,2,{initialReduce:{toString:0},initMargin:3})],sell:[]}}});
add('adjust-return-margin-object-coercion','adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts:-1,avgPrice:100}],{orderBooks:{'1':{buy:[order('nested','alice',10,2,{initialReduce:false,initMargin:{toString:0}})],sell:[]}}});
add('orderbook-first/margin-used-object-coercion','adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts:-1,avgPrice:100}],{orderBooks:{'1':{buy:[order('nested','alice',10,2,{initialReduce:true,initMargin:3,marginUsed:{toString:0}})],sell:[]}}},{orderbookFirst:true});
add('adjust-new-integer-contract-enumeration','adjustOrdersForAddress',['alice',1,{propertyId:7,block:20},{contracts:0,avgPrice:100}],{orderBooks:{'2':{buy:[],sell:[]}}});
const chosen=flags.includes('--proof10')?scenes.slice(0,10):scenes;
const records=[];const flush=async()=>{for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));};
function snapshot(e){const T=e.load('tally'),C=e.load('contractRegistry');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),instances:e.instances.map((value,index)=>({id:index+1,...value})),tally:{addresses:T.addresses,instance:T.instance?{...T.instance}:undefined},contract:{instancePresent:!!C.instance,contractSeries:C.instance?.contractSeries},unsupported:e.unsupported.slice()};}
(async()=>{
 for(const scene of chosen){
  const environmentInput=scene.orderbookFirst?{entry:'orderbook-first',preload:'orderbook'}:{};
  const e=driver.exports.environment(environmentInput);await flush();
  const rows=e.table('orderBooks');rows.splice(0,Infinity,...(scene.missing?[]:[{_id:'1',value:JSON.stringify(scene.document)}]));
  const receiver=new e.Orderbook('1');e.instanceId(receiver);e.effects.length=0;e.logs.length=0;
  if(scene.precedingCancel){await receiver.cancelContractOrdersForSize(...scene.precedingCancel);e.effects.length=0;e.logs.length=0;}
  if(scene.failure)e.setFailure(scene.failure);
  if(scene.factoryFailure){const original=e.storage.getDatabase;e.storage.getDatabase=async function(name){if(name==='orderBooks'){const error=Error('actual factory failure');e.effects.push({kind:'database',name,method:'getDatabase',args:[name],thrown:{name:error.name,message:error.message}});throw error;}return original.apply(this,arguments);};}
  const rawInput={rawHarnessConstructorInput:e.rawHarnessConstructorInput,environmentInput,constructor:['1'],initialRows:scene.missing?[]:[{_id:'1',value:JSON.stringify(scene.document)}],method:scene.method,args:structuredClone(scene.args),precedingCancel:scene.precedingCancel,failure:scene.failure,factoryFailure:scene.factoryFailure};
  const before=snapshot(e);e.effects.length=0;let outcome;
  try{outcome={transport:'Pending',returned:await Reflect.apply(receiver[scene.method],receiver,scene.args)};}catch(error){outcome={transport:'PendingThrow',thrown:{name:error.name,message:error.message}};}
  await flush();const after=snapshot(e);assert.deepEqual(after.unsupported,[],scene.name+' reached unowned dependency');
  records.push({name:scene.name,rawInput,before,outcome,after,effects:structuredClone(e.effects.filter(v=>['database','db','uuid','random','rpc-clock','source'].includes(v.kind))),logs:structuredClone(e.logs)});
 }
 const payload=records.map(record=>JSON.stringify(tag(record))).join('\n')+'\n';const target=path.join(out,flags.includes('--proof10')?'orderbook_lifecycle_helpers_proof10.jsonl':'orderbook_lifecycle_helpers.jsonl');
 if(flags.includes('--write')){fs.mkdirSync(out,{recursive:true});fs.writeFileSync(target,payload);}else assert.equal(fs.readFileSync(target,'utf8').replace(/\r\n/g,'\n'),payload);
 if(flags.includes('--print'))process.stdout.write(payload);else console.log(JSON.stringify({calls:records.length,sourceSHA256:'c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98',harnessSHA,numeric:flags.includes('--lock')?'9.1.2':'9.3.1',fixtureSHA256:sha(payload),businessBodiesChanged:false}));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
