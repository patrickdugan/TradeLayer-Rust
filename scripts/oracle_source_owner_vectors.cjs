"use strict";
// Complete, unchanged pinned oracle.js. The adapter clones NeDB documents and
// records exact calls; insurance.liquidate is the explicit external boundary.
const fs=require("node:fs"),path=require("node:path"),vm=require("node:vm"),crypto=require("node:crypto"),assert=require("node:assert/strict");
const {createRequire}=require("node:module");
const {readPinnedSource,loadBigNumber}=require("./legacy_numeric_oracle.cjs");
const root=path.resolve(__dirname,".."),flag=process.argv.indexOf("--module-root");
const external=createRequire(path.join(flag<0?(process.env.TRADELAYER_JS_ROOT||path.resolve(root,"../../../../tradelayer.js")):process.argv[flag+1],"package.json"));
const source=readPinnedSource(path.join(root,"fixtures/legacy_oracle_source.txt"),"70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d");
const bridge=readPinnedSource(path.join(root,"fixtures/legacy_oracle_bridge_source.txt"),"954a7fbf30b66e52b587a2d7108e080c41da812acff77ccd457c6e547cb1587f");
const BigNumber=loadBigNumber(),secp=external("tiny-secp256k1"),clone=x=>structuredClone(x),quiet={log(){},error(){},warn(){}};
function diagnostic(x){
  if(x===undefined)return["undefined"];if(x===null)return["null"];
  if(typeof x==="boolean")return["boolean",x];if(typeof x==="string")return["string",x];
  if(typeof x==="number"){const b=Buffer.alloc(8);b.writeDoubleBE(x);return["number",Number.isNaN(x)?"NaN":x===Infinity?"Infinity":x===-Infinity?"-Infinity":b.toString("hex")];}
  if(Array.isArray(x))return["array",Array.from(x,diagnostic)];
  if(typeof x==="object")return["object",Object.keys(x).map(k=>[k,diagnostic(x[k])])];
  throw Error("Outside SourceValue domain: "+typeof x);
}
const hex=x=>Buffer.from(JSON.stringify(diagnostic(x))).toString("hex");
function matches(row,q){return Object.entries(q).every(([k,v])=>v&&typeof v==="object"?Object.entries(v).every(([op,v])=>op==="$gte"?row[k]>=v:op==="$lte"?row[k]<=v:false):row[k]===v);}
let Oracle,dbs,trace,dependencies,failures,insurance;
function dbcall(collection,operation,query,update,options){
  trace.push({collection,operation,query:clone(query),update:clone(update),options:clone(options)});
  const i=failures.findIndex(f=>f.collection===collection&&f.operation===operation);
  if(i>=0){if(failures[i].skip===0){const f=failures.splice(i,1)[0],e=Error(f.message);e.name=f.name;throw e;}failures[i].skip--;}
}
class Database{
  constructor(name){this.name=name;this.rows=[];}
  async findAsync(q){dbcall(this.name,"findAsync",q);return clone(this.rows.filter(r=>matches(r,q)));}
  async findOneAsync(q){dbcall(this.name,"findOneAsync",q);return clone(this.rows.find(r=>matches(r,q))||null);}
  async insertAsync(doc){dbcall(this.name,"insertAsync",doc);if(this.rows.some(r=>r._id===doc._id))throw Error("duplicate ID");this.rows.push(clone(doc));return clone(doc);}
  async updateAsync(q,u,o={}){dbcall(this.name,"updateAsync",q,u,o);const i=this.rows.findIndex(r=>matches(r,q));
    if(i<0&&!o.upsert)return 0;const doc=u.$set?{...(i<0?q:this.rows[i]),...clone(u.$set)}:clone(u);
    if(i<0)this.rows.push(doc);else this.rows[i]=doc;return i<0?0:1;}
}
function reset(){
  trace=[];dependencies=[];failures=[];insurance="return";dbs={oracleList:new Database("oracleList"),oracleData:new Database("oracleData")};
  const bridgeModule={exports:{}};vm.runInNewContext(bridge,{module:bridgeModule,exports:bridgeModule.exports,Buffer,require(n){if(n==="crypto")return crypto;if(n==="tiny-secp256k1")return secp;throw Error("Unexpected "+n);}},{filename:"pinned/dlcOracleBridge.js",timeout:2000});
  const module={exports:{}};vm.runInNewContext(source,{module,exports:module.exports,console:quiet,require(n){
    if(n==="./db")return{getDatabase:async name=>{dbcall(name,"getDatabase");return dbs[name];}};
    if(n==="crypto")return crypto;if(n==="bignumber.js")return BigNumber;if(n==="./dlcOracleBridge.js")return bridgeModule.exports;
    if(n==="./insurance.js")return{liquidate:async(...args)=>{dependencies.push({target:"insurance.liquidate",arguments:clone(args)});if(insurance==="throw")throw Error("injected insurance rejection");}};
    throw Error("Unexpected import "+n);
  }},{filename:"pinned/oracle.js",timeout:2000});Oracle=module.exports;
}
function state(){return{oracleList:dbs.oracleList.rows,oracleData:dbs.oracleData.rows,oracles:Array.from(Oracle.getInstance().oracles),lastOracleUpdateBlock:Array.from(Oracle.lastOracleUpdateBlock)};}
const rows=[],methods=new Set();let scenario="";
async function action(label,command){
  trace=[];dependencies=[];let outcome;
  try{let value;
    if(command.op==="reset")reset();
    else if(command.op==="import")dbs[command.collection].rows=clone(command.rows);
    else if(command.op==="failure")failures.push({...command.failure});
    else if(command.op==="insurance")insurance=command.mode;
    else if(command.op==="restart"){const persisted=clone([dbs.oracleList.rows,dbs.oracleData.rows]);reset();[dbs.oracleList.rows,dbs.oracleData.rows]=persisted;}
    else {assert.equal(command.op,"call");methods.add(command.method);const target=["addOracle","saveOracleData","loadOracleData"].includes(command.method)?Oracle.getInstance():Oracle;value=await target[command.method](...command.arguments);}
    outcome={returned:value};
  }catch(e){outcome={thrown:{name:e.name,message:e.message}};}
  rows.push(["V",scenario+"/"+label,hex(command),hex(outcome),hex(state()),hex(trace),hex(dependencies)].join("\t"));
}
const call=(label,method,...args)=>action(label,{op:"call",method,arguments:args});
const imported=(label,collection,rows)=>action(label,{op:"import",collection,rows});
const fail=(label,collection,operation,skip=0)=>action(label,{op:"failure",failure:{collection,operation,skip,name:"Error",message:"injected "+collection+" "+operation}});
const start=async name=>{scenario=name;await action("reset",{op:"reset"});};
async function main(){
  await start("metadata-full-lifecycle");
  await call("missing-info","getOracleInfo",1);await call("missing-admin","isAdmin","a",1);await call("missing-undefined-admin","isAdmin",undefined,1);await call("missing-verify","verifyAdmin",1,"a");
  await call("create","createOracle",{name:"BTC",ticker:"BTC",url:"url",backupAddress:"b",clearlists:[1,"2",{complex:[null,false]}],lag:"-0.5",adminAddress:"a"});
  await call("create-positional","createOracle","ETH","c");await call("all","getAllOracles");await call("next","getNextId");
  await call("backup","isAdmin","b",1);await call("backup-not-primary","verifyAdmin",1,"b");
  await call("update-unauthed","updateAdmin",1,"new",false);await call("new-admin","isAdmin","new",1);
  await call("backup-update-truthy","updateAdmin",1,{address:"nested"},"false");await call("info","getOracleInfo",1);
  await call("missing-update","updateAdmin",99,"x");await call("missing-close","closeOracle",99);
  await call("close","closeOracle",1);await call("closed-info","getOracleInfo",1);
  await call("closed-can-publish","publishData",1,"10",12,8,9,10);await call("refresh-drops-cache-price","getAllOracles");await call("info-after-refresh","getOracleInfo",1);
  await action("restart",{op:"restart"});await call("read-load","getOracleInfo",1);await call("persisted-price","getOraclePrice",1);

  await start("nested-imports-and-stale-cache");
  await imported("full-legacy-documents","oracleList",[{_id:"oracle-7",name:{type:"commodity",name:"old",adminAddress:"nested-admin",backupAddress:"nested-backup",whitelists:[1,2]},type:{kind:"legacy"},clearlists:[3],whitelists:[9],lag:-2,extra:{keep:[true,null]}},{_id:"oracle-9",id:"9",name:"stale",adminAddress:"stale-admin",data:{}}]);
  await call("next-db-fallback","getNextId");await call("load","load");await call("nested-primary","isAdmin","nested-admin",7);await call("nested-backup","isAdmin","nested-backup",7);await call("verify-ignores-nested","verifyAdmin",7,"nested-admin");
  await call("nested-data-preserved","publishData",7,100,101,99,100,1);await call("cached-info","getOracleInfo",7);
  await imported("erase-db","oracleList",[]);await call("stale-load","load");await call("stale-next","getNextId");await call("stale-cache-info","getOracleInfo",9);await call("db-admin-no-cache","isAdmin","stale-admin",9);await call("cache-admin","verifyAdmin",9,"stale-admin");
  await action("restart-drops-stale",{op:"restart"});await call("now-missing","getOracleInfo",9);
  await call("numeric-map-key","addOracle",12,{name:"raw",adminAddress:"numeric",_id:"oracle-30",id:30});await call("add-spread-key","getOracleInfo",30);await imported("erase-again","oracleList",[]);await call("numeric-cache-key-fails-split","getNextId");

  await start("allocation-coercion");
  await imported("mixed-ids","oracleList",[{_id:"oracle-6tail"},{_id:"oracle-3",id:null},{_id:"oracle-0x20"},{_id:"oracle-99",id:"bad"},{_id:"oracle-4",id:4.5},{_id:"oracle-100",id:Infinity}]);
  await call("number-fallback-no-parse-int","getNextId");await call("fractional-create","createOracle",{name:{nested:[true]},ticker:[],url:false,backupAddress:0,clearlists:"1,2",lag:null,adminAddress:{object:"address"}});
  await call("full-object-info","getOracleInfo",33);await call("lag-nan-default","createOracle",{lag:"x"});await call("array-config","createOracle",[]);await call("false-positional","createOracle",false,"fallback");
  await call("primitive-string-spread","addOracle","oracle-string","abc");await call("primitive-array-spread","addOracle","oracle-array",[1,{unknown:true}]);
  await imported("safe-integer-rounding","oracleList",[{_id:"oracle-huge",id:9007199254740992}]);await call("large-id-no-increment","getNextId");await call("large-create","createOracle","huge","a");await call("duplicate-still-cache-unchanged","createOracle","conflict","a");

  await start("publication-typed-keys-cache-and-history");
  await call("unknown-publish","publishData",5,"100","120","80","99",10);await call("unknown-info","getOracleInfo",5);await call("same-block-skip","publishData",5,500,0,0,0,10);await call("older-skip","publishData",5,500,0,0,0,9);
  await call("string-id-distinct-watermark","publishData","5",500,1,2,3,10);await call("number-price","getOraclePrice",5);await call("string-price","getOraclePrice","5");
  await call("numeric-clamp","publishData",5,"200",777,888,999,11);await call("raw-save-upsert","saveOracleData",5,{price:"22.000000005",extra:{preserved:true}},11);
  await call("history","loadOracleData",5);await call("history-exact","loadOracleData",5,11,11);await call("history-reversed","loadOracleData",5,12,10);await call("twap-rounding","getTWAP",5,11,0);
  await call("unsorted-insert","saveOracleData",5,{price:30},1);await call("history-unsorted","loadOracleData",5,0,100);await call("latest-tie-first","getOraclePrice",5);
  await action("restart",{op:"restart"});await call("restart-price-only","getOracleInfo",5);await call("same-block-after-restart","publishData",5,25,undefined,undefined,undefined,11);await call("history-after-restart","loadOracleData",5);
  await call("undefined-block","publishData",5,50);await call("nan-watermark","publishData",5,60,null,false,[],NaN);await call("nan-continues","publishData",5,70,null,false,[],12);

  await start("price-filter-and-twap-nonprices");
  const prices=[undefined,"bad",Infinity,NaN,null,false,"",[],["2"],"1.00000000000000000001",0,-10,"0.000047619047619047619047619047619"];
  for(let i=0;i<prices.length;i++){await call("save-"+i,"saveOracleData",1,{price:prices[i]},i);await call("price-"+i,"getOraclePrice",1);await call("twap-"+i,"getTWAP",1,i,0);}
  await call("publish-negative-limits","publishData",1,5,undefined,undefined,undefined,20);
  await call("stake-contaminates-twap","recordStake",1,"s",2,1,21);await call("twap-catches-missing-data","getTWAP",1,21,0);await call("history-filters-type","loadOracleData",1,20,21);
  await imported("block-height-string-order","oracleData",[{_id:"a",oracleId:2,data:{price:2},blockHeight:"9"},{_id:"b",oracleId:2,data:{price:3},blockHeight:"10"},{_id:"c",oracleId:2,data:{price:4},blockHeight:"9"}]);await call("lexicographic-latest-tie","getOraclePrice",2);

  await start("stake-fraud-relay");
  await call("stake-missing","getStake",1,"alice");await call("stake","recordStake",1,"alice",2,"1.123456789",1);await call("stake-truncation","recordStake",1,"alice",3,"0.000000009",2);await call("negative-stake","recordStake",1,"bob",2,"-1.123456789",2);
  await call("no-stake-fraud","applyFraudProof",1,"missing","c",1,"proof",3);await call("slash","applyFraudProof",1,"alice","c","0.2","proof",3);await call("negative-slash","applyFraudProof",1,"alice","c",-1,"neg",4);await call("cap","applyFraudProof",1,"alice","c",100,"max",5);await call("negative-current","applyFraudProof",1,"bob","c",10,"negative",5);
  await call("unicode-evidence","applyFraudProof",1,"alice","c",-1,"界".repeat(30),6);await call("prefix-collision","applyFraudProof",1,"alice","c",0,"界".repeat(30)+"different",6);
  await call("large-number-stake","recordStake",1,"large",2,"9007199254740993.12345678",6);await call("large-number-add","recordStake",1,"large",2,1,7);await call("nan-stake","recordStake",1,"nan",2,"bad",7);await call("nan-fraud","applyFraudProof",1,"nan","c",1,"bad",8);
  await call("relay","relayTradeLayerState",1,"a","state","s1","dlc",10,JSON.stringify({signatureHex:" AABB ",stateHash:"fallback"}));
  await call("relay-reuse","relayTradeLayerState",1,"b","state","s1","dlc",11,JSON.stringify({signatureHex:"aabb"}));await call("relay-conflict","relayTradeLayerState",1,"b","state","s2","dlc",12,JSON.stringify({signatureHex:"AABB"}));
  await call("relay-fallback-state","relayTradeLayerState",1,"b","state","","d2",12,JSON.stringify({signatureHex:"ccdd",stateHash:"from-blob"}));await call("relay-fallback-reuse","relayTradeLayerState",1,"b","state","from-blob","d2",12,JSON.stringify({signatureHex:"CCDD"}));
  await call("relay-malformed","relayTradeLayerState",1,"b","state",null,null,13,"not-json");await call("relay-object-coercion","relayTradeLayerState",1,"b","state",{},false,14,{signatureHex:"ignored"});
  await call("relay-base64","relayTradeLayerState",1,"b","state","s3","dlc",15,"b64:"+Buffer.from(JSON.stringify({signatureHex:"eeff"})).toString("base64"));

  await start("failures-partial-writes");
  await fail("create-insert-fault","oracleList","insertAsync");await call("create-failed","createOracle","name","a");await call("create-retry","createOracle","name","a");
  await fail("add-insert-fault","oracleList","insertAsync");await call("add-keeps-cache","addOracle","oracle-2",{id:2,name:"cached",adminAddress:"b"});await call("add-cache-visible","getOracleInfo",2);await call("add-db-admin-absent","isAdmin","b",2);
  await call("add-duplicate-poisons-cache","addOracle","oracle-1",{id:1,name:"poison",adminAddress:"evil"});await call("verify-poisoned-cache","verifyAdmin",1,"evil");await call("db-admin-unchanged","isAdmin","a",1);
  await fail("load-swallowed","oracleList","findAsync");await call("load-caught","load");await fail("all-load-swallowed","oracleList","getDatabase");await call("all-returns-cache-after-failure","getAllOracles");
  await fail("price-read-fails","oracleData","findAsync");await call("publish-watermark-poison","publishData",1,100,120,80,99,10);await call("retry-skipped","publishData",1,100,120,80,99,10);await call("no-price","getOraclePrice",1);
  await fail("save-fails","oracleData","updateAsync");await call("publish-cache-before-write","publishData",1,100,120,80,99,11);await call("cache-info-after-fail","getOracleInfo",1);await call("failed-save-still-no-price","getOraclePrice",1);await call("retry-skips","publishData",1,100,120,80,99,11);
  await fail("update-fails","oracleList","updateAsync");await call("update-no-cache-effect","updateAdmin",1,"not-written");await fail("twap-read-fault","oracleData","findAsync");await call("twap-swallowed","getTWAP",1,11,1);await fail("history-read-fault","oracleData","findAsync");await call("history-rethrows","loadOracleData",1);
  await call("stake-for-partial","recordStake",1,"staker",2,10,1);await fail("second-fraud-write-fault","oracleData","updateAsync",1);await call("fraud-stake-written-no-proof","applyFraudProof",1,"staker","c",3,"proof",2);await call("stake-remains-reduced","getStake",1,"staker");
  await fail("second-relay-write-fault","oracleData","updateAsync",1);await call("signature-written-before-relay-fault","relayTradeLayerState",1,"a","state","state","dlc",1,JSON.stringify({signatureHex:"00aa"}));await call("failed-relay-signature-still-protects","relayTradeLayerState",1,"a","state","other","dlc",2,JSON.stringify({signatureHex:"00aa"}));
  await action("insurance-reject",{op:"insurance",mode:"throw"});await call("close-stays-closed-after-rejection","closeOracle",1);await call("closed-info","getOracleInfo",1);await action("insurance-return",{op:"insurance",mode:"return"});await call("close-again","closeOracle",1);
  await action("restart",{op:"restart"});await call("retry-publish-after-restart","publishData",1,100,120,80,99,10);await call("load-removes-cached-data","getAllOracles");

  await start("circuit-boundaries");await call("create","createOracle","bounds","a");
  for(let i=0;i<40;i++){const baseline=new BigNumber(((i*7919+104729)%999983)+1).dividedBy("10000").times(i%7===0?-1:1),boundary=baseline.times(i%2?"1.05":"0.95").decimalPlaces(4),delta=new BigNumber(i%3===0?"-1e-21":i%3===1?"0":"1e-21");await call("save-"+i,"saveOracleData",1,{price:baseline.toFixed()},i*2+1);await call("publish-"+i,"publishData",1,boundary.plus(delta).toFixed(),undefined,undefined,undefined,i*2+2);}
  await start("radix-and-number-runtime-errors");
  for(const [i,p] of ["0x10","-0x10","+0b1.1","0x.8","0x1.","0x1.0000000000001","0b0.00000000000000000000000000000000000000001"].entries()) {await call("radix-stake-"+i,"recordStake",1,"radix-"+i,2,p,i);await call("radix-save-"+i,"saveOracleData",1,{price:p},i);await call("radix-publish-"+i,"publishData",1,100,undefined,undefined,undefined,i+100);}
  await call("price-conversion-throws","saveOracleData",2,{price:{toString:1}},1);await call("price-filter-rethrows","getOraclePrice",2);await call("twap-catches-price-conversion","getTWAP",2,1,0);
  await call("first-object-block","publishData",3,1,undefined,undefined,undefined,{toString:1});await call("watermark-comparison-throws","publishData",3,2,undefined,undefined,undefined,2);
  await imported("two-uncoercible-blocks","oracleData",[{_id:"a",oracleId:4,data:{price:1},blockHeight:{toString:1}},{_id:"b",oracleId:4,data:{price:2},blockHeight:2}]);await call("latest-comparison-rethrows","getOraclePrice",4);await call("range-query-caught","getTWAP",4,3,3);
  assert.equal(methods.size,19);
  const output="# pinned oracle.js dca0888b827c5bc28fa9189bc9e1d507f5c5670c; full documents, singleton, typed watermarks, real BigNumber9.3.1 and bridge parser; insurance callback boundary\n"+rows.join("\n")+"\n";
  const filename=path.join(root,"fixtures/oracle_source_owner_vectors.tsv");
  if(process.argv.includes("--check"))assert.equal(fs.readFileSync(filename,"utf8").replace(/\r\n/g,"\n"),output);else fs.writeFileSync(filename,output);
  console.log(JSON.stringify({vectors:rows.length,methods:[...methods].sort(),sha256:crypto.createHash("sha256").update(output).digest("hex")}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
