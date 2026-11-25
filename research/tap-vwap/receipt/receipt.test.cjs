"use strict";
const assert=require("node:assert/strict");
const {test}=require("node:test");
const fs=require("node:fs");
const path=require("node:path");
const {createHash,createECDH}=require("node:crypto");
const {encode,digest,nullifier,verifySchnorr,verifyTier1}=require("./receipt.cjs");

const P=0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const N=0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const b32=n=>Buffer.from(n.toString(16).padStart(64,"0"),"hex");
const bytes=(n,l=32)=>Buffer.alloc(l,n).toString("hex");
const sha=b=>createHash("sha256").update(b).digest();
const tagged=(tag,data)=>{const t=sha(Buffer.from(tag,"ascii"));return sha(Buffer.concat([t,t,data]));};
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

const golden=JSON.parse(fs.readFileSync(path.join(__dirname,"golden-fixtures.json"),"utf8"));

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
  const s=Buffer.from("e907831f80848d1069a5371b402410364bdf1c5f8307b0084c55f1ce2dca821525f66a4a85ea8b71e482a74f382d2ce5ebeee8fdb2172f477df4900d310536c0","hex");
  assert(verifySchnorr(p,Buffer.alloc(32),s));
  const mutated=Buffer.from(s); mutated[0]^=1;
  assert(!verifySchnorr(p,Buffer.alloc(32),mutated));
});

test("official BIP340 32-byte test vector suite (indices 0..14)",()=>{
  assert.equal(golden.bip340_official_32b_vectors.length,15);
  for(const v of golden.bip340_official_32b_vectors) {
    const got=verifySchnorr(
      Buffer.from(v.pubkey,"hex"),
      Buffer.from(v.message,"hex"),
      Buffer.from(v.signature,"hex")
    );
    assert.equal(got,v.valid,`BIP340 vector ${v.index}: ${v.comment}`);
  }
});

test("BIP340 boundary negatives (bad point, r>=p, s>=n, infinity, odd-y R, malformed lengths)",()=>{
  const validPub=Buffer.from(pub(3n),"hex");
  const validMsg=Buffer.alloc(32,0x42);
  const validSig=Buffer.from(signFixture(3n,validMsg,31n),"hex");
  assert(verifySchnorr(validPub,validMsg,validSig));

  // pub >= p and quadratic non-residue x=5 (5^3 + 7 = 132, non-square mod p)
  assert(!verifySchnorr(b32(P),validMsg,validSig));
  assert(!verifySchnorr(b32(P+1n),validMsg,validSig));
  assert(!verifySchnorr(Buffer.alloc(32,0xff),validMsg,validSig));
  assert(!verifySchnorr(b32(5n),validMsg,validSig));

  // r >= p
  assert(!verifySchnorr(validPub,validMsg,Buffer.concat([b32(P),validSig.subarray(32)])));
  assert(!verifySchnorr(validPub,validMsg,Buffer.concat([b32(P+1n),validSig.subarray(32)])));
  assert(!verifySchnorr(validPub,validMsg,Buffer.concat([Buffer.alloc(32,0xff),validSig.subarray(32)])));

  // s >= n
  assert(!verifySchnorr(validPub,validMsg,Buffer.concat([validSig.subarray(0,32),b32(N)])));
  assert(!verifySchnorr(validPub,validMsg,Buffer.concat([validSig.subarray(0,32),b32(N+1n)])));
  assert(!verifySchnorr(validPub,validMsg,Buffer.concat([validSig.subarray(0,32),Buffer.alloc(32,0xff)])));

  // Constructed point-at-infinity R: s*G - e*P = O when s = e*d mod n
  let d=3n; if(point(d)[0]===3) d=N-d;
  for(const rx of [Buffer.alloc(32,0),b32(1n),validPub]) {
    const e=BigInt("0x"+tagged("BIP0340/challenge",Buffer.concat([rx,validPub,validMsg])).toString("hex"))%N;
    const infSig=Buffer.concat([rx,b32((e*d)%N)]);
    assert(!verifySchnorr(validPub,validMsg,infSig));
  }

  // Constructed odd-y R: find k such that k*G has odd y (prefix 0x03), sign without negating k
  let oddK=1n;
  while(point(oddK)[0]!==3) oddK++;
  const oddRx=point(oddK).subarray(1);
  const oddE=BigInt("0x"+tagged("BIP0340/challenge",Buffer.concat([oddRx,validPub,validMsg])).toString("hex"))%N;
  const oddSig=Buffer.concat([oddRx,b32((oddK+oddE*d)%N)]);
  assert(!verifySchnorr(validPub,validMsg,oddSig));

  // Buffer length boundaries
  assert(!verifySchnorr(validPub.subarray(0,31),validMsg,validSig));
  assert(!verifySchnorr(validPub,validMsg.subarray(0,31),validSig));
  assert(!verifySchnorr(validPub,Buffer.concat([validMsg,Buffer.from([0])]),validSig));
  assert(!verifySchnorr(validPub,validMsg,validSig.subarray(0,63)));
});

