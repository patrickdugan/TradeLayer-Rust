'use strict';
// Actual unchanged Main class in a real child. Captures the two console effects
// omitted by the older broad observer, then performs the real process.exit(0).
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const cp = require('node:child_process');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const stage = path.resolve(__dirname, '..');
const repo = path.resolve(process.env.PROOF_REPO || stage);
const pin = '006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009';
const sha = x => crypto.createHash('sha256').update(x).digest('hex');
const flags = process.argv.slice(2);
if (flags[0] === '--child') {
  const source = fs.readFileSync(path.join(repo, 'fixtures/native_genesis/source/main.js'), 'utf8');
  assert.equal(sha(source), pin);
  const start = source.indexOf('class Main {');
  const end = source.indexOf('\nmodule.exports = Main', start);
  assert.ok(start >= 0 && end > start);
  const classText = source.slice(start, end);
  const effects = [];
  const profile = flags[1];
  // This callback must never run: process.exit occurs before an await/return.
  queueMicrotask(() => { process.stderr.write('unexpected post-exit drain'); });
  const main = vm.runInNewContext(classText + '\nMain;', {
    console: { log(...values) { effects.push({ kind: 'console', level: 'log', values }); } },
    process: { exit(code) {
      effects.push({ kind: 'process-exit', code });
      process.stdout.write(JSON.stringify({
        profile, mainSHA256: pin, methodSHA256: sha(main.prototype.shutdown.toString()),
        exitCode: code, normalReturn: false, effects,
      }) + '\n');
      process.exit(code);
    } },
  }, { filename: 'pinned/complete-Main-class.js' });
  const receiver = new Proxy({}, { get() { throw Error('shutdown read its receiver'); } });
  const args = profile === 'ignored-values' ? [undefined, null, false, NaN, Infinity, { value: 'ignored' }] : [];
  main.prototype.shutdown.call(profile === 'unbound' ? null : receiver, ...args);
  throw Error('source shutdown unexpectedly returned');
} else {
  const rows = [];
  const tag = value => {
    if (value === undefined) return ['undefined'];
    if (value === null) return ['null'];
    if (typeof value === 'boolean') return ['boolean', value];
    if (typeof value === 'string') return ['string', value];
    if (typeof value === 'number') {
      const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value); return ['number', bytes.toString('hex')];
    }
    if (Array.isArray(value)) return ['array', value.map(tag)];
    return ['object', Object.entries(value).map(([k, v]) => [k, tag(v)])];
  };
  for (const profile of ['ordinary', 'ignored-values', 'unbound']) {
    const result = cp.spawnSync(process.execPath, [__filename, '--child', profile], {
      encoding: 'utf8', env: process.env, maxBuffer: 1024 * 1024,
    });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.signal, null);
    assert.equal(result.stderr, '');
    const lines = result.stdout.trim().split('\n');
    assert.equal(lines.length, 1, 'source exit terminates before ordinary return/drain');
    const row = JSON.parse(lines[0]);
    assert.equal(row.exitCode, 0); assert.equal(row.normalReturn, false);
    assert.deepEqual(row.effects, [
      { kind: 'console', level: 'log', values: ['Saving state to database...'] },
      { kind: 'console', level: 'log', values: ['Shutdown completed.'] },
      { kind: 'process-exit', code: 0 },
    ]);
    rows.push(tag(row));
  }
  const payload = rows.map(JSON.stringify).join('\n') + '\n';
  const file = path.join(stage, 'fixtures/main_shutdown_control_vectors.jsonl');
  if (flags.includes('--check')) assert.equal(fs.readFileSync(file, 'utf8'), payload);
  else fs.writeFileSync(file, payload);
  console.log(JSON.stringify({ actualSourceInvocations: 3, exitCode: 0, normalReturn: false,
    noPostExitDrain: true, mainSHA256: pin, fixtureSHA256: sha(payload) }));
}
