#!/usr/bin/env node
"use strict";
// Staging layout; --repo ROOT points to the unchanged shared dependency archives.
const assert=require("node:assert/strict"),fs=require("node:fs"),path=require("node:path"),vm=require("node:vm"),crypto=require("node:crypto");
const argv=process.argv.slice(2),stage=path.resolve(__dirname,".."),at=argv.indexOf("--repo");
const repo=at<0?path.resolve(__dirname,".."):path.resolve(argv[at+1]);
const {loadBigNumber,readPinnedSource}=require(path.join(repo,"scripts/legacy_numeric_oracle.cjs"));
const source=readPinnedSource(path.join(stage,"fixtures/legacy_validity_source.txt"),"ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00");
function moduleOf(code,imports,capture=false){const module={exports:{}},sandbox={module,exports:module.exports,Buffer,console:{log(){},error(){},warn(){}},require(name){assert(Object.hasOwn(imports,name),name);return imports[name]}};vm.runInNewContext(code,sandbox,{timeout:2000});return capture?{exports:module.exports,sandbox}:module.exports}
const BN=loadBigNumber();
const options=moduleOf(readPinnedSource(path.join(repo,"fixtures/legacy_options_source.txt"),"02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703"),{"bignumber.js":BN});
function tag(v){if(v===undefined)return["undefined"];if(v===null)return["null"];if(typeof v==="boolean")return["boolean",v];if(typeof v==="string")return["string",v];if(typeof v==="number"){const b=Buffer.alloc(8);b.writeDoubleBE(v);return["number",Number.isNaN(v)?"NaN":v===Infinity?"Infinity":v===-Infinity?"-Infinity":b.toString("hex")]}if(BN.isBigNumber(v))return["bignumber",v.toString()];if(Object.prototype.toString.call(v)==="[object Promise]")return["promise"];if(Array.isArray(v))return["array",Array.from(v,tag)];return["object",Object.keys(v).map(k=>[k,tag(v[k])])]}
const hex=v=>Buffer.from(JSON.stringify(v)).toString("hex");
const property={type:2,whitelistId:0,issuer:"admin",backupAddress:"backup"};
const contract={collateralPropertyId:5,notionalValue:1,leverage:10,underlyingOracleId:1,onChainData:[[5,6]],inverse:true,native:true,whitelist:0};
const channel={A:{5:100,6:100},B:{5:100,6:100},participants:{A:"alice",B:"bob"},payEnabled:{A:true,B:true},clearLists:{A:[],B:[]}};
const defaults={
 "Activation.isTxTypeActive":true,"Activation.getAdmin":"admin","Activation.areActivationsAboveThreshold":true,
 "Property.doesTickerExist":false,"Property.getPropertyData":property,"Property.isManagedAndAdmin":true,
 "Oracle.getOracleInfo":{adminAddress:"admin",backupAddress:"backup"},"Oracle.isAdmin":true,"Oracle.getOraclePrice":100,
 "Contract.getContractInfo":contract,"Contract.isDuplicateNativeContract":false,"Contract.getInitialMargin":10,
 "Tally.getTally":{available:100},"Tally.getAddressBalances":[],"Tally.hasSufficientBalance":{hasSufficient:true,available:100,shortfall:0},"Tally.hasSufficientChannel":{hasSufficient:true,available:100,shortfall:0},
 "ClearList.getBanlist":null,"ClearList.getCountryCodeByAddress":{countryCode:"CA"},"ClearList.isAddressInClearlist":true,"ClearList.isAddressInClearlistOrDerived":true,"ClearList.getAttestations":[],"ClearList.getList":{adminAddress:"admin",backupAddress:"backup"},"ClearList.getClearlistById":{adminAddress:"admin"},
 "Channels.getChannel":channel,"Channels.getCommitAddresses":{commitAddressA:"alice",commitAddressB:"bob"},"Channels.isValidChannel":true,
 "Margin.getPositionForAddress":{contracts:0,margin:100,avgPrice:100},"Margin.margins.get":{},
 "Volume.getLastPrice":100,"Synthetic.exists":true,"Procedural.ensureIssuanceContext":{valid:true},"Procedural.ensureRedemptionContext":{valid:true},
 "Orderbook.getOrdersForAddress":[{txid:"resting"}],"TxUtils.validateAddressWrapper":{isvalid:true,isValid:true},"TxUtils.getTransactionOutputs":[{vout:0,address:"alice",satoshis:100,scriptPubKey:{addresses:["alice"]}},{vout:1,address:"bob",satoshis:100,scriptPubKey:{addresses:["bob"]}}],"TxUtils.getRawTransaction":{vin:[{}]},"TxUtils.getAddressTypeUniversal":"p2wpkh","TxUtils.extractPubkeyByType":["key"],"TxUtils.createMultisig":"channel",
 "Vesting.getChain":"LTC","Vesting.getTest":false,"Scaling.isSettlementNeutralized":false,"Scaling.getTradeStatus":{status:"active"},"DB.findOneAsync":null,
 "Dlc.parseRelayBlob":{},"Dlc.validateRelayBundle":{valid:true},"Zk.verifyProof":true,"GuestTally.arePaymentsValid":true,"GuestTally.hasSufficientBalance":{hasSufficient:true},"GuestChannels.isValidChannel":true,"Bai.isValidBaiUrbunTerms":true,"Murabaha.isValidMurabahaTerms":true,"Bech32.decode":{prefix:"bc"},
 "Margin.loadMarginMap":null,"Margin.getInstance":null,"Orderbook.getOrderbookInstance":null,"DB.getDatabase":null,"Vesting.getInstance":{$mode:"pending",value:null},
};
const asyncTargets=new Set(Object.keys(defaults).filter(k=>!k.startsWith("Activation.getAdmin")&&!k.startsWith("Activation.are")&&!k.startsWith("Dlc.")&&!k.startsWith("Zk.")&&!k.startsWith("Guest")&&!k.startsWith("Bai.")&&!k.startsWith("Murabaha.")&&!k.startsWith("Bech32.")&&!k.startsWith("TxUtils.getAddress")&&!k.startsWith("TxUtils.extract")&&!k.startsWith("Vesting.get")&&!k.startsWith("Margin.margins")&&!k.startsWith("Orderbook.getOrders")));
function environment(overrides={}){
 const trace=[],deferred=[];
 function dep(target,args){const entry=Object.hasOwn(overrides,target)?overrides[target]:defaults[target];assert.notEqual(entry,undefined,`unconfigured ${target}`);const spec=entry&&typeof entry==="object"&&Object.hasOwn(entry,"$mode")?entry:{$mode:asyncTargets.has(target)?"pending":"immediate",value:entry};trace.push([target,tag(args),spec.$mode,["throw","pending-throw"].includes(spec.$mode)?[spec.name,spec.message]:tag(spec.value)]);if(["throw","pending-throw"].includes(spec.$mode)){const e=new Error(spec.message);e.name=spec.name;if(spec.$mode==="throw")throw e;const promise=Promise.reject(e);promise.catch(error=>deferred.push(error));return promise}const v=structuredClone(spec.value);return spec.$mode==="pending"?Promise.resolve(v):v}
 const group=(prefix,names)=>Object.fromEntries(names.split(" ").map(name=>[name,(...args)=>dep(`${prefix}.${name}`,args)]));
 const margin={getPositionForAddress:(...args)=>dep("Margin.getPositionForAddress",args),margins:{get:(...args)=>dep("Margin.margins.get",args)}};
 const imports={
  "./txUtils.js":group("TxUtils","validateAddressWrapper getTransactionOutputs getRawTransaction getAddressTypeUniversal extractPubkeyByType createMultisig"),
  "./db":{getDatabase:async category=>{await dep("DB.getDatabase",[category]);return{findOneAsync:(...args)=>dep("DB.findOneAsync",args)}}},
  "./activation.js":{getInstance:()=>group("Activation","isTxTypeActive getAdmin areActivationsAboveThreshold")},
  "./property.js":group("Property","doesTickerExist getPropertyData isManagedAndAdmin"),"./oracle.js":group("Oracle","getOracleInfo isAdmin getOraclePrice"),
  "./contractRegistry.js":group("Contract","getContractInfo isDuplicateNativeContract getInitialMargin"),"./tally.js":group("Tally","getTally getAddressBalances hasSufficientBalance hasSufficientChannel"),
  "./orderbook.js":{getOrderbookInstance:async(...args)=>{await dep("Orderbook.getOrderbookInstance",args);return{getOrdersForAddress:(...args)=>dep("Orderbook.getOrdersForAddress",args)}}},
  "./channels.js":group("Channels","getChannel getCommitAddresses isValidChannel"),"./marginMap.js":{loadMarginMap:async(...args)=>{await dep("Margin.loadMarginMap",args);return margin},getInstance:async(...args)=>{await dep("Margin.getInstance",args);return margin}},
  "./clearlist.js":group("ClearList","getBanlist getCountryCodeByAddress isAddressInClearlist isAddressInClearlistOrDerived getAttestations getList getClearlistById"),
  "./volumeIndex.js":group("Volume","getLastPrice"),"./vaults.js":group("Synthetic","exists"),"./vesting.js":{getInstance:async(...args)=>{await dep("Vesting.getInstance",args);return group("Vesting","getChain getTest")}},
  "./scaling.js":{ScalingL2:group("Scaling","isSettlementNeutralized getTradeStatus"),SettleType:{},SettleStatus:{}},
  "./dlcOracleBridge.js":group("Dlc","parseRelayBlob validateRelayBundle"),"./procedural.js":{ProceduralRegistry:group("Procedural","ensureIssuanceContext ensureRedemptionContext")},
  "./options.js":options,"bignumber.js":BN,"crypto":crypto,"bech32":{bech32:group("Bech32","decode")},
  "bitcoinjs-lib":{networks:{bitcoin:{}},payments:{}},"tiny-secp256k1":{},"bip32":{BIP32Factory:()=>({fromBase58(){throw Error("unreachable xpub adapter")}})},
 };
 const loaded=moduleOf(source,imports,true),validity=loaded.exports,initialKeys=new Set(Object.keys(loaded.sandbox));
 return {validity,trace,deferred,dep,globalWrites:()=>Object.keys(loaded.sandbox).filter(k=>!initialKeys.has(k)).map(k=>[k,tag(loaded.sandbox[k])]),guest:target=>new Proxy({},{get:(_t,name)=>(...args)=>dep(`${target}.${String(name)}`,args)})};
}
const inventory=Object.keys(environment().validity);assert.equal(inventory.length,43);
const base={block:101,senderAddress:"admin",reason:"old",amount:2,initialAmount:5,ticker:"TOKEN",managed:true,propertyId:5,propertyIds:5,amounts:2,address:"destination",recipientAddress:"destination",satsExpected:100,payToAddress:1,tokenOutput:0,channelAddress:"channel",payEnabled:true,clearLists:[],isContract:false,offeredPropertyId:5,desiredPropertyId:6,propertyIdOffered:5,propertyIdDesired:6,amountOffered:2,amountExpected:3,amountDesired:3,cancelParams:{},backupAddress:"backup",name:"list",newAddress:"new",whitelist:true,oracle:false,token:false,id:1,targetAddress:"destination",metaData:"CA",revoke:false,isRedeem:false,id2:6,amount2:3,amountGranted:2,addressToGrantTo:"destination",amountDestroyed:2,oracleId:1,price:100,native:false,underlyingOracleId:1,collateralPropertyId:5,notionalPropertyId:null,notionalValue:1,expiryPeriod:0,series:1,inverse:true,fee:false,contractId:1,sell:false,expiryBlock:200,columnAIsSeller:false,columnAIsOfferer:true,column:true,withdrawAll:false,toChannelAddress:"destination",isColumnA:true,pay:false,txTypesToActivate:[0,1,18],settleType:0,txidNeutralized1:"trade",txidNeutralized2:"keep",markPrice:100,netAmount:2,aPaysBDirection:true,channelRoot:"root",blockStart:10,blockEnd:20,action:0,stakedPropertyId:5,evidenceHash:"evidence",accusedAddress:"accused",stateHash:"state",relayType:0};
function argsFor(method,p){if(method==="isActivated")return[101,"tx",18];if(method==="isValidNumber")return[p.amount];if(method==="isRestrictedProceduralToken")return[{type:7,proceduralType:1}];if(method==="isTradePriceWithinLeverageBounds")return[{tradePrice:p.price,markPrice:100,leverage:10}];if(method==="hasReferencePrice")return[1,101];if(method==="hasSufficientChannelMargin")return[100,p.amount,"A"];if(method==="checkFlipMargin")return[100,p.amount,"A","flipLong"];if(method==="isListed")return[[1,2],"alice"];if(method==="checkWhitelist")return[[1],"alice","A",p];if(method==="parseTicker")return[p.ticker];if(["validatePayToTokens","validateTradeBaiUrbun","validateTradeMurabaha","validateBatchMoveZkRollup"].includes(method))return[p];return["admin",p,"tx",[{vout:0,satoshis:100},{vout:1,satoshis:100}]]}
const rows=[],covered=new Map(inventory.map(n=>[n,0]));
async function add(name,method,params=base,overrides={},explicitArgs=null){const env=environment(overrides),args=structuredClone(explicitArgs||argsFor(method,params));const original=structuredClone(args);if(method==="validatePayToTokens")args.push(env.guest("GuestTally"));if(method==="validateTradeBaiUrbun")args.push(env.guest("GuestChannels"),env.guest("Bai"));if(method==="validateTradeMurabaha")args.push(env.guest("GuestChannels"),env.guest("Murabaha"));if(method==="validateBatchMoveZkRollup")args.push(env.guest("Zk"),env.guest("GuestTally"));let status,value,caught;try{value=await env.validity[method](...args);status="ok"}catch(e){caught=e;status="throw";value=[e.name,e.message]}await Promise.resolve();const finalArgs=args.slice(0,original.length);rows.push(["V",name,method,hex(tag(original)),hex(overrides),status,hex(status==="ok"?tag(value):value),hex(tag(finalArgs)),hex(env.trace),hex(env.deferred.filter(e=>e!==caught).map(e=>[e.name,e.message])),hex(env.globalWrites())].join("\t"));covered.set(method,covered.get(method)+1)}
async function main(){
 for(const method of inventory){await add(`${method}-normal`,method);await add(`${method}-empty`,method,{});await add(`${method}-inactive`,method,base,{"Activation.isTxTypeActive":false});await add(`${method}-negative`,method,{...base,amount:-1,amounts:-1,price:-1,initialAmount:-1,amountGranted:-1,amountDestroyed:-1,amountOffered:-1,amountExpected:-1,amountDesired:-1});await add(`${method}-zero`,method,{...base,amount:0,amounts:0,price:0,initialAmount:0,amountGranted:0,amountDestroyed:0});await add(`${method}-no-backend-rows`,method,base,{"Property.getPropertyData":null,"Contract.getContractInfo":null,"Oracle.getOracleInfo":null,"Channels.getChannel":null,"Channels.getCommitAddresses":{commitAddressA:null,commitAddressB:null}})}
 for(const amount of [null,false,true,"1","-0",0.00000001,9007199254740991,9007199254740992,1e308])for(const method of ["isValidNumber","validateSend","validateCommit","validateTradeContractOnchain","validateStakeFraudProof"])await add(`${method}-amount-${JSON.stringify(amount)}`,method,{...base,amount,amounts:amount,price:amount});
 for(const price of [95.649999999,95.65,95.650000001,100,104.349999999,104.35,104.350000001])await add(`leverage-edge-${price}`,"isTradePriceWithinLeverageBounds",{...base,price});
 for(const [name,method,changes,overrides]of[
  ["array-send-undefined-helper","validateSend",{amounts:[1,2]},{}],
  ["missing-ticker-throw","validateTokenIssue",{ticker:null},{}],
  ["shortfall-overwrites-invalid","validateTradeTokenForUTXO",{}, {"Activation.isTxTypeActive":false,"Tally.hasSufficientChannel":{hasSufficient:false,shortfall:1}}],
  ["utxo-missing-tokenoutput-global","validateTradeTokenForUTXO",{tokenOutput:"bad"},{}],
  ["utxo-empty-outputs","validateTradeTokenForUTXO",{},{}],
  ["commit-reference","validateCommit",{ref:"1"},{}],
  ["transfer-string-reference","validateTransfer",{ref:"1"},{}],
  ["closed-oracle-promise","validateCloseOracle",{}, {"Oracle.isAdmin":false}],
  ["native-null-promises","validateCreateContractSeries",{native:true,onChainData:[[555,666]],notionalPropertyId:5},{"Property.getPropertyData":null}],
  ["native-empty-pairs","validateCreateContractSeries",{native:true,onChainData:[],notionalPropertyId:5},{}],
  ["missing-ref-price","validateTradeContractOnchain",{}, {"Oracle.getOraclePrice":0}],
  ["bootstrap-true-as-price","validateTradeContractOnchain",{}, {"Contract.getContractInfo":{...contract,underlyingOracleId:0,onChainData:[],firstMark:{blockHeight:100,price:100}}}],
  ["contract-partial-reduce","validateTradeContractOnchain",{}, {"Margin.getPositionForAddress":{contracts:-5}}],
  ["contract-shortfall-negative","validateTradeContractOnchain",{}, {"Tally.hasSufficientBalance":{hasSufficient:false,shortfall:50}}],
  ["token-channel-missing-isListed","validateTradeTokensChannel",{}, {"Property.getPropertyData":{...property,whitelistId:7}}],
  ["synthetic-collateral-initMargin","validateMintSynthetic",{}, {"Tally.hasSufficientBalance":{hasSufficient:false,available:100}}],
  ["synthetic-real-short","validateMintSynthetic",{}, {"Margin.getPositionForAddress":{contracts:-5,margin:0,avgPrice:100}}],
  ["procedural-grant","validateGrantManagedToken",{dlcHash:"hash"},{"Property.getPropertyData":{type:7,proceduralType:1}}],
  ["procedural-missing-destination","validateGrantManagedToken",{addressToGrantTo:""},{"Property.getPropertyData":{type:7,proceduralType:1}}],
  ["settle-close","validateSettleChannelPNL",{settleType:1},{}],
  ["settle-net","validateSettleChannelPNL",{settleType:2,netAmount:101},{}],
  ["settle-king","validateSettleChannelPNL",{settleType:3},{}],
  ["king-count","validateKingSettle",{}, {"DB.findOneAsync":{settlements:[{block:10,status:"active"},{block:20,status:"swept"}],trades:[{block:15}],netSettles:[{block:21}]}}],
  ["stake-relay","validateStakeFraudProof",{action:2,relayBlob:"{}"},{}],
  ["stake-fraud","validateStakeFraudProof",{action:1},{}],
  ["option-primary","validateOptionTrade",{contractId:"1-200-C-100"},{}],
  ["option-combo","validateOptionTrade",{contractId:"1-200-C-100",comboTicker:"1-200-C-110",comboAmount:2,comboPrice:99},{}],
 ]){const args=name==="utxo-empty-outputs"?["admin",{...base,...changes},"tx",[]]:null;await add(name,method,{...base,...changes},overrides,args)}
 for(const method of inventory.filter(m=>m.startsWith("validate")&&!["validatePayToTokens","validateTradeBaiUrbun","validateTradeMurabaha","validateBatchMoveZkRollup"].includes(m))){
  const args=argsFor(method,{...base,senderAddress:"guest"});args[0]="guest";await add(`${method}-nonadmin`,method,base,{},args);
  const invalid=argsFor(method,base);invalid[1]=null;await add(`${method}-null-params`,method,base,{},invalid);
 }
 for(const value of [0,1297,false,"text",null])await add(`publish-primitive-${JSON.stringify(value)}`,"validatePublishOracleData",base,{},["admin",value,"tx"]);
 for(const method of["validatePayToTokens","validateTradeBaiUrbun","validateTradeMurabaha"])await add(`${method}-Types-signature`,method,base,{},["admin",base,"tx"]);
 for(const [name,method,target]of[["awaited-rejection","validateSend","Activation.isTxTypeActive"],["unawaited-close-rejection","validateCloseOracle","Oracle.isAdmin"],["unawaited-channel-rejection","validateTradeTokensChannel","Tally.hasSufficientChannel"],["unawaited-guest-rejection","validatePayToTokens","GuestTally.hasSufficientBalance"]])await add(name,method,base,{[target]:{$mode:"pending-throw",name:"Error",message:"snapshot read rejected"}});
 for(const [name,method,params,target,value]of[["immediate-close","validateCloseOracle",base,"Oracle.isAdmin",false],["immediate-channel","validateTradeTokensChannel",base,"Tally.hasSufficientChannel",{hasSufficient:true}],["immediate-native-null","validateCreateContractSeries",{...base,native:true,onChainData:[[5,6]],notionalPropertyId:5},"Property.getPropertyData",null]])await add(name,method,params,{[target]:{$mode:"immediate",value}});
 for(const price of["104.349999999999999999","104.35","104.350000000000000001","95.650000000000000001","95.649999999999999999"])await add(`decimal-band-${price}`,"isTradePriceWithinLeverageBounds",base,{},[{tradePrice:price,markPrice:"100",leverage:10}]);
 for(const amount of[-0,"-0","-0.000000001","NaN","Infinity",-1.5])await add(`mint-rounded-${String(amount)}-${Object.is(amount,-0)}`,"validateMintSynthetic",{...base,amount});
 for(const type of["native","vesting"])await add(`issuance-impossible-${type}`,"validateTokenIssue",{...base,type,propertyId:type==="native"?1:2});
 for(const address of["1BoatSLRHtKNngkdXEeobR76b53LETtpyT","LZMF1fTj6ks8RD9qsW5zPaMZy6FCFXPLH9","bc1qdiagnostic","mtest","2test",null,0])await add(`address-${JSON.stringify(address)}`,"validateCreateWhitelist",{...base,backupAddress:address},{"TxUtils.validateAddressWrapper":{isvalid:false,isValid:true}});
 for(const [name,changes,overrides]of[
 ["clearlist-no-membership",{},{"ClearList.isAddressInClearlist":false,"Property.getPropertyData":{...property,whitelistId:[7,8]}}],
 ["clearlist-null-property",{},{"Property.getPropertyData":null}],
 ["missing-orders",{},{"Orderbook.getOrdersForAddress":[]}],
 ["null-cancel",{cancelParams:null},{}],
 ["bad-cancel-price",{cancelParams:{price:"1",side:"x",txid:"old"}},{}],
 ])await add(name,"validateCancelOrder",{...base,...changes},overrides);
 for(const target of["Property.getPropertyData","TxUtils.validateAddressWrapper","Tally.getTally","ClearList.getAttestations"])await add(`send-sync-error-${target}`,"validateSend",base,{[target]:{$mode:"throw",name:"TypeError",message:"explicit snapshot failure"}});
 for(const [name,changes,overrides]of[
  ["self-cert",{id:0,targetAddress:"admin",metaData:"ca"},{}],
  ["self-cert-wrong-target",{id:0,targetAddress:"other",metaData:"bad"},{}],
  ["banlist-valid",{id:0,targetAddress:"BANLIST",metaData:"us; kp|RU"},{}],
  ["banlist-invalid",{id:0,targetAddress:"BANLIST",metaData:"USA"},{}],
  ["attest-ref-zero",{ref:null},{}],
  ["revoke-absent",{revoke:true},{"ClearList.isAddressInClearlist":false}],
 ])await add(name,"validateIssueOrRevokeAttestation",{...base,...changes},overrides);
 for(const [name,changes,overrides]of[
  ["native-banned-send",{propertyIds:1},{"ClearList.getCountryCodeByAddress":{countryCode:"US"},"ClearList.isAddressInClearlist":false}],
  ["native-banned-before-threshold",{propertyIds:1},{"ClearList.getCountryCodeByAddress":{countryCode:"US"},"ClearList.isAddressInClearlist":false,"Activation.areActivationsAboveThreshold":false}],
 ])await add(name,"validateSend",{...base,...changes},overrides,["guest",{...base,...changes},"tx"]);
 for(const [i,blob]of[
  {options:{"1-200-C-100":{contracts:3,avgPrice:90,margin:10}}},
  {options:{"1-200-C-100":{contracts:3,avgPrice:90},"1-200-C-90":{contracts:-3,avgPrice:95}}},
  {options:{"1-200-C-100":{contracts:-3,avgPrice:110}}},
  {options:{"1-200-C-100":{contracts:3,avgPrice:90},"1-200-C-110":{contracts:-3,avgPrice:95}}},
 ].entries())for(const combo of[false,true])await add(`option-portfolio-${i}-${combo}`,"validateOptionTrade",{...base,contractId:"1-200-C-100",price:100,...(combo?{comboTicker:"1-200-C-110",comboAmount:2,comboPrice:98}:{})},{"Margin.margins.get":blob});
 for(const q of[-2,0,2])for(const sell of[false,true])for(const amount of[1,3])await add(`channel-position-${q}-${sell}-${amount}`,"validateTradeContractChannel",{...base,columnAIsSeller:sell,amount},{"Margin.getPositionForAddress":{contracts:q,margin:0,avgPrice:100}});
 for(const [name,changes,overrides]of[
 ["withdraw-authorized",{column:true},{}],["withdraw-wrong-column",{column:false},{}],["withdraw-insufficient",{amount:101},{}],
 ])await add(name,"validateWithdrawal",base,overrides,["alice",{...base,...changes},"tx"]);
 for(const method of["validateSettleChannelPNL","validateKingSettle"])for(const [i,changes]of[{settleType:"1"},{settleType:2,netAmount:null},{settleType:2,columnAIsSeller:undefined},{settleType:3,channelRoot:""},{blockStart:0},{blockStart:30,blockEnd:20},{expiryBlock:100},{netAmount:-101},{netAmount:"bad"}].entries())await add(`${method}-settlement-${i}`,method,{...base,...changes});
 let seed=0x476b19e3;for(let i=0;i<64;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;const b=Buffer.alloc(8);b.writeUInt32BE(seed);seed=(Math.imul(seed,1664525)+1013904223)>>>0;b.writeUInt32BE(seed,4);await add(`finite-number-bits-${i}`,"isValidNumber",base,{},[b.readDoubleBE()])}
 for(const [i,ticker]of["1-200-C-100","-0x10-X-NaN","s-+0x10-P-0","s-\u008512-C-2","s-\ufeff12-C-\ufeff.25tail","s-12345678901234567890-C-1"].entries())await add(`ticker-coercion-${i}`,"parseTicker",base,{},[ticker]);
 await add("margin-string-lexical-9-10","hasSufficientChannelMargin",base,{},["9","10","A"]);
 await add("margin-string-lexical-10-9","hasSufficientChannelMargin",base,{},["10","9","A"]);
 await add("activation-large-prefixed-number","validateActivateTradeLayer",{...base,txTypesToActivate:["0x100000000000000000000000000000001"]});
 await add("activation-nel-not-whitespace","validateActivateTradeLayer",{...base,txTypesToActivate:["\u00851"]});
 await add("unsupported-own-toString-coercion","validateActivateTradeLayer",{...base,txTypesToActivate:[{toString:1}]});rows[rows.length-1]=rows[rows.length-1].replace(/^V\t/,"Q\t");
 await add("unsupported-prefixed-bignumber","validateMintSynthetic",{...base,amount:"0x1"});rows[rows.length-1]=rows[rows.length-1].replace(/^V\t/,"Q\t");
 for(const method of inventory)assert(covered.get(method)>0,method);
 const text="# Complete unchanged validity.js dca0888b; SHA256 ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00; genuine BigNumber9.3.1/options; explicit snapshot dependencies.\n# V\tcase\tmethod\ttagged-argumentsJSONhex\tbackendOverridesJSONhex\tok|throw\ttagged-return-or-errorJSONhex\ttagged-final-argumentsJSONhex\tordered-dependency-traceJSONhex\tdeferred-rejectionsJSONhex\tglobal-writesJSONhex\n# defaults "+hex(defaults)+"\n# pending "+hex([...asyncTargets])+"\n"+inventory.map(n=>`# method ${n}`).join("\n")+"\n"+rows.join("\n")+"\n";
 const file=path.join(stage,"fixtures/validity_vectors.tsv");if(argv.includes("--write"))fs.writeFileSync(file,text);else assert.equal(fs.readFileSync(file,"utf8").replace(/\r\n/g,"\n"),text);
 console.log(`PASS validity ${inventory.length} exports ${rows.length} observations; Node ${process.version}`);
}
module.exports={environment,tag,defaults,asyncTargets};
if(require.main===module)main().catch(e=>{console.error(e);process.exitCode=1});
