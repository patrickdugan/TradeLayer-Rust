#!/usr/bin/env node
"use strict";
/*
 * File 6/49: execute BYTE-EXACT pinned txDecoder.js in a Node VM.
 * Genuine pinned BigNumber, Base94, Base256 and converter source execute
 * offline; no wallets, chain, DB or external network. Source VM is
 * observational only; a decoded object is NEVER an accepted transaction.
 */
const assert=require("node:assert/strict");
const fs=require("node:fs");
const path=require("node:path");
const crypto=require("node:crypto");
const vm=require("node:vm");
const ROOT=path.resolve(__dirname,"..");
const sourceArg=process.argv.indexOf("--source");
const filename=sourceArg>=0?path.resolve(process.argv[sourceArg+1]):path.join(ROOT,"fixtures/legacy_types_source_txDecoder.txt");
const bytes=Buffer.from(fs.readFileSync(filename,"utf8").replace(/\r\n/g,"\n"));
const sourceSha=crypto.createHash("sha1").update(Buffer.concat([
 Buffer.from("blob "+bytes.length+"\0"),bytes
])).digest("hex");
assert.equal(sourceSha,"4728f1bdce1190428f91ae5b02f380584073c431");

const {readPinnedSource,loadBigNumber}=require("./legacy_numeric_oracle.cjs");
function loadReference(name, hash, dependencies={}) {
 const code=readPinnedSource(path.join(ROOT,"fixtures",name),hash);
 const module={exports:{}};
 vm.runInNewContext(code,{module,exports:module.exports,console:{log(){},error(){}},
  require(key){assert(Object.hasOwn(dependencies,key),"unapproved reference import "+key);return dependencies[key]}
 },{timeout:2000,filename:name});
 return module.exports;
}
const BigNumber=loadBigNumber();
const converter=loadReference("legacy_base_converter_source.txt","a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62");
const base94=loadReference("legacy_base94_source.txt","0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb",{"bigint-base-converter":converter});
const actualBase256=loadReference("legacy_base256_source.txt","11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc",{"bigint-base-converter":converter});
let base256Calls=0;
const base256=new Proxy(actualBase256,{get(target,key){const value=target[key];return typeof value==='function'?function(...args){base256Calls++;return value.apply(target,args)}:value}});
const sourceModule={exports:{}};
vm.runInNewContext(bytes.toString("utf8"),{
 module:sourceModule,exports:sourceModule.exports,console:{log(){},error(){}},
 require(key){
  if(key==="bignumber.js")return BigNumber;
  if(key==="./base94.js")return base94;
  if(key==="./base256.js")return base256;
  throw Error("unapproved module import "+key);
 }
},{timeout:1000,filename:"pinned-txDecoder.js"});
const decoder=sourceModule.exports;
const methods={
 4:"decodeCommitToken",
 7:"decodeCreateWhitelist",
 8:"decodeUpdateAdmin",
 9:"decodeIssueOrRevokeAttestation",
 15:"decodeCloseOracle",
 21:"decodeWithdrawal",
 22:"decodeTransfer",
 30:"decodeStakeFraudProof",
 32:"decodeBatchMoveZkRollup",
 33:"decodeColoredCoin"
};
const lines=fs.readFileSync(path.join(ROOT,"fixtures/legacy_txdecoder_body_vectors.tsv"),"utf8")
 .replace(/\r\n/g,"\n").split("\n").filter(s=>s&&!s.startsWith("#")).map(s=>{
  const [rawId,body,tag]=s.split("\t");
  assert(tag&&body!==undefined);
  return {id:Number(rawId),body,tag};
});
assert.equal(lines.length,17);
function norm(value){return JSON.parse(JSON.stringify(value))}
let accepted=0,malformed=0;
for(const row of lines){
 const fn=decoder[methods[row.id]];
 assert.equal(typeof fn,"function");
 if(row.tag.endsWith("-throws")){
  assert.throws(()=>fn(row.body),{name:"TypeError"},row.tag);
  malformed++;
  continue;
 }
 const p=fn(row.body);
 switch(row.tag){
  case "commit-singleton-list":
   assert.equal(p.propertyId,7);assert.equal(p.amount,10/1e8);
   assert.equal(p.channelAddress,"channel1");assert.equal(p.ref,false);
   assert.equal(p.payEnabled,true);
   assert.deepEqual(Array.from(p.clearLists),[7]);
   assert.equal(p.isColoredOutput,false);assert.equal(p.commitClearlistId,11);
   break;
  case "commit-synthetic-raw-ref":
   assert.equal(p.propertyId,"s-1-10");
   assert.equal(p.amount,2/1e8);assert.equal(p.ref,"04");
   assert.equal(p.channelAddress,"");assert.deepEqual(Array.from(p.clearLists),[11]);
   assert.equal(p.isColoredOutput,true);assert.equal(p.commitClearlistId,2);
   break;
  case "commit-comma-bracket-misparse":
   assert.equal(Number.isNaN(p.clearLists[0]),true);
   assert.equal(p.commitClearlistId,0);
   assert.equal(p.isColoredOutput,false);
   malformed++;break;
  case "whitelist-four-fields":
   assert.deepEqual(norm(p),{
    backupAddress:"backupB",name:"ListOne",url:"url",description:"desc"
   });break;
  case "admin-base36-and-backup":
   assert.deepEqual(norm(p),{
    newAddress:"adminC",whitelist:true,oracle:false,
    token:false,id:11,updateBackup:true
   });break;
  case "attestation-revoke-uppercase":
   assert.deepEqual(norm(p),{
    revoke:true,id:11,targetAddress:"alice",metaData:"OK"
   });break;
  case "attestation-post-marker-selector-is-not-revoke":
   assert.equal(p.revoke,false);assert.equal(p.id,7);
   assert.equal(p.metaData,"OK");break;
  case "close-oracle-primitive":
   assert.equal(p,15);assert.equal(typeof p,"number");break;
  case "withdrawal-decimal-ref":
   assert.equal(p.withdrawAll,true);assert.equal(p.propertyId,7);
   assert.equal(p.amount,10/1e8);assert.equal(p.column,true);
   assert.equal(p.ref,10);assert.equal(p.channelAddress,"");break;
  case "withdrawal-bad-ref-false":
   assert.equal(p.withdrawAll,false);assert.equal(p.propertyId,"s-1-10");
   assert.equal(p.amount,2/1e8);assert.equal(p.ref,false);break;
  case "transfer-text-ref":
   assert.equal(p.propertyId,7);assert.equal(p.amount,10/1e8);
   assert.equal(p.isColumnA,true);assert.equal(p.toChannelAddress,"");
   assert.equal(p.ref,"0");assert.equal(p.pay,true);assert.equal(p.payRef,"payRef");
   break;
  case "stake-fraud-all-fields":
   assert.equal(p.action,1);assert.equal(p.oracleId,11);
   assert.equal(p.stakedPropertyId,7);assert.equal(p.amount,10/1e8);
   assert.equal(p.autoRoll,true);assert.equal(p.relayType,2);
   assert.equal(p.dlcRef,"ref");assert.equal(p.nextDlcRef,"next");
   break;
  case "zk-batch-opaque-not-validated":
   assert.equal(p.ordinalRevealJSON,row.body);assert.equal(p.valid,undefined);
   break;
  case "colored-decimal-action-base36-ratio":
   assert.equal(p.encodeDecodeRecode,2);assert.equal(p.propertyId,10);
   assert.equal(p.satsRatio,36);assert.equal(p.homeAddress,"home");
   assert.equal(p.valid,undefined);break;
  default:throw Error("unhandled vector "+row.tag);
 }
 accepted++;
}
assert.equal(accepted,14);
assert.equal(malformed,4);

