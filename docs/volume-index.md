# Volume index observations

`src/volume_index.rs` retains the original exact eight-decimal append-only
reporting projection. Its included `volume_index_source_impl.rs` adds finite
source arithmetic, complete source-method characterization and a separately
named mutable metadata owner. Neither layer changes balances, reward issuance,
consensus, transaction admission or clearing. Source defaults and caller
metadata are never authenticated price evidence.

The reference is `tradelayer.js` commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, `src/volumeIndex.js`, SHA-256
`d4d4329254125ce8e4217d22b0ba57a1581c6279df999af60281b0365dffb08c`
over the LF-normalized Git source bytes.
The existing JS attenuation test checks only declining reward shares and a
reward below the fee; it does not establish exact historical reward equivalence.

## Implemented surface

| Rust API | Coverage and domain |
| --- | --- |
| `calculate_vwap` | Exact numerator/denominator for nonnegative, representable eight-decimal volume and price inputs. Models the arithmetic in `calculateVWAP`; callers swap amount1/amount2 for its contract mode. Empty/zero-weight input returns `None`. |
| `WeightedPrice::price_atoms_half_up` | Checked eight-decimal ROUND_HALF_UP, matching `getVWAP`'s BigNumber rounding for this input domain. |
| `fee_linked_reward_atoms` | Checked rational fee-share multiplication with ROUND_DOWN. Covers the constant-share subset of `calculateAttenuatedLiquidityReward`; it does not compute the logarithmic share. |
| `VolumeHistory::insert`, `insert_batch` | Unique event observations with bounded identifiers and configurable capacity up to 65,536. Batch insertion validates the entire batch before mutation. Duplicate event IDs fail closed. |
| `sample_range` | Inclusive block window sorted by height and event ID. Out-of-order insertion is supported. |
| `totals_through` | Inclusive as-of totals over retained observations: all volume, type UTXO, and type contract. Uses checked `u128` totals. |
| `vwap` | Exact directional pair VWAP over an inclusive block window, using `raw_volume_atoms` as an explicit weight. |
| `source_vwap` | Source-characterization variant that includes either pair order without reciprocating reverse-pair prices, as the JS `getVWAP` implementation does. |
| `source_block_volumes` | Characterizes JS `getBlockVolumes`: counts the LTC subtotal when the full source `_id` contains character `0`, including IDs such as `10-21`. This is distinct from the type UTXO subtotal. Empty input returns a typed zero total; JS returns scalar `0`. |
| `rollback_after` | Removes observations strictly above a retained height; allows a replacement suffix to reuse removed event IDs. This is a local reporting operation, not source reorganization or chain authentication parity. |

Amount and price fields are unsigned eight-decimal atoms. Negative values,
non-finite JS Numbers, source coercions, values with greater decimal precision,
and malformed source object shapes are outside the accepted domain. Arithmetic
overflow produces an error. Exact rational prices remain available before
rounding; `calculateVWAP` itself returns an unrounded JS Number.

`VolumeRecord.event_id` provides unique observation identity while `source_id`
retains the original source document ID. Callers must prevent multiple event IDs
for the same real trade and must supply correctly oriented pair prices and
volume weights. The projection does not verify valuation or provenance.

## Deliberate source boundaries

The source is not a coherent append-only historical index:

- `saveVolumeDataById` overwrites `_id`, then increments global cumulative volume
  on every invocation. Repeated saves can count volume repeatedly despite a
  single surviving observation. Rust rejects duplicate event IDs instead.
- The writer stores `value.rawVolume`; `getVWAP` reads `value.volume`.
  `sampleVolumesByBlock`, `sampleVolumesByBlockRange`, and
  `calculateCumulativeVolume` read top-level fields that the writer does not set.
  Rust requires explicit typed inputs and does not invent missing source fields.
- `getCumulativeVolumes` queries `< block`, and the updater overwrites its sole
  global/LTC cumulative documents. It cannot reconstruct a complete historical
  prefix. The Rust `totals_through` API explicitly uses an inclusive prefix of
  retained event observations. The source updater never writes a contract total;
  Rust's contract subtotal is an explicit additional reporting projection.
