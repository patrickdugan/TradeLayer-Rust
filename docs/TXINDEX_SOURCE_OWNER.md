# Literal TxIndex metadata owner

`txindex_source_owner.rs` implements all 18 static and 3 instance methods of
`src/txIndex.js`, plus its singleton constructor, from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete archived
source has LF SHA-256
`095449d90f511df44aff7b316105f57d4c0508ea8fb0b6f9c4a5f358edf6ca51`.
This is a backend-independent legacy compatibility component. It does not
introduce a Rust NeDB runtime or choose a physical storage backend. Recent
compact-index and web-worker storage work belongs to its existing owners;
adapting that storage to these reference semantics is a separate integration.
Scanner-to-owner integration,
whole-runtime replay, and authenticated transaction admission are not established
by these tests.

Six Rust tests verify the standalone component. The independent unchanged-source
corpus contains 320 observations: 319 supported observations compared for
outcomes, stable owner state, database rows, remaining failures and complete
ordered dependency effects; one separately counted RegExp-injection observation
is explicitly refused locally. The first ten observations are also checked as a
small initial proof, not counted again. The corpus includes 61 resets, 22
constructor calls, 4 module/database restarts, 211 static calls and 22 instance
calls. It executes real NeDB 1.8.0 and complete pinned TxUtils, Types and decoder
modules. Both archived BigNumber 9.3.1 and lockfile BigNumber 9.1.2 reproduce the
same checked observations. See [the source reference](TXINDEX_SOURCE_REFERENCE.md)
for package provisioning, full method inventories and fixture provenance.

## Preserved behavior

The static class client and `parseBlock` differ from the instance's `client:null`
and `parseBlock:0`. Every `getInstance` calls static `init`, even when the
singleton already exists. A failed init retains an already allocated singleton
and any client assignment made before its failing `.chain` dereference.
Import-time TxUtils initialization has its own original client Promise. Its
rejection survives the owner checkpoint: later `getTransactionOutputs` awaits
that same rejected Promise outside its catch, rather than manufacturing a new
RPC call or a replacement error.

Genesis initialization retains an existing row. MaxHeight and indexExists are
separate writes, and checkForIndex tests row presence rather than its value.
Source exceptions retain completed writes, including MaxHeight when writing the
flag fails. `processBlockData` catches a transaction-row write failure, continues
to its height/flag writes, and otherwise propagates the source failure. Instance
`saveTransactionData` uses its fifth argument on insert and its second argument
plus payload on update; it omits update options and catches database failures.
Validity and reason are written at the transaction row's top level. The first
matching NeDB row wins in the source regex/suffix lookups.

The first nulldata output wins. Marker detection remains lowercase hexadecimal
at nibble offset at most six, with the original substring offsets, malformed/odd
hex truncation and printable-byte test. The two reached TxUtils methods are
literal ports over original RPC transaction documents: getSender follows the
first input's parent output, while getTransactionOutputs rounds source Numbers
and filters addresses before TxIndex projects numeric vout 0/1. JSON objects with
a `$buffer` field stay ordinary objects in direct isPrintableASCII calls; they
are not fabricated Buffer instances. Calling Number.toString with `latin1`
preserves its actual RangeError.

`processTransaction` calls the existing actual Types port with exactly
`[txId, marker, payload]`, preserving the source's three-versus-nine argument
mismatch. Its unused async result remains `SourceValue::Promise` in the returned
transaction and in the actual database request. Dependency observations retain
the real fulfilled Error value or rejected exception. The source fixture's
delayed observer allows genuine host unhandled rejections before recording
settlement. No decoded validity or financial success is supplied by a fixture.

The unbound Datastore in clearTxIndex and unbound txIndexDB in resetIndexFlag
remain source ReferenceErrors. They do not clear the composing backend's rows.

## Integration contract

`TxIndexSourceOwner` owns only singleton/class/module cache metadata. It has no
database rows, RPC transaction store, Ledger, scanner journal or financial
authority. The intended sequence is `new`, explicit `load_module`, then
`construct` or `evaluate`/`invoke_source`. `load_module` models and drains the
original TxUtils import initializer, separately from static init. Environment
input is supplied with `set_forward_index`; it only enables forward mode for
the exact strings `1` and `true`.

The backend protocol is:

