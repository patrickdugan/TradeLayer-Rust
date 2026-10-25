#!/usr/bin/env node
"use strict";

// Execute the unmodified pinned vaults.js with an in-memory asynchronous DB.
// The archived real BigNumber source is loaded; no wallet/chain/legacy DB runs.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");

const SOURCE_SHA256 = "3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983";
const OPERATIONS = [
    ["C", "s-1-2"],
    ["U", "s-1-2", "-3", "5", "7", "10"],
    ["C", "s-1-3"],
    ["U", "s-1-3", "-4", "11", "13", "20"],
    ["U", "s-2-2", "-9", "17", "19", "30"], // source auto-creates
    ["R", "s-1-2", "1", "2", "3", "-4"],
    // Direct DB/map-shaped string seeds exercise BigNumber query arithmetic,
    // not += update parity (source += concatenates string balances).
    ["S", "s-3-2", "0", "0.000000004", "0", "1.000000004"],
    ["S", "s-3-3", "0", "0.000000004", "0", "0.000000004"],
    ["S", "s-4-2", "0", "1.234567895", "0", "0.000000005"]
];
const QUERIES = [
    ["B", "1", "31"], ["B", "2", "36"], ["B", "3", "0.00000001"],
    ["B", "4", "1.2345679"], ["B", "99", "0"],
    ["O", "1", "26"], ["O", "2", "30"], ["O", "3", "1.00000001"],
    ["O", "4", "0.00000001"], ["O", "99", "0"],
    ["O", "s-1-2", "6"], ["O", "s-1-3", "20"], ["O", "s-99-2", "0"]
];
const QUIRKS = [
    ["create_save_value", "undefined"], ["create_repeated_resets", "0"],
    ["exists_missing", "true"], ["register_vault_link", "undefined"],
    ["find_registered_synthetic", "null"], ["find_compound_vault", "null"],
    ["generate_id", "1-2"], ["parse_generated", "Error:Invalid synthetic token ID format"],
    ["parse_three_parts", "s,1,2"], ["parse_empty_parts", ",,"],
    ["invalid_property_valid_contract", "true"],
    ["funding_scalar_contracts", "-3"], ["funding_scalar_saves", "0"],
    ["funding_object_contracts", "11"], ["funding_object_save_value", "undefined"],
    ["redeem_missing", "undefined"], ["update_string_concatenates", "10-2"],
    ["send_direct", "ReferenceError:TallyMap is not defined"],
    ["send_channel", "TypeError:this.getChannel is not a function"],
    ["trade_direct", "ReferenceError:Orderbook is not defined"],
    ["trade_channel", "TypeError:this.getChannel is not a function"],
    ["post_margin", "TypeError:SynthRegistry.isValidSyntheticTokenId is not a function"],
    ["reuse_no_flags", "TypeError:this.generateVaultId is not a function"],
    ["rebase_registered", "Error:Synthetic token not found"],
    ["reload_saved_vault_count", "0"], ["reload_saved_synthetic_count", "0"],
    ["numeric_string_collateral_query", "0"], ["numeric_string_outstanding_query", "0"],
    ["update_accepts_negative_balances", "-1,-2,-3"], ["redeem_accepts_negative_balances", "1,-1,-1,-1"]
];

async function caught(action) {
    try { return String(await action()); }
    catch (error) { return `${error.name}:${error.message}`; }
}

