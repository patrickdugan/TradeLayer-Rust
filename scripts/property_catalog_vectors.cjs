"use strict";
// Complete exact property.js bytes execute unchanged, with genuine archived BN.
const fs=require("node:fs"),path=require("node:path"),vm=require("node:vm"),crypto=require("node:crypto"),assert=require("node:assert/strict");
const {loadBigNumber}=require("./legacy_numeric_oracle.cjs");
const BigNumber=loadBigNumber();
const root=path.resolve(__dirname,"..");
const source=fs.readFileSync(path.join(root,"fixtures/reference_fidelity/property.js"));
assert.equal(crypto.createHash("sha256").update(source).digest("hex"),"ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1");
const clone=v=>v===undefined?undefined:JSON.parse(JSON.stringify(v));
function diag(v){
  if(v===undefined)return["undefined"];
  if(v===null)return["null"];
  if(typeof v==="number") {const b=Buffer.alloc(8);b.writeDoubleBE(v);return["number",Number.isNaN(v)?"NaN":v===Infinity?"Infinity":v===-Infinity?"-Infinity":b.toString("hex")];}
  if(typeof v==="boolean")return["boolean",v];
  if(typeof v==="string")return["string",v];
  if(Array.isArray(v))return["array",v.map(diag)];
  if(v&&typeof v.name==="string"&&typeof v.message==="string"&&Object.prototype.toString.call(v)==="[object Error]")return diag({name:v.name,message:v.message});
  return["object",Object.entries(v).map(([k,v])=>[k,diag(v)])];
}
const row=(type=2)=>({ticker:"ABC",totalInCirculation:10,type,whitelistId:4,issuer:"alice",backupAddress:"backup"});
const document=rows=>({_id:"propertyIndex",value:JSON.stringify(rows)});
const call=(method,args=[],failures=[],tallyFailure=false)=>({method,args,failures,tallyFailure});
const replace=value=>({replace:value});
const cases=[];
function add(name,rows,actions,doc=document(rows)){cases.push({name,doc,actions});}
for(const [kind,type]of[["Fixed",1],["Managed",2],["Native",3],["Vesting",4],["Synthetic",5],["Non-Fungible",6],["Procedural",7]]){
  add(`kind-${type}`,[],[call("createToken",["ABC","1.234567895",kind,4,"alice","backup",{proceduralType:"8"}]),call("getPropertyIndex"),call("getPropertyData",["01"]),call("isManagedAndAdmin",[1,"alice"])]);
}
for(const id of[1,"1","01",true,[1],null,undefined,"s-7-1","s-7-2",99,"0x1","\ufeff1\u00a0"])
  add(`lookup-${cases.length}`,[[1,row()],["s-7-1",row(5)]],[call("getPropertyData",[id])]);
