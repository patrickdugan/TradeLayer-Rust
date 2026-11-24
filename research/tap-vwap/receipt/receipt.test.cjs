"use strict";
const assert=require("node:assert/strict");
const {test}=require("node:test");
const {createHash,createECDH}=require("node:crypto");
const {encode,digest,nullifier,verifySchnorr,verifyTier1}=require("./receipt.cjs");
const N=0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const b32=n=>Buffer.from(n.toString(16).padStart(64,"0"),"hex");
const bytes=(n,l=32)=>Buffer.alloc(l,n).toString("hex");
const sha=b=>createHash("sha256").update(b).digest();
const tagged=(tag,data)=>{const t=sha(Buffer.from(tag));return sha(Buffer.concat([t,t,data]));};
function point(n) {const e=createECDH("secp256k1"); e.setPrivateKey(b32(n));return e.getPublicKey(null,"compressed");}
const pub=n=>point(n).subarray(1).toString("hex");
// Disposable fixture scalars only. Independent OpenSSL point multiplication.
// Deliberately deterministic test nonce, NEVER use this signer with real keys.
function signFixture(d,msg,k) {
  const p=point(d); if(p[0]===3)d=N-d;
  const r=point(k);if(r[0]===3)k=N-k;
  const rx=r.subarray(1), px=p.subarray(1);
  const e=BigInt("0x"+tagged("BIP0340/challenge",Buffer.concat([rx,px,msg])).toString("hex"))%N;
  return Buffer.concat([rx,b32((k+e*d)%N)]).toString("hex");
}
const r={
  genesis:bytes(1),policy:bytes(2),asset:bytes(3),
  edgeKey:pub(3n),btcPeerKey:pub(5n),assetPeerKey:pub(7n),
  rfqId:bytes(4),quoteTranscript:bytes(5),paymentHash:bytes(6),
  assetPre:bytes(7),assetPost:bytes(8),btcPre:bytes(9),btcPost:bytes(10),
  direction:0,assetEdgeSide:0,btcEdgeSide:1,
  assetOutpoint:bytes(11,36),assetOfferer:1,assetHtlc:"13",
  btcOutpoint:bytes(12,36),btcOfferer:1,btcHtlc:"17",
  assetAtoms:"60000000000",grossBtcMsat:"100000000000"
};
const expected=Object.fromEntries(["genesis","policy","asset","edgeKey","btcPeerKey","assetPeerKey","assetOutpoint","btcOutpoint","assetEdgeSide","btcEdgeSide"].map(k=>[k,r[k]]));
const msg=digest(r);
const signatures={edge:signFixture(3n,msg,31n),btcPeer:signFixture(5n,msg,37n),assetPeer:signFixture(7n,msg,41n)};
test("BIP340 vector 0 and signature mutation",()=>{
  const p=Buffer.from("f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9","hex");
  const s=Buffer.from("e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821526f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0","hex");
  assert(verifySchnorr(p,Buffer.alloc(32),s));s[0]^=1;assert(!verifySchnorr(p,Buffer.alloc(32),s));
});
test("canonical layout and tier-1 assertion only",()=>{
  assert.equal(encode(r).length,526);
  const accepted=verifyTier1(r,signatures,expected);
  assert.equal(accepted.tier,"tier1_attested_only");assert.equal(accepted.eligibleForIndex,false);
  assert.equal(accepted.digest,msg.toString("hex"));
});
test("amount mutation invalidates signatures",()=>assert.throws(()=>verifyTier1({...r,grossBtcMsat:"100000000001"},signatures,expected),/signature/));
test("omitted signature rejects",()=>assert.throws(()=>verifyTier1(r,{edge:signatures.edge},expected),/signature/));
test("role swap rejects",()=>assert.throws(()=>verifyTier1(r,{...signatures,btcPeer:signatures.assetPeer},expected),/signature/));
test("wrong chain policy or peer bindings reject",()=>{
  for(const k of ["genesis","policy","asset","btcPeerKey","assetOutpoint","assetEdgeSide"])
    assert.throws(()=>verifyTier1(r,signatures,{...expected,[k]:null}),/context/);
});
test("nullifier invariant to price, quote and policy; sensitive to asset HTLC",()=>{
  assert.deepEqual(nullifier(r),nullifier({...r,grossBtcMsat:"1",rfqId:bytes(15),policy:bytes(16)}));
  assert.notDeepEqual(nullifier(r),nullifier({...r,assetHtlc:"14"}));
});
test("opposite conversion direction requires opposite offerer on both legs",()=>{
  assert.throws(()=>encode({...r,direction:1}),/offerer/);
  assert.equal(encode({...r,direction:1,assetOfferer:0,btcOfferer:0}).length,526);
});
test("reject ambiguous encodings and zero/overflow amounts",()=>{
  for(const val of ["0","01","-1","18446744073709551616",1])
    assert.throws(()=>encode({...r,assetAtoms:val}),/invalid/);
  assert.throws(()=>encode({...r,genesis:"AB".repeat(32)}),/invalid/);
  assert.throws(()=>encode({...r,btcPost:r.btcPre}),/unchanged/);
  assert.throws(()=>encode({...r,btcOutpoint:r.assetOutpoint}),/distinct channels/);
});
test("changing channel state commitment invalidates signature",()=>assert.throws(()=>verifyTier1({...r,assetPost:bytes(20)},signatures,expected),/signature/));
