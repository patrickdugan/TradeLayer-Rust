'use strict';
// Full unchanged TxUtils/TxIndex imports + exact unchanged Main class. No
// promise observer is attached to Main.txIndex unless the input asks for one.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),Module=require('node:module'),crypto=require('node:crypto'),cp=require('node:child_process'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const base=fs.readFileSync(path.join(stage,'scripts/ordered_bootstrap_original_source_base.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(sha(base),'509015e9124795f3adda0997d5eacc495a1b80831b68d95fc078281f78852715');
for(const [file,hash]of [['scripts/source_contract_execution_vectors.cjs','cb7af611bc3224057036fcbdb3ec6f13d1984b8402c306b99c5a1f55fb144c64'],['scripts/legacy_numeric_oracle.cjs','e570de41173962d5013c3eb289b86903c3d6bd070be068e6f20c6372d86a7a15'],['scripts/scaling_vectors.cjs','a43a5fcca5894f900d37703574b297ef088eedfa6131935dc90c0805bc8a6498']])assert.equal(sha(fs.readFileSync(path.join(repo,file),'utf8').replace(/\r\n/g,'\n')),hash,'exact reused source observer input '+file);
const cut=base.indexOf('const inventory={');assert.ok(cut>0);
// Reuse only the pinned environment builder. Its original full financial
// modules remain original; no earlier reset/constructor/promise handler runs.
const loader=new Module(path.join(stage,'scripts/bootstrap-source-loader.cjs'),module);loader.filename=path.join(stage,'scripts/bootstrap-source-loader.cjs');loader.paths=Module._nodeModulePaths(path.dirname(loader.filename));
loader._compile(base.slice(0,cut)+'\nmodule.exports={environment:mod.exports.environment,BN:numeric.loadBigNumber()};',loader.filename);
const {environment,BN}=loader.exports;
const pins={
 TxUtils:['source_channel_txutils_reference.txt','38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3'],
 TxIndex:['legacy_txindex_source_txIndex.txt','095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51'],
 Main:['native_genesis/source/main.js','006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009'],
};
const sources=Object.fromEntries(Object.entries(pins).map(([name,[file,hash]])=>{const raw=fs.readFileSync(path.join(repo,'fixtures',file),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(raw),hash);return[name,raw];}));
const external=Module.createRequire(path.join(process.env.TRADELAYER_JS_ROOT||path.resolve(repo,'../../../../tradelayer.js'),'package.json'));
assert.equal(external('bitcore-lib-ltc/package.json').version,'10.10.5');assert.equal(external('big-json/package.json').version,'3.2.0');
function tag(value,seen=new Set()){
 if(value===undefined)return['undefined'];if(value===null)return['null'];if(typeof value==='string'||typeof value==='boolean')return[typeof value,value];
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return['number',Number.isNaN(value)?'NaN':b.toString('hex')];}
 if(typeof value==='function')return['function',value.name];if(Object.prototype.toString.call(value)==='[object Promise]')return['promise'];if(value?.constructor?.name==='BigNumber')return['bignumber',value.valueOf()];
 if(seen.has(value))return['cycle'];seen.add(value);let result;
 if(Array.isArray(value))result=['array',value.map(v=>tag(v,seen))];else if(Object.prototype.toString.call(value)==='[object Map]')result=['map',[...value].map(([k,v])=>[tag(k,seen),tag(v,seen)])];else if(Object.prototype.toString.call(value)==='[object Set]')result=['set',[...value].map(v=>tag(v,seen))];else result=['object',Object.entries(value).map(([k,v])=>[k,tag(v,seen)])];seen.delete(value);return result;
}
const baselineScenarios=[
 {id:'immediate-success-import-before-static'},
 {id:'deferred-import-resolves-before-main',import:'deferred',settleBeforeMain:'resolve'},
 {id:'deferred-import-resolves-after-main',import:'deferred',settleAfterMain:'resolve'},
 {id:'deferred-import-rejects-before-main',import:'deferred',settleBeforeMain:'reject'},
 {id:'deferred-import-rejects-after-main',import:'deferred',settleAfterMain:'reject'},
 {id:'immediate-import-rejects-before-main',import:'reject',flushBeforeMain:true},
 {id:'immediate-import-rejects-same-turn-main',import:'reject'},
 {id:'deferred-static-rejects-after-main',index:'deferred',settleIndexAfterMain:'reject'},
 {id:'immediate-static-rejects-same-turn-main',index:'reject'},
 {id:'both-deferred-reject-import-first',import:'deferred',index:'deferred',rejectOrder:['import','index']},
 {id:'both-deferred-reject-index-first',import:'deferred',index:'deferred',rejectOrder:['index','import']},
 {id:'rejected-import-raw-call-preserves-original-init-reference-error',import:'deferred',settleAfterMain:'reject',rawAfterFailure:true},
 {id:'main-height-rpc-rejection-is-separate-from-bootstrap',rpcFailure:true},
 {id:'static-rejection-explicit-late-handler',index:'deferred',settleIndexAfterMain:'reject',lateHandler:true},
];
const extraScenarios=[
 {id:'static-immediate-reject-handler-before-first-turn',index:'reject',earlyHandler:true},
 {id:'static-deferred-handler-before-reject',index:'deferred',settleIndexAfterMain:'reject',handlerBeforeIndexSettle:true},
 {id:'static-deferred-reject-then-handler-same-turn',index:'deferred',settleIndexAfterMain:'reject',handlerAfterIndexSettle:true},
 {id:'static-notified-late-handler-repeat-turns',index:'deferred',settleIndexAfterMain:'reject',lateHandler:true,repeatTurns:true},
 {id:'both-immediate-reject-one-turn',import:'reject',index:'reject',repeatTurns:true},
 {id:'both-deferred-reject-one-turn-import-first',import:'deferred',index:'deferred',sameTurn:[['import','reject'],['index','reject']]},
 {id:'both-deferred-reject-one-turn-static-first',import:'deferred',index:'deferred',sameTurn:[['index','reject'],['import','reject']]},
 {id:'same-turn-import-resolve-static-reject',import:'deferred',index:'deferred',sameTurn:[['import','resolve'],['index','reject']]},
 {id:'same-turn-static-resolve-import-reject',import:'deferred',index:'deferred',sameTurn:[['index','resolve'],['import','reject']]},
 {id:'both-deferred-resolve-static-first-checkpoints',import:'deferred',index:'deferred',sameTurn:[['index','resolve'],['import','resolve']]},
 {id:'same-error-object-rejects-two-source-promises',import:'deferred',index:'deferred',sameTurn:[['index','reject'],['import','reject']],sharedReason:true},
 {id:'two-late-handlers-one-rejection-handled-notification',index:'deferred',settleIndexAfterMain:'reject',lateHandler:true,secondLateHandler:true,repeatTurns:true},
];
const scenarios=process.argv.includes('--baseline')?baselineScenarios:extraScenarios;
async function flush(){for(let i=0;i<5;i++)await new Promise(r=>setImmediate(r));}
async function child(spec){
 const e=environment({clientProfile:{chain:'LTC',test:false}}),trace=[],frames=[],records=[],unhandled=[],handled=[],objectIds=new WeakMap();let nextId=1,Main,TxIndex,TxUtils,main;
 const id=value=>{if(!value||!['object','function'].includes(typeof value))return null;if(!objectIds.has(value))objectIds.set(value,nextId++);return objectIds.get(value);};
 const reason=value=>({object:id(value),name:value?.name,message:value?.message,code:value?.code,type:typeof value});
 const push=value=>trace.push({at:trace.length,...value});
 const rawFacts={chain:'LTC',test:false,height:11,transactions:[{txid:'raw-bootstrap-transaction',vin:[],vout:[]}]};
 let heightCalls=0,handlerCalls=0;
 const sharedReason=spec.sharedReason?Object.assign(Error('same recorded rejected object'),{name:'RecordedSharedClientError',code:'ECLIENT_SHARED'}):null;
 const clientError=label=>sharedReason||Object.assign(Error('recorded '+label+' initialization rejection'),{name:'RecordedClientInitError',code:'ECLIENT_'+label.toUpperCase()});
 const client={chain:rawFacts.chain,test:rawFacts.test,
  async getBlockCount(...args){const ordinal=++heightCalls;if(spec.rpcFailure){const error=Object.assign(Error('recorded height RPC rejection'),{name:'RecordedRpcError',code:'ERPC_HEIGHT'});push({kind:'rpc',method:'Client.getBlockCount',args:tag(args),ordinal,thrown:reason(error)});throw error;}push({kind:'rpc',method:'Client.getBlockCount',args:tag(args),ordinal,returned:rawFacts.height});return rawFacts.height;},
  async getChain(...args){push({kind:'rpc',method:'Client.getChain',args:tag(args),returned:rawFacts.chain});return rawFacts.chain;},
  async getTest(...args){push({kind:'rpc',method:'Client.getTest',args:tag(args),returned:rawFacts.test});return rawFacts.test;},
  async getRawTransaction(...args){const doc=structuredClone(rawFacts.transactions.find(x=>x.txid===args[0]));push({kind:'rpc',method:'Client.getRawTransaction',args:tag(args),returned:tag(doc)});return doc;},
 };
 id(client);
 const getInstanceCalls=[],pending=new Map();
 const ClientWrapper={getInstance(...args){
  const ordinal=getInstanceCalls.length+1,label=ordinal===1?'import':ordinal===2?'index':'extra-'+ordinal,mode=spec[label]||'resolve';
  const call={ordinal,label,args:tag(args),mode};getInstanceCalls.push(call);push({kind:'client-input',method:'Client.getInstance',...call});
  if(mode==='deferred'){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});pending.set(label,{resolve,reject,call});call.promise=id(promise);return promise;}
  if(mode==='reject'){const error=clientError(label);push({kind:'client-input-return',ordinal,thrown:reason(error)});return Promise.reject(error);}
  push({kind:'client-input-return',ordinal,returnedClient:id(client)});return Promise.resolve(client);
 }};
 // Data-only client boundary shared by every reached actual business module.
 Object.assign(e.adapters['./client'],ClientWrapper);
 process.on('unhandledRejection',(error,promise)=>{const event={kind:'unhandled-rejection',reason:reason(error),promise:id(promise),isMainIndexPromise:!!main&&promise===main.txIndex};unhandled.push(event);push(event);});
 process.on('rejectionHandled',promise=>{const event={kind:'rejection-handled',promise:id(promise),isMainIndexPromise:!!main&&promise===main.txIndex};handled.push(event);push(event);});
 function compile(name,scope){const m={exports:{}};const wrapper=vm.runInContext('(function(exports,require,module){\n'+sources[name]+'\n})',e.context,{filename:'unchanged-full/'+name+'.js',timeout:3000});wrapper(m.exports,key=>{if(!Object.hasOwn(scope,key))throw Error('Unavailable genuine module boundary '+name+':'+key);return scope[key];},m);return m.exports;}
 function snapshot(point,control){const names=[...e.tables.keys()];const tables=tag([...e.tables]);assert.deepEqual([...e.tables.keys()],names);frames.push({case:spec.id,point,control:tag(control),trace:structuredClone(trace),effects:tag(e.effects),logs:tag(e.logs),sourceTables:tables,
  bindings:{txUtilsClientOwn:!!TxUtils&&Object.hasOwn(TxUtils,'client'),txUtilsClient:id(TxUtils?.client),indexClassClientOwn:!!TxIndex&&Object.hasOwn(TxIndex,'client'),indexClassClient:id(TxIndex?.client),indexInstanceClient:TxIndex?.instance?tag(TxIndex.instance.client):tag(undefined),mainClient:id(main?.client),sameMainSingleton:!!main&&Main.instance===main,sameIndexInstance:!!main&&!!TxIndex.instance&&Object.getPrototypeOf(TxIndex.instance)===TxIndex.prototype,indexPromise:id(main?.txIndex)},unhandled:structuredClone(unhandled),rejectionHandled:structuredClone(handled)});}
 function settle(label,kind){const slot=pending.get(label);assert.ok(slot,'actual pending client boundary '+label);pending.delete(label);push({kind:'input-settlement',label,action:kind,ordinal:slot.call.ordinal});if(kind==='resolve')slot.resolve(client);else{const error=clientError(label);push({kind:'client-input-return',ordinal:slot.call.ordinal,thrown:reason(error)});slot.reject(error);}}
 snapshot('raw-reference-construction',{input:spec,rawRPCFacts:rawFacts,fullModules:['TxUtils','TxIndex'],mainProfile:'exact original Main class; full module init/reOrg boundary excluded'});
 TxUtils=compile('TxUtils',{'bitcore-lib-ltc':external('bitcore-lib-ltc'),'bignumber.js':BN,'./txEncoder.js':e.load('txEncoder'),'./consensus.js':e.load('consensus'),'./client':ClientWrapper});
 snapshot('immediately-after-full-txutils-import',{kind:'ImportFullOriginalTxUtils'});
 TxIndex=compile('TxIndex',{'big-json':external('big-json'),'util':require('node:util'),'./txUtils':TxUtils,'./db.js':e.storage,'./client.js':ClientWrapper,'./types.js':e.types});
 snapshot('after-full-txindex-import-before-construction',{kind:'ImportFullOriginalTxIndex'});
 assert.equal(getInstanceCalls.length,1,'TxIndex import alone must not invent static initialization');assert.deepEqual(getInstanceCalls[0].args,tag([]));
 if(spec.settleBeforeMain){settle('import',spec.settleBeforeMain);await flush();snapshot('import-settled-before-main',{kind:'SettleImportBeforeMain'});}
 if(spec.flushBeforeMain){await flush();snapshot('import-turn-before-main',{kind:'DrainOriginalImportJobsOnly'});}
 const full=sources.Main,start=full.indexOf('class Main {'),end=full.lastIndexOf('module.exports = Main'),classBytes=full.slice(start,end).trimEnd();assert.ok(start>=0&&end>start);
 const genesis=vm.runInContext(full.slice(full.indexOf('const GENESIS_BLOCK_HEIGHTS ='),full.indexOf('\nclass Main {'))+'\nGENESIS_BLOCK_HEIGHTS',e.context);
 const scope={client,db:e.storage,TxIndex,TxUtils,TallyMap:e.load('tally'),Consensus:e.load('consensus'),Types:e.types,Logic:e.logic,Orderbook:e.Orderbook,Channels:e.load('channels'),Clearing:e.load('clearing'),VolumeIndex:e.load('volumeIndex'),TradeLayerManager:e.load('vesting'),Persistence:e.load('persistence'),GENESIS_BLOCK_HEIGHTS:genesis,console:e.context.console,COIN:100000000,pause:false,initialize(){throw Error('Unavailable original init module');},setTimeout(){throw Error('Unavailable timer in finite bootstrap profile');},process:e.context.process};
 Main=vm.runInContext('(function('+Object.keys(scope).join(',')+'){\n'+classBytes+'\nreturn Main;})',e.context,{filename:'unchanged-full-class/Main.js',timeout:3000})(...Object.values(scope));
 main=new Main();snapshot('main-constructor-returned-static-init-pending',{kind:'ConstructOriginalMain'});
 assert.equal(getInstanceCalls.length,2);assert.deepEqual(getInstanceCalls[1].args,tag([true]));assert.equal(Object.getPrototypeOf(main),Main.prototype);assert.equal(Main.instance,main);
 function attach(label){push({kind:'explicit-caller-input',method:'CatchActualMainIndexPromise',label});main.txIndex.catch(error=>{handlerCalls++;push({kind:'explicit-catch-result',label,reason:reason(error)});});}
 if(spec.earlyHandler){attach('constructor-return-before-await');snapshot('handler-attached-before-first-source-jobs',{kind:'AttachActualIndexHandlerBeforeTurn'});}
 let returned;try{returned={returned:tag(await main.getCurrentBlockHeight())};}catch(error){returned={thrown:reason(error)};}
 push({kind:'caller-result',method:'Main.getCurrentBlockHeight',...returned});snapshot('main-height-call-settled-before-turn-notification',{target:'Main.getCurrentBlockHeight',args:[],result:returned});
 await flush();snapshot('main-invocation-jobs-and-notifications-drained',{kind:'DrainFiniteOriginalJobs'});
 if(spec.settleAfterMain){settle('import',spec.settleAfterMain);await flush();snapshot('import-settled-after-main',{kind:'SettleImportAfterMain'});}
 if(spec.settleIndexAfterMain){
  if(spec.handlerBeforeIndexSettle){attach('before-client-rejection');snapshot('handler-before-client-settlement',{kind:'AttachActualIndexHandler'});}
  settle('index',spec.settleIndexAfterMain);
  if(spec.handlerAfterIndexSettle){attach('after-reject-before-any-drain');snapshot('handler-after-client-rejection-before-microtasks',{kind:'AttachActualIndexHandler'});}
  await flush();snapshot('index-settled-after-main',{kind:'SettleIndexAfterMain'});
 }
 if(spec.sameTurn){
  for(const [label,action]of spec.sameTurn){settle(label,action);snapshot('client-'+label+'-'+action+'-without-drain',{kind:'SettleActualClientWithoutDrain',label,action});}
  for(let checkpoint=1;checkpoint<=6;checkpoint++){await Promise.resolve();snapshot('microtask-checkpoint-'+checkpoint,{kind:'AdvanceOneActualHostAwaitCheckpoint',checkpoint});}
  await flush();snapshot('same-turn-client-settlements-notified',{kind:'DrainActualFiniteTurn'});
 }
 if(spec.rejectOrder)for(const label of spec.rejectOrder){settle(label,'reject');await flush();snapshot('reject-'+label+'-after-main',{kind:'RejectActualDeferredClient',label});}
 if(spec.rawAfterFailure){let result;try{result={returned:tag(await TxUtils.getRawTransaction(rawFacts.transactions[0].txid))};}catch(error){result={thrown:reason(error)};}snapshot('literal-txutils-use-after-rejected-import',{target:'TxUtils.getRawTransaction',args:[rawFacts.transactions[0].txid],result});assert.equal(result.thrown?.name,'ReferenceError');assert.equal(result.thrown?.message,'init is not defined');assert.equal(trace.filter(x=>x.method==='Client.getRawTransaction').length,0);}
 if(spec.lateHandler){push({kind:'explicit-caller-input',method:'CatchActualMainIndexPromiseAfterNotification'});main.txIndex.catch(error=>{handlerCalls++;push({kind:'explicit-late-catch-result',reason:reason(error)});});await flush();snapshot('explicit-late-caller-handler',{kind:'AttachActualCallerCatch'});assert.equal(handled.length,1);}
 if(spec.secondLateHandler){attach('second-late-catch');await flush();snapshot('second-late-handler-does-not-renotify',{kind:'AttachSecondActualHandler'});assert.equal(handled.length,1);}
 if(spec.repeatTurns){const notified=unhandled.length,handledCount=handled.length;for(let i=0;i<2;i++){await flush();snapshot('repeat-turn-'+(i+1),{kind:'DrainAnotherActualTurn'});assert.equal(unhandled.length,notified);assert.equal(handled.length,handledCount);}}
 assert.equal(pending.size,0);assert.equal(getInstanceCalls.length,2);
 const sameRejected=spec.sameTurn?.filter(x=>x[1]==='reject').length||0;const suppressIndex=spec.earlyHandler||spec.handlerBeforeIndexSettle||spec.handlerAfterIndexSettle;
 const expected=(spec.import==='reject'||spec.settleBeforeMain==='reject'||spec.settleAfterMain==='reject'?1:0)+((spec.index==='reject'||spec.settleIndexAfterMain==='reject')&&!suppressIndex?1:0)+(spec.rejectOrder?2:0)+sameRejected;
 assert.equal(unhandled.length,expected,'actual discarded source init promises notify once each');
 for(const event of unhandled){const original=trace.find(x=>x.kind==='client-input-return'&&x.thrown?.code===event.reason.code);assert.ok(original);assert.equal(event.reason.object,original.thrown.object,'preserve actual rejected Error identity through original async init');if(!spec.sharedReason)assert.equal(event.isMainIndexPromise,event.reason.code==='ECLIENT_INDEX');}
 if(spec.rejectOrder)assert.deepEqual(unhandled.map(x=>x.reason.code),spec.rejectOrder.map(x=>'ECLIENT_'+x.toUpperCase()));
 if(spec.settleBeforeMain==='reject'||spec.flushBeforeMain)assert.ok(frames.find(x=>x.point==='main-constructor-returned-static-init-pending').unhandled.length>0);
 if(spec.import==='deferred'&&!spec.settleBeforeMain)assert.equal(frames.find(x=>x.point==='main-invocation-jobs-and-notifications-drained').bindings.txUtilsClient,null);
 const callerPoint=trace.findIndex(x=>x.kind==='caller-result');
 for(const event of unhandled){const at=trace.findIndex(x=>x.kind===event.kind&&x.promise===event.promise);assert.ok(at>=0);if(spec.settleBeforeMain==='reject'||spec.flushBeforeMain)assert.ok(at<callerPoint);else assert.ok(at>callerPoint,'same-turn or explicit later rejection notification follows actual Main caller completion');}
 assert.equal(TxIndex.instance.client,null,'original static init does not rewrite instance client');
 if(!expected&&!suppressIndex){assert.equal(TxUtils.client,client);assert.equal(TxIndex.client,client);}
 if(suppressIndex){assert.equal(handlerCalls,1);assert.equal(handled.length,0);assert.equal(unhandled.filter(e=>e.isMainIndexPromise).length,0);}
 if(spec.sharedReason){assert.equal(unhandled.length,2);assert.equal(unhandled[0].reason.object,unhandled[1].reason.object);assert.deepEqual(unhandled.map(e=>e.isMainIndexPromise).sort(),[false,true]);}
 if(spec.sameTurn){assert.deepEqual(unhandled.map(e=>e.isMainIndexPromise),spec.sameTurn.filter(x=>x[1]==='reject').map(x=>x[0]==='index'));}
 if(spec.secondLateHandler)assert.equal(handlerCalls,2);
 assert.equal(heightCalls,1);assert.equal(e.state().unsupported.length,0);
 return{spec,frames,summary:{...(process.argv.includes('--baseline')?{}:{handlerCalls,microtaskCheckpoints:frames.filter(f=>f.point.startsWith('microtask-checkpoint')).map(f=>({point:f.point,bindings:f.bindings,unhandled:f.unhandled.length}))}),case:spec.id,observations:frames.length,fullModulePins:pins,mainClassSHA256:sha(classBytes),wrapperCalls:getInstanceCalls,unhandled:unhandled.map(x=>({code:x.reason.code,isMainIndexPromise:x.isMainIndexPromise})),rejectionHandled:handled.length,rawRPCFacts:rawFacts}};
}
function fullMainImportBoundary(){
 const m={exports:{}},imports=[];let result;
 try{vm.runInNewContext(sources.Main,{module:m,exports:m.exports,require(name){imports.push(name);if(name==='events'||name==='util')return require(name);throw Object.assign(Error('Unavailable full Main module dependency: '+name),{name:'SourceBoundaryUnavailable'});},console:{log(){},error(){},warn(){}},process:{exit(){throw Error('unreached source exit');}}},{filename:'unchanged-full/Main.js',timeout:1000});result={returned:tag(m.exports)};}catch(error){result={thrown:{name:error.name,message:error.message}};}
 assert.deepEqual(imports,['events','util','./reOrg.js']);assert.equal(result.thrown?.name,'SourceBoundaryUnavailable');
 return{case:'full-main-module-unavailable-reorg-boundary',point:'unchanged-full-main-module-import-refusal',input:{availableBuiltins:['events','util'],otherModuleCapabilities:'unavailable'},imports,result,exported:tag(m.exports),sourcePin:pins.Main,claim:'No successful full Main module/IIFE initialization is inferred from class controls'};
}
async function parent(){const runs=[];for(const spec of scenarios){const childResult=cp.spawnSync(process.execPath,[__filename,'--child',spec.id,...(process.argv.includes('--lock')?['--lock']:[]),...(process.argv.includes('--baseline')?['--baseline']:[])],{env:{...process.env,PROOF_REPO:repo,NODE_NO_WARNINGS:'1'},windowsHide:true,encoding:'utf8',maxBuffer:32*1024*1024,timeout:30000});assert.equal(childResult.error,undefined);assert.equal(childResult.status,0,childResult.stderr);assert.equal(childResult.stderr,'');runs.push(JSON.parse(childResult.stdout));}
 const boundary=fullMainImportBoundary(),bytes=[...runs.flatMap(x=>x.frames),boundary].map(x=>JSON.stringify(x)).join('\n')+'\n',file=path.join(stage,(process.argv.includes('--baseline')?'fixtures/native_main_bootstrap_failures':'fixtures/native_main_bootstrap_microtasks')+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 console.log(JSON.stringify({profile:process.argv.includes('--baseline')?'TL-ORIGINAL-CLIENT-IMPORT-INIT-FAILURE-ORDER-13-1':'TL-ORIGINAL-BOOTSTRAP-MICROTASK-CONTROLS-13-1',node:process.version,numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',cases:runs.length,observations:runs.reduce((n,x)=>n+x.frames.length,0)+1,explicitUnavailableFullMainImports:1,fixtureSHA256:sha(bytes),scope:'unchanged full TxUtils+TxIndex; unchanged complete Main class, full Main module IIFEs excluded; recorded client facade facts, no business answer inputs',summaries:runs.map(x=>x.summary),boundary},null,2));}
if(process.argv.includes('--child')){const spec=scenarios.find(x=>x.id===process.argv[process.argv.indexOf('--child')+1]);assert.ok(spec);child(spec).then(result=>fs.writeSync(1,JSON.stringify(result)+'\n'),error=>{console.error(error.stack);process.exitCode=1;});}else parent().catch(error=>{console.error(error.stack);process.exitCode=1;});
