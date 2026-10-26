use tradelayer_core::contract_settlement_source::process_contract_matches;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

// Execute the unchanged archived financial modules through the independent
// composed driver. The driver is loaded in memory without its CLI main; only
// dependency observations are added. No financial return value is supplied.
const ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),{AsyncLocalStorage}=require('node:async_hooks');
const root=process.argv[1],file=path.join(root,'scripts/source_contract_execution_vectors.cjs');
let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.lastIndexOf('\nmain().catch'))+'\nmodule.exports={environment,scenes};\n';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const BigNumber=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();
function diag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='boolean'||typeof v==='string')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BigNumber.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Array.isArray(v))return['array',v.map(diag)];if(typeof v==='object')return['object',Object.keys(v).map(k=>[k,diag(v[k])])];throw Error('Unrepresentable '+typeof v);}
const hex=v=>Buffer.from(JSON.stringify(v)).toString('hex');
const als=new AsyncLocalStorage();let observations=0;
process.on('unhandledRejection',()=>{});
async function flush(){for(let i=0;i<4;i++)await new Promise(resolve=>setImmediate(resolve));}
async function sceneRun(scene){
 const e=driver.exports.environment(scene),Margin=e.load('marginMap'),ids=new WeakMap(),instances=new Map();let next=0,part=0;
 let injectedHistory=false;
 const handle=i=>{if(!ids.has(i)){ids.set(i,++next);instances.set(next,i);}return{sourceMarginInstance:ids.get(i)}};
 const inspect=(ctx,address)=>{const h=handle(ctx.alias),value=ctx.alias.margins.get(address);ctx.trace.push({target:'Margin.inspectPosition',arguments:diag([h,address]),returned:{pending:diag(value)}});};
 function wrap(target,method,namespace,instance=false){const original=target[method];if(typeof original!=='function')throw Error('Missing '+namespace+'.'+method);
 target[method]=function(...args){const ctx=als.getStore(),top=ctx&&ctx.depth===0;if(namespace==='Margin'&&method==='loadMarginMap'&&top)ctx.pastHistory=false;
 let receiver=instance?handle(this):null,projected=args;
 if(instance&&namespace==='Margin'&&(method==='realizePnl'||method==='reduceMargin')){const at=method==='realizePnl'?6:0,pos=args[at],address=args[method==='realizePnl'?0:4];if(this.margins.get(address)===pos)receiver={...receiver,sourcePositionAliases:[{argument:at,address}]};}
 if(top&&namespace==='Orderbook'&&method==='recordContractTrade'){inspect(ctx,ctx.buyer);inspect(ctx,ctx.seller);ctx.pastHistory=true;}
 if(top&&namespace==='TradeHistory'&&method==='savePNL')ctx.trace.push({target:'Clock.nowISOString',arguments:diag([]),returned:{immediate:diag(args[0].timestamp)}});
 if(top&&namespace==='TradeHistory'&&method==='savePNL'&&scene.deferredHistoryFailure&&!injectedHistory){injectedHistory=true;e.setFailure({collection:'tradeHistory',op:'update',message:'injected un-awaited history storage rejection'});}
 if(receiver)projected=[receiver,...args];
 const event=top?{target:namespace+'.'+method,arguments:diag(projected)}:null;if(event)ctx.trace.push(event);
 const finish=value=>{if(namespace==='Margin'&&method==='loadMarginMap'){const h=handle(value);if(event)event.returned={pending:diag(h)};}else if(event)event.returned={immediate:diag(value)};
  if(top&&namespace==='Margin'&&method==='updateContractBalancesWithMatch'){ctx.alias=this;ctx.buyer=args[0].buyOrder.buyerAddress;ctx.seller=args[0].sellOrder.sellerAddress;ctx.contract=args[0].sellOrder.contractId;}
  if(top&&namespace==='Margin'&&method==='settlePNL'&&ctx.pastHistory)inspect(ctx,args[0]);
  if(top&&namespace==='Margin'&&method==='saveMarginMap'){inspect(ctx,ctx.buyer);inspect(ctx,ctx.seller);}
  return value;};
 const fail=error=>{if(event)event.returned={pendingThrow:{name:error.name,message:error.message}};throw error;};
 let result;try{result=ctx?als.run({...ctx,depth:ctx.depth+1},()=>original.apply(this,args)):original.apply(this,args);}catch(error){if(event)event.returned={throw:{name:error.name,message:error.message}};throw error;}
 if(result&&typeof result.then==='function')return result.then(value=>{const returned=finish(value);if(event&&event.returned?.immediate)event.returned={pending:event.returned.immediate};return returned;},fail);
 return finish(result);
 };}
 const statics={Contract:['contractRegistry','isInverse','getCollateralId','getNotionalValue','getInitialMargin','moveCollateralToMargin','getContractInfo'],Tally:['tally','getTally','updateFeeCache','hasSufficientBalance','hasSufficientReserve','hasSufficientMargin','updateBalance'],Clearing:['clearing','isPriceUpdatedForBlockHeight','recordTrade'],VolumeIndex:['volumeIndex','getContractUnitLTCValue','saveVolumeDataById','baselineLiquidityReward','calculateLiquidityReward'],ClearList:['clearlist','isAddressInClearlist'],Channels:['channels','getCommitAddresses'],PnlIou:['iou','addIouClaims','addProfit','addLoss']};
 for(const [namespace,[moduleName,...methods]]of Object.entries(statics))for(const method of methods)wrap(e.load(moduleName),method,namespace);
 wrap(Margin,'loadMarginMap','Margin');for(const method of ['getPositionForAddress','setInitialMargin','recordMarginMapDelta','updateContractBalancesWithMatch','settlePNL','realizePnl','reduceMargin','saveMarginMap'])wrap(Margin.prototype,method,'Margin',true);
 for(const method of ['recordContractTrade'])wrap(e.Orderbook.prototype,method,'Orderbook');
 for(const method of ['cancelExcessOrders','sourceCrossContractReserve'])wrap(e.Orderbook,method,'Orderbook');
 wrap(e.load('tradeHistoryManager').prototype,'savePNL','TradeHistory');
 const original=e.Orderbook.prototype.processContractMatches;
 e.Orderbook.prototype.processContractMatches=async function(...args){const input=diag(args),current=++part,ctx={depth:0,trace:[],alias:null,buyer:null,seller:null,pastHistory:false};let result,sourceReturn;
  try{sourceReturn=await als.run(ctx,()=>original.apply(this,args));result={returned:diag(sourceReturn)};}catch(error){if(error.message==='VolumeIndex.getContractUnitLTCValue is not a function')ctx.trace.push({target:'VolumeIndex.getContractUnitLTCValue',arguments:diag([ctx.contract]),returned:{throw:{name:error.name,message:error.message}}});result={thrown:{name:error.name,message:error.message}};}
  await flush();
  process.stdout.write([scene.name+'/'+current,hex(input),ctx.trace.map(event=>hex(diag(event))).join(','),hex(diag(result))].join('\t')+'\n');observations++;
  if(result.thrown){const error=Error(result.thrown.message);error.name=result.thrown.name;throw error;}
  // The genuine result, not its diagnostic, is required by the actual caller.
  return sourceReturn;
 };
 for(let i=0;i<scene.actions.length;i++){const a=scene.actions[i];e.setHeight(a.block??a.drain??a.publish?.height??a.markProbe?.height??a.flushFees??10);try{
 if(a.raw!==undefined){const tx=(i+1).toString(16).padStart(64,'0'),params=await e.types.decodePayload(tx,18,'tl',a.raw,a.sender,null,0,0,a.block??10);if(params.valid===true)await e.logic.typeSwitch(18,params);}
 else if(a.drain!==undefined)await e.Orderbook.processQueuedOnChainOrdersForBlock(a.drain);
 else if(a.failure)e.setFailure(a.failure);
 else if(a.markProbe)await e.load('clearing').isPriceUpdatedForBlockHeight(a.markProbe.contractId,a.markProbe.height);
 else if(a.publish){const{price,height}=a.publish;await e.load('oracle').publishData(1,price,price,price,price,height);}
 else if(a.flushFees!==undefined)await e.load('tally').flushQueuedFeeAccruals(a.flushFees);
 }catch(error){}await flush();e.effects.length=0;}
}
(async()=>{for(const scene of driver.exports.scenes)await sceneRun(scene);await sceneRun({...driver.exports.scenes.find(s=>s.name==='close-positions'),name:'deferred-history-rejection',deferredHistoryFailure:true});await sceneRun({...driver.exports.scenes.find(s=>s.name==='raw-reduce-shifts-liquidation'),name:'source-low-funds-reserve-cross',funds:10});process.stderr.write(JSON.stringify({observations}));})().catch(error=>{console.error(error);process.exitCode=1;});
"###;
fn decode(hex: &str) -> V {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    tradelayer_core::property_catalog::parse_diagnostic_value(std::str::from_utf8(&bytes).unwrap())
        .unwrap()
}
fn list(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("array")
    }
}
fn text(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("string")
    }
}
struct Recorded {
    events: Vec<V>,
    at: usize,
}
impl SourceBackend for Recorded {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let event = self
            .events
            .get(self.at)
            .unwrap_or_else(|| panic!("unexpected call {} {:?}", call.target, call.arguments));
        self.at += 1;
        assert_eq!(
            text(event.get("target")),
            call.target,
            "call target {}",
            self.at
        );
        // The oracle stores tagged values as ordinary JSON. Unwrap that one layer.
        let from_tag = |tag: V| {
            tradelayer_core::property_catalog::parse_diagnostic_value(&json(&tag)).unwrap()
        };
        assert_eq!(
            V::Array(call.arguments.clone()),
            from_tag(event.get("arguments")),
            "arguments for {} at {}",
            call.target,
            self.at
        );
        let returned = event.get("returned");
        for (key, pending) in [("immediate", false), ("pending", true)] {
            let value = returned.get(key);
            if value != V::Undefined {
                return Ok(if pending {
                    DependencyReturn::Pending(from_tag(value))
                } else {
                    DependencyReturn::Immediate(from_tag(value))
                });
            }
        }
        for (key, pending) in [("throw", false), ("pendingThrow", true)] {
            let error = returned.get(key);
            if error != V::Undefined {
                let e = SourceException {
                    name: text(error.get("name")),
                    message: text(error.get("message")),
                };
                return Ok(if pending {
                    DependencyReturn::PendingThrow(e)
                } else {
                    DependencyReturn::Throw(e)
                });
            }
        }
        panic!("unknown return {:?}", returned)
    }
}
fn json(v: &V) -> String {
    tradelayer_core::property_catalog::json_stringify(v).unwrap()
}
#[test]
fn actual_composed_source_settlement_replays_exact_dependencies_values_aliases_and_prefixes() {
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
    let mut count = 0;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 4);
        let args = list(decode(fields[1]));
        let events = fields[2]
            .split(',')
            .filter(|s| !s.is_empty())
            .map(decode)
            .collect();
        let expected = decode(fields[3]);
        let mut backend = Recorded { events, at: 0 };
        let actual = process_contract_matches(&args, &mut backend)
            .unwrap_or_else(|e| panic!("{}: {:?}", fields[0], e));
        let actual = match actual {
            SourceOutcome::Returned(value) => V::Object(vec![("returned".into(), value)]),
            SourceOutcome::Thrown(e) => V::Object(vec![(
                "thrown".into(),
                V::Object(vec![
                    ("name".into(), V::String(e.name)),
                    ("message".into(), V::String(e.message)),
                ]),
            )]),
        };
        let expected = if expected.get("returned") != V::Undefined {
            V::Object(vec![(
                "returned".into(),
                tradelayer_core::property_catalog::parse_diagnostic_value(&json(
                    &expected.get("returned"),
                ))
                .unwrap(),
            )])
        } else {
            expected
        };
        assert_eq!(actual, expected, "source outcome {}", fields[0]);
        assert_eq!(
            backend.at,
            backend.events.len(),
            "all calls consumed {}",
            fields[0]
        );
        count += 1;
    }
    assert_eq!(count, 27, "composed settlement coverage");
}

