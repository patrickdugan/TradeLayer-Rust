//! Genuine full-class mark settlement; outcomes never answer dependencies.
use tradelayer_core::source_contract_runtime::{SourceContractRuntime, StorageFailure};
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn text(v: V) -> String {
    let V::String(v) = v else {
        panic!("string expected")
    };
    v
}
fn array(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("array expected")
    };
    v
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn result(v: DependencyReturn) -> V {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn returned(v: DependencyReturn) -> V {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => v,
        other => panic!("read must return {other:?}"),
    }
}
fn compare(a: &V, b: &V, label: &str) {
    fn canonical(v: &V) -> V {
        match v {
            V::BigNumber(v) => V::BigNumber(
                v.parse::<tradelayer_core::decimal::Decimal>()
                    .map_or_else(|_| v.clone(), |d| d.to_string()),
            ),
            V::Array(v) => V::Array(v.iter().map(canonical).collect()),
            V::Object(v) => V::Object(v.iter().map(|(k, v)| (k.clone(), canonical(v))).collect()),
            _ => v.clone(),
        }
    }
    fn diff(a: &V, b: &V, p: String) -> String {
        match (a, b) {
            (V::Array(a), V::Array(b)) => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        return diff(a, b, format!("{p}[{i}]"));
                    }
                }
                format!("{p} lengths {} != {}", a.len(), b.len())
            }
            (V::Object(a), V::Object(b)) => {
                for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                    if ak != bk {
                        return format!("{p} fields {ak} != {bk}");
                    };
                    if av != bv {
                        return diff(av, bv, format!("{p}.{ak}"));
                    }
                }
                format!("{p} lengths {} != {}", a.len(), b.len())
            }
            _ => format!("{p}: {a:?} != {b:?}"),
        }
    }
    let a = canonical(a);
    let b = canonical(b);
    if a != b {
        if label.contains("ordered actual effects") {
            if let (V::Array(actual), V::Array(expected)) = (&a, &b) {
                let at = actual
                    .iter()
                    .zip(expected)
                    .position(|(a, b)| a != b)
                    .unwrap_or(actual.len().min(expected.len()));
                for (name, events) in [("actual", actual), ("source", expected)] {
                    for (i, event) in events.iter().enumerate().skip(at.saturating_sub(2)).take(7) {
                        eprintln!(
                            "{name}[{i}] {:?} {:?} {:?}",
                            event.get("kind"),
                            event.get("name"),
                            event.get("method")
                        );
                    }
                }
            }
        }
        panic!("{label}: {}", diff(&a, &b, "$".into()))
    }
}
const REFERENCE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('driver environment changed');code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,BN,types,logic,Orderbook,effects');
// Extend only the reference loader inventory with the unchanged pinned vault
// module. No financial method or returned value is replaced.
code=code.replace('const pins = {',"const pins = {vaults:['legacy_vaults_source.txt','3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983'],");code=code.replace("'scaling','vaults','procedural'","'scaling','procedural'");
code+='\nmodule.exports={environment,BN};\n';const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);const {environment,BN}=driver.exports;let e,instances,keys,books;
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',D='third-loss-actor';
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return['array',[...v].map(([k,v])=>['array',[tag(k),tag(v)]])];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
const bal=(available,margin)=>({amount:available+margin,available,reserved:0,margin,vesting:0,channelBalance:0});
function aliasState(){const refs=[],id=v=>{let i=refs.indexOf(v);if(i<0){i=refs.length;refs.push(v);}return i;};return{margins:instances.map(m=>[...m.margins].map(([key,pos])=>[key,id(pos)])),contexts:[...keys].map(key=>[key,e.load('clearing').getPositionsFromCache(key).map(id)])};}
function state(){const c=e.load('clearing'),contract=e.load('contractRegistry');const source={tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),insurance:e.load('insurance').instances};return{aliases:aliasState(),tables:source.tables,tally:[...(e.load('tally').addresses??new Map())],margins:instances.map(i=>({seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]})),books:books.map((b,i)=>({id:i+1,...b})),contexts:[...keys].map(k=>[k,c.getPositionsFromCache(k)]),blockTrades:c.blockTrades,oracleMarks:c.latestOracleMarkById,nativeMarks:c.latestNativeMarkById,deleverageTrades:c.deleverageTrades,liquidationRecords:c.liquidationRecords,contractSeries:contract.instance?.contractSeries??[],insurance:source.insurance,contractMod:!!contract.modFlag,tallyMod:e.load('tally').modFlag};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))})));}
async function reset(name,options={}){e=environment({});instances=[];keys=new Set();books=[];const Book=e.load('orderbook'),loadBook=Book.prototype.loadOrderBook;Book.prototype.loadOrderBook=function(...args){if(!books.includes(this))books.push(this);return loadBook.apply(this,args);};const Margin=e.load('marginMap');Object.defineProperty(Margin.prototype,'seriesId',{configurable:true,set(value){Object.defineProperty(this,'seriesId',{value,writable:true,enumerable:true,configurable:true});instances.push(this);}});const c=e.load('clearing'),init=c.initPositionCache,release=c.flushPositionCache;c.initPositionCache=function(...args){const key=init.apply(this,args);keys.add(key);return key;};c.flushPositionCache=function(...args){const value=release.apply(this,args);keys.delete(args[0]);return value;};
 const meta=e.table('contractList')[0].data;meta.inverse=!!options.inverse;meta.expiryPeriod=0;meta.volAnnual=0;
 const last=100,current=options.negative?98:102;e.table('oracleData').splice(0,Infinity,{_id:'prior',oracleId:1,blockHeight:9,data:{price:last}},{_id:'current',oracleId:1,blockHeight:10,data:{price:current}});
 const shortMargin=options.partial?2:options.liquidation?0:10,shortAvail=options.liquidation||options.partial?0:100;
 const positions=[[A,{address:A,contracts:1,margin:10,unrealizedPNL:0,avgPrice:100}],[B,{address:B,contracts:-1,margin:shortMargin,unrealizedPNL:0,avgPrice:100}]];
 if(Object.hasOwn(options,'bag'))positions[1][1].options=options.bag;
 if(options.third){positions[0][1].contracts=2;positions.push([D,{address:D,contracts:-1,margin:10,unrealizedPNL:0,avgPrice:100}]);if(Object.hasOwn(options,'thirdBag'))positions[2][1].options=options.thirdBag;}

 e.table('marginMaps').splice(0,Infinity,...(options.empty?[]:[{_id:JSON.stringify({seriesId:1}),value:JSON.stringify(positions)}]));
 if(options.badPrimary)e.table('marginMaps')[0].value='{';
 if(Object.hasOwn(options,'altBag')||Object.hasOwn(options,'altThirdBag')||options.badAlternate){const alt=[[B,{address:B,contracts:-1,margin:10,avgPrice:100,unrealizedPNL:0,options:options.altBag??{}}]];if(options.third)alt.push([D,{address:D,contracts:-1,margin:10,avgPrice:100,unrealizedPNL:0,options:options.altThirdBag??{}}]);e.table('marginMaps').push({_id:JSON.stringify({seriesId:'1'}),value:options.badAlternate?'{':JSON.stringify(alt)});}
 const tallies=[[A,{7:bal(100,10)}],[B,{7:bal(shortAvail,shortMargin)}]];if(options.third)tallies.push([D,{7:bal(100,10)}]);e.table('tallyMap')[0].data=JSON.stringify(tallies);
 await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],failure){e.effects.length=0;e.setHeight(10);if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}let outcome;try{let value;if(target==='updateFromSource'){const c=e.load('clearing'),r=e.load('contractRegistry'),id=args[0],block=args[1],info=await c.isPriceUpdatedForBlockHeight(id,block),collateral=await r.getCollateralId(id),inverse=await r.isInverse(id),notional=await r.getNotionalValue(id,info.thisPrice);value=await c.updateMarginMaps(block,id,collateral,inverse,notional.notionalPerContract,info);}else{const[ns,method]=target.split('.');value=await e.load(ns==='Clearing'?'clearing':'contractRegistry')[method](...args);}outcome={returned:value};}catch(error){outcome={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{target,args,failure},outcome);}
