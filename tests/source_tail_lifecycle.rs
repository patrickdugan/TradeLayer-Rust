//! Complete source audit; focused runtime admission has no proof or custody success.
use tradelayer_core::issuance_intent::JsonValue as J;
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
const REFERENCE: &str = r###"
'use strict';
// Genuine unchanged source. No global helper alias, proof verifier or balances supplied.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),crypto=require('node:crypto'),vm=require('node:vm'),assert=require('node:assert/strict');
const stage=path.resolve(__dirname,'..'),repo=path.resolve(process.env.PROOF_REPO||path.join(stage,'../../outputs/TradeLayer-Rust'));
const flags=process.argv.slice(2),lock=flags.includes('--lock'),proofOnly=flags.includes('--proof10');
const numeric=require(path.join(repo,'scripts/legacy_numeric_oracle.cjs'));
if(lock){const packed=fs.readFileSync(path.join(repo,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');numeric.loadBigNumber=()=>{const m={exports:{}};vm.runInNewContext(raw.toString(),{module:m,exports:m.exports},{timeout:1000});return m.exports;};}
const pins={logic:['native_genesis/source/logic.js','f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d'],types:['legacy_types_source_types.txt','a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1'],txDecoder:['legacy_types_source_txDecoder.txt','a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076'],txEncoder:['legacy_types_source_txEncoder.txt','c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42'],main:['native_genesis/source/main.js','006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009']};
for(const[file,sha]of Object.values(pins))numeric.readPinnedSource(path.join(repo,'fixtures',file),sha);
const reference=path.join(repo,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(reference,'utf8');code=code.slice(0,code.indexOf('const scenes=['));const needle='return {types,logic,Orderbook,effects';assert.equal(code.split(needle).length,2);code=code.replace(needle,'return {context,tables,copy,types,logic,Orderbook,effects')+'\nmodule.exports={environment};';const mod=new Module(reference,module);mod.filename=reference;mod.paths=Module._nodeModulePaths(path.dirname(reference));mod._compile(code,reference);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT';let e,receiver,baseline;const records=[],inventory={sourceCommit:'dca0888b827c5bc28fa9189bc9e1d507f5c5670c',pins,methods:{},absence:{},mainGate:'decodedParams.valid === true; observed only, daemon/Consensus not executed'};
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(Object.prototype.toString.call(v)==='[object Error]')return['error',v.name,v.message];if(Array.isArray(v))return['array',v.map(tag)];if(Object.prototype.toString.call(v)==='[object Map]')return['map',[...v].map(([k,v])=>[tag(k),tag(v)])];if(Object.prototype.toString.call(v)==='[object Set]')return['set',[...v].map(tag)];if(v&&typeof v==='object'&&v.constructor?.name==='BigNumber')return['bignumber',v.toString()];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(r=>setImmediate(r));}
function snapshot(){return tag(e.copy(e.state()));}
function emit(name,command,result){assert.deepEqual(e.state().unsupported,[],name+' unowned boundary');const state=snapshot();assert.deepEqual(state,baseline,name+' no financial or source cache writes');records.push({name,command:tag(command),result:tag(result),state,effects:tag(e.effects),summary:result.outcome?.thrown?result.outcome.thrown:result.outcome?.returnedError??result.mainStrictGate??'returned'});}
async function reset(name,options={}){e=mod.exports.environment(options);receiver={Logic:e.logic,Types:e.types,Decode:e.load('txDecoder'),Encode:e.load('txEncoder'),Validity:e.load('validity')};for(const[label,methods]of Object.entries({Logic:['typeSwitch','batchMoveZkRollup','isValidRollupDataStructure','bindSmartContract','mintColoredCoin'],Types:['decodePayload','encodePayload'],Decode:['decodeBatchMoveZkRollup','decodePublishNewTx','decodeColoredCoin','decodeAbstractionBridge'],Encode:['encodeBatchMoveZkRollup','encodeColoredCoin','encodeAbstractionBridge','encodeBindSmartContract'],Validity:['validateBatchMoveZkRollup']})){for(const method of methods){const target=receiver[label],original=target[method];assert.equal(typeof original,'function',label+'.'+method);inventory.methods[label+'.'+method]={async:Object.prototype.toString.call(original)==='[object AsyncFunction]',bodySha256:crypto.createHash('sha256').update(original.toString()).digest('hex')};target[method]=function(...args){e.effects.push({kind:'source-function',target:label+'.'+method,args:structuredClone(args)});return Reflect.apply(original,this,args);};}}
for(const[label,names]of Object.entries({Logic:['coloredCoin','crossLayerBridge','smartContractBind'],Decode:['decodeCrossLayerBridge','decodeSmartContractBind']}))for(const method of names)inventory.absence[label+'.'+method]=typeof receiver[label][method];inventory.absence.globalRollupStructureHelper=typeof e.context.isValidRollupDataStructure;await flush();snapshot();baseline=snapshot();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function resultValue(value){return Object.prototype.toString.call(value)==='[object Error]'?{returnedError:{name:value.name,message:value.message,own:Object.fromEntries(Object.entries(value))}}:{returned:value};}
async function call(name,target,args=[]){e.effects.length=0;const[label,method]=target.split('.');let transport='Immediate',outcome;try{let value=Reflect.apply(receiver[label][method],receiver[label],args);if(value&&typeof value.then==='function'){transport='Pending';try{value=await value;outcome=resultValue(value);}catch(er){transport='PendingThrow';outcome={thrown:{name:er.name,message:er.message}};}}else outcome=resultValue(value);}catch(er){transport='Throw';outcome={thrown:{name:er.name,message:er.message}};}await flush();emit(name,{target,args},{transport,outcome});}
async function decode(name,type,payload,marker='tl',dispatch=false){e.effects.length=0;const args=[name,type,marker,payload,A,null,0,0,10];let params,outcome,gate,transport="Pending";try{params=await e.types.decodePayload(...args);gate=params.valid===true;let value;if(dispatch)value=await e.logic.typeSwitch(type,params);outcome={params,...resultValue(value)};}catch(er){transport="PendingThrow";outcome={params,thrown:{name:er.name,message:er.message}};}await flush();emit(name,{profile:dispatch?'decode-then-direct-typeSwitch':'types-decode-and-observe-main-gate',arguments:args},{transport,outcome,mainStrictGate:gate});}
(async()=>{
await reset('proof10-main');
await decode('type32-is-ordinal-text-no-valid',32,'[]');
await decode('type33-numeric-fields-no-valid',33,'0,7,1,home');
await decode('type34-missing-decoder',34,'7,1');
await decode('type35-missing-decoder',35,'hash,1,chain');
await call('batch-malformed-json','Logic.batchMoveZkRollup',[undefined,'{',null]);
await call('batch-valid-json-unbound-helper','Logic.batchMoveZkRollup',[{},'[]',{}]);
await call('direct-switch32-missing-rollup','Logic.typeSwitch',[32,{}]);
await call('direct-switch33-missing-method','Logic.typeSwitch',[33,{}]);
await call('direct-switch35-missing-method','Logic.typeSwitch',[35,{}]);
if(!proofOnly){
for(const entry of ['types-first','tally-first','volume-first']){await reset('entry-'+entry,entry==='tally-first'?{preload:'tally'}:entry==='volume-first'?{preload:'volumeIndex'}:{entry});for(const kind of [32,33,34,35])await decode(entry+'-'+kind,kind,'0,7,1,address');}
await reset('rollup-json-boundary');
for(const [name,raw]of [['missing',undefined],['null-argument',null],['empty',''],['text','garbage'],['trailing','[] x'],['array','[]'],['object','{}'],['null-json','null'],['false-json','false'],['number','1'],['string','"text"'],['valid-record','[{"fromAddress":"a","toAddress":"b","propertyId":7,"amount":1}]'],['null-element','[null]'],['own-property-shadow','[{"hasOwnProperty":0}]'],['nonfinite-json','1e400'],['array-coercion',[1]],['object-coercion',{}]])await call('batch-'+name,'Logic.batchMoveZkRollup',[undefined,raw,undefined]);
await reset('pure-structure-helper');
for(const[name,v]of [['empty',[]],['valid',[{fromAddress:'a',toAddress:'b',propertyId:7,amount:1}]],['undefined-values',[{fromAddress:undefined,toAddress:undefined,propertyId:undefined,amount:undefined}]],['missing-field',[{fromAddress:'a'}]],['null-element',[null]],['undefined-element',[undefined]],['null',null],['missing',undefined],['string','[]'],['number',1],['shadow',[{hasOwnProperty:0}]],['nested-array',[[]]],['zero-fields',[{fromAddress:0,toAddress:0,propertyId:0,amount:0}]]])await call('structure-'+name,'Logic.isValidRollupDataStructure',[v]);
await reset('direct-switch');
for(const kind of [32,33,34,35]){for(const[name,v]of [['false',false],['zero',0],['empty',''],['zero-text','0'],['empty-array',[]]])await call(`switch-${kind}-guard-${name}`,'Logic.typeSwitch',[kind,{valid:v}]);await call('switch-'+kind+'-null','Logic.typeSwitch',[kind,null]);await call('switch-'+kind+'-missing','Logic.typeSwitch',[kind,{}]);}
await call('switch32-valid-json-still-unbound','Logic.typeSwitch',[32,{rollupData:'[]',zkVerifier:{},zkProof:{}}]);
await call('actual-bind-stub','Logic.bindSmartContract',[{contractHash:'hash'}]);await call('actual-colored-stub','Logic.mintColoredCoin',[{propertyId:7}]);
await reset('types-and-decoders');
for(const kind of [32,33,34,35]){await decode('decode-then-direct-switch-'+kind,kind,'0,7,1,address','tl',true);await decode('nonstring-body-'+kind,kind,null);await decode('wrong-marker-'+kind,kind,'anything','xx');await decode('string-type-'+kind,String(kind),'anything');}
for(const body of ['',',,,','-1,z~,0x10,home','1,zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz,2','\ufeff2, 7, 1,address','2,7,1,address,ignored'])await decode('colored-body-'+JSON.stringify(body),33,body);
await call('standalone-abstraction-real-method','Decode.decodeAbstractionBridge',['7,a']);await call('publish-new-transaction-text','Decode.decodePublishNewTx',['anything']);await call('standalone-rollup-any-value','Decode.decodeBatchMoveZkRollup',[{opaque:'data'}]);
for(const raw of [null,undefined,{},1])await call('colored-direct-type-'+String(raw),'Decode.decodeColoredCoin',[raw]);
await call('validity-no-verifier','Validity.validateBatchMoveZkRollup',[{}]);await call('validity-empty-verifier','Validity.validateBatchMoveZkRollup',[{},{}]);await call('validity-null-params','Validity.validateBatchMoveZkRollup',[null,{}]);
await reset('encode-roundtrip');
const params={32:{payments:[],proof:'proof',miscLogic:{},miscData:{}},33:{encodeDecodeRecode:1,propertyId:7,SatsRatio:10,homeAddress:'home'},34:{propertyId:7,amount:1,destinationAddr:'dest'},35:{contractHash:'hash',type:1,hostChain:'chain'}};
for(const kind of [32,33,34,35]){await call('encode-'+kind,'Types.encodePayload',[kind,params[kind]]);const payload=e.types.encodePayload(kind,params[kind]);await decode('encoded-body-decode-'+kind,kind,payload.slice(3));}
}
const proof=records.slice(0,10).map(v=>JSON.stringify(v)).join('\n')+'\n',full=records.map(v=>JSON.stringify(v)).join('\n')+'\n';
inventory.observations=records.length;inventory.resets=records.filter(v=>v.name.startsWith('proof10-')||v.name.startsWith('entry-')||['rollup-json-boundary','pure-structure-helper','direct-switch','types-and-decoders','encode-roundtrip'].includes(v.name)).length;inventory.proof10Sha256=crypto.createHash('sha256').update(proof).digest('hex');
if(flags.includes('--test-frames')){for(const record of records){const tables=record.state[1].find(([name])=>name==='tables')[1];console.log(JSON.stringify({name:record.name,command:JSON.stringify(record.command),result:JSON.stringify(record.result),stateTables:JSON.stringify(tables),effects:JSON.stringify(record.effects)}));}return;}
if(flags.includes('--print')){process.stdout.write(full);return;}
const outputs={'source_tail_proof10.jsonl':proof,...(!proofOnly?{'source_tail_vectors.jsonl':full,'source_tail_inventory.json':JSON.stringify(inventory,null,2)+'\n'}:{})};
for(const[name,bytes]of Object.entries(outputs)){const target=path.join(stage,'fixtures',name);if(flags.includes('--check'))assert.equal(fs.readFileSync(target,'utf8'),bytes,name+' source reference differs');else{fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes);}}
console.error(`source-tail ${records.length} genuine observations (${lock?'9.1.2':'9.3.1'}); no proof/financial verdict supplied`);
})().catch(er=>{console.error(er.stack||er);process.exitCode=1;});

"###;
fn s(v: &str) -> V {
    V::String(v.into())
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn field<'a>(value: &'a J, name: &str) -> &'a J {
    let J::Object(o) = value else {
        panic!("object")
    };
    o.get(name).unwrap()
}
fn text(v: &J) -> String {
    let J::String(v) = v else { panic!("string") };
    v.clone()
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else { panic!("array") };
    v
}
fn diagnostic(v: &J) -> V {
    let raw = if let J::String(v) = v {
        v.clone()
    } else {
        v.to_json().unwrap()
    };
    tradelayer_core::property_catalog::parse_diagnostic_value(&raw).unwrap()
}
fn state_tables(v: &J) -> V {
    diagnostic(field(v, "stateTables"))
}
fn cases(lock: bool) -> Vec<J> {
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .args([
            "-e",
            REFERENCE,
            "tail-reference",
            "--test-frames",
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
        .map(|v| J::parse(v).unwrap())
        .collect()
}
fn admitted(c: &V) -> bool {
    if c.get("target") == s("Logic.batchMoveZkRollup") {
        return true;
    }
    if c.get("profile") != s("types-decode-and-observe-main-gate") {
        return false;
    }
    let args = array(c.get("arguments"));
    args.get(2) == Some(&s("tl"))
        && [32., 33., 34., 35.]
            .iter()
            .any(|n| args.get(1) == Some(&V::number(*n)))
}
fn command(c: &V) -> C {
    if c.get("target") != V::Undefined {
        C::Invoke(DependencyCall {
            target: "Logic.batchMoveZkRollup".into(),
            arguments: array(c.get("args")),
        })
    } else {
        C::ExecuteContract(array(c.get("arguments")))
    }
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
fn source_outcome(value: SourceOutcome, last: V) -> V {
    match value {
        SourceOutcome::Returned(v) => v,
        SourceOutcome::Thrown(e) => obj(&[
            ("params", last),
            (
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            ),
        ]),
    }
}
fn actual(runtime: &mut SourceContractRuntime, c: &V) -> V {
    match command(c) {
        C::Invoke(call) => returned(runtime.invoke_dependency(&call).unwrap()),
        C::ExecuteContract(a) => {
            let outcome = runtime.execute_contract_unverified(&a).unwrap();
            source_outcome(
                outcome,
                runtime
                    .last_contract_parameters()
                    .cloned()
                    .unwrap_or(V::Undefined),
            )
        }
        _ => unreachable!(),
    }
}
fn wanted(case: &J, c: &V) -> V {
    let result = diagnostic(field(case, "result"));
    if c.get("target") != V::Undefined {
        result
    } else {
        result.get("outcome")
    }
}
fn source_effects(v: V) -> V {
    V::Array(
        array(v)
            .into_iter()
            .filter_map(|v| {
                if v.get("kind") == s("dependency")
                    && v.get("target") == s("Logic.batchMoveZkRollup")
                {
                    Some(obj(&[
                        ("kind", s("source-function")),
                        ("target", v.get("target")),
                        ("args", v.get("args")),
                    ]))
                } else if v.get("kind") == s("source-function")
                    && v.get("target") == s("Logic.batchMoveZkRollup")
                    || v.get("kind") == s("db")
                    || v.get("kind") == s("database")
                {
                    Some(v)
                } else {
                    None
                }
            })
            .collect(),
    )
}
fn setup(runtime: &mut SourceContractRuntime, case: &J) {
    for table in array(state_tables(case)) {
        let V::String(name) = table.get("name") else {
            panic!("name")
        };
        runtime
            .import_collection(&name, array(table.get("rows")))
            .unwrap();
    }
}
#[test]
fn literal_raw_tail_and_sync_rollup_failures_match_genuine_source() {
    let baseline = cases(false);
    assert_eq!(baseline, cases(true));
    assert_eq!(baseline.len(), 131);
    let mut runtime = SourceContractRuntime::with_policy(P::ProofLifecycleV6);
    let mut count = 0;
    for case in &baseline {
        let c = diagnostic(field(case, "command"));
        if c.get("reset") != V::Undefined {
            runtime = SourceContractRuntime::with_policy(P::ProofLifecycleV6);
            setup(&mut runtime, case);
            continue;
        }
        if !admitted(&c) {
            continue;
        }
        count += 1;
        let name = text(field(case, "name"));
        let before = runtime.state_value().unwrap().get("tables");
        assert_eq!(
            actual(&mut runtime, &c),
            wanted(case, &c),
            "{name} source result and exact synchronous transport"
        );
        assert_eq!(
            runtime.state_value().unwrap().get("tables"),
            before,
            "{name} no metadata or balance writes"
        );
        assert_eq!(runtime.tally_cache(), V::Array(vec![]));
        assert_eq!(
            source_effects(V::Array(runtime.take_effects())),
            source_effects(diagnostic(field(case, "effects"))),
            "{name} ordered original call prefix"
        );
    }
    assert_eq!(count, 49);
}
#[test]
fn tail_source_errors_replay_and_old_policies_keep_prior_refusals() {
    let cases = cases(false);
    let mut replay = SourceContractReplay::with_policy(P::ProofLifecycleV6).unwrap();
    let mut count = 0;
    for case in &cases {
        let c = diagnostic(field(case, "command"));
        if c.get("reset") != V::Undefined {
            replay = SourceContractReplay::with_policy(P::ProofLifecycleV6).unwrap();
            for table in array(state_tables(case)) {
                let V::String(name) = table.get("name") else {
                    panic!("name")
                };
                replay
                    .apply(C::Import {
                        collection: name,
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
            continue;
        }
        if !admitted(&c) {
            continue;
        }
        count += 1;
        let receipt = replay.apply(command(&c)).unwrap();
        let actual = match receipt.outcome {
            O::Dependency(v) => returned(v),
            O::Source(v) => source_outcome(
                v,
                replay
                    .runtime()
                    .last_contract_parameters()
                    .cloned()
                    .unwrap_or(V::Undefined),
            ),
            v => panic!("not a local refusal: {v:?}"),
        };
        assert_eq!(actual, wanted(case, &c));
        let bytes = replay.checkpoint().unwrap();
        let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), bytes);
        let wrong = SourceContractReplayHead {
            commands: replay.head().commands,
            commitment: [0; 32],
        };
        assert!(SourceContractReplay::from_checkpoint(&bytes, wrong).is_err());
        replay = restored;
    }
    assert_eq!(count, 49);
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        let before = runtime.clone();
        assert_eq!(
            runtime.invoke_dependency(&DependencyCall {
                target: "Logic.batchMoveZkRollup".into(),
                arguments: vec![V::Null, s("[]"), V::Null]
            }),
            Err(SourceRuleError::UnknownDependency)
        );
        for kind in [32., 33., 34., 35.] {
            assert_eq!(
                runtime.execute_contract_unverified(&[
                    s("old"),
                    V::number(kind),
                    s("tl"),
                    s("body"),
                    s("actor"),
                    V::Null,
                    V::number(0.),
                    V::number(0.),
                    V::number(10.)
                ]),
                Err(SourceRuleError::InvalidInput)
            );
        }
        assert_eq!(runtime, before);
    }
}
