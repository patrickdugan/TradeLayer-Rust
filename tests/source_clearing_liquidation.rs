//! Genuine full pinned modules supply outcomes, mutations and ordered effects.
use tradelayer_core::source_contract_runtime::SourceContractRuntime;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceValue as V,
};
fn text(v: V) -> String {
    let V::String(v) = v else {
        panic!("string {v:?}")
    };
    v
}
fn arr(v: V) -> Vec<V> {
    let V::Array(v) = v else {
        panic!("array {v:?}")
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
fn observed(v: DependencyReturn) -> V {
    match v {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => obj(&[("returned", v)]),
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => obj(&[(
            "thrown",
            obj(&[
                ("name", V::String(e.name)),
                ("message", V::String(e.message)),
            ]),
        )]),
    }
}
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));code=code.replace('return {types,logic,Orderbook,effects','return {storage,tables,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};\n';const m=new Module(file,module);m.filename=file;m.paths=Module._nodeModulePaths(path.dirname(file));m._compile(code,file);const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
const A='liquidating',B='counterparty',C='second-counterparty';let e,books,ctxKeys,databases,unhandled;
// Observe rejected un-awaited source history writes without changing the
// financial method's Promise result; the failing DB event remains in the trace.
process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function copied(v){if(BN.isBigNumber(v))return new BN(v);if(Array.isArray(v))return v.map(copied);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,copied(v)]));return v;}
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){let b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Object.prototype.toString.call(v)==='[object Map]')return['array',[...v].map(([k,v])=>['array',[tag(k),tag(v)]])];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<4;i++)await new Promise(r=>setImmediate(r));}
function balance(a,m=0,r=0){return{amount:a+m+r,available:a,reserved:r,margin:m,vesting:0,channelBalance:0};}
function order(sender,amount,price,extra={}){return{sender,amount,price,contractId:1,blockTime:1,txid:'order-'+sender,initMargin:10*amount,sell:false,...extra};}
function state(){const clearing=e.load('clearing');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:[...(e.load('tally').addresses??new Map())],contexts:ctxKeys.map(k=>[k,clearing.getPositionsFromCache(k)]),deleverageTrades:clearing.deleverageTrades,liquidationRecords:clearing.liquidationRecords,unhandledRejections:unhandled,books:books.map(b=>({tickSize:b.tickSize,orderBookKey:b.orderBookKey,orderBooks:b.orderBooks,block:b.block}))};}
function emit(name,command,outcome){console.log(JSON.stringify(tag({name,command,outcome,state:state(),effects:e.effects.filter(v=>['database','db','uuid','random','rpc-clock'].includes(v.kind))})));}
async function reset(name,o={}){e=m.exports.environment(o.entry==='tally-first'?{preload:'tally'}:o.entry==='volume-first'?{preload:'volumeIndex'}:{entry:o.entry??'main-subgraph'});books=[];ctxKeys=[];databases=new Map();unhandled=[];const original=e.Orderbook.prototype.loadOrderBook;e.Orderbook.prototype.loadOrderBook=async function(...args){if(!books.includes(this))books.push(this);return original.apply(this,args);};
 const meta=e.table('contractList')[0].data;meta.inverse=!!o.inverse;
 const positions=[{address:A,contracts:o.short?-2:2,margin:o.positionMargin??20,unrealizedPNL:-10,avgPrice:100,lastMark:110},{address:B,contracts:o.short?2:-2,margin:20,unrealizedPNL:10,avgPrice:100,lastMark:110},{address:C,contracts:o.short?1:-1,margin:10,unrealizedPNL:5,avgPrice:100,lastMark:110}];
 e.table('marginMaps').push({_id:JSON.stringify({seriesId:1}),value:JSON.stringify(positions.map(p=>[p.address,p]))});
 e.table('tallyMap')[0].data=JSON.stringify([[A,{7:balance(o.available??0,o.margin??5,o.reserve??0)}],[B,{7:balance(100,20,100)}],[C,{7:balance(100,10,100)}],['amm',{7:balance(o.prune?0:1000,o.prune?0:1000)}]]);
 let buy=[],sell=[];if(o.liquidity){const side=o.short?'sell':'buy';const price=o.short?102:99;const levels=o.amm?[order('amm',.25,price,{isLiq:false}),order(B,1,price)]:[order(B,1,price)];if(side==='buy')buy=levels;else sell=levels;}
 if(o.prune){buy=[order(A,1,o.worst?110:100,{initMargin:o.worst?0:10}),order('unknown',1,100),order('amm',1,100)];}
 e.table('orderBooks').push({_id:'1',value:JSON.stringify({buy,sell})});await flush();e.effects.length=0;emit(name,{reset:o},{returned:undefined});
 await call(name+'-init','Clearing.initPositionCache',[1,10,positions]);await call(name+'-book','Orderbook.getOrderbookInstance',['1']);}