- `getLastPrice` accepts a height but does not apply it to its DB lookup.
  `SourceVolumeStore::get_last_price` explicitly preserves that behavior;
  `retained_price_as_of` is a separate repair that refuses future rows.
- `computeLiquidityRewardShare` uses `Math.log10` and floating-point policy
  coercions. The new finite source helpers are reporting calculations. The
  unchanged exact fee helper still takes a supplied rational share. Neither
  helper issues tokens or grants authority to spend fees.

Production database adapters, authenticated valuation and reward issuance
remain unavailable. The complete source surface and named repairs follow.

## Finite source surface

`SOURCE_METHODS` lists all 27 static methods; the constructor is also executed
by the unchanged-source oracle. `SourceContractFields` represents the selected
flat or `raw.data` object after a caller's dependency adapter; normalization
does not turn it into an accepted economic contract definition.

| Pinned method | Functional Rust projection and actual source boundary |
| --- | --- |
| constructor | Source throws `ReferenceError` for undeclared `dbPath`. `SourceVolumeStore::new` is a bounded, independent metadata constructor; the oracle also observes the constructor with that global explicitly injected. |
| `saveVolumeDataById` | `SourceVolumeAction::SaveVolume` overwrites the same document ID, stores `value.rawVolume/ltcVolume/price/type/blockHeight`, then updates the two cumulative documents. Repeated saves increment totals again. |
| `updateCumulativeVolumes` | `UpdateCumulative` adds global volume, adds LTC subtotal only for exact `UTXO`, q8 HalfUp then Number, writes LTC before global. Contract total is never implicitly updated. Negative finite metadata and negative zero are retained where the source retains them. |
| `getUTXOEquivalentVolume` | Store workflow accepts typed token or contract requests, returns `SourceValuation` with every queried `SourcePrice` origin. Contract inputs with falsy collateral/notional or absent inverse return `None`; unsupported types also return `None`. |
| `getTokenPriceInLTC` | Exact direct `0-token` lookup with truthy numeric-price test and explicit `Fallback` price 0.001. It does not automatically value token 0 at 1: source token-pair valuation can itself encounter the `0-0` fallback. |
| `calculateVolIndex` | Source fails at `.sort` on the Promise returned by promisified `findAsync`. Pure `source_volatility` implements ordered positive-price log returns, sample variance and annualization by `sqrt(144*365)`. `SaveVolatility` explicitly repairs cursor/await handling, uses stable descending nested block height, deterministic caller time and writes `volIndex` only when at least two documents were selected. Limit zero means no limit. |
| `_normalizeContractInfo` | `source_normalize_contract_info` preserves selected inner ID, then outer ID, then requested ID; defaults notional property/value/leverage to 0/1/1 and converts zero leverage to 1. Typed metadata retains ticker, native/inverse, collateral ID and on-chain pairs. Coercive strings/objects remain source observations. |
| `getContractUnitLTCValue` | `source_contract_unit_ltc(..., false)` computes Number notional times token price, divided by leverage, then binary64 `toFixed(8)` rounding. Base-chain notional property uses price 1. Nonpositive or overflowed notional returns source zero. Native/inverse flags do not alter this helper's formula. |
| `getContractUnitLTCValueDetails` | Same numeric helper with `details=true`, permits negative notional results and rejects zero. Source returns a Number despite the telemetry-oriented name; provider errors propagate instead of being swallowed as in the normal helper. |
| `getLastPrice` | Source-compatible direct/inverse lookup, q20 BigNumber reciprocal, supplied height deliberately ignored. Inverse uses only the first two hyphen components. `retained_price_as_of` separately considers eligible direct then inverse rows and never recovers an overwritten historical version by invention. |
| `getCumulativeVolumes` | Strict `< block` lookup of the surviving global/LTC/contract documents. Later overwrites can make an earlier query return zero. Each source DB read has its own swallowed-error boundary; the metadata owner has no simulated I/O errors. |
| `getBlockVolumes` | `get_block_volumes` sums nested LTC volume with BigNumber/Number boundaries and tests the full ID for character `0`. `SourceBlockVolumeResult` distinguishes the source's empty scalar zero from its nonempty totals object. Missing numeric schema yields an explicit finite-domain error instead of NaN. |
| `getVolumeDataById` | Source calls `findOneAsync` on the unresolved `getDatabase` Promise. Typed `document` supplies useful lookup over the explicit metadata owner. |
| `sampleVolumesByBlock`, `sampleVolumesByBlockRange` | Same unresolved-database source defect. Typed sampling explicitly uses the source's **top-level** height/volume fields, so nested writer records do not suddenly become valid samples. |
| `calculateCumulativeVolume` | Same unresolved-database defect; resolved-adapter source sums top-level volume with Number addition. Typed function supports numeric property IDs and the resulting safe literal prefix. Arbitrary unescaped-regex text remains a quirk observation. |
| `saveCumulativeVolume` | Source references an undefined bare `saveVolumeDataById` function. Explicit `Put` permits caller-specified documents; no successful source save is invented. |
| `auditVWAP` | Source normally fails through its broken sampler. Typed method uses the explicit top-level samples and actual source token-price lookup/fallback, with sequential Number accumulation. Empty/zero total gives `None`. |
| `calculateVWAP` | Existing exact ratio API remains unchanged. New `source_calculate_vwap` implements sequential finite Number accumulation over explicitly supplied volume/price pairs; caller swaps amount1/amount2 for contract mode. |
| `getVwapData` | Source normally fails through unresolved database access, then throws its replacement error. Its second `findAsync` argument is an options-shaped object, not an actual cursor sort/limit; the oracle records that boundary without claiming NeDB honors it. `repaired_recent_vwap` explicitly sorts top-level height descending and limits matching rows. |
| `saveVWAP` | Source references undeclared `volume` and `price`, ignoring its `vwap` argument. No successful source write is invented. A caller may explicitly `Put` the intended `vwap-ID` document with both values. |
| `computeLiquidityRewardShare` | `source_liquidity_reward_share` implements the source log10 attenuation, defaults/clamps, min/max ordering and 0.99 cap. Finite options may deliberately produce a negative share; this is metadata characterization, not an issuance policy repair. |
| `calculateAttenuatedLiquidityReward` | Clamps negative fee to zero; multiplies Number-to-BigNumber fee/share and q8 ROUND_DOWN before Number. It is not a fee debit or payout. |
| `calculateLiquidityReward` | `liquidity_reward` retains explicit fee-paid override, default rate 0.00005, cumulative-context fallback and block-dependent strict-prefix query. Token is unused in the source and grants no property authority. |
| `baselineLiquidityReward` | Context wrapper that overrides `feePaid` with the explicit fee. |
| `getVWAP` | Nested `value.volume/price`, inclusive trailing window and both orientations without reciprocal conversion, BigNumber q20 division then q8 HalfUp and Number. Saved `rawVolume` is not substituted for missing `volume`. |
| `getTLPriceInLTC` | Exact `vwap-1` lookup with explicit source fallback origin. |
| `vestTokens` | `source_vesting_projection` normally returns zero tier metadata because the static global is unset. An explicitly supplied static volume above 1e9 reaches undeclared `totalVolume` and returns an unsupported-path error. No vesting balances are created. |

