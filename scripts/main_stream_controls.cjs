'use strict';
// Full pinned Main/TxIndex bodies from the reviewed service harness. Only its
// external txIndex receiver is replaced with genuine NeDB1.8.0; no verdict or
// financial result is injected. A transparent processTxSet observer delegates.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),assert=require('node:assert/strict'),crypto=require('node:crypto'),util=require('node:util');
const stage=path.resolve(__dirname,'..'),repo=fs.existsSync(path.join(stage,'fixtures/native_genesis/source/main.js'))?stage:path.resolve(stage,'../../outputs/TradeLayer-Rust');process.env.PROOF_REPO=repo;
const sourceNpmRoot=process.env.TL_JS_MODULE_ROOT||process.env.TRADELAYER_JS_ROOT;if(!sourceNpmRoot)throw Error('Set the explicit genuine npm prefix');const requireGenuine=Module.createRequire(path.join(sourceNpmRoot,'package.json'));
for(const k of ['isDate','isRegExp'])if(!util[k])util[k]=util.types[k];
const Datastore=requireGenuine('nedb');assert.equal(requireGenuine('nedb/package.json').version,'1.8.0');
const original=fs.readFileSync(path.join(stage,'scripts/main_service_vectors.cjs'),'utf8');const cut=original.indexOf('async function main(){');assert.ok(cut>0);
const prefix=original.slice(0,cut)+`\nmodule.exports={reset,flush,tag,inventory,access:()=>({e,m,Main,TxIndex})};`;
const loader=new Module(path.join(stage,'scripts/main-stream-loader.cjs'),module);loader.filename=path.join(stage,'scripts/main-stream-loader.cjs');loader.paths=Module._nodeModulePaths(path.join(stage,'scripts'));loader._compile(prefix,loader.filename);const h=loader.exports;
function sha256d(b){return crypto.createHash('sha256').update(crypto.createHash('sha256').update(b).digest()).digest();}
function digest(rows){let chain=sha256d(Buffer.from('TL-TXINDEX-COMPLETE-ROW-STREAM-2'));for(let i=0;i<rows.length;i++){const body=Buffer.from(JSON.stringify(h.tag(rows[i]))),n=Buffer.alloc(8),len=Buffer.alloc(4);n.writeBigUInt64BE(BigInt(i));len.writeUInt32BE(body.length);chain=sha256d(Buffer.concat([chain,n,len,body]));}return chain.toString('hex');}
const tx=(id,payload)=>({_id:id,value:{txId:id.split('-').slice(2).join('-'),marker:'tl',payload,sender:{senderAddress:'source-input-address',amount:0},reference:[]}});
const cases=[
 ['funding-before-trade',[tx('tx-10-c','201'),tx('tx-10-b','k'),tx('tx-10-a','4')]],
 ['later-malformed-blocks-earlier-processing',[tx('tx-10-a','201'),tx('tx-10-z',4)]],
 ['empty-payload-still-creates-max-height',[tx('tx-10-a','201'),tx('tx-11-z','')]],
 ['nan-height-prevents-processing',[tx('tx-10-a','201'),tx('tx-bad-z','4')]],
 ['array-payload-coercion',[tx('tx-10-a',['k']),tx('tx-10-b',[20]),tx('tx-10-c',['04 '])]],
 ['hex-and-prefix-height',[tx('tx-0xA-a','201'),tx('tx-10tail-b','4')]],
 ['grouping-above-aggregate-limit',Array.from({length:1025},(_,i)=>tx(`tx-10-${i.toString().padStart(4,'0')}`,i%3===0?'4':i%3===1?'k':'201'))],
];
(async()=>{
 const output=[];
 for(const[name,rows]of cases){
  await h.reset(`stream/${name}`,{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',rows,mainGenesis:10,maxHeight:10,observeTypes:true});
  const{e,m}=h.access();const db=new Datastore();const insert=util.promisify(db.insert.bind(db));for(const row of e.table('txIndex'))await insert(row);e.tables.delete('txIndex');
  // The experiment owns indexed history, not a live RPC service. Let the real
  // suffix check fail at that explicit external boundary rather than starting
  // the source's unbounded incoming-block loop on a virtual microtask clock.
  m.client.getBlockCount=function(){throw Object.assign(Error('Unprovided later RPC height'),{name:'SourceBoundaryUnavailable'});};
  const sourceFactory=e.storage.getDatabase;
  const txReceiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[`${method}Async`,async(...args)=>util.promisify(db[method].bind(db))(...args)]));
  e.storage.getDatabase=function(category,...args){if(category==='txIndex')return Promise.resolve(txReceiver);return Reflect.apply(sourceFactory,this,[category,...args]);};
  const before=await util.promisify(db.find.bind(db))({});const phases=[];const originalProcess=m.processTxSet;
  m.processTxSet=function(txSet,height){phases.push({height:h.tag(height),rows:txSet.length,rowStreamDigest:digest(txSet),ids:txSet.length<=16?txSet.map(r=>r._id):undefined});return Reflect.apply(originalProcess,this,arguments);};
  let result;try{result={returned:h.tag(await m.constructConsensusFromIndex(10,false))};}catch(error){result={thrown:{name:error.name,message:error.message}};}await h.flush();delete m.processTxSet;
  const after=await util.promisify(db.find.bind(db))({});
  output.push({kind:'case',name,profile:'unchanged Main class + genuine NeDB metadata + existing recorded external runtime; later live RPC height unavailable',initialRows:before.length,initialDigest:digest(before),sourceMethods:h.inventory.methods['Main.constructConsensusFromIndex'],result,phases,finalRows:after.length,finalDigest:digest(after)});
  for(const row of before)output.push({kind:'input-row',name,row:h.tag(row)});
  console.log(name,JSON.stringify({result,phases:phases.map(p=>({height:p.height,rows:p.rows}))}));
 }
 const text=output.map(r=>JSON.stringify(r)).join('\n')+'\n',file=path.join(stage,'fixtures/main_stream_controls.jsonl');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),text);else fs.writeFileSync(file,text);
 console.log(`PASS ${cases.length} unchanged Main controls; no fabricated processTxSet result`);
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
