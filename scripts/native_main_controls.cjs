'use strict';
// Full pinned Main/TxIndex/financial modules; genuine NeDB owns txIndex only.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const assert=require('node:assert/strict'),crypto=require('node:crypto'),util=require('node:util');
const stage=path.resolve(__dirname,'..');
const repo=path.resolve(process.env.PROOF_REPO||stage);
process.env.PROOF_REPO=repo;
const prefixRoot=process.env.TL_JS_MODULE_ROOT||process.env.TRADELAYER_JS_ROOT;
if(!prefixRoot)throw Error('Explicit existing genuine npm prefix is required');
const requireGenuine=Module.createRequire(path.join(prefixRoot,'package.json'));
for(const key of ['isDate','isRegExp'])if(!util[key])util[key]=util.types[key];
const Datastore=requireGenuine('nedb');assert.equal(requireGenuine('nedb/package.json').version,'1.8.0');
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const pins=JSON.parse(fs.readFileSync(path.join(repo,'fixtures/reference_fidelity/native_main_source_inputs.json'),'utf8'));
const original=fs.readFileSync(path.join(repo,'scripts/main_service_vectors.cjs'),'utf8').replace(/\r\n/g,'\n');
assert.equal(pins.files['scripts/main_service_vectors.cjs'],'fc6117265275b594ea61ddf0746ebdf0c4a96aa0ee7207739e65b624c2f54e8d');
assert.equal(sha(original),pins.files['scripts/main_service_vectors.cjs']);
for(const [file,expected] of Object.entries(pins.files))assert.equal(sha(fs.readFileSync(path.join(repo,file),'utf8').replace(/\r\n/g,'\n')),expected,'pinned reproduction input '+file);
const cut=original.indexOf('async function main(){');assert.ok(cut>0);
const loader=new Module(path.join(stage,'scripts/native-main-loader.cjs'),module);
loader.filename=path.join(stage,'scripts/native-main-loader.cjs');
loader.paths=Module._nodeModulePaths(path.join(stage,'scripts'));
loader._compile(original.slice(0,cut)+'\nmodule.exports={reset,flush,tag,inventory,access:()=>({e,m,Main,TxIndex})};',loader.filename);
const h=loader.exports;
function hash(b){return crypto.createHash('sha256').update(crypto.createHash('sha256').update(b).digest()).digest();}
function digest(rows,profile='TL-TXINDEX-COMPLETE-ROW-STREAM-2'){let chain=hash(Buffer.from(profile));for(let i=0;i<rows.length;i++){
 const body=Buffer.from(JSON.stringify(h.tag(rows[i]))),number=Buffer.alloc(8),size=Buffer.alloc(4);
 number.writeBigUInt64BE(BigInt(i));size.writeUInt32BE(body.length);chain=hash(Buffer.concat([chain,number,size,body]));
}return chain.toString('hex');}
const txid='ac51132ae7087eca33d746db2622442dd53e300de4f38aa9b4bc418555d4a2c8';
const native={_id:'tx-10-'+txid,value:{txId:txid,marker:'tl',payload:'00,production-code',sender:{senderAddress:'MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB',amount:0},reference:[]}};
const pads=()=>Array.from({length:1099},(_,i)=>({_id:`tx-10-pad${i.toString().padStart(4,'0')}`,value:{txId:`pad${i.toString().padStart(4,'0')}`}}));
function queryKind(query){const id=query?._id;if(id instanceof RegExp||Object.prototype.toString.call(id)==='[object RegExp]')return{regexp:id.source};
 const regex=id?.$regex;if(regex)return{regexp:regex.source??String(regex)};
 return typeof id==='string'?{id}:{all:true};}
