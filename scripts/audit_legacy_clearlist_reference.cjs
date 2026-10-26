"use strict";
// Executes the exact Git-extracted clearlist.js and validity.js bytes.
// The storage adapter below is a typed-ID, cloned-document MODEL of NeDB,
// not NeDB itself. Cryptographic BIP32/address libraries are not exercised.
const fs = require("node:fs"), path = require("node:path"), vm = require("node:vm");
const crypto = require("node:crypto"), assert = require("node:assert/strict");
const here = __dirname, observations = [], dbCalls = [];
const clone = x => structuredClone(x);
const quiet = {log() {}, error() {}, warn() {}};
function source(name, expected) {
  const data = Buffer.from(fs.readFileSync(path.join(here,"../fixtures",name === "clearlist.js" ? "legacy_clearlist_audit_reference.txt" : "legacy_validity_source.txt"),"utf8").replace(/\r\n/g,"\n"));
  assert.equal(crypto.createHash("sha256").update(data).digest("hex"),expected);
  return data.toString("utf8");
}
const clearlistSource = source("clearlist.js","fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1");
const validitySource = source("validity.js","ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00");
const field = (row,key) => key.split(".").reduce((a,k)=>a?.[k],row);
function matches(row,query) {
  return Object.entries(query).every(([key,value]) => {
    if(key === "$or") return value.some(q=>matches(row,q));
    const actual = field(row,key);
    if(value && typeof value === "object" && !Array.isArray(value)) {
      return Object.entries(value).every(([op,v]) => {
        if(op === "$exists") return (actual !== undefined) === v;
        if(op === "$ne") return actual !== v;
        throw Error("Unmodeled database operator: "+op);
      });
    }
    return actual === value;
  });
}
class Database {
  constructor(name) { this.name=name; this.rows=[]; }
  async findAsync(query) { dbCalls.push([this.name,"findAsync",clone(query)]); return clone(this.rows.filter(r=>matches(r,query))); }
  async findOneAsync(query) { dbCalls.push([this.name,"findOneAsync",clone(query)]); return clone(this.rows.find(r=>matches(r,query)) || null); }
  async updateAsync(query,update,options={}) {
    dbCalls.push([this.name,"updateAsync",clone(query),clone(update),clone(options)]);
    let row=this.rows.find(r=>matches(r,query));
    if(!row) { if(!options.upsert) return 0; row=clone(query); this.rows.push(row); }
    assert.deepEqual(Object.keys(update),["$set"]);
    for(const [key,value] of Object.entries(update.$set)) row[key]=clone(value);
    return 1;
  }
}
function loadModule(code,imports,filename) {
  const module={exports:{}};
  vm.runInNewContext(code,{module,exports:module.exports,console:quiet,Buffer,
    require(name) { assert(Object.hasOwn(imports,name),"Unmodeled import "+name); return imports[name]; }
  },{filename,timeout:2000});
  return module.exports;
}
function environment() {
  const databases = {clearlists:new Database("clearlists"),attestations:new Database("attestations")};
  const db={getDatabase:async name=>{assert(Object.hasOwn(databases,name));return databases[name];}};
  const manager=loadModule(clearlistSource,{"./db.js":db,crypto},"pinned/clearlist.js");
  const empty={};
  const validity=loadModule(validitySource,{
    "./clearlist.js":manager,"./db":db,crypto,
    "./activation.js":{getInstance:()=>({isTxTypeActive:async()=>true,getAdmin:()=>"protocol-admin"})},
    "./txUtils.js":{validateAddressWrapper:async()=>({isvalid:true,isValid:true}),getTransactionOutputs:async()=>[{vout:1,address:"alice"}]},
    "./property.js":empty,"./oracle.js":empty,"./contractRegistry.js":empty,"./tally.js":empty,"./orderbook.js":empty,
    "./channels.js":empty,"./marginMap.js":empty,"./volumeIndex.js":empty,"./vaults.js":empty,"./vesting.js":empty,
    "./scaling.js":empty,"./dlcOracleBridge.js":empty,"./procedural.js":empty,"./options.js":empty,
    "bignumber.js":class UnusedBigNumber {constructor(){throw Error("BigNumber not exercised");}},
    "bitcoinjs-lib":{networks:{bitcoin:{}}},"tiny-secp256k1":empty,"bip32":{BIP32Factory:()=>empty},
    bech32:{bech32:{decode(){throw Error("Bech32 not exercised");}}}
  },"pinned/validity.js");
  return {manager,validity,databases};
}
async function check(name,run) {
  const before=dbCalls.length; const detail=await run(environment());
  observations.push({case:name,status:"PASS",detail:detail || null,database_model_calls:dbCalls.length-before});
}
async function main() {
  await check("create, aliases, URL omission, false ID allocation, explicit replacement",async({manager:m,databases:d})=>{
    const first=await m.createClearlist("admin","one","https://discarded","description","backup");
    assert.equal(first.id,1); assert.equal(first.url,undefined); assert.equal(m.clearlists.size,0);
    assert.equal((await m.createclearlist({admin_address:"other",name:"two",url:"discard",backup:"b",id:0})).id,2);
    await m.createClearlist("replacement","replace","", "", "",1);
    assert.equal((await m.getClearlistById(1)).admin,"replacement");
    assert.equal((await m.createclearlist("third","three","","","b")).id,3);
    assert.equal(d.clearlists.rows.length,3);
  });
  await check("typed ID and parseInt prefix allocation",async({manager:m})=>{
    await m.createClearlist("a","name","","","b","12tail");
    assert.equal((await m.createClearlist("a","next","","","b")).id,13);
    assert.equal(await m.getClearlistById(12),false);
    assert.equal((await m.getClearlistById("12tail")).id,"12tail");
  });
  await check("admin update string shadow, primary only verify, numeric reload",async({manager:m,databases:d})=>{
    await m.createClearlist("old","list","","","backup");
    await assert.rejects(m.verifyAdmin(1,"old"),/Clearlist not found/);
    await m.getClearlistById(1); assert.equal(await m.verifyAdmin(1,"backup"),false);
    await m.updateAdmin(1,"new",false); assert.equal(await m.verifyAdmin(1,"new"),true);
    assert.equal(d.clearlists.rows.find(x=>x._id===1).data.admin,"old");
    assert.equal(d.clearlists.rows.find(x=>x._id==="1").data.admin,"new");
    assert.equal((await m.getClearlistById(1)).admin,"old");
    assert.equal((await m.getClearlistById("1")).admin,"new");
    await m.updateAdmin(1,"new-backup",true);
    assert.equal(m.clearlists.get(1).backupAddress,"new-backup");
  });
  await check("load leaves stale cache after modeled database deletion",async({manager:m,databases:d})=>{
    await m.createClearlist("a","name","","","b",9); await m.loadClearlists();
    d.clearlists.rows=[]; await m.loadClearlists(); assert.equal((await m.getClearlistById(9)).id,9);
  });
  await check("type8 real helper rejects type7-created list",async({manager:m,validity:v})=>{
    await m.createClearlist("old","list","","","backup");
    assert.equal(await m.getList(1),null);
    const p=await v.validateUpdateAdmin("old",{block:101,id:1,whitelist:true,token:false,oracle:false,newAddress:"new"},"tx");
    assert.equal(p.valid,false); assert.match(p.reason,/Whitelist not found/);
    return {valid:p.valid,reason:p.reason};
  });
  await check("direct attestation uses one address key across lists and replaces active row",async({manager:m,databases:d})=>{
    await m.addAttestation(1,"alice","first",10); await m.addAttestation(1,"alice","updated",11);
    assert.equal((await m.getAttestationHistory("alice",1)).length,1);
    await m.addAttestation(2,"alice","other",12);
    assert.equal(await m.isAddressInClearlist(1,"alice"),false); assert.equal(await m.isAddressInClearlist(2,"alice"),true);
    assert.equal((await m.getAttestations(1)).length,0); assert.equal(d.attestations.rows.length,1);
    assert.equal(d.attestations.rows[0]._id,"alice");
  });
  await check("revoke fallback crosses lists and can revoke already revoked row",async({manager:m,databases:d})=>{
    await m.addAttestation(2,"alice","other",12); await m.revokeAttestation(99,"alice","wrong list",13);
    const row=d.attestations.rows[0].data; assert.equal(row.listId,2); assert.equal(row.id,99); assert.equal(row.status,"revoked");
    await m.revokeAttestation(1,"alice","again",14); assert.equal(d.attestations.rows[0].data.id,1);
    await assert.rejects(m.revokeAttestation(1,"nobody","absent",15),/Attestation not found/);
  });
  await check("legacy clearlistId queries and multirow revocation",async({manager:m,databases:d})=>{
    d.attestations.rows=[{_id:"legacy-a",data:{clearlistId:3,address:"alice",status:"active",timestamp:1}},
      {_id:"legacy-b",data:{listId:3,address:"alice",status:"active",timestamp:2}}];
    assert.equal((await m.getAttestations(3)).length,2); assert.equal(await m.isAddressInClearlist(3,"alice"),true);
    assert.equal((await m.getAttestationHistory("alice",3)).length,1);
    await m.revokeAttestation(3,"alice","all",4); assert(d.attestations.rows.every(x=>x.data.status==="revoked"));
  });
  await check("root-only attestation revocation succeeds via canonical query",async({manager:m,databases:d})=>{
    const root=m.buildMerkleTree(["synthetic"]).root;
    await m.storeMerkleRoot(1,root,"alice",2); assert.equal(await m.isAddressInClearlist(1,"alice"),true);
    await m.revokeAttestation(1,"alice","revoke-root",3);
    assert.equal(d.attestations.rows[0].data.status,"revoked"); assert.equal(await m.isAddressInClearlist(1,"alice"),false);
    return "canonical query revokes a root-keyed row without an address-keyed row";
  });
  await check("direct and root history sorts descending and revoke covers both",async({manager:m,databases:d})=>{
    const root=m.buildMerkleTree(["synthetic"]).root;
    await m.addAttestation(1,"alice","one",1); await m.storeMerkleRoot(1,root,"alice",2);
    const history=await m.getAttestationHistory("alice",1);
    assert.equal(history.length,2); assert.equal(history[0].data.timestamp,2); assert.equal(history[1].data.timestamp,1);
    await m.revokeAttestation(1,"alice","all",3); assert(d.attestations.rows.every(x=>x.data.status==="revoked"));
    return "source history is descending by timestamp and canonical revocation covers both rows";
  });
  await check("canonical root match prevents wrong-list address fallback",async({manager:m,databases:d})=>{
    const root=m.buildMerkleTree(["synthetic"]).root;
    await m.addAttestation(2,"alice","other-list",10); await m.storeMerkleRoot(1,root,"alice",11);
    await m.revokeAttestation(1,"alice","root",12);
    assert.equal(d.attestations.rows.find(x=>x._id==="alice").data.status,"active");
    assert.equal(await m.isAddressInClearlist(1,"alice"),false);
    await m.revokeAttestation(1,"alice","fallback",13);
    assert.equal(d.attestations.rows.find(x=>x._id==="alice").data.status,"revoked");
  });
  await check("country string, metadata object, legacy status and block-height precedence",async({manager:m,databases:d})=>{
    await m.addAttestation(0,"alice"," us ",0); let country=await m.getCountryCodeByAddress("alice");
    assert.equal(country.countryCode,"US"); assert.equal(country.blockHeight,null);
    await m.addAttestation(2,"alice","other",12); assert.equal(await m.getCountryCodeByAddress("alice"),null);
    d.attestations.rows.push({_id:"legacy",data:{address:"bob",clearlistId:"0",data:{country:" nz "},blockHeight:7,timestamp:8}});
    country=await m.getCountryCodeByAddress("bob"); assert.equal(country.countryCode,"NZ"); assert.equal(country.blockHeight,7);
    for(const data of [{countryCode:"",country:" ca ",code:"NZ",value:"GB"},{countryCode:" ca "},{code:" ca "},{value:" ca "}]) {
      await m.addAttestation(0,"aliases",data,10);
      country=await m.getCountryCodeByAddress("aliases"); assert.equal(country.countryCode,"CA"); assert.equal(country.blockHeight,10);
    }
    await m.addAttestation(0,"aliases",{countryCode:" ",country:"CA"},11);
    assert.equal(await m.getCountryCodeByAddress("aliases"),null);
  });
  await check("banlist setter persists and cache changes but getter returns undefined",async({manager:m,databases:d})=>{
    assert.equal(await m.getBanlist(),undefined); await m.setBanList(["GB","KP"],20);
    assert.equal(m.banlist.join(","),"GB,KP"); assert.equal(await m.getBanlist(),undefined);
    assert.equal(d.clearlists.rows[0]._id,"globalBanlist"); assert.equal(d.clearlists.rows[0].timestamp,20);
  });
  await check("type9 actual clearlist dependency, backup rejection, list0 and banned-country command",async({manager:m,validity:v})=>{
    await m.createClearlist("admin","list","","","backup");
    const params={block:101,id:1,targetAddress:"alice",metaData:"OK",revoke:false};
    assert.equal((await v.validateIssueOrRevokeAttestation("admin",{...params},"tx")).valid,true);
    assert.equal((await v.validateIssueOrRevokeAttestation("backup",{...params},"tx")).valid,false);
    assert.equal((await v.validateIssueOrRevokeAttestation("alice",{...params,id:0,metaData:"ca"},"tx")).valid,true);
    assert.equal((await v.validateIssueOrRevokeAttestation("other",{...params,id:0,metaData:"ca"},"tx")).valid,false);
    assert.equal((await v.validateIssueOrRevokeAttestation("protocol-admin",{...params,id:0,targetAddress:"BANLIST",metaData:"us; kp|RU"},"tx")).valid,true);
    assert.equal((await v.validateIssueOrRevokeAttestation("admin",{...params,revoke:true},"tx")).valid,false);
    await m.addAttestation(1,"alice","OK",20);
    assert.equal((await v.validateIssueOrRevokeAttestation("admin",{...params,revoke:true},"tx")).valid,true);
  });
  await check("type9 repeated issue accepted by validator",async({manager:m,validity:v})=>{
    await m.createClearlist("admin","list","","","backup"); await m.addAttestation(1,"alice","one",1);
    const result=await v.validateIssueOrRevokeAttestation("admin",{block:101,id:1,targetAddress:"alice",metaData:"two",revoke:false},"tx");
    assert.equal(result.valid,true);
  });
  await check("xpub markers store metadata but derivation libraries remain unavailable",async({manager:m,databases:d})=>{
    await m.addAttestation(1,"a","xpub:synthetic",10); assert.equal(d.attestations.rows[0].data.xpub,"synthetic");
    await m.addAttestationWithXpub(2,"b","meta","explicit-synthetic",11); assert.equal(d.attestations.rows[1].data.xpub,"explicit-synthetic");
    assert.equal(await m.isAddressInClearlistOrDerived(1,"a"),true);
    assert.equal(await m.isAddressInClearlistOrDerived(1,"unattested"),false);
    return "false on a non-direct xpub candidate is a missing-library source behavior in this harness; it does not verify BIP32";
  });
  await check("explicit empty xpub does not infer inline metadata prefix",async({manager:m,databases:d})=>{
    await m.addAttestationWithXpub(1,"alice","xpub:synthetic","",1);
    assert.equal(d.attestations.rows[0].data.xpub,null);
    await m.addAttestation(1,"bob","xpub:",2); assert.equal(d.attestations.rows[1].data.xpub,"");
    return "explicit empty xpub is null; an inline empty xpub remains the empty string";
  });
  await check("Merkle exact source ASCII-hex hashing, odd parent proof and volatile root registration",async({manager:m,databases:d})=>{
    const one=m.buildMerkleTree(["xpub-synthetic-0"]); assert.equal(one.root,"82f8fcd70bcdf26565adb613a0dfe04955ee27054ab30f03e23160219aa73de9");
    const five=Array.from({length:5},(_,i)=>"xpub-synthetic-"+i),tree=m.buildMerkleTree(five);
    assert.equal(tree.root,"a9a5833ec88f79b211ba29e8d3bc936c02dba0ffaeaf2a48149304b679b63f3a");
    assert.equal(m.verifyMerkleProof(five[4],m.generateMerkleProof(five,five[4]),tree.root),false);
    assert.equal(m.registerXpubForRoot(one.root,"xpub-synthetic-0",m.generateMerkleProof(["xpub-synthetic-0"],"xpub-synthetic-0")),false);
    await m.storeMerkleRoot(1,one.root,"admin",5);
    assert.equal(m.registerXpubForRoot(one.root,"xpub-synthetic-0",m.generateMerkleProof(["xpub-synthetic-0"],"xpub-synthetic-0")),true);
    assert.equal(await m.isAddressInClearlist(1,"admin"),true);
    assert.equal(d.attestations.rows[0]._id,"merkle:1:"+one.root);
    m.merkleXpubMap.clear(); await m.loadClearlists(); assert.equal(m.merkleXpubMap.size,0);
  });
  const report={node:process.version,check_count:observations.length,status:"PASS",source:"Git blobs, hashes verified before executing",storage:"in-memory NeDB query/update MODEL",rust_execution:"separate focused Rust audit tests; this command executes JavaScript only",observations,dbCalls};
  const evidenceAt=process.argv.indexOf("--evidence");
  if(evidenceAt>=0) fs.writeFileSync(path.resolve(process.argv[evidenceAt+1]),JSON.stringify(report,null,2)+"\n");
  console.log(`PASS ${observations.length} exact-source checks; Node ${process.version}; ${dbCalls.length} modeled database calls`);
  console.log("BOUNDARY: exact JS source, modeled database, explicit activation/address stubs, no real NeDB/BIP32/Rust execution");
}
main().catch(error=>{console.error(error);process.exitCode=1;});
