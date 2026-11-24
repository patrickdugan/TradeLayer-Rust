import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";
import assert from "node:assert/strict";
import {makeReceiptVerifier} from "./receipt_membership.mjs";
function tests(factory,hash){
 const c=factory(hash),{H,raw,hex,ascii,cat}=c;
 let checks=0;const ok=(b,m)=>{if(!b)throw Error(m);checks++};const bad=(f,m)=>{let thrown=false;try{f()}catch{thrown=true}ok(thrown,m)};
 const h=n=>n.toString(16).padStart(2,"0").repeat(32),leaf=n=>raw(h(n));
 const token="usdc.testnet",vault="bridge.testnet",layout=[7n,2n,19n],dep=h(99);
 const action=(method,args)=>({tag:2,method,args:hex(ascii(JSON.stringify(args))),gas:10000000000000n,deposit:0n});
 const common={signer:"alice.testnet",refundTo:null,key:{tag:0,bytes:h(88)},auth:0,version:110n,gasPrice:1n,output:[],input:[],actions:[]};
 const v={...common,tag:5,predecessor:token,receiver:vault,id:h(1),output:[{id:dep,receiver:token}],actions:[action("ft_on_transfer",{sender_id:"alice.testnet",amount:"10000000",msg:h(42)})]};
 const r={...common,tag:7,predecessor:token,receiver:token,id:h(2),input:[dep],actions:[action("ft_resolve_transfer",{sender_id:"alice.testnet",receiver_id:vault,amount:"10000000"})]};
 const d={tag:1,predecessor:vault,receiver:token,id:h(3),dataId:dep,data:hex(ascii('"2000000"'))};
 const filler={...common,tag:0,predecessor:"system",receiver:"alice.testnet",id:h(4),actions:[{tag:3,deposit:123n}]};
 function merkle(xs,idx){
   let a=xs.map(x=>H(x)),i=idx,path=[];
   while(a.length>1){const j=i^1;if(j<a.length)path.push([i&1?0:1,hex(a[j])]);let next=[];for(let k=0;k<a.length;k+=2)next.push(k+1<a.length?H(a[k],a[k+1]):a[k]);a=next;i>>=1}
   return {root:a[0],path};
 }
 // All three lists are placed in a single synthetic block, purely to
 // exercise byte binding and odd-width shard trees. No execution schedule claim.
 const lists=[[c.encodeList(layout[0],[filler]),c.encodeList(layout[1],[v]),c.encodeList(layout[2],[r])],[c.encodeList(layout[0],[]),c.encodeList(layout[1],[]),c.encodeList(layout[2],[d])],[c.encodeList(layout[0],[]),c.encodeList(layout[1],[]),c.encodeList(layout[2],[])]];
 const chunkRoots=lists.map(x=>merkle(x.map(y=>H(y)),0).root);
 const blockRoot=merkle(chunkRoots,0).root;
 const lite=new Uint8Array(208).fill(8),rest=cat(leaf(81),blockRoot,leaf(82),leaf(83),new Uint8Array(47)),prev=h(77);
 const blockHash=H(H(H(lite),H(rest)),raw(prev)),blockPath=[[1,h(78)]],headRoot=hex(c.fold(blockHash,blockPath));
 const policy={layout,headerProfile:"REST_V4_TO_V8_PREFIX32",headRoot,token,vault};
 function proof(from,to,id){return {bytes:lists[from][to],fromShard:layout[from],toShard:layout[to],receiptPath:merkle(lists[from].map(y=>H(y)),to).path,chunkPath:merkle(chunkRoots,from).path,lite,rest,prev,blockHash:hex(blockHash),blockPath,receiptIndex:0,expectedId:id}}
 const pv=proof(0,1,v.id),pr=proof(0,2,r.id),pd=proof(1,2,d.id);
 const dv=c.proveList(pv,policy),dr=c.proveList(pr,policy),dd=c.proveList(pd,policy);
 ok(dv.tag===5&&dr.tag===7&&dd.tag===1,"versions/membership");
 ok(c.proveList(proof(0,0,filler.id),policy).tag===0,"legacy receipt");
 ok(c.decodeList(lists[2][0]).receipts.length===0,"empty list");
 ok(hex(c.atIndex(H(H(pd.bytes)),pd.receiptPath,2,3))===hex(chunkRoots[1]),"odd width promotion");
 for(let n=1;n<=31;n++){const xs=Array.from({length:n},(_,i)=>H(ascii("shard"),c.num(BigInt(i),8)));for(let i=0;i<n;i++){let p=merkle(xs,i);ok(hex(c.atIndex(H(xs[i]),p.path,i,n))===hex(p.root),"index path "+n+"/"+i)}}
 const mutate=(a,i)=>{const b=a.slice();b[i]^=1;return b};
 bad(()=>c.proveList({...pv,bytes:mutate(pv.bytes,pv.bytes.length-1)},policy),"receipt body mutation");
 bad(()=>c.proveList({...pv,expectedId:r.id},policy),"id swap");
 bad(()=>c.proveList({...pv,toShard:19n},policy),"wrong destination");
 bad(()=>c.proveList({...pv,fromShard:2n},policy),"wrong source");
 bad(()=>c.proveList({...pv,receiptIndex:1},policy),"index");
 bad(()=>c.proveList({...pv,bytes:c.cat(pv.bytes,Uint8Array.of(0))},policy),"trailing");
 bad(()=>c.decodeList(pv.bytes.slice(0,-1)),"truncation");
 bad(()=>c.proveList({...pv,receiptPath:[[0,h(66)],...pv.receiptPath]},policy),"extra path");
 bad(()=>c.proveList({...pv,chunkPath:pv.chunkPath.map(([q,x],i)=>[i===0?1-q:q,x])},policy),"direction");
 bad(()=>c.proveList({...pv,rest:mutate(rest,32)},policy),"root substitution");
 bad(()=>c.proveList({...pv,rest:mutate(rest,0)},policy),"header suffix mutation");
 bad(()=>c.proveList({...pv,lite:mutate(lite,0)},policy),"lite mutation");
 bad(()=>c.proveList(pv,{...policy,headRoot:h(79)}),"head");
 bad(()=>c.proveList(pv,{...policy,layout:[2n,7n,19n]}),"layout order");
 bad(()=>c.proveList(pv,{...policy,headerProfile:"V2"}),"unreviewed header version");
 bad(()=>c.decodeList(c.encodeList(7n,[v,v])),"duplicate id");
 bad(()=>c.decodeList(c.encodeList(7n,[{...v,tag:2}])),"unsupported variant");
 const outcomes=[{id:v.id,executor:vault,status:{kind:"value",value:'"2000000"'}},{id:r.id,executor:token,status:{kind:"value",value:'"8000000"'}}];
 const graph={receiver:dv,resolver:dr,data:dd};
 ok(c.bindDependency(graph,outcomes,policy).dataId===dep,"dependency");
 bad(()=>c.bindDependency({...graph,data:{...dd,data:hex(ascii('"0"'))}},outcomes,policy),"promise return swap");
 bad(()=>c.bindDependency({...graph,data:{...dd,data:null}},outcomes,policy),"failed data");
 bad(()=>c.bindDependency({...graph,data:{...dd,dataId:h(50)}},outcomes,policy),"data id");
 bad(()=>c.bindDependency({...graph,resolver:{...dr,input:[h(51)]}},outcomes,policy),"input mismatch");
 bad(()=>c.bindDependency({...graph,receiver:{...dv,output:[{id:dep,receiver:vault}]}},outcomes,policy),"data receiver");
 bad(()=>c.bindDependency({...graph,data:{...dd,predecessor:"mallory.testnet"}},outcomes,policy),"data predecessor");
 bad(()=>c.bindDependency({...graph,resolver:{...dr,actions:[{...dr.actions[0],method:"ft_transfer"}]}},outcomes,policy),"method");
 bad(()=>c.bindDependency(graph,[{...outcomes[0],id:h(52)},outcomes[1]],policy),"outcome id");
 return {checks,indexed_merkle_cases:496,fixture_receipt_list_bytes:[pv.bytes.length,pr.bytes.length,pd.bytes.length],synthetic_head_root:headRoot,synthetic_block_hash:hex(blockHash),synthetic_receipt_root:hex(blockRoot),nearcore_source_pin:"440567f0a21df411a54b28fef2ebe586920b599d",live_receipt_fixture_verified:false,near_finality_verified:false,action_authentication_verified:false,nova_verified:false,can_mint:false,production_ready:false};
}
const result=tests(makeReceiptVerifier,b=>createHash("sha256").update(b).digest());
assert.deepEqual(result,JSON.parse(readFileSync(new URL("./evidence_b.json",import.meta.url),"utf8")));
console.log(JSON.stringify(result,null,2));
