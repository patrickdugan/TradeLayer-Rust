# Oracle publication in the literal contract runtime

`source_contract_oracle_impl.rs` composes the actual `Oracle.publishData`,
`Oracle.getOracleInfo`, and `Oracle.load` lifecycle with the contract runtime's
shared `oracleList` and `oracleData` collections. The source is the complete
unchanged `fixtures/legacy_oracle_source.txt` from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA-256
`70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d`.

There is one authoritative collection store. This include does not import a
detached `OracleSourceOwner`, replay recorded database answers, or own balances.
Its singleton cache and typed publication watermarks live directly in
`ContractCore`; the runtime exposes both through `state_value().sourceOracle`.
The independently verified Oracle owner's small numeric helpers supply the
same JavaScript coercions and BigNumber circuit calculation without owning
another copy of the rows.

The source sets its per-ID watermark before singleton initialization or any
database access. A rejected read or write therefore leaves the watermark, and
a retry at the same or older height skips. A rejected publication write also
leaves the already refreshed cache. The source preserves the complete existing
metadata, changes only `data` and `lastPublishedBlock` in that cache, and writes
the complete publication row to `oracleData`. It does not persist the refreshed
metadata to `oracleList`. The five-percent circuit changes only `data.price`;
high, low, and close preserve their original values and types.

The singleton and cache have distinct source behavior. Price reads do not
initialize the singleton. `getOracleInfo` initializes before an optional load;
`load` initializes only after its successful collection read and catches source
database exceptions. Loading merges persisted documents without clearing stale
cache entries. Imported metadata can consequently remain stale in the cache
until a load overwrites its key. Publication without metadata makes the source
fallback cache document, without creating an `oracleList` row.

Watermarks use primitive JavaScript Map keys with SameValueZero equality:
numeric and string IDs differ, NaN keys compare equal, and negative zero becomes
positive zero. The height comparison retains JavaScript relational coercion,
including lexical ordering when both operands are strings. Arbitrary object
identity keys and custom coercion methods are outside the declared bounded
SourceValue boundary and produce local errors instead of fabricated source
results. Local capacity/domain errors roll back the runtime; source exceptions
retain the actual prefix of cache and database effects.

`tests/source_contract_oracle.rs` evaluates 140 actions against the unchanged entire module in Node
with the archived genuine BigNumber constructor and an instrumented shared
collection adapter. After every action it compares the actual returned/thrown
result, complete persisted rows, singleton cache, typed watermarks, and ordered
database calls including exact argument arity and failure outcomes. The corpus
covers cache preservation and refresh, missing metadata, every publication DB
failure boundary, skipped retries, singleton initialization failures, typed IDs,
NaN/zero/undefined keys or heights, lexical latest-row selection, hexadecimal
and octal prices, arrays, booleans, null, infinity, signed zero, and circuit
rounding. Continuation uses complete runtime clones; this focused test does not
claim durable checkpoint validation or authentication of imported source state.

The public dependency entry points are `Oracle.publishData` with its original
six arguments, `Oracle.getOracleInfo` with its original ID, `Oracle.load`, and
the existing actual `Oracle.getOraclePrice` read. No caller-supplied oracle
verdict or supplied publication result is accepted.
