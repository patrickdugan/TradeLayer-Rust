#!/usr/bin/env node
"use strict";

// Every observation executes the complete unmodified pinned Persistence class.
// The virtual POSIX filesystem, clock, RPC and NeDB adapters are explicit test
// boundaries. No real filesystem restore or blockchain authority is inferred.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SOURCE_COMMIT = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
const SOURCE_SHA256 = "628c5b3cfd919fd518aedc2b757cdc256f7ccc36c447090f30aebd8a677ce2e1";
const args = process.argv.slice(2);
const si = args.indexOf("--source-root");
const sourceRoot = si < 0 ? null : args[si + 1];
if (si >= 0) assert.ok(sourceRoot, "--source-root requires a directory");
const source = readPinnedSource(sourceRoot ? path.join(sourceRoot, "src", "persistence.js") : path.join(fixtures, "legacy_persistence_source.txt"), SOURCE_SHA256);
const posix = path.posix;
const clone = value => structuredClone(value);
const BASE = "/virtual/nedb-data";

function harness(options = {}) {
  const files = new Map();
  const directories = new Set(["/", "/virtual", "/virtual/src"]);
  const events = [];
  const rows = { persistence: new Map(), consensus: new Map() };
  const faults = { copy: [], unlink: [], rpc: false, deferTrack: false };
  let clock = 1700000000000;
  let releaseClient, releaseTrack;
  let trackDone = false;
  const normalize = value => posix.normalize(value);
  const mkdir = name => { for (let at = normalize(name); !directories.has(at); at = posix.dirname(at)) directories.add(at); };
  function fault(queue) {
    if (!queue.length) return;
    const code = queue.shift();
    if (code) throw Object.assign(new Error(`injected ${code}`), { code });
  }
  const childNames = name => [...new Set([...directories, ...files.keys()].filter(item => item !== normalize(name) && posix.dirname(item) === normalize(name)).map(item => posix.basename(item)))].sort();
  const fakeFs = { promises: {
    async mkdir(name) { mkdir(name); },
    async readdir(name, opts) {
      if (!directories.has(normalize(name))) throw Object.assign(new Error("missing directory"), { code: "ENOENT" });
      const names = childNames(name);
      return opts?.withFileTypes ? names.map(name2 => ({ name: name2, isDirectory() { return directories.has(posix.join(name, name2)); } })) : names;
    },
    async readFile(name) { if (!files.has(normalize(name))) throw new Error("missing file"); return files.get(normalize(name)); },
    async writeFile(name, data) { events.push(["write", normalize(name)]); files.set(normalize(name), String(data)); },
    async copyFile(from, to) {
      events.push(["copy", normalize(from), normalize(to)]); fault(faults.copy);
      if (!files.has(normalize(from))) throw Object.assign(new Error("missing source"), { code: "ENOENT" });
      files.set(normalize(to), files.get(normalize(from)));
    },
    async unlink(name) { events.push(["unlink", normalize(name)]); fault(faults.unlink); if (!files.delete(normalize(name))) throw Object.assign(new Error("missing file"), { code: "ENOENT" }); },
    async stat(name) { return { isDirectory() { return directories.has(normalize(name)); }, mtimeMs: Number(posix.basename(name).split("-").at(-1)) || 0 }; },
    async rm(name) { events.push(["rm", normalize(name)]); for (const key of [...files.keys()]) if (key === name || key.startsWith(`${name}/`)) files.delete(key); for (const key of [...directories]) if (key === name || key.startsWith(`${name}/`)) directories.delete(key); }
  } };
  function matches(row, query) {
    return Object.entries(query).every(([key, wanted]) => wanted && typeof wanted === "object"
      ? ("$exists" in wanted ? (key in row) === wanted.$exists : row[key] < wanted.$lt)
      : row[key] === wanted);
  }
  const db = { async getDatabase(category) {
    const store = rows[category]; assert.ok(store, `unexpected category ${category}`);
    return {
      async findOneAsync(query) { return clone([...store.values()].find(row => matches(row, query)) || null); },
      async findAsync(query) { return [...store.values()].filter(row => matches(row, query)).map(clone); },
      async updateAsync(query, update) {
        events.push(["update", category, query._id, clone(update.$set)]);
        if (faults.deferTrack && query._id === "TrackHeight") await new Promise(resolve => { releaseTrack = () => { trackDone = true; resolve(); }; });
        const next = { ...(store.get(query._id) || query), ...clone(update.$set) };
        store.set(query._id, next); return 1;
      },
      async removeAsync(query) { events.push(["remove", category, clone(query)]); let count = 0; for (const [key, row] of [...store]) if (matches(row, query)) { store.delete(key); count++; } return count; }
    };
  } };
  const canonical = new Map();
  const client = {
    async getChain() { return options.chain || "LTC"; },
    async getTest() { return options.test ?? false; },
    async getBlockHash(height) { if (faults.rpc) throw new Error("RPC unavailable"); return canonical.get(height) || `canonical-${height}`; },
    async getBlock(hash) { if (faults.rpc) throw new Error("RPC unavailable"); return { hash }; }
  };
  const Consensus = {
    async stateConsensusHash() { events.push(["stateHash"]); return "0123456789abcdef"; },
    async storeConsensusHash(height, hash) { events.push(["storeHash", height, hash]); }
  };
  class ClockDate extends Date {
    constructor(...values) { super(...(values.length ? values : [clock])); }
    static now() { return clock; }
  }
  const module = { exports: {} };
  vm.runInNewContext(source, {
    module, exports: module.exports, __dirname: "/virtual/src", Date: ClockDate,
    process: { env: { TL_NEDB_ROOT: BASE } }, console: { log() {}, warn() {}, error() {} },
    setTimeout(callback, milliseconds) { events.push(["delay", milliseconds]); Promise.resolve().then(callback); return 0; },
    require(name) {
      if (name === "fs") return fakeFs;
      if (name === "path") return posix;
      if (name === "./db.js") return db;
      if (name === "./consensus.js") return Consensus;
      if (name === "./client.js") return { getInstance() { return options.deferClient ? new Promise(resolve => { releaseClient = () => resolve(client); }) : Promise.resolve(client); } };
      throw new Error(`unexpected dependency ${name}`);
    }
  }, { filename: "pinned-persistence.js", timeout: 1000 });
  return {
    P: module.exports, files, directories, rows, faults, events, canonical,
    async instance(interval = 1000) { return module.exports.getInstance({ network: options.chain || "LTC", test: options.test ?? false, snapshotInterval: interval }); },
    mkdir, releaseClient() { releaseClient(); }, releaseTrack() { releaseTrack?.(); },
    get trackDone() { return trackDone; }, tick() { clock++; },
    snapshot(height) {
      const dir = `${height}-fixture`; const full = `${BASE}/ltc-main-snapshots/${dir}`;
      mkdir(full); files.set(`${full}/meta.json`, JSON.stringify({ blockHeight: height, consensusHash: `hash-${height}`, files: [] }));
      return dir;
    }
  };
}

