//! Unchanged Logic/Validity/Margin/Property/Vault graph over shared source DB.
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
'use strict';
// This scratch driver loads complete pinned modules. Only the declared
// database, clock/UUID and external dependency boundaries are supplied.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
code=code.replace('const pins = {',"const pins = {vaults:['legacy_vaults_source.txt','3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983'],");
code=code.replace("'txIndex','scaling','vaults'","'txIndex','scaling'");
// Native console methods return undefined. The older recording adapter's
// Array.push return is not a financial/source result and must not leak.
code=code.replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})", "(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');
code+='\nmodule.exports={environment,tag};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const{environment,tag}=driver.exports;
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT';
let e,unhandled=[],instances=[],handles=[];
process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const P=e.load('property'),V=e.load('vaults'),T=e.load('tally'),objects=[];const marginState={instances:instances.map(m=>({seriesId:m.seriesId,margins:[...m.margins].map(([k,v])=>{let i=objects.indexOf(v);if(i<0){i=objects.length;objects.push(v);}return[k,i];})})),objects};return{marginState,tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),property:P.instance?.propertyIndex??new Map(),vaults:V.vaults,syntheticTokens:V.syntheticTokens,tally:T.addresses??new Map(),margins:instances.map(m=>({seriesId:m.seriesId,margins:m.margins})),unhandled};}
function emit(name,command,result){console.log(JSON.stringify(norm(tag({name,command,result,state:state(),effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock'].includes(v.kind))}))));}
async function reset(name,options={}){
 e=environment({});unhandled=[];instances=[];handles=[];
 const M=e.load('marginMap'),original=M.loadMarginMap;
 // Observe original resolved objects without supplying or replacing values.
 M.loadMarginMap=function(...args){const p=original.apply(this,args);p.then(value=>{if(value instanceof M&&!instances.includes(value))instances.push(value);},()=>{});return p;};
 e.load('vaults');
 Object.assign(e.table('contractList')[0].data,{native:options.native??true,inverse:options.inverse??true,onChainData:[[7,7]],notionalValue:options.notional??1});
 e.table('volumeIndex').push({_id:'7-7',value:{price:100,blockHeight:10}});
 const position={contracts:options.contracts??-10,margin:options.margin??1,unrealizedPNL:0,address:A,avgPrice:100};
 e.table('marginMaps').splice(0,Infinity,...(options.noPosition?[]:[{_id:JSON.stringify({seriesId:1}),value:JSON.stringify([[A,position]])}]));
 const tally=JSON.parse(e.table('tallyMap')[0].data);tally[0][1][7].margin=options.margin??1;tally[0][1][7].available=options.funds??1000;
 e.table('tallyMap')[0].data=JSON.stringify(tally);
 if(options.oracleRows)e.table('oracleData').splice(0,Infinity,...structuredClone(options.oracleRows));
 if(options.persistedVault)e.table('vaults').push({_id:'s-7-1',value:JSON.stringify({contracts:2,margin:.2,available:0,outstanding:2})});
 await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});
}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function invoke(name,target,args,failure){e.effects.length=0;if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}let transport,outcome;try{let [ns,method]=target.split('.'),receiver=ns==='Logic'?e.logic:ns==='Vaults'?e.load('vaults'):ns==='Property'?e.load('property'):ns==='Contract'?e.load('contractRegistry'):ns==='Margin'?e.load('marginMap'):null;let values=args;
 if(ns==='Property'&&method==='addProperty')receiver=receiver.getInstance();
 if(ns==='Margin'&&typeof args[0]==='object'&&args[0]?.sourceMarginInstance){receiver=handles[args[0].sourceMarginInstance-1];values=args.slice(1);}
 let value=receiver[method](...values);if(value&&typeof value.then==='function'){transport='Pending';try{value=await value;outcome={returned:value}}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}}}}else{transport='Immediate';outcome={returned:value}}
 if(ns==='Margin'&&['getInstance','loadMarginMap'].includes(method)&&outcome?.returned){handles.push(outcome.returned);outcome.returned={sourceMarginInstance:instances.indexOf(outcome.returned)+1};}
 if(ns==='Property'&&method==='getInstance')outcome.returned={sourcePropertyInstance:true};
 }catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{target,args,failure},{transport,outcome});}
