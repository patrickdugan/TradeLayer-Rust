# Complete pinned txIndex source reference

`scripts/txindex_source_vectors.cjs` executes the complete unchanged pinned
`txIndex.js`, full `TxUtils`, `Types`, encoder, decoder and base-conversion
modules. It supplies RPC documents and a genuine NeDB receiver at the declared
external dependency boundary. It supplies no sender, reference-output,
decoder-success, validity or financial verdict.

The source commit is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete `src/txIndex.js`
archive is `fixtures/legacy_txindex_source_txIndex.txt`, 19,891 LF-normalized
bytes, Git blob `3d7bdafbdffe5af7d3250c4dcc80b8358c485597`, SHA256
`095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51`.
The generator checks that blob and every archived dependency before evaluation.
`fixtures/txindex_source_method_inventory.json` records all module pins, every
actual method's source-body hash and line, and its observed cases. It asserts
that all reflected source methods appear in the corpus.

## Reproduction and dependencies

The reference requires Node 24 and four explicitly provisioned genuine npm
packages. The tested versions are `nedb@1.8.0`, `big-json@3.2.0`,
`bitcore-lib-ltc@10.10.5` and `bitcoinjs-lib@6.1.8`. Their versions are checked at
startup. NeDB's reached `model.js` and `datastore.js` are additionally checked
against their raw SHA256 values, respectively
`57d4551a20abcd4efe48880700d30378f09b7e8b145617010729562837b27258` and
`9c5575bdaba5127b748305a27f0cac7727595aa9f2f2c15f8735254d765df8ee`.

From a clean mounted repository, provision a separate npm prefix and select it
explicitly. This PowerShell example does not install packages into the source
tree's `node_modules`:

```powershell
npm install --prefix work/txindex-js-reference --ignore-scripts --no-audit --no-fund --save-exact nedb@1.8.0 big-json@3.2.0 bitcore-lib-ltc@10.10.5 bitcoinjs-lib@6.1.8
$env:TRADELAYER_JS_ROOT = (Resolve-Path work/txindex-js-reference).Path
node scripts/txindex_source_vectors.cjs --proof10 --check
node scripts/txindex_source_vectors.cjs --check
node scripts/txindex_source_vectors.cjs --lock --check
```

CI can use its already provisioned reference prefix, adding the exact NeDB and
big-json packages to its existing exact crypto dependencies. Set
`TRADELAYER_JS_ROOT` to that prefix before both Node reference checks and Rust
tests that launch the reference. `TL_JS_MODULE_ROOT` and `--module-root PATH`
are explicit alternatives. Resolution uses `createRequire` from the selected
prefix. A missing prefix or package fails; there is no fallback to an
uncommitted local JavaScript checkout. No TradeLayer checkout, npm package,
wallet or RPC server is needed beyond the declared dependencies. Source
archives and BigNumber references come from checked-in repository fixtures.

NeDB 1.8.0 expects removed legacy Node utility functions. On Node 24 the driver
restores only missing `util.isDate` and `util.isRegExp`, using the identical
`util.types.isDate` and `util.types.isRegExp` functions. It leaves the existing
`util.isArray` alone; Node's deprecation warning is expected. This is an
explicit host-library compatibility boundary, not a TradeLayer source edit.
NeDB calls use actual `util.promisify` wrappers, matching the reached API in
pinned `db.js`. The source's async `getDatabase` factory is retained.

Ordinary cases use a genuine in-memory NeDB instance. The durable restart
scene uses a genuine file-backed instance in a fresh
`work/nedb/case-*` directory beneath the generator's root, then reopens that
actual file. These scratch files are not fixture inputs and are never read
from a previous run. The driver does not run production `db.js` startup,
select a real network, or open existing user databases.

## Inventory and observed behavior

There are **21 callable source methods: 18 static and 3 instance methods**,
plus the constructor. Only `isPrintableASCII` and the constructor are
synchronous. The precise reflected inventory is:

