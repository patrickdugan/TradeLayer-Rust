#!/usr/bin/env node
"use strict";

// Synthetic query oracle for tradeHistoryManager.js at
// tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c.
// --source executes the unmodified, SHA-256-pinned source with an in-memory
// DB stub. No legacy dependencies, RPC, wallets or database files are loaded.
// The archived complete source always runs, with genuine archived BigNumber.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { loadBigNumber } = require("./legacy_numeric_oracle.cjs");

// Git blob bytes use LF; normalize a Windows working checkout's CRLF only.
const SOURCE_SHA256 = "670035f8153abda542c732deee0cf995fcd1e1e7c7b9f50464fce6ab1b5c1569";
const RECORDS = [
    [9, "contract", 9, "-", 0, 0, "ALICE", "bob", false],
    [8, "token", 5, 6, 9, 0, "ALICE", "bob", "-"],
    [5, "contract", 9, "-", 10, 0, "alice", "carol", true],
    [1, "token", 5, 6, 10, 1, "alice", "bob", "-"],
    [3, "token", 5, 7, 10, 2, "alice", "bob", "-"],
    [2, "token", 6, 5, 11, 0, "Alice", "carol", "-"],
    [4, "contract", 9, "-", 12, 0, "alice", "bob", false],
    [6, "contract", 9, "-", 12, 1, "Alice", "dave", true],
    [7, "contract", 10, "-", 12, 2, "alice", "bob", true],
    [10, "token", 5, 6, 13, 0, "bob", "Alice", "-"]
];
const QUERIES = [
    ["token", 5, 6, "Alice", "legacy"],
    ["token", 6, 5, "Alice", "legacy"],
    ["token", 5, 6, "alice", "legacy"],
    ["token", 5, 6, "ALICE", "legacy"],
    ["token", 5, 6, "Alice", "exact"],
    ["token", 5, 5, "alice", "legacy"],
    ["token", 5, 6, "nobody", "legacy"],
    ["contract", 9, "-", "Alice", "legacy"],
    ["contract", 9, "-", "alice", "legacy"],
    ["contract", 9, "-", "ALICE", "legacy"],
    ["contract", 9, "-", "Alice", "exact"],
    ["contract", 10, "-", "alice", "legacy"],
    ["all", 9, "-", "-", "-"],
    ["all", 99, "-", "-", "-"],
    ["range", 9, 10, 12, "-"],
    ["range", 9, 12, 12, "-"],
    ["range", 9, 0, 0, "-"],
    ["range", 9, 13, 20, "-"],
    ["liquidation", 9, 12, "-", "-"],
    ["liquidation", 9, 10, "-", "-"],
    ["liquidation", 10, 12, "-", "-"],
    ["liquidation", 9, 11, "-", "-"],
    ["first", 9, "-", "-", "-"],
    ["first", 10, "-", "-", "-"],
    ["first", 99, "-", "-", "-"]
];

function project(q) {
    const [kind, a, b, c, mode] = q;
    let rows = RECORDS;
    const addressMatches = r => [r[6], r[7]].some(stored =>
        stored === c || (mode === "legacy" && stored === c.toLowerCase()));
    if (kind === "token") rows = rows.filter(r => r[1] === "token" &&
        ((r[2] === a && r[3] === b) || (r[2] === b && r[3] === a)) && addressMatches(r));
    else {
        rows = rows.filter(r => r[1] === "contract" && r[2] === a);
        if (kind === "contract") rows = rows.filter(addressMatches);
        if (kind === "range") rows = rows.filter(r => r[4] >= b && r[4] <= c);
        if (kind === "liquidation") rows = rows.filter(r => r[4] === b && r[8] === true);
        if (kind === "first") return rows.length ? String(rows[0][4]) : "-";
    }
    return rows.map(r => r[0]).join(",") || "-";
}

function field(doc, name) {
    return name.split(".").reduce((value, part) => value?.[part], doc);
}
function matches(doc, query) {
    return Object.entries(query).every(([name, expected]) => {
        if (name === "$and") return expected.every(q => matches(doc, q));
        if (name === "$or") return expected.some(q => matches(doc, q));
        const actual = field(doc, name);
        if (expected && typeof expected === "object") {
            return Object.entries(expected).every(([op, value]) => {
                if (op === "$in") return value.includes(actual);
                if (op === "$gte") return actual >= value;
                if (op === "$lte") return actual <= value;
                if (op === "$regex") return value.test(actual);
                throw Error(`unsupported DB operator: ${op}`);
            });
        }
        return actual === expected;
    });
}

