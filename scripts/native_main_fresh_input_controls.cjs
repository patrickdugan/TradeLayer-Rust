'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),assert=require('node:assert/strict'),cp=require('node:child_process');
const stage=__dirname,repo=path.resolve(process.env.PROOF_REPO||path.join(__dirname,'..'));
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const original=fs.readFileSync(path.join(repo,'scripts/native_main_service_tail_observer2_controls.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(sha(original),'eccac0f9e43b18d64c61fbf73e7523361da95a6b9c85be619053c05d6c32796c');
const marker='async function scene(';assert.equal(original.split(marker).length,2);
let prefix=original.slice(0,original.indexOf(marker));
const location="path.join(stage,'scripts/native_service_source_base.cjs')";assert.equal(prefix.split(location).length,2);prefix=prefix.replace(location,"path.join(repo,'scripts/native_service_source_base.cjs')");
const loader=new Module(path.join(stage,'unchanged-source-loader.cjs'),module);loader.filename=path.join(stage,'unchanged-source-loader.cjs');loader.paths=Module._nodeModulePaths(repo);
loader._compile(prefix+'\nmodule.exports={h,rawLiveFacts,Datastore,util,deferred,until,native};',loader.filename);
const {h,rawLiveFacts,Datastore,util,deferred,until}=loader.exports;
const modes=['tip-captured','network-captured','hash-captured','block-captured','fetch-retry-fresh-block','recovery-fresh-network','incoming-fresh-tip','raw-captured','decoded-document-captured','validity-held-fresh-facts'];
function block(height,hash,tx=[]){return{height,hash,previousblockhash:'fresh'+(height-1),tx};}
async function scene(mode){
 const live=rawLiveFacts(),financial=['raw-captured','decoded-document-captured','validity-held-fresh-facts'].includes(mode);
 const first=block(11,'fresh11',financial?[live.id]:[]),second=block(12,'fresh12',[]),changed=block(11,'changed11',[]);
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:11},init:'LTC',rows:[],mainGenesis:10,maxHeight:10,observeTypes:true,
  orchestration:{height:11,networkInfo:{networkactive:mode!=='recovery-fresh-network',connections:2},blocks:[{height:11,hash:first.hash,block:first}],failures:mode==='fetch-retry-fresh-block'?[{method:'Client.getBlockHash',ordinal:1,code:'ETIMEDOUT',message:'fresh-input real first fetch timeout'}]:[]}};
 await h.reset('fresh-input/'+mode,reset);const {e,m}=h.access();h.observeMainInvocations();
 const native=new Datastore();for(const row of e.table('txIndex'))await util.promisify(native.insert.bind(native))(row);e.tables.delete('txIndex');
 const nativeOperations=[],held=deferred(),release=deferred();let holdConsumed=false;
 const receiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[method+'Async',async(...args)=>{
  const operation={ordinal:nativeOperations.length,atEffect:e.effects.length,method:method+'Async',args:h.tag(args)};nativeOperations.push(operation);
  if(mode==='validity-held-fresh-facts'&&!holdConsumed&&method==='update'&&args[0]?._id===`tx-11-${live.id}`&&Object.hasOwn(args[1]?.$set||{},'valid')){holdConsumed=true;operation.pending=true;held.resolve();await release.promise;operation.pending=false;}
  try{const value=await util.promisify(native[method].bind(native))(...args);operation.returned=h.tag(value);operation.endEffect=e.effects.length;return value;}
  catch(error){operation.thrown={name:error.name,message:error.message};operation.endEffect=e.effects.length;throw error;}
 }]));
 const factory=e.storage.getDatabase;e.storage.getDatabase=function(name,...args){if(name!=='txIndex')return Reflect.apply(factory,this,[name,...args]);nativeOperations.push({ordinal:nativeOperations.length,atEffect:e.effects.length,method:'getDatabase',args:h.tag([name,...args])});return Promise.resolve(receiver);};
 let documents=[{id:live.id,raw:live.raw,document:structuredClone(live.document)},{id:live.parent.txid,document:structuredClone(live.parent)}];
 const documentCounts={};
 m.client.getRawTransaction=async function(id,verbose,...rest){
  documentCounts.getRawTransaction=(documentCounts.getRawTransaction||0)+1;
  const row=documents.find(row=>row.id===id),value=structuredClone(verbose===false?row?.raw:row?.document);
  e.effects.push({kind:'rpc-data',method:'Client.getRawTransaction',args:e.copy([id,verbose,...rest]),returned:e.copy(value)});
  if(mode==='raw-captured'&&!holdConsumed&&id===live.id&&verbose===false){holdConsumed=true;held.resolve();await release.promise;}
  return value;
 };
 m.client.decoderawtransaction=async function(raw){
  documentCounts.decoderawtransaction=(documentCounts.decoderawtransaction||0)+1;
  const value=structuredClone(documents.find(row=>row.raw===raw)?.document);
  e.effects.push({kind:'rpc-data',method:'Client.decoderawtransaction',args:e.copy([raw]),returned:e.copy(value)});
  if(mode==='decoded-document-captured'&&!holdConsumed){holdConsumed=true;held.resolve();await release.promise;}
  return value;
 };
 const captureMethod={'tip-captured':'getBlockCount','network-captured':'getNetworkInfo','hash-captured':'getBlockHash','block-captured':'getBlock'}[mode];
 if(captureMethod){const original=m.client[captureMethod];m.client[captureMethod]=async function(...args){const captured=await Reflect.apply(original,this,args);if(!holdConsumed){holdConsumed=true;held.resolve();await release.promise;}return captured;};}
 const records=[],caller={status:'NotEntered'};
 async function snapshot(point,rawControl){await h.flush();records.push({mode,point,rawControl:h.tag(rawControl),caller:h.tag(caller),sourceState:h.tag(h.state()),scheduler:h.schedulerState(),nativeRows:h.tag(await util.promisify(native.find.bind(native))({})),nativeOperations:structuredClone(nativeOperations),documentCounts:structuredClone(documentCounts),effects:h.tag(e.effects),logs:h.tag(e.logs)});}
 function feed(raw){
  const counts=structuredClone(h.state().productionIO.counts),client=m.client,index=h.access().TxIndex.client;
  if(raw.io)h.facts(raw.io);
  if(Object.hasOwn(raw,'documents'))documents=structuredClone(raw.documents);
  assert.deepEqual(h.state().productionIO.counts,counts,'raw facts do not reset RPC ordinals');assert.equal(m.client,client);assert.equal(h.access().TxIndex.client,index);
 }
 function invoke(target,args){caller.status='Pending';let value;try{value=Reflect.apply(m[target],m,args);}catch(error){caller.status='Thrown';caller.error={name:error.name,message:error.message};return;}
  Promise.resolve(value).then(value=>{caller.status='Fulfilled';caller.value=h.tag(value);},error=>{caller.status='Rejected';caller.error={name:error.name,message:error.message};});
 }
 const target=mode==='tip-captured'?'getCurrentBlockHeight':mode==='network-captured'?'checkNetworkStatus':mode==='recovery-fresh-network'?'enterRecoveryMode':mode==='incoming-fresh-tip'?'processIncomingBlocks':financial?'processBlock':'fetchWithRetry';
 const args=target==='enterRecoveryMode'?[10,10]:target==='processIncomingBlocks'?[1,10,11]:target==='processBlock'?[first,11]:target==='fetchWithRetry'?[11]:[];
 records.push({mode,point:'input',rawReset:h.tag(reset),rawHarnessConstructorInput:h.tag(h.rawFixtureInitialization()),rawTxUtilsInitialization:h.tag(h.rawTxUtilsInitialization()),rawRPCDocuments:h.tag(documents),rawCommand:h.tag({target:'Main.'+target,args}),classPins:h.inventory.classes,sourceMethods:h.inventory.methods,nativeInput:h.tag(await util.promisify(native.find.bind(native))({}))});
 e.effects.length=0;e.logs.length=0;nativeOperations.length=0;invoke(target,args);
 const timed=['fetch-retry-fresh-block','recovery-fresh-network','incoming-fresh-tip'].includes(mode);
 if(timed)await until(()=>h.clock().next()!==undefined,'genuine initial timer');else await held.promise;
 await snapshot('same-original-await-held',{kind:timed?'OriginalTimerHeld':'OriginalDependencyPromiseHeld'});
 let rawFeed;
 if(mode==='tip-captured'||mode==='incoming-fresh-tip')rawFeed={io:{height:12,blocks:[{height:11,hash:first.hash,block:first},{height:12,hash:second.hash,block:second}]}};
 else if(mode==='network-captured')rawFeed={io:{networkInfo:{networkactive:false,connections:0}}};
 else if(mode==='recovery-fresh-network')rawFeed={io:{networkInfo:{networkactive:true,connections:5}}};
 else if(['hash-captured','block-captured','fetch-retry-fresh-block'].includes(mode))rawFeed={io:{blocks:[{height:11,hash:changed.hash,block:changed}],failures:[]}};
 else if(mode==='raw-captured')rawFeed={documents:[{id:live.id,document:structuredClone(live.document)},{id:live.parent.txid,document:structuredClone(live.parent)}]};
 else if(mode==='decoded-document-captured'){
  const updated=structuredClone(live.document);updated.vout[1].value=0.25;
  rawFeed={documents:[{id:live.id,raw:live.raw,document:updated},{id:live.parent.txid,document:structuredClone(live.parent)}]};
 }else rawFeed={io:{height:12,blocks:[{height:11,hash:changed.hash,block:changed},{height:12,hash:second.hash,block:second}]},documents:[]};
 const heldCalls=documentCounts.decoderawtransaction||0,heldRows=await util.promisify(native.find.bind(native))({}),heldConsensus=structuredClone(e.tables.get('consensus')||[]);
 feed(rawFeed);await snapshot('raw-facts-fed-same-await-still-held',{kind:'RawRpcFactFeed',raw:rawFeed});
 assert.equal(caller.status,'Pending');assert.deepEqual(await util.promisify(native.find.bind(native))({}),heldRows);assert.deepEqual(e.tables.get('consensus')||[],heldConsensus);
 if(timed){const id=h.clock().next();assert.equal(h.clock().snapshot().timers.find(timer=>timer.id===id).delay,mode==='fetch-retry-fresh-block'?1000:10000);h.clock().fire(id);}else release.resolve();
 if(mode==='incoming-fresh-tip'){
  await until(()=>e.tables.get('consensus')?.some(row=>row._id==='TrackHeight'&&row.value===12)&&h.clock().snapshot().timers.filter(timer=>timer.state==='Scheduled').length===1,'same Incoming processes new factual block12');
  assert.equal(caller.status,'Pending');assert.equal(h.clock().snapshot().timers.filter(timer=>timer.state==='Scheduled')[0].delay,10000);
 }else await until(()=>caller.status!=='Pending','same original caller settles after raw feed');
 await snapshot('same-original-await-resumed',{kind:'ReleaseSameOriginalAwait'});
 if(mode==='tip-captured'){
  assert.deepEqual(caller.value,h.tag(11));const later=await m.getCurrentBlockHeight();assert.equal(later,12);await snapshot('future-call-sees-fresh-tip',{target:'Main.getCurrentBlockHeight',args:[]});
 }else if(mode==='network-captured'){
  assert.deepEqual(caller.value,h.tag({status:true,connections:2}));assert.deepEqual(h.tag(await m.checkNetworkStatus()),h.tag({status:false,reason:'Network inactive'}));await snapshot('future-call-sees-fresh-network',{target:'Main.checkNetworkStatus',args:[]});
 }else if(mode==='block-captured'){
  assert.deepEqual(caller.value,h.tag(first));assert.deepEqual(h.tag(await m.fetchWithRetry(11)),h.tag(changed));await snapshot('future-call-sees-fresh-block',{target:'Main.fetchWithRetry',args:[11]});
 }else if(mode==='hash-captured')assert.deepEqual(caller.value,h.tag(undefined));
 else if(mode==='fetch-retry-fresh-block')assert.deepEqual(caller.value,h.tag(changed));
 else if(mode==='recovery-fresh-network')assert.equal(caller.status,'Fulfilled');
 else if(financial){
  assert.equal(caller.status,'Fulfilled');assert.equal(documentCounts.decoderawtransaction,mode==='validity-held-fresh-facts'?heldCalls:1,'no decode replay');
  const actualRow=await util.promisify(native.findOne.bind(native))({_id:`tx-11-${live.id}`});
  if(mode==='raw-captured'){
   assert.equal(actualRow,null,'the future decode lookup sees the newly absent raw/document mapping; no native row is invented');
   assert.ok(e.effects.some(effect=>effect.kind==='rpc-data'&&effect.method==='Client.decoderawtransaction'&&effect.returned===undefined));
  }else assert.equal(actualRow.value.txId,live.id);
  if(mode==='validity-held-fresh-facts'){
   assert.equal(heldConsensus.find(row=>row._id===live.id).processed,true);assert.ok(e.tables.get('propertyList')?.length);assert.ok(e.tables.get('tallyMap')?.length);
   assert.equal((await util.promisify(native.findOne.bind(native))({_id:`tx-11-${live.id}`})).valid,true);assert.equal(await m.getCurrentBlockHeight(),12);await snapshot('future-call-sees-fresh-tip-without-financial-reexecution',{target:'Main.getCurrentBlockHeight',args:[]});
  }
 }
 assert.deepEqual(e.state().unsupported,[]);h.stop();return records;
}
async function parent(){
 const all=[];
 for(const mode of modes){const child=cp.spawnSync(process.execPath,[__filename,'--child',mode,...(process.argv.includes('--lock')?['--lock']:[])],{env:{...process.env,NODE_NO_WARNINGS:'1'},encoding:'utf8',windowsHide:true,timeout:30000,maxBuffer:64*1024*1024});assert.equal(child.error,undefined);assert.equal(child.status,0,mode+'\n'+child.stderr);assert.equal(child.stderr,'');all.push(...JSON.parse(child.stdout));}
 const raw=all.map(row=>JSON.stringify(row)).join('\n')+'\n',file=path.join(repo,'fixtures/main_fresh_input'+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),raw);else fs.writeFileSync(file,raw);
 h.stop();console.log(JSON.stringify({cases:modes.length,observations:all.length,numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',fixtureSHA256:sha(raw),classBodiesChanged:false,samePromise:true,financialStateImporter:false}));
}
if(process.argv.includes('--child'))scene(process.argv[process.argv.indexOf('--child')+1]).then(records=>fs.writeSync(1,JSON.stringify(records)+'\n'),error=>{console.error(error.stack);process.exitCode=1;});else parent().catch(error=>{console.error(error.stack);process.exitCode=1;});
