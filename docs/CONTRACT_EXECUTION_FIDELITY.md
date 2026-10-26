# Contract execution fidelity audit

Static review of archived source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. This document records inspected
control flow and proposed composed-source cases; it reports no new execution
results or runtime parity. Completing three validation reads does not establish
the source's subsequent queue, collateral, position and clearing effects.

## Raw type 18 dispatch

The decoder returns `contractId, price, amount, sell, insurance, reduce, post,
stop`; price is a base-36 integer divided by 1e8. Types adds the original actor,
txid and block, then awaits the actual validator.

Verified references: `fixtures/legacy_types_source_txDecoder.txt:388`,
`fixtures/legacy_types_source_types.txt:334`, and
`fixtures/legacy_validity_source.txt:1638`.

`Logic.typeSwitch` passes twelve arguments to `tradeContractOnchain`, with the
following literal shift:

| Queued Logic parameter | Value supplied by typeSwitch |
| --- | --- |
| `isLiq` | decoded `reduce` |
| `reduce` | decoded `post` |
| `post` | decoded `stop` |
| `stop` | block height |

Verified references: `fixtures/native_genesis/source/logic.js:246` and `:1069`.

For ordinary flags all false, the queued order has `isLiq=false, reduce=false,
post=false, stop=<numeric height>`. That numeric stop does not itself prevent
placement or matching: the inspected contract matcher never reads `stop`, and
its contract shape checker does not require a boolean stop. Raw post therefore
does not confer post priority; raw stop does confer the source's same-block post
maker/price behavior. Raw reduce selects liquidation behavior. The insurance
argument is unused inside `addContractOrder` and is omitted from the stored
contract order.

Logic initially persists a queue entry and active-pair metadata. It performs no
immediate reserve transfer or position update. Later,
`processQueuedOnChainOrdersForBlock` drains entries with height <= the current
height, ordered by height and the source sender comparator, and executes each
entry against positions changed by earlier entries.

Verified references in `fixtures/legacy_orderbook_source.txt`:
`queueOnChainContractOrder:263`, `processQueuedOnChainOrdersForBlock:298`,
`addContractOrder:1059`, `insertOrder:583`, `matchContractOrders:1373`.

## Effects beyond the three validation adapters

The validation calls `Margin.loadMarginMap`, `Margin.getPositionForAddress` and
`Contract.getInitialMargin` need genuine pending-owner reads. The actual source
also performs this write graph after validation:

1. Persist `queue-<contractId>`, `activePairs`, and the complete JSON orderbook.
   Placement reads positions again, reserves initial margin for new exposure,
   reserves only a flip's excess, and reserves none for a pure reduction.
   `ContractRegistry.getInitialMargin` uses price-dependent BigNumber arithmetic:
   inverse margin rounds upward to eight places; linear margin uses the default
   eight-place rounding. `moveCollateralToReserve` updates available/reserved
   Tally buckets. A failed reserve returns no matches and skips placement.
2. Save the matched orderbook **before** processing financial effects. Matching
   calculates margin at the fill price, then `processContractMatches` reads
   inverse/collateral/notional metadata, mutable Clearing price caches, both
   Tally balances and both actual positions.
3. Update fee caches and debit or rebate available/reserved/margin buckets.
   Reconcile reserve versus fill-price margin through
   `moveCollateralToMargin` and `MarginMap.setInitialMargin`. Update contracts,
   average prices, liquidation/bankruptcy fields, margin deltas and durable maps.
4. Record Clearing's same-block open/close pools, settle opening/closing PnL,
   source losses from available/margin/cancelled reserves, release margin, and
   persist trade/PNL history. Closing imbalances can update PnlIou claims,
   profit and loss records.
5. Resolve contract-unit LTC value, persist volume/cumulative volume, evaluate
   clearlist liquidity rewards, and finally save the margin map. A zero or
   nonfinite contract-unit volume value throws after earlier financial writes.
   Reward paths issue un-awaited property-3 Tally credits.

Verified references:

- `fixtures/legacy_contract_registry_source.txt`: `getNotionalValue:428`,
  `getInitialMargin:504`, `moveCollateralToReserve:570`,
  `moveCollateralToMargin:594`.
