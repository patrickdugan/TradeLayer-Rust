import { makeModel } from './model.mjs';

// ZTB-PILOT-01, 2026-11-25. Research-only; deterministic fixture executor.
function run(makeModel) {
  // Independent, fictional finalized source and execution fixtures. No live proof adapter.
  const {fresh,apply,invariant}=makeModel();
  const U=1000000n, S=100000000n, usd=n=>BigInt(n)*U;
  const ok=(b,m)=>{if(!b)throw Error(m); checks++;};
  const must=(b,m)=>{if(!b)throw Error(m);};
  let checks=0;
  function reject(f,m){let rejected=false;try{f();}catch(e){rejected=true;}ok(rejected,m);}
  const source=[
    {chain:"near-test-fixture",token:"mock-usdc",vault:"vault-fixture",locator:"resolver-a:0",route:"near-tl",asset:"USDC",owner:"alice",amount:usd(4000)},
    {chain:"near-test-fixture",token:"mock-usdc",vault:"vault-fixture",locator:"resolver-b:0",route:"near-tl",asset:"USDC",owner:"bob",amount:usd(3000)},
    {chain:"near-test-fixture",token:"mock-usdc",vault:"vault-fixture",locator:"resolver-c:0",route:"near-tl",asset:"USDC",owner:"carol",amount:usd(3000)},
    {chain:"bitcoin-regtest-fixture",token:"BTC",vault:"utxoref-fixture",locator:"fixture-tx:0",route:"btc-tl",asset:"BTC",owner:"bob",amount:S/10n}
  ].map(Object.freeze);
  // Global source identity intentionally excludes verifier ID, relay, head, asset route and recipient.
  // Production needs canonical hash encoding and state-root persistence; strings here are fixtures.
  const key=e=>JSON.stringify([e.chain,e.token,e.vault,e.locator]);
  const same=(a,b,fields)=>fields.every(k=>a[k]===b[k]);
  const fields=Object.keys(source[0]);
  const trusted=new Map(source.map(e=>[key(e),e]));
  const caps={USDC:usd(10000),BTC:S/10n};
  const start=()=>({USDC:fresh(),BTC:fresh(),used:new Set(),trades:new Set(),settlements:new Set()});
  function deposit(state,e,program) {
    must(["fixture-v1","fixture-v2"].includes(program),"fixture program");
    const k=key(e), c=trusted.get(k);
    must(c&&same(c,e,fields),"source binding");
    must(!state.used.has(k),"domain-wide source replay");
    must(state[c.asset].D+c.amount<=caps[c.asset],"pilot cumulative cap");
    const next=apply(apply(state[c.asset],{kind:"deposit",id:k,owner:c.owner,amount:c.amount}),{kind:"mint",id:k});
    return {...state,[c.asset]:next,used:new Set([...state.used,k])};
  }
  // Independently matched orders; same-asset balances on both sides are necessary.
  const orders=[
    Object.freeze({id:"spot-1",buyer:"alice",seller:"bob",sats:S/100n,price:usd(60000),quote:usd(600)}),
    Object.freeze({id:"spot-2",buyer:"alice",seller:"carol",sats:S/100n,price:usd(60000),quote:usd(600)}),
    Object.freeze({id:"spot-3",buyer:"alice",seller:"bob",sats:S/100n,price:usd(60000),quote:usd(600)})
  ];
  const orderTable=new Map(orders.map(e=>[e.id,e]));
  function spot(state,e) {
    const c=orderTable.get(e.id);
    must(c&&same(c,e,Object.keys(c))&&!state.trades.has(c.id),"matched order / replay");
    must(c.buyer!==c.seller&&c.sats>0n&&c.price>0n&&c.sats*c.price%S===0n&&c.quote===c.sats*c.price/S,"quote units");
    // Pure apply() clones input. Publish neither leg unless both succeed.
    const quote=apply(state.USDC,{kind:"send",owner:c.buyer,to:c.seller,amount:c.quote});
    const base=apply(state.BTC,{kind:"send",owner:c.seller,to:c.buyer,amount:c.sats});
    return {...state,USDC:quote,BTC:base,trades:new Set([...state.trades,c.id])};
  }
  const accounts=["alice","bob","carol"];
  const balance=(s,a)=>s.available.get(a)??0n;
  const margin=(s,a)=>s.margin.get(a)??0n;
  const mark=Object.freeze({id:"epoch-1",opening:usd(60000),closing:usd(64000)});
  const positions=new Map([["alice",S/20n],["bob",-S/50n],["carol",-3n*S/100n]]);
  function settle(state,e) {
    must(e.asset==="USDC"&&e.id===mark.id&&!state.settlements.has(e.id),"settlement scope / replay");
    must(e.opening===mark.opening&&e.closing===mark.closing,"independent mark");
    const obligations=new Map();
    let posSum=0n;
    for (const [a,q] of positions) {
      posSum+=q;
      const n=q*(mark.closing-mark.opening);
      must(n%S===0n,"exact pilot rounding");
      obligations.set(a,n/S);
    }
    must(posSum===0n,"position balance");
    // No caller-defined obligations; existing model verifies graph against derived net amounts.
    const quote=apply(state.USDC,{kind:"settle",id:e.id,flows:e.flows,obligations});
    return {...state,USDC:quote,settlements:new Set([...state.settlements,e.id])};
  }
  let s=start();
  reject(()=>deposit(s,{...source[0],locator:"absent"},"fixture-v1"),"phantom deposit");
  for(const [field,bad] of Object.entries({chain:"foreign",token:"fake",vault:"other",route:"other",asset:"BTC",owner:"mallory",amount:1n})){
    reject(()=>deposit(s,{...source[0],[field]:bad},"fixture-v1"),"deposit binding "+field);
  }
  reject(()=>deposit(s,source[0],"unknown-program"),"program not pinned");
  for(const e of source)s=deposit(s,e,"fixture-v1");
  reject(()=>deposit(s,source[0],"fixture-v2"),"cross-verifier replay");
  reject(()=>deposit(s,source[3],"fixture-v2"),"BTC outpoint replay");
  ok(s.USDC.D===usd(10000)&&s.BTC.D===S/10n,"separate opening reserves");
  reject(()=>spot(s,{...orders[0],quote:usd(1)}),"forged quote");
  reject(()=>spot(s,orders[1]),"unfunded BTC leg");
  ok(balance(s.USDC,"alice")===usd(4000)&&balance(s.USDC,"carol")===usd(3000),"failed swap atomic");
  s=spot(s,orders[0]);
  ok(balance(s.USDC,"alice")===usd(3400)&&balance(s.USDC,"bob")===usd(3600),"quote leg");
  ok(balance(s.BTC,"alice")===S/100n&&balance(s.BTC,"bob")===9n*S/100n,"base leg");
  reject(()=>spot(s,orders[0]),"spot replay");
  for(const a of accounts)s={...s,USDC:apply(s.USDC,{kind:"reserve",owner:a,amount:usd(1000)})};
  reject(()=>apply(s.USDC,{kind:"burn",id:"reserved",owner:"alice",amount:usd(3000),destination:"alice.testnet"}),"margin inaccessible");
  const legit={asset:"USDC",...mark,flows:[{from:"bob",to:"alice",amount:usd(80)},{from:"carol",to:"alice",amount:usd(120)}]};
  reject(()=>settle(s,{...legit,asset:"BTC"}),"cross-asset netting");
  reject(()=>settle(s,{...legit,closing:usd(65000)}),"forged oracle mark");
  reject(()=>settle(s,{...legit,flows:[{from:"bob",to:"alice",amount:usd(200)}]}),"conserving forged obligation");
  s=settle(s,legit);
  ok(margin(s.USDC,"alice")===usd(1200)&&margin(s.USDC,"bob")===usd(920)&&margin(s.USDC,"carol")===usd(880),"authorized fixture PnL");
  reject(()=>settle(s,legit),"settlement replay");
  ok(invariant(s.BTC).outstanding===S/10n,"PnL cannot mutate BTC");
  for(const a of accounts)s={...s,USDC:apply(s.USDC,{kind:"release",owner:a,amount:margin(s.USDC,a)})};
  ok(balance(s.USDC,"alice")===usd(3600)&&balance(s.USDC,"bob")===usd(3520)&&balance(s.USDC,"carol")===usd(2880),"post-close balances");
  // This model invokes fixture events; real withdrawal requires authenticated TLSTATE/TLEXIT.
  let q=apply(s.USDC,{kind:"burn",id:"exit-1",owner:"alice",amount:usd(600),destination:"alice.testnet"});
  q=apply(q,{kind:"begin_payout",id:"exit-1"});
  reject(()=>apply(q,{kind:"begin_payout",id:"exit-1"}),"no ambiguous retry");
  q=apply(q,{kind:"payout_result",id:"exit-1",attempt:1,success:false});
  q=apply(q,{kind:"begin_payout",id:"exit-1"});
  reject(()=>apply(q,{kind:"payout_result",id:"exit-1",attempt:1,success:true}),"stale callback");
  q=apply(q,{kind:"payout_result",id:"exit-1",attempt:2,success:true});
  reject(()=>apply(q,{kind:"payout_result",id:"exit-1",attempt:2,success:true}),"duplicate payout");
  s={...s,USDC:q};
  ok(invariant(s.USDC).reserve===usd(9400)&&invariant(s.USDC).outstanding===usd(9400)&&invariant(s.USDC).pending===0n,"USDC final reconciliation");
  ok(invariant(s.BTC).reserve===S/10n&&invariant(s.BTC).outstanding===S/10n,"BTC final reconciliation");
  return {experiment:"ZTB-PILOT-01",checks,usdc_reserve_atoms:String(invariant(s.USDC).reserve),btc_reserve_sats:String(invariant(s.BTC).reserve),usdc_balances:Object.fromEntries(accounts.map(a=>[a,String(balance(s.USDC,a))])),btc_balances:Object.fromEntries(accounts.map(a=>[a,String(balance(s.BTC,a))])),fixture_only:true,near_finality_verified:false,btc_anchor_verified:false,nova_verified:false,economic_authorized:false,production_ready:false};
}

console.log(JSON.stringify(run(makeModel), null, 2));
