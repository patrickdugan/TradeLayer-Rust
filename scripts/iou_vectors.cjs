#!/usr/bin/env node
"use strict";

// Execute unmodified iou.js in a VM with an in-memory NeDB-shaped adapter.
// No hand-written arithmetic oracle: every fixture state/result is source output.
// Uses the complete archived, SHA256-pinned BigNumber reference.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber, BIG_NUMBER_VERSION } = require("./legacy_numeric_oracle.cjs");

const SOURCE_SHA256 = "7ad5e87e4d72cf57c640c8c15564a3da21dfb4f2e692097ac8aab8215018cf96";
const SOURCE_COMMIT = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
const SCENARIOS = [
    ["buckets", [
        ["loss", 1, 2, 10, "0"],
        ["profit", 1, 2, 10, "-1"],
        ["loss", 1, 2, 10, "10"],
        ["profit", 1, 2, 10, "13"],
        ["delta", 1, 2, 10, "2.5"],
        ["delta", 1, 2, 10, "-0.25"],
        ["reduction", 1, 2, 10, "-"],
        ["loss", 1, 2, 11, "0.5"],
        ["reduction", 1, 2, 11, "-"],
        ["profit", 1, 2, 9, "0.125"],
        ["loss", 3, 2, 9, "4"],
        ["profit", 4, 2, 9, "7"],
        ["total", 1, 2, 9, "-"],
        ["delta", 1, 2, 20, "0"],
        ["delete", 4, 2, 9, "-"],
        ["total", 1, 2, 9, "-"]
    ]],
    ["losers_sign", [
        ["apply", 1, 2, 10, "4"],
        ["profit", 1, 2, 10, "5"],
        ["apply", 1, 2, 10, "2"],
        ["apply", 1, 2, 10, "20"],
        ["apply", 1, 2, 11, "-1"],
        ["apply", 1, 2, 11, "1"]
    ]],
    ["crossing", [
        ["profit", 1, 2, 10, "5"],
        ["loss", 1, 2, 11, "7"],
        ["reduction", 1, 2, 11, "-"],
        ["loss", 1, 2, 11, "5"],
        ["reduction", 1, 2, 11, "-"]
    ]],
    ["claim_rounding", [
        ["accrue", 1, 2, 10, "alice,bob,1,2,1"],
        ["accrue", 1, 2, 10, "alice,bob,1,2,1.123456789012345678901"],
        ["accrue", 1, 2, 10, "same,same,1,2,1"],
        ["accrue", 1, 2, 11, "negative,positive,-4,2,0.000000000000000000005"],
        ["accrue", 1, 2, 11, "none,none,-1,-2,1"],
        ["accrue", 1, 2, 11, "none,none,1,2,-1"],
        ["accrue", 1, 2, 12, "10,2,1,1,2"],
        ["accrue", 1, 2, 12, "01,0,1,1,2"],
        ["accrue", 1, 2, 12, "4294967295,4294967294,1,1,2"]
    ]],
    ["payout", [
        ["accrue", 1, 2, 1, "alice,bob,1,2,9"],
        ["pay", 1, 2, 1, "2"],
        ["loss", 1, 2, 10, "2"],
        ["profit", 1, 2, 10, "10"],
        ["pay", 1, 2, 9, "2"],
        ["pay", 1, 2, 10, "0"],
        ["pay", 1, 2, 10, "100"],
        ["pay", 1, 2, 10, "100"],
        ["pay", 1, 2, 10, "100"],
        ["pay", 1, 2, 10, "100"],
        ["pay", 1, 2, 10, "100"],
        ["pay", 1, 2, 10, "100"],
        ["profit", 1, 2, 11, "1"],
        ["pay", 1, 2, 11, "1"]
    ]],
    ["tiny", [
        ["loss", 1, 2, 1, "0.000000000000000000001"],
        ["loss", 1, 2, 1, "0.000000000000000000005"],
        ["profit", 1, 2, 1, "0.000000000000000000001"],
        ["accrue", 1, 2, 1, "alice,bob,1,2,0.00000000000000000001"],
        ["pay", 1, 2, 1, "0.00000000000000000001"],
        ["save", 1, 2, 1, "alice=0.000000000000000000001,bob=0.000000000000000000001,zero=0,negative=-1"],
        ["pay", 1, 2, 1, "1"],
        ["pay", 1, 2, 1, "1"]
    ]],
    ["delete", [
        ["loss", 0, 0, 0, "3.123456789012345678905"],
        ["accrue", 0, 0, 0, "0,z,1,1,2"],
        ["loss", 3, 0, 1, "2"],
        ["accrue", 3, 0, 1, "a,b,0,1,4"],
        ["loss", 3, 4, 1, "7"],
        ["delete", 0, 0, 1, "-"],
        ["pay", 0, 0, 0, "7"],
        ["delete_property", 3, 0, 1, "-"],
        ["delete_property", 3, 0, 1, "-"]
    ]]
];