async function selection(last, heights) {
  const h = harness(); const p = await h.instance();
  const ancestor = Math.min(5, last - 1);
  for (let height = 0; height <= last; height++) {
    await p.recordBlockHeader(height, `local-${height}`, `local-${height - 1}`);
    h.canonical.set(height, height <= ancestor ? `local-${height}` : `different-${height}`);
  }
  for (const height of heights) h.snapshot(height);
  const result = await p.detectAndHandleReorg();
  return { selected: heights.length ? result.restoredFrom : null, result };
}

(async () => {
  const lines = ["# Actual complete pinned persistence.js source; observations are not safe restore policies.", `# Source ${SOURCE_COMMIT} SHA256 ${SOURCE_SHA256}`, "# name\toperation\targ1\targ2\texpected"];
  const emit = (name, operation, arg1, arg2, expected) => lines.push([name, operation, arg1, arg2, typeof expected === "string" ? expected : JSON.stringify(expected)].join("\t"));
  for (const [height, interval] of [[0,0],[0,1000],[1,1000],[999,1000],[1000,1000],[1001,1000],[2000,1000],[0,1],[1,1],[7,2],[8,2],[3000,1000]]) {
    const h = harness(); const p = await h.instance(interval);
    await p.maybeCheckpoint(height);
    emit(`cadence-${height}-${interval}`, "cadence", height, interval, h.events.some(event => event[0] === "storeHash"));
  }
  for (const height of [0,999,1000,1001,2000,5000]) {
    const h = harness(); const p = await h.instance(); await p.cleanupPersistence(height);
    const event = h.events.find(event => event[0] === "remove");
    emit(`cleanup-${height}`, "cleanup", height, "-", String(event[2].height.$lt));
  }
  for (const [last, heights] of [[10,[]],[10,[3,8]],[10,[11,12]],[10,[10]],[10,[3,8,10,12]],[5,[3,8]],[0,[1]],[100,[1,20,99]]]) {
    const observed = await selection(last, heights);
    emit(`selection-${last}-${heights.join("-")}`, "selection", last, heights.join(","), observed.selected === null ? "null" : String(observed.selected));
  }
  {
    const h = harness({ deferClient: true }); const pending = h.P.getInstance({ network: "LTC", test: false });
    const early = await h.P.getInstance(); const before = { dbDir: early.dbDir, snapshotsDir: early.snapshotsDir };
    h.releaseClient(); await pending;
    emit("singleton-published-before-init", "observation", "-", "-", before);
  }
  {
    const h = harness({ chain: "  LtC  ", test: true }); const p = await h.instance();
    emit("trimmed-network-layout", "observation", "-", "-", [p.networkFull, p.dbDir, p.snapshotsDir, p.backupsDir]);
  }
  {
    const h = harness(); const p = await h.instance();
    await p.recordBlockHeader(2,"old","one"); await p.recordBlockHeader(1,"one","zero"); await p.recordBlockHeader(2,"new","one");
    const known = await p.getLastKnownBlock();
    await p.updateMaxProcessed(2); await p.saveTrackHeight(1);
    emit("header-upsert-and-height-markers", "observation", "-", "-", { header: await p.getBlockHeader(2), known, consensus: [...h.rows.consensus.values()] });
  }
  {
    const h = harness(); const p = await h.instance(); await p.recordBlockHeader(10,"stale","nine");
    emit("rpc-canonical-equals-incoming-local-stale", "observation", "-", "-", await p.checkForReorgForNewBlock(11,"canonical-10"));
    emit("local-link-fast-path", "observation", "-", "-", await p.checkForReorgForNewBlock(11,"stale"));
    emit("genesis-parent-early-return", "observation", "-", "-", await p.checkForReorgForNewBlock(1,"anything"));
    h.faults.rpc=true;
    emit("rpc-failure-undefined-logger-fails-open", "observation", "-", "-", await p.checkForReorgForNewBlock(11,"other"));
  }
  {
    const observed=await selection(10,[3,8]);
    emit("snapshot-above-common-ancestor", "observation", "-", "-", observed.result);
  }
  {
    const h=harness(); const p=await h.instance();
    h.files.set(`${p.dbDir}/consensus.db`,"old-max-height");
    h.files.set(`${p.dbDir}/ignore.txt`,"ignored");
    await p.recordBlockHeader(0,"zero","none"); await p.recordBlockHeader(1000,"thousand","prior");
    await p.createSnapshot(2000,"0123456789abcdef");
    const metas=await p.listSnapshots();
    emit("snapshot-copy-before-max-marker", "observation", "-", "-", { meta:metas[0], copied:h.files.get(`${p.snapshotsDir}/${metas[0].dir}/consensus.db`), events:h.events.filter(e=>e[0]==="copy"||e[0]==="update"||e[0]==="remove"), remaining:[...h.rows.persistence.values()].map(r=>r.height) });
  }
  {
    const h=harness(); const p=await h.instance();
    for(const height of [1,2,3])h.snapshot(height);
    h.mkdir(`${p.snapshotsDir}/broken`);h.files.set(`${p.snapshotsDir}/broken/meta.json`,"broken JSON");
    await p.pruneOldSnapshots(2);
    emit("catalog-sort-invalid-meta-and-pruning", "observation", "-", "-", (await p.listSnapshots()).map(m=>m.blockHeight));
  }
  {
    const h=harness(); const p=await h.instance();h.faults.deferTrack=true;
    h.snapshot(1);h.mkdir(`${p.backupsDir}/backup-1`);h.mkdir(`${p.backupsDir}/backup-2`);
    await p.restoreSnapshot({dir:"1-fixture",blockHeight:1,files:[]});
    await Promise.resolve();
    emit("restore-returns-before-trackheight", "observation", "-", "-", { trackDone:h.trackDone, backups:[...h.directories].filter(d=>posix.dirname(d)===p.backupsDir).map(d=>posix.basename(d)).sort() });
    h.releaseTrack();
  }
  {
    const h=harness();const p=await h.instance();h.snapshot(1);
    h.files.set(`${p.dbDir}/ledger.db`,"live-ledger");let failure;
    try{await p.restoreSnapshot({dir:"1-fixture",blockHeight:1,files:["missing.db"]})}catch(error){failure=error.code}
    emit("restore-failure-after-deleting-live-db", "observation", "-", "-", { failure, liveDeleted:!h.files.has(`${p.dbDir}/ledger.db`), backupPreserved:[...h.files].some(([name,value])=>name.startsWith(`${p.backupsDir}/`)&&name.endsWith("/ledger.db")&&value==="live-ledger") });
  }
  {
    const h=harness();const p=await h.instance();h.files.set("/copy-source","content");
    h.faults.copy=["EBUSY","EPERM",null];await p.safeCopy("/copy-source","/copy-target");
    h.faults.unlink=["EBUSY",null];await p.safeDelete("/copy-target");await p.safeDelete("/already-missing");
    emit("copy-delete-bounded-retries", "observation", "-", "-", { delays:h.events.filter(e=>e[0]==="delay").length, copiedThenDeleted:!h.files.has("/copy-target") });
    h.faults.copy=Array(5).fill("EBUSY");let failure;try{await p.safeCopy("/copy-source","/copy-target")}catch(e){failure=e.message}
    emit("copy-five-attempt-exhaustion", "observation", "-", "-", { failure, copyAttempts:h.events.filter(e=>e[0]==="copy").length, delays:h.events.filter(e=>e[0]==="delay").length });
    h.faults.copy=["ENOENT"];try{await p.safeCopy("/copy-source","/copy-target")}catch(e){emit("copy-other-error-no-retry", "observation", "-", "-", e.code)}
  }
  const output=lines.join("\n")+"\n";const target=path.join(fixtures,"persistence_vectors.tsv");
  if(args.includes("--write"))fs.writeFileSync(target,output);
  else assert.equal(fs.readFileSync(target,"utf8").replace(/\r\n/g,"\n"),output,"persistence vectors differ");
  console.log(`persistence: ${lines.length-3} actual source observations including 26 integer metadata vectors passed`);
})().catch(error=>{console.error(error);process.exitCode=1});