async function raw(name,type,property=7,amount=2){e.effects.length=0;const params={propertyIdUsed:property,contractIdUsed:1,amount:amount*1e8};const encoded=e.load('txEncoder')[type===24?'encodeMintSynthetic':'encodeRedeemSynthetic'](params),args=[name,type,'tl',encoded.slice(3),A,null,0,0,10];let decoded,outcome;try{decoded=await e.types.decodePayload(...args);let returned;if(decoded.valid===true)returned=await e.logic.typeSwitch(type,decoded);outcome={params:decoded,returned}}catch(error){outcome={params:decoded,thrown:{name:error.name,message:error.message}}}await flush();emit(name,{raw:args},{outcome});}
async function mint(name='mint'){await raw(name,24);}
(async()=>{
await reset('cold-mint');await mint('cold-first-mint');await mint('cold-second-mint');
await reset('warm-mint');await invoke('warm-property-index','Property.getPropertyIndex',[]);await mint('warm-first-mint');await mint('warm-second-mint-prefix');await mint('warm-second-mint-retry');
await reset('redeem-native');await invoke('redeem-warm-property','Property.getPropertyIndex',[]);await mint('redeem-seed-mint');await raw('native-redeem-reference-error',25,7,1);await raw('native-redeem-reference-error-retry',25,7,1);
await reset('linear-invalid',{inverse:false});await mint('raw-linear-invalid');
await reset('oracle-invalid',{native:false});await mint('raw-oracle-invalid');
await reset('mint-no-collateral',{margin:0,funds:0});await mint('raw-mint-initMargin-reference-error');
await reset('direct-no-position',{noPosition:true});await invoke('metadata-prefix-before-no-position-error','Logic.mintSynthetic',[A,7,1,2,10,0,2,.2]);
await reset('vault-direct');await invoke('exists-empty-is-true','Vaults.exists',['missing']);await invoke('create-omits-save-value','Vaults.createVault',[7,1]);await invoke('register-omits-vaultId','Vaults.registerSyntheticToken',['s-7-1',1,7]);await invoke('lookup-missing-vaultId','Vaults.getVaultId',['s-7-1']);await invoke('first-vault-update','Vaults.updateVault',['s-7-1',{contracts:2,margin:.2},2,.3]);await invoke('vault-string-plus','Vaults.updateVault',['s-7-1',{contracts:'3',margin:'4'},'5','6']);await invoke('vault-redeem-retains-plus','Vaults.updateVaultRedeem',['s-7-1',{contracts:-1,margin:1,available:1},-2]);await invoke('vault-total-balance','Vaults.getTotalBalanceForProperty',[7]);await invoke('vault-total-outstanding-id','Vaults.getTotalOutstandingForProperty',['s-7-1']);await invoke('vault-total-string-pid-is-distinct','Vaults.getTotalOutstandingForProperty',['7']);
await reset('vault-load-failure');await invoke('failed-factory-retains-initialized-maps','Vaults.getVault',['s-7-1'],{collection:'vaults',op:'getDatabase',message:'vault-load'});await invoke('retry-skips-loader','Vaults.getVault',['s-7-1']);
await reset('vault-unawaited-failure');await invoke('synthetic-find-failure-is-unhandled','Vaults.getVault',['s-7-1'],{collection:'syntheticTokens',op:'findOne',message:'unawaited-load'});await invoke('exists-find-failure-still-true','Vaults.exists',['s-7-1'],{collection:'syntheticTokens',op:'findOne',message:'unawaited-exists'});
await reset('vault-restart',{persistedVault:true});await invoke('restart-ignores-persisted-object','Vaults.getVault',['s-7-1']);await invoke('redeem-log-before-vault-guard','Logic.redeemSynthetic',[A,'s-7-1',1,1,10]);
await reset('property-update');await invoke('property-total-promise-update-error','Property.updateTotalInCirculation',[7,-2]);await invoke('property-total-retry-same-error','Property.updateTotalInCirculation',[7,-2]);
await invoke('property-total-unawaited-factory-failure','Property.updateTotalInCirculation',[7,-2],{collection:'propertyList',op:'getDatabase',skip:2,message:'property-unawaited-factory'});await invoke('property-total-after-factory-retry','Property.updateTotalInCirculation',[7,-2]);
await reset('direct-margin-alias');await invoke('load-exact-margin-handle','Margin.getInstance',[1]);await invoke('direct-margin-first-mint','Margin.moveMarginAndContractsForMint',[{sourceMarginInstance:1},A,7,1,2,.2,10]);await invoke('direct-margin-second-mint-alias-prefix','Margin.moveMarginAndContractsForMint',[{sourceMarginInstance:1},A,7,1,2,.2,10]);
await reset('oracle-redeem',{native:false});await invoke('oracle-redeem-vault','Vaults.updateVault',['s-7-1',{contracts:2,margin:.2},2,0]);await invoke('oracle-redeem-promise-price','Logic.redeemSynthetic',[A,'s-7-1',1,1,10]);await invoke('oracle-redeem-unawaited-find-failure','Logic.redeemSynthetic',[A,'s-7-1',1,1,10],{collection:'oracleData',op:'find',message:'oracle-unawaited'});await invoke('oracle-redeem-unawaited-factory-failure','Logic.redeemSynthetic',[A,'s-7-1',1,1,10],{collection:'oracleData',op:'getDatabase',message:'oracle-factory'});
await invoke('oracle-initial-margin-factory-failure','Logic.redeemSynthetic',[A,'s-7-1',1,1,10],{collection:'contractList',op:'getDatabase',skip:1,message:'oracle-margin-factory'});await invoke('oracle-initial-margin-read-failure','Logic.redeemSynthetic',[A,'s-7-1',1,1,10],{collection:'contractList',op:'findOne',skip:1,message:'oracle-margin-read'});
await reset('late-vault-save');await invoke('late-save-prefix','Vaults.updateVault',['s-7-1',{contracts:2,margin:.2},2,.3],{collection:'vaults',op:'update',message:'vault-save'});await invoke('late-save-retry-adds-again','Vaults.updateVault',['s-7-1',{contracts:2,margin:.2},2,.3]);
await reset('initial-margin');for(const [name,price]of [['positive',3],['negative',-3],['zero',0],['nan','NaN'],['hex','0x64']])await invoke('initial-margin-'+name,'Contract.getInitialMargin',[1,price]);
for(const [label,rows]of [['height-primitive-error',[{_id:'height-1',oracleId:1,blockHeight:{valueOf:null,toString:null},data:{price:100}},{_id:'height-2',oracleId:1,blockHeight:{valueOf:null,toString:null},data:{price:101}}]],['filtered-bad-price',[{_id:'bad-1',oracleId:1,blockHeight:1,data:{price:'NaN'}},{_id:'bad-2',oracleId:1,blockHeight:2,data:null}]],['price-primitive-error',[{_id:'price-1',oracleId:1,blockHeight:1,data:{price:{valueOf:null,toString:null}}}]]]){await reset('oracle-filter-'+label,{native:false,oracleRows:rows});await invoke('oracle-filter-seed-'+label,'Vaults.updateVault',['s-7-1',{contracts:2,margin:.2},2,0]);await invoke('oracle-filter-redeem-'+label,'Logic.redeemSynthetic',[A,'s-7-1',1,1,10]);await invoke('oracle-filter-retry-'+label,'Logic.redeemSynthetic',[A,'s-7-1',1,1,10]);}
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
            compare(
                &table.get("rows"),
                &V::Array(vec![]),
                &format!("{name} unexpected {collection}"),
            );
        }
    }
    let global = state.get("globalClearing");
    compare(
        &global.get("propertyCache"),
        &expected.get("property"),
        &format!("{name} property singleton Map"),
    );
    compare(
        &state.get("syntheticLifecycle").get("vaults"),
        &expected.get("vaults"),
        &format!("{name} live vault Map"),
    );
    compare(
        &state.get("syntheticLifecycle").get("syntheticTokens"),
        &expected.get("syntheticTokens"),
        &format!("{name} live synthetic token Map"),
    );
    compare(
        &runtime.tally_cache(),
        &expected.get("tally"),
        &format!("{name} sole Tally cache"),
    );
    compare(
        &state.get("margins"),
        &expected.get("marginState"),
        &format!("{name} complete Margin instance/object arena"),
    );
    compare(
        &global.get("unhandledRejections"),
        &expected.get("unhandled"),
        &format!("{name} independent unawaited rejections"),
    );
}
fn apply(runtime: &mut SourceContractRuntime, case: &V) {
    let command = case.get("command");
    if let V::Object(_) = command.get("failure") {
        runtime
            .fail_storage(failure(command.get("failure")))
            .unwrap();
    }
    let name = text(case.get("name"));
    let result = if let V::Array(args) = command.get("raw") {
        let outcome = runtime
            .execute_contract_unverified(&args)
            .unwrap_or_else(|e| panic!("{name} local {e:?}"));
        let outcome = match outcome {
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
        call_result(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: text(command.get("target")),
                    arguments: array(command.get("args")),
                })
                .unwrap_or_else(|e| panic!("{name} local {e:?}")),
        )
    };
    compare(
        &result,
        &case.get("result"),
        &format!("{name} literal outcome and transport"),
    );
}
fn compare_policy_genuine_synthetic_graph_matches_full_state_aliases_and_ordered_effects(
    policy: tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy,
) {
    let mut runtime = SourceContractRuntime::with_policy(policy);
    let mut resets = 0;
    let mut calls = 0;
    for case in source_cases() {
        if matches!(case.get("command").get("reset"), V::Object(_)) {
            resets += 1;
            runtime = SourceContractRuntime::with_policy(policy);
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
        } else {
            apply(&mut runtime, &case);
            calls += 1;
        }
        assert_state(&runtime, &case);
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{} all ordered effects", text(case.get("name"))),
        );
    }
    assert_eq!((resets, calls), (19, 59));
}

