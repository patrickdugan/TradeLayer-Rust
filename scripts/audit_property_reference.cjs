"use strict";
// Runs the COMPLETE Git-object property.js bytes unchanged in a fresh VM per test.
// Genuine repository-archived BigNumber9.3.1 and node:path. Injected capabilities:
// async DB factory/async row methods, tally.updateBalance, and quiet console only.
// No injected PropertyManager methods, number replacements, or Rust simulation.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const test = require('node:test');
const repo = path.resolve(__dirname, '..');
const sourcePath = path.join(repo, 'fixtures/reference_fidelity/property.js');
const sourceBytes = fs.readFileSync(sourcePath);
assert.equal(crypto.createHash('sha256').update(sourceBytes).digest('hex'),
  'ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1',
  'exact Git-object property.js bytes at dca0888b827c5bc28fa9189bc9e1d507f5c5670c');
const { loadBigNumber, BIG_NUMBER_VERSION, BIG_NUMBER_SHA256 } = require('./legacy_numeric_oracle.cjs');
const BN = loadBigNumber();
const bnPath = path.join(repo, 'fixtures/legacy_bignumber_source.txt');
const bnPackage = { version: BIG_NUMBER_VERSION };
const plain = value => JSON.parse(JSON.stringify(value));
function env(rows = [], options = {}) {
  let doc = options.doc === undefined ? { _id: 'propertyIndex', value: JSON.stringify(rows) } : options.doc;
  const calls = [], logs = [], tally = [];
  const table = {
    async findOneAsync(query) { calls.push(['find', plain(query)]); if (options.failFind) throw Error('injected DB read failure'); return doc === null ? null : plain(doc); },
    async updateAsync(query, update, config) {
      calls.push(['update', plain(query), plain(update), config && plain(config)]);
      if (options.failSave) throw Error('injected DB write failure');
      doc = update.$set ? { ...doc, ...plain(update.$set) } : plain(update);
      return 1;
    },
  };
  const capabilities = {
    './db.js': { async getDatabase(name) { assert.equal(name, 'propertyList'); calls.push(['db', name]); return table; } },
    './tally.js': { async updateBalance(...args) { tally.push(args); if (options.failTally) throw Error('injected tally failure'); } },
    'path': path,
    'bignumber.js': BN,
  };
  const module = { exports: {} };
  vm.runInNewContext(sourceBytes.toString('utf8'), {
    module, exports: module.exports,
    require(name) { assert.ok(Object.hasOwn(capabilities, name), `unexpected dependency ${name}`); return capabilities[name]; },
    console: { log(...args) { logs.push(['log', ...args]); }, error(...args) { logs.push(['error', ...args]); } },
  }, { filename: sourcePath, timeout: 1500 });
  const P = module.exports, pm = P.getInstance();
  return { P, pm, calls, logs, tally, options,
    read() { return doc && plain(doc); }, rows() { return JSON.parse(doc.value); },
    set(next) { doc = next === null ? null : plain(next); } };
}
const row = (type = 2) => ({ ticker: 'ABC', totalInCirculation: 10, type, whitelistId: 4, issuer: 'alice', backupAddress: 'backup' });

