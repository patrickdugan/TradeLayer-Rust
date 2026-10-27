//! Observe the real call before awaiting it: transport is source behavior too.
use tradelayer_core::margin_map_source_owner::MarginMapSourceOwner;
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceRuleError, SourceValue as V,
};

fn s(value: &str) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(value: V) -> String {
    let V::String(value) = value else {
        panic!("string expected")
    };
    value
}
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array expected")
    };
    value
}
fn returned(value: DependencyReturn) -> V {
    let (transport, value) = match value {
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
    obj(&[("transport", s(transport)), ("outcome", value)])
}
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');
code=code.replace('return {types,logic,Orderbook,effects','return {tables,types,logic,Orderbook,effects');
code+='\nmodule.exports={environment,tag,BN};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const {environment,tag,BN}=driver.exports;
const norm=v=>v[0]==='map'?['array',v[1].map(([k,v])=>['array',[norm(k),norm(v)]])]:v[0]==='array'?['array',v[1].map(norm)]:v[0]==='object'?['object',v[1].map(([k,v])=>[k,norm(v)])]:v;
function emit(value){console.log(JSON.stringify(norm(tag(value))));}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
(async()=>{
 const e=environment({}),c=e.load('clearing'),m=await e.load('marginMap').getInstance(1);await flush();
 emit({reset:true,tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)}))});
 async function call(target,args,mode,failure){let transport,outcome;const receiver=target.startsWith('Margin.')?m:target.startsWith('Contract.')?e.load('contractRegistry'):c,method=target.split('.')[1];
  if(failure)e.setFailure(failure);
  try{let value=receiver[method](...args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else{transport='Immediate';outcome={returned:value};}}
  catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}
  await flush();emit({target,args,mode,failure,expected:{transport,outcome},contractModRaw:e.load('contractRegistry').modFlag,effectiveContractMod:!!e.load('contractRegistry').modFlag});
 }
 await call('Margin.calculateLiquidationPrice',[1,10,1,1,false,true,100,0]);
 await call('Margin.calculateMarginRequirement',[1,100,false]);
 await call('Margin.estimatePNL',[1,100,102,false,1]);
 await call('Margin.generateLiquidationOrder',[{address:'actor',contracts:1,margin:10},1,1,10,100,new BN(90)]);
 await call('Margin.generateLiquidationOrder',[{address:'actor',contracts:1,margin:10},1,1,10,100,90]);
 await call('Margin.calcPriceDelta',[1,100,102,false,1,true]);
 await call('Margin.needsLiquidation',[{id:1,marketPrice:100}]);
 await call('Margin.computeMaintenanceMarginRequirement',[1,null]);
 await call('Clearing.clampFundingRate',[100]);
 await call('Clearing.calculateClearingPNL',[{oldContracts:1,previousMarkPrice:100,currentMarkPrice:102,inverse:false,notional:1}]);
 await call('Clearing.calculateNewContractPNL',[{newContracts:1,avgEntryPrice:100,lastPrice:102,inverse:false,notional:1}]);
 await call('Clearing.computeLossCoverage',[4,10,{premiumMTM:2,maintNaked:1}]);
 await call('Clearing.recordTrade',[1,'actor',1,0,100,'txid',true]);
 await call('Clearing.recordDeleverageTrade',[1,'actor',{matchSize:1}]);
 await call('Clearing.recordLiquidation',[1,'actor',{contracts:1}]);
 await call('Clearing.getLiquidation',[1,'actor']);
 await call('Clearing.getDeleverageTradesThisBlock',[1,'actor']);
 await call('Clearing.resetBlockTracking',[]);
 await call('Clearing.getLiquidation',[1,'actor']);
 await call('Clearing.getDeleverageTradesThisBlock',[1,'actor']);
 await call('Clearing.recordTrade',[1,'actor',1,0,100,'txid2',true]);
 await call('Clearing.recordDeleverageTrade',[1,'actor',{matchSize:2}]);
 await call('Clearing.recordLiquidation',[1,'actor',{contracts:2}]);
 await call('Clearing.resetBlockTrades',[]);
 await call('Clearing.getLiquidation',[1,'actor']);
 await call('Clearing.getDeleverageTradesThisBlock',[1,'actor']);
 await call('Clearing.initPositionCache',[1,10,[{address:'actor',contracts:1,avgPrice:100,margin:10}]]);
 await call('Clearing.getPositionsFromCache',['1:10']);
 await call('Clearing.addOrUpdatePositionInCache',['1:10','other',{address:'other',contracts:-1,avgPrice:100,margin:10}]);
 // Same live source position; the Rust adapter names its actual arena object.
 await call('Clearing.applyAssignedUnderlyingPosition',[c.getPositionsFromCache('1:10')[0],1,102,2],'position0');
 await call('Clearing.getPositionsFromCache',['1:10']);
 await call('Clearing.flushPositionCache',['1:10']);
 await call('Clearing.getPositionsFromCache',['1:10']);
 await call('Clearing.resolveOptionExerciseTarget',[1,{contracts:{unexpired:[{id:2,expirationBlock:20}]}},15]);
 await call('Clearing.computeExerciseContracts',[{type:'Call'},2]);
 await call('Clearing.computeLiquidationPriceFromLoss',[100,10,1,1,false]);
 await call('Clearing.getBalance',['actor']);
 await call('Clearing.saveClearingSettlementEvent',[1,{},10]);
 for(const flag of [undefined,null,false,0,1,[],{},'armed'])await call('Contract.setModFlag',[flag]);
 e.table('contractList').splice(0);emit({clearContracts:true});
 // The first find loads makeSettlement's empty Map; the second rejects the
 // net check. The exact flag survives because its reset follows that await.
 await call('Clearing.clearingFunction',[10],undefined,{collection:'contractList',op:'find',skip:1,message:'net-contracts'});
})().catch(error=>{console.error(error);process.exitCode=1});
"###;