for(const amount of[".000000005","-.000000005",-2,0,"invalid",null,false,"Infinity","-Infinity","NaN","9007199254740993","1e-100",-0]){
  add(`grant-${cases.length}`,[[1,row(1)]],[call("grantTokens",["1","bob",amount,50])]);
  add(`redeem-${cases.length}`,[[1,row(7)]],[call("redeemTokens",[1,"bob",amount,51])]);
  add(`quantize-${cases.length}`,[],[call("quantize8",[amount])]);
}
add("singleton",[],[call("constructor"),call("getInstance")]);
add("load-duplicate-map-keys",[[2,row()],["2",row(3)],[2,row(7)]],[call("load"),call("getPropertyIndex"),call("getNextPropertyId")]);
add("load-composite-key-identity",[],[call("load"),call("getNextPropertyId")],{value:[[[1],row()],[{},row(3)],[[1],row(7)]]});
add("allocator-synthetic-key",[["s-7-1",row(5)]],[call("getNextPropertyId"),call("createToken",["NEW",1,"Fixed"])]);
add("allocator-negative-and-null",[[-5,row()],[null,row()]],[call("getNextPropertyId")]);
add("allocator-infinity",[],[call("getNextPropertyId")],{value:[[Infinity,row()]]});
add("map-negative-zero-nan",[],[call("addProperty",[-0,"ZERO",1,"Fixed"]),call("addProperty",[NaN,"NAN",1,"Managed"]),call("addProperty",[NaN,"NAN",2,"Managed"]),call("getPropertyIndex")]);
add("cached-ticker-return-error",[[1,row()]],[call("load"),call("createToken",["ABC",1,"Fixed"])]);
add("ticker-key-typeerror",[["ABC",row()]],[call("load"),call("createToken",["ABC",1,"Fixed"])]);
add("stale-duplicate-accepted",[[1,row()]],[call("createToken",["ABC",1,"Fixed"])]);
add("upsert-fields",[[1,{...row(),proceduralType:2}]],[call("load"),call("addProperty",[1,"",".000000005","Native",0,"","",{proceduralType:"9"}]),call("addProperty",[1,"NEW",-2,"Fixed",8,"bob","new-backup",{proceduralType:null}])]);
add("invalid-type",[],[call("addProperty",[1,"ABC",1,"managed"])]);
add("admin-stale-cache",[[1,row()]],[call("load"),call("updateAdmin",["01","bob",false]),call("updateAdmin",[1,"new-backup",true]),call("getPropertyIndex"),call("getPropertyData",[1])]);
add("admin-missing",[[1,row()]],[call("updateAdmin",[99,"bob",false])]);
add("admin-null-property",[[1,null]],[call("updateAdmin",[1,"bob",false])]);
add("managed-strict",[[1,row(2)],[2,row(7)],[3,row(1)],[4,{...row(),type:"2"}]],[...[[1,"alice"],[2,"alice"],["1","alice"],[1,"backup"],[3,"alice"],[4,"alice"],[99,"alice"]].map(a=>call("isManagedAndAdmin",a))]);
add("ticker-case-sensitive",[[1,row()]],[call("doesTickerExist",["ABC"]),call("doesTickerExist",["abc"]),replace(null),call("doesTickerExist",["ABC"])]);
add("broken-methods",[[1,row(5)]],[call("getAMM",[1,2]),call("isSyntheticToken",[1]),call("updateTotalInCirculation",[1,2]),call("updateTotalInCirculation",["1",2]),call("updateTotalInCirculation",[99,2])]);
add("string-id-duplicate",[["1",row()]],[call("grantTokens",["1","bob",1,50])]);
add("missing-grant-redeem",[],[call("grantTokens",[1,"bob",1,50]),call("redeemTokens",[1,"bob",1,50])]);
add("inspect-save",[[1,row()]],[call("inspectPropertyIndex"),call("save")]);
add("missing-clears-cache",[[1,row()]],[call("load"),replace(null),call("load")]);
for(const bad of["bad-json",[[1,row(),"extra"]],{no:"rows"}])add(`malformed-${cases.length}`,[[1,row()]],[call("load"),replace({value:bad}),call("load"),call("getPropertyData",[1]),call("isManagedAndAdmin",[1,"alice"])]);
add("stale-cache-query",[[1,row()]],[call("load"),replace(document([[2,{...row(),ticker:"NEW"}]])),call("getPropertyIndex"),call("getPropertyData",[2]),call("getNextPropertyId")]);
for(const operation of[0,1,2,3,4,5])add(`grant-db-failure-${operation}`,[[1,row()]],[call("grantTokens",[1,"bob",2,50],[{operation,name:"Error",message:"injected database failure"}])]);
add("grant-tally-failure",[[1,row()]],[call("grantTokens",[1,"bob",2,50],[],true)]);
for(const operation of[0,1])add(`save-failure-${operation}`,[],[call("addProperty",[1,"ABC",1,"Fixed"],[{operation,name:"Error",message:"injected database failure"}])]);
add("read-failure-stale",[[1,row()]],[call("load"),call("load",[],[{operation:1,name:"Error",message:"injected read failure"}])]);
add("empty-document",[],[call("load"),call("getPropertyData",[1]),call("isManagedAndAdmin",[1,"alice"]),call("updateAdmin",[1,"bob",false])],null);
add("getTallyMap-reference-domain",[],[call("getTallyMap")]);
add("null-meta-upsert-partial-cache",[[1,row()]],[call("load"),call("addProperty",[1,"NEW",2,"Fixed",8,"bob","new-backup",null]),call("getPropertyIndex")]);
add("null-meta-new-no-cache",[],[call("addProperty",[1,"NEW",2,"Fixed",8,"bob","new-backup",null])]);
for(const amount of[[1],[],[null],{},true])add(`numeric-coercion-${cases.length}`,[[1,row()]],[call("grantTokens",[1,"bob",amount,50])]);

