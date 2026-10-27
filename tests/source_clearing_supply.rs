//! Full unchanged CommonJS execution supplies supply expectations only.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};

fn s(value: &str) -> V {
    V::String(value.into())
}
fn array(value: V) -> Vec<V> {
    let V::Array(a) = value else {
        panic!("array {value:?}")
    };
    a
}
fn text(value: V) -> String {
    let V::String(s) = value else {
        panic!("string {value:?}")
    };
    s
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn outcome(value: DependencyReturn) -> V {
    match value {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}

const EXPECTED_ACTIONS: usize = 187;
const EXPECTED_CALLS: usize = 120;
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('composed environment changed');
code=code.replace("'scaling','vaults','procedural'","'scaling','procedural'");
code=code.replace('const sources = Object.fromEntries',"pins.vaults=['legacy_vaults_source.txt','3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983'];\nconst sources = Object.fromEntries");
code=code.replace('return {types,logic,Orderbook,effects' ,'return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';
const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();let e,instances,rejected;process.on('unhandledRejection',error=>rejected.push({name:error.name,message:error.message}));
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='funding-third';
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return['array',[...v].map(([k,v])=>['array',[tag(k),tag(v)]])];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function balance(value){return{amount:value,available:value,reserved:0,margin:0,vesting:0,channelBalance:0};}
function seed(options={}){
 e.table('contractList').splice(0);
 let properties=options.properties??[[7,{ticker:'P7',totalInCirculation:options.expected??0,type:1}]];
 if(options.emptyProperties)properties=[];
 e.table('propertyList').splice(0,Infinity,{_id:'propertyIndex',value:JSON.stringify(properties)});
 const value=options.available??0;
 e.table('tallyMap').splice(0,Infinity,{_id:'tallyMap',data:JSON.stringify([[A,{7:{amount:value,available:value,reserved:0,margin:0,vesting:0,channelBalance:0}}]])});
}
function state(){const T=e.load('tally'),P=e.load('property'),Vault=e.load('vaults');return{
 tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:[...(T.addresses??new Map())],margins:instances.map(i=>({seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]})),
 tallyMod:T.modFlag,tallyStaticInitialized:!!T.addresses,propertyCache:P.instance?[...P.instance.propertyIndex]:[],vaultInitialized:!!Vault.vaults,
 unhandledRejections:rejected,channels:[...e.load('channels').channelsRegistry],insurance:[...e.load('insurance').instances]};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))})));}
async function reset(name,options={}){e=m.exports.environment(options.entry==='tally-first'?{preload:'tally'}:options.entry==='volume-first'?{preload:'volumeIndex'}:{entry:options.entry??'main-subgraph'});instances=[];rejected=[];
 const Margin=e.load('marginMap'),original=Margin.loadMarginMap;Margin.loadMarginMap=async function(...args){const result=await original.apply(this,args);if(result&&result.margins&&!instances.includes(result))instances.push(result);return result;};
 seed(options);await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args,extra={}){e.effects.length=0;if(extra.failure){if(extra.failure.op==='getDatabase')factoryFailure(extra.failure);else e.setFailure(extra.failure);}let outcome;const names={Clearing:'clearing',Tally:'tally',Property:'property',Contract:'contractRegistry',Channels:'channels',Vaults:'vaults',Insurance:'insurance'};
 try{const [ns,...tail]=target.split('.');outcome={returned:await e.load(names[ns])[tail.join('.')](...args)}}catch(error){outcome={thrown:{name:error.name,message:error.message}}}
 await flush();emit(name,{target,args,...extra},outcome);}