(async()=>{
const good={contracts:1,avgPrice:90,margin:0};
for(const [label,options]of [
 ['primary-null',{bag:{'1-100-C-90':null}}],
 ['alternate-null',{altBag:{'1-100-C-90':null}}],
 ['alternate-array-null',{altBag:[null]}],
 ['null-current-falls-back',{bag:{'1-100-C-90':null},altBag:{'1-100-C-90':good}}],
 ['primary-array-null',{bag:[null]}],
 ['primary-string',{bag:'abc'}],
 ['nan-strike',{bag:{'1-100-C-NaN':good}}],
 ['missing-quantity',{bag:{'1-100-C-90':{margin:2}}}],
 ['falsey-null-bag',{bag:null}]
]){await reset('direct-'+label,options);await call('direct-'+label+'-first','Clearing.computeOptionAdjustments',[1,B,100,10,144]);await call('direct-'+label+'-retry','Clearing.computeOptionAdjustments',[1,B,100,10,144]);}
for(const op of ['findOne','getDatabase'])for(const skip of [0,1]){const label=op+'-'+skip;await reset('direct-load-'+label);await call('direct-load-'+label+'-failed','Clearing.computeOptionAdjustments',[1,B,100,10,144],{collection:'marginMaps',op,skip,message:'adjustment-'+label});await call('direct-load-'+label+'-retry','Clearing.computeOptionAdjustments',[1,B,100,10,144]);}
for(const option of ['badPrimary','badAlternate']){await reset('direct-json-'+option,{[option]:true});await call('direct-json-'+option+'-first','Clearing.computeOptionAdjustments',[1,B,100,10,144]);await call('direct-json-'+option+'-retry','Clearing.computeOptionAdjustments',[1,B,100,10,144]);}
for(const [label,options,failure]of [
 ['success',{third:true}],
 ['primary-null',{third:true,thirdBag:{'1-100-C-90':null}}],
 ['alternate-null',{third:true,altThirdBag:{'1-100-C-90':null}}],
 ...['findOne','getDatabase'].flatMap(op=>[4,5].map(skip=>[op+'-'+skip,{third:true},{collection:'marginMaps',op,skip,message:'loss-adjustment-'+op+'-'+skip}]))
]){await reset('loss-'+label,options);await call('loss-'+label+'-prime','Clearing.isPriceUpdatedForBlockHeight',[1,9]);await call('loss-'+label+'-first','updateFromSource',[1,10],failure);await call('loss-'+label+'-retry','updateFromSource',[1,10]);}
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
        .map(|line| {
            tradelayer_core::property_catalog::parse_diagnostic_value(line)
                .unwrap_or_else(|e| panic!("diagnostic {e:?}"))
        })
        .collect()
}
fn invoke(runtime: &mut SourceContractRuntime, target: &str, args: Vec<V>) -> DependencyReturn {
    runtime
        .invoke_dependency(&DependencyCall {
            target: target.into(),
            arguments: args,
        })
        .unwrap_or_else(|e| panic!("{target}: local error {e:?}"))
}

fn assert_source_state(runtime: &SourceContractRuntime, case: &V) {
    let name = text(case.get("name"));
    if name == "loss-primary-null-first" {
        let rows = array(runtime.tally_cache());
        let loser = rows
            .iter()
            .map(|row| array(row.clone()))
            .find(|row| row[0] == s("1BitcoinEaterAddressDontSendf59kuE"))
            .unwrap();
        assert_eq!(loser[1].get("7").get("available"), V::number(98.));
        assert!(!runtime.collection("iou").is_empty());
    }
    let expected = case.get("state");
    let mut names = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let collection = text(table.get("name"));
        names.insert(collection.clone());
        compare(
            &V::Array(runtime.collection(&collection).to_vec()),
            &table.get("rows"),
            &format!("{name} whole {collection}"),
        )
    }
    let state = runtime.state_value().unwrap();
    for table in array(state.get("tables")) {
        let collection = text(table.get("name"));
        if !names.contains(&collection) {
            compare(
                &table.get("rows"),
                &V::Array(vec![]),
                &format!("{name} extra {collection}"),
            )
        }
    }
    compare(
        &runtime.tally_cache(),
        &expected.get("tally"),
        &format!("{name} Tally cache"),
    );
    let instances = array(expected.get("margins"));
    assert_eq!(
        array(runtime.margins().state_value().unwrap().get("instances")).len(),
        instances.len(),
        "{name} Margin instances"
    );
    for (i, expected) in instances.iter().enumerate() {
        compare(
            &runtime.margins().instance_value(i as u64 + 1).unwrap(),
            expected,
            &format!("{name} full Margin{i}"),
        )
    }
    let books = array(expected.get("books"));
    assert_eq!(
        array(runtime.queue().state_value().get("instances")).len(),
        books.len(),
        "{name} queue receivers"
    );
    for book in books {
        let V::Number(id) = book.get("id") else {
            panic!()
        };
        let V::Object(fields) = book else { panic!() };
        let expected = V::Object(fields.into_iter().filter(|(k, _)| k != "id").collect());
        compare(
            &runtime
                .queue()
                .instance_value(f64::from_bits(id) as u64)
                .unwrap(),
            &expected,
            &format!("{name} actual book cache"),
        )
    }
    for field in ["blockTrades", "oracleMarks", "nativeMarks"] {
        compare(
            &state.get(field),
            &expected.get(field),
            &format!("{name} {field}"),
        )
    }
    let mut arena_ids = vec![];
    let mut reference = |id: V| {
        let index = if let Some(index) = arena_ids.iter().position(|v| v == &id) {
            index
        } else {
            arena_ids.push(id);
            arena_ids.len() - 1
        };
        V::number(index as f64)
    };
    let margin_aliases = V::Array(
        array(state.get("margins").get("instances"))
            .into_iter()
            .map(|instance| {
                V::Array(
                    array(instance.get("margins"))
                        .into_iter()
                        .map(|pair| {
                            let pair = array(pair);
                            V::Array(vec![pair[0].clone(), reference(pair[1].clone())])
                        })
                        .collect(),
                )
            })
            .collect(),
    );
    let context_aliases = V::Array(
        array(state.get("globalClearing").get("contexts"))
            .into_iter()
            .map(|pair| {
                let pair = array(pair);
                V::Array(vec![
                    pair[0].clone(),
                    V::Array(
                        array(pair[1].clone())
                            .into_iter()
                            .map(&mut reference)
                            .collect(),
                    ),
                ])
            })
            .collect(),
    );
    compare(
        &obj(&[("margins", margin_aliases), ("contexts", context_aliases)]),
        &expected.get("aliases"),
        &format!("{name} actual cross-cache reference identity"),
    );
    let global = state.get("globalClearing");
    for field in [
        "deleverageTrades",
        "liquidationRecords",
        "contractSeries",
        "tallyMod",
    ] {
        compare(
            &global.get(field),
            &expected.get(field),
            &format!("{name} {field}"),
        )
    }
    compare(
        &state.get("continuation").get("contractMod"),
        &expected.get("contractMod"),
        &format!("{name} contract mod"),
    );
    let V::Object(insurance) = state.get("insurance") else {
        panic!()
    };
    compare(
        &V::Array(
            insurance
                .into_iter()
                .map(|(k, v)| V::Array(vec![s(&k), v]))
                .collect(),
        ),
        &expected.get("insurance"),
        &format!("{name} insurance cache"),
    );
    let objects = array(state.get("margins").get("objects"));
    let contexts = V::Array(
        array(global.get("contexts"))
            .into_iter()
            .map(|pair| {
                let pair = array(pair);
                V::Array(vec![
                    pair[0].clone(),
                    V::Array(
                        array(pair[1].clone())
                            .iter()
                            .map(|id| {
                                let V::Number(id) = id else { panic!() };
                                objects[f64::from_bits(*id) as usize].clone()
                            })
                            .collect(),
                    ),
                ])
            })
            .collect(),
    );
    compare(
        &contexts,
        &expected.get("contexts"),
        &format!("{name} full context aliases/positions"),
    );
}
#[test]
fn actual_option_adjustments_and_loss_settlement_retain_exact_failure_prefixes() {
    let mut runtime = SourceContractRuntime::with_policy(
        tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy::OptionLifecycleV4,
    );
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            runtime = SourceContractRuntime::with_policy(tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy::OptionLifecycleV4);
            for table in array(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), array(table.get("rows")))
                    .unwrap()
            }
        } else {
            runtime.set_height(V::number(10.)).unwrap();
            let f = command.get("failure");
            if matches!(f, V::Object(_)) {
                let op = text(f.get("op"));
                runtime
                    .fail_storage(StorageFailure {
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
                    .unwrap();
            };
            let target = text(command.get("target"));
            let args = array(command.get("args"));
            let actual = if target == "updateFromSource" {
                let id = args[0].clone();
                let block = args[1].clone();
                let info = returned(invoke(
                    &mut runtime,
                    "Clearing.isPriceUpdatedForBlockHeight",
                    vec![id.clone(), block.clone()],
                ));
                let collateral = returned(invoke(
                    &mut runtime,
                    "Contract.getCollateralId",
                    vec![id.clone()],
                ));
                let inverse =
                    returned(invoke(&mut runtime, "Contract.isInverse", vec![id.clone()]));
                let notional = returned(invoke(
                    &mut runtime,
                    "Contract.getNotionalValue",
                    vec![id.clone(), info.get("thisPrice")],
                ));
                invoke(
                    &mut runtime,
                    "Clearing.updateMarginMaps",
                    vec![
                        block,
                        id,
                        collateral,
                        inverse,
                        notional.get("notionalPerContract"),
                        info,
                    ],
                )
            } else {
                invoke(&mut runtime, &target, args)
            };
            compare(
                &result(actual),
                &case.get("outcome"),
                &format!("{name} source outcome"),
            );
        }
        assert_source_state(&runtime, &case);
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered actual effects"),
        );
        count += 1;
    }
    assert_eq!(count, 73, "genuine option adjustment observations");
}