| Static method | Reached behavior |
| --- | --- |
| `init` | Bind the class client, return the class |
| `getInstance` | Create/reuse the singleton; always repeat initialization |
| `initializeIndex` | Preserve an existing genesis document |
| `extractBlockData` | Sequential scan, forward-mode resume, unawaited final save |
| `saveMaxHeight` | Write `MaxHeight`, then the `indexExists` flag |
| `fetchChainTip` | Preserve the source's wrapped RPC error |
| `fetchBlockData` | Fetch hash then block, rethrow failures |
| `processBlockData` | Process in source order, write each transaction, save height |
| `fetchTransactionData` | Preserve raw RPC arguments and failures |
| `DecodeRawTransaction` | First nulldata, marker offsets, ASCII guard and swallowed failures |
| `isPrintableASCII` | Actual JavaScript `toString('latin1')` and regexp behavior |
| `processTransaction` | Actual TxUtils reads and three-argument Types call |
| `upsertTxValidityAndReason` | Update the first matching existing row only |
| `clearTxIndex` | Actual unbound `Datastore` ReferenceError |
| `resetIndexFlag` | Actual unbound `txIndexDB` ReferenceError |
| `findMaxIndexedBlock` | Return the raw stored value or null |
| `getTransactionData` | Actual dynamic regexp query and syntax errors |
| `checkForIndex` | Test row presence; swallow source read failures |

The instance methods are `saveTransactionData`, `loadIndex` and
`initializeOrLoadDB`. The first insert stores its final `txDetails` argument;
an update uses the second `txData` argument and payload. `loadIndex` assigns
each row's `value` to an ordinary JavaScript object, including undefined values
and integer-property ordering. Both initialization methods keep an existing
genesis value. The constructor always returns the same singleton. Static
initialization assigns the class client while the singleton's own client
remains null and its own `parseBlock` remains zero.

The final corpus has **320 unique observations: 61 resets, 22 direct constructor
calls, 4 restarts, 211 static calls and 22 instance calls**. Thirty calls inject
failures at actual external call boundaries. Cases cover first and later
database writes, retained prefixes and retries, raw client-factory failures and
values, null/undefined/nonfinite flags, malformed RPC documents, marker offsets,
odd/invalid hex, printable and nonprintable payloads, sequential mixed blocks,
duplicate block processing, raw source validity fields, dynamic regexp syntax
errors, forward mode and actual string concatenation of a stored string height.
Real NeDB failures also cover nested dollar and dotted document keys in genesis,
the first `saveTransactionData` insert, its different update argument, and
validity/reason updates. The source's caught failures preserve existing rows;
uncaught failures retain their original errors. Each bad-key attempt is followed
by a genuine valid retry. These failures come from NeDB itself, without an
injected failure verdict. Duplicate IDs are checked in the separate genuine
NeDB boundary fixture; the sequential source find-before-insert graph does not
manufacture an otherwise unavailable duplicate insertion.

The raw serialized legacy/SegWit cases reuse
`fixtures/txindex_segwit_hash_vectors.tsv` and parse those bytes with genuine
bitcoinjs. All RPC responses, parent documents and block inputs remain explicit
unverified capability inputs. Other adversarial RPC records are deliberately
synthetic or malformed. They test the original source's handling of supplied
RPC data, and are not chain, transaction-authentication or UTXO proofs.

## Promise, persistence and timing distinctions

`processTransaction` calls `Types.decodePayload(txId, marker, payload)` without
awaiting it. Its returned object contains the original Promise. An ordinary
payload therefore produces a Promise fulfilled with `Error('Invalid payload')`;
the exact special payload `tl` reaches the unknown-type Error, while a numeric
marker can produce a rejected TypeError. No successful decoder answer is
invented to bypass that source defect.