(async()=>{
 for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
  await reset('empty-clearing-'+entry,{entry,emptyProperties:true});await call(entry+'-empty-clearing','Clearing.clearingFunction',[10]);
  await call(entry+'-set-contract-flag','Contract.setModFlag',[true]);await call(entry+'-net-zero-clears-flag','Clearing.clearingFunction',[10]);
 }
 await reset('actual-channel-removal');
 await call('add-empty-channel','Channels.setChannel',['empty',{participants:{A,B},A:{7:0},B:{7:0}}]);
 await call('add-live-channel','Channels.setChannel',['live',{participants:{A,B},A:{7:1},B:{7:0}}]);
 await call('clearing-deletes-cache-but-undefined-db-key','Clearing.clearingFunction',[10]);
 await call('load-restores-undeleted-empty-channel','Channels.loadChannelsRegistry',[]);
 await call('channel-remove-write-failure-prefix','Clearing.clearingFunction',[10],{failure:{collection:'channels',op:'remove',message:'channels-remove'}});
 await reset('equal-supply',{available:2,expected:2});await call('exact-supply','Clearing.getTotalTokenBalances',[10]);
 await call('actual-delta-sets-flag','Tally.updateBalance',[A,7,0,0,0,0,'supply-test',10]);await call('clearing-success-resets-tally-flag','Clearing.clearingFunction',[10]);
 await reset('mismatch-keeps-tally-flag',{available:2,expected:1});await call('delta-before-supply-rejection','Tally.updateBalance',[A,7,0,0,0,0,'supply-test',10]);await call('mismatch-message-and-flag','Clearing.clearingFunction',[10]);
 await reset('allowed-properties',{properties:[[2,{ticker:'P2',totalInCirculation:100,type:1}],[3,{ticker:'P3',totalInCirculation:100,type:1}],[4,{ticker:'P4',totalInCirculation:100,type:1}],[9,{ticker:'P9',totalInCirculation:100,type:2}]]});await call('literal-allowed-supply-ids-and-type','Clearing.getTotalTokenBalances',[11]);
 await reset('string-id-is-not-allowed',{properties:[['2',{ticker:'P2',totalInCirculation:1,type:1}]]});await call('string-two-mismatch','Clearing.getTotalTokenBalances',[11]);
 for(const [name,id,available,expected]of [['loss',5,0,2],['profit',5,2,1],['string-loss','5',0,1]]){
  await reset('property-five-'+name,{properties:[[id,{ticker:'P5',totalInCirculation:expected,type:1}]],available});
  if(available)await imported('p5-tally-'+name,'tallyMap',[{_id:'tallyMap',data:JSON.stringify([[A,{5:{amount:available,available,reserved:0,margin:0,vesting:0,channelBalance:0}}]])}]);
  await call('property5-reconcile-'+name,'Clearing.getTotalTokenBalances',[12]);await call('property5-recheck-'+name,'Clearing.getTotalTokenBalances',[12]);
 }
 for(const[name,available,expected]of [['negative-one-sat',0,1e-8],['positive-one-sat',1e-8,0],['half-sat-round-up',0,5e-9]]){
  await reset(name,{available,expected});await call('dust-'+name,'Clearing.getTotalTokenBalances',[13]);await call('dust-recheck-'+name,'Clearing.getTotalTokenBalances',[13]);
 }
 await reset('cached-property-index',{available:2,expected:2});await call('cache-property','Property.getPropertyIndex',[]);await imported('replace-persisted-property','propertyList',[{_id:'propertyIndex',value:JSON.stringify([[7,{ticker:'changed',totalInCirculation:999,type:1}]])}]);await call('cached-property-metadata-stays-authoritative','Clearing.getTotalTokenBalances',[14]);
 await reset('vault-documents-remain-unhydrated');await imported('import-vault-document','vaults',[{_id:'s-7-1',data:{margin:50,available:50,outstanding:100}}]);await imported('import-synthetic-document','syntheticTokens',[{_id:'s-7-1',data:{vaultId:'s-7-1'}}]);await call('vault-find-one-object-does-not-hydrate','Vaults.getTotalBalanceForProperty',[7]);await call('synthetic-outstanding-remains-zero','Vaults.getTotalOutstandingForProperty',['s-7-1']);
 for(const[collection,op]of [['propertyList','getDatabase'],['propertyList','findOne'],['tallyMap','findOne'],['feeCache','find'],['insurance','find'],['vaults','getDatabase'],['vaults','findOne'],['syntheticTokens','getDatabase'],['syntheticTokens','findOne'],['iou','find']]){
  await reset('supply-failure-'+collection+'-'+op);await call('failure-'+collection+'-'+op,'Clearing.getTotalTokenBalances',[15],{failure:{collection,op,message:'supply-'+collection+'-'+op}});await call('retry-'+collection+'-'+op,'Clearing.getTotalTokenBalances',[15]);
 }
 for(const[collection,op]of [['tallyMapDelta','getDatabase'],['tallyMapDelta','findOne'],['tallyMapDelta','insert'],['tallyMap','findOne'],['insurance','findOne'],['insurance','insert'],['insurance','update']]){
  await reset('dust-failure-'+collection+'-'+op,{expected:1e-8});await call('unawaited-dust-'+collection+'-'+op,'Clearing.getTotalTokenBalances',[16],{failure:{collection,op,message:'dust-'+collection+'-'+op}});await call('dust-retry-'+collection+'-'+op,'Clearing.getTotalTokenBalances',[16]);
 }
 await reset('direct-tally-delta');await call('direct-record-keeps-all-eleven-args','Tally.recordTallyMapDelta',['A',17,7,999,1,2,3,4,5,'custom',null]);
 await call('direct-delta-rejects-when-awaited','Tally.recordTallyMapDelta',['A',17,7,999,0,0,0,0,0,'custom','tx'],{failure:{collection:'tallyMapDelta',op:'insert',message:'awaited-delta'}});
 await reset('missing-expected-supply',{available:2,properties:[[7,{ticker:'P7',type:1}]]});await call('nan-expected-skips-comparison','Clearing.getTotalTokenBalances',[18]);
 await reset('nonfinite-expected-supply',{properties:[[7,{ticker:'P7',totalInCirculation:'Infinity',type:1}]]});await call('infinite-mismatch-text','Clearing.getTotalTokenBalances',[18]);
 await reset('dust-full-clearing',{expected:1e-8});await call('dust-trigger-flag','Tally.updateBalance',[A,7,0,0,0,0,'dust',19]);await call('dust-clearing-resets-flag-after-async-delta','Clearing.clearingFunction',[19]);
 await reset('dust-full-clearing-background-failure',{expected:1e-8});await call('dust-failure-trigger-flag','Tally.updateBalance',[A,7,0,0,0,0,'dust',19]);await call('background-delta-rejection-does-not-reject-clearing','Clearing.clearingFunction',[19],{failure:{collection:'tallyMapDelta',op:'insert',message:'background-delta'}});
 await reset('initial-delta-tally-load-fails');await call('delta-unawaited-getTally-rejects-after-first-failed-load','Tally.recordTallyMapDelta',['A',20,7,0,0,0,0,0,0,'direct',''],{failure:{collection:'tallyMap',op:'findOne',message:'first-delta-tally-load'}});
 await call('delta-followup-initializes-static-map','Tally.getTotalForProperty',[7]);
 await reset('initial-values-read-fails');await call('get-total-tally-first-failed-load','Tally.getTotalTally',[7],{failure:{collection:'tallyMap',op:'findOne',message:'first-total-load'}});
 await call('load-empty-tally-before-next-failure','Tally.loadFromDB',[]);await call('failed-load-keeps-initialized-empty-map','Tally.getTotalTally',[7],{failure:{collection:'tallyMap',op:'findOne',message:'later-total-load'}});
 await reset('invalid-property-container');await imported('null-properties','tallyMap',[{_id:'tallyMap',data:JSON.stringify([[A,null]])}]);await call('null-properties-get-total','Tally.getTotalForProperty',[7]);
 await reset('property-map-primitive-key-semantics');await imported('raw-nan-negative-zero-property-ids','propertyList',[{_id:'propertyIndex',value:[[NaN,{ticker:'first',totalInCirculation:0,type:1}],[NaN,{ticker:'last',totalInCirculation:0,type:1}],[-0,{ticker:'zero',totalInCirculation:0,type:1}]]}]);await call('map-samevaluezero-and-canonical-zero','Property.getPropertyIndex',[]);
 const healthyTally=[{_id:'tallyMap',data:JSON.stringify([[A,{7:balance(2)}]])}],malformedTally=[{_id:'tallyMap',data:'{bad-json'}];
 for(const[method,args]of [['getTally',[A,7]],['updateBalance',[A,7,0,0,0,0,'first-load',21]]]){
  for(const op of ['getDatabase','findOne']){
   await reset('first-'+method+'-'+op,{available:2});
   await call('first-'+method+'-caught-'+op+'-then-has-throws','Tally.'+method,args,{failure:{collection:'tallyMap',op,message:'first-'+method+'-'+op}});
   await call('first-'+method+'-'+op+'-retry-initializes','Tally.'+method,args);
  }
  await reset('first-'+method+'-malformed-json',{available:2});await imported('malformed-'+method+'-persisted-json','tallyMap',malformedTally);
  await call('first-'+method+'-caught-json-then-has-throws','Tally.'+method,args);
  await imported('repair-'+method+'-persisted-json','tallyMap',healthyTally);await call('first-'+method+'-json-retry-initializes','Tally.'+method,args);
  await reset('initialized-'+method+'-read-failure',{available:2});await call('initialize-static-before-'+method,'Tally.loadFromDB',[]);
  await call('initialized-'+method+'-keeps-cached-map-after-failure','Tally.'+method,args,{failure:{collection:'tallyMap',op:'findOne',message:'later-'+method+'-read'}});
 }
 await reset('save-before-static-load',{available:2});await call('save-before-load-catches-noniterable-static-map','Tally.saveToDB',[22]);
 await call('save-before-load-factory-failure-is-caught','Tally.saveToDB',[22],{failure:{collection:'tallyMap',op:'getDatabase',message:'first-save-factory'}});
 await call('save-retry-still-does-not-initialize-static-map','Tally.saveToDB',[22]);await call('get-after-unsaved-attempts-initializes','Tally.getTally',[A,7]);
 await call('save-write-failure-keeps-initialized-cache','Tally.saveToDB',[22],{failure:{collection:'tallyMap',op:'update',message:'initialized-save-update'}});
 await call('save-write-retry-persists-initialized-cache','Tally.saveToDB',[22]);
 await reset('save-does-not-consume-read-failure',{available:2});await call('save-before-load-never-finds-row','Tally.saveToDB',[23],{failure:{collection:'tallyMap',op:'findOne',message:'save-read-not-consumed'}});
 await call('get-consumes-save-unreached-read-failure','Tally.getTally',[A,7]);await call('get-after-unreached-read-failure-initializes','Tally.getTally',[A,7]);
 await reset('save-and-malformed-persisted-json',{available:2});await imported('save-before-load-malformed-json','tallyMap',malformedTally);
 await call('save-before-load-does-not-parse-persisted-json','Tally.saveToDB',[24]);await call('get-after-save-still-fails-malformed-json','Tally.getTally',[A,7]);
 await imported('repair-before-initializing-save-cache','tallyMap',healthyTally);await call('initialize-cache-before-malformed-reload','Tally.loadFromDB',[]);
 await imported('malformed-json-after-initialized-cache','tallyMap',malformedTally);await call('malformed-reload-retains-initialized-cache','Tally.loadFromDB',[]);
 await call('save-replaces-malformed-json-with-actual-cache','Tally.saveToDB',[24]);await call('get-reads-repaired-cache-save','Tally.getTally',[A,7]);
})().catch(error=>{console.error(error);process.exitCode=1});

