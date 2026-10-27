//! Full unchanged CommonJS execution supplies funding expectations only.
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

const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
if(!code.includes('return {types,logic,Orderbook,effects'))throw Error('composed environment changed');
code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';
const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);
const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();let e,instances;
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='funding-third';
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return['array',[...v].map(([k,v])=>['array',[tag(k),tag(v)]])];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
function balance(value){return{amount:value,available:value,reserved:0,margin:0,vesting:0,channelBalance:0};}
function seed(negative=false,native=false){const meta=e.table('contractList')[0].data;meta.expiryPeriod=0;meta.native=native;
 e.table('oracleData').splice(0,Infinity,...[negative?102:100,negative?100:102].map((price,i)=>({_id:'oracle-'+i,oracleId:1,blockHeight:i*24,data:{price}})));
 e.table('volumeIndex').splice(0,Infinity,...[negative?102:100,negative?100:102].map((price,i)=>({_id:'0-7',value:{price,volume:1,blockHeight:i*24}})));
 e.table('tallyMap')[0].data=JSON.stringify([[A,{7:balance(1000)}],[C,{7:balance(1000)}],[B,{7:balance(1000)}]]);
 e.table('marginMaps').push({_id:JSON.stringify({seriesId:1}),value:JSON.stringify([[A,{address:A,contracts:1,margin:10,unrealizedPNL:0,avgPrice:100}],[C,{address:C,contracts:2,margin:20,unrealizedPNL:0,avgPrice:100}],[B,{address:B,contracts:-3,margin:30,unrealizedPNL:0,avgPrice:100}]])});
}
function state(){return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:[...(e.load('tally').addresses??new Map())],margins:instances.map(i=>({seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]}))};}
function emit(name,command,result){console.log(JSON.stringify(tag({name,command,outcome:result,state:state(),effects:e.effects.filter(x=>['database','db','uuid','random','rpc-clock'].includes(x.kind))})));}
async function reset(name,options={}){e=m.exports.environment(options.entry==='tally-first'?{preload:'tally'}:options.entry==='volume-first'?{preload:'volumeIndex'}:{entry:options.entry??'main-subgraph'});instances=[];
 const Margin=e.load('marginMap'),original=Margin.loadMarginMap;Margin.loadMarginMap=async function(...args){const result=await original.apply(this,args);if(result&&result.margins&&!instances.includes(result))instances.push(result);return result;};
 seed(!!options.negative,!!options.native);if(options.shortfall){const rows=JSON.parse(e.table('tallyMap')[0].data);rows.find(([address])=>address===B)[1][7]=balance(0);e.table('tallyMap')[0].data=JSON.stringify(rows);}
 if(options.empty)e.table('marginMaps').splice(0);
 await flush();e.effects.length=0;emit(name,{reset:options},{returned:undefined});}
