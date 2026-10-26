"use strict";
// Unchanged pinned module, clone-isolating typed-ID adapter, genuine installed
// bitcore-lib-ltc. No rewritten JS methods or mocked derived-address answers.
const fs=require("node:fs"),path=require("node:path"),vm=require("node:vm"),crypto=require("node:crypto"),assert=require("node:assert/strict");
const {createRequire}=require("node:module");
const here=__dirname,root=path.resolve(here,".."),flag=process.argv.indexOf("--module-root");
const moduleRoot=flag<0?(process.env.TRADELAYER_JS_ROOT||path.resolve(here,"../../../../../tradelayer.js")):process.argv[flag+1];
const external=createRequire(path.join(moduleRoot,"package.json"));
const fallback=process.argv.includes("--fallback-oracle");
const selectedLibrary=external(fallback?"bitcore-lib":"bitcore-lib-ltc");
const source=fs.readFileSync(path.join(root,"fixtures/legacy_clearlist_audit_reference.txt"),"utf8").replace(/\r\n/g,"\n");
const hash=crypto.createHash("sha256").update(source).digest("hex");
assert.equal(hash,"fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1");
const clone=x=>structuredClone(x),quiet={log(){},error(){},warn(){}};
function diagnostic(x){
  if(x===undefined)return["undefined"];if(x===null)return["null"];
  if(typeof x==="boolean")return["boolean",x];if(typeof x==="string")return["string",x];
  if(typeof x==="number"){const b=Buffer.alloc(8);b.writeDoubleBE(x);return["number",Number.isNaN(x)?"NaN":x===Infinity?"Infinity":x===-Infinity?"-Infinity":b.toString("hex")];}
  if(Array.isArray(x))return["array",Array.from(x,diagnostic)];
  if(typeof x==="object")return["object",Object.keys(x).map(k=>[k,diagnostic(x[k])])];
  throw Error("Outside SourceValue domain: "+typeof x);
}
const hex=x=>Buffer.from(JSON.stringify(diagnostic(x))).toString("hex");
const field=(row,key)=>key.split(".").reduce((a,k)=>a?.[k],row);
function matches(row,query){return Object.entries(query).every(([key,v])=>{
  if(key==="$or")return v.some(q=>matches(row,q));const a=field(row,key);
  if(v&&typeof v==="object"&&!Array.isArray(v))return Object.entries(v).every(([op,v])=>op==="$exists"?(a!==undefined)===v:op==="$ne"?a!==v:(()=>{throw Error("Unmodeled operator "+op)})());
  return a===v;
});}
let manager,databases,trace,failures;
function dbcall(collection,operation,query,update,options){
  trace.push({collection,operation,query:clone(query),update:clone(update),options:clone(options)});
  const index=failures.findIndex(f=>f.collection===collection&&f.operation===operation);
  if(index>=0){if(failures[index].skip===0){const f=failures.splice(index,1)[0],e=Error(f.message);e.name=f.name;throw e;}failures[index].skip--;}
}
class Database{
  constructor(name){this.name=name;this.rows=[];}
  async findAsync(q){dbcall(this.name,"findAsync",q);return clone(this.rows.filter(r=>matches(r,q)));}
  async findOneAsync(q){dbcall(this.name,"findOneAsync",q);return clone(this.rows.find(r=>matches(r,q))||null);}
  async updateAsync(q,u,o={}){dbcall(this.name,"updateAsync",q,u,o);let r=this.rows.find(r=>matches(r,q));
    if(!r){if(!o.upsert)return 0;r=clone(q);this.rows.push(r);}assert.deepEqual(Object.keys(u),["$set"]);for(const[k,v]of Object.entries(u.$set))r[k]=clone(v);return 1;}
}
function reset(){
  trace=[];failures=[];databases={clearlists:new Database("clearlists"),attestations:new Database("attestations")};
  const module={exports:{}};vm.runInNewContext(source,{module,exports:module.exports,Buffer,console:quiet,require(name){
    if(name==="./db.js")return{getDatabase:async collection=>{assert(Object.hasOwn(databases,collection));dbcall(collection,"getDatabase");return databases[collection];}};
    if(name==="crypto")return crypto;if(name==="bitcore-lib-ltc"){if(fallback)throw Error("explicit Litecoin module-load failure");return selectedLibrary;}
    if(name==="bitcore-lib"&&fallback)return selectedLibrary;
    throw Error("Unexpected import "+name);
  }},{filename:"pinned/clearlist.js",timeout:2000});manager=module.exports;
}
function state(){return{clearlists:databases.clearlists.rows,attestations:databases.attestations.rows,
  clearlistsMap:Array.from(manager.clearlists),banlist:manager.banlist,xpubCache:Array.from(manager.xpubCache),merkleXpubMap:Array.from(manager.merkleXpubMap)};}
