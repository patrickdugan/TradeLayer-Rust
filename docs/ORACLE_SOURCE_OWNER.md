# Pinned oracle document and singleton owner

`oracle_source_owner::OracleSourceOwner` executes the complete metadata and
publication surface of `src/oracle.js` at commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete fixture is byte-equal to
`git show <commit>:src/oracle.js`, with SHA256
`70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d`.

This is mutable ownership of two document collections and both singleton Maps.
It contains no Ledger. It does not mirror the stricter `Registry` oracle records
or use a projection as the authoritative document. The older `oracle::OracleStore`
remains a separate typed API: it rejects unknown feeds, flattens configuration,
checks callers, and repairs several failed-write behaviors. Those changes make
it unsuitable as the implementation of the pinned source owner.

## State and lifecycle

The owner holds insertion-ordered, complete `oracleList` and `oracleData` rows,
the singleton `oracles` Map, and static `lastOracleUpdateBlock` Map. Documents use
`validity_source::SourceValue`; imported nested `name`, `type`, administrator and
backup fields, `clearlists`, `whitelists`, lag, unknown fields, and arbitrary data
payloads survive storage and checkpoints without flattening. Source-created
documents include only the fields that `createOracle` actually constructs.

One attached owner represents the module's singleton. `new()` starts the empty
module state; constructor/getInstance sharing is expressed by reusing that owner.
`addOracle`, `saveOracleData`, and `loadOracleData` are instance methods exposed
through the same dispatcher. Function objects and Map handles returned by
`getInstance()` are not fabricated as serializable source values.

The database and loaded Map contain independently cloned documents. `load`
merges durable rows into the Map without clearing stale entries, and swallows
source database exceptions. `getAllOracles` always loads; `getOracleInfo` loads
only when the Map is empty and may fall back to a database row without caching
it. Publication writes its latest payload to the Map and price collection,
without persisting that payload to `oracleList`. A later load, administrator
update, or closure can therefore replace the cached publication fields.

`getNextId` examines durable documents first, using `Number(doc.id ?? keyPart)`.
If no positive finite ID exists, it parses cached string keys with `parseInt`.
It retains fractional/unsafe Number IDs, ignores cached larger IDs when the
database has a positive ID, and throws when a fallback cache key has no `split`.
`addOracle` updates the Map before attempting its insert, including duplicate
insert failures; `createOracle` inserts before caching.

## Source operations

| Methods | Observable behavior |
| --- | --- |
| `getAllOracles`, `load`, `getOracleInfo`, `getNextId` | Actual database/cache reads, load swallowing, stale entries and allocation rules. |
| `addOracle`, `createOracle` | Source object spread/defaulting, typed keys, all constructed fields, original failure ordering. |
| `isAdmin` | Reads the durable row and uses primary/backup fields with nested `name` fallback. A missing row and an undefined sender can return true, as in source. |
| `verifyAdmin` | Checks cached primary address first, then the database; ignores nested/backup administrators and can return null for an absent oracle. |
| `updateAdmin` | Unauthenticated source write; truthiness chooses primary versus backup, updates the database before refreshing the Map. |
| `publishData` | Original six arguments, early watermark mutation, stale-block skip, Number-coercing circuit comparisons, four-place half-up limits, sentinel price 1, implicit unknown metadata, OHLC preserved. Returns undefined. |
| `getOraclePrice` | Includes every row with `data.price` coercible to a finite Number, regardless of record type; retains the original value and first latest-height tie. Returns Number 1 when absent. |
| `saveOracleData`, `loadOracleData` | Original upsert key, full payload, inclusive/default history bounds, insertion order, price-record type filter only for history. |
| `getTWAP` | Equal-weight mean, 20-place BigNumber division then eight-place half-up Number conversion. No record-type filter; a stake/relay/fraud row can cause caught failure and null. NaN arithmetic returns NaN. |
| `recordStake`, `getStake` | Metadata upsert, Number bridge for previous amount, eight-place truncation, negative amounts and property replacement. |
| `applyFraudProof` | Source min/Number arithmetic, negative slashes, UTF-16 evidence prefix and collisions. Stake update precedes proof write and remains if the proof write fails. |
| `relayTradeLayerState` | Genuine pinned bridge parser, normalized signature digest key, effective replay fields, original fields in returned relay row. Signature-use insertion precedes relay insertion. |
| `closeOracle` | Durable closure and cache refresh precede exact external `insurance.liquidate(adminAddress, true)` call. Closure remains when that call rejects. |

No operation adds activation, caller authentication, a funding bond, proof
verification, token transfers, or a restriction on publishing closed feeds.
Source metadata is not evidence of those effects.

## External dependencies and failure semantics