async function call(name,target,args,extra={}){e.effects.length=0;if(extra.failure)e.setFailure(extra.failure);let result,projected=copied(args);try{const ns=target.split('.')[0],method=target.slice(ns.length+1);if(ns==='db'){if(method==='getDatabase'){databases.set(args[0],await e.storage.getDatabase(...args));result={returned:{sourceDatabase:args[0]}};}else result={returned:await databases.get(args[0].sourceDatabase)[method](...args.slice(1))};}else if(ns==='Orderbook'){if(method==='getOrderbookInstance'){const book=await e.Orderbook[method](...args);if(!books.includes(book))books.push(book);result={returned:{orderbookInstance:books.indexOf(book)+1}};}else result={returned:await books[args[0].orderbookInstance-1][method](...args.slice(1))};}else if(method==='handleLiquidation'){const actual=args.slice();actual[1]=books[args[1].orderbookInstance-1];actual[2]=e.load('tally');actual[3]=e.load('clearing').getPositionsFromCache(args[0])[0];result={returned:await e.load('clearing')[method](...actual)};}else{const value=await e.load('clearing')[method](...args);if(method==='initPositionCache'&&!ctxKeys.includes(value))ctxKeys.push(value);result={returned:value};}}catch(error){result={thrown:{name:error.name,message:error.message}}}await flush();emit(name,{target,args:projected,...extra},result);}
async function handle(name,extra={}){const p=e.load('clearing').getPositionsFromCache('1:10')[0];return call(name,'Clearing.handleLiquidation',['1:10',{orderbookInstance:1},{sourceTally:true},p,1,10,!!e.table('contractList')[0].data.inverse,7,'total',10,1,100,false,0,undefined,{lastPrice:110,thisPrice:100}],extra);}
function pure(name,book,args){let result;try{result={returned:e.load('clearing')._sweepAmmLiquidityFirst(book,...args)}}catch(error){result={thrown:{name:error.name,message:error.message}}}console.log(JSON.stringify(tag({name,pure:true,args,bookBefore:currentBefore,bookAfter:book,outcome:result})));}
let currentBefore;
(async()=>{
 await reset('pure-base');const liq={address:A,contractId:1,amount:2,price:99,sell:true,txid:'liq-id'};
 const scenes=[{name:'highest-safe-amm-first',book:{buy:[order('amm',1,99),order('amm',1,101),order(B,10,102)],sell:[]},args:[liq,1.5,99,false]},
 {name:'safe-short-asks',book:{buy:[],sell:[order('amm',1,101),order('amm',1,99)]},args:[{...liq,sell:false},1.5,101,false]},
 {name:'inverse-boundary',book:{buy:[order('amm',1,98),order('amm',1,101)],sell:[]},args:[liq,2,99,true]},
 {name:'no-amm-prefixed-sender',book:{buy:[order('amm:pool',1,100),order(B,1,100)],sell:[]},args:[liq,2,99,false]},
 {name:'zero-cap',book:{buy:[order('amm',1,100)],sell:[]},args:[liq,0,99,false]},
 {name:'invalid-levels',book:{buy:[false,order('amm',1,'bad'),order('amm',Infinity,100),order('amm',1,100)],sell:[]},args:[liq,2,99,false]},
 {name:'late-null-filter-retains-level-mutation',book:{buy:[order('amm',1,100),null],sell:[]},args:[liq,1,99,false]},
 {name:'missing-side',book:{sell:[]},args:[liq,1,99,false]}];
 for(const scene of scenes){currentBefore=structuredClone(scene.book);pure(scene.name,scene.book,scene.args)}
 for(const inverse of [false,true])for(const contracts of [2,-2,0])for(const equity of [0,5])await call('bankruptcy-'+inverse+'-'+contracts+'-'+equity,'Clearing.computeLiquidationPriceFromLoss',[100,equity,contracts,1,inverse]);
 await call('actual-storage-handle','db.getDatabase',['bnStorage']);
 const bnHandle={sourceDatabase:'bnStorage'};await call('actual-big-number-storage-coefficients','db.insertAsync',[bnHandle,{_id:'bn',values:['97.5','.1','1000000000000000','1e-15','0','-0','NaN','Infinity','-Infinity'].map(x=>new BN(x))}]);
 await call('actual-big-number-nested-update','db.updateAsync',[bnHandle,{_id:'bn'},{$set:{nested:{price:new BN('123456789012345.67890123456789')}}},{upsert:true}]);await call('actual-big-number-stored-read','db.findOneAsync',[bnHandle,{_id:'bn'}]);
 for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){await reset('empty-book-adl-'+entry,{entry});await handle('actual-partial-adl-'+entry);}
 await reset('zero-equity-generates-null',{available:0,margin:0,positionMargin:0});await handle('source-liquidation-order-null');
 await reset('full-liquidation',{available:20,margin:20});await handle('source-total-size');
 await reset('short-liquidation',{short:true});await handle('source-short-adl');
 await reset('inverse-liquidation',{inverse:true});await handle('source-inverse-adl');
 await reset('orderbook-fill',{liquidity:true});await handle('source-safe-book-prefix-fill');
 await reset('amm-before-book',{liquidity:true,amm:true});await handle('source-amm-first-then-book');
 await reset('unawaited-history-write-failure',{liquidity:true});await handle('actual-unawaited-history-write-failure',{failure:{collection:'tradeHistory',op:'update',skip:1,message:'unawaited-history-write'}});
 await reset('pool-debit-failure');await handle('actual-pool-debit-delta-failure',{failure:{collection:'tallyMapDelta',op:'insert',message:'liquidation-pool-delta'}});
 await reset('adl-write-failure');await handle('actual-adl-write-failure',{failure:{collection:'liquidations',op:'update',message:'adl-write'}});
 await reset('prune-original');await call('prune-fresh-empty-book','Orderbook._pruneInstaLiqOrdersFromFreshBook',[{orderbookInstance:1},100,10,1,1,false]);
 await reset('prune-no-collateral-refund',{prune:true,available:0,margin:0,reserve:10});await call('actual-prune-refund-unknown-and-virtual-amm','Orderbook._pruneInstaLiqOrdersFromFreshBook',[{orderbookInstance:1},100,10,1,1,false]);
 await reset('prune-worst-loss-fallback',{prune:true,worst:true,available:5,margin:0,reserve:100});await call('actual-prune-fallback-initial-margin','Orderbook._pruneInstaLiqOrdersFromFreshBook',[{orderbookInstance:1},100,10,1,1,false]);
 await reset('prune-refund-fails-after-splice',{prune:true,available:0,margin:0,reserve:10});await call('actual-prune-splice-before-failed-refund','Orderbook._pruneInstaLiqOrdersFromFreshBook',[{orderbookInstance:1},100,10,1,1,false],{failure:{collection:'tallyMapDelta',op:'insert',message:'prune-refund-delta'}});
 await reset('queue-detailed-real-return-and-throw');await call('actual-queue-load-read-throws','Orderbook.loadOrderBook',[{orderbookInstance:1},'1'],{failure:{collection:'orderBooks',op:'findOne',message:'queue-book-read'}});await call('actual-queue-save-write-throws','Orderbook.saveOrderBook',[{orderbookInstance:1},{buy:[],sell:[],revision:2},'1'],{failure:{collection:'orderBooks',op:'update',message:'queue-book-write'}});await call('actual-queue-save-retry','Orderbook.saveOrderBook',[{orderbookInstance:1},{buy:[],sell:[],revision:3},'1']);
})().catch(error=>{console.error(error);process.exitCode=1});
"###;
fn run_cases() -> Vec<V> {
    let out = std::process::Command::new("node")
        .args(["-e", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases = std::str::from_utf8(&out.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            tradelayer_core::property_catalog::parse_diagnostic_value(l)
                .unwrap_or_else(|e| panic!("diagnostic {e:?}: {}", &l[..l.len().min(200)]))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cases.len(),
        101,
        "genuine liquidation observation inventory"
    );
    cases
}
#[test]
fn actual_source_amm_sweep_preserves_matching_mutation_and_thrown_prefix() {
    let mut n = 0;
    for case in run_cases() {
        if case.get("pure") != V::Bool(true) {
            continue;
        }
        let mut book = case.get("bookBefore");
        let result = tradelayer_core::source_contract_runtime::sweep_amm_liquidity_first_source(
            &mut book,
            &arr(case.get("args")),
        )
        .unwrap();
        let actual = match result {
            SourceOutcome::Returned(v) => obj(&[("returned", v)]),
            SourceOutcome::Thrown(e) => obj(&[(
                "thrown",
                obj(&[
                    ("name", V::String(e.name)),
                    ("message", V::String(e.message)),
                ]),
            )]),
        };
        assert_eq!(actual, case.get("outcome"), "{}", text(case.get("name")));
        assert_eq!(
            book,
            case.get("bookAfter"),
            "{} mutation",
            text(case.get("name"))
        );
        n += 1;
    }
    assert_eq!(n, 8)
}
#[test]
fn actual_source_liquidation_uses_real_tally_margin_queue_and_cache_owners() {
    let mut runtime = SourceContractRuntime::new();
    let mut calls = 0;
    let mut effect_errors = vec![];
    for case in run_cases() {
        if case.get("pure") == V::Bool(true) {
            continue;
        }
        let name = text(case.get("name"));
        let c = case.get("command");
        if !matches!(c.get("reset"), V::Undefined) {
            runtime = SourceContractRuntime::new();
            let entry = c.get("reset").get("entry");
            if let V::String(entry) = entry {
                runtime.set_entry(&entry).unwrap()
            }
            for table in arr(case.get("state").get("tables")) {
                runtime
                    .import_collection(&text(table.get("name")), arr(table.get("rows")))
                    .unwrap()
            }
        } else {
            if !matches!(c.get("failure"), V::Undefined) {
                let f = c.get("failure");
                runtime
                    .fail_storage(tradelayer_core::source_contract_runtime::StorageFailure {
                        collection: text(f.get("collection")),
                        operation: format!("{}Async", text(f.get("op"))),
                        skip: if let V::Number(bits) = f.get("skip") {
                            f64::from_bits(bits) as usize
                        } else {
                            0
                        },
                        message: text(f.get("message")),
                    })
                    .unwrap()
            }
            let result = runtime
                .invoke_dependency(&DependencyCall {
                    target: text(c.get("target")),
                    arguments: arr(c.get("args")),
                })
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(observed(result), case.get("outcome"), "{name} outcome");
            calls += 1;
        }
        for table in arr(case.get("state").get("tables")) {
            let collection = text(table.get("name"));
            assert_eq!(
                V::Array(runtime.collection(&collection).to_vec()),
                table.get("rows"),
                "{name} persisted {collection}"
            );
        }
        assert_eq!(
            runtime.tally_cache(),
            case.get("state").get("tally"),
            "{name} Tallycache"
        );
        for ctx in arr(case.get("state").get("contexts")) {
            let pair = arr(ctx);
            let result = runtime
                .invoke_dependency(&DependencyCall {
                    target: "Clearing.getPositionsFromCache".into(),
                    arguments: vec![pair[0].clone()],
                })
                .unwrap();
            assert_eq!(
                observed(result),
                obj(&[("returned", pair[1].clone())]),
                "{name} context"
            );
        }
        let global = runtime.state_value().unwrap().get("globalClearing");
        assert_eq!(
            global.get("deleverageTrades"),
            case.get("state").get("deleverageTrades"),
            "{name} deleverage tracking"
        );
        assert_eq!(
            global.get("liquidationRecords"),
            case.get("state").get("liquidationRecords"),
            "{name} liquidation tracking"
        );
        assert_eq!(
            global.get("unhandledRejections"),
            case.get("state").get("unhandledRejections"),
            "{name} genuine deferred rejections"
        );
        for (i, book) in arr(case.get("state").get("books")).iter().enumerate() {
            assert_eq!(
                runtime.queue().instance_value(i as u64 + 1).unwrap(),
                *book,
                "{name} queueinstance{}",
                i + 1
            )
        }
        let effects=runtime.take_effects().into_iter().filter(|v|v.get("collection")==V::String("orderBooks".into())||matches!(v.get("kind"),V::String(k)if ["database","db","uuid","random","rpc-clock"].contains(&k.as_str()))).collect::<Vec<_>>();
        let expected = arr(case.get("effects"));
        for (i, (a, b)) in effects.iter().zip(&expected).enumerate() {
            if a != b {
                effect_errors.push(format!("{name} effect{i}: {a:?} != {b:?}"));
                break;
            }
        }
        if effects.len() != expected.len() {
            effect_errors.push(format!(
                "{name} effect lengths {} != {}",
                effects.len(),
                expected.len()
            ));
        }
    }
    assert_eq!(calls, 74, "genuine Core method invocation inventory");
    assert!(effect_errors.is_empty(), "{}", effect_errors.join("\n"));
}
