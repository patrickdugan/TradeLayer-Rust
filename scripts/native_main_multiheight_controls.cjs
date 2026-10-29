'use strict';
// Original pinned Main/grouping/financial bodies. No decoded result importer.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);
process.env.PROOF_REPO=repo;const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const driver=fs.readFileSync(path.join(repo,'scripts/native_main_controls.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(sha(driver),'953df9389b84b663edeb55d7485efadf65e01af7130c86cb20bb61211f94fb8f');
const cut=driver.indexOf('async function runCase(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/native-multiheight-loader.cjs'),module);
loader.filename=path.join(stage,'scripts/native-multiheight-loader.cjs');loader.paths=Module._nodeModulePaths(path.join(stage,'scripts'));
loader._compile(driver.slice(0,cut)+'\nmodule.exports={h,native,Datastore,util,pins};',loader.filename);
const {h,native,Datastore,util,pins}=loader.exports;
const tailFailure={name:'SourceBoundaryUnavailable',message:'Native Main control has no live RPC service'};
function rawRow(height,id,payload='00,production-code'){
 const row=structuredClone(native);row._id=`tx-${height}-${id}`;row.value.txId=id;row.value.payload=payload;return row;
}
const skipped=rawRow(9,'multiheight-previous9');
const funding=rawRow(10,'multiheight-funding10','48,200000000,LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789,1,0,,');
const trade=rawRow(11,'multiheight-fresh11');
const sameHeightTrade=rawRow(10,'aaa-before-funding-in-native-id-order');
const errorValue=e=>({name:e.name,message:e.message});
async function scene(name,{fundingRejections=false,clearingFactory,allProcessed=false}={}){
 const rows=[structuredClone(skipped),structuredClone(funding),structuredClone(trade)];if(fundingRejections)rows.push(structuredClone(sameHeightTrade));
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:11},init:'LTC',rows,mainGenesis:10,maxHeight:11,observeTypes:true};
 await h.reset('native-multiheight/'+name,reset);
 const {e,m}=h.access(),db=new Datastore();
 for(const row of e.table('txIndex'))await util.promisify(db.insert.bind(db))(row);
 e.tables.delete('txIndex');const initialNative=await util.promisify(db.find.bind(db))({});
 const operations=[];let nativeFailures=[];let factoryFailure;
 const receiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[method+'Async',async(...args)=>{
  const operation={ordinal:operations.length,atEffect:e.effects.length,method:method+'Async',args:h.tag(args)};operations.push(operation);
  if(method==='update'&&args[0]?._id===funding._id&&nativeFailures.length){const error=Error(nativeFailures.shift());operation.thrown=errorValue(error);operation.endEffect=e.effects.length;throw error;}
  try{const returned=await util.promisify(db[method].bind(db))(...args);operation.returned=h.tag(returned);operation.endEffect=e.effects.length;return returned;}
  catch(error){operation.thrown=errorValue(error);operation.endEffect=e.effects.length;throw error;}
 }]));
 const originalFactory=e.storage.getDatabase;
 e.storage.getDatabase=function(category,...args){
  if(factoryFailure?.collection===category){if(factoryFailure.skip>0)factoryFailure.skip--;else{const error=Error(factoryFailure.message);factoryFailure=undefined;e.effects.push({kind:'database',name:category,method:'getDatabase',args:e.copy([category,...args]),thrown:errorValue(error)});return Promise.reject(error);}}
  if(category!=='txIndex')return Reflect.apply(originalFactory,this,[category,...args]);
  operations.push({ordinal:operations.length,atEffect:e.effects.length,method:'getDatabase',args:h.tag([category,...args]),returned:{database:'txIndex'}});return Promise.resolve(receiver);
 };
 const bootstrap=[];
 for(const row of (allProcessed?rows:[skipped])){
  const height=Number(row._id.split('-')[1]),start=e.effects.length,opStart=operations.length;let result;
  try{result={returned:await m.processTxSet([structuredClone(row)],height)};}catch(error){result={thrown:errorValue(error)};}
  await h.flush();assert.equal(result.thrown,undefined,'actual raw bootstrap must execute');assert.equal(result.returned,0);
  bootstrap.push({command:h.tag({target:'Main.processTxSet',args:[[row],height]}),result:h.tag(result),effects:h.tag(e.effects.slice(start)),nativeOperations:structuredClone(operations.slice(opStart)),observedSourceState:h.tag(e.state())});
 }
 let rpcCalls=0;m.client.getBlockCount=function(...args){rpcCalls++;const error=Object.assign(Error(tailFailure.message),{name:tailFailure.name});e.effects.push({kind:'rpc-input',method:'Client.getBlockCount',ordinal:rpcCalls,args:e.copy(args),thrown:errorValue(error)});throw error;};
 const rowCalls=[],originalRows=m.processTxSet;
 m.processTxSet=function(...args){const observation={atEffect:e.effects.length,height:args[1],ids:Array.from(args[0],r=>r.value.txId),args:h.tag(args)};rowCalls.push(observation);const promise=Reflect.apply(originalRows,this,args);promise.then(value=>{observation.returned=h.tag(value);observation.endEffect=e.effects.length;},error=>{observation.thrown=errorValue(error);observation.endEffect=e.effects.length;});return promise;};
 e.effects.length=0;e.logs.length=0;operations.length=0;
 if(fundingRejections)nativeFailures=['recorded-funding-primary-validity-rejection','recorded-funding-fatal-validity-rejection'];
 if(clearingFactory)factoryFailure=structuredClone(clearingFactory);
 const input={name,point:'input',mainSHA256:pins.mainSourceSHA256,sourceMethod:h.inventory.methods['Main.constructConsensusFromIndex'],rawReset:h.tag(reset),rawInitialNativeInput:h.tag(initialNative),rawBootstrapCommands:bootstrap,postBootstrapIndexObservedOnly:h.tag(await util.promisify(db.find.bind(db))({})),postBootstrapSourceObservedOnly:h.tag(e.state()),command:h.tag({target:'Main.constructConsensusFromIndex',args:[8]}),recordedNativeFailures:structuredClone(nativeFailures),recordedFactoryFailure:h.tag(clearingFactory),tailFailure};
 let result;try{result={transport:'Pending',returned:await m.constructConsensusFromIndex(8)};}catch(error){result={transport:'PendingThrow',thrown:errorValue(error)};}
 await h.flush();const sourceState=e.state(),nativeRows=await util.promisify(db.find.bind(db))({});
 assert.deepEqual(sourceState.unsupported,[],'no supplied/unavailable financial dependency');
 const phases=e.effects.map((effect,at)=>({effect,at})).filter(x=>x.effect.kind==='source-function'&&x.effect.target!=='Types.decodePayload').map(x=>({at:x.at,target:x.effect.target,height:x.effect.args[0]}));
 const consensus=sourceState.tables.find(t=>t.name==='consensus').rows;
 const marks=e.effects.filter(x=>x.collection==='consensus'&&x.op==='update'&&x.args[1]?.$set?.processed===true);
 const summary={name,result,parseBlock:m.parseBlock,decode:e.effects.filter(x=>x.kind==='source-function'&&x.target==='Types.decodePayload').length,marks:marks.map(x=>({id:x.args[0]._id,params:x.args[1].$set.params})),maxProcessedHeight:consensus.find(x=>x._id==='MaxProcessedHeight')?.value,processed:rows.map(row=>({id:row.value.txId,row:consensus.find(x=>x._id===row.value.txId)})),phases,rowCalls,rpcCalls,factories:e.effects.map((effect,at)=>({effect,at})).filter(x=>x.effect.kind==='database').map(x=>({at:x.at,name:x.effect.name,thrown:x.effect.thrown})),failureEffects:e.effects.filter(x=>x.thrown).map(x=>({collection:x.collection,name:x.name,method:x.method,thrown:x.thrown})),nativeFailures:operations.filter(x=>x.thrown)};
 const output={name,point:'returned-original-multiheight-catchup',result:h.tag(result),parseBlock:h.tag(m.parseBlock),rowCalls,sourceState:h.tag(sourceState),nativeRows:h.tag(nativeRows),effects:h.tag(e.effects),logs:h.tag(e.logs),operations};
 const six=['Tally.flushQueuedFeeAccruals','Logic.settleLiquidityRewards','Orderbook.processQueuedOnChainOrdersForBlock','Orderbook.processQueuedChannelTradesForBlock','Channels.processWithdrawals','Clearing.clearingFunction'];
 const phasePairs=phases.map(x=>[x.height,x.target]);assert.equal(result.transport,'PendingThrow');
 assert.equal(rowCalls[0].height,9);assert.deepEqual(rowCalls[0].ids,[]);assert.deepEqual(rowCalls[0].returned,h.tag(0));
 assert.equal(rowCalls[1].height,9);assert.deepEqual(rowCalls[1].ids,[skipped.value.txId]);assert.deepEqual(rowCalls[1].returned,h.tag(1));
 assert.ok(!rowCalls.some(x=>x.height===8),'start8 empty height advances without an invented row call');
 if(fundingRejections){
  assert.deepEqual(result.thrown,{name:'Error',message:'recorded-funding-fatal-validity-rejection'});
  assert.equal(m.parseBlock,10);assert.equal(summary.decode,1);assert.equal(rowCalls.length,3);
  assert.deepEqual(rowCalls[2].ids,[funding.value.txId]);assert.equal(rowCalls[2].thrown.message,result.thrown.message);
  assert.equal(marks.length,2);assert.ok(marks.every(x=>x.args[0]._id===funding.value.txId));
  assert.equal(marks[1].args[1].$set.params.reason,'Fatal tx processing error: recorded-funding-primary-validity-rejection');
  assert.deepEqual(summary.nativeFailures.map(x=>x.thrown.message),['recorded-funding-primary-validity-rejection','recorded-funding-fatal-validity-rejection']);
  assert.deepEqual(phasePairs,[]);assert.equal(summary.maxProcessedHeight,undefined);assert.equal(rpcCalls,0);
  assert.ok(!consensus.some(x=>[trade.value.txId,sameHeightTrade.value.txId].includes(x._id)));
 }else if(clearingFactory){
  assert.deepEqual(result.thrown,{name:'Error',message:clearingFactory.message});assert.equal(m.parseBlock,10);
  assert.equal(summary.decode,1);assert.equal(marks.length,1);assert.equal(marks[0].args[0]._id,funding.value.txId);
  assert.deepEqual(phasePairs,six.map(x=>[10,x]));assert.equal(summary.maxProcessedHeight,undefined);assert.equal(rpcCalls,0);
  assert.ok(!consensus.some(x=>x._id===trade.value.txId));assert.equal(nativeRows.find(x=>x._id===funding._id).valid,false);
 }else{
  assert.deepEqual(result.thrown,tailFailure);assert.equal(m.parseBlock,11);assert.equal(summary.maxProcessedHeight,11);assert.equal(rpcCalls,1);
  assert.equal(rowCalls.length,6);assert.deepEqual(rowCalls.map(x=>x.height),[9,9,10,10,11,11]);
  if(allProcessed){assert.equal(summary.decode,0);assert.equal(marks.length,0);assert.deepEqual(phasePairs,[]);assert.deepEqual(rowCalls.map(x=>x.returned),[0,1,1,0,0,1].map(x=>h.tag(x)));}
  else{assert.equal(summary.decode,2);assert.deepEqual(marks.map(x=>x.args[0]._id),[funding.value.txId,trade.value.txId]);assert.deepEqual(phasePairs,[...six.map(x=>[10,x]),...six.map(x=>[11,x])]);assert.deepEqual(rowCalls.map(x=>x.returned),[0,1,0,0,0,0].map(x=>h.tag(x)));}
 }
 return{records:[input,output],summary};
}
(async()=>{
 const controls=[];
 controls.push(await scene('skipped9-fresh-funding10-trade11'));
 controls.push(await scene('funding-primary-and-fatal-validity-reject-before-trades',{fundingRejections:true}));
 const base=controls[0].summary,clearing=base.phases.find(x=>x.target==='Clearing.clearingFunction'&&x.height===10);
 assert.ok(clearing,'actual baseline reaches clearing at10');
 const target=base.factories.find(x=>x.at>clearing.at&&x.name==='oracleData');assert.ok(target,'actual original clearing reaches uncaught oracleData factory');
 const skip=base.factories.filter(x=>x.name==='oracleData'&&x.at<target.at).length;
 const failure={collection:'oracleData',op:'getDatabase',skip,message:'recorded-clearing-oracleData-factory-rejection'};
 controls.push(await scene('clearing-factory-rejects-after-earlier-settlement',{clearingFactory:failure}));
 controls.push(await scene('all-three-heights-processed-skip-all-settlement',{allProcessed:true}));
 const bytes=controls.flatMap(x=>x.records).map(x=>JSON.stringify(x)).join('\n')+'\n',locked=process.argv.includes('--lock');
 const file=path.join(stage,'fixtures/native_main_multiheight'+(locked?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 console.log(JSON.stringify({profile:'TL-ORIGINAL-NATIVE-MAIN-MULTIHEIGHT-CONTROLS-1',node:process.version,numeric:locked?'genuine-9.1.2':'genuine-9.3.1',mainCalls:4,rawBootstrapCalls:6,observations:8,clearingFactoryFailure:failure,fixtureSHA256:sha(bytes),controls:controls.map(x=>x.summary)},null,2));
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
