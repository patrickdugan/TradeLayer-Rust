'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const out=path.join(__dirname,'../fixtures/sort_vectors.tsv');
const lines=[];let serial=0;
function execute(keys,descending,label){
 const trace=[];const rows=keys.map((key,id)=>({key,id}));
 rows.sort((a,b)=>{trace.push(a.id+','+b.id);return descending?b.key-a.key:a.key-b.key;});
 const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
 lines.push([serial++,label,+descending,keys.map(bits).join(','),rows.map(x=>x.id).join(','),trace.join(';')].join('|'));
}
for(let length=0;length<=7;length++)for(let code=0;code<4**length;code++){
 let v=code;const keys=Array.from({length},()=>{const k=v%4;v=Math.floor(v/4);return k===3?NaN:k;});
 execute(keys,false,'exhaustive-'+length+'-'+code);execute(keys,true,'exhaustive-'+length+'-'+code);
}
let seed=0x6754b219;
function next(){seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;return seed>>>0;}
const sizes=[8,31,32,33,63,64,65,66,95,96,127,128,129,191,192,255,256,257,511,512,513,767,768,1023,1024];
for(const length of sizes)for(let mode=0;mode<24;mode++){
 const keys=Array.from({length},(_,i)=>{
  const r=next();
  if(mode===0)return i;
  if(mode===1)return length-i;
  if(mode===2)return Math.floor(i/32)%2?length-i:i;
  if(mode===3)return i%33===0?NaN:i;
  if(mode===4)return i%7===0?NaN:length-i;
  if(mode===5)return [NaN,Infinity,-Infinity,-0,0,1,-1][r%7];
  if(mode===6)return i<length/3?100+i:i<2*length/3?i-length:200+i;
  if(mode===7)return (i%64)<32?1000-i:i;
  return r%5===0?NaN:(r%257)-128;
 });
 execute(keys,false,'boundary-'+length+'-'+mode);execute(keys,true,'boundary-'+length+'-'+mode);
}
const bytes=lines.join('\n')+'\n';
if(process.argv.includes('--print'))process.stdout.write(bytes);else if(process.argv.includes('--check'))assert.equal(fs.readFileSync(out,'utf8'),bytes);else fs.writeFileSync(out,bytes);
(process.argv.includes('--print')?console.error:console.log)(JSON.stringify({node:process.version,v8:process.versions.v8,cases:lines.length,sha256:require('node:crypto').createHash('sha256').update(bytes).digest('hex')}));