#[test]
fn option_adjustment_failure_contexts_and_retries_restore_complete_aliases() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy;
    fn call(
        replay: &mut SourceContractReplay,
        effects: &mut Vec<V>,
        target: &str,
        arguments: Vec<V>,
    ) -> DependencyReturn {
        let receipt = replay
            .apply(C::Invoke(DependencyCall {
                target: target.into(),
                arguments,
            }))
            .unwrap();
        effects.extend(receipt.effects);
        match receipt.outcome {
            O::Dependency(v) => v,
            other => panic!("{target}: {other:?}"),
        }
    }
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::OptionLifecycleV4).unwrap();
    let mut count = 0;
    for case in reference() {
        let mut effects = vec![];
        let name = text(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            replay =
                SourceContractReplay::with_policy(SourceContractRuntimePolicy::OptionLifecycleV4)
                    .unwrap();
            for table in array(case.get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
        } else {
            replay.apply(C::SetHeight(V::number(10.))).unwrap();
            let f = command.get("failure");
            if matches!(f, V::Object(_)) {
                let op = text(f.get("op"));
                replay
                    .apply(C::FailStorage(StorageFailure {
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
                    }))
                    .unwrap();
            }
            let target = text(command.get("target"));
            let args = array(command.get("args"));
            let actual = if target == "updateFromSource" {
                let id = args[0].clone();
                let block = args[1].clone();
                let info = returned(call(
                    &mut replay,
                    &mut effects,
                    "Clearing.isPriceUpdatedForBlockHeight",
                    vec![id.clone(), block.clone()],
                ));
                let collateral = returned(call(
                    &mut replay,
                    &mut effects,
                    "Contract.getCollateralId",
                    vec![id.clone()],
                ));
                let inverse = returned(call(
                    &mut replay,
                    &mut effects,
                    "Contract.isInverse",
                    vec![id.clone()],
                ));
                let notional = returned(call(
                    &mut replay,
                    &mut effects,
                    "Contract.getNotionalValue",
                    vec![id.clone(), info.get("thisPrice")],
                ));
                call(
                    &mut replay,
                    &mut effects,
                    "Clearing.updateMarginMaps",
                    vec![
                        block,
                        id,
                        collateral,
                        inverse,
                        notional.get("notionalPerContract"),
                        info,
                    ],
                )
            } else {
                call(&mut replay, &mut effects, &target, args)
            };
            compare(
                &result(actual),
                &case.get("outcome"),
                &format!("{name} replayed genuine outcome"),
            );
        }
        let effects=effects.into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} replayed ordered effects"),
        );
        assert_source_state(replay.runtime(), &case);
        let before = replay.runtime().state_value().unwrap();
        let retained_head = replay.head();
        let bytes = replay.checkpoint().unwrap();
        replay = SourceContractReplay::from_checkpoint(&bytes, retained_head).unwrap();
        compare(
            &replay.runtime().state_value().unwrap(),
            &before,
            &format!("{name} full owner contexts/object aliases after raw replay"),
        );
        assert!(
            replay.checkpoint().unwrap() == bytes,
            "{name} canonical checkpoint"
        );
        count += 1;
    }
    assert_eq!(count, 73);
}