async function checkPinnedSource(sourcePath) {
    const source = fs.readFileSync(sourcePath, "utf8").replace(/\r\n/g, "\n");
    assert.equal(crypto.createHash("sha256").update(source).digest("hex"), SOURCE_SHA256,
        "tradeHistoryManager.js differs from pinned source");
    const docs = RECORDS.map(([id, kind, a, b, height, , buyer, seller, liquidation]) => ({
        _id: `${kind}-${a}-${id}`, key: kind === "token" ? `token-${a}-${b}` : `contract-${a}`,
        type: kind, blockHeight: height,
        trade: { id, buyer, seller, buyerAddress: buyer, sellerAddress: seller, liquidation }
    }));
    const db = {
        findOneAsync: async query => docs.find(d => matches(d, query)),
        findAsync: async query => docs.filter(d => matches(d, query)),
        find: query => {
            let result = docs.filter(d => matches(d, query));
            const cursor = {
                sort: () => { result.sort((a, b) => a.blockHeight - b.blockHeight); return cursor; },
                limit: n => { result = result.slice(0, n); return cursor; },
                exec: callback => callback(null, result)
            };
            return cursor;
        }
    };
    const sandbox = {
        module: {exports: {}}, console: { log: () => {} },
        require: name => {
            if (name === "./db.js") return {getDatabase: async name => {
                assert.equal(name, "tradeHistory"); return db;
            }};
            if (name === "bignumber.js") return loadBigNumber();
            throw Error(`unexpected dependency: ${name}`);
        }
    };
    vm.runInNewContext(source, sandbox, {filename: sourcePath, timeout: 1000});
    const History = sandbox.module.exports;
    const history = new History();
    let checked = 0;
    for (const q of QUERIES) {
        const [kind, a, b, c, mode] = q;
        let result;
        // The source has exact contract lookup but no separate exact-token API.
        if (kind === "token" && mode === "exact") continue;
        if (kind === "token") result = await History.getTokenTradeHistoryForAddress(a, b, c);
        if (kind === "contract") result = mode === "legacy"
            ? await History.getContractTradeHistoryForAddress(a, c)
            : await history.getTradeHistoryForAddress(c, a);
        if (kind === "all") result = await history.loadTradeHistory(a);
        if (kind === "range") result = await History.getTradesForContractBetweenBlocks(a, b, c);
        if (kind === "liquidation") result = await History.getLiquidationTradesForContractAtBlock(a, b);
        if (kind === "first") result = await history.getFirstTradeBlock(a);
        const actual = kind === "first" ? (result === null ? "-" : String(result))
            : result.map(r => r.id).join(",") || "-";
        assert.equal(actual, project(q), JSON.stringify(q));
        checked++;
    }
    console.log(`PASS: ${checked} queries executed against SHA-256-pinned JS source with synthetic DB rows`);
}

