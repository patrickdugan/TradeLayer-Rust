'use strict';
// Observe unchanged pinned Main -> Logic -> Activation/Vesting/Tally bodies.
// The only exceptional inputs are recorded DB rejections. Observer wrappers
// forward the original promises and never supply a financial result.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);
process.env.PROOF_REPO=repo;
const driver=fs.readFileSync(path.join(repo,'scripts/native_main_controls.cjs'),'utf8').replace(/\r\n/g,'\n');
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
assert.equal(sha(driver),'953df9389b84b663edeb55d7485efadf65e01af7130c86cb20bb61211f94fb8f');
const cut=driver.indexOf('async function runCase(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/main0-live-loader.cjs'),module);
loader.filename=path.join(stage,'scripts/main0-live-loader.cjs');loader.paths=Module._nodeModulePaths(path.join(stage,'scripts'));
loader._compile(driver.slice(0,cut)+'\nmodule.exports={h,native,Datastore,util,digest,pins};',loader.filename);
const {h,native,Datastore,util,digest,pins}=loader.exports;
const errorValue=error=>({name:error.name,message:error.message});
async function scene(name,failure){
 const reset={empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',rows:[structuredClone(native)],mainGenesis:10,maxHeight:10,observeTypes:true};
 await h.reset('main0-live/'+name,reset);
 const {e,m}=h.access(),db=new Datastore();
 for(const row of e.table('txIndex'))await util.promisify(db.insert.bind(db))(row);
 e.tables.delete('txIndex');
 const operations=[];
 const receiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[method+'Async',async(...args)=>{
  const operation={ordinal:operations.length,atEffect:e.effects.length,method:method+'Async',args:h.tag(args)};operations.push(operation);
  try{const returned=await util.promisify(db[method].bind(db))(...args);operation.returned=h.tag(returned);operation.endEffect=e.effects.length;return returned;}
  catch(error){operation.thrown=errorValue(error);operation.endEffect=e.effects.length;throw error;}
 }]));
 const originalFactory=e.storage.getDatabase;
 e.storage.getDatabase=function(category,...args){if(category!=='txIndex')return Reflect.apply(originalFactory,this,[category,...args]);operations.push({ordinal:operations.length,atEffect:e.effects.length,endEffect:e.effects.length,method:'getDatabase',args:h.tag([category,...args]),returned:{database:'txIndex'}});return Promise.resolve(receiver);};
 const logic=[];const originalLogic=e.logic.typeSwitch;
 e.logic.typeSwitch=function(...args){
  const observation={atEffect:e.effects.length,kind:'Logic.typeSwitch',type:args[0],validAtEntry:args[1]?.valid,argsAtEntry:h.tag(args)};logic.push(observation);
  const promise=Reflect.apply(originalLogic,this,args);
  promise.then(returned=>{observation.returned=h.tag(returned);observation.endEffect=e.effects.length;},error=>{observation.thrown=errorValue(error);observation.endEffect=e.effects.length;});
  return promise;
 };
 const consensus=e.load('consensus'),marks=[],originalMark=consensus.markTxAsProcessed;
 consensus.markTxAsProcessed=function(...args){marks.push({atEffect:e.effects.length,args:h.tag(args)});return Reflect.apply(originalMark,this,args);};
 e.effects.length=0;e.logs.length=0;
 const before=h.tag(e.state()),logsStart=e.logs.length;
 if(failure)e.setFailure(failure);
 const sourceMethods={main:h.inventory.methods['Main.processTx'],logicTypeSwitchSHA256:sha(originalLogic.toString()),activationActivateSHA256:sha(e.load('activation').prototype.activate.toString())};
 const input={name,point:'input',sourceMethods,reset:h.tag(reset),command:h.tag({target:'Main.processTx',args:[[structuredClone(native.value)],10]}),failure:h.tag(failure),sourceState:before,indexInput:h.tag(await util.promisify(db.find.bind(db))({}))};
 let result;try{result={transport:'Pending',returned:await m.processTx([structuredClone(native.value)],10)};}catch(error){result={transport:'PendingThrow',thrown:errorValue(error)};}
 await h.flush();
 const indexRows=await util.promisify(db.find.bind(db))({}),sourceState=e.state();
 const processed=sourceState.tables.find(t=>t.name==='consensus')?.rows.find(r=>r._id===native.value.txId);
 assert.equal(sourceState.unsupported.length,0,'no hidden financial capability substitution');
 const output={name,point:'returned-original-live-call',result:h.tag(result),processed:h.tag(processed),selectedNative:h.tag(indexRows.find(r=>r._id===native._id)),sourceState:h.tag(sourceState),logic,marks,operations,effects:h.tag(e.effects),logs:h.tag(e.logs.slice(logsStart))};
 const summary={name,result,logic:logic.map(x=>({validAtEntry:x.validAtEntry,returned:x.returned,thrown:x.thrown})),valid:processed?.params?.valid,reason:processed?.params?.reason,marks:marks.length,decode:e.effects.filter(x=>x.kind==='source-function'&&x.target==='Types.decodePayload').length,failureEffects:e.effects.filter(x=>x.thrown).map(x=>({collection:x.collection,method:x.method,query:x.args?.[0],thrown:x.thrown})),tallyUpdates:e.effects.filter(x=>x.collection==='tallyMap'&&x.op==='update').map(x=>({query:x.args[0],thrown:x.thrown}))};
 assert.equal(result.transport,'Pending');assert.equal(result.returned,true,'actual Main processedAny remains true after Logic rejection');
 assert.equal(logic.length,1);assert.equal(logic[0].validAtEntry,true);assert.equal(marks.length,1);assert.equal(summary.decode,1);
 const rejects=name.startsWith('tally-');assert.equal(processed.params.valid,!rejects);assert.equal(indexRows.find(r=>r._id===native._id).valid,!rejects);
 if(rejects){assert.equal(logic[0].thrown.message,failure.message);assert.equal(processed.params.reason,' Logic error: '+failure.message);}
 else assert.equal(logic[0].thrown,undefined);
 if(name==='tally-init-flag-rejects-after-financial-prefix'){
  assert.equal(summary.tallyUpdates.filter(x=>!x.thrown).length,5);
  assert.ok(sourceState.tables.find(x=>x.name==='propertyList').rows.length>0);
  assert.ok(sourceState.tables.find(x=>x.name==='tallyMap').rows.some(x=>x._id==='tallyMap'));
  assert.ok(!sourceState.tables.find(x=>x.name==='tallyMap').rows.some(x=>x._id==='$TLinit'),'failed flag is absent while financial prefix remains');
 }
 return{records:[input,output],summary};
}
(async()=>{
 const controls=[];
 controls.push(await scene('baseline'));
 controls.push(await scene('activations-update-swallowed',{collection:'activations',op:'update',skip:0,message:'recorded-activation-save-rejection'}));
 controls.push(await scene('tally-init-read-rejects-logic',{collection:'tallyMap',op:'findOne',skip:0,message:'recorded-tally-init-read-rejection'}));
 const updates=controls[0].summary.tallyUpdates,flagOrdinal=updates.findIndex(x=>x.query?._id==='$TLinit');
 assert.equal(flagOrdinal,5,'actual unchanged baseline reaches initialization flag after five balance writes');
 controls.push(await scene('tally-init-flag-rejects-after-financial-prefix',{collection:'tallyMap',op:'update',skip:flagOrdinal,message:'recorded-tally-init-flag-rejection'}));
 const records=controls.flatMap(x=>x.records),bytes=records.map(x=>JSON.stringify(x)).join('\n')+'\n';
 const locked=process.argv.includes('--lock'),file=path.join(stage,'fixtures/main0_live_logic'+(locked?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 const summary={profile:'TL-ORIGINAL-MAIN0-LIVE-LOGIC-REJECTION-CONTROLS-1',node:process.version,v8:process.versions.v8,numeric:locked?'genuine-9.1.2':'genuine-9.3.1',main:pins.mainSourceSHA256,calls:4,observations:records.length,flagUpdateSkip:flagOrdinal,fixtureSHA256:sha(bytes),controls:controls.map(x=>x.summary)};
 console.log(JSON.stringify(summary,null,2));
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
