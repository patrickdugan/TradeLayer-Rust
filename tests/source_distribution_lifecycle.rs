//! Literal distribution/trade prefixes over the unchanged complete JS modules.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    SourceContractReplayHead,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array")
    };
    value
}
fn text(value: V) -> String {
    let V::String(value) = value else {
        panic!("string")
    };
    value
}
fn returned(value: DependencyReturn) -> V {
    let (transport, outcome) = match value {
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
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm');
const root=process.argv[1],version=process.argv[2],numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN;
if(version==='9.1.2'){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],source=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));if(crypto.createHash('sha256').update(source).digest('hex')!=='20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e')throw Error('genuine9.1.2');const loaded={exports:{}};vm.runInNewContext(source.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}else{if(version!=='9.3.1')throw Error('version');BN=numeric.loadBigNumber();}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');const end=code.indexOf('const scenes=[');if(end<0)throw Error('source environment changed');code=code.slice(0,end);if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('source inventory changed');code=code.replace('return {types,logic,Orderbook,effects','return {context,storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE';let e,lastParams,bound=false,unhandled=[];
process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),O=e.load('oracle'),C=e.load('contractRegistry'),Ch=e.load('channels');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,oracleCache:O.instance?.oracles??new Map(),oracleInitialized:!!O.instance,contractMod:C.modFlag,channels:[...Ch.channelsRegistry],unhandled,lastParams,bound};}
function emit(name,command,result,extra){console.log(JSON.stringify(tag({name,command,result,state:state(),effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind)),...extra})));}
async function reset(name,options={}){e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',inactive:options.inactive}:options.entry==='volume-first'?{preload:'volumeIndex',inactive:options.inactive}:{entry:options.entry??'main-subgraph',inactive:options.inactive});lastParams=undefined;bound=false;unhandled=[];for(const method of ['payToTokens','tradeBaiUrbun','tradeMurabaha']){const original=e.logic[method];e.logic[method]=function(...args){e.effects.push({kind:'source-function',target:'Logic.'+method,args:structuredClone(args.map(v=>v===e.load('channels')?{sourceChannels:true}:v))});return Reflect.apply(original,this,args);};}await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
async function call(name,method,args=[]){e.effects.length=0;let transport='Immediate',outcome;try{let value=Reflect.apply(e.logic[method],e.logic,args.map(v=>v&&v.sourceChannels===true?e.load('channels'):v));if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target:'Logic.'+method,args},{transport,outcome});}
async function bind(name){e.effects.length=0;e.context.tallyMap=e.load('tally');bound=true;emit(name,{target:'SourceGlobal.bindTally',args:[]},{transport:'Pending',outcome:{returned:undefined}});}
async function raw(name,type,payload){e.effects.length=0;lastParams=undefined;const args=[name,type,'tl',payload,A,null,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args},{outcome});}
(async()=>{
for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 for(const[name,args]of [['omitted',[]],['normal',[1,7,2,A,10]],['null-values',[null,null,null]],['negative',[1,7,-1,A,10]],['noncoercible-arguments',[{toString:null},[],{},A,10]]])await call(entry+'-pay-unbound-'+name,'payToTokens',args);
 await call(entry+'-bai-omitted','tradeBaiUrbun',[]);
 for(const[name,receiver]of [['null',null],['number',7],['string','chan'],['array',[]],['record',{hasChannel:true}],['actual-Channels',{sourceChannels:true}]])await call(entry+'-bai-'+name,'tradeBaiUrbun',[{}, {}, receiver,'chan',7,1,100,2,3,100,90]);
 await call(entry+'-mur-normal','tradeMurabaha',['chan',A,B,7,2,1,100]);await call(entry+'-mur-no-arguments','tradeMurabaha',[]);
 for(const[name,payload]of [['normal','1,7,2'],['empty',''],['malformed','-1,!,bad'],['null',null],['undefined',undefined],['number',7]])await raw(entry+'-raw26-'+name,26,payload);
 for(const type of [28,29])for(const[name,payload]of [['normal','chan,7,1'],['empty',''],['null',null],['undefined',undefined],['number',7]])await raw(entry+'-raw'+type+'-'+name,type,payload);
 await bind(entry+'-bind-real-Tally-export');
 for(const[name,args]of [['normal',[1,7,2,A,10]],['omitted',[]],['noncoercible-arguments',[{toString:null},[],{},A,10]]])await call(entry+'-pay-export-'+name,'payToTokens',args);
 await raw(entry+'-raw26-bound-class-does-not-repair-validator',26,'1,7,2');
}
for(const amount of [2,0]){
 e=driver.exports.environment({});lastParams=undefined;bound=false;unhandled=[];await flush();e.effects.length=0;const instance=await e.load('tally').getInstance();await flush();const factoryEffects=e.effects.filter(v=>['database','db'].includes(v.kind));e.context.tallyMap=instance;e.effects.length=0;let error;try{await e.logic.payToTokens(1,7,amount,A,10);}catch(value){error={name:value.name,message:value.message};}await flush();emit('node-only-real-instance-'+amount,{nodeOnly:true},{transport:'PendingThrow',outcome:{thrown:error}},{factoryEffects,instanceAddresses:[...instance.addresses],staticAddressCount:e.load('tally').addresses.size});
}
})().catch(error=>{console.error(error);process.exitCode=1;});
"###;
fn cases(version: &str) -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR"), version])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect()
}
fn assert_state(runtime: &SourceContractRuntime, case: &V) {
    let expected = case.get("state");
    let name = text(case.get("name"));
    let mut known = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let collection = text(table.get("name"));
        known.insert(collection.clone());
        assert_eq!(
            V::Array(runtime.collection(&collection).to_vec()),
            table.get("rows"),
            "{name} exact shared rows {collection}"
        );
    }
    let actual = runtime.state_value().unwrap();
    for table in array(actual.get("tables")) {
        if !known.contains(&text(table.get("name"))) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} unexpected rows"
            );
        }
    }
    let global = actual.get("globalClearing");
    for (key, wanted) in [
        ("tallyStaticInitialized", "tallyInitialized"),
        ("tallyMod", "tallyMod"),
        ("contractModRaw", "contractMod"),
        ("unhandledRejections", "unhandled"),
    ] {
        assert_eq!(global.get(key), expected.get(wanted), "{name} {key}");
    }
    assert_eq!(
        runtime.tally_cache(),
        expected.get("tally"),
        "{name} sole Tally"
    );
    assert_eq!(
        global.get("channels").get("registry"),
        expected.get("channels"),
        "{name} actual Channels"
    );
    assert_eq!(
        actual.get("oracleSourceCache"),
        expected.get("oracleCache"),
        "{name} Oracle"
    );
    assert_eq!(
        actual.get("oracleSourceInitialized"),
        expected.get("oracleInitialized"),
        "{name} Oracle flag"
    );
    assert_eq!(
        runtime.margins().state_value().unwrap().get("instances"),
        V::Array(vec![]),
        "{name} no fabricated Margin receiver"
    );
    assert_eq!(
        runtime
            .last_contract_parameters()
            .cloned()
            .unwrap_or(V::Undefined),
        expected.get("lastParams"),
        "{name} raw parameters"
    );
    assert_eq!(
        actual.get("globals").get("tallyMap"),
        if expected.get("bound") == V::Bool(true) {
            obj(&[("sourceTally", V::Bool(true))])
        } else {
            V::Undefined
        },
        "{name} actual global binding"
    );
}
fn apply(runtime: &mut SourceContractRuntime, case: &V) -> V {
    let command = case.get("command");
    if let V::Array(args) = command.get("raw") {
        let outcome = match runtime.execute_contract_unverified(&args).unwrap() {
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
        obj(&[("outcome", outcome)])
    } else {
        returned(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: text(command.get("target")),
                    arguments: array(command.get("args")),
                })
                .unwrap(),
        )
    }
}
fn effects(values: Vec<V>) -> V {
    V::Array(values.into_iter().filter_map(|v|{if matches!(v.get("kind"),V::String(k)if["database","db","uuid","random","rpc-clock"].contains(&k.as_str())){Some(v)}else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if["Logic.payToTokens","Logic.tradeBaiUrbun","Logic.tradeMurabaha"].contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}else{None}}).collect())
}
#[test]
fn unchanged_distribution_graph_both_genuine_numeric_versions() {
    let archived = cases("9.3.1");
    assert_eq!(archived, cases("9.1.2"));
    let mut runtime = SourceContractRuntime::with_policy(P::DistributionLifecycleV5);
    let mut calls = 0;
    let mut node_only = 0;
    for case in &archived {
        let command = case.get("command");
        let name = text(case.get("name"));
        if command.get("nodeOnly") == V::Bool(true) {
            node_only += 1;
            assert_eq!(case.get("instanceAddresses"), V::Array(vec![]));
            assert_eq!(case.get("staticAddressCount"), n(2.));
            assert_eq!(array(case.get("factoryEffects")).len(), 2);
            assert_eq!(
                case.get("result")
                    .get("outcome")
                    .get("thrown")
                    .get("message"),
                s(if name.ends_with('2') {
                    "Insufficient tokens for distribution"
                } else {
                    "No holders for target token"
                })
            );
            continue;
        }
        if let V::Object(_) = command.get("reset") {
            runtime = SourceContractRuntime::with_policy(P::DistributionLifecycleV5);
            runtime
                .set_entry(&text(command.get("reset").get("entry")))
                .unwrap();
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
        } else {
            assert_eq!(
                apply(&mut runtime, case),
                case.get("result"),
                "{name} exact outcome and sync/async transport"
            );
            calls += 1;
        }
        assert_state(&runtime, case);
        assert_eq!(
            effects(runtime.take_effects()),
            case.get("effects"),
            "{name} exact ordered source/DB/dependency effects"
        );
    }
    assert_eq!((calls, node_only, archived.len()), (140, 2, 146));
}
#[test]
fn legacy_refusals_and_literal_failure_journal_restore_then_continue() {
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        for method in ["payToTokens", "tradeBaiUrbun", "tradeMurabaha"] {
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: format!("Logic.{method}"),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
        for kind in [26., 28., 29.] {
            assert_eq!(
                runtime.execute_contract_unverified(&[
                    s("legacy"),
                    n(kind),
                    s("tl"),
                    s("1,7,2"),
                    s("A"),
                    V::Null,
                    n(0.),
                    n(0.),
                    n(10.)
                ]),
                Err(SourceRuleError::InvalidInput)
            );
            assert_eq!(runtime, before);
        }
    }
    assert_eq!(SourceContractRuntime::new().policy(), P::GlobalClearingV2);
    let archived = cases("9.3.1");
    let mut replay = SourceContractReplay::with_policy(P::DistributionLifecycleV5).unwrap();
    for case in archived
        .iter()
        .take_while(|v| v.get("name") != s("entry-types-first"))
    {
        let command = case.get("command");
        if let V::Object(_) = command.get("reset") {
            for table in array(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
        } else {
            let command = if let V::Array(args) = command.get("raw") {
                C::ExecuteContract(args)
            } else {
                C::Invoke(DependencyCall {
                    target: text(command.get("target")),
                    arguments: array(command.get("args")),
                })
            };
            replay.apply(command).unwrap();
        }
        assert_state(replay.runtime(), case);
        let checkpoint = replay.checkpoint().unwrap();
        assert_eq!(&checkpoint[..5], b"TLCR\x05");
        let restored = SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), checkpoint);
        replay = restored;
    }
}