async function imported(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{returned:undefined});}
function factoryFailure(f){const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=async function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});throw error;}}return original.apply(this,args);};}
async function call(name,target,args,extra={}){e.effects.length=0;if(extra.failure){if(extra.failure.op==='getDatabase')factoryFailure(extra.failure);else e.setFailure(extra.failure);}const ns=target.split('.')[0],method=target.slice(ns.length+1),modules={Clearing:'clearing',Contract:'contractRegistry',Oracle:'oracle',VolumeIndex:'volumeIndex'};let result;
 try{result={returned:await e.load(modules[ns])[method](...args)}}catch(error){result={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{target,args,...extra},result);}
(async()=>{
 for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){await reset('positive-'+entry,{entry});await call(entry+'-calculate','Clearing.calculateFundingRate',[1,24]);await call(entry+'-full-funding','Clearing.applyFundingRates',[24]);await call(entry+'-event-range','Clearing.loadFundingEvents',[1,24,24]);}
 await reset('negative-source-direction',{negative:true});await call('negative-rate','Clearing.calculateFundingRate',[1,24]);await call('negative-rate-literal-transfers','Clearing.applyFundingRates',[24]);
 await reset('native-volume-funding',{native:true});await call('native-vwap','VolumeIndex.getVWAP',[0,7,24,192]);await call('native-current-index','Clearing.getIndexPrice',[1,24]);await call('native-full-funding','Clearing.applyFundingRates',[24]);
 await call('native-index-before-first-data','Clearing.getIndexPrice',[1,-1]);await call('native-index-missing-contract','Clearing.getIndexPrice',[99,24]);
 await imported('native-produced-volume-shape','volumeIndex',[{_id:'0-7',value:{price:102,rawVolume:1,ltcVolume:1,blockHeight:24,type:'contract'}}]);
 await call('native-source-vwap-missing-volume-is-nan','VolumeIndex.getVWAP',[0,7,24,192]);await call('native-source-nan-vwap-skips-funding','Clearing.applyFundingRates',[24]);
 await reset('periodic-filter-and-clamp');await call('non-period-no-financial-reads','Clearing.applyFundingRates',[23]);
 for(const value of [0,-0,4.99,-4.99,5,-5,5.01,-5.01,100,-100,Infinity,-Infinity,NaN,'bad',new BN('1e400'),'0x10','-0b101','0o10','\ufeff10','\u008510',[10]])await call('clamp-'+String(value),'Clearing.clampFundingRate',[value]);
 await imported('large-deviation-oracle','oracleData',[{_id:'old',oracleId:1,blockHeight:0,data:{price:1}},{_id:'latest',oracleId:1,blockHeight:24,data:{price:100}}]);await call('positive-rate-cap','Clearing.calculateFundingRate',[1,24]);
 await imported('negative-large-deviation','oracleData',[{_id:'old',oracleId:1,blockHeight:0,data:{price:100}},{_id:'latest',oracleId:1,blockHeight:24,data:{price:1}}]);await call('negative-rate-cap','Clearing.calculateFundingRate',[1,24]);
 await imported('missing-twap-data','oracleData',[]);await call('missing-twap','Oracle.getTWAP',[1,24,192]);await call('missing-twap-zero-rate','Clearing.calculateFundingRate',[1,24]);
 await imported('nan-twap-data','oracleData',[{_id:'bad-price',oracleId:1,blockHeight:24,data:{price:'bad'}}]);await call('nan-twap','Oracle.getTWAP',[1,24,192]);await call('nan-twap-zero-rate','Clearing.calculateFundingRate',[1,24]);
 await imported('malformed-twap-entry','oracleData',[{_id:'null-data',oracleId:1,blockHeight:24,data:null}]);await call('twap-catches-dereference','Oracle.getTWAP',[1,24,192]);
 await reset('funding-empty-map',{empty:true});await call('empty-still-reads-index-notional','Clearing.applyFundingToPositions',[1,1,24]);
 await reset('funding-invalid-direct-rates');for(const rate of [0,-0,NaN,Infinity,-Infinity])await call('direct-rate-'+String(rate),'Clearing.applyFundingToPositions',[1,rate,24]);
 await reset('payer-shortfall-prefix',{shortfall:true});await call('payer-two-shortfall-retains-first-debit','Clearing.processFundingPayments',[[{address:A,contracts:1},{address:B,contracts:1}],[{address:C,contracts:2}],1,1,24]);
 await reset('receiver-empty-retains-debits');await call('no-receivers-after-debit','Clearing.processFundingPayments',[[{address:A,contracts:1}],[],.1,1,24]);
 await reset('separate-side-rounding');await call('three-payers-zero-rounding-one-credit','Clearing.processFundingPayments',[[{address:A,contracts:1},{address:C,contracts:1},{address:B,contracts:1}],[{address:B,contracts:3}],.00000001,1,24]);
 for(const total of [0,-0,Infinity,NaN])await call('nonfinite-or-zero-total-'+String(total),'Clearing.processFundingPayments',[null,null,total,1,24]);
 await call('zero-payer-contracts-return-after-collateral-read','Clearing.processFundingPayments',[[{address:A,contracts:0}],[{address:B,contracts:1}],.1,1,24]);
 await call('null-payers-reduce-throws-after-collateral-read','Clearing.processFundingPayments',[null,[],.1,1,24]);
 await call('bad-receivers-retain-payer-debit','Clearing.processFundingPayments',[[{address:A,contracts:1}],{},.1,1,24]);
 await reset('funding-event-write-is-caught');await call('payments-retained-event-write-fails','Clearing.applyFundingRates',[24],{failure:{collection:'fundingEvents',op:'update',message:'funding-event-write'}});await call('load-events-after-failed-write','Clearing.loadFundingEvents',[1,0,24]);
 await call('event-explicit-save','Clearing.saveFundingEvent',[1,-.125,24]);await call('event-replace-same-id','Clearing.saveFundingEvent',[1,.25,24]);await call('event-range-excludes','Clearing.loadFundingEvents',[1,0,23]);await call('event-range-inclusive','Clearing.loadFundingEvents',[1,24,24]);
 await call('events-find-failure-caught','Clearing.loadFundingEvents',[1,0,24],{failure:{collection:'fundingEvents',op:'find',message:'funding-event-find'}});
 await call('events-factory-failure-caught','Clearing.saveFundingEvent',[1,.5,48],{failure:{collection:'fundingEvents',op:'getDatabase',message:'funding-event-factory'}});
 await reset('funding-failures');await call('registry-find-caught-to-no-contracts','Clearing.applyFundingRates',[24],{failure:{collection:'contractList',op:'find',message:'perpetual-list-read'}});
 await call('twap-db-failure-zero-rate','Clearing.calculateFundingRate',[1,24],{failure:{collection:'oracleData',op:'find',message:'twap-read'}});
 await call('twap-factory-failure-caught','Oracle.getTWAP',[1,24,192],{failure:{collection:'oracleData',op:'getDatabase',message:'twap-factory'}});
 await call('position-load-factory-failure-propagates','Clearing.applyFundingToPositions',[1,1,24],{failure:{collection:'marginMaps',op:'getDatabase',message:'funding-margin-factory'}});
 await call('payer-delta-failure-retains-prefix','Clearing.processFundingPayments',[[{address:A,contracts:1}],[{address:B,contracts:1}],.1,1,24],{failure:{collection:'tallyMapDelta',op:'insert',message:'funding-delta-write'}});
 await reset('perpetual-selector');await imported('registry-selector-input','contractList',[
 {_id:'p1',id:1,type:'contractSeries',data:{id:1,expiryPeriod:0,ticker:'TEST'}},
 {_id:'p2',id:2,type:'contractSeries',data:{id:'two',expiryPeriod:1,ticker:'XPERP'}},
 {_id:'p3',id:3,type:'contractSeries',data:{id:3,expiryPeriod:'0',ticker:'perp'}},
 {_id:'p4',id:4,type:'contractSeries',data:{id:4,expiryPeriod:9,ticker:['PERP']}}
 ]);await call('actual-perpetual-selector','Contract.getAllPerpContracts',[]);
 await imported('selector-null-data','contractList',[{_id:'bad',type:'contractSeries',data:null}]);await call('selector-catches-null-data','Contract.getAllPerpContracts',[]);
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
fn unchanged_source_funding_graph_drives_actual_tally_margin_storage_and_prefixes() {
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
        let effects=runtime.take_effects().into_iter().filter(|v|matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect::<Vec<_>>();
        compare_value(
            &V::Array(effects),
            &case.get("effects"),
            &format!("{name} ordered effects"),
        );
        actions += 1;
    }
    assert_eq!(actions, 104, "funding source inventory changed");
    assert_eq!(calls, 81, "funding calls changed");
}
