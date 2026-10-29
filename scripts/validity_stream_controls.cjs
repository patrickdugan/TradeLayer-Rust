'use strict';
const sourceNpmRoot=process.env.TL_JS_MODULE_ROOT||process.env.TRADELAYER_JS_ROOT;if(!sourceNpmRoot)throw Error('Set the explicit genuine npm prefix');process.argv.push('--module-root',sourceNpmRoot);
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),util=require('node:util');
const stage=path.resolve(__dirname,'..'),repo=fs.existsSync(path.join(stage,'fixtures/legacy_txindex_source_txIndex.txt'))?stage:path.resolve(stage,'../../outputs/TradeLayer-Rust');
const {Environment}=require('./txindex_source_vectors.cjs');
const BN=(()=>{if(!process.argv.includes('--lock'))return require(path.join(repo,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();const packed=fs.readFileSync(path.join(repo,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],bytes=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(bytes).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine BigNumber pin');const m={exports:{}};require('node:vm').runInNewContext(bytes.toString(),{module:m,exports:m.exports});return m.exports;})();
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['boolean',v];if(typeof v==='string')return['string',v];if(typeof v==='number'){if(!Number.isFinite(v))return['number',String(v)];const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',b.toString('hex')];}if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
function hash(b){return crypto.createHash('sha256').update(crypto.createHash('sha256').update(b).digest()).digest();}
function digest(rows){let state=hash(Buffer.from('TL-TXINDEX-STREAMED-VALIDITY-1'));for(let i=0;i<rows.length;i++){const b=Buffer.from(JSON.stringify(tag(rows[i]))),n=Buffer.alloc(8),size=Buffer.alloc(4);n.writeBigUInt64BE(BigInt(i));size.writeUInt32BE(b.length);state=hash(Buffer.concat([state,n,size,b]));}return state.toString('hex');}
const row=id=>({_id:id,value:{payload:'201',txId:id,marker:'tl'}});
const large=Array.from({length:1025},(_,i)=>row(`tx-10-${i.toString().padStart(4,'0')}`));large.push(row('tx-2-target'),row('tx-10-target'),{_id:'other',value:1});
const cases=[
 ['first-lexical-suffix',[row('tx-2-target'),row('tx-10-target'),row('txtop-target')],['target',18,false,'source-reason']],
 ['above-aggregate-limit',large,['target',18,true,'source-reason']],
 ['array-id-coercion',[row('tx-1-x,,y')],[['x',null,'y'],4,false,'array-id']],
 ['negative-zero-id',[row('tx-1-0')],[-0,4,false,'zero']],
 ['coercion-error-after-query',[],[{toString:1},18,false,'none']],
 ['invalid-reason-matched',[row('tx-1-target')],['target',18,false,{nested:{$bad:1}}]],
 ['invalid-reason-unmatched',[row('tx-1-other')],['missing',18,false,{nested:{$bad:1}}]],
];
(async()=>{const output=[];for(const[name,seed,args]of cases){const e=new Environment(BN);await e.settle();for(const r of seed)await util.promisify(e.db.insert.bind(e.db))(r);const before=e.db.getAllData();const result=await e.invoke({scope:'static',method:'upsertTxValidityAndReason',args});const query=result.effects.find(e=>e.target==='db.findAsync');assertQuery(query);const after=e.db.getAllData();
 output.push({kind:'case',name,args:tag(args),initialRows:before.length,matchedRows:query.returned.length,queryDigest:digest(query.returned),outcome:result.outcome,finalRows:after.length});for(const r of before)output.push({kind:'input-row',name,row:tag(r)});for(const r of after)output.push({kind:'output-row',name,row:tag(r)});console.log(name,JSON.stringify({matchedRows:query.returned.length,outcome:result.outcome}));}
 const text=output.map(r=>JSON.stringify(r)).join('\n')+'\n',file=path.join(stage,'fixtures/validity_stream_controls.jsonl');if(process.argv.includes('--check'))require('node:assert/strict').equal(fs.readFileSync(file,'utf8'),text);else fs.writeFileSync(file,text);console.log(`PASS ${cases.length} genuine unchanged TxIndex/NeDB controls`);
})().catch(e=>{console.error(e.stack);process.exitCode=1;});
function assertQuery(q){if(!q||!Array.isArray(q.returned))throw Error('actual complete source prefix query required');}
