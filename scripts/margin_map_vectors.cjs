#!/usr/bin/env node
"use strict";

// Execute the full original class, not a rewritten arithmetic oracle. The DB,
// contract registry, tally, and UUID are explicit deterministic adapters only.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const HASH = "ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636";
const args=process.argv.slice(2);
const sourceArg=args.indexOf("--source");
const source=readPinnedSource(sourceArg<0?path.join(fixtures,"legacy_margin_map_source.txt"):args[sourceArg+1],HASH);
const BigNumber=loadBigNumber();
const optionModule={exports:{}};
vm.runInNewContext(readPinnedSource(path.join(fixtures,"legacy_options_source.txt"),
  "02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703"),
  {module:optionModule,exports:optionModule.exports,require:name=>{assert.equal(name,"bignumber.js");return BigNumber;}},
  {filename:"pinned-options.js",timeout:1000});
let serial=0;
const databases=new Map();
const clone=x=>JSON.parse(JSON.stringify(x));
const db={async getDatabase(name){
  if(!databases.has(name))databases.set(name,new Map());
  const rows=databases.get(name);
  return {
    async findOneAsync(query){return rows.has(query._id)?clone(rows.get(query._id)):null;},
    async findAsync(){return [...rows.values()].map(clone);},
    async insertAsync(doc){rows.set(doc._id,clone(doc));},
    async updateAsync(query,update){
      rows.set(query._id,clone(update.$set?{...(rows.get(query._id)||{}),...update.$set,_id:query._id}:update));
    }
  };
}};
const registry={async getContractInfo(){return {notionalValue:1,collateralPropertyId:2};},
  async getInitialMargin(){return 2;},async getCollateralId(){return 2;},setModFlag(){}};
const tally={async getTally(){return {available:10,reserved:0};},
  async hasSufficientMargin(){return {hasSufficient:true,shortfall:0};},async updateBalance(){}};
const moduleBox={exports:{}};
vm.runInNewContext(source,{module:moduleBox,exports:moduleBox.exports,
  console:{log(){},warn(){},error(){}},require(name){
    if(name==="bignumber.js")return BigNumber;
    if(name==="uuid")return {v4:()=>`source-${++serial}`};
    if(name==="./db.js")return db;
    if(name==="./options.js")return optionModule.exports;
    if(name==="./contractRegistry.js")return registry;
    if(name==="./tally.js")return tally;
    throw Error(`unexpected dependency ${name}`);
  }},{filename:"pinned-marginMap.js",timeout:1000});
const MarginMap=moduleBox.exports;
const n=x=>x===null||x===undefined?"null":String(x);
const shape=p=>[p.contracts,p.margin,p.unrealizedPNL,p.avgPrice,p.liqPrice,p.bankruptcyPrice,p.realizedPNL||0,p.iouClaim||0].map(n).join(",");
const pos=(contracts=0,margin=0,avgPrice=0,unrealizedPNL=0,address="alice")=>
  ({address,contracts,margin,avgPrice,unrealizedPNL,liqPrice:null,bankruptcyPrice:null});
