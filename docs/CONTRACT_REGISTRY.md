# Contract catalog and collateral calculations

`src/contract_registry.rs` implements a caller-driven contract-series catalog,
metadata queries, notional and initial-margin calculations, funding-event
journal, and ordered collateral transfer plans. It is separate from the earlier
transaction registry in `src/registry.rs`. Creation does not authenticate an
issuer, whitelist, property, oracle or chain event. A plan does not move the sole
ledger, AMM capital, channel balances or margin-map positions. Those stores must
be committed together by a future authenticated execution path.

The reference is the complete unmodified `src/contractRegistry.js` at
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, archived as
`fixtures/legacy_contract_registry_source.txt`. Its LF SHA256 is
`d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121`.
The offline Node VM executes that exact file with in-memory database and
dependency providers, plus the archived actual BigNumber 9.3.1 implementation.
The mocks supply data and record effects; registry equations remain in the
unmodified source. No npm installation or live chain is required.

## Concrete behavior

Series creation allocates max numeric id plus one, builds native property-pair
and oracle tickers, defaults the whitelist to zero, generates expiry records,
and retains the source AMM constructor configuration `(0, 1, 10, seriesId)`.
Property zero has an explicit `ContractAsset::BaseChain` spelling, preserving the
nonzero invariant of the ledger's `PropertyId`. Native base-chain notional uses
the source's hard-coded `LTC` ticker. This is a source compatibility detail;
the caller must supply the correct network context before integrating it.
Oracle null takes the source `null != 0` creation branch and requires an
explicit oracle ticker provider. Null and numeric zero have different behavior.

Queries retain insertion order. The perpetual filter uses `expiryPeriod == 0`
or the case-sensitive `PERP` ticker substring, including the source-created
perpetual-looking tickers for finite expiries. Zero expiry deliberately creates
duplicate generated identifiers when the requested series count is greater
than one. The source has no expiry rollover transition to port here.
`getContractInfo` accepts the decimal series prefix in `1-123` and `1.5`.
The inverse-native lookup filters **collateral** property, despite its notional
method name. Duplicate detection retains the source's operator precedence:
an exact forward pair ignores notional property, while the reversed pair
requires it. These quirks are exercised against actual source output.

Notional calculation preserves all four native/oracle and inverse/linear
branches, invalid or absent mark fallback, and missing-contract zeros. Inverse
valuation divides `1 / mark` at 20 decimal places before multiplying notional.
Initial margin instead divides `notional / price / leverage` sequentially at
20 places. Inverse margin uses ceiling at eight places, including negative
values; linear margin uses eight-place half-up rounding. Numeric leverage zero
defaults to ten. The source returns binary64 numbers, so these outputs pass
through a finite binary64 bridge after decimal rounding. This is intentional,
including precision loss above 2^53. The original `notionalPerContract` metadata
remains exact as a Decimal except for a native-inverse contract whose mark
remains invalid after fallback: that source early return converts the metadata
to Number as well.

Reserve plans multiply the source-rounded per-contract margin by the absolute
binary64 amount. Insufficient available balance returns `None`, matching the
source's null result. Margin plans retain ordered reserve refund/top-up,
buyer/seller fee deduction, virtual `amm`/`amm:` sender handling, reserve then
available then existing-margin priorities, and channel funding/shortfall
effects. `initial_margin` is already a total. Existing margin is reused without
being credited again. Plans expose the ordered tally deltas, channel debits,
final local balances and the value passed to margin-map `setInitialMargin`.
The virtual-AMM shortcut happens after source reserve reconciliation and fee
deduction, so those earlier effects are still observable when applicable.

Funding helpers intentionally use left-to-right binary64 addition and
multiplication. `calculateAveragePrice` is not an exact decimal average.
The event journal retains insertion order, permits duplicate heights, and does
not create settlement transfers. The source's “latest oracle” helper returns
the first matching provider record rather than the maximum height.

## Source methods and boundaries

The source contains 40 distinct non-constructor method names; `getNextId` is
declared twice and the database-based second declaration wins. The table covers
every distinct method rather than counting type-safe helpers as extra parity.

