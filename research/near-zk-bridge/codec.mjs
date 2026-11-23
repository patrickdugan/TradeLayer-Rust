// Canonical ZTB-02 research ABI. Hash function is injected for independent fixtures.
function codecFactory(hash){
  const assert=(b,m)=>{if(!b)throw Error(m)};
  const ascii=s=>{assert(typeof s==="string"&&/^[\x20-\x7e]+$/.test(s),"ascii");return Uint8Array.from([...s].map(c=>c.charCodeAt(0)))};
  const cat=(...xs)=>{const a=new Uint8Array(xs.reduce((n,x)=>n+x.length,0));let o=0;for(const x of xs){a.set(x,o);o+=x.length}return a};
  const hex=a=>Array.from(a,x=>x.toString(16).padStart(2,"0")).join("");
  const raw=s=>{assert(typeof s==="string"&&/^[0-9a-f]{64}$/.test(s),"digest");return Uint8Array.from(s.match(/../g).map(x=>parseInt(x,16)))};
  const H=(tag,...xs)=>hash(cat(ascii(tag),Uint8Array.of(0),...xs));
  const uint=(n,k)=>{assert(typeof n==="bigint"&&n>=0n&&n<(1n<<BigInt(8*k)),"integer range");const a=new Uint8Array(k);for(let i=0;i<k;i++){a[i]=Number(n&255n);n>>=8n}return a};
  const read=(a)=>{let n=0n;for(let i=a.length-1;i>=0;i--)n=(n<<8n)+BigInt(a[i]);return n};
  const account=s=>{assert(typeof s==="string"&&s.length>=2&&s.length<=64&&/^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(s),"account");return ascii(s)};
  const named=(tag,s)=>{const b=account(s);return H(tag,uint(BigInt(b.length),1),b)};
  const domainFields=["nearGenesis","bitcoinGenesis","activation","policy"];
  const domain=d=>hex(H("TLBRIDGE/DOMAIN/V1",...domainFields.map(k=>raw(d[k])),named("TLBRIDGE/TOKEN/V1",d.token),named("TLBRIDGE/VAULT/V1",d.vault),uint(d.property,4)));
  const digests=["domain","program","key","previousRoot","nextRoot","parentBlock","tipBlock","nearFinalBlock","depositRoot","exitRoot"];
  const nums=["epoch","firstHeight","lastHeight","depositCount","exitCount","deposited","minted","burned"];
  const MAGIC="TLSTMT01", LENGTH=8+32*digests.length+8*nums.length;
  function encode(s){
    assert(Object.keys(s).length===digests.length+nums.length&&digests.concat(nums).every(k=>Object.hasOwn(s,k)),"statement fields");
    assert(s.firstHeight<=s.lastHeight&&s.epoch>0n,"interval");
    assert(s.deposited>=s.minted&&s.minted>=s.burned,"supply");
    assert(s.previousRoot!==s.nextRoot,"unchanged root");
    return cat(ascii(MAGIC),...digests.map(k=>raw(s[k])),...nums.map(k=>uint(s[k],8)));
  }
  function decode(b){
    assert(b instanceof Uint8Array&&b.length===LENGTH,"statement size");
    assert(hex(b.slice(0,8))===hex(ascii(MAGIC)),"statement magic");
    const s={};let p=8;
    for(const k of digests){s[k]=hex(b.slice(p,p+32));p+=32}
    for(const k of nums){s[k]=read(b.slice(p,p+8));p+=8}
    encode(s);return s;
  }
  const statementHash=s=>hash(encode(s));
  const packet=s=>cat(ascii("TLBRDG01"),statementHash(s));
  const limbs=b=>{assert(b.length===32,"digest length");const be=x=>BigInt("0x"+hex(x));return [be(b.slice(0,16)),be(b.slice(16))]};
  const depositId=d=>hex(H("TLBRIDGE/DEPOSIT-ID/V1",raw(d.domain),raw(d.receipt),uint(d.event,4)));
  function depositLeaf(d){
    assert(d.amount>0n,"zero deposit");
    return H("TLBRIDGE/DEPOSIT-LEAF/V1",raw(d.domain),uint(d.index,8),raw(depositId(d)),named("TLBRIDGE/SENDER/V1",d.sender),raw(d.recipientScriptHash),uint(d.amount,8));
  }
  const DEPTH=16, CAP=1<<DEPTH;
  const node=(a,b)=>H("TLBRIDGE/DEPOSIT-NODE/V1",a,b);
  const empty=[H("TLBRIDGE/DEPOSIT-EMPTY/V1")];
  for(let i=0;i<DEPTH;i++)empty.push(node(empty[i],empty[i]));
  const countRoot=(count,r)=>H("TLBRIDGE/DEPOSIT-ROOT/V1",uint(BigInt(count),8),r);
  function tree(leaves){
    assert(leaves.length<=CAP,"capacity");const levels=[new Map(leaves.map((a,i)=>[i,a]))];
    for(let l=0;l<DEPTH;l++){
      const prev=levels[l],next=new Map();
      for(const p of new Set([...prev.keys()].map(i=>Math.floor(i/2))))next.set(p,node(prev.get(2*p)??empty[l],prev.get(2*p+1)??empty[l]));
      levels.push(next);
    }
    return {root:hex(countRoot(leaves.length,levels[DEPTH].get(0)??empty[DEPTH])),path(i){assert(Number.isInteger(i)&&i>=0&&i<leaves.length,"leaf index");const out=[];for(let l=0;l<DEPTH;l++){out.push(hex(levels[l].get(i^1)??empty[l]));i>>=1}return out}};
  }
  function member(d,count,path,root){
    try{
      assert(typeof count==="bigint"&&count>0n&&count<=BigInt(CAP)&&d.index>=0n&&d.index<count&&path.length===DEPTH,"membership bounds");
      let x=depositLeaf(d),i=Number(d.index);
      for(const p of path){const y=raw(p);x=i&1?node(y,x):node(x,y);i>>=1}
      return hex(countRoot(Number(count),x))===root;
    }catch{return false}
  }
  return {hex,raw,domain,encode,decode,packet,statementHash,limbs,depositId,depositLeaf,tree,member,LENGTH,CAP};
}
export { codecFactory };
