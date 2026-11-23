// ZTB-01. Isolated, unauthenticated research accounting reference.
function makeModel() {
  const MAX = (1n << 64n) - 1n;
  const must = (x, msg) => { if (!x) throw new Error(msg); };
  const amount = x => { must(typeof x === "bigint" && x > 0n && x <= MAX, "amount"); return x; };
  const key = x => { must(typeof x === "string" && x.length > 0 && x.length <= 256, "id"); return x; };
  const get = (m,k) => m.get(k) ?? 0n;
  const add = (m,k,v) => { key(k); const n=get(m,k)+v; must(n>=0n && n<=MAX,"balance"); if(n===0n)m.delete(k);else m.set(k,n); };
  const sum = m => [...m.values()].reduce((a,b)=>a+b,0n);
  const clone = s => ({...s, available:new Map(s.available), margin:new Map(s.margin), utxos:new Map([...s.utxos].map(([k,v])=>[k,{...v}])), deposits:new Map([...s.deposits].map(([k,v])=>[k,{...v}])), exits:new Map([...s.exits].map(([k,v])=>[k,{...v}])), spentOutpoints:new Set(s.spentOutpoints), settlements:new Set(s.settlements)});
  const fresh = () => ({D:0n,M:0n,B:0n,R:0n,available:new Map(),margin:new Map(),utxos:new Map(),deposits:new Map(),exits:new Map(),spentOutpoints:new Set(),settlements:new Set()});
  function invariant(s) {
    const deposits=[...s.deposits.values()], exits=[...s.exits.values()];
    const U=[...s.utxos.values()].reduce((a,b)=>a+b.amount,0n);
    const outstanding=U+sum(s.available)+sum(s.margin);
    const pending=exits.filter(x=>x.status!=="paid").reduce((a,x)=>a+x.amount,0n);
    must([s.D,s.M,s.B,s.R].every(x=>x>=0n&&x<=MAX),"counter bound");
    must(s.D===deposits.reduce((a,x)=>a+x.amount,0n),"deposit sum");
    must(s.M===deposits.filter(x=>x.minted).reduce((a,x)=>a+x.amount,0n),"mint sum");
    must(s.B===exits.reduce((a,x)=>a+x.amount,0n),"burn sum");
    must(s.R===exits.filter(x=>x.status==="paid").reduce((a,x)=>a+x.amount,0n),"paid sum");
    must(s.D>=s.M && s.M>=s.B && s.B>=s.R,"counter order");
    must(s.M-s.B===outstanding && s.B-s.R===pending,"liability conservation");
    must(s.D-s.R>=outstanding+pending,"reserve backing");
    for(const [k,v] of s.utxos){must(!s.spentOutpoints.has(k),"spent outpoint");amount(v.amount);key(v.owner);}
    return {reserve:s.D-s.R,outstanding,pending,unminted:s.D-s.M};
  }
  // Unauthenticated fixture input only. Real callers must derive all events from pinned
  // finalized chain evidence and economic execution; this model has no custody API.
  function apply(prior, ev) {
    const s=clone(prior);
    const id=()=>key(ev.id), a=()=>amount(ev.amount), owner=()=>key(ev.owner);
    const freshOutpoint=()=>{const k=key(ev.outpoint);must(!s.utxos.has(k)&&!s.spentOutpoints.has(k),"outpoint replay");return k;};
    switch(ev.kind) {
      case "deposit": {const k=id();must(!s.deposits.has(k),"deposit replay");const n=a();s.deposits.set(k,{amount:n,owner:owner(),minted:false});s.D+=n;break;}
      case "mint": {const k=id(),d=s.deposits.get(k);must(d&&!d.minted,"unknown or consumed deposit");d.minted=true;s.M+=d.amount;add(s.available,d.owner,d.amount);break;}
      case "send": {const n=a();add(s.available,owner(),-n);add(s.available,key(ev.to),n);break;}
      case "export": {const k=freshOutpoint(),n=a(),o=owner();add(s.available,o,-n);s.utxos.set(k,{owner:o,amount:n});break;}
      case "import": {const k=key(ev.outpoint),u=s.utxos.get(k);must(u&&u.owner===owner(),"outpoint authority");s.utxos.delete(k);s.spentOutpoints.add(k);add(s.available,u.owner,u.amount);break;}
      case "utxo_send": {const k=key(ev.from),u=s.utxos.get(k),next=freshOutpoint();must(u&&u.owner===owner(),"outpoint authority");s.utxos.delete(k);s.spentOutpoints.add(k);s.utxos.set(next,{owner:key(ev.to),amount:u.amount});break;}
      case "reserve": {const n=a(),o=owner();add(s.available,o,-n);add(s.margin,o,n);break;}
      case "release": {const n=a(),o=owner();add(s.margin,o,-n);add(s.available,o,n);break;}
      case "settle": {
        const k=id();must(!s.settlements.has(k),"settlement replay");
        must(Array.isArray(ev.flows)&&ev.flows.length>0&&ev.flows.length<=256,"flow count");
        must(ev.obligations instanceof Map,"obligation map");
        const incoming=new Map(),outgoing=new Map(),net=new Map();
        for(const f of ev.flows){const from=key(f.from),to=key(f.to),n=amount(f.amount);must(from!==to,"self flow");add(outgoing,from,n);add(incoming,to,n);}
        for(const o of new Set([...incoming.keys(),...outgoing.keys(),...ev.obligations.keys()])){
          key(o);const debit=get(outgoing,o),credit=get(incoming,o),expected=ev.obligations.get(o)??0n;
          must(typeof expected==="bigint"&&credit-debit===expected,"obligation mismatch");
          must(debit<=get(s.margin,o),"collateral capacity");
          net.set(o,credit-debit);
        }
        for(const [o,v] of net)add(s.margin,o,v);
        s.settlements.add(k);break;
      }
      case "burn": {const k=id(),n=a();must(!s.exits.has(k),"exit replay");add(s.available,owner(),-n);s.exits.set(k,{amount:n,destination:key(ev.destination),status:"pending",attempt:0});s.B+=n;break;}
      case "begin_payout": {const x=s.exits.get(id());must(x&&x.status==="pending","exit unavailable");must(x.attempt<Number.MAX_SAFE_INTEGER,"attempt overflow");x.attempt++;x.status="inflight";break;}
      case "payout_result": {const x=s.exits.get(id());must(x&&x.status==="inflight"&&ev.attempt===x.attempt&&typeof ev.success==="boolean","callback mismatch");x.status=ev.success?"paid":"pending";if(ev.success)s.R+=x.amount;break;}
      default:throw new Error("unsupported event");
    }
    invariant(s);return s;
  }
  return {fresh,apply,invariant,MAX};
}
export { makeModel };
