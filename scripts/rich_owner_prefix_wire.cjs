const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const base=path.resolve(__dirname,'..');
const sourceAt=process.argv.indexOf('--source-fixture');
if(sourceAt>=0)assert.ok(process.argv[sourceAt+1],'--source-fixture requires a path');
const source=sourceAt>=0?path.resolve(process.argv[sourceAt+1]):path.join(base,'fixtures','rich_owner_prefix_vectors.jsonl');
const realm=v=>({HostIO:0,SourceVM:1,Shared:2})[v];
function encodeGraph(g){const out=[Buffer.from([84,76,82,86,1])],caps={intrinsics:new Map(),callables:new Set()};
 const b=v=>out.push(Buffer.from([v])),u32=v=>{const x=Buffer.alloc(4);x.writeUInt32BE(v);out.push(x);},u64=v=>out.push(Buffer.from(v,'hex')),units=v=>{u32(v.length);for(const c of v){const x=Buffer.alloc(2);x.writeUInt16BE(c);out.push(x);}},text=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
 function atom(a){switch(a.tag){case'Undefined':b(0);break;case'Null':b(1);break;case'Boolean':b(2);b(+a.value);break;case'Binary64':b(3);u64(a.bits);break;case'UTF16':b(4);units(a.units);break;case'BigInt':b(5);units(text(a.decimal));break;case'SymbolRef':b(6);u32(a.id);break;case'HeapRef':b(7);u32(a.id);break;case'Intrinsic':b(8);b(realm(a.realm));units(text(a.name));caps.intrinsics.set(a.realm+'/'+a.name,[realm(a.realm),a.name]);break;default:throw Error('source atom '+a.tag);}}
 const roots=g.roots||[g.root];u32(roots.length);for(const root of roots)atom(root);u32(g.symbols.length);for(const s of g.symbols){if(s.globalKey!==null){b(2);units(text(s.globalKey));}else if(s.description===null)b(0);else{b(1);units(s.description);}}
 u32(g.objects.length);for(const o of g.objects){assert.equal(o.id,g.objects.indexOf(o));switch(o.kind){case'Ordinary':b(0);break;case'Array':b(1);break;case'NativeError':{b(2);assert.equal(o.prototype.tag,'Intrinsic');const name=o.prototype.name.split('.')[0],kinds=['Error','TypeError','RangeError','SyntaxError','ReferenceError','URIError','EvalError','AggregateError'];assert.ok(kinds.includes(name));b(kinds.indexOf(name));b(realm(o.prototype.realm));break;}case'Callable':{b(3);const binding='ObservedOnly/'+o.id+'/'+o.sourceSha256;units(text(binding));caps.callables.add(binding);break;}default:throw Error(o.kind);}atom(o.prototype);u32(o.properties.length);for(const p of o.properties){if(p.key.tag==='UTF16'){b(0);units(p.key.units);}else{assert.equal(p.key.tag,'SymbolRef');b(1);u32(p.key.id);}b(+p.enumerable+(+p.configurable<<1));if(p.kind==='Data'){b(0);b(+p.writable);atom(p.value);}else{b(1);atom(p.get);atom(p.set);}}}
 return{bytes:Buffer.concat(out),caps};}
function untag(v){switch(v[0]){case'undefined':return undefined;case'null':return null;case'boolean':case'string':return v[1];case'number':{assert.equal(typeof v[1],'string',JSON.stringify(v));if(v[1]==='NaN')return NaN;if(v[1]==='Infinity')return Infinity;if(v[1]==='-Infinity')return -Infinity;if(v[1]==='-0')return -0;assert.equal(v[1].length,16,JSON.stringify(v));return Buffer.from(v[1],'hex').readDoubleBE();}case'array':return v[1].map(untag);case'object':return Object.fromEntries(v[1].map(([k,v])=>[k,untag(v)]));default:throw Error('unexpected diagnostic tag '+v[0]);}}

module.exports={encodeGraph,untag};
if(require.main===module){
 const records=fs.readFileSync(source,'utf8').trim().split('\n').map(JSON.parse),rows=[];
 for(let i=0;i<records.length;i+=2){const a=records[i],b=records[i+1];rows.push([b.name,encodeGraph(a.rawFactGraph).bytes.toString('hex'),encodeGraph(b.finalGraph).bytes.toString('hex'),b.result.transport].join('\t'));}
 const output=rows.join('\n')+'\n',file=path.join(base,'fixtures','rich_owner_prefix.tsv');if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),output);else fs.writeFileSync(file,output);
 console.log(JSON.stringify({originalMainCalls:rows.length,wireGraphs:rows.length*2}));
}
