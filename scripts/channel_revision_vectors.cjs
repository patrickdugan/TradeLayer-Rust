#!/usr/bin/env node
'use strict';
// NEW opt-in policy vectors, not historical JS consensus vectors. Signing below
// exists only in this test oracle; no production Rust private-key/signing API.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {createRequire}=require('node:module');
const root=path.resolve(__dirname,'..'),file=path.join(root,'fixtures/channel_revision_vectors.tsv');
const argv=process.argv.slice(2),at=argv.indexOf('--tiny-root');
const N=0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const P=0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const G={x:0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n,y:0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8n};
const h=b=>crypto.createHash('sha256').update(b).digest(),hh=b=>h(h(b));
const hex=b=>Buffer.from(b).toString('hex'),from=s=>Buffer.from(s,'hex'),big=b=>BigInt('0x'+hex(b));
const int=(n,k)=>{n=BigInt(n);assert(n>=0n&&n<(1n<<BigInt(k*8)));return from(n.toString(16).padStart(k*2,'0'));};
const u8=n=>int(n,1),u16=n=>int(n,2),u32=n=>int(n,4),u64=n=>int(n,8),u128=n=>int(n,16);
const domain=s=>Buffer.from('TradeLayer/'+s+'/v1/');
const names={policy:domain('SignedChannelPolicy'),revision:domain('SignedChannelRevision'),allocation:domain('ChannelAllocation'),bootstrapAllocation:domain('ChannelBootstrapAllocation'),bootstrapLedger:domain('ChannelBootstrapLedger'),bootstrapHead:domain('ChannelRevisionBootstrap')};
const genesis={1:'000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f',2:'12a765e31ffd4059bada1e25190f6e98c99d9714d334efa41a195a7e7e04bfe2',3:'1a91e3dace36e2be3bf030a65679fe821aa1d6ef92e7c9902eb318182c355691'};
const ec=d=>{let c=crypto.createECDH('secp256k1');c.setPrivateKey(int(d,32));return c;};
const keys=[1n,2n].map(d=>({secret:int(d,32),public:ec(d).getPublicKey(null,'compressed'),uncompressed:ec(d).getPublicKey(null,'uncompressed')}));
let tiny=null;
if(at>=0){
 const req=createRequire(path.join(path.resolve(argv[at+1]),'package.json')),entry=req.resolve('tiny-secp256k1'),dir=path.resolve(path.dirname(entry),'..','..');
 assert.equal(JSON.parse(fs.readFileSync(path.join(dir,'package.json'),'utf8')).version,'2.2.4');
 for(const[name,sha]of Object.entries({'lib/cjs/index.cjs':'3864bb04fb2a7a663038301f312010f462142c2555763600ae8eb3a7ecda8b01','lib/cjs/validate.cjs':'f0638d147bac3b60764b2b68f79d72cdccad52523fcd0a98599b32e0873f58e2','lib/secp256k1.wasm':'5cbbcb8adc7acc3b78a3fd31c76d573302705ff5fd714d03f5a2602591197cb5'}))assert.equal(hex(h(fs.readFileSync(path.join(dir,name)))),sha,name);
 tiny=req('tiny-secp256k1');
 for(const key of keys)assert.deepEqual(Buffer.from(tiny.pointFromScalar(key.secret,true)),key.public);
}
function account(s){const b=Buffer.from(s);assert(b.length>=1&&b.length<=90&&[...b].every(c=>c>=0x21&&c<=0x7e));return Buffer.concat([u8(b.length),b]);}
function allocationBytes(rows){return Buffer.concat([u16(rows.length),...rows.map(r=>Buffer.concat([u32(r.property),u128(r.atoms_a),u128(r.atoms_b)]))]);}
function bootstrapAllocation(rows){return hh(Buffer.concat([names.bootstrapAllocation,allocationBytes(rows)]));}
function bootstrapLedger(bytes){return hh(Buffer.concat([names.bootstrapLedger,bytes]));}
function policyBytes(p){return Buffer.concat([names.policy,u8(1),u8(p.network),from(genesis[p.network]),account(p.channel),account(p.participant_a),account(p.participant_b),from(p.public_key_a),from(p.public_key_b),u64(p.funding_epoch),u16(p.funding_outpoints.length),...p.funding_outpoints.map(o=>Buffer.concat([from(o.txid_display),u32(o.vout)])),from(p.script_pubkey_sha256),u64(p.bootstrap.height),from(p.bootstrap.id),from(p.bootstrap_ledger_commitment),from(p.bootstrap_allocation_commitment),u8(0)]);}
function policyId(p){return hh(policyBytes(p));}
function allocationDigest(id,rows){return hh(Buffer.concat([names.allocation,id,allocationBytes(rows)]));}
function initialHead(p,rows){const id=policyId(p),ad=allocationDigest(id,rows);assert.deepEqual(bootstrapAllocation(rows),from(p.bootstrap_allocation_commitment));return{sequence:'0',revision_id:hex(hh(Buffer.concat([names.bootstrapHead,id,ad]))),allocation_digest:hex(ad)};}
function revisionBytes(b){return Buffer.concat([names.revision,u8(0),from(b.policy_id),u64(b.sequence),from(b.previous_revision_id),from(b.before_allocation_digest),u64(b.parent.height),from(b.parent.id),u64(b.valid_from_height),u64(b.valid_until_height),allocationBytes(b.allocation)]);}
function fixturePolicy(network,rows,extra={}){return{network,channel:'channel',participant_a:'alice',participant_b:'bob',public_key_a:hex(keys[0].public),public_key_b:hex(keys[1].public),funding_epoch:'1',funding_outpoints:[{txid_display:hex(h('funding:'+network)),vout:1}],script_pubkey_sha256:hex(h('script:'+network)),bootstrap:{height:'0',id:genesis[network]},bootstrap_ledger_commitment:hex(bootstrapLedger(Buffer.from('fixture-bootstrap:'+network))),bootstrap_allocation_commitment:hex(bootstrapAllocation(rows)),...extra};}
function bodyFor(p,head,allocation,height=1,parentId=genesis[p.network]){return{policy_id:hex(policyId(p)),sequence:(BigInt(head.sequence)+1n).toString(),previous_revision_id:head.revision_id,before_allocation_digest:head.allocation_digest,parent:{height:String(height-1),id:parentId},valid_from_height:String(height),valid_until_height:String(height+10),allocation};}
function testOnlySignRequest(request){
 assert(tiny,'signing fixture requires --tiny-root with genuine pinned tiny2.2.4');
 const p=request.policy,b=request.revision;
 assert.equal(p.public_key_a,hex(keys[0].public));assert.equal(p.public_key_b,hex(keys[1].public));assert.equal(b.policy_id,hex(policyId(p)));
 const preimage=revisionBytes(b),digest=hh(preimage),a=Buffer.from(tiny.sign(digest,keys[0].secret)),c=Buffer.from(tiny.sign(digest,keys[1].secret));
 return{policy_hex:hex(policyBytes(p)),preimage_hex:hex(preimage),revision_id:hex(digest),signature_a:hex(a),signature_b:hex(c),signed_hex:hex(Buffer.concat([preimage,a,c]))};
}
// Independent affine BigInt ECDSA. Distinct formulas/reduction/representation
// from Rust's fixed-limb Montgomery/Jacobian implementation and tiny's WASM.
const mod=(a,m=P)=>(a%m+m)%m;
function inverse(a,m=P){let[x,y,s,t]=[mod(a,m),m,1n,0n];while(y){let q=x/y;[x,y]=[y,x-q*y];[s,t]=[t,s-q*t];}assert.equal(x,1n);return mod(s,m);}
function add(a,b){if(!a)return b;if(!b)return a;if(a.x===b.x&&a.y!==b.y)return null;if(a.x===b.x&&a.y===0n)return null;const slope=a.x===b.x?mod(3n*a.x*a.x*inverse(2n*a.y)):mod((b.y-a.y)*inverse(b.x-a.x)),x=mod(slope*slope-a.x-b.x);return{x,y:mod(slope*(a.x-x)-a.y)};}
function multiply(q,n){let out=null;while(n){if(n&1n)out=add(out,q);q=add(q,q);n>>=1n;}return out;}
function independentVerify(digest,key,sig){const r=big(sig.subarray(0,32)),s=big(sig.subarray(32));if(r===0n||s===0n||r>=N||s>=N)return'signature';if(s>N/2n)return false;const raw=crypto.ECDH.convertKey(key,'secp256k1',undefined,undefined,'uncompressed'),q={x:big(raw.subarray(1,33)),y:big(raw.subarray(33))},inv=inverse(s,N),point=add(multiply(G,big(digest)*inv%N),multiply(q,r*inv%N));return point!==null&&point.x%N===r;}
function opensslVerify(preimage,key,sig){const point=crypto.ECDH.convertKey(key,'secp256k1',undefined,undefined,'uncompressed'),publicKey=crypto.createPublicKey({key:Buffer.concat([from('3056301006072a8648ce3d020106052b8104000a034200'),point]),format:'der',type:'spki'});return crypto.verify('sha256',h(preimage),{key:publicKey,dsaEncoding:'ieee-p1363'},sig);}
function cases(){
 const out=[];
 for(const network of[1,2,3]){
  const initial=[{property:7,atoms_a:'100000000',atoms_b:'200000000'},{property:9,atoms_a:((1n<<100n)+3n).toString(),atoms_b:'7'}],after=[{property:7,atoms_a:'125000000',atoms_b:'175000000'},{property:9,atoms_a:((1n<<100n)+9n).toString(),atoms_b:'1'}];
  const policy=fixturePolicy(network,initial),head=initialHead(policy,initial),first=bodyFor(policy,head,after);
  out.push({name:'network-'+network,policy,before:initial,head,revision:first,context:{network,parent:first.parent,height:'1'}});
  const head1={sequence:'1',revision_id:hex(hh(revisionBytes(first))),allocation_digest:hex(allocationDigest(policyId(policy),after))},second=bodyFor(policy,head1,initial,2,hex(h('block1:'+network)));
  out.push({name:'second-'+network,policy,before:after,head:head1,revision:second,context:{network,parent:second.parent,height:'2'}});
 }
 for(let i=0;i<12;i++){
  let before=[],after=[];for(let j=1;j<=1+i%8;j++){let total=big(h('pool:'+i+':'+j))%(1n<<100n)+1n,a=big(h('a:'+i+':'+j))%(total+1n),b=big(h('b:'+i+':'+j))%(total+1n);before.push({property:j,atoms_a:a.toString(),atoms_b:(total-a).toString()});after.push({property:j,atoms_a:b.toString(),atoms_b:(total-b).toString()});}
  const policy=fixturePolicy(1+i%3,before,{funding_epoch:String(i+2),funding_outpoints:[{txid_display:'11'.repeat(32),vout:0},{txid_display:'11'.repeat(32),vout:1},{txid_display:'ff'.repeat(32),vout:0xffffffff}]}),head=initialHead(policy,before),revision=bodyFor(policy,head,after);
  out.push({name:'seeded-'+i,policy,before,head,revision,context:{network:policy.network,parent:revision.parent,height:'1'}});
 }
 const max=(1n<<128n)-1n,before=Array.from({length:256},(_,i)=>({property:i+1,atoms_a:(max-1n).toString(),atoms_b:'1'})),after=before.map(r=>({...r,atoms_a:'0',atoms_b:max.toString()}));
 const policy=fixturePolicy(2,before,{channel:'c'.repeat(90),participant_a:'a'.repeat(90),participant_b:'b'.repeat(90),funding_epoch:'18446744073709551615',funding_outpoints:Array.from({length:64},(_,i)=>({txid_display:'11'.repeat(32),vout:i}))}),head=initialHead(policy,before),revision=bodyFor(policy,head,after);
 out.push({name:'maximum-fields',policy,before,head,revision,context:{network:2,parent:revision.parent,height:'1'}});
 return out;
}
function generate(existing,writing){
 const rows=[],old=new Map(existing.filter(l=>l.startsWith('V\t')).map(l=>{let f=l.split('\t');return[f[1],f];}));
 const valid=cases();
 for(const c of valid){
  const preimage=revisionBytes(c.revision),digest=hh(preimage),prior=old.get(c.name);
  let signatureA,signatureB;
  if(writing){const signed=testOnlySignRequest(c);signatureA=from(signed.signature_a);signatureB=from(signed.signature_b);}
  else{assert(prior,'missing '+c.name);const oldSigned=from(prior[13]);assert.equal(oldSigned.length,preimage.length+128);signatureA=oldSigned.subarray(-128,-64);signatureB=oldSigned.subarray(-64);}
  for(const[key,sig]of[[keys[0].public,signatureA],[keys[1].public,signatureB]]){assert.equal(independentVerify(digest,key,sig),true,c.name);assert.equal(opensslVerify(preimage,key,sig),true,c.name);if(tiny)assert.equal(tiny.verify(digest,key,sig,true),true,c.name);}
  rows.push(['V',c.name,hex(policyBytes(c.policy)),hex(policyId(c.policy)),hex(allocationBytes(c.before)),c.head.sequence,c.head.revision_id,c.head.allocation_digest,c.context.network,c.context.parent.height,c.context.parent.id,c.context.height,hex(preimage),hex(Buffer.concat([preimage,signatureA,signatureB])),hex(digest),hex(allocationDigest(policyId(c.policy),c.revision.allocation)),hex(bootstrapAllocation(c.before))].join('\t'));
 }
 const base=rows[0].split('\t'),p=valid[0].policy,pre=from(base[12]),signed=from(base[13]),a=signed.subarray(-128,-64),b=signed.subarray(-64);
 const s=(name,x,expected)=>rows.push(['S',name,base[2],hex(x),expected].join('\t'));
 s('correct',signed,'true');s('swapped',Buffer.concat([pre,b,a]),'false');
 for(const[index,sig]of[[0,a],[1,b]]){let high=Buffer.concat([sig.subarray(0,32),int(N-big(sig.subarray(32)),32)]);s('high-s-'+index,Buffer.concat([pre,index? a:high,index?high:b]),'false');for(const[n,r,t]of[['zero-r',0n,1n],['zero-s',1n,0n],['r-order',N,1n],['s-order',1n,N]]){let broken=Buffer.concat([int(r,32),int(t,32)]);s(n+'-'+index,Buffer.concat([pre,index?a:broken,index?broken:b]),'InvalidSignature');}}
 for(let i=0;i<16;i++){let changed=Buffer.from(signed);changed[pre.length+(i%2)*64+i]^=1;s('signature-bit-'+i,changed,'false');}
 let changed=Buffer.from(signed);changed[pre.length-1]^=1;s('allocation-bit',changed,'false');
 changed=Buffer.from(signed);changed[names.revision.length+1]^=1;s('policy-bit',changed,'WrongPolicy');
 const x=(name,kind,bytes,error)=>rows.push(['X',name,kind,hex(bytes)||'-',error].join('\t'));
 for(const n of[0,1,names.revision.length,pre.length-1,signed.length-1])x('signed-cut-'+n,'signed',signed.subarray(0,n),'Truncated');
 x('signed-trailing','signed',Buffer.concat([signed,u8(0)]),'NonCanonical');x('signed-cap','signed',Buffer.alloc(16385),'Capacity');
 changed=Buffer.from(signed);changed[names.revision.length]=1;x('unknown-purpose','signed',changed,'InvalidInput');
 const bad={...valid[0].revision,sequence:'0'};x('zero-sequence','body',revisionBytes(bad),'InvalidInput');
 x('reversed-window','body',revisionBytes({...bad,sequence:'1',valid_from_height:'8',valid_until_height:'7'}),'InvalidInput');
 x('unsafe-height','body',revisionBytes({...bad,sequence:'1',valid_until_height:'9007199254740992'}),'InvalidInput');
 const pb=policyBytes(p);for(const n of[0,1,names.policy.length,pb.length-1])x('policy-cut-'+n,'policy',pb.subarray(0,n),'Truncated');
 x('policy-trailing','policy',Buffer.concat([pb,u8(0)]),'NonCanonical');x('policy-cap','policy',Buffer.alloc(4097),'Capacity');
 for(const[name,policy,error]of[['same-key',{...p,public_key_b:p.public_key_a},'DuplicateKey'],['same-party',{...p,participant_b:p.participant_a},'InvalidPolicy'],['epoch-zero',{...p,funding_epoch:'0'},'InvalidPolicy'],['unsorted-funding',{...p,funding_outpoints:[{txid_display:'ff'.repeat(32),vout:1},{txid_display:'11'.repeat(32),vout:1}]},'NonCanonical'],['duplicate-funding',{...p,funding_outpoints:[p.funding_outpoints[0],p.funding_outpoints[0]]},'NonCanonical'],['bad-key',{...p,public_key_a:'02'+'ff'.repeat(32)},'InvalidKey']])x(name,'policy',policyBytes(policy),error);
 changed=Buffer.from(pb);changed[names.policy.length]=2;x('policy-version','policy',changed,'InvalidPolicy');changed=Buffer.from(pb);changed[changed.length-1]=1;x('binding-policy','policy',changed,'InvalidPolicy');changed=Buffer.from(pb);changed[names.policy.length+2]^=1;x('wrong-genesis','policy',changed,'InvalidPolicy');
 for(const[name,allocation,error]of[['zero-rows',[],'Capacity'],['zero-total',[{property:1,atoms_a:'0',atoms_b:'0'}],'InvalidAllocation'],['overflow-total',[{property:1,atoms_a:((1n<<128n)-1n).toString(),atoms_b:'1'}],'Arithmetic'],['zero-property',[{property:0,atoms_a:'1',atoms_b:'0'}],'InvalidAllocation'],['duplicate-property',[{property:1,atoms_a:'1',atoms_b:'0'},{property:1,atoms_a:'1',atoms_b:'0'}],'NonCanonical'],['unsorted-property',[{property:2,atoms_a:'1',atoms_b:'0'},{property:1,atoms_a:'1',atoms_b:'0'}],'NonCanonical']])x(name,'allocation',allocationBytes(allocation),error);
 for(const row of rows.filter(r=>r.startsWith('S\t'))){const f=row.split('\t'),envelope=from(f[3]),pbytes=envelope.subarray(0,-128),digest=hh(pbytes),expectedPolicy=hh(from(f[2]));let actual;
  if(!pbytes.subarray(names.revision.length+1,names.revision.length+33).equals(expectedPolicy))actual='WrongPolicy';
  else{let first=independentVerify(digest,keys[0].public,envelope.subarray(-128,-64));if(first==='signature')actual='InvalidSignature';else if(!first)actual='false';else{let second=independentVerify(digest,keys[1].public,envelope.subarray(-64));actual=second==='signature'?'InvalidSignature':String(second);}}
  assert.equal(actual,f[4],f[1]);
 }
 return rows;
}
function main(){
 const si=argv.indexOf('--sign-request');if(si>=0){console.log(JSON.stringify(testOnlySignRequest(JSON.parse(fs.readFileSync(argv[si+1],'utf8')))));return;}
 const existing=fs.existsSync(file)?fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n').trimEnd().split('\n'):[];
 const writing=argv.includes('--write');if(writing)assert(tiny,'--write requires --tiny-root');
 const rows=generate(existing,writing),text='# SupersedingCoSignedAllocationV1 NEW policy; genuine tiny2.2.4 deterministic signatures, OpenSSL and independent affine BigInt verification.\n# Generator Node24.19.0/OpenSSL3.5.7; runtime patches are reported, not acceptance gates.\n# V name policyHex policyId beforeHex previousSeq previousId previousAllocationDigest network parentHeight parentId height preimageHex signedHex newId newAllocationDigest bootstrapBeforeDigest\n# S name policyHex signedHex signatureResult; X name parser inputHex expectedError\n'+rows.join('\n')+'\n';
 if(writing)fs.writeFileSync(file,text);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),text);
 const counts={};for(const r of rows)counts[r[0]]=(counts[r[0]]||0)+1;
 console.log('PASS channel revision '+JSON.stringify(counts)+' Node'+process.versions.node+' OpenSSL'+process.versions.openssl+(tiny?' genuine pinned tiny2.2.4':''));
}
if(require.main===module)main();
module.exports={allocationBytes,bootstrapAllocation,bootstrapLedger,policyBytes,policyId,initialHead,allocationDigest,revisionBytes,bodyFor,fixturePolicy,testOnlySignRequest,keys:keys.map(k=>({public:hex(k.public)}))};
