//! Whole pinned modules, actual mutable financial owners, declared storage and
//! environment boundaries. No mocked validation, margin or settlement verdicts.
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceOutcome, SourceRuleError,
    SourceValue as V,
};

const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
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
function emit(name,command,outcome){console.log(JSON.stringify(norm(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock','rpc'].includes(x.kind))}))));}
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
 await reset('channel-credit-reset');await call('channel-exact-credit','Tally.updateChannelBalance',[C,7,.123456789,'credit',10]);await call('zero-number-omits-delta','Tally.updateChannelBalance',[C,7,0,'zero',10]);await call('zero-string-records-delta','Tally.updateChannelBalance',[C,7,'0','string-zero',10]);await call('negative-zero-omits-delta','Tally.updateChannelBalance',[C,7,-0,'negative-zero',10]);
 await reset('channel-overdraft-reset',{channel:1});await call('channel-overdraft-throws','Tally.updateChannelBalance',[C,7,-2,'overdraft',10]);await call('new-address-overdraft-cache-prefix','Tally.updateChannelBalance',['new-channel',9,-1,'new-overdraft',10]);
 await reset('channel-delta-failure-reset');await call('channel-delta-failure','Tally.updateChannelBalance',[C,7,1,'failed-delta',10],{failure:{collection:'tallyMapDelta',op:'insert',message:'channel-delta'}});
 await reset('channel-save-failure-reset');await call('channel-save-failure-is-caught','Tally.updateChannelBalance',[C,7,1,'failed-save',10],{failure:{collection:'tallyMap',op:'update',message:'channel-save'}});
 for(const [name,change]of [['undefined',undefined],['bool',true],['nan',NaN],['infinity',Infinity],['negative-infinity',-Infinity],['hex','0x10']]){await reset('channel-number-'+name);await call('actual-channel-number-'+name,'Tally.updateChannelBalance',[C,7,change,'number-domain',10]);}
 await reset('channel-collateral-covered');await call('channel-margin-covered','Contract.moveCollateralToMargin',marginArgs(A));
 await reset('channel-collateral-shortfall',{channel:3});await call('channel-margin-available-top-up','Contract.moveCollateralToMargin',marginArgs(A));
 await reset('channel-collateral-absent');await call('channel-margin-undefined-throws','Contract.moveCollateralToMargin',marginArgs(A,'missing-channel'));
 await reset('channel-column-insufficient',{columnA:1});await call('channel-margin-column-failure-retains-tally','Contract.moveCollateralToMargin',marginArgs(A));
 await reset('channel-available-insufficient',{channel:3,available:1});await call('channel-margin-top-up-failure-retains-channel-debit','Contract.moveCollateralToMargin',marginArgs(A));
 await reset('channel-credit-failure');await call('channel-margin-credit-failure-prefix','Contract.moveCollateralToMargin',marginArgs(A),{failure:{collection:'tallyMapDelta',op:'insert',skip:1,message:'channel-credit-delta'}});
 await reset('default-unbound-token');await instance();await call('default-token-reference-error','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch()],10,'unbound-token',true]);await reset('direct-token-channel',{bindTally:true});await instance();await call('channel-token-ordinary-reserves','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch()],10,'channel-token-id',true]);
 await reset('direct-token-channel-method',{bindTally:true});await instance();await call('channel-token-direct-helper','Orderbook.processTokenChannelTrades',[{orderbookInstance:1},[tokenMatch()],10,'helper-token-id']);
 for(const [name,st,bt]of [['seller-maker',9,10],['buyer-maker',10,9],['split',10,10],['lexical-time','10','9'],['array-time',['10'],['9']]]){await reset('spot-token-'+name,{bindTally:true});await instance();await call('actual-token-'+name,'Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch(st,bt)],10,'spot-token-id',false]);}
 await reset('token-insufficient-reserve',{reserve:1,bindTally:true});await instance();await call('token-fee-before-failed-debit','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch(9,10)],10,'failed-token',false]);
 await reset('token-history-write-failure',{bindTally:true});await instance();await call('token-history-failure-after-balances','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch(9,10)],10,'history-token',false],{failure:{collection:'tradeHistory',op:'update',message:'token-history'}});
 await reset('negative-fractional-token-fee',{bindTally:true});await instance();await call('source-floor-negative-token-fee','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch(9,10,{amountOfTokenA:-.00000001,amountOfTokenB:-.000012345})],10,'negative-token',false]);
 await reset('boolean-token-amount',{bindTally:true});await instance();await call('source-token-bignumber-bool-nan','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch(9,10,{amountOfTokenA:true})],10,'bool-token',true]);
 await reset('token-role-mutation-before-error');await instance();await call('source-buy-role-before-seller-primitive-error','Orderbook.processTokenMatches',[{orderbookInstance:1},[tokenMatch(10,10,{sellOrder:'bad'})],10,'primitive-token',false]);
 await reset('token-invalid-matches');await instance();for(const [name,value]of [['nonarray',{}],['empty',[]],['null-entry',[null]],['missing-leg',[{}]],['null-helper',null]])await call('token-shape-'+name,name==='null-helper'?'Orderbook.processTokenChannelTrades':'Orderbook.processTokenMatches',[{orderbookInstance:1},value,10,'invalid-token',false]);
 await reset('logic-token-missing-sender-fields');await call('actual-logic-token-sender-defect','Logic.tradeTokensChannel',[7,8,2,4,20,true,C,10,'logic-token-id',true]);
 await reset('bound-logic-token-wrong-sender',{bindTally:true});await call('actual-bound-logic-token-sender-defect','Logic.tradeTokensChannel',[7,8,2,4,20,true,C,10,'bound-logic-token-id',true]);const tokenPayload=['7','8',(2e8).toString(36),(4e8).toString(36),'1',(20).toString(36),'1','0','0'].join(',');
 await reset('raw-token-funded-true');await raw('raw20-funded-rejected-A',20,tokenPayload);
 await reset('raw-token-funded-false');await raw('raw20-funded-rejected-B',20,tokenPayload.replace(',1,k,1,0,0',',0,k,1,0,0'));
 const contractPayload=['1',(100e8).toString(36),'1','1',(20).toString(36),'0','1'].join(',');
 await reset('raw-contract-channel-funded');await raw('raw19-original-fields',19,contractPayload);
 await reset('raw-contract-channel-shortfall',{channel:3});await raw('raw19-available-top-up',19,contractPayload);
 await reset('own-channel-queue-direct-drain');await call('actual-enqueue-channel-token','Orderbook.queueChannelTrade',['token','7-8',C,tokenMatch(),10,'queued-token']);await call('own-enqueued-payload-defect','Orderbook.processQueuedChannelTradesForBlock',[10]);
 await reset('own-channel-queue-main-order');await call('actual-enqueue-channel-contract','Orderbook.queueChannelTrade',['contract','1',C,{original:true},10,'queued-contract']);await call('onchain-drain-removes-channel-active-pair','Orderbook.processQueuedOnChainOrdersForBlock',[10]);await call('main-channel-drain-sees-no-active-pair','Orderbook.processQueuedChannelTradesForBlock',[10]);
 await reset('channel-future-queue');await call('actual-enqueue-future','Orderbook.queueChannelTrade',['token','7-8',C,tokenMatch(),11,'future-token']);await call('future-channel-preserved','Orderbook.processQueuedChannelTradesForBlock',[10]);
 await reset('imported-payload-channel-queue',{bindTally:true});await imported('actual-queue-import','orderBooks',[{_id:'channel-queue-7-8',trades:[{kind:'token',pairKey:'7-8',sender:C,blockHeight:10,txid:'payload-token',payload:{match:tokenMatch()}}]},{_id:'activePairs',pairs:['channel-7-8']}]);await call('payload-token-real-settlement','Orderbook.processQueuedChannelTradesForBlock',[10]);
 await reset('imported-payload-late-failure',{bindTally:true});await imported('actual-late-queue-import','orderBooks',[{_id:'channel-queue-7-8',trades:[{kind:'token',sender:C,blockHeight:10,txid:'first',payload:{match:tokenMatch()}},{kind:'token',sender:C,blockHeight:10,txid:'second',match:tokenMatch()}]},{_id:'activePairs',pairs:['channel-7-8']}]);await call('first-real-fill-before-late-payload-failure','Orderbook.processQueuedChannelTradesForBlock',[10]);
 for(const failure of [{op:'update'},{op:'loadDatabase'},{op:'findOne',skip:1},{op:'update',skip:1},{op:'loadDatabase',skip:1}]){const label=failure.op+'-'+(failure.skip??0);await reset('channel-enqueue-failure-'+label);await call('actual-channel-enqueue-prefix-'+label,'Orderbook.queueChannelTrade',['token','7-8',C,tokenMatch(),10,'failure-token'],{failure:{collection:'orderBooks',message:'queue-'+label,...failure}});}
 await reset('channel-ready-unknown-kind');await imported('actual-unknown-kind-import','orderBooks',[{_id:'channel-queue-1',trades:[{kind:'other',sender:'z',blockHeight:'10',txid:'z'},{kind:'other',sender:'a',blockHeight:10,txid:'a'},{kind:'other',sender:'b',blockHeight:9,txid:'b'}]},{_id:'activePairs',pairs:['channel-1']}]);await call('actual-unknown-kind-consumed','Orderbook.processQueuedChannelTradesForBlock',[10]);
 await reset('channel-drain-queue-update-failure');await imported('actual-drain-failure-import','orderBooks',[{_id:'channel-queue-1',trades:[{kind:'other',sender:C,blockHeight:10,txid:'unknown'}]},{_id:'activePairs',pairs:['channel-1']}]);await call('channel-clear-fails-after-real-instance-load','Orderbook.processQueuedChannelTradesForBlock',[10],{failure:{collection:'orderBooks',op:'update',message:'queue-clear'}});
 await reset('channel-v8-small-sort-upper-bound');await imported('actual-63-ready-import','orderBooks',[{_id:'channel-queue-1',trades:Array.from({length:63},(_,i)=>({kind:'other',sender:'sender-'+String(62-i).padStart(2,'0'),blockHeight:10,txid:'tx-'+i}))},{_id:'activePairs',pairs:['channel-1']}]);await call('actual-63-ready-v8-sort','Orderbook.processQueuedChannelTradesForBlock',[10]);
 for(const cached of [2,3,4,6])for(const fail of [-1,0,1]){await reset('transfer-rpc-cached-'+cached+'-failure-'+fail,{rpc:true,cached});await call('actual-transfer-microtasks-'+cached+'-'+fail,'Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer'],fail<0?{}:{failure:{collection:'channels',op:'update',skip:cached+fail,message:'final-channels-update-'+fail}});}
 await reset('transfer-debit-rejection',{rpc:true,channel:1});await call('unawaited-transfer-debit-rejects-credit-persists','Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer']);
 await reset('transfer-delta-rejection',{rpc:true,cached:6});await call('unawaited-transfer-delta-rejects-credit-persists','Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer'],{failure:{collection:'tallyMapDelta',op:'insert',message:'transfer-delta'}});
 for(const [label,amount]of [['zero',0],['negative-zero',-0],['string-zero','0']]){await reset('transfer-'+label,{rpc:true,cached:6});await call('actual-transfer-'+label,'Logic.transfer',[C,'destination-channel',7,amount,true,false,undefined,10,'source-transfer']);}
 for(const failure of [{collection:'tallyMap',op:'getDatabase'},{collection:'tallyMap',op:'getDatabase',skip:1},{collection:'tallyMap',op:'findOne'},{collection:'tallyMap',op:'findOne',skip:1},{collection:'tallyMapDelta',op:'insert'},{collection:'tallyMapDelta',op:'insert',skip:1}]){const label=failure.collection+'-'+failure.op+'-'+(failure.skip??0);await reset('transfer-failure-'+label,{rpc:true,cached:6});await call('actual-transfer-failure-'+label,'Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer'],{failure:{...failure,message:label}});}
 await reset('transfer-stale-cache',{rpc:true,cached:6});await call('actual-prime-before-import','Tally.getTally',[C,7]);const staleRows=structuredClone(e.table('tallyMap'));const staleData=JSON.parse(staleRows[0].data);staleData.find(x=>x[0]===C)[1][7].channelBalance=100;staleData.find(x=>x[0]===C)[1][7].amount=100;staleRows[0].data=JSON.stringify(staleData);await imported('actual-change-persisted-under-cache','tallyMap',staleRows);await call('actual-transfer-failed-load-with-stale-cache','Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer'],{failure:{collection:'tallyMap',op:'getDatabase',message:'stale-load-factory'}});
 await reset('transfer-stale-credit-cache',{rpc:true,cached:6});await call('actual-prime-credit-before-import','Tally.getTally',[C,7]);const creditRows=structuredClone(e.table('tallyMap'));const creditData=JSON.parse(creditRows[0].data);creditData.find(x=>x[0]===C)[1][7].channelBalance=100;creditData.find(x=>x[0]===C)[1][7].amount=100;creditRows[0].data=JSON.stringify(creditData);await imported('actual-change-persisted-under-credit-cache','tallyMap',creditRows);await call('actual-transfer-failed-credit-load-with-stale-cache','Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer'],{failure:{collection:'tallyMap',op:'getDatabase',skip:1,message:'stale-credit-load-factory'}});
 await reset('transfer-same-owner-identity',{rpc:true});await call('actual-transfer-same-source-destination','Logic.transfer',[C,C,7,2,true,false,undefined,10,'source-transfer']);
 await reset('transfer-same-live-tally-reference',{rpc:true,pubkeys:true});await call('actual-transfer-same-live-address-object','Logic.transfer',[C,C,7,2,true,false,undefined,10,'source-transfer']);
 await reset('transfer-same-live-reference-credit-failure',{rpc:true,pubkeys:true});await call('actual-transfer-same-live-reference-credit-failure','Logic.transfer',[C,C,7,2,true,false,undefined,10,'source-transfer'],{failure:{collection:'tallyMapDelta',op:'insert',skip:1,message:'same-live-credit-delta'}});
 for(const [label,rows]of [['missing-row',[]],['false-data',[{_id:'tallyMap',data:false}]],['empty-string',[{_id:'tallyMap',data:''}]]])for(const stale of [false,true]){await reset('transfer-'+label+'-'+stale,{rpc:true,cached:6});if(stale)await call('prime-before-'+label,'Tally.getTally',[C,7]);await imported('import-'+label,'tallyMap',rows);await call('transfer-successful-load-'+label+'-'+stale,'Logic.transfer',[C,'destination-channel',7,2,true,false,undefined,10,'source-transfer']);}
 await reset('zero-token-volume-default',{bindTally:true});await call('actual-token-price-default','VolumeIndex.getTokenPriceInLTC',[8]);await call('actual-zero-token-volume-default','Logic.tradeTokensChannel',[7,8,0,0,20,true,C,10,'zero-logic',true]);
 for(const [label,price]of [['observed-price',.02],['falsey-price',0]]){await reset('zero-token-volume-'+label,{bindTally:true});await imported('import-token-'+label,'volumeIndex',[{_id:'0-8',value:{price}}]);await call('actual-token-'+label,'VolumeIndex.getTokenPriceInLTC',[8]);await call('actual-zero-token-'+label,'Logic.tradeTokensChannel',[7,8,0,0,20,true,C,10,'zero-logic',true]);}
 for(const [label,failure]of [['read',{op:'findOne'}],['price-factory',{op:'getDatabase'}],['save-factory',{op:'getDatabase',skip:1}],['save-write',{op:'update'}]]){await reset('zero-token-volume-failure-'+label,{bindTally:true});await call('actual-zero-token-volume-failure-'+label,'Logic.tradeTokensChannel',[7,8,0,0,20,true,C,10,'zero-logic',true],{failure:{collection:'volumeIndex',message:'zero-volume-'+label,...failure}});await call('actual-zero-token-volume-retry-'+label,'Logic.tradeTokensChannel',[7,8,0,0,20,true,C,10,'zero-retry',true]);}
 await reset('direct-token-price-read-failure');await call('actual-token-price-read-failure','VolumeIndex.getTokenPriceInLTC',[8],{failure:{collection:'volumeIndex',op:'findOne',message:'direct-token-price-read'}});await call('actual-token-price-read-retry','VolumeIndex.getTokenPriceInLTC',[8]);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;

fn string(v: V) -> String {
    let V::String(t) = v else {
        panic!("string {v:?}")
    };
    t
}
fn array(v: V) -> Vec<V> {
    let V::Array(t) = v else {
        panic!("array {v:?}")
    };
    t
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
fn observed(result: DependencyReturn) -> V {
    match result {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[
                ("name", V::String(e.name)),
                ("message", V::String(e.message)),
            ]),
        )]),
    }
}
fn source_observed(result: SourceOutcome) -> V {
    match result {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[
                ("name", V::String(e.name)),
                ("message", V::String(e.message)),
            ]),
        )]),
    }
}
fn cases() -> Vec<V> {
    let result = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let cases = std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            tradelayer_core::property_catalog::parse_diagnostic_value(line)
                .unwrap_or_else(|e| panic!("{e:?}: {}", &line[..line.len().min(200)]))
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 236, "unchanged source observation inventory");
    cases
}

