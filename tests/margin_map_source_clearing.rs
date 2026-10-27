use tradelayer_core::margin_map_source_owner::{
    MarginMapSourceOwner, PositionAlias, PositionArrayAlias, PositionObjectAlias,
};
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
    failure: Option<(String, usize)>,
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
        if self
            .failure
            .as_ref()
            .is_some_and(|(target, _)| target == &c.target)
        {
            if self.failure.as_ref().unwrap().1 > 0 {
                self.failure.as_mut().unwrap().1 -= 1;
            } else {
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
            "Contract.getInitialMargin" => n(2.0),
            "Contract.getCollateralId" => n(7.0),
            "Tally.hasSufficientMargin" => {
                if a[0] == s("shortfall") {
                    obj(&[("hasSufficient", V::Bool(false)), ("shortfall", n(1.0))])
                } else {
                    obj(&[("hasSufficient", V::Bool(true)), ("shortfall", n(0.0))])
                }
            }
            "Tally.updateBalance" => V::Undefined,
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
                if method == "db.findAsync" {
                    return Ok(DependencyReturn::Pending(V::Array(rows.clone())));
                }
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

const EXPECTED_ACTIONS: usize = 90;
// The complete hash-pinned source module runs unchanged; only declared external boundaries are provided.
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.argv[1],code=fs.readFileSync(path.join(root,'fixtures/legacy_margin_map_source.txt'),'utf8').replace(/\r\n/g,'\n');
assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636');
const BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean')return['boolean',v];if(typeof v==='string')return['string',v];if(typeof v==='number'){let b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')]}if(BN.isBigNumber(v))return['bignumber',v.valueOf()];if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])]}
let Margin,instances=[],tables=[],calls=[],uuid=0,failure=null;
const copy=v=>structuredClone(v),rows=name=>{let row=tables.find(([k])=>k===name);if(!row){row=[name,[]];tables.push(row)}return row[1]};
function dep(target,args,run){calls.push({target,arguments:copy(args)});if(failure?.target===target){if(failure.skip>0)failure.skip--;else{failure=null;throw Error('injected '+target)}}return run()}
const db={async getDatabase(name){const handle={sourceDatabase:name};return dep('db.getDatabase',[name],()=>{rows(name);return {async findAsync(...args){return dep('db.findAsync',[handle,...args],()=>copy(rows(name)))},async findOneAsync(query){return dep('db.findOneAsync',[handle,query],()=>copy(rows(name).find(x=>x._id===query._id)||null))},async insertAsync(doc){return dep('db.insertAsync',[handle,doc],()=>{rows(name).push(copy(doc));return copy(doc)})},async updateAsync(query,update,options){return dep('db.updateAsync',[handle,...arguments],()=>{let r=rows(name),i=r.findIndex(x=>x._id===query._id);if(i<0){if(!options?.upsert)return 0;r.push(copy(query));i=r.length-1}if(update.$set){for(const[k,v]of Object.entries(update.$set))r[i][k]=copy(v)}else r[i]=copy(update);return 1})}}})}};
const Contract={async getInitialMargin(...args){return dep('Contract.getInitialMargin',args,()=>2)},async getCollateralId(...args){return dep('Contract.getCollateralId',args,()=>7)},async getContractInfo(id){return dep('Contract.getContractInfo',[id],()=>({notionalValue:1,collateralPropertyId:7}))},setModFlag(flag){return dep('Contract.setModFlag',[flag],()=>undefined)}};
const Tally={async hasSufficientMargin(...args){return dep('Tally.hasSufficientMargin',args,()=>({hasSufficient:args[0]!=='shortfall',shortfall:args[0]==='shortfall'?1:0}))},async updateBalance(...args){return dep('Tally.updateBalance',args,()=>undefined)},async getTally(...args){return dep('Tally.getTally',args,()=>({available:100}))}};
const module={exports:{}};vm.runInNewContext(code,{module,exports:module.exports,console:{log(){},warn(){},error(){}},require(name){if(name==='./db.js')return db;if(name==='bignumber.js')return BN;if(name==='uuid')return{v4(){return dep('uuid.v4',[],()=>`uuid-${++uuid}`)}};if(name==='./contractRegistry.js')return Contract;if(name==='./tally.js')return Tally;if(name==='./options.js')return{};throw Error('Unowned '+name)}},{filename:'pinned-marginMap.js',timeout:2000});Margin=module.exports;
function register(instance){if(instance instanceof Margin&&!instances.includes(instance))instances.push(instance);return instance}
const load=Margin.loadMarginMap;Margin.loadMarginMap=async function(...args){return register(await load.apply(this,args))};
function instanceValue(i){return{seriesId:i.seriesId,margins:[...i.margins],expiryIndex:[...i.expiryIndex],tickerExpiry:[...i.tickerExpiry],optionOI:[...i.optionOI]}}
async function action(name,command){calls=[];let args=command.args||[],result;const before=tag(args);for(const a of command.aliases||[])args[a.argument]=instances[a.instance-1].margins.get(a.address);for(const a of command.arrayAliases||[])args[a.argument][a.index]=instances[a.instance-1].margins.get(a.address);try{let v;if(command.op==='create'){v={sourceMarginInstance:instances.push(new Margin(command.series))}}else if(command.op==='cache'){instances[command.receiver-1].margins.set(command.address,command.position);v=undefined}else if(command.op==='fail'){failure={target:command.target,skip:command.skip||0};v=undefined}else if(command.op==='seed'){rows(command.collection).splice(0,Infinity,...copy(command.rows));v=undefined}else{let receiver=command.receiver?instances[command.receiver-1]:Margin;v=await receiver[command.method](...args);if(v instanceof Margin)v={sourceMarginInstance:instances.indexOf(v)+1}}result={returned:v}}catch(e){result={thrown:{name:e.name,message:e.message}}}console.log(JSON.stringify(tag({name,command:{...command,args:undefined,arguments:before},outcome:result,args,calls,instances:instances.map(instanceValue),tables}))) }
const p=(address,contracts=0,margin=0,avgPrice=0,unrealizedPNL=0)=>({address,contracts,margin,avgPrice,unrealizedPNL,extra:{retained:true}});
const alias=(argument,address,instance=1)=>[{argument,instance,address}];
async function call(name,method,args,receiver=1,aliases=[],arrayAliases=[]){return action(name,{op:'call',method,args,receiver,aliases,arrayAliases})}
(async()=>{
await action('create-clearing-instance',{op:'create',series:1});
await call('merge-flatten-ious-strict-zero','mergePositions',[[p('flat'),{...p('iou'),iouClaim:1},{...p('string-zero','0')},p('live',3,30,100)],1]);
await call('merge-duplicate-last-reference','mergePositions',[[p('same',1,2),p('same',2,3),p('flat')],1,false]);
await call('merge-actual-array-alias','mergePositions',[[null,p('external',-2,20)],1,false],1,[],[{argument:0,index:0,instance:1,address:'same'}]);
await call('update-margin-existing-absolute-delta-bug','updateMargin',['same',1,2,10]);
await call('update-margin-root-alias','updateMargin',['external',1,-1,11,null],1,alias(4,'external'));
await call('update-margin-missing-undefined-key','updateMargin',['missing',1,7,12]);
await call('clear-existing-root-alias','clear',[null,'same',-2.123456789,100,1,13,99,101],1,alias(0,'same'));
await action('arm-clear-write-caught',{op:'fail',target:'db.updateAsync'});
await call('clear-write-false-still-delta','clear',[p('independent',-1,10,100), 'different-log-address',3,100,1,14,102,100]);
await action('arm-update-margin-delta',{op:'fail',target:'db.insertAsync'});
await call('margin-prefix-before-delta-failure','updateMargin',['same',1,5,15]);
await action('arm-merge-save',{op:'fail',target:'db.updateAsync'});
await call('merge-replaces-before-save-failure','mergePositions',[[p('zero'),p('keep',-2,12,100)],1,false]);
await action('cache-flat-string-margin',{op:'cache',receiver:1,address:'string-margin',position:p('string-margin',0,'0')});
await action('cache-flat-string-contracts',{op:'cache',receiver:1,address:'string-contracts',position:p('string-contracts','0')});
await action('cache-flat-iou',{op:'cache',receiver:1,address:'iou',position:{...p('iou'),iouClaim:'0.00000001'}});
await call('cleanup-strict-number-fields-and-iou','cleanupFlatPositions',[999]);
await call('maintenance-missing','computeMaintenanceMarginRequirement',[1,null]);
await call('maintenance-signed-string-number','computeMaintenanceMarginRequirement',[1,p('X','-3',0,'100')]);
await call('maintenance-source-number-large','computeMaintenanceMarginRequirement',[1,p('X','9007199254740993',0,100)]);
await call('below-missing','isBelowMaintenanceMargin',['missing',1]);
await call('below-cached','isBelowMaintenanceMargin',['keep',1]);
await call('below-external-ignores-upnl','isBelowMaintenanceMargin',['X',1,p('X',-3,2,100,999)]);
await action('arm-maintenance',{op:'fail',target:'Contract.getInitialMargin'});
await call('maintenance-dependency-reject','computeMaintenanceMarginRequirement',[1,p('X',2,2,100)]);
for(const [name,args] of [
 ['no-amount',[null,1,0]],['zero-position',[p('X'),1,1,20,100,new BN(90)]],
 ['no-equity',[p('X',2,0),1,1,20,100,new BN(90)]],
 ['number-price-source-error',[p('X',2,1),1,1,20,100,90]],
 ['null-price-source-error',[p('X',2,1),1,1,20,100,null]],
 ['long-order',[{...p('X',2,1),available:2},1,'1.5',20,999,new BN('90.123456789')]],
 ['short-order',[p('X',-2,3),1,2,20,999,new BN(110)]]
])await call('generate-'+name,'generateLiquidationOrder',args);
await call('save-liquidation-complete-document','saveLiquidationOrders',[1,p('X',-2,3),{amount:2},'below',21,5,1,2,{cp:'B'},{extra:1}]);
await call('fetch-normal-liquidation-document-not-array','fetchLiquidationVolume',[21,1,999]);
await call('record-trade-contract-history','recordContractTrade',[{contractId:1,isLiq:true,amount:2,tradePrice:105},21,'seller','buyer']);
await call('record-unrelated-contract-still-counted','recordContractTrade',[{contractId:9,isLiq:true,amount:1,tradePrice:200},'21']);
await call('fetch-source-vwap-ignores-contract-and-sums-prices','fetchLiquidationVolume',[21,1]);
await call('save-trade-spread-overrides-generated-id','saveTrade',[{key:'manual',_id:'overridden',blockHeight:22,extra:'kept'}]);
await action('arm-trade-uuid',{op:'fail',target:'uuid.v4'});
await call('trade-db-before-uuid-failure','recordContractTrade',[{contractId:1},23]);
await action('arm-trade-write',{op:'fail',target:'db.updateAsync'});
await call('trade-write-failure-retains-uuid','recordContractTrade',[{contractId:1},23]);
await action('arm-liquidation-read',{op:'fail',target:'db.findOneAsync'});
await call('fetch-catches-liquidation-read','fetchLiquidationVolume',[21,1]);
await action('arm-trades-read',{op:'fail',target:'db.findAsync'});
await call('fetch-trades-read-rejects','fetchLiquidationVolume',[21,1]);
for(const [name,args] of [
 ['linear-long',[2,100,105,false,3,true]],['linear-short',[2,100,105,false,0,false]],
 ['inverse',[2,100,105,true,3,true]],['signed-zero',[0,100,105,false,1,false]],
 ['nan',[1,0,0,true,1,true]],['small-scientific',[1,0,0.00000001,false,1,true]]
])await call('price-delta-'+name,'calcPriceDelta',args);
await call('adjust-request-zero','adjustDeleveraging',[[p('A',2,20)],0,'A',1,0,false,24,100]);
await call('adjust-missing-index','adjustDeleveraging',[[],0,'A',1,1,false,24,100]);
await call('adjust-independent-long-clamp','adjustDeleveraging',[[p('A',2,20,100,9)],0,'log-A',1,5,false,24,100]);
await call('adjust-cached-array-short','adjustDeleveraging',[[null],0,'keep',1,1,true,25,100],1,[],[{argument:0,index:0,instance:1,address:'keep'}]);
await call('adjust-shortfall','adjustDeleveraging',[[p('shortfall',-3,30,100,5)],0,'shortfall',1,1,true,26,100]);
await action('arm-adjust-initial',{op:'fail',target:'Contract.getInitialMargin'});
await call('adjust-contract-mutation-before-dependency-failure','adjustDeleveraging',[[p('A',-2,20)],0,'A',1,1,true,27,100]);
await action('arm-adjust-tally',{op:'fail',target:'Tally.updateBalance'});
await call('adjust-saved-margin-before-tally-failure','adjustDeleveraging',[[p('A',3,30)],0,'A',1,1,false,28,100]);
await action('arm-adjust-second-delta',{op:'fail',target:'db.insertAsync',skip:2});
await call('adjust-second-duplicate-delta-failure','adjustDeleveraging',[[p('A',2,20,100,9)],0,'A',1,2,false,29,100]);
const cp=(address,contracts,lastMark,margin=30)=>({...p(address,contracts,margin,100),lastMark});
await call('simple-winner-pool-weighted','simpleDeleverage',[[cp('liquidator',4,100),cp('winner-small',-1,120),cp('winner-large',-3,110),cp('loser',-5,90)],1,3,true,95,'liquidator',false,2,30,100,7,12]);
await call('simple-fallback-all-opposites-proportional-pool','simpleDeleverage',[[cp('B',-1,90),cp('C',-3,90),cp('wrong',2,110)],1,2,true,100,'X',false,1,31,100,7,8]);
await call('simple-inverse-winner-ignores-notional','simpleDeleverage',[[cp('B',2,90),cp('C',1,80)],1,2,false,105,'X',true,999,32,100,7,0]);
await call('simple-zero-remaining-still-gets-db','simpleDeleverage',[[cp('B',-2,110)],1,0,true,100,'X',false,1,33,100,7,5]);
await action('arm-simple-adl-write',{op:'fail',target:'db.updateAsync',skip:1});
await call('simple-adl-write-after-position-save','simpleDeleverage',[[cp('B',-2,110)],1,1,true,100,'X',false,1,34,100,7,5]);
await call('simple-array-method-typeerror','simpleDeleverage',[null,1,1,true,100,'X',false,1,35,100,7,5]);
await call('execute-deleveraging-actual-two-reloads','executeDeleveraging',['B',1,1,true,100,36]);
await call('execute-missing-position-stores-undefined-key','executeDeleveraging',['missing',1,1,false,100,37]);
await call('dynamic-undefined-contract-registry-after-loads','dynamicDeleverage',[1,true,2,100]);
await call('net-exposure-undefined-registry','calculateNetExposure',['B',7]);
await call('market-price-misspelled-registry','getMarketPrice',[{id:1}]);
await call('needs-liquidation-map-object-index-defect','needsLiquidation',[{id:1,marketPrice:100}]);
await call('needs-liquidation-map-size-builtin','needsLiquidation',[{id:'size',marketPrice:100}]);
await call('simple-invalid-remaining-short-circuits-adjust','simpleDeleverage',[[cp('N',-2,110)],1,'bad',true,100,'X',false,1,38,100,7,1]);
await action('cache-shared-array-object',{op:'cache',receiver:1,address:'alias-key',position:cp('shared',-3,110)});
await call('install-second-shared-map-key','updateMargin',['alias-key',1,0,39]);
await call('simple-repeated-established-object-alias','simpleDeleverage',[[null,null],1,4,true,100,'X',false,1,40,100,7,4],1,[],[{argument:0,index:0,instance:1,address:'alias-key'},{argument:0,index:1,instance:1,address:'shared'}]);
await action('arm-cleanup-save',{op:'fail',target:'db.updateAsync'});
await call('cleanup-deletes-before-save-failure','cleanupFlatPositions',[1]);
await action('arm-liquidation-save',{op:'fail',target:'db.updateAsync'});
await call('save-liquidation-propagates-write-failure','saveLiquidationOrders',[1,p('X',-2,3),{amount:2},'below',41]);
await call('merge-source-string-iterable','mergePositions',['ab',1,false]);
await call('constructor-expiry-index-empty','getExpiringTickersUpTo',[999]);
await call('constructor-expiry-cleanup-no-hydration','cleanupExpiredTickersUpTo',[999]);
await call('expiry-index-remains-empty','getExpiringTickersUpTo',[Infinity]);
})().catch(e=>{console.error(e);process.exitCode=1});