| Target | Arguments and meaning |
| --- | --- |
| `ClientWrapper.getInstance` | `[]` at TxUtils import; `[true]` for each static init. Return the actual bound client capability, or the actual source value/exception. |
| `client.getBlockCount` | `[client]` followed by the original zero arguments. |
| `client.getBlockHash`, `client.getBlock`, `client.decoderawtransaction` | `[client, originalArgument]`; original method spelling is retained. |
| `client.getRawTransaction` | `[client, ...originalArguments]`; the one-argument TxUtils calls differ from TxIndex's three arguments, including explicit Undefined omissions. |
| `db.getDatabase` | `["txIndex"]`, returning the composing backend's database capability. |
| `db.findOneAsync`, `db.findAsync`, `db.insertAsync`, `db.updateAsync` | `[database, ...originalArguments]`, with original positional omissions and options. |

The client handle is an adapter projection `{$rpcClient: positiveInteger}`;
backend bindings associate that identity with the actual original RPC input
source. A client descriptor may also carry its actual `chain` property for the
source init console expression. These capability descriptors are not evidence
of a trusted node or a financial verdict. The static and TxUtils clients must
remain independently bound. Database capabilities follow the existing
`{sourceDatabase:"txIndex"}` convention. RegExp query values follow the existing
`{$regexp:pattern,$flags:""}` descriptor convention.

The source DB request is recorded before storage conversion. NeDB deep-copying
a real Promise or Error produces `{}`; its memory rows retain Undefined, NaN,
Infinity and signed zero. Actual NeDB disk serialization omits undefined object
fields, uses null for nonfinite Numbers and undefined array entries, and writes
negative zero as zero. The Rust test backend independently implements the query,
copy, update and persistence subset exercised here, including recursive NeDB
field validation and duplicate `_id` rejection. It is a test backend, not a
complete NeDB implementation. Genuine Node-only duplicate-error own properties
are archived separately; `SourceException` transports name and message.
Constructed source Error values use the declared `$error` diagnostic projection
at this boundary; arbitrary caller objects are not proof of Error identity.

A future adapter can borrow the existing compact index or Core collection and
translate these compatibility calls over the same authoritative indexed rows.
To claim legacy source compatibility it must reproduce the observed copy/read
and restart values at that interface; this does not require storing a NeDB
database or reproducing its physical layout. A distinct modern storage policy
may intentionally change those semantics and should be tested and described
separately. No second persistence authority is needed. That composition has not
been implemented or tested by this component handoff. The existing parallel
scanner, raw block ingestion and external performance branch were not modified.

## Timing, bounds and checkpoints

The API provides stable, sequential, after-drain boundaries. The final unawaited
saveMaxHeight is drained before the component checkpoint; its rejection appears
in `deferred_rejections`, while extractBlockData still returns Undefined.
Source `atReturn` observations are preserved independently and explicitly tested
to differ from those stable boundaries: the background save can still have no
update started, and a restarted TxUtils client can still be uninitialized.
Exact microtask timing at outer Promise resolution, overlapping public commands,
and crashes between that return and the drain are not implemented or claimed.

SourceValue's depth/node/text limits apply, with at most 16 arguments, 1,024
scanned blocks, 4,096 dependency observations, 16 MiB of dependency payload and a
2 MiB encoded component state. Literal transaction IDs and independently verified
RegExp constructor failures for `[`, `(` and `)` are supported. Other injected
RegExp programs are explicit `UnsupportedNumericDomain` errors; no fabricated
match is returned. Prototype-mutating `__proto__` loadIndex keys, callable or
accessor-bearing user objects, unrepresentable lone UTF-16 surrogates and direct
BigNumber argument coercion are outside this bounded SourceValue domain. Internal
source Buffer operations used by decoding are implemented from their actual
bytes, without reinterpreting plain JSON objects as Buffers.

`TLTI` version 1 checkpoints contain only this component's cache and original
initializer rejection. CRC, exact state shape, canonical NaN encoding, singleton
and initializer invariants, byte lengths and exact re-encoding are checked.
Tests resume subsequent commands from restored checkpoints, reject corrupt or
inconsistent snapshots, and stop source-unbounded scans with an explicit local
Capacity error. Module loading restores the prior owner for local backend or
state-bound errors so it can be retried.

Local admission errors restore this owner only. A generic SourceBackend may
already have effects; atomic rollback of backend rows, RPC observations, failure
queues and receipts requires a candidate or transactional composing backend.
Source exceptions intentionally retain their completed prefix. A TLTI snapshot
alone is neither a whole-runtime snapshot nor replay or admission authority.

The default Rust tests locate scripts and fixtures through CARGO_MANIFEST_DIR
and execute the unchanged Node oracle with `--check`. Set
`TL_TXINDEX_LOCKED_DEPENDENCY=1` to additionally select `--lock`. The optional
`TL_TXINDEX_ORACLE_STAGE` path is for isolated work-only staging; normal clean
checkouts use the repository's own scripts and fixtures. An explicit provisioned
npm prefix is required as described in the source reference.