async function checkSource(sourcePath) {
    const source = readPinnedSource(sourcePath, SOURCE_SHA256);
    const BigNumber = loadBigNumber();
    assert.equal(BigNumber.config().ROUNDING_MODE, BigNumber.ROUND_HALF_UP);
    const tables = new Map(), saves = [];
    const table = name => {
        if (!tables.has(name)) tables.set(name, new Map());
        return tables.get(name);
    };
    const sandbox = {
        module: {exports: {}}, console: {log() {}, error() {}},
        require: name => {
            if (name === "bignumber.js") return BigNumber;
            if (name === "./db.js") return {getDatabase: async name => ({
                findOneAsync: async query => query._id === undefined
                    ? table(name).values().next().value ?? null
                    : table(name).get(query._id) ?? null,
                updateAsync: async (query, value, options) => {
                    assert.equal(options.upsert, true);
                    assert.equal(query._id, value._id);
                    table(name).set(query._id, value); saves.push([name, value]);
                }
            })};
            if (name === "./property.js") return {load: async () => {}, propertyIndex: new Map()};
            if (name === "./contractRegistry.js") return {getContractInfo: async id => id === "2" ? {id: 2} : null};
            if (name === "./marginMap") return {};
            throw Error(`unexpected source dependency: ${name}`);
        }
    };
    vm.runInNewContext(source, sandbox, {filename: sourcePath, timeout: 1000});
    const Registry = sandbox.module.exports;
    for (const [op, id, contracts, margin, available, outstanding] of OPERATIONS) {
        const [, property, contract] = id.split("-");
        if (op === "C") await Registry.createVault(Number(property), Number(contract));
        if (op === "U") await Registry.updateVault(id,
            {contracts: Number(contracts), margin: Number(margin)}, Number(outstanding), Number(available));
        if (op === "R") await Registry.updateVaultRedeem(id,
            {contracts: Number(contracts), margin: Number(margin), available: Number(available)}, Number(outstanding));
        if (op === "S") Registry.vaults.set(id, {propertyId: Number(property), contractId: Number(contract), contracts, margin, available, outstanding});
    }
    for (const [kind, property, expected] of QUERIES) {
        const id = property.startsWith("s-") ? property : Number(property);
        const actual = kind === "B" ? await Registry.getTotalBalanceForProperty(id)
            : await Registry.getTotalOutstandingForProperty(id);
        assert.equal(actual.toFixed(), expected, `${kind} ${property}`);
    }
    const observed = new Map();
    const record = (key, value) => observed.set(key, String(value));
    record("numeric_string_collateral_query", await Registry.getTotalBalanceForProperty("1"));
    record("numeric_string_outstanding_query", await Registry.getTotalOutstandingForProperty("1"));
    await Registry.updateVault("s-9-2", {}, -3, -2);
    await Registry.updateVault("s-9-2", {margin: -1}, 0, 0);
    const negativeUpdate = await Registry.getVault("s-9-2");
    record("update_accepts_negative_balances", [negativeUpdate.margin, negativeUpdate.available, negativeUpdate.outstanding].join(","));
    await Registry.createVault(9, 3);
    await Registry.updateVaultRedeem("s-9-3", {contracts: 1, margin: 1, available: 1}, -1);
    const negativeRedemption = await Registry.getVault("s-9-3");
    record("redeem_accepts_negative_balances", [negativeRedemption.contracts, negativeRedemption.margin, negativeRedemption.available, negativeRedemption.outstanding].join(","));
    const id = await Registry.createVault(8, 9);
    record("create_save_value", table("vaults").get(id).value);
    await Registry.updateVault(id, {contracts: 1, margin: 2}, 3, 4);
    await Registry.createVault(8, 9);
    record("create_repeated_resets", (await Registry.getVault(id)).outstanding);
    record("exists_missing", await Registry.exists("absent"));
    await Registry.registerSyntheticToken("1-2", 2, 1);
    record("register_vault_link", await Registry.getVaultId("1-2"));
    record("find_registered_synthetic", await Registry.findSyntheticTokenIdByVaultId("s-1-2"));
    record("find_compound_vault", await Registry.findVaultIdByCompoundIdentifier(1, 2));
    record("generate_id", Registry.generateSyntheticTokenId(1, 2));
    record("parse_generated", await caught(() => Registry.parseSyntheticTokenId("1-2")));
    const parsed = await Registry.parseSyntheticTokenId("s-1-2");
    record("parse_three_parts", [parsed.underlyingPropertyId, parsed.hedgeContractId, parsed.vaultId].join(","));
    const empty = await Registry.parseSyntheticTokenId("--");
    record("parse_empty_parts", [empty.underlyingPropertyId, empty.hedgeContractId, empty.vaultId].join(","));
    record("invalid_property_valid_contract", await Registry.isSyntheticProperty("999-2"));
    Registry.vaults.set("s-7-2", {contracts: -3, margin: 0, available: 0, outstanding: 0});
    const saveCount = saves.length;
    await Registry.applyPerpetualSwapFunding("s-7-2", 2, 0.1);
    record("funding_scalar_contracts", Registry.vaults.get("s-7-2").contracts);
    record("funding_scalar_saves", saves.length - saveCount);
    Registry.vaults.set("s-7-3", {contracts: {3: 10}});
    await Registry.applyPerpetualSwapFunding("s-7-3", 3, 0.1);
    record("funding_object_contracts", Registry.vaults.get("s-7-3").contracts[3]);
    record("funding_object_save_value", table("vaults").get("s-7-3").value);
    record("redeem_missing", await Registry.updateVaultRedeem("missing", {}, 1));
    Registry.vaults.set("s-7-4", {contracts: 0, margin: 0, available: 0, outstanding: "10"});
    await Registry.updateVault("s-7-4", {}, -2, 0);
    record("update_string_concatenates", Registry.vaults.get("s-7-4").outstanding);
    record("send_direct", await caught(() => Registry.sendSyntheticCurrency("a", "b", "1-2", 1, false)));
    record("send_channel", await caught(() => Registry.sendSyntheticCurrency("a", "b", "1-2", 1, true)));
    record("trade_direct", await caught(() => Registry.tradeSyntheticCurrency({syntheticTokenId: "1-2"}, false)));
    record("trade_channel", await caught(() => Registry.tradeSyntheticCurrency({syntheticTokenId: "1-2"}, true)));
    record("post_margin", await caught(() => Registry.postMargin("a", "s-1-2", 1, 2)));
    record("reuse_no_flags", await caught(() => Registry.reuseVaultNumber()));
    record("rebase_registered", await caught(() => Registry.rebaseSyntheticCurrency("s-1-2", 0.1)));
    Registry.vaults = undefined; Registry.syntheticTokens = undefined;
    await Registry.initializeIfNeeded();
    record("reload_saved_vault_count", Registry.vaults.size);
    record("reload_saved_synthetic_count", Registry.syntheticTokens.size);
    assert.equal(observed.size, QUIRKS.length);
    for (const [key, expected] of QUIRKS) assert.equal(observed.get(key), expected, key);
    console.log(`PASS: ${OPERATIONS.length} lifecycle operations, ${QUERIES.length} BigNumber queries and ${QUIRKS.length} quirks against SHA-256-pinned vaults.js`);
}

