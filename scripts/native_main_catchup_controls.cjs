'use strict';
// Complete pinned modules, genuine NeDB metadata, recorded IO failures only.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);
process.env.PROOF_REPO=repo;
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const driver=fs.readFileSync(path.join(repo,'scripts/native_main_controls.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(sha(driver),'953df9389b84b663edeb55d7485efadf65e01af7130c86cb20bb61211f94fb8f');
const cut=driver.indexOf('async function runCase(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/native-catchup-loader.cjs'),module);
loader.filename=path.join(stage,'scripts/native-catchup-loader.cjs');loader.paths=Module._nodeModulePaths(path.join(stage,'scripts'));
loader._compile(driver.slice(0,cut)+'\nmodule.exports={h,native,Datastore,util,pins};',loader.filename);
const {h,native,Datastore,util,pins}=loader.exports;
const errorValue=error=>({name:error.name,message:error.message});
const volumeCommands=[['0-99',20000000000,20000000000,1,10,'UTXO'],['0-98',1,1,1,9,'UTXO']];
const tailFailure={name:'SourceBoundaryUnavailable',message:'Native Main control has no live RPC service'};
async function scene(name,{wholeQuery=false,withdrawSkip,volume=false}={}){
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',rows:[structuredClone(native)],mainGenesis:10,maxHeight:10,observeTypes:true};
 await h.reset('native-catchup/'+name,reset);
 const {e,m}=h.access(),db=new Datastore();
 for(const row of e.table('txIndex'))await util.promisify(db.insert.bind(db))(row);
 e.tables.delete('txIndex');
 const operations=[];let wholeQueryPending=wholeQuery;
 const receiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[method+'Async',async(...args)=>{
  const operation={ordinal:operations.length,atEffect:e.effects.length,method:method+'Async',args:h.tag(args)};operations.push(operation);
  if(method==='find'&&wholeQueryPending){wholeQueryPending=false;const error=Error('recorded-whole-index-query-rejection');operation.thrown=errorValue(error);operation.endEffect=e.effects.length;throw error;}
  try{const returned=await util.promisify(db[method].bind(db))(...args);operation.returned=h.tag(returned);operation.endEffect=e.effects.length;return returned;}
  catch(error){operation.thrown=errorValue(error);operation.endEffect=e.effects.length;throw error;}
 }]));
 const originalFactory=e.storage.getDatabase;
 e.storage.getDatabase=function(category,...args){if(category!=='txIndex')return Reflect.apply(originalFactory,this,[category,...args]);operations.push({ordinal:operations.length,atEffect:e.effects.length,endEffect:e.effects.length,method:'getDatabase',args:h.tag([category,...args]),returned:{database:'txIndex'}});return Promise.resolve(receiver);};
 let rpcOrdinal=0;
 m.client.getBlockCount=function(...args){rpcOrdinal++;const error=Object.assign(Error(tailFailure.message),{name:tailFailure.name});e.effects.push({kind:'rpc-input',method:'Client.getBlockCount',ordinal:rpcOrdinal,args:e.copy(args),thrown:errorValue(error)});throw error;};
 const calls=[];
 function observe(receiver,method,target){const original=receiver[method];assert.equal(typeof original,'function');receiver[method]=function(...args){const observation={target,atEffect:e.effects.length,args:h.tag(args),sourceSHA256:sha(original.toString())};calls.push(observation);let returned;try{returned=Reflect.apply(original,this,args);}catch(error){observation.thrown=errorValue(error);observation.endEffect=e.effects.length;throw error;}if(returned&&typeof returned.then==='function')returned.then(value=>{observation.returned=h.tag(value);observation.endEffect=e.effects.length;},error=>{observation.thrown=errorValue(error);observation.endEffect=e.effects.length;});else{observation.returned=h.tag(returned);observation.endEffect=e.effects.length;}return returned;};}
 const Volume=e.load('volumeIndex'),Tally=e.load('tally'),Vesting=e.load('vesting');
 for(const [receiver,method,target] of [[Volume,'getCumulativeVolumes','VolumeIndex.getCumulativeVolumes'],[Volume,'getBlockVolumes','VolumeIndex.getBlockVolumes'],[Vesting,'updateVesting','TradeLayerManager.updateVesting'],[Tally,'applyVesting','Tally.applyVesting']])observe(receiver,method,target);
 const bootstrap=[];
 if(volume)for(const args of volumeCommands){const start=e.effects.length;let result;try{result={returned:await Volume.saveVolumeDataById(...args)};}catch(error){result={thrown:errorValue(error)};}await h.flush();assert.equal(result.thrown,undefined,'raw original volume bootstrap must execute successfully');bootstrap.push({command:h.tag({target:'VolumeIndex.saveVolumeDataById',args}),result:h.tag(result),effects:h.tag(e.effects.slice(start)),sourceState:h.tag(e.state())});}
 e.effects.length=0;e.logs.length=0;calls.length=0;operations.length=0;
 const before=h.tag(e.state()),failure=wholeQuery?{collection:'txIndex',op:'find',skip:0,message:'recorded-whole-index-query-rejection'}:withdrawSkip===undefined?undefined:{collection:'withdrawQueue',op:'find',skip:withdrawSkip,message:'recorded-withdrawal-read-rejection'};
 if(withdrawSkip!==undefined)e.setFailure(failure);
 const input={name,point:'input',reset:h.tag(reset),bootstrap,command:h.tag({target:'Main.constructConsensusFromIndex',args:[10]}),failure:h.tag(failure),tailFailure,sourceMethod:h.inventory.methods['Main.constructConsensusFromIndex'],sourceState:before,indexInput:h.tag(await util.promisify(db.find.bind(db))({}))};
 let result;try{result={transport:'Pending',returned:await m.constructConsensusFromIndex(10)};}catch(error){result={transport:'PendingThrow',thrown:errorValue(error)};}
 await h.flush();
 const sourceState=e.state(),processed=sourceState.tables.find(t=>t.name==='consensus')?.rows.find(r=>r._id===native.value.txId),height=sourceState.tables.find(t=>t.name==='consensus')?.rows.find(r=>r._id==='MaxProcessedHeight');
 const indexRows=await util.promisify(db.find.bind(db))({});
 const phaseEffects=e.effects.map((effect,at)=>({effect,at})).filter(x=>x.effect.kind==='source-function'&&x.effect.target!=='Types.decodePayload');
 const summary={name,result,processed:!!processed,valid:processed?.params?.valid,maxProcessedHeight:height?.value,decode:e.effects.filter(x=>x.kind==='source-function'&&x.target==='Types.decodePayload').length,phases:phaseEffects.map(x=>x.effect.target),calls,withdrawReads:e.effects.map((effect,at)=>({effect,at})).filter(x=>x.effect.collection==='withdrawQueue'&&x.effect.op==='find').map(x=>({at:x.at,query:h.tag(x.effect.args[0]),thrown:x.effect.thrown})),rpcCalls:rpcOrdinal,failureEffects:e.effects.filter(x=>x.thrown).map(x=>({collection:x.collection,method:x.method,query:h.tag(x.args?.[0]),thrown:x.thrown}))};
 const output={name,point:'returned-original-catchup-call',result:h.tag(result),processed:h.tag(processed),height:h.tag(height),sourceState:h.tag(sourceState),indexRows:h.tag(indexRows),effects:h.tag(e.effects),logs:h.tag(e.logs),operations,calls};
 assert.equal(sourceState.unsupported.length,0,'no hidden unowned financial dependency');
 const expectedPhases=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Channels.processWithdrawals','Clearing.clearingFunction'];
 assert.equal(result.transport,'PendingThrow');
 if(wholeQuery){
  assert.deepEqual(result.thrown,{name:'Error',message:'recorded-whole-index-query-rejection'});
  assert.equal(summary.decode,0);assert.equal(processed,undefined);assert.equal(height,undefined);assert.equal(rpcOrdinal,0);assert.deepEqual(summary.phases,[]);
  assert.equal(sourceState.tables.find(t=>t.name==='propertyList').rows.length,0);
 }else{
  assert.equal(summary.decode,1);assert.equal(processed.processed,true);assert.equal(processed.params.valid,true);
  if(withdrawSkip!==undefined){
   assert.deepEqual(result.thrown,{name:'Error',message:'recorded-withdrawal-read-rejection'});
   assert.deepEqual(summary.phases,expectedPhases.slice(0,5));assert.equal(height,undefined);assert.equal(rpcOrdinal,0);
  }else{
   assert.deepEqual(result.thrown,tailFailure);assert.deepEqual(summary.phases,expectedPhases);assert.equal(height.value,10);assert.equal(rpcOrdinal,1);
  }
 }
 if(volume){
  assert.deepEqual(calls.map(c=>c.target),['VolumeIndex.getCumulativeVolumes','VolumeIndex.getBlockVolumes','TradeLayerManager.updateVesting','Tally.applyVesting','Tally.applyVesting']);
  assert.deepEqual(calls[0].returned,h.tag({globalCumulativeVolume:20000000001,ltcPairTotalVolume:20000000001,contractCumulativeVolume:0}));
  assert.deepEqual(calls[1].returned,h.tag({ltcPairs:20000000000,global:20000000000}));
  assert.deepEqual(calls[2].args,h.tag([20000000001,20000000000,20000000001,20000000000]));
  assert.deepEqual(calls[2].returned,h.tag({two:250000,three:45454.54545606061}));
  assert.deepEqual(calls[3].args,h.tag([2,250000,10]));assert.deepEqual(calls[4].args,h.tag([3,45454.54545606061,10]));
 }
 return{records:[input,output],summary};
}
(async()=>{
 const controls=[];
 controls.push(await scene('baseline-real-tail-rejection'));
 controls.push(await scene('whole-index-query-fails-before-any-row',{wholeQuery:true}));
 const reads=controls[0].summary.withdrawReads;
 assert.equal(reads.length,2,'baseline actual source reads withdrawal table in state hash and then in settlement');
 const skip=reads.length-1;
 controls.push(await scene('withdrawal-read-fails-after-four-settlement-calls',{withdrawSkip:skip}));
 controls.push(await scene('nonzero-raw-volume-bootstrap-reaches-vesting',{volume:true}));
 const records=controls.flatMap(x=>x.records),bytes=records.map(x=>JSON.stringify(x)).join('\n')+'\n';
 const locked=process.argv.includes('--lock'),file=path.join(stage,'fixtures/native_main_catchup_controls'+(locked?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 console.log(JSON.stringify({profile:'TL-ORIGINAL-NATIVE-MAIN-CATCHUP-CONTROLS-1',node:process.version,numeric:locked?'genuine-9.1.2':'genuine-9.3.1',calls:4,bootstrapCalls:2,observations:records.length,withdrawalSkip:skip,fixtureSHA256:sha(bytes),controls:controls.map(x=>x.summary)},null,2));
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
