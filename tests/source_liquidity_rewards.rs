//! Whole pinned Logic reward producer/queue/settlement over sole actual Core.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay as Replay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};
fn n(x: f64) -> V {
    V::number(x)
}
fn s(x: &str) -> V {
    V::String(x.into())
}
fn obj(a: &[(&str, V)]) -> V {
    V::Object(a.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn arr(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("array {v:?}")
    }
}
fn txt(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("string {v:?}")
    }
}
fn invoke(target: &str, arguments: Vec<V>) -> C {
    C::Invoke(DependencyCall {
        target: target.into(),
        arguments,
    })
}
const REFERENCE: &str = r###"const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=path.resolve(process.argv[1]||'outputs/TradeLayer-Rust'),version=process.env.TL_SOURCE_BN_VERSION||'9.3.1',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));
if(version==='9.1.2'){const z=require('node:zlib'),vm=require('node:vm'),crypto=require('node:crypto'),packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],source=z.gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('9.1.2 hash');const m={exports:{}};vm.runInNewContext(source.toString(),{module:m,exports:m.exports},{timeout:1000});numeric.loadBigNumber=()=>m.exports;}else if(version!=='9.3.1')throw Error('version');
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));code=code.replace('return {types,logic,Orderbook,effects','return {storage,context,tables,instances,types,logic,Orderbook,effects');code+='\nmodule.exports={environment,tag,A,B,BN};\n';const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);const {environment,tag,A,B,BN}=m.exports;
let e,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
const norm=v=>v[0]==='bignumber'?['bignumber',new BN(v[1]).isZero()&&new BN(v[1]).isNegative()?'-0':new BN(v[1]).toFixed()]:v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<4;i++)await new Promise(r=>setImmediate(r));}
function state(){const T=e.load('tally'),Ch=e.load('channels');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),queue:e.logic._liquidityRewardQueue,volumeCache:e.logic._liquidityRewardVolumeCache,tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,channels:Ch.channelsRegistry,books:e.instances.map(v=>({tickSize:v.tickSize,orderBookKey:v.orderBookKey,orderBooks:v.orderBooks,block:v.block})),unhandled};}
function emit(name,command,result){console.log(JSON.stringify(norm(tag({name,command,result,state:state(),effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock','rpc','source-function'].includes(v.kind))}))));}
async function reset(name,options={}){unhandled=[];e=environment({entry:options.entry??'main-subgraph'});Object.assign(e.context.process.env,options.env||{});if(options.channel){const T=JSON.parse(e.table('tallyMap')[0].data),p=options.property??7;T.push(['reward-channel',{[p]:{amount:100,available:0,reserved:0,margin:0,vesting:0,channelBalance:100}}]);for(const[,b]of T){if(!b[p])b[p]={amount:1000,available:1000,reserved:0,margin:0,vesting:0,channelBalance:0};}e.table('tallyMap')[0].data=JSON.stringify(T);e.table('channels').push({_id:'reward-channel',data:{channel:'reward-channel',participants:{A,B},A:{[p]:100},B:{[p]:100},commits:[]}});const props=JSON.parse(e.table('propertyList')[0].value);if(!props.some(([id])=>id===p))props.push([p,{ticker:'REWARD',issuer:A,type:1}]);e.table('propertyList')[0].value=JSON.stringify(props);for(const address of options.listed??[A,B])e.table('attestations').push({_id:'reward-'+address,data:{address,listId:2,status:'active'}});e.table('attestations').push({_id:'issuer-reward',data:{address:A,listId:1,status:'active'}});}
for(const method of ['queueLiquidityReward','settleLiquidityRewards','tradeTokenForUTXO']){const original=e.logic[method];e.logic[method]=function(...args){e.effects.push({kind:'source-function',target:'Logic.'+method,args:args.slice()});return Reflect.apply(original,this,args);};}
await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op==='getDatabase'){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}else e.setFailure(f);}

function ownerFor(method){if(!method.includes('.'))return e.logic;const ns=method.split('.')[0];return e.load(({VolumeIndex:'volumeIndex'})[ns]||ns.toLowerCase());}
async function call(name,method,args=[],failure){e.effects.length=0;if(failure)fail(failure);let transport,outcome;try{let value=Reflect.apply(ownerFor(method)[method.split('.').pop()],ownerFor(method),args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value}}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}}}}else{transport='Immediate';outcome={returned:value}}}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{target:method.includes('.')?method:'Logic.'+method,args,failure},{transport,outcome});}
(async()=>{
await reset('initial-ten');
await call('empty-settlement','settleLiquidityRewards',[10]);
await call('queue-positive','queueLiquidityReward',[A,'1.123456789',10,'first','firstReason']);
await call('aggregate-preserves-first-metadata','queueLiquidityReward',[A,'.000000019','10','second','secondReason']);
await call('queue-second-address','queueLiquidityReward',[B,2,10,'third','']);
await call('settle-real-tally','settleLiquidityRewards',[10]);
await call('settle-deleted-queue-again','settleLiquidityRewards',['10']);
await call('sub-satoshi-reward-ignored','queueLiquidityReward',[A,'.000000009',10,'tiny']);
await call('negative-reward-ignored','queueLiquidityReward',[A,-2,10,'negative']);
await call('nan-source-accepted-into-queue','queueLiquidityReward',[A,'NaN',10,'nan']);
await call('nan-source-settlement','settleLiquidityRewards',[10]);

const pair=async()=>{await call('queue-A','queueLiquidityReward',[A,1,10,'first-A']);await call('queue-B','queueLiquidityReward',[B,2,10,'first-B']);};
for(const f of [{collection:'tallyMapDelta',op:'insert',skip:1},{collection:'tallyMap',op:'update'},{collection:'tallyMap',op:'getDatabase'},{collection:'tallyMap',op:'findOne'}]){const label=f.collection+'-'+f.op+'-'+(f.skip??0);await reset('settlement-failure-'+label);await pair();await call('settle-prefix-'+label,'settleLiquidityRewards',[10],{...f,message:label});await call('settle-retry-'+label,'settleLiquidityRewards',[10]);}
await reset('block-and-address-keys');for(const[address,block,amount]of [[0,10,1],[-0,'10',2],['0',10,3],[undefined,undefined,1],[null,null,2],[NaN,false,3]])await call('queue-original-key-'+String(address)+'-'+String(block),'queueLiquidityReward',[address,amount,block,'keys',null]);for(const block of [10,undefined,null,false])await call('settle-original-block-'+String(block),'settleLiquidityRewards',[block]);
for(const[label,amount]of [['bool',true],['array',['1.123456789']],['bad-object',{}],['infinite',Infinity],['negative-infinite',-Infinity]]){await reset('numeric-'+label);await call('queue-numeric-'+label,'queueLiquidityReward',[A,amount,10,label]);await call('settle-numeric-'+label,'settleLiquidityRewards',[10]);}
const trade=(p=7,amount=2,column=true,price=1)=>['reward-channel',B,p,amount,column,2e8,A,2e8,price,1,null,10,'reward-trade'];
await reset('producer-missing-channel');await call('producer-no-channel-noop','tradeTokenForUTXO',trade());
await reset('producer-ordinary-default-env',{channel:true});await call('producer-ordinary-default-env','tradeTokenForUTXO',trade());await call('settle-ineligible-noop','settleLiquidityRewards',[10]);
await reset('producer-default-property-zero',{channel:true,property:0});await call('producer-eligible-default-zero','tradeTokenForUTXO',trade(0));await call('settle-genuine-default-rewards','settleLiquidityRewards',[10]);
await reset('producer-eligible-seven',{channel:true,env:{LIQUIDITY_REWARD_PROPERTY_ALLOWLIST:'7'}});await call('producer-genuine-listed-seven','tradeTokenForUTXO',trade());await call('producer-reuses-volume-cache','tradeTokenForUTXO',trade());await call('settle-genuine-seven-rewards','settleLiquidityRewards',[10]);
for(const listed of [[],['reward-channel'],[B],['reward-channel',B]]){await reset('producer-participants-'+listed.length+'-'+String(listed[0]),{channel:true,listed,env:{LIQUIDITY_REWARD_PROPERTY_ALLOWLIST:'7'}});await call('producer-eligible-participant-count','tradeTokenForUTXO',trade(7,2,true,NaN));await call('settle-participant-queues','settleLiquidityRewards',[10]);}
await reset('producer-const-reassignment',{channel:true});await call('producer-too-large-const-error','tradeTokenForUTXO',trade(7,200));
for(const f of [{collection:'channels',op:'update'},{collection:'tallyMapDelta',op:'insert'},{collection:'feeCache',op:'update'},{collection:'volumeIndex',op:'update'},{collection:'tradeHistory',op:'update'},{collection:'attestations',op:'find'},{collection:'volumeIndex',op:'findOne',skip:2}]){const label=f.collection+'-'+f.op+'-'+(f.skip??0);await reset('producer-failure-'+label,{channel:true,listed:['reward-channel',B],env:{LIQUIDITY_REWARD_PROPERTY_ALLOWLIST:'7'}});await call('producer-prefix-'+label,'tradeTokenForUTXO',trade(),{...f,message:label});await call('producer-retry-'+label,'tradeTokenForUTXO',trade());await call('producer-settle-'+label,'settleLiquidityRewards',[10]);}

async function imported(name,collection,rows){e.table(collection).splice(0,Infinity,...structuredClone(rows));e.effects.length=0;emit(name,{seed:{collection,rows}},{outcome:{returned:undefined}});}
await reset('cumulative-source-direct');await call('cumulative-empty','VolumeIndex.getCumulativeVolumes',[10]);
await imported('cumulative-ordered-original-rows','volumeIndex',[{_id:'globalCumulativeVolume',value:{block:'8',globalCumulativeVolume:'8'}},{_id:'globalCumulativeVolume',value:{block:'9',globalCumulativeVolume:'9'}},{_id:'globalCumulativeVolume',value:{block:7,globalCumulativeVolume:7}},{_id:'contractCumulativeVolume',value:{block:9,contractCumulativeVolume:6}},{_id:'ltcPairCumulativeVolume',value:{block:9,ltcPairTotalVolume:3}}]);
await call('cumulative-latest-retains-string-value','VolumeIndex.getCumulativeVolumes',[10]);await call('cumulative-original-string-query-bound','VolumeIndex.getCumulativeVolumes',['10']);
for(const skip of [0,1,2]){await call('cumulative-independent-read-catch-'+skip,'VolumeIndex.getCumulativeVolumes',[10],{collection:'volumeIndex',op:'find',skip,message:'cum-read-'+skip});await call('cumulative-read-retry-'+skip,'VolumeIndex.getCumulativeVolumes',[10]);}
await call('cumulative-factory-first-failure','VolumeIndex.getCumulativeVolumes',[10],{collection:'volumeIndex',op:'getDatabase',message:'cum-factory'});await call('cumulative-factory-retry','VolumeIndex.getCumulativeVolumes',[10]);

for(const key of ['LIQUIDITY_REWARD_MAX_SHARE','LIQUIDITY_REWARD_MIN_SHARE','LIQUIDITY_REWARD_PIVOT_LTC','LIQUIDITY_REWARD_SLOPE']){await reset('producer-nan-environment-'+key,{channel:true,listed:['reward-channel',B],env:{LIQUIDITY_REWARD_PROPERTY_ALLOWLIST:'7',[key]:'bad'}});await call('producer-nan-context-'+key,'tradeTokenForUTXO',trade());await call('settle-nan-budget-no-queue-'+key,'settleLiquidityRewards',[10]);}
await reset('direct-source-reward-math');await call('direct-source-max-nan','VolumeIndex.calculateLiquidityReward',[2,7,{feePaid:.0001,cumulativeLtc:2,maxShare:NaN}]);

await call('direct-share-negative-zero','VolumeIndex.calculateLiquidityReward',[2,7,{feePaid:.0001,cumulativeLtc:2,maxShare:-0,minShare:-0}]);
await call('direct-negative-fee-still-reads-cumulative','VolumeIndex.calculateLiquidityReward',[2,7,{feePaid:-1,cumulativeLtc:0}]);
await call('direct-null-reward-context','VolumeIndex.calculateLiquidityReward',[2,7,null]);
await call('direct-nan-fee-still-reads-cumulative','VolumeIndex.calculateLiquidityReward',[2,7,{feePaid:NaN,cumulativeLtc:0}]);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn source() -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| tradelayer_core::property_catalog::parse_diagnostic_value(l).unwrap())
        .collect()
}
fn result(o: &O) -> V {
    let (transport, outcome) = match o {
        O::Dependency(DependencyReturn::Immediate(v)) => {
            ("Immediate", obj(&[("returned", v.clone())]))
        }
        O::Dependency(DependencyReturn::Pending(v)) => ("Pending", obj(&[("returned", v.clone())])),
        O::Dependency(DependencyReturn::Throw(e)) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
        O::Dependency(DependencyReturn::PendingThrow(e)) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
        o => panic!("source outcome {o:?}"),
    };
    obj(&[("transport", s(transport)), ("outcome", outcome)])
}
fn genuine_reward_producer_queue_settlement_and_retained_prefixes_replay_for_policy(policy: P) {
    let cases = source();
    let mut replay = Replay::with_policy(policy).unwrap();
    let (mut resets, mut comparable, mut refusals) = (0, 0, 0);
    for case in cases {
        let name = txt(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            resets += 1;
            replay = Replay::with_policy(policy).unwrap();
            let options = command.get("reset");
            if let V::String(entry) = options.get("entry") {
                replay.apply(C::SetEntry(entry)).unwrap();
            }
            for table in arr(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: txt(table.get("name")),
                        rows: arr(table.get("rows")),
                    })
                    .unwrap();
            }
            if let V::Object(fields) = options.get("env") {
                replay
                    .apply(invoke(
                        "SourceEnvironment.configure",
                        vec![V::Object(fields)],
                    ))
                    .unwrap();
            }
            continue;
        }
        if matches!(command.get("seed"), V::Object(_)) {
            let seed = command.get("seed");
            replay
                .apply(C::Import {
                    collection: txt(seed.get("collection")),
                    rows: arr(seed.get("rows")),
                })
                .unwrap();
            continue;
        }
        if let V::Object(_) = command.get("failure") {
            let f = command.get("failure");
            let op = txt(f.get("op"));
            replay
                .apply(C::FailStorage(StorageFailure {
                    collection: txt(f.get("collection")),
                    operation: if op == "getDatabase" {
                        op
                    } else {
                        format!("{op}Async")
                    },
                    skip: match f.get("skip") {
                        V::Number(b) => f64::from_bits(b) as usize,
                        _ => 0,
                    },
                    message: txt(f.get("message")),
                }))
                .unwrap();
        }
        let before = replay.runtime().state_value().unwrap();
        let receipt = replay
            .apply(invoke(
                &txt(command.get("target")),
                arr(command.get("args")),
            ))
            .unwrap();
        if name == "settle-numeric-infinite" {
            assert_eq!(
                receipt.outcome,
                O::LocalError(SourceRuleError::UnsupportedNumericDomain)
            );
            assert_eq!(
                replay.runtime().state_value().unwrap(),
                before,
                "nonfinite financial domain local rollback"
            );
            assert!(receipt.effects.is_empty());
            refusals += 1;
            continue;
        }
        assert!(
            !matches!(receipt.outcome, O::LocalError(_)),
            "{name} {:?} effects={:?}",
            receipt.outcome,
            receipt.effects
        );
        comparable += 1;
        assert_eq!(
            result(&receipt.outcome),
            case.get("result"),
            "{name} original source outcome"
        );
        let state = replay.runtime().state_value().unwrap();
        let expected = case.get("state");
        for table in arr(expected.get("tables")) {
            assert_eq!(
                V::Array(
                    replay
                        .runtime()
                        .collection(&txt(table.get("name")))
                        .to_vec()
                ),
                table.get("rows"),
                "{name} persisted {:?}",
                table.get("name")
            );
        }
        for key in ["queue", "volumeCache"] {
            assert_eq!(
                state.get("liquidityRewards").get(key),
                expected.get(key),
                "{name} actual Logic {key}"
            );
        }
        assert_eq!(
            state.get("tally"),
            expected.get("tally"),
            "{name} sole Tally cache"
        );
        assert_eq!(
            state.get("globalClearing").get("tallyStaticInitialized"),
            expected.get("tallyInitialized"),
            "{name} Tally initialization"
        );
        assert_eq!(
            state.get("globalClearing").get("channels").get("registry"),
            expected.get("channels"),
            "{name} live channel cache"
        );
        assert_eq!(
            state.get("globalClearing").get("unhandledRejections"),
            expected.get("unhandled"),
            "{name} unhandled continuations"
        );
        for (i, book) in arr(expected.get("books")).iter().enumerate() {
            assert_eq!(
                replay
                    .runtime()
                    .queue()
                    .instance_value(i as u64 + 1)
                    .unwrap(),
                *book,
                "{name} queue instance{i}"
            );
        }
        let effects=receipt.effects.iter().filter_map(|e|if matches!(e.get("kind"),V::String(t)if["database","db","uuid","random","rpc-clock","rpc"].contains(&t.as_str())){Some(e.clone())}else if e.get("kind")==s("dependency")&&matches!(e.get("target"),V::String(t)if t.starts_with("Logic.")){Some(obj(&[("kind",s("source-function")),("target",e.get("target")),("args",e.get("args"))]))}else{None}).collect::<Vec<_>>();
        assert_eq!(
            V::Array(effects),
            case.get("effects"),
            "{name} complete ordered DB/environment/source effects"
        );
        let saved = replay.checkpoint().unwrap();
        assert_eq!(
            Replay::from_checkpoint(&saved, replay.head()).unwrap(),
            replay,
            "{name} complete actual replay retains cache/queue/failure continuation"
        );
    }
    assert_eq!((resets, comparable, refusals), (33, 107, 1));
}
#[test]
fn genuine_reward_producer_queue_settlement_and_retained_prefixes_replay() {
    genuine_reward_producer_queue_settlement_and_retained_prefixes_replay_for_policy(
        P::ProofLifecycleV6,
    );
}