async function fullMethodOracle(sourcePath, write) {
    const source=fs.readFileSync(sourcePath,"utf8").replace(/\r\n/g,"\n");
    assert.equal(crypto.createHash("sha256").update(source).digest("hex"),SOURCE_SHA256);
    const BigNumber=loadBigNumber();
    let state;
    const clone=value=>JSON.parse(JSON.stringify(value));
    const observation=value=>JSON.stringify(value,(_,item)=>{
        if(item===undefined)return {$undefined:true};
        if(typeof item==="number"&&!Number.isFinite(item))return {$number:String(item)};
        if(typeof item==="number"&&Object.is(item,-0))return {$number:"-0"};
        if(Object.prototype.toString.call(item)==="[object RegExp]")return {$regex:String(item)};
        return item;
    });
    const hex=text=>Buffer.from(text,"utf8").toString("hex")||"-";
    const db={
        findOneAsync:async query=>{state.calls.push(["findOneAsync",query]);if(state.fail==="findOneAsync")throw Error("injected-findOneAsync");return state.docs.find(doc=>matches(doc,query));},
        findAsync:async query=>{state.calls.push(["findAsync",query]);if(state.fail==="findAsync")throw Error("injected-findAsync");return state.docs.filter(doc=>matches(doc,query));},
        updateAsync:async(query,update,options)=>{
            state.calls.push(["updateAsync",query,update,options]);if(state.fail==="updateAsync")throw Error("injected-updateAsync");
            let index=state.docs.findIndex(doc=>matches(doc,query));
            const previous=index<0?{_id:query._id}:state.docs[index];
            const next=Object.hasOwn(update,"$set")?{...previous,...clone(update.$set)}:clone(update);
            if(index<0)state.docs.push(next);else state.docs[index]=next;return 1;
        },
        find:query=>{
            state.calls.push(["find",query]);if(state.fail==="find")throw Error("injected-find");
            let found=state.docs.filter(doc=>matches(doc,query));
            const cursor={sort(spec){state.calls.push(["sort",spec]);found.sort((a,b)=>a.blockHeight-b.blockHeight);return cursor;},limit(limit){state.calls.push(["limit",limit]);found=found.slice(0,limit);return cursor;},exec(callback){if(state.fail==="exec")callback(Error("injected-exec"));else callback(null,found);}};
            return cursor;
        }
    };
    const module={exports:{}};
    vm.runInNewContext(source,{module,console:{log(){},error(){},table(value){state.table=value;}},require(name){
        if(name==="bignumber.js")return BigNumber;
        if(name==="./db.js")return {getDatabase:async category=>{
            assert.equal(category,"tradeHistory");state.calls.push(["getDatabase",category]);state.dbRequests++;
            if(state.fail==="getDatabase")throw Error("injected-getDatabase");
            if(state.arrayFallback&&state.dbRequests>1)return state.docs;
            return state.noFindAsync?{...db,findAsync:undefined}:db;
        }};
        throw Error(`unexpected source import ${name}`);
    }},{timeout:1000});
    const History=module.exports;
    const instanceMethods=Object.getOwnPropertyNames(History.prototype).filter(name=>name!=="constructor");
    const staticMethods=Object.getOwnPropertyNames(History).filter(name=>typeof History[name]==="function");
    assert.equal(instanceMethods.length+staticMethods.length,18);
    const covered=new Set(), rows=[], quirks=[];
    async function add(name,method,args,docs=[],options={}) {
        state={docs:clone(docs),calls:[],dbRequests:0,table:undefined,...options};
        const history=new History();let result,status="ok";
        try {result=await (staticMethods.includes(method)?History:history)[method](...args);}
        catch(error){status="throw";result={name:error.name,message:error.message};}
        const details={calls:state.calls,documents:state.docs};
        if(result?.openTrades&&result?.closeTrades){details.aliases=[];for(let a=0;a<result.openTrades.length;a++)for(let b=0;b<result.closeTrades.length;b++)if(result.openTrades[a]===result.closeTrades[b])details.aliases.push([a,b]);}
        if(state.table!==undefined){details.tableWasPromise=typeof state.table?.then==="function";try{details.tableResolution=await state.table;}catch(error){details.tableError=error.name+":"+error.message;}}
        const cells=["M",name,method,hex(observation(args)),status,hex(observation(result)),hex(observation(details))];
        rows.push(cells.join("\t"));covered.add(method);
        if(options.quirk)quirks.push(["Q",...cells.slice(1),options.quirk].join("\t"));
    }
    const flat={id:1,contractId:9,buyerAddress:"alice",sellerAddress:"bob",amount:2,price:100,block:1,buyerTx:"buy",sellerTx:"sell",buyerClose:0,sellerClose:0,liquidation:false};
    const wrap=(trade,height)=>({buyerAddress:trade.buyerAddress,sellerAddress:trade.sellerAddress,blockHeight:height,trade});
    const aggregate=trades=>[{_id:"contract-9",trades}];
    const legacy=[{_id:"contract-9-id",key:"contract-9",type:"contract",blockHeight:1,trade:flat},
        {_id:"contract-9-id2",key:"contract-9",type:"contract",blockHeight:2,trade:{...flat,id:2,liquidation:true}}];
    await add("load-aggregate-array","loadTradeHistory",[9],aggregate([flat]));
    await add("load-single-trade","loadTradeHistory",[9],[{_id:"contract-9",trade:flat}]);
    await add("load-single-document","loadTradeHistory",[9],[{_id:"contract-9",key:"contract-9",buyer:"alice"}]);
    await add("load-legacy-prefix","loadTradeHistory",[9],legacy);
    await add("load-single-null-trade","loadTradeHistory",[9],[{_id:"contract-9",trade:null}]);
    await add("load-regex-interpolation","loadTradeHistory",[".*"],legacy,{quirk:"unescaped-contract-string-is-regexp-pattern"});
    await add("load-last-resort-real-handle","loadTradeHistory",[9],legacy,{noFindAsync:true,quirk:"DB-handle-is-not-array-so-last-resort-always-empty"});
    await add("load-last-resort-array-adapter","loadTradeHistory",[9],legacy,{noFindAsync:true,arrayFallback:true,quirk:"explicit-second-getDatabase-array-adapter-characterizes-unreachable-real-handle-branch"});
    await add("instance-address-flat-aliases","getTradeHistoryForAddress",["alice",9],aggregate([flat,{...flat,buyerAddress:"",counterpartyAddress:"alice"},{...flat,buyer:"not-alice",buyerAddress:"alice",sellerAddress:"other"}]));
    const tokenDocs=[{_id:"token-5-6-a",key:"token-5-6",trade:{buyer:"alice",seller:"bob",id:1}},{_id:"token-6-5-b",key:"token-6-5",trade:{buyer:"Alice",seller:"bob",id:2}}];
    await add("static-token-pair-case","getTokenTradeHistoryForAddress",[6,5,"ALICE"],tokenDocs);
    await add("static-token-error-string","getTokenTradeHistoryForAddress",[5,6,"alice"],tokenDocs,{fail:"findAsync",quirk:"token-query-swallows-error-and-returns-string"});
    await add("static-contract-case","getContractTradeHistoryForAddress",[9,"ALICE"],legacy);
    await add("static-contract-error","getContractTradeHistoryForAddress",[9,"alice"],legacy,{fail:"findAsync",quirk:"contract-query-propagates-error-unlike-token-query"});
    await add("position-flat-shape-error","getPositionHistoryForContract",["alice",9],aggregate([flat]),{quirk:"load-unwrapped-flat-rows-but-consumer-dereferences-trade.trade"});
    await add("position-wrapped-buyer","getPositionHistoryForContract",["alice",9],aggregate([wrap(flat,1),wrap({...flat,amount:3,price:110,block:2},2)]));
    await add("position-wrapped-seller-mutation","getPositionHistoryForContract",["bob",9],aggregate([wrap(flat,1)]),{quirk:"loaded-seller-row-amount-mutated-not-a-DB-write"});
    await add("position-strict-contract-type","getPositionHistoryForContract",["alice","9"],[{_id:"contract-9",trades:[wrap(flat,1)]}],{quirk:"numeric-inner-contract-does-not-equal-string-contract"});
    await add("txids-missing-contract-forward","getTxIdsForContract",["alice",9,"close"],aggregate([wrap(flat,1)]),{quirk:"queries-contract-undefined-and-ignores-action"});
    await add("txids-explicit-undefined-key","getTxIdsForContract",["alice",9,"ignored"],[{_id:"contract-undefined",trades:[wrap(flat,1)]}],{quirk:"only-accidentally-works-with-contract-undefined-document"});
    await add("category-opening","getCategorizedTrades",["alice",9],aggregate([wrap(flat,1)]));
    await add("category-actual-close-field-name","getCategorizedTrades",["alice",9],aggregate([wrap({...flat,buyerClose:1},1)]),{quirk:"reads-buyerClosed-but-source-orderbook-stores-buyerClose"});
    await add("category-buyer-flip-alias","getCategorizedTrades",["alice",9],aggregate([wrap({...flat,buyerClose:1,buyerClosed:1,buyerFullClose:false,flipLong:3},1)]),{quirk:"same-object-in-open-and-close-mutated-to-flip-amount"});
    await add("category-seller-flip-wrong-amount","getCategorizedTrades",["bob",9],aggregate([wrap({...flat,sellerClose:1,sellerClosed:1,sellerFullClose:false,flipShort:3},1)]),{quirk:"seller-flip-reuses-sellerClosed-instead-of-flipShort-and-aliases"});
    await add("category-self-trade-alias","getCategorizedTrades",["alice",9],aggregate([wrap({...flat,sellerAddress:"alice"},1)]),{quirk:"one-individualTrade-object-reused-for-both-participants"});
    await add("category-missing-close-flags","getCategorizedTrades",["alice",9],aggregate([wrap({...flat,buyerClose:undefined,sellerClose:undefined},1)]),{quirk:"undefined-close-flag-enters-nullish-close-branch"});
    await add("average-method","computeAverageEntryPrices",[[{price:"100",amount:"2"},{price:"110",amount:"-3"}]]);
    await add("track-open-not-cumulative","trackPositionHistory",["alice",9],aggregate([wrap(flat,1),wrap({...flat,amount:3,price:110,block:2},2)]),{quirk:"prevBlockHeight-never-updated-and-position-reset-after-each-nonzero-block"});
    await add("track-number-minus-error","trackPositionHistory",["alice",9],aggregate([wrap({...flat,buyerClose:1,buyerClosed:1,buyerFullClose:true,flipLong:1},1)]),{quirk:"Number-average-has-no-minus-method"});
    await add("track-zero-block-not-pushed","trackPositionHistory",["alice",9],aggregate([wrap({...flat,block:0},0)]),{quirk:"zero-block-equals-initial-prevBlockHeight-and-is-never-output"});
    await add("breakup-method","breakUpOpeningTrades",[[{i:0,blockHeight:2},{i:1,blockHeight:3}],[{blockHeight:1}]]);
    await add("lifo-composed-missing-fields","calculateLIFOEntry",["alice",1,9],aggregate([wrap(flat,1)]),{quirk:"category-amount-block-fields-do-not-supply-tradedAmount-blockHeight"});
    await add("lifo-empty","calculateLIFOEntry",["alice",1,9]);
    await add("save-pnl","savePNL",[{address:"alice",contractId:9,height:1,accountingPNL:5,settlementPNL:3}] );
    await add("save-pnl-same-height-overwrite","savePNL",[{address:"alice",contractId:9,height:1,accountingPNL:8}],[{_id:"rPNL-alice-9-1",value:"{\"accountingPNL\":5}"}],{quirk:"same-owner-contract-height-overwrites-previous-PnL"});
    await add("save-json-value","save",["custom",{x:1,nested:[null,true,"text"]}]);
    await add("save-error-propagates","save",["custom",{x:1}],[],{fail:"updateAsync",quirk:"save-catches-logs-and-rethrows"});
    await add("display-promise","displayPositionHistory",["alice",9],aggregate([wrap(flat,1)]),{quirk:"console.table-receives-unawaited-Promise-and-method-returns-undefined"});
    await add("save-trade-missing-uuid","saveTrade",[{key:"contract-9",blockHeight:1,trade:flat}],[],{quirk:"uuidv4-never-imported-so-normal-method-always-rejects-before-update"});
    await add("range-inclusive","getTradesForContractBetweenBlocks",[9,1,2],legacy);
    await add("first-block","getFirstTradeBlock",[9],legacy.slice().reverse());
    await add("first-block-empty","getFirstTradeBlock",[99],legacy);
    await add("first-block-callback-error","getFirstTradeBlock",[9],legacy,{fail:"exec",quirk:"cursor-callback-errors-reject-Promise"});
    await add("liquidation-exact-true","getLiquidationTradesForContractAtBlock",[9,2],[...legacy,{_id:"contract-9-x",key:"contract-9",blockHeight:2,trade:{...flat,id:3,liquidation:1}}]);
    for(const method of [...instanceMethods,...staticMethods])assert(covered.has(method),`uncovered source method ${method}`);

    function numberBits(number){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(number);return bytes.toString("hex");}
    const weighted=[[],[["100","2"],["110","-3"]],[["1","0"],["-1","0"]],[["-0.00000000000000000001","1"],["0","100"]],[["1","3"],["2","2"]],[["1e20","1e-20"],["-1e20","-1e-20"]]];
    let seed=0x7a49dca0;
    for(let i=0;i<96;i++) {const entries=[];for(let j=0;j<4;j++){seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;const price=((seed|0)/1000000).toFixed(6);seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;const amount=((seed|0)/100000000).toFixed(8);entries.push([price,amount]);}weighted.push(entries);}
    const observer=new History();
    for(let i=0;i<weighted.length;i++){const entries=weighted[i];const result=observer.computeAverageEntryPrices(entries.map(([price,amount])=>({price,amount})));rows.push(["A",`weighted-${i}`,entries.map(pair=>pair.join(",")).join(";")||"_",numberBits(result)].join("\t"));}
    const breakup=[ [[],[]], [[2],[1]], [[1,2,2,4],[1,3]], [[null,null,null],[null]], [[3,1,3],[2]], [[0,1],[0]], [[2,2],[1]] ];
    for(let i=0;i<96;i++){const opens=[],closes=[];for(let j=0;j<6;j++){seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;opens.push((seed>>>0)%7===0?null:(seed>>>0)%10);seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;closes.push((seed>>>0)%5===0?null:(seed>>>0)%10);}breakup.push([opens,closes]);}
    const heights=values=>values.map(value=>value===null?"-":String(value)).join(",")||"_";
    for(let i=0;i<breakup.length;i++){const [opens,closes]=breakup[i];const result=observer.breakUpOpeningTrades(opens.map((height,index)=>height===null?{index}:{index,blockHeight:height}),closes.map(height=>height===null?{}:{blockHeight:height}));rows.push(["B",`breakup-${i}`,heights(opens),heights(closes),result.map(group=>group.map(row=>row.index).join(",")||"_").join(";")].join("\t"));}
    function emit(name,contents,header){const output=header+"\n"+contents.join("\n")+"\n";const filename=path.join(__dirname,"../fixtures",name);if(write)fs.writeFileSync(filename,output);else assert.equal(fs.readFileSync(filename,"utf8").replace(/\r\n/g,"\n"),output,`${name} drift`);}
    emit("trade_history_method_vectors.tsv",rows,"# Complete unchanged tradeHistoryManager.js with genuine BigNumber9.3.1; explicit metadata DB adapter.\n# M name method args-observation-JSONhex ok|throw result-observation-JSONhex details-observation-JSONhex\n# A name price,amount;... result-f64bits; B name open-heights close-heights index-groups. _=empty, -=missing height.\n# Observation JSON tags preserve undefined/nonfinite/-0; aliases reported explicitly; no source expression rewritten.");
    emit("trade_history_quirks.tsv",quirks,"# Q name method args-observation-JSONhex ok|throw result-observation-JSONhex details-observation-JSONhex source-quirk\n# Errors and broken compositions are observations, not repaired source successes or settlement authority.");
    console.log(`PASS: all18 source methods; ${rows.filter(row=>row.startsWith("M\t")).length} method scenes, ${weighted.length} weighted means, ${breakup.length} breakup cases, ${quirks.length} explicit quirks`);
}

