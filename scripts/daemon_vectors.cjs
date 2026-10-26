#!/usr/bin/env node
"use strict";
// COMPLETE unchanged main.js; source method bodies are never rewritten. Every
// external dependency is an explicit inert/recording boundary. Selected caller
// methods are isolated only to terminate otherwise unbounded source loops.
const assert=require("node:assert/strict"),fs=require("node:fs"),path=require("node:path"),vm=require("node:vm");
const {fixtures,readPinnedSource}=require("./legacy_numeric_oracle.cjs");
const args=process.argv.slice(2),at=args.indexOf("--source"),SHA="006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009";
assert.ok(!(args.includes("--check")&&args.includes("--write")));if(at>=0)assert.ok(args[at+1]);
const source=readPinnedSource(at<0?path.join(fixtures,"legacy_main_source.txt"):args[at+1],SHA);
const rows=[],coverage=new Set(),hex=x=>Buffer.from(JSON.stringify(x)).toString("hex");
const emit=(name,value)=>rows.push(["V",name,hex(value)].join("\t"));
async function load(options={}){
 const c={events:[],delays:[],meta:new Map(),seen:new Set(),indexRows:[],blockTx:[],tip:3,maxIndex:1,indexExists:true,net:{networkactive:true,connections:1},netResults:[],fetchErrors:[],exit:[],logicError:false};
 const ev=(name,...values)=>c.events.push([name,...values]);
 const client={chain:"BTC",async getChain(){ev("getChain");return"BTC"},async getTest(){return false},async getBlockCount(){return c.tip},async getNetworkInfo(){const v=c.netResults.length?c.netResults.shift():c.net;if(v instanceof Error)throw v;return v}};
 const stores={txIndex:{async findAsync(){return c.indexRows}},consensus:{async findOneAsync(q){return c.meta.has(q._id)?{value:c.meta.get(q._id)}:null},async updateAsync(q,u){c.meta.set(q._id,u.$set.value);ev("meta",q._id,u.$set.value)}}};
 const database={initialized:true,async getDatabase(n){assert.ok(stores[n],"unexpected DB "+n);return stores[n]},async init(){ev("db.init")}};
 const indexInstance={parseBlock:2,async initializeOrLoadDB(h){ev("initializeOrLoadDB",h)}};
 const TxIndex={async getInstance(){return indexInstance},async findMaxIndexedBlock(){return c.maxIndex},async checkForIndex(){return c.indexExists},async initializeIndex(h){ev("initializeIndex",h)},async extractBlockData(h){ev("extractBlockData",h);return h},async processBlockData(){ev("processBlockData");return c.blockTx},async upsertTxValidityAndReason(id,t,v,r){ev("validity",id,t,v,r||"")},async fetchBlockData(h){ev("fetch",h);if(c.fetchErrors.length)throw c.fetchErrors.shift();return {hash:"h"+h,previousblockhash:"p"+h}}};
 const persistence={async checkForReorgForNewBlock(h,p){ev("checkReorg",h,p);return false},async detectAndHandleReorg(){return {restoredFrom:0,commonAncestor:0}},async recordBlockHeader(...a){ev("header",...a)},async maybeCheckpoint(h){ev("checkpoint",h)},async handleReorg(){ev("handleReorg")}};
 const Activation={getInstance(){return {chain:"BTC",test:false,adminAddress:"admin",async init(){ev("activation.init")}}}};
 const Logic={async typeSwitch(type,p){ev("logic",type,p.block);if(c.logicError)throw Error("injected logic failure")},async settleLiquidityRewards(h){ev("rewards",h)}};
 const modules={"./txIndex.js":TxIndex,"./activation.js":Activation,"./persistence.js":{async getInstance(){return persistence}},"./types.js":{async decodePayload(id,type){return {valid:true,type,txid:id,reason:""}}},"./logic.js":Logic,
 "./consensus.js":{async checkIfTxProcessed(id){return c.seen.has(id)},async markTxAsProcessed(id,p){c.seen.add(id);ev("processed",id,p.valid,p.reason||"")}},
 "./tally.js":{getInstance(){return{}},async flushQueuedFeeAccruals(h){ev("fees",h)},async applyVesting(){ev("vesting")}},
 "./orderbook.js":{async processQueuedOnChainOrdersForBlock(h){ev("onchain",h)},async processQueuedChannelTradesForBlock(h){ev("channel",h)}},"./channels.js":{async processWithdrawals(h){ev("withdrawals",h)}},"./clearing.js":{async clearingFunction(h){ev("clearing",h)}},
 "./volumeIndex.js":{async getCumulativeVolumes(){return {ltcPairTotalVolume:0,globalCumulativeVolume:0}},async getBlockVolumes(){return{global:0,ltcPairs:0}}},"./vesting.js":{async updateVesting(){ev("updateVesting");return null}},"./txUtils.js":{async getLatestBlockHeight(){return c.tip}},"./reOrg.js":{},"./validity.js":{},"./marginMap.js":{},"./property.js":{},"./contractRegistry.js":{},"./oracle.js":{},"./txEncoder.js":{},"./amm.js":{},"./txDecoder.js":{}};
 let release;const init=()=>options.defer?new Promise(r=>{release=()=>r({Client:client,Db:database})}):Promise.resolve({Client:client,Db:options.missingDb?undefined:database});
 const module={exports:{}};vm.runInNewContext(source,{module,exports:module.exports,console:{log(){},warn(){},error(){}},process:{exit(code){c.exit.push(code)}},setTimeout(fn,ms){c.delays.push(ms);fn()},require(name){if(name==="./init")return init;if(name==="events"||name==="util")return require(name);if(name==="fs")return{};assert.ok(Object.hasOwn(modules,name),"unexpected module "+name);return modules[name]}},{filename:"complete-pinned-main.js",timeout:1000});
 const Main=module.exports;
 for(const name of Object.getOwnPropertyNames(Main.prototype).filter(x=>x!=="constructor")){const fn=Main.prototype[name];Main.prototype[name]=function(...a){coverage.add(name);return fn.apply(this,a)}}
 for(const name of["getInstance","getGenesisBlock","getLastBlock"]){const fn=Main[name];Main[name]=function(...a){coverage.add(name);return fn.apply(this,a)}}
 if(!options.defer)for(let i=0;i<6;i++)await Promise.resolve();
 return{Main,c,release,client,database};
}
const tx=(id,payload)=>({txId:id,payload,marker:"tl",sender:{senderAddress:"alice",amount:100000000},reference:{address:"bob",amount:100000000}});
(async()=>{
 const early=await load({defer:true});let error;try{await early.Main.getInstance()}catch(e){error=e.name}assert.equal(error,"TypeError");assert.equal(early.Main.isInitializing,true);assert.equal((await early.Main.getInstance()),early.Main.instance);emit("singleton-published-before-client",[error,early.Main.isInitializing,!!early.Main.instance,early.Main.instance.client===undefined]);early.release();
 const missing=await load({missingDb:true});const missingMain=await missing.Main.getInstance();try{await missingMain.initialize()}catch(e){emit("missing-db-init-dereference",e.name)}
 const f=await load(),{Main,c}=f,m=await Main.getInstance();
 const instanceMethods=Object.getOwnPropertyNames(Main.prototype).filter(x=>x!=="constructor"),methods=[...instanceMethods,"getInstance","getGenesisBlock","getLastBlock"].sort();
 const originalSync=m.syncIfNecessary;m.syncIfNecessary=async()=>{c.events.push(["isolated-sync-boundary"])};
 await m.initialize();assert.equal(m.genesisBlock,880000);emit("source-genesis-selection",m.genesisBlock);
 assert.equal(await m.getCurrentBlockHeight(),3);await m.initOrLoadTxIndex();await m.syncIndex(3);await m.constructOrLoadConsensus();
 c.indexRows=[{_id:"tx-1-a",value:tx("a","mdata")},{_id:"tx-1-b",value:tx("b","kdata")},{_id:"tx-1-c",value:tx("c","4data")}];
 c.events=[];await m.constructConsensusFromIndex(1);const order=c.events.filter(x=>x[0]==="logic").map(x=>x[1]);assert.deepEqual(order,[20,4,22]);emit("catchup-funding-order",order);emit("catchup-postprocessing",c.events.filter(x=>["fees","rewards","onchain","channel","withdrawals","clearing"].includes(x[0])));
 c.events=[];c.logicError=true;await m.processTxSet([{value:tx("logic-fail","0data")}],2);assert.equal(c.seen.has("logic-fail"),true);emit("logic-failure-marked-processed",c.events);c.logicError=false;
 c.events=[];await m.processTx([tx("realtime-unbound","idata")],2);const processed=c.events.find(x=>x[0]==="processed");assert.equal(processed[2],false);assert.match(processed[3],/activationInstance is not defined/);emit("realtime-unbound-activation",processed);
 c.events=[];await m.processTx([tx("real-zero","0data")],2);emit("realtime-type-zero",c.events);
 await m.saveMaxProcessedHeight(1);await m.saveTrackHeight(2);assert.equal(await m.loadMaxProcessedHeight(),1);assert.equal(await m.loadTrackHeight(),2);emit("height-markers",[await m.loadMaxProcessedHeight(),await m.loadTrackHeight()]);emit("lag",await m.checkBlockLag());emit("sync",await m.checkSync());
 assert.equal(m.setPause(),true);assert.equal(m.setPause(),false);
 const originalIncoming=m.processIncomingBlocks;let starts=0;m.processIncomingBlocks=()=>{starts++;return new Promise(()=>{})};await originalSync.call(m);assert.equal(starts,1);emit("sync-fire-and-forget",starts);m.processIncomingBlocks=originalIncoming;
 for(const net of[{networkactive:false,connections:1},{networkactive:true,connections:0},{networkactive:true,connections:2}]){c.net=net;emit("network-"+net.networkactive+"-"+net.connections,await m.checkNetworkStatus())}
 c.netResults=[{networkactive:false,connections:0},{networkactive:true,connections:1}];c.delays=[];assert.equal(await m.enterRecoveryMode(1,3),2);emit("recovery-resumes-saved-height",[2,c.delays]);
 c.fetchErrors=Array.from({length:3},()=>Object.assign(Error("timeout"),{code:"ETIMEDOUT"}));c.events=[];c.delays=[];await m.fetchWithRetry(3);assert.deepEqual(c.delays,[1000,2000,4000]);emit("fetch-four-attempts",[c.events.filter(x=>x[0]==="fetch").length,c.delays]);
 c.blockTx=[tx("live-funding","4body"),tx("live-transfer","mbody"),tx("live-other","kbody")];c.events=[];await m.processBlock({hash:"h3",previousblockhash:"h2"},3);emit("live-pre-apply-checkpoint",c.events.slice(0,4));assert.deepEqual(c.events.slice(0,4).map(x=>x[0]),["checkReorg","header","checkpoint","processBlockData"]);
 await m.blockHandlerMid([],4);await m.blockHandlerEnd("h4",4);await m.handleReorg(4);
 c.tip=0;c.meta.set("TrackHeight",0);m.setPause();await m.processIncomingBlocks(0,0,0);m.setPause();emit("paused-loop-exit",true);
 assert.equal(await Main.getGenesisBlock(),880000);assert.equal(await Main.getLastBlock(),0);await m.delay(7);await m.shutdown();assert.deepEqual(c.exit,[0]);emit("shutdown-no-flush",c.exit);
 assert.deepEqual([...coverage].sort(),methods,"every active source method executed");rows.push(["M",methods.length,methods.join(",")].join("\t"));
 const text="# Complete pinned main.js observations; external calls are explicit recording mocks\n"+rows.join("\n")+"\n",target=path.join(fixtures,"daemon_vectors.tsv");if(args.includes("--check"))assert.equal(fs.readFileSync(target,"utf8").replace(/\r\n/g,"\n"),text);else fs.writeFileSync(target,text);console.log("main source oracle: "+rows.length+" observations; "+methods.length+" active methods plus constructor");
})().catch(e=>{console.error(e);process.exitCode=1});
