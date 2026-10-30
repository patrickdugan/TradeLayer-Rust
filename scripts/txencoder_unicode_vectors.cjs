#!/usr/bin/env node
'use strict';
// Complete unchanged pinned encoder and genuine numeric/base dependencies.
// Cases are raw inputs; source return strings and throws are observations only.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const crypto=require('node:crypto'),vm=require('node:vm'),zlib=require('node:zlib');
const args=process.argv.slice(2);
const value=(flag,fallback)=>{const at=args.indexOf(flag);return at<0?fallback:args[at+1];};
const repo=path.resolve(value('--repo',path.join(__dirname,'..')));
const out=path.resolve(value('--out',path.join(repo,'fixtures')));
const locked=args.includes('--lock'),proof=args.includes('--proof10');
assert(!(args.includes('--write')&&args.includes('--check')));
const sha=raw=>crypto.createHash('sha256').update(raw).digest('hex');
const pins={
 encoder:['legacy_types_source_txEncoder.txt','c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42'],
 base94:['legacy_base94_source.txt','0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb'],
 base256:['legacy_base256_source.txt','11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc'],
 converter:['legacy_base_converter_source.txt','a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62'],
};
function read(file,hash){const code=fs.readFileSync(path.join(repo,'fixtures',file),'utf8').replace(/\r\n/g,'\n');assert.equal(sha(code),hash,file);return code;}
function load(code,imports,filename){const module={exports:{}};vm.runInNewContext(code,{module,exports:module.exports,console:{log(){},warn(){},error(){}},require(name){assert.ok(Object.hasOwn(imports,name),name);return imports[name];}},{filename,timeout:2000});return module.exports;}
let numericCode,numericHash;
if(locked){const packed=fs.readFileSync(path.join(repo,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1];numericCode=zlib.gunzipSync(Buffer.from(packed,'base64')).toString('utf8');numericHash='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e';assert.equal(sha(numericCode),numericHash);}
else{numericHash='58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16';numericCode=read('legacy_bignumber_source.txt',numericHash);}
const BigNumber=load(numericCode,{},'genuine-bignumber-'+(locked?'9.1.2':'9.3.1')+'.js');
assert.equal(new BigNumber('0.1').plus('0.2').toString(),'0.3');
const converter=load(read(...pins.converter),{},'pinned-converter.js');
const base94=load(read(...pins.base94),{'bigint-base-converter':converter},'pinned-base94.js');
const base256=load(read(...pins.base256),{'bigint-base-converter':converter},'pinned-base256.js');
const encoder=load(read(...pins.encoder),{'bignumber.js':BigNumber,'./base94.js':base94,'./base256.js':base256},'pinned-complete-txEncoder.js');
const cases=[];
const add=(name,method,input,kind='V')=>cases.push({name,method,input,kind});
const issue=ticker=>({initialAmount:1,ticker,whitelists:[],managed:false,backupAddress:'backup',nft:false,coloredCoinHybrid:false});
// First ten genuine complete-module calls are a portable small proof corpus.
add('issue-greek','encodeTokenIssue',issue('Ω'));
add('issue-composed','encodeTokenIssue',issue('é'));
add('issue-decomposed','encodeTokenIssue',issue('e\u0301'));
add('issue-astral','encodeTokenIssue',issue('🪙'));
add('list-cjk','encodeCreateWhitelist',{backupAddress:'备份',name:'清单',url:'https://example.invalid/é',description:'説明'});
add('attestation-combining','encodeIssueOrRevokeAttestation',{revoke:true,id:1,targetAddress:'alice',metaData:'e\u0301'});
add('send-all-astral','encodeSend',{sendAll:true,address:'🪙接收者'});
add('helper-unicode','encodePropertyId','synth-Ω');
add('bind-body-unicode','encodeBindSmartContract',{contractHash:'哈希',type:'模式',hostChain:'链'});
add('rollup-json-unicode','encodeBatchMoveZkRollup',{proof:'证明',payments:[{fromAddress:'甲',propertyIds:[1],amounts:[1],toAddress:'乙',sentPropertyIds:[2],sentAmounts:[2]}],miscLogic:{'2':'二','1':'一','clé':'é'},miscData:['e\u0301','🪙']});
for(const text of ['日本語','العربية','हिन्दी','\u0085raw','line\u2028separator','paragraph\u2029separator','zero\u200dwidth','\uFEFFraw','comma,semi;colon:pipe|','🪙'.repeat(21),'🪙'.repeat(22)]){
 add('list-'+cases.length,'encodeCreateWhitelist',{backupAddress:'backup',name:text,url:'site',description:text});
 add('send-'+cases.length,'encodeSend',{sendAll:false,address:text,propertyId:1,amount:1});
}
for(const address of ['🪙'.repeat(21),'🪙'.repeat(22),'🪙'.repeat(20)+'ab','🪙'.repeat(20)+'abc','é'.repeat(42),'é'.repeat(43)]){
 add('commit-utf16-'+cases.length,'encodeCommit',{propertyId:1,amount:1,channelAddress:address,ref:'unicode-ref',payEnabled:true,clearLists:[]});
 add('transfer-utf16-'+cases.length,'encodeTransfer',{propertyId:1,amount:1,isColumnA:true,destinationAddr:address,pay:false,ref:'ref-Ω',payRef:'支付'});
}
add('oracle-metadata','encodeCreateOracle',{ticker:'价格',url:'https://example.invalid/🪙',backupAddress:'备份',whitelists:[1],lag:1,enum:'€'});
add('admin-address','encodeUpdateAdmin',{newAddress:'管理者',whitelist:true,oracle:false,token:false,id:1,updateBackup:false});
add('grant-dlc','encodeGrantManagedToken',{propertyId:1,amountGranted:'0.5',addressToGrantTo:'受取人',dlcTemplateId:'模板',dlcContractId:'契约',settlementState:'完成',dlcHash:'哈希'});
add('redeem-dlc','encodeRedeemManagedToken',{propertyId:1,amountDestroyed:'0.5',dlcTemplateId:'模板',dlcContractId:'契约',settlementState:'完成'});
add('option-combo','encodeOptionTrade',{contractId:'期权',price:1,amount:1,columnAIsSeller:true,expiryBlock:100,columnAIsMaker:false,comboTicker:'组合🪙',comboPrice:2,comboAmount:1});
add('colored-home','encodeColoredCoin',{encodeDecodeRecode:1,propertyId:1,SatsRatio:1,homeAddress:'归属🪙'});
add('bridge-destination','encodeAbstractionBridge',{propertyId:1,amount:1,destinationAddr:'目标🪙'});
add('unicode-object-key-ignored','encodeCreateWhitelist',{backupAddress:'backup',name:'name',url:'site',description:'note','未知':{'clé':'值'}});
add('unicode-array-string-helper','encodePropertyId',['é',null,'🪙']);
for(const text of ['\uFEFF10','\u00A010','\u008510','\u200B10'])add('synthetic-space-'+cases.length,'encodePropertyId','s-'+text+'-2');
for(const text of ['\uFEFF12\uFEFF','\u00A012\u00A0','\u008512\u0085','\u200B12\u200B'])add('amm-number-space-'+cases.length,'encodeAMMPool',{isRedeem:false,isContract:true,id:text,amount:text,id2:1,amount2:1});
add('bn-js-space','encodeTradeTokenForUTXO',{propertyId:1,amount:'\uFEFF0.5\uFEFF',columnA:true,satsExpected:'0.00000001',tokenOutput:0,payToAddress:'受取人',isColoredOutput:false});
add('max-utf8-admitted','encodeCreateWhitelist',{backupAddress:'',name:'🪙'.repeat(1000),url:'',description:''});
// Source observations outside retained numeric/control/resource/API guards.
add('guard-ascii-control','encodeCreateWhitelist',{name:'é\u0001'},'Q');
add('guard-overlong-unicode','encodeCreateWhitelist',{name:'🪙'.repeat(1025)},'Q');
add('guard-fractional-number','encodeTokenIssue',{...issue('é'),initialAmount:0.5},'Q');
add('guard-missing-source-array','encodeTokenIssue',{ticker:'é',initialAmount:1,whitelists:null},'Q');
add('guard-custom-tostring','encodePropertyId',{'toString':0,'未知':'值'},'Q');
const chosen=proof?cases.slice(0,10):cases;
const hex=text=>Buffer.from(text,'utf8').toString('hex')||'-';
const rows=chosen.map(c=>{let kind,value;try{const actual=encoder[c.method](c.input);assert(actual===undefined||typeof actual==='string');kind=actual===undefined?'undefined':'string';value=actual===undefined?'-':hex(actual);}catch(error){kind='throw';value=hex(error.name+':'+error.message);}return[c.kind,c.name,c.method,hex(JSON.stringify(c.input)),kind,value].join('\t');});
const payload=rows.join('\n')+'\n';
const name=proof?'txencoder_unicode_proof10.tsv':'txencoder_unicode_vectors.tsv';
const target=path.join(out,name);
if(args.includes('--write')){fs.mkdirSync(out,{recursive:true});fs.writeFileSync(target,payload);}
else{assert.equal(fs.readFileSync(target,'utf8').replace(/\r\n/g,'\n'),payload,'complete unchanged Unicode encoder regeneration');}
console.log(JSON.stringify({sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',sourcePins:pins,numericVersion:locked?'9.1.2':'9.3.1',numericSHA256:numericHash,actualCalls:chosen.length,admittedSuccessRows:chosen.filter(c=>c.kind==='V').length,guardObservations:chosen.filter(c=>c.kind==='Q').length,fixtureSHA256:sha(payload),sourceBodiesChanged:false,financialOutcomeInput:false}));