| Source method(s) | Rust behavior and remaining boundary |
| --- | --- |
| `getInstance`, `setModFlag` | Explicit owned `ContractRegistry`, `set_mod_flag`; no global singleton |
| `createContractSeries` | `create_contract_series` with explicit provider tickers; typed catalog mutation |
| `generateContracts` | `generate_contracts`; integer heights and bounded count |
| `getNextId`, `getNextIdFromMap` | `next_id`, `next_id_from_map_keys`; bounded typed numeric ids |
| `loadContractSeries`, `getContractSeries` | `insert_loaded`, `get_contract_series`; explicit restoration and lookups |
| `getContractInfo` | `get_contract_info` with source decimal-prefix lookup |
| `getAllContracts`, `getAllPerpContracts`, `getAllContractsForCollateral` | `all_contracts`, `all_perpetual_contracts`, `contracts_for_collateral` |
| `lookupInverseNativeByNotionalPid`, `isDuplicateNativeContract` | Source filtering, ordering and pair-precedence behavior retained |
| `getContractType`, `isNativeContract`, `isOracleContract`, `isInverse` | Typed flags; absent `isOracleContract` is `false` instead of JS `null` |
| `getCollateralId`, `getOracleId` | Typed getters; missing series returns an error instead of dereferencing null |
| `getNotionalValue`, `getInitialMargin` | Source-ordered arithmetic and finite number bridge |
| `getCollateralValue` | Explicit property/oracle scalar providers; real oracle module lacks the called `getOracleData` |
| `moveCollateralToReserve`, `moveCollateralToMargin` | Ordered atomic local plans; real cross-store authenticated commit remains open |
| `getAMM` | `amm_configuration` exposes creation fields and `instantiate_amm_pool` instantiates the typed AMM; mutable capital/orders require an explicit AMM snapshot |
| `updateAMM` | Actual static method fails on undefined static `contractList`; no invented successful invocation |
| `isValidSeriesId` | Typed catalog membership repair; actual source reads a different, normally empty cache |
| `hasOpenPositions` | Actual method fails on missing `MarginMap`; `has_positive_open_position` exposes only its positive-position predicate |
| `getPriceAtBlock` | `first_price_pair` exposes native provider routing; source oracle path is broken for ordinary records |
| `getLatestOracleData` | `first_oracle_record` retains first-record provider ordering |
| `calculateFundingRate` | Native/missing series zero; oracle series explicitly reports broken source path |
| `calculateAveragePrice`, `calculateFundingAmount` | Numeric binary64 helper equations |
| `saveFundingEvent`, `loadFundingEvents` | Bounded typed local journal with insertion-order queries |
| `applyFundingRateToSystem` | Source immediately references undefined `ContractsRegistry`; settlement remains unimplemented |
| `saveDataToDb`, `saveAllData`, `loadContractsFromDB` | No NeDB compatibility adapter or durable checkpoint implemented; explicit typed restoration only |

## Deliberate repairs and uncovered domains

Catalog caches are unified in the typed store. Source creation saves documents
but never populates `instance.contractList`, so `isValidSeriesId` can be false
immediately after successful creation. The static `updateAMM` and
`loadContractsFromDB` refer to static maps although the constructor initializes
instance maps. `loadContractsFromDB` also returns before its asynchronous work,
classifies type `oracle` while saving `oracleContracts`, and destructures a
`seriesId` field while saving `id`. `saveAllData` can save the separate empty
instance maps. Rust does not reproduce these cache/storage failures as a
successful catalog lifecycle.

The source oracle price-at-block query reads the contract-list database, then
indexes a normal record as though it were an array. Empty results return null;
ordinary nonempty records throw. Oracle funding imports missing `./Oracles`
and, even if that import were supplied, uses undefined `avgTradePrice`.
`applyFundingRateToSystem` also has undefined registry, margin, tally and synth
bindings. No funded settlement is inferred from these placeholders.

Creation requires typed boolean flags, finite decimal metadata, nonzero u32
series ids, integer safe heights, bounded ticker text and bounded vectors.
Source arbitrary JS objects, prototype keys, null/undefined distinctions beyond
the explicit oracle-null case, string concatenation, signed/fractional expiry
parameters, NaN/Infinity metadata and arbitrary precision ids are outside this
typed domain. Zero leverage represents the source numeric zero; the distinct
truthy string `"0"` is not representable by a bare Decimal field. Nonfinite
arithmetic results return an error. The binary64 shortest-string bridge has
source vectors on the checked host; universal cross-runtime float formatting
equivalence is not asserted.

Every collateral bucket must stay nonnegative. Source refund/top-up code may
emit a refund greater than the provided reserve before later funding checks.
Real tally validation then determines whether those calls fail. Typed plans
reject negative buckets, negative fees-adjusted margin, missing channel
balances and insufficiency, without publishing any partial mutation. This is
an atomicity repair, not a claim of identical source failure side effects.
The caller-supplied channel shortfall is derived from an actual nonnegative
balance, excluding the source's ambiguous comparison with the literal string
`"undefined"`. No address signature or channel membership is validated here.

Local bounds are 65,536 series, 4,096 expiry records per series, 64 data pairs,
256 ticker bytes, 65,536 funding events, and JS-safe integer heights. These are
resource caps, not historical consensus limits. Historical transaction replay,
durable storage, funding application, authenticated property/oracle creation,
AMM hydration, channel execution and sole-ledger integration are still needed
before this module can satisfy a complete production port.

## Verification

```text
node scripts/contract_registry_vectors.cjs
node scripts/contract_registry_vectors.cjs --source PATH/contractRegistry.js
cargo test --test contract_registry --frozen
```

The fixture holds 219 actual-source cases: 208 typed differential comparisons
and 11 separately asserted source/dependency observations. Rust tests also
exercise restoration order, local bounds, rollback, source number precision,
inverse ceiling, funding-event order, missing-series behavior, channel errors
and negative collateral rejection. These are synthetic source vectors, not a
historical replay corpus or chain-authorization proof.
