#!/usr/bin/env node
'use strict';
// Complete pinned source modules; supplied inputs stop at RPC and real Nedb.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),util=require('node:util');
const {createRequire}=require('node:module');
const stage=path.resolve(__dirname,'..'),root=fs.existsSync(path.join(stage,'fixtures/legacy_txindex_source_txIndex.txt'))?stage:path.resolve(stage,'../../outputs/TradeLayer-Rust');
const moduleFlag=process.argv.indexOf('--module-root');
const moduleRoot=moduleFlag>=0?process.argv[moduleFlag+1]:process.env.TL_JS_MODULE_ROOT||process.env.TRADELAYER_JS_ROOT;
assert.ok(moduleRoot,'Set TRADELAYER_JS_ROOT (or TL_JS_MODULE_ROOT / --module-root) to an explicitly provisioned npm prefix; see docs/TXINDEX_SOURCE_REFERENCE.md.');
const external=createRequire(path.resolve(moduleRoot,'package.json'));
// NeDB1.8 targets legacy Node's util API. No TradeLayer function is changed.
const legacyHost=[];
for(const name of ['isDate','isRegExp'])if(typeof util[name]!=='function'){util[name]=util.types[name];legacyHost.push(`util.${name}=util.types.${name}`);}
const Datastore=external('nedb');assert.equal(external('nedb/package.json').version,'1.8.0');
for(const[name,version]of [['big-json','3.2.0'],['bitcoinjs-lib','6.1.8'],['bitcore-lib-ltc','10.10.5']])assert.equal(external(`${name}/package.json`).version,version,`genuine dependency ${name}`);
const nedbBase=path.dirname(external.resolve('nedb/package.json'));
for(const[file,sha]of [['lib/model.js','57d4551a20abcd4efe48880700d30378f09b7e8b145617010729562837b27258'],['lib/datastore.js','9c5575bdaba5127b748305a27f0cac7727595aa9f2f2c15f8735254d765df8ee']])assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(nedbBase,file))).digest('hex'),sha,`genuine NeDB ${file}`);
let currentEnvironment;
process.on('unhandledRejection',(error,promise)=>{assert.ok(currentEnvironment,'source background error without environment');const entry=currentEnvironment.promiseIDs.get(promise);if(entry){entry.status='rejected';entry.error=exception(error);}currentEnvironment.backgroundErrors.push({...exception(error),...(entry?{promise:entry.id}:{})});});
// Late diagnostic observation occurs only after the real unhandled checkpoint.
process.on('rejectionHandled',()=>{});
const pins={
 txIndex:['legacy_txindex_source_txIndex.txt','095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51'],
 txUtils:['source_channel_txutils_reference.txt','38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3'],
 types:['legacy_types_source_types.txt','a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1'],
 txEncoder:['legacy_types_source_txEncoder.txt','c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42'],
 txDecoder:['legacy_types_source_txDecoder.txt','a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076'],
 base94:['legacy_base94_source.txt','0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb'],
 base256:['legacy_base256_source.txt','11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc'],
 converter:['legacy_base_converter_source.txt','a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62'],
};
const source=Object.fromEntries(Object.entries(pins).map(([name,[file,sha]])=>{const code=fs.readFileSync(path.join(root,'fixtures',file),'utf8').replace(/\r\n/g,'\n');assert.equal(crypto.createHash('sha256').update(code).digest('hex'),sha,file);return[name,code];}));
assert.equal(crypto.createHash('sha1').update(Buffer.concat([Buffer.from(`blob ${Buffer.byteLength(source.txIndex)}\0`),Buffer.from(source.txIndex)])).digest('hex'),'3d7bdafbdffe5af7d3250c4dcc80b8358c485597');
function bigNumber(version){
 if(version==='9.3.1')return require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
 assert.equal(version,'9.1.2');const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],code=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));
 assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const m={exports:{}};vm.runInNewContext(code.toString('utf8'),{module:m,exports:m.exports},{timeout:1000});return m.exports;
}
const clone=v=>structuredClone(v),exception=e=>({name:e.name,message:e.message});
class Environment {
 constructor(BN){this.BN=BN;this.db=new Datastore();this.effects=[];this.logs=[];this.active=new Set();this.failures=[];this.rpc={tip:0,blocks:[],transactions:[]};this.handles=[];this.promiseIDs=new WeakMap();this.promises=[];this.backgroundErrors=[];currentEnvironment=this;this.start();}
 async diskDatabase(){if(!this.databaseFile){const scratch=path.join(stage,'work','nedb');fs.mkdirSync(scratch,{recursive:true});const directory=fs.mkdtempSync(path.join(scratch,'case-'));this.databaseFile=path.join(directory,'txIndex.db');}await new Promise((resolve,reject)=>{this.db=new Datastore({filename:this.databaseFile,autoload:true,onload:error=>error?reject(error):resolve()});});}
 encode(value,seen=new Set()){
  if(value===undefined)return{$undefined:true};if(typeof value==='number'){if(Number.isNaN(value))return{$number:'NaN'};if(!Number.isFinite(value))return{$number:String(value)};if(Object.is(value,-0))return{$number:'-0'};return value;}
  if(value===null||typeof value==='string'||typeof value==='boolean')return value;
  if(value===this.Index)return{$txIndexClass:true};if(value===this.client)return{$rpcClient:1};if(value===this.storage)return{$database:'txIndex'};const h=this.handles.indexOf(value);if(h>=0)return{$txIndexInstance:h+1};
  if(Buffer.isBuffer(value))return{$buffer:value.toString('hex')};
  if(Object.prototype.toString.call(value)==='[object RegExp]')return{$regexp:value.source,flags:value.flags};
  if(Object.prototype.toString.call(value).endsWith('Error]'))return{$error:exception(value)};
  if(value&&typeof value.then==='function')return{$promise:this.observePromise(value).id};
  if(this.BN.isBigNumber(value))return{$bignumber:value.toString()};
  if(typeof value==='function')return{$function:value.name};
  if(seen.has(value))throw Error('unexpected diagnostic cycle');seen.add(value);let result;
  if(Array.isArray(value))result=value.map(v=>this.encode(v,seen));else result=Object.fromEntries(Object.entries(value).map(([k,v])=>[k,this.encode(v,seen)]));seen.delete(value);return result;
 }
 observePromise(p){let found=this.promiseIDs.get(p);if(found)return found;found={id:this.promises.length+1,status:'unobserved'};this.promiseIDs.set(p,found);this.promiseValues.set(found.id,p);this.promises.push(found);return found;}
 fault(target){const i=this.failures.findIndex(f=>f.target===target);if(i<0)return;const f=this.failures[i];if(f.skip){f.skip--;return;}this.failures.splice(i,1);throw Error(f.message);}
 async external(target,args,run){const event={target,args:this.encode(args)};this.effects.push(event);try{this.fault(target);const v=await run();event.returned=this.encode(v);return v;}catch(e){event.thrown=exception(e);throw e;}}
 database(){const e=this;return Object.fromEntries(['findOne','find','update','insert','remove'].map(method=>[`${method}Async`,function(...args){const op=e.external(`db.${method}Async`,args,()=>util.promisify(e.db[method].bind(e.db))(...args));e.active.add(op);op.then(()=>e.active.delete(op),()=>e.active.delete(op));return op;}]));}
 start(){
  this.modules=new Map();this.handles=[];this.promiseIDs=new WeakMap();this.promises=[];this.promiseValues=new Map();this.observedPromises=new Set();
  const e=this;this.client={chain:'LTC',
   getBlockCount(...args){return e.external('client.getBlockCount',args,()=>e.rpc.tip);},
   getBlockHash(...args){return e.external('client.getBlockHash',args,()=>{const b=e.rpc.blocks.find(b=>b.height===args[0]);if(!b)throw Error(`missing block height ${args[0]}`);return b.block.hash;});},
   getBlock(...args){return e.external('client.getBlock',args,()=>{const b=e.rpc.blocks.find(b=>b.block.hash===args[0]);if(!b)throw Error(`missing block hash ${args[0]}`);return clone(b.block);});},
   getRawTransaction(...args){return e.external('client.getRawTransaction',args,()=>{const tx=e.rpc.transactions.find(tx=>tx.id===args[0]);if(!tx)throw Error(`missing raw transaction ${args[0]}`);return clone(args[1]===false?tx.hex:tx.decoded);});},
   decoderawtransaction(...args){return e.external('client.decoderawtransaction',args,()=>{const tx=e.rpc.transactions.find(tx=>tx.hex===args[0]);if(!tx)throw Error(`missing raw decoder input ${args[0]}`);return clone(tx.decoded);});},
  };
  this.storage=this.database();
  const clientWrapper={getInstance(...args){return e.external('ClientWrapper.getInstance',args,()=>Object.hasOwn(e,'clientValue')?e.clientValue:e.client);}};
  const forbidden=name=>new Proxy(function unavailable(){throw Error(`Unowned dependency ${name}`);},{get(_target,key){if(key==='then')return undefined;return forbidden(`${name}.${String(key)}`);}});
  this.adapters={'big-json':external('big-json'),'util':util,'bignumber.js':this.BN,'bitcore-lib-ltc':external('bitcore-lib-ltc'),'./client':clientWrapper,'./db':{getDatabase(...args){return e.external('db.getDatabase',args,()=>e.storage);}},'./consensus':forbidden('Consensus'),'./validity':forbidden('Validity')};
  this.context=vm.createContext({Buffer,process:{env:{}},console:Object.fromEntries(['log','error','warn'].map(level=>[level,(...args)=>e.logs.push({level,args:e.encode(args)})]))});
  this.Index=this.load('txIndex');
 }
 load(name){if(this.modules.has(name))return this.modules.get(name).exports;const m={exports:{}};this.modules.set(name,m);assert.ok(source[name],`pinned source ${name}`);
  vm.runInContext(`(function(exports,require,module){\n${source[name]}\n})`,this.context,{filename:`pinned/${name}.js`,timeout:3000})(m.exports,key=>{if(key.startsWith('./'))key=key.replace(/\.js$/,'');if(key==='bigint-base-converter')return this.load('converter');if(Object.hasOwn(this.adapters,key))return this.adapters[key];if(key.startsWith('./')&&source[key.slice(2)])return this.load(key.slice(2));throw Error(`Unmapped import ${name}:${key}`);},m);
  if(name==='types'){const original=m.exports.decodePayload,e=this;m.exports.decodePayload=function(...args){const event={target:'Types.decodePayload',args:e.encode(args)};e.effects.push(event);try{const p=original.apply(this,args);e.observePromise(p);event.returned=e.encode(p);return p;}catch(error){event.thrown=exception(error);throw error;}};}
  return m.exports;
 }
 async settle(){for(let i=0;i<100;i++){const count=this.effects.length;await new Promise(r=>setImmediate(r));if(!this.active.size&&count===this.effects.length){await new Promise(r=>setImmediate(r));if(!this.active.size&&count===this.effects.length){for(const found of this.promises){if(this.observedPromises.has(found.id))continue;this.observedPromises.add(found.id);await this.promiseValues.get(found.id).then(v=>{found.status='fulfilled';found.value=this.encode(v);},e=>{found.status='rejected';found.error=exception(e);});}return;}}}throw Error('bounded external operation drain exceeded');}
 state(){return this.encode({singleton:this.Index.instance,classClient:this.Index.client,classParseBlock:this.Index.parseBlock,instances:this.handles.map((v,i)=>({id:i+1,client:v.client,parseBlock:v.parseBlock})),txUtilsClient:this.load('txUtils').client,rows:this.db.getAllData(),forward:this.context.process.env.TL_FORWARD_INDEX,failures:this.failures,promises:this.promises});}
 configure(config){if(config.rpc)this.rpc=clone(config.rpc);if(config.failures)this.failures=clone(config.failures);if(Object.hasOwn(config,'clientValue'))this.clientValue=clone(config.clientValue);if(Object.hasOwn(config,'forward')){if(config.forward===undefined)delete this.context.process.env.TL_FORWARD_INDEX;else this.context.process.env.TL_FORWARD_INDEX=config.forward;}}
 async invoke(command){
  this.effects=[];this.logs=[];if(command.configure)this.configure(command.configure);let result,transport;
  try{if(command.scope==='restart'){if(command.database)await this.diskDatabase();this.start();result=undefined;transport='Immediate';}
   else if(command.scope==='constructor'){result=new this.Index(...(command.args||[]));if(!this.handles.includes(result))this.handles.push(result);transport='Immediate';}
   else {const receiver=command.scope==='instance'?this.handles[(command.instance||1)-1]:this.Index;result=receiver[command.method](...(command.args||[]));transport=result&&typeof result.then==='function'?'Pending':'Immediate';}
   if(transport==='Pending'){try{result=await result;}catch(e){transport='PendingThrow';result=e;}}
  }catch(e){transport='Throw';result=e;}
  if(result&&result===this.Index.instance&&!this.handles.includes(result))this.handles.push(result);
  const outcome=transport.endsWith('Throw')?{thrown:exception(result)}:{returned:this.encode(result)};
  const atReturn={state:this.state(),effects:this.encode(this.effects),backgroundErrors:clone(this.backgroundErrors)};
  await this.settle();return{command:this.encode(command),outcome:{transport,...outcome},atReturn,state:this.state(),effects:this.encode(this.effects),logs:this.encode(this.logs),backgroundErrors:clone(this.backgroundErrors)};
 }
}
const args=(method,...args)=>({scope:'static',method,args});
const inst=(method,...args)=>({scope:'instance',instance:1,method,args});
const initialized=()=>args('getInstance');
const input=(id,script,overrides={})=>({id,hex:`02${Buffer.from(id).toString('hex')}00`,decoded:{txid:id,vin:[{txid:'parent',vout:0}],vout:[{n:2,value:1,scriptPubKey:{type:'pubkeyhash',addresses:['change']}},{n:1,value:0.000006,scriptPubKey:{type:'pubkeyhash',addresses:['other']}},{n:0,value:0.000004,scriptPubKey:{type:'pubkeyhash',addresses:['first']}},{n:3,value:0,scriptPubKey:{type:'nulldata',hex:script}}],...overrides}});
const parent=()=>({id:'parent',hex:'02000000',decoded:{vin:[],vout:[{n:0,value:2.5,scriptPubKey:{type:'pubkeyhash',addresses:['sender']}}]}});
const carrier=body=>'6a'+(Buffer.byteLength(body)+2).toString(16).padStart(2,'0')+'746c'+Buffer.from(body).toString('hex');
const rpc=(transactions=[],blocks=[],tip=0)=>({transactions:[parent(),...transactions],blocks,tip});
async function build(version,proofOnly=false){let e=new Environment(bigNumber(version));await e.settle();e.effects=[];const records=[];async function call(name,command){records.push({case:name,...await e.invoke(command)});}
 async function reset(name,config={}){e=new Environment(bigNumber(version));await e.settle();if(config.durable)await e.diskDatabase();if(config.rows&&config.rows.length)await util.promisify(e.db.insert.bind(e.db))(clone(config.rows));e.configure(config);e.effects=[];records.push({case:`reset/${name}`,command:e.encode({scope:'reset',...config}),outcome:{transport:'Immediate',returned:{$undefined:true}},atReturn:{state:e.state(),effects:[],backgroundErrors:[]},state:e.state(),effects:[],logs:[],backgroundErrors:[]});}
 await call('proof/01-constructor',{scope:'constructor',args:[]});
 await call('proof/02-constructor-singleton',{scope:'constructor',args:[true]});
 await call('proof/03-static-init',args('init'));
 await call('proof/04-get-instance-reinitializes',args('getInstance',true));
 await call('proof/05-initialize-genesis',args('initializeIndex',123));
 await call('proof/06-existing-genesis',args('initializeIndex',999));
 await call('proof/07-missing-max',args('findMaxIndexedBlock'));
 await call('proof/08-save-height',args('saveMaxHeight',150));
 await call('proof/09-read-max',args('findMaxIndexedBlock'));
 await call('proof/10-index-flag',args('checkForIndex'));
 if(proofOnly)return records;
 await fullCorpus({call,reset,get environment(){return e;}});
 const invoked=new Set(records.flatMap(record=>record.command.method?[record.command.method]:[]));
 assert.deepEqual([...invoked].sort(),[...Object.getOwnPropertyNames(e.Index).filter(key=>typeof e.Index[key]==='function'),...Object.getOwnPropertyNames(e.Index.prototype).filter(key=>key!=='constructor')].sort(),'complete real method inventory');
 return records;
}
async function fullCorpus(driver){const {call,reset}=driver;
 await reset('cache-restart');
 await call('cache/static-client-absent',args('fetchChainTip'));
 await call('cache/constructor-does-not-set-class-client',{scope:'constructor',args:[]});
 await call('cache/client-still-absent',args('fetchChainTip'));
 await call('cache/static-init-only',args('init'));
 await call('cache/fetch-tip',args('fetchChainTip'));
 await call('cache/save-before-restart',args('saveMaxHeight',12));
 await call('cache/module-restart-retains-db',{scope:'restart'});
 await call('cache/max-survives-restart',args('findMaxIndexedBlock'));
 await call('cache/getInstance-after-restart',initialized());
 await call('cache/repeat-getInstance-repeats-client-init',initialized());
 for(const value of [undefined,null,false,0,'client-without-methods',{}]){
  const label=value===undefined?'undefined':JSON.stringify(value);
  await reset(`raw-client-value-${label}`);
  await call(`client/${label}-raw-factory-value-init`,{...args('init'),configure:{clientValue:value}});
  await call(`client/${label}-raw-factory-value-fetch-tip`,args('fetchChainTip'));
 }

 await reset('height-and-flags',{rows:[{_id:'indexExists',value:false}]});
 await call('height/check-presence-not-value',args('checkForIndex'));
 for(const value of [undefined,null,0,false,'0',NaN,-0,'12',Infinity])await call(`height/save-${typeof value}-${driver.environment.encode(value)?.$number||String(value)}`,args('saveMaxHeight',value));
 await call('height/find-raw-infinity',args('findMaxIndexedBlock'));
 await call('height/reset-unbound-global',args('resetIndexFlag'));
 await call('height/clear-unbound-Datastore',args('clearTxIndex'));
 await call('height/load-flag-rows-as-object-after-constructor',{scope:'constructor'});
 await call('height/load-index',inst('loadIndex'));

 await reset('instance-save-load');await call('rows/constructor',{scope:'constructor'});
 await call('rows/init-or-load-genesis',inst('initializeOrLoadDB','001'));
 await call('rows/init-or-load-retains-original',inst('initializeOrLoadDB',999));
 await call('rows/first-save-uses-last-argument',inst('saveTransactionData','id',{actual:'tx-data'},'body',8,{first:'details'}));
 await call('rows/query-row-without-value',args('getTransactionData','id'));
 await call('rows/update-uses-second-argument-and-payload',inst('saveTransactionData','id',{updated:true},'new-body',8,{ignored:true}));
 await call('rows/load-index-has-undefined-value',inst('loadIndex'));
 await call('rows/validity-on-instance-saved-row',args('upsertTxValidityAndReason','id',18,false,'invalid-source'));
 await call('rows/missing-validity-does-not-create',args('upsertTxValidityAndReason','missing',1,true,''));
 await call('rows/absent-id-is-null',args('getTransactionData','absent'));
 await call('rows/restart',{scope:'restart'});await call('rows/restart-new-singleton',initialized());await call('rows/restart-load',inst('loadIndex'));

 await reset('row-lookup-regex',{rows:[{_id:'tx-2-a',value:'second'},{_id:'tx-10-a',value:'first-neDB-order'},{_id:'tx-3-ab',value:'ab'},{_id:'txNotDash-a',value:'suffix-only'},{_id:'other-a',value:'not-tx'},{_id:'2',value:'integer-key'}]});
 await call('lookup/exact-pattern-first-db-match',args('getTransactionData','a'));
 await call('lookup/first-prefix-suffix-row-validity',args('upsertTxValidityAndReason','a','18','false',undefined));
 for(const id of ['[','(',')'])await call(`lookup/invalid-regexp-${id}`,args('getTransactionData',id));
 await call('lookup/regex-metacharacters-are-source-semantics',{...args('getTransactionData','.'),domain:'arbitrary-regexp'});
 await call('lookup/constructor',{scope:'constructor'});await call('lookup/load-object-integer-key-order',inst('loadIndex'));

 await reset('rpc-and-decode',{rpc:rpc([input('ordinary',carrier('i1,price'))],[{height:5,block:{hash:'block5',tx:['ordinary']}}],5)});
 await call('rpc/init',initialized());await call('rpc/chain-tip',args('fetchChainTip'));await call('rpc/block-height',args('fetchBlockData',5));
 await call('rpc/raw-original-arity',args('fetchTransactionData','ordinary',false,'block5'));
 await call('rpc/verbose-request',args('fetchTransactionData','ordinary',true));
 await call('rpc/decode-raw',args('DecodeRawTransaction',driver.environment.rpc.transactions.find(t=>t.id==='ordinary').hex));
 await call('rpc/real-TxUtils-and-unawaited-Types',args('processTransaction','i1,price','ordinary','tl'));
 await call('rpc/payload-tl-unknown-type-future',args('processTransaction','tl','ordinary','tl'));
 await call('rpc/numeric-marker-rejected-future',args('processTransaction','tl','ordinary',18));
 await call('rpc/process-block-actual-promise-copy',args('processBlockData',{hash:'block5',tx:['ordinary']},5));
 await call('rpc/persisted-promise-is-empty-object',args('getTransactionData','ordinary'));
 await call('rpc/validity-is-top-level',args('upsertTxValidityAndReason','ordinary',18,false,'future-error'));

 for(const [name,script,decoded]of [
  ['normal-offset4',carrier('20;alice;5;2')],['fallback-offset2','6a746c3230'],['offset5','6a4c0746c3230'],['offset6','6a4c02746c3230'],['too-late','6a4c0000746c3230'],['uppercase-marker','6a02746C3230'],['odd-trailing-nibble','6a02746c3230f'],['empty-payload','6a02746c'],['non-ascii-byte','6a02746c00'],['high-latin1-byte','6a02746cff'],['invalid-hex-truncation','6a02746c32zz33'],['no-nulldata',null,{vout:[]}],['null-decoded',null,null],['malformed-vout',null,{vout:[{}]}],['first-nulldata-wins',null,{vout:[{scriptPubKey:{type:'nulldata',hex:'6a026869'}},{scriptPubKey:{type:'nulldata',hex:carrier('i1,price')}}]}]
 ]){
  const tx=input('decode',script);if(decoded!==undefined)tx.decoded=decoded;
  await call(`decode/${name}`,{...args('DecodeRawTransaction',tx.hex),configure:{rpc:rpc([tx])}});
 }
 for(const value of ['', ' ~', 'line\n', 'é',undefined,null,1,[],['A','B'],{$buffer:'00'},{$buffer:'ff'}])await call(`ascii/${JSON.stringify(driver.environment.encode(value))}`,args('isPrintableASCII',value));

 await reset('whole-block-prefixes',{rpc:rpc([input('A',carrier('i1,100')),input('B',carrier('20;alice;5;2')),input('C','6a026869')])});
 await call('block/init',initialized());await call('block/sequential-mixed',args('processBlockData',{hash:'given',tx:['A','C','B']},20));
 await call('block/retry-duplicates-update-docs',args('processBlockData',{hash:'given',tx:['A','B']},20));
 await call('block/no-marker-does-not-advance-max',args('processBlockData',{hash:'given',tx:['C']},30));
 await call('block/malformed-tx-list',args('processBlockData',{hash:'given',tx:null},31));

 await failuresCorpus(driver);
}
async function failuresCorpus(driver){const {call,reset}=driver;
 for(const [name,command,target,skip]of [
  ['genesis-database',args('initializeIndex',1),'db.getDatabase',0],['genesis-read',args('initializeIndex',1),'db.findOneAsync',0],['genesis-insert',args('initializeIndex',1),'db.insertAsync',0],
  ['height-first-update',args('saveMaxHeight',9),'db.updateAsync',0],['height-flag-update',args('saveMaxHeight',9),'db.updateAsync',1],
  ['max-read',args('findMaxIndexedBlock'),'db.findOneAsync',0],['check-swallow',args('checkForIndex'),'db.findOneAsync',0],['query-rethrow',args('getTransactionData','a'),'db.findOneAsync',0],
  ['validity-scan',args('upsertTxValidityAndReason','a',18,true,''),'db.findAsync',0],['validity-write',args('upsertTxValidityAndReason','a',18,true,''),'db.updateAsync',0],
  ['instance-init-read',inst('initializeOrLoadDB',1),'db.findOneAsync',0],['instance-save-insert-swallow',inst('saveTransactionData','new',{},'body',1,{}),'db.insertAsync',0],
  ['instance-save-update-swallow',inst('saveTransactionData','a',{},'body',1,{}),'db.updateAsync',0],['instance-load',inst('loadIndex'),'db.findAsync',0],
 ]){
  await reset(`fault-${name}`,{rows:[{_id:'tx-1-a',value:{before:true}}]});await call(`fault/${name}-constructor`,{scope:'constructor'});
  await call(`fault/${name}`,{...command,configure:{failures:[{target,skip,message:name}]}});await call(`retry/${name}`,command);
 }
 for(const [name,command,target,skip]of [
  ['client-init',args('init'),'ClientWrapper.getInstance',0],['chain-tip-wrapped',args('fetchChainTip'),'client.getBlockCount',0],['block-hash-rethrow',args('fetchBlockData',1),'client.getBlockHash',0],['block-rethrow',args('fetchBlockData',1),'client.getBlock',0],['raw-rethrow',args('fetchTransactionData','T',false),'client.getRawTransaction',0],['decoder-swallow',args('DecodeRawTransaction',input('T',carrier('i1,100')).hex),'client.decoderawtransaction',0],
  ['sender-first-read-caught',args('processTransaction','i1,100','T','tl'),'client.getRawTransaction',0],['sender-parent-read-rejects',args('processTransaction','i1,100','T','tl'),'client.getRawTransaction',1],['outputs-read-caught-then-filter-throws',args('processTransaction','i1,100','T','tl'),'client.getRawTransaction',2],
  ['block-row-write-caught',args('processBlockData',{hash:'h1',tx:['T']},1),'db.updateAsync',0],['block-max-write-rejects',args('processBlockData',{hash:'h1',tx:['T']},1),'db.updateAsync',1],['block-flag-write-rejects',args('processBlockData',{hash:'h1',tx:['T']},1),'db.updateAsync',2],
 ]){
  await reset(`rpcfault-${name}`,{rpc:rpc([input('T',carrier('i1,100'))],[{height:1,block:{hash:'h1',tx:['T']}}],1)});await call(`rpcfault/${name}-init`,initialized());
  await call(`rpcfault/${name}`,{...command,configure:{failures:[{target,skip,message:name}]}});await call(`rpcretry/${name}`,command);
 }

 for(const [name,tx]of [
  ['no-vin',input('T',carrier('i1,100'),{vin:[]})],['no-parent-id',input('T',carrier('i1,100'),{vin:[{vout:0}]})],['invalid-parent-vout',input('T',carrier('i1,100'),{vin:[{txid:'parent',vout:100}]})],['string-vout-excluded',input('T',carrier('i1,100'),{vout:[{n:'0',value:1,scriptPubKey:{addresses:['string-index']}}]})],['output-fraction-and-address-filter',input('T',carrier('i1,100'),{vout:[{n:0,value:0.000000015,scriptPubKey:{addresses:['first']}},{n:1,value:'0.000000025',scriptPubKey:{addresses:['second']}},{n:1,value:9,scriptPubKey:{addresses:[]}},{n:2,value:2,scriptPubKey:{addresses:['excluded']}}]})],['missing-output-script-caught',input('T',carrier('i1,100'),{vout:[{n:0,value:1}]})]
 ]){await reset(`raw-input-${name}`,{rpc:rpc([tx])});await call(`raw-input/${name}`,args('processTransaction','i1,100','T','tl'));}

 const emptyBlocks=[0,1,2,3].map(height=>({height,block:{hash:`height-${height}`,tx:[]}}));
 for(const [name,start,forward,rows,tip]of [
  ['normal-from-1',1,undefined,[],2],['undefined-start-defaults0',undefined,undefined,[],1],['null-start-defaults0',null,undefined,[],1],['already-above-tip',3,undefined,[],2],
  ['forward-no-max-starts-tip',0,'1',[],2],['forward-resumes-plus1',0,'true',[{_id:'MaxHeight',value:1}],3],['forward-string-max-concatenates',0,'1',[{_id:'MaxHeight',value:'1'}],3],['forward-false-string-does-not-enable',1,'false',[],2],['forward-uppercase-does-not-enable',1,'TRUE',[],2],
 ]){await reset(`extract-${name}`,{rows,rpc:rpc([],emptyBlocks,tip),forward});await call(`extract/${name}-init`,initialized());await call(`extract/${name}`,args('extractBlockData',start));await call(`extract/${name}-max-after-background`,args('findMaxIndexedBlock'));}
 await reset('extract-failed-max-falls-back',{rpc:rpc([],emptyBlocks,2),forward:'1'});await call('extract/failure-init',initialized());await call('extract/max-read-error-falls-back-to-original-start',{...args('extractBlockData',1),configure:{failures:[{target:'db.findOneAsync',skip:0,message:'resume-read'}]}});
 for(const skip of [0,1]){
  await reset(`extract-background-save-rejection-${skip}`,{rpc:rpc([],emptyBlocks,1)});await call(`extract/background-${skip}-init`,initialized());
  await call(`extract/background-${skip}-primary-return-is-undefined`,{...args('extractBlockData',1),configure:{failures:[{target:'db.updateAsync',skip,message:`unawaited-save-${skip}`}]}});
  await call(`extract/background-${skip}-read-partial-prefix`,args('findMaxIndexedBlock'));
 }

 await reset('durable-nedb-restart',{durable:true});await call('durable/constructor',{scope:'constructor'});
 await call('durable/genesis-object-with-undefined-and-nan',args('initializeIndex',{missing:undefined,nan:NaN,negativeZero:-0,positiveInfinity:Infinity,array:[undefined,NaN,-0]}));
 await call('durable/height-nan-memory',args('saveMaxHeight',NaN));
 await call('durable/load-before-real-db-restart',inst('loadIndex'));
 await call('durable/reload-real-persisted-database',{scope:'restart',database:true});
 await call('durable/new-instance',initialized());await call('durable/load-after-real-db-restart',inst('loadIndex'));
 await call('durable/max-nan-persisted-as-null',args('findMaxIndexedBlock'));

 // Actual serializable raw transactions from the existing independent hash
 // vectors. RPC documents remain explicit unverified inputs, not chain proof.
 const bitcoin=external('bitcoinjs-lib');
 const rawVectors=fs.readFileSync(path.join(root,'fixtures/txindex_segwit_hash_vectors.tsv'),'utf8').split('\n').filter(line=>line.startsWith('legacy\t')||line.startsWith('segwit_same_txid\t'));
 for(const line of rawVectors){const [label,hex,,rawID]=line.split('\t'),parsed=bitcoin.Transaction.fromHex(hex),id=parsed.getId();assert.equal(Buffer.from(rawID,'hex').reverse().toString('hex'),id);
  const tx={id,hex,decoded:{txid:id,version:parsed.version,locktime:parsed.locktime,vin:parsed.ins.map(v=>({txid:Buffer.from(v.hash).reverse().toString('hex'),vout:v.index,sequence:v.sequence})),vout:parsed.outs.map((v,n)=>({n,value:Number(v.value)/1e8,scriptPubKey:{type:'nulldata',hex:Buffer.from(v.script).toString('hex')}}))}};
  const parentInput={id:'22'.repeat(32),hex:'02000000',decoded:{vin:[],vout:[0,1,2,3].map(n=>({n,value:n+1,scriptPubKey:{type:'pubkeyhash',addresses:['sender']}}))}};
  await reset(`serialized-raw-${label}`,{rpc:{tip:1,blocks:[],transactions:[tx,parentInput]}});await call(`serialized/${label}-init`,initialized());
  await call(`serialized/${label}-fetch`,args('fetchTransactionData',id,false,'unverified-block'));
  await call(`serialized/${label}-decode`,args('DecodeRawTransaction',hex));
  await call(`serialized/${label}-whole-process`,args('processBlockData',{hash:'unverified-block',tx:[id]},1));
 }
 await reset('startup-client-promise-rejection',{rpc:rpc([input('T',carrier('i1,100'))])});
 await call('startup/restart-retains-original-rejected-client-promise',{scope:'restart',configure:{failures:[{target:'ClientWrapper.getInstance',skip:0,message:'startup-factory'}]}});
 await call('startup/outputs-awaits-same-original-rejection',args('processTransaction','i1,100','T','tl'));

 // Real NeDB validates nested document keys before publishing a write. These
 // failures are produced by the actual datastore, not the injected-fault hook.
 await reset('real-nedb-invalid-source-document-keys');
 await call('bad-key/constructor',{scope:'constructor'});
 await call('bad-key/genesis-nested-dollar-rejects',args('initializeIndex',{nested:{$bad:1}}));
 await call('bad-key/genesis-valid-retry',args('initializeIndex','good'));
 await call('bad-key/first-save-dotted-last-argument-caught',inst('saveTransactionData','bad',{},'p',1,{'dotted.key':1}));
 await call('bad-key/first-save-valid-last-argument-retry',inst('saveTransactionData','bad',{},'p',1,{good:'first'}));
 await call('bad-key/update-nested-dollar-second-argument-caught',inst('saveTransactionData','bad',{nested:{$bad:1}},'changed',1,{ignored:true}));
 await call('bad-key/update-valid-second-argument-retry',inst('saveTransactionData','bad',{good:'updated'},'changed',1,{ignored:true}));
 await call('bad-key/validity-nested-dollar-reason-rejects',args('upsertTxValidityAndReason','bad',18,false,{nested:{$bad:1}}));
 await call('bad-key/validity-dollar-reason-valid-retry',args('upsertTxValidityAndReason','bad',18,false,'valid-string'));
 await call('bad-key/validity-dotted-reason-rejects',args('upsertTxValidityAndReason','bad',18,true,{'dotted.key':1}));
 await call('bad-key/validity-dotted-reason-valid-retry',args('upsertTxValidityAndReason','bad',18,true,'final-string'));
}
async function boundary(version){
 const e=new Environment(bigNumber(version));await e.settle();const model=external('nedb/lib/model');
 const fulfilled=Promise.resolve(new Error('Invalid payload')),rejected=Promise.reject(new TypeError('decoder rejection'));rejected.catch(()=>{});
 const input={_id:'boundary',decodedParams:fulfilled,rejectedParams:rejected,error:new Error('returned error'),missing:undefined,nan:NaN,infinity:Infinity,negativeInfinity:-Infinity,negativeZero:-0,array:[undefined,NaN,-0]};
 const original=e.encode(input),copied=model.deepCopy(input),serialized=model.serialize(input),reloaded=model.deserialize(serialized);await e.settle();
 const rejectedFields=[];for(const value of [{_id:'bad',nested:{$bad:1}},{_id:'bad','dotted.key':1}])try{model.serialize(value);throw Error('bad source key accepted');}catch(error){rejectedFields.push({input:e.encode(value),thrown:exception(error)});}
 const db=new Datastore(),insert=util.promisify(db.insert.bind(db));await insert({_id:'same',value:1});let duplicate;
 try{await insert({_id:'same',value:2});throw Error('duplicate accepted');}catch(error){duplicate={thrown:exception(error),properties:e.encode(Object.fromEntries(Object.entries(error))),rows:e.encode(db.getAllData())};}
 return{boundary:'genuine NeDB1.8.0 under narrowly restored legacy Node util helpers',legacyHost:['util.isDate=util.types.isDate','util.isRegExp=util.types.isRegExp'],original,memoryCopy:e.encode(copied),serialized,reloaded:e.encode(reloaded),futures:clone(e.promises),rejectedFields,duplicate};
}
async function inventory(records,version){const e=new Environment(bigNumber(version));await e.settle();const methods=[];
 for(const[scope,receiver]of [['static',e.Index],['instance',e.Index.prototype]])for(const name of Object.getOwnPropertyNames(receiver)){
  if(name==='constructor'||typeof receiver[name]!=='function')continue;const fn=receiver[name],start=source.txIndex.indexOf(fn.toString());assert.ok(start>=0,`genuine function ${name}`);
  methods.push({name,scope,async:fn.constructor.name==='AsyncFunction',line:source.txIndex.slice(0,start).split('\n').length,bodySha256:crypto.createHash('sha256').update(fn.toString()).digest('hex'),cases:records.filter(r=>r.command.scope===scope&&r.command.method===name).map(r=>r.case)});
 }
 const scopes=Object.fromEntries(['reset','constructor','restart','static','instance'].map(scope=>[scope,records.filter(r=>r.command.scope===scope).length]));
 return{sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',sourcePath:'src/txIndex.js',gitBlob:'3d7bdafbdffe5af7d3250c4dcc80b8358c485597',lfSha256:pins.txIndex[1],completeBytes:Buffer.byteLength(source.txIndex),callableMethods:methods.length,constructor:{line:source.txIndex.slice(0,source.txIndex.indexOf('constructor()')).split('\n').length,cases:records.filter(r=>r.command.scope==='constructor').map(r=>r.case)},observations:records.length,scopes,methods,pins,dependencies:{nedb:'1.8.0',modelSha256:'57d4551a20abcd4efe48880700d30378f09b7e8b145617010729562837b27258',datastoreSha256:'9c5575bdaba5127b748305a27f0cac7727595aa9f2f2c15f8735254d765df8ee',bigNumberBaseline:'9.3.1',bigNumberBaselineSha256:'58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16',bigNumberLockReference:'9.1.2',bigNumberLockSha256:'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e'},diagnosticOnlyCases:records.filter(r=>r.command.domain).map(r=>({case:r.case,domain:r.command.domain})),recordSha256:crypto.createHash('sha256').update(records.map(r=>JSON.stringify(r)).join('\n')+'\n').digest('hex')};
}
async function main(){const flags=process.argv.slice(2),proofOnly=flags.includes('--proof10'),version=flags.includes('--lock')?'9.1.2':'9.3.1';const records=await build(version,proofOnly);
 function output(name,text){const file=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),text,`checked ${name} differs`);else fs.writeFileSync(file,text);}
 output(proofOnly?'txindex_source_proof10.jsonl':'txindex_source_vectors.jsonl',records.map(r=>JSON.stringify(r)).join('\n')+'\n');
 if(!proofOnly){output('txindex_source_method_inventory.json',JSON.stringify(await inventory(records,version),null,2)+'\n');output('txindex_nedb_boundary.json',JSON.stringify(await boundary(version),null,2)+'\n');}
 console.log(`${flags.includes('--check')?'PASS':'WROTE'} ${records.length} genuine txIndex observations (${version})`);
}
module.exports={Environment,build,pins,legacyHost};
if(require.main===module)main().catch(error=>{console.error(error.stack||error);process.exitCode=1});
