'use strict';
// Complete unchanged pinned Main/TxIndex class bodies and original protocol
// modules through the explicitly declared existing callable-class harness.
// This does not claim the full Main module-global IIFEs execute here.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);process.env.PROOF_REPO=repo;
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const pins=JSON.parse(fs.readFileSync(path.join(stage,'fixtures/coercive_sync_source_inputs.json'),'utf8'));
const observer=fs.readFileSync(path.join(stage,'scripts/coercive_sync_observer.cjs'),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(observer),pins.observerSHA256);
const cut=observer.indexOf('async function scene(');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/coercive-sync-input-loader.cjs'),module);loader.filename=path.join(stage,'scripts/coercive-sync-input-loader.cjs');loader.paths=Module._nodeModulePaths(path.dirname(loader.filename));
loader._compile(observer.slice(0,cut)+'\nmodule.exports={h};',loader.filename);const {h}=loader.exports;
const cases=[
 {id:'first10/01-number-equal',max:10,tip:10},
 {id:'first10/02-string-equal',max:'10',tip:10},
 {id:'first10/03-nonnumeric-string',max:'bad',tip:10},
 {id:'first10/04-undefined',max:undefined,tip:10},
 {id:'first10/05-NaN',max:NaN,tip:10},
 {id:'first10/06-true-equal',max:true,tip:1},
 {id:'first10/07-false-zero',max:false,tip:0},
 {id:'first10/08-string-zero',max:'0',tip:0},
 {id:'first10/09-whitespace-zero',max:' ',tip:0},
 {id:'first10/10-ahead-number',max:12,tip:10},
];
const block=height=>({height,hash:'coercive-'+JSON.stringify(h.tag(height)),block:{hash:'coercive-'+JSON.stringify(h.tag(height)),previousblockhash:'recorded-parent',height,tx:[]}});
cases.push(
 {id:'extended/11-string-extract-real-empty-blocks',max:'10',tip:11,blocks:['10',11].map(block)},
 {id:'extended/12-boolean-extract-keeps-original-false-height',max:false,tip:1,blocks:[false,1].map(block)},
 {id:'extended/13-two-string-lexical-no-extract',max:'5',tip:'10'},
 {id:'extended/14-two-string-lexical-extract',max:'10',tip:'9',blocks:['10'].map(block)},
 {id:'extended/15-empty-string-tip-string-extract',max:'',tip:'0',blocks:[''].map(block)},
 {id:'extended/16-abstract-object-equality-takes-genesis',max:'[object Object]',tip:11,blocks:[10,11].map(block)},
 {id:'extended/17-hex-string-equal-number',max:'0xA',tip:10},
 {id:'extended/18-undefined-equals-null-tip',max:undefined,tip:null},
 {id:'extended/19-same-nonnumeric-strings',max:'bad',tip:'bad'},
 {id:'extended/20-main-height-rejection-is-swallowed',max:'10',tip:11,failures:[{method:'Client.getBlockCount',ordinal:1,code:'EIO',message:'recorded Main tip rejection'}]},
 {id:'extended/21-extractor-tip-rejection-is-swallowed',max:'10',tip:11,failures:[{method:'Client.getBlockCount',ordinal:2,code:'EIO',message:'recorded extractor tip rejection'}]},
 {id:'extended/22-extractor-fetch-rejection-is-swallowed',max:'10',tip:11,failures:[{method:'Client.getBlockHash',ordinal:1,code:'EIO',message:'recorded extractor fetch rejection'}]},
 {id:'extended/23-returned-consensus-promise-rejection-is-not-caught',max:'10',tip:10,dbFailure:{collection:'txIndex',op:'find',skip:0,message:'recorded consensus read rejection'}},
 {id:'extended/24-string-negative-zero',max:'-0',tip:0},
 {id:'extended/25-string-zero-is-not-strict-number-zero',max:'0',tip:1,blocks:['0',1].map(block)},
 {id:'extended/26-number-zero-uses-genesis-instead',max:0,tip:1},
);
async function scene(input){
 const blocks=input.blocks||[],reset={empty:true,clientProfile:{chain:'LTC',test:false,height:input.tip},init:'LTC',rows:[],mainGenesis:10,maxHeight:10,observeTypes:true,orchestration:{height:input.tip,networkInfo:{networkactive:true,connections:2},blocks,failures:input.failures||[]}};
 await h.reset('coercive-sync/'+input.id,reset);const {e,m,Main,TxIndex}=h.access();
 if(input.dbFailure)e.setFailure(input.dbFailure);
 const calls=[];
 for(const [receiver,key,target] of [[m,'syncIndex','Main.syncIndex'],[m,'initOrLoadTxIndex','Main.initOrLoadTxIndex'],[m,'constructOrLoadConsensus','Main.constructOrLoadConsensus'],[m,'constructConsensusFromIndex','Main.constructConsensusFromIndex'],[TxIndex,'extractBlockData','TxIndex.extractBlockData'],[TxIndex,'saveMaxHeight','TxIndex.saveMaxHeight'],[TxIndex,'processBlockData','TxIndex.processBlockData']]){
  const original=receiver[key];assert.equal(typeof original,'function');receiver[key]=function(...args){calls.push({ordinal:calls.length,target,args:h.tag(args),originalSHA256:sha(original.toString())});return Reflect.apply(original,this,args);};
 }
 e.effects.length=0;e.logs.length=0;
 const records=[{kind:'input',case:input.id,profile:'TL-ORIGINAL-MAIN-COERCIVE-SYNC-SCALAR-1',rawReset:h.tag(reset),rawMax:h.tag(input.max),...(input.dbFailure?{rawStorageFailure:h.tag(input.dbFailure)}:{}),rawHarnessConstructorInput:h.tag(h.rawFixtureInitialization()),rawTxUtilsImport:h.tag(h.rawTxUtilsInitialization()),sourcePins:h.inventory.classes}];
 let result;try{const value=await m.syncIndex(input.max);result={transport:'Pending',returned:h.tag(value)};}catch(error){result={transport:'PendingThrow',thrown:h.tag({name:error.name,message:error.message,code:error.code})};}
 if(input.dbFailure){assert.equal(result.transport,'PendingThrow','actual consensus failure must reach the caller');assert.ok(e.effects.some(effect=>effect.thrown?.message===input.dbFailure.message),'declared IO adapter operation was reached');}
 function snapshot(point){records.push({kind:'observation',case:input.id,point,result,calls:structuredClone(calls),effects:h.tag(e.effects),logs:h.tag(e.logs),rows:h.tag(e.tables.get('txIndex')||[]),sourceState:h.tag(h.state()),main:h.tag({genesisBlock:m.genesisBlock,parseBlock:m.parseBlock,sameSingleton:Main.instance===m}),index:h.tag({parseBlock:TxIndex.parseBlock,clientSame:TxIndex.client===m.client}),scheduler:h.schedulerState()});}
 snapshot('actual-caller-settled');await h.flush();snapshot('finite-source-jobs-drained');
 assert.deepEqual(e.state().unsupported,[],'no substituted business result is accepted');h.stop();return {records,summary:{case:input.id,result,calls:calls.map(call=>({target:call.target,args:call.args})),effects:e.effects.length,rows:h.tag(e.tables.get('txIndex')||[])}};
}
async function parent(){
 const selected=process.argv.includes('--first-ten')?cases.slice(0,10):cases;
 const outputs=[];for(const item of selected){const args=[__filename,'--child',item.id,...(process.argv.includes('--lock')?['--lock']:[])];const child=cp.spawnSync(process.execPath,args,{env:{...process.env,NODE_NO_WARNINGS:'1'},encoding:'utf8',windowsHide:true,maxBuffer:64*1024*1024,timeout:30000});assert.equal(child.error,undefined);assert.equal(child.status,0,child.stderr);assert.equal(child.stderr,'');outputs.push(JSON.parse(child.stdout));}
 const bytes=outputs.flatMap(item=>item.records).map(item=>JSON.stringify(item)).join('\n')+'\n';const output=path.join(stage,'fixtures/coercive_sync_'+(process.argv.includes('--first-ten')?'first10':'vectors')+(process.argv.includes('--lock')?'_locked':'')+'.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(output,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(output,bytes);
 console.log(JSON.stringify({profile:'TL-ORIGINAL-MAIN-COERCIVE-SYNC-SCALAR-1',node:process.version,numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',sourceCalls:selected.length,records:outputs.reduce((count,item)=>count+item.records.length,0),fixtureSHA256:sha(bytes),summaries:outputs.map(item=>item.summary)},null,2));
}
const selected=process.argv.includes('--child')?cases.find(item=>item.id===process.argv[process.argv.indexOf('--child')+1]):null;
(selected?scene(selected).then(value=>fs.writeSync(1,JSON.stringify(value)+'\n')):parent()).catch(error=>{console.error(error.stack);process.exitCode=1;});
