'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);process.env.PROOF_REPO=repo;
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const pins=JSON.parse(fs.readFileSync(path.join(stage,'fixtures/null_sync_source_inputs.json'),'utf8'));
const observer=fs.readFileSync(path.join(stage,'scripts/null_sync_observer.cjs'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(observer),pins.observerSHA256);
const cut=observer.indexOf('async function scene(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/null-sync-input-loader.cjs'),module);loader.filename=path.join(stage,'scripts/null-sync-input-loader.cjs');loader.paths=Module._nodeModulePaths(path.dirname(loader.filename));loader._compile(observer.slice(0,cut)+'\nmodule.exports={h};',loader.filename);const {h}=loader.exports;
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
];
function gate(){let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};}
async function until(predicate,label){const end=Date.now()+10000;while(!predicate()){assert.ok(Date.now()<end,'actual source boundary: '+label);await new Promise(r=>setImmediate(r));}}
async function scene(input){
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:input.height},init:'LTC',rows:[],mainGenesis:10,maxHeight:input.max,observeTypes:true,orchestration:{height:input.height,networkInfo:{networkactive:true,connections:2},blocks:[],failures:input.failures||[]}};
 await h.reset('null-sync/'+input.id,reset);const {e,m,TxIndex}=h.access();h.observeMainInvocations();
 if(input.dbFailure)e.setFailure(input.dbFailure);
 const childGate=gate(),parentGate=gate(),boundary=[];let heldChild=false,heldParent=false;
 // The factual DB adapter caches this SAME receiver. Wrap only its selected
 // IO methods before entering source, preserving the original factory Promise
 // and every unheld method Promise without an added fulfillment reaction.
 const actualDb=await e.storage.getDatabase('txIndex');
 for(const key of ['findOneAsync','insertAsync']){
  const fn=actualDb[key];actualDb[key]=function(...values){
    const hold=!heldChild&&((input.holdRead&&key==='findOneAsync'&&values[0]?._id==='MaxHeight')||(input.holdInsert&&key==='insertAsync'));
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
 e.effects.length=0;e.logs.length=0;const caller={state:'NotEntered'},records=[];
 records.push({kind:'input',case:input.id,profile:'TL-ORIGINAL-MAIN-NULL-SYNC-DETACHED-1',rawReset:h.tag(reset),rawCommand:h.tag({target:'Main.syncIndex',args:[null]}),rawBoundaryProfile:h.tag({holdRead:!!input.holdRead,holdInsert:!!input.holdInsert,holdParentTip:!!input.holdParentTip,...(input.dbFailure?{dbFailure:input.dbFailure}:{})}),rawHarnessConstructorInput:h.tag(h.rawFixtureInitialization()),rawTxUtilsImport:h.tag(h.rawTxUtilsInitialization()),sourcePins:h.inventory.classes});
 function snap(point){records.push({kind:'observation',case:input.id,point,caller:h.tag(caller),boundary:structuredClone(boundary),scheduler:h.schedulerState(),effects:h.tag(e.effects),logs:h.tag(e.logs),rows:h.tag(e.tables.get('txIndex')||[]),sourceState:h.tag(h.state()),index:h.tag({parseBlock:TxIndex.parseBlock})});}
 caller.state='Pending';const actual=m.syncIndex(null);actual.then(value=>{caller.state='Fulfilled';caller.returned=h.tag(value);},error=>{caller.state='Rejected';caller.error=h.tag({name:error.name,message:error.message,code:error.code});});
 await h.flush();snap('initial-reached-source-boundary');
 if(input.holdRead||input.holdInsert){assert.ok(heldChild,'actual original child reached declared DB boundary');if(!input.holdParentTip)assert.equal(caller.state,'Fulfilled');childGate.resolve();await h.flush();snap('same-detached-child-database-resumed');}
 if(input.holdParentTip){assert.ok(heldParent);assert.equal(caller.state,'Pending');parentGate.resolve();await h.flush();snap('same-parent-tip-resumed');}
 await until(()=>caller.state!=='Pending','external caller settlement');await h.flush();snap('finite-reached-source-jobs-drained');
 const invocations=h.schedulerState().invocations,init=invocations.filter(x=>x.method==='initOrLoadTxIndex');assert.equal(init.length,1,'exact real detached initOrLoad invocation');assert.ok(init[0].resolutionSignaled,'detached child settled without attaching a handler');
 const unhandled=e.effects.filter(x=>x.kind==='unhandled-rejection');if([2,8,9].some(n=>input.id.includes('/0'+n+'-')))assert.equal(unhandled.length,1,'real discarded child rejects unhandled');else assert.equal(unhandled.length,0);
 assert.deepEqual(e.state().unsupported,[]);h.stop();return{records,summary:{case:input.id,caller,childSettled:init[0].resolutionSignaled,invocations:invocations.map(({method,resolutionSignaled})=>({method,resolutionSignaled})),unhandled:h.tag(unhandled),records:records.length}};
}
async function parent(){const selected=cases.slice(0,10),outputs=[];for(const item of selected){const child=cp.spawnSync(process.execPath,[__filename,'--child',item.id,...(process.argv.includes('--lock')?['--lock']:[])],{env:{...process.env,NODE_NO_WARNINGS:'1'},windowsHide:true,encoding:'utf8',timeout:30000,maxBuffer:64*1024*1024});assert.equal(child.error,undefined);assert.equal(child.status,0,child.stderr);assert.equal(child.stderr,'');outputs.push(JSON.parse(child.stdout));}const raw=outputs.flatMap(x=>x.records).map(x=>JSON.stringify(x)).join('\n')+'\n',file=path.join(stage,'fixtures/null_sync_first10'+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),raw);else fs.writeFileSync(file,raw);console.log(JSON.stringify({profile:'TL-ORIGINAL-MAIN-NULL-SYNC-DETACHED-1',numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',node:process.version,externalNullCalls:10,records:outputs.reduce((n,x)=>n+x.records.length,0),fixtureSHA256:sha(raw),summaries:outputs.map(x=>x.summary)},null,2));}
const selected=process.argv.includes('--child')?cases.find(x=>x.id===process.argv[process.argv.indexOf('--child')+1]):null;(selected?scene(selected).then(value=>fs.writeSync(1,JSON.stringify(value)+'\n')):parent()).catch(error=>{console.error(error.stack);process.exitCode=1;});
