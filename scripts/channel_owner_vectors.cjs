#!/usr/bin/env node
'use strict';
// Test-only genuine signatures for the actual funded owner fixture. No historical
// wire/source parity or L1 authentication is asserted by these NEW policy vectors.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const c=require('./channel_revision_vectors.cjs');
const root=path.resolve(__dirname,'..'),file=path.join(root,'fixtures/channel_owner_vectors.tsv');
const raw=fs.readFileSync(path.join(root,'fixtures/channel_owner_bootstrap.hex'),'utf8').trim();
assert(/^[0-9a-f]+$/.test(raw)&&raw.startsWith('544c4c02'));
const hex=b=>Buffer.from(b).toString('hex'),hh=b=>crypto.createHash('sha256').update(crypto.createHash('sha256').update(b).digest()).digest();
const u64=n=>{let b=Buffer.alloc(8);b.writeBigUInt64BE(BigInt(n));return b;};
const writing=process.argv.includes('--write'),old=writing?new Map():new Map(fs.readFileSync(file,'utf8').trimEnd().split('\n').filter(l=>!l.startsWith('#')).map(l=>{let f=l.split('\t');return[f[0],f];}));
const out=[];const rows=(a,b)=>[{property:7,atoms_a:String(a),atoms_b:String(b)}];
for(const network of[1,2,3]){
 const p=c.fixturePolicy(network,rows(600,400),{bootstrap:{height:'2',id:'22'.repeat(32)},bootstrap_ledger_commitment:hex(c.bootstrapLedger(Buffer.from(raw,'hex')))});
 let head=c.initialHead(p,rows(600,400));
 for(const[name,allocation,height,parentId]of[['first',rows(350,650),4,'33'.repeat(32)],['second',rows(1,999),5,'44'.repeat(32)]]){
  const body=c.bodyFor(p,head,allocation,height,parentId),id=network+'-'+name;
  const signed=writing?c.testOnlySignRequest({policy:p,revision:body}):{policy_hex:hex(c.policyBytes(p)),signed_hex:old.get(id)[2]};
  const bytes=Buffer.from(signed.signed_hex,'hex'),pre=c.revisionBytes(body);assert.deepEqual(bytes.subarray(0,-128),pre);
  // Independently verify both slots with OpenSSL, which hashes SHA256(P) again.
  for(const[pub,sig]of[[p.public_key_a,bytes.subarray(-128,-64)],[p.public_key_b,bytes.subarray(-64)]]){
   const point=crypto.ECDH.convertKey(Buffer.from(pub,'hex'),'secp256k1',undefined,undefined,'uncompressed');const key=crypto.createPublicKey({key:Buffer.concat([Buffer.from('3056301006072a8648ce3d020106052b8104000a034200','hex'),point]),format:'der',type:'spki'});
   assert(crypto.verify('sha256',crypto.createHash('sha256').update(pre).digest(),{key,dsaEncoding:'ieee-p1363'},sig));
  }
  const enroll=hh(Buffer.concat([Buffer.from('TradeLayer/ChannelEnrollment/v1/'),c.policyId(p),u64(2),Buffer.from('22'.repeat(32),'hex'),u64(3)]));
  out.push([id,signed.policy_hex,signed.signed_hex,hex(enroll)].join('\t'));
  head={sequence:body.sequence,revision_id:hex(hh(pre)),allocation_digest:hex(c.allocationDigest(c.policyId(p),allocation))};
 }
 // Valid competing first revision with exactly the same supplied block context.
 for(const[name,parentId]of[['alternate','33'.repeat(32)],['wrong-parent','99'.repeat(32)]]){
 const body=c.bodyFor(p,c.initialHead(p,rows(600,400)),rows(700,300),4,parentId),id=network+'-'+name;
 const signed=writing?c.testOnlySignRequest({policy:p,revision:body}):{policy_hex:hex(c.policyBytes(p)),signed_hex:old.get(id)[2]};
 const bytes=Buffer.from(signed.signed_hex,'hex'),pre=c.revisionBytes(body);assert.deepEqual(bytes.subarray(0,-128),pre);
 for(const[pub,sig]of[[p.public_key_a,bytes.subarray(-128,-64)],[p.public_key_b,bytes.subarray(-64)]]){
 const point=crypto.ECDH.convertKey(Buffer.from(pub,'hex'),'secp256k1',undefined,undefined,'uncompressed');const key=crypto.createPublicKey({key:Buffer.concat([Buffer.from('3056301006072a8648ce3d020106052b8104000a034200','hex'),point]),format:'der',type:'spki'});assert(crypto.verify('sha256',crypto.createHash('sha256').update(pre).digest(),{key,dsaEncoding:'ieee-p1363'},sig));
 }
 out.push([id,signed.policy_hex,signed.signed_hex,out[out.length-1].split('\t')[3]].join('\t'));
 }
}
const text='# NEW fixed-pool policy. id policyHex signedHex deterministicEnrollmentReplayId\n'+out.join('\n')+'\n';
if(writing)fs.writeFileSync(file,text);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),text);
console.log('PASS actual funded owner '+out.length+' signed revisions; actual TLL bootstrap bytes + independent OpenSSL verification');