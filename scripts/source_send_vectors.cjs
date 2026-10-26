#!/usr/bin/env node
'use strict';
// Composes the complete unchanged pinned Types, decoder, validity, Property,
// ClearList, Logic and Tally modules. Only storage, activation, clock, UUID,
// RPC address-format service and unused external services are injected. No
// supplied validation verdicts replace the source validator.
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '..');
const {loadBigNumber, readPinnedSource} = require('./legacy_numeric_oracle.cjs');
require('./native_source_oracle.cjs').verifyNativeSources(root);
const BN = loadBigNumber();
const pins = {
  types: ['fixtures/legacy_types_source_types.txt', 'a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1'],
  decoder: ['fixtures/legacy_types_source_txDecoder.txt', 'a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076'],
  validity: ['fixtures/legacy_validity_source.txt', 'ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00'],
  property: ['fixtures/reference_fidelity/property.js', 'ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1'],
  clearlist: ['fixtures/legacy_clearlist_audit_reference.txt', 'fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1'],
  tally: ['fixtures/native_genesis/source/tally.js', 'cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef'],
  logic: ['fixtures/native_genesis/source/logic.js', 'f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d'],
};
const sources = Object.fromEntries(Object.entries(pins).map(([name, [file, hash]]) =>
  [name, readPinnedSource(path.join(root, file), hash)]));
