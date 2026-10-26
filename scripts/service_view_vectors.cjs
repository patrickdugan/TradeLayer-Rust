"use strict";
// Entire pinned interface.js, unchanged. Dependency objects below are explicit
// adapters, not claims that the legacy modules actually export these methods.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const stream = require("node:stream");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SHA = "d2887a94f2e6770194aa976b91a155fe534b14f96dc4ba99c91f00dcff8181bb";
const args = process.argv.slice(2);
const sourceArg = args.indexOf("--source");
const source = readPinnedSource(sourceArg < 0 ? path.join(fixtures,"legacy_interface_source.txt") : args[sourceArg+1],SHA);
const imports = ["vesting","persistence","orderbook","insurance","txIndex","reOrg","validity","txUtils","channels","tally","marginMap","property","contractRegistry","consensus","txEncoder","types","txDecoder","clearing"];
const scenes = [
  ["getConsensusHashForBlock",[23],"Consensus.getData",["consensusHash_23"]],
  ["getFeatureActivationStatus",[18],"TradeLayerManager.isTxTypeActive",[18]],
  ["getAllBalancesForAddress",["alice"],"TallyMap.getAddressBalances",["alice"]],
  ["getTotalTokens",[7],"TallyMap.totalTokens",[7]],
  ["getBalancesAcrossAllWallets",[],"WalletCache.getAllBalances",[]],
  ["isTransactionTypeActive",[18],"TradeLayerManager.isTxTypeActive",[18]],
  ["getAllActiveTransactionTypes",[],"TradeLayerManager.getActiveTransactionTypes",[]],
  ["getAddressesWithBalanceForProperty",[7],"TallyMap.getAddressesWithBalanceForProperty",[7]],
  ["getTransaction",["ab"],"TxIndex.getTransactionDetails",["ab"]],
  ["getProperty",[7],"PropertyManager.getPropertyDetails",[7]],
  ["listProperties",[],"PropertyManager.getAllProperties",[]],
  ["getGrants",[7],"PropertyManager.getPropertyGrants",[7]],
  ["getPayToToken",[7],"TxUtils.getPayToTokenTransactions",[7]],
  ["listBlockTransactions",[23],"BlockHistory.getBlockTransactions",[23]],
  ["listBlocksTransactions",[2,23],"BlockHistory.getTransactionsInRange",[2,23]],
  ["listPendingTransactions",[],"TxUtils.getPendingTransactions",[""]],
  ["updateWalletCache",[],"WalletCache.updateCache",[]],
  ["listOracles",[],"OracleManager.listAllOracles",[]],
  ["listWhitelists",[],"WhitelistManager.listAllWhitelists",[]],
  ["listVaultsBySyntheticProperty",[7],"VaultManager.listVaultsForProperty",[7]],
  ["getOptionsChainBySeriesId",[1],"OptionsChainManager.getOptionsChain",[1]],
  ["getInsuranceFundBalance",[1],"InsuranceFund.getBalanceForContract",[1]],
  ["getInsuranceFundPayoutHistory",[1,2,23],"InsuranceFund.getPayoutHistoryForContract",[1,2,23]],
  ["getAuditData",[23,1],"clearing.fetchAuditData",["contract-1-block-23"]],
];
const names={vesting:"TradeLayerManager",tally:"TallyMap",insurance:"InsuranceFund",txIndex:"TxIndex",txUtils:"TxUtils",property:"PropertyManager",consensus:"Consensus"};
function observed(value) {
  if(value===undefined)return {$undefined:true};
  return JSON.parse(JSON.stringify(value,(_k,v)=>typeof v==="number"&&!Number.isFinite(v)?{$number:String(v)}:v));
}
function create(options={}) {
  const trace=[],logs=[],files=[];
  const adapter=name=>new Proxy({}, {get(_target,method){return (...arguments_)=>{
    const target=`${name}.${String(method)}`; trace.push({target,arguments:observed(arguments_)});
    if(options.fail===target)throw new Error(`adapter failure: ${target}`);
    if(target==="TallyMap.load")return Promise.resolve();
    if(target==="TallyMap.getTallyMapData"){
      if(options.specialRows==="nonfinite")return [undefined,NaN,Infinity,-Infinity,-0];
      if(options.specialRows==="throwing-iterator")return {[Symbol.iterator](){return {next(){throw new Error("iterator failed");}};}};
      if(options.specialRows==="cyclic"){const row={};row.self=row;return [row];}
      if(options.specialRows==="bigint")return [1n];
      return options.rows??[{address:"alice",balances:{7:{available:1.25}}}];
    }
    if(target==="TradeLayerManager.isTxTypeActive")return options.active??true;
    if(target==="BlockChainInfo.getLatestBlockHeight")return 42;
    return {target,arguments:observed(arguments_)};
  };}});
  const context={module:{exports:{}},console:{log:(...a)=>logs.push(["log",...a.map(observed)]),error:(...a)=>logs.push(["error",...a.map(v=>v instanceof Error || (v&&v.name&&v.message)?{name:v.name,message:v.message}:observed(v))])},
    require(name){
      if(name==="fs")return {createWriteStream(filename){trace.push({target:"fs.createWriteStream",arguments:[filename]});const chunks=[];const file={filename,chunks};files.push(file);return new stream.Writable({write(chunk,_encoding,done){chunks.push(Buffer.from(chunk));done(options.writeError?new Error("write failed"):undefined);}});}};
      if(name==="stream")return stream;
      if(name==="util")return require("node:util");
      assert(name.startsWith("./")&&imports.includes(name.slice(2,-3)),`unexpected import ${name}`);
      if(options.emptyImports)return {};
      const key=name.slice(2,-3);return adapter(names[key]??key);
    }};
  context.exports=context.module.exports;
  if(options.globals)for(const name of ["WalletCache","BlockHistory","OracleManager","WhitelistManager","VaultManager","OptionsChainManager","BlockChainInfo"])context[name]=adapter(name);
  vm.runInNewContext(source,context,{filename:"pinned-interface.js",timeout:1000});
  const instance=new context.module.exports();
  if(options.globals)instance.clearing=adapter("clearing");
  return {instance,trace,logs,files};
}
async function invoke(method,arguments_,options={}) {
  const env=create(options);let status,value;
  try {value=observed(await env.instance[method](...arguments_));status="ok";}
  catch(error){status="throw";value={name:error.name,message:error.message};}
  return {status,value,trace:env.trace,logs:env.logs,files:env.files.map(f=>({filename:f.filename,hex:Buffer.concat(f.chunks).toString("hex")}))};
}
const rows=[`# Full unchanged interface.js dca0888b; LF SHA256 ${SHA}`,
  "# M name method argsJSONhex optionsJSONhex status resultJSONhex traceJSONhex logsJSONhex filesJSONhex",
  "# S name inputJSONhex exact-UTF8-stream-hex (source JSON.stringify item + LF)",
  "# I prototype-methodsJSONhex source-method-declaration-count (includes overwritten declaration)"];