Promise IDs identify the original Promise objects. A transparent observer
records the unchanged Types call and its original return. It attaches no
fulfillment or rejection handler until after the actual source/NeDB drain and
the first host unhandled-rejection checkpoint. Thus genuine rejected futures
emit their real `backgroundErrors`. Late diagnostic observation then records
their fulfillment or rejection; resulting observer-only `rejectionHandled`
events are not represented as source events. At caller return, a newly seen
Promise is explicitly `unobserved`, not asserted to be pending. The startup
TxUtils IIFE is separately exercised: its client-factory rejection survives
module restart and is later rethrown by the original cached client Promise
before any new RPC call.

The test host installs an `unhandledRejection` listener to record the original
rejection and inspect its persisted prefix. This deliberately permits inspection
to continue; it does not claim that a production Node process survives its
default unhandled-rejection termination policy. The recorded method return,
original rejection and writes remain distinct observations.

NeDB receives the original unawaited Promise. Its genuine memory copy stores
that Promise as `{}`. A returned Error object is also copied as `{}`. Memory
rows retain undefined properties, NaN, infinities and negative zero; actual file
serialization omits undefined object fields, converts nonfinite numbers and
undefined array entries to null, and negative zero to zero. The separate
`fixtures/txindex_nedb_boundary.json` records real deep-copy, serializer,
deserializer, invalid-field and duplicate-key behavior. The source lifecycle's
durable scene reopens the file and compares the real changed rows.

Each record has `outcome`, `atReturn` and drained `state`/`effects`. The primary
return of `extractBlockData` can succeed before its final height save finishes,
and a later failed save can emit an unhandled rejection while retaining its
actual partial writes. The corpus retains both snapshots. A component port
that only exposes a stable-drain API must state that limitation; these records
do not license claiming exact caller-return scheduling parity.

## Format and limits

The JSONL records are readable expected observations, not dependency answers
for the Rust implementation. Commands declare source receiver, original
arguments and external configuration. Effects retain actual call order,
argument count and returned/thrown values. `db.*` effects omit the adapter
receiver; the Rust backend may prepend its explicit source-database handle.
State includes the class/singleton distinction, complete NeDB rows, original
TxUtils client, environment value, pending failure schedule and separately
observed Types futures. Source console messages are retained as diagnostics.

Reserved diagnostic tags represent undefined, nonfinite numbers, errors,
Promises, classes, instances and genuine RPC/database handles. They are not an
authentication scheme or a general untrusted object codec. In particular,
ordinary direct `isPrintableASCII` arguments `{$buffer:'00'}` and
`{$buffer:'ff'}` are literal objects: the source stringifies them as
`[object Object]` and returns true. They must not be fabricated into Buffers.
Internal DecodeRawTransaction buffers are constructed by the unchanged source.

The `getTransactionData('.')` record is explicitly marked `arbitrary-regexp`:
it records actual regexp-injection semantics separately from a portable
component's declared literal-pattern domain. Invalid regexps `[`, `(` and `)`
record genuine source SyntaxErrors. A local unsupported-domain refusal cannot
be reported as the same source result. Accessors, functions, arbitrary
prototypes and arbitrary ambient `Datastore`/`txIndexDB` globals are not claimed
as supported caller input. The two broken clear/reset methods are evaluated in
the actual default module environment, where their free globals are absent.

Both complete reference runs compare byte for byte under archived genuine
BigNumber 9.3.1 and 9.1.2. Their raw source SHA256 values are
`58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16` and
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.
The reached graph loads those constructors; the source's wrong decoder call
does not establish broad financial arithmetic coverage or all-version parity.

This reference proves source behavior under its declared inputs. An owner-cache
checkpoint alone does not bind database rows, RPC inputs, failure schedules,
an external expected replay head or deferred dependency receipts. Whole-state
replay needs those independently bound inputs. A local resource/domain error
also needs rollback of the composing backend if earlier writes occurred;
rolling back only the owner is not whole-execution rollback. No index result
here asserts canonical-chain membership, actor authority, script validation,
custody or historical acceptance.