const A = '1BoatSLRHtKNngkdXEeobR76b53LETtpyT';
const B = '1BitcoinEaterAddressDontSendf59kuE';
const row = (available, vesting = 0) => ({amount:available, available, reserved:0, margin:0, vesting, channelBalance:0});
const amount = atoms => BigInt(atoms).toString(36) + '~';
const scalar = (id, encoded, address = B) => `0;${address};${id};${encoded}`;
// Explicit offline equivalent of the owner's Bitcoin Base58Check format RPC
// response. The unchanged source's own local checksum routine still executes.
function validBitcoinAddress(address) {
  if (typeof address !== 'string') return false;
  let value = 0n;
  for (const char of address) {
    const digit = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'.indexOf(char);
    if (digit < 0) return false;
    value = value * 58n + BigInt(digit);
  }
  let hex = value.toString(16); if (hex.length % 2) hex = '0' + hex;
  const decoded = Buffer.concat([Buffer.alloc(address.match(/^1*/)[0].length), Buffer.from(hex, 'hex')]);
  if (decoded.length !== 25 || ![0, 5].includes(decoded[0])) return false;
  const digest = crypto.createHash('sha256').update(crypto.createHash('sha256').update(decoded.subarray(0,21)).digest()).digest();
  return digest.subarray(0,4).equals(decoded.subarray(21));
}
function environment(scene) {
  const tables = new Map(), modules = {}, trace = [];
  const table = name => {if (!tables.has(name)) tables.set(name, new Map()); return tables.get(name);};
  const properties = [1, 2, 3, 4, 7, 8].map(id => [id, {
    ticker:`P${id}`, totalInCirculation:100, type: id === 2 || id === 3 ? 4 : 1,
    whitelistId: scene.whitelist && id === 7 ? 5 : 0, issuer:A, backupAddress:B,
  }]);
  table('propertyList').set('propertyIndex', {_id:'propertyIndex', value:JSON.stringify(properties)});
  const id = scene.property || 7;
  const balances = scene.empty ? [] : [[A, {[id]:row(scene.native ? 10 : 100, scene.native ? 10 : 0)}],
    [B, {[id]:row(scene.native ? 10 : scene.huge ? 9007199254740992 : 0, scene.native ? 10 : 0)}]];
  table('tallyMap').set('tallyMap', {_id:'tallyMap', data:JSON.stringify(balances)});
  const field = (value, dotted) => dotted.split('.').reduce((v, key) => v?.[key], value);
  const matches = (doc, query) => Object.entries(query).every(([key, value]) => key === '$or'
    ? value.some(q => matches(doc, q)) : field(doc, key) === value);
  const database = name => ({
    async findOneAsync(query) {trace.push(['findOne', name]); return structuredClone([...table(name).values()].find(doc => matches(doc, query)) || null);},
    async findAsync(query) {trace.push(['find', name]); return structuredClone([...table(name).values()].filter(doc => matches(doc, query)));},
    async updateAsync(query, update) {trace.push(['update', name]); const old = table(name).get(query._id) || {_id:query._id}; table(name).set(query._id, structuredClone(update.$set ? {...old, ...update.$set} : update)); return 1;},
    async insertAsync(doc) {trace.push(['insert', name]); table(name).set(doc._id, structuredClone(doc)); return doc;},
  });
  // Source tally.js contains an un-awaited getDatabase call in unrelated fee
  // paths. The explicit synchronous database factory matches existing source
  // native composition oracles; all database methods still return promises.
  const storage = {getDatabase:database};
  const activation = {getInstance:() => ({
    async isTxTypeActive() {return !scene.inactive;}, getAdmin() {return 'admin';},
    areActivationsAboveThreshold() {return false;},
  })};
  let uuid = 0;
  const external = {
    'bignumber.js':BN, crypto, fs:{}, path, uuid:{v4:() => `source-send-${++uuid}`},
    './activation.js':activation, './db.js':storage, './db':storage,
    './txUtils.js':{async validateAddressWrapper(address) {return {isvalid:validBitcoinAddress(address)};}},
    './txUtils':{async validateAddressWrapper(address) {return {isvalid:validBitcoinAddress(address)};}},
    'bitcoinjs-lib':{networks:{bitcoin:{}}, payments:{}}, 'tiny-secp256k1':{},
    bip32:{BIP32Factory:() => ({})},
    bech32:{bech32:{decode() {throw Error('unused Bech32 adapter');}}},
    './txEncoder':{},
    './base94.js':{}, './base256.js':{},
  };
  const names = {'./types.js':'types', './txDecoder.js':'decoder', './txDecoder':'decoder',
    './validity.js':'validity', './validity':'validity', './property.js':'property',
    './clearlist.js':'clearlist', './tally.js':'tally'};
  const quiet = {log(){}, warn(){}, error(){}};
  function load(name) {
    if (modules[name]) return modules[name].exports;
    const module = {exports:{}}; modules[name] = module;
    class Clock extends Date {static now() {return 0;}}
    vm.runInNewContext(sources[name], {module, exports:module.exports, Buffer, Date:Clock,
      process:{env:{}}, console:quiet, require(key) {
        if (names[key]) return load(names[key]);
        if (Object.hasOwn(external, key)) return external[key];
        if (key.startsWith('./')) return {};
        throw Error(`Unexpected external ${name}: ${key}`);
      }}, {filename:`pinned-source-send-${name}.js`, timeout:2000});
    return module.exports;
  }
  return {types:load('types'), logic:load('logic'), tables, trace, id};
}
const scenes = [
  {name:'scalar-whole', body:scalar(7, 'a')},
  {name:'scalar-fraction', body:scalar(7, amount(110000000))},
  {name:'scalar-self', body:scalar(7, 'a', A)},
  {name:'comma-four-fields-collapse-to-scalar', body:scalar('7,8', '2,3')},
  {name:'source-send-all-missing-property', body:`1;${B}`},
  {name:'source-wire-multi-missing-address', body:`0;${B};7,8;2,3;;`},
  {name:'source-insufficient', body:scalar(7, '2t')},
  {name:'source-no-tally', body:scalar(7, '1'), empty:true},
  {name:'source-clearlist-recipient-field-defect', body:scalar(7, '1'), whitelist:true},
  {name:'source-inactive', body:scalar(7, '1'), inactive:true},
  {name:'source-native2-coupled', body:scalar(2, amount(110000000)), property:2, native:true},
  {name:'source-native3-coupled', body:scalar(3, amount(110000000)), property:3, native:true},
  {name:'source-native-subwhole-throws', body:scalar(2, amount(50000000)), property:2, native:true},
  {name:'source-recipient-number-loss', body:scalar(7, '1'), huge:true},
  {name:'direct-logic-send-all-shadow', direct:[true, A, B, 7, 1, 2]},
  {name:'direct-logic-multi-unbound-processSend', direct:[false, A, [B,B], [7,8], [1,1], 2]},
  {name:'direct-logic-multi-length-error', direct:[false, A, [B], [7,8], [1,1], 2]},
];
async function main() {
  const rows = [];
  for (const scene of scenes) {
    const e = environment(scene);
    let params, outcome = 'returned', reason = '';
    try {
      if (scene.direct) await e.logic.sendToken(...scene.direct);
      else {
        params = await e.types.decodePayload('ab'.repeat(32), 2, 'tl', scene.body, A, null, 1, 0, 2);
        if (params.valid === true) await e.logic.typeSwitch(2, params);
        reason = params.reason || '';
      }
    } catch (error) {outcome = `${error.name}: ${error.message}`;}
    // Flush genuine un-awaited source getTally work before observing storage.
    await Promise.resolve(); await Promise.resolve();
    const balances = new Map(JSON.parse(e.tables.get('tallyMap').get('tallyMap').data));
    const atoms = (owner, bucket) => new BN(balances.get(owner)?.[e.id]?.[bucket] || 0).times(1e8).toFixed();
    const deltas = [...(e.tables.get('tallyMapDelta') || new Map()).values()].map(doc => doc.data.type).join(',');
    const values = [scene.name, scene.body || '(direct)', params ? String(params.valid) : '(direct)', outcome,
      atoms(A,'available'), atoms(B,'available'), atoms(A,'vesting'), atoms(B,'vesting'), deltas || '-', reason || '-'];
    assert(values.every(value => !/[\t\r\n]/.test(value)));
    rows.push(values.join('\t'));
  }
  const text = '# Complete pinned source dca0888b827c5bc28fa9189bc9e1d507f5c5670c; see scripts/source_send_vectors.cjs\n'
    + 'scene\tbody\tvalid\toutcome\tsender_atoms\trecipient_atoms\tsender_markers\trecipient_markers\tdeltas\treason\n'
    + rows.join('\n') + '\n';
  const target = path.join(root, 'fixtures/source_send_vectors.tsv');
  if (process.argv.includes('--write')) fs.writeFileSync(target, text);
  else assert.equal(fs.readFileSync(target, 'utf8').replace(/\r\n/g,'\n'), text);
  console.log(`PASS ${rows.length} composed pinned-source send scenes`);
}
main().catch(error => {console.error(error); process.exitCode = 1;});