async function main() {
    const args = process.argv.slice(2), sourceIndex = args.indexOf("--source");
    assert(args.every((arg, index) => arg === "--check" || arg === "--source" || (sourceIndex >= 0 && index === sourceIndex + 1)),
        "usage: node scripts/vault_vectors.cjs [--check] [--source path]");
    if (sourceIndex >= 0) assert(args[sourceIndex + 1], "--source requires a path");
    await checkSource(sourceIndex >= 0 ? args[sourceIndex + 1] : path.join(fixtures, "legacy_vaults_source.txt"));
    const fixture = [
        "# vaults.js at tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c",
        `# source_sha256=${SOURCE_SHA256}`,
        "# C id; U/S/R id contracts margin available outstanding (R outstanding is signed source delta)",
        ...OPERATIONS.map(row => row.join("\t")),
        "# Q kind(B=collateral,O=outstanding) numeric_property_or_vault expected",
        ...QUERIES.map(row => ["Q", ...row].join("\t")),
        "# K source_quirk expected (observations only; deliberately not typed store semantics)",
        ...QUIRKS.map(row => ["K", ...row].join("\t"))
    ].join("\n") + "\n";
    const destination = path.join(__dirname, "../fixtures/vault_vectors.tsv");
    if (args.includes("--check")) assert.equal(fs.readFileSync(destination, "utf8"), fixture, "vault fixture drift");
    else fs.writeFileSync(destination, fixture);
    console.log(`PASS: vault fixture ${args.includes("--check") ? "verified" : "written"}`);
    console.log("BOUNDARY: typed local lifecycle and source characterization; no mint authorization, script checks, ledger transfer, production persistence, funding or rebase settlement");
}
main().catch(error => {console.error(error); process.exitCode = 1;});