"###;
fn compare_value(actual: &V, expected: &V, name: &str) {
    if actual == expected {
        return;
    }
    fn diff(a: &V, b: &V, p: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return diff(a, b, format!("{p}[{i}]"));
                    }
                }
                format!("{p}: lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{p}: fields {ak} != {bk}");
                    };
                    if av != bv {
                        return diff(av, bv, format!("{p}.{ak}"));
                    }
                }
                format!("{p}: field lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    panic!("{name}: {}", diff(actual, expected, "$".into()));
}

#[test]
fn unchanged_source_supply_channels_flags_dust_and_failures() {
    let result = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut runtime = SourceContractRuntime::new();
    let mut actions = 0;
    let mut calls = 0;
    for line in std::str::from_utf8(&result.stdout).unwrap().lines() {
        let case = tradelayer_core::property_catalog::parse_diagnostic_value(line)
            .unwrap_or_else(|e| panic!("diagnostic {e:?}: {}", &line[..line.len().min(200)]));
        let name = text(case.get("name"));
        let command = case.get("command");
        if !matches!(command.get("reset"), V::Undefined) {
            runtime = SourceContractRuntime::new();
            let entry = command.get("reset").get("entry");
            runtime
                .set_entry(if let V::String(ref s) = entry {
                    s
                } else {
                    "main-subgraph"
                })
                .unwrap();
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap();
            }
        } else if !matches!(command.get("seed"), V::Undefined) {
            let seed = command.get("seed");
            runtime
                .import_collection(&text(seed.get("collection")), array(seed.get("rows")))
                .unwrap();
        } else {
            if !matches!(command.get("failure"), V::Undefined) {
                let f = command.get("failure");
                let op = text(f.get("op"));
                runtime
                    .fail_storage(StorageFailure {
                        collection: text(f.get("collection")),
                        operation: if op == "getDatabase" {
                            op
                        } else {
                            format!("{op}Async")
                        },
                        skip: if let V::Number(b) = f.get("skip") {
                            f64::from_bits(b) as usize
                        } else {
                            0
                        },
                        message: text(f.get("message")),
                    })
                    .unwrap();
            }
            let actual = runtime
                .invoke_dependency(&DependencyCall {
                    target: text(command.get("target")),
                    arguments: array(command.get("args")),
                })
                .unwrap_or_else(|e| panic!("{name}: local {e:?}"));
            compare_value(
                &outcome(actual),
                &case.get("outcome"),
                &format!("{name} outcome"),
            );
            calls += 1;
        }
        let mut names = std::collections::BTreeSet::new();
        for table in array(case.get("state").get("tables")) {
            let collection = text(table.get("name"));
            names.insert(collection.clone());
            compare_value(
                &V::Array(runtime.collection(&collection).to_vec()),
                &table.get("rows"),
                &format!("{name} persisted {collection}"),
            );
        }
        for table in array(runtime.state_value().unwrap().get("tables")) {
            let collection = text(table.get("name"));
            if !names.contains(&collection) {
                assert_eq!(
                    table.get("rows"),
                    V::Array(vec![]),
                    "{name}: unexpected {collection}"
                );
            }
        }
        compare_value(
            &runtime.tally_cache(),
            &case.get("state").get("tally"),
            &format!("{name} Tally cache"),
        );
        let expected_instances = array(case.get("state").get("margins"));
        let actual_instances = array(runtime.margins().state_value().unwrap().get("instances"));
        assert_eq!(
            actual_instances.len(),
            expected_instances.len(),
            "{name} loaded Margin instances"
        );
        for (i, expected) in expected_instances.iter().enumerate() {
            compare_value(
                &runtime.margins().instance_value(i as u64 + 1).unwrap(),
                expected,
                &format!("{name} Margin instance{}", i + 1),
            );
        }
        let actual = runtime.state_value().unwrap();
        let global = actual.get("globalClearing");
        let expected = case.get("state");
        for field in [
            "tallyMod",
            "tallyStaticInitialized",
            "propertyCache",
            "vaultInitialized",
            "unhandledRejections",
        ] {
            compare_value(
                &global.get(field),
                &expected.get(field),
                &format!("{name} {field}"),
            );
        }
        let channels = global.get("channels");
        compare_value(
            &channels.get("registry"),
            &expected.get("channels"),
            &format!("{name} channels"),
        );
        let V::Object(insurance) = actual.get("insurance") else {
            panic!("insurance")
        };
        compare_value(
            &V::Array(
                insurance
                    .into_iter()
                    .map(|(k, v)| V::Array(vec![s(&k), v]))
                    .collect(),
            ),
            &expected.get("insurance"),
            &format!("{name} Insurancecache"),
        );
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect::<Vec<_>>();
        compare_value(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered effects"),
        );
        actions += 1;
    }
    assert_eq!(actions, EXPECTED_ACTIONS, "supply source inventory changed");
    assert_eq!(calls, EXPECTED_CALLS, "supply calls changed");
}