test('constructor/getInstance: one instance and separate empty property/AMM maps', () => {
  const e = env(); assert.equal(new e.P(), e.pm); assert.equal(e.P.getInstance(), e.pm);
  assert.equal(e.pm.propertyIndex.size, 0); assert.equal(e.pm.ammIndex.size, 0);
});
test('load: missing row clears stale memory', async () => {
  const e = env([], { doc: null }); e.pm.propertyIndex.set(9, row());
  assert.equal(await e.P.load(), undefined); assert.equal(e.pm.propertyIndex.size, 0);
});
test('load: string pairs preserve insertion order and mixed key types; duplicate replaces value', async () => {
  const e = env([[2, row()], ['2', { ...row(), ticker: 'STR' }], [2, { ...row(), ticker: 'NEW' }]]);
  await e.P.load(); assert.deepEqual(plain([...e.pm.propertyIndex]), [[2, { ...row(), ticker: 'NEW' }], ['2', { ...row(), ticker: 'STR' }]]);
});
test('load accepts already-decoded pair arrays; invalid pair shape clears memory', async () => {
  const e = env([], { doc: { value: [[1, row()]] } }); await e.P.load(); assert.equal(e.pm.propertyIndex.size, 1);
  e.set({ value: [[1, row(), 'extra']] }); await e.P.load(); assert.equal(e.pm.propertyIndex.size, 0);
});
test('load: parse/read error is swallowed and leaves stale memory intact', async () => {
  const e = env([], { doc: { value: 'bad-json' } }); e.pm.propertyIndex.set(6, row());
  await e.P.load(); assert.equal(e.pm.propertyIndex.size, 1); assert.equal(e.logs.at(-1)[0], 'error');
  e.options.failFind = true; await e.P.load(); assert.equal(e.pm.propertyIndex.size, 1);
});
test('getNextPropertyId reloads persisted max with JS numeric coercion', async () => {
  const e = env([[2, row()], ['8', row()]]); e.pm.propertyIndex.set(100, row());
  assert.equal(await e.pm.getNextPropertyId(), 9); assert.equal(e.pm.propertyIndex.has(100), false);
});
test('getNextPropertyId with synthetic string key produces NaN', async () => {
  const e = env([['s-7-1', row(5)]]); assert.equal(Number.isNaN(await e.pm.getNextPropertyId()), true);
});
for (const [kind, type] of [['Fixed',1], ['Managed',2], ['Native',3], ['Vesting',4], ['Synthetic',5], ['Non-Fungible',6], ['Procedural',7]]) {
  test(`createToken/addProperty: ${kind} maps to source class ${type}, saves quantized metadata only`, async () => {
    const e = env(); assert.equal(await e.pm.createToken('ABC', '1.234567895', kind, 4, 'alice', 'backup', { proceduralType: '8' }), 1);
    assert.deepEqual(e.rows(), [[1, { ...row(type), totalInCirculation: 1.2345679, proceduralType: 8 }]]);
    assert.equal(e.tally.length, 0);
  });
}
test('createToken duplicate ticker returns Error object instead of throwing', async () => {
  const e = env([[1, row()]]); await e.P.load(); const result = await e.pm.createToken('ABC',1,'Fixed');
  assert.equal(result.name, 'Error'); assert.match(result.message, /already exists/); assert.equal(e.rows().length, 1);
});
test('createToken checks duplicates before reload: persisted duplicate can be accepted with empty cache', async () => {
  const e = env([[1, row()]]); assert.equal(await e.pm.createToken('ABC',1,'Fixed'), 2);
  assert.equal(e.rows().filter(([,v]) => v.ticker === 'ABC').length, 2);
});
test('createToken ticker-as-map-key branch throws TypeError', async () => {
  const e = env([['ABC', row()]]); await e.P.load(); await assert.rejects(e.pm.createToken('ABC',1,'Fixed'), { name: 'TypeError' });
});
test('addProperty invalid class rejects before save', async () => {
  const e = env(); await assert.rejects(e.pm.addProperty(1,'ABC',1,'managed'), /Invalid property type/); assert.equal(e.pm.propertyIndex.size,0);
});
test('addProperty existing ID increments supply and truthy-falls-back fields; can change class/metadata', async () => {
  const e = env([[1, { ...row(), proceduralType: 2 }]]); await e.P.load();
  await e.pm.addProperty(1,'', '.000000005','Native',0,'','',{ proceduralType:'9' });
  assert.deepEqual(e.rows(), [[1,{ ...row(3),totalInCirculation:10.00000001,proceduralType:9 }]]);
  await e.pm.addProperty(1,'NEW',-2,'Fixed',8,'bob','new-backup',{ proceduralType:null });
  assert.deepEqual(e.rows(), [[1,{ticker:'NEW',totalInCirculation:8.00000001,type:1,whitelistId:8,issuer:'bob',backupAddress:'new-backup',proceduralType:9}]]);
});
test('addProperty accepts negative and nonnumeric supplies; JSON serializes NaN as null', async () => {
  const e = env(); await e.pm.addProperty(0,'NEG','-1.000000005','Fixed'); assert.equal(e.rows()[0][1].totalInCirculation,-1.00000001);
  await e.pm.addProperty('odd-key','NAN','invalid','Fixed'); assert.equal(e.rows()[1][1].totalInCirculation,null);
});
test('addProperty class map also accepts inherited toString key; saved type function is omitted', async () => {
  const e = env(); await e.pm.addProperty(1,'ABC',1,'toString');
  assert.equal(typeof e.pm.propertyIndex.get(1).type,'function'); assert.equal(Object.hasOwn(e.rows()[0][1],'type'),false);
});
test('save failure leaves mutation in singleton but does not update DB', async () => {
  const e = env([], { failSave:true }); await assert.rejects(e.pm.addProperty(1,'ABC',1,'Fixed'),/injected DB write failure/);
  assert.equal(e.pm.propertyIndex.size,1); assert.deepEqual(e.rows(),[]);
});
test('getPropertyIndex returns only four fields and retains nonempty stale cache', async () => {
  const e = env([[1,row()]]); assert.deepEqual(plain(await e.P.getPropertyIndex()),[{id:1,ticker:'ABC',totalInCirculation:10,type:2}]);
  e.set({value:JSON.stringify([[2,{...row(),ticker:'NEW'}]])}); assert.equal((await e.P.getPropertyIndex())[0].id,1);
});
test('doesTickerExist reads DB with case-sensitive exact match; bad JSON throws', async () => {
  const e = env([[1,row()]]); assert.equal(await e.P.doesTickerExist('ABC'),true); assert.equal(await e.P.doesTickerExist('abc'),false);
  e.set(null); assert.equal(await e.P.doesTickerExist('ABC'),false); e.set({value:'bad'}); await assert.rejects(e.P.doesTickerExist('ABC'),{name:'SyntaxError'});
});
test('isManagedAndAdmin accepts exactly numeric class 2/7 and exact key/issuer; errors return false', async () => {
  const e = env([[1,row(2)],[2,row(7)],[3,row(1)],[4,{...row(),type:'2'}]]);
  assert.equal(await e.P.isManagedAndAdmin(1,'alice'),true); assert.equal(await e.P.isManagedAndAdmin(2,'alice'),true);
  for (const [id,actor] of [['1','alice'],[1,'backup'],[3,'alice'],[4,'alice'],[5,'alice']]) assert.equal(await e.P.isManagedAndAdmin(id,actor),false);
});
test('getPropertyData supports Number-equivalent IDs and exact s-prefixed synthetic key', async () => {
  const e = env([[1,row()],['s-7-1',row(5)]]);
  for(const id of [1,'1','01',true]) assert.equal((await e.P.getPropertyData(id)).ticker,'ABC');
  assert.equal((await e.P.getPropertyData('s-7-1')).type,5); assert.equal(await e.P.getPropertyData('s-7-2'),null);
  assert.equal(await e.P.getPropertyData(99),null); e.set({value:'bad'}); assert.equal(await e.P.getPropertyData(1),null);
});
test('updateAdmin writes primary/backup without caller auth and leaves singleton cache stale', async () => {
  const e = env([[1,row()]]); await e.P.load(); await e.P.updateAdmin('01','bob',false); await e.P.updateAdmin(1,'new-backup',true);
  assert.equal(e.rows()[0][1].issuer,'bob'); assert.equal(e.rows()[0][1].backupAddress,'new-backup'); assert.equal(e.pm.propertyIndex.get(1).issuer,'alice');
  await assert.rejects(e.P.updateAdmin(99,'x',false),/does not exist/);
});
test('updateTotalInCirculation fails at unawaited async DB factory; persisted row unchanged', async () => {
  const e = env([[1,row()]]); await assert.rejects(e.P.updateTotalInCirculation(1,2),{name:'TypeError',message:'base.updateAsync is not a function'});
  assert.equal(e.rows()[0][1].totalInCirculation,10); await assert.rejects(e.P.updateTotalInCirculation(99,1),/Property not found/);
});
test('getAMM fails at undefined PropertyRegistry before any AMM initialization', async () => {
  const e = env(); await assert.rejects(e.P.getAMM(1,2),{name:'ReferenceError',message:'PropertyRegistry is not defined'});
});
test('isSyntheticToken fails because static this.propertyIndex is absent after load', async () => {
  const e = env([[1,row(5)]]); await assert.rejects(e.P.isSyntheticToken(1),{name:'TypeError'});
});
test('grantTokens has no class/admin check and passes original ID/amount plus provenance to tally', async () => {
  const e = env([[1,row(1)]]); await e.pm.grantTokens('1','bob','.000000005',50);
  assert.equal(e.rows()[0][1].totalInCirculation,10.00000001);
  assert.deepEqual(e.tally,[['bob','1','.000000005',0,0,0,'grantToken',50]]);
});
test('grantTokens string persisted key yields both string and numeric key after Number(propertyId) set', async () => {
  const e = env([['1',row()]]); await e.pm.grantTokens('1','bob',1,50);
  assert.deepEqual(e.rows().map(([id,v])=>[id,v.totalInCirculation]),[['1',10],[1,11]]);
});
test('redeemTokens checks total supply but delegates recipient balance; no class/admin check', async () => {
  const e = env([[1,row(1)]]); await e.pm.redeemTokens(1,'bob',3,51);
  assert.equal(e.rows()[0][1].totalInCirculation,7); assert.deepEqual(e.tally,[['bob',1,-3,0,0,0,'redeemToken',51]]);
});
test('redeemTokens excess supply and missing property fail before tally', async () => {
  const e = env([[1,row()]]); await assert.rejects(e.pm.redeemTokens(1,'bob',11,51),/Insufficient/);
  await assert.rejects(e.pm.redeemTokens(99,'bob',1,51),/not found/); await assert.rejects(e.pm.grantTokens(99,'bob',1,51),/not found/);
  assert.equal(e.tally.length,0);
});
test('negative redemption increases declared supply and launches positive tally credit', async () => {
  const e = env([[1,row()]]); await e.pm.redeemTokens(1,'bob',-2,51);
  assert.equal(e.rows()[0][1].totalInCirculation,12); assert.equal(e.tally[0][2],2);
});
test('grant tally failure leaves raised singleton supply while DB retains old declared supply', async () => {
  const e = env([[1,row()]],{failTally:true}); await assert.rejects(e.pm.grantTokens(1,'bob',2,50),/injected tally failure/);
  assert.equal(e.pm.propertyIndex.get(1).totalInCirculation,12); assert.equal(e.rows()[0][1].totalInCirculation,10);
});
test('grant save failure follows successful tally call and leaves DB supply stale', async () => {
  const e = env([[1,row()]],{failSave:true}); await assert.rejects(e.pm.grantTokens(1,'bob',2,50),/injected DB write failure/);
  assert.equal(e.tally.length,1); assert.equal(e.pm.propertyIndex.get(1).totalInCirculation,12); assert.equal(e.rows()[0][1].totalInCirculation,10);
});
test('inspectPropertyIndex reloads DB and prints both representations without return value', async () => {
  const e = env([[1,row()]]); assert.equal(await e.pm.inspectPropertyIndex(),undefined);
  assert.equal(e.pm.propertyIndex.size,1); assert.deepEqual(e.logs.map(x=>x[1]),['Properties as Array:','Properties as Object:']);
});

