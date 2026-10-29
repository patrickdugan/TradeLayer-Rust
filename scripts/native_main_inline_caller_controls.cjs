'use strict';
// Original Main/TxIndex/TxUtils only. Async DB gates are outside business bodies.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),cp=require('node:child_process'),v8=require('node:v8'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);process.env.PROOF_REPO=repo;
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const observer=fs.readFileSync(path.join(stage,'scripts/inline_await_original_observer.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(sha(observer),'215f551d1f9372676cf5f6c6d6f711b2d9ef0e41612a22e684c66519515e391d');
const cut=observer.indexOf('async function scene(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/inline-source-loader.cjs'),module);loader.filename=path.join(stage,'scripts/inline-source-loader.cjs');loader.paths=Module._nodeModulePaths(path.dirname(loader.filename));
const prefix=observer.slice(0,cut),access='access:()=>({e,m,Main,TxIndex})';assert.equal(prefix.split(access).length,2);
loader._compile(prefix.replace(access,'access:()=>({e,m,Main,TxIndex,actualTxUtils})')+'\nmodule.exports={h,rawLiveFacts,Datastore,util,pins};',loader.filename);
const {h,rawLiveFacts,Datastore,util,pins}=loader.exports;
const cases=['sync-save-second','sync-final-first','init-final-first','initialize-final-first'].flatMap(target=>['fulfill','reject','applied-held'].map(outcome=>({id:target+'/'+outcome,target,outcome})));
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};}
async function until(predicate,label){const end=Date.now()+15000;while(!predicate()){if(Date.now()>end)throw Error('actual inline source boundary not reached: '+label);await new Promise(resolve=>setImmediate(resolve));}}
function failure(facts){return Object.assign(Error(facts.message),{name:facts.name,code:facts.code});}
async function scene(input){
 const live=rawLiveFacts(),initialize=input.target==='initialize-final-first',withTx=input.target==='sync-save-second',first=initialize?2819000:10,last=first+1;
 const blocks=[first,last].map(height=>({hash:'caller-block'+height,previousblockhash:'caller-block'+(height-1),height,tx:withTx&&height===last?[live.id]:[]}));
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:last},init:'LTC',rows:[],mainGenesis:10,maxHeight:initialize?0:10,observeTypes:true,orchestration:{height:last,networkInfo:{networkactive:true,connections:2},blocks:blocks.map(block=>({height:block.height,hash:block.hash,block})),failures:[]}};
 await h.reset('inline-caller/'+input.id,reset);const {e,m,Main,TxIndex,actualTxUtils}=h.access();
 const db=new Datastore(),initialRows=[{_id:'MaxHeight',value:initialize?0:10}];
 for(const row of initialRows)await util.promisify(db.insert.bind(db))(structuredClone(row));e.tables.delete('txIndex');
 const entered=deferred(),release=deferred(),operations=[],trace=[],methodCalls=[],caller={status:'NotEntered'};let selected,gateUsed=false;
 const emit=event=>{trace.push({ordinal:trace.length,atEffect:e.effects.length,...event});};
 const settled=new WeakSet(),promiseEvents=new WeakMap();const stopHooks=v8.promiseHooks.createHook({settled(promise){settled.add(promise);const event=promiseEvents.get(promise);if(event&&!event.resolutionSignaled){event.resolutionSignaled=true;emit({kind:'method-resolution-signaled',call:event.ordinal,target:event.target});}}});
 function watch(receiver,key,target){const original=receiver[key];assert.equal(typeof original,'function');const hash=sha(original.toString());receiver[key]=function(...args){const event={ordinal:methodCalls.length,target,args:h.tag(args),originalSHA256:hash,resolutionSignaled:false};methodCalls.push(event);emit({kind:'method-enter',call:event.ordinal,target,args:h.tag(args)});try{const result=Reflect.apply(original,this,args);if(result&&typeof result.then==='function'){promiseEvents.set(result,event);event.resolutionSignaled=settled.has(result);}else{event.immediate=h.tag(result);event.resolutionSignaled=true;}return result;}catch(error){event.synchronousThrow={name:error.name,message:error.message};event.resolutionSignaled=true;throw error;}};}
 for(const key of ['initialize','initOrLoadTxIndex','syncIndex','blockHandlerBegin','constructOrLoadConsensus','constructConsensusFromIndex','syncIfNecessary','processIncomingBlocks'])watch(Main.prototype,key,'Main.'+key);
 for(const key of ['processBlockData','processTransaction','saveMaxHeight','checkForIndex','getInstance','initializeIndex','findMaxIndexedBlock','extractBlockData','fetchChainTip','fetchBlockData'])watch(TxIndex,key,'TxIndex.'+key);
 watch(TxIndex.prototype,'initializeOrLoadDB','TxIndex.instance.initializeOrLoadDB');
 const errorFacts={name:'Error',code:'EIO',message:'recorded-inline-'+input.target+'-rejection'};
 function targetOperation(method,args){return method==='update'&&args[0]?._id===(input.target==='sync-save-second'?'indexExists':'MaxHeight');}
 const receiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[method+'Async',async(...args)=>{
  const event={ordinal:operations.length,method:method+'Async',args:h.tag(args),startEffect:e.effects.length,phase:'Entered'};operations.push(event);emit({kind:'native-enter',operation:event.ordinal,method:event.method,args:event.args});
  const real=()=>util.promisify(db[method].bind(db))(...args);
  if(!gateUsed&&targetOperation(method,args)){
   gateUsed=true;selected=event;event.selected=true;
   if(input.outcome==='applied-held'){
    const value=await real();event.appliedReturn=h.tag(value);event.phase='AppliedSourceReturnHeld';emit({kind:'native-applied-source-return-held',operation:event.ordinal,returned:h.tag(value)});entered.resolve();await release.promise;throw Error('observer unexpectedly released a declared lost-return gate');
   }
   event.phase='HeldBeforeApply';emit({kind:'native-held-before-apply',operation:event.ordinal});entered.resolve();await release.promise;
   if(input.outcome==='reject'){const error=failure(errorFacts);event.phase='RejectedBeforeApply';event.thrown={name:error.name,message:error.message,code:error.code};event.endEffect=e.effects.length;emit({kind:'native-rejected-before-apply',operation:event.ordinal,thrown:event.thrown});throw error;}
  }
  try{const value=await real();event.phase='Fulfilled';event.returned=h.tag(value);event.endEffect=e.effects.length;emit({kind:'native-fulfilled',operation:event.ordinal,returned:event.returned});return value;}
  catch(error){event.phase='Rejected';event.thrown={name:error.name,message:error.message,code:error.code};event.endEffect=e.effects.length;emit({kind:'native-rejected',operation:event.ordinal,thrown:event.thrown});throw error;}
 }]));
 const factory=e.storage.getDatabase;e.storage.getDatabase=function(category,...args){if(category!=='txIndex')return Reflect.apply(factory,this,[category,...args]);emit({kind:'native-factory',method:'getDatabase',args:h.tag([category,...args])});return Promise.resolve(receiver);};
 m.client.getRawTransaction=async function(id,verbose,...args){const value=id===live.id?(verbose===false?live.raw:live.document):id===live.parent.txid?live.parent:undefined;e.effects.push({kind:'rpc-data',method:'Client.getRawTransaction',args:e.copy([id,verbose,...args]),returned:e.copy(value)});return structuredClone(value);};
 m.client.decoderawtransaction=async function(raw){assert.equal(raw,live.raw);e.effects.push({kind:'rpc-data',method:'Client.decoderawtransaction',args:[raw],returned:e.copy(live.document)});return structuredClone(live.document);};
 e.effects.length=0;e.logs.length=0;trace.length=0;methodCalls.length=0;
 const command=input.target==='sync-save-second'||input.target==='sync-final-first'?{target:'Main.syncIndex',args:[first]}:{target:initialize?'Main.initialize':'Main.initOrLoadTxIndex',args:[]};
 const records=[{kind:'input',case:input.id,rawReset:h.tag(reset),rawHarnessConstructorInput:h.tag(h.rawFixtureInitialization()),rawTxUtilsImport:h.tag(h.rawTxUtilsInitialization()),rawNativeRows:h.tag(initialRows),rawBlocks:h.tag(blocks),rawRPCDocuments:h.tag(withTx?live:undefined),command:h.tag(command),gate:h.tag({target:input.target,outcome:input.outcome,error:input.outcome==='reject'?errorFacts:undefined}),sourcePin:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',classPins:h.inventory.classes,sourceMethods:h.inventory.methods}];
 async function snapshot(point,control){await h.flush();const rows=await util.promisify(db.find.bind(db))({});records.push({kind:'observation',case:input.id,point,control:h.tag(control),caller:h.tag(caller),nativeRows:h.tag(rows),sourceState:h.tag(h.state()),cache:h.tag({mainStatic:{isInitializing:Main.isInitializing,persistenceInitialized:Main.persistenceInitialized},mainSingletonSame:Main.instance===m,indexStaticClient:TxIndex.client,indexStaticParseBlock:TxIndex.parseBlock,indexInstance:TxIndex.instance,indexInstanceClientIsMain:TxIndex.instance.client===m.client,indexStaticClientIsMain:TxIndex.client===m.client,mainIndexPromiseIsInstance:m.txIndex===TxIndex.instance,txUtilsClient:actualTxUtils.client,txUtilsClientIsMain:actualTxUtils.client===m.client}),methodCalls:structuredClone(methodCalls),nativeOperations:structuredClone(operations),trace:structuredClone(trace),effects:h.tag(e.effects),logs:h.tag(e.logs),scheduler:h.schedulerState()});return rows;}
 caller.status='Pending';emit({kind:'external-caller-enter'});
 const call=command.target==='Main.syncIndex'?m.syncIndex(first):initialize?m.initialize():m.initOrLoadTxIndex();
 call.then(value=>{caller.status='Fulfilled';caller.value=h.tag(value);emit({kind:'external-caller-fulfilled',returned:caller.value});},error=>{caller.status='Rejected';caller.error={name:error.name,message:error.message,code:error.code};emit({kind:'external-caller-rejected',thrown:caller.error});});
 if(initialize){
  await until(()=>h.clock().next()!==undefined,'original initialize1500ms wait');await snapshot('original-startup-delay-pending',{kind:'OriginalTimerHeld'});const timer=h.clock().next();assert.equal(h.clock().snapshot().timers.find(x=>x.id===timer).delay,1500);emit({kind:'external-fire-original-timer',timer});h.clock().fire(timer);
 }
 await until(()=>gateUsed&&selected?.phase!=='Entered','selected genuine inline DB await');await entered.promise;const heldRows=await snapshot(input.outcome==='applied-held'?'actual-write-applied-original-source-return-lost':'original-source-held-before-actual-write',{kind:input.outcome==='applied-held'?'HoldSameOriginalReturnAfterRealNeDBApply':'HoldSameOriginalNativeAwait'});
 assert.equal(caller.status,input.target==='sync-save-second'?'Pending':'Fulfilled','real Main caller vs detached final save boundary');
 if(input.outcome!=='applied-held'){
  emit({kind:'external-release-same-native-await'});release.resolve();
  await until(()=>selected.phase==='Fulfilled'||selected.phase==='RejectedBeforeApply','actual original DB completion');
  await until(()=>caller.status!=='Pending','original Main caller after same await release');
  if(input.target!=='sync-save-second'&&input.outcome==='reject')await until(()=>e.effects.some(x=>x.kind==='unhandled-rejection'),'real discarded final save rejection notification');
  else await until(()=>methodCalls.filter(x=>x.target==='TxIndex.saveMaxHeight').every(x=>x.resolutionSignaled),'actual save method resolution signal');
  await snapshot('original-caller-after-same-await-'+input.outcome,{kind:'ReleaseSameOriginalNativeAwait',outcome:input.outcome});

 }
 const finalRows=await util.promisify(db.find.bind(db))({});assert.deepEqual(e.state().unsupported,[],'no unsupported business success supplied');
 const unhandled=e.effects.filter(x=>x.kind==='unhandled-rejection');
 const summary={case:input.id,target:input.target,outcome:input.outcome,caller,records:records.length,selected:structuredClone(selected),rows:finalRows,methodCalls,trace,nativeOperations:operations,unhandled:h.tag(unhandled),logText:e.logs.map(row=>row.values),sourcePromisePending:caller.status==='Pending',detachedFinalSave:input.target!=='sync-save-second'};
 if(input.target==='sync-save-second'){
  assert.equal(caller.status,input.outcome==='applied-held'?'Pending':'Fulfilled');
  assert.ok(finalRows.some(row=>row._id.startsWith('tx-')),'actual transaction write precedes awaited height flag');
  assert.equal(finalRows.find(row=>row._id==='MaxHeight').value,last);
  if(input.outcome==='reject'){assert.equal(unhandled.length,0);assert.ok(e.logs.some(row=>JSON.stringify(row).includes('Error during syncIndex')));assert.equal(methodCalls.filter(x=>x.target==='TxIndex.saveMaxHeight').length,1);}
 }else{
  assert.equal(caller.status,'Fulfilled','unawaited final save does not hold the real Main caller');
  assert.equal(finalRows.find(row=>row._id==='MaxHeight').value,input.outcome==='reject'?initialRows[0].value:last);
  assert.equal(finalRows.some(row=>row._id==='indexExists'),input.outcome==='fulfill');
  assert.equal(unhandled.length,input.outcome==='reject'?1:0);
  assert.ok(methodCalls.find(x=>x.target==='TxIndex.extractBlockData').resolutionSignaled,'actual extractor settled while its final source save may be held');
  if(initialize)assert.ok(methodCalls.some(x=>x.target==='Main.constructOrLoadConsensus'),'real Main.initialize continued past discarded save');
 }
 stopHooks();h.stop();return{records,summary};
}
async function parent(){const controls=[];for(const input of cases){const args=[__filename,'--child',input.id,...(process.argv.includes('--lock')?['--lock']:[])];const child=cp.spawnSync(process.execPath,args,{encoding:'utf8',env:{...process.env,NODE_NO_WARNINGS:'1'},windowsHide:true,maxBuffer:64*1024*1024,timeout:45000});assert.equal(child.error,undefined);assert.equal(child.status,0,child.stderr);assert.equal(child.stderr,'');const result=JSON.parse(child.stdout.trim());result.records.at(-1).observerBoundary={childExit:child.status,sourceProcessExit:false,pendingIsNotSourceCompletion:true};controls.push(result);}const bytes=controls.flatMap(x=>x.records).map(x=>JSON.stringify(x)).join('\n')+'\n';const output=path.join(stage,'fixtures/native_main_inline_callers'+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(output,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(output,bytes);console.log(JSON.stringify({profile:'TL-ORIGINAL-MAIN-INLINE-CALLERS-1',node:process.version,numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',cases:cases.length,observations:controls.reduce((n,x)=>n+x.records.length,0),fixtureSHA256:sha(bytes),controls:controls.map(x=>x.summary)},null,2));}
const input=process.argv.includes('--child')?cases.find(row=>row.id===process.argv[process.argv.indexOf('--child')+1]):null;
(input?scene(input).then(value=>fs.writeSync(1,JSON.stringify(value)+'\n')):parent()).catch(error=>{console.error(error.stack);process.exitCode=1;});