test("independent golden encoder fixtures freeze wire bytes, digest and nullifier",()=>{
  assert.equal(golden.wire_version,1);
  assert.equal(golden.wire_byte_length,526);
  assert.equal(golden.receipt_fixtures.length,2);
  for(const fx of golden.receipt_fixtures) {
    const raw=encode(fx.receipt);
    assert.equal(raw.length,526);
    assert.equal(raw.toString("hex"),fx.encoded_hex);
    assert.equal(sha(raw).toString("hex"),fx.encoded_sha256);
    assert.equal(digest(fx.receipt).toString("hex"),fx.digest_hex);
    assert.equal(nullifier(fx.receipt).toString("hex"),fx.nullifier_hex);
    const exp=Object.fromEntries(["genesis","policy","asset","edgeKey","btcPeerKey","assetPeerKey","assetOutpoint","btcOutpoint","assetEdgeSide","btcEdgeSide"].map(k=>[k,fx.receipt[k]]));
    const res=verifyTier1(fx.receipt,fx.signatures,exp);
    assert.equal(res.tier,"tier1_attested_only");
    assert.equal(res.eligibleForIndex,false);
    assert.equal(res.digest,fx.digest_hex);
    assert.equal(res.nullifier,fx.nullifier_hex);
  }
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
  for(const k of ["genesis","policy","asset","btcPeerKey","assetOutpoint","assetEdgeSide","btcEdgeSide"])
    assert.throws(()=>verifyTier1(r,signatures,{...expected,[k]:null}),/context/);
});

test("nullifier invariant to price, quote, policy and receipt-key rotation; sensitive to asset HTLC and offerer",()=>{
  assert.deepEqual(nullifier(r),nullifier({...r,grossBtcMsat:"1",rfqId:bytes(15),policy:bytes(16)}));
  assert.deepEqual(
    nullifier(r),
    nullifier({...r,edgeKey:pub(43n),btcPeerKey:pub(47n),assetPeerKey:pub(53n)})
  );
  assert.notDeepEqual(nullifier(r),nullifier({...r,assetHtlc:"14"}));
  assert.notDeepEqual(
    nullifier(r),
    nullifier({...r,assetEdgeSide:1,assetOfferer:0})
  );
});

test("opposite conversion direction requires opposite offerer on both legs",()=>{
  assert.throws(()=>encode({...r,direction:1}),/offerer/);
  assert.equal(encode({...r,direction:1,assetOfferer:0,btcOfferer:0}).length,526);
});

test("reject ambiguous encodings, zero/overflow amounts, and unresolved funding/splice identities",()=>{
  for(const val of ["0","01","-1","18446744073709551616",1])
    assert.throws(()=>encode({...r,assetAtoms:val}),/invalid/);
  assert.throws(()=>encode({...r,genesis:"AB".repeat(32)}),/invalid/);
  assert.throws(()=>encode({...r,btcPost:r.btcPre}),/unchanged/);
  assert.throws(()=>encode({...r,btcOutpoint:r.assetOutpoint}),/distinct channels/);
  assert.throws(()=>encode({...r,assetOutpoint:"00".repeat(32)+"00000001"}),/unresolved/);
  assert.throws(()=>encode({...r,btcOutpoint:bytes(12,32)+"ffffffff"}),/unresolved/);
  assert.throws(()=>encode({...r,splicePending:true}),/unresolved/);
  assert.throws(()=>encode({...r,spliceUnresolved:true}),/unresolved/);
  assert.throws(()=>encode({...r,fundingResolved:false}),/unresolved/);
});

test("changing channel state commitment invalidates signature",()=>assert.throws(()=>verifyTier1({...r,assetPost:bytes(20)},signatures,expected),/signature/));