function clone(value) {
    return JSON.parse(JSON.stringify(value));
}
function decimal(BigNumber, value) {
    return new BigNumber(value || 0).toFixed();
}
function makeSource(source, sourcePath, BigNumber) {
    const docs = new Map();
    const matches = (doc, query) => Object.entries(query).every(([name, value]) => doc[name] === value);
    const db = {
        findOneAsync: async query => {
            const result = [...docs.values()].find(doc => matches(doc, query));
            return result ? clone(result) : null;
        },
        findAsync: async query => [...docs.values()].filter(doc => matches(doc, query)).map(clone),
        updateAsync: async (query, doc, options) => {
            assert.equal(query._id, doc._id);
            assert.equal(options.upsert, true);
            docs.set(doc._id, clone(doc));
        },
        insertAsync: async doc => {
            assert(!docs.has(doc._id));
            docs.set(doc._id, clone(doc));
        },
        removeAsync: async (query, options) => {
            for (const [id, doc] of docs) {
                if (matches(doc, query)) {
                    docs.delete(id);
                    if (!options.multi) break;
                }
            }
        }
    };
    // Deterministic unique audit IDs; audits remain in the DB and exercise the
    // source's total-query exclusion and deleteForProperty selectors.
    let sequence = 0;
    const sandbox = {
        module: { exports: {} },
        console: { log() {}, warn() {} },
        Date: { now: () => 1000 + sequence++ },
        Math: { random: () => ++sequence / 1000000 },
        require: name => {
            if (name === "bignumber.js") return BigNumber;
            if (name === "./db.js") return { getDatabase: async name => {
                assert.equal(name, "iou"); return db;
            } };
            if (name === "./marginMap.js") return { getInstance: async () => ({}) };
            throw Error(`unexpected dependency ${name}`);
        }
    };
    vm.runInNewContext(source, sandbox, { filename: sourcePath, timeout: 1000 });
    return { Iou: sandbox.module.exports, docs };
}

function state(docs, BigNumber) {
    const buckets = [...docs.values()].filter(doc => doc.type !== "audit" && !doc._id.startsWith("claims:"))
        .sort((a, b) => a.contractId - b.contractId || a.propertyId - b.propertyId)
        .map(doc => [doc.contractId + ":" + doc.propertyId, doc.lastBlock,
            ...["amount", "blockStartAmount", "blockDelta", "blockLosses", "blockProfits"]
                .map(name => decimal(BigNumber, doc[name]))].join("/"))
        .join(";") || "-";
    const maps = [...docs.values()].filter(doc => doc._id.startsWith("claims:"))
        .sort((a, b) => a.contractId - b.contractId || a.propertyId - b.propertyId)
        .map(doc => `${doc.contractId}:${doc.propertyId}/${doc.lastBlock}/` +
            (Object.entries(doc.claims).map(([address, value]) => `${address}=${decimal(BigNumber, value)}`).join(",") || "-"))
        .join(";") || "-";
    return [buckets, maps];
}

