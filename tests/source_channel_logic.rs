//! Whole unchanged Logic/Channels/Scaling graph over a declared recording DB.
use tradelayer_core::source_contract_replay::{SourceContractCommand as C, SourceContractReplay};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn text(v: V) -> String {
    let V::String(v) = v else { panic!("string") };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else { panic!("array") };
    v
}
fn call_result(result: DependencyReturn) -> V {
    let (transport, outcome) = match result {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
        DependencyReturn::Throw(e) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
        DependencyReturn::PendingThrow(e) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
fn compare(actual: &V, expected: &V, label: &str) {
    fn first(a: &V, b: &V, p: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return first(a, b, format!("{p}[{i}]"));
                    }
                }
                format!("{p}: length {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{p}: field {ak} != {bk}");
                    }
                    if av != bv {
                        return first(av, bv, format!("{p}.{ak}"));
                    }
                }
                format!("{p}: length {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    if actual != expected {
        panic!("{label}: {}", first(actual, expected, "$".into()))
    }
}
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');
// Extend the pinned module inventory and the external DB's genuine $push
// operator. No source function, validator or financial response is replaced.
code=code.replace('const pins = {',"const pins = {scaling:['legacy_scaling_source.txt','ea2103b70dd051d39e44c9201a1eed2107b8110ac9803a0b825b92942799ca61'],");
code=code.replace("'txIndex','scaling','vaults'","'txIndex','vaults'");
const operator="else throw Error(`Unsupported storage update ${op}`);";
if(!code.includes(operator))throw Error('DB operator boundary changed');
code=code.replace(operator,"else if(op==='$push'){const previous=field(rows[i],k)??[];if(!Array.isArray(previous))throw Error('Cannot push to non-array');setField(rows[i],k,[...previous,v]);}else throw Error(`Unsupported storage update ${op}`);");
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,instances,types,logic,Orderbook,effects');code+='\nmodule.exports={environment,tag,BN};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);const{environment,tag,BN}=driver.exports;let e,unhandled=[],marginInstances=[];
process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
const norm=v=>v[0]==='bignumber'?['bignumber',new BN(v[1]).toFixed()]:v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE';
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const c=e.load('channels'),t=e.load('tally'),l=e.load('clearing'),k=e.load('contractRegistry');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),channels:c.channelsRegistry,pendingWithdrawals:c.pendingWithdrawals,tally:t.addresses??new Map(),margins:marginInstances.map(v=>({seriesId:v.seriesId,margins:v.margins})),books:e.instances.map(v=>({tickSize:v.tickSize,orderBookKey:v.orderBookKey,orderBooks:v.orderBooks,block:v.block})),feeBuffer:t._feeAccrualBuffer??new Map(),blockTrades:l.blockTrades,oracleMarks:l.latestOracleMarkById,nativeMarks:l.latestNativeMarkById,contractMod:!!k.modFlag,unhandled};}
function emit(name,command,result){console.log(JSON.stringify(norm(tag({name,command,result,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))}))));}
const channel=(a=100,b=50)=>({channel:'c',participants:{A,B},A:{7:a},B:{7:b},commits:[],clearLists:[]});
const tracking=()=>({_id:'c',trades:[{txid:'t',block:2,status:'pending',params:{price:100,amount:2}},{txid:'t',block:3,status:'pending',params:{price:99,amount:3}}],keepAlives:[],settlements:[{txid:'prior',block:3,status:'live'}],closes:[],netSettles:[],kingSettlements:[]});
async function reset(name,options={}){e=environment({});unhandled=[];marginInstances=[];const M=e.load('marginMap'),load=M.loadMarginMap;M.loadMarginMap=function(...args){const p=load.apply(this,args);p.then(v=>{if(v instanceof M&&!marginInstances.includes(v))marginInstances.push(v);},()=>{});return p;};e.load('channels');e.load('scaling');e.table('channels').splice(0,Infinity,...(options.missing?[]:[{_id:'c',data:channel(options.a??100,options.b??50)},{_id:'d',data:{...channel(0,0),channel:'d'}}]));e.table('scaling').splice(0,Infinity,...(options.empty?[]:[tracking()]));if(options.offset){e.table('scaling')[0].trades[0].params={contractId:1,price:100,amount:1,columnAIsMaker:true};const tally=JSON.parse(e.table('tallyMap')[0].data);tally.push(['c',{7:{amount:200,available:0,reserved:0,margin:0,vesting:0,channelBalance:200}}]);e.table('tallyMap')[0].data=JSON.stringify(tally);}await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,method,args,failure){e.effects.length=0;if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}let transport,outcome;try{let value=e.logic[method](...args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value}}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}}}}else{transport='Immediate';outcome={returned:value}}}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{method,args,failure,unsupported:method==='transfer'},{transport,outcome});}
async function raw(name,type,params,block=10){e.effects.length=0;const encoded=e.load('txEncoder')[type===23?'encodeSettleChannelPNL':'encodeKingSettle'](params);const arguments=[name,type,'tl',encoded.slice(3),'c',null,0,0,block];let decoded,outcome;try{decoded=await e.types.decodePayload(...arguments);let returned;if(decoded.valid===true)returned=await e.logic.typeSwitch(type,decoded);outcome={params:decoded,returned}}catch(error){outcome={params:decoded,thrown:{name:error.name,message:error.message}}}await flush();emit(name,{raw:arguments},{outcome});}
const king=(extra={})=>({senderAddress:'c',blockStart:1,blockEnd:9,propertyId:7,netAmount:3,aPaysBDirection:true,channelRoot:'root',totalContracts:2,neutralizedCount:0,block:10,txid:'king',...extra});
(async()=>{
await reset('direct-metadata');await call('keep-first-duplicate-only','processKeepAlive',['c','t',10,'keep']);await call('settle-keep-records-real-scaling','settleChannelPNL',['c',{settleType:0,txidNeutralized1:'t'},10,'keep2']);await call('close-records-and-neutralizes','processClosePosition',['c','t','keep',102,false,11,'close']);await call('net-matches-first-per-array','processNetSettle',['c','t','keep2',-3,true,12,'net']);await call('string-enum-is-unknown','settleChannelPNL',['c',{settleType:'0',txidNeutralized1:'t'},13,'unknown']);await call('null-params-throws','settleChannelPNL',['c',null,13,'bad']);
await reset('close-contract-offset',{offset:true});await call('close-reaches-real-contract-offset','processClosePosition',['c','t','keep',102,false,11,'offset']);
await reset('close-contract-offset-failure',{offset:true});await call('close-offset-margin-factory-failure-prefix','processClosePosition',['c','t','keep',102,false,11,'offset-failed'],{collection:'marginMaps',op:'getDatabase',message:'offset-margin-factory'});await call('close-offset-retry-executes-real-financial-graph','processClosePosition',['c','t','keep',102,false,11,'offset-retry']);
await reset('close-contract-offset-live-alias-failure',{offset:true});await call('close-offset-live-alias-prefix-before-delta-failure','processClosePosition',['c','t','keep',102,false,11,'offset-alias-failed'],{collection:'marginMapDelta',op:'insert',skip:2,message:'offset-alias-delta'});await call('close-offset-live-alias-retry','processClosePosition',['c','t','keep',102,false,11,'offset-alias-retry']);
await reset('empty-tracking',{empty:true});await call('close-missing-doc-noop','processClosePosition',['c','t','k',102,false,10,'close']);await call('net-creates-full-empty-doc','processNetSettle',['c','a','b','-4',false,10,'net']);await call('keep-missing-trade','processKeepAlive',['c','missing',11,'keep']);
await reset('king-string-credit',{b:'50'});await call('king-string-plus','batchSettlement',[king()]);await call('king-retry-is-second-payment','batchSettlement',[king()]);await call('king-other-direction-negative-amount','batchSettlement',[king({aPaysBDirection:false,netAmount:-2,txid:'other'})]);
await reset('king-insufficient-direct');await call('king-permits-negative-balances','batchSettlement',[king({netAmount:200})]);
await reset('king-missing-property');await call('king-sweep-persists-before-property-error','batchSettlement',[king({propertyId:null})]);
for(const [name,failure]of [['channel-save',{collection:'channels',op:'update',message:'channel-save'}],['late-scaling',{collection:'scaling',op:'update',skip:1,message:'late-scaling'}]]){await reset('failure-'+name);await call(name+'-prefix','batchSettlement',[king()],failure);await call(name+'-retry','batchSettlement',[king()]);}
await reset('settle-record-failure');await call('keep-prefix-before-scaling-record-failure','settleChannelPNL',['c',{settleType:0,txidNeutralized1:'t'},10,'keep'],{collection:'scaling',op:'update',skip:1,message:'record'});await call('keep-retry-duplicates-metadata','settleChannelPNL',['c',{settleType:0,txidNeutralized1:'t'},10,'keep']);
await reset('raw-23');await raw('raw-keep',23,{settleType:0,txidNeutralized1:'t',expiryBlock:30});await raw('raw-close',23,{settleType:1,txidNeutralized1:'t',txidNeutralized2:'raw-keep',markPrice:102,expiryBlock:30});await raw('raw-net',23,{settleType:2,txidNeutralized1:'prior',txidNeutralized2:'raw-close',netAmount:2,propertyId:7,expiryBlock:30,columnAIsSeller:true});await raw('raw-king-via23',23,{settleType:3,blockStart:1,blockEnd:9,propertyId:7,netAmount:3,aPaysBDirection:true,channelRoot:'root',totalContracts:2,expiryBlock:30});
await reset('raw-31');await raw('raw-king31',31,king());await raw('raw-king31-repeat',31,king());await raw('raw-king31-insufficient',31,king({netAmount:200}));
await reset('withdraw-cold');await call('withdraw-is-sync-with-two-unawaited-calls','withdrawal',[false,'c',7,3,A,10,false]);await call('withdraw-write-failure-still-returns','withdrawal',[false,'c',7,3,A,11,true],{collection:'withdrawQueue',op:'update',message:'withdraw-write'});
await reset('withdraw-cold-load-failure');await call('withdraw-getChannel-failure-independent-of-enqueue','withdrawal',[false,'c',7,3,A,10,false],{collection:'channels',op:'find',message:'channel-read'});
await reset('transfer-different');await call('transfer-live-references-and-unawaited-tally','transfer',['c','d',7,3,true,false,'',10,'transfer']);
await reset('transfer-same');await call('transfer-same-key-live-reference','transfer',['c','c',7,3,true,true,'PAY',10,'same']);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn source_cases() -> Vec<V> {
    let result = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn failure(value: V) -> StorageFailure {
    StorageFailure {
        collection: text(value.get("collection")),
        operation: {
            let op = text(value.get("op"));
            if op == "getDatabase" {
                op
            } else {
                format!("{op}Async")
            }
        },
        skip: match value.get("skip") {
            V::Number(bits) => f64::from_bits(bits) as usize,
            _ => 0,
        },
        message: text(value.get("message")),
    }
}
fn assert_state(runtime: &SourceContractRuntime, case: &V) {
    let name = text(case.get("name"));
    let expected = case.get("state");
    let mut names = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let collection = text(table.get("name"));
        names.insert(collection.clone());
        compare(
            &V::Array(runtime.collection(&collection).to_vec()),
            &table.get("rows"),
            &format!("{name} persisted {collection}"),
        );
    }
    let state = runtime.state_value().unwrap();
    for table in array(state.get("tables")) {
        let collection = text(table.get("name"));
        if !names.contains(&collection) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} unexpected {collection}"
            )
        }
    }
    let global = state.get("globalClearing");
    let margin_state = runtime.margins().state_value().unwrap();
    let objects = array(margin_state.get("objects"));
    let margins = array(margin_state.get("instances"))
        .into_iter()
        .map(|instance| {
            let entries = array(instance.get("margins"))
                .into_iter()
                .map(|entry| {
                    let pair = array(entry);
                    let V::Number(bits) = pair[1] else {
                        panic!("position identity")
                    };
                    V::Array(vec![
                        pair[0].clone(),
                        objects[f64::from_bits(bits) as usize].clone(),
                    ])
                })
                .collect();
            obj(&[
                ("seriesId", instance.get("seriesId")),
                ("margins", V::Array(entries)),
            ])
        })
        .collect();
    compare(
        &V::Array(margins),
        &expected.get("margins"),
        &format!("{name} every live Margin instance"),
    );
    for (field, key) in [
        ("feeBuffer", "feeBuffer"),
        ("blockTrades", "blockTrades"),
        ("oracleMarks", "oracleMarks"),
        ("nativeMarks", "nativeMarks"),
    ] {
        compare(
            &state.get(field),
            &expected.get(key),
            &format!("{name} source financial cache {field}"),
        );
    }
    compare(
        &state.get("continuation").get("contractMod"),
        &expected.get("contractMod"),
        &format!("{name} contract modification flag"),
    );
    for (index, book) in array(expected.get("books")).iter().enumerate() {
        compare(
            &runtime.queue().instance_value(index as u64 + 1).unwrap(),
            book,
            &format!("{name} actual book instance{}", index + 1),
        );
    }
    compare(
        &global.get("channels").get("registry"),
        &expected.get("channels"),
        &format!("{name} complete live Channels registry"),
    );
    compare(
        &global.get("channels").get("pendingWithdrawals"),
        &expected.get("pendingWithdrawals"),
        &format!("{name} pending withdrawals"),
    );
    compare(
        &runtime.tally_cache(),
        &expected.get("tally"),
        &format!("{name} Tally"),
    );
    compare(
        &global.get("unhandledRejections"),
        &expected.get("unhandled"),
        &format!("{name} actual unhandled source rejections"),
    );
}
#[test]
fn genuine_channel_logic_direct_and_raw_lifecycle_matches_all_rows_cache_and_ordered_effects() {
    let mut runtime = SourceContractRuntime::with_policy(P::SourceLifecycleV3);
    let mut calls = 0;
    let mut actions = 0;
    for case in source_cases() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            runtime = SourceContractRuntime::with_policy(P::SourceLifecycleV3);
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
        } else {
            if command.get("unsupported") == V::Bool(true) {
                assert_eq!(
                    case.get("result")
                        .get("outcome")
                        .get("thrown")
                        .get("message"),
                    s("Unowned boundary: TxUtils.getRawTransaction")
                );
                let before = runtime.clone();
                assert_eq!(
                    runtime.invoke_dependency(&DependencyCall {
                        target: "Logic.transfer".into(),
                        arguments: array(command.get("args")),
                    }),
                    Err(SourceRuleError::UnknownDependency)
                );
                assert!(
                    runtime == before,
                    "unowned RPC admission must retain the complete owner"
                );
                actions += 1;
                continue;
            }
            let f = command.get("failure");
            if matches!(f, V::Object(_)) {
                runtime.fail_storage(failure(f)).unwrap();
            }
            let actual = if let V::Array(arguments) = command.get("raw") {
                let outcome = runtime
                    .execute_contract_unverified(&arguments)
                    .unwrap_or_else(|e| panic!("{name} local {e:?}"));
                let result = match outcome {
                    SourceOutcome::Returned(v) => v,
                    SourceOutcome::Thrown(e) => obj(&[
                        (
                            "params",
                            runtime
                                .last_contract_parameters()
                                .cloned()
                                .unwrap_or(V::Undefined),
                        ),
                        (
                            "thrown",
                            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                        ),
                    ]),
                };
                obj(&[("outcome", result)])
            } else {
                call_result(
                    runtime
                        .invoke_dependency(&DependencyCall {
                            target: format!("Logic.{}", text(command.get("method"))),
                            arguments: array(command.get("args")),
                        })
                        .unwrap_or_else(|e| panic!("{name} local {e:?}")),
                )
            };
            compare(
                &actual,
                &case.get("result"),
                &format!("{name} original outcome and transport"),
            );
            calls += 1;
        }
        assert_state(&runtime, &case);
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} complete ordered DB/capability effects"),
        );
        actions += 1;
    }
    assert_eq!(calls, 35);
    assert_eq!(actions, 54);
}

#[test]
fn channel_partial_prefixes_replay_and_continue_without_substituting_poststate() {
    let cases = source_cases();
    for selected in [
        "failure-channel-save",
        "failure-late-scaling",
        "settle-record-failure",
        "close-contract-offset-failure",
        "close-contract-offset-live-alias-failure",
    ] {
        let start = cases
            .iter()
            .position(|c| c.get("name") == s(selected))
            .unwrap();
        let mut replay = SourceContractReplay::with_policy(P::SourceLifecycleV3).unwrap();
        for table in array(cases[start].get("state").get("tables")) {
            replay
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        for case in &cases[start + 1..start + 3] {
            let command = case.get("command");
            let f = command.get("failure");
            if matches!(f, V::Object(_)) {
                replay.apply(C::FailStorage(failure(f))).unwrap();
            }
            replay
                .apply(C::Invoke(DependencyCall {
                    target: format!("Logic.{}", text(command.get("method"))),
                    arguments: array(command.get("args")),
                }))
                .unwrap();
            assert_state(replay.runtime(), case);
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert!(
                replay == restored,
                "{} whole continuation changed",
                text(case.get("name"))
            );
            replay = restored;
        }
    }
}
