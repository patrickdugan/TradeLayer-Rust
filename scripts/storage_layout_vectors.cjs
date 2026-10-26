#!/usr/bin/env node
"use strict";

// Executes unmodified db.js to characterize its category names and folders.
// Datastore/RPC adapters prevent disk/network access; no NeDB query equivalence.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const util = require("node:util");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SOURCE_COMMIT = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
const SOURCE_SHA256 = "b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38";
const args = process.argv.slice(2);
const index = args.indexOf("--source-root");
const sourceRoot = index < 0 ? null : args[index + 1];
if (index >= 0) assert.ok(sourceRoot, "--source-root requires a directory");
const source = readPinnedSource(sourceRoot ? path.join(sourceRoot, "src", "db.js") : path.join(fixtures, "legacy_db_source.txt"), SOURCE_SHA256);

async function execute(chain, test, bootstrap, rootKind) {
  const diskRoot = path.parse(process.cwd()).root;
  const sourceDir = path.join(diskRoot, "source-reference", "src");
  const root = rootKind === "absolute" ? path.join(diskRoot, "oracle-db") : rootKind === "relative" ? "relative-db" : undefined;
  const env = {};
  if (root !== undefined) env.TL_NEDB_ROOT = root;
  if (bootstrap === "skip") {
    env.TL_SKIP_RPC_BOOT = "1";
    env.TL_FORCE_TEST = test ? "1" : "0";
    if (chain) env.CHAIN = chain;
  }
  const opened = [];
  class Datastore {
    constructor(options) {
      assert.equal(options.autoload, true);
      this.filename = options.filename;
      opened.push(options.filename);
    }
    find() {}
    insert() {}
    remove() {}
    update() {}
    findOne() {}
    count() {}
  }
  let rpcCalls = 0;
  const client = { async getInstance() {
    rpcCalls++;
    return { async getChain() { return chain; }, async getTest() { return test; } };
  } };
  const module = { exports: {} };
  vm.runInNewContext(source, {
    module, exports: module.exports, __dirname: sourceDir,
    process: { env }, console: { log() {}, error() {} },
    require(name) {
      if (name === "nedb") return Datastore;
      if (name === "path") return path;
      if (name === "util") return util;
      if (name === "./client") return client;
      throw new Error(`unexpected db.js dependency ${name}`);
    }
  }, { filename: "pinned-db.js", timeout: 1000 });
  const db = module.exports;
  await db.init();
  assert.equal(rpcCalls, bootstrap === "skip" ? 0 : 1);
  assert.equal(opened.length, 32);
  assert.equal((await db.getDatabase("procedural")).filename, opened.at(-1));
  assert.equal(await db.getDatabase("missing"), undefined);
  const base = root === undefined ? path.join(sourceDir, "..", "nedb-data") : path.isAbsolute(root) ? root : path.join(sourceDir, "..", root);
  const folder = path.relative(base, db.path);
  assert.equal(path.basename(db.path), folder);
  const categories = opened.map(filename => {
    assert.equal(path.dirname(filename), db.path);
    assert.equal(path.extname(filename), ".db");
    return path.basename(filename, ".db");
  });
  return { categories, folder };
}

(async () => {
  const scenarios = [
    ["skip-btc-main", "bTc", false, "skip", "absolute"],
    ["skip-ltc-test", "lTc", true, "skip", "relative"],
    ["skip-doge-main", "DoGe", false, "skip", "default"],
    ["rpc-ltc-main", "LTC", false, "rpc", "relative"],
    ["rpc-btc-test", "BTC", true, "rpc", "default"],
    ["rpc-doge-test", "DOGE", true, "rpc", "absolute"],
    ["skip-default-chain", "", true, "skip", "default"],
  ];
  const lines = ["# Actual db.js category/folder results only; no NeDB storage/query parity.", `# Source ${SOURCE_COMMIT} SHA256 ${SOURCE_SHA256}`, "# category\tindex\tname\tlegacy-filename OR layout\tname\tchain\ttest(0/1)\tfolder\tbootstrap\troot-kind"];
  let categories;
  for (const [name, chain, test, bootstrap, rootKind] of scenarios) {
    const result = await execute(chain, test, bootstrap, rootKind);
    if (!categories) {
      categories = result.categories;
      for (const [i, category] of categories.entries()) lines.push(["category", i, category, `${category}.db`].join("\t"));
    } else assert.deepEqual(result.categories, categories);
    lines.push(["layout", name, chain || "LTC", Number(test), result.folder, bootstrap, rootKind].join("\t"));
  }
  const output = lines.join("\n") + "\n";
  const target = path.join(fixtures, "storage_layout_vectors.tsv");
  if (args.includes("--write")) fs.writeFileSync(target, output);
  else assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), output, "db layout vectors differ");
  console.log(`storage layout: ${categories.length} actual categories and ${scenarios.length} actual folder/bootstrap scenarios passed`);
})().catch(error => { console.error(error); process.exitCode = 1; });
