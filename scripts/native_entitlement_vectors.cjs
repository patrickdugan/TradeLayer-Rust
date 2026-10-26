#!/usr/bin/env node
'use strict';
// Full unchanged pinned source; only explicit external modules/storage/clock are
// injected. This audit grants no issuance, custody, admission or source parity
// for the separately named funded policy.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const repo=path.resolve(__dirname,'..'),root=path.join(repo,'fixtures/native_genesis'),pins=JSON.parse(fs.readFileSync(path.join(root,'entitlement-pins.json'))),source={};
require('./native_source_oracle.cjs').verifyNativeSources(repo);
for(const[name,pin]of Object.entries(pins.files)){const b=fs.readFileSync(path.join(root,'source',name+'.js'));assert.equal(crypto.createHash('sha256').update(b).digest('hex'),pin.sha256);source[name]=b.toString('utf8');}
const BN=require(path.join(repo,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
const quiet={log(){},warn(){},error(){}};
const tag=v=>v===undefined?{$undefined:true}:typeof v==='number'&&!Number.isFinite(v)?{$number:String(v)}:Object.is(v,-0)?{$number:'-0'}:Array.isArray(v)?v.map(tag):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,tag(v)])):v;
function env(syncDb){
 const trace=[],tables=new Map(),modules={};let uuid=0;
 const map=n=>{if(!tables.has(n))tables.set(n,new Map());return tables.get(n)};
 const row=(available,vesting=0)=>({amount:available,available,reserved:0,margin:0,vesting,channelBalance:0});
 const initial=[['alice',{1:row(0),2:row(10,10)}],['bob',{1:row(0),2:row(10,10)}]];
 map('tallyMap').set('tallyMap',{_id:'tallyMap',data:JSON.stringify(initial)});
 const database=category=>({findOneAsync:async q=>{trace.push({target:'DB.findOneAsync',category,query:q});return structuredClone(map(category).get(q._id)||null)},updateAsync:async(q,u)=>{trace.push({target:'DB.updateAsync',category,query:q});const old=map(category).get(q._id)||{_id:q._id};map(category).set(q._id,structuredClone(u.$set?{...old,...u.$set}:u));return 1},insertAsync:async doc=>{trace.push({target:'DB.insertAsync',category});map(category).set(doc._id,structuredClone(doc));return structuredClone(doc)}});
 const getDatabase=category=>{trace.push({target:'DB.getDatabase',category});return syncDb?database(category):Promise.resolve(database(category))};
 const property={getPropertyData:async id=>{trace.push({target:'Property.getPropertyData',id});return{totalInCirculation:20}}};
 const load=name=>{if(modules[name])return modules[name].exports;const m={exports:{}};modules[name]=m;class Clock extends Date{static now(){return 0}}
 vm.runInNewContext(source[name],{module:m,exports:m.exports,console:quiet,Date:Clock,Buffer,process:{env:{}},require:target=>{
 if(target==='bignumber.js')return BN;if(target==='uuid')return{v4:()=>`entitlement-${++uuid}`};if(target==='./db.js')return{getDatabase};if(target==='./tally.js')return load('tally');if(target==='./vesting.js')return load('vesting');if(target==='./property.js')return property;if(target==='./activation.js')return{getInstance:()=>({})};if(target==='fs')return{};return{};
 }},{filename:'pinned-'+name+'.js',timeout:2000});return m.exports};
 const balances=()=>JSON.parse(map('tallyMap').get('tallyMap').data);
 return{logic:load('logic'),tally:load('tally'),vesting:load('vesting'),balances,trace};
}
async function observed(call){try{return{returned:tag(await call())}}catch(e){return{thrown:{name:e.name,message:e.message}}}}
async function main(){
 const scenes=[];
 for(const sync of[false,true]){
  const e=env(sync),send=await observed(()=>e.logic.vestingSend('alice','bob',2,1.1,10)),afterSend=e.balances(),apply=await observed(()=>e.tally.applyVesting(2,20,11));
  scenes.push({name:sync?'sync-adapter-reachable-marker-shortfall':'actual-async-db-factory-empty-address-path',syncDatabaseFactory:sync,send,afterSend,apply,afterApply:e.balances(),trace:e.trace});
 }
 const e=env(true);const curve=await e.vesting.updateVesting(1e9,0,1e12,0);scenes.push({name:'curve-is-cumulative-repeated-call',first:tag(curve),second:tag(await e.vesting.updateVesting(1e9,0,1e12,0))});
 const n=env(true);scenes.push({name:'fraction-below-one-whole-coupon-rejected',outcome:await observed(()=>n.logic.vestingSend('alice','bob',2,0.9,10)),balances:n.balances()});
 const result={sourceCommit:pins.commit,sourcePins:pins.files,policy:'source observation only; no funded policy/default/amount selected',scenes};
 assert.equal(scenes[0].afterSend[0][1]['2'].available,8.9);assert.equal(scenes[0].afterSend[0][1]['2'].vesting,9);assert.equal(scenes[0].afterSend[1][1]['2'].available,11.1);assert.equal(scenes[0].afterSend[1][1]['2'].vesting,11);
 assert.equal(scenes[0].afterApply[0][1]['1'].available,0);assert(scenes[1].apply.thrown.message.startsWith('Vesting balance cannot go negative'));assert.equal(scenes[1].afterApply[0][1]['1'].available,8.9);assert.equal(scenes[1].afterApply[1][1]['1'].available,0);
 assert.deepEqual(scenes[2].first,scenes[2].second);assert(scenes[3].outcome.thrown);
 const text=JSON.stringify(result,null,2)+'\n',file=path.join(root,'source-entitlement-evidence.json');if(process.argv.includes('--write'))fs.writeFileSync(file,text);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),text);console.log('PASS 4 genuine complete-source entitlement scenes, reachable whole-unit drift/partial sourcefailure/asyncDBno-op/cumulative curve');
}
main().catch(e=>{console.error(e);process.exitCode=1});