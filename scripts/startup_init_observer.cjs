'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm'),cp=require('node:child_process');
const assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);process.env.PROOF_REPO=repo;
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const nativeDriver=fs.readFileSync(path.join(repo,'scripts/native_main_controls.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(sha(nativeDriver),'953df9389b84b663edeb55d7485efadf65e01af7130c86cb20bb61211f94fb8f');
const nativeCut=nativeDriver.indexOf('async function runCase('),nativeModule=new Module(path.join(stage,'scripts/native-input-loader.cjs'),module);
nativeModule.filename=path.join(stage,'scripts/native-input-loader.cjs');nativeModule.paths=Module._nodeModulePaths(path.dirname(nativeModule.filename));
nativeModule._compile(nativeDriver.slice(0,nativeCut)+'\nmodule.exports={native,Datastore,util,pins};',nativeModule.filename);
const {native,Datastore,util,pins}=nativeModule.exports;
const baseBytes=fs.readFileSync(path.join(stage,'scripts/startup_init_source_base.cjs'));
assert.ok(['5c41bc2792b1caa518c3c04500888c0b648b634689429c7ad3a4364d7bdacae2','509015e9124795f3adda0997d5eacc495a1b80831b68d95fc078281f78852715'].includes(sha(baseBytes)),'exact earlier CRLF or canonical LF base bytes only');
const base=baseBytes.toString('utf8').replace(/\r\n/g,'\n');
assert.equal(sha(base),'509015e9124795f3adda0997d5eacc495a1b80831b68d95fc078281f78852715');
let code=base.slice(0,base.indexOf('async function checkpointControl('));
// Audit-only correction: reset emission must not create an activations table.
const auditOld="activationRows:e.copy(e.table('activations'))";
assert.equal(code.split(auditOld).length,2);
code=code.replace(auditOld,"activationRows:e.copy(e.tables.get('activations')||[])");
// Capture DECLARED raw reference-fixture construction, before reset(empty)
// clears rows. This is IO input, never a financial method/result snapshot.
const resetAnchor='async function reset(';
assert.equal(code.split(resetAnchor).length,2);
code=code.replace(resetAnchor,`let rawFixtureInitialization,rawTxUtilsInitialization;
const originalFixtureEnvironment=mod.exports.environment;
mod.exports.environment=function(scene){const value=originalFixtureEnvironment(scene);rawFixtureInitialization={profile:'TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1',collections:[...value.tables].map(([collection,rows])=>({collection,rows:value.copy(rows)}))};return value;};
`+resetAnchor);

// Transparent import-boundary observation around the unchanged full module.
const txInvoke='txWrapper(txModule.exports,';assert.equal(code.split(txInvoke).length,2);
code=code.replace(txInvoke,'const txInitEffectStart=e.effects.length;'+txInvoke);
const txAssigned='actualTxUtils=txModule.exports;';assert.equal(code.split(txAssigned).length,2);
code=code.replace(txAssigned,txAssigned+"rawTxUtilsInitialization={profile:'TL-ORIGINAL-TXUTILS-IMPORT-INIT-1',sourceSHA256:'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3',effects:e.copy(e.effects.slice(txInitEffectStart))};");
// Dependency composition only: delayed original TxIndex constructor gets the
// actual original TxUtils object and its real lazy CommonJS Types import.
const oldIndex="TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:new Proxy({},{get(_t,k){return unavailable('TxUtils.'+String(k));}}),transparentIndex:[],process:e.context.process});";
const newIndex="const buildSourceIndex=()=>{TxIndex=compileClass('TxIndex','txIndex',{db:e.storage,console:e.context.console,ClientWrapper,TxUtils:actualTxUtils,require:key=>{if(key==='./types.js')return e.types;throw Error('Unmapped literal TxIndex import '+key);},transparentIndex:[],process:e.context.process});};";
assert.equal(code.split(oldIndex).length,2);code=code.replace(oldIndex,newIndex);
const afterTxUtils='actualTxUtils=txModule.exports;';assert.equal(code.split(afterTxUtils).length,2);code=code.replace(afterTxUtils,afterTxUtils+'buildSourceIndex();');
const oldObserve="['syncIfNecessary','processIncomingBlocks'";assert.equal(code.split(oldObserve).length,2);
code=code.replace(oldObserve,"['syncIndex','initOrLoadTxIndex','constructOrLoadConsensus','constructConsensusFromIndex','processTx','blockHandlerBegin','blockHandlerMid','blockHandlerEnd','syncIfNecessary','processIncomingBlocks'");
const serviceModule=new Module(path.join(stage,'scripts/service-source-loader.cjs'),module);
serviceModule.filename=path.join(stage,'scripts/service-source-loader.cjs');serviceModule.paths=Module._nodeModulePaths(path.dirname(serviceModule.filename));
serviceModule._compile(code+'\nmodule.exports={reset,flush,tag,inventory,state,observeMainInvocations,schedulerState,clock:()=>driverClock,stop:stopResolutionHook,rawFixtureInitialization:()=>rawFixtureInitialization,rawTxUtilsInitialization:()=>rawTxUtilsInitialization,compileClass,promiseSignaled:value=>resolutionSignals.has(value),support:()=>({actualTxUtils,persistenceClass}),access:()=>({e,m,Main,TxIndex}),facts:changes=>Object.assign(productionIOState.facts,structuredClone(changes))};',serviceModule.filename);
const h=serviceModule.exports;
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};}
async function until(predicate,label){const end=Date.now()+10000;while(!predicate()){if(Date.now()>end)throw Error('actual source boundary not reached: '+label);await new Promise(resolve=>setImmediate(resolve));}}
function classHash(file,name){const source=fs.readFileSync(path.join(repo,'fixtures',file),'utf8').replace(/\r\n/g,'\n');return sha(source.slice(source.indexOf('class '+name+' {'),source.lastIndexOf('module.exports = '+name)).trimEnd());}
function rawLiveFacts(){
 const external=Module.createRequire(path.join(process.env.TRADELAYER_JS_ROOT,'package.json')),bitcore=external('bitcore-lib-ltc');
 const admin=native.value.sender.senderAddress,address=bitcore.Address.fromString(admin);
 const parentId='22'.repeat(32),payload=Buffer.from('tl00,service-live-code'),op=Buffer.concat([Buffer.from([0x6a,payload.length]),payload]);
 const pkh=bitcore.Script.fromAddress(address).toBuffer(),scriptType=address.type==='scripthash'?'scripthash':'pubkeyhash';
 const amount=Buffer.alloc(8);amount.writeBigUInt64LE(100000000n);
 const raw=Buffer.concat([Buffer.from('0100000001','hex'),Buffer.from(parentId,'hex').reverse(),Buffer.from('0000000000ffffffff02','hex'),Buffer.alloc(8),Buffer.from([op.length]),op,amount,Buffer.from([pkh.length]),pkh,Buffer.alloc(4)]);
 const id=Buffer.from(crypto.createHash('sha256').update(crypto.createHash('sha256').update(raw).digest()).digest()).reverse().toString('hex');
 const parsed=new bitcore.Transaction(raw);assert.equal(parsed.id,id);assert.equal(parsed.outputs[1].satoshis,100000000);assert.equal(parsed.outputs[1].script.toAddress().toString(),admin);
 const document={txid:id,vin:[{txid:parentId,vout:0,scriptSig:{asm:'',hex:''},sequence:4294967295}],vout:[{n:0,value:0,scriptPubKey:{type:'nulldata',hex:op.toString('hex')}},{n:1,value:1,scriptPubKey:{type:scriptType,hex:pkh.toString('hex'),addresses:[admin]}}]};
 const parent={txid:parentId,vout:[{n:0,value:2,scriptPubKey:{type:scriptType,hex:pkh.toString('hex'),addresses:[admin]}}]};
 return{id,raw:raw.toString('hex'),document,parent};
}
async function scene(mode){
 const live=rawLiveFacts(),block={hash:'service-block11',previousblockhash:'service-block10',height:11,tx:mode==='same-held-live-validity'? [live.id]:[]};
 const failures=mode==='detached-live-rpc-failure'?[{method:'Client.getBlockCount',ordinal:2,name:'Error',code:'EIO',message:'recorded-live-count-failure'}]:[];
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:11},init:'LTC',rows:[structuredClone(native)],mainGenesis:10,maxHeight:10,observeTypes:true,orchestration:{height:11,networkInfo:{networkactive:true,connections:2},blocks:[{height:11,hash:block.hash,block}],failures}};
 await h.reset('configured-service-tail/'+mode,reset);const {e,m,Main,TxIndex}=h.access();
 assert.equal(h.inventory.classes.Main.sha256,classHash('native_genesis/source/main.js','Main'));
 assert.equal(h.inventory.classes.TxIndex.sha256,classHash('legacy_txindex_source_txIndex.txt','TxIndex'));
 h.observeMainInvocations();
 const db=new Datastore();for(const row of e.table('txIndex'))await util.promisify(db.insert.bind(db))(row);e.tables.delete('txIndex');
 const operations=[],entered=deferred(),release=deferred();let firstLiveValidity=true;
 const receiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[method+'Async',async(...args)=>{
  const operation={ordinal:operations.length,atEffect:e.effects.length,method:method+'Async',args:h.tag(args)};operations.push(operation);
  if(mode==='same-held-live-validity'&&method==='update'&&firstLiveValidity&&args[0]?._id===`tx-11-${live.id}`&&Object.hasOwn(args[1]?.$set||{},'valid')){
   firstLiveValidity=false;operation.pendingNativeWrite=true;entered.resolve();await release.promise;operation.pendingNativeWrite=false;
  }
  try{const value=await util.promisify(db[method].bind(db))(...args);operation.returned=h.tag(value);operation.endEffect=e.effects.length;return value;}
  catch(error){operation.thrown={name:error.name,message:error.message};operation.endEffect=e.effects.length;throw error;}
 }]));
 const factory=e.storage.getDatabase;e.storage.getDatabase=function(name,...args){if(name!=='txIndex')return Reflect.apply(factory,this,[name,...args]);operations.push({ordinal:operations.length,atEffect:e.effects.length,method:'getDatabase',args:h.tag([name,...args]),returned:{database:name}});return Promise.resolve(receiver);};
 if(mode==='same-held-live-validity'){
  m.client.getRawTransaction=async function(id,verbose,...args){const value=id===live.id?(verbose===false?live.raw:live.document):id===live.parent.txid?live.parent:undefined;e.effects.push({kind:'rpc-data',method:'Client.getRawTransaction',args:e.copy([id,verbose,...args]),returned:e.copy(value)});return structuredClone(value);};
  m.client.decoderawtransaction=async function(raw){assert.equal(raw,live.raw);const value=structuredClone(live.document);e.effects.push({kind:'rpc-data',method:'Client.decoderawtransaction',args:[raw],returned:e.copy(value)});return value;};
 }
 e.effects.length=0;e.logs.length=0;operations.length=0;
 const caller={status:'NotEntered'},records=[];
 async function snapshot(point,control){await h.flush();records.push({mode,point,control:h.tag(control),caller:h.tag(caller),scheduler:h.schedulerState(),sourceState:h.tag(h.state()),nativeRows:h.tag(await util.promisify(db.find.bind(db))({})),nativeOperations:structuredClone(operations),effects:h.tag(e.effects),logs:h.tag(e.logs)});}
 records.push({mode,point:'input',rawReset:h.tag(reset),rawHarnessConstructorInput:h.tag(h.rawFixtureInitialization()),rawTxUtilsInitialization:h.tag(h.rawTxUtilsInitialization()),rawRPCDocuments:mode==='same-held-live-validity'?h.tag(live):h.tag(undefined),command:h.tag({target:'Main.constructConsensusFromIndex',args:[10]}),classPins:h.inventory.classes,sourceMethods:h.inventory.methods,sourceState:h.tag(h.state()),nativeInput:h.tag(await util.promisify(db.find.bind(db))({}))});
 caller.status='Pending';const promise=m.constructConsensusFromIndex(10);promise.then(value=>{caller.status='Fulfilled';caller.value=h.tag(value);},error=>{caller.status='Rejected';caller.error={name:error.name,message:error.message};});
 if(mode==='same-held-live-validity'){
  let timer;await Promise.race([entered.promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('genuine configured source did not reach live validity await')),10000);})]);clearTimeout(timer);
  await until(()=>caller.status==='Fulfilled','outer catchup return while original live validity is held');
  const held=await util.promisify(db.findOne.bind(db))({_id:`tx-11-${live.id}`});assert.equal(held.valid,undefined);
  const processed=e.tables.get('consensus').find(x=>x._id===live.id);assert.equal(processed.processed,true);assert.equal(processed.params.valid,true);
  assert.equal(h.schedulerState().invocations.find(x=>x.method==='processTx').resolutionSignaled,false);
  await snapshot('catchup-returned-live-original-validity-promise-held',{kind:'OriginalNativeValidityAwaitHeld'});
  const pause=m.setPause();assert.equal(pause,true);await snapshot('pause-does-not-resolve-original-live-validity',{target:'Main.setPause',args:[],returned:pause});
  assert.equal(h.schedulerState().invocations.find(x=>x.method==='processTx').resolutionSignaled,false);assert.equal(operations.filter(x=>x.pendingNativeWrite).length,1);
  release.resolve();await until(()=>e.tables.get('consensus')?.some(x=>x._id==='TrackHeight'&&x.value===11)&&h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===1000),'same live validity resumed to real recursive pause wait');await snapshot('same-held-live-promise-resumed-service-now-pause-wait',{kind:'ReleaseSameOriginalNativeAwait'});
 }else{
  if(mode==='detached-live-rpc-failure')await until(()=>caller.status==='Fulfilled'&&e.effects.some(x=>x.kind==='unhandled-rejection'&&x.thrown.message==='recorded-live-count-failure'),'real discarded incoming Promise rejection event');
  else await until(()=>caller.status==='Fulfilled'&&e.tables.get('consensus')?.some(x=>x._id==='TrackHeight'&&x.value===11)&&h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===10000),'real empty block11 and original live10s timer');
  await snapshot(mode==='detached-live-rpc-failure'?'catchup-returned-detached-live-await-rejected':'catchup-returned-live-empty-block11-timer-pending',{kind:'DrainFiniteReachedJobsOnly'});
  if(mode==='live-iteration-pause-boundary'){
   const pause=m.setPause();assert.equal(pause,true);await snapshot('pause-with-original-10s-wait-still-pending',{target:'Main.setPause',args:[],returned:pause});
   const id=h.clock().next();assert.notEqual(id,undefined);const fired=h.clock().fire(id);await until(()=>h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===1000),'real recursive sync pause wait after firing live10s');await snapshot('same-live-wait-fired-now-original-recursive-sync-paused',{kind:'FireOriginalPendingTimer',id,returned:fired});
  }
 }
 const summary={mode,caller,scheduler:h.schedulerState(),records:records.length,nativeRows:await util.promisify(db.find.bind(db))({}),consensus:e.tables.get('consensus'),persistence:e.tables.get('persistence'),rpcCalls:e.effects.filter(x=>x.kind==='rpc-input'||x.kind==='rpc-data'),unhandled:e.effects.filter(x=>x.kind==='unhandled-rejection'),phases:e.effects.filter(x=>x.kind==='source-function'&&x.target!=='Types.decodePayload'),pendingNative:operations.filter(x=>x.pendingNativeWrite)};
 assert.deepEqual(e.state().unsupported,[],'no unowned financial success substitution');
 assert.equal(caller.status,'Fulfilled');assert.deepEqual(caller.value,h.tag(undefined));
 const incoming=summary.scheduler.invocations.filter(x=>x.method==='processIncomingBlocks');assert.equal(incoming.length,1);
 if(mode==='detached-live-rpc-failure'){
  assert.equal(incoming[0].resolutionSignaled,true);assert.equal(summary.unhandled.length,1);assert.equal(summary.unhandled[0].thrown.message,'recorded-live-count-failure');
  assert.equal(summary.consensus.find(x=>x._id==='MaxProcessedHeight').value,10);assert.ok(!summary.consensus.some(x=>x._id==='TrackHeight'));assert.equal(summary.persistence,undefined);assert.deepEqual(summary.scheduler.clock.timers,[]);
 }else{
  assert.equal(incoming[0].resolutionSignaled,false);assert.equal(summary.unhandled.length,0);assert.equal(summary.consensus.find(x=>x._id==='TrackHeight').value,11);assert.equal(summary.persistence.find(x=>x._id==='block-11').hash,block.hash);
  assert.equal(summary.scheduler.clock.timers.filter(x=>x.state==='Scheduled'&&x.delay===1000).length,1);
  if(mode==='same-held-live-validity'){
   assert.equal(summary.nativeRows.find(x=>x._id===`tx-11-${live.id}`).valid,true);assert.equal(summary.consensus.find(x=>x._id==='MaxProcessedHeight').value,11);
   assert.equal(e.effects.filter(x=>x.collection==='consensus'&&x.op==='update'&&x.args[0]?._id===live.id).length,1);
   assert.equal(summary.scheduler.invocations.find(x=>x.method==='processTx').resolutionSignaled,true);assert.equal(summary.pendingNative.length,0);
  }else{assert.equal(summary.consensus.find(x=>x._id==='MaxProcessedHeight').value,10);assert.equal(summary.scheduler.clock.timers[0].delay,10000);assert.equal(summary.scheduler.clock.timers[0].state,'Fired');}
 }
 return{records,summary};
}
async function parent(){
 const controls=[];for(const mode of ['live-iteration-pause-boundary','detached-live-rpc-failure','same-held-live-validity']){
  const args=[__filename,'--child',mode];if(process.argv.includes('--lock'))args.push('--lock');
  const child=cp.spawnSync(process.execPath,args,{encoding:'utf8',env:{...process.env,NODE_NO_WARNINGS:'1'},windowsHide:true,maxBuffer:64*1024*1024,timeout:30000});
  assert.equal(child.error,undefined);assert.equal(child.status,0,child.stderr);assert.equal(child.signal,null);assert.equal(child.stderr,'');
  const lines=child.stdout.trim().split('\n');assert.equal(lines.length,1);const control=JSON.parse(lines[0]);control.records.at(-1).observerChildBoundary={status:child.status,signal:child.signal,sourceProcessExit:false,pendingBoundaryIsNotSourceCompletion:true};controls.push(control);
 }
 const bytes=controls.flatMap(x=>x.records).map(x=>JSON.stringify(x)).join('\n')+'\n',locked=process.argv.includes('--lock');
 const file=path.join(stage,'fixtures/native_main_service_tail_observer2'+(locked?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 h.stop();console.log(JSON.stringify({profile:'TL-ORIGINAL-CONFIGURED-MAIN-SERVICE-TAIL-CONTROLS-1',node:process.version,numeric:locked?'genuine-9.1.2':'genuine-9.3.1',cases:3,sourceObservations:controls.reduce((n,x)=>n+x.records.length,0),pendingIsNotSourceCompletion:true,fixtureSHA256:sha(bytes),controls:controls.map(x=>x.summary)},null,2));
}
if(process.argv.includes('--child'))scene(process.argv[process.argv.indexOf('--child')+1]).then(control=>{h.stop();fs.writeSync(1,JSON.stringify(control)+'\n');},error=>{console.error(error.stack);process.exitCode=1;});
else parent().catch(error=>{console.error(error.stack);process.exitCode=1;});
