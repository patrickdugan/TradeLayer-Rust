# Fractional funding calculations and conserving plans

`funding.rs` provides `source_average`, `source_rate`, `collateral_notional` and `plan_funding`.
It owns no Ledger, positions, oracle authority, funding schedule, replay set or
durable database. The complete economic owner must select scoped observations,
stage all custody legs, record its epoch and persist the combined state.

The reference is complete unchanged `clearing.js` at JavaScript commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, archived as
`fixtures/legacy_clearing_source.txt`, LF SHA256
`612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528`.
The actual notional calculation also executes the complete previously archived
`contractRegistry.js`, SHA256
`d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121`.
This is a funding subset, not a complete clearing port.

## Source calculations

`source_average(prices)` follows actual `oracle.getTWAP`: equal weights for
selected rows, sum/count at20 places, then q8 HalfUp, then Number. Empty input
returns None. This final quantization/Number bridge must occur before rate
calculation; passing an unrounded20-place mean can change threshold decisions.
The complete unchanged oracle.js archive (SHA256
`70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d`)
is executed for these vectors; its unused DLC bridge is an explicit adapter.

`source_rate(index, average)` follows actual BigNumber operation order:

1. Divide `index-average` by average at 20 decimal places, HalfUp.
2. Multiply by 10,000 and round basis points to two places, HalfUp.
3. Ignore absolute values below five bps; otherwise subtract five from the
   magnitude and preserve the sign.
4. Divide by eight and round to four places, HalfUp.
5. Cap magnitude at 12.5 bps and cross the source Number return boundary.

This preserves index-minus-average sign. The output is basis points per
funding event, not a chain-independent hourly rate. Zero numeric observations
return zero; negative prices and out-of-domain values are rejected. Source
truthiness also distinguishes numeric zero from string `"0"`; the latter is
recorded as a separate characterization rather than silently attached to a
typed Decimal. Nonfinite inputs cannot be constructed as Decimal.

`collateral_notional(raw, index, inverse, oracle)` supplies the scalar actually
selected by `applyFundingToPositions`, rather than the raw per-contract
notional used for mark PnL:

| Contract | Operation |
| --- | --- |
| Native or oracle inverse | Divide 1/index at 20 places, multiply raw notional, round eight places, return Number. |
| Oracle linear | raw notional * index, round eight places, return Number. |
| Native linear | Raw notional through Number, without additional eight-place rounding. |

Dividing raw/index directly can change inverse boundaries, so the source's
division-first order is retained. This API requires a positive index and does
not invoke fallback oracles. Zero raw notional is observable, but cannot fund a
nonempty nonzero-rate execution plan. Source return Number rounding, including
values beyond JavaScript's safe integer precision, is covered by vectors.

## ConservingFundingV1 execution plan

`FundingPosition` carries an opaque owner, exact fractional signed contracts,
and a caller-supplied actual available-atom observation. `FundingPlan` returns
positive `FundingPayment` debits and credits, signed `rate_bps`, selected
`collateral_notional` and `total_atoms`. Lists are sorted by ascending ASCII
owner and omit zero legs. No returned observation asserts successful funding.

Positive rate charges longs and credits shorts; negative rate charges shorts
and credits longs. Every payer is charged its own positive
`q8_HalfUp(abs(contracts)*collateral_notional*abs(rate)/10000)` exactly once.
The source's `Math.abs(Number(contracts))` bridge is deliberately absent from
this repaired planner: fractional Decimal position weights remain exact.

The sum of those individually rounded charges is the only credit budget.
Each receiver's ideal atom share is the exact rational
`total_atoms*abs(receiver_contracts)/sum(abs(receiver_contracts))`. Take each
floor, then assign remaining atoms by descending exact remainder and ascending
ASCII address for ties. Numerators, denominators and remainders use the Decimal
integer engine; no 20-place approximate ratio or f64 weight changes their rank.
All remainders share one denominator. Their sum is an integer smaller than the
number of receivers, so at most one additional atom per receiver is required.

Every payer's available balance and every receiver's final u128 addition is
checked before a plan is returned. Duplicate owners, missing sides, aggregate
overflow and invalid inputs reject the complete operation. There is no partial
mutable caller state. Nonflat one-sided books at a nonzero rate reject even if
tiny individual charges would round to zero. Empty/flat books and zero-rate
events produce zero plans. A zero total with both sides is valid and has no
legs. The planner does not require equal total long/short weights: the owning
position system separately enforces net open-interest conservation.

Resource limits are 4,096 positions, 40 coefficient digits and 20 decimal places
per input. Executable rate magnitude is at most 12.5 bps with at most four
places. These are local bounds, not historical consensus rules. Atom totals
must fit u128. The owner must debit all payers into its existing derived funding
escrow, credit receivers from that same escrow, verify zero residue, and commit
the epoch/receipts with its single Ledger. Do not nest ClearingSandbox: its
integer positions and owned Ledger are a separate research interface.

## Actual-source oracle and discrepancies

`node scripts/clearing_vectors.cjs --check` evaluates the complete unchanged
Clearing class and ContractRegistry class using pinned BigNumber 9.3.1. Explicit
adapters provide contract rows, native VWAP/oracle TWAP and index observations,
positions, NeDB-shaped reads/writes, deterministic clock and traced balance
calls. Other dependencies reject unexpected use. No source method or funding
formula is replaced. The main TSV records **117 actual source cases**: 9 average, 42 rate,
48 collateral-notional and 18 positive fractional payer cases. Rust tests
compare every main case; dependency traces are saved in the fifth column.

The **19 source characterizations** in `clearing_quirks.tsv` record:

- Negative rates pass a negative total into redistribution: supposed short
  payers receive credit while long receivers are debited.
