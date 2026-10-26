#!/usr/bin/env node
"use strict";

// Execute complete, unmodified reOrg.js. The only dependency is a deterministic
// litecoin.Client-shaped RPC adapter; console calls are recorded as source output.
const assert=require("node:assert/strict");
const fs=require("node:fs");
const path=require("node:path");
const vm=require("node:vm");
const {fixtures,readPinnedSource}=require("./legacy_numeric_oracle.cjs");
const SHA="3e4cabb0dc560fd8c601b72e05f0e698ce02adc646eaef241189fd192d906a82";
const args=process.argv.slice(2),sourceArg=args.indexOf("--source");
const source=readPinnedSource(sourceArg<0?path.join(fixtures,"legacy_reorg_source.txt"):args[sourceArg+1],SHA);
const hex=value=>Buffer.from(String(value),"utf8").toString("hex");
const pack=values=>values.length?values.join(";"):"-";
const ok=(hash,height=10,reportedHash=hash)=>[hash,{height,hash:reportedHash}];
const bad=(hash,message)=>[hash,{error:message}];
const tip=(status,hash)=>({status,hash});
const cases=[
  {name:"constructor-only",method:"construct"},
  {name:"direct-success",method:"get",request:"a",blocks:[ok("a")]},
  {name:"direct-error",method:"get",request:"missing",blocks:[bad("missing","block not available")]},
  {name:"direct-empty-hash",method:"get",request:"",blocks:[ok("",0,"")]},
  {name:"direct-reported-mismatch",method:"get",request:"a",blocks:[ok("a",99,"unrelated")]},
  {name:"empty-tip-list"},
  {name:"ignored-statuses",tips:[tip("active","a"),tip("invalid","b"),tip("valid-headers","c"),tip("","d")]},
  {name:"case-sensitive-status",tips:[tip("VALID-FORK","a"),tip("Headers-Only","b"),tip("valid-fork ","c")]},
  {name:"one-valid-fork",tips:[tip("valid-fork","a")],blocks:[ok("a",700000)]},
  {name:"one-headers-only",tips:[tip("headers-only","b")],blocks:[ok("b",800000)]},
  {name:"rpc-order-not-height-order",tips:[tip("valid-fork","high"),tip("active","active"),tip("headers-only","low"),tip("invalid","invalid"),tip("valid-fork","middle")],
    blocks:[ok("high",100),ok("low",3),ok("middle",50)]},
  {name:"duplicate-candidates",tips:[tip("valid-fork","same"),tip("headers-only","same")],blocks:[ok("same",1)]},
  {name:"first-block-aborts-rest",tips:[tip("headers-only","missing"),tip("valid-fork","later")],blocks:[bad("missing","pruned or headers only"),ok("later",20)]},
  {name:"partial-prefix-before-error",tips:[tip("valid-fork","first"),tip("headers-only","missing"),tip("valid-fork","last")],blocks:[ok("first",2),bad("missing","disk read failed"),ok("last",3)]},
  {name:"tips-error-swallowed",tipsError:"getchaintips unavailable"},
  {name:"empty-error-message",tips:[tip("valid-fork","a")],blocks:[bad("a","")]},
  {name:"reported-hash-not-compared",tips:[tip("valid-fork","requested")],blocks:[ok("requested",100,"different")]},
  {name:"unicode-and-control-metadata",tips:[tip("valid-fork","☃")],blocks:[ok("☃",1,"returned\thash\nline")]},
  {name:"max-safe-height",tips:[tip("valid-fork","x")],blocks:[ok("x",9007199254740991)]},
  {name:"empty-tip-hash",tips:[tip("headers-only","")],blocks:[ok("",0,"")]},
];
async function run(scenario){
  const trace=[],logs=[],returned=[],blocks=new Map(scenario.blocks||[]);
  const config={tag:scenario.name,host:"caller-owned",port:1234};
  class Client {
    constructor(received){assert.equal(received,config);trace.push("construct");}
    async cmd(method){
      assert.equal(method,"getchaintips");trace.push("tips");
      if(Object.hasOwn(scenario,"tipsError"))throw Error(scenario.tipsError);
      return scenario.tips||[];
    }
    async getBlock(hash){
      trace.push("block:"+hex(hash));assert.ok(blocks.has(hash),`unexpected block fetch ${hash}`);
      const block=blocks.get(hash);if(Object.hasOwn(block,"error"))throw Error(block.error);
      returned.push([hex(hash),block.height,hex(block.hash)].join(":"));return block;
    }
  }
  const module={exports:{}};
  vm.runInNewContext(source,{module,exports:module.exports,
    require:name=>{assert.equal(name,"litecoin");return {Client};},
    console:{log:(...args)=>logs.push({level:"log",message:args.join(" ")}),error:(prefix,error)=>logs.push({level:"error",prefix,message:error.message})}},
    {filename:"pinned-reOrg.js",timeout:1000});
  const checker=new module.exports(config),method=scenario.method||"check";
  assert.equal(trace.length,1,"constructor performs no RPC");
  let outcome;
  if(method==="construct")outcome="constructed";
  else if(method==="get"){
    try{const out=await checker.getBlock(scenario.request);assert.equal(out,blocks.get(scenario.request));outcome="ok";}
    catch(error){outcome="throw:"+hex(error.message);}
  }else{
    assert.equal(await checker.checkReOrg(),undefined,"ordinary source return is always undefined");
    const failure=logs.findLast(log=>log.level==="error");
    outcome=failure?"swallowed:"+hex(failure.message):"undefined";
  }
  const tips=pack((scenario.tips||[]).map(t=>hex(t.status)+":"+hex(t.hash)));
  const blockSpec=pack((scenario.blocks||[]).map(([hash,block])=>Object.hasOwn(block,"error")
    ?hex(hash)+":err:"+hex(block.error):hex(hash)+":ok:"+block.height+":"+hex(block.hash)));
  return [scenario.name,method,tips,Object.hasOwn(scenario,"tipsError")?hex(scenario.tipsError):"-",blockSpec,
    method==="get"?hex(scenario.request):"-",pack(trace),pack(logs.map(log=>hex(log.level==="log"?log.message:log.prefix+" "+log.message))),pack(returned),outcome].join("\t");
}
async function main(){
  const rows=[];for(const scenario of cases)rows.push(await run(scenario));
  const result="# Executed unmodified reOrg.js dca0888b827c5bc28fa9189bc9e1d507f5c5670c\n# source LF SHA256 "+SHA+"\n"+
    "# name\tmethod\ttips_statushex:hashhex\ttips_errorhex_or_dash\tblocks\trequested_hashhex\tactual_RPC_trace\tactual_source_loghex\tactual_returned_blocks\tactual_outcome\n"+rows.join("\n")+"\n";
  const target=path.join(fixtures,"reorg_vectors.tsv");
  if(args.includes("--write"))fs.writeFileSync(target,result);else assert.equal(fs.readFileSync(target,"utf8"),result,"source observer drift");
  console.log(`reorg: ${rows.length} actual source scenarios; all three active methods exercised; SHA ${SHA}`);
}
main().catch(error=>{console.error(error);process.exitCode=1;});
