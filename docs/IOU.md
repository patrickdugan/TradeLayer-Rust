# Offline IOU buckets and claims

`src/iou.rs` ports the stateful bucket, claim-accrual, and claim-reduction
functions in `tradelayer.js/src/iou.js` at
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The source SHA-256 after CRLF-to-LF
normalization is
`7ad5e87e4d72cf57c640c8c15564a3da21dfb4f2e692097ac8aab8215018cf96`.

The store changes its own IOU state and returns proposed payout allocations.
It owns no ledger balance and performs no signature, chain, or collateral
authorization. The pinned JS payout function obtains a `MarginMap` instance
but does not actually credit it. Connecting allocations to the live ledger
would require a separate authenticated settlement transition, replay evidence,
and a decision about the source's repeated-payout behavior.

The separate [funded local adapter](FUNDED_IOU.md) now connects these
allocations to actual escrow transfers under an explicit atom policy.
[Complete local checkpoints](IOU_CHECKPOINTS.md) preserve both source IOU state
and the funded adapter's full ledger, ownership and replay metadata. Neither
addition authenticates historical or live settlement inputs.

## Implemented behavior

| Source functions | Rust behavior |
| --- | --- |
| `key`, `claimKey` | `IouKey` pairs distinguish bucket and claim-map namespaces internally; IDs are exact `u32` values. |
| `get`, `getDoc`, `getBucket` | `get` returns zero for missing buckets; `bucket` returns complete decimal fields. |
| `addLoss`, `addProfit`, `addDelta` | Stateful exact decimal updates, positive-input gates, separate losses/profits and signed delta, block-start bookkeeping. |
| `blockReductionTowardZero` | `max(abs(start) - abs(end), 0)`, including crossings through zero. |
| `applyToLosers` | Source-exact negative `addDelta` call when the bucket is negative and mark delta is positive. |
| `getClaimMap`, `saveClaimMap`, `addIouClaims` | Bounded state, sequential buyer then seller accrual, positive PnL ratios, retained claim insertion order. |
| `payOutstandingIous` | Current-bucket-block gate, loss-pool cap, positive-claim filtering, per-claim decimal allocations and atomic claim reduction/deletion. |
| `getTotalForProperty` | Sum all signed bucket amounts for a property, then clamp the final sum at zero. Claim maps contribute zero. |
| `delete`, `deleteForProperty` | Bucket-only deletion preserves claims; property deletion removes both bucket and claim maps. |

An updated block differs whenever its number differs from the stored block;
older blocks reset the counters too. The store does not invent a monotonic
chain/reorg rule. A first loss/profit starts with an amount of zero. Zero and
negative `addLoss`/`addProfit` calls and zero deltas leave state unchanged.

## Source quirks retained

1. `addProfit` subtracts from bucket **amount**, despite the adjacent comment
   saying profits do not reduce the payout pool. Payouts use `blockLosses`,
   which profits leave unchanged.
2. `applyToLosers` makes a negative bucket more negative by applying
   `-min(markDelta, abs(deficit))`. The port preserves that sign.
3. `payOutstandingIous` reduces claims and does not subtract from bucket amount
   or `blockLosses`. Repeated calls in one block reuse the same loss pool until
   claims are exhausted. Rust returns these proposed allocations without
   crediting any account.
4. Claims accrue as `delta * (positivePnl / sumPositivePnl)`: division rounds
   first. Payouts use `(payout * claim) / totalClaims`: multiplication occurs
   first. BigNumber's default division is 20 decimal places, `ROUND_HALF_UP`.
   Rounding residuals are retained rather than redistributed.
5. `BigNumber.toString(10)` also rounds to 20 decimal places. Every source
   bucket-field write and updated-claim write therefore rounds independently.
   A positive subprecision loss can create a stored zero bucket. Two accruals
   to the same buyer/seller address persist and round in sequence.
6. JavaScript object enumeration puts canonical array-index address strings
   `0` through `4294967294` first, in ascending numeric order. Other keys
   preserve insertion order. The allocation output and stored claim order
   preserve this behavior, including nonindex strings such as `01` and
   `4294967295`.
7. Nonpositive claims imported through `saveClaimMap` do not participate in
   payouts and remain in the document. Empty claim maps remain present too.

## Explicit input and storage boundary

`IouStore` is in-memory and bounded to 4096 combined bucket/claim-map records
and 16384 claimants globally. Fallible arithmetic, bounds, and input validation
complete before a mutation is installed. No partial buyer accrual, partial
payout, or counter update survives an error.

IDs are unsigned `u32`, blocks are unsigned integers no greater than
`9007199254740991`, and claimant names use the crate's bounded printable ASCII
`AccountId`. These are opaque identifiers, not evidence of address ownership.
Inherited `Object.prototype` names (for example `__proto__` and `constructor`)
are rejected explicitly: source plain-object prototype behavior is outside
the typed claimant domain. `save_claim_map` accepts finite positive, zero, or
negative decimal claims and rejects duplicate claimant keys. It does not
silently coerce JS `null`, booleans, malformed numeric values, NaN, or Infinity.
Decimal precision and magnitude also obey the shared decimal module's bounds.

The source DB's catch-and-return-zero error paths, nondeterministic audit IDs
and timestamps, audit-insert warnings, Mongo/NeDB I/O, durability, migration,
and asynchronous failure ordering are outside this in-memory port. The vector
runner does execute the source's audit inserts, audit exclusion from totals,
and property-delete selectors with synthetic documents; that does not prove
production DB behavior. Cross-module transaction admission and funded IOU
redemption remain open roadmap work.

## Verification

`fixtures/iou_vectors.tsv` contains 67 sequential transitions across seven fresh
stores. Each row includes the actual source result, every bucket and claim map
after the transition, and the property total. Cases cover loss/profit resets,
backwards blocks, sign behavior, fractional accrual, same-address updates,
integer-key enumeration, source `toString(10)` rounding, payout gates, repeated
payouts, subprecision claims, and both deletion scopes.

Verify the archived pinned sources without a separate legacy checkout or npm
installation. The optional source flag checks another checkout at the same pin:

```text
node scripts/iou_vectors.cjs --check
node scripts/iou_vectors.cjs --source /path/to/tradelayer.js/src/iou.js --check
cargo test --frozen --test iou
```

The runner checks the LF source hash, executes the unmodified module in a VM,
and uses the complete archived BigNumber 9.3.1 library with a verified code hash
and asserted 20-place HALF_UP defaults. See [decimal arithmetic](DECIMAL.md).
The DB and unused MarginMap dependency are explicit in-memory stubs. Rust
tests consume those source-produced strings and additionally check error
atomicity, global bounds, claimant replacement, and loss-pool reuse.
