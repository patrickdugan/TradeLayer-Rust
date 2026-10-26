use tradelayer_core::margin_map_source_owner::{MarginMapSourceOwner, PositionAlias};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
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
fn array(v: V) -> Vec<V> {
    let V::Array(a) = v else { panic!("array") };
    a
}
fn text(v: V) -> String {
    let V::String(a) = v else { panic!("string") };
    a
}
fn num(v: V) -> u64 {
    let V::Number(a) = v else { panic!("number") };
    f64::from_bits(a) as u64
}
#[derive(Default)]
struct Backend {
    tables: Vec<(String, Vec<V>)>,
    calls: Vec<V>,
    uuid: u64,
    failure: Option<String>,
}
impl Backend {
    fn rows(&mut self, name: &str) -> &mut Vec<V> {
        if !self.tables.iter().any(|(k, _)| k == name) {
            self.tables.push((name.into(), vec![]))
        }
        &mut self.tables.iter_mut().find(|(k, _)| k == name).unwrap().1
    }
    fn state(&self) -> V {
        V::Array(
            self.tables
                .iter()
                .map(|(name, rows)| V::Array(vec![s(name), V::Array(rows.clone())]))
                .collect(),
        )
    }
}
impl SourceBackend for Backend {
    fn invoke(&mut self, c: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.calls.push(obj(&[
            ("target", s(&c.target)),
            ("arguments", V::Array(c.arguments.clone())),
        ]));
        if self.failure.as_ref() == Some(&c.target) {
            self.failure = None;
            let error = SourceException {
                name: "Error".into(),
                message: format!("injected {}", c.target),
            };
            return Ok(
                if matches!(c.target.as_str(), "uuid.v4" | "Contract.setModFlag") {
                    DependencyReturn::Throw(error)
                } else {
                    DependencyReturn::PendingThrow(error)
                },
            );
        }
        let a = &c.arguments;
        let v = match c.target.as_str() {
            "uuid.v4" => {
                self.uuid += 1;
                return Ok(DependencyReturn::Immediate(s(&format!(
                    "uuid-{}",
                    self.uuid
                ))));
            }
            "Contract.getContractInfo" => {
                obj(&[("notionalValue", n(1.0)), ("collateralPropertyId", n(7.0))])
            }
            "Tally.getTally" => obj(&[("available", n(100.0))]),
            "Contract.setModFlag" => return Ok(DependencyReturn::Immediate(V::Undefined)),
            "db.getDatabase" => {
                let name = text(a[0].clone());
                self.rows(&name);
                obj(&[("sourceDatabase", s(&name))])
            }
            method if method.starts_with("db.") => {
                let name = text(a[0].get("sourceDatabase"));
                let rows = self.rows(&name);
                let query = &a[1];
                let key = query.get("_id");
                let index = rows.iter().position(|row| row.get("_id") == key);
                match method {
                    "db.findOneAsync" => index.map(|i| rows[i].clone()).unwrap_or(V::Null),
                    "db.insertAsync" => {
                        rows.push(a[1].clone());
                        a[1].clone()
                    }
                    "db.updateAsync" => {
                        let update = a[2].clone();
                        let i = if let Some(i) = index {
                            i
                        } else if a.get(3).is_some_and(|o| o.get("upsert") == V::Bool(true)) {
                            rows.push(query.clone());
                            rows.len() - 1
                        } else {
                            return Ok(DependencyReturn::Pending(n(0.0)));
                        };
                        if let V::Object(fields) = update.get("$set") {
                            for (k, v) in fields {
                                rows[i].insert(&k, v)?
                            }
                        } else {
                            rows[i] = update
                        }
                        n(1.0)
                    }
                    _ => return Err(SourceRuleError::UnknownDependency),
                }
            }
            _ => return Err(SourceRuleError::UnknownDependency),
        };
        Ok(DependencyReturn::Pending(v))
    }
}
fn observed(outcome: SourceOutcome) -> V {
    match outcome {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}

// Entire archived module; only storage/UUID/Contract/Tally boundaries are
// declared. No source method or financial calculation is replaced.
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.argv[1],code=fs.readFileSync(path.join(root,'fixtures/legacy_margin_map_source.txt'),'utf8').replace(/\r\n/g,'\n');
assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636');
const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['boolean',v];if(typeof v==='string')return['string',v];if(typeof v==='number'){let b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')]}if(BN.isBigNumber(v))return['bignumber',v.valueOf()];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])]}
let Margin,instances=[],tables=[],calls=[],uuid=0,failure=null;
const copy=v=>structuredClone(v),rows=name=>{let row=tables.find(([k])=>k===name);if(!row){row=[name,[]];tables.push(row)}return row[1]};
function dep(target,args,run){calls.push({target,arguments:copy(args)});if(failure===target){failure=null;throw Error('injected '+target)}return run()}
const db={async getDatabase(name){const handle={sourceDatabase:name};return dep('db.getDatabase',[name],()=>{rows(name);return {async findOneAsync(query){return dep('db.findOneAsync',[handle,query],()=>copy(rows(name).find(x=>x._id===query._id)||null))},async insertAsync(doc){return dep('db.insertAsync',[handle,doc],()=>{rows(name).push(copy(doc));return copy(doc)})},async updateAsync(query,update,options){return dep('db.updateAsync',[handle,...arguments],()=>{let r=rows(name),i=r.findIndex(x=>x._id===query._id);if(i<0){if(!options?.upsert)return 0;r.push(copy(query));i=r.length-1}if(update.$set){for(const[k,v]of Object.entries(update.$set))r[i][k]=copy(v)}else r[i]=copy(update);return 1})}}})}};
const Contract={async getContractInfo(id){return dep('Contract.getContractInfo',[id],()=>({notionalValue:1,collateralPropertyId:7}))},setModFlag(flag){return dep('Contract.setModFlag',[flag],()=>undefined)}};
const Tally={async getTally(...args){return dep('Tally.getTally',args,()=>({available:100}))}};
const module={exports:{}};vm.runInNewContext(code,{module,exports:module.exports,console:{log(){},warn(){},error(){}},require(name){if(name==='./db.js')return db;if(name==='bignumber.js')return BN;if(name==='uuid')return{v4(){return dep('uuid.v4',[],()=>`uuid-${++uuid}`)}};if(name==='./contractRegistry.js')return Contract;if(name==='./tally.js')return Tally;if(name==='./options.js')return{};throw Error('Unowned '+name)}},{filename:'pinned-marginMap.js',timeout:2000});Margin=module.exports;
function register(instance){if(instance instanceof Margin&&!instances.includes(instance))instances.push(instance);return instance}
const load=Margin.loadMarginMap;Margin.loadMarginMap=async function(...args){return register(await load.apply(this,args))};
function instanceValue(i){return{seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]}}
async function action(name,command){calls=[];let args=command.args||[],result;const before=tag(args);for(const a of command.aliases||[])args[a.argument]=instances[a.instance-1].margins.get(a.address);try{let v;if(command.op==='create'){v={sourceMarginInstance:instances.push(new Margin(command.series))}}else if(command.op==='cache'){instances[command.receiver-1].margins.set(command.address,command.position);v=undefined}else if(command.op==='fail'){failure=command.target;v=undefined}else if(command.op==='seed'){rows(command.collection).splice(0,Infinity,...copy(command.rows));v=undefined}else{let receiver=command.receiver?instances[command.receiver-1]:Margin;v=await receiver[command.method](...args);if(v instanceof Margin)v={sourceMarginInstance:instances.indexOf(v)+1}}result={returned:v}}catch(e){result={thrown:{name:e.name,message:e.message}}}console.log(JSON.stringify(tag({name,command:{...command,args:undefined,arguments:before},outcome:result,args,calls,instances:instances.map(instanceValue),tables}))) }
const p=(address,contracts=0,margin=0,avgPrice=0,unrealizedPNL=0)=>({address,contracts,margin,avgPrice,unrealizedPNL,extra:{retained:true}});
const alias=(argument,address,instance=1)=>[{argument,instance,address}];
async function call(name,method,args,receiver=1,aliases=[]){return action(name,{op:'call',method,args,receiver,aliases})}
(async()=>{
await call('load-numeric','loadMarginMap',[1],0);await call('load-string','getInstance',['1'],0);await call('missing-position','getPositionForAddress',['absent',1],1);
await call('initial-empty-skips-save','setInitialMargin',['A',1,5,10],1);await call('initial-populated-save-true','setInitialMargin',['A',1,1,11],1);
await call('open','updateContractBalances',['A',2,100,false,null,false,0,0,1,false,12],1,alias(4,'A'));
await call('extend','updateContractBalances',['A',1,110,false,null,false,0,0,1,false,13],1,alias(4,'A'));
await call('settle-linear-ignores-notional','settlePNL',['A',2,105,100,1,14,false,3]);
await call('realize-cached-alias','realizePnl',['A',1,110,100,false,1,null,false,1,15],1,alias(6,'A'));
await call('realize-independent-object','realizePnl',['A',1,101,100,false,1,p('external',2,20,100,-3),false,1,15]);
await call('post-more-margin','setInitialMargin',['A',1,40,16]);
await call('reduce-with-fee','reduceMargin',[null,999,10,1,'A',false,true,1,17],1,alias(0,'A'));
await call('fee-reduction-can-go-negative','feeMarginReduce',['A',null,40,1,18],1,alias(1,'A'));
await call('all-positions-reloads','getAllPositions',[1]);
await call('write-position-preserves-other-rows','writePositionToMap',[1,p('new-position',1,10,100)]);
await call('write-position-replaces-key','writePositionToMap',[1,p('new-position',2,20,101)]);
await action('cache-two-keys',{op:'cache',receiver:1,address:'other-key',position:p('A',2,40,100,0.1)});
await call('settle-key-alias-and-number-subtraction','settlePNL',['other-key',2,100.1,100,1,19,false,1]);
await call('alias-realization-via-other-key','realizePnl',['other-key',1,100.05,100,false,1,null,false,1],1,alias(6,'A'));
await call('save-shared-object-keys','saveMarginMap',[20]);
await call('cached-price-before-injected-read','updateContractBalances',['A',1,120,false,null,false,0,0,1,false,21],1,alias(4,'A'));
for(const target of ['Contract.getContractInfo','Tally.getTally','db.updateAsync','db.insertAsync','Contract.setModFlag']){await action('arm-'+target,{op:'fail',target});await call('partial-'+target,'updateContractBalances',['A',1,130,false,null,false,0,0,1,false,22],1,alias(4,'A'))}
await call('missing-realized-bignumber-zero','realizePnl',['A',1,110,100,false,1,null,false,1]);
await call('missing-reduce-object','reduceMargin',[null]);await call('missing-settle-dereferences-first','settlePNL',['not-there',1,100,90,1,23,false,1]);
await call('initial-external-full-fields','setInitialMargin',['Z',1,0.123456789,24,p('different-address',5,1,100,2)]);
await call('flat-reset','updateContractBalances',['A',7,100,true,p('A',7,20,100,0),false,7,0,1,false,25]);
const match={buyOrder:{buyerAddress:'B',amount:2,sell:false,contractId:1,isLiq:false},sellOrder:{sellerAddress:'C',amount:2,sell:true,contractId:1,isLiq:true},tradePrice:100,buyerPosition:p('B'),sellerPosition:p('C'),inverse:false};
await call('both-match-positions','updateContractBalancesWithMatch',[match,false,0,0,0,0,26]);
await call('settle-match-bp-alias','settlePNL',['B',2,101,100,1,26,false,1]);
await call('inverse-settle-default-notional','settlePNL',['C',-2,110,100,1,27,true]);
for(const inv of [false,true])for(const long of [false,true])for(const count of [0,1,3])await call(`liq-${inv}-${long}-${count}`,'calculateLiquidationPrice',[1,10,count,1,inv,long,100,2]);
for(const [name,args] of [['linear',[3,100,false]],['inverse',[3,100,true]]])await call(name,'calculateMarginRequirement',args);
await call('estimate-linear','estimatePNL',[2,100,105,false,7]);await call('estimate-inverse-fallback','estimatePNL',[2,100,105,true,0]);
await call('average-short-source-number-abs','updateAveragePrice',[p('A',-2,0,100),1,110,1,true]);
await call('radix-string-big-number-inputs','calculateMarginRequirement',['0x10','0b10',false]);
await call('unicode-invalid-big-number-input','calculateMarginRequirement',[1,'€',false]);
await call('nonfinite-big-number-input','calculateMarginRequirement',[1,NaN,false]);
await call('source-number-abs-large-position','updateAveragePrice',[p('A','9007199254740993',0,100),1,101,1,false]);
await call('null-address-source-typeerror','getPositionForAddress',[null,1]);
await action('bech32-input',{op:'seed',collection:'marginMaps',rows:[{_id:'{"seriesId":2}',value:JSON.stringify([['BC1QTEST',p(undefined,2,3,100)]])}]});
await call('bech32-fallback','getPositionForAddress',['bc1qtest',2]);await call('uppercase-no-fallback','getPositionForAddress',['Bc1qtest',2]);
await call('flag-returns-raw-value','loadMarginMap',[2,true],0);
await action('signed-zero-overflow-json',{op:'seed',collection:'marginMaps',rows:[{_id:'{"seriesId":4}',value:'[["json-position",{"address":"json-position","contracts":-0,"margin":1e500,"unrealizedPNL":-0}]]'}]});
await call('source-json-number-bits','loadMarginMap',[4],0);
await call('source-json-number-roundtrip','saveMarginMap',[28],instances.length);
await action('arm-load-find',{op:'fail',target:'db.findOneAsync'});await call('caught-load-find','loadMarginMap',[2],0);
await action('arm-all-positions-load',{op:'fail',target:'db.findOneAsync'});await call('all-positions-missing-loaded-map','getAllPositions',[2]);
await action('arm-load-db',{op:'fail',target:'db.getDatabase'});await call('uncaught-load-db','loadMarginMap',[2],0);
await action('malformed-row',{op:'seed',collection:'marginMaps',rows:[{_id:'{"seriesId":3}',value:'bad json'}]});await call('caught-load-parse','loadMarginMap',[3],0);
})().catch(e=>{console.error(e);process.exitCode=1});
"###;

#[test]
fn full_pinned_source_methods_state_aliases_partial_writes_and_checkpoint_continuation() {
    let root = env!("CARGO_MANIFEST_DIR");
    let output = std::process::Command::new("node")
        .args(["-e", ORACLE, root])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut owner = MarginMapSourceOwner::new();
    let mut backend = Backend::default();
    let mut count = 0;
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        let row = tradelayer_core::property_catalog::parse_diagnostic_value(line).unwrap();
        let name = text(row.get("name"));
        let command = row.get("command");
        let op = text(command.get("op"));
        backend.calls.clear();
        match op.as_str() {
            "cache" => owner
                .set_cached_position(
                    num(command.get("receiver")),
                    command.get("address"),
                    command.get("position"),
                )
                .unwrap(),
            "fail" => backend.failure = Some(text(command.get("target"))),
            "seed" => *backend.rows(&text(command.get("collection"))) = array(command.get("rows")),
            _ => {
                let result = if op == "create" {
                    SourceOutcome::Returned(obj(&[(
                        "sourceMarginInstance",
                        n(owner.create_instance(command.get("series")).unwrap() as f64),
                    )]))
                } else {
                    let args_tag = command.get("arguments");
                    let args = tradelayer_core::property_catalog::parse_diagnostic_value(
                        &tradelayer_core::property_catalog::json_stringify(&args_tag).unwrap(),
                    )
                    .unwrap();
                    let mut args = array(args);
                    let aliases = array(command.get("aliases"))
                        .into_iter()
                        .map(|a| PositionAlias {
                            argument: num(a.get("argument")) as usize,
                            instance: num(a.get("instance")),
                            address: a.get("address"),
                        })
                        .collect::<Vec<_>>();
                    for alias in &aliases {
                        args[alias.argument] = owner
                            .inspect_position(alias.instance, &alias.address)
                            .unwrap()
                    }
                    let receiver = num(command.get("receiver"));
                    let evaluation = owner
                        .evaluate_detailed(
                            (receiver != 0).then_some(receiver),
                            &SourceCall {
                                method: text(command.get("method")),
                                arguments: args,
                            },
                            &aliases,
                            &mut backend,
                        )
                        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
                    assert_eq!(
                        V::Array(evaluation.arguments_after),
                        row.get("args"),
                        "{name} argument mutations"
                    );
                    evaluation.outcome
                };
                assert_eq!(observed(result), row.get("outcome"), "{name} outcome");
            }
        }
        assert_eq!(
            V::Array(backend.calls.clone()),
            row.get("calls"),
            "{name} dependency order/arguments"
        );
        assert_eq!(
            backend.state(),
            row.get("tables"),
            "{name} complete source documents"
        );
        let instances = array(row.get("instances"));
        for (i, expected) in instances.iter().enumerate() {
            assert_eq!(
                &owner.instance_value(i as u64 + 1).unwrap(),
                expected,
                "{name} instance{}",
                i + 1
            )
        }
        let bytes = owner.checkpoint().unwrap();
        let restored = MarginMapSourceOwner::from_checkpoint(&bytes).unwrap();
        assert_eq!(restored, owner, "{name} complete identity restore");
        owner = restored;
        count += 1;
    }
    assert_eq!(count, 76, "full source action corpus must remain complete");
}

