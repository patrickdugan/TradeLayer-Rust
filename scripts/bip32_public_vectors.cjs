#!/usr/bin/env node
'use strict';
// Genuine bitcore source execution + independent Node/OpenSSL HMAC and
// tiny-secp256k1/libsecp256k1 public point tweaks. No dependency substitutes.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const {spawnSync} = require('node:child_process');
const root = path.resolve(__dirname, '..');
const output = path.join(root, 'fixtures', 'bip32_public_vectors.tsv');
const libArg = process.argv.indexOf('--lib-root');
assert(libArg >= 0, 'Pass --lib-root pointing to a genuine installed tradelayer.js checkout');
const req = createRequire(path.join(path.resolve(process.argv[libArg + 1]), 'package.json'));
// Both bitcore packages patch the same cached elliptic point prototype. Loading
// both in one process corrupts that prototype. Genuine oracles therefore run
// in separate processes, just as successful LTC and BTC-fallback source runs do.
const workerArg = process.argv.indexOf('--oracle-library');
const workerLibrary = workerArg >= 0 ? process.argv[workerArg + 1] : null;
assert(workerLibrary === null || ['ltc', 'btc'].includes(workerLibrary));
const ltc = workerLibrary ? req(workerLibrary === 'ltc' ? 'bitcore-lib-ltc' : 'bitcore-lib') : null;
const btc = ltc, tiny = req('tiny-secp256k1');
const sha = data => crypto.createHash('sha256').update(data).digest();
const hash160 = data => crypto.createHash('ripemd160').update(sha(data)).digest();
const hmac = (key, data) => crypto.createHmac('sha512', key).update(data).digest();
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const P = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const buf = n => Buffer.from(n.toString(16).padStart(64, '0'), 'hex');
const big = b => BigInt('0x' + Buffer.from(b).toString('hex'));
const hx = b => Buffer.from(b).toString('hex');
const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
function encode(data) {
  const bytes = Buffer.concat([data, sha(sha(data)).subarray(0, 4)]);
  let n = big(bytes), out = '';
  while (n) { out = alphabet[Number(n % 58n)] + out; n /= 58n; }
  for (const b of bytes) { if (b) break; out = '1' + out; }
  return out;
}
function decode(text) {
  let n = 0n;
  for (const ch of text) { assert(alphabet.includes(ch)); n = n * 58n + BigInt(alphabet.indexOf(ch)); }
  let hex = n.toString(16); if (hex.length % 2) hex = '0' + hex;
  let bytes = Buffer.from(hex, 'hex');
  for (const ch of text) { if (ch !== '1') break; bytes = Buffer.concat([Buffer.from([0]), bytes]); }
  const data = bytes.subarray(0, -4);
  assert.deepEqual(sha(sha(data)).subarray(0, 4), bytes.subarray(-4));
  return data;
}
const publicVectors = [
  ['v1-m', 'xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8'],
  ['v1-0h', 'xpub68Gmy5EdvgibQVfPdqkBBCHxA5htiqg55crXYuXoQRKfDBFA1WEjWgP6LHhwBZeNK1VTsfTFUHCdrfp1bgwQ9xv5ski8PX9rL2dZXvgGDnw'],
  ['v1-0h-1', 'xpub6ASuArnXKPbfEwhqN6e3mwBcDTgzisQN1wXN9BJcM47sSikHjJf3UFHKkNAWbWMiGj7Wf5uMash7SyYq527Hqck2AxYysAA7xmALppuCkwQ'],
  ['v1-0h-1-2h', 'xpub6D4BDPcP2GT577Vvch3R8wDkScZWzQzMMUm3PWbmWvVJrZwQY4VUNgqFJPMM3No2dFDFGTsxxpG5uJh7n7epu4trkrX7x7DogT5Uv6fcLW5'],
  ['v1-0h-1-2h-2', 'xpub6FHa3pjLCk84BayeJxFW2SP4XRrFd1JYnxeLeU8EqN3vDfZmbqBqaGJAyiLjTAwm6ZLRQUMv1ZACTj37sR62cfN7fe5JnJ7dh8zL4fiyLHV'],
  ['v1-0h-1-2h-2-1000000000', 'xpub6H1LXWLaKsWFhvm6RVpEL9P4KfRZSW7abD2ttkWP3SSQvnyA8FSVqNTEcYFgJS2UaFcxupHiYkro49S8yGasTvXEYBVPamhGW6cFJodrTHy'],
  ['v2-m', 'xpub661MyMwAqRbcFW31YEwpkMuc5THy2PSt5bDMsktWQcFF8syAmRUapSCGu8ED9W6oDMSgv6Zz8idoc4a6mr8BDzTJY47LJhkJ8UB7WEGuduB'],
  ['v2-0', 'xpub69H7F5d8KSRgmmdJg2KhpAK8SR3DjMwAdkxj3ZuxV27CprR9LgpeyGmXUbC6wb7ERfvrnKZjXoUmmDznezpbZb7ap6r1D3tgFxHmwMkQTPH'],
  ['v2-0-2147483647h', 'xpub6ASAVgeehLbnwdqV6UKMHVzgqAG8Gr6riv3Fxxpj8ksbH9ebxaEyBLZ85ySDhKiLDBrQSARLq1uNRts8RuJiHjaDMBU4Zn9h8LZNnBC5y4a'],
  ['v2-0-2147483647h-1', 'xpub6DF8uhdarytz3FWdA8TvFSvvAh8dP3283MY7p2V4SeE2wyWmG5mg5EwVvmdMVCQcoNJxGoWaU9DCWh89LojfZ537wTfunKau47EL2dhHKon'],
  ['v2-0-2147483647h-1-2147483646h', 'xpub6ERApfZwUNrhLCkDtcHTcxd75RbzS1ed54G1LkBUHQVHQKqhMkhgbmJbZRkrgZw4koxb5JaHWkY4ALHY2grBGRjaDMzQLcgJvLJuZZvRcEL'],
  ['v2-0-2147483647h-1-2147483646h-2', 'xpub6FnCn6nSzZAw5Tw7cgR9bi15UV96gLZhjDstkXXxvCLsUXBGXPdSnLFbdpq8p9HmGsApME5hQTZ3emM2rnY5agb9rXpVGyy3bdW6EEgAtqt'],
  ['v3-m', 'xpub661MyMwAqRbcEZVB4dScxMAdx6d4nFc9nvyvH3v4gJL378CSRZiYmhRoP7mBy6gSPSCYk6SzXPTf3ND1cZAceL7SfJ1Z3GC8vBgp2epUt13'],
  ['v3-0h', 'xpub68NZiKmJWnxxS6aaHmn81bvJeTESw724CRDs6HbuccFQN9Ku14VQrADWgqbhhTHBaohPX4CjNLf9fq9MYo6oDaPPLPxSb7gwQN3ih19Zm4Y'],
  ['v4-m', 'xpub661MyMwAqRbcGczjuMoRm6dXaLDEhW1u34gKenbeYqAix21mdUKJyuyu5F1rzYGVxyL6tmgBUAEPrEz92mBXjByMRiJdba9wpnN37RLLAXa'],
  ['v4-0h', 'xpub69AUMk3qDBi3uW1sXgjCmVjJ2G6WQoYSnNHyzkmdCHEhSZ4tBok37xfFEqHd2AddP56Tqp4o56AePAgCjYdvpW2PU2jbUPFKsav5ut6Ch1m'],
  ['v4-0h-1h', 'xpub6BJA1jSqiukeaesWfxe6sNK9CCGaujFFSJLomWHprUL9DePQ4JDkM5d88n49sMGJxrhpjazuXYWdMf17C9T5XnxkopaeS7jGk1GyyVziaMt'],
];
const invalidVectors = [
  ['public-version-private-data', 'xpub661MyMwAqRbcEYS8w7XLSVeEsBXy79zSzH1J8vCdxAZningWLdN3zgtU6LBpB85b3D2yc8sfvZU521AAwdZafEz7mnzBBsz4wKY5fTtTQBm'],
  ['private-version-public-data', 'xprv9s21ZrQH143K24Mfq5zL5MhWK9hUhhGbd45hLXo2Pq2oqzMMo63oStZzFGTQQD3dC4H2D5GBj7vWvSQaaBv5cxi9gafk7NF3pnBju6dwKvH'],
  ['public-prefix04', 'xpub661MyMwAqRbcEYS8w7XLSVeEsBXy79zSzH1J8vCdxAZningWLdN3zgtU6Txnt3siSujt9RCVYsx4qHZGc62TG4McvMGcAUjeuwZdduYEvFn'],
  ['private-prefix04', 'xprv9s21ZrQH143K24Mfq5zL5MhWK9hUhhGbd45hLXo2Pq2oqzMMo63oStZzFGpWnsj83BHtEy5Zt8CcDr1UiRXuWCmTQLxEK9vbz5gPstX92JQ'],
  ['public-prefix01', 'xpub661MyMwAqRbcEYS8w7XLSVeEsBXy79zSzH1J8vCdxAZningWLdN3zgtU6N8ZMMXctdiCjxTNq964yKkwrkBJJwpzZS4HS2fxvyYUA4q2Xe4'],
  ['private-prefix01', 'xprv9s21ZrQH143K24Mfq5zL5MhWK9hUhhGbd45hLXo2Pq2oqzMMo63oStZzFAzHGBP2UuGCqWLTAPLcMtD9y5gkZ6Eq3Rjuahrv17fEQ3Qen6J'],
  ['private-root-fingerprint', 'xprv9s2SPatNQ9Vc6GTbVMFPFo7jsaZySyzk7L8n2uqKXJen3KUmvQNTuLh3fhZMBoG3G4ZW1N2kZuHEPY53qmbZzCHshoQnNf4GvELZfqTUrcv'],
  ['public-root-fingerprint', 'xpub661no6RGEX3uJkY4bNnPcw4URcQTrSibUZ4NqJEw5eBkv7ovTwgiT91XX27VbEXGENhYRCf7hyEbWrR3FewATdCEebj6znwMfQkhRYHRLpJ'],
  ['private-root-index', 'xprv9s21ZrQH4r4TsiLvyLXqM9P7k1K3EYhA1kkD6xuquB5i39AU8KF42acDyL3qsDbU9NmZn6MsGSUYZEsuoePmjzsB3eFKSUEh3Gu1N3cqVUN'],
  ['public-root-index', 'xpub661MyMwAuDcm6CRQ5N4qiHKrJ39Xe1R1NyfouMKTTWcguwVcfrZJaNvhpebzGerh7gucBvzEQWRugZDuDXjNDRmXzSZe4c7mnTK97pTvGS8'],
  ['unknown-version-private', 'DMwo58pR1QLEFihHiXPVykYB6fJmsTeHvyTp7hRThAtCX8CvYzgPcn8XnmdfHGMQzT7ayAmfo4z3gY5KfbrZWZ6St24UVf2Qgo6oujFktLHdHY4'],
  ['unknown-version-public', 'DMwo58pR1QLEFihHiXPVykYB6fJmsTeHvyTp7hRThAtCX8CvYzgPcn8XnmdfHPmHJiEDXkTiJTVV9rHEBUem2mwVbbNfvT2MTcAqj3nesx8uBf9'],
  ['private-zero', 'xprv9s21ZrQH143K24Mfq5zL5MhWK9hUhhGbd45hLXo2Pq2oqzMMo63oStZzF93Y5wvzdUayhgkkFoicQZcP3y52uPPxFnfoLZB21Teqt1VvEHx'],
  ['private-order', 'xprv9s21ZrQH143K24Mfq5zL5MhWK9hUhhGbd45hLXo2Pq2oqzMMo63oStZzFAzHGBP2UuGCqWLTAPLcMtD5SDKr24z3aiUvKr9bJpdrcLg1y3G'],
  ['public-off-curve', 'xpub661MyMwAqRbcEYS8w7XLSVeEsBXy79zSzH1J8vCdxAZningWLdN3zgtU6Q5JXayek4PRsn35jii4veMimro1xefsM58PgBMrvdYre8QyULY'],
  ['checksum', 'xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHL'],
];

