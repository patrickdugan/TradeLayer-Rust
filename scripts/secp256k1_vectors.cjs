#!/usr/bin/env node
'use strict';
// Independent affine BigInt math (not Rust's Montgomery/Jacobian algorithm),
// plus genuine tiny-secp256k1 and Node/OpenSSL. Signing is test-oracle only.
const assert=require('node:assert/strict'),crypto=require('node:crypto'),fs=require('node:fs'),path=require('node:path');
const {createRequire}=require('node:module');
const root=path.resolve(__dirname,'..'),file=path.join(root,'fixtures','secp256k1_vectors.tsv');
const P=0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const N=0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const G={x:0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n,y:0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8n};
const hx=(n,bytes=32)=>n.toString(16).padStart(bytes*2,'0'),buf=n=>Buffer.from(hx(n),'hex'),big=b=>BigInt('0x'+Buffer.from(b).toString('hex'));
const hash=s=>crypto.createHash('sha256').update(s).digest();
const mod=(a,m=P)=>(a%m+m)%m;
function inv(a,m=P){let [old,r]=[mod(a,m),m],[s,t]=[1n,0n];while(r){const q=old/r;[old,r]=[r,old-q*r];[s,t]=[t,s-q*t];}assert.equal(old,1n);return mod(s,m);}
function pow(a,n,m=P){let out=1n;while(n){if(n&1n)out=out*a%m;a=a*a%m;n>>=1n;}return out;}
function add(a,b){if(!a)return b;if(!b)return a;if(a.x===b.x&&a.y!==b.y)return null;if(a.x===b.x&&a.y===0n)return null;const slope=a.x===b.x?mod(3n*a.x*a.x*inv(2n*a.y)):mod((b.y-a.y)*inv(b.x-a.x));const x=mod(slope*slope-a.x-b.x);return {x,y:mod(slope*(a.x-x)-a.y)};}
function mul(q,n){let out=null;while(n){if(n&1n)out=add(out,q);q=add(q,q);n>>=1n;}return out;}
function encode(q,compressed=true){return compressed?Buffer.concat([Buffer.from([Number(2n+(q.y&1n))]),buf(q.x)]):Buffer.concat([Buffer.from([4]),buf(q.x),buf(q.y)]);}
function parse(key,hybrid=false){
 const b=Buffer.from(key);if(!((b.length===33&&(b[0]===2||b[0]===3))||(b.length===65&&(b[0]===4||hybrid&&(b[0]===6||b[0]===7)))))return null;
 const x=big(b.subarray(1,33));if(x>=P)return null;const rhs=mod(x*x*x+7n);let y;
 if(b.length===33){y=pow(rhs,(P+1n)/4n);if(y*y%P!==rhs)return null;if((y&1n)!==BigInt(b[0]&1))y=mod(-y);if((y&1n)!==BigInt(b[0]&1))return null;}
 else{y=big(b.subarray(33));if(y>=P||y*y%P!==rhs)return null;if(b[0]!==4&&(y&1n)!==BigInt(b[0]&1))return null;}
 return {x,y};
}
const verified=new Map();
function verify(digest,key,signature){
 const q=parse(key);if(!q)return 'key';const r=big(signature.subarray(0,32)),s=big(signature.subarray(32));
 if(r===0n||s===0n||r>=N||s>=N)return 'signature';
 const sn=s>N/2n?N-s:s,cache=Buffer.from(digest).toString('hex')+encode(q).toString('hex')+hx(r)+hx(sn);
 if(verified.has(cache))return verified.get(cache);
 const inverse=inv(sn,N),a=mod(big(digest)*inverse,N),b=mod(r*inverse,N);const point=add(mul(G,a),mul(q,b));const valid=point!==null&&point.x%N===r;verified.set(cache,valid);return valid;
}
let tiny=null;
const ti=process.argv.indexOf('--tiny-root');
if(ti>=0){
 const req=createRequire(path.join(path.resolve(process.argv[ti+1]),'package.json')),entry=req.resolve('tiny-secp256k1'),dir=path.resolve(path.dirname(entry),'..','..');
 assert.equal(JSON.parse(fs.readFileSync(path.join(dir,'package.json'),'utf8')).version,'2.2.4');
 for(const [name,sha]of Object.entries({'lib/cjs/index.cjs':'3864bb04fb2a7a663038301f312010f462142c2555763600ae8eb3a7ecda8b01','lib/cjs/validate.cjs':'f0638d147bac3b60764b2b68f79d72cdccad52523fcd0a98599b32e0873f58e2','lib/secp256k1.wasm':'5cbbcb8adc7acc3b78a3fd31c76d573302705ff5fd714d03f5a2602591197cb5'}))assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(dir,name))).digest('hex'),sha,name);
 tiny=req('tiny-secp256k1');
}
function observed(digest,key,sig,strict){try{return String(tiny.verify(digest,key,sig,strict));}catch(error){return 'throw:'+error.message;}}
function nativeResult(result,strict,sig){if(typeof result==='string')return result;if(strict&&big(sig.subarray(32))>N/2n)return 'false';return String(result);}
function opensslPublic(d){const ec=crypto.createECDH('secp256k1');ec.setPrivateKey(buf(d));return ec.getPublicKey(null,'compressed');}
function privateKey(d){return crypto.createPrivateKey({key:Buffer.concat([Buffer.from('302e0201010420','hex'),buf(d),Buffer.from('a00706052b8104000a','hex')]),format:'der',type:'sec1'});}
function publicKey(q){return crypto.createPublicKey({key:Buffer.concat([Buffer.from('3056301006072a8648ce3d020106052b8104000a034200','hex'),encode(q,false)]),format:'der',type:'spki'});}
function arithmeticRows(){
 const out=[];for(const [name,m]of [['p',P],['n',N]]){
  const edges=[0n,1n,2n,3n,m-1n,m-2n,m/2n,m/2n+1n,(1n<<64n)-1n,1n<<64n,(1n<<128n)-1n,1n<<128n,(1n<<192n)-1n,1n<<192n,mod((1n<<256n)-1n,m),0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaan%m,0x5555555555555555555555555555555555555555555555555555555555555555n%m];
  const pairs=[];for(const a of edges)for(const b of edges)pairs.push([a,b]);
  for(let i=0;i<256;i++)pairs.push([big(hash('arithmetic:'+name+':a:'+i))%m,big(hash('arithmetic:'+name+':b:'+i))%m]);
  for(const [i,[a,b]]of pairs.entries())out.push(['M',name+'-'+i,name,hx(a),hx(b),hx((a+b)%m),hx(mod(a-b,m)),hx(a*b%m),a===0n?'-':hx(inv(a,m))].join('\t'));
 }return out;
}
function generation(existing){
 assert(tiny,'--write requires --tiny-root pointing at genuine pinned tiny-secp256k1');const rows=arithmeticRows();
 const point=(name,key,domain='canonical')=>{const q=parse(key);let actual;try{actual=Buffer.from(tiny.pointCompress(key,true)).toString('hex');}catch(error){actual='throw:'+error.message;}rows.push(['P',name,key.toString('hex')||'-',q?encode(q).toString('hex'):'key',actual,domain].join('\t'));};
 const signature=(name,digest,key,sig)=>{const answer=verify(digest,key,sig);rows.push(['V',name,digest.toString('hex'),key.toString('hex')||'-',sig.toString('hex'),nativeResult(answer,false,sig),nativeResult(answer,true,sig),observed(digest,key,sig,false),observed(digest,key,sig,true)].join('\t'));};
 for(let i=0;i<64;i++){
  const d=i===0?1n:i===1?N-1n:big(hash('public-key:'+i))%(N-1n)+1n;
  const q=Buffer.from(tiny.pointFromScalar(buf(d),true));assert.deepEqual(q,opensslPublic(d));if(i<8)assert.deepEqual(q,encode(mul(G,d)));
  point('point-'+i,q);point('uncompressed-'+i,Buffer.from(tiny.pointFromScalar(buf(d),false)));
  if(i<32){
   const digest=i===0?Buffer.alloc(32):i===1?buf(N):i===2?Buffer.alloc(32,255):hash('digest:'+i),sig=Buffer.from(tiny.sign(digest,buf(d))),high=Buffer.concat([sig.subarray(0,32),buf(N-big(sig.subarray(32)))]);
   signature('tiny-low-'+i,digest,q,sig);signature('tiny-high-'+i,digest,q,high);
   signature('tiny-uncompressed-'+i,digest,Buffer.from(tiny.pointFromScalar(buf(d),false)),sig);
   const other=Buffer.from(digest);other[i%32]^=1;signature('message-mutation-'+i,other,q,sig);
   const otherR=Buffer.from(sig);otherR[i%32]^=1;signature('r-mutation-'+i,digest,q,otherR);
   const otherS=Buffer.from(sig);otherS[32+i%32]^=1;signature('s-mutation-'+i,digest,q,otherS);
   const otherQ=Buffer.from(q);otherQ[0]^=1;signature('key-negation-'+i,digest,otherQ,sig);
  }
 }
 let t=1n;while(N+t<P&&!parse(Buffer.concat([Buffer.from([2]),buf(N+t)])))t++;
 const rare=Buffer.concat([Buffer.from([2]),buf(N+t)]),rareQ=Buffer.from(tiny.pointMultiply(rare,buf(inv(t,N)),true));
 signature('affine-x-greater-than-order',Buffer.alloc(32),rareQ,Buffer.concat([buf(t),buf(1n)]));
 point('rare-high-x',rare);
 signature('infinity-sum-g',buf(N-1n),encode(G),Buffer.concat([buf(1n),buf(1n)]));
 signature('infinity-sum-negative-g',buf(1n),encode({x:G.x,y:P-G.y}),Buffer.concat([buf(1n),buf(1n)]));
 const oneSig=Buffer.from(tiny.sign(buf(1n),buf(1n)));
 const malformed=[['empty',Buffer.alloc(0)],['infinity',Buffer.from([0])],['x-only',buf(G.x)],['short',encode(G).subarray(0,32)],['long',Buffer.concat([encode(G),Buffer.from([0])])],['prefix0',Buffer.concat([Buffer.from([0]),buf(G.x)])],['prefix5',Buffer.concat([Buffer.from([5]),buf(G.x)])],['x-p',Buffer.concat([Buffer.from([2]),buf(P)])],['x-max',Buffer.concat([Buffer.from([3]),Buffer.alloc(32,255)])],['x-zero',Buffer.concat([Buffer.from([2]),Buffer.alloc(32)])],['y-p',Buffer.concat([Buffer.from([4]),buf(G.x),buf(P)])],['off-curve',Buffer.concat([Buffer.from([4]),buf(G.x),buf(G.y+1n)])],['hybrid-even',Buffer.concat([Buffer.from([6]),buf(G.x),buf(G.y)])],['hybrid-wrong-parity',Buffer.concat([Buffer.from([7]),buf(G.x),buf(G.y)])]];
 for(const [name,key]of malformed){point('bad-'+name,key,name.startsWith('hybrid')?'hybrid-outside-native':'canonical');signature('bad-key-'+name,buf(1n),key,oneSig);}
 for(const [name,r,s]of [['zero-r',0n,1n],['zero-s',1n,0n],['zero-both',0n,0n],['r-order',N,1n],['s-order',1n,N],['r-max',(1n<<256n)-1n,1n],['s-max',1n,(1n<<256n)-1n],['one-one',1n,1n],['largest',N-1n,N-1n],['half-s',1n,N/2n],['over-half-s',1n,N/2n+1n]])signature(name,buf(1n),encode(G),Buffer.concat([buf(r),buf(s)]));
 const recorded=new Map(existing.filter(line=>line.startsWith('O\t')).map(line=>[line.split('\t')[1],line]));
 for(let i=0;i<12;i++){
  const name='openssl-'+i,d=big(hash('openssl-key:'+i))%(N-1n)+1n,message=hash('openssl-message:'+i),digest=hash(message),q=parse(opensslPublic(d));
  const old=recorded.get(name);let sig;
  if(old){const f=old.split('\t');assert.equal(f[2],message.toString('hex'));assert.equal(f[3],digest.toString('hex'));assert.equal(f[4],encode(q).toString('hex'));sig=Buffer.from(f[5],'hex');}
  else sig=crypto.sign('sha256',message,{key:privateKey(d),dsaEncoding:'ieee-p1363'});
  assert.equal(crypto.verify('sha256',message,{key:publicKey(q),dsaEncoding:'ieee-p1363'},sig),true);
  assert.equal(verify(digest,encode(q),sig),true);assert.equal(tiny.verify(digest,encode(q),sig),true);
  rows.push(['O',name,message.toString('hex'),digest.toString('hex'),encode(q).toString('hex'),sig.toString('hex'),'true',String(tiny.verify(digest,encode(q),sig,true))].join('\t'));
 }return rows;
}
function validate(lines){
 let counts={M:0,P:0,V:0,O:0};
 for(const line of lines){if(!line||line.startsWith('#'))continue;const f=line.split('\t');assert(Object.hasOwn(counts,f[0]));counts[f[0]]++;
  if(f[0]==='M')continue;
  if(f[0]==='P'){const key=f[2]==='-'?Buffer.alloc(0):Buffer.from(f[2],'hex'),q=parse(key);assert.equal(q?encode(q).toString('hex'):'key',f[3],f[1]);if(tiny){let actual;try{actual=Buffer.from(tiny.pointCompress(key,true)).toString('hex');}catch(error){actual='throw:'+error.message;}assert.equal(actual,f[4],f[1]);}continue;}
  if(f[0]==='V'){
   const digest=Buffer.from(f[2],'hex'),key=f[3]==='-'?Buffer.alloc(0):Buffer.from(f[3],'hex'),sig=Buffer.from(f[4],'hex'),answer=verify(digest,key,sig);
   assert.equal(nativeResult(answer,false,sig),f[5],f[1]);assert.equal(nativeResult(answer,true,sig),f[6],f[1]);
   if(tiny){assert.equal(observed(digest,key,sig,false),f[7],f[1]);assert.equal(observed(digest,key,sig,true),f[8],f[1]);}continue;
  }
  const message=Buffer.from(f[2],'hex'),digest=Buffer.from(f[3],'hex'),key=Buffer.from(f[4],'hex'),sig=Buffer.from(f[5],'hex'),q=parse(key);
  assert.deepEqual(hash(message),digest);assert(q);assert.equal(crypto.verify('sha256',message,{key:publicKey(q),dsaEncoding:'ieee-p1363'},sig),true,f[1]);
  const result=verify(digest,key,sig);assert.equal(nativeResult(result,false,sig),f[6]);assert.equal(nativeResult(result,true,sig),f[7]);
  if(tiny){assert.equal(String(tiny.verify(digest,key,sig)),f[6]);assert.equal(String(tiny.verify(digest,key,sig,true)),f[7]);}
 }
 const expected=arithmeticRows(),actual=lines.filter(line=>line.startsWith('M\t'));assert.deepEqual(actual,expected);
 assert(counts.P>=128&&counts.V>=200&&counts.O===12);return counts;
}
const existing=fs.existsSync(file)?fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n').split('\n'):[];
if(process.argv.includes('--write')){
 const rows=generation(existing);const header='# Public-only verification vectors: independent affine BigInt math + genuine tiny-secp256k1 2.2.4 + Node/OpenSSL.\n# Captured generator Node24.19.0 OpenSSL3.5.7; runtime patches recorded, not cryptographic acceptance gates.\n# M name modulus a b add sub mul inverse; P name key nativeCanonical tinyCanonical domain; V name digest key compactSig nativeDefault nativeLowS tinyDefault tinyStrict; O name message sha256 key compactSig default lowS\n';fs.writeFileSync(file,header+rows.join('\n')+'\n');
}
const lines=fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n').split('\n');const counts=validate(lines);
console.log('PASS secp256k1 '+JSON.stringify(counts)+' Node'+process.versions.node+' OpenSSL'+process.versions.openssl+(tiny?' genuine tiny2.2.4 pinned JS/WASM':''));
