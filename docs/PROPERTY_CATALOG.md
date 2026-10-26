# Owned property.js catalog

`property_catalog::PropertyCatalog` owns the actual persisted `propertyIndex`
document and the singleton's insertion-ordered `Map` cache. It implements the
bounded source behavior of the complete archived `property.js` at JavaScript
commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA256
`ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1`.
The untouched source is `fixtures/reference_fidelity/property.js`.

This is real mutable catalog state, including ticker, circulation, class,
whitelist, issuer, backup address and procedural subtype. It is not another
spendable balance store. Source declarations can disagree with funded custody.
`grantTokens` and `redeemTokens` invoke the caller's actual
`SourceBackend` capability `Tally.updateBalance`; the enclosing economic owner
must provide its sole Ledger implementation and transaction admission.

## API and source behavior

```rust,ignore
let mut catalog = PropertyCatalog::from_document(initial_document)?;
let evaluation = catalog.execute("createToken", &arguments, &mut backend)?;
let property = catalog.get_property_data(SourceValue::number(1.0))?;
let bytes = catalog.checkpoint_bytes()?;
let restored = PropertyCatalog::from_checkpoint_bytes(&bytes)?;
```

`from_document` starts with an empty singleton cache, exactly as constructing the
source manager does. `document()` and `cache_entries()` expose immutable actual
state. `replace_document` journals an external database write and leaves cached
entries stale. Source reads operate on this owned document; no fabricated
external DB reply supplies property metadata. `get_property_data`,
`is_managed_and_admin`, and `does_ticker_exist` provide ordinary query helpers;
`execute` preserves source return values and thrown exceptions separately.
Returned Error instances use the explicit diagnostic fields `{name, message}`;
their JavaScript prototype and stack graph are outside the SourceValue domain.

All seven named classes map to their source codes: Fixed=1, Managed=2,
Native=3, Vesting=4, Synthetic=5, Non-Fungible=6 and Procedural=7. This metadata
mapping does not invent NFT, native entitlement, vesting, synthetic or
procedural custody semantics.

The eighteen class methods have explicit dispatch:

| Methods | Behavior |
| --- | --- |
| `constructor`, `getInstance` | Same owned singleton. `get_instance()` returns the same Rust reference; diagnostic calls set `singleton_return=true` and return Undefined because `SourceValue` has no Map reference graph. |
| `load` | Parses strings or accepts decoded pair arrays; preserves Map insertion order, replaces duplicate primitive keys without moving their slot, clears missing/invalid-shaped data, and retains stale cache on caught parse/read failure. |
| `getNextPropertyId` | Reloads, then performs numeric coercion and `Math.max(...)+1`, including NaN from synthetic string keys. |
| `createToken` | Checks stale cached duplicate tickers before reloading; can accept a persisted duplicate. A matching ticker value returns an Error object; a matching Map key throws the source TypeError. |
| `addProperty` | Performs the raw source upsert, including supply addition, truthy field fallback, class replacement, Number-converted procedural metadata, and source quantization. Existing object mutations survive a later source exception. |
| `save` | Serializes actual Map entries to the owned document, including omitted Undefined object fields and nonfinite numbers becoming JSON null. |
| `inspectPropertyIndex` | Reloads and returns Undefined. Console presentation is outside the recorded capability boundary. |
| `getPropertyIndex` | Loads only an empty cache and returns the source's four-field projection. Nonempty stale cache remains stale. |
| `getPropertyData` | Reads persisted JSON, first tries an exact s-prefixed key, then Number-equivalent IDs; caught source errors return null. |
| `isManagedAndAdmin` | Reads persisted JSON; uses strict ID, numeric class 2/7, and strict issuer equality. Caught source errors return false. |
| `doesTickerExist` | Reads persisted JSON and checks case-sensitive strict ticker equality. |
| `updateAdmin` | Uses numeric-coercing persisted lookup, writes issuer or backup, and leaves cached metadata stale. The helper itself has no actor check. |
| `grantTokens`, `redeemTokens` | Preserve the raw helper's class/admin permissiveness, supply rounding, numeric Map key insertion and original tally ID/amount/provenance. Redemption checks declared supply, then delegates custody to tally. |
| `getAMM` | Throws `ReferenceError: PropertyRegistry is not defined`. |
| `isSyntheticToken` | Loads the instance, then throws because the source reads an absent static `propertyIndex`. |
| `updateTotalInCirculation` | Computes the source update, performs the source reads, then preserves the matching-ID failure `base.updateAsync is not a function` from the unawaited async DB factory. Strict-ID mismatch retains its separate source Error. |

The actual `quantize8` helper is ported over the existing dependency-free
`Decimal` arithmetic, including binary64 input conversion, decimal addition or
subtraction, eight-place half-up rounding, NaN/Infinity and negative-zero
behavior. The `getTallyMap` helper's exported module reference is explicitly
`UnsupportedDomain` as a standalone value; real grant/redeem calls do invoke
the actual tally capability.