// Exactly two declarations of the same object property; JS uses the LAST.
// base256 mock throws if the original first method were invoked.
const declarations=[...bytes.toString("utf8").matchAll(/^\s*decodeSettleChannelPNL\s*:/gm)];
assert.equal(declarations.length,2);
const settle=decoder.decodeSettleChannelPNL("one,two,12.5,2,1,0,99.25,10");
assert.equal(settle.markPrice,parseFloat(base94.fromBase94("12.5")));
assert.equal(settle.settleType,2);
assert.equal(settle.netAmount,parseFloat(base94.fromBase94("99.25")));
assert.equal(settle.expiryBlock,parseInt(base94.fromBase94("10")));
assert.equal(settle.close,undefined);

// The exported bridge helper is a different name from Types Type34 route.
assert.equal(typeof decoder.decodeAbstractionBridge,"function");
assert.equal(typeof decoder.decodeCrossLayerBridge,"undefined");
const helper=decoder.decodeAbstractionBridge("a,b,ignored-destination");
assert.equal(helper.propertyId,10);assert.equal(helper.amount,11);
assert.equal(Object.hasOwn(helper,"destinationAddr"),false);

// The send scalar branch intercepts four semicolon elements FIRST, even if
// elements themselves contain comma-delimited multi-property fields.
const malformedMulti=decoder.decodeSend("0;;5,7;a,b");
assert.equal(malformedMulti.propertyIds,5);
assert.equal(malformedMulti.amounts,10);
assert.equal(malformedMulti.isColoredOutput,false);
assert.equal(decoder.decodeAmount("a~"),10/1e8);
assert.equal(decoder.decodeAmount("a"),10);
assert.equal(decoder.decodePropertyId("s-1-a"),"s-1-10");
assert.equal(decoder.decodePropertyId("s1-a"),"s-1-10");

