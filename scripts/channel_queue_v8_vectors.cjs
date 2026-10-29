'use strict';
// The original whole Orderbook body is executed by the existing pinned queue
// oracle. Only external DB/placement boundaries and transparent sort observation
// are declared here. Financial methods are never answered by this fixture.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),Module=require('node:module'),vm=require('node:vm'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||stage);
const numeric=require(path.join(repo,'scripts/legacy_numeric_oracle.cjs'));
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
if(process.argv.includes('--lock')){
 const packed=fs.readFileSync(path.join(repo,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1];
 const bytes=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));
 assert.equal(sha(bytes),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');
 numeric.loadBigNumber=()=>{const module={exports:{}};vm.runInNewContext(bytes.toString(),{module,exports:module.exports},{timeout:1000});return module.exports;};
}
const reference=fs.readFileSync(path.join(repo,'tests/contract_queue_source_owner.rs'),'utf8').replace(/\r\n/g,'\n');
let code=reference.split('const QUEUE_ORACLE: &str = r###"')[1].split('"###;')[0];
code=code.slice(0,code.indexOf('async function main(){'));
assert.equal(sha(code),'b69c56e53a0fc3a0b5b2419a7b7322c873764db4c9cf714a444ac055d06c3431','unchanged full-source queue harness prefix');
assert.equal(code.split('vm.runInNewContext(source,').length,2);
code=code.replace('vm.runInNewContext(source,','observedRun(source,');
code=code.replace('const root=process.argv[1]','const root='+JSON.stringify(repo));
code+=String.raw`
let runs=[],output=[];
function observedRun(body,environment,options){
 const context=vm.createContext({...environment,
 __sortBefore(rows){runs.push({before:clone(rows),pairs:[],error:null});},
 __sortPair(a,b){runs.at(-1).pairs.push([a.auditOrdinal,b.auditOrdinal]);},
 __sortError(e){runs.at(-1).error={name:e.name,message:e.message};},
 __sortAfter(rows){runs.at(-1).after=clone(rows);}});
 vm.runInContext('(function(){const original=Array.prototype.sort;Array.prototype.sort=function(compare){if(typeof compare!=="function")return Reflect.apply(original,this,[compare]);__sortBefore(this);try{return Reflect.apply(original,this,[function(a,b){__sortPair(a,b);return Reflect.apply(compare,undefined,[a,b]);}]);}catch(error){__sortError(error);throw error;}finally{__sortAfter(this);}};})()',context);
 return vm.runInContext(body,context,options);
}
function entry(i,shape='reverse'){
 const ordinal=shape==='reverse'?i:shape==='runs'?(i%2?i+17:i):i;
 const row={kind:'other',sender:'sender-'+String((i*37)%97).padStart(3,'0'),blockHeight:10,txid:'tx-'+String(ordinal).padStart(5,'0'),auditOrdinal:i};
 if(shape==='mixed')row.blockHeight=i%3===0?'10':i%3===1?10:[];
 if(shape==='ties'){row.sender='sender-same';row.txid='same';}
 if(shape==='infinity')row.blockHeight=i%3===0?Infinity:i%3===1?'Infinity':10;
 return row;
}
const configs=[
 ['zero',0,'reverse'],['one',1,'reverse'],['two',2,'reverse'],['legacy63',63,'reverse'],
 ['merge64',64,'reverse'],['merge65',65,'reverse'],['runs128',128,'runs'],['max1024',1024,'reverse'],
 ['nontransitive64',64,'mixed'],['ties65',65,'ties'],
 ['mixed-known64',64,'reverse',{known:true}],['known-partial65',65,'reverse',{known:true,write:true,rejectAt:2}],
 ['bad-txid128',128,'ties',{badTx:true}],['bad-sender64',64,'ties',{badSender:true}],
 ['first-pair-prefix128',64,'reverse',{twoPairs:true,failure:true}],
 ['nan-subtraction65',65,'infinity',{height:Infinity}],
];
async function runCase([name,n,shape,options={}]){
 reset();runs=[];
 const trades=Array.from({length:n},(_,i)=>entry(shape==='reverse'?n-i-1:i,shape));
 if(options.known)for(let i=0;i<trades.length;i++){trades[i].kind=i%2?'token':'contract';trades[i].payload={match:{rawInput:i}};}
 if(options.badTx)trades[Math.floor(n/2)].txid=0;
 if(options.badSender)trades[Math.floor(n/2)].sender=0;
 const rows=[{_id:'activePairs',pairs:options.twoPairs?['channel-1','channel-2']:['channel-1']},{_id:'channel-queue-1',trades}];
 if(options.twoPairs)rows.push({_id:'channel-queue-2',trades:trades.map(x=>({...x,auditOrdinal:x.auditOrdinal+n}))});
 documents=clone(rows);cap={count:0,write:!!options.write,rejectAt:options.rejectAt||0};
 failures=options.failure?[{operation:'updateAsync',skip:1,name:'Error',message:'second-pair-clear-failure'}]:[];
 const input={rows,height:options.height??10,write:cap.write,rejectAt:cap.rejectAt,failures:clone(failures)};
 // These stand-ins are declared placement boundaries, not financial answers.
 // Every known call is recorded; optional writes invoke original saveOrderBook.
 Orderbook.prototype.processTokenMatches=async function(...args){return placement(this,'Orderbook.processTokenMatches',args);};
 Orderbook.prototype.processContractMatches=async function(...args){return placement(this,'Orderbook.processContractMatches',args);};
 let result;try{result={returned:await Orderbook.processQueuedChannelTradesForBlock(input.height)};}catch(error){result={thrown:{name:error.name,message:error.message}};}
 output.push(['CASE',name,hex(input),hex(result),hex(state()),hex(dbCalls),hex(placementCalls),hex(external)].join('\t'));
 for(let i=0;i<runs.length;i++){
  const run=runs[i];
  output.push(['SORT',name+'/'+i,hex(run.before),hex(run.error?{thrown:run.error}:{returned:undefined}),hex(run.after),run.pairs.map(p=>p.join(',')).join(';')].join('\t'));
 }
}
(async()=>{for(const config of configs)await runCase(config);process.stdout.write(output.join('\n')+'\n');})().catch(error=>{console.error(error.stack);process.exitCode=1});
`;
// Generalize only the declared callback recorder for the two channel methods.
code=code.replace("const original=args.slice();if(target==='Orderbook.addContractOrder')original[12]=handle(original[12]);else original.push(handle(instance));", "const original=args.slice();if(target==='Orderbook.addContractOrder')original[12]=handle(original[12]);else if(target==='Orderbook.processTokenMatches'||target==='Orderbook.processContractMatches')original.unshift(handle(instance));else original.push(handle(instance));");
const target=new Module(path.join(__dirname,'channel-sort-source-driver.cjs'),module);target.filename=path.join(__dirname,'channel-sort-source-driver.cjs');target.paths=Module._nodeModulePaths(__dirname);
const oldWrite=process.stdout.write;let bytes='';process.stdout.write=(value)=>{bytes+=value;return true;};
target._compile(code,target.filename);
process.on('beforeExit',()=>{
 process.stdout.write=oldWrite;
 const file=path.join(stage,'fixtures/channel_queue_v8_vectors'+(process.argv.includes('--lock')?'_locked':'')+'.tsv');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),bytes);else fs.writeFileSync(file,bytes);
 console.log(JSON.stringify({profile:'TL-CHANNEL-V8-SORT-SOURCE-1',numeric:process.argv.includes('--lock')?'genuine9.1.2':'genuine9.3.1',sourceCases:16,records:bytes.trim().split('\n').length,sha256:sha(bytes)}));
});