async function run(c){
  let doc=clone(c.doc),effects=[],failures=[],operation=0,tallyFailure=false;
  const dbEffect=(op,value)=>{const f=failures.find(f=>f.operation===operation++);effects.push(["db",op,value,f?[[f.name,f.message]]:[]]);if(f)throw Object.assign(Error(f.message),{name:f.name});};
  const table={async findOneAsync(query){dbEffect("findOneAsync",clone(query));return clone(doc)},async updateAsync(query,update,options){dbEffect("updateAsync",[clone(query),clone(update),clone(options)]);doc=update.$set?{...doc,...clone(update.$set)}:clone(update);return 1}};
  const module={exports:{}};
  const context=vm.createContext({module,exports:module.exports,require(name){if(name==="path")return path;if(name==="bignumber.js")return BigNumber;if(name==="./db.js")return{async getDatabase(name){dbEffect("getDatabase",name);return table}};if(name==="./tally.js")return{async updateBalance(...args){effects.push(["tally","Tally.updateBalance",args,tallyFailure?["pendingThrow",["Error","injected tally failure"]]:["pending",undefined]]);if(tallyFailure)throw Error("injected tally failure")}};throw Error(name)},console:{log(){},error(){}}});
  vm.runInContext(source.toString("utf8"),context,{filename:"property.js",timeout:1000});
  const P=module.exports,pm=P.getInstance(),results=[];
  for(const action of c.actions){
    if(Object.hasOwn(action,"replace")){doc=clone(action.replace);results.push(["document"]);continue}
    effects=[];operation=0;failures=action.failures;tallyFailure=action.tallyFailure;
    if(action.method==="getTallyMap"){assert.equal(typeof vm.runInContext("getTallyMap",context)().updateBalance,"function");results.push(["unsupported","module-export-reference"]);continue}
    const singleton=["constructor","getInstance"].includes(action.method);let outcome;
    try{
      let value;
      if(action.method==="constructor")value=new P();
      else if(action.method==="quantize8")value=vm.runInContext("quantize8",context)(...action.args);
      else {const owner=typeof pm[action.method]==="function"?pm:P;value=await owner[action.method](...action.args)}
      if(singleton)assert.equal(value,pm);
      outcome=["return",singleton?undefined:value];
    }catch(error){outcome=["throw",[error.name,error.message]]}
    results.push([outcome,singleton,effects]);
  }
  return[results,doc===undefined?[]:[doc],[...pm.propertyIndex.entries()]];
}
(async()=>{
  const lines=["# Exact unchanged property.js; archived genuine BigNumber 9.3.1; lockfile 9.1.2 not executed.","# name\tinitial_document_diagnostic\tactions_diagnostic\texpected_diagnostic"];
  for(const c of cases){const actions=c.actions.map(a=>Object.hasOwn(a,"replace")?["document",a.replace===undefined?[]:[clone(a.replace)]]:["call",a.method,a.args,a.failures.map(f=>[String(f.operation),[f.name,f.message]]),a.tallyFailure]);lines.push([c.name,JSON.stringify(diag(c.doc===undefined?[]:[clone(c.doc)])),JSON.stringify(diag(actions)),JSON.stringify(diag(await run(c)))].join("\t"))}
  const out=lines.join("\n")+"\n",target=path.join(root,"fixtures/property_catalog_vectors.tsv");
  if(process.argv.includes("--check")){assert.equal(fs.readFileSync(target,"utf8"),out);console.log(`checked ${cases.length} property catalog vectors`)}else{fs.writeFileSync(target,out);console.log(`wrote ${cases.length} property catalog vectors`)}
})().catch(error=>{console.error(error);process.exitCode=1});