Token-pair LTC valuation deliberately follows Number multiplication/addition.
Contract-trade LTC valuation follows BigNumber
`volume * collateralLtc * notional`, additionally `* tradePrice` only when
`inverse == false`, then q8 HalfUp/Number. Contract-unit LTC valuation is a
different source formula and uses binary64 `toFixed(8)`, including actual binary
ties, rather than decimal-string q8. These formulas are not interchangeable.

All typed Number fields must be finite; source NaN/Infinity, string concatenation,
truthiness coercion and malformed data shapes are separate oracle observations.
Finite source outputs are preserved, including harmless overflowing notional
intermediates where the normal unit helper returns zero. Final nonfinite outputs
fail explicitly. IDs are bounded to 256 UTF-8 bytes, kinds to 32 bytes, source
heights to 2^53-1, and explicit times to the JS Date finite range. Unsupported
source paths have named `SourceVolumeError::UnsupportedSourcePath` dispositions.
An accepted maximum context height can derive an internal exclusive cumulative
bound of 2^53; this does not relax public query/document height limits. Reward
clamps use explicit JS signed-zero ties: max chooses positive zero when either
zero is positive, min chooses negative zero when either is negative. All 32
combinations of cumulative/max/min/pivot/slope zero signs have source-generated
share and fee bit tests. Actual division-underflow negative zero becomes positive
zero through Number `toFixed`, while a nonzero negative tiny input rounds to
negative zero, matching the distinct source branches.

