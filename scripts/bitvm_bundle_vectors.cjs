#!/usr/bin/env node
'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const {readPinnedSource}=require('./legacy_numeric_oracle.cjs');
const root=path.resolve(__dirname,'..');
const hashes={bitvm_bundle:'e63a2f2e09e195a03ab2e3acb3fb6f582917c628a4c08774911c4ba6558903fe',bitvm_cache:'053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca',bitvm_risk:'14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66'};
const sourceNames={bitvm_bundle:'bitvmBundle',bitvm_cache:'bitvmCache',bitvm_risk:'bitvmRisk'};
const hex=s=>Buffer.from(s,'utf8').toString('hex')||'-';
function load(stem,{env={},database,files={},now=()=>0}={}){
 const idx=process.argv.indexOf('--source-root');const filename=idx<0?path.join(root,'fixtures','legacy_'+stem+'_source.txt'):path.join(process.argv[idx+1],'src',sourceNames[stem]+'.js');
 const code=readPinnedSource(filename,hashes[stem]);const module={exports:{}};
 const context=vm.createContext({module,exports:module.exports,process:{env},Date:{now},Buffer,console:{log(){},error(){}},require(key){
  if(key==='crypto')return crypto;
  if(key==='path')return {resolve:p=>path.posix.resolve('/oracle',p)};
  if(key==='fs')return {readFileSync(p){if(!Object.hasOwn(files,p))throw Error('missing virtual artifact');return files[p]}};
  if(key==='./db')return database;
  throw Error('unapproved import '+key);
 }});
 vm.runInContext(code,context,{timeout:1000,filename});return {exports:module.exports,context};
}
function dbAdapter(){const rows=new Map(),calls=[];let writes=0,fail=0;const base={async findOneAsync(q){calls.push('get:'+q._id);return rows.has(q._id)?structuredClone(rows.get(q._id)):null},async updateAsync(q,u){calls.push('write:'+q._id);if(++writes===fail)throw Error('injected write failure');assert.deepEqual(Object.keys(u),['$set']);rows.set(q._id,{...(rows.get(q._id)||{}),...structuredClone(u.$set)})}};return {rows,calls,database:{async getDatabase(category){assert.equal(category,'procedural');calls.push('base:'+category);return base}},failNext(n){fail=writes+n},snapshot(){return [...rows.entries()].sort(([a],[b])=>a<b?-1:a>b?1:0).map(([,v])=>v)}}}
function save(stem,lines){const output='# Full unchanged pinned source; genuine Node crypto; explicit FS/DB/environment/time adapters.\n# op name inputJSONhex contextJSONhex millis resultJSONhex rowsJSONhex callsJSONhex; trace rows characterize source storage failure, not atomic Rust policy.\n'+lines.join('\n')+'\n';const file=path.join(root,'fixtures',stem+'_vectors.tsv');if(process.argv.includes('--write'))fs.writeFileSync(file,output);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),output);console.log('PASS '+stem+': '+lines.length+' full-source observations');}
async function run(){const lines=[];const module=load('bitvm_bundle').exports;
 for(const [name,value]of Object.entries({empty:{},ordered:{b:2,a:1},reverse:{a:1,b:2},hash_omitted:{bundleHash:'ignored',a:1},nested:{bundleHash:'ignore',nested:{bundleHash:'keep'}},integer_keys:{'10':'ten','2':'two',z:0},numbers:[-0,1e-7,1e21,1e-6,1.2345678901234567],strings:'ABC',bmp:'\u2603',falsy:false,truthy:7,null:null})){
  lines.push(['hash',name,hex(JSON.stringify(value)),'-',0,hex(JSON.stringify(module.computeBundleHash(value))),'-'].join('\t'));
 }
 const cases=[['normal',{a:1},'computed'],['uppercase',{a:1},'upper'],['wrong-expected',{a:1},'wrong'],['missing-expected',{a:1},'missing'],['missing-declared',{a:1},'undeclared'],['bad-self-hash',{a:1},'bad'],['selected-string',{selectedBucketPct:'2.5',sourceHashes:{x:'abc'}},'computed'],['selected-nan',{selectedBucketPct:'bad',sourceHashes:'opaque'},'computed'],['source-falsy',{sourceHashes:0,selectedBucketPct:false},'computed'],['environment-path',{a:1},'env']];
 for(const [name,doc,mode]of cases){const computed=module.computeBundleHash(doc);if(mode!=='undeclared')doc.bundleHash=mode==='bad'?'0'.repeat(64):computed;const expected=mode==='missing'?'':mode==='wrong'?'f'.repeat(64):mode==='upper'?' '+computed.toUpperCase()+' ':computed;const input={expected,path:mode==='env'?'':'artifact.json',label:mode==='env'?'/oracle/fallback.json':'/oracle/artifact.json',raw:JSON.stringify(doc)};const bound=load('bitvm_bundle',{env:mode==='env'?{TL_BITVM_BUNDLE_PATH:'fallback.json'}:{},files:{[input.label]:input.raw}}).exports;const result=await bound.verifyBundleHash(expected,input.path);lines.push(['verify',name,hex(JSON.stringify(input)),'-',0,hex(JSON.stringify(result)),'-'].join('\t'));}
 const fail=await load('bitvm_bundle').exports.verifyBundleHash('a','missing.json');assert.equal(fail.valid,false);assert.match(fail.reason,/Bundle load\/verify failed/);lines.push(['trace','missing-file',hex('{}'),'-',0,hex(JSON.stringify(fail)),'-'].join('\t'));save('bitvm_bundle',lines);
}
module.exports={load,dbAdapter,save,hex,root};if(require.main===module)run().catch(e=>{console.error(e);process.exitCode=1});
