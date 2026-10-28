#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=process.env.ACTIVATION_REPO||path.resolve(__dirname,'..'),flags=process.argv.slice(1),lock=flags.includes('--lock')||process.env.TL_SOURCE_BN_VERSION==='9.1.2',numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],raw=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(raw.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('const sources = Object.fromEntries(',`pins.procedural=['legacy_procedural_source.txt','0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e'];\nconst sources = Object.fromEntries(`);
replace("'vaults','procedural','bitvmCache'","'vaults','bitvmCache'");
// Ordinary host console methods return undefined; the reused audit driver's
// logs.push return length must not become a fabricated addProperty result.
replace("(...values)=>logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))})","(...values)=>{logs.push({level,values:values.map(value=>typeof value==='string'?value:String(value))});}");
replace('return {types,logic,Orderbook,effects','return {adapters,context,storage,tables,unsupported,copy,types,logic,Orderbook,effects');code+='\nmodule.exports={environment};';
const activationStart=code.indexOf("'./db':storage,'./activation':"),activationEnd=code.indexOf("    './txUtils':",activationStart);assert.ok(activationStart>=0&&activationEnd>activationStart);code=code.slice(0,activationStart)+"'./db':storage, './client':forbidden('ClientWrapper'),\n"+code.slice(activationEnd);
replace('const sources = Object.fromEntries(',`pins.activation=['native_genesis/source/activation.js','268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e'];\nconst sources = Object.fromEntries(`);

replace('const sources = Object.fromEntries(',`Object.assign(pins,{
 vesting:['native_genesis/source/vesting.js','4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b'],
 main:['native_genesis/source/main.js','006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009'],
 persistence:['legacy_persistence_source.txt','628c5b3cfd919fd518aedc2b757cdc256f7ccc36c447090f30aebd8a677ce2e1'],
 reOrg:['legacy_reorg_source.txt','3e4cabb0dc560fd8c601b72e05f0e698ce02adc646eaef241189fd192d906a82'],
 vaults:['legacy_vaults_source.txt','3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983'],
 txIndex:['legacy_txindex_source_txIndex.txt','095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51']});\nconst sources = Object.fromEntries(`);
replace("['vesting','txIndex'", "['txIndex'");
replace("'./client':forbidden('ClientWrapper')",`'./client':{async getInstance(...args){effects.push({kind:'rpc-input',target:'Client.getInstance',args});if(!scene.clientProfile){const error=Object.assign(Error('Unavailable explicit Client metadata profile'),{name:'SourceBoundaryUnavailable'});effects.push({kind:'unavailable',target:'Client.getInstance',args,thrown:{name:error.name,message:error.message}});throw error;}return{async getChain(...args){effects.push({kind:'rpc-input',target:'Client.getChain',args,returned:scene.clientProfile.chain});return scene.clientProfile.chain;},async getTest(...args){effects.push({kind:'rpc-input',target:'Client.getTest',args,returned:scene.clientProfile.test});return scene.clientProfile.test;}};}}`);
replace("'fs':forbidden('fs')",`'fs':{existsSync(file){const name=path.basename(file,'.js'),returned=Object.hasOwn(sources,name);effects.push({kind:'source-file',method:'existsSync',name,returned});return returned;},promises:{async readFile(file,encoding){const name=path.basename(file,'.js');if(!Object.hasOwn(sources,name))throw Error('Unavailable archived source file '+name);effects.push({kind:'source-file',method:'readFile',name,sha256:crypto.createHash('sha256').update(sources[name]).digest('hex')});return sources[name];}}}`);
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);