#[test]
fn checkpoint_damage_splices_and_unknown_dependencies_are_explicit() {
    let mut owner = MarginMapSourceOwner::new();
    let id = owner.create_instance(n(1.0)).unwrap();
    owner
        .set_cached_position(
            id,
            s("A"),
            obj(&[
                ("address", s("A")),
                ("contracts", n(-2.0)),
                ("margin", n(20.0)),
            ]),
        )
        .unwrap();
    let bytes = owner.checkpoint().unwrap();
    for cut in 0..bytes.len() {
        assert!(MarginMapSourceOwner::from_checkpoint(&bytes[..cut]).is_err())
    }
    for i in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[i] ^= 1;
        assert!(MarginMapSourceOwner::from_checkpoint(&bad).is_err())
    }
    let mut state = owner.state_value().unwrap();
    state.insert("extra", V::Bool(true)).unwrap();
    assert_eq!(
        MarginMapSourceOwner::from_state_value(&state),
        Err(SourceRuleError::InvalidInput)
    );
    struct Absent;
    impl SourceBackend for Absent {
        fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            Err(SourceRuleError::UnknownDependency)
        }
    }
    assert_eq!(
        owner.evaluate(
            &SourceCall {
                method: "loadMarginMap".into(),
                arguments: vec![n(1.0)]
            },
            &mut Absent
        ),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(
        owner.inspect_position(id, &s("missing")).unwrap(),
        V::Undefined
    );
    // A valid CRC cannot make an oversized child count allocate or parse.
    let mut oversized = b"TLMP\x01\x06".to_vec();
    oversized.extend_from_slice(&1025u32.to_be_bytes());
    let mut crc = !0u32;
    for byte in &oversized {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    oversized.extend_from_slice(&(!crc).to_be_bytes());
    assert_eq!(
        MarginMapSourceOwner::from_checkpoint(&oversized),
        Err(SourceRuleError::Capacity)
    );
}
