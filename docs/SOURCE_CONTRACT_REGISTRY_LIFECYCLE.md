# Literal contract definition lifecycle

`source_contract_registry_lifecycle_impl.rs` ports the reached definition methods
from `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete
ContractRegistry archive is `fixtures/legacy_contract_registry_source.txt`, LF
SHA256 `d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121`.
The include operates on the existing literal runtime's collections and caches.
It does not add a registry or spendable ledger beside those owners.

## Creation and actual persistence

The include implements `createContractSeries`, `getNextIdFromMap`,
`generateContracts`, `saveDataToDb`, `isDuplicateNativeContract`,
`getContractSeries`, the effective later `getNextId`,
`getAllContractsForCollateral`, `lookupInverseNativeByNotionalPid`,
`getContractType`, `getAMM`, `isValidSeriesId`, and `saveAllData`. The Logic bridge
constructs the exact source parameter object before entering creation. Its direct
result is the series ID; the raw type 16 switch discards that result.

Creation reads `contractList`, constructs a local Map, calculates its ID, creates
the actual AMM fields, defaults whitelist, obtains its ticker and generates the
children. It then saves every Map entry in order, including old definitions.
Each update queries only `{id}`, so a row of another type can be replaced. A
failed later update retains earlier writes and DB counters. Source exceptions
retain their prefixes; unsupported-domain/capacity errors follow the runtime's
separate local rollback policy.

The local creation Map is temporary source execution state. Creation does not
update the singleton's constructor Maps or its separately loaded
`contractSeries` Map. The existing `contract_series` cache remains the sole
cached series owner. Two V3 flags distinguish singleton construction and an
initialized series Map from undefined fields. A loaded empty Map remains truthy
and stale until an actual load refreshes it. The constructor's `contractList`,
`oracleList` and `nativeList` stay empty in this graph; `isValidSeriesId` and
`saveAllData` observe those actual empty Maps.

The two ID methods are intentionally different. Creation's `getNextIdFromMap`
ignores a parsed NaN because it never exceeds the maximum. The effective later
`getNextId` reads the DB and uses `Math.max`, so a malformed ID poisons its result
with NaN. The earlier same-named singleton method is shadowed in JavaScript.

## Source defects and dated definitions

Raw native type 16 retains the decoder's inverted pair-parsing condition: native
payloads produce an empty array and validity rejects them. Direct native Logic
creation remains available where the actual source permits it. Native duplicate
checking preserves the array-of-pairs mismatch, strict pair comparisons,
forward/reverse notional precedence, caught load failures and stale cache.

Oracle creation reads `oracleInfo.name.ticker`. Ordinary flat source oracle
metadata therefore throws before any contract write. A genuine imported nested
legacy document can succeed. Neither route manufactures the missing nested name.

Expiry generation preserves zero and negative periods/counts, repeated child
IDs, fractional direct-call loop counts, parseInt prefixes and Number rounding.
Registry lookup resolves a child string such as `2-34` to numeric series 2;
`getContractSeries` instead uses its argument directly. No dated rollover or
physical settlement is invented. Dated created tickers still contain `PERP` or
`OPERP`, so the source perpetual filter includes them.

`getAMM` returns the enumerable fields of the source's hydrated AMM object. This
definition slice does not claim the complete AMM liquidity/custody graph.

## Actual metadata and reference reads

`Property.getPropertyData` reads and parses the real property document on every
call, independent of the Property index cache. Exact synthetic string lookup
precedes Number-key matching. Missing rows, malformed JSON and source storage
exceptions return null under the source catch.

`Volume.getLastPrice` and the corresponding VolumeIndex route await the real DB
factory before validating the pair. They return a truthy direct price unchanged,
otherwise try the reversed pair and return the BigNumber reciprocal as a Number.
The source ignores its height argument. Missing/invalid pairs and caught source
errors return null. Creation does not populate `indexPair` or `firstMark`; these
reads do not repair native reference validation by inventing either field.

## Replay policy and bounded arguments

The new surface belongs to `SourceLifecycleV3`. V1/V2 dispatch and diagnostic
bytes retain their existing admission results. V3 diagnostics and replay bind
the singleton/cache flags as well as collections, actual cache entries and
continuation counters. The default runtime/replay constructor remains V2; this
new surface requires explicit V3 selection.

For direct Map-taking diagnostic commands, an array of `[key,value]` pairs is the
explicit Map argument protocol. Internal creation obtains all entries from the
actual collection. Primitive Map keys and source SameValueZero behavior are
supported; arbitrary object-identity keys are outside this protocol. Source
integer parsing is bounded to 128 integer bits, generated arrays and Maps to the
runtime's 1,024-entry bound. Larger unsupported inputs fail locally instead of
receiving fabricated source results.

## Evidence

`tests/source_contract_registry_lifecycle.rs` executes the complete unchanged
CommonJS graph for 166 actions, including 140 calls, and compares results,
exceptions, all persisted collections,
actual registry/Oracle cache state, raw decoded parameters and ordered storage,
activation, UUID, random and clock effects. The corpus covers four module entry
orders, native/oracle and dated/perpetual definitions, multiple series, strict
versus parsed IDs, hydrated AMMs, stale duplicate caches, old-row-first failures,
actual retries and real Property/Volume read failures. A second test restores
retained creation/save exception prefixes through actual journal replay and
continues their retries.

The same corpus independently executes with archived BigNumber 9.3.1 and genuine
9.1.2. For 9.1.2 the test decompresses the complete dependency already archived
by `scripts/scaling_vectors.cjs` and checks raw source SHA256
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e` before using
its constructor. Only the test dependency loader changes; protocol modules are
unchanged. Equal observations establish equality for these tested executions,
not whole-version equivalence or a change to the older fixture provenance.

`tests/source_contract_lifecycle_versions.rs` adds three version checks. The
original compiled V2 six-command fixture restores and re-encodes byte for byte;
new registry/read/Logic capabilities remain rejected under V2. A corrected CRC,
including one paired with a substituted V3 bootstrap commitment, cannot promote
the old journal. An explicitly selected V3 registry journal records actual
creation, queries, native invalidity and flat-oracle exceptions without local
dependency errors, and restores after every command. The unchanged V1 compiled
fixture regression also continues to pass. Fixture provenance and its independent
head are recorded in `fixtures/source_contract_replay_v2.txt`.
