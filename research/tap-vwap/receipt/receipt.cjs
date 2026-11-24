"use strict";
// Research-only tier-1 assertion receipt. No channel-state validation, quote
// validation, proof admission, signing, secret keys or consensus integration.
const { createHash } = require("node:crypto");
const P = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const G = [
  0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n,
  0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8n,
];
const mod = (x,m=P) => (x%m+m)%m;
function pow(x,y,m=P) {
  let z=1n; x=mod(x,m);
  while(y) { if(y&1n) z=z*x%m; x=x*x%m; y>>=1n; }
  return z;
}
function add(a,b) {
  if(!a) return b; if(!b) return a;
  if(a[0]===b[0] && mod(a[1]+b[1])===0n) return null;
  const s=a[0]===b[0]
    ? mod(3n*a[0]*a[0]*pow(2n*a[1],P-2n))
    : mod((b[1]-a[1])*pow(b[0]-a[0],P-2n));
  const x=mod(s*s-a[0]-b[0]); return [x,mod(s*(a[0]-x)-a[1])];
}
function mul(k,a=G) {
  let r=null; while(k) { if(k&1n) r=add(r,a); a=add(a,a); k>>=1n; } return r;
}
function lift(x) {
  if(x>=P) return null;
  const c=mod(x*x*x+7n), y=pow(c,(P+1n)/4n);
  return mod(y*y)===c ? [x,y&1n?P-y:y] : null;
}
const hash = b => createHash("sha256").update(b).digest();
function tagged(tag,data) {
  const t=hash(Buffer.from(tag,"ascii")); return hash(Buffer.concat([t,t,data]));
}
const int = b => BigInt("0x"+b.toString("hex"));
function hex(s,len,name) {
  if(typeof s!=="string" || s.length!==len*2 || !/^[0-9a-f]+$/.test(s))
    throw Error("invalid "+name);
  return Buffer.from(s,"hex");
}
function u64(s,name,positive=false) {
  if(typeof s!=="string" || !/^(0|[1-9][0-9]{0,19})$/.test(s))
    throw Error("invalid "+name);
  const n=BigInt(s);
  if(n>0xffffffffffffffffn || (positive && n===0n)) throw Error("invalid "+name);
  const b=Buffer.alloc(8); b.writeBigUInt64BE(n); return b;
}
function outpoint(s,name) { return hex(s,36,name); }
function bit(n,name) {
  if(n!==0 && n!==1) throw Error("invalid "+name);
  return Buffer.from([n]);
}
// BIP340 public verification reference, variable time, research only.
// Production adapter must use an independently reviewed secp256k1 library
// and the circuit must match native acceptance. Never use for secret operations.
function verifySchnorr(pub,msg,sig) {
  if(!Buffer.isBuffer(pub)||pub.length!==32||!Buffer.isBuffer(msg)||msg.length!==32||
     !Buffer.isBuffer(sig)||sig.length!==64) return false;
  const a=lift(int(pub)), r=int(sig.subarray(0,32)), s=int(sig.subarray(32));
  if(!a||r>=P||s>=N) return false;
  const e=int(tagged("BIP0340/challenge",Buffer.concat([sig.subarray(0,32),pub,msg])))%N;
  const R=add(mul(s),mul(mod(-e,N),a));
  return R!==null && (R[1]&1n)===0n && R[0]===r;
}
// Direction: 0 = edge purchases asset (asset in, BTC out).
//            1 = edge sells asset (BTC in, asset out).
// Outpoint bytes: displayed txid in lexical byte order, then vout u32 BE.
// Offerer bit: immutable funding-peer ordering from the independently
// authenticated channel registry, NOT ordering of rotating receipt keys.
// assetEdgeSide and btcEdgeSide are registry-bound edge positions (0/1).
const fields32=["genesis","policy","asset","edgeKey","btcPeerKey","assetPeerKey",
  "rfqId","quoteTranscript","paymentHash","assetPre","assetPost","btcPre","btcPost"];
function encode(r) {
  const a=fields32.map(k=>hex(r[k],32,k));
  const keys=[r.edgeKey,r.btcPeerKey,r.assetPeerKey];
  if(new Set(keys).size!==3) throw Error("distinct roles required");
  if(keys.some(k=>!lift(int(hex(k,32,"key"))))) throw Error("invalid curve key");
  if(r.assetOutpoint===r.btcOutpoint) throw Error("distinct channels required");
  if(r.assetPre===r.assetPost||r.btcPre===r.btcPost) throw Error("unchanged state commitment");
  const d=bit(r.direction,"direction");
  const aes=bit(r.assetEdgeSide,"asset edge side");
  const bes=bit(r.btcEdgeSide,"btc edge side");
  const wantAsset=r.direction===0?1-r.assetEdgeSide:r.assetEdgeSide;
  const wantBtc=r.direction===0?r.btcEdgeSide:1-r.btcEdgeSide;
  if(r.assetOfferer!==wantAsset||r.btcOfferer!==wantBtc) throw Error("offerer/direction");
  return Buffer.concat([Buffer.from([1]),...a,d,aes,bes,
    outpoint(r.assetOutpoint,"asset outpoint"),bit(r.assetOfferer,"asset offerer"),
    u64(r.assetHtlc,"asset htlc"),outpoint(r.btcOutpoint,"btc outpoint"),
    bit(r.btcOfferer,"btc offerer"),u64(r.btcHtlc,"btc htlc"),
    u64(r.assetAtoms,"asset atoms",true),u64(r.grossBtcMsat,"gross btc msat",true)]);
}
function digest(r) { return tagged("TradeLayer/TAP-VWAP/Receipt/v1",encode(r)); }
function nullifier(r) {
  encode(r);
  // No policy, reporter, epoch, RFQ, amount or signature salt: replay identity
  // survives index windows. Funding peer orientation must remain
  // immutable across receipt-key rotation; unresolved splices blocked.
  return tagged("TradeLayer/TAP-VWAP/AssetHTLC/v1",Buffer.concat([
    hex(r.genesis,32,"genesis"),outpoint(r.assetOutpoint,"asset outpoint"),
    bit(r.assetOfferer,"asset offerer"),u64(r.assetHtlc,"asset htlc")]));
}
const bindings=["genesis","policy","asset","edgeKey","btcPeerKey","assetPeerKey",
  "assetOutpoint","btcOutpoint","assetEdgeSide","btcEdgeSide"];
function verifyTier1(r,signatures,expected) {
  // Expected bindings must come from an independently trusted registry adapter.
  // This function does not establish registry membership, funding, channel
  // states, timestamps, HTLC linkage, fee attribution, or quote authenticity.
  if(!expected||bindings.some(k=>!Object.hasOwn(expected,k)||r[k]!==expected[k]))
    throw Error("trusted context mismatch");
  const msg=digest(r);
  for(const [role,k] of [["edge","edgeKey"],["btcPeer","btcPeerKey"],["assetPeer","assetPeerKey"]]) {
    if(!verifySchnorr(hex(r[k],32,k),msg,hex(signatures?.[role],64,role+" signature")))
      throw Error("invalid "+role+" signature");
  }
  return {tier:"tier1_attested_only",digest:msg.toString("hex"),
    nullifier:nullifier(r).toString("hex"),assetAtoms:r.assetAtoms,
    grossBtcMsat:r.grossBtcMsat,eligibleForIndex:false};
}
module.exports={encode,digest,nullifier,verifySchnorr,verifyTier1};