// Independent CKDpub uses libsecp256k1 WASM pointAddScalar, not bitcore's
// elliptic/BN implementation and not Rust's Montgomery/Jacobian code.
function derive(parent, index, sourceMode = false) {
  assert(index < 0x80000000);
  const raw = decode(parent), data = Buffer.alloc(37);
  if (sourceMode && big(raw.subarray(46)) >= P) buf(big(raw.subarray(46)) % P).copy(raw, 46);
  raw.copy(data, 0, 45); data.writeUInt32BE(index, 33);
  const digest = hmac(raw.subarray(13, 45), data);
  let scalar = big(digest.subarray(0, 32));
  if (sourceMode) scalar %= N;
  else if (scalar >= N) return derive(parent, index + 1);
  const point = tiny.pointAddScalar(raw.subarray(45), buf(scalar), true);
  if (!point) return derive(parent, index + 1, sourceMode);
  const child = Buffer.alloc(78);
  raw.copy(child, 0, 0, 4); child[4] = (raw[4] + 1) & 255;
  hash160(raw.subarray(45)).copy(child, 5, 0, 4);
  child.writeUInt32BE(index, 9); digest.copy(child, 13, 32); Buffer.from(point).copy(child, 45);
  return encode(child);
}
function address(xpub, prefix) {
  return encode(Buffer.concat([Buffer.from([prefix]), hash160(decode(xpub).subarray(45))]));
}
function synthetic(depth = 0) {
  const raw = Buffer.alloc(78); raw.writeUInt32BE(0x0488b21e);
  raw[4] = depth; raw.fill(1, 13, 45);
  Buffer.from(tiny.pointFromScalar(buf(1n), true)).copy(raw, 45);
  return encode(raw);
}