const hx=value=>Buffer.from(JSON.stringify(value),"utf8").toString("hex");
async function add(name,method,arguments_,options){const result=await invoke(method,arguments_,options);rows.push(["M",name,method,hx(arguments_),hx(options),result.status,hx(result.value),hx(result.trace),hx(result.logs),hx(result.files)].join("\t"));return result;}
(async()=>{
  const inventory=Object.getOwnPropertyNames(Object.getPrototypeOf(create().instance));
  const declarations=[...source.matchAll(/^    (?:async )?([A-Za-z]+)\(/gm)].map(m=>m[1]);
  assert.equal(declarations.length,28);assert.equal(inventory.length,27);
  rows.push(["I",hx(inventory),declarations.length].join("\t"));
  for(const [method,arguments_,target,expected] of scenes){
    const result=await add(`injected-${method}`,method,arguments_,{globals:true});
    assert.equal(result.status,"ok",method);
    assert(result.trace.some(t=>t.target===target&&JSON.stringify(t.arguments)===JSON.stringify(expected)),target);
    await add(`missing-global-${method}`,method,arguments_,{});
    await add(`empty-import-${method}`,method,arguments_,{emptyImports:true});
  }
  const inactive=await add("inactive-feature","getFeatureActivationStatus",[999],{active:false});assert.equal(inactive.value.status,"inactive");
  await add("pending-explicit-empty","listPendingTransactions",[""],{});
  await add("pending-explicit-null","listPendingTransactions",[null],{});
  await add("coercive-consensus-key","getConsensusHashForBlock",[{}],{});
  await add("coercive-audit-key","getAuditData",[[2,3],null],{globals:true});
  const latest=await add("default-latest-payout","getInsuranceFundPayoutHistory",[1,2],{globals:true});assert.equal(latest.trace[0].target,"BlockChainInfo.getLatestBlockHeight");
  await add("null-latest-payout-missing-global","getInsuranceFundPayoutHistory",[1,2,null],{});
  await add("explicit-zero-payout","getInsuranceFundPayoutHistory",[1,2,0],{});
  await add("audit-missing-instance-clearing","getAuditData",[2,1],{});
  await add("audit-delegate-throws","getAuditData",[2,1],{globals:true,fail:"clearing.fetchAuditData"});
  for(const [name,input] of [["empty",[]],["objects",[{address:"alice",balances:{7:{available:1.25}}},[1,"two",null]]],["unicode",[{"é":"雪",quote:'"\\\n'},true,0,null]],["numeric-keys",[{"10":10,"2":2,z:0}]],["nested-empty",[{array:[],object:{}}]]]){
    const env=create();const chunks=[];for await(const chunk of env.instance.createReadableStreamFromIterable(input))chunks.push(Buffer.from(chunk));
    rows.push(["S",name,hx(input),Buffer.concat(chunks).toString("hex")||"-"].join("\t"));
  }
  await add("audit-success","JSONAuditTallyMap",[],{});
  const writeFailure=await add("audit-write-failure-swallowed","JSONAuditTallyMap",[],{writeError:true});assert.equal(writeFailure.status,"ok");assert(writeFailure.logs.some(l=>l[0]==="error"));
  const loadFailure=await add("audit-load-failure-escapes","JSONAuditTallyMap",[],{fail:"TallyMap.load"});assert.equal(loadFailure.status,"throw");assert.equal(loadFailure.files.length,0);
  const invalidRows=await add("audit-not-iterable-before-try","JSONAuditTallyMap",[],{rows:{a:1}});assert.equal(invalidRows.status,"throw");assert.equal(invalidRows.files.length,0);
  const nonfinite=await add("audit-undefined-nonfinite-number-coercions","JSONAuditTallyMap",[],{specialRows:"nonfinite"});assert.equal(nonfinite.status,"ok");assert.equal(Buffer.from(nonfinite.files[0].hex,"hex").toString(),"undefined\nnull\nnull\nnull\n0\n");
  for(const specialRows of ["throwing-iterator","cyclic","bigint"]){const result=await add(`audit-${specialRows}-stream-error-swallowed`,"JSONAuditTallyMap",[],{specialRows});assert.equal(result.status,"ok");assert(result.logs.some(l=>l[0]==="error"));}
  const text=rows.join("\n")+"\n";
  const filename=path.join(fixtures,"service_view_vectors.tsv");
  if(args.includes("--write"))fs.writeFileSync(filename,text);
  else assert.equal(fs.readFileSync(filename,"utf8").replace(/\r\n/g,"\n"),text,"regenerate with --write");
  console.log(`interface.js: ${inventory.length-1} active methods, ${rows.filter(r=>r.startsWith("M\t")).length} call scenes, ${rows.filter(r=>r.startsWith("S\t")).length} exact stream cases; SHA verified`);
})().catch(error=>{console.error(error);process.exitCode=1;});
