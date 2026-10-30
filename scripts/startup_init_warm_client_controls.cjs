'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),cp=require('node:child_process'),crypto=require('node:crypto'),vm=require('node:vm'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);process.env.PROOF_REPO=repo;
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const pins=JSON.parse(fs.readFileSync(path.join(stage,'fixtures/startup_init_source_inputs.json'),'utf8'));
const observer=fs.readFileSync(path.join(stage,'scripts/startup_init_observer.cjs'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(observer),pins.observerSHA256);
const cut=observer.indexOf('async function scene(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/startup-init-loader.cjs'),module);loader.filename=path.join(stage,'scripts/startup-init-loader.cjs');loader.paths=Module._nodeModulePaths(path.dirname(loader.filename));loader._compile(observer.slice(0,cut)+'\nmodule.exports={h,Datastore,util};',loader.filename);const {h,Datastore,util}=loader.exports;
const mainSource=fs.readFileSync(path.join(repo,'fixtures/native_genesis/source/main.js'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(mainSource),'006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009');
const dbSource=fs.readFileSync(path.join(repo,'fixtures/legacy_db_source.txt'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(dbSource),pins.dbSourceSHA256);
const initSource=fs.readFileSync(path.join(repo,'fixtures/legacy_init_source.txt'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(initSource),pins.initSourceSHA256);
const cases=[
 {id:'warm/16-null-main-client-throws-getTest-after-db-short-circuit',ownChain:undefined,warm:true,mainClient:'null'},
 {id:'warm/17-undefined-main-client-throws-getTest-after-db-short-circuit',ownChain:undefined,warm:true,mainClient:'undefined'},
];
function gate(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
async function until(test,label){const end=Date.now()+10000;while(!test()){assert.ok(Date.now()<end,'actual source boundary not reached: '+label);await new Promise(r=>setImmediate(r));}}
async function scene(input){
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:0},init:'LTC',rows:[],maxHeight:0,mainGenesis:10,observeTypes:true,orchestration:{height:0,networkInfo:{networkactive:true,connections:2},blocks:[]}};
 await h.reset('startup-init/'+input.id,reset);const {e,m:oldMain}=h.access(),support=h.support(),client=oldMain.client;client.chain=input.ownChain;
 const rawCategories=[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),auto=gate(),clientCalls=[],operations=[],opens=[],constructed=[];let clientOrdinal=0;
 const external=Module.createRequire(path.join(process.env.TRADELAYER_JS_ROOT,'package.json'));assert.equal(external('nedb/package.json').version,'1.8.0');
 const wrapper=e.adapters['./client'],originalClient=wrapper.getInstance;
 wrapper.getInstance=function(...args){
  const ordinal=++clientOrdinal,call={ordinal,args:h.tag(args),sameActualClient:true,settlement:'ForwardedUnchangedRawPromise'};clientCalls.push(call);
  const raw=Reflect.apply(originalClient,this,args);
  if(ordinal===1){call.settlement='Held';return Promise.resolve(raw).then(value=>auto.promise.then(()=>{call.settlement='Fulfilled';e.effects.push({kind:'client-settlement',ordinal,returned:{sameActualClient:value===client}});return value;},error=>{call.settlement='Rejected';call.thrown={name:error.name,message:error.message,code:error.code};e.effects.push({kind:'client-settlement',ordinal,thrown:call.thrown});throw error;}));}
  if(input.rejectInitClient&&ordinal===3)return Promise.resolve(raw).then(()=>{const error=Object.assign(Error('recorded actual global initialize client rejection'),{code:'EIO'});call.settlement='Rejected';call.thrown={name:error.name,message:error.message,code:error.code};e.effects.push({kind:'client-settlement',ordinal,thrown:call.thrown});throw error;});
  return raw;
 };
 class DatastoreIO {
  constructor(options){
   this.category=path.posix.basename(options.filename,'.db');const observation={category:this.category,filename:options.filename,autoload:options.autoload};opens.push(observation);e.effects.push({kind:'datastore-open',...observation});
   if(input.categoryFailure===this.category){const error=Object.assign(Error('recorded datastore constructor rejection '+this.category),{code:'EACCES'});observation.thrown={name:error.name,message:error.message,code:error.code};throw error;}
   this.actual=new Datastore();constructed.push(this);for(const row of rawCategories.find(x=>x.name===this.category)?.rows||[])this.actual.insert(row,error=>{if(error)throw error;});
  }
 }
 for(const method of ['find','findOne','insert','update','remove','count'])DatastoreIO.prototype[method]=function(...args){
  const callback=args.pop(),operation={ordinal:operations.length,category:this.category,method,args:h.tag(args)};operations.push(operation);
  return this.actual[method](...args,(error,value)=>{if(error)operation.thrown={name:error.name,message:error.message};else operation.returned=h.tag(value);callback(error,value);});
 };
 const instantiate=(text,name,required)=>{const module={exports:{}};vm.runInContext('(function(require,module,exports,__dirname){\n'+text+'\n})',e.context,{filename:'unchanged-startup-module/'+name+'.js'})(required,module,module.exports,'/tl-source');return module.exports;};
 e.effects.length=0;e.logs.length=0;
 // The exact original module creates no Promise before its final async IIFE.
 // Capture that actual first creation through V8, without adding a handler.
 let dbModulePromise;
 const stopDbPromiseCapture=require('node:v8').promiseHooks.onInit(promise=>{if(!dbModulePromise)dbModulePromise=promise;});
 const actualDb=instantiate(dbSource,'db',key=>key==='nedb'?DatastoreIO:key==='path'?path.posix:key==='util'?util:key==='./client'?wrapper:(()=>{throw Error('unmapped original DB dependency '+key)})());
 stopDbPromiseCapture();assert.ok(dbModulePromise);
 const originalInitPromise=actualDb.initPromise;assert.ok(originalInitPromise&&typeof originalInitPromise.then==='function');
 const factory=actualDb.getDatabase;actualDb.getDatabase=function(...args){e.effects.push({kind:'database',name:args[0],method:'getDatabase',args:e.copy(args)});return Reflect.apply(factory,this,args);};
 // Original protocol modules retain their existing reference, forwarding all
 // subsequent IO to the SAME actual original Database owner and row stores.
 e.storage.getDatabase=(...args)=>actualDb.getDatabase(...args);e.adapters['./db']=actualDb;
 const actualInit=instantiate(initSource,'init',key=>key==='./client'?wrapper:key==='./db'?actualDb:(()=>{throw Error('unmapped original init dependency '+key)})());
 const TxIndex=h.compileClass('TxIndex','txIndex',{db:actualDb,console:e.context.console,ClientWrapper:wrapper,TxUtils:support.actualTxUtils,require:key=>{if(key==='./types.js')return e.types;throw Error('unmapped original index import '+key);},transparentIndex:[],process:e.context.process});
 const preamble=mainSource.slice(mainSource.indexOf('const GENESIS_BLOCK_HEIGHTS ='),mainSource.indexOf('\nclass Main {'));
 const genesis=vm.runInContext('(function(){'+preamble+';return GENESIS_BLOCK_HEIGHTS;})()',e.context);
 const mainClient=input.mainClient==='null'?null:input.mainClient==='undefined'?undefined:client;
 const Main=h.compileClass('Main','main',{client:mainClient,db:actualDb,TxIndex,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:e.load('vesting'),Persistence:support.persistenceClass,GENESIS_BLOCK_HEIGHTS:genesis,initialize:actualInit,console:e.context.console,COIN:100000000,pause:false,setTimeout:e.context.setTimeout,process:e.context.process,TxUtils:support.actualTxUtils});
 const m=new Main();await h.flush();const records=[],caller={state:'NotEntered'},unhandledIdentities=[];
 const errorIds=new WeakMap(),promiseIds=new WeakMap();let nextError=0,nextPromise=0;
 const errorId=value=>{if(!value||!['object','function'].includes(typeof value))return null;if(!errorIds.has(value))errorIds.set(value,++nextError);return errorIds.get(value);};
 const promiseId=value=>{if(!value||!['object','function'].includes(typeof value))return null;if(!promiseIds.has(value))promiseIds.set(value,++nextPromise);return promiseIds.get(value);};
 const observeUnhandled=(error,promise)=>unhandledIdentities.push({errorIdentity:errorId(error),promiseIdentity:promiseId(promise),sameInternalDbPromise:promise===originalInitPromise,sameCapturedDbModulePromise:promise===dbModulePromise,name:error?.name,message:error?.message,code:error?.code});
 process.on('unhandledRejection',observeUnhandled);
 function snapshot(point,control){
  const database={initialized:actualDb.initialized,initializing:actualDb.initializing,path:actualDb.path,test:actualDb.test,categories:Object.keys(actualDb.databases),sameOriginalInitPromise:actualDb.initPromise===originalInitPromise,initPromiseIdentity:promiseId(actualDb.initPromise),initPromiseSettled:h.promiseSignaled(actualDb.initPromise),modulePromiseIdentity:promiseId(dbModulePromise),modulePromiseSettled:h.promiseSignaled(dbModulePromise),modulePromiseDistinctFromInternal:dbModulePromise!==originalInitPromise};
  records.push({kind:'observation',case:input.id,point,control:h.tag(control),caller:h.tag(caller),database:h.tag(database),clientCalls:structuredClone(clientCalls),datastoreOpens:structuredClone(opens),operations:structuredClone(operations),effects:h.tag(e.effects),logs:h.tag(e.logs),unhandledIdentities:structuredClone(unhandledIdentities),clock:h.clock().snapshot(),main:h.tag({samePrototype:Object.getPrototypeOf(m)===Main.prototype,sameSingleton:Main.instance===m,clientSame:m.client===client,client:m.client===null?null:m.client===undefined?undefined:{chain:m.client.chain,test:m.client.test},genesisBlock:m.genesisBlock,parseBlock:m.parseBlock,chain:m.chain,test:m.test,indexPromiseIdentity:promiseId(m.txIndex),indexPromiseSettled:h.promiseSignaled(m.txIndex)}),dependencies:{dbSame:actualDb===e.adapters['./db'],sameClassClient:TxIndex.client===client}});
 }
 records.push({kind:'input',case:input.id,profile:'TL-ORIGINAL-MAIN-INITIALIZE-COLD-CALLABLE-1',rawReset:h.tag(reset),rawStartupInputs:h.tag(input),rawCategories:h.tag(rawCategories),rawConstructorInput:h.tag(h.rawFixtureInitialization()),sourcePins:{main:sha(mainSource),init:sha(initSource),db:sha(dbSource),classes:h.inventory.classes},boundaries:{client:'same actual recorded client; first DB import getInstance settlement retained; no financial result',storage:'unchanged full original Database opens32 genuine NeDB1.8 memory data handles; original filenames observed; no host file durability claim',main:'unchanged complete Main class and constructor; full Main module-global IIFEs not executed'}});
 if(input.warm){auto.resolve();await until(()=>actualDb.initialized,'real warm DB initialized');await h.flush();}
 caller.state='Pending';const result=m.initialize();caller.promiseIdentity=promiseId(result);result.then(value=>{caller.state='Fulfilled';caller.value=h.tag(value);},error=>{caller.state='Rejected';caller.error={name:error.name,message:error.message,code:error.code};caller.errorIdentity=errorId(error);});await h.flush();snapshot('actual-1500ms-pending',{kind:'EnterActualMainInitialize'});
 async function fire(delay,label){const id=h.clock().next();assert.notEqual(id,undefined,label);const timer=h.clock().snapshot().timers.find(x=>x.id===id);assert.equal(timer.delay,delay,label);assert.deepEqual(h.clock().fire(id),{fired:id});await h.flush();snapshot(label,{kind:'FireOriginalTimer',id,delay});}
 if(input.autoRejectEarly){auto.reject(Object.assign(Error('recorded auto DB client rejection'),{code:'EIO'}));await h.flush();snapshot('same-original-module-db-rejected-before1500',{kind:'RejectOriginalAutoDbClient'});assert.equal(actualDb.initializing,false);assert.equal(unhandledIdentities.length,1);}
 await fire(1500,'same-1500ms-resolved');
 if(input.autoRejectEarly){await fire(500,'same-global-init-500ms-resolved');if(input.categoryFailure){await until(()=>caller.state==='Rejected','new actual DB failure rejects parent with its own error');await h.flush();snapshot('same-new-db-error-distinct-from-original-module-error',{kind:'DrainActualInitializeSuffix'});assert.notEqual(caller.errorIdentity,unhandledIdentities[0].errorIdentity);assert.notEqual(actualDb.initPromise,originalInitPromise);assert.equal(unhandledIdentities.length,1);}else{await until(()=>h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===300),'new actual DB init retries after finalized automatic rejection');await h.flush();snapshot('same-main-real300-after-new-db-init',{kind:'RetainOriginalMainDelay'});assert.equal(actualDb.initialized,true);assert.notEqual(actualDb.initPromise,originalInitPromise);await fire(300,'same-main-300ms-resolved');await until(()=>caller.state==='Fulfilled','same original Main completes after actual new DB init');await h.flush();snapshot('same-original-initialize-suffix-fulfilled',{kind:'DrainActualInitializeSuffix'});assert.equal(unhandledIdentities.length,1);}}
 else if(input.mainClient){assert.equal(caller.state,'Rejected');}
 else if(input.rejectInitClient){assert.equal(caller.state,'Rejected');assert.ok(!h.clock().snapshot().timers.some(x=>x.delay===500));}
 else if(input.finish){if(!input.warm){assert.equal(caller.state,'Pending');assert.ok(!h.clock().snapshot().timers.some(x=>x.delay===300));auto.resolve();await h.flush();snapshot('same-db-auto-init-released',{kind:'ReleaseOriginalAutoDbClient'});}await until(()=>caller.state!=='Pending','actual finite empty initialize completion');await h.flush();snapshot('actual-empty-initialize-completed',{kind:'DrainReachedFiniteJobsOnly'});}
 else {
  assert.equal(caller.state,'Pending');assert.ok(h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===500));
  if(input.autoRelease==='before500'){auto.resolve();await until(()=>actualDb.initialized,'original DB initialization');await h.flush();snapshot('same-db-auto-init-released-before500',{kind:'ReleaseOriginalAutoDbClient'});}
  await fire(500,'same-global-init-500ms-resolved');
  if(input.autoReject||input.categoryFailure||input.autoRelease==='after500'){
   assert.equal(caller.state,'Pending');assert.ok(!h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===300),'Main must await the actual shared DB initPromise');
   if(input.autoReject)auto.reject(Object.assign(Error('recorded auto DB client rejection'),{code:'EIO'}));else auto.resolve();
   await h.flush();snapshot('same-shared-db-init-settled',{kind:input.autoReject?'RejectOriginalAutoDbClient':'ReleaseOriginalAutoDbClient'});
  }
  if(input.autoReject||input.categoryFailure){await until(()=>caller.state==='Rejected','actual source initializer rejection');await h.flush();snapshot('actual-source-rejection-retained',{kind:'DrainReachedFiniteJobsOnly'});assert.equal(caller.state,'Rejected');}
  else{await until(()=>h.clock().snapshot().timers.some(x=>x.state==='Scheduled'&&x.delay===300),'actual Main300ms wait');await h.flush();snapshot('actual-main-300ms-pending',{kind:'RetainOriginalMainDelay'});assert.equal(caller.state,'Pending');assert.equal(actualDb.initialized,true);assert.equal(opens.length,32);if(input.finishAfter300){await fire(300,'same-main-300ms-resolved');await until(()=>caller.state!=='Pending','actual entire empty Initialize suffix returns');await h.flush();snapshot('same-original-initialize-suffix-fulfilled',{kind:'DrainActualInitializeSuffix'});assert.equal(caller.state,'Fulfilled');}}
 }
 if((input.autoReject||input.categoryFailure)&&!input.autoRejectEarly){assert.equal(unhandledIdentities.length,1);assert.equal(unhandledIdentities[0].errorIdentity,caller.errorIdentity,'the real original shared DB failure object reaches both parent and discarded module');assert.equal(unhandledIdentities[0].sameInternalDbPromise,false,'discarded module Promise is distinct from its awaited internal DB promise');assert.equal(unhandledIdentities[0].sameCapturedDbModulePromise,true,'the actual captured module IIFE is the discarded rejecting Promise');}
 assert.deepEqual(e.state().unsupported,[]);process.removeListener('unhandledRejection',observeUnhandled);h.stop();return{records,summary:{case:input.id,caller,records:records.length,categories:Object.keys(actualDb.databases),timers:h.clock().snapshot(),clientCalls,unhandledIdentities}};
}
async function parent(){const outputs=[];for(const item of cases){const child=cp.spawnSync(process.execPath,[__filename,'--child',item.id,...(process.argv.includes('--lock')?['--lock']:[])],{env:{...process.env,NODE_NO_WARNINGS:'1'},windowsHide:true,encoding:'utf8',timeout:30000,maxBuffer:64*1024*1024});assert.equal(child.error,undefined);assert.equal(child.status,0,child.stderr);assert.equal(child.stderr,'');outputs.push(JSON.parse(child.stdout));}const raw=outputs.flatMap(x=>x.records).map(x=>JSON.stringify(x)).join('\n')+'\n',file=path.join(stage,'fixtures/startup_init_warm_client'+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),raw);else fs.writeFileSync(file,raw);console.log(JSON.stringify({profile:'TL-ORIGINAL-MAIN-INITIALIZE-COLD-CALLABLE-1',numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',node:process.version,externalInitializeCalls:cases.length,records:outputs.reduce((n,x)=>n+x.records.length,0),fixtureSHA256:sha(raw),summaries:outputs.map(x=>x.summary)},null,2));}
const selected=process.argv.includes('--child')?cases.find(x=>x.id===process.argv[process.argv.indexOf('--child')+1]):null;(selected?scene(selected).then(value=>fs.writeSync(1,JSON.stringify(value)+'\n')):parent()).catch(error=>{console.error(error.stack);process.exitCode=1;});
