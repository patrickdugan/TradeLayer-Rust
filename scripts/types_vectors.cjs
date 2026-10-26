#!/usr/bin/env node
'use strict';
// Complete unchanged Types and codec source, genuine archived numeric modules.
// Identity/replacement validity modes are explicitly dispatch-only boundaries.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const argv = process.argv.slice(2), stage = path.resolve(__dirname, '..'), at = argv.indexOf('--repo');
const repo = at >= 0 ? path.resolve(argv[at + 1]) : [stage, path.resolve(stage, '../../outputs/TradeLayer-Rust')].find(p => fs.existsSync(path.join(p, 'fixtures/legacy_types_source_types.txt')));
const {loadBigNumber, readPinnedSource} = require(path.join(repo, 'scripts/legacy_numeric_oracle.cjs'));
const pins = {
  types: ['legacy_types_source_types.txt','a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1'],
  encoder: ['legacy_types_source_txEncoder.txt','c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42'],
  decoder: ['legacy_types_source_txDecoder.txt','a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076'],
  converter: ['legacy_base_converter_source.txt','a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62'],
  base94: ['legacy_base94_source.txt','0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb'],
  base256: ['legacy_base256_source.txt','11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc'],
};
function load(name, imports) {
  const [file, hash] = pins[name], module = {exports:{}};
  vm.runInNewContext(readPinnedSource(path.join(repo,'fixtures',file),hash), {module,exports:module.exports,console:{log(){},error(){},warn(){}},Buffer,
    require(key){ assert(Object.hasOwn(imports,key),`unexpected ${key}`); return imports[key]; }}, {filename:file,timeout:2000});
  return module.exports;
}
const BN = loadBigNumber(), converter = load('converter',{});
const numeric = {'bignumber.js':BN,'./base94.js':load('base94',{'bigint-base-converter':converter}),'./base256.js':load('base256',{'bigint-base-converter':converter})};
const encoder = load('encoder',numeric), decoder = load('decoder',numeric);
const routeRows = fs.readFileSync(path.join(repo,'fixtures/legacy_types_routes.tsv'),'utf8').split(/\r?\n/).filter(l=>l&&!l.startsWith('#')&&!l.startsWith('id\t')).map(l=>l.split('\t'));
assert.equal(routeRows.length,36); assert.equal(Object.keys(decoder).length,36);
function tag(v) {
  if(v===undefined)return['undefined']; if(v===null)return['null'];
  if(typeof v==='boolean')return['boolean',v]; if(typeof v==='string')return['string',v];
  if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}
  if(BN.isBigNumber(v))return['bignumber',v.toString()];
  if(Object.prototype.toString.call(v)==='[object Promise]')return['promise'];
  if(Array.isArray(v))return['array',Array.from(v,tag)];
  return['object',Object.keys(v).map(k=>[k,tag(v[k])])];
}
const error = e => [e.name,e.message];
const cloneTag = v => JSON.parse(JSON.stringify(tag(v)));
const hex = v => Buffer.from(JSON.stringify(v)).toString('hex');
const rows = [], covered = new Set();
let sourceHarness;
function fullValidityEnvironment(overrides) {
  if(!sourceHarness){
    const file=[path.join(__dirname,'validity_vectors.cjs'),path.resolve(__dirname,'../../full-validity-port/scripts/validity_vectors.cjs')].find(fs.existsSync);
    const before=process.argv.length;process.argv.push('--repo',repo);
    try{sourceHarness=require(file);}finally{process.argv.length=before;}
  }
  return sourceHarness.environment(overrides);
}
const base = (type,body='1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1') => ['a'.repeat(64),type,'tl',body,'actor',[{vout:0,address:'zero'},{vout:1,address:'one'},{vout:2,address:'two'},{vout:3,address:'three'}],12.5,25.25,101];
async function observe(name,args,mode='pending-identity') {
  const initial = cloneTag(args), state = {decoder:null,validity:null,lookup:null};
  const overrides=mode==='source-inactive'?{'Activation.isTxTypeActive':false}:mode==='source-deferred'?{'Tally.hasSufficientChannel':{$mode:'pending-throw',name:'Error',message:'snapshot read rejected'}}:mode==='source-awaited-rejection'?{'Activation.isTxTypeActive':{$mode:'pending-throw',name:'Error',message:'snapshot read rejected'}}:{};
  const sourceEnv=mode.startsWith('source')?fullValidityEnvironment(overrides):null;
  const proxied = new Proxy(decoder,{get(target,key){
    state.lookup = String(key);
    const method=target[key]; if(typeof method!=='function')return method;
    return function(body){const record={method:String(key),body,returned:null,thrown:null};state.decoder=record;
      try{const value=method.call(target,body);record.returned=cloneTag(value);return value;}
      catch(e){record.thrown=error(e);throw e;}};
  }});
  const validity = new Proxy({},{get(_target,method){return function(...arguments_) {
    const record={method:String(method),arguments:cloneTag(arguments_),pending:!mode.startsWith('immediate'),outcome:'returned',value:null,argumentsAfter:null};state.validity=record;
    if(sourceEnv){
      let returned;
      try{returned=sourceEnv.validity[method](...arguments_);}
      catch(e){record.outcome='thrown';record.value=error(e);record.argumentsAfter=cloneTag(arguments_);throw e;}
      return Promise.resolve(returned).then(value=>{record.value=cloneTag(value);record.argumentsAfter=cloneTag(arguments_);return value;},e=>{record.outcome='thrown';record.value=error(e);record.argumentsAfter=cloneTag(arguments_);throw e;});
    }
    let value=arguments_[1], thrown=null;
    if(mode.endsWith('mutate')&&value!==null&&typeof value==='object'){value.valid=true;value.reason='adapter-observation';value.adapterOnly=true;}
    if(mode.endsWith('null'))value=null;
    if(mode.endsWith('undefined'))value=undefined;
    if(mode.endsWith('replacement'))value={valid:false,reason:'replacement'};
    if(mode.endsWith('bad-valid'))value={valid:{toString:0},reason:''};
    if(mode.endsWith('throw')){thrown=new Error('validity adapter failure');thrown.name='RangeError';record.outcome='thrown';}
    record.value=thrown?error(thrown):cloneTag(value);record.argumentsAfter=cloneTag(arguments_);
    if(thrown){if(record.pending)return Promise.reject(thrown);throw thrown;}
    return record.pending?Promise.resolve(value):value;
  };}});
  const types=load('types',{'./txEncoder':encoder,'./txDecoder':proxied,'./validity':validity,'./txUtils':{},'./txIndex.js':{},'bignumber.js':BN});
  let status,value,caught;
  const promise=types.decodePayload(...args);
  assert.equal(Object.prototype.toString.call(promise),'[object Promise]');
  try{const result=await promise;if(Object.prototype.toString.call(result)==='[object Error]'){status='returned-error';value=error(result);}else{status='returned';value=tag(result);}}
  catch(e){caught=e;status='rejected';value=error(e);if(!state.decoder&&state.lookup&&e.message===`Decode.${state.lookup} is not a function`){state.decoder={method:state.lookup,body:args[3],returned:null,thrown:error(e)};}}
  await Promise.resolve();
  if(state.validity){state.validity.dependencies=sourceEnv?sourceEnv.trace:[];state.validity.deferredRejections=sourceEnv?sourceEnv.deferred.filter(e=>e!==caught).map(error):[];state.validity.globalWrites=sourceEnv?sourceEnv.globalWrites():[];}
  const output={async:true,status,value,argumentsAfter:tag(args),decoder:state.decoder,validity:state.validity};
  const scope = /\\ud[89ab][0-9a-f]{2}|\\ud[cdef][0-9a-f]{2}/i.test(JSON.stringify(output)) ? 'unsupported-utf16-exception' : 'dispatch-only';
  rows.push(['T',name,hex(initial),mode,hex(output),sourceEnv?'source-rules-snapshot-dependencies':scope,hex(overrides),hex(sourceEnv?sourceEnv.trace:[])].join('\t'));
  if(typeof args[1]==='number'&&Number.isInteger(args[1])&&args[1]>=0&&args[1]<36)covered.add(args[1]);
}
async function main(){
  for(let type=0;type<36;type++)for(const mode of ['pending-identity','immediate-identity','pending-mutate','pending-null','pending-undefined','pending-throw','immediate-throw','pending-replacement','pending-bad-valid'])await observe(`route-${type}-${mode}`,base(type),mode);
  // Extend the complete independently executed decoder corpus into real Types
  // mutation/dispatch. Helpers without a Types route remain codec-only tests.
  for(const line of fs.readFileSync(path.join(repo,'fixtures/legacy_txdecoder_historical_vectors.tsv'),'utf8').split(/\r?\n/).filter(l=>l&&!l.startsWith('#'))){
    const f=line.split('\t'), route=routeRows.find(r=>r[3]===f[0]);if(!route)continue;
    const body=f[2]==='-'?'':Buffer.from(f[2],'hex').toString('utf8');
    await observe(`codec-${f[0]}-${f[1]}`,base(Number(route[0]),body));
  }
  for(const type of [null,true,false,'7',{},[],NaN,Infinity,-1,36,-0])await observe(`selector-${JSON.stringify(tag(type))}`,base(type));
  for(const marker of [undefined,null,false,true,0,'bad',['tl'],[['tl']],['tl',''],{}, {toString:0},{valueOf:0}]){const args=base(7);args[2]=marker;await observe(`marker-${JSON.stringify(tag(marker))}`,args);}
  for(const body of [undefined,null,0,false,[],{}, {toString:0}])for(const type of [0,7,28]){const args=base(type);args[3]=body;await observe(`body-${type}-${JSON.stringify(tag(body))}`,args);}
  for(const refs of [undefined,null,0,'address',{},[],[null],[{}],[{vout:'0',address:'string'}],[{vout:0},{vout:1,address:'one'}],[{vout:0,address:'first'},{vout:0,address:'second'},{vout:1,address:'delivery'}]]){const args=base(3,'7,1,0,a,1,0,0,1');args[5]=refs;await observe(`utxo-refs-${JSON.stringify(tag(refs))}`,args);}
  for(const refs of [undefined,null,0,'literal',{},[],[null,{}, {address:''},{address:'first'},{address:'second'}],{address:'preferred',senderAddress:'backup'},{address:'',senderAddress:'backup'},{address:7,senderAddress:'fallback'}]){const args=base(11,'7,1,');args[5]=refs;await observe(`grant-refs-${JSON.stringify(tag(refs))}`,args);}
  for(const type of [0,7,18,35]){const args=base(type);args[8]={toString:0};await observe(`block-coercion-${type}`,args);}
  for(const args of [[],['tx'],['tx','tl','payload'],['tx','tl','tl'],[...base(7),'ignored-extra']])await observe(`arity-${args.length}-${rows.length}`,args);
  for(let type=0;type<36;type++)for(const mode of ['source-normal','source-inactive'])await observe(`full-source-${type}-${mode}`,base(type),mode);
  await observe('full-source-global-order',base(18,'1,4ldqpds,2,0,0,0,0,0'),'source-global-order');
  await observe('full-source-deferred-channel',base(20,'5,6,5yc1s,5yc1s,1,5k,0,0,0'),'source-deferred');
  await observe('full-source-awaited-rejection',base(7,'backup,List,url,desc'),'source-awaited-rejection');
  assert.equal(covered.size,36);
  const text='# Full unchanged Types+codec source; genuine archived numeric modules.\n# T name taggedArgsJSONhex validityAdapterMode diagnosticJSONhex evidenceScope backendOverridesJSONhex dependencyTraceJSONhex\n'+Object.entries(pins).map(([n,[p,h]])=>`# sha256 ${n} ${h} ${p}`).join('\n')+'\n# source-defaults '+hex(sourceHarness.defaults)+'\n# source-pending '+hex([...sourceHarness.asyncTargets])+'\n'+rows.join('\n')+'\n';
  const file=path.join(stage,'fixtures/types_vectors.tsv');if(argv.includes('--write'))fs.writeFileSync(file,text);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),text);
  console.log(`PASS Types all36 routes ${rows.length} full-source dispatch observations; genuine BigNumber9.3.1/Base94/Base256; Node ${process.version}`);
}
main().catch(e=>{console.error(e);process.exitCode=1;});
