'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);process.env.PROOF_REPO=repo;
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const pins=JSON.parse(fs.readFileSync(path.join(stage,'fixtures/null_live_source_inputs.json'),'utf8'));
const observer=fs.readFileSync(path.join(stage,'scripts/null_live_observer.cjs'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(observer),pins.observerSHA256);
const cut=observer.indexOf('async function scene(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/null-sync-input-loader.cjs'),module);loader.filename=path.join(stage,'scripts/null-sync-input-loader.cjs');loader.paths=Module._nodeModulePaths(path.dirname(loader.filename));loader._compile(observer.slice(0,cut)+'\nmodule.exports={h,rawLiveFacts,native,Datastore,util};',loader.filename);const {h,rawLiveFacts,native,Datastore,util}=loader.exports;
const cases=[
 {id:'first10/01-child-read-held-parent-returns',height:10,max:10,holdRead:true},
 {id:'first10/02-child-read-rejects-unhandled',height:10,max:10,holdRead:true,dbFailure:{collection:'txIndex',op:'findOne',skip:0,message:'recorded detached child read rejection'}},
 {id:'first10/03-child-initializer-write-held',height:10,max:0,holdInsert:true},
 {id:'first10/04-parent-tip-held-child-finishes-first',height:10,max:10,holdParentTip:true},
 {id:'first10/05-both-held-release-child-first',height:10,max:10,holdRead:true,holdParentTip:true},
 {id:'first10/06-parent-tip-rejects-caught',height:10,max:10,failures:[{method:'Client.getBlockCount',ordinal:1,code:'EIO',message:'recorded parent height rejection'}]},
 {id:'first10/07-child-tip-rejects-caught',height:10,max:10,failures:[{method:'Client.getBlockCount',ordinal:2,code:'EIO',message:'recorded child height rejection'}]},
 {id:'first10/08-child-initializer-rejection-unhandled',height:10,max:0,dbFailure:{collection:'txIndex',op:'insert',skip:0,message:'recorded detached initializer rejection'}},
 {id:'first10/09-child-returned-consensus-rejection-unhandled',height:10,max:10,dbFailure:{collection:'txIndex',op:'find',skip:0,message:'recorded detached consensus rejection'}},
 {id:'first10/10-null-tip-parent-consensus-child-return',height:null,max:10},
 {id:'extra/11-parent-releases-before-held-child',height:10,max:10,holdRead:true,holdParentTip:true,parentFirst:true},
 {id:'extra/12-null-tip-zero-max-child-insert-held',height:null,max:0,holdInsert:true},
 {id:'extra/13-null-tip-null-max-child-insert-held',height:null,max:10,patchMax:null,holdInsert:true},
 {id:'extra/14-undefined-tip-parent-consensus',height:undefined,max:10},
 {id:'extra/15-child-extract-undefined-block-caught',height:11,max:10},
 {id:'extra/16-child-genesis-read-rejection-unhandled',height:10,max:0,dbFailure:{collection:'txIndex',op:'findOne',skip:1,message:'recorded detached genesis read rejection'},expectedUnhandled:1},
 {id:'extra/17-child-factory-rejection-unhandled',height:10,max:10,factoryFailure:{collection:'txIndex',skip:0,message:'recorded detached factory rejection'},expectedUnhandled:1},
 {id:'extra/18-child-unhandled-before-parent-rejection-settles',height:10,max:10,holdRead:true,holdParentTip:true,dbFailure:{collection:'txIndex',op:'findOne',skip:0,message:'recorded held detached read rejection'},failures:[{method:'Client.getBlockCount',ordinal:1,code:'EIO',message:'recorded held parent height rejection'}],expectedUnhandled:1},
 {id:'extra/19-child-string-max-uses-real-consensus',height:10,max:'10'},
 {id:'extra/20-child-abstract-object-equality-extracts-genesis',height:10,max:'[object Object]',blocks:[{height:10,hash:'null-real10',block:{hash:'null-real10',previousblockhash:'null-real9',height:10,tx:[]}}]},
];
cases.push({id:'live/21-null-child-returns-with-real-live-validity-held',height:11,max:11,holdValidity:true});
function gate(){let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};}
async function until(predicate,label){const end=Date.now()+10000;while(!predicate()){assert.ok(Date.now()<end,'actual source boundary: '+label);await new Promise(r=>setImmediate(r));}}
async function scene(input){
 const live=input.holdValidity?rawLiveFacts():null;
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:input.height},init:'LTC',rows:live?[structuredClone(native)]:[],mainGenesis:10,maxHeight:input.max,observeTypes:true,orchestration:{height:input.height,networkInfo:{networkactive:true,connections:2},blocks:live?[{height:11,hash:'null-live11',block:{hash:'null-live11',previousblockhash:'null-live10',height:11,tx:[live.id]}}]:input.blocks||[],failures:input.failures||[]}};
 await h.reset('null-sync/'+input.id,reset);const {e,m,TxIndex}=h.access();h.observeMainInvocations();
 if(live){
  m.client.getRawTransaction=async function(id,verbose,...args){const value=id===live.id?(verbose===false?live.raw:live.document):id===live.parent.txid?live.parent:undefined;e.effects.push({kind:'rpc-data',method:'Client.getRawTransaction',args:e.copy([id,verbose,...args]),returned:e.copy(value)});return structuredClone(value);};
  m.client.decoderawtransaction=async function(raw){assert.equal(raw,live.raw);const value=structuredClone(live.document);e.effects.push({kind:'rpc-data',method:'Client.decoderawtransaction',args:[raw],returned:e.copy(value)});return value;};
 }

 if(input.dbFailure)e.setFailure(input.dbFailure);
 if(Object.hasOwn(input,'patchMax'))e.tables.get('txIndex').find(row=>row._id==='MaxHeight').value=input.patchMax;
 const childGate=gate(),parentGate=gate(),boundary=[],nativeOperations=[];let heldChild=false,heldParent=false,nativeDb;
 if(live){
  nativeDb=new Datastore();for(const row of e.tables.get('txIndex')||[])await util.promisify(nativeDb.insert.bind(nativeDb))(row);e.tables.delete('txIndex');
  const receiver=Object.fromEntries(['findOne','find','insert','update','remove'].map(method=>[method+'Async',async(...args)=>{
   const operation={ordinal:nativeOperations.length,atEffect:e.effects.length,method:method+'Async',args:h.tag(args)};nativeOperations.push(operation);
   try{const value=await util.promisify(nativeDb[method].bind(nativeDb))(...args);operation.returned=h.tag(value);operation.endEffect=e.effects.length;return value;}catch(error){operation.thrown={name:error.name,message:error.message};operation.endEffect=e.effects.length;throw error;}
  }]));
  const factory=e.storage.getDatabase;e.storage.getDatabase=function(name,...args){if(name!=='txIndex')return Reflect.apply(factory,this,[name,...args]);e.effects.push({kind:'database',name,method:'getDatabase',args:e.copy([name,...args]),returned:{database:name}});return Promise.resolve(receiver);};
 }
 // The factual DB adapter caches this SAME receiver. Wrap only its selected
 // IO methods before entering source, preserving the original factory Promise
 // and every unheld method Promise without an added fulfillment reaction.
 const actualDb=await e.storage.getDatabase('txIndex');
 for(const key of ['findOneAsync','insertAsync','updateAsync']){
  const fn=actualDb[key];actualDb[key]=function(...values){
    const hold=!heldChild&&((input.holdRead&&key==='findOneAsync'&&values[0]?._id==='MaxHeight')||(input.holdInsert&&key==='insertAsync')||(live&&key==='updateAsync'&&values[0]?._id===`tx-11-${live.id}`&&Object.hasOwn(values[1]?.$set||{},'valid')));
    if(!hold)return Reflect.apply(fn,this,values);
    heldChild=true;boundary.push({kind:'ChildDatabaseAwaitEntered',method:key,args:h.tag(values)});
    return childGate.promise.then(()=>{boundary.push({kind:'ChildDatabaseAwaitReleased',method:key});return Reflect.apply(fn,actualDb,values);});
  };
 }
 if(input.holdParentTip){const original=m.client.getBlockCount;m.client.getBlockCount=function(...args){
  const raw=Reflect.apply(original,this,args);
  if(heldParent)return raw;
  heldParent=true;boundary.push({kind:'ParentTipAwaitEntered',args:h.tag(args)});
  return Promise.resolve(raw).then(value=>parentGate.promise.then(()=>{boundary.push({kind:'ParentTipAwaitReleased'});return value;}),error=>parentGate.promise.then(()=>{boundary.push({kind:'ParentTipAwaitReleasedRejected'});throw error;}));
 };}
 if(input.factoryFailure)h.factoryFailureInput(input.factoryFailure);
 e.effects.length=0;e.logs.length=0;const caller={state:'NotEntered'},records=[];
 records.push({kind:'input',case:input.id,profile:'TL-ORIGINAL-MAIN-NULL-SYNC-DETACHED-1',rawReset:h.tag(reset),rawCommand:h.tag({target:'Main.syncIndex',args:[null]}),rawBoundaryProfile:h.tag({holdRead:!!input.holdRead,holdInsert:!!input.holdInsert,holdParentTip:!!input.holdParentTip,...(input.dbFailure?{dbFailure:input.dbFailure}:{}),...(input.factoryFailure?{factoryFailure:input.factoryFailure}:{}),...(Object.hasOwn(input,'patchMax')?{rawMaxPatch:input.patchMax}:{}),...(input.parentFirst?{parentFirst:true}:{}),...(live?{holdLiveValidity:true,nativeDatabase:'genuine-nedb-1.8.0'}:{})}),...(live?{rawRPCDocuments:h.tag(live)}:{}),rawHarnessConstructorInput:h.tag(h.rawFixtureInitialization()),rawTxUtilsImport:h.tag(h.rawTxUtilsInitialization()),sourcePins:h.inventory.classes});
 async function snap(point){records.push({kind:'observation',case:input.id,point,caller:h.tag(caller),boundary:structuredClone(boundary),scheduler:h.schedulerState(),effects:h.tag(e.effects),logs:h.tag(e.logs),rows:h.tag(nativeDb?await util.promisify(nativeDb.find.bind(nativeDb))({}):e.tables.get('txIndex')||[]),...(live?{nativeOperations:structuredClone(nativeOperations)}:{}),sourceState:h.tag(h.state()),index:h.tag({parseBlock:TxIndex.parseBlock})});}
 caller.state='Pending';const actual=m.syncIndex(null);actual.then(value=>{caller.state='Fulfilled';caller.returned=h.tag(value);},error=>{caller.state='Rejected';caller.error=h.tag({name:error.name,message:error.message,code:error.code});});
 if(live){try{await until(()=>heldChild&&h.schedulerState().invocations.some(x=>x.method==='initOrLoadTxIndex'&&x.resolutionSignaled),'real null child returned while discarded live native validity is held');}catch(error){fs.writeFileSync(path.join(stage,'evidence/live-boundary-debug.json'),JSON.stringify({caller,boundary,scheduler:h.schedulerState(),effects:h.tag(e.effects),logs:h.tag(e.logs),nativeRows:h.tag(e.tables.get('txIndex')||[]),unsupported:e.state().unsupported}));throw error;}}
 await h.flush();await snap('initial-reached-source-boundary');
 if(input.parentFirst){assert.ok(heldParent);assert.equal(caller.state,'Pending');parentGate.resolve();await h.flush();await snap('same-parent-tip-resumed');}
 if(input.holdRead||input.holdInsert){assert.ok(heldChild,'actual original child reached declared DB boundary');if(!input.holdParentTip)assert.equal(caller.state,'Fulfilled');childGate.resolve();await h.flush();await snap('same-detached-child-database-resumed');}
 if(input.holdParentTip&&!input.parentFirst){assert.ok(heldParent);assert.equal(caller.state,'Pending');parentGate.resolve();await h.flush();await snap('same-parent-tip-resumed');}
 await until(()=>caller.state!=='Pending','external caller settlement');await h.flush();await snap('finite-reached-source-jobs-drained');
 const invocations=h.schedulerState().invocations,init=invocations.filter(x=>x.method==='initOrLoadTxIndex');assert.equal(init.length,1,'exact real detached initOrLoad invocation');assert.ok(init[0].resolutionSignaled,'detached child settled without attaching a handler');
 const unhandled=e.effects.filter(x=>x.kind==='unhandled-rejection');const expectedUnhandled=input.expectedUnhandled??([2,8,9].some(n=>input.id.includes('/0'+n+'-'))?1:0);assert.equal(unhandled.length,expectedUnhandled,'real discarded child rejection notifications');
 assert.deepEqual(e.state().unsupported,[]);h.stop();return{records,summary:{case:input.id,caller,childSettled:init[0].resolutionSignaled,invocations:invocations.map(({method,resolutionSignaled})=>({method,resolutionSignaled})),unhandled:h.tag(unhandled),records:records.length}};
}
async function parent(){const selected=process.argv.includes('--first-ten')?cases.slice(0,10):cases,outputs=[];for(const item of selected){const child=cp.spawnSync(process.execPath,[__filename,'--child',item.id,...(process.argv.includes('--lock')?['--lock']:[])],{env:{...process.env,NODE_NO_WARNINGS:'1'},windowsHide:true,encoding:'utf8',timeout:30000,maxBuffer:64*1024*1024});assert.equal(child.error,undefined);assert.equal(child.status,0,child.stderr);assert.equal(child.stderr,'');outputs.push(JSON.parse(child.stdout));}const raw=outputs.flatMap(x=>x.records).map(x=>JSON.stringify(x)).join('\n')+'\n',file=path.join(stage,'fixtures/null_sync_first10'+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),raw);else fs.writeFileSync(file,raw);console.log(JSON.stringify({profile:'TL-ORIGINAL-MAIN-NULL-SYNC-DETACHED-1',numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',node:process.version,externalNullCalls:selected.length,records:outputs.reduce((n,x)=>n+x.records.length,0),fixtureSHA256:sha(raw),summaries:outputs.map(x=>x.summary)},null,2));}
const selected=process.argv.includes('--child')?cases.find(x=>x.id===process.argv[process.argv.indexOf('--child')+1]):null;(selected?scene(selected).then(value=>fs.writeSync(1,JSON.stringify(value)+'\n')):parent()).catch(error=>{console.error(error.stack);process.exitCode=1;});