fn repair_crc(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.len() - 4);
    let mut crc = !0u32;
    for byte in bytes.iter() {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
}

#[test]
fn independently_compiled_populated_v4_fixture_and_v4_v5_domains_stay_distinct() {
    // Captured by Rust1.90 MSVC from c025384c before the V5 mount existed.
    // The external expected head is independent of the admitted bytes.
    let bytes = include_bytes!("../fixtures/source_contract_replay_v4.bin");
    assert_eq!(bytes.len(), 14_588);
    let hex = "a294259075ae5cb069be1f549fae755f02e0d6d166321ca85084e6a7454dd5ee";
    let mut commitment = [0; 32];
    for (index, byte) in commitment.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * index..2 * index + 2], 16).unwrap();
    }
    let pin = SourceContractReplayHead {
        commands: 14,
        commitment,
    };
    let mut old = SourceContractReplay::from_checkpoint(bytes, pin).unwrap();
    assert_eq!(old.checkpoint().unwrap(), bytes);
    assert_eq!(old.runtime().policy(), P::OptionLifecycleV4);
    let handle = obj(&[("sourceMarginInstance", n(1.))]);
    let receipts: Vec<_> = old.receipts().collect();
    assert_eq!(
        receipts
            .iter()
            .map(|v| v.outcome.clone())
            .collect::<Vec<_>>(),
        vec![
            O::Void,
            O::Dependency(DependencyReturn::Pending(s("s-7-1"))),
            O::Dependency(DependencyReturn::Pending(V::Undefined)),
            O::Dependency(DependencyReturn::Pending(handle.clone())),
            O::Dependency(DependencyReturn::Pending(obj(&[
                ("contracts", n(4.)),
                ("avgPrice", n(10.)),
                ("margin", n(5.)),
                ("closedQty", n(0.)),
                ("flipQty", n(0.))
            ]))),
            O::Dependency(DependencyReturn::Immediate(V::Undefined)),
            O::Dependency(DependencyReturn::Pending(V::Undefined)),
            O::Dependency(DependencyReturn::Pending(V::Array(vec![s("1-100-C-100")]))),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::UnknownDependency),
            O::LocalError(SourceRuleError::InvalidInput),
            O::LocalError(SourceRuleError::InvalidInput),
            O::LocalError(SourceRuleError::InvalidInput),
        ]
    );
    assert_eq!(
        receipts.iter().map(|v| v.effects.len()).collect::<Vec<_>>(),
        vec![0, 7, 3, 3, 8, 1, 3, 1, 0, 0, 0, 0, 0, 0]
    );
    let state = old.runtime().state_value().unwrap();
    assert_eq!(state.get("runtimePolicy"), s("OptionLifecycleV4"));
    assert_eq!(
        array(state.get("syntheticLifecycle").get("vaults")).len(),
        1
    );
    assert_eq!(
        state.get("syntheticLifecycle").get("syntheticTokens"),
        V::Array(vec![V::Array(vec![
            n(9.),
            obj(&[("contract", n(1.)), ("property", n(7.))])
        ])])
    );
    assert_eq!(old.runtime().collection("marginMaps").len(), 1);
    assert_eq!(old.runtime().collection("marginMapDelta").len(), 1);
    let position = old
        .runtime()
        .margins()
        .inspect_position(1, &s("alice"))
        .unwrap();
    assert_eq!(
        position.get("options").get("1-100-C-100").get("contracts"),
        n(4.)
    );
    assert_eq!(
        position.get("options").get("1-100-C-100").get("avgPrice"),
        n(10.)
    );
    assert_eq!(
        position.get("options").get("1-100-C-100").get("margin"),
        n(5.)
    );
    let instance = old.runtime().margins().instance_value(1).unwrap();
    assert_eq!(
        instance.get("expiryIndex"),
        V::Array(vec![V::Array(vec![
            n(100.),
            V::Array(vec![s("1-100-C-100")])
        ])])
    );
    assert_eq!(
        instance.get("tickerExpiry"),
        V::Array(vec![V::Array(vec![s("1-100-C-100"), n(100.)])])
    );
    assert_eq!(
        instance.get("optionOI"),
        // The source increments once for the zero -> nonzero transition.
        V::Array(vec![V::Array(vec![s("1-100-C-100"), n(1.)])])
    );
    for target in [
        "Logic.payToTokens",
        "Logic.tradeBaiUrbun",
        "Logic.tradeMurabaha",
    ] {
        let before = old.runtime().clone();
        let receipt = old
            .apply(C::Invoke(DependencyCall {
                target: target.into(),
                arguments: vec![],
            }))
            .unwrap();
        assert_eq!(
            receipt.outcome,
            O::LocalError(SourceRuleError::UnknownDependency)
        );
        assert!(receipt.effects.is_empty());
        assert_eq!(old.runtime(), &before);
    }
    let mut resumed =
        SourceContractReplay::from_checkpoint(&old.checkpoint().unwrap(), old.head()).unwrap();
    let continuation = C::Invoke(DependencyCall {
        target: "Margin.getExpiringTickersUpTo".into(),
        arguments: vec![handle, n(100.)],
    });
    assert_eq!(
        old.apply(continuation.clone()).unwrap(),
        resumed.apply(continuation).unwrap()
    );
    assert_eq!(old, resumed);

    let mut fifth = SourceContractReplay::with_policy(P::DistributionLifecycleV5).unwrap();
    fifth
        .apply(C::Invoke(DependencyCall {
            target: "Logic.payToTokens".into(),
            arguments: vec![],
        }))
        .unwrap();
    assert_eq!(
        fifth.runtime().state_value().unwrap().get("runtimePolicy"),
        s("DistributionLifecycleV5")
    );
    for (original, source, target, version, head) in [
        (
            bytes.to_vec(),
            P::OptionLifecycleV4,
            P::DistributionLifecycleV5,
            5,
            pin,
        ),
        (
            fifth.checkpoint().unwrap(),
            P::DistributionLifecycleV5,
            P::OptionLifecycleV4,
            4,
            fifth.head(),
        ),
    ] {
        let mut edited = original.clone();
        edited[4] = version;
        repair_crc(&mut edited);
        assert!(SourceContractReplay::from_checkpoint(&edited, head).is_err());
        let destination = SourceContractReplay::with_policy(target).unwrap();
        edited[5..37].copy_from_slice(&destination.checkpoint().unwrap()[5..37]);
        repair_crc(&mut edited);
        assert!(SourceContractReplay::from_checkpoint(&edited, head).is_err());
        assert!(SourceContractReplay::from_checkpoint(&edited, destination.head()).is_err());
        assert_eq!(
            SourceContractReplay::from_checkpoint(&original, head)
                .unwrap()
                .runtime()
                .policy(),
            source
        );
    }
}