#[test]
fn v7_inherits_genuine_reward_producer_queue_settlement_and_retained_prefixes_replay() {
    genuine_reward_producer_queue_settlement_and_retained_prefixes_replay_for_policy(
        P::LowerLifecycleV7,
    );
}

#[test]
fn older_policies_keep_reward_routes_unavailable() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        for method in [
            "queueLiquidityReward",
            "settleLiquidityRewards",
            "tradeTokenForUTXO",
        ] {
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: format!("Logic.{method}"),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
    }
}
#[test]
fn reward_identity_and_capacity_limits_refuse_locally_without_mutating_owner() {
    let mut runtime = SourceContractRuntime::with_policy(P::ProofLifecycleV6);
    let before = runtime.clone();
    assert_eq!(
        runtime.invoke_dependency(&DependencyCall {
            target: "Logic.queueLiquidityReward".into(),
            arguments: vec![obj(&[]), n(1.), n(10.), s("object-key")]
        }),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert_eq!(runtime, before);
    let mut reached = false;
    for i in 0..1500 {
        let before = runtime.clone();
        let call = DependencyCall {
            target: "Logic.queueLiquidityReward".into(),
            arguments: vec![s(&format!("actor{i}")), n(1.), n(10.), s("bounded")],
        };
        match runtime.invoke_dependency(&call) {
            Ok(_) => {
                runtime.take_effects();
            }
            Err(SourceRuleError::Capacity) => {
                assert_eq!(
                    runtime, before,
                    "capacity rollback includes attempted private-map mutation"
                );
                reached = true;
                break;
            }
            e => panic!("unexpected capacity result {e:?}"),
        }
    }
    assert!(
        reached,
        "existing SourceValue state bound must apply to actual private queue"
    );
}
