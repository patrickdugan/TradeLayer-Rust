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
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE';
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return['array',[...v].map(([k,v])=>['array',[tag(k),tag(v)]])];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
const bal=(available,margin)=>({amount:available+margin,available,reserved:0,margin,vesting:0,channelBalance:0});
function state(){const c=e.load('clearing'),contract=e.load('contractRegistry');const source=e.state();return{tables:source.tables,tally:[...(e.load('tally').addresses??new Map())],margins:instances.map(i=>({seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]})),books:books.map((b,i)=>({id:i+1,...b})),contexts:[...keys].map(k=>[k,c.getPositionsFromCache(k)]),blockTrades:c.blockTrades,oracleMarks:c.latestOracleMarkById,nativeMarks:c.latestNativeMarkById,deleverageTrades:c.deleverageTrades,liquidationRecords:c.liquidationRecords,contractSeries:contract.instance?.contractSeries??[],insurance:source.insurance,contractMod:!!contract.modFlag,tallyMod:e.load('tally').modFlag};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))})));}
async function reset(name,options={}){e=environment({});instances=[];keys=new Set();books=[];const Book=e.load('orderbook'),loadBook=Book.prototype.loadOrderBook;Book.prototype.loadOrderBook=function(...args){if(!books.includes(this))books.push(this);return loadBook.apply(this,args);};const Margin=e.load('marginMap'),load=Margin.loadMarginMap;Margin.loadMarginMap=async function(...args){const value=await load.apply(this,args);if(value?.margins&&!instances.includes(value))instances.push(value);return value;};const c=e.load('clearing'),init=c.initPositionCache,release=c.flushPositionCache;c.initPositionCache=function(...args){const key=init.apply(this,args);keys.add(key);return key;};c.flushPositionCache=function(...args){const value=release.apply(this,args);keys.delete(args[0]);return value;};
 const meta=e.table('contractList')[0].data;meta.inverse=!!options.inverse;meta.expiryPeriod=0;meta.volAnnual=0;
 const last=100,current=options.negative?98:102;e.table('oracleData').splice(0,Infinity,{_id:'prior',oracleId:1,blockHeight:9,data:{price:last}},{_id:'current',oracleId:1,blockHeight:10,data:{price:current}});
 const shortMargin=options.partial?2:options.liquidation?0:10,shortAvail=options.liquidation||options.partial?0:100;
 const positions=[[A,{address:A,contracts:1,margin:10,unrealizedPNL:0,avgPrice:100}],[B,{address:B,contracts:-1,margin:shortMargin,unrealizedPNL:0,avgPrice:100}]];
 if(options.options)positions[1][1].options={'1-100-C-90':{contracts:1,avgPrice:0,margin:0},'invalid':{contracts:9}};
 e.table('marginMaps').splice(0,Infinity,...(options.empty?[]:[{_id:JSON.stringify({seriesId:1}),value:JSON.stringify(positions)}]));
 e.table('tallyMap')[0].data=JSON.stringify([[A,{7:bal(100,10)}],[B,{7:bal(shortAvail,shortMargin)}]]);
 await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args=[],failure){e.effects.length=0;e.setHeight(10);if(failure){if(failure.op==='getDatabase')factoryFailure(failure);else e.setFailure(failure);}let outcome;try{let value;if(target==='updateFromSource'){const c=e.load('clearing'),r=e.load('contractRegistry'),id=args[0],block=args[1],info=await c.isPriceUpdatedForBlockHeight(id,block),collateral=await r.getCollateralId(id),inverse=await r.isInverse(id),notional=await r.getNotionalValue(id,info.thisPrice);value=await c.updateMarginMaps(block,id,collateral,inverse,notional.notionalPerContract,info);}else{const[ns,method]=target.split('.');value=await e.load(ns==='Clearing'?'clearing':'contractRegistry')[method](...args);}outcome={returned:value};}catch(error){outcome={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{target,args,failure},outcome);}
(async()=>{
 await reset('no-positions',{empty:true});await call('empty-actual-source-mark','updateFromSource',[1,10]);
 await reset('no-last-price');await call('no-last-still-merges','updateFromSource',[1,10]);
 for(const options of [{},{negative:true},{inverse:true},{options:true},{liquidation:true},{partial:true}]){const name=Object.keys(options)[0]??'solvent';await reset(name,options);await call(name+'-prime','Clearing.isPriceUpdatedForBlockHeight',[1,9]);await call(name+'-update','updateFromSource',[1,10]);}
 for(const [name,collection,op]of [['loss-iou-write','iou','update'],['tally-delta-write','tallyMapDelta','insert'],['margin-merge-write','marginMaps','update']]){await reset('failure-'+name);await call(name+'-prime','Clearing.isPriceUpdatedForBlockHeight',[1,9]);await call(name+'-failed','updateFromSource',[1,10],{collection,op,message:name});await call(name+'-retry','updateFromSource',[1,10]);}
 await reset('make-first-mark');await call('make-first-price','Clearing.makeSettlement',[10]);await call('make-repeated-same-block','Clearing.makeSettlement',[10]);
 await reset('make-solvent');await call('make-prime-prior','Clearing.isPriceUpdatedForBlockHeight',[1,9]);await call('make-solvent-full','Clearing.makeSettlement',[10]);await call('make-solvent-retry','Clearing.makeSettlement',[10]);
 await reset('make-loss-iou-failure');await call('make-failure-prime','Clearing.isPriceUpdatedForBlockHeight',[1,9]);await call('make-failure-prefix','Clearing.makeSettlement',[10],{collection:'iou',op:'update',message:'make-iou'});await call('make-failure-retry-actual-cache','Clearing.makeSettlement',[10]);
 await reset('insurance-dust');for(const address of [A,B,'third'])await call('deleverage-'+address,'Clearing.recordDeleverageTrade',[1,address,{matchSize:1}]);await call('dust-missing-source-method-after-credits','Clearing.distributeInsuranceProRataToDelev',[1,7,1,10]);
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
#[test]
fn genuine_full_mark_settlement_mutates_sole_tally_margin_iou_insurance_and_books() {
    let mut runtime = SourceContractRuntime::new();
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            runtime = SourceContractRuntime::new();
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
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect();
        compare(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered actual effects"),
        );
        count += 1;
    }
    assert_eq!(count, 50, "genuine global mark observations");
}

#[test]
fn failed_global_mark_context_and_make_settlement_retry_restore_complete_aliases() {
    use tradelayer_core::source_contract_replay::{
        SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
    };
    use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy;
    fn call(
        replay: &mut SourceContractReplay,
        target: &str,
        arguments: Vec<V>,
    ) -> DependencyReturn {
        match replay
            .apply(C::Invoke(DependencyCall {
                target: target.into(),
                arguments,
            }))
            .unwrap()
            .outcome
        {
            O::Dependency(v) => v,
            other => panic!("{target}: {other:?}"),
        }
    }
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::GlobalClearingV2).unwrap();
    let mut active = false;
    let mut count = 0;
    for case in reference() {
        let name = text(case.get("name"));
        let command = case.get("command");
        if matches!(command.get("reset"), V::Object(_)) {
            active = matches!(
                name.as_str(),
                "failure-loss-iou-write" | "make-loss-iou-failure"
            );
            if !active {
                continue;
            };
            replay =
                SourceContractReplay::with_policy(SourceContractRuntimePolicy::GlobalClearingV2)
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
            if !active {
                continue;
            };
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
                    "Clearing.isPriceUpdatedForBlockHeight",
                    vec![id.clone(), block.clone()],
                ));
                let collateral = returned(call(
                    &mut replay,
                    "Contract.getCollateralId",
                    vec![id.clone()],
                ));
                let inverse = returned(call(&mut replay, "Contract.isInverse", vec![id.clone()]));
                let notional = returned(call(
                    &mut replay,
                    "Contract.getNotionalValue",
                    vec![id.clone(), info.get("thisPrice")],
                ));
                call(
                    &mut replay,
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
                call(&mut replay, &target, args)
            };
            compare(
                &result(actual),
                &case.get("outcome"),
                &format!("{name} replayed genuine outcome"),
            );
        }
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
    assert_eq!(count, 8);
}