## Effects, partial failure and integration

`CatalogEvaluation` contains `SourceOutcome`, the singleton flag and a single
ordered `CatalogEffect` stream. It includes database open/read/write attempts,
their arguments and injected exceptions, plus actual `DependencyObservation`
for tally calls. Console logs are intentionally not represented as effects.

`execute_with_faults` accepts sorted unique `DatabaseFailure` positions in the
zero-based database-operation stream. The document is still owned state;
fault injection only makes the named capability attempt reject. This permits
replay of partial failure without treating an externally supplied row as
catalog authority.

Source order is preserved: grant/redeem mutate the singleton after loading,
then await tally, then save. A source tally rejection leaves changed cache and
old persisted supply. A source save rejection follows the tally observation
and also leaves the database stale. An actual economic owner may stage its
Ledger and catalog together when transaction policy requires atomic admission;
that is a distinct policy from these raw source helper effects.

Backend `Throw` and `PendingThrow` represent source exceptions. A backend
`Err(SourceRuleError)` represents a local unsupported/integration failure, not
a simulated JavaScript exception. The backend must return bounded shared
`SourceValue` observations and must use the exception variants for source
failures. Each encoded backend reply is limited to 4,096 bytes and the same
ordinary-value domain. A reply outside that contract is a local error; callers
must stage their external Ledger if they need rollback for integration errors.
Its successful return value is recorded even though property.js
ignores it. Metadata serialization limits are forecast without external
effects before a real tally call is launched.

## Bounded domain

Limits are explicit implementation bounds, not newly inferred source business
rules: eight arguments, 1,024 Map entries, 1,024 journal events, 32 injected
database failures, an 8 MiB checkpoint, the existing ordinary JSON-document
limits, and shared SourceValue node/depth/byte limits. A combined limit may be
reached before an individual count limit. Calls and document replacements
validate their canonical checkpoint before committing owned state.

The value domain contains primitives, ordinary arrays and plain objects without
custom coercion/JSON methods. Primitive Map keys use SameValueZero, including
NaN identity and normalization of negative zero. Composite keys loaded from
JSON remain distinct even when structurally equal. Cross-call object-reference
aliases, proxies, functions, mutated JavaScript prototypes, Promise/BigNumber
objects supplied as external values, inherited function-valued class-map
entries, and special properties attached to arrays are explicitly unsupported.
Object fields pretending to be a BigNumber object are rejected. Indexed
malformed string rows containing non-BMP characters are unsupported because
individual UTF-16 surrogate values have no Rust String representation.

Numeric constructor strings longer than 512 bytes, prefixed BigNumber forms,
and arithmetic beyond the existing Decimal bounds are `UnsupportedDomain`.
Number coercion does support bounded hexadecimal/binary/octal strings.
Uncaught malformed-JSON SyntaxErrors are `UnsupportedDomain` because their
exact messages depend on the JS engine; source methods that swallow parse
errors still preserve their exact null/false/stale-cache outcomes. Unpaired
surrogates and JSON number overflow outside the existing JSON value domain
also reject explicitly. None of these omissions is silently repaired.
Injected rejection of the deliberately unawaited factory inside
`updateTotalInCirculation` is unsupported because its extra unhandled Promise
rejection has process-level scheduling behavior.

## Checkpoints and complete history

The versioned `TLPC` checkpoint contains the initial document, every method
call and explicit external document replacement, every backend reply and
source failure, and the expected final persisted document and Map cache.
Restore reruns the complete journal, verifies every ordered effect and outcome,
compares final cache/document state and requires byte-for-byte canonical
re-encoding. It does not accept an independently edited final snapshot merely
because its CRC was recomputed. The CRC detects accidental corruption; caller
expectations and chain admission remain necessary to authenticate history.

## Reproduction and scope of evidence

```text
node scripts/property_catalog_vectors.cjs --check
cargo test --test property_catalog
cargo clippy --lib --test property_catalog -- -D warnings
```

The deterministic generator executes the complete exact source bytes unchanged
in a fresh CommonJS VM per case. It reuses `legacy_numeric_oracle.cjs` to load
the genuine complete archived BigNumber **9.3.1**, LF-normalized SHA256
`58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16`.
The source lockfile specifies **9.1.2**; 9.1.2 is not executed and equivalence
across those dependency versions is not claimed.

The 103-case corpus compares values, exception name/message, ordered DB/tally
arguments and rejection observations, actual cache entries and persisted JSON.
It covers all seven kinds and all class methods. One getTallyMap case executes
the real helper to characterize its function-valued export, and verifies Rust's
explicit unsupported-reference result; this is not an equivalence success for
that reference graph. Rust also verifies replay equality, bounded history,
unsupported/capacity rejection, corruption and forged-history rejection.
The DB and tally boundaries in the JavaScript oracle are deterministic isolated
capabilities. Those fixtures do not prove NeDB behavior, signatures, chain
admission or real funded tally accounting. The enclosing integration must test
its real sole-Ledger backend separately.
