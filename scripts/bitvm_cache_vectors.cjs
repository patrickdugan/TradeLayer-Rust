#!/usr/bin/env node
'use strict';
const assert=require('node:assert/strict');const {load,dbAdapter,save,hex}=require('./bitvm_bundle_vectors.cjs');
async function run(){let adapter,cache,now=0;const lines=[];
 async function step(op,name,input={},context={},id=''){
  const start=adapter?adapter.calls.length:0;now+=10;let value,result;
  try{if(op==='reset'){adapter=dbAdapter();cache=load('bitvm_cache',{env:input,database:adapter.database,now:()=>now}).exports.BitvmCacheRegistry;value=null}
   else if(op==='id')value=cache.buildCacheId(input);
   else if(op==='blocks')value=cache._challengeBlocks(input.raw);
   else if(op==='get')value=await cache.get(id);
   else if(op==='open')value=await cache.open(input,context);
   else value=await cache[op](id,input);
   result={ok:true,value:value??null};
  }catch(e){result={ok:false,error:e.message}}
  const args=op==='reset'||op==='id'||op==='blocks'||op==='open'?input:{id,detail:input};
  lines.push([op,name,hex(JSON.stringify(args)),hex(JSON.stringify(context)),now,hex(JSON.stringify(result)),hex(JSON.stringify(adapter.snapshot())),hex(JSON.stringify(adapter.calls.slice(op==='reset'?0:start)))].join('\t'));return result;
 }
 await step('reset','default-env');
 for(const [name,raw]of [['undefined',undefined],['null',null],['empty',''],['negative',-1],['fraction',2.9],['invalid','x'],['hex','0x10'],['array',[]],['bool',true]])await step('blocks',name,raw===undefined?{}:{raw});
 for(const [name,parts]of [['default',{}],['all',{dlcRef:'d',stateHash:'s',bundleHash:'b',propertyId:'7',amount:'2.5',fromAddress:'a',toAddress:'b',cacheAddress:'c'}],['explicit',{cacheId:42}],['nonfinite',{propertyId:'bad',amount:'Infinity'}]])await step('id',name,parts);
 await step('get','unknown',{}, {},'x');
 await step('open','invalid-pid',{cacheId:'x',propertyId:0,amount:1});await step('open','invalid-amount',{cacheId:'x',propertyId:7,amount:0});await step('open','invalid-bond',{cacheId:'x',propertyId:7,amount:2,cacheBondAmount:-1});
 await step('open','initial',{cacheId:'a',propertyId:7,amount:2,challengeBlocks:6,cacheBondAmount:1},{block:10,senderAddress:'opener',dlcRef:'d',stateHash:'s'});
 await step('open','duplicate',{cacheId:'a',propertyId:7,amount:2},{block:11});
 await step('finalize','early',{block:15},{},'a');
 await step('challenge','first',{challengerAddress:'alice',challengeBondAmount:3,block:11},{},'a');
 await step('challenge','zero-bond-invalid-unused-pid',{challengeBondAmount:0,challengeBondPropertyId:'bad',block:11},{},'a');
 await step('challenge','second-late',{challengerAddress:'bob',challengeBondAmount:4,block:999},{},'a');
 await step('challenge','bad-bond',{challengeBondAmount:-1},{},'a');
 await step('finalize','challenged',{block:1000},{},'a');
 await step('resolve','bad-verdict',{verdict:' reject ',block:12},{},'a');
 await step('resolve','reject-shortens',{verdict:'reject',block:12},{},'a');
 await step('finalize','wrong-recipient',{block:12,toAddress:'wrong'},{},'a');
 await step('finalize','wrong-amount',{block:12,amount:3},{},'a');
 await step('finalize','wrong-pid',{block:12,propertyId:8},{},'a');
 await step('finalize','release',{block:12,txid:'tx'},{},'a');
 await step('challenge','released',{block:13},{},'a');
 await step('open','reopen-retains',{cacheId:'a',propertyId:7,amount:9},{block:20,senderAddress:'other'});
 await step('get','stale-fields',{}, {},'a');
 await step('resolve','uphold-without-challenge',{verdict:'uphold',block:21},{},'a');
 await step('finalize','upheld',{block:30},{},'a');
 await step('challenge','rechallenge-upheld',{block:22},{},'a');
 await step('resolve','reject-upheld',{verdict:'REJECT',block:22},{},'a');
 await step('open','derived-id',{propertyId:1.5,amount:'0.29',challengeBlocks:null,recipientAddress:'recipient'},{block:3.5,dlcRef:'d'});
 await step('open','object-recipient',{cacheId:'object',propertyId:7,amount:1,challengeBlocks:0,toAddress:{a:1}},{block:1});
 await step('finalize','object-recipient-identity',{block:1,toAddress:{a:1}},{},'object');
 await step('finalize','object-recipient-default',{block:1},{},'object');
 await step('open','array-recipient',{cacheId:'array',propertyId:7,amount:1,challengeBlocks:0,toAddress:['a']},{block:1});
 await step('finalize','array-recipient-identity',{block:1,toAddress:['a']},{},'array');
 await step('finalize','array-recipient-default',{block:1},{},'array');
 await step('open','boundary-created-nonfinite',{cacheId:'nonfinite',propertyId:7,amount:1},{block:'bad'});
 await step('reset','fallback-fraction',{TL_BITVM_CHALLENGE_BLOCKS:'2.5'});await step('blocks','fallback-not-floored',{raw:-1});
 await step('reset','fallback-nan',{TL_BITVM_CHALLENGE_BLOCKS:'bad'});await step('blocks','fallback-nan',{raw:'bad'});
 // Source write failure leaves no committed row for cache's single-write call.
 adapter.failNext(1);const failed=await step('open','trace-write-failure',{cacheId:'f',propertyId:7,amount:1,challengeBlocks:1},{block:1});assert.equal(failed.ok,false);assert.equal(adapter.snapshot().length,0);
 save('bitvm_cache',lines);
}
run().catch(e=>{console.error(e);process.exitCode=1});