const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='LKQa26zKBtyM2kw6aMEYyj4fCbCqxJyrPw',records=[];let e,lastParams,unhandled=[];process.on('unhandledRejection',error=>unhandled.push({name:error.name,message:error.message}));
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Error]')return tag({name:v.name,message:v.message});if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<6;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),C=e.load('channels');return{activation:{...e.load('activation').instance},vesting:e.load('vesting').instance?{...e.load('vesting').instance}:undefined,vestingStatic:{adminAddress:e.load('vesting').adminAddress,sponsorAddress:e.load('vesting').sponsorAddress,expiryInterval:e.load('vesting').expiryInterval,hedgeLeverage:e.load('vesting').hedgeLeverage},consensusVector:e.load('consensus').consensusVector,insuranceInstances:e.load('insurance').instances,clearlist:{lists:e.load('clearlist').clearlists,banlist:e.load('clearlist').banlist,xpubCache:e.load('clearlist').xpubCache,roots:e.load('clearlist').merkleXpubMap},tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,tallyInstance:T.instance?{addresses:T.instance.addresses,feeCache:T.instance.feeCache,modFlag:T.instance.modFlag}:undefined,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,channels:C.channelsRegistry,withdrawals:C.pendingWithdrawals,lastParams,unhandled,unsupported:e.unsupported.slice()};}
function emit(name,command,result){const snapshot=e.copy(state());if(!command.unsupported)assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command,result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','rpc','rpc-input','source-file','unavailable','activation'].includes(v.kind))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){e.effects.push({kind:'source-function',target:label,args:structuredClone(args)});return Reflect.apply(original,this,args);};}
function bindRpc(){
 const source=fs.readFileSync(path.join(root,'fixtures/source_channel_txutils_reference.txt'),'utf8');assert.equal(crypto.createHash('sha256').update(source).digest('hex'),'38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3');
 const moduleRoot=process.env.TRADELAYER_JS_ROOT||path.resolve(root,'../../../../tradelayer.js'),external=Module.createRequire(path.join(moduleRoot,'package.json')),own={exports:{}};
 const wrapper=vm.runInContext('(function(exports,require,module){\n'+source+'\n})',e.context,{filename:'pinned/txUtils.js'});
 wrapper(own.exports,key=>key==='bitcore-lib-ltc'?external(key):key==='bignumber.js'?BN:key==='./txEncoder.js'?e.load('txEncoder'):key==='./consensus.js'?e.load('consensus'):key==='./client'?{getInstance(){return Promise.resolve({})}}:(()=>{throw Error('Unmapped TxUtils require '+key)})(),own);
 // This is the pre-existing explicit raw-RPC boundary profile. Original raw
 // transaction documents drive unchanged pure classifier/pubkey methods.
 Object.assign(e.adapters['./txUtils'],{getAddressTypeUniversal:own.exports.getAddressTypeUniversal,extractPubkeyByType:own.exports.extractPubkeyByType,async getRawTransaction(...args){const value=structuredClone(e.table('sourceRpcTransactions').find(row=>row.txid===args[0])?.transaction);e.effects.push({kind:'rpc',method:'TxUtils.getRawTransaction',args,returned:structuredClone(value)});return value;}});
}
async function reset(name,options={}){
 e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;unhandled=[];
 const activation=e.load('activation').getInstance();
 if(options.empty){for(const table of e.tables.values())table.splice(0,Infinity);}
 let initOutcome;if(options.init){try{initOutcome={returned:await activation.init(options.init)}}catch(error){initOutcome={thrown:{name:error.name,message:error.message}};}}
 if(options.insuranceRows)e.table('insurance').splice(0,Infinity,...structuredClone(options.insuranceRows));
 if(options.initialized)e.table('tallyMap').push({_id:'$TLinit',initialized:true});if(options.seedRegistry)e.table('activations').push({_id:'activationsList',value:JSON.stringify(activation.initializeTxRegistry())});
 if(options.tallyRows)e.table('tallyMap').splice(0,Infinity,...structuredClone(options.tallyRows));
 if(options.rpc){e.table('sourceRpcTransactions').push({txid:'raw4-first-commit',transaction:{vin:[{scriptSig:{asm:'sig '+'02'+'11'.repeat(32)}}],vout:[]}});bindRpc();}
 for(const method of ['activateTradeLayer'])observe(e.logic,method,'Logic.'+method);
 for(const method of ['getInstance','getTally','loadFromDB','updateBalance','updateChannelBalance','getAddressBalances','hasSufficientChannel'])observe(e.load('tally'),method,'Tally.'+method);
 observe(e.load('tally').prototype,'getAddressBalances','Tally.instance.getAddressBalances');
 for(const method of ['getChannel','loadChannelsRegistry','recordCommitToChannel','handleChannelPubkey','predictColumnForAddress','assignColumnBasedOnAddress','resolveColumn','recordChannelDelta','saveChannelsRegistry','recordParticipantChange'])observe(e.load('channels'),method,'Channels.'+method);
 await flush();e.effects.length=0;emit(name,{reset:options},{initOutcome,outcome:{returned:undefined}});
}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function raw(name,type,payload,sender=A,reference=null,failure){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,reference,0,0,10];let outcome;try{lastParams=await e.types.decodePayload(...args);let returned;if(lastParams.valid===true)returned=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [ns,method]=target.split('.'),receiver=ns==='Logic'?e.logic:e.load('tally');let transport='Immediate',outcome;try{const value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure,unsupported},{transport,outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.table(collection).splice(0,Infinity,...structuredClone(rows));emit(name,{seed:{collection,rows}},{outcome:{returned:undefined}});}
async function bindTally(name){e.effects.length=0;e.context.tallyMap=e.load('tally');emit(name,{bindTally:true},{outcome:{returned:undefined}});}
const balance=(available,vesting=0)=>({amount:available+vesting,available,reserved:0,margin:0,vesting,channelBalance:0}),tallyRows=(available=1000,vesting=0,id=7)=>[{_id:'tallyMap',data:JSON.stringify([[A,{[id]:balance(available,vesting)}]])}];
const amount=v=>(v*1e8).toString(36),send=(v,p=7)=>'0;'+B+';'+p.toString(36)+';'+v;


const sender=()=>e.load('activation').getInstance().getAdmin();
const supply=[];
function auditSupply(name){
 const properties=JSON.parse(e.table('propertyList').find(row=>row._id==='propertyIndex')?.value||'[]'),tally=JSON.parse(e.table('tallyMap').find(row=>row._id==='tallyMap')?.data||'[]');
 const holdings=properties.map(([id,p])=>{let owned=new BN(0),markers=new BN(0),insurance=new BN(0);for(const[,values]of tally){const balance=values[id];if(balance){for(const key of ['available','reserved','margin','channelBalance'])owned=owned.plus(balance[key]||0);markers=markers.plus(balance.vesting||0);}}for(const row of e.table('insurance'))for(const balance of row.value?.balances||[]){if(String(balance.propertyId)===String(id))insurance=insurance.plus(balance.amountAvailable||0).plus(balance.amountVesting||0);}return{id,ticker:p.ticker,declared:String(p.totalInCirculation),tallyOwned:owned.toFixed(),vestingMarker:markers.toFixed(),insuranceOwned:insurance.toFixed(),declaredLessActual:new BN(p.totalInCirculation).minus(owned).minus(insurance).toFixed()};});
 const ids=new Set(properties.map(([id])=>String(id))),uncataloguedTally=[];
 for(const[address,balances]of tally)for(const[property,balance]of Object.entries(balances))if(!ids.has(property))uncataloguedTally.push({address,property,balance});
 const uncataloguedInsurance=[];for(const row of e.table('insurance'))for(const balance of row.value?.balances||[])if(!ids.has(String(balance.propertyId)))uncataloguedInsurance.push(balance);
 supply.push({name,holdings,uncataloguedTally,uncataloguedInsurance,initializationFlag:e.table('tallyMap').find(row=>row._id==='$TLinit')?.initialized??false,consensusEntries:e.load('consensus').consensusVector.length});
}



(async()=>{
 for(const[name,balances]of [
  ['null-first',[null]],
  ['match-before-null',[{propertyId:'1',amountAvailable:'5'},null]],
  ['null-after-nonmatch',[{propertyId:'2',amountAvailable:'7'},null,{propertyId:'1',amountAvailable:'5'}]],
  ['property-string-coercion-failure',[{propertyId:{toString:1},amountAvailable:'5'}]],
 ]){
  await reset('insurance-find-'+name,{empty:true,clientProfile:{chain:'LTC',test:false},init:'LTC',seedRegistry:true,
   insuranceRows:[{key:'1',value:{balances,hedgeRatio:0.5}}]});
  await call('insurance-find-bootstrap-'+name,'Logic.activateTradeLayer',[0,10,'insurance-find']);
 }
 const payload=records.map(row=>JSON.stringify(tag(row))).join('\n')+'\n';
 if(flags.includes('--stdout')){process.stdout.write(payload);return;}
 const stage=path.resolve(__dirname,'..'),file=path.join(stage,'fixtures/v10_insurance_find_vectors.jsonl');fs.mkdirSync(path.dirname(file),{recursive:true});
 if(flags.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),payload);else fs.writeFileSync(file,payload);
 if(flags.includes('--print')){process.stdout.write(payload);return;}
 console.log(JSON.stringify({frames:records.length,calls:records.filter(row=>row.command.target).length,version:lock?'9.1.2':'9.3.1',sha256:crypto.createHash('sha256').update(payload).digest('hex')},null,2));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
