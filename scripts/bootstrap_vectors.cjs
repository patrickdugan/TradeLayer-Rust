#!/usr/bin/env node
"use strict";

// Execute the complete unmodified init.js with explicit client/database and
// virtual timer adapters. Nothing imports the eager real db.js singleton.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const HASH = "55c7dd6da798a58dc57d34ad75be357f9811574db5e6f384620055f966447cc0";
const index = process.argv.indexOf("--source");
const sourcePath = index < 0 ? path.join(fixtures, "legacy_init_source.txt") : process.argv[index + 1];
const source = readPinnedSource(sourcePath, HASH);
const rows = [];
const names = value => value === undefined ? "undefined" : value === null ? "null" : String(value);
async function settle() { for (let i = 0; i < 12; ++i) await Promise.resolve(); }

async function observe(options) {
  const { label, helper = false, present = true,
  clientError = false, databaseError = false, timerError = false, timeout = 5000,
    interval = 150, polls = 40 } = options;
  const chain = Object.hasOwn(options, "chain") ? options.chain : "LTC";
  const events = [], timers = [];
  const client = present ? { chain } : undefined;
  let elapsed = 0, status = "pending", returned;
  const database = { init: async input => {
    events.push(`db/${names(input)}`);
    if (databaseError) throw Error("database failure");
  } };
  const sandbox = {
    module: { exports: {} }, console: { log() {} },
    Date: { now: () => elapsed },
    setTimeout: (resolve, delay) => {
      events.push(`timer/${delay}`);
      if (timerError) throw Error("timer failure");
      timers.push({ resolve, delay });
    },
    require: name => {
      if (name === "./client") return { getInstance: async () => {
        events.push("client");
        if (clientError) throw Error("client failure");
        return client;
      } };
      if (name === "./db") return database;
      throw Error(`unexpected source dependency ${name}`);
    },
  };
  vm.runInNewContext(source, sandbox, { filename: sourcePath, timeout: 1000 });
  const promise = helper ? sandbox.waitForClientChain(client, timeout, interval) : sandbox.module.exports();
  promise.then(value => { status = "ready"; returned = value; }, error => { status = `error/${error.message}`; });
  await settle();
  // Stop after the requested number of registered timers. A final pending
  // timer remains unresolved, proving the source did not enforce its timeout.
  while (status === "pending" && timers.length && events.filter(e => e.startsWith("timer/")).length < polls) {
    const timer = timers.shift(); elapsed += timer.delay; timer.resolve(); await settle();
  }
  // Normal initialize has one timer; it must be advanced to finish.
  if (present && status === "pending" && timers.length) {
    const timer = timers.shift(); elapsed += timer.delay; timer.resolve(); await settle();
  }
  if (status === "ready" && !helper) {
    assert.equal(returned.Client, client); assert.equal(returned.Db, database);
  }
  if (!present && !clientError) assert.equal(status, "pending");
  if (!present && timeout < elapsed) assert.equal(status, "pending", "timeout remains ignored");
  const compressed = [];
  for (const event of events) {
    const last = compressed.at(-1);
    if (last?.event === event) last.count++;
    else compressed.push({ event, count: 1 });
  }
  rows.push([helper ? "helper" : "initialize", label, present, names(chain), clientError,
    databaseError, timerError, timeout, interval, polls, elapsed,
    compressed.map(({ event, count }) => count === 1 ? event : `${event}*${count}`).join(";") || "-", status].join("\t"));
}

async function main() {
  for (const chain of ["BTC", "LTC", "DOGE", undefined, null, "unknown"]) {
    await observe({ label: `present-${names(chain)}`, chain });
  }
  await observe({ label: "client-failure", clientError: true });
  await observe({ label: "database-failure", databaseError: true });
  await observe({ label: "timer-failure", timerError: true });
  await observe({ label: "missing-client", present: false, chain: undefined });
  await observe({ label: "helper-present-no-chain", helper: true, chain: undefined });
  await observe({ label: "helper-ignored-timeout", helper: true, present: false,
    chain: undefined, timeout: 20, interval: 7, polls: 6 });
  assert.equal(rows.length, 12);
  const output = "# Unmodified pinned init.js; virtual time and caller-owned dependency adapters.\n" + rows.join("\n") + "\n";
  const destination = path.join(fixtures, "bootstrap_vectors.tsv");
  if (process.argv.includes("--write")) fs.writeFileSync(destination, output);
  else assert.equal(fs.readFileSync(destination, "utf8"), output);
  console.log(`bootstrap: ${rows.length} exact-source ordering/failure/readiness cases; SHA ${HASH}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