#[test]
fn option_adjustment_dereference_repairs_preserve_earlier_policy_behavior() {
    use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
    let cases = reference();
    let seed = &cases[0];
    for policy in [P::OnchainV1, P::GlobalClearingV2, P::SourceLifecycleV3] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        for table in array(seed.get("state").get("tables")) {
            runtime
                .import_collection(&text(table.get("name")), array(table.get("rows")))
                .unwrap();
        }
        let call = DependencyCall {
            target: "Clearing.computeOptionAdjustments".into(),
            arguments: vec![
                V::number(1.),
                s("1BitcoinEaterAddressDontSendf59kuE"),
                V::number(100.),
                V::number(10.),
                V::number(144.),
            ],
        };
        if policy == P::OnchainV1 {
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&call),
                Err(tradelayer_core::validity_source::SourceRuleError::UnknownDependency)
            );
            assert!(runtime == before);
            continue;
        }
        let actual = runtime.invoke_dependency(&call).unwrap();
        assert_eq!(
            result(actual),
            obj(&[(
                "returned",
                obj(&[
                    ("premiumMTM", V::BigNumber("0".into())),
                    ("intrinsicNet", V::BigNumber("0".into())),
                    ("maintNaked", V::BigNumber("0".into())),
                    ("optionCount", V::number(0.))
                ])
            )])
        );
        runtime
            .fail_storage(StorageFailure {
                collection: "marginMaps".into(),
                operation: "findOneAsync".into(),
                skip: 0,
                message: "legacy-adjustment-load".into(),
            })
            .unwrap();
        let before = runtime.clone();
        assert_eq!(
            runtime.invoke_dependency(&call),
            Err(tradelayer_core::validity_source::SourceRuleError::InvalidInput)
        );
        assert!(runtime == before);
    }
}