fn reference() -> Vec<V> {
    let output = std::process::Command::new("node")
        .args(["-e", REFERENCE, env!("CARGO_MANIFEST_DIR")])
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
fn value(result: DependencyReturn) -> V {
    match result {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v,
        other => panic!("unexpected initialization: {other:?}"),
    }
}
struct NoDependencies;
impl SourceBackend for NoDependencies {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        panic!("pure source helper unexpectedly invoked a dependency")
    }
}

#[test]
fn genuine_sync_returns_sync_throws_and_async_rejections_keep_their_transport() {
    let mut runtime = SourceContractRuntime::new();
    let mut owner = MarginMapSourceOwner::new();
    let owner_instance = owner.create_instance(V::number(1.0)).unwrap();
    let mut margin_handle = V::Undefined;
    let mut count = 0;
    for case in reference() {
        if case.get("reset") == V::Bool(true) {
            for table in array(case.get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
            margin_handle = value(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: "Margin.getInstance".into(),
                        arguments: vec![V::number(1.0)],
                    })
                    .unwrap(),
            );
            continue;
        }
        if case.get("clearContracts") == V::Bool(true) {
            runtime.import_collection("contractList", vec![]).unwrap();
            continue;
        }
        let target = text(case.get("target"));
        let original = array(case.get("args"));
        let mut arguments = original.clone();
        if target.starts_with("Margin.") {
            arguments.insert(0, margin_handle.clone());
            if case.get("expected").get("transport") != s("Pending") {
                let result = owner
                    .invoke_source_instance(
                        Some(owner_instance),
                        target.strip_prefix("Margin.").unwrap(),
                        &original,
                        &[],
                        &mut NoDependencies,
                    )
                    .unwrap();
                assert_eq!(
                    returned(result),
                    case.get("expected"),
                    "direct owner {target}"
                );
            }
        }
        if case.get("mode") == s("position0") {
            arguments[0] = obj(&[("sourcePositionObject", V::number(0.0))]);
        }
        let failure = case.get("failure");
        if matches!(failure, V::Object(_)) {
            runtime
                .fail_storage(StorageFailure {
                    collection: text(failure.get("collection")),
                    operation: format!("{}Async", text(failure.get("op"))),
                    skip: 1,
                    message: text(failure.get("message")),
                })
                .unwrap();
        }
        let result = runtime
            .invoke_dependency(&DependencyCall {
                target: target.clone(),
                arguments,
            })
            .unwrap();
        assert_eq!(
            returned(result),
            case.get("expected"),
            "runtime {target}, observation {count}"
        );
        let state = runtime.state_value().unwrap();
        assert_eq!(
            state.get("globalClearing").get("contractModRaw"),
            case.get("contractModRaw"),
            "raw Contract flag after {target}"
        );
        assert_eq!(
            state.get("continuation").get("contractMod"),
            case.get("effectiveContractMod"),
            "effective Contract flag after {target}"
        );
        count += 1;
    }
    assert_eq!(count, 47);
}

#[test]
fn frozen_v1_keeps_original_pending_transport_and_rejects_new_helpers() {
    let mut runtime = SourceContractRuntime::with_policy(SourceContractRuntimePolicy::OnchainV1);
    let handle = value(
        runtime
            .invoke_dependency(&DependencyCall {
                target: "Margin.getInstance".into(),
                arguments: vec![V::number(1.0)],
            })
            .unwrap(),
    );
    let calls = [
        (
            "Margin.calculateLiquidationPrice",
            vec![
                V::number(1.0),
                V::number(10.0),
                V::number(1.0),
                V::number(1.0),
                V::Bool(false),
                V::Bool(true),
                V::number(100.0),
                V::number(0.0),
            ],
        ),
        (
            "Margin.calculateMarginRequirement",
            vec![V::number(1.0), V::number(100.0), V::Bool(false)],
        ),
        (
            "Margin.estimatePNL",
            vec![
                V::number(1.0),
                V::number(100.0),
                V::number(102.0),
                V::Bool(false),
                V::number(1.0),
            ],
        ),
    ];
    for (target, mut arguments) in calls {
        arguments.insert(0, handle.clone());
        assert!(
            matches!(
                runtime
                    .invoke_dependency(&DependencyCall {
                        target: target.into(),
                        arguments
                    })
                    .unwrap(),
                DependencyReturn::Pending(_)
            ),
            "{target}"
        );
    }
    assert!(matches!(
        runtime
            .invoke_dependency(&DependencyCall {
                target: "Clearing.recordTrade".into(),
                arguments: vec![
                    V::number(1.0),
                    s("actor"),
                    V::number(1.0),
                    V::number(0.0),
                    V::number(100.0),
                    s("tx"),
                    V::Bool(true)
                ],
            })
            .unwrap(),
        DependencyReturn::Pending(_)
    ));
    for target in [
        "Clearing.getLiquidation",
        "Clearing.getDeleverageTradesThisBlock",
        "Clearing.resetBlockTracking",
        "Clearing.resetBlockTrades",
        "Margin.generateLiquidationOrder",
    ] {
        let before = runtime.clone();
        assert_eq!(
            runtime.invoke_dependency(&DependencyCall {
                target: target.into(),
                arguments: vec![]
            }),
            Err(SourceRuleError::UnknownDependency),
            "{target}"
        );
        assert_eq!(runtime, before, "local admission failure changed {target}");
    }
}