Logarithm/square-root helpers use Rust binary64 math in source operation order.
Transcendental last bits can differ from V8/platform math; source share and
volatility vectors use a small floating tolerance, while q8 fee and valuation
vectors compare exact bits. This is not a claim of bit equivalence for every
near-rounding-boundary transcendental input, and no economic reward/price
authority is attached to the result.

## Atomic mutable metadata and complete replay

`SourceMetadataAtomicV1` is independent of `VolumeHistory`. It preserves source
overwrites, first-insertion document order, repeated-save counting, two distinct
schemas and strict cumulative lookup. `Put` explicitly imports/upserts supplied
documents; no source-origin/authentication flag is created. `apply` and
`apply_batch` stage the complete operation or batch before committing, repairing
the source's partial three-write behavior. A late capacity/arithmetic failure
leaves documents and action history unchanged. Batch application clones once.

Capacity is at most 65,536 surviving documents and 16,384 complete actions.
`rollback_to_action_count` is an explicit action-prefix replay repair, not block
selection or source reorg consensus. It rebuilds overwritten versions and totals
from retained actions; later queries never invent older documents automatically.
The original exact index's block rollback, duplicate rejection and byte fixtures
remain unchanged.

The separate `TLVI` version-1 checkpoint stores capacity, the complete typed
action log and redundant derived documents in first-insertion order, followed
by CRC32. Every finite Number bit pattern, optional field, ID, height, type and
time is bounded/validated. Decode replays actions, compares the complete derived
document bytes, rejects noncanonical tags/NaN/Infinity/counts, every truncated
prefix, altered derived data, trailing bytes and another version, then demands
an identical re-encoding. It does not serialize an unauthenticated cache as an
accepted state. CRC is corruption detection, not authentication. Volatility
replay requires compatible binary64 transcendental results on the restoring
platform; this metadata checkpoint is not a cross-platform consensus format.

## Verification

`scripts/volume_index_vectors.cjs` verifies the pinned file digest and executes
the source class in a VM with an in-memory DB stub. It loads the reference
checkout's pinned BigNumber 9.3.1, now archived and hash-checked locally; network, disk DB, and contract-registry
operations are not invoked. Eight source-executed VWAP vectors cover empty and
zero weights, ordinary and contract-swapped entries, fractions, and sub-atom
rounding. Seven vectors cover the source's constant-share fee rounding subset.
The original 15 rows and archive remain byte-identical. The extended script also
executes the complete source for all 27 methods plus constructor: 49 method
scenes, 312 numeric vectors and 26 explicit quirks in
`volume_index_methods.tsv`/`volume_index_quirks.tsv`. It uses genuine archived
BigNumber 9.3.1, captures every DB/Contract query/update trace and final document,
and injects deterministic `Date.now`. No source expressions are rewritten.

The default adapter reproduces real `db.js`'s async `getDatabase` and promisified
`findAsync` return shapes. Separately labelled resolved/cursor adapters exercise
the arithmetic behind broken dependency boundaries; they are not fabricated
success under the default adapter. Real NeDB persistence, daemon HTTP and actual
contract-registry authority are never invoked. Partial second/third write
failures, undefined variables, ignored height, schema mismatch, repeated save,
falsy/string fallback, regex interpolation and vesting failures are retained.
Committed TSV fixtures are consumed by Rust tests without Node dependencies.

Regenerate with:

```powershell
node scripts/volume_index_vectors.cjs
node scripts/volume_index_vectors.cjs --source-root C:\path\to\tradelayer.js --write
```

Omit `--write` to verify source execution against the committed fixtures. Run
`cargo test --frozen --test volume_index` for fixture comparison, atomic failure,
inclusive window/type subtotal, source classification, reverse-pair behavior,
rollback/replacement, and arithmetic/capacity boundary checks.

The default command executes the archived source without a separate checkout
or npm installation. See [decimal arithmetic](DECIMAL.md) for its pinned reference.
