// Source-pinned synthetic native receipt-list proof reference. Research only.
function makeReceiptVerifier(hash) {
  const need=(x,m)=>{if(!x)throw Error(m)};
  const cat=(...xs)=>{const b=new Uint8Array(xs.reduce((a,x)=>a+x.length,0));let n=0;for(const x of xs){b.set(x,n);n+=x.length}return b};
  const hex=b=>Array.from(b,x=>x.toString(16).padStart(2,"0")).join("");
  const raw=h=>{need(typeof h==="string"&&/^[0-9a-f]{64}$/.test(h),"hash");return Uint8Array.from(h.match(/../g).map(x=>parseInt(x,16)))};
  const ascii=s=>{need(typeof s==="string"&&/^[\x20-\x7e]*$/.test(s),"ASCII");return Uint8Array.from([...s].map(x=>x.charCodeAt(0)))};
  const H=(...xs)=>hash(cat(...xs));
  const num=(x,n)=>{need(typeof x==="bigint"&&x>=0n&&x<1n<<BigInt(n*8),"integer");let b=new Uint8Array(n);for(let i=0;i<n;i++){b[i]=Number(x&255n);x>>=8n}return b};
  const vec=b=>cat(num(BigInt(b.length),4),b);
  const string=s=>vec(ascii(s));
  const seq=xs=>cat(num(BigInt(xs.length),4),...xs);
  const acct=s=>{need(s.length>=2&&s.length<=64&&/^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(s),"account id");return s};
  const MAX_LIST=256,MAX_BYTES=262144,MAX_ARGS=8192;
  class Cursor {
    constructor(bytes){need(bytes instanceof Uint8Array&&bytes.length<=MAX_BYTES,"byte cap");this.b=bytes;this.i=0}
    take(n){need(Number.isSafeInteger(n)&&n>=0&&n<=this.b.length-this.i,"truncated");const r=this.b.slice(this.i,this.i+n);this.i+=n;return r}
    uint(n){const b=this.take(n);let x=0n;for(let i=n-1;i>=0;i--)x=(x<<8n)+BigInt(b[i]);return x}
    u8(){return Number(this.uint(1))}
    count(max){const n=Number(this.uint(4));need(n<=max,"count cap");return n}
    bytes(max){return this.take(this.count(max))}
    str(max){const b=this.bytes(max);need(b.every(x=>x>=32&&x<=126),"non-ASCII");return String.fromCharCode(...b)}
    account(){return acct(this.str(64))}
    digest(){return hex(this.take(32))}
    list(max,read){const n=this.count(max);return Array.from({length:n},()=>read())}
    option(read){const t=this.u8();need(t===0||t===1,"option tag");return t===0?null:read()}
    end(){need(this.i===this.b.length,"trailing bytes")}
  }
  function readKey(c){const t=c.u8();need(t===0||t===1,"key tag");return {tag:t,bytes:hex(c.take(t===0?32:64))}}
  function readReceipt(c){
    const predecessor=c.account(),receiver=c.account(),id=c.digest(),tag=c.u8();
    if(tag===1)return {predecessor,receiver,id,tag,dataId:c.digest(),data:c.option(()=>hex(c.bytes(MAX_ARGS)))};
    need([0,5,7].includes(tag),"unsupported receipt version");
    const signer=c.account();
    const refundTo=tag===0?null:c.option(()=>c.account());
    let key=null,auth=null,version=null;
    if(tag===7){auth=c.u8();need(auth===0||auth===1,"authorization context");if(auth===0)key=readKey(c)}
    else key=readKey(c);
    const gasPrice=c.uint(16);if(tag===7)version=c.uint(4);
    const output=c.list(16,()=>({id:c.digest(),receiver:c.account()}));
    const input=c.list(16,()=>c.digest());
    need(new Set(input).size===input.length&&new Set(output.map(x=>x.id)).size===output.length,"duplicate data id");
    const actions=c.list(16,()=>{
      const t=c.u8();
      if(t===3)return {tag:t,deposit:c.uint(16)};
      need(t===2,"unsupported action");
      return {tag:t,method:c.str(128),args:hex(c.bytes(MAX_ARGS)),gas:c.uint(8),deposit:c.uint(16)};
    });
    return {predecessor,receiver,id,tag,signer,refundTo,key,auth,version,gasPrice,output,input,actions};
  }
  // ReceiptV0 serializes without an outer enum discriminator. A proof opens
  // the whole ordered Borsh vector, not a per-receipt Merkle branch.
  function decodeList(bytes){
    const c=new Cursor(bytes),shard=c.uint(8),receipts=c.list(MAX_LIST,()=>readReceipt(c));c.end();
    need(new Set(receipts.map(x=>x.id)).size===receipts.length,"duplicate receipt id");
    return {shard,receipts};
  }
  function atIndex(leaf,path,index,total){
    need(Number.isSafeInteger(index)&&Number.isSafeInteger(total)&&total>0&&total<=1024&&index>=0&&index<total,"index bounds");
    let r=leaf,used=0;for(let k=0;(1<<k)<total;k++){
      const b=index>>k,sibling=b^1;
      if((sibling<<k)>=total)continue;
      need(used<path.length,"missing sibling");const [d,h]=path[used++],expected=b&1?0:1;
      need(d===expected,"index direction");r=d===0?H(raw(h),r):H(r,raw(h));
    }
    need(used===path.length,"extra sibling");return r;
  }
  function fold(leaf,path){
    need(Array.isArray(path)&&path.length<=64,"block path bound");let r=leaf;
    for(const [d,h] of path){need(d===0||d===1,"direction");r=d===0?H(raw(h),r):H(r,raw(h))}
    return r;
  }
  function proveList(p,policy){
    // policy must originate in the independently authenticated light-client
    // state. Here it is only a frozen fixture pin. A real epoch layout proof
    // is still required; header inner-rest prefix profile must be pinned.
    const layout=policy.layout;
    need(policy.headerProfile==="REST_V4_TO_V8_PREFIX32"&&Array.isArray(layout)&&layout.length>0&&layout.length<=1024&&new Set(layout).size===layout.length,"policy/layout");
    const from=layout.indexOf(p.fromShard),to=layout.indexOf(p.toShard);
    need(from>=0&&to>=0,"shard id");
    const decoded=decodeList(p.bytes);need(decoded.shard===p.toShard,"destination shard");
    const listHash=H(p.bytes);
    const chunkRoot=atIndex(H(listHash),p.receiptPath,to,layout.length);
    const blockReceiptRoot=atIndex(H(chunkRoot),p.chunkPath,from,layout.length);
    need(p.lite instanceof Uint8Array&&p.lite.length===208&&p.rest instanceof Uint8Array&&p.rest.length>=128&&p.rest.length<=MAX_BYTES,"header bytes");
    need(hex(blockReceiptRoot)===hex(p.rest.slice(32,64)),"header receipt root");
    const blockHash=H(H(H(p.lite),H(p.rest)),raw(p.prev));
    need(hex(blockHash)===p.blockHash,"block hash");
    need(hex(fold(blockHash,p.blockPath))===policy.headRoot,"pinned head root");
    need(Number.isSafeInteger(p.receiptIndex)&&p.receiptIndex>=0&&p.receiptIndex<decoded.receipts.length,"receipt index");
    const receipt=decoded.receipts[p.receiptIndex];need(receipt.id===p.expectedId,"receipt identity");
    return receipt;
  }
  function bindDependency({receiver,resolver,data},outcomes,policy){
    const [v,r]=outcomes;
    need(receiver.id===v.id&&resolver.id===r.id&&v.executor===policy.vault&&r.executor===policy.token,"outcome identity");
    need([receiver.id,resolver.id,data.id].every((x,i,a)=>a.indexOf(x)===i),"receipt identity collision");
    need(receiver.predecessor===policy.token&&receiver.receiver===policy.vault&&resolver.predecessor===policy.token&&resolver.receiver===policy.token,"action predecessor");
    need(receiver.actions.length===1&&receiver.actions[0].tag===2&&receiver.actions[0].method==="ft_on_transfer","receiver method");
    need(resolver.actions.length===1&&resolver.actions[0].tag===2&&resolver.actions[0].method==="ft_resolve_transfer","resolver method");
    need(receiver.input.length===0&&receiver.output.length===1&&resolver.input.length===1&&resolver.output.length===0,"data arity");
    need(receiver.output[0].id===resolver.input[0]&&receiver.output[0].receiver===policy.token,"dependency id");
    need(data.tag===1&&data.predecessor===policy.vault&&data.receiver===policy.token&&data.dataId===resolver.input[0],"data receipt routing");
    need(v.status.kind==="value"&&data.data===hex(ascii(v.status.value)),"actual promise data");
    need(r.status.kind==="value","terminal outcome");
    return {dataId:data.dataId,receiverId:v.id,resolverId:r.id,argumentBytes:[receiver.actions[0].args,resolver.actions[0].args],near_finality_verified:false,action_authentication_verified:false,can_mint:false};
  }
  // Fixture encoder independent of decoder layout cursor. Supports the same
  // deliberately narrow action/data profile; not a general nearcore codec.
  function encodeReceipt(r){
    const pre=[string(r.predecessor),string(r.receiver),raw(r.id),num(BigInt(r.tag),1)];
    const unhex=s=>Uint8Array.from(s.match(/../g)?.map(x=>parseInt(x,16))??[]);
    const opt=(v,f)=>v===null?Uint8Array.of(0):cat(Uint8Array.of(1),f(v));
    if(r.tag===1)return cat(...pre,raw(r.dataId),opt(r.data,s=>vec(unhex(s))));
    const key=()=>cat(num(BigInt(r.key.tag),1),unhex(r.key.bytes));
    const body=[string(r.signer)];
    if(r.tag!==0)body.push(opt(r.refundTo,string));
    if(r.tag===7){body.push(num(BigInt(r.auth),1));if(r.auth===0)body.push(key())}
    else body.push(key());
    body.push(num(r.gasPrice,16));if(r.tag===7)body.push(num(r.version,4));
    body.push(seq(r.output.map(d=>cat(raw(d.id),string(d.receiver)))),seq(r.input.map(raw)),seq(r.actions.map(a=>a.tag===3?cat(Uint8Array.of(3),num(a.deposit,16)):cat(Uint8Array.of(2),string(a.method),vec(unhex(a.args)),num(a.gas,8),num(a.deposit,16)))));
    return cat(...pre,...body);
  }
  const encodeList=(shard,rs)=>cat(num(shard,8),seq(rs.map(encodeReceipt)));
  return {need,H,cat,raw,hex,ascii,num,decodeList,atIndex,fold,proveList,bindDependency,encodeReceipt,encodeList};
}
export {makeReceiptVerifier};
