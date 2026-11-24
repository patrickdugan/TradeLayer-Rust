import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";
import assert from "node:assert/strict";
import {reference} from "./reference.mjs";
function tests(make,hash){
 const c=make(hash);let checks=0;const ok=(v,m)=>{if(!v)throw Error(m);checks++};const bad=(f,m)=>{let threw=false;try{f()}catch{threw=true}ok(threw,m)};
 const h=n=>n.toString(16).padStart(2,"0").repeat(32);
 // Nearcore development guide: documented receipt, partial outcome and shard root.
 const known={id:c.hex(c.b58("6bdKUtGbybhYEQ2hb2BFCTDMrtPBw8YDnFpANZHGt5im")),executor:"node0",children:[],gas:223182562500n,burnt:0n,logs:[],status:{kind:"value",value:""}};
 const siblings=[[1,"BWwZ4wHuzaUxdDSrhAEPjFQtDgwzb8K4zoNzfX9A3SkK"],[0,"Dpg4nQQwbkBZMmdNYcZiDPiihZPpsyviSTdDZgBRAn2z"],[1,"BruTLiGx8f71ufoMKzD4H4MbAvWGd3FLL5JoJS3XJS3c"]].map(([d,b])=>[d,c.hex(c.b58(b))]);
 const expected=c.hex(c.b58("2sZ81kLj2cw5UHTjdTeMxmaWn2zFeyr5pFunxn6aGTNB"));
 const actual=c.hex(c.path(c.outcomeHash(known),siblings));
 ok(actual===expected,"nearcore published shard root: "+actual+" != "+expected);
 ok(c.hex(c.path(c.outcomeHash({...known,gas:known.gas+1n}),siblings))!==expected,"documented mutation");
 const policy={token:"usdc.testnet",vault:"bridge.testnet",domain:h(22)};
 const event=(from,to,n,memo)=>"EVENT_JSON:"+JSON.stringify({standard:"nep141",version:"1.0.0",event:"ft_transfer",data:[{old_owner_id:from,new_owner_id:to,amount:String(n),...(memo?{memo}:{})}]});
 function fixture(unused){
  const a=10000000n,u=BigInt(unused),k=a-u, sender="alice.testnet",script=h(42);
  const t={id:h(1),executor:policy.token,children:[h(2),h(3)],gas:20n,burnt:10n,status:{kind:"receipt",id:h(3)},logs:[event(sender,policy.vault,a,null)]};
  const v={id:h(2),executor:policy.vault,children:[],gas:20n,burnt:10n,status:{kind:"value",value:JSON.stringify(String(u))},logs:[`ZTB_DEPOSIT_V1:${policy.domain}:7:${sender}:${script}:${a}:${u}`]};
  const r={id:h(3),executor:policy.token,children:[],gas:20n,burnt:10n,status:{kind:"value",value:JSON.stringify(String(k))},logs:u?[event(policy.vault,sender,u,"refund")]:[]};
  const graph={transfer:{id:t.id,method:"ft_transfer_call",predecessor:sender,receiver:policy.token,sender,vault:policy.vault,amount:String(a),script},receive:{id:v.id,method:"ft_on_transfer",predecessor:policy.token,receiver:policy.vault,sender,amount:String(a),script,sequence:"7",outputData:[{id:h(99),receiver:policy.token}]},resolve:{id:r.id,method:"ft_resolve_transfer",predecessor:policy.token,receiver:policy.token,sender,vault:policy.vault,amount:String(a),inputData:[h(99)]}};
  return {p:[t,v,r],graph};
 }
 const f=fixture(2000000);const t=f.p[0];
 // Independently pinned head root simulated; all other layers hash real bytes.
 const shard=c.path(c.outcomeHash(t),[[1,h(44)]]);
 const block=c.path(c.H(shard),[[0,h(45)]]);
 const header={prevBlockHash:h(46),innerRestHash:h(47),lite:{height:101n,epoch:h(48),nextEpoch:h(49),prevState:h(50),outcomeRoot:c.hex(block),timestamp:123456789n,nextBp:h(51),blockMerkleRoot:h(52)}};
 const bh=c.blockHash(header),head=c.hex(c.path(bh,[[1,h(53)]]));
 const proof={outcome:t,outcomePath:[[1,h(44)]],shardPath:[[0,h(45)]],header,blockHash:c.hex(bh),blockPath:[[1,h(53)]]};
 ok(c.proveOutcome(proof,head).id===t.id,"three-level membership");
 bad(()=>c.proveOutcome(proof,h(54)),"head substitution");
 bad(()=>c.proveOutcome({...proof,outcome:{...t,logs:[]}},head),"log substitution");
 bad(()=>c.proveOutcome({...proof,outcome:{...t,children:[h(2)]}},head),"receipt edge substitution");
 bad(()=>c.proveOutcome({...proof,blockHash:h(54)},head),"block hash substitution");
 bad(()=>c.proveOutcome({...proof,shardPath:[[1,h(45)]]},head),"shard direction");
 bad(()=>c.proveOutcome({...proof,header:{...header,lite:{...header.lite,height:102n}}},head),"height substitution");
 bad(()=>c.proveOutcome({...proof,outcomePath:[[2,h(44)]]},head),"invalid path flag");
 const d=c.inspectGraph(f.p,f.graph,policy);
 ok(d.amount==="8000000"&&d.creditable&&!d.can_mint,"partial refund");
 const full=fixture(0);ok(c.inspectGraph(full.p,full.graph,policy).amount==="10000000","full retain");
 const zero=fixture(10000000);ok(!c.inspectGraph(zero.p,zero.graph,policy).creditable,"full refund");
 const replace=(i,fields)=>f.p.map((x,j)=>i===j?{...x,...fields}:x);
 bad(()=>c.inspectGraph(replace(2,{status:{kind:"value",value:'"10000000"'}}),f.graph,policy),"incomplete refund");
 bad(()=>c.inspectGraph(replace(2,{logs:["EVENT_JSON:burn"]}),f.graph,policy),"sender deleted burn");
 bad(()=>c.inspectGraph(replace(2,{logs:[...f.p[2].logs,"EVENT_JSON:burn"]}),f.graph,policy),"extra burn");
 bad(()=>c.inspectGraph(replace(0,{status:{kind:"receipt",id:h(55)}}),f.graph,policy),"unrelated resolver");
 bad(()=>c.inspectGraph(replace(2,{executor:"fake.testnet"}),f.graph,policy),"executor spoof");
 bad(()=>c.inspectGraph(replace(1,{logs:["ZTB02B_CANDIDATE:7"]}),f.graph,policy),"sequence-only log");
 bad(()=>c.inspectGraph(f.p,{...f.graph,receive:{...f.graph.receive,script:h(58)}},policy),"recipient substitution");
 bad(()=>c.inspectGraph(f.p,{...f.graph,receive:{...f.graph.receive,sequence:"8"}},policy),"candidate substitution");
 bad(()=>c.inspectGraph(f.p,{...f.graph,resolve:{...f.graph.resolve,inputData:[h(59)]}},policy),"data dependency substitution");
 bad(()=>c.inspectGraph(f.p,{...f.graph,resolve:{...f.graph.resolve,method:"ft_transfer"}},policy),"action substitution");
 bad(()=>c.inspectGraph(f.p,{...f.graph,resolve:{...f.graph.resolve,predecessor:"mallory.testnet"}},policy),"resolver predecessor");
 bad(()=>c.inspectGraph(f.p,{...f.graph,receive:{...f.graph.receive,amount:"01"}},policy),"amount substitution");
 bad(()=>c.inspectGraph(f.p,f.graph,{...policy,domain:h(60)}),"domain substitution");
 bad(()=>c.inspectGraph(replace(1,{status:{kind:"value",value:'"02000000"'}}),f.graph,policy),"noncanonical amount");
 const sameCandidateNewReceipt={...f.graph,resolve:{...f.graph.resolve,id:h(61)}};
 const p2=replace(2,{id:h(61)}).map((x,i)=>i===0?{...x,children:[h(2),h(61)],status:{kind:"receipt",id:h(61)}}:x);
 const d2=c.inspectGraph(p2,sameCandidateNewReceipt,policy);
 ok(d.depositId!==d2.depositId&&d.candidateNullifier===d2.candidateNullifier,"dual nullifier necessity");
 return {checks,published_shard_root:actual,synthetic_head_root:head,partial_refund_atoms:d.amount,deposit_id:d.depositId,candidate_nullifier:d.candidateNullifier,near_finality_verified:false,action_authentication_verified:false,nova_verified:false,can_mint:false,production_ready:false};
}
const report=tests(reference,b=>createHash("sha256").update(b).digest());
assert.deepEqual(report,JSON.parse(readFileSync(new URL("./evidence.json",import.meta.url),"utf8")));
console.log(JSON.stringify(report,null,2));
