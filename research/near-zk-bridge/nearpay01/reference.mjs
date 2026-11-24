// NEARPAY01-A conditional native reference; no custody authorization.
function reference(hash){
 const req=(v,m)=>{if(!v)throw Error(m)};
 const cat=(...a)=>{const r=new Uint8Array(a.reduce((s,v)=>s+v.length,0));let i=0;for(const v of a){r.set(v,i);i+=v.length}return r};
 const ascii=s=>{req(typeof s==="string"&&/^[\x20-\x7e]*$/.test(s),"ASCII");return Uint8Array.from([...s].map(x=>x.charCodeAt(0)))};
 const hex=b=>Array.from(b,x=>x.toString(16).padStart(2,"0")).join("");
 const raw=s=>{req(typeof s==="string"&&/^[0-9a-f]{64}$/.test(s),"hash");return Uint8Array.from(s.match(/../g).map(x=>parseInt(x,16)))};
 const uint=(n,k)=>{req(typeof n==="bigint"&&n>=0n&&n<1n<<BigInt(k*8),"range");const r=new Uint8Array(k);for(let i=0;i<k;i++){r[i]=Number(n&255n);n>>=8n}return r};
 const str=s=>{const b=ascii(s);return cat(uint(BigInt(b.length),4),b)};
 const H=(...a)=>hash(cat(...a));
 const tagged=(tag,...a)=>H(ascii(tag),Uint8Array.of(0),...a);
 const account=s=>{req(typeof s==="string"&&s.length>=2&&s.length<=64&&/^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(s),"account");return s};
 const amount=s=>{req(typeof s==="string"&&/^(0|[1-9][0-9]{0,19})$/.test(s),"amount string");const n=BigInt(s);uint(n,8);return n};
 const u128text=s=>{req(/^"(0|[1-9][0-9]{0,19})"$/.test(s),"return ABI");return amount(s.slice(1,-1))};
 const b58=s=>{const A="123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";let n=0n;for(const c of s){const i=A.indexOf(c);req(i>=0,"base58");n=n*58n+BigInt(i)}const a=[];while(n){a.unshift(Number(n&255n));n>>=8n}for(const c of s){if(c!=="1")break;a.unshift(0)}req(a.length===32,"base58 size");return Uint8Array.from(a)};
 const equal=(a,b)=>hex(a)===hex(b);
 // Narrow success-only NEAR partial execution outcome profile. Logs are hashed
 // separately, metadata deliberately absent. ASCII restriction is a refusal.
 function outcomeHash(o){
   req(o.children.length<=32&&new Set(o.children).size===o.children.length&&o.logs.length<=32,"outcome bounds");
   const status=o.status.kind==="value"?cat(Uint8Array.of(2),str(o.status.value)):
     o.status.kind==="receipt"?cat(Uint8Array.of(3),raw(o.status.id)):null;
   req(status!==null,"unsupported status");
   const partial=H(uint(BigInt(o.children.length),4),...o.children.map(raw),uint(o.gas,8),uint(o.burnt,16),str(account(o.executor)),status);
   const items=[raw(o.id),partial,...o.logs.map(l=>{req(l.length<=4096,"log size");return H(ascii(l))})];
   return H(uint(BigInt(items.length),4),...items);
 }
 function path(root,p){
   req(Array.isArray(p)&&p.length<=64,"path bound");
   for(const [d,s] of p){req(d===0||d===1,"direction");root=d===0?H(raw(s),root):H(root,raw(s))}
   return root;
 }
 function blockHash(b){
   const l=b.lite;
   const inner=H(uint(l.height,8),raw(l.epoch),raw(l.nextEpoch),raw(l.prevState),raw(l.outcomeRoot),uint(l.timestamp,8),raw(l.nextBp),raw(l.blockMerkleRoot));
   return H(H(inner,raw(b.innerRestHash)),raw(b.prevBlockHash));
 }
 function proveOutcome(proof,headRoot){
   // HEAD ROOT is a pre-authenticated input to this subrelation, never supplied
   // authority. This function deliberately does not verify consensus finality.
   const shard=path(outcomeHash(proof.outcome),proof.outcomePath);
   const block=path(H(shard),proof.shardPath);
   req(equal(block,raw(proof.header.lite.outcomeRoot)),"block outcome root");
   const bh=blockHash(proof.header);
   req(equal(bh,raw(proof.blockHash)),"block hash");
   req(equal(path(bh,proof.blockPath),raw(headRoot)),"head ancestry");
   return proof.outcome;
 }
 const depId=(d,r,i)=>hex(tagged("TLBRIDGE/DEPOSIT-ID/V1",raw(d),raw(r),uint(i,4)));
 const candidateId=(d,n)=>hex(tagged("TLBRIDGE/CANDIDATE-ID/V1",raw(d),uint(n,8)));
 // Graph semantics are explicitly CONDITIONAL on independently authenticated
 // action receipt bodies/data edges and exact deployed token/vault code.
 // These are NOT authenticated by proveOutcome. No method authorizes funds.
 function inspectGraph(p,g,policy){
   const [t,v,r]=p;
   req(new Set([t.id,v.id,r.id]).size===3,"distinct ids");
   req(t.executor===policy.token&&v.executor===policy.vault&&r.executor===policy.token,"executor");
   req(t.status.kind==="receipt"&&t.status.id===r.id&&t.children.includes(v.id)&&t.children.includes(r.id),"promise links");
   req(v.status.kind==="value"&&r.status.kind==="value","terminal status");
   req(g.transfer.id===t.id&&g.receive.id===v.id&&g.resolve.id===r.id,"action identity");
   const a=g.transfer,b=g.receive,c=g.resolve;
   req(a.method==="ft_transfer_call"&&b.method==="ft_on_transfer"&&c.method==="ft_resolve_transfer","method");
   req(a.predecessor===a.sender&&a.receiver===policy.token&&b.predecessor===policy.token&&b.receiver===policy.vault&&c.predecessor===policy.token&&c.receiver===policy.token,"predecessor");
   req(a.vault===policy.vault&&c.vault===policy.vault&&a.sender===b.sender&&b.sender===c.sender,"account binding");
   account(a.sender);req(a.amount===b.amount&&b.amount===c.amount&&a.script===b.script,"argument binding");raw(a.script);
   req(a.outputData===undefined,"unexpected transfer data edge");
   req(b.outputData.length===1&&c.inputData.length===1&&b.outputData[0].id===c.inputData[0]&&b.outputData[0].receiver===policy.token,"data dependency");
   raw(c.inputData[0]);
   const requested=amount(a.amount),unused=u128text(v.status.value),used=u128text(r.status.value);
   req(requested>0n&&unused<=requested&&used===requested-unused,"terminal amount mismatch");
   // Exact candidate event: the pinned upgraded vault emits canonical ASCII.
   // Existing ZTB-02B candidate logs only sequence, so need a new log or state proof.
   const seq=amount(b.sequence);
   req(v.logs.length===1&&v.logs[0]===`ZTB_DEPOSIT_V1:${policy.domain}:${b.sequence}:${a.sender}:${a.script}:${a.amount}:${unused}`,"candidate binding");
   const ev=(from,to,n,memo)=>"EVENT_JSON:"+JSON.stringify({standard:"nep141",version:"1.0.0",event:"ft_transfer",data:[{old_owner_id:from,new_owner_id:to,amount:String(n),...(memo?{memo}:{})}]});
   req(t.logs.length===1&&t.logs[0]===ev(a.sender,policy.vault,requested,null),"initial transfer log");
   // Strict audited log profile: all unexpected logs refused, including burn-on-refund.
   req(unused===0n?r.logs.length===0:r.logs.length===1&&r.logs[0]===ev(policy.vault,a.sender,unused,"refund"),"refund/burn discrepancy");
   return {depositId:depId(policy.domain,r.id,0n),candidateNullifier:candidateId(policy.domain,seq),amount:used.toString(),recipient:a.script,sender:a.sender,creditable:used>0n,action_authentication_verified:false,near_finality_verified:false,can_mint:false};
 }
 return {hex,raw,ascii,b58,H,outcomeHash,path,blockHash,proveOutcome,inspectGraph};
}
export { reference };
