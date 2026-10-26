use tradelayer_core::contract_placement_source::{
    add_contract_order, insert_order, is_valid_order, is_valid_orderbook, match_contract_orders,
    ContractPlacementBackend,
};
use tradelayer_core::contract_queue_source_owner::{ContractQueueSourceOwner, QueueDbFailure};
use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
fn s(t: &str) -> V {
    V::String(t.into())
}
fn n(x: f64) -> V {
    V::number(x)
}
fn o(f: &[(&str, V)]) -> V {
    V::Object(f.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn arr(v: &V) -> Vec<V> {
    if let V::Array(a) = v {
        a.clone()
    } else {
        panic!("array")
    }
}
fn txt(v: &V) -> String {
    if let V::String(t) = v {
        t.clone()
    } else {
        panic!("string")
    }
}
fn num(v: &V) -> f64 {
    if let V::Number(b) = v {
        f64::from_bits(*b)
    } else {
        panic!("number")
    }
}
fn decode(hex: &str) -> V {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    tradelayer_core::property_catalog::parse_diagnostic_value(std::str::from_utf8(&bytes).unwrap())
        .unwrap()
}
fn outcome(v: SourceOutcome) -> V {
    match v {
        SourceOutcome::Returned(v) => o(&[("returned", v)]),
        SourceOutcome::Thrown(e) => o(&[(
            "thrown",
            o(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn diff(a: &V, b: &V, path: &str) -> String {
    match (a, b) {
        (V::Object(a), V::Object(b)) if a.len() == b.len() => {
            for ((ka, a), (kb, b)) in a.iter().zip(b) {
                if ka != kb {
                    return format!("{path}: {ka}/{kb}");
                }
                if a != b {
                    return diff(a, b, &format!("{path}.{ka}"));
                }
            }
        }
        (V::Array(a), V::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                if a != b {
                    return diff(a, b, &format!("{path}[{i}]"));
                }
            }
        }
        _ => (),
    };
    format!("{path}: {a:?} / {b:?}")
        .chars()
        .take(1500)
        .collect()
}
// Entire pinned class, unchanged placement/insert/match/shape methods. The
// declared dependencies calculate or reject at explicit external boundaries;
// no fixture supplies matches, books, or a fabricated settlement response.
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.argv[1],source=fs.readFileSync(path.join(root,'fixtures/legacy_orderbook_source.txt'),'utf8').replace(/\r\n/g,'\n');assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98');const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
function tag(x){if(x===undefined)return['undefined'];if(x===null)return['null'];if(typeof x==='string'||typeof x==='boolean')return[typeof x,x];if(typeof x==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(x);return['number',Number.isNaN(x)?'NaN':x===Infinity?'Infinity':x===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(x))return['bignumber',x.isZero()&&x.isNegative()?'-0':x.toFixed()];if(Array.isArray(x))return['array',x.map(tag)];return['object',Object.keys(x).map(k=>[k,tag(x[k])])];}const hex=x=>Buffer.from(JSON.stringify(tag(x))).toString('hex');
let rows,instances,ids,events,config,positions,counts,external,failures,Orderbook,lastBook;
function observe(i){if(!ids.has(i)){ids.set(i,instances.length+1);instances.push(i);}return ids.get(i);}
function record(target,args){events.push({target,arguments:structuredClone(args)});}
function dependency(target,args){record(target,args);const count=(counts[target]=(counts[target]||0)+1);if(config.rejectTarget===target&&config.rejectAt===count)throw Error('injected '+target);if(target==='Margin.loadMarginMap')return Object.hasOwn(config,'load')?config.load:{seriesId:args[0]};if(target==='Margin.getPositionForAddress')return positions[args[0]]||{contracts:0,margin:0,unrealizedPNL:0};if(target==='Contract.moveCollateralToReserve'){const value=Object.hasOwn(config,'reserve')?config.reserve:new BN(args[2]).times(args[3]).div(10).decimalPlaces(8).toNumber();external.push({target,amount:args[2],returned:value,txid:args[5]});return value;}if(target==='Contract.getInitialMargin')return Object.hasOwn(config,'initial')?config.initial:new BN(args[1]).div(10).decimalPlaces(8).toNumber();if(target==='Orderbook.processContractMatches'){external.push({target,arguments:structuredClone(args)});return undefined;}throw Error('unknown '+target);}
function dbcall(operation,query,update,options){events.push({collection:'orderBooks',operation,query:structuredClone(query),update:structuredClone(update),options:structuredClone(options)});let i=failures.findIndex(f=>f.operation===operation);if(i>=0){if(failures[i].skip===0){const f=failures.splice(i,1)[0];throw Error(f.message);}failures[i].skip--;}}
const db={getDatabase:async name=>{assert.equal(name,'orderBooks');dbcall('getDatabase');return{findOneAsync:async q=>{dbcall('findOneAsync',q);return structuredClone(rows.find(r=>r._id===q._id)||null);},updateAsync:async(q,u,p)=>{dbcall('updateAsync',q,u,p);const i=rows.findIndex(r=>r._id===q._id);if(i<0)rows.push(structuredClone(u));else rows[i]=structuredClone(u);return 1;},loadDatabase:async()=>{dbcall('loadDatabase');}};}};
function reset(){rows=[];instances=[];ids=new WeakMap();events=[];config={};positions={};counts={};external=[];failures=[];lastBook=undefined;const module={exports:{}};vm.runInNewContext(source,{module,exports:module.exports,console:{log(){},warn(){},error(){}},require(name){if(name==='bignumber.js')return BN;if(name==='./db.js'||name==='./db')return db;if(name==='./contractRegistry.js')return{getInitialMargin:async(...a)=>dependency('Contract.getInitialMargin',a),moveCollateralToReserve:async(...a)=>dependency('Contract.moveCollateralToReserve',a)};if(name==='./marginMap.js')return{loadMarginMap:async(...a)=>{const loaded=dependency('Margin.loadMarginMap',a);if(loaded===null||typeof loaded!=='object')return loaded;return{getPositionForAddress:async(...args)=>dependency('Margin.getPositionForAddress',args)};}};if(name==='uuid')return{};return{};}},{timeout:2000});Orderbook=module.exports;const load=Orderbook.prototype.loadOrderBook;Orderbook.prototype.loadOrderBook=async function(...a){observe(this);return load.apply(this,a);};Orderbook.prototype.processContractMatches=async function(...a){return dependency('Orderbook.processContractMatches',[{orderbookInstance:observe(this)},...a]);};}
function state(){return{queue:{orderBooks:rows,instances:instances.map(i=>[observe(i),{tickSize:i.tickSize,orderBookKey:i.orderBookKey,orderBooks:i.orderBooks,block:i.block}])},lastBook,external};}
let scene='',count=0;async function action(label,c){const command=hex(c);events=[];let result;try{let value;switch(c.op){case'reset':reset();break;case'import':rows=structuredClone(c.rows);break;case'positions':positions=c.value;break;case'config':config=c.value;counts={};break;case'failure':failures.push(c.value);break;case'instance':value=observe(new Orderbook(c.key));break;case'queue':value=await Orderbook.queueOnChainContractOrder(...c.arguments);break;case'drain':value=await Orderbook.processQueuedOnChainOrdersForBlock(c.height);break;case'add':{const args=c.arguments.slice();const h=args[12];if(h)args[12]=instances[h.orderbookInstance-1];value=await Orderbook.prototype.addContractOrder(...args);break;}case'match':lastBook=c.book;value=await Orderbook.prototype.matchContractOrders(lastBook);break;case'insert':lastBook=c.book;value=await Orderbook.prototype.insertOrder(c.order,lastBook,c.sell,c.liq);break;case'valid':value=Orderbook.prototype.isValidOrder(c.order,c.contract);break;case'validbook':value=Orderbook.prototype.isValidOrderbook.call({isValidOrder:Orderbook.prototype.isValidOrder},c.book,c.contract);break;default:throw Error(c.op);}result={returned:value};}catch(e){result={thrown:{name:e.name,message:e.message}};}process.stdout.write([scene+'/'+label,command,hex(result),hex(state()),hex(events)].join('\t')+'\n');count++;}
const start=async name=>{scene=name;await action('reset',{op:'reset'});};const imp=(label,book)=>action(label,{op:'import',rows:[{_id:'1',value:typeof book==='string'?book:JSON.stringify(book)}]});const conf=(label,value)=>action(label,{op:'config',value});
const order=(sender,amount=1,price=100,blockTime=10,extra={})=>({contractId:1,amount,price,blockTime,sell:sender==='seller',initMargin:10*amount,sender,txid:sender+'-id',isLiq:false,reduce:false,post:false,stop:10,initialReduce:false,...extra});const add=(label,sender='buyer',sell=false,amount=1,extra={})=>action(label,{op:'add',arguments:[1,100,amount,sell,'ignored-insurance',10,label,sender,extra.isLiq??false,extra.reduce??false,extra.post??false,extra.stop??10,extra.handle]});
async function main(){
await start('real-books-and-shifted-flags');await action('existing-instance',{op:'instance',key:'1'});await add('unmatched-buyer','buyer',false,3,{isLiq:'shifted-liq',reduce:'shifted-reduce',post:'shifted-post',stop:99,handle:{orderbookInstance:1}});await add('partial-seller','seller',true,1,{handle:{orderbookInstance:1}});await add('second-seller','seller-two',true,2,{handle:{orderbookInstance:1}});await add('same-id-not-deduplicated','buyer',false,1,{handle:{orderbookInstance:1}});await add('same-id-not-deduplicated','buyer',false,1,{handle:{orderbookInstance:1}});
await start('queue-to-real-placement');await action('queue-buyer',{op:'queue',arguments:[1,'buyer',{contractId:1,price:100,amount:2,sell:false,blockTime:10,isLiq:false,reduce:false,post:false,stop:10},10,'original-queued-buy']});await action('queue-seller',{op:'queue',arguments:[1,'seller',{contractId:1,price:100,amount:1,sell:true,blockTime:10,isLiq:false,reduce:false,post:true,stop:10},10,'original-queued-sell']});await action('drain-computes-matches',{op:'drain',height:10});await action('queue-second-seller',{op:'queue',arguments:[1,'seller-two',{contractId:1,price:100,amount:1,sell:true,blockTime:11,isLiq:false,reduce:false,post:false,stop:11},11,'second-seller']});await action('drain-saved-resting-order',{op:'drain',height:11});
await start('queued-settlement-throw-and-retry');await conf('reject-settlement',{rejectTarget:'Orderbook.processContractMatches',rejectAt:1});await action('queue-buy',{op:'queue',arguments:[1,'buyer',{contractId:1,price:100,amount:1,sell:false,blockTime:10},10,'buy-id']});await action('queue-sell',{op:'queue',arguments:[1,'seller',{contractId:1,price:100,amount:1,sell:true,blockTime:10},10,'sell-id']});await action('late-reject-keeps-queue',{op:'drain',height:10});await conf('settlement-restored',{});await action('retry-original-queued-ids',{op:'drain',height:10});
await start('reduce-flip-and-reserve-failure');await action('positions',{op:'positions',value:{long:{contracts:2},short:{contracts:-2}}});await add('reduce-long','long',true,1);await add('reduce-short','short',false,1);await add('flip-long','long',true,5);await add('flip-short','short',false,5);await conf('reserve-zero',{reserve:0});await add('zero-opening-skipped');await add('zero-flip-skipped','long',true,5);await conf('reserve-negative',{reserve:-1});await add('negative-skipped');await conf('reserve-nan',{reserve:NaN});await add('nan-skipped');await conf('reserve-string',{reserve:'1.5'});await add('string-reserve-kept');
await start('maker-post-market-self-trade');for(const [label,buy,sell]of[
 ['earlier-seller',order('buyer',1,102,12),order('seller',1,100,10)],['earlier-buyer',order('buyer',1,102,10),order('seller',1,100,12)],['same-block-sell-post',order('buyer',1,102,10),order('seller',1,100,10,{post:true})],['same-block-buy-post',order('buyer',1,102,10,{post:true}),order('seller',1,100,10)],['same-block-no-maker',order('buyer',1,102,10),order('seller',1,100,10)],['buy-market',order('buyer',1,1,10,{isMarket:true}),order('seller',1,100,12)],['sell-market',order('buyer',1,1,12),order('seller',1,100,10,{isMarket:true})],['both-market',order('buyer',1,1,12,{isMarket:true}),order('seller',1,100,10,{isMarket:true})],['self-seller-maker',order('same',1,102,12),order('same',1,100,10)],['self-same-block-removes-buy',order('same',1,102,10),order('same',1,100,10)],['no-cross',order('buyer',1,99),order('seller',1,100)]])await action(label,{op:'match',book:{buy:[buy],sell:[sell],extra:'retained'}});
await start('source-sort-zero-recursion-and-strings');await action('price-time-sort',{op:'match',book:{buy:[order('buyer-a',1,100,12),order('buyer-b',1,101,10),order('buyer-c',1,101,9)],sell:[order('seller-a',1,101,12),order('seller-b',0,99,10),order('seller-c',1,100,11)]}});await action('partial-fixed-margin',{op:'match',book:{buy:[order('buyer',3.5,'100.000000005',10)],sell:[order('seller',1.25,'100.000000005',10)]}});await conf('bad-initial-margin',{initial:NaN});await action('nan-initial-default-zero',{op:'match',book:{buy:[order('buyer')],sell:[order('seller')]}});await conf('zero-initial-margin',{initial:0});await action('zero-margin',{op:'match',book:{buy:[order('buyer')],sell:[order('seller')]}});await conf('numeric-margin-string',{initial:'0.000000015'});await action('bigdecimal-rounding',{op:'match',book:{buy:[order('buyer',2)],sell:[order('seller',1)]}});await conf('reset-config',{});await action('negative-amounts-unrepaired',{op:'match',book:{buy:[order('buyer',-1)],sell:[order('seller',1)]}});
await start('special-number-and-parser-domains');await conf('explicit-external-initial',{initial:1});for(const [label,ba,sa,bp,sp]of[['number-infinity',Infinity,1,100,100],['string-infinity','Infinity',1,100,100],['nan-price',1,1,NaN,100],['hex-binary-octal','0x2','0b1','0x64','0o144'],['signed-zero-price',1,1,-0,0],['underflow-amount','1e-400',1,100,100]])await action(label,{op:'match',book:{buy:[order('buyer',ba,bp)],sell:[order('seller',sa,sp)]}});await action('sort-throw-preserves-original-order',{op:'match',book:{buy:[order('b1',1,102),order('b2',1,101),order('b3',1,103),null],sell:[order('seller',1)]}});for(const load of[undefined,null,3]){await conf('missing-map-instance',{load});await add('map-method-lookup-throws');}
await action('large-consistent-sort',{op:'match',book:{buy:Array.from({length:64},(_,i)=>({contractId:1,amount:1,price:100+(i%3),blockTime:10+(i%5),sender:'buyer-'+i,txid:'id-'+i})),sell:[order('seller',1,100)]}});await start('insert-and-validators');await action('time-not-blocktime',{op:'insert',order:{txid:'new',time:5},book:{buy:[{txid:'later',time:6},{txid:'earlier',time:3}],sell:[]},sell:false});await action('absent-time-appends',{op:'insert',order:{blockTime:1},book:{buy:[{blockTime:100}],sell:[]},sell:false});await action('json-string-insert',{op:'insert',order:order('buyer'),book:'{"buy":[],"sell":[],"negativeZero":-0,"overflow":1e400}',sell:false});await action('malformed-string-undefined',{op:'insert',order:order('buyer'),book:'{bad',sell:false});await action('falsy-empty',{op:'insert',order:order('buyer'),book:null,sell:false});await action('wrong-side-findindex',{op:'insert',order:order('buyer'),book:{buy:{}},sell:false});await action('generated-order-contract-shape-false',{op:'valid',order:order('buyer'),contract:true});const contractShape={contractId:1,amount:1,side:false,initMargin:10,txid:'id',isLiq:false,reduce:false,post:false};const tokenShape={offeredPropertyId:7,desiredPropertyId:8,amountOffered:1,amountExpected:1,blockTime:10,sender:'a',price:1};await action('actual-contract-shape',{op:'valid',order:contractShape,contract:true});await action('nan-fields-still-number',{op:'valid',order:{...contractShape,amount:NaN},contract:null});await action('token-shape',{op:'valid',order:tokenShape,contract:false});await action('array-every-index-zero-token-branch',{op:'validbook',book:{buy:[contractShape],sell:[]},contract:true});await action('array-every-index-one-contract-branch',{op:'validbook',book:{buy:[tokenShape,contractShape],sell:[]},contract:false});await action('missing-side-log-throws',{op:'validbook',book:{buy:[]},contract:true});await action('null-order-throws',{op:'valid',order:null,contract:true});await action('shadowed-order-method',{op:'valid',order:{hasOwnProperty:false},contract:true});await action('shadowed-book-method',{op:'validbook',book:{hasOwnProperty:null,buy:[],sell:[]},contract:true});
await start('failure-prefix-cache-and-persistence');await imp('saved-opposite-order',{buy:[],sell:[order('seller',1)]});await conf('late-settlement-reject',{rejectTarget:'Orderbook.processContractMatches',rejectAt:1});await add('saved-book-before-settlement-rejection');await conf('reserve-throws',{rejectTarget:'Contract.moveCollateralToReserve',rejectAt:1});await add('before-book-load');await conf('two-match-initial-reject',{rejectTarget:'Contract.getInitialMargin',rejectAt:2});await imp('two-sellers',{buy:[],sell:[order('seller-a',1),order('seller-b',1)]});await add('cache-mutated-book-not-saved','buyer',false,2);await conf('clear-config',{});await action('save-fault',{op:'failure',value:{operation:'updateAsync',skip:0,message:'injected updateAsync'}});await add('save-reject-after-cache-mutation');await imp('missing-sell-array',{buy:[]});await add('insert-cache-prefix-before-shape-error');await imp('invalid-json','{bad');await add('caught-load-produces-new-book');await imp('parsed-string',JSON.stringify('{broken'));await add('insert-parsing-fails-returns-undefined');
process.stderr.write(JSON.stringify({actions:count}));}
main().catch(e=>{console.error(e);process.exitCode=1;});
"###;
struct Backend {
    config: V,
    positions: V,
    counts: std::collections::BTreeMap<String, usize>,
    events: Vec<V>,
    external: Vec<V>,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            config: o(&[]),
            positions: o(&[]),
            counts: Default::default(),
            events: vec![],
            external: vec![],
        }
    }
}
impl Backend {
    fn collect(&mut self, queue: &mut ContractQueueSourceOwner) {
        self.events
            .extend(queue.take_db_calls().into_iter().map(|c| c.value()));
    }
}
impl SourceBackend for Backend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.events.push(o(&[
            ("target", s(&call.target)),
            ("arguments", V::Array(call.arguments.clone())),
        ]));
        let count = self.counts.entry(call.target.clone()).or_default();
        *count += 1;
        if self.config.get("rejectTarget") == s(&call.target)
            && self.config.get("rejectAt") == n(*count as f64)
        {
            return Ok(DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message: format!("injected {}", call.target),
            }));
        }
        let a = &call.arguments;
        let result = match call.target.as_str() {
            "Margin.loadMarginMap" => {
                if let V::Object(fields) = &self.config {
                    fields
                        .iter()
                        .find(|(k, _)| k == "load")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(|| o(&[("seriesId", a[0].clone())]))
                } else {
                    o(&[("seriesId", a[0].clone())])
                }
            }
            "Margin.getPositionForAddress" => {
                let v = self.positions.get(&txt(&a[0]));
                if matches!(v, V::Undefined) {
                    o(&[
                        ("contracts", n(0.0)),
                        ("margin", n(0.0)),
                        ("unrealizedPNL", n(0.0)),
                    ])
                } else {
                    v
                }
            }
            "Contract.moveCollateralToReserve" => {
                let v = if let V::Object(f) = &self.config {
                    f.iter()
                        .find(|(k, _)| k == "reserve")
                        .map(|(_, v)| v.clone())
                } else {
                    None
                };
                let v = v.unwrap_or_else(|| {
                    let a = Decimal::from(num(&a[2]) as u64);
                    let price = num(&call.arguments[3])
                        .to_string()
                        .parse::<Decimal>()
                        .unwrap();
                    n(a.checked_mul(&price)
                        .unwrap()
                        .checked_div(&10u32.into(), 20, Rounding::HalfUp)
                        .unwrap()
                        .quantize(8, Rounding::HalfUp)
                        .unwrap()
                        .to_string()
                        .parse()
                        .unwrap())
                });
                self.external.push(o(&[
                    ("target", s(&call.target)),
                    ("amount", a[2].clone()),
                    ("returned", v.clone()),
                    ("txid", a[5].clone()),
                ]));
                v
            }
            "Contract.getInitialMargin" => {
                let v = if let V::Object(f) = &self.config {
                    f.iter()
                        .find(|(k, _)| k == "initial")
                        .map(|(_, v)| v.clone())
                } else {
                    None
                };
                v.unwrap_or_else(|| {
                    let price = match &a[1] {
                        V::Number(_) => num(&a[1]).to_string(),
                        V::String(t) => t.clone(),
                        _ => panic!("price"),
                    }
                    .parse::<Decimal>()
                    .unwrap();
                    n(price
                        .checked_div(&10u32.into(), 20, Rounding::HalfUp)
                        .unwrap()
                        .quantize(8, Rounding::HalfUp)
                        .unwrap()
                        .to_string()
                        .parse()
                        .unwrap())
                })
            }
            "Orderbook.processContractMatches" => {
                self.external.push(o(&[
                    ("target", s(&call.target)),
                    ("arguments", V::Array(a.clone())),
                ]));
                V::Undefined
            }
            _ => return Err(SourceRuleError::UnknownDependency),
        };
        Ok(DependencyReturn::Pending(result))
    }
}
impl ContractPlacementBackend for Backend {
    fn invoke_in_queue(
        &mut self,
        queue: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        self.collect(queue);
        self.invoke(call)
    }
}
#[test]
fn complete_pinned_placement_match_insert_and_shape_methods_replay_against_one_mutable_queue() {
    let root = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let output = std::process::Command::new("node")
        .arg("-e")
        .arg(ORACLE)
        .arg(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut queue = ContractQueueSourceOwner::new();
    let mut backend = Backend::default();
    let mut last_book = V::Undefined;
    let mut count = 0;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        assert_eq!(f.len(), 5);
        let case = f[0];
        let command = decode(f[1]);
        backend.events.clear();
        let result = match txt(&command.get("op")).as_str() {
            "reset" => {
                queue = ContractQueueSourceOwner::new();
                backend = Backend::default();
                last_book = V::Undefined;
                SourceOutcome::Returned(V::Undefined)
            }
            "import" => {
                queue.import_rows(arr(&command.get("rows"))).unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "positions" => {
                backend.positions = command.get("value");
                SourceOutcome::Returned(V::Undefined)
            }
            "config" => {
                backend.config = command.get("value");
                backend.counts.clear();
                SourceOutcome::Returned(V::Undefined)
            }
            "failure" => {
                let v = command.get("value");
                queue
                    .fail_database_call(QueueDbFailure {
                        operation: txt(&v.get("operation")),
                        skip: num(&v.get("skip")) as usize,
                        exception: SourceException {
                            name: "Error".into(),
                            message: txt(&v.get("message")),
                        },
                    })
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "instance" => SourceOutcome::Returned(n(queue
                .create_instance(command.get("key"), None)
                .unwrap() as f64)),
            "queue" => queue
                .evaluate(&SourceCall {
                    method: "queueOnChainContractOrder".into(),
                    arguments: arr(&command.get("arguments")),
                })
                .unwrap(),
            "drain" => queue
                .evaluate_with_capability(
                    &SourceCall {
                        method: "processQueuedOnChainOrdersForBlock".into(),
                        arguments: vec![command.get("height")],
                    },
                    &mut |queue, call| {
                        assert_eq!(call.target, "Orderbook.addContractOrder");
                        Ok(
                            match add_contract_order(queue, &call.arguments, &mut backend)? {
                                SourceOutcome::Returned(v) => DependencyReturn::Pending(v),
                                SourceOutcome::Thrown(e) => DependencyReturn::PendingThrow(e),
                            },
                        )
                    },
                )
                .unwrap_or_else(|e| panic!("{case}: {e:?}")),
            "add" => add_contract_order(&mut queue, &arr(&command.get("arguments")), &mut backend)
                .unwrap_or_else(|e| panic!("{case}: {e:?}")),
            "match" => {
                last_book = command.get("book");
                match_contract_orders(&mut last_book, &mut backend)
                    .unwrap_or_else(|e| panic!("{case}: {e:?}"))
            }
            "insert" => {
                last_book = command.get("book");
                insert_order(
                    &command.get("order"),
                    &mut last_book,
                    &command.get("sell"),
                    &command.get("liq"),
                )
                .unwrap()
            }
            "valid" => is_valid_order(&command.get("order"), &command.get("contract")).unwrap(),
            "validbook" => {
                is_valid_orderbook(&command.get("book"), &command.get("contract")).unwrap()
            }
            _ => panic!("op"),
        };
        backend.collect(&mut queue);
        let actual = outcome(result);
        let expected = decode(f[2]);
        assert!(
            actual == expected,
            "result {case}: {}",
            diff(&actual, &expected, "$")
        );
        let actual = o(&[
            ("queue", queue.state_value()),
            ("lastBook", last_book.clone()),
            ("external", V::Array(backend.external.clone())),
        ]);
        let expected = decode(f[3]);
        assert!(
            actual == expected,
            "state {case}: {}",
            diff(&actual, &expected, "$")
        );
        let expected = decode(f[4]);
        let actual = V::Array(backend.events.clone());
        assert!(
            actual == expected,
            "ordered effects {case}: {}",
            diff(&actual, &expected, "$")
        );
        let bytes = queue.encode().unwrap();
        queue = ContractQueueSourceOwner::decode(&bytes).unwrap();
        assert!(queue.encode().unwrap() == bytes);
        count += 1;
    }
    assert_eq!(count, 109, "complete source action corpus");
}
#[test]
fn absent_dependency_cannot_invent_a_reserve_or_a_match() {
    struct Missing;
    impl SourceBackend for Missing {
        fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            Err(SourceRuleError::UnknownDependency)
        }
    }
    impl ContractPlacementBackend for Missing {}
    let mut q = ContractQueueSourceOwner::new();
    let result = add_contract_order(
        &mut q,
        &[
            n(1.0),
            n(100.0),
            n(1.0),
            V::Bool(false),
            V::Bool(false),
            n(10.0),
            s("id"),
            s("a"),
        ],
        &mut Missing,
    );
    assert_eq!(result, Err(SourceRuleError::UnknownDependency));
    assert!(q.rows().is_empty());
    assert_eq!(arr(&q.state_value().get("instances")).len(), 1);
}

// Real Tally integration: the reference runs complete pinned tally.js and the
// Rust adapter forwards to the source runtime's one authoritative Tally owner.
const RECLAIM_ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');const root=process.argv[1];const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
function source(file,sha){const text=fs.readFileSync(path.join(root,'fixtures',file),'utf8').replace(/\r\n/g,'\n');assert.equal(crypto.createHash('sha256').update(text).digest('hex'),sha);return text;}
const bookSource=source('legacy_orderbook_source.txt','c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98'),tallySource=source('native_genesis/source/tally.js','cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef'),contractSource=source('legacy_contract_registry_source.txt','d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121');
function tag(x){if(x===undefined)return['undefined'];if(x===null)return['null'];if(typeof x==='string'||typeof x==='boolean')return[typeof x,x];if(typeof x==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(x);return['number',Number.isNaN(x)?'NaN':x===Infinity?'Infinity':x===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(x))return['bignumber',x.isZero()&&x.isNegative()?'-0':x.toFixed()];if(Array.isArray(x))return['array',x.map(tag)];return['object',Object.keys(x).map(k=>[k,tag(x[k])])];}const hex=x=>Buffer.from(JSON.stringify(tag(x))).toString('hex');
const order=(id,margin,price)=>({contractId:id,amount:1,price,blockTime:10,sell:false,initMargin:margin,sender:'alice',txid:'original-'+price,isLiq:false,reduce:false,post:false});
const one=(id,orders)=>({_id:String(id),value:JSON.stringify({buy:orders,sell:[]})});const balance=(available,reserved)=>({_id:'tallyMap',data:JSON.stringify([['alice',{'7':{amount:available+reserved,available,reserved,margin:0,vesting:0,channelBalance:0}}]])});
const scenes=[
 {name:'sufficient-early-return-repeats-original-orders',books:[one(1,[order(1,4,90),order(1,8,100)])],tally:balance(5,30),contracts:[],actions:[{method:'cancelExcessOrders',args:['alice',1,new BN(10),7,10]},{method:'cancelExcessOrders',args:['alice',1,new BN(10),7,11]}]},
 {name:'insufficient-total-saves-book',books:[one(1,[order(1,4,90),order(1,8,100)])],tally:balance(5,30),contracts:[],actions:[{method:'cancelExcessOrders',args:['alice',1,new BN(50),7,10]}]},
 {name:'real-tally-partial-negative-reserve',books:[one(1,[order(1,8,100)])],tally:balance(5,5),contracts:[],actions:[{method:'cancelExcessOrders',args:['alice',1,new BN(10),7,10]}]},
 {name:'cross-global-missing-after-credit',books:[one(2,[order(2,15,100)])],tally:balance(0,100),contracts:[{id:1},{id:2,collateralPropertyId:99}],actions:[{method:'sourceCrossContractReserve',args:['alice',7,new BN(10),1,10]}]},
 {name:'cross-global-present',ambient:77,books:[one(2,[order(2,15,100)])],tally:balance(0,100),contracts:[{id:1},{id:2}],actions:[{method:'sourceCrossContractReserve',args:['alice',7,new BN(10),1,10]}]},
 {name:'cross-multiple-insufficient-books',ambient:77,books:[one(2,[order(2,4,100)]),one(3,[order(3,6,100)])],tally:balance(0,100),contracts:[{id:2},{id:3}],actions:[{method:'sourceCrossContractReserve',args:['alice',7,new BN(20),1,10]}]},
 {name:'malformed-cancellation-sell-side',books:[{_id:'1',value:'{"buy":[],"sell":null}'}],tally:balance(5,30),contracts:[],actions:[{method:'cancelExcessOrders',args:['alice',1,new BN(10),7,10]}]},
 {name:'contract-query-caught',failure:true,books:[],tally:balance(5,30),contracts:[{id:2}],actions:[{method:'sourceCrossContractReserve',args:['alice',7,new BN(20),1,10]}]}
];
async function main(){for(const scene of scenes){let uuid=0,Orderbook,Tally,Contract,instances=[],ids=new WeakMap(),calls=[];const tables=new Map([['orderBooks',structuredClone(scene.books)],['tallyMap',[structuredClone(scene.tally)]],['contractList',scene.contracts.map((data,i)=>({_id:'contract-'+i,type:'contractSeries',data}))]]);const rows=name=>{if(!tables.has(name))tables.set(name,[]);return tables.get(name);};
const db={getDatabase:async name=>({findOneAsync:async q=>structuredClone(rows(name).find(r=>Object.keys(q).every(k=>r[k]===q[k]))||null),findAsync:async q=>{if(scene.failure&&name==='contractList')throw Error('injected contract list');return structuredClone(rows(name).filter(r=>Object.keys(q).every(k=>r[k]===q[k])));},updateAsync:async(q,u,opts)=>{let r=rows(name).find(r=>r._id===q._id);if(!r){r={...q};rows(name).push(r);}if(u.$set)Object.assign(r,structuredClone(u.$set));else{const i=rows(name).indexOf(r);rows(name)[i]=structuredClone(u);}return 1;},insertAsync:async row=>{rows(name).push(structuredClone(row));return row;}})};
const modules={};function load(name,text){const module={exports:{}};const context={module,exports:module.exports,console:{log(){},error(){},warn(){}},require(name){if(name==='bignumber.js')return BN;if(name==='./db.js'||name==='./db')return db;if(name==='uuid')return{v4:()=> 'source-contract-'+(++uuid)};if(name==='./tally.js')return Tally;if(name==='./contractRegistry.js')return Contract;if(name==='./orderbook.js')return Orderbook;return{};}};Object.defineProperty(context,'block',{get(){calls.push({target:'SourceGlobal.get',arguments:['block']});if(!Object.hasOwn(scene,'ambient'))throw ReferenceError('block is not defined');return scene.ambient;}});vm.runInNewContext(text,context,{timeout:2000});return module.exports;}
Orderbook=load('orderbook',bookSource);Contract=load('contract',contractSource);Tally=load('tally',tallySource);const oldLoad=Orderbook.prototype.loadOrderBook;Orderbook.prototype.loadOrderBook=async function(...args){if(!ids.has(this)){ids.set(this,instances.length+1);instances.push(this);}return oldLoad.apply(this,args);};for(const [owner,method,target]of[[Contract,'getAllContracts','Contract.getAllContracts'],[Tally,'getTally','Tally.getTally'],[Tally,'updateBalance','Tally.updateBalance']]){const original=owner[method];owner[method]=async function(...args){calls.push({target,arguments:structuredClone(args)});return original.apply(this,args);};}
const setup={books:scene.books,tally:scene.tally,contracts:rows('contractList'),ambient:scene.ambient,failure:!!scene.failure};process.stdout.write([scene.name+'/initial',hex(setup)].join('\t')+'\n');
for(let i=0;i<scene.actions.length;i++){const command=scene.actions[i];calls=[];let result;try{result={returned:await Orderbook[command.method](...command.args)};}catch(e){result={thrown:{name:e.name,message:e.message}};}await Promise.resolve();const state={queue:{orderBooks:rows('orderBooks'),instances:instances.map(instance=>[ids.get(instance),{tickSize:instance.tickSize,orderBookKey:instance.orderBookKey,orderBooks:instance.orderBooks,block:instance.block}])},tallyMap:rows('tallyMap'),tallyCache:Array.from(Tally.addresses||[]),deltas:rows('tallyMapDelta')};process.stdout.write([scene.name+'/'+i,hex(command),hex(result),hex(state)].join('\t')+'\n');}}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
"###;
struct ActualTallyBackend {
    runtime: tradelayer_core::source_contract_runtime::SourceContractRuntime,
    ambient: V,
}
impl SourceBackend for ActualTallyBackend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == "SourceGlobal.get" {
            return Ok(if matches!(self.ambient, V::Undefined) {
                DependencyReturn::PendingThrow(SourceException {
                    name: "ReferenceError".into(),
                    message: "block is not defined".into(),
                })
            } else {
                DependencyReturn::Pending(self.ambient.clone())
            });
        }
        self.runtime.invoke_dependency(call)
    }
}
impl ContractPlacementBackend for ActualTallyBackend {}
#[test]
fn cancellation_and_cross_reserve_use_genuine_tally_owners_and_preserve_both_source_defects() {
    let root = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let output = std::process::Command::new("node")
        .arg("-e")
        .arg(RECLAIM_ORACLE)
        .arg(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut queue = ContractQueueSourceOwner::new();
    let mut backend = ActualTallyBackend {
        runtime: tradelayer_core::source_contract_runtime::SourceContractRuntime::new(),
        ambient: V::Undefined,
    };
    let mut count = 0;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let case = f[0];
        if case.ends_with("/initial") {
            let setup = decode(f[1]);
            queue = ContractQueueSourceOwner::new();
            queue.import_rows(arr(&setup.get("books"))).unwrap();
            backend = ActualTallyBackend {
                runtime: tradelayer_core::source_contract_runtime::SourceContractRuntime::new(),
                ambient: setup.get("ambient"),
            };
            backend
                .runtime
                .import_collection("tallyMap", vec![setup.get("tally")])
                .unwrap();
            backend
                .runtime
                .import_collection("contractList", arr(&setup.get("contracts")))
                .unwrap();
            if setup.get("failure") == V::Bool(true) {
                backend
                    .runtime
                    .fail_storage(tradelayer_core::source_contract_runtime::StorageFailure {
                        collection: "contractList".into(),
                        operation: "findAsync".into(),
                        skip: 0,
                        message: "injected contract list".into(),
                    })
                    .unwrap();
            }
            continue;
        }
        let command = decode(f[1]);
        let args = arr(&command.get("args"));
        let actual = if command.get("method") == s("cancelExcessOrders") {
            tradelayer_core::contract_placement_source::cancel_excess_orders(
                &mut queue,
                &args,
                &mut backend,
            )
        } else {
            tradelayer_core::contract_placement_source::source_cross_contract_reserve(
                &mut queue,
                &args,
                &mut backend,
            )
        }
        .unwrap_or_else(|e| panic!("{case}: {e:?}"));
        let expected = decode(f[2]);
        let actual = outcome(actual);
        assert!(
            actual == expected,
            "outcome {case}: {}",
            diff(&actual, &expected, "$")
        );
        let actual = o(&[
            ("queue", queue.state_value()),
            (
                "tallyMap",
                V::Array(backend.runtime.collection("tallyMap").to_vec()),
            ),
            ("tallyCache", backend.runtime.tally_cache()),
            (
                "deltas",
                V::Array(backend.runtime.collection("tallyMapDelta").to_vec()),
            ),
        ]);
        let expected = decode(f[3]);
        assert!(
            actual == expected,
            "actual Tally and queue state {case}: {}",
            diff(&actual, &expected, "$")
        );
        assert!(
            backend.runtime.queue().rows().is_empty(),
            "only the separately supplied queue owns orderBooks"
        );
        queue = ContractQueueSourceOwner::decode(&queue.encode().unwrap()).unwrap();
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn local_value_capacity_and_large_unsupported_comparators_restore_the_input_book() {
    let mut book = o(&[
        ("buy", V::Array(vec![o(&[("time", n(1.0))]); 1024])),
        ("sell", V::Array(vec![])),
    ]);
    let before = book.clone();
    assert_eq!(
        insert_order(
            &o(&[("time", n(2.0))]),
            &mut book,
            &V::Bool(false),
            &V::Bool(false)
        ),
        Err(SourceRuleError::Capacity)
    );
    assert!(book == before);
    let mut book = o(&[
        (
            "buy",
            V::Array(vec![
                o(&[
                    ("price", n(f64::NAN)),
                    ("amount", n(1.0)),
                    ("blockTime", n(1.0))
                ]);
                64
            ]),
        ),
        (
            "sell",
            V::Array(vec![o(&[("price", n(1.0)), ("amount", n(1.0))])]),
        ),
    ]);
    let before = book.clone();
    assert_eq!(
        match_contract_orders(&mut book, &mut Backend::default()),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert!(book == before);
}
