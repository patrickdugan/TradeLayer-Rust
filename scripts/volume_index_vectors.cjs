#!/usr/bin/env node
"use strict";
// Executes the pinned class with an in-memory DB stub. No daemon/DB/network runs.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const crypto = require("node:crypto");
const { fixtures, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const args = process.argv.slice(2);
const sourceFlag = args.indexOf("--source-root");
if (sourceFlag >= 0 && !args[sourceFlag + 1]) {
  throw Error("usage: node scripts/volume_index_vectors.cjs [--source-root PATH] [--write]");
}
const sourcePath = sourceFlag >= 0 ? path.join(path.resolve(args[sourceFlag + 1]), "src", "volumeIndex.js")
  : path.join(fixtures, "legacy_volume_index_source.txt");
const source = fs.readFileSync(sourcePath, "utf8").replace(/\r\n/g, "\n");
const digest = crypto.createHash("sha256").update(source).digest("hex");
if (digest !== "d4d4329254125ce8e4217d22b0ba57a1581c6279df999af60281b0365dffb08c") {
  throw Error("pinned volumeIndex.js source hash mismatch");
}
const BigNumber = loadBigNumber();
let docs = [];
const database = {
  findAsync: async () => docs,
  findOneAsync: async () => null,
  updateAsync: async () => { throw Error("unexpected source DB write"); },
};
const context = {
  module: { exports: {} },
  console: { log() {}, warn() {}, error() {} },
  require(name) {
    if (name === "bignumber.js") return BigNumber;
    if (name === "./db.js") return { getDatabase: async () => database };
    if (["node-fetch", "litecoin", "util", "./contractRegistry.js"].includes(name)) return {};
    throw Error(`unexpected dependency: ${name}`);
  },
};
vm.runInNewContext(source, context, { filename: "pinned-volumeIndex.js", timeout: 1000 });
const VolumeIndex = context.module.exports;
const cases = [
  ["empty", false, []],
  ["zero_weight", false, [[0, 900000000]]],
  ["weighted", false, [[200000000, 125000000], [300000000, 250000000]]],
  ["amounts", false, [[200000000, 1000000000], [300000000, 2000000000]]],
  ["contract_swaps", true, [[200000000, 1000000000], [300000000, 2000000000]]],
  ["fractional", false, [[1000000, 123000000], [2000000, 321000000]]],
  ["half_atom_rounds_up", false, [[100000000, 1], [100000000, 2]]],
  ["below_half_atom", false, [[200000000, 1], [100000000, 2]]],
];
async function main() {
  const rows = [];
  for (const [name, contract, entries] of cases) {
    const inputs = entries.map(([a, b]) => ({ amount1: a / 1e8, amount2: b / 1e8 }));
    const raw = VolumeIndex.calculateVWAP(inputs, contract);
    const normalized = entries.map(([a, b]) => contract ? [b, a] : [a, b]);
    docs = normalized.map(([volume, price]) => ({ value: { volume: volume / 1e8, price: price / 1e8 } }));
    const rounded = await VolumeIndex.getVWAP(1, 2, 100, 10);
    const atoms = rounded === null ? "null" : new BigNumber(rounded).times(1e8).toFixed(0);
    rows.push([name, contract ? "1" : "0", entries.map(e => e.join(":")).join(","), String(raw), atoms].join("\t"));
  }
  const vwap = "# pinned=" + digest + "\n# scenario\tcontract\tamount1_atoms:amount2_atoms\tcalculateVWAP_number\tgetVWAP_price_atoms\n" + rows.join("\n") + "\n";
  const fees = [[0,35,100],[1,35,100],[9,35,100],[100000000,35,100],[123456789,2,100],[987654321,99,100],[101,1,1000]];
  const feeRows = fees.map(([fee, n, d]) => {
    const reward = VolumeIndex.calculateAttenuatedLiquidityReward(fee / 1e8, 0, { maxShare: n/d, minShare: n/d, slope: 0 });
    return [fee,n,d,new BigNumber(reward).times(1e8).toFixed(0)].join("\t");
  });
  const feeText = "# Source attenuation's constant-share subset only; no logarithm parity claim.\n# fee_atoms\tshare_numerator\tshare_denominator\treward_atoms\n" + feeRows.join("\n") + "\n";
  for (const [file, expected] of [["volume_index_vwap.tsv", vwap], ["volume_index_fee.tsv", feeText]]) {
    const target = path.join(__dirname, "..", "fixtures", file);
    if (args.includes("--write")) fs.writeFileSync(target, expected);
    else if (fs.readFileSync(target, "utf8").replace(/\r\n/g,"\n") !== expected) throw Error(`source vector drift: ${file}`);
  }
  await fullSourceCorpus();
  console.log(`Pinned volume source verified: ${rows.length} VWAP and ${feeRows.length} constant-share fee vectors`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });

// Full method inventory and finite arithmetic, with actual source unchanged.
// async-getDatabase + Promise-findAsync is the real db.js boundary shape.
// resolved/cursor modes are explicitly named adapters for otherwise broken paths.
function observe(value) {
  if(value===undefined)return {$undefined:true};
  if(typeof value==="number" && (!Number.isFinite(value)||Object.is(value,-0)))return {$number:String(value),bits:bits(value)};
  if(Array.isArray(value))return value.map(observe);
  if(value && typeof value==="object"){const result={};for(const key of Object.keys(value))result[key]=observe(value[key]);return result;}
  return value;
}
function bits(value){const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString("hex");}
function resultBits(result){return result===null?"none":typeof result==="number"?(Number.isFinite(result)?bits(result):"nonfinite"):result===undefined?"undefined":"throw";}
function at(document,key){return key.split('.').reduce((value,k)=>value?.[k],document);}
function matches(document,query){return Object.entries(query).every(([key,want])=>{
  const value=at(document,key);if(want&&typeof want==="object")return Object.entries(want).every(([op,arg])=>op==="$lt"?value<arg:op==="$gte"?value>=arg:op==="$lte"?value<=arg:op==="$in"?arg.includes(value):op==="$regex"?new RegExp(arg).test(value):false);return value===want;
});}
function environment(options={}){
  let documents=structuredClone(options.docs??[]);const trace=[],logs=[];let writes=0;
  const database={
    findOneAsync(query){trace.push({method:"findOneAsync",arguments:[observe(query)]});if(options.failRead)throw Error("read failed");return Promise.resolve(documents.find(d=>matches(d,query))??null);},
    findAsync(query,projection){trace.push({method:"findAsync",arguments:[observe(query),observe(projection)]});if(options.failRead)throw Error("read failed");let found=documents.filter(d=>matches(d,query));
      if(options.cursor)return {sort(order){trace.push({method:"cursor.sort",arguments:[order]});const [key,direction]=Object.entries(order)[0];found.sort((a,b)=>at(a,key)===at(b,key)?0:at(a,key)==null?1:at(b,key)==null?-1:(at(a,key)<at(b,key)?-1:1)*direction);return this;},limit(limit){trace.push({method:"cursor.limit",arguments:[limit]});if(limit>0)found=found.slice(0,limit);return this;},then(resolve,reject){return Promise.resolve(found).then(resolve,reject);}};
      return Promise.resolve(found);
    },
    async updateAsync(query,document,settings){trace.push({method:"updateAsync",arguments:[observe(query),observe(document),observe(settings)]});writes++;if(options.failWrite===writes)throw Error(`write failed ${writes}`);const index=documents.findIndex(d=>matches(d,query));if(index<0)documents.push(structuredClone({...document,_id:document._id??query._id}));else documents[index]=structuredClone({...document,_id:document._id??documents[index]._id});return 1;},
  };
  const getDatabase=category=>{trace.push({method:"getDatabase",arguments:[category]});return options.dbMode==="resolved"?database:Promise.resolve(database);};
  const contracts={async getContractInfo(id){trace.push({method:"Contracts.getContractInfo",arguments:[observe(id)]});if(options.failContract)throw Error("contract failed");return options.contract??{};}};
  const context={module:{exports:{}},Date:{now:()=>options.now??12345},console:{log:(...a)=>logs.push(["log",...a.map(v=>typeof v==="object"?observe(v):v)]),warn:(...a)=>logs.push(["warn",...a.map(observe)]),error:(...a)=>logs.push(["error",...a.map(v=>v?.name&&v?.message?{name:v.name,message:v.message}:observe(v))])},
    require(name){if(name==="bignumber.js")return BigNumber;if(name==="./db.js")return {getDatabase};if(name==="./contractRegistry.js")return contracts;if(["node-fetch","litecoin","util"].includes(name))return {};throw Error(`unexpected dependency ${name}`);}};
  if(options.dbPath!==undefined)context.dbPath=options.dbPath;
  vm.runInNewContext(source,context,{filename:"pinned-volumeIndex-full.js",timeout:1000});
  const Type=context.module.exports;if(options.global!==undefined)Type.globalCumulativeVolume=options.global;
  return {Type,trace,logs,documents:()=>documents};
}
async function execute(method,arguments_,options={}){
  const env=environment(options);let value,status;
  try{value=method==="constructor"?new env.Type({ignored:true}):method==="program"?await Promise.all([]):await env.Type[method](...arguments_);
    if(method==="program"){value=[];for(const [name,params]of arguments_)value.push(await env.Type[name](...params));}status="ok";
  }catch(error){status="throw";value={name:error.name,message:error.message};}
  return {status,value,documents:env.documents(),trace:env.trace,logs:env.logs};
}
async function fullSourceCorpus(){
  const assert=require("node:assert/strict");const hx=value=>Buffer.from(JSON.stringify(observe(value))).toString("hex");
  const rows=[`# complete unchanged volumeIndex.js ${digest}; finite Number domain; real BigNumber9.3.1`,"# I static-method-namesJSONhex; M name method argsHEX optionsHEX status resultHEX docsHEX traceHEX logsHEX", "# U unit: name propertyId notionalbits leveragebits tokenpricebits details result; T token volume/prices; C contract volume/collateral/notional/inverse/tradeprice; R share; F fee; V volatility; W Number VWAP"];
  const quirks=["# Q name method argsHEX optionsHEX status resultHEX docsHEX traceHEX logsHEX note"];
  const inventory=Object.getOwnPropertyNames(environment().Type).filter(k=>!['length','name','prototype'].includes(k));assert.equal(inventory.length,27);rows.push(["I",hx(inventory)].join("\t"));
  async function scene(name,method,arguments_,options={},note){const result=await execute(method,arguments_,options);const row=[note?"Q":"M",name,method,hx(arguments_),hx(options),result.status,hx(result.value),hx(result.documents),hx(result.trace),hx(result.logs)];if(note){row.push(note);quirks.push(row.join("\t"));}else rows.push(row.join("\t"));return result;}
  const basic=[['constructor',[]],['saveVolumeDataById',['1-2',2,3,4,10,'UTXO']],['getUTXOEquivalentVolume',[2,'1','contract',7,10,false,100]],['updateCumulativeVolumes',[3,'UTXO','1-2',10]],['getTokenPriceInLTC',[7]],['calculateVolIndex',[]],['_normalizeContractInfo',[1]],['getContractUnitLTCValue',[1]],['getContractUnitLTCValueDetails',[1]],['getLastPrice',['1-2',10]],['getCumulativeVolumes',[10]],['getBlockVolumes',[10]],['getVolumeDataById',['1-2']],['sampleVolumesByBlock',[10]],['sampleVolumesByBlockRange',[1,10]],['calculateCumulativeVolume',[1,2]],['saveCumulativeVolume',[1,2,3]],['auditVWAP',[10]],['calculateVWAP',[[{amount:2,price:4}]]],['getVwapData',[1,2,10]],['saveVWAP',[1,10,2]],['computeLiquidityRewardShare',[1000]],['calculateAttenuatedLiquidityReward',[1,1000]],['calculateLiquidityReward',[100,1,{block:10}]],['getVWAP',[1,2,10,5]],['baselineLiquidityReward',[100,0.1,1,{block:10}]],['getTLPriceInLTC',[]],['vestTokens',[100]]];
  for(const [method,params]of basic)await scene(`default-${method}`,method,params);
  const doc=(id,height,price,volume,ltcVolume=volume)=>({_id:id,value:{blockHeight:height,price,volume,rawVolume:volume,ltcVolume}});
  const samples=[{_id:'1-2-1',blockHeight:8,volume:2,value:{price:5,volume:2}},{_id:'1-2-2',blockHeight:10,volume:3,value:{price:10,volume:3}},doc('1-2',10,4,2),doc('2-1',9,8,2),doc('0-7',11,0.25,10),{_id:'globalCumulativeVolume',value:{block:9,globalCumulativeVolume:40}},{_id:'ltcPairCumulativeVolume',value:{block:9,ltcPairTotalVolume:10}}];
  for(const method of ['getVolumeDataById','sampleVolumesByBlock','sampleVolumesByBlockRange','calculateCumulativeVolume','auditVWAP','getVwapData']){const item=basic.find(([m])=>m===method);await scene(`resolved-boundary-${method}`,method,item[1],{docs:samples,dbMode:'resolved'});}
  for(const [method,params]of [['getLastPrice',['1-2',1]],['getLastPrice',['2-0',1]],['getCumulativeVolumes',[9]],['getCumulativeVolumes',[10]],['getBlockVolumes',[10]],['getVWAP',[1,2,10,5]],['getTokenPriceInLTC',[7]],['getTLPriceInLTC',[]],['calculateLiquidityReward',[100,1,{block:9}]],['baselineLiquidityReward',[100,0.2,1,{block:9}]]])await scene(`populated-${method}-${rows.length}`,method,params,{docs:samples});
  await scene('repeat-save-overwrite-double-count','program',[["saveVolumeDataById",['1-2',1,3,5,10,'UTXO']],["saveVolumeDataById",['1-2',1,3,5,10,'UTXO']],["getCumulativeVolumes",[11]],["getBlockVolumes",[10]],["getVWAP",[1,2,10,10]]]);
  await scene('save-schema-mismatch','program',[["saveVolumeDataById",['1-2',1,3,5,10,'token']],["sampleVolumesByBlock",[10]],["getVWAP",[1,2,10,10]]],{dbMode:'resolved'});
  await scene('context-fallback-global','calculateLiquidityReward',[100,7,{feeRate:0.001,block:9}],{docs:[samples[5]]});
  await scene('max-context-exclusive-bound','calculateLiquidityReward',[0,7,{feePaid:1,block:Number.MAX_SAFE_INTEGER}]);
  await scene('negative-cumulative-permitted','program',[["updateCumulativeVolumes",[-0.000000001,'UTXO','x',10]],["getCumulativeVolumes",[11]]]);
  await scene('ctor-injected-undefined-global','constructor',[],{dbPath:'explicit-adapter-path'},'Constructor only works after explicitly injecting the undeclared dbPath global.');
  await scene('partial-save-second-write-failure','saveVolumeDataById',['1-2',1,3,5,10,'UTXO'],{failWrite:2},'Raw document survives failed cumulative write; source operation is not atomic.');
  await scene('partial-save-third-write-failure','saveVolumeDataById',['1-2',1,3,5,10,'UTXO'],{failWrite:3},'LTC cumulative survives failed global write.');
  await scene('contract-total-never-updated','program',[["saveVolumeDataById",['1',1,3,5,10,'contract']],["getCumulativeVolumes",[11]]],{},'Contract type increases only global; no contract cumulative document is written.');
  await scene('source-id-zero-character','getBlockVolumes',[10],{docs:[doc('10-21',10,3,4,7),doc('1-2',10,3,4,2)]},'Includes every zero in source document ID, not exact native-token pair identity.');
  await scene('source-ignored-height','getLastPrice',['1-2',1],{docs:[doc('1-2',100,8,1)]},'Future document is returned despite height1.');
  await scene('source-inverse-first-two-components','getLastPrice',['1-2-extra',1],{docs:[doc('2-1',100,4,1)]},'Inverse only uses first2 hyphen components.');
  await scene('source-zero-price-fallback','getTokenPriceInLTC',[7],{docs:[doc('0-7',10,0,1)]},'Truthy check substitutes0.001 for numeric zero.');
  await scene('source-zero-string-truthy','getTokenPriceInLTC',[7],{docs:[doc('0-7',10,'0',1)]},'String zero is truthy and returned unchanged; outside numeric typed store.');
  await scene('source-string-volume-coercion','calculateVWAP',[[{amount:'2',price:4},{amount:'3',price:5}]],{},'Number+= string concatenates total to023, multiplication still coerces.');
  await scene('source-contract-missing-collateral','getUTXOEquivalentVolume',[1,'1','contract',0,1,false,100],{},'Falsy collateral ID bypasses contract path, yielding undefined.');
  await scene('source-contract-zero-notional','getUTXOEquivalentVolume',[1,'1','contract',7,0,false,100],{},'Falsy notional bypasses contract path.');
  await scene('source-unsupported-volume-type','getUTXOEquivalentVolume',[1,'1','UTXO'],{},'No generic UTXO branch: returns undefined.');
  await scene('source-flat-normalized','_normalizeContractInfo',[9],{contract:{id:1,notionalValue:'2.5',leverage:0,native:'false',inverse:0,onChainData:'bad'}},'Flat/wrapped shapes and JS truthiness/Number defaults are retained.');
  await scene('source-nested-normalized','_normalizeContractInfo',[9],{contract:{id:1,data:{ticker:'X',notionalPropertyId:'LTC',notionalValue:null,leverage:'0',inverse:'false'}}},'Nullish notional defaults1; zero leverage defaults1; string false is truthy.');
  await scene('source-unit-catches-provider-failure','getContractUnitLTCValue',[1],{failContract:true},'Normal unit helper catches errors and returns0.');
  await scene('source-details-propagates-provider-failure','getContractUnitLTCValueDetails',[1],{failContract:true},'Details variant propagates errors and throws for zero value.');
  await scene('source-cumulative-read-failure-swallowed','getCumulativeVolumes',[10],{failRead:true},'Individual read errors are caught independently and return zero fields.');
  await scene('source-last-read-failure-null','getLastPrice',['1-2',10],{failRead:true},'Last-price read errors are caught and return null.');
  await scene('source-block-read-failure-undefined','getBlockVolumes',[10],{failRead:true},'Block-volume catch returns console.error undefined.');
  await scene('source-regex-unescaped','calculateCumulativeVolume',['1.*',2],{dbMode:'resolved',docs:[{_id:'123-2-x',volume:7}]},'Caller text is interpolated into unescaped regular expression.');
  await scene('source-recent-options-not-cursor','getVwapData',[1,2,1],{dbMode:'resolved',docs:samples},'Second findAsync argument is not a cursor sort/limit. Explicit adapter records it without honoring options.');
  await scene('source-vesting-threshold-failure','vestTokens',[1],{global:1e10},'Log threshold reaches undeclared totalVolume.');
  for(const global of [0,1e9,-1])await scene(`source-vesting-zero-${global}`,'vestTokens',[1],{global},'Undefined/zero/low globals return zero tier metadata, no issuance.');
  // All numeric expectations come from the actual complete class, not formulas here.
  const expected=async(method,params,options)=>{const r=await execute(method,params,options);return r.status==='throw'?'throw':resultBits(r.value);};
  for(const [name,pid,notional,leverage,price]of [['base',0,1,10,99],['token',7,2.5,4,0.25],['zero-leverage',0,1,0,1],['binary-half-low',0,1.000000005,1,1],['tiny',0,0.000000005,1,1],['negative',0,-1.25,2,1],['negative-leverage',0,1.25,-2,1],['negative-zero',0,-0,1,1],['large',0,1e21,1,1],['overflow-notional',7,1e308,1,1e308],['underflow',7,1e-300,1,1e-300],['division-negative-zero',0,1e-300,-1e308,1],['negative-tiny-nonzero',0,1e-300,-1,1]])for(const details of[false,true]){
    const options={contract:{data:{notionalPropertyId:pid,notionalValue:notional,leverage}},docs:[doc('0-7',10,price,1)]};const e=await expected(details?'getContractUnitLTCValueDetails':'getContractUnitLTCValue',[1],options);rows.push(['U',`${name}-${details}`,pid,bits(notional),bits(leverage),bits(price),+details,e].join('\t'));
  }
  for(const [name,volumes,prices]of [['plain',[2,3],[0.1,0.2]],['fractional',[0.1,0.2],[0.1,0.2]],['negative',[-2,3],[-0.5,2]],['overflow',[1e308,1e308],[1e308,1e308]]]){const e=await expected('getUTXOEquivalentVolume',[volumes,'1-2','token'],{docs:[doc('0-1',10,prices[0],1),doc('0-2',10,prices[1],1)]});rows.push(['T',name,...volumes.map(bits),...prices.map(bits),e].join('\t'));}
  for(const [name,volume,collateral,notional,price]of [['plain',2,0.25,10,100],['fraction',0.1,0.3,0.2,1.23456789],['negative',-0.000000001,1,1,1],['negative-zero',-0,1,1,1],['large',1e10,2,3,4]])for(const inverse of[false,true]){const e=await expected('getUTXOEquivalentVolume',[volume,'1','contract',7,notional,inverse,price],{docs:[doc('0-7',10,collateral,1)]});rows.push(['C',`${name}-${inverse}`,bits(volume),bits(collateral),bits(notional),+inverse,bits(price),e].join('\t'));}
  let seed=0x435189ad;const random=()=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed/2**32;};
  const rewardCases=[[0,0.35,0.02,1000,1],[1000,0.35,0.02,1000,1],[1e12,0.35,0.02,1000,1],[-1,0.35,0.02,1000,1],[1e308,0.35,0.02,-1,0],[1e12,3,2,1,1],[0,-0.35,-0.5,1000,1]];
  for(let i=0;i<80;i++)rewardCases.push([random()*1e11,random(),random()*0.1,random()*10000,random()*4]);
  for(let i=0;i<rewardCases.length;i++){const[c,maxShare,minShare,pivotLtc,slope]=rewardCases[i];const opts={maxShare,minShare,pivotLtc,slope};rows.push(['R',`reward-${i}`,...rewardCases[i].map(bits),await expected('computeLiquidityRewardShare',[c,opts])].join('\t'));for(const fee of[i%3===0?1e-9:random()*100])rows.push(['F',`fee-${i}`,bits(fee),...rewardCases[i].map(bits),await expected('calculateAttenuatedLiquidityReward',[fee,c,opts])].join('\t'));}
  const negativeZeroOptions={maxShare:-0,minShare:-0,pivotLtc:1000,slope:0};
  rows.push(['R','negative-zero-options',...[0,-0,-0,1000,0].map(bits),await expected('computeLiquidityRewardShare',[0,negativeZeroOptions])].join('\t'));
  rows.push(['F','negative-zero-options',bits(1),...[0,-0,-0,1000,0].map(bits),await expected('calculateAttenuatedLiquidityReward',[1,0,negativeZeroOptions])].join('\t'));
  for(let mask=0;mask<32;mask++){
    const values=Array.from({length:5},(_,bit)=>(mask&(1<<bit))?-0:0);
    const[cumulative,maxShare,minShare,pivotLtc,slope]=values;
    const opts={maxShare,minShare,pivotLtc,slope};
    rows.push(['R',`zero-grid-${mask}`,...values.map(bits),await expected('computeLiquidityRewardShare',[cumulative,opts])].join('\t'));
    rows.push(['F',`zero-grid-${mask}`,bits(1),...values.map(bits),await expected('calculateAttenuatedLiquidityReward',[1,cumulative,opts])].join('\t'));
  }
  for(const [name,prices]of [['empty',[]],['one',[100]],['two',[100,101]],['three',[100,101,103]],['flat',[100,100,100]],['filtered',[0,-1,100]],['all-filtered',[0,-1]],['extreme',[1e-300,1e300,1]]]){const e=await expected('calculateVolIndex',[14400],{cursor:true,docs:prices.map((p,i)=>doc(`x${i}`,100-i,p,1))});rows.push(['V',name,prices.map(bits).join(',')||'-',e].join('\t'));}
  for(let i=0;i<24;i++){const entries=Array.from({length:3+Math.floor(random()*8)},()=>({amount:random()*100,price:random()*100}));const result=await expected('calculateVWAP',[entries]);rows.push(['W',`number-vwap-${i}`,entries.map(e=>`${bits(e.amount)}:${bits(e.price)}`).join(','),result].join('\t'));}
  const full=rows.join('\n')+'\n',quirkText=quirks.join('\n')+'\n';for(const[file,text]of[['volume_index_methods.tsv',full],['volume_index_quirks.tsv',quirkText]]){const filePath=path.join(fixtures,file);if(args.includes('--write'))fs.writeFileSync(filePath,text);else assert.equal(fs.readFileSync(filePath,'utf8').replace(/\r\n/g,'\n'),text,`source drift: ${file}`);}
  console.log(`Full VolumeIndex: ${inventory.length} static methods +constructor, ${rows.filter(r=>r.startsWith('M\t')).length} scenes, ${rows.filter(r=>/^[UTCRFVW]\t/.test(r)).length} numeric vectors, ${quirks.length-1} quirks`);
}