async function main() {
    const args = process.argv.slice(2);
    const index = args.indexOf("--source");
    if (index >= 0) assert(args[index + 1], "--source requires a path");
    assert(args.every((arg, i) => arg === "--source" || arg === "--check" || (index >= 0 && i === index + 1)));
    const sourcePath = index >= 0 ? path.resolve(args[index + 1]) : path.join(fixtures, "legacy_iou_source.txt");
    const source = readPinnedSource(sourcePath, SOURCE_SHA256);
    const libraryVersion = BIG_NUMBER_VERSION;
    const BigNumber = loadBigNumber();
    const config = BigNumber.config();
    assert.equal(config.DECIMAL_PLACES, 20);
    assert.equal(config.ROUNDING_MODE, BigNumber.ROUND_HALF_UP);
    const lines = [
        `# Source-executed iou.js state vectors at ${SOURCE_COMMIT}`,
        `# source_sha256=${SOURCE_SHA256}`,
        `# bignumber_version=${libraryVersion}`,
        "# BigNumber default: DECIMAL_PLACES=20 ROUND_HALF_UP; toString(10) rounds stored writes too",
        "# scenario step op contract property block args result buckets claims total_for_property"
    ];
    let count = 0;
    for (const [scenario, steps] of SCENARIOS) {
        const { Iou, docs } = makeSource(source, sourcePath, BigNumber);
        for (const [step, [op, contract, property, block, arg]] of steps.entries()) {
            let result = "void";
            if (op === "loss" || op === "profit" || op === "delta") {
                const method = { loss: "addLoss", profit: "addProfit", delta: "addDelta" }[op];
                result = await Iou[method](contract, property, arg, block) ? "bucket" : "none";
            } else if (op === "apply") {
                await Iou.applyToLosers(contract, new BigNumber(arg), block, property);
            } else if (op === "accrue") {
                const [buyer, seller, buyerPnl, sellerPnl, delta] = arg.split(",");
                result = await Iou.addIouClaims(contract, property, block, buyer, seller, buyerPnl, sellerPnl, delta)
                    ? "claims" : "none";
            } else if (op === "pay") {
                result = (await Iou.payOutstandingIous(contract, property, arg, block))
                    .map(allocation => `${allocation.address}=${decimal(BigNumber, allocation.amount)}`).join(",") || "-";
            } else if (op === "save") {
                const claims = Object.fromEntries(arg.split(",").map(entry => entry.split("=")));
                await Iou.saveClaimMap({ _id: Iou.claimKey(contract, property), contractId: contract,
                    propertyId: property, lastBlock: block, claims });
            } else if (op === "delete") {
                await Iou.delete(contract, property);
            } else if (op === "delete_property") {
                await Iou.deleteForProperty(property);
            } else if (op === "reduction") {
                result = decimal(BigNumber, Iou.blockReductionTowardZero(await Iou.getDoc(contract, property)));
            } else if (op === "total") {
                result = decimal(BigNumber, await Iou.getTotalForProperty(property));
            } else throw Error(`unknown operation ${op}`);
            const [buckets, maps] = state(docs, BigNumber);
            const total = decimal(BigNumber, await Iou.getTotalForProperty(property));
            lines.push([scenario, step, op, contract, property, block, arg, result, buckets, maps, total].join("\t"));
            count++;
        }
    }
    const fixture = lines.join("\n") + "\n";
    const destination = path.join(__dirname, "../fixtures/iou_vectors.tsv");
    if (args.includes("--check")) assert.equal(fs.readFileSync(destination, "utf8"), fixture, "IOU fixture drift");
    else fs.writeFileSync(destination, fixture);
    console.log(`PASS: ${count} transitions executed against SHA-256-pinned iou.js and real BigNumber`);
    console.log("BOUNDARY: synthetic DB, stable typed inputs; no authentication, MarginMap funding, durable DB or economic correction");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