struct ActualBackend<'a>(&'a mut SourceContractRuntime);
impl SourceBackend for ActualBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.0.invoke_dependency(call)
    }
}
fn compare_graph(detailed_tokens: bool) {
    let mut runtime =
        SourceContractRuntime::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3);
    let mut calls = 0;
    let mut raw_calls = 0;
    let mut effect_errors = vec![];
    for case in cases() {
        let name = string(case.get("name"));
        let c = case.get("command");
        if !matches!(c.get("reset"), V::Undefined) {
            runtime =
                SourceContractRuntime::with_policy(SourceContractRuntimePolicy::SourceLifecycleV3);
            if let V::String(entry) = c.get("reset").get("entry") {
                runtime.set_entry(&entry).unwrap();
            }
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&string(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
            if c.get("reset").get("bindTally") == V::Bool(true) {
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: "SourceGlobal.bindTally".into(),
                        arguments: vec![],
                    })
                    .unwrap();
            }
            if c.get("reset").get("rpc") == V::Bool(true) {
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: "SourceGlobal.bindRpcTransactions".into(),
                        arguments: vec![],
                    })
                    .unwrap();
            }
        } else if !matches!(c.get("seed"), V::Undefined) {
            let seed = c.get("seed");
            runtime
                .import_collection(&string(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
        } else {
            if !matches!(c.get("failure"), V::Undefined) {
                let f = c.get("failure");
                runtime
                    .fail_storage(StorageFailure {
                        collection: string(f.get("collection")),
                        operation: {
                            let op = string(f.get("op"));
                            if op == "loadDatabase" || op == "getDatabase" {
                                op
                            } else {
                                format!("{op}Async")
                            }
                        },
                        skip: if let V::Number(bits) = f.get("skip") {
                            f64::from_bits(bits) as usize
                        } else {
                            0
                        },
                        message: string(f.get("message")),
                    })
                    .unwrap();
            }
            let result = if !matches!(c.get("raw"), V::Undefined) {
                raw_calls += 1;
                source_observed(
                    runtime
                        .execute_contract_unverified(&array(c.get("raw")))
                        .unwrap_or_else(|e| panic!("{name}: {e:?}")),
                )
            } else if detailed_tokens
                && matches!(c.get("target"),V::String(t) if t=="Orderbook.processTokenMatches" || t=="Orderbook.processTokenChannelTrades")
            {
                calls += 1;
                let args = array(c.get("args"));
                let result = tradelayer_core::token_settlement_source::evaluate_token_settlement(
                    &args[1..],
                    c.get("target") == V::String("Orderbook.processTokenChannelTrades".into()),
                    &mut ActualBackend(&mut runtime),
                )
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                assert_eq!(
                    result.arguments_after,
                    array(c.get("argumentsAfter"))[1..],
                    "{name} source match mutation"
                );
                source_observed(result.outcome)
            } else {
                calls += 1;
                observed(
                    runtime
                        .invoke_dependency(&DependencyCall {
                            target: string(c.get("target")),
                            arguments: array(c.get("args")),
                        })
                        .unwrap_or_else(|e| panic!("{name}: {e:?}")),
                )
            };
            assert_eq!(result, case.get("outcome"), "{name} actual outcome");
        }
        for table in array(case.get("state").get("tables")) {
            let collection = string(table.get("name"));
            assert_eq!(
                V::Array(runtime.collection(&collection).to_vec()),
                table.get("rows"),
                "{name} persisted {collection}"
            );
        }
        assert_eq!(
            runtime.tally_cache(),
            case.get("state").get("tally"),
            "{name} actual Tally cache"
        );
        let state = runtime.state_value().unwrap();
        let channels = state.get("globalClearing").get("channels");
        assert_eq!(
            channels.get("registry"),
            case.get("state").get("channels"),
            "{name} actual channel cache"
        );
        assert_eq!(
            channels.get("pendingWithdrawals"),
            case.get("state").get("withdrawals"),
            "{name} withdrawals"
        );
        assert_eq!(
            state.get("feeBuffer"),
            case.get("state").get("feeBuffer"),
            "{name} fee buffer"
        );
        assert_eq!(
            state.get("blockTrades"),
            case.get("state").get("blockTrades"),
            "{name} clearing trades"
        );
        assert_eq!(
            state.get("globalClearing").get("unhandledRejections"),
            case.get("state").get("unhandledRejections"),
            "{name} unawaited source rejections"
        );
        for (i, book) in array(case.get("state").get("books")).iter().enumerate() {
            assert_eq!(
                runtime.queue().instance_value(i as u64 + 1).unwrap(),
                *book,
                "{name} instance{}",
                i + 1
            );
        }
        let effects=runtime.take_effects().into_iter().filter(|v|v.get("collection")==V::String("orderBooks".into())||matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock","rpc"].contains(&k.as_str()))).collect::<Vec<_>>();
        let expected = array(case.get("effects"));
        for (i, (actual, wanted)) in effects.iter().zip(&expected).enumerate() {
            if actual != wanted {
                effect_errors.push(format!("{name} effect{i}: {actual:?} != {wanted:?}"));
                break;
            }
        }
        if effects.len() != expected.len() {
            effect_errors.push(format!(
                "{name} effect lengths {} != {}",
                effects.len(),
                expected.len()
            ));
        }
    }
    assert_eq!(calls, 126, "actual method invocation inventory");
    assert_eq!(raw_calls, 4);
    assert!(effect_errors.is_empty(), "{}", effect_errors.join("\n"));
}
#[test]
fn actual_channel_financial_graph_uses_sole_source_tally_margin_and_queue() {
    compare_graph(false);
}
#[test]
fn actual_token_match_mutations_and_thrown_prefix_use_real_financial_owners() {
    compare_graph(true);
}

#[test]
fn token_price_capability_preserves_legacy_policy_refusal() {
    for policy in [
        SourceContractRuntimePolicy::OnchainV1,
        SourceContractRuntimePolicy::GlobalClearingV2,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        assert_eq!(
            runtime.invoke_dependency(&DependencyCall {
                target: "VolumeIndex.getTokenPriceInLTC".into(),
                arguments: vec![V::number(8.0)],
            }),
            Err(SourceRuleError::UnknownDependency)
        );
        assert_eq!(runtime, before);
    }
}

#[test]
fn channel_ready_sort_capacity_is_explicit_and_rolls_back_local_state() {
    use tradelayer_core::contract_queue_source_owner::ContractQueueSourceOwner;
    use tradelayer_core::validity_source::SourceCall;
    let mut owner = ContractQueueSourceOwner::new();
    let rows = (0..64)
        .map(|i| {
            obj(&[
                ("kind", V::String("other".into())),
                ("sender", V::String(format!("actor-{i}"))),
                ("blockHeight", V::number(10.0)),
                ("txid", V::String(format!("tx-{i}"))),
            ])
        })
        .collect();
    owner
        .import_rows(vec![
            obj(&[
                ("_id", V::String("activePairs".into())),
                ("pairs", V::Array(vec![V::String("channel-1".into())])),
            ]),
            obj(&[
                ("_id", V::String("channel-queue-1".into())),
                ("trades", V::Array(rows)),
            ]),
        ])
        .unwrap();
    let before = owner.checkpoint().unwrap();
    let error = owner
        .evaluate_channel_with_capability(
            &SourceCall {
                method: "processQueuedChannelTradesForBlock".into(),
                arguments: vec![V::number(10.0)],
            },
            &mut |_, _| panic!("sort admission precedes any settlement"),
        )
        .unwrap_err();
    assert_eq!(error, SourceRuleError::Capacity);
    assert_eq!(owner.checkpoint().unwrap(), before);
    assert!(owner.take_db_calls().is_empty());
    assert!(owner.take_detailed_db_calls().unwrap().is_empty());
}
