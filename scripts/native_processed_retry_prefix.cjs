'use strict';
// Observation-only sidecar: no source body or returned Promise is changed.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const stage=path.resolve(__dirname,'..');
const driver=fs.readFileSync(path.join(stage,'scripts/native_main_controls.cjs'),'utf8');
const cut=driver.indexOf('(async()=>{');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/retry-prefix-loader.cjs'),module);
loader.filename=path.join(stage,'scripts/retry-prefix-loader.cjs');loader.paths=Module._nodeModulePaths(path.join(stage,'scripts'));
loader._compile(driver.slice(0,cut)+'\nmodule.exports={runCase,access:()=>h.access()};',loader.filename);
const observed=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Channels.processWithdrawals','Clearing.clearingFunction'];
async function one(name,direct){
 const frames=await loader.exports.runCase(name,{direct});const {e}=loader.exports.access();
 const phases=e.effects.filter(effect=>effect.kind==='source-function'&&observed.includes(effect.target)).map(effect=>({target:effect.target,args:effect.args}));
 return{name,direct,header:frames[0],phases};
}
(async()=>{
 const fresh=await one('fresh-catchup-processes-real-main0',false);
 const retried=await one('already-processed-row-changes-catchup-branch',true);
 assert.deepEqual(fresh.phases.map(p=>p.target),observed);
 assert.equal(retried.phases.length,0,'naive retry skips settlement source calls');
 assert.equal(fresh.header.financialProcessed,true);assert.equal(retried.header.financialProcessed,true);
 const bytes=JSON.stringify({profile:'TL-MAIN-ACTUAL-PROCESSED-RETRY-CONTROL-1',fresh,retried},null,2)+'\n';
 const file=path.join(stage,'fixtures/native_processed_retry_prefix.json');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 console.log(JSON.stringify({freshPhaseCalls:fresh.phases.length,alreadyProcessedPhaseCalls:retried.phases.length,fixtureSHA256:crypto.createHash('sha256').update(bytes).digest('hex')}));
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