- Independently rounded receiver shares can credit two atoms from a one-atom
  debit. A missing receiver side leaves only a debit prefix.
- A dependency exception on a later balance call leaves earlier successful
  calls visible. These are traced attempts, not fabricated Ledger execution.
- Source cadence is block%24: 23 and 25 do nothing; 24 pays; 48 with no positions
  still saves a nonzero-rate funding event. A zero rate saves no event.
- Funding-event write errors are swallowed. Average-provider failures return
  rate zero. Neither is evidence of an atomically completed epoch.
- Cold mark-cache priming can select a future row. Numeric oracle index values
  have no as-of height, while future object values are rejected. The active
  later `getIndexPrice` declaration calls its oracle with one argument.
- A truthy string-zero index produces -12.5 bps where numeric zero skips.
- Active `calculateClearingPNL` returns a sub-atom raw result while the older
  `calculateMarkToMarkPNL` helper rounds it to zero.
- Direct `OracleList.saveOracleData` upserts key `oracle-id-height`: saving100
  then200 at the same height leaves one row, TWAP200 and rate0. The high-level
  `OracleList.publishData` separately guards the first attempt at each height:
  its second same-height call is skipped, including after the first write
  failed. These are different source entry-point behaviors.

The existing JointRegistry accepts successive same-height publications and
keeps their external event IDs. The funded owner's history uses only its final
accepted publication at each oracle/height, matching one upserted row per
height. This is an explicit JointRegistry admission policy, not parity with
OracleList.publishData's first-attempt guard or circuit-breaker behavior.
Both the overwritten and final event IDs remain in global replay protection;
only the final stored observation contributes weight to the funding average.

The conserving planner intentionally differs from negative-sign, independent
rounding, missing-counterparty and partial-failure behavior. Its regressions
cover both signs, separately rounded half-atom payers, Hamilton exact fractional
ranking below 20-place ratio precision, ASCII ties, exact large quantities,
last-payer insufficiency, receiver overflow, all input permutations, empty and
tiny epochs, numeric limits and aggregate overflow.

## Complete active-method inventory and scope

The unchanged class has a constructor, **79 active static methods and zero
instance methods**. Duplicate `getIndexPrice` declarations resolve to the later
definition. Inventory is also emitted by inspecting the actual VM class.

| Active source methods | Scope here |
| --- | --- |
| `applyFundingRates`, `calculateFundingRate`, `getIndexPrice`, `clampFundingRate`, `applyFundingToPositions`, `processFundingPayments`, `saveFundingEvent`, `loadFundingEvents` | Source rate/notional and real call traces; conserving fractional payment plan. The complete owner implements observations, cadence, event IDs and custody. No source storage success is inferred. |
| `_ensureBlockTradeEntry`, `recordTrade`, `_normalizeTrades`, `_normalizeEntry`, `computeOpenedRemainderFromTrades`, `getTrades`, `countTrades`, `hadMultipleTrades`, `hadAnyTrade`, `recordDeleverageTrade`, `getDeleveragedThisBlock`, `getDeleverageTradesThisBlock`, `recordLiquidation`, `getLiquidation`, `getVintageBreakdown`, `resetBlockTracking`, `resetBlockTrades`, `initPositionCache`, `getPositionsFromCache`, `updatePositionInCache`, `addOrUpdatePositionInCache`, `flushPositionCache` | Transient trade/vintage/cache orchestration outside this pure funding module. |
| `computeLossCoverage`, `recordClearingRun`, `clearingFunction`, `verifyNetContracts`, `getTotalTokenBalances`, `updateAllPositions`, `reconcileReserve`, `sourceLoss`, `updateLastExchangeBlock`, `fetchLiquidationVolume`, `loadClearingDeltasForBlock` | Driver, solvency and reconciliation integration outside this module. Existing owner retains custody and invariants. |
| `isPriceUpdatedForBlockHeight`, `settleLiqNewContractsFromDB`, `settleNewContracts`, `pruneInstaLiqOrders`, `makeSettlement`, `normalizePositionMarks`, `applyTradeToOpenStats`, `computeOpenedAvgByAddress`, `consensusAddressSort`, `updateMarginMaps`, `getMarkTradeWindow`, `applyLossPoolDrain`, `flattenMark`, `calculateMarkToMarkPNL`, `recomputeContractBalanceSnapshot`, `computeLiquidationPriceFromLoss`, `getOpenedByAddressFromTrades`, `updatePositions`, `_sweepAmmLiquidityFirst`, `handleLiquidation` | Mark selection/raw-rounding quirks characterized; actual mark clearing, liquidation and AMM/ADL execution remain separate functional work. |
| `resolveOptionExerciseTarget`, `computeExerciseContracts`, `applyAssignedUnderlyingPosition`, `settleOptionExpiries` | Options expiry orchestration outside this subset. |
| `getLatestPositionByAddress`, `sortPositionsForPNL`, `calculateClearingPNL`, `calculateNewContractPNL`, `getBalance`, `settleIousForBlock`, `performAdditionalSettlementTasks`, `resolveInsuranceMeta`, `distributeInsuranceProRataToDelev`, `computeOptionAdjustments`, `saveClearingSettlementEvent`, `loadClearingSettlementEvents`, `calculateTotalMargin`, `isMarginConsistent` | Raw PnL discrepancy characterized; funded IOU/insurance/settlement accounting remains with the complete owner and existing ports. |

Funding cadence, mark selection/freshness, average sample history, source-specific
VWAP/TWAP selection, completed zero epochs, replay and durable rollback require
separate owner metadata. This module creates no authentication for observations
or authorization for wire settlement types. Adding mark transfers to existing
average-cost closes also requires explicit settled-basis accounting to avoid
double payment; funding correctness alone does not establish mark clearing.
