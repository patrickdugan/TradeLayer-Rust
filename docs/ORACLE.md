# Stateful oracle metadata and prices

`src/oracle.rs` implements the finite, typed state transitions of
`tradelayer.js/src/oracle.js` at
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Its complete unmodified source is
archived as `fixtures/legacy_oracle_source.txt`, LF-normalized SHA-256
`70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d`.
The source bridge parser is separately archived as
`fixtures/legacy_oracle_bridge_source.txt`, SHA-256
`954a7fbf30b66e52b587a2d7108e080c41da812acff77ccd457c6e547cb1587f`.

The store changes actual local oracle, price-history, stake-metadata,
fraud-record and relay-replay state. It owns no token balances. Logical
administrator checks compare supplied opaque `AccountId`s, which do not prove
sender authentication. Chain authorization, activation, clearlist policy,
funded staking, valid fraud evidence and DLC signature verification remain
separate integration requirements.

## Method coverage

| Source methods | Rust stateful behavior |
| --- | --- |
| Constructor / `getInstance` | Independent cloneable `OracleStore`, rather than one global process singleton. |
| `getAllOracles`, `load` | `get_all`, `load_metadata` refresh cached metadata from the store's separate persisted-metadata projection. |
| `addOracle`, `getOracleInfo` | Bounded typed `add` and `get`; preserve insertion order and distinguish cached latest publication from persisted metadata. Duplicate-add corruption is repaired explicitly. |
| `createOracle`, `getNextId` | Metadata defaults, ticker/name fallback, configurable lag, clearlist IDs, maximum-ID allocation including gaps and exhaustion. |
| `isAdmin`, `verifyAdmin` | Primary or backup for `is_admin`; primary only for `verify_admin`. Missing oracles return false. |
| `updateAdmin` | Logical admin guard, primary/backup replacement, and source metadata-cache refresh. |
| `getOraclePrice` | Latest finite typed price by greatest block; default 1; stake/fraud/relay rows are excluded. |
| `publishData` | Logical-admin `publish` or explicitly unauthenticated `publish_metadata`; process-local per-oracle watermark, 5% circuit breaker rounded to four places, unchanged high/low/close fields, cached metadata and upserted history. |
| `saveOracleData`, `loadOracleData` | `save_price_record` bypasses watermark/metadata as source does; inclusive unsorted `history`, price rows only, replacement at the same oracle/block key. |
| `getTWAP` | Inclusive trailing-block query; equal row weighting, BigNumber division at 20 places then eight-place rounding and Number conversion. Source mixed-record contamination is preserved. |
| `recordStake`, `getStake` | Stateful per-oracle/staker metadata; signed additive amounts, eight-place truncation, Number bridge, property-ID replacement, and previous/current receipt. |
| `applyFraudProof` | Stateful alleged slash and fraud-record upsert; missing-stake zero result, cap to recorded stake, negative slash behavior, eight-place truncation and evidence-prefix collisions. No evidence verification or payout is invented. |
| `relayTradeLayerState` | Stateful normalized signature-use keys and relay rows, effective state fallback, per-oracle replay protection and overwrite rules. Parsed bridge fields are explicit caller input; raw blob is preserved. |
| `closeOracle` | Logical-admin metadata closure followed by an explicit `InsuranceLiquidationRequest`. The caller must execute and fund that request. |

## Preserved source behavior

Publication watermarks skip the same or an older block. The source watermark
is static process memory rather than persisted state; the explicit
`reset_publication_watermarks` operation models a source restart. Stored price
rows still determine the circuit-breaker baseline, and a restart may replace a
row at an already published block. It is not a chain reorganization policy.

A previous price numerically equal to 1 disables the circuit breaker. This
uses JavaScript Number equality, including a decimal string such as
`1.00000000000000000001` which Number rounds to 1. Circuit bounds use exact
decimal multiplication by 1.05/0.95 followed by four-place HALF_UP rounding;
there is no intermediate division rounding. Comparisons cross the source's
Number bridge. A price that is not clamped retains its exact finite decimal
value. Zero and negative prices are permitted by the source storage helper;
negative prices expose its inverted-bound if/else order. High, low and close
are never clipped. Closed metadata does not prevent source publication.
An authenticated live adapter must separately enforce valid price and
closed-oracle policy before admitting these transitions.

Publication writes latest fields only to the metadata cache, not the oracle
metadata database. Calling source `load`/`getAllOracles`, `updateAdmin` or
`closeOracle` refreshes the cache from database metadata and can remove those
latest fields. Price history and the independent watermark survive that
refresh. The Rust store models both projections rather than inferring latest
price from the metadata cache.

`getTWAP` is an arithmetic mean of price **rows**, not elapsed-time weighting.
Its source database query omits `type: 'oracle'`. Any in-range stake, fraud or
relay row lacks `data.price` and makes the source catch an exception and
return null. Rust returns `None` for that same contamination. Signature-use
rows have no `blockHeight` and do not contaminate the query. `history` filters
price rows and remains usable. Neither query sorts its result.