#[test]
fn genuine_synthetic_graph_matches_full_state_aliases_and_ordered_effects() {
    compare_policy_genuine_synthetic_graph_matches_full_state_aliases_and_ordered_effects(
        P::OptionLifecycleV4,
    );
    compare_policy_genuine_synthetic_graph_matches_full_state_aliases_and_ordered_effects(
        tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy::ProofLifecycleV6,
    );
}
fn compare_policy_synthetic_failed_prefix_and_live_aliases_restore_before_retry(
    policy: tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy,
) {
    let cases = source_cases();
    for selected in [
        "warm-mint",
        "redeem-native",
        "vault-load-failure",
        "late-vault-save",
        "direct-margin-alias",
        "oracle-redeem",
        "property-update",
        "oracle-filter-height-primitive-error",
        "oracle-filter-filtered-bad-price",
        "oracle-filter-price-primitive-error",
    ] {
        let start = cases
            .iter()
            .position(|c| c.get("name") == s(selected))
            .unwrap();
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        for table in array(cases[start].get("state").get("tables")) {
            replay
                .apply(C::Import {
                    collection: text(table.get("name")),
                    rows: array(table.get("rows")),
                })
                .unwrap();
        }
        for case in cases[start + 1..]
            .iter()
            .take_while(|c| !matches!(c.get("command").get("reset"), V::Object(_)))
        {
            let cmd = case.get("command");
            if matches!(cmd.get("failure"), V::Object(_)) {
                replay
                    .apply(C::FailStorage(failure(cmd.get("failure"))))
                    .unwrap();
            }
            replay
                .apply(if let V::Array(arguments) = cmd.get("raw") {
                    C::ExecuteContract(arguments)
                } else {
                    C::Invoke(DependencyCall {
                        target: text(cmd.get("target")),
                        arguments: array(cmd.get("args")),
                    })
                })
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
    for policy in [P::OnchainV1, P::GlobalClearingV2, P::SourceLifecycleV3] {
        let mut old = SourceContractRuntime::with_policy(policy);
        let before = old.clone();
        assert_eq!(
            old.invoke_dependency(&DependencyCall {
                target: "Logic.mintSynthetic".into(),
                arguments: vec![]
            }),
            Err(SourceRuleError::UnknownDependency)
        );
        assert!(old == before, "earlier policy admitted new synthetic graph");
    }
    let mut bounded = SourceContractRuntime::with_policy(policy);
    for table in array(cases[0].get("state").get("tables")) {
        bounded
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
    let before = bounded.clone();
    let actor = s("1BoatSLRHtKNngkdXEeobR76b53LETtpyT");
    assert_eq!(
        bounded.invoke_dependency(&DependencyCall {
            target: "Logic.mintSynthetic".into(),
            arguments: vec![
                actor,
                V::number(7.),
                V::number(1.),
                V::number(2.),
                V::number(10.),
                V::number(0.),
                V::String("1".repeat(513)),
                V::number(0.2)
            ]
        }),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert!(
        bounded == before,
        "bounded local refusal retained speculative metadata prefix"
    );
}

#[test]
fn synthetic_failed_prefix_and_live_aliases_restore_before_retry() {
    compare_policy_synthetic_failed_prefix_and_live_aliases_restore_before_retry(
        P::OptionLifecycleV4,
    );
    compare_policy_synthetic_failed_prefix_and_live_aliases_restore_before_retry(
        tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy::ProofLifecycleV6,
    );
}