#[test]
fn unawaited_dust_rejection_and_insurance_prefix_replay_then_continue() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    let output = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let cases = std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|line| tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap())
        .collect::<Vec<_>>();
    let start = cases
        .iter()
        .position(|case| case.get("name") == s("dust-failure-tallyMapDelta-insert"))
        .unwrap();
    let mut replay = SourceContractReplay::new().unwrap();
    for table in array(cases[start].get("state").get("tables")) {
        replay
            .apply(C::Import {
                collection: text(table.get("name")),
                rows: array(table.get("rows")),
            })
            .unwrap();
    }
    replay
        .apply(C::FailStorage(StorageFailure {
            collection: "tallyMapDelta".into(),
            operation: "insertAsync".into(),
            skip: 0,
            message: "dust-tallyMapDelta-insert".into(),
        }))
        .unwrap();
    for (index, case) in cases[start + 1..start + 3].iter().enumerate() {
        let command = case.get("command");
        let receipt = replay
            .apply(C::Invoke(DependencyCall {
                target: text(command.get("target")),
                arguments: array(command.get("args")),
            }))
            .unwrap();
        assert!(matches!(
            receipt.outcome,
            O::Dependency(DependencyReturn::Pending(V::Undefined))
        ));
        let rejected = replay
            .runtime()
            .state_value()
            .unwrap()
            .get("globalClearing")
            .get("unhandledRejections");
        assert_eq!(
            array(rejected).len(),
            1,
            "background rejection retained at step{index}"
        );
        for table in array(case.get("state").get("tables")) {
            compare_value(
                &V::Array(
                    replay
                        .runtime()
                        .collection(&text(table.get("name")))
                        .to_vec(),
                ),
                &table.get("rows"),
                "genuine retained dust/insurance documents",
            );
        }
        let checkpoint = replay.checkpoint().unwrap();
        let restored = SourceContractReplay::from_checkpoint(&checkpoint, replay.head()).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.checkpoint().unwrap(), checkpoint);
        replay = restored;
    }
}
