use tradelayer_core::contract_queue_source_owner::{
    ContractQueueSourceOwner, QueueDbFailure, MAX_ROWS, METHODS,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceCall, SourceException, SourceOutcome, SourceRuleError,
    SourceValue as V,
};
fn s(v: &str) -> V {
    V::String(v.into())
}
fn n(v: f64) -> V {
    V::number(v)
}
fn obj(v: &[(&str, V)]) -> V {
    V::Object(v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}
fn text(v: &V) -> String {
    if let V::String(v) = v {
        v.clone()
    } else {
        panic!("expected string")
    }
}
fn array(v: &V) -> Vec<V> {
    if let V::Array(v) = v {
        v.clone()
    } else {
        panic!("expected array")
    }
}
fn num(v: &V) -> f64 {
    if let V::Number(v) = v {
        f64::from_bits(*v)
    } else {
        panic!("expected Number")
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
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}
fn returned(v: SourceOutcome) -> V {
    if let SourceOutcome::Returned(v) = v {
        v
    } else {
        panic!("expected returned")
    }
}
fn difference(a: &V, b: &V, path: &str) -> String {
    if let (V::Object(a), V::Object(b)) = (a, b) {
        if a.len() != b.len() {
            return format!("{path}: object lengths {} vs {}", a.len(), b.len());
        }
        for ((ka, a), (kb, b)) in a.iter().zip(b) {
            if ka != kb {
                return format!("{path}: key {ka} vs {kb}");
            }
            if a != b {
                return difference(a, b, &format!("{path}.{ka}"));
            }
        }
    }
    if let (V::Array(a), V::Array(b)) = (a, b) {
        if a.len() != b.len() {
            return format!("{path}: array lengths {} vs {}", a.len(), b.len());
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            if a != b {
                return difference(a, b, &format!("{path}[{i}]"));
            }
        }
    }
    format!("{path}: {:?} vs {:?}", a, b)
        .chars()
        .take(1024)
        .collect()
}

// The entire hash-pinned module executes. Original queue/load/save methods are
// untouched. Only out-of-scope placement is replaced by a declared capability
// (return/reject/real source save), with no invented matches or balance effects.
const QUEUE_ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.argv[1],source=fs.readFileSync(path.join(root,'fixtures/legacy_orderbook_source.txt'),'utf8').replace(/\r\n/g,'\n');
assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98');
const BigNumber=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber(),clone=x=>structuredClone(x);
let Orderbook,documents,instances,ids,dbCalls,placementCalls,failures,cap,external;
function diagnostic(x){if(x===undefined)return['undefined'];if(x===null)return['null'];if(typeof x==='boolean')return['boolean',x];if(typeof x==='string')return['string',x];if(typeof x==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(x);return['number',Number.isNaN(x)?'NaN':x===Infinity?'Infinity':x===-Infinity?'-Infinity':b.toString('hex')];}if(x instanceof BigNumber)return['bignumber',x.isZero()&&x.isNegative()?'-0':x.toFixed()];if(Array.isArray(x))return['array',Array.from(x,diagnostic)];if(typeof x==='object')return['object',Object.keys(x).map(k=>[k,diagnostic(x[k])])];throw Error('Unsupported value '+typeof x);}
const hex=x=>Buffer.from(JSON.stringify(diagnostic(x))).toString('hex');
function observe(instance){if(!ids.has(instance)){ids.set(instance,instances.length+1);instances.push(instance);}return ids.get(instance);}
function handle(instance){return{orderbookInstance:observe(instance)};}
function state(){return{orderBooks:documents,instances:instances.map(i=>[ids.get(i),{tickSize:i.tickSize,orderBookKey:i.orderBookKey,orderBooks:i.orderBooks,block:i.block}])};}
function dbcall(operation,query,update,options){dbCalls.push({collection:'orderBooks',operation,query:clone(query),update:clone(update),options:clone(options)});const at=failures.findIndex(f=>f.operation===operation);if(at>=0){if(failures[at].skip===0){const f=failures.splice(at,1)[0],e=Error(f.message);e.name=f.name;throw e;}failures[at].skip--;}}
const db={getDatabase:async name=>{assert.equal(name,'orderBooks');dbcall('getDatabase');return{findOneAsync:async q=>{dbcall('findOneAsync',q);return clone(documents.find(d=>d._id===q._id)||null);},updateAsync:async(q,u,o)=>{dbcall('updateAsync',q,u,o);const i=documents.findIndex(d=>d._id===q._id);if(i>=0)documents[i]=clone(u);else if(o?.upsert)documents.push(clone(u));return i>=0?1:0;},loadDatabase:async()=>{dbcall('loadDatabase');}};}};
async function placement(instance,target,args){const original=args.slice();if(target==='Orderbook.addContractOrder')original[12]=handle(original[12]);else original.push(handle(instance));placementCalls.push({target,arguments:clone(original)});cap.count++;external.push({target,attempt:cap.count,txid:target==='Orderbook.addContractOrder'?args[6]:args[2]});if(cap.write)await instance.saveOrderBook({buy:[],sell:[],externalCapability:true,attempt:cap.count},instance.orderBookKey);if(cap.rejectAt===cap.count)throw Error('injected placement rejection');return undefined;}
function reset(){documents=[];instances=[];ids=new WeakMap();dbCalls=[];placementCalls=[];failures=[];cap={count:0,write:false,rejectAt:0};external=[];const module={exports:{}};vm.runInNewContext(source,{module,exports:module.exports,console:{log(){},warn(){},error(){}},require(name){if(name==='bignumber.js')return BigNumber;if(name==='./db.js')return db;if(name==='uuid')return{v4(){throw Error('uuid outside queue surface');}};return new Proxy({}, {get(_o,k){throw Error('Outside queue dependency '+name+'.'+String(k));}});}},{filename:'pinned-orderbook.js',timeout:2000});Orderbook=module.exports;const load=Orderbook.prototype.loadOrderBook;Orderbook.prototype.loadOrderBook=async function(...args){observe(this);return load.apply(this,args);};Orderbook.prototype.addContractOrder=async function(...args){return placement(this,'Orderbook.addContractOrder',args);};Orderbook.prototype.addTokenOrder=async function(...args){return placement(this,'Orderbook.addTokenOrder',args);};}
let scene='';const methods=new Set();let count=0;
async function action(label,command){dbCalls=[];placementCalls=[];let result;try{let value;if(command.op==='reset')reset();else if(command.op==='import')documents=clone(command.rows);else if(command.op==='failure')failures.push({...command.failure});else if(command.op==='capability')cap={count:0,write:command.write,rejectAt:command.rejectAt};else if(command.op==='restart'){const rows=clone(documents);reset();documents=rows;}else if(command.op==='new-instance'){const i=Object.hasOwn(command,'tickSize')?new Orderbook(command.key,command.tickSize):new Orderbook(command.key);value=observe(i);}else{assert(['call','instance-call'].includes(command.op));methods.add(command.method);const target=command.op==='call'?Orderbook:instances[command.instance-1];value=await target[command.method](...command.arguments);if(command.method==='getOrderbookInstance')value=handle(value);if(command.method==='_getOrderbookDB')value={sourceDatabase:'orderBooks'};}result={returned:value};}catch(e){result={thrown:{name:e.name,message:e.message}};}process.stdout.write([scene+'/'+label,hex(command),hex(result),hex(state()),hex(dbCalls),hex(placementCalls),hex(external)].join('\t')+'\n');count++;}
const start=async name=>{scene=name;await action('reset',{op:'reset'});};
const call=(label,method,...args)=>action(label,{op:'call',method,arguments:args});
const inst=(label,instance,method,...args)=>action(label,{op:'instance-call',instance,method,arguments:args});
const fail=(label,operation,skip=0)=>action(label,{op:'failure',failure:{operation,skip,name:'Error',message:'injected '+operation}});
const imported=(label,rows)=>action(label,{op:'import',rows});
const capability=(label,write=false,rejectAt=0)=>action(label,{op:'capability',write,rejectAt});
const params=(overrides={})=>({contractId:1,price:'100.000000005',amount:2,sell:false,insurance:true,blockTime:10,isLiq:0,reduce:'reduce-original',post:true,stop:false,txid:'nested-original-id',extra:{clearlists:[3,'4'],keep:undefined},...overrides});
async function main(){
await start('books-and-instances');await call('db','_getOrderbookDB');await action('new',{op:'new-instance',key:7});await inst('missing-does-not-cache',1,'loadOrderBook',7);await inst('save-json-bridges',1,'saveOrderBook',{buy:[{price:new BigNumber('1e21'),negativeZero:new BigNumber('-0'),extra:undefined}],sell:[],n:NaN,z:-0,holes:[undefined,Infinity]},7);await inst('load-caches',1,'loadOrderBook',7);await inst('save-does-not-refresh-cache',1,'saveOrderBook',{buy:[],sell:[],version:2},'7');await call('fresh-instance','getOrderbookInstance','7');await imported('malformed-json',[{_id:'7',value:'{bad json'}]);await inst('parse-error-keeps-cache',1,'loadOrderBook',7);await call('parse-error-new-empty-cache','getOrderbookInstance',7);await imported('parsed-null',[{_id:'null',value:'null'},{_id:'number',value:5},{_id:'falsy',value:false}]);await inst('parsed-null',1,'loadOrderBook','null');await inst('number-json-coercion',1,'loadOrderBook','number');await inst('falsy-empty',1,'loadOrderBook','falsy');await inst('save-undefined',1,'saveOrderBook',undefined,'undefined');await action('custom-tick',{op:'new-instance',key:'custom',tickSize:'original tick'});await action('restart',{op:'restart'});await call('restored-book','getOrderbookInstance','number');
await start('json-number-parse-bits');await imported('source-json-numbers',[{_id:'zero',value:'-0'},{_id:'nested-zero',value:'{"buy":[],"sell":[],"n":-0}'},{_id:'overflow',value:'1e400'},{_id:'negative-overflow',value:'-1e400'},{_id:'number-array',value:'[-1e-4000,1e-4000,1e400,-1e400,0.1,9007199254740993]'},{_id:'fields',value:'{"2":-0,"1":1e400,"n":1,"n":-0,"__proto__":{"n":-0}}'}]);await call('root-negative-zero','getOrderbookInstance','zero');await call('nested-negative-zero','getOrderbookInstance','nested-zero');await call('root-overflow','getOrderbookInstance','overflow');await call('negative-overflow','getOrderbookInstance','negative-overflow');await call('nested-overflow-and-underflow','getOrderbookInstance','number-array');await call('duplicate-integer-and-own-proto-fields','getOrderbookInstance','fields');
await start('contract-token-sort-full-params');await call('contract-c','queueOnChainContractOrder',1,'address-c',params({contractId:'shifted-id',insurance:'shift-insurance',isLiq:'source-liq'}),'10','tx-c');await call('contract-a','queueOnChainContractOrder','1','address-a',params(),10,'tx-b');await call('contract-a-tie','queueOnChainContractOrder',1,'address-a',params({stop:'preserve-stop'}),10,'tx-a');await call('token-earlier','queueOnChainTokenOrder',1,'address-z',{offeredPropertyId:7,desiredPropertyId:8,amountOffered:'2',unknown:{retained:true}},9,'token-id');await call('future','queueOnChainContractOrder',1,'address-a',params({amount:3}),11,'future');await call('replay-same-id-preserved','queueOnChainContractOrder',1,'address-a',params({post:false}),10,'tx-a');await capability('placement-can-write',true);await call('ready-through-10','processQueuedOnChainOrdersForBlock',10);await call('future-untouched','processQueuedOnChainOrdersForBlock',10);await call('future-through-string-height','processQueuedOnChainOrdersForBlock','11');await call('empty-active-list','processQueuedOnChainOrdersForBlock',12);
await start('sender-locale-and-txid-ties');await imported('addresses',[{_id:'activePairs',pairs:['locale']},{_id:'queue-locale',orders:['addr-1','addr-z','A','a','aA','Aa','aaa','bA','1','0','','aa-a','a_a','prefix-Abc','prefix-abc'].map((sender,i)=>({kind:'contract',blockHeight:1,sender,txid:'id-'+i,params:params()})).concat(['B','b','a','A','a_a','a-a','0',''].map(txid=>({kind:'contract',blockHeight:1,sender:'same-address',txid,params:params()})))}]);await call('sort-case-punctuation-missing-and-id-ties','processQueuedOnChainOrdersForBlock',1);
await start('earlier-pair-cleared-later-pair-fails');await call('first-pair','queueOnChainContractOrder','first','a',params(),1,'original-first');await call('second-pair','queueOnChainContractOrder','second','b',params({contractId:2}),1,'original-second');await capability('capability-rejects-second-pair',true,2);await call('first-queue-cleared-second-keeps-active-list','processQueuedOnChainOrdersForBlock',1);await capability('retry-second-only',true);await call('retry-clears-missing-first-and-second','processQueuedOnChainOrdersForBlock',1);
await start('reference-active-pairs-and-bignumber-coercion');await call('number-object-key','queueOnChainContractOrder',new BigNumber('1e-8'),'a',params(),new BigNumber('20'),'bn-small');await call('negative-zero-key','queueOnChainContractOrder',new BigNumber('-0'),'a',params(),1,'bn-zero');await call('bn-key-instance','getOrderbookInstance',new BigNumber('1e-8'));await call('finish-numeric-object-keys','processQueuedOnChainOrdersForBlock',20);await call('reference-identity-pairs','_updateActivePairs',[{}, {}, ['1'], ['1']]);await call('remove-reference-pairs','processQueuedOnChainOrdersForBlock',20);
await start('mixed-typed-active-pairs');await call('typed-number','_addActivePair',1);await call('typed-string','_addActivePair','1');await call('number-repeat','_addActivePair',1);await call('queue-one','queueOnChainContractOrder',1,'last-a',params(),1,'id');await call('process-duplicate-queue-key','processQueuedOnChainOrdersForBlock',1);await call('arbitrary-update','_updateActivePairs',['missing','other']);await call('missing-queues-cleaned','processQueuedOnChainOrdersForBlock',5);await imported('bad-active-pairs',[{_id:'activePairs',pairs:'not array'}]);await call('bad-list-skipped','processQueuedOnChainOrdersForBlock',1);
await start('nan-future-unknown-and-malformed');await imported('mixed-entries',[{_id:'activePairs',pairs:['1']},{_id:'queue-1',orders:[{kind:'unknown',blockHeight:null,sender:'a',txid:'unknown'},{kind:'contract',blockHeight:'not-number',sender:'a',txid:'nan',params:params()},{kind:'contract',blockHeight:4,sender:'a',txid:'future',params:params()}],extra:'source replaces row'}]);await call('unknown-dropped-nan-future','processQueuedOnChainOrdersForBlock',2);await call('nan-current-no-ready','processQueuedOnChainOrdersForBlock',NaN);await call('future-with-nan-retained','processQueuedOnChainOrdersForBlock',4);await imported('null-queued-entry',[{_id:'activePairs',pairs:['1']},{_id:'queue-1',orders:[null]}]);await call('null-entry-throws','processQueuedOnChainOrdersForBlock',10);await imported('null-contract-params',[{_id:'activePairs',pairs:['1']},{_id:'queue-1',orders:[{kind:'contract',blockHeight:1,sender:'a',txid:'id',params:null}]}]);await call('params-throws-after-cache-load','processQueuedOnChainOrdersForBlock',10);await imported('null-sender-tie',[{_id:'activePairs',pairs:['1']},{_id:'queue-1',orders:[{kind:'contract',blockHeight:1,sender:'a',txid:'id',params:params()},{kind:'contract',blockHeight:1,sender:null,txid:'other',params:params()}]}]);await call('sort-throws-before-instance','processQueuedOnChainOrdersForBlock',10);
await start('queue-partial-db-failures');await fail('queue-reload-fails','loadDatabase');await call('queue-write-survives','queueOnChainContractOrder',1,'a',params(),1,'same');await call('retry-adds-duplicate','queueOnChainContractOrder',1,'a',params(),1,'same');await fail('active-pair-update-fails','updateAsync',1);await call('second-queue-orphaned','queueOnChainContractOrder',2,'a',params({contractId:2}),1,'other');await call('only-first-pair-active','processQueuedOnChainOrdersForBlock',1);await call('repair-by-source-retry-only','queueOnChainContractOrder',2,'a',params({contractId:2}),1,'other');await call('process-duplicate-second','processQueuedOnChainOrdersForBlock',1);await action('restart',{op:'restart'});
await start('late-placement-failure');await call('queue-a','queueOnChainContractOrder',1,'a',params(),1,'a');await call('queue-b','queueOnChainContractOrder',1,'b',params(),1,'b');await capability('save-before-reject',true,2);await call('second-placement-rejects-queue-uncleared','processQueuedOnChainOrdersForBlock',1);await capability('retry-real-capability',true,0);await call('retry-both-original-ids','processQueuedOnChainOrdersForBlock',1);
await start('late-db-failures');await call('queue-a','queueOnChainContractOrder',1,'a',params(),1,'a');await call('queue-b','queueOnChainContractOrder',1,'b',params(),1,'b');await capability('placement-no-book-writes');await fail('queue-clear-fails','updateAsync');await call('placements-before-failed-clear','processQueuedOnChainOrdersForBlock',1);await fail('final-active-update-fails','updateAsync',1);await call('queue-cleared-active-still-old','processQueuedOnChainOrdersForBlock',1);await call('retry-empty-no-placement','processQueuedOnChainOrdersForBlock',1);await call('queue-post-load','queueOnChainContractOrder',3,'a',params({contractId:3}),1,'post-load');await capability('placement-writes-book',true);await fail('post-load-database-fails','getDatabase',3);await call('placement-before-getdatabase-failure','processQueuedOnChainOrdersForBlock',1);await call('retry-post-load','processQueuedOnChainOrdersForBlock',1);
await start('book-db-catch-boundaries');await action('instance',{op:'new-instance',key:'1'});await fail('find-fails','findOneAsync');await inst('load-catches-find',1,'loadOrderBook','1');await fail('db-fails','getDatabase');await inst('load-does-not-catch-getdb',1,'loadOrderBook','1');await fail('save-update-fails','updateAsync');await inst('save-rethrows',1,'saveOrderBook',{buy:[],sell:[]},'1');await call('failed-get-instance-before-db','getOrderbookInstance',{toString:1});
assert.equal(methods.size,9);process.stderr.write(JSON.stringify({actions:count,methods:[...methods].sort()}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
"###;

#[derive(Default)]
struct Capability {
    count: usize,
    write: bool,
    reject_at: usize,
    external: Vec<V>,
}
impl Capability {
    fn invoke(
        &mut self,
        owner: &mut ContractQueueSourceOwner,
        call: &DependencyCall,
    ) -> Result<DependencyReturn, SourceRuleError> {
        let contract = call.target == "Orderbook.addContractOrder";
        assert!(contract || call.target == "Orderbook.addTokenOrder");
        let handle = call.arguments.last().unwrap();
        let id = num(&handle.get("orderbookInstance")) as u64;
        assert_eq!(call.arguments.len(), if contract { 13 } else { 4 });
        self.count += 1;
        self.external.push(obj(&[
            ("target", s(&call.target)),
            ("attempt", n(self.count as f64)),
            ("txid", call.arguments[if contract { 6 } else { 2 }].clone()),
        ]));
        if self.write {
            let key = owner.instance_value(id)?.get("orderBookKey");
            let value = obj(&[
                ("buy", V::Array(vec![])),
                ("sell", V::Array(vec![])),
                ("externalCapability", V::Bool(true)),
                ("attempt", n(self.count as f64)),
            ]);
            match owner.evaluate_instance(
                id,
                &SourceCall {
                    method: "saveOrderBook".into(),
                    arguments: vec![value, key],
                },
            )? {
                SourceOutcome::Returned(_) => (),
                SourceOutcome::Thrown(e) => return Ok(DependencyReturn::PendingThrow(e)),
            }
        }
        Ok(if self.reject_at == self.count {
            DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message: "injected placement rejection".into(),
            })
        } else {
            DependencyReturn::Pending(V::Undefined)
        })
    }
}
#[test]
fn unchanged_pinned_queue_methods_replay_all_documents_instances_calls_and_partial_effects() {
    let root = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let output = std::process::Command::new("node")
        .arg("-e")
        .arg(QUEUE_ORACLE)
        .arg(&root)
        .output()
        .expect("Node is required for the pinned JS queue oracle");
    assert!(
        output.status.success(),
        "queue JS oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut owner = ContractQueueSourceOwner::new();
    let mut capability = Capability::default();
    let mut methods = std::collections::BTreeSet::new();
    let mut count = 0;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 7);
        let case = fields[0];
        let command = decode(fields[1]);
        let op = text(&command.get("op"));
        let result = match op.as_str() {
            "reset" => {
                owner = ContractQueueSourceOwner::new();
                capability = Capability::default();
                SourceOutcome::Returned(V::Undefined)
            }
            "import" => {
                owner.import_rows(array(&command.get("rows"))).unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "failure" => {
                let f = command.get("failure");
                owner
                    .fail_database_call(QueueDbFailure {
                        operation: text(&f.get("operation")),
                        skip: num(&f.get("skip")) as usize,
                        exception: SourceException {
                            name: text(&f.get("name")),
                            message: text(&f.get("message")),
                        },
                    })
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "capability" => {
                capability.count = 0;
                capability.write = command.get("write") == V::Bool(true);
                capability.reject_at = num(&command.get("rejectAt")) as usize;
                SourceOutcome::Returned(V::Undefined)
            }
            "restart" => {
                owner =
                    ContractQueueSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap())
                        .unwrap();
                capability = Capability::default();
                SourceOutcome::Returned(V::Undefined)
            }
            "new-instance" => {
                let tick = command.get("tickSize");
                let id = owner
                    .create_instance(
                        command.get("key"),
                        if matches!(tick, V::Undefined) {
                            None
                        } else {
                            Some(tick)
                        },
                    )
                    .unwrap();
                SourceOutcome::Returned(n(id as f64))
            }
            "call" | "instance-call" => {
                let method = text(&command.get("method"));
                methods.insert(method.clone());
                let call = SourceCall {
                    method,
                    arguments: array(&command.get("arguments")),
                };
                if op == "instance-call" {
                    owner.evaluate_instance(num(&command.get("instance")) as u64, &call)
                } else {
                    owner.evaluate_with_capability(&call, &mut |owner, call| {
                        capability.invoke(owner, call)
                    })
                }
                .unwrap_or_else(|e| panic!("{case}: {e:?}"))
            }
            _ => panic!("unknown action"),
        };
        assert!(outcome(result) == decode(fields[2]), "outcome: {case}");
        let expected = decode(fields[3]);
        let actual = owner.state_value();
        assert!(
            actual == expected,
            "full owner state: {case}: {}",
            difference(&actual, &expected, "$")
        );
        assert!(
            V::Array(owner.take_db_calls().iter().map(|c| c.value()).collect())
                == decode(fields[4]),
            "DB calls: {case}"
        );
        assert!(
            V::Array(
                owner
                    .take_placement_calls()
                    .into_iter()
                    .map(|c| obj(&[
                        ("target", s(&c.target)),
                        ("arguments", V::Array(c.arguments))
                    ]))
                    .collect()
            ) == decode(fields[5]),
            "placement boundary: {case}"
        );
        assert!(
            V::Array(capability.external.clone()) == decode(fields[6]),
            "earlier external effects: {case}"
        );
        let bytes = owner.checkpoint().unwrap();
        let restored = ContractQueueSourceOwner::from_checkpoint(&bytes).unwrap();
        assert!(restored == owner, "checkpoint owner equality: {case}");
        assert!(
            restored.checkpoint().unwrap() == bytes,
            "canonical checkpoint: {case}"
        );
        owner = restored;
        count += 1;
    }
    assert_eq!(count, 117, "full source corpus must remain complete");
    assert_eq!(methods, METHODS.into_iter().map(str::to_string).collect());
}

fn call(owner: &mut ContractQueueSourceOwner, method: &str, args: Vec<V>) -> SourceOutcome {
    owner
        .evaluate(&SourceCall {
            method: method.into(),
            arguments: args,
        })
        .unwrap()
}
#[test]
fn absent_capability_never_manufactures_placement_or_clears_pending_queue() {
    let mut owner = ContractQueueSourceOwner::new();
    returned(call(
        &mut owner,
        "queueOnChainContractOrder",
        vec![
            n(1.0),
            s("a"),
            obj(&[("contractId", n(1.0)), ("amount", n(1.0))]),
            n(1.0),
            s("original"),
        ],
    ));
    let rows = owner.rows().to_vec();
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "processQueuedOnChainOrdersForBlock".into(),
            arguments: vec![n(1.0)]
        }),
        Err(SourceRuleError::UnknownDependency)
    );
    assert!(owner.rows() == rows);
    assert_eq!(owner.take_placement_calls().len(), 1);
    assert!(!array(&owner.state_value().get("instances")).is_empty());
}
#[test]
fn complete_recovery_and_durable_restart_preserve_their_distinct_cache_and_fault_lifecycles() {
    let mut owner = ContractQueueSourceOwner::new();
    let instance = owner.create_instance(s("1"), None).unwrap();
    returned(
        owner
            .evaluate_instance(
                instance,
                &SourceCall {
                    method: "saveOrderBook".into(),
                    arguments: vec![
                        obj(&[
                            ("buy", V::Array(vec![])),
                            ("sell", V::Array(vec![])),
                            ("extra", s("persisted")),
                        ]),
                        s("1"),
                    ],
                },
            )
            .unwrap(),
    );
    returned(
        owner
            .evaluate_instance(
                instance,
                &SourceCall {
                    method: "loadOrderBook".into(),
                    arguments: vec![s("1")],
                },
            )
            .unwrap(),
    );
    owner
        .fail_database_call(QueueDbFailure {
            operation: "findOneAsync".into(),
            skip: 1,
            exception: SourceException {
                name: "Error".into(),
                message: "queued".into(),
            },
        })
        .unwrap();
    let complete = owner.encode().unwrap();
    let continued = ContractQueueSourceOwner::decode(&complete).unwrap();
    assert!(continued == owner);
    let restarted =
        ContractQueueSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap()).unwrap();
    assert!(restarted.rows() == owner.rows());
    assert!(array(&restarted.state_value().get("instances")).is_empty());
    assert!(restarted != owner);
    assert!(ContractQueueSourceOwner::restore_durable(&complete).is_err());
    assert!(ContractQueueSourceOwner::decode(&owner.durable_checkpoint().unwrap()).is_err());
    for at in 0..complete.len() {
        let mut bad = complete.clone();
        bad[at] ^= 1;
        assert!(ContractQueueSourceOwner::decode(&bad).is_err());
    }
    for len in 0..complete.len() {
        assert!(ContractQueueSourceOwner::decode(&complete[..len]).is_err());
    }
}
#[test]
fn local_collection_capacity_restores_documents_caches_and_failure_schedule() {
    let mut owner = ContractQueueSourceOwner::new();
    owner
        .import_rows(
            (0..MAX_ROWS)
                .map(|i| obj(&[("_id", s(&format!("other-{i}")))]))
                .collect(),
        )
        .unwrap();
    owner
        .fail_database_call(QueueDbFailure {
            operation: "getDatabase".into(),
            skip: 1,
            exception: SourceException {
                name: "Error".into(),
                message: "must remain scheduled".into(),
            },
        })
        .unwrap();
    let before = owner.encode().unwrap();
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "queueOnChainContractOrder".into(),
            arguments: vec![n(1.0), s("a"), obj(&[]), n(1.0), s("id")]
        }),
        Err(SourceRuleError::Capacity)
    );
    assert!(owner.encode().unwrap() == before);
}