The declared database boundary is a clone-isolating NeDB-like adapter with typed
primitive IDs, ordered full rows, strict equality queries, inclusive relational
range queries, duplicate-insert rejection, replacement upserts and `$set`.
This owner implements that adapter's mutable state; it does not read NeDB files
or port NeDB indexes, filesystem recovery, generated IDs, or object identity.
Imported rows must be bounded objects with unique primitive `_id` values.
Queries, writes, collection acquisition, and one-shot failures are observable
through `take_db_calls()` and `fail_database_call()`.

Relay parsing reuses `dlc_oracle_bridge::source_parse_relay_blob`; SHA256 is the
repository's native implementation. Oracle.js invokes the parser, not relay
signature verification. Its replay bookkeeping must not be interpreted as proof
that a signature is valid.

`evaluate_with_backend` and `invoke_source_with_backend` forward the exact
`insurance.liquidate` call to a supplied `SourceBackend`. The pinned insurance
signature is `(adminAddress, contractId, isOracle = false)`, so oracle.js passes
`true` as **contractId**, leaving isOracle false. The owner does not repair that
argument bug or call a different backstop operation. Actual insurance behavior
and financial effects belong to the external owner. The source's insurance
method can itself swallow internal exceptions; oracle.js observes whatever its
real dependency returns or rejects with.

Plain `evaluate`/`invoke_source` have no insurance implementation. They return
`UnknownDependency` at that boundary and retain the already executed closure
prefix. A supplied backend's source throw/rejection becomes `Thrown`/
`PendingThrow`, preserving every earlier write. The fixture uses an explicit
return/reject callback to test that boundary; it does not claim to execute the
insurance fund. Local resource/domain errors restore the pre-call owner and
failure schedule instead of inventing JavaScript behavior.

All 19 operations are async in source and become `Pending` or `PendingThrow` in
`SourceBackend`. Targets accept `Oracle.<method>` or `oracle.<method>`; no
caller-supplied answer recorder provides database or oracle results.

## Checkpoints and resource limits

`encode()`/`decode()` and `checkpoint()`/`from_checkpoint()` encode complete
continuation state. `durable_checkpoint()`/`restore_durable()` encode just the
two document collections, recreating empty singleton Maps and failure schedule
for a process restart. These modes are deliberately distinct.

The canonical format is `TLOS` plus version byte 1, mode byte, ordered row
sections, and (in complete mode) ordered Map sections and queued injected
database failures. Counts, string lengths and failure skip counts are big-endian
u32; Number payloads are IEEE-754 u64 bits. SourceValue tags retain undefined,
null, booleans, strings, arrays, object field order, Promise and BigNumber values.
CRC32 covers the entire payload. Decoding rejects wrong versions/modes,
truncation, trailing data, corruption, duplicate IDs/Map keys, noncanonical
negative-zero Map keys, noncanonical NaNs and invalid re-encoding.

Each collection, Map, array and failure queue is bounded to 1,024 entries;
the complete encoding is bounded to 16 MiB. Each SourceValue additionally uses
the shared 8,192-node, depth-24 and 1-MiB text limits. Decimal operations use
the existing bounded arbitrary-precision decimal engine, BigNumber-compatible
NaN/infinity/negative-zero behavior, radix parsing and explicit rounding. A
BigNumber input longer than 512 bytes is outside the numeric domain.
Object/function/symbol Map-key identity, coercion hooks implemented as functions,
and lone UTF-16 surrogates are outside this UTF-8 value boundary. A UTF-16 slice
that would create a lone surrogate is rejected explicitly.
The decoder charges each value's aggregate node and text budget while parsing,
before allocating the next child or text payload; individually small subtrees
cannot defer that aggregate check until after a large allocation.

Drainable database/dependency traces are bounded to 16 MiB each and omit old
trace entries when needed. They are diagnostic and excluded from checkpoints
and owner equality; source documents, caches, watermarks and queued failures
are included in complete owner equality.

## Verification

`node scripts/oracle_source_owner_vectors.cjs --check` executes the unchanged,
hash-pinned full oracle.js and bridge modules, genuine archived BigNumber 9.3.1,
and installed tiny-secp256k1 (loaded by the bridge). It verifies 311 actions
covering every source operation. The fixture compares returned values,
exceptions, complete rows, singleton Maps, typed watermarks, exact database
calls, and external dependency calls. Cases include nested legacy fields,
type/whitelist retention, malformed numeric values, radix inputs, implicit feeds,
unsafe/fractional allocation, cache poisoning, restarts, circuit thresholds,
partial stake/fraud/relay writes, and failed publication/closure prefixes.

`tests/oracle_source_owner.rs` restores the full owner checkpoint before every
subsequent fixture action, including pending failure injection. Additional tests
compare complete continuation with process restart, retain closure prefixes,
reject byte corruption and truncation, reject correctly checksummed hostile
sections and aliases, and preserve state/fault schedules on local capacity
rejection. These checks establish the declared source/adapter boundary; external
authorization and insurance economics remain separate obligations.
