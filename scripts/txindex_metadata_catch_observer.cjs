#!/usr/bin/env node
'use strict';
// Execute the complete pinned module. Only DB and console boundaries are recorded.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const root = path.resolve(__dirname, '..');
const code = fs.readFileSync(path.join(root, 'fixtures/legacy_txindex_source_txIndex.txt'), 'utf8').replace(/\r\n/g, '\n');
assert.equal(crypto.createHash('sha256').update(code).digest('hex'), '095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51');
const prefix = process.env.TRADELAYER_JS_ROOT || process.env.TL_JS_MODULE_ROOT;
assert.ok(prefix, 'Provision the pinned genuine npm dependencies through TRADELAYER_JS_ROOT');
const external = createRequire(path.resolve(prefix, 'package.json'));
assert.equal(external('big-json/package.json').version, '3.2.0');
const cases = [
  ['existing_log', 'initializeIndex', true, ['log_existing']],
  ['insert_log', 'initializeIndex', false, ['log_insert']],
  ['factory', 'initializeIndex', false, ['factory']],
  ['find', 'initializeIndex', false, ['find']],
  ['insert', 'initializeIndex', false, ['insert']],
  ['catch_replaces', 'initializeIndex', false, ['find', 'error_existing']],
  ['max_find', 'findMaxIndexedBlock', true, ['find']],
  ['max_catch_replaces', 'findMaxIndexedBlock', true, ['find', 'error_max']],
];
const errorLabels = new Map([
  ['Error checking for existing genesis block:', 'error_existing'],
  ['Error initializing genesis block:', 'error_insert'],
  ['Error finding MaxIndexedBlock:', 'error_max'],
]);
async function observe([id, method, existing, failures]) {
  const events = [], first = Object.freeze({ token: 'first' }), replacement = Object.freeze({ token: 'replacement' });
  let rows = existing ? 1 : 0;
  function step(event) {
    events.push(event);
    if (failures.includes(event)) throw event.startsWith('error_') ? replacement : first;
  }
  const base = {
    async findOneAsync() { step('find'); return existing ? { value: 7 } : null; },
    async insertAsync(row) { step('insert'); rows++; return row; },
  };
  const module = { exports: {} };
  const unused = Object.freeze({});
  const context = {
    module, exports: module.exports,
    require(name) {
      if (name === 'big-json') return external('big-json');
      if (name === 'util') return require('node:util');
      if (name === './db.js') return { async getDatabase(name) { assert.equal(name, 'txIndex'); step('factory'); return base; } };
      if (name === './txUtils' || name === './client.js') return unused;
      throw new Error(`Unexpected module ${name}`);
    },
    console: {
      log(message) {
        assert.ok(message === 'Genesis block is already initialized:' || message === 'Genesis block initialized:');
        step(message === 'Genesis block is already initialized:' ? 'log_existing' : 'log_insert');
      },
      error(message, thrown) {
        assert.strictEqual(thrown, first, 'source catch must retain the exact error');
        assert.ok(errorLabels.has(message), message);
        step(errorLabels.get(message));
      },
    },
  };
  // Infinite-loop watchdog only; a cold or loaded host can exceed 1s here.
  vm.runInNewContext(code, context, { filename: 'pinned/txIndex.js', timeout: 30000 });
  let result;
  try {
    const returned = await module.exports[method](10);
    assert.strictEqual(returned, undefined);
    result = 'returned';
  } catch (error) {
    assert.ok(error === first || error === replacement, 'unexpected host/source exception');
    result = error === first ? 'first' : 'replacement';
  }
  return [id, result, rows, events.join(',')].join('\t');
}
(async () => {
  for (const item of cases) process.stdout.write(await observe(item) + '\n');
})().catch(error => { console.error(error.stack || error); process.exitCode = 1; });