fn checksum(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut crc = !0u32;
    for byte in &bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes
}
fn forged_row(row: &[u8]) -> Vec<u8> {
    let mut bytes = b"TLQS\x01\x01".to_vec();
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(row);
    bytes.extend_from_slice(&0u32.to_be_bytes()); // instances
    bytes.extend_from_slice(&0u32.to_be_bytes()); // scheduled failures
    checksum(bytes)
}
#[test]
fn checkpoint_counts_have_symmetric_domains_and_reject_valid_crc_splices() {
    let mut owner = ContractQueueSourceOwner::new();
    for extra in [
        V::Array(vec![V::Null; MAX_ROWS]),
        V::Object(
            (0..MAX_ROWS)
                .map(|i| (format!("field-{i}"), V::Null))
                .collect(),
        ),
    ] {
        owner
            .import_rows(vec![obj(&[("_id", s("a")), ("extra", extra)])])
            .unwrap();
        assert!(ContractQueueSourceOwner::decode(&owner.encode().unwrap()).unwrap() == owner);
    }
    let before = owner.encode().unwrap();
    assert_eq!(
        owner.import_rows(vec![obj(&[
            ("_id", s("a")),
            ("extra", V::Array(vec![V::Null; MAX_ROWS + 1]))
        ])]),
        Err(SourceRuleError::Capacity)
    );
    assert!(owner.encode().unwrap() == before);

    owner
        .import_rows(vec![obj(&[("_id", s("a"))]), obj(&[("_id", s("b"))])])
        .unwrap();
    let bytes = owner.encode().unwrap();
    let mut duplicate = bytes[..bytes.len() - 4].to_vec();
    let at = duplicate.iter().rposition(|v| *v == b'b').unwrap();
    duplicate[at] = b'a';
    assert_eq!(
        ContractQueueSourceOwner::decode(&checksum(duplicate)).unwrap_err(),
        SourceRuleError::InvalidInput
    );
    let mut trailing = bytes[..bytes.len() - 4].to_vec();
    trailing.push(0);
    assert!(ContractQueueSourceOwner::decode(&checksum(trailing)).is_err());
    let mut excessive = bytes[..bytes.len() - 4].to_vec();
    excessive[6..10].copy_from_slice(&((MAX_ROWS + 1) as u32).to_be_bytes());
    assert_eq!(
        ContractQueueSourceOwner::decode(&checksum(excessive)).unwrap_err(),
        SourceRuleError::Capacity
    );

    let mut with_instance = ContractQueueSourceOwner::new();
    with_instance.create_instance(s("1"), None).unwrap();
    let bytes = with_instance.encode().unwrap();
    let mut id = bytes[..bytes.len() - 4].to_vec();
    id[14..22].copy_from_slice(&2u64.to_be_bytes());
    assert_eq!(
        ContractQueueSourceOwner::decode(&checksum(id)).unwrap_err(),
        SourceRuleError::InvalidInput
    );
}
#[test]
fn checkpoint_parser_charges_aggregate_nodes_depth_and_text_before_allocation() {
    // Truncated just beyond the aggregate node boundary. Parsing must reject
    // Capacity before trying to read a missing 8193rd node or build its subtree.
    let mut row = vec![7];
    row.extend_from_slice(&1u32.to_be_bytes());
    row.extend_from_slice(&5u32.to_be_bytes());
    row.extend_from_slice(b"extra");
    row.push(6);
    row.extend_from_slice(&9u32.to_be_bytes());
    for _ in 0..8 {
        row.push(6);
        row.extend_from_slice(&1024u32.to_be_bytes());
        row.extend(std::iter::repeat_n(1u8, 1024));
    }
    assert_eq!(
        ContractQueueSourceOwner::decode(&forged_row(&row)).unwrap_err(),
        SourceRuleError::Capacity
    );

    let mut depth = vec![7];
    depth.extend_from_slice(&1u32.to_be_bytes());
    depth.extend_from_slice(&1u32.to_be_bytes());
    depth.push(b'x');
    for _ in 0..25 {
        depth.push(6);
        depth.extend_from_slice(&1u32.to_be_bytes());
    }
    assert_eq!(
        ContractQueueSourceOwner::decode(&forged_row(&depth)).unwrap_err(),
        SourceRuleError::Capacity
    );

    let mut text = vec![7];
    text.extend_from_slice(&1u32.to_be_bytes());
    text.extend_from_slice(
        &((tradelayer_core::validity_source::MAX_BYTES + 1) as u32).to_be_bytes(),
    );
    assert_eq!(
        ContractQueueSourceOwner::decode(&forged_row(&text)).unwrap_err(),
        SourceRuleError::Capacity
    );
}