const rows=[];
const quirks=[];
const methods=Object.getOwnPropertyNames(MarginMap.prototype).filter(name=>name!=="constructor");
const add=(kind,name,input,output)=>rows.push([kind,name,input,output].join("\t"));
async function main(){
  const m=new MarginMap(1);
  for(const inverse of [false,true])for(const count of ["1","-3","2.5"])for(const entry of ["7","100"]){
    const exit=String(Number(entry)+3.125);
    for(const notional of ["1","2.5","0"]){
      add("estimate",`estimate-${rows.length}`,[count,entry,exit,inverse,notional].join(","),n(m.estimatePNL(count,entry,exit,inverse,notional)));
      for(const long of [false,true])add("price",`price-${rows.length}`,[count,entry,exit,inverse,notional,long].join(","),n(m.calcPriceDelta(count,entry,exit,inverse,notional,long)));
    }
    add("requirement",`requirement-${rows.length}`,[count,entry,inverse].join(","),n(m.calculateMarginRequirement(count,entry,inverse)));
  }
  for(const inverse of [false,true])for(const long of [false,true])for(const margin of ["0.1","2","100"]){
    const a=["1",margin,long?"2":"-2","1",inverse,long,"100","0.25"];
    const q=m.calculateLiquidationPrice(...a);
    add("liquidation",`liquidation-${rows.length}`,a.join(","),[q.bankruptcyPrice,q.liquidationPrice].map(n).join(","));
  }
  for(const count of [-4,0,4])for(const sell of [false,true]){
    const p=pos(count,20,10);const amount="2.5",price="13";
    add("average",`average-${rows.length}`,[count,10,amount,price,sell].join(","),n(await m.updateAveragePrice(p,amount,price,1,sell)));
  }
  for(const [count,avg,amount,price,sell] of [[9007199254740992,1e16,"1",1e16,false],
    ["9007199254740993",1e16,"1",1e16,false],["0.29999999999999999",10,"0.1",13,false]]){
    add("average",`average-number-bridge-${rows.length}`,[count,avg,amount,price,sell].join(","),n(await m.updateAveragePrice(pos(count,20,avg),amount,price,1,sell)));
  }
  for(const count of ["1.0000000049999999999999","9007199254740993"]){
    add("maintenance",`maintenance-number-bridge-${rows.length}`,count,n(await m.computeMaintenanceMarginRequirement(1,pos(count,20,10))));
  }
  for(const a of [["0","1.23456789","9007199254740993","0.000000000000001",false,true,"100","0"],
    ["0","0.1","1.0000000049999999999999","1",false,false,"100","0"]]){
    const q=m.calculateLiquidationPrice(...a);
    add("liquidation",`liquidation-number-bridge-${rows.length}`,a.join(","),[q.bankruptcyPrice,q.liquidationPrice].map(n).join(","));
  }
  const initial=[
    [0,20,0,0,"3","100",false,0,0],
    [3,20,100,1,"2","110",false,0,0],
    [5,20,104,1,"2","90",true,2,0],
    [3,20,104,1,"3","110",true,3,0],
    [3,20,104,1,"5","110",true,3,2],
    [-3,20,100,-2,"2","90",true,0,0],
    [-3,20,100,-2,"1","110",false,0,0],
    [-3,20,100,-2,"5","110",false,3,2],
  ];
  for(const [count,margin,avg,upnl,amount,px,sell,close,flip] of initial){
    const mm=new MarginMap(1),p=pos(count,margin,avg,upnl);mm.margins.set("alice",p);
    const out=await mm.updateContractBalances("alice",amount,px,sell,p,false,close,flip,1,false,10,false);
    add("trade",`trade-${rows.length}`,[count,margin,avg,upnl,amount,px,sell,close,flip].join(","),shape(out));
  }
  for(const [count,margin,avg,upnl] of [[4,20,10,7],[-4,20,10,-7],[0,0,0,0]]){
    for(const [operation,value] of [["initial","1.123456789"],["update","-2.000000005"],["fee","0.333333335"],["clear","2.123456789"]]){
      const mm=new MarginMap(1),p=pos(count,margin,avg,upnl);mm.margins.set("alice",p);
      let out;
      if(operation==="initial")out=await mm.setInitialMargin("alice",1,value,10,p);
      if(operation==="update")out=await mm.updateMargin("alice",1,value,10,p);
      if(operation==="fee")out=await mm.feeMarginReduce("alice",p,value,1,10);
      if(operation==="clear")out=await mm.clear(p,"alice",value,12,1,10,14,11);
      add(operation,`${operation}-${rows.length}`,[count,margin,avg,upnl,value].join(","),shape(out));
    }
    for(const feeDebit of [false,true])for(const fee of ["0","1.5","25"]){
      const mm=new MarginMap(1),p=pos(count,margin,avg,upnl);mm.margins.set("alice",p);
      const reduction=await mm.reduceMargin(p,2,2,1,"alice",false,feeDebit,fee,10);
      add("reduce",`reduce-${rows.length}`,[count,margin,avg,upnl,feeDebit,fee].join(","),n(reduction)+";"+shape(p));
    }
    for(const inverse of [false,true])for(const buy of [false,true]){
      const mm=new MarginMap(1),p=pos(count,margin,10,upnl);mm.margins.set("alice",p);
      const out=await mm.realizePnl("alice",2,13,10,inverse,2.5,p,buy,1,10);
      add("realize",`realize-${rows.length}`,[count,margin,10,upnl,inverse,buy].join(","),shape(out));
    }
    for(const inverse of [false,true]){
      const mm=new MarginMap(1),p=pos(count,margin,10,upnl);mm.margins.set("alice",p);
      const out=await mm.settlePNL("alice",2,13,11,1,10,inverse,2.5);
      add("settle",`settle-${rows.length}`,[count,margin,10,upnl,inverse].join(","),n(out)+";"+shape(p));
    }
  }
  const om=new MarginMap(1);
  for(const [delta,px,credit] of [[2,10,1],[3,15,0],[-2,20,0],[-3,8,0],[-2,12,0],[5,9,2],[-1,8,0]]){
    const result=await om.applyOptionTrade("alice","1-100-C-20",delta,px,10,credit);
    add("option",`option-${rows.length}`,[delta,px,credit].join(","),[result.contracts,result.avgPrice,result.margin,result.closedQty,result.flipQty,om.optionOI.size].map(n).join(","));
  }
  // Actual source quirks: assertions fail if future adapters silently repair.
  assert.equal(om.optionOI.size,0,"extra this prevents automatic expiry indexing");
  quirks.push(["option-index-extra-this",0]);
  const im=new MarginMap(1);im.margins.set("alice",{...pos(1,20,10),iouClaim:2});await im.saveMarginMap(10);
  await im.applyIouClaimDelta("alice","bob",5,0,3,1);
  assert.equal(im.margins.get("alice").iouClaim,2);quirks.push(["iou-claim-projection-no-persistence",2]);
  const mm=new MarginMap(1);mm.margins.set("alice",pos(-4,20,10));
  const mint=await mm.moveMarginAndContractsForMint("alice",2,1,-2,5,10);
  quirks.push(["mint-first",JSON.stringify(mint),shape(mm.margins.get("alice")),shape(mm.margins.get("s-2-1"))]);
  async function failure(name,operation){try{await operation();throw Error("unexpected success");}catch(error){
    assert.notEqual(error.message,"unexpected success");quirks.push([name,error.name,String(error.message)]);
  }}
  await failure("mint-existing",()=>mm.moveMarginAndContractsForMint("alice",2,1,-1,1,10));
  await failure("redeem",()=>mm.moveMarginAndContractsForRedeem("alice",2,1,1,{contracts:-2,margin:5,outstanding:10,available:0},1,2,10,10));
  await failure("dynamic-undefined-registry",()=>mm.dynamicDeleverage(1,true,1,10));
  await failure("market-undefined-registry",()=>mm.getMarketPrice({id:1}));
  await failure("needs-liquidation-map-object-mismatch",()=>mm.needsLiquidation({id:1,marketPrice:10}));
  await failure("settle-missing-dereference",()=>mm.settlePNL("absent",1,10,10,1,10,false,1));
  // ADL executes the real reduceMargin + tally adapter, including duplicate logs.
  for(const inverse of [false,true])for(const mark of [8,12]){
    const am=new MarginMap(1);const cache=[pos(-3,20,10,0,"bob"),pos(-2,15,10,0,"carol"),pos(5,30,10,0,"liquidator")];
    cache[0].lastMark=10;cache[1].lastMark=11;cache.forEach(p=>am.margins.set(p.address,p));
    const result=await am.simpleDeleverage(cache,1,4,true,10,"liquidator",inverse,2.5,10,mark,2,7);
    add("adl",`adl-${rows.length}`,[inverse,mark].join(","),[result.totalDeleveraged,
      result.counterparties.map(c=>[c.address,c.matchSize,c.updatedPosition.contracts,c.updatedPosition.margin,c.updatedPosition.avgPrice].join(",")).join(";"),
      result.poolAssignments.map(c=>[c.address,c.poolShare].join(",")).join(";")].join("|"));
  }
  // Direct index helper is usable despite applyOptionTrade's call-site bug.
  const ix=new MarginMap(1);
  for(const [before,after,ticker] of [[0,1,"1-200-C-10"],[0,-1,"1-100-P-9"],[0,2,"1-200-P-8"],
    [0,1,"1-200-C-10"],[1,0,"1-200-C-10"],[1,0,"1-200-C-10"],[0,0,"1-300-C-10"]]){
    ix._touchExpiryIndex(before,after,ticker,optionModule.exports.parseTicker(ticker));
    add("index",`index-${rows.length}`,[before,after,ticker].join(","),[(await ix.getExpiringTickersUpTo(300)).join(";"),ix.optionOI.get(ticker)||0].join("|"));
  }
  for(const [count,base,ticker,qty] of [[2,80,"1-1-P-90",0.01],[-2,120,"1-1-C-110",0.01],
    [2,95,"1-1-P-90",1],[2,80,"1-1-C-100",1],[2,80,"invalid",1],[0,80,"1-1-P-90",1],
    [1,80,"1-100-P-abc",1],[-1,80,"1-100-C-abc",1]]){
    const pp=new MarginMap(1);pp.margins.set("alice",{...pos(count,10,100),options:{[ticker]:{contracts:qty}}});
    add("protection",`protection-${rows.length}`,[count,base,ticker,qty].join(","),n(await pp.calcLiquidationWithOptions("alice",base,100,999)));
  }
  for(const [count,margin,contracts,requested] of [[-4,20,-2,5],[-10,2,-1,5],[-2,2,-2,5],[4,10,-2,3]]){
    const mm=new MarginMap(1);mm.margins.set("alice",pos(count,margin,10));
    const out=await mm.moveMarginAndContractsForMint("alice",2,1,contracts,requested,10);
    add("mint",`mint-${rows.length}`,[count,margin,contracts,requested].join(","),
      [out.contracts,out.margin,out.excess].map(n).join(",")+"|"+shape(mm.margins.get("alice"))+"|"+shape(mm.margins.get("s-2-1")));
  }
  for(const [count,margin,available,amount] of [[2,10,0,1],[-2,0,2,1],[2,0,0,1],[0,10,0,1],[2,10,0,0],
    [2,10,0,"1.123456789012345678901"]]){
    const mm=new MarginMap(1),p={...pos(count,margin,10),available};
    const out=mm.generateLiquidationOrder(p,1,amount,10,100,new BigNumber("99.125"));
    add("order",`order-${rows.length}`,[count,margin,available,amount].join(","),out?[out.amount,out.price,out.sell,out.isLiq,out.isMarket].map(n).join(","):"null");
  }
  for(const query of ["BC1QABC","bc1qabc","Bc1qabc","absent"]){
    const mm=new MarginMap(1);mm.margins.set("BC1QABC",pos(2,10,100,0,"BC1QABC"));await mm.saveMarginMap(10);
    const out=await mm.getPositionForAddress(query,1);
    add("lookup",`lookup-${rows.length}`,query,[out.contracts,out.margin,out.unrealizedPNL].map(n).join(","));
  }
  for(const flatten of [false,true]){
    const mm=new MarginMap(1);const positions=[{...pos(0,100,0,0,"funded")},{...pos(0,0,0,0,"claim"),iouClaim:1},pos(2,10,100,0,"active")];
    await mm.mergePositions(positions,1,flatten);
    add("merge",`merge-${rows.length}`,String(flatten),[...mm.margins.keys()].join(","));
  }
  for(const [before,size] of [[0.3,0.1],[-0.3,0.1],[0.3,"0.100000000000000000001"]]){
    const mm=new MarginMap(1),pp=pos(before,20,10);mm.margins.set("alice",pp);
    const out=await mm.adjustDeleveraging([pp],0,"alice",1,size,false,10,10);
    add("adjust",`adjust-number-bridge-${rows.length}`,[before,size].join(","),shape(out));
  }
  quirks.push(["prototype-methods",methods.join(",")]);
  for(const [filename,body] of [["margin_map_vectors.tsv",rows.join("\n")],["margin_map_quirks.tsv",quirks.map(row=>row.join("\t")).join("\n")]]){
    const text="# Executed unmodified marginMap.js dca0888b827c5bc28fa9189bc9e1d507f5c5670c\n# source LF SHA256 "+HASH+"\n"+body+"\n";
    const target=path.join(fixtures,filename);
    if(args.includes("--write"))fs.writeFileSync(target,text);else assert.equal(fs.readFileSync(target,"utf8"),text,filename);
  }
  console.log(`Pinned marginMap source verified: ${rows.length} calculation/state/ADL vectors, ${quirks.length} source characterizations, ${methods.length} instance methods`);
}
main().catch(error=>{console.error(error);process.exitCode=1;});
