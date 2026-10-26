#!/usr/bin/env node
'use strict';
// Full unchanged pinned source; only explicit external modules/storage/clock are
// injected. This audit grants no issuance, custody, admission or source parity
// for the separately named funded policy.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const repo=fs.existsSync(path.resolve(__dirname,'../fixtures/native_genesis/entitlement-pins.json'))?path.resolve(__dirname,'..'):path.resolve(__dirname,'../../../outputs/TradeLayer-Rust');
const root=path.join(repo,'fixtures/native_genesis'),pins=JSON.parse(fs.readFileSync(path.join(root,'entitlement-pins.json'))),source={};
require(path.join(repo,'scripts/native_source_oracle.cjs')).verifyNativeSources(repo);
for(const[name,pin]of Object.entries(pins.files)){const b=fs.readFileSync(path.join(root,'source',name+'.js'));assert.equal(crypto.createHash('sha256').update(b).digest('hex'),pin.sha256);source[name]=b.toString('utf8');}
const BN=require(path.join(repo,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
const quiet={log(){},warn(){},error(){}};
const tag=v=>v===undefined?{$undefined:true}:typeof v==='number'&&!Number.isFinite(v)?{$number:String(v)}:Object.is(v,-0)?{$number:'-0'}:Array.isArray(v)?v.map(tag):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,tag(v)])):v;
function env(syncDb, initialOverride, declared=20){
 const trace=[],tables=new Map(),modules={};let uuid=0;
 const map=n=>{if(!tables.has(n))tables.set(n,new Map());return tables.get(n)};
 const row=(available,vesting=0)=>({amount:available,available,reserved:0,margin:0,vesting,channelBalance:0});
 const initial=initialOverride||[['alice',{1:row(0),2:row(10,10)}],['bob',{1:row(0),2:row(10,10)}]];
 map('tallyMap').set('tallyMap',{_id:'tallyMap',data:JSON.stringify(initial)});
 const database=category=>({findOneAsync:async q=>{trace.push({target:'DB.findOneAsync',category,query:q});return structuredClone(map(category).get(q._id)||null)},updateAsync:async(q,u)=>{trace.push({target:'DB.updateAsync',category,query:q});const old=map(category).get(q._id)||{_id:q._id};map(category).set(q._id,structuredClone(u.$set?{...old,...u.$set}:u));return 1},insertAsync:async doc=>{trace.push({target:'DB.insertAsync',category});map(category).set(doc._id,structuredClone(doc));return structuredClone(doc)}});
 const getDatabase=category=>{trace.push({target:'DB.getDatabase',category});return syncDb?database(category):Promise.resolve(database(category))};
 const property={getPropertyData:async id=>{trace.push({target:'Property.getPropertyData',id});return{totalInCirculation:declared}}};
 const load=name=>{if(modules[name])return modules[name].exports;const m={exports:{}};modules[name]=m;class Clock extends Date{static now(){return 0}}
 vm.runInNewContext(source[name],{module:m,exports:m.exports,console:quiet,Date:Clock,Buffer,process:{env:{}},require:target=>{
 if(target==='bignumber.js')return BN;if(target==='uuid')return{v4:()=>`entitlement-${++uuid}`};if(target==='./db.js')return{getDatabase};if(target==='./tally.js')return load('tally');if(target==='./vesting.js')return load('vesting');if(target==='./property.js')return property;if(target==='./activation.js')return{getInstance:()=>({})};if(target==='fs')return{};return{};
 }},{filename:'pinned-'+name+'.js',timeout:2000});return m.exports};
 const balances=()=>JSON.parse(map('tallyMap').get('tallyMap').data);
 return{logic:load('logic'),tally:load('tally'),vesting:load('vesting'),balances,trace};
}
async function observed(call){try{return{returned:tag(await call())}}catch(e){return{thrown:{name:e.name,message:e.message}}}}
async function main(){
 const row=(available,vesting=0)=>({amount:available,available,reserved:0,margin:0,vesting,channelBalance:0});
 const huge=18014398509481984;
 const e=env(true,[['alice',{1:row(0),2:row(6,huge)}],['bob',{1:row(0),2:row(0,0)}]]);
 const unbridged=e.logic.calculateVestingMovement(5,{available:6,vesting:huge});
 const send=await observed(()=>e.logic.vestingSend('alice','bob',2,5,10));
 const payout=env(true,[['alice',{1:row(9007199254740992),2:row(1,1)}]],1);
 const paid=await observed(()=>payout.tally.applyVesting(2,1,11));
 const result={sourceCommit:pins.commit,sourcePins:pins.files,scenes:[{name:'whole-marker-string-then-Number-loss',unbridged,number:Number(unbridged),send,after:e.balances(),trace:e.trace},{name:'post-payout-available-Number-loss',paid,after:payout.balances(),trace:payout.trace}]};
 assert.equal(unbridged,'15011998757901653');assert.equal(Number(unbridged),15011998757901652);assert.equal(result.scenes[0].after[1][1]['2'].vesting,15011998757901652);
 assert.equal(result.scenes[1].after[0][1]['1'].available,9007199254740992);assert.equal(result.scenes[1].after[0][1]['2'].vesting,0);
 const text=JSON.stringify(result,null,2)+'\n',file=path.resolve(__dirname,'../fixtures/native_entitlement_owner_vectors.json');if(process.argv.includes('--write'))fs.writeFileSync(file,text);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),text);console.log('PASS 2 genuine complete-source native Number-boundary scenes');
}
main().catch(e=>{console.error(e);process.exitCode=1});