Stake metadata is keyed by oracle and address, not property. Repeated writes
can replace the property ID and accept negative deltas. Every write truncates
toward zero at eight decimal places and converts to Number; large values can
lose integer precision. Fraud metadata takes `min(current, Number(slash))`
without proving evidence valid. A negative alleged slash increases recorded
stake; a negative recorded stake can produce a negative slash. The evidence
key retains only the first 24 ASCII characters, so distinct evidence strings
with that prefix and the same block overwrite the same fraud row. These are
observable source semantics, not authorization to mint or transfer assets.

Relay replay bookkeeping trims and lowercases the supplied ASCII signature,
then hashes that **text** with SHA-256. It does not hash decoded signature
bytes or validate hexadecimal encoding. Reusing a signature for identical
effective state and DLC reference is allowed; changing either fails without
updating local state. The same signature in another oracle has a separate
namespace. The replay record may use a state hash parsed from the blob while
the returned relay row preserves the empty explicit state hash. Relay rows
with the same oracle/type/block replace each other. `oracle.js` itself does
not call bridge signature verification.

## Deliberate boundaries and repairs

The typed store accepts integer nonzero IDs up to `9007199254740991`, unsigned
blocks up to that limit, printable bounded ASCII `AccountId`s, finite shared
`Decimal`s, typed metadata and optional finite OHLC fields. There are at most
4096 oracle records, 65536 combined history/stake/fraud/relay/signature-use
records, 4096 bytes per metadata/replay text field and 65536 raw blob bytes.
Evidence and signatures are restricted to ASCII so source UTF-16 prefixing
and case conversion have exact typed meanings. Other strings may be UTF-8.

Explicit `Number`/`toNumber` boundaries round through finite IEEE-754 numbers
and their shortest decimal representation. Nonfinite results are errors;
NaN, Infinity, arbitrary JS object coercion, malformed numbers, fractional
IDs, undefined administrators, nested-object administrator fallback and
oversized values are outside the typed domain. Cross-platform identity of
all shortest-float strings has not been proven beyond the executed vectors.

Fallible local operations leave state unchanged. This repairs source
`publishData` advancing its watermark and cache before a failed database
write, which poisons a same-block retry. Unknown publications are rejected
instead of creating incomplete implicit metadata. Duplicate `addOracle`
inserts are rejected before changing either metadata projection, repairing
the source's cache-before-failed-insert behavior. All 17 source failure and
unsafe-domain characterizations are retained in `fixtures/oracle_quirks.tsv`;
they are not described as successful Rust compatibility cases.

`close` changes metadata and returns a liquidation request, matching the
source ordering before its insurance call. It does not claim that insurance
paid or automatically roll back metadata if that separate request fails.
Repeated closures can generate repeated requests as in the source, and an
integration must add settlement idempotency. Fraud/slash receipts likewise
do not own escrowed assets or credit the challenger. Relay callers must
provide parser output from their bridge adapter; this module does not
implement a JSON/base64 parser or verify DLC signatures. The vector runner
does execute the original bridge parser, including alias/base64 cases.

Storage is an in-memory model of the metadata and data collections. No NeDB
durability, restart codec, asynchronous database errors or production migration
is claimed. Successful source `load` behavior is modeled; database error
swallowing is characterized separately where relevant. Historical replay,
funded ledger integration, authenticated sender/clearlist admission, proof
verification and full contract/insurance wiring remain open roadmap work.
The source exports no `getOracleData` method, despite callers in
`contractRegistry.js` referring to it; this missing integration is not filled
by silently aliasing a differently shaped query.

## Verification

`fixtures/oracle_vectors.tsv` contains 246 actual source outputs across seven
independent stores. Cases cover metadata/defaults/admin rotation/closure,
publication skip and restart behavior, exact sentinel/circuit boundaries,
cached metadata refresh, inclusive unsorted history, two-stage TWAP rounding,
stake Number precision, signed slash quirks, mixed-row contamination and
relay replay/fallback/overwrite behavior. Sixty-four separately seeded circuit
baselines probe exact rounded boundaries and sub-Number-precision offsets.
`fixtures/oracle_quirks.tsv` adds 17
source-only failure and unsafe-domain observations.

The runner executes unmodified SHA-pinned oracle and bridge source in fresh
Node VMs, with the archived BigNumber 9.3.1 reference. Database mocks perform
NeDB-shaped query/upsert operations; insurance records calls and can fail.
The bridge's unused secp verifier throws if invoked, so the harness cannot
silently fabricate a successful signature check.

```text
node scripts/oracle_vectors.cjs
node scripts/oracle_vectors.cjs --source-root /path/to/tradelayer.js
cargo test --test oracle --frozen
```

Seven Rust tests replay every lifecycle output and test logical authorization,
failed-operation atomicity, metadata-cache reload, numeric/resource limits,
duplicate import rejection, evidence-prefix collisions and the source SHA-256
signature-key construction. They prove these local finite-domain behaviors,
not historical acceptance or funded production authorization.
