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
const add=(name,method,input,kind='V',raw)=>cases.push({name,method,raw:raw===undefined?JSON.stringify(input):raw,kind});
const number=(name,n)=>add(name,'encodePropertyId',n,'V',Object.is(n,-0)?'-0':JSON.stringify(n));
// First ten calls execute the complete source method, not an expression replica.
for(const [name,n] of [['half',0.5],['tenth',0.1],['negative-tenth',-0.1],['third',1/3],['above-one',1.0000000000000002],['below-one',0.9999999999999999],['one-half',1.5],['negative-zero',-0],['minimum-subnormal',Number.MIN_VALUE],['minimum-normal',2.2250738585072014e-308]])number(name,n);
const bytes=new DataView(new ArrayBuffer(8));
const bits=n=>{bytes.setFloat64(0,n,false);return bytes.getBigUint64(0,false);};
const fromBits=n=>{bytes.setBigUint64(0,n,false);return bytes.getFloat64(0,false);};
const nearby=(name,n)=>{const b=bits(n);for(const d of [-2n,-1n,0n,1n,2n])if(b+d>=0n){const v=fromBits(b+d);if(Number.isFinite(v)&&Math.abs(v)<=Number.MAX_SAFE_INTEGER)number(name+'-'+d,v);}};
for(const exponent of [-1074,-1073,-1072,-1023,-1022,-1000,-750,-500,-250,-100,-54,-53,-52,-51,-24,-17,-8,-3,-2,-1,0,1,2,7,16,24,40,50,51,52])nearby('pow2-'+exponent,2**exponent);
for(const value of [0.1,0.2,0.3,0.4,0.5,0.9,1/3,2/3,1/36,35/36,1/1296,35/1296,1.1,Math.PI,Math.E,35.99999999999999,36,36.1,1295.9999999999998,1296,Number.MAX_SAFE_INTEGER]){nearby('near-'+value,value);nearby('negative-'+value,-value);}
// Deterministic raw binary64 facts across subnormal/normal/fraction boundaries.
let state=0x9e3779b97f4a7c15n;
const random=()=>{state^=state<<13n;state=BigInt.asUintN(64,state);state^=state>>7n;state^=state<<17n;state=BigInt.asUintN(64,state);return state;};
for(let i=0;i<4096;i++){
 const mantissa=random()&0x000fffffffffffffn;
 const exponent=Number(random()%1075n);
 const sign=(random()&1n)<<63n;
 number('binary64-'+i,fromBits(sign|(BigInt(exponent)<<52n)|mantissa));
}
// Real complete encoder call sites reached by fractional Number/toNumber.
add('send-fractional-property','encodeSend',{propertyId:0.5,amount:1,address:'alice'});
add('issue-fractional-fields','encodeTokenIssue',{initialAmount:0.1,ticker:'é',whitelists:[0.5,1/3],managed:false,backupAddress:'backup',proceduralType:1.5});
add('multisend-fractional-properties','encodeSend',{propertyId:[0.1,0.5],amount:[1,2],address:'alice'});
add('scaled-number-half','encodeTradeTokenForUTXO',{propertyId:0.5,amount:'0.000000005',columnA:true,satsExpected:'0.000000001',tokenOutput:0.1,payToAddress:'alice',isColoredOutput:false});
add('commit-fractional-property','encodeCommit',{propertyId:1/3,amount:1,channelAddress:'channel',ref:1,payEnabled:false,clearLists:[]});
add('whitelist-fractional-id','encodeIssueOrRevokeAttestation',{revoke:false,id:0.1,targetAddress:'alice',metaData:'note'});
add('admin-fractional-id','encodeUpdateAdmin',{newAddress:'alice',whitelist:true,id:1.5});
add('amm-fractional-numbers','encodeAMMPool',{isRedeem:false,isContract:true,id:0.1,amount:0.5,id2:1.5,amount2:0.25});
add('oracle-fractional-lag','encodeCreateOracle',{ticker:'price',url:'site',backupAddress:'backup',whitelists:[0.5],lag:0.1});
// Retained guards are observed source calls, not claimed native parity.
add('guard-unsafe-integer','encodePropertyId',9007199254740992,'Q');
add('guard-unsafe-negative','encodePropertyId',-9007199254740992,'Q');
add('guard-nonfinite-computed','encodeStakeFraudProof',{action:'Infinity',amount:1},'Q');
add('guard-custom-coercion','encodePropertyId',{toString:0},'Q');
const chosen=proof?cases.slice(0,10):cases;
const hex=text=>Buffer.from(text,'utf8').toString('hex')||'-';
const rows=chosen.map(c=>{let kind,value;try{const actual=encoder[c.method](JSON.parse(c.raw));assert(actual===undefined||typeof actual==='string');kind=actual===undefined?'undefined':'string';value=actual===undefined?'-':hex(actual);}catch(error){kind='throw';value=hex(error.name+':'+error.message);}return[c.kind,c.name,c.method,hex(c.raw),kind,value].join('\t');});
const payload=rows.join('\n')+'\n';
const target=path.join(out,proof?'txencoder_fraction_proof10.tsv':'txencoder_fraction_vectors.tsv');
if(args.includes('--write')){fs.mkdirSync(out,{recursive:true});fs.writeFileSync(target,payload);}else assert.equal(fs.readFileSync(target,'utf8').replace(/\r\n/g,'\n'),payload,'complete unchanged fraction encoder regeneration');
const admitted=rows.filter((row,i)=>chosen[i].kind==='V');
assert(admitted.every(row=>row.split('\t')[4]==='string'));
console.log(JSON.stringify({sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',sourcePins:pins,numericVersion:locked?'9.1.2':'9.3.1',numericSHA256:numericHash,node:process.versions.node,v8:process.versions.v8,actualCalls:chosen.length,admittedStrings:admitted.length,guardObservations:chosen.length-admitted.length,fixtureSHA256:sha(payload),sourceBodiesChanged:false,expectedFinancialInput:false}));