if (process.env.PROPERTY_REVIEW_MANIFEST) {
  fs.writeFileSync(process.env.PROPERTY_REVIEW_MANIFEST, JSON.stringify({
    source: { path:sourcePath, sha256:crypto.createHash('sha256').update(sourceBytes).digest('hex'), executedUnchanged:true },
    runtime: { node:process.version, executable:process.execPath },
    realDependencies: [{name:'bignumber.js',version:bnPackage.version,path:bnPath,sha256:crypto.createHash('sha256').update(fs.readFileSync(bnPath)).digest('hex'),lfNormalizedSha256:BIG_NUMBER_SHA256,implementation:'Complete genuine repository-archived source; verified by existing legacy_numeric_oracle.cjs'},{name:'node:path',version:process.version}],
    capabilityBoundaries: ['async getDatabase and async findOneAsync/updateAsync: isolated in-memory deterministic rows; source DB module not executed, no NeDB persistence or RPC','tally.updateBalance: call recorder with injected rejection; actual tally implementation not executed, no real custody or balance invariant proven','console: call recorder; no other globals or PropertyManager methods injected'],
    expectedSourceDefectsArePassingCharacterizations:true,
    rustExecution:'NONE: static review only; no Cargo invoked; existing Rust tests read, never represented as newly passing',
    externalPatch:'8deba4a unavailable; no claim that its six files or purported tests were reviewed',
  },null,2)+'\n');
}