const outputFile=path.join(ROOT,'fixtures/legacy_txdecoder_historical_vectors.tsv');
function hex(text){return Buffer.from(text,'utf8').toString('hex')||'-'}
function tagged(value){
 if(value===undefined)return ['undefined'];
 if(value===null)return ['null'];
 if(typeof value==='boolean')return ['boolean',value];
 if(typeof value==='string')return ['string',value];
 if(typeof value==='number'){
  let text=Number.isNaN(value)?'NaN':value===Infinity?'Infinity':value===-Infinity?'-Infinity':null;
  if(text===null){const b=Buffer.alloc(8);b.writeDoubleBE(value);text=b.toString('hex')}
  return ['number',text];
 }
 if(Array.isArray(value))return ['array',Array.from(value,tagged)];
 assert.equal(typeof value,'object');
 return ['object',Object.keys(value).map(key=>[key,tagged(value[key])])];
}
const rows=[],seen=new Set();
function add(method,name,body){
 assert.equal(typeof decoder[method],'function',method);
 assert(Buffer.byteLength(body)<=4096,name);
 const key=method+'\0'+body;if(seen.has(key))return;seen.add(key);
 let result;
 try{result=['ok',hex(JSON.stringify(tagged(decoder[method](body))))]}
 catch(error){result=['throw',error.name,hex(error.message)]}
 rows.push([method,name,hex(body),...result].join('\t'));
}
const active=Object.keys(decoder);
assert.equal(active.length,36);
for(const method of active){
 for(const [name,body]of Object.entries({empty:'',zeros:'0,0,0,0,0,0,0,0,0,0,0,0,0,0,0',ones:'1,1,1,1,1,1,1,1,1,1,1,1,1,1,1',missing:'a,b',defaults:',,,,,,,,,,,,,,',prefix:' -01!? ,+1tail,0x10,0b11,0o7,NaN,Infinity,-0,1.2e+2tail',unicode:'☃,straße,ıſﬃé,\u0000\n\u2028,\ufeff-0',framed:'tl91,7,alice,ok'}))add(method,name,body);
}
const normal={decodeAmount:"a~",
 decodeActivateTradeLayer:'0;2;30;bad,hash',decodeTokenIssue:'z,TOKEN,7;b,1,backup,1,0,2',
 decodeSend:'0;alice;7;a~',decodePropertyId:'s-1-a',decodeTradeTokenForUTXO:'s-1-a,a,1,10,0x10,-2,1,tag',
 decodeCommitToken:'s-1-a,a,ref:04:ignored,1,[7],1,b',decodeOnChainTokenForToken:'7,s-1-a,1.a,2.i,1,0',
 decodeCancelOrder:'0,7,b,8,99,1',decodeCreateWhitelist:'backup,list,url,description',decodeUpdateAdmin:'admin,1,0,0,b,1',
 decodeIssueOrRevokeAttestation:'1,b,alice,straßeıſﬃ',decodeAMMPool:'1,1,s-1-a,a,7,b,ignored,0x10,1e2,blob',
 decodeGrantManagedToken:'7,a,alice,legacy,template,contract,state,trailing',decodeRedeemManagedToken:'7,a,template,contract,state',
 decodeCreateOracle:'PRICE,url,backup,7;b,c',decodePublishOracleData:'1,7ps,1,2,3',decodeCloseOracle:'f',
 decodeCreateFutureContractSeries:'0,1,7:a;:b;z:;1:2:3,s-1-a,a,7,10.5e1tail,5,2,1,1',
 decodeExerciseDerivative:'a,b',decodeTradeContractOnchain:'1,4ldqpds,2,1,0,1,0,1',decodeTradeContractChannel:'1,4ldqpds,2,1,10,0,1',
 decodeTradeTokensChannel:'7,s-1-a,a,b,1,10,1,1,0',decodeWithdrawal:'1,s-1-a,a,1,ref:-10junk:ignored',
 decodeTransfer:'s-1-a,a,1,ref:04:ignored,1,pay',decodeMintSynthetic:'s-1-a,b,a',decodeRedeemSynthetic:'s-1-a,b,a',
 decodePayToTokens:'a,b,c',decodeOptionTrade:'CALL,1a~,b,1,c,1,PUT,a,2',decodeSettleChannelPNL:'☃,rawtx,1|!,3,1,0,2|",#,!,",#,0,root,$,%',
 decodeKingSettle:'!,",#,1|!,1,root,$,%',decodeStakeFraudProof:'1,b,7,a,alice,hash,2,state,ref,settle,blob,1,next',
 decodeBatchMoveZkRollup:'{"x":1}\n\u0000☃',decodePublishNewTx:'[1,2,"☃"]',decodeColoredCoin:'2,a,10,home',decodeAbstractionBridge:'a,b,destination'
};
assert.deepEqual(Object.keys(normal).sort(),active.slice().sort());
for(const [method,body]of Object.entries(normal))add(method,'normal',body);
const numeric=['','0','-0','+0','-00','+','-','  -0  ','\ufeff+1tail','\u00851','\u200b1','\u20001','0x10','0Xff','0b10','0o10','012','1.5','-1.5','.i','1.','1.A','1.aA','1e3','1e-3','NaN','nan','Infinity','-Infinity','infinity','1_0','1..2',' ','.','-.','-.0','+.0','+.','aA','AA','0XFF','0x.1','0xFf','0xFF','0x0x11','0b0b11','0o0o11','0xInfinity','-0xInfinity','0xNaN','0x-1','+0xFF','0x+1','0x0Xff','0x00','0x-','0x1..2','0~','-0~','a~','-a~','a~~','zzzzzzzzzz','zzzzzzzzzzzzzzzzzzzz','0000000zzzzzzzzzzzzzzzzzzzz','z'.repeat(230),'z'.repeat(4096),'0'.repeat(4096)];
for(let i=0;i<numeric.length;i++){
 for(const method of ['decodeAmount','decodePropertyId','decodeCloseOracle'])add(method,'numeric-'+i,numeric[i]);
 if(Buffer.byteLength(numeric[i])<4000){
  add('decodeTradeTokenForUTXO','bn36-'+i,'7,'+numeric[i]+',1,a,1,1');
  add('decodeCancelOrder','bn10-'+i,'0,7,b,'+numeric[i]+',99,1');
  add('decodePublishOracleData','oracle-'+i,'1,'+numeric[i]+',a,b,c');
  add('decodeCreateFutureContractSeries','float-'+i,'0,1,,7,1,7,'+numeric[i]);
 }
}
for(const property of ['s','s-','s1','s1-a','s-1-a','s-1-a-b','s--a','s+1-a','s-1-?','s-zzzzzzzzzzzzzzzzzzzz-1','S-1-a','s1tail-a!']){
 for(const method of ['decodePropertyId','decodeMintSynthetic','decodeRedeemSynthetic'])add(method,'synthetic-'+hex(property),method==='decodePropertyId'?property:property+',1,a');
}
for(const body of ['1;alice','0','0;alice','0;alice;7','0;alice;5,7;a,b','0;alice;5,7;a,b;1','0;alice;5,7;a,b;1;1','0;alice;5,7,9;a;1;1','0;alice;5,7;;0;0','0;alice;?;NaN','0;;5,7;a,b;;;;','0;alice;7;0;1','0;alice;7;0;1;1','0;alice;7;0;1;1;1'])add('decodeSend','send-'+hex(body),body);
for(const truth of ['1','01','+1','1.0','1e0','0x1',' 1 ','true','0','NaN','0x0001','0b0001','0o0001','+0x1','-0x1','0x1.0','0x1tail','1e+000','+1.']){
 add('decodeCancelOrder','loose-'+truth,truth+',7,b,8,99,1');
 add('decodeUpdateAdmin','strict-'+truth,'alice,'+truth+','+truth+','+truth+',1,'+truth);
}
for(const len of [19,20,21])for(const text of ['x','☃','😀']){
 add('decodeCancelOrder','txid-len-'+len+'-'+hex(text),'1,7,0,'+text.repeat(len));
 add('decodeCancelOrder','spot-txid-len-'+len+'-'+hex(text),'0,7,b,0,'+text.repeat(len));
}
for(const body of ['7,a,channel,1,[],0','7,a,channel,1,[7,b],1,2','7,a,ref:,0,[?],0,?','7,a,,0,b,0','7,a'])add('decodeCommitToken','commit-'+hex(body),body);
for(const body of ['1,7,a,1,ref:','1,7,a,1,ref:0x10','1,7,a,1,ref:-0','1,7,a,1,ref:+10junk','1,7,a,1,ref:Infinity'])add('decodeWithdrawal','withdraw-'+hex(body),body);
for(const body of ['x,1~,2,0,0,0,','x,a,1,0,0,0,,','x,a,1,0,0,0,,a,?'])add('decodeOptionTrade','combo-'+hex(body),body);
for(const body of ['1,1,7:a;:b;1:2:3,7,1,7,10','0,1,7:a;:b;1:2:3,7,1,7,.5e+2suffix','0,1,,7,1,7,Infinitytail'])add('decodeCreateFutureContractSeries','native-'+hex(body),body);
for(const raw of ['!','0','|','||','1|!','!|!!','!|!|#','~','?',' ', '☃','\u{1f600}','\u0000','z'.repeat(260)]){
 add('decodeSettleChannelPNL','base94-'+hex(raw),'raw1,raw2,'+raw+',0,0,0,'+raw+','+raw+','+raw);
 add('decodeKingSettle','base94-'+hex(raw),raw+','+raw+','+raw+','+raw+',0,root,'+raw+','+raw);
}
// Fixed host corpus includes prefixes/leading-zero alignment and values beyond
// safe integer precision. This freezes the actual executing V8 parseInt policy.
let seed=0x65847291;
function next(){seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed}
for(let i=0;i<512;i++){
 const length=1+next()%220;let raw='';for(let j=0;j<length;j++)raw+=(next()%36).toString(36);
 raw='0'.repeat(next()%8)+raw;
 if(i%3===0)raw='-'+raw;if(i%5===0)raw='\ufeff +'+raw.replace(/^-/,'');if(i%7===0)raw+='!tail';
 add('decodeAmount','seed-'+i,raw+(i%2?'~':''));
}
assert.equal(base256Calls,0,'the active duplicate never calls Base256');
const covered=new Set(rows.map(row=>row.split('\t')[0]));assert.deepEqual([...covered].sort(),active.slice().sort());
const result='# Pinned full txDecoder.js dca0888b; genuine archived BigNumber9.3.1/Base94/Base256/converter.\n'+
 '# method\tcase\tbody UTF8 hex (- empty)\tok\ttagged JSON UTF8 hex OR throw\tname\tmessage UTF8 hex\n'+rows.join('\n')+'\n';
if(process.argv.includes('--write'))fs.writeFileSync(outputFile,result);
else assert.equal(fs.readFileSync(outputFile,'utf8').replace(/\r\n/g,'\n'),result,'historical decoder fixture differs; regenerate --write');
console.log('PASS: original17 strict vectors plus '+rows.length+' historical observations across '+covered.size+' active exports');
console.log('PASS: full pinned source and genuine numeric/base references; later Type23 shadows Base256; missing dispatch stays absent');
console.log('BOUNDARY: string bodies <=4096UTF8 bytes; source VM host numeric policy; no transaction/authentication/economic authority');