#[test]
fn source_book_json_rejects_bounded_and_surrogate_domains_without_empty_book_success() {
    let cases = [
        (
            "\"\\ud800\"".to_string(),
            SourceRuleError::UnsupportedNumericDomain,
        ),
        (
            format!("{}0{}", "[".repeat(17), "]".repeat(17)),
            SourceRuleError::Capacity,
        ),
        (
            format!("[{}]", vec!["null"; 1025].join(",")),
            SourceRuleError::Capacity,
        ),
        (
            format!(
                "\"{}\"",
                "x".repeat(tradelayer_core::issuance_intent::MAX_JSON_BYTES)
            ),
            SourceRuleError::Capacity,
        ),
    ];
    for (value, expected) in cases {
        let mut owner = ContractQueueSourceOwner::new();
        owner
            .import_rows(vec![obj(&[("_id", s("1")), ("value", s(&value))])])
            .unwrap();
        owner
            .fail_database_call(QueueDbFailure {
                operation: "getDatabase".into(),
                skip: 1,
                exception: SourceException {
                    name: "Error".into(),
                    message: "must survive local rejection".into(),
                },
            })
            .unwrap();
        let before = owner.checkpoint().unwrap();
        assert_eq!(
            owner.evaluate(&SourceCall {
                method: "getOrderbookInstance".into(),
                arguments: vec![s("1")],
            }),
            Err(expected)
        );
        assert_eq!(
            owner.checkpoint().unwrap(),
            before,
            "local JSON domain rejection must roll back instance, cache and fault consumption"
        );
    }
}