const rows=[],methods=new Set();let scenario="";
async function action(label,command){
  trace=[];let outcome;
  try{
    let value;
    if(command.op==="reset")reset();
    else if(command.op==="import")databases[command.collection].rows=clone(command.rows);
    else if(command.op==="failure")failures.push({...command.failure});
    else if(command.op==="restart"){const persisted=clone([databases.clearlists.rows,databases.attestations.rows]);reset();[databases.clearlists.rows,databases.attestations.rows]=persisted;}
    else{assert.equal(command.op,"call");methods.add(command.method);value=await manager[command.method](...command.arguments);}
    outcome={returned:value};
  }catch(e){outcome={thrown:{name:e.name,message:e.message}};}
  rows.push(["V",scenario+"/"+label,hex(command),hex(outcome),hex(state()),hex(trace)].join("\t"));
}
async function start(name){scenario=name;await action("reset",{op:"reset",library:fallback?"bitcoin":"litecoin"});}
const call=(label,method,...args)=>action(label,{op:"call",method,arguments:args});
const imported=(label,collection,rows)=>action(label,{op:"import",collection,rows});
const fail=(label,collection,operation,skip=0)=>action(label,{op:"failure",failure:{collection,operation,skip,name:"Error",message:"injected "+collection+" "+operation}});
const xpub="xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
async function main(){
  if(fallback){
    await start("bitcoin-dependency-fallback");
    await call("derive","deriveAddressesFromXpub",xpub,"ignored-litecoin-network");
    const addresses=manager.deriveAddressesFromXpub(xpub);assert.equal(addresses[0],"12CL4K2eVqj7hQTix7dM7CVHCkpP17Pry3");
    await call("cached","deriveAddressesFromXpub",xpub,"testnet");
    await call("inline","addAttestation",1,"issuer","xpub:"+xpub,1);
    await call("derived","isAddressInClearlistOrDerived",1,addresses[39]);
    await call("parse-error-does-not-change-library","deriveAddressesFromXpub","invalid");
    process.stdout.write(JSON.stringify(rows));return;
  }
  await start("create-admin-typed-ids");
  await call("uncached-admin","verifyAdmin",1,"old");
  await call("auto","createClearlist","old","list","ignored","description","backup");
  await call("cache-still-empty","verifyAdmin",1,"old");
  await call("getList-wrong-collection","getList",1);
  await call("load","getClearlistById",1);
  await call("primary","verifyAdmin",1,"old");await call("backup-rejected","verifyAdmin",1,"backup");
  await call("admin-shadow","updateAdmin",1,"new",false);
  await call("new-cache-admin","verifyAdmin",1,"new");
  await call("numeric-reload-old","getClearlistById",1);
  await call("string-shadow-new","getClearlistById","1");
  await call("backup-shadow","updateAdmin",1,"backup-new",true);
  await call("new-backup-not-primary","verifyAdmin",1,"backup-new");
  await call("numeric-reload-old-backup","getClearlistById",1);
  await call("explicit-upsert","createClearlist","replacement","overwrite","url","","backup",1);
  await call("no-cache-create","verifyAdmin",1,"replacement");
  await call("get-overwrite","getClearlistById",1);
  await call("missing","getClearlistById",99);

  await start("allocation-alias-stale");
  await call("object-alias","createclearlist",{adminAddress:"",admin:"a",admin_address:"fallback",name:"n",url:"ignored",description:{nested:[1,null,true]},backupAddress:false,backup:"b",id:"12tail"});
  await call("prefix-next","getNextId");
  await call("positional-alias","createclearlist","a","n",null,"d","b");
  await call("hex-id","createClearlist","a","hex","","","b","0x20rest");
  await call("negative-zero-auto","createClearlist","a","zero","","","b",-0);
  await call("false-auto","createClearlist","a","false","","","b",false);
  await call("string-zero","createClearlist","a","string0","","","b","0");
  await call("unicode-numeric","createClearlist","a","unicode","","","b","\ufeff+42tail");
  await call("load-all","loadClearlists");
  await imported("delete-database","clearlists",[]);await call("load-keeps-stale","loadClearlists");
  await call("stale-lookup","getClearlistById","12tail");await call("stale-allocation","getNextId");
  await action("restart-drops-stale",{op:"restart"});await call("empty-after-restart","getClearlistById","12tail");

  await start("overwrite-revocation-history");
  await call("issue","addAttestation",1,"alice",{unicode:"é",list:[undefined,0,false]},10);
  await call("membership","isAddressInClearlist",1,"alice");await call("derived-direct-shortcut","isAddressInClearlistOrDerived",1,"alice");
  await call("repeat-overwrite","addAttestation",1,"alice","second",11);
  await call("history-one-row","getAttestationHistory","alice",1);
  await call("cross-list-overwrite","addAttestation",2,"alice","other",12);
  await call("first-list-lost","isAddressInClearlist",1,"alice");
  await call("canonical-empty-fallback","revokeAttestation",99,"alice","cross-list",13);
  await call("already-revoked-fallback","revokeAttestation",1,"alice",{reason:"again"},14);
  await call("history-revoked","getAttestationHistory","alice",2);
  await call("missing-revoke","revokeAttestation",1,"absent","missing",15);
  await call("attestations-do-not-filter-status","getAttestations",2);
  await imported("legacy-rows","attestations",[
    {_id:"legacy-a",extra:{keep:true},data:{clearlistId:3,address:"bob",status:"active",timestamp:"1",data:"old"}},
    {_id:"legacy-b",data:{listId:3,address:"bob",status:"active",timestamp:9}},
    {_id:"legacy-c",data:{listId:3,address:"bob",status:"active",timestamp:9}},
    {_id:3,data:{adminAddress:"odd-row-admin",backupAddress:"b"}},
    {_id:"attestation:fallback",data:{listId:4,address:"fallback",status:"revoked",timestamp:0}},
    {_id:"fallback",data:{listId:5,address:"fallback",status:"active",timestamp:1}}
  ]);
  await call("getList-legacy-row","getList",3);await call("aliases","getAttestations",3);
  await call("history-excludes-clearlistId","getAttestationHistory","bob",3);
  await call("canonical-all","revokeAttestation",3,"bob","all",20);
  await call("legacy-prefix-before-address","revokeAttestation",88,"fallback","legacy-first",21);

  await start("history-coercion-runtime-order");
  const timestamps=[null,3,2,undefined,0,1,2];
  await imported("inconsistent-comparator-short-array","attestations",timestamps.map((timestamp,i)=>({_id:"mixed-"+i,data:{address:"alice",listId:1,timestamp}})));
  await call("v8-natural-run-binary-insertion","getAttestationHistory","alice",1);
  await imported("large-numeric-array","attestations",Array.from({length:80},(_,i)=>({_id:"large-"+i,data:{address:"alice",listId:1,timestamp:i%5===0?String(i%11):i%11}})));
  await call("large-stable-numeric-sort","getAttestationHistory","alice",1);
  await imported("all-nan-large-array","attestations",Array.from({length:65},(_,i)=>({_id:"nan-"+i,data:{address:"alice",listId:1,timestamp:undefined}})));
  await call("all-nan-stable-order","getAttestationHistory","alice",1);
  await imported("missing-data-rows","attestations",[{_id:"a"},{_id:"b"}]);await call("nested-property-error","getAttestationHistory",undefined,undefined);

  await start("country-coercion-aliases");
  await call("self-cert","addAttestation",0,"alice"," \ufeffus\u00a0",0);
  await call("country","getCountryCodeByAddress","alice");
  await call("overwrite-country","addAttestation",2,"alice","no-country",12);
  await call("country-overwritten","getCountryCodeByAddress","alice");
  const countries=[
    {clearlistId:"0",data:{country:" nz "},blockHeight:7,timestamp:8},
    {listId:null,data:{code:"ß"},blockHeight:0,timestamp:9},
    {listId:false,status:"",data:{value:" ca "},timestamp:0},
    {listId:[],data:" gb ",timestamp:"height"},
    {listId:"0x0",data:{countryCode:"",country:" ca ",code:"nz"},timestamp:11},
    {listId:"0",data:{countryCode:1,country:"CA"},timestamp:12},
    {listId:0,data:{countryCode:" ",country:"CA"},timestamp:13},
    {clearlistId:0,listId:undefined,data:{value:"au"},blockHeight:false,timestamp:null},
    {listId:0,status:"revoked",data:"US",timestamp:14},
    {listId:undefined,clearlistId:"",data:"se",timestamp:15},
    {listId:{toString:"not-callable"},data:"GB",timestamp:16}
  ];
  for(let i=0;i<countries.length;i++){
    await imported("import-"+i,"attestations",[{_id:"country-"+i,data:{address:"country",...countries[i]}}]);
    await call("get-"+i,"getCountryCodeByAddress","country");
  }
  await imported("first-matching-country","attestations",[{_id:"invalid",data:{address:"many",listId:0,data:{countryCode:true,country:"US"}}},{_id:"valid",data:{address:"many",clearlistId:"0",data:"sg",timestamp:100}},{_id:"later",data:{address:"many",listId:0,data:"us",timestamp:200}}]);
  await call("first-valid-in-query-order","getCountryCodeByAddress","many");await call("absent","getCountryCodeByAddress","absent");

  await start("banlist-load-restart");
  await call("getter-before","getBanlist");await call("set-alias","setBanList",["GB","KP"],20);
  await call("getter-undefined","getBanlist");await call("load-banlist-is-a-list","loadClearlists");
  await call("banlist-map-lookup","getClearlistById","globalBanlist");await call("next-id-skips-banlist","getNextId");
  await call("set-object-permitted","setBanlist",{countries:["SG"],arbitrary:true},21);
  await action("restart-resets-singleton-banlist",{op:"restart"});await call("getter-still-undefined","getBanlist");await call("load-persisted-object","getClearlistById","globalBanlist");

  await start("merkle-xpub-cache");
  const keys=[xpub,"second","third","fourth","fifth"],tree=manager.buildMerkleTree(keys),proof=manager.generateMerkleProof(keys,xpub),single=manager.buildMerkleTree([xpub]);
  await call("empty-tree","buildMerkleTree",[]);await call("null-tree","buildMerkleTree",null);await call("odd-tree","buildMerkleTree",keys);
  await call("proof","generateMerkleProof",keys,xpub);await call("missing-proof","generateMerkleProof",keys,"missing");
  await call("verify-proof","verifyMerkleProof",xpub,proof,tree.root);await call("wrong-root","verifyMerkleProof",xpub,proof,"wrong");
  await call("odd-upper-proof-defect","verifyMerkleProof","fifth",manager.generateMerkleProof(keys,"fifth"),tree.root);
  await call("root-unregistered","registerXpubForRoot",tree.root,xpub,proof);
  await call("store","storeMerkleRoot",1,tree.root,"root-admin",10);
  await call("register","registerXpubForRoot",tree.root,xpub,proof);await call("register-dedup","registerXpubForRoot",tree.root,xpub,proof);
  await call("wrong-proof","registerXpubForRoot",tree.root,xpub,[]);
  await call("derive","deriveAddressesFromXpub",xpub,"ignored-network");
  const addresses=manager.deriveAddressesFromXpub(xpub);
  assert.equal(addresses.length,40);assert.equal(addresses[0],"LLRHKXLUaVyAxD9t8FcePDZ3QyBf7E4gu5");
  await call("derive-cache-network-ignored","deriveAddressesFromXpub",xpub,"testnet");
  await call("derived-root-first","isAddressInClearlistOrDerived",1,addresses[0]);await call("derived-root-last","isAddressInClearlistOrDerived",1,addresses[39]);
  await call("wrong-list","isAddressInClearlistOrDerived",2,addresses[0]);
  await call("share-root-second-list","storeMerkleRoot",2,tree.root,"second-admin",11);
  await call("root-map-cross-list","isAddressInClearlistOrDerived",2,addresses[20]);
  await call("revoke-root","revokeAttestation",1,"root-admin","revoke",12);
  await call("revoked-root-inactive","isAddressInClearlistOrDerived",1,addresses[0]);
  await call("other-root-active","isAddressInClearlistOrDerived",2,addresses[0]);
  await action("restart-loses-root-registration",{op:"restart"});await call("root-missing-after-restart","isAddressInClearlistOrDerived",2,addresses[0]);
  await call("cannot-register-after-load","registerXpubForRoot",tree.root,xpub,proof);
  await call("re-store-initializes-root","storeMerkleRoot",2,tree.root,"second-admin",13);
  await call("register-after-re-store","registerXpubForRoot",tree.root,xpub,proof);
  await call("inline","addAttestation",3,"issuer","xpub:"+xpub,14);
  await call("inline-derived","isAddressInClearlistOrDerived",3,addresses[19]);
  await call("explicit-empty-no-inline-fallback","addAttestationWithXpub",4,"explicit","xpub:"+xpub,"",15);
  await call("explicit-null-skipped","isAddressInClearlistOrDerived",4,addresses[0]);
  await call("inline-empty","addAttestation",4,"empty","xpub:",16);
  await call("explicit","addAttestationWithXpub",4,"explicit","metadata",xpub,17);
  await call("explicit-derived","isAddressInClearlistOrDerived",4,addresses[39]);
  await call("invalid-xpub-not-cached","deriveAddressesFromXpub","invalid");
  await call("invalid-xpub-retried","deriveAddressesFromXpub","invalid");
  await call("single-tree","buildMerkleTree",[xpub]);await call("single-proof","generateMerkleProof",[xpub],xpub);
  await call("single-proof-verifies","verifyMerkleProof",xpub,manager.generateMerkleProof([xpub],xpub),single.root);

  await start("root-direct-order-fallback");
  const r=manager.buildMerkleTree(["single"]).root;
  await call("wrong-list-direct","addAttestation",2,"alice","other",10);await call("root","storeMerkleRoot",1,r,"alice",11);
  await call("root-is-direct-membership","isAddressInClearlist",1,"alice");
  await call("root-canonical-revoke","revokeAttestation",1,"alice","canonical",12);
  await call("wrong-list-direct-remains","isAddressInClearlist",2,"alice");
  await call("second-revoke-falls-across-list","revokeAttestation",1,"alice","fallback",13);
  await call("direct-same-list","addAttestation",1,"alice","direct",14);await call("reactivate-root","storeMerkleRoot",1,r,"alice",15);
  await call("history-descending","getAttestationHistory","alice",1);await call("revoke-both","revokeAttestation",1,"alice","both",16);
  await call("equal-time-stable-history","getAttestationHistory","alice",1);

  await start("adapter-failures");
  await fail("fail-load","clearlists","findAsync");await call("load-catches","loadClearlists");
  await fail("fail-next-load","clearlists","getDatabase");await call("next-continues","getNextId");
  await call("create","createClearlist","old","list","","","backup",1);await call("load","loadClearlists");
  await fail("admin-write-reject","clearlists","updateAsync");await call("admin-mutates-cache-before-reject","updateAdmin",1,"new",false);await call("cache-after-reject","verifyAdmin",1,"new");
  await fail("country-read-reject","attestations","findAsync");await call("country-catches","getCountryCodeByAddress","alice");
  await fail("banlist-write-reject","clearlists","updateAsync");await call("setter-rethrows-cache-unchanged","setBanlist",["SG"],100);
  await fail("banlist-read-reject","clearlists","findAsync");await call("getter-rethrows","getBanlist");
  await fail("attestation-write-reject","attestations","updateAsync");await call("issue-rethrows","addAttestation",1,"alice","data",10);
  await imported("multirow","attestations",[{_id:"a",data:{listId:1,address:"alice",status:"active",timestamp:1}},{_id:"b",data:{listId:1,address:"alice",status:"active",timestamp:2}}]);
  await fail("second-revoke-write-reject","attestations","updateAsync",1);await call("partial-revocation","revokeAttestation",1,"alice","part",20);
  await call("membership-still-true","isAddressInClearlist",1,"alice");

  await start("malformed-supported-values");
  await call("missing-create-args","createClearlist");await call("load-undefined-fields","loadClearlists");
  await call("missing-get","getList");await call("tree-nonarray","buildMerkleTree",{length:2});
  await call("crypto-invalid-number","verifyMerkleProof",7,[],"root");
  await call("noniterable-proof","verifyMerkleProof","x",null,"root");
  await call("null-proof-sibling","verifyMerkleProof","x",[null],"root");
  await call("string-proof-iterable","verifyMerkleProof","x","a","root");
  await imported("primitive-map-data","clearlists",[{_id:1,data:"primitive"},{_id:"2",data:null}]);await call("load-primitives","loadClearlists");
  await call("admin-primitive-error","updateAdmin",1,"a",false);await call("admin-null-not-found","verifyAdmin","2","a");
  await call("create-string-id","createClearlist","a","n","","","b",true);await call("boolean-id-lookup","getClearlistById",true);

  const expected=Array.from(source.matchAll(/static\s+(?:async\s+)?(\w+)\s*\(/g),m=>m[1]);
  assert.equal(expected.length,25);assert.deepEqual([...methods].sort(),expected.sort());
  // Loading both installed bitcore packages into one process mutates a shared
  // elliptic prototype, so exercise the actual fallback in its own process.
  const child=require("node:child_process").spawnSync(process.execPath,[__filename,"--module-root",moduleRoot,"--fallback-oracle"],{encoding:"utf8",maxBuffer:8*1024*1024});
  assert.equal(child.status,0,child.stderr);rows.push(...JSON.parse(child.stdout));
  const header="# Exact clearlist.js "+hash+"; real bitcore-lib-ltc "+external("bitcore-lib-ltc/package.json").version+" and bitcore-lib "+external("bitcore-lib/package.json").version+"; cloned typed-ID DB adapter; V case action outcome state db-calls (tagged JSON hex)\n";
  const output=path.join(root,"fixtures/clearlist_source_owner_vectors.tsv"),bytes=header+rows.join("\n")+"\n";
  if(process.argv.includes("--check"))assert.equal(fs.readFileSync(output,"utf8"),bytes,"Stored clearlist owner corpus differs; inspect before regeneration");
  else fs.writeFileSync(output,bytes);
  console.log(JSON.stringify({methods:methods.size,observations:rows.length,sourceSha256:hash,bitcoreLibLtc:external("bitcore-lib-ltc/package.json").version,bitcoreLib:external("bitcore-lib/package.json").version,output}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