- `fixtures/legacy_margin_map_source.txt`: `loadMarginMap:24`,
  `setInitialMargin:202`, `saveMarginMap:342`,
  `updateContractBalancesWithMatch:359`, `updateContractBalances:392`,
  `realizePnl:1028`, `recordMarginMapDelta:1075`, `settlePNL:1207`,
  `getPositionForAddress:2076`.
- `fixtures/legacy_orderbook_source.txt`: `processContractMatches:2222`,
  `sourceFundsForLoss:1975`, `locateFee:3174`,
  `recordContractTrade:150`, volume write at `:2944`,
  reward qualification methods at `:1677` and `:1716`.
- `fixtures/legacy_clearing_source.txt`: `recordTrade:97`,
  `isPriceUpdatedForBlockHeight:1081`.
- `fixtures/legacy_volume_index_source.txt`: `saveVolumeDataById:21`,
  `getContractUnitLTCValue:178`, `calculateLiquidityReward:504`,
  `baselineLiquidityReward:560`.

## Source defects and ConservingV1 distinctions

These are observable source behaviors, not reasons to silently repair dispatch:

- The validator does not check positive/integer contract amount. Zero/default
  amount can validate and queue; placement reserves zero and successfully skips
  the order. Negative amounts require their own explicit supported-domain decision.
- Each queued order validates before end-phase reservations. Several individually
  valid orders can oversubscribe one balance; source later skips an unfunded
  placement rather than undoing earlier source effects.
- Self-cross matching removes an order without refunding its already reserved
  Tally collateral. Source partial-fill `initMargin` becomes a fixed-eight string.
- The seller's `moveCollateralToMargin` call supplies the buyer maker flag and
  only thirteen arguments: seller position enters `flag`, while `txid` and
  `position` are omitted (`legacy_orderbook_source.txt:2491`).
- Linear `settlePNL` performs JavaScript `(price-lastMark)*contracts` without a
  notional multiplier; `realizePnl` uses the notional multiplier. These are
  distinct source computations, not interchangeable formulas.
- Basic rewards inspect `issuerAddress`, although source-created contracts store
  `issuer`. Enhanced rewards inspect root match addresses, while ordinary matches
  contain nested order addresses. Substituting the intended fields changes source.
- A source feed's absent-price default 1 can satisfy validation's reference-price
  check (`legacy_validity_source.txt:2134`). Requiring a persisted funded price
  is an additional economic policy. Clearing's mutable price cache can also give
  different previous marks to successive matches within one publication block.
- An end-phase exception can leave the already saved matched book and financial
  prefix while the ready queue remains uncleared. Original raw transactions were
  processed when queued; an end-phase retry can process retained entries again.

Current `src/types_economic.rs:816` projects type 18 into typed `Order`, rejects
insurance/reduce/stop, requires positive quantity, and directly maps decoded post.
That direct post mapping differs from the actual Logic argument shift.
`src/economic_block_stage_impl.rs:229`, `src/economic.rs:422` and `:473`, and
`src/contract_orderbook.rs:314` implement the current coordinated economic path.
Atomic late-failure rollback, replay-fenced drain, reserve refunds, collected
funding before payouts, and integer conservation are deliberate ConservingV1
choices. They must remain distinguished from the inspected source write graph.

## High-value composed-source cases

The next corpus should execute unchanged Types, validator, Logic, Orderbook,
ContractRegistry, MarginMap, Tally and the reached Clearing/volume/history paths
against declared storage/services. Fixed dependency answers alone cannot prove
matched-position ownership.

- Ordinary unmatched placement, then opposite-side matching in the same and a
  later block; inspect queue flags, reserves, fees, complete positions and maps.
- Each raw flag separately, especially post versus stop, plus insurance's unused
  argument and reduce's liquidation shift.
- Zero amount, negative amount, insufficient collateral, and two individually
  valid queued orders competing for one balance.
- Pure close, exact close, flip, partial fill, same-block reopen/close, and a
  later queue entry whose reserve decision depends on an earlier match.
- Self-cross collateral, source Number/eight-place boundaries, and fee payments
  sourced separately from available, reserve and margin.
- Missing source price default 1, a newly published mark with multiple matches,
  and actual mutable Clearing trade/cache continuation across restart.
- Failures after book save, fee write, position write or volume conversion;
  compare retained prefixes, processed raw IDs, uncleared queues and retry effects.