"###;
#[test]
fn full_pinned_clearing_methods_args_aliases_ordered_writes_and_failures() {
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
            "fail" => {
                backend.failure = Some((
                    text(command.get("target")),
                    if command.get("skip") == V::Undefined {
                        0
                    } else {
                        num(command.get("skip")) as usize
                    },
                ))
            }
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
                    let array_aliases = if command.get("arrayAliases") == V::Undefined {
                        vec![]
                    } else {
                        array(command.get("arrayAliases"))
                            .into_iter()
                            .map(|a| PositionArrayAlias {
                                argument: num(a.get("argument")) as usize,
                                index: num(a.get("index")) as usize,
                                instance: num(a.get("instance")),
                                address: a.get("address"),
                            })
                            .collect::<Vec<_>>()
                    };
                    let receiver = num(command.get("receiver"));
                    let evaluation = owner
                        .evaluate_detailed_with_array_aliases(
                            (receiver != 0).then_some(receiver),
                            &SourceCall {
                                method: text(command.get("method")),
                                arguments: args,
                            },
                            &aliases,
                            &array_aliases,
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
    assert_eq!(
        count, EXPECTED_ACTIONS,
        "full clearing action corpus must remain complete"
    );
}

#[test]
fn explicit_arena_identity_survives_clearing_merge_mutations_and_restart() {
    let mut owner = MarginMapSourceOwner::new();
    let receiver = owner.create_instance(n(1.0)).unwrap();
    let object = owner
        .allocate_position_object(obj(&[
            ("address", s("A")),
            ("contracts", n(2.0)),
            ("margin", n(10.0)),
            ("unrealizedPNL", n(0.0)),
            ("avgPrice", n(100.0)),
        ]))
        .unwrap();
    let mut backend = Backend::default();
    let run = |owner: &mut MarginMapSourceOwner,
               backend: &mut Backend,
               method: &str,
               args: Vec<V>,
               alias: PositionObjectAlias| {
        owner
            .evaluate_detailed_with_object_aliases(
                Some(receiver),
                &SourceCall {
                    method: method.into(),
                    arguments: args,
                },
                &[],
                &[],
                &[alias],
                backend,
            )
            .unwrap()
    };
    let cleared = run(
        &mut owner,
        &mut backend,
        "clear",
        vec![
            V::Null,
            s("A"),
            n(3.0),
            n(100.0),
            n(1.0),
            n(10.0),
            n(101.0),
            n(100.0),
        ],
        PositionObjectAlias {
            argument: 0,
            index: None,
            object,
        },
    );
    assert!(matches!(cleared.outcome, SourceOutcome::Returned(_)));
    let mut external = owner.inspect_position_object(object).unwrap();
    external.insert("margin", n(17.0)).unwrap();
    owner.replace_position_object(object, external).unwrap();
    assert_eq!(
        owner
            .inspect_position(receiver, &s("A"))
            .unwrap()
            .get("margin"),
        n(17.0)
    );
    run(
        &mut owner,
        &mut backend,
        "mergePositions",
        vec![V::Array(vec![V::Null]), n(1.0), V::Bool(false)],
        PositionObjectAlias {
            argument: 0,
            index: Some(0),
            object,
        },
    );
    let checkpoint = owner.checkpoint().unwrap();
    owner = MarginMapSourceOwner::from_checkpoint(&checkpoint).unwrap();
    let adjusted = run(
        &mut owner,
        &mut backend,
        "adjustDeleveraging",
        vec![
            V::Array(vec![V::Null]),
            n(0.0),
            s("A"),
            n(1.0),
            n(1.0),
            V::Bool(false),
            n(11.0),
            n(101.0),
        ],
        PositionObjectAlias {
            argument: 0,
            index: Some(0),
            object,
        },
    );
    assert_eq!(
        owner.inspect_position_object(object).unwrap(),
        owner.inspect_position(receiver, &s("A")).unwrap()
    );
    assert_eq!(
        owner
            .inspect_position_object(object)
            .unwrap()
            .get("contracts"),
        n(1.0)
    );
    assert_eq!(
        array(adjusted.arguments_after[0].clone())[0],
        owner.inspect_position_object(object).unwrap()
    );
    assert_eq!(
        owner.replace_position_object(usize::MAX, V::Null),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        owner.inspect_position_object(usize::MAX),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        owner.evaluate_detailed_with_object_aliases(
            Some(receiver),
            &SourceCall {
                method: "clear".into(),
                arguments: vec![V::Null]
            },
            &[],
            &[],
            &[PositionObjectAlias {
                argument: 0,
                index: None,
                object: usize::MAX
            }],
            &mut backend
        ),
        Err(SourceRuleError::InvalidInput)
    );
    let unreferenced = owner
        .allocate_position_object(obj(&[("future", V::Bool(true))]))
        .unwrap();
    assert_eq!(
        MarginMapSourceOwner::from_checkpoint(&owner.checkpoint().unwrap())
            .unwrap()
            .inspect_position_object(unreferenced)
            .unwrap(),
        obj(&[("future", V::Bool(true))])
    );
    assert!(matches!(
        owner
            .invoke_source_instance(
                Some(receiver),
                "generateLiquidationOrder",
                &[V::Null, n(1.0), n(0.0)],
                &[],
                &mut backend
            )
            .unwrap(),
        DependencyReturn::Immediate(V::Null)
    ));
    assert!(matches!(
        owner
            .invoke_source_instance(
                Some(receiver),
                "needsLiquidation",
                &[obj(&[("id", n(1.0))])],
                &[],
                &mut backend
            )
            .unwrap(),
        DependencyReturn::Throw(_)
    ));
}
