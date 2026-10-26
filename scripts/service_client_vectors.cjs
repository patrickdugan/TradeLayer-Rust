#!/usr/bin/env node
'use strict';
// Complete unchanged clients run in a VM. The injected Axios boundary records
// calls and supplies responses/errors; it is not a running TradeLayer backend.
// --axios-root additionally checks genuine installed Axios transformations.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
const {readPinnedSource}=require('./legacy_numeric_oracle.cjs');
const root=path.resolve(__dirname,'..');
const spec={express:['interfaceExpress','interface_express','8cfa990d9aa460d29f5d309beb0987a040e06e2840632fac38e713d94836aab2'],wallet:['walletInterface','wallet_interface','d22fbe4ee631ca15fbb4fe4fb2af65c7a424ef4f249eb8686f8aa7b5fd2d8735']};
const hex=s=>Buffer.from(s,'utf8').toString('hex')||'-';
const json=v=>JSON.stringify(v);
function source(kind){const [file,stem,hash]=spec[kind],i=process.argv.indexOf('--source-root');return readPinnedSource(i<0?path.join(root,'fixtures','legacy_'+stem+'_source.txt'):path.join(process.argv[i+1],'src',file+'.js'),hash);}
function number(n){if(Number.isNaN(n))return 'nan';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function requestProjection(verb,url,data,config){
 const u=new URL(url),query={};let supported=true;
 for(const [key,value]of Object.entries(config?.params||{})){if(value==null)continue;if(typeof value==='object'){supported=false;continue;}query[key]=String(value);}
 const bodyPresent=verb==='POST'&&data!==undefined;
 return {method:verb,path:decodeURI(u.pathname),query,body:bodyPresent?JSON.parse(json(data)):null,body_present:bodyPresent,supported};
}
let genuine=null;const ai=process.argv.indexOf('--axios-root');
if(ai>=0){const req=createRequire(path.join(path.resolve(process.argv[ai+1]),'package.json'));genuine=req('axios');assert.equal(genuine.VERSION,'1.20.0','recorded genuine Axios version');}
async function runOne(kind,method,args,scenario={}){
 const calls=[];let oracleFailure=null;const failure=Object.assign(new Error('injected transport failure'),{name:'Error'});
 let data=Object.hasOwn(scenario,'data')?scenario.data:{ok:true};
 if(Object.hasOwn(scenario,'wire')){try{data=JSON.parse(scenario.wire);}catch{data=scenario.wire;}}
 const response={status:scenario.status||200,statusText:'OK',headers:{'content-type':'application/json'},data};
 const axios={};
 for(const [fn,verb]of [['get','GET'],['post','POST']])axios[fn]=async(url,dataOrConfig)=>{
  const data=verb==='POST'?dataOrConfig:undefined,config=verb==='GET'?dataOrConfig:undefined;
  const projected=requestProjection(verb,url,data,config);
  calls.push({url,config:config??null,request:projected});
  if(genuine){try{
   let seen;
   const wire=Object.hasOwn(scenario,'wire')?scenario.wire:json(response.data);
   const instance=genuine.create({adapter:async cfg=>{seen=cfg;return {status:response.status,statusText:'OK',headers:{},config:cfg,data:wire};}});
   const actual=verb==='POST'?await instance.post(url,data):await instance.get(url,config);
   assert.equal(typeof actual.data,typeof response.data);
   if(typeof actual.data==='number')assert.equal(number(actual.data),number(response.data));
   else assert.equal(json(actual.data),json(response.data));
   assert.equal(seen.data===undefined,!projected.body_present);
   if(projected.body_present)assert.equal(seen.data,json(data));
   const uri=new URL(instance.getUri(seen));
   if(projected.supported)assert.deepEqual(Object.fromEntries(uri.searchParams),{...Object.fromEntries(new URL(url).searchParams),...projected.query});
  }catch(error){oracleFailure=error;throw error;}}
  if(scenario.failure==='transport')throw failure;
  if(response.status<200||response.status>=300){failure.response=response;throw failure;}
  return response;
 };
 const module={exports:{}};vm.runInNewContext(source(kind),{module,exports:module.exports,require(name){assert.equal(name,'axios');return axios;},console:{log(){},error(){}}},{timeout:1000});
 const exports=Object.keys(module.exports);
 let result;
 try{const value=await module.exports[method](...args);result=value===undefined?{kind:'undefined',swallowed:calls.length>0&&(scenario.failure==='transport'||response.status>=300||response.status<200)}:typeof value==='number'?{kind:'number',bits:number(value)}:value===response?{kind:'response',status:response.status,body:response.data}:{kind:'data',value};}
 catch(error){result={kind:'throw',name:error.name,message:error.message,original:error===failure};}
 if(oracleFailure)throw oracleFailure;
 return {exports,calls,result};
}
async function main(){
 const lines=[];
 async function add(kind,name,method,args,scenario={},domain='supported'){
  const observed=await runOne(kind,method,args,scenario);
  lines.push([kind,name,method,hex(json(args)),hex(json(scenario)),domain,hex(json(observed.calls)),hex(json(observed.result))].join('\t'));
 }
 const e={initMain:[false],listProperties:[],getAllBalancesForAddress:['alice'],getActivations:[],getOrderBook:[7,8],getContractOrderBook:[1],listContractSeries:[],listOracles:[],getContractPositionForAddressAndContractId:['alice',1],getTradeHistory:[7,8],getContractTradeHistory:[1],getFundingHistory:[1],getOracleHistory:[1],getClearingHistory:[1],getWalletPositions:[],getAddressesWithProperty:[7]};
 const w={initMain:[],listProperties:[],getContractInfo:[1],getInitialMargin:[1,100],getAllBalancesForAddress:['alice'],getActivations:[],getOrderBook:[{propertyId1:7,propertyId2:8}],getContractOrderBook:[{contractId:1}],listContractSeries:[],listOracles:[],getColumn:[{channel:'channel',addressA:'alice',addressB:'bob'}],getContractPositionForAddressAndContractId:[{address:'alice',contractId:1}],getTradeHistory:[{propertyId1:7,propertyId2:8}],getContractTradeHistory:[{contractId:1}],getFundingHistory:[{contractId:1}],getOracleHistory:[{oracleId:1}],getMaxProcessedHeight:[],getTrackHeight:[],checkSync:[]};
 assert.equal(Object.keys((await runOne('express','initMain',[])).exports).length,16);
 assert.equal((source('express').match(/async getContractPositionForAddressAndContractId\(/g)||[]).length,2);
 for(const [kind,methods]of [['express',e],['wallet',w]]){
  assert.deepEqual((await runOne(kind,'initMain',[])).exports,Object.keys(methods));
  for(const [method,args]of Object.entries(methods)){
   await add(kind,method+'-ok',method,args,{data:{ok:true,value:7}});
   await add(kind,method+'-network',method,args,{failure:'transport'});
   await add(kind,method+'-http',method,args,{status:503,data:{error:'busy'}});
   await add(kind,method+'-missing',method,[]);
  }
 }
 const nums={zero:0,negativezero:-0,small:1e-7,large:1e21,string:'2.5',blank:' \t\n',hex:'0x20000000000001',binary:'0b11',octal:'0o17',badhex:'-0x10',nan:'bad',inf:'Infinity',neginf:'-Infinity',negativezero_string:'-0',null:null,yes:true,no:false,empty:[],one:['2.5'],nested:[[null]],many:[1,2],object:{},valueof:{valueOf:null},tostring:{toString:null},objarray:[{toString:null}],arrayzero:[-0],arraynull:[null],numericexp:'1e309',tiny:'-1e-999',unicode:'\ufeff2\u00a0'};
 for(const [name,data]of Object.entries(nums))await add('wallet','number-'+name,'getInitialMargin',[1,100],Object.is(data,-0)?{wire:'-0'}:{data});
 for(const [name,wire]of Object.entries({overflow:'1e309',underflow:'-1e-999',text:'plain-text',empty:'',malformed:'{bad',duplicate:'{"x":1,"x":2}',zeroarray:'[-0]',infinitearray:'[1e309]'}))await add('wallet','wire-'+name,'getInitialMargin',[1,100],{wire});
 for(const [name,args]of Object.entries({missing:[],null:[null],primitive:[7],false:[false],array:[[]],empty:[{}],extra:[{channel:'x',addressA:'a',addressB:'b',extra:'ignored'}]}))await add('wallet','column-'+name,'getColumn',args);
 for(const [name,value]of Object.entries({unicode:'☃',space:'a b',undefined:null,bool:true,array:[1,null,2],object:{x:1},valueof:{valueOf:null},tostring:{toString:1}}))await add('express','path-'+name,'getFundingHistory',[value]);
 for(const [name,value]of Object.entries({trailing:'a ',onlyspace:' ',leadingandtrailing:'  a  ',nonbreaking:'a\u00a0'}))await add('express','path-space-'+name,'getFundingHistory',[value]);
 for(const [name,value]of Object.entries({slash:'a/b',backslash:'a\\b',query:'a?b=c',fragment:'a#b',percent:'a%2Fb',dot:'.',dotdot:'..',control:'a\nb'}))await add('express','raw-path-'+name,'getFundingHistory',[value],{},'raw-url-only');
 await add('wallet','array-query','getContractInfo',[[1,2]],{},'nested-query-only');
 await add('wallet','object-query','getInitialMargin',[{x:1},100],{},'nested-query-only');
 await add('wallet','null-query','getInitialMargin',[null,null],{data:'-0'});
 await add('wallet','false-query','getInitialMargin',[false,0],{data:0});
 await add('express','extra-ignored','listProperties',[1,2,3]);
 await add('express','body-enumeration','getAllBalancesForAddress',[{'10':'x','2':'y',z:1}]);
 for(const kind of ['express','wallet'])await add(kind,'unknown-method','notAnExport',[]);
 const output='# Complete unchanged pinned interfaceExpress.js + walletInterface.js; explicit Axios response/error boundary.\n# Optional --axios-root validates genuine Axios1.20.0 request transforms; no backend/proof success is inferred.\n# kind name method argsJSONhex scenarioJSONhex domain callsJSONhex resultJSONhex\n'+lines.join('\n')+'\n';
 const file=path.join(root,'fixtures','service_client_vectors.tsv');
 if(process.argv.includes('--write'))fs.writeFileSync(file,output);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),output);
 console.log('PASS service clients: '+lines.length+' full-source observations'+(genuine?' + genuine Axios1.20.0 transforms':''));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
