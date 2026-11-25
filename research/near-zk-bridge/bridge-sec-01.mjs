import { makeModel } from "./model.mjs";

// BRIDGE-SEC-01: synthetic authorization-boundary regression.
// No live chain calls, private keys, proof generation, or custody.
// All quantities are integer fixture token units (not USDC atoms).
function run(makeModel) {
  const {fresh,apply,invariant}=makeModel();
  let checks=0;
  const check=(ok,label)=>{if(!ok)throw new Error(label);checks++;};
  const reject=(fn,label)=>{
    let rejected=false;
    try {fn();} catch(e) {if(!(e instanceof Error))throw e;rejected=true;}
    check(rejected,label);
  };
  const opening=5213905n, phantom=5213000n;
  const initial=apply(apply(fresh(),{kind:"deposit",id:"honest-0",owner:"honest",amount:opening}),
    {kind:"mint",id:"honest-0"});
  const fake={kind:"deposit",id:"absent-331",owner:"fixture-adversary",amount:phantom};
  // This arm represents a caller that mistakes authenticated relay metadata
  // for proof of source execution. The current model trusts its caller.
  function relayOnly(s,event,relay,activeRelay) {
    if(relay!==activeRelay)throw new Error("relay");
    return apply(s,event);
  }
  let vulnerable=relayOnly(initial,fake,"epoch-A","epoch-A");
  vulnerable=apply(vulnerable,{kind:"mint",id:fake.id});
  vulnerable=apply(vulnerable,{kind:"burn",id:"exit-1",owner:fake.owner,amount:phantom,destination:"fixture-destination"});
  vulnerable=apply(vulnerable,{kind:"begin_payout",id:"exit-1"});
  vulnerable=apply(vulnerable,{kind:"payout_result",id:"exit-1",attempt:1,success:true});
  const realReserve=opening-phantom; // independent of claimed deposit counter
  check(realReserve===905n,"historical scale");
  check(invariant(vulnerable).reserve===opening,"internal accounting appears solvent");
  check(invariant(vulnerable).outstanding===opening,"honest liability persists");
  check(invariant(vulnerable).outstanding-realReserve===phantom,"external shortfall");
  // Independent trusted fixture, representing the OUTPUT of a future
  // authenticated source-chain proof verifier. Neither relay nor event
  // submission is permitted to populate this table or select its checkpoint.
  const genuine=Object.freeze({kind:"deposit",id:"real-1",owner:"alice",amount:100n,
    chain:"near-fixture",route:"near-to-tl-fixture",asset:"mock-usdc",
    vault:"mock-vault",root:"finalized-root-1",finalized:true});
  const trusted=new Map([[genuine.id,genuine]]);
  const fields=["kind","id","owner","amount","chain","route","asset","vault","root","finalized"];
  function guard(s,event,relay,policy) {
    if(policy.paused||relay!==policy.relay)throw new Error("admission closed");
    const canonical=trusted.get(event.id);
    if(!canonical||!canonical.finalized||!fields.every(f=>event[f]===canonical[f]))
      throw new Error("independent evidence mismatch");
    return apply(s,{kind:"deposit",id:canonical.id,owner:canonical.owner,amount:canonical.amount});
  }
  const a={relay:"epoch-A",paused:false}, b={relay:"epoch-B",paused:false};
  const before=invariant(initial);
  reject(()=>guard(initial,{...genuine,...fake},"epoch-A",a),"valid relay / nonexistent source");
  reject(()=>guard(initial,{...genuine,...fake,id:"absent-332"},"epoch-B",b),"replacement relay / nonexistent source");
  reject(()=>guard(initial,genuine,"epoch-A",b),"retired relay");
  for(const [field,bad] of Object.entries({owner:"mallory",amount:101n,chain:"other",route:"other",
    asset:"other",vault:"other",root:"orphan-root",finalized:false,kind:"mint"})){
    reject(()=>guard(initial,{...genuine,[field]:bad},"epoch-B",b),"binding: "+field);
  }
  reject(()=>guard(initial,genuine,"epoch-B",{...b,paused:true}),"independent pause");
  check(invariant(initial).reserve===before.reserve&&invariant(initial).outstanding===before.outstanding,
    "all rejected submissions leave original state untouched");
  const funded=guard(initial,genuine,"epoch-A",a);
  reject(()=>guard(funded,genuine,"epoch-B",b),"deposit replay across rotation");
  const minted=apply(funded,{kind:"mint",id:genuine.id});
  reject(()=>apply(minted,{kind:"mint",id:genuine.id}),"mint replay");
  reject(()=>apply(minted,{kind:"burn",id:"invalid-exit",owner:fake.owner,amount:phantom,destination:"fixture"}),"unfunded exit");
  const balance=invariant(minted);
  check(balance.reserve===opening+100n&&balance.outstanding===opening+100n&&balance.pending===0n,
    "genuine finalized deposit survives gate");
  return {experiment:"BRIDGE-SEC-01",checks,fixture_opening:String(opening),
    relay_only_real_reserve:String(realReserve),relay_only_shortfall:String(phantom),
    guarded_real_reserve:String(opening+100n),guarded_outstanding:String(balance.outstanding),
    native_finality_verified:false,nova_verified:false,economic_authorized:false,production_ready:false};
}
console.log(JSON.stringify(run(makeModel),null,2));
