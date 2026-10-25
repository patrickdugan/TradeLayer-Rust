#!/usr/bin/env node
"use strict";

// Synthetic query oracle for tradeHistoryManager.js at
// tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c.
// --source executes the unmodified, SHA-256-pinned source with an in-memory
// DB stub. No legacy dependencies, RPC, wallets or database files are loaded.
// Without --source this checks the explicitly scoped predicate projection.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

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
            if (name === "bignumber.js") return class UnusedBigNumber {};
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

async function main() {
    const args = process.argv.slice(2);
    const check = args.includes("--check");
    const sourceIndex = args.indexOf("--source");
    const allowed = args.filter((arg, index) => arg === "--check" || arg === "--source" || (sourceIndex >= 0 && index === sourceIndex + 1));
    assert.equal(args.length, allowed.length, "usage: node scripts/trade_history_vectors.cjs [--check] [--source path]");
    if (sourceIndex >= 0) {
        assert(args[sourceIndex + 1], "--source requires a path");
        await checkPinnedSource(args[sourceIndex + 1]);
    }
    const fixture = [
        "# Synthetic query vectors for tradelayer.js@dca0888b tradeHistoryManager.js",
        `# source_sha256=${SOURCE_SHA256}`,
        "# R id kind property1_or_contract property2 height event_index buyer seller liquidation",
        ...RECORDS.map(r => ["R", ...r].join("\t")),
        "# Q kind a b c address_mode expected_ids_or_first_block (- means empty)",
        ...QUERIES.map(q => ["Q", ...q, project(q)].join("\t"))
    ].join("\n") + "\n";
    const destination = path.join(__dirname, "../fixtures/trade_history_vectors.tsv");
    if (check) assert.equal(fs.readFileSync(destination, "utf8"), fixture, "trade history fixture drift");
    else fs.writeFileSync(destination, fixture);
    console.log(`PASS: ${RECORDS.length} synthetic rows and ${QUERIES.length} query vectors`);
    console.log("BOUNDARY: query predicates only; no execution, authorization, PnL or legacy DB migration parity");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
