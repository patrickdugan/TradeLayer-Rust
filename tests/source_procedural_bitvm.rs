//! Unchanged full Procedural/BitVM modules over the sole shared runtime stores.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P, StorageFailure,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V,
};
const REFERENCE: &str = r###"
'use strict';
// Actual full pinned classes, with storage/clock/environment boundaries only.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),assert=require('node:assert/strict');
const root=process.env.PROOF_REPO||path.resolve(__dirname,'../../outputs/TradeLayer-Rust');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));
if(process.argv.includes('--lock')){const vm=require('node:vm'),crypto=require('node:crypto');const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');numeric.loadBigNumber=()=>{const m={exports:{}};vm.runInNewContext(raw.toString(),{module:m,exports:m.exports},{timeout:1000});return m.exports;};}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(a,b){assert.equal(code.split(a).length,2,a);code=code.replace(a,b);}
const pins={procedural:['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'],bitvmCache:['legacy_bitvm_cache_source.txt','053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca'],bitvmRisk:['legacy_bitvm_risk_source.txt','14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66']};
replace('const sources = Object.fromEntries(',`Object.assign(pins,${JSON.stringify(pins)});\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache','bitvmBundle','bitvmRisk'","'vaults','bitvmBundle'");
replace('return {types,logic,Orderbook,effects','return {context,tables,storage,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
let e,receivers,env={};const records=[];
const targets={Procedural:['procedural','ProceduralRegistry'],BitvmCache:['bitvmCache','BitvmCacheRegistry'],BitvmRisk:['bitvmRisk']};
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(r=>setImmediate(r));}
function state(){return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),environment:{...env},unsupported:e.state().unsupported};}
function emit(name,command,result){assert.deepEqual(e.state().unsupported,[],name);records.push({name,command,result,state:state(),effects:e.effects.filter(v=>['source-function','database','db'].includes(v.kind))});}
async function reset(name,environment={}){e=m.exports.environment({});env={...environment};Object.assign(e.context.process.env,env);receivers={};for(const[label,[module,klass]]of Object.entries(targets)){const target=klass?e.load(module)[klass]:e.load(module);receivers[label]=target;for(const key of Reflect.ownKeys(target)){if(typeof target[key]!=='function'||['length','name','prototype'].includes(key))continue;const original=target[key];target[key]=function(...args){e.effects.push({kind:'source-function',target:label+'.'+String(key),args:structuredClone(args)});return Reflect.apply(original,this,args);};}}await flush();e.effects.length=0;emit(name,{reset:{environment:env}},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip||0;e.storage.getDatabase=async function(name){if(name==='procedural'){if(skip>0)skip--;else{e.storage.getDatabase=original;const er=Error(f.message);e.effects.push({kind:'database',name,method:'getDatabase',args:[...arguments],thrown:{name:er.name,message:er.message}});throw er;}}return original.apply(this,arguments);};}
async function call(name,target,args=[],failure){e.effects.length=0;if(failure)fail(failure);const[label,key]=target.split('.');let transport='Immediate',outcome;try{let v=Reflect.apply(receivers[label][key],receivers[label],args);if(v&&typeof v.then==='function'){transport='Pending';try{v=await v;outcome={returned:v};}catch(er){transport='PendingThrow';outcome={thrown:{name:er.name,message:er.message}};}}else outcome={returned:v};}catch(er){transport='Throw';outcome={thrown:{name:er.name,message:er.message}};}await flush();emit(name,{target,args,failure},{transport,outcome});}
async function configure(name,environment){e.effects.length=0;env={...environment};for(const k of Object.keys(e.context.process.env))delete e.context.process.env[k];Object.assign(e.context.process.env,env);emit(name,{configure:env},{outcome:{returned:undefined}});}
(async()=>{
await reset('procedural');
await call('missing-template','Procedural.getTemplate',['T']);
await call('template','Procedural.upsertTemplate',['T',{templateHash:' AbCd ',extra:1}]);
await call('contract','Procedural.upsertContract',['C','T','OPEN',{marker:true}]);
await call('issuance','Procedural.ensureIssuanceContext',['T','C','open','ABCD']);
await call('transition','Procedural.transitionContract',['C','settled',{state:'wrong',block:10}]);
await call('redemption','Procedural.ensureRedemptionContext',['T','C','settled']);
await call('fund-next','Procedural.transitionContract',['NEXT','FUNDED',{rolledFrom:'C'}]);
await call('read-next','Procedural.getContract',['NEXT']);
await call('normalize','Procedural._normalizeHash',['\ufeffAbC\u00a0']);
await call('unknown-template','Procedural.ensureIssuanceContext',['unknown','C','OPEN','hash']);
await call('missing-ids','Procedural.ensureIssuanceContext',['','C','','hash']);
await call('missing-hash','Procedural.ensureIssuanceContext',['T','C']);
await call('bad-hash','Procedural.ensureIssuanceContext',['T','C','','different']);
await call('not-mintable','Procedural.ensureIssuanceContext',['T','C','','abcd']);
await call('state-mismatch','Procedural.ensureRedemptionContext',['T','C','CLOSED']);
await call('default-contract','Procedural.upsertContract',['default','T']);
await call('metadata-spread-string','Procedural.transitionContract',['spread','open','abc']);
await call('metadata-spread-array','Procedural.upsertTemplate',['array',['a','b']]);
await call('metadata-override','Procedural.upsertTemplate',['override',{_id:'different',type:'other',templateId:9}]);
await call('null-metadata','Procedural.upsertContract',['null','T',null,null]);
await call('null-next-state','Procedural.transitionContract',['null',null]);
await call('state-object-error','Procedural.transitionContract',['bad',{toString:1}]);
await call('missing-redemption','Procedural.ensureRedemptionContext',['T','absent']);
await call('wrong-template','Procedural.ensureRedemptionContext',['other','C']);
await call('not-redeemable','Procedural.ensureRedemptionContext',['T','default']);
await call('unknown-contract','Procedural.ensureIssuanceContext',['T','absent','','abcd']);
await call('issuance-state-mismatch','Procedural.ensureIssuanceContext',['T','default','FUNDED','abcd']);
await call('issuance-template-mismatch','Procedural.ensureIssuanceContext',['T','NEXT','','abcd']);
await call('empty-hash-template','Procedural.upsertTemplate',['empty']);
await call('missing-published-hash','Procedural.ensureIssuanceContext',['empty','C','','hash']);
await call('missing-redemption-ids','Procedural.ensureRedemptionContext',['','']);
await call('normalize-non-js-space','Procedural._normalizeHash',['\u0085AB\u0085']);
await reset('cache');
await call('default-blocks','BitvmCache._challengeBlocks',[]);
await call('null-blocks','BitvmCache._challengeBlocks',[null]);
await call('negative-blocks','BitvmCache._challengeBlocks',[-1]);
await call('nan-blocks','BitvmCache._challengeBlocks',[NaN]);
await call('explicit-id','BitvmCache.buildCacheId',[{cacheId:7}]);
await call('hashed-id','BitvmCache.buildCacheId',[{propertyId:'7',amount:'1',stateHash:'state'}]);
await call('number-object-error','BitvmCache._challengeBlocks',[{toString:1}]);
await call('null-context-prefix','BitvmCache.open',[{propertyId:7,amount:1,fromAddress:'a',toAddress:'b',cacheAddress:'c'},null]);
await call('object-recipient-open','BitvmCache.open',[{propertyId:7,amount:1,cacheId:'object-to',toAddress:{id:'recipient'},challengeBlocks:0},{block:1}]);
await call('object-recipient-finalize','BitvmCache.finalize',['object-to',{block:1}]);
await call('get-missing','BitvmCache.get',['cache']);
await call('null-build-parts','BitvmCache.buildCacheId',[null]);
const open={cacheId:'cache',propertyId:7,amount:3,cacheBondAmount:2,cacheBondPropertyId:8,challengeBlocks:6},context={senderAddress:'alice',dlcRef:'C',stateHash:'state',block:10};
await call('open','BitvmCache.open',[open,context]);
await call('duplicate','BitvmCache.open',[open,context]);
await call('negative-bond-property','BitvmCache.open',[{propertyId:7,amount:1,cacheBondAmount:1,cacheBondPropertyId:-1}]);
await call('negative-challenge-bond','BitvmCache.challenge',['cache',{challengeBondAmount:-1}]);
await call('negative-challenge-property','BitvmCache.challenge',['cache',{challengeBondAmount:1,challengeBondPropertyId:-1}]);
await call('early','BitvmCache.finalize',['cache',{block:15}]);
await call('challenge','BitvmCache.challenge',['cache',{challengerAddress:'bob',evidenceHash:'bad',block:11,challengeBondAmount:1,challengeBondPropertyId:9}]);
await call('challenged-finalize','BitvmCache.finalize',['cache',{block:16}]);
await call('second-challenge-keeps-bond','BitvmCache.challenge',['cache',{challengerAddress:'charlie',block:12,challengeBondAmount:9,challengeBondPropertyId:7}]);
await call('bad-verdict','BitvmCache.resolve',['cache',{verdict:'unknown',block:13}]);
await call('reject','BitvmCache.resolve',['cache',{verdict:'reject',resolverAddress:'admin',reason:'ok',block:13}]);
await call('wrong-recipient','BitvmCache.finalize',['cache',{block:13,toAddress:'wrong'}]);
await call('wrong-amount','BitvmCache.finalize',['cache',{block:13,amount:2}]);
await call('wrong-property','BitvmCache.finalize',['cache',{block:13,propertyId:8}]);
await call('finalize','BitvmCache.finalize',['cache',{block:13,txid:'release'}]);
await call('released-resolve','BitvmCache.resolve',['cache',{verdict:'reject',block:14}]);
await call('released-challenge','BitvmCache.challenge',['cache']);
await call('reopen','BitvmCache.open',[open,{...context,block:20}]);
await call('uphold','BitvmCache.resolve',['cache',{verdict:'UPHOLD',resolverAddress:'admin',block:21}]);
await call('upheld-finalize','BitvmCache.finalize',['cache',{block:30}]);
await call('upheld-reject','BitvmCache.resolve',['cache',{verdict:'reject',block:30}]);
await call('after-reject-finalize','BitvmCache.finalize',['cache',{block:30}]);
for(const [name,target,args]of[['null-open','open',[null]],['bad-pid','open',[{propertyId:0,amount:1}]],['bad-amount','open',[{propertyId:7,amount:NaN}]],['bad-bond','open',[{propertyId:7,amount:1,cacheBondAmount:-1}]],['missing-resolve','resolve',['absent',{}]],['null-detail','challenge',['cache',null]]])await call(name,'BitvmCache.'+target,args);
await configure('nonfinite-env',{TL_BITVM_CHALLENGE_BLOCKS:'garbage'});
await call('fallback-nan','BitvmCache._challengeBlocks',[]);
await call('open-nan-deadline','BitvmCache.open',[{propertyId:7,amount:1,cacheId:'nan'},context]);
await call('finalize-nan-deadline','BitvmCache.finalize',['nan',{block:10}]);
await reset('risk',{TL_BITVM_MAX_PENDING_ESCROW:'10',TL_BITVM_MAX_PENDING_ESCROW_PER_DLC:'8',TL_BITVM_MAX_DEPOSIT_PER_WINDOW:'9',TL_BITVM_MAX_WITHDRAW_PER_WINDOW:'7',TL_BITVM_MAX_SWEEP_PER_WINDOW:'5',TL_BITVM_SCHED_WINDOW_BLOCKS:'10'});
await call('invalid-risk','BitvmRisk.onCacheOpen',[{propertyId:0,amount:1}]);
await call('risk-open','BitvmRisk.onCacheOpen',[{propertyId:7,amount:4,dlcRef:'C',block:10}]);
await call('risk-dlc-cap','BitvmRisk.onCacheOpen',[{propertyId:7,amount:5,dlcRef:'C',block:10}]);
await call('risk-deposit-cap','BitvmRisk.onCacheOpen',[{propertyId:7,amount:6,dlcRef:'other',block:10}]);
await call('risk-global-cap','BitvmRisk.onCacheOpen',[{propertyId:7,amount:7,dlcRef:'other',block:20}]);
await call('risk-sweep','BitvmRisk.onSweep',[{propertyId:7,amount:3,block:10}]);
await call('risk-sweep-cap','BitvmRisk.onSweep',[{propertyId:7,amount:3,block:10}]);
await call('risk-release','BitvmRisk.onEscrowRelease',[{propertyId:7,amount:3,dlcRef:'C',block:10}]);
await call('risk-withdraw-cap','BitvmRisk.onEscrowRelease',[{propertyId:7,amount:5,dlcRef:'C',block:10}]);
await call('risk-release-clamps','BitvmRisk.onEscrowRelease',[{propertyId:7,amount:4,dlcRef:'C',block:10}]);
await call('risk-next-window','BitvmRisk.onCacheOpen',[{propertyId:'7',amount:'2.5',dlcRef:0,block:20}]);
await call('risk-nonfinite-block','BitvmRisk.onCacheOpen',[{propertyId:7,amount:1,block:'bad'}]);
await call('risk-null','BitvmRisk.onSweep',[null]);
await call('risk-undefined','BitvmRisk.onEscrowRelease');
await call('risk-sweep-ignores-dlc','BitvmRisk.onSweep',[{propertyId:7,amount:1,block:20,dlcRef:{toString:1}}]);
for(const [name,target,args,op,skip]of[
 ['transition-read','Procedural.transitionContract',['C','OPEN',{x:1}],'findOne',0],['transition-write','Procedural.transitionContract',['C','OPEN',{x:1}],'update',0],
 ['open-write','BitvmCache.open',[open,context],'update',0],['challenge-write','BitvmCache.challenge',['cache',{block:11,challengerAddress:'bob'}],'update',0],
 ['finalize-write','BitvmCache.finalize',['cache',{block:16}],'update',0],['resolve-write','BitvmCache.resolve',['cache',{verdict:'reject',block:11}],'update',0],
 ['risk-first','BitvmRisk.onCacheOpen',[{propertyId:7,amount:2,dlcRef:'C',block:10}],'update',0],['risk-second','BitvmRisk.onCacheOpen',[{propertyId:7,amount:2,dlcRef:'C',block:10}],'update',1],['risk-third','BitvmRisk.onCacheOpen',[{propertyId:7,amount:2,dlcRef:'C',block:10}],'update',2],
 ['risk-factory','BitvmRisk.onCacheOpen',[{propertyId:7,amount:2,dlcRef:'C',block:10}],'getDatabase',3]
]){await reset('failure-'+name);if(['challenge-write','finalize-write','resolve-write'].includes(name))await call(name+'-setup','BitvmCache.open',[open,context]);await call(name+'-failure',target,args,{collection:'procedural',op,skip,message:name});await call(name+'-retry',target,args);}
for(const record of records)console.log(JSON.stringify(tag(record)));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1;});

"###;
fn cases(lock: bool) -> Vec<V> {
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .args([
            "-e",
            REFERENCE,
            "pb-reference",
            if lock { "--lock" } else { "--baseline" },
        ])
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
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(v: V) -> String {
    let V::String(v) = v else { panic!("string") };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else { panic!("array") };
    v
}
fn outcome(v: DependencyReturn) -> V {
    let (transport, result) = match v {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
        DependencyReturn::Throw(e) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(e.name)), ("message", s(e.message))]),
            )]),
        ),
        DependencyReturn::PendingThrow(e) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(e.name)), ("message", s(e.message))]),
            )]),
        ),
    };
    obj(&[("transport", s(transport)), ("outcome", result)])
}
fn invocation(command: &V) -> DependencyCall {
    if command.get("configure") != V::Undefined {
        return DependencyCall {
            target: "SourceEnvironment.configure".into(),
            arguments: vec![command.get("configure")],
        };
    }
    DependencyCall {
        target: text(command.get("target")),
        arguments: array(command.get("args")),
    }
}
fn failure(command: &V) -> Option<StorageFailure> {
    let f = command.get("failure");
    if f == V::Undefined {
        return None;
    }
    let op = text(f.get("op"));
    Some(StorageFailure {
        collection: text(f.get("collection")),
        operation: if op == "getDatabase" {
            op
        } else {
            format!("{op}Async")
        },
        skip: if let V::Number(bits) = f.get("skip") {
            f64::from_bits(bits) as usize
        } else {
            0
        },
        message: text(f.get("message")),
    })
}
fn effects(values: Vec<V>) -> V {
    V::Array(values.into_iter().filter_map(|v|{
  if v.get("kind")==s("database")||v.get("kind")==s("db"){Some(v)}
  else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if ["Procedural.","BitvmCache.","BitvmRisk."].iter().any(|prefix|t.starts_with(prefix))){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
  else{None}
 }).collect())
}
fn state(runtime: &SourceContractRuntime, case: &V) {
    let expected = case.get("state");
    let name = text(case.get("name"));
    let mut known = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let collection = text(table.get("name"));
        known.insert(collection.clone());
        assert_eq!(
            V::Array(runtime.collection(&collection).to_vec()),
            table.get("rows"),
            "{name} complete {collection} rows"
        );
    }
    let actual = runtime.state_value().unwrap();
    for table in array(actual.get("tables")) {
        let collection = text(table.get("name"));
        if !known.contains(&collection) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} no invented {collection}"
            );
        }
    }
    assert_eq!(
        actual.get("proofLifecycle").get("environment"),
        expected.get("environment"),
        "{name} exact environment"
    );
    assert_eq!(
        runtime.tally_cache(),
        V::Array(vec![]),
        "{name} adapters do not create financial authority"
    );
    assert_eq!(
        runtime.margins().state_value().unwrap().get("instances"),
        V::Array(vec![]),
        "{name} no second Margin owner"
    );
}
fn setup(runtime: &mut SourceContractRuntime, case: &V) {
    for table in array(case.get("state").get("tables")) {
        runtime
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
    runtime
        .invoke_dependency(&DependencyCall {
            target: "SourceEnvironment.configure".into(),
            arguments: vec![case.get("state").get("environment")],
        })
        .unwrap();
    runtime.take_effects();
}
fn actual_full_procedural_and_bitvm_classes_match_both_numeric_versions_for_policy(policy: P) {
    let reference = cases(false);
    assert_eq!(reference, cases(true));
    let mut runtime = SourceContractRuntime::with_policy(policy);
    let (mut resets, mut configured, mut calls) = (0, 0, 0);
    for case in &reference {
        let command = case.get("command");
        let name = text(case.get("name"));
        if command.get("reset") != V::Undefined {
            resets += 1;
            runtime = SourceContractRuntime::with_policy(policy);
            setup(&mut runtime, case);
        } else {
            if let Some(f) = failure(&command) {
                runtime.fail_storage(f).unwrap();
            }
            let returned = runtime.invoke_dependency(&invocation(&command)).unwrap();
            let result = if command.get("configure") != V::Undefined {
                configured += 1;
                assert_eq!(returned, DependencyReturn::Immediate(V::Undefined));
                obj(&[("outcome", obj(&[("returned", V::Undefined)]))])
            } else {
                calls += 1;
                outcome(returned)
            };
            assert_eq!(
                result,
                case.get("result"),
                "{name} exact return/error/transport"
            );
        }
        state(&runtime, case);
        assert_eq!(
            effects(runtime.take_effects()),
            case.get("effects"),
            "{name} full ordered source/DB effects"
        );
    }
    assert_eq!(
        (reference.len(), resets, configured, calls),
        (127, 13, 1, 113)
    );
}
#[test]
fn actual_full_procedural_and_bitvm_classes_match_both_numeric_versions() {
    actual_full_procedural_and_bitvm_classes_match_both_numeric_versions_for_policy(
        P::ProofLifecycleV6,
    );
}

#[test]
fn v7_inherits_actual_full_procedural_and_bitvm_classes_match_both_numeric_versions() {
    actual_full_procedural_and_bitvm_classes_match_both_numeric_versions_for_policy(
        P::LowerLifecycleV7,
    );
}

#[test]
fn source_partial_writes_and_environment_replay_without_atomic_repair() {
    let cases = cases(false);
    let mut replay = SourceContractReplay::with_policy(P::ProofLifecycleV6).unwrap();
    let mut failures = 0;
    for case in &cases {
        let c = case.get("command");
        let name = text(case.get("name"));
        if c.get("reset") != V::Undefined {
            replay = SourceContractReplay::with_policy(P::ProofLifecycleV6).unwrap();
            for table in array(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
            replay
                .apply(C::Invoke(DependencyCall {
                    target: "SourceEnvironment.configure".into(),
                    arguments: vec![case.get("state").get("environment")],
                }))
                .unwrap();
        } else {
            if let Some(f) = failure(&c) {
                failures += 1;
                replay.apply(C::FailStorage(f)).unwrap();
            }
            let receipt = replay.apply(C::Invoke(invocation(&c))).unwrap();
            assert!(
                !matches!(receipt.outcome, O::LocalError(_)),
                "{name} source call admitted, no false local/source substitution"
            );
            assert_eq!(
                effects(receipt.effects),
                case.get("effects"),
                "{name} original ordered effects"
            );
        }
        state(replay.runtime(), case);
        if name.contains("failure")
            || name.contains("retry")
            || name == "risk-open"
            || name == "open-nan-deadline"
        {
            let bytes = replay.checkpoint().unwrap();
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(
                restored, replay,
                "{name} restore exact partial state/failure schedule"
            );
            assert_eq!(restored.checkpoint().unwrap(), bytes);
            replay = restored;
        }
    }
    assert_eq!(failures, 10);
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        for target in [
            "Procedural.transitionContract",
            "BitvmCache.open",
            "BitvmRisk.onCacheOpen",
            "SourceEnvironment.configure",
        ] {
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: target.into(),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
        assert_eq!(
            runtime.state_value().unwrap().get("proofLifecycle"),
            V::Undefined
        );
    }
}
