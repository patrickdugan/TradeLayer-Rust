# Literal source IOU allocation

`source_iou_settlement_impl.rs` adds the full read and allocation helper graph
from unchanged `fixtures/legacy_iou_source.txt`, pinned to
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA-256
`7ad5e87e4d72cf57c640c8c15564a3da21dfb4f2e692097ac8aab8215018cf96`.
Its public dependency namespace is `PnlIou`; arguments retain the original
source order and types. Existing accrual methods `addLoss`, `addProfit`, and
`addIouClaims` remain in the shared accounting include.

The added methods are `key`, `_db`, `audit`, `claimKey`, `getClaimMap`,
`saveClaimMap`, `getBucket`, `addDelta`, `blockReductionTowardZero`,
`applyToLosers`, `payOutstandingIous`, `get`, `getDoc`,
`getTotalForProperty`, `delete`, and `deleteForProperty`. All persisted state
remains in the same Core `iou` collection; audit IDs use the same bounded
clock/random continuation. A reached Margin load uses the actual mutable
Margin owner and shared backend. There is no second balance authority or
supplied IOU result.

`payOutstandingIous` requires a bucket updated in the requested numeric block.
It caps the requested amount by `blockLosses` and positive claims, emits a
summary audit, loads the actual Margin map, calculates proportional shares,
emits each item audit, and saves the reduced claim document. Shares retain
BigNumber division at twenty decimal places. The returned amounts are
BigNumbers. The method itself never credits Tally or Margin, and never debits
the bucket's `blockLosses`; its caller must execute any later source credits.
Repeated calls consequently use the same recorded loss pool with reduced
claims. These source omissions are preserved.

Read exceptions retain their literal catch boundaries. `get`, `getDoc`, and
`getBucket` catch document-read errors, while the preceding database-factory
error propagates. Claim reads and writes propagate. Audit insert failures are
caught after consuming the actual random value; an audit factory failure
propagates. An item or summary audit can therefore persist before a later
Margin load or claim-save failure, and a retry repeats that source prefix.

Keys preserve source coercion: bucket keys interpolate original values, while
claim keys use JavaScript Number conversion. Property deletion uses a numeric
query with strict stored-value matching. `getTotalForProperty` scans all
non-audit documents and sums amounts with matching numeric property IDs; it
returns a Number zero for negative or NaN totals, distinct from its BigNumber
zero read-failure result. `applyToLosers` retains the source's negative-delta
route into `addProfit`, and a plain numeric mark retains the missing `.lte()`
method error.

The helper reuses the verified bounded BigNumber constructor from the insurance
slice and preserves signed zero, NaN, and infinities through sums, products,
comparisons, and division. Decimal resource limits and arbitrary JavaScript
object-identity/coercion graphs remain explicit local domains. Source claim
documents are ordinary objects; mutation of array or string claim containers
is explicitly unavailable rather than silently changing their type. Local
domain/capacity failures roll back the coupled runtime, while source throws
retain prior effects.

`tests/source_iou_settlement.rs` executes 89 observations against the unchanged entire IOU and Margin
modules in the pinned CommonJS realm. It compares returned/thrown values,
complete persisted collections, actual loaded Margin instances, Tally caches,
and ordered DB/random effects. Cases cover proportional thirds and numeric
address ordering, repeated allocation, stale blocks, empty or nonpositive
claims, NaN propagation, every reached failure boundary and retry, legacy
routes, audit overrides, typed totals, and deletion. Its observations never
become answers supplied to the Rust execution.

The failed claim-save and failed Margin-factory scenes also restore after each
of eight observations using the original-command journal and an independently
retained expected head. Continuation checks compare full current owner state
and canonical bytes before retrying from the restored runtime.

The entire 89-observation reference corpus matches exactly under the archived
genuine BigNumber 9.3.1 and the verified source-lock 9.1.2 preload. This checks
these executions and does not assert general version equivalence.