async function runCase(name,{direct=false,directLive=false,malformed=false,failUpdateOnce=false}={}){
 const rows=[structuredClone(native),...pads()];if(malformed)rows.push({_id:'tx-10-zzzz-malformed',value:{txId:'zzzz-malformed',payload:4}});
 await h.reset('native13/'+name,{empty:true,clientProfile:{chain:'LTC',test:false,height:10},init:'LTC',rows,mainGenesis:10,maxHeight:10,observeTypes:true});
 const {e,m}=h.access();const db=new Datastore();const insert=util.promisify(db.insert.bind(db));
 for(const row of e.table('txIndex'))await insert(row);e.tables.delete('txIndex');
 m.client.getBlockCount=function(){throw Object.assign(Error('Native Main control has no live RPC service'),{name:'SourceBoundaryUnavailable'});};
 const operations=[];let operation=0,updateFailed=false;
 const txReceiver=Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[`${method}Async`,async(...args)=>{
  if(method==='update'&&failUpdateOnce&&!updateFailed){updateFailed=true;const error=Error('native-source-update-failure');operations.push({sequence:operation++,method:'updateAsync',query:queryKind(args[0]),args,thrown:{name:error.name,message:error.message}});throw error;}
  const returned=await util.promisify(db[method].bind(db))(...args);
  const record={sequence:operation++,method:method+'Async',query:queryKind(args[0])};
  if(Array.isArray(returned)&&method==='find')Object.assign(record,{rows:returned.length,rowChain:digest(returned),validityChain:record.query.regexp==='^tx'?digest(returned,'TL-TXINDEX-STREAMED-VALIDITY-1'):undefined,complete:true});
  else if(method==='findOne')record.returned=returned;else Object.assign(record,{args,returned});
  operations.push(record);return returned;
 }]));
 const originalFactory=e.storage.getDatabase;
 e.storage.getDatabase=function(category,...args){if(category==='txIndex')return Promise.resolve(txReceiver);return Reflect.apply(originalFactory,this,[category,...args]);};
 const before=await util.promisify(db.find.bind(db))({});const phases=[];let directResult,directFinancial;
 if(direct){directResult=await m.processTxSet([structuredClone(native)],10);directFinancial=e.state().tables.filter(t=>['propertyList','tallyMap','activations','consensus'].includes(t.name));}
 if(directLive){directResult=await m.processTx([structuredClone(native.value)],10);directFinancial=e.state().tables.filter(t=>['propertyList','tallyMap','activations','consensus'].includes(t.name));}
 const originalProcess=m.processTxSet;
 m.processTxSet=function(txSet,height){phases.push({height,rows:txSet.length,rowChain:digest(txSet)});return Reflect.apply(originalProcess,this,arguments);};
 let result;try{result={returned:await m.constructConsensusFromIndex(10)};}catch(error){result={thrown:{name:error.name,message:error.message}};}
 await h.flush();delete m.processTxSet;
 const after=await util.promisify(db.find.bind(db))({});assert.equal(e.tables.has('txIndex'),false,'financial store must not regain a metadata mirror');
 const financial=e.state().tables.filter(t=>['propertyList','tallyMap','activations','consensus'].includes(t.name));
 const processed=financial.find(t=>t.name==='consensus')?.rows.find(r=>r._id===txid);
 if(malformed){assert.equal(phases.length,0);assert.equal(processed,undefined);assert.equal(result.thrown?.name,'TypeError');}
 else{assert.equal(processed?.processed,true);assert.ok(financial.find(t=>t.name==='propertyList')?.rows.length>0);assert.ok(financial.find(t=>t.name==='tallyMap')?.rows.length>0);
  assert.ok(operations.some(o=>o.method==='findAsync'&&o.query.regexp==='^tx'&&o.rows>=1100&&o.complete));
   assert.equal(after.find(r=>r._id===native._id).valid,!failUpdateOnce);
   if(failUpdateOnce){assert.equal(processed.params.valid,false);assert.match(processed.params.reason,/Fatal tx processing error: native-source-update-failure/);assert.equal(operations.filter(o=>o.method==='findAsync'&&o.query.regexp==='^tx').length,2);}}
 const output=[{kind:'case',name,mainSHA256:pins.mainSourceSHA256,sourceMethod:h.inventory.methods['Main.constructConsensusFromIndex'],
  initialRows:before.length,initialChain:digest(before),finalRows:after.length,finalChain:digest(after),result,directResult,phases,operations,financialProcessed:!!processed,...(directLive?{directSourceMethod:h.inventory.methods['Main.processTx']}:{})}];
 for(const row of before)output.push({kind:'input-row',name,row});
 for(const row of after)output.push({kind:'output-row',name,row});
 for(const table of financial)for(const row of table.rows)output.push({kind:'financial-row',name,collection:table.name,row});
 for(const table of directFinancial||[])for(const row of table.rows)output.push({kind:'direct-financial-row',name,collection:table.name,row});
 return output;
}
(async()=>{
 const rows=[];rows.push(...await runCase('direct-main0-and-full-grouping',{direct:true}));
 rows.push(...await runCase('full-catchup-handles-main0'));
 rows.push(...await runCase('late-malformed-prevents-earlier-main0',{malformed:true}));
 rows.push(...await runCase('native-update-error-retains-real-main0-prefix',{failUpdateOnce:true}));
 rows.push(...await runCase('direct-live-main0-native-validity',{directLive:true}));
 const payload=rows.map(row=>JSON.stringify(h.tag(row))).join('\n')+'\n';
 const file=path.join(stage,'fixtures/native_main_controls.jsonl');
 if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),payload);else fs.writeFileSync(file,payload);
 console.log(JSON.stringify({cases:5,realMain0Cases:4,minimumIndexedRows:1100,genuineNeDB:'1.8.0',fixtureSHA256:sha(payload),frames:rows.length}));
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