async function main() {
    const args = process.argv.slice(2);
    const write = args.includes("--write");
    const check = !write;
    const sourceIndex = args.indexOf("--source");
    const allowed = args.filter((arg, index) => arg === "--write" || arg === "--check" || arg === "--source" || (sourceIndex >= 0 && index === sourceIndex + 1));
    assert.equal(args.length, allowed.length, "usage: node scripts/trade_history_vectors.cjs [--write|--check] [--source path]");
    assert(!(write && args.includes("--check")), "choose --write or --check");
    const sourcePath = sourceIndex >= 0 ? args[sourceIndex + 1] : path.join(__dirname,"../fixtures/legacy_trade_history_source.txt");
    assert(sourcePath, "--source requires a path");
    await checkPinnedSource(sourcePath);
    const fixture = [
        "# Synthetic query vectors for tradelayer.js@dca0888b tradeHistoryManager.js",
        `# source_sha256=${SOURCE_SHA256}`,
        "# R id kind property1_or_contract property2 height event_index buyer seller liquidation",
        ...RECORDS.map(r => ["R", ...r].join("\t")),
        "# Q kind a b c address_mode expected_ids_or_first_block (- means empty)",
        ...QUERIES.map(q => ["Q", ...q, project(q)].join("\t"))
    ].join("\n") + "\n";
    const destination = path.join(__dirname, "../fixtures/trade_history_vectors.tsv");
    if (check) assert.equal(fs.readFileSync(destination, "utf8").replace(/\r\n/g,"\n"), fixture, "trade history fixture drift");
    else fs.writeFileSync(destination, fixture);
    console.log(`PASS: ${RECORDS.length} synthetic rows and ${QUERIES.length} query vectors`);
    await fullMethodOracle(sourcePath, write);
    console.log("BOUNDARY: source observations and explicit reporting repairs; no execution or settlement authority");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