const FEE_ORACLE: &str = r###"
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');const root=process.argv[1],BN=require(path.join(root,'scripts/legacy_numeric_oracle.cjs')).loadBigNumber();const source=fs.readFileSync(path.join(root,'fixtures/legacy_orderbook_source.txt'),'utf8').replace(/\r\n/g,'\n');if(crypto.createHash('sha256').update(source).digest('hex')!=='c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98')throw Error('source pin');const m={exports:{}};vm.runInNewContext(source,{module:m,exports:m.exports,console:{log(){},warn(){},error(){}},require(n){if(n==='bignumber.js')return BN;if(n==='uuid')return{v4(){throw Error('unused uuid')}};return new Proxy({}, {get(){throw Error('fee dependency unexpectedly reached')}})}});const orderbook=new m.exports('1');
function d(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.isZero()&&v.isNegative()?'-0':v.toFixed()];if(Array.isArray(v))return['array',v.map(d)];return['object',Object.keys(v).map(k=>[k,d(v[k])])];}const h=v=>Buffer.from(JSON.stringify(d(v))).toString('hex');
let count=0;for(const input of [
 {amountBuy:1,amountSell:1,lastMark:100,notionalValue:100},
 {amountBuy:2,amountSell:2,lastMark:100,notionalValue:100},
 {amountBuy:'0.00001',amountSell:'0.00002',lastMark:'1.00000000000000001',notionalValue:'3.14159265358979323846'},
 {amountBuy:-0,amountSell:0,lastMark:1,notionalValue:1},
 {amountBuy:1,amountSell:2,lastMark:'1e400',notionalValue:'1e400'},
 {amountBuy:Infinity,amountSell:1,lastMark:1,notionalValue:1},
 {amountBuy:1,amountSell:1,lastMark:-Infinity,notionalValue:'1e400'},
 {amountBuy:1,amountSell:1,lastMark:0,notionalValue:0},
 {amountBuy:undefined,amountSell:null,lastMark:1,notionalValue:1},
 {amountBuy:-1,amountSell:-2,lastMark:1,notionalValue:3},
 ])for(const inverse of [false,true])for(const [buyMaker,sellMaker]of [[false,false],[true,false],[false,true],[true,true],[undefined,true]])for(const channel of [false,true]){const x={...input,buyMaker,sellMaker,isInverse:inverse,channel};const result=orderbook.calculateFee(x);process.stdout.write([count++,h([x]),h(result)].join('\t')+'\n');}