// This fixture executes genuine raw18 validation/logic and genuine placement.
// Project only the enqueue boundary: placement results never become authority
// for this owner's processor or an economic settlement substitute.
const COMPOSED_QUEUE_PROJECTION: &str = r###"
const fs=require('node:fs'),path=require('node:path');
const get=(x,k)=>x?.[0]==='object'?x[1].find(p=>p[0]===k)?.[1]:undefined;
const str=x=>x?.[0]==='string'?x[1]:undefined;
const arr=x=>x?.[0]==='array'?x[1]:[];
const U=['undefined'],S=x=>['string',x],O=x=>['object',Object.entries(x)],A=x=>['array',x];
const hex=x=>Buffer.from(JSON.stringify(x)).toString('hex'),previous=new Map();let count=0;
for(const line of fs.readFileSync(path.join(process.argv[1],'fixtures/source_contract_execution_vectors.tsv'),'utf8').split(/\r?\n/)){
 const f=line.split('\t');if(f.length!==7||f[0]!=='V')continue;
 const scene=f[1].slice(0,f[1].lastIndexOf('/')),state=JSON.parse(Buffer.from(f[4],'hex'));
 const table=arr(get(state,'tables')).find(t=>str(get(t,'name'))==='orderBooks');
 const rows=get(table,'rows')||A([]),effects=arr(JSON.parse(Buffer.from(f[5],'hex')));
 const enqueues=effects.map((e,i)=>({e,i})).filter(({e})=>str(get(e,'kind'))==='source'&&str(get(e,'target'))==='Orderbook'&&str(get(e,'method'))==='queueOnChainContractOrder');
 if(enqueues.length===1&&get(enqueues[0].e,'returned')){
  const {e,i}=enqueues[0],calls=[];
  for(const c of effects.slice(i+1)){
   const kind=str(get(c,'kind')),args=arr(get(c,'args'));
   if(kind==='database'&&str(get(c,'name'))==='orderBooks')calls.push(O({collection:S('orderBooks'),operation:S('getDatabase'),query:U,update:U,options:U}));
   if(kind==='db'&&str(get(c,'collection'))==='orderBooks')calls.push(O({collection:S('orderBooks'),operation:get(c,'method'),query:args[0]||U,update:args[1]||U,options:args[2]||U}));
  }
  const command=O({method:get(e,'method'),arguments:get(e,'args')});
  process.stdout.write([f[1],hex(previous.get(scene)||A([])),hex(command),hex(O({returned:get(e,'returned')})),hex(rows),hex(A(calls))].join('\t')+'\n');count++;
 }
 previous.set(scene,rows);
}
process.stderr.write(JSON.stringify({enqueueBoundaries:count}));
"###;
#[test]
fn genuine_composed_raw18_enqueue_observations_replay_original_shifted_params_and_document_writes()
{
    let root = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let output = std::process::Command::new("node")
        .arg("-e")
        .arg(COMPOSED_QUEUE_PROJECTION)
        .arg(root)
        .output()
        .expect("Node is required for source fixture projection");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut count = 0;
    let mut observed_shift = false;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        assert_eq!(f.len(), 6);
        let case = f[0];
        let command = decode(f[2]);
        let args = array(&command.get("arguments"));
        let mut owner = ContractQueueSourceOwner::new();
        owner.import_rows(array(&decode(f[1]))).unwrap();
        let result = owner
            .evaluate(&SourceCall {
                method: text(&command.get("method")),
                arguments: args.clone(),
            })
            .unwrap();
        assert!(
            outcome(result) == decode(f[3]),
            "composed queue outcome: {case}"
        );
        let actual = V::Array(owner.rows().to_vec());
        let expected = decode(f[4]);
        assert!(
            actual == expected,
            "composed actual document write graph: {case}: {}",
            difference(&actual, &expected, "$")
        );
        assert!(
            V::Array(owner.take_db_calls().iter().map(|c| c.value()).collect()) == decode(f[5]),
            "composed ordered queue storage calls: {case}"
        );
        let params = &args[2];
        if case == "raw-post-shifts-reduce/1" {
            assert!(params.get("reduce") == V::Bool(true));
            observed_shift = true;
        }
        assert!(ContractQueueSourceOwner::decode(&owner.encode().unwrap()).unwrap() == owner);
        count += 1;
    }
    assert_eq!(
        count, 57,
        "genuine composed enqueue corpus must remain complete"
    );
    assert!(
        observed_shift,
        "the actual shifted source flags must be observed"
    );
}
