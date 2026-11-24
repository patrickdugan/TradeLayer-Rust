"use strict";
// Synthetic model: no receipt authentication or ZK verification.
function run() {
  const U64=(1n<<64n)-1n, U128=(1n<<128n)-1n, SCALE=10n**13n, BTC=10n**11n;
  const asset="regtest-fixture-asset", chain="regtest-fixture-genesis";
  const bounded=(v,max)=>typeof v==="bigint"&&v>0n&&v<=max;
  const fail=(s)=>{throw new Error(s)};
  const q8=(u,b)=>{
    if(!bounded(u,U128)||!bounded(b,U128))fail("amount range");
    const n=SCALE*u,q=n/b,r=n%b;
    return q+(2n*r>=b?1n:0n);
  };
  const format=(p)=>`${p/100000000n}.${(p%100000000n).toString().padStart(8,"0")}`;
  const median=(p)=>{
    if(!p.length)return null;
    const a=p.slice().sort((x,y)=>x<y?-1:x>y?1:0), i=Math.floor(a.length/2);
    return a.length%2?a[i]:(a[i-1]+a[i]+1n)/2n;
  };
  const clean=()=>({seen:new Set(),quotes:new Map(),lastEpoch:-1});
  const safeText=(x)=>typeof x==="string"&&/^[A-Za-z0-9_-]{1,128}$/.test(x);
  function window(prev,epoch,rows,registry,minGroups=5) {
    if(!Number.isSafeInteger(epoch)||epoch!==prev.lastEpoch+1)fail("epoch order");
    if(!Number.isSafeInteger(minGroups)||minGroups<1)fail("group floor");
    if(!Array.isArray(rows)||rows.length>10000)fail("batch size");
    const seen=new Set(prev.seen),quotes=new Map(prev.quotes),groups=new Map();
    let B=0n,U=0n;
    for(const r of rows){
      if(r.chain!==chain||r.asset!==asset||r.epoch!==epoch)fail("domain/epoch");
      if(r.status!=="settled"||!r.feeKnown||!["asset_buy","asset_sell"].includes(r.direction))fail("eligibility");
      if(!bounded(r.b,U64)||!bounded(r.u,U64)||!bounded(r.quoteMaxB,U64))fail("amount range");
      if(!safeText(r.channel)||![0,1].includes(r.offerer)||typeof r.htlc!=="bigint"||r.htlc<0n||r.htlc>U64||!safeText(r.quote)||!safeText(r.signer))fail("identity");
      const g=registry.get(r.operator);
      if(!safeText(g))fail("registry");
      const key=JSON.stringify([chain,r.channel,r.offerer,r.htlc.toString()]);
      if(seen.has(key))fail("duplicate");
      const qkey=JSON.stringify([r.signer,r.quote]);
      const old=quotes.get(qkey)||{used:0n,max:r.quoteMaxB,asset:r.asset,direction:r.direction};
      if(old.max!==r.quoteMaxB||old.asset!==r.asset||old.direction!==r.direction||old.used+r.b>old.max)fail("quote cap");
      const nextB=B+r.b,nextU=U+r.u;
      if(nextB>U128||nextU>U128)fail("sum overflow");
      const oldG=groups.get(g)||{b:0n,u:0n};
      groups.set(g,{b:oldG.b+r.b,u:oldG.u+r.u});
      B=nextB;U=nextU;seen.add(key);quotes.set(qkey,{...old,used:old.used+r.b});
    }
    const groupPrices=[...groups.values()].map(g=>q8(g.u,g.b));
    const raw=B?q8(U,B):null, med=median(groupPrices);
    const publish=groups.size>=minGroups&&B>0n;
    return {state:{seen,quotes,lastEpoch:epoch},raw,med,publish,B,U,count:rows.length,groups:groups.size};
  }
  function row(i,operator,b,price,epoch=0){
    const u=b*BigInt(price)*1000000n/BTC;
    return {chain,asset,epoch,status:"settled",feeKnown:true,direction:"asset_buy",b,u,channel:"channel"+i,offerer:0,htlc:0n,quote:"quote"+i,signer:operator,operator,quoteMaxB:b};
  }
  const reg=new Map(Array.from({length:5},(_,i)=>["h"+i,"honest"+i]));
  const honest=Array.from({length:5},(_,i)=>row(i,"h"+i,BTC,60000));
  const bad=Array.from({length:6},(_,i)=>row(i+10,"a"+i,2n*BTC,72000));
  const shared=new Map([...reg,...bad.map(r=>[r.operator,"attacker"])]);
  const sybil=new Map([...reg,...bad.map((r,i)=>[r.operator,"sybil"+i])]);
  const result=[];
  function eq(a,b,msg){if(a!==b)fail("assert "+msg+": "+a+" != "+b);}
  function test(name,f){f();result.push(name);}
  function rejects(name,f,reason){test(name,()=>{let err;try{f()}catch(e){err=e.message}eq(err,reason,name)});}
  const baseline=window(clean(),0,honest,reg);
  const attack=window(clean(),0,[...honest,...bad],shared);
  const sybilAttack=window(clean(),0,[...honest,...bad],sybil);
  const withheld=window(clean(),0,[honest[0],honest[1],...bad],shared);
  test("exact unit normalization",()=>eq(baseline.raw,60000n*100000000n,"baseline"));
  test("q8 half-up tie",()=>eq(q8(1n,2n*SCALE),1n,"tie"));
  test("weighted aggregate",()=>eq(attack.raw,6847058823529n,"raw"));
  test("equal-group median under honest majority",()=>eq(attack.med,baseline.raw,"robust"));
  test("sybil attack preserves raw and breaks group median",()=>{eq(sybilAttack.raw,attack.raw,"raw");eq(sybilAttack.med,72000n*100000000n,"sybil");});
  test("withholding fails group floor",()=>eq(withheld.publish,false,"availability"));
  test("empty window unavailable",()=>{const e=window(clean(),0,[],reg);eq(e.raw,null,"raw");eq(e.publish,false,"empty");});
  test("row order invariance",()=>{const x=window(clean(),0,[...honest,...bad].reverse(),shared);eq(x.raw,attack.raw,"raw");eq(x.med,attack.med,"med");});
  rejects("duplicate same report",()=>window(clean(),0,[honest[0],honest[0]],reg),"duplicate");
  rejects("duplicate alternate reporter",()=>window(clean(),0,[honest[0],{...honest[0],operator:"h1"}],reg),"duplicate");
  rejects("duplicate across epochs",()=>window(baseline.state,1,[{...honest[0],epoch:1}],reg),"duplicate");
  rejects("wrong asset",()=>window(clean(),0,[{...honest[0],asset:"other"}],reg),"domain/epoch");
  rejects("wrong chain",()=>window(clean(),0,[{...honest[0],chain:"other"}],reg),"domain/epoch");
  rejects("wrong epoch",()=>window(clean(),0,[{...honest[0],epoch:1}],reg),"domain/epoch");
  rejects("epoch gap",()=>window(clean(),2,[],reg),"epoch order");
  rejects("failed HTLC",()=>window(clean(),0,[{...honest[0],status:"failed"}],reg),"eligibility");
  rejects("unknown fee attribution",()=>window(clean(),0,[{...honest[0],feeKnown:false}],reg),"eligibility");
  rejects("unknown group",()=>window(clean(),0,[{...honest[0],operator:"unknown"}],reg),"registry");
  rejects("zero amount",()=>window(clean(),0,[{...honest[0],b:0n}],reg),"amount range");
  rejects("out-of-range amount",()=>window(clean(),0,[{...honest[0],u:U64+1n}],reg),"amount range");
  test("quote split across epochs",()=>{
    const part={...honest[0],quoteMaxB:2n*BTC};
    const a=window(clean(),0,[part],reg);
    const next={...part,epoch:1,htlc:1n};
    const b=window(a.state,1,[next],reg);
    eq(b.state.quotes.get(JSON.stringify([part.signer,part.quote])).used,2n*BTC,"quote sum");
    try{window(b.state,2,[{...next,epoch:2,htlc:2n}],reg);fail("accepted overfill")}catch(e){eq(e.message,"quote cap","overfill")}
  });
  rejects("quote cap altered",()=>window(clean(),0,[{...honest[0],quoteMaxB:2n*BTC},{...honest[0],htlc:1n,quoteMaxB:3n*BTC}],reg),"quote cap");
  test("atomic batch rejection",()=>{
    const prev=clean();try{window(prev,0,[honest[0],honest[0]],reg);fail("accepted duplicate")}catch(e){eq(e.message,"duplicate","batch")}
    eq(prev.seen.size,0,"unchanged seen");eq(prev.quotes.size,0,"unchanged quote");eq(prev.lastEpoch,-1,"unchanged epoch");
  });
  const summarize=x=>({base_msat:x.B.toString(),asset_atoms:x.U.toString(),raw_q8:x.raw===null?null:format(x.raw),group_median_q8:x.med===null?null:format(x.med),groups:x.groups,receipt_count:x.count,group_floor_pass:x.publish});
  return {stage:"TAP-VWAP-01A",scope:"Synthetic normalization/arithmetic only; all receipt authenticity is assumed by fixtures. Group floor alone is not production publication authorization.",tests:result,scenarios:{baseline:summarize(baseline),single_control_group:summarize(attack),sybil_misregistration:summarize(sybilAttack),withheld_honest_groups:summarize(withheld)}};
}
console.log(JSON.stringify(run(), null, 2));