"###;

#[test]
fn unchanged_source_fee_math_preserves_decimal_rounding_specials_and_number_bridges() {
    let root = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let output = std::process::Command::new("node")
        .arg("-e")
        .arg(FEE_ORACLE)
        .arg(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut count = 0;
    for line in std::str::from_utf8(&output.stdout).unwrap().lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        let actual =
            tradelayer_core::contract_settlement_source::calculate_fee(&list(decode(f[1])))
                .unwrap();
        assert_eq!(
            actual,
            SourceOutcome::Returned(decode(f[2])),
            "fee case {}",
            f[0]
        );
        count += 1;
    }
    assert_eq!(count, 200);
}

#[test]
fn empty_self_cross_malformed_and_bounded_inputs_never_invent_financial_calls() {
    struct NoCalls;
    impl SourceBackend for NoCalls {
        fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            panic!("unexpected financial call {}", call.target)
        }
    }
    let mut backend = NoCalls;
    for matches in [
        V::Null,
        V::Undefined,
        V::String("not an array".into()),
        V::Array(vec![]),
    ] {
        assert_eq!(
            process_contract_matches(&[matches], &mut backend).unwrap(),
            SourceOutcome::Returned(V::Array(vec![]))
        );
    }
    let order = V::Object(vec![
        ("buyerAddress".into(), V::String("same".into())),
        ("sellerAddress".into(), V::String("same".into())),
    ]);
    let matched = V::Object(vec![
        ("buyOrder".into(), order.clone()),
        ("sellOrder".into(), order),
    ]);
    assert_eq!(
        process_contract_matches(&[V::Array(vec![matched])], &mut backend).unwrap(),
        SourceOutcome::Returned(V::Array(vec![]))
    );
    assert_eq!(
        process_contract_matches(&[V::Array(vec![V::Null])], &mut backend).unwrap(),
        SourceOutcome::Thrown(SourceException {
            name: "TypeError".into(),
            message: "Cannot read properties of null (reading 'buyOrder')".into()
        })
    );
    assert_eq!(
        process_contract_matches(&[const { V::Null }; 5], &mut backend),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        process_contract_matches(&[V::Array(vec![V::Null; 1025])], &mut backend),
        Err(SourceRuleError::Capacity)
    );
    let fee = V::Object(vec![("amountBuy".into(), V::String("9".repeat(513)))]);
    assert_eq!(
        tradelayer_core::contract_settlement_source::calculate_fee(&[fee]),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
}
