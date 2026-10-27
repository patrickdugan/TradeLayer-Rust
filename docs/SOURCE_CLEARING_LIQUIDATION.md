# Pinned source liquidation

`source_clearing_liquidation_impl.rs` ports the actual liquidation graph from
commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c` into the source runtime's
`GlobalClearingV2` policy. It uses the existing Tally, Margin arena, queue and
settlement owners. It introduces no second balance authority, supplied
financial result, or conserving liquidation policy.

Inspected unchanged source methods are
`Clearing.computeLiquidationPriceFromLoss`, `_sweepAmmLiquidityFirst`, and
`handleLiquidation` in `legacy_clearing_source.txt` (starting at lines 2232,
2344 and 2454), and `Orderbook.estimateLiquidation` and
`_pruneInstaLiqOrdersFromFreshBook` in `legacy_orderbook_source.txt` (1226 and
2096). Reached Margin methods use their actual owner implementations:
`computeMaintenanceMarginRequirement`, `generateLiquidationOrder`,
`simpleDeleverage`, and its reduction/delta/save operations.

## Execution and reference identity

The original `handleLiquidation` arguments remain:
`[ctxKey, orderbook, Tally, position, contractId, block, inverse, collateralId,
liquidationType, marginDent, notional, markPrice, applyDent, markShortfall,
tallySnapshot, priceInfo]`.
The explicit adapters represent the actual orderbook as
`{orderbookInstance:id}` and the known Tally namespace as `{sourceTally:true}`.
An arbitrary object is not accepted as a supplied Tally implementation.

The handler loads a genuine fresh Margin instance, computes maintenance and
partial/total size from actual collateral, derives the source bankruptcy
boundary, and calls the actual liquidation-order generator and book estimate.
It sweeps safe AMM orders first, inserts only the remaining safe prefix, runs
the real matcher and financial settlement, and saves the real book. It then
computes loss on the unfilled remainder, seizes available collateral before
margin, calls actual ADL, applies capped pool credits, updates the clearing
cache and returns the source summary.

`Orderbook.inspectBook`, `setCachedBook` and `newInstance` are explicit adapter
projections for actual owned receiver state. They add no fabricated source
business calls or financial effects. AMM level changes update that same live
book after each mutation, preserving the cached prefix if a later operation
throws. The pure `sweep_amm_liquidity_first_source(&mut book, arguments)` helper
also exposes its real mutated argument and retained source exception boundary.

Clearing contexts store stable IDs in the Margin object's existing arena.
The live context-array projection is refreshed before ADL, so replacement
entries produced by earlier trades are visible. `Margin.simpleDeleverage`
receives explicit `sourcePositionObjectAliases` for argument 0's array entries.
Its in-place mutations affect those actual objects. Subsequent source cache
replacement allocates a new object ID, preserving older references rather
than mutating an unrelated loaded position by equal value.

## Source behavior retained

- AMM preference requires sender exactly `amm`; the pruning helper separately
  recognizes both `amm` and `amm:` prefixes when deciding reserve refunds.
- The estimator stops at the first unsafe valid book level. It does not sort
  or search past that level to manufacture safe liquidity.
- ADL uses the actual source's counterparties, margin releases, pool shares,
  deltas and persisted liquidation records. Empty or insufficient
  counterparties do not cause an invented conserving fallback.
- `marginDent`, the loss implied by mark movement on the unfilled size, and
  seized collateral remain distinct source calculations.
- The source zeroing patch sets `averagePrice: null` while retaining the
  pre-existing `avgPrice` property. The port preserves that spelling.
- The returned `totalDeleveraged` is the source's book fill plus requested ADL
  remainder; it is not repaired to report only counterparties actually found.
- Pruning loads a fresh book instance, scans backwards, splices before reserve
  refunds, and saves only at the original final boundary. Failed refunds retain
  the cached splice and earlier successful effects while leaving the persisted
  book at its actual earlier state.

BigNumber operations reuse the existing bounded source arithmetic, including
20-place HALF_UP division, the source's explicit 30/eight-place rounding,
nonfinite values and signed zero. Number operations retain binary64 bridges.
Input trees, arena capacity, decimal resources and queue limits remain explicit
local bounds. The enclosing runtime rolls back local unsupported/capacity
errors; source throws retain their real prefix.

## Actual asynchronous history and storage boundaries

The composed liquidation tests exposed a previously unobserved source ordering
requirement: `processContractMatches` calls `TradeHistory.savePNL` without
awaiting it. Its source `save` suspends at the database factory. A subsequent
Margin operation can emit its UUID and factory call before the history write
resumes.

`source_contract_history_schedule_impl.rs` implements this known unawaited
call path as bounded jobs containing the source literal history parameters.
The next genuine awaited database-factory boundary resumes the real JSON
serialization and update, with no effect-log rearrangement. Public stable
boundaries drain remaining jobs. Rejected writes retain their database event
and actual unhandled-rejection diagnostic while the financial method continues.
Direct awaited history calls retain the ordinary implementation. The limit is
1,024 jobs and 8 MiB of parameters, plus shared value bounds.

The declared source storage adapter uses `structuredClone`. Genuine BigNumber
objects therefore become plain enumerable `{s,e,c}` records in stored rows and
database observations. `source_storage_clone` preserves this base-1e14
representation, including signed zero and nonfinite constructor fields. It
does not restore a lost prototype or replace the object with a decimal string.
The runtime applies this only at the matching storage boundaries in V2.

The queue owner now records detailed database events at actual factory,
load/find and update execution, including arguments and returned/thrown values.
`take_detailed_db_calls` exposes those bounded observations. Existing
`QueueDbCall` values, V1 checkpoint bytes, state projection and equality remain
unchanged. Detailed traces are ephemeral and excluded from recovery state;
V2 trace exhaustion is an explicit local capacity failure.

## Verification

`tests/source_clearing_liquidation.rs` executes the unchanged complete archived
CommonJS modules through the established full environment. Financial outcomes
come from genuine Tally, Margin, Orderbook and settlement methods. Storage,
clock, UUID, random and external imports remain declared boundaries; a
rejection observer records actual rejected unawaited writes.

The two tests cover **101 observations**, including **74 actual Core method
invocations** and **eight pure AMM mutation scenes**. They compare complete
outcomes, every expected persisted collection, Tally cache, clearing contexts,
tracking maps, deferred rejections, all observed queue instances, and the full
ordered database/environment effects. Cases include all four existing entry
profiles; partial/total, long/short and inverse ADL; real book and AMM fills;
null liquidation generation; collateral/write failures; fresh-book pruning and
refund failures; and actual BigNumber storage fields.

Both tests pass in an isolated Rust harness against the archived BigNumber
9.3.1 constructor and the hash-verified pinned-lock 9.1.2 constructor. The
existing eight queue tests also pass unchanged, including their 117 source
actions, 57 composed enqueue boundaries, checkpoint and resource checks.
These checks do not establish parity for every remaining global clearing,
insurance, expiry, channel or external storage-recovery path. No Cargo or
shared integration file was changed by this slice's author; the parent owns
the coherent checked commit.
