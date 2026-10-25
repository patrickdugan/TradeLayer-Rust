# Volume index observations

`src/volume_index.rs` adds a bounded in-memory reporting projection and exact
eight-decimal volume/VWAP helpers. It receives caller assertions and does not
change balances, reward issuance, consensus, transaction admission, or clearing.
It is a partial port, not a production replacement for `volumeIndex.js`.

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
- `getLastPrice` accepts a height but does not apply it to its DB lookup. Rust
  does not expose a price lookup that silently ignores an as-of height.
- `computeLiquidityRewardShare` uses `Math.log10` and floating-point policy
  coercions. No replacement approximation is used for issuance. The exact fee
  helper takes an explicitly supplied share between zero and one; source shares
  also have a 0.99 cap that a caller must apply when modeling that policy.

Still unported: database persistence and production adapters; source cumulative
document mutation; price lookup/default fallback and inverse lookup;
`getUTXOEquivalentVolume`; contract normalization/unit LTC valuation; volatility
and annualization; `auditVWAP`; recent-entry retrieval in `getVwapData`;
`saveVWAP`; logarithmic attenuation and its context/fee-rate wrappers;
`getTLPriceInLTC`; and vesting. `saveCumulativeVolume`, `saveVWAP`, and
`vestTokens` reference undefined variables in the pinned source; no replacement
behavior is inferred for those paths.

## Verification

`scripts/volume_index_vectors.cjs` verifies the pinned file digest and executes
the source class in a VM with an in-memory DB stub. It loads the reference
checkout's pinned BigNumber 9.3.1, now archived and hash-checked locally; network, disk DB, and contract-registry
operations are not invoked. Eight source-executed VWAP vectors cover empty and
zero weights, ordinary and contract-swapped entries, fractions, and sub-atom
rounding. Seven vectors cover the source's constant-share fee rounding subset.
The committed TSV fixtures are consumed by Rust tests without Node dependencies.

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
