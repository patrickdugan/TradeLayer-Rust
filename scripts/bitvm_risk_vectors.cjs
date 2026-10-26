#!/usr/bin/env node
'use strict';const assert=require('node:assert/strict');const {load,dbAdapter,save,hex}=require('./bitvm_bundle_vectors.cjs');
async function run(){let adapter,risk,now=0;const lines=[];
 async function step(op,name,input={}){const start=adapter?adapter.calls.length:0;now+=10;let result;try{if(op==='reset'){adapter=dbAdapter();risk=load('bitvm_risk',{env:input,database:adapter.database,now:()=>now}).exports;result={ok:true}}else{await risk[op](input);result={ok:true}}}catch(e){result={ok:false,error:e.message}}lines.push([op,name,hex(JSON.stringify(input)),hex('{}'),now,hex(JSON.stringify(result)),hex(JSON.stringify(adapter.snapshot())),hex(JSON.stringify(adapter.calls.slice(op==='reset'?0:start)))].join('\t'));return result}
 await step('reset','no-caps');
 for(const bad of [{},{propertyId:0,amount:1},{propertyId:7,amount:-1},{propertyId:'bad',amount:2},{propertyId:7,amount:'Infinity'}])await step('onCacheOpen','invalid-noop',bad);
 await step('onCacheOpen','a',{propertyId:7,amount:5,dlcRef:'a',block:143});await step('onCacheOpen','b',{propertyId:7,amount:5,dlcRef:'b',block:143});
 await step('onCacheOpen','repeat-not-idempotent',{propertyId:7,amount:5,dlcRef:'b',block:143});
 await step('onEscrowRelease','overrelease-clamps',{propertyId:7,amount:20,dlcRef:'a',block:144});
 await step('onSweep','disabled-not-recorded',{propertyId:7,amount:100,block:144});
 await step('onCacheOpen','default-dlc',{propertyId:1.5,amount:0.1,block:-1});await step('onCacheOpen','default-dlc-alias',{propertyId:1.5,amount:0.2,dlcRef:'default',block:0});
 await step('reset','caps',{TL_BITVM_MAX_PENDING_ESCROW:'10',TL_BITVM_MAX_PENDING_ESCROW_PER_DLC:'6',TL_BITVM_MAX_DEPOSIT_PER_WINDOW:'9',TL_BITVM_MAX_WITHDRAW_PER_WINDOW:'4',TL_BITVM_MAX_SWEEP_PER_WINDOW:'3',TL_BITVM_SCHED_WINDOW_BLOCKS:'10'});
 await step('onCacheOpen','initial',{propertyId:7,amount:6,dlcRef:'a',block:9});await step('onCacheOpen','dlc-cap',{propertyId:7,amount:1,dlcRef:'a',block:9});await step('onCacheOpen','global-cap',{propertyId:7,amount:5,dlcRef:'b',block:9});await step('onCacheOpen','deposit-cap',{propertyId:7,amount:4,dlcRef:'b',block:9});
 await step('onCacheOpen','next-window',{propertyId:7,amount:4,dlcRef:'b',block:10});await step('onEscrowRelease','withdraw-at-cap',{propertyId:7,amount:4,dlcRef:'b',block:10});await step('onEscrowRelease','withdraw-over-cap',{propertyId:7,amount:1,dlcRef:'a',block:10});
 await step('onSweep','sweep-at-cap',{propertyId:7,amount:3,block:10});await step('onSweep','sweep-over-cap',{propertyId:7,amount:0.1,block:10});await step('onSweep','sweep-next-window',{propertyId:7,amount:3,block:20});
 await step('reset','bad-env',{TL_BITVM_MAX_PENDING_ESCROW:'NaN',TL_BITVM_SCHED_WINDOW_BLOCKS:'0.5',TL_BITVM_MAX_SWEEP_PER_WINDOW:'-1'});await step('onCacheOpen','bad-window-default',{propertyId:7,amount:1,block:'bad'});await step('onCacheOpen','hex-amount',{propertyId:'0x7',amount:'0x10',block:1});
 await step('reset','fractional-cap',{TL_BITVM_MAX_PENDING_ESCROW:'1'});
 for(let i=0;i<11;i++)await step('onCacheOpen','binary64-tenth-'+i,{propertyId:7,amount:0.1,dlcRef:'d',block:1});
 await step('reset','coercions');
 for(const amount of [true,[],['0.5']])await step('onCacheOpen','json-number-'+JSON.stringify(amount),{propertyId:7,amount,dlcRef:['d'],block:1});
 await step('reset','overflow');await step('onCacheOpen','max-finite',{propertyId:7,amount:Number.MAX_VALUE});await step('onCacheOpen','boundary-overflow',{propertyId:7,amount:Number.MAX_VALUE});
 await step('reset','failure-trace');adapter.failNext(2);await step('onCacheOpen','trace-partial-write',{propertyId:7,amount:3,dlcRef:'d',block:1});assert.equal(adapter.snapshot().length,1);
 save('bitvm_risk',lines);
}
run().catch(e=>{console.error(e);process.exitCode=1});
