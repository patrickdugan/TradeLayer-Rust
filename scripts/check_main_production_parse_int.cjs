'use strict';
// The literal Main class uses the native omitted-radix parseInt operation.
// Check independently captured binary64 results without a Rust implementation.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const file = path.join(__dirname, '../fixtures/main_production_parse_int.tsv');
const rows = fs.readFileSync(file, 'utf8').trimEnd().split(/\r?\n/).filter(line => !line.startsWith('#'));
for (const line of rows) {
  const [name, inputHex, expected] = line.split('\t');
  const bits = Buffer.alloc(8);
  bits.writeDoubleBE(parseInt(Buffer.from(inputHex, 'hex').toString('utf8')));
  assert.equal(bits.toString('hex'), expected, name);
}
console.log(JSON.stringify({cases: rows.length, operation: 'native omitted-radix parseInt', result: 'PASS'}));
