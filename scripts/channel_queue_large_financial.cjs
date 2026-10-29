
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.env.PROOF_REPO||(process.argv[1]&&fs.existsSync(path.join(process.argv[1],'Cargo.toml'))?path.resolve(process.argv[1]):path.resolve(__dirname,'..')),file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
if(process.argv.includes('--lock')){const raw=require('node:zlib').gunzipSync(Buffer.from(fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],'base64'));if(require('node:crypto').createHash('sha256').update(raw).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine numeric lock');require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber=()=>{const module={exports:{}};require('node:vm').runInNewContext(raw.toString(),{module,exports:module.exports},{timeout:1000});return module.exports;};}
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,instances,context,adapters,types,logic,Orderbook,effects');
code+='\nmodule.exports={environment,tag,A,B,BN};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag,A,B,BN}=driver.exports,C='source-channel';let e,unhandled=[];
process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
const norm=v=>v[0]==='bignumber'?['bignumber',new BN(v[1]).isZero()&&new BN(v[1]).isNegative()?'-0':new BN(v[1]).toFixed()]:v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),H=e.load('channels'),L=e.load('clearing');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:[...(T.addresses??new Map())],channels:[...H.channelsRegistry],withdrawals:H.pendingWithdrawals,books:e.instances.map(x=>({tickSize:x.tickSize,orderBookKey:x.orderBookKey,orderBooks:x.orderBooks,block:x.block})),feeBuffer:[...(T._feeAccrualBuffer??new Map())],blockTrades:[...L.blockTrades],unhandledRejections:unhandled};}
let latestOutcome;
function emit(name,command,outcome){latestOutcome=outcome;console.log(JSON.stringify(norm(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock','rpc'].includes(x.kind))}))));}
function bindRpc(){
 const source=fs.readFileSync(path.join(root,'fixtures/source_channel_txutils_reference.txt'),'utf8');
 if(require('node:crypto').createHash('sha256').update(source).digest('hex')!=='38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3')throw Error('TxUtils pin');
 const originalRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(root,'../../../../tradelayer.js'),external=Module.createRequire(path.join(originalRoot,'package.json')),own={exports:{}};
 const wrapper=require('node:vm').runInContext('(function(exports,require,module){\n'+source+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve({})}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 Object.assign(e.adapters['./txUtils'],{getAddressTypeUniversal:own.exports.getAddressTypeUniversal,extractPubkeyByType:own.exports.extractPubkeyByType,async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(r=>r.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'TxUtils.getRawTransaction',args,returned:structuredClone(value)});return value;}});
}
const bal=(a=1000,r=20,m=0,c=0)=>({amount:a+r+m+c,available:a,reserved:r,margin:m,vesting:0,channelBalance:c});
async function reset(name,options={}){unhandled=[];e=environment({entry:options.entry??'main-subgraph'});if(options.bindTally)e.context.tallyMap=e.load('tally');const properties=JSON.parse(e.table('propertyList')[0].value);properties.push([8,{ticker:'P8',totalInCirculation:1000000,type:1,whitelistId:0,issuer:A}]);e.table('propertyList')[0].value=JSON.stringify(properties);
 e.table('tallyMap')[0].data=JSON.stringify([[A,{7:bal(options.available??1000,options.reserve??20),8:bal()}],[B,{7:bal(),8:bal()}],[C,{7:bal(0,0,0,options.channel??200),8:bal(0,0,0,200)}]]);
 const channel={channel:C,participants:{A,B},A:{7:options.columnA??100,8:100},B:{7:100,8:100},commits:[],clearLists:{A:[],B:[]},payEnabled:{A:false,B:false}};
 if(options.pubkeys)channel.channelPubkeys={A:'',B:''};
 e.table('channels').push({_id:C,data:channel});
 for(let i=0;i<(options.cached??2)-2;i++)e.table('channels').push({_id:'cached-'+i,data:{channel:'cached-'+i,participants:{A:'',B:''},A:{},B:{}}});
 if(options.rpc){e.table('sourceRpcTransactions').push({txid:'source-transfer',transaction:{vin:[{}],vout:[]}});bindRpc();}
 await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args,extra={}){e.effects.length=0;if(extra.failure){if(extra.failure.op==='getDatabase')factoryFailure(extra.failure);else e.setFailure(extra.failure);}const originalArgs=structuredClone(args);let result;
 try{const [namespace,method]=target.split('.');if(namespace==='Tally')result={returned:await e.load('tally')[method](...args)};
 else if(namespace==='Contract')result={returned:await e.load('contractRegistry')[method](...args)};
 else if(namespace==='Logic')result={returned:await e.logic[method](...args)};
 else if(namespace==='VolumeIndex')result={returned:await e.load('volumeIndex')[method](...args)};
 else if(namespace==='Orderbook'){if(['processTokenMatches','processTokenChannelTrades','processContractMatches'].includes(method)){const receiver=e.instances[args[0].orderbookInstance-1];result={returned:await receiver[method](...args.slice(1))};}else result={returned:await e.Orderbook[method](...args)};}
 else throw Error('unknown '+target);
 if(result.returned instanceof e.Orderbook)result={returned:{orderbookInstance:e.instances.indexOf(result.returned)+1}};
 }catch(error){result={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args:originalArgs,argumentsAfter:args,...extra},result);}
async function raw(name,type,payload){e.effects.length=0;const args=[name,type,'tl',payload,C,null,0,0,10];let result;
 try{const params=await e.types.decodePayload(...args);let returned;if(params.valid===true)returned=await e.logic.typeSwitch(type,params);result={returned:{params,returned}};}catch(error){result={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args},result);}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
const position=address=>({contracts:0,margin:0,unrealizedPNL:0,avgPrice:0,address});
const marginArgs=(sender,channel=C)=>[sender,1,1,100,100,false,10,true,channel,10,{},false,false,'source-channel-margin',position(sender)];
function tokenMatch(sellerTime=10,buyerTime=10,extra={}){return{sellOrder:{senderAddress:A,offeredPropertyId:7,desiredPropertyId:8,blockTime:sellerTime},buyOrder:{senderAddress:B,offeredPropertyId:7,desiredPropertyId:8,blockTime:buyerTime},amountOfTokenA:2,amountOfTokenB:4,tradePrice:.5,...extra};}
async function instance(name='actual-book'){return call(name,'Orderbook.getOrderbookInstance',['1']);}
(async()=>{
 for(const late of [false,true]){
  const name=late?'large65-real-financial-then-payload-error':'large64-real-token-financial';
  await reset(name,{bindTally:true});
  const trades=Array.from({length:63},(_,i)=>({kind:'other',sender:'sender-'+String(62-i).padStart(3,'0'),blockHeight:9,txid:'unknown-'+i}));
  trades.push({kind:'token',pairKey:'7-8',sender:C,blockHeight:10,txid:'actual-financial-token',payload:{match:tokenMatch()}});
  if(late)trades.push({kind:'token',pairKey:'7-8',sender:C,blockHeight:11,txid:'missing-payload',match:tokenMatch()});
  await imported(name+'-raw-queue','orderBooks',[{_id:'channel-queue-7-8',trades},{_id:'activePairs',pairs:['channel-7-8']}]);
  const beforeTally=JSON.stringify(e.tables.get('tallyMap'));
  await call(name+'-actual-source','Orderbook.processQueuedChannelTradesForBlock',[11]);
  const assert=require('node:assert/strict');
  assert.notEqual(JSON.stringify(e.tables.get('tallyMap')),beforeTally,'real original Tally writes must occur');
  assert.equal(e.tables.get('orderBooks').find(r=>r._id==='channel-queue-7-8').trades.length,late?65:0);
  if(late){assert.equal(latestOutcome.thrown.name,'TypeError');assert.equal(latestOutcome.thrown.message,"Cannot read properties of undefined (reading 'match')");}
  else assert.equal(latestOutcome.returned,undefined);
 }
})().catch(error=>{console.error(error);process.exitCode=1});