const reference = fs.readFileSync(path.join(root, 'fixtures', 'legacy_clearlist_audit_reference.txt'), 'utf8');
const match = reference.match(/static deriveAddressesFromXpub\(xpub, network\) \{[\s\S]*?\n    \}/);
assert(match, 'Frozen source function not found');
function sourceClass(library) {
  const context = {require(name) {
    if (library === 'btc' && name === 'bitcore-lib-ltc') throw new Error('Litecoin absent (explicit fallback scenario)');
    return req(name);
  }, console: {error() {}}};
  return vm.runInNewContext(`class Source { static xpubCache = new Map(); ${match[0]} }; Source`, context);
}

function generate() {
  const rows = [
    '# BIP32 public corpus v1; BIP32 published vectors are BSD-2-Clause.',
    '# Spec: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki',
    '# source-function-sha256=' + hx(sha(match[0])),
  ];
  for (const name of ['bitcore-lib-ltc', 'bitcore-lib', 'tiny-secp256k1']) {
    const file = req.resolve(name), dir = name === 'tiny-secp256k1' ? path.resolve(path.dirname(file), '../..') : path.dirname(file);
    const pkg = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8'));
    rows.push('# genuine ' + name + '=' + pkg.version);
    for (const relative of name === 'tiny-secp256k1' ? ['lib/cjs/index.cjs', 'lib/secp256k1.wasm'] : ['lib/hdpublickey.js', 'lib/crypto/point.js', 'lib/networks.js', 'lib/publickey.js']) {
      rows.push('# ' + name + '/' + relative + ' sha256=' + hx(sha(fs.readFileSync(path.join(dir, relative)))));
    }
  }
  for (const [name, xpub] of publicVectors) {
    const key = new ltc.HDPublicKey(xpub); assert.equal(key.xpubkey, xpub);
    const raw = decode(xpub);
    rows.push(['P', name, xpub, raw[4], hx(raw.subarray(5, 9)), raw.readUInt32BE(9), hx(raw.subarray(13, 45)), hx(raw.subarray(45))].join('\t'));
  }
  for (const [name, xpub] of invalidVectors) rows.push(['I', name, xpub].join('\t'));
  const edges = [
    ['v1-0h-1', publicVectors[1][1], 1, publicVectors[2][1]],
    ['v1-2h-2', publicVectors[3][1], 2, publicVectors[4][1]],
    ['v1-last', publicVectors[4][1], 1000000000, publicVectors[5][1]],
    ['v2-0', publicVectors[6][1], 0, publicVectors[7][1]],
    ['v2-1', publicVectors[8][1], 1, publicVectors[9][1]],
    ['v2-2', publicVectors[10][1], 2, publicVectors[11][1]],
  ];
  for (let i = 0; i < 48; i++) {
    const parent = publicVectors[i % publicVectors.length][1];
    const index = i < 4 ? [0, 1, 0x7ffffffe, 0x7fffffff][i] : sha('child-index:' + i).readUInt32BE() & 0x7fffffff;
    edges.push(['independent-' + i, parent, index, derive(parent, index)]);
  }
  for (const [name, parent, index, child] of edges) {
    assert.equal(derive(parent, index), child, name);
    assert.equal(new ltc.HDPublicKey(parent).deriveChild(index).xpubkey, child, name + ' ltc');
    assert.equal(new btc.HDPublicKey(parent).deriveChild(index).xpubkey, child, name + ' btc');
    rows.push(['D', name, parent, index, child, address(child, 0), address(child, 0x30), address(child, 0x6f)].join('\t'));
  }
  const sourceRoots = [publicVectors[0], publicVectors[1], publicVectors[12]];
  const testRaw = decode(publicVectors[0][1]); testRaw.writeUInt32BE(0x043587cf);
  sourceRoots.push(['tpub', encode(testRaw)]);
  for (const name of ['public-root-fingerprint', 'public-root-index']) sourceRoots.push(invalidVectors.find(row => row[0] === name));
  sourceRoots.push(['depth254', synthetic(254)], ['depth255', synthetic(255)]);
  for (const [name, x, prefix] of [['field-overflow-even', P + 1n, 2], ['field-overflow-odd', P + 2n, 3]]) {
    const raw = decode(publicVectors[0][1]); raw[45] = prefix; buf(x).copy(raw, 46);
    sourceRoots.push([name, encode(raw)]);
  }
  for (const library of [workerLibrary]) {
    const Source = sourceClass(library);
    for (const [name, xpub] of sourceRoots) {
      const result = Source.deriveAddressesFromXpub(xpub, 'deliberately-ignored');
      assert.equal(result.length, 40, name);
      assert.strictEqual(Source.deriveAddressesFromXpub(xpub, 'other-network'), result, 'source cache is per xpub');
      const prefix = decode(xpub).readUInt32BE() === 0x043587cf ? 0x6f : library === 'ltc' ? 0x30 : 0;
      for (const chain of [0, 1]) for (let i = 0; i < 20; i++) {
        const child = derive(derive(xpub, chain, true), i, true);
        assert.equal(result[chain * 20 + i], address(child, prefix));
        rows.push(['S', library, name, xpub, chain, i, result[chain * 20 + i]].join('\t'));
      }
    }
    for (const [name, xpub] of [['invalid', 'bad-xpub'], ['private', 'xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi']]) {
      const result = Source.deriveAddressesFromXpub(xpub, null);
      assert.equal(result.length, name === 'private' ? 40 : 0);
      assert.equal(Source.xpubCache.has(xpub), name === 'private');
      rows.push(['O', library, name, xpub, result.length, Source.xpubCache.has(xpub)].join('\t'));
    }
  }
  // Force boundary HMAC outputs while still executing genuine library CKDpub.
  // The original Hash function is restored after each test, including throws.
  for (const [library, lib] of [[workerLibrary, ltc]]) {
    for (const [name, scalar] of [['zero', 0n], ['order', N], ['order-plus-one', N + 1n], ['maximum', (1n << 256n) - 1n], ['infinity', N - 1n]]) {
      const previous = lib.crypto.Hash.sha512hmac; const indices = [];
      lib.crypto.Hash.sha512hmac = function(data) {
        indices.push(data.readUInt32BE(33));
        return Buffer.concat([buf(indices.length === 1 ? scalar : 1n), Buffer.alloc(32, indices.length)]);
      };
      try {
        const child = new lib.HDPublicKey(synthetic()).deriveChild(7);
        const used = name === 'infinity' ? 8 : 7;
        assert.deepEqual(indices, name === 'infinity' ? [7, 8] : [7]);
        const expected = tiny.pointAddScalar(decode(synthetic()).subarray(45), buf(name === 'infinity' ? 1n : scalar % N), true);
        assert.equal(hx(child.publicKey.toBuffer()), hx(expected));
        rows.push(['C', library, name, synthetic(), hx(buf(scalar)), used, child.xpubkey, indices.length].join('\t'));
      } finally { lib.crypto.Hash.sha512hmac = previous; }
    }
  }
  for (let i = 0; i < 96; i++) {
    const scalar = big(sha('parent-public-point:' + i)) % (N - 1n) + 1n;
    const key = Buffer.from(tiny.pointFromScalar(buf(scalar), i % 2 === 0));
    const tweak = i < 5 ? [0n, 1n, N - 1n, N, N - scalar][i] : big(sha('public-tweak:' + i));
    let expected;
    try { const point = tiny.pointAddScalar(key, buf(tweak), true); expected = point ? hx(point) : 'infinity'; }
    catch { expected = 'arithmetic'; }
    rows.push(['T', 'tweak-' + i, hx(key), hx(buf(tweak)), expected].join('\t'));
  }
  for (const size of [0, 1, 3, 63, 64, 111, 112, 113, 127, 128, 129, 239, 240, 255, 256, 257, 1024]) {
    const data = Buffer.from(Array.from({length: size}, (_, i) => (i * 31 + 7) & 255));
    rows.push(['H', 'sha512-' + size, hx(data) || '-', crypto.createHash('sha512').update(data).digest('hex')].join('\t'));
    for (const keySize of [0, 32, 128, 131]) {
      const key = Buffer.alloc(keySize, 0xaa);
      rows.push(['M', 'hmac-' + size + '-' + keySize, hx(key) || '-', hx(data) || '-', hx(hmac(key, data))].join('\t'));
    }
  }
  return rows.join('\n') + '\n';
}
let result;
if (workerLibrary) {
  process.stdout.write(generate());
  process.exit(0);
} else {
  const all = [];
  for (const library of ['ltc', 'btc']) {
    const child = spawnSync(process.execPath, [__filename, '--lib-root', process.argv[libArg + 1], '--oracle-library', library], {encoding: 'utf8', maxBuffer: 8 * 1024 * 1024});
    assert.equal(child.status, 0, child.stderr);
    all.push(...child.stdout.trimEnd().split('\n'));
  }
  result = [...new Set(all)].join('\n') + '\n';
}
if (process.argv.includes('--write')) fs.writeFileSync(output, result);
else assert.equal(fs.readFileSync(output, 'utf8'), result, 'Stored BIP32 corpus differs; inspect before regeneration');
const counts = {};
for (const line of result.split('\n')) if (line && !line.startsWith('#')) counts[line[0]] = (counts[line[0]] || 0) + 1;
console.log(JSON.stringify({genuineLibraries: true, operation: process.argv.includes('--write') ? 'wrote' : 'checked', counts}));
