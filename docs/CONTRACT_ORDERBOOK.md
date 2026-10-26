# Contract orderbook metadata

`contract_orderbook` implements a bounded, independently staged contract book.
It owns orders and replay metadata, **no Ledger, balances, margin positions,
fee cache, IOU store or authority to settle a trade**. Its caller must combine
the book with the existing sole Ledger and economic components in one complete
state transaction. These are supplied orders, not authenticated wire inputs.

The reference is the complete 4,123-line `src/orderbook.js` from JavaScript
commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The unchanged LF archive is
`fixtures/legacy_orderbook_source.txt`, SHA256
`c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98`.

## Executable scope

Each `ContractOrderbook` has one nonzero contract ID. `Order` has a transaction
ID, opaque owner, height, buy/sell side, positive Decimal quantity and price,
post flag and market flag. Resting orders keep their original input, remaining
quantity, insertion sequence, initial-reduction flag and opening reserve
quantity. The market flag models actual source book inputs; it does not claim
that ordinary source `addContractOrder` accepts such a wire flag (it does not
copy an `isMarket` argument into its order).

Queue priority is height, last/second-last/third-last sender character, full
address and transaction ID. Suffix characters rank letters before digits,
missing characters last; letters compare case-insensitively. The full-address
fallback follows ordinary Latin ASCII alphanumeric locale ordering. Typed
owners are restricted to ASCII alphanumeric identifiers, so Unicode,
punctuation and host-dependent locale behavior are outside this API domain.
The enclosing coordinator determines ordering between different contracts;
the source processes persisted active-pair insertion order, not sorted IDs.

Matching sorts buys by descending price, sells by ascending price, then height
and stable insertion sequence. The actual source gives market orders **no
additional sort priority**. A market order at the front bypasses the crossing
test and takes the other side's price. Two front market orders use the buy
price, with maker flags still determined by height. Limits use the earlier
order's price; equal-height limits default to buy price and neither maker.
An equal-height sell post flag takes precedence over a buy post flag and uses
sell price. Post flags do not reject a crossing order or implement conventional
post-only semantics. Self-cross detection happens before the post rule: remove
the sell if it is maker, otherwise remove the buy. Removed owned orders are
returned explicitly for the custody coordinator to reconcile.

Decimal price comparisons and minimum quantities follow BigNumber. Actual
source `toNumber()` boundaries are retained for emitted match quantity and
each remaining quantity after subtraction. Source admission also converts the
existing position to Number, and native subtraction determines flip overshoot.
Thus `9007199254740993` and `0.1` boundary behavior is deliberate. The typed
domain limits each number to 40 coefficient digits and 20 decimal places;
all native-number results must remain finite. This domain includes fractional
contracts and prices. It is not a claim of exact real-number accounting across
source Number bridges. The custody coordinator must explicitly validate atom
conversion and funding.

## Reserve intentions and custody interleaving

`reserve_quantity(order, current_position)` returns the full opening/increasing
quantity, zero for a pure reduction, or only the Number-based overshoot for a
flip. Its result is a quantity, not a token amount or proof of a successful
reservation. Currency collateral depends on the registry policy and price.

For partial fills this port explicitly repairs reserve-quantity accounting:
consume the reducing portion first, retain at most the remaining opening
quantity, and return `buy_reserve_used`/`sell_reserve_used` in the match. The
source instead rewrites `initMargin` as native multiplication by remaining
amount at the latest fill's margin rate, even for a previously unreserved
reducing order. That source rewrite is not treated as custody evidence here.
Every cancellation returns remaining reserve quantities and never transfers
funds. Actual escrow currency totals and dust must be checked by the owner.

The funded coordinator can:

1. Clone its complete economic state.
2. Call `drain_queued_through(height)`; order IDs remain seen and each exact
   drained order becomes a pending admission.
3. For each returned order, read the current position after prior settlements,
   obtain its reserve intent and fund the actual custody transfer. Call
   `place(order, current_position)` and settle all returned matches, fees,
   margin changes, profits/losses and IOUs inside that staged economic state.
4. Call `finish_height(height)` once no ready queued/pending orders remain.
5. Verify all custody/component invariants, then persist and publish the entire
   transaction atomically.

`ready_orders` plus `admit_queued` is an alternative without draining.
`process_through` is a local convenience projection: it obtains initial signed
positions through a read-only callback and applies emitted fills to projected
positions before later admissions. Callback writes to external state cannot
be rolled back by this component. For real custody use the interleaved API.

All fallible mutations stage a clone before commit. Drained orders must be
placed in the returned order, exactly once, with original fields unchanged;
direct admission cannot bypass an outstanding drained prefix. `BookSnapshot`
includes queued and drained orders, resting metadata, sorted historical seen
IDs, next sequence and processed height. Structural import checks live-ID
uniqueness, membership in the seen history, reserve/remaining bounds, insertion
sequence uniqueness/order, contract and numeric limits. Import remains
unverified and cannot reconstruct historical balances or authorize a position.
The parent supplies a complete durable economic codec; this module introduces
no separate economic persistence owner.

Local limits are 4,096 live orders across queued, drained and resting states,
65,536 retained IDs, and the JavaScript safe-integer maximum height. No source
acceptance claim is attached to those resource limits. One bounded iterative
match operation replaces source recursion. Each iteration removes at least one
order, so the complete operation is bounded by live orders. Cancellation frees
live capacity while retaining historical replay IDs.

## Complete source method map

The unchanged source has 15 static and 54 instance methods, excluding its
constructor. All are accounted for below; this does not claim that economic,
token or broken source methods have been implemented by the metadata module.

| Source active methods | Port/integration boundary |
| --- | --- |
| constructor, `getOrderbookInstance`, `loadOrderBook`, `saveOrderBook`, `_getOrderbookDB` | `new`, `snapshot`, `from_snapshot`; database ownership remains with the complete-state coordinator. Corrupt source JSON becoming an empty book is characterized, not imported. |
| `_addActivePair`, `_updateActivePairs`, `queueOnChainContractOrder`, `processQueuedOnChainOrdersForBlock` | Queue, readiness, drain, admission and finalized-height APIs for one contract. Parent retains cross-contract scheduling. |
| `addressCharRank`, `compareSenderAddresses` (top-level helpers) | Bounded-domain `compare_sender_addresses`, used by actual queue processing. |
| `addContractOrder`, `insertOrder`, `matchContractOrders`, `getAddressOrders`, `getOrdersForAddress`, `getOrderBookData`, `getReserveByAddress` | `place`/`admit_queued`, matching, `resting`/`side`/owner queries, `reserve_for_owner`, complete snapshot. No collateral transfer is implied. |
| `cancelOrdersByCriteria`, `shouldCancelOrder`, `cancelAllContractOrders`, `cancelContractOrderByTxid`, `cancelContractBuyOrdersByPrice`, `cancelAllOrdersForAddress`, `cancelContractOrdersForSize` | Owner-checked `cancel` supports all, exact ID and side-specific threshold. Explicit repairs replace unsafe/no-op/wrong-array source branches. Size-based partial order cancellation is not exposed: caller may cancel/re-admit explicitly in one complete transaction. |
| `isValidOrderbook`, `isValidOrder` | Typed structural validation. Source requires inconsistent `side` versus `sell` fields and passes the callback's index as `contract`; its disabled validation is not trusted. |
| `decomposePositionChange`, `processContractMatches`, `deriveTradeDelta` | Parent coordinator combines actual matched metadata with MarginMap position changes, mark/average capture, funded settlement and IOUs. `reserve_quantity` covers only the admission decomposition. |
| `calculateFee`, `resolveMaker`, `locateFee`, `routeMatchFees` | Authoritative tally fee plan belongs to the separate tally module/parent coordinator. This module has no duplicate fee algorithm. |
| `sourceFundsForLoss`, `sourceCrossContractReserve`, `cancelExcessOrders`, `adjustOrdersForAddress`, `_pruneInstaLiqOrdersFromFreshBook`, `estimateLiquidation` | Economic/loss/liquidation orchestration remains outside this book. No virtual AMM funding, reserve recovery or liquidation authority is inferred. |
| `evaluateBasicLiquidityReward`, `evaluateEnhancedLiquidityReward`, `updateVolumeAndRewards` | Parent-owned volume/reward accounting with explicit funding/issuance rules. |
| `saveTrade`, `recordTokenTrade`, `recordContractTrade`, `getTokenTradeHistoryByPropertyIdPair`, `getContractTradeHistoryByContractId`, `getTradeHistoryByAddress`, `buildTradeObject`, `recordTrade` | Match receipts are deterministic metadata. Trade-history indexing/persistence belongs to the parent; no UUID-based writes here. |
| `processContractMatchesShort`, `validateMatch`, `calculateFees`, `handleFlipLogic`, `moveCollateral`, `updateContractBalances`, `realizePnLAndSettle` | Active but broken refactor; missing unqualified method references are characterized. It does not replace the main settlement path. |
| `queueChannelTrade`, `processQueuedChannelTradesForBlock`, `processTokenChannelTrades` | Channel execution is outside this module; source queue payload mismatch is not silently repaired as authenticated channel acceptance. |
| `queueOnChainTokenOrder`, `addTokenOrder`, `normalizeOrderBookKey`, `calculatePrice`, `divideAndRound`, `matchTokenOrders`, `processTokenMatches`, `cancelAllTokenOrders`, `cancelTokenOrderByTxid`, `cancelTokenBuyOrdersByPrice`, `cancelTokenSellOrdersByPrice` | Existing SpotExchange and separately documented token policies; this module implements no second token book or balance owner. Source lowercase `tallyMap`/undefined `order` failures remain source characterizations. |

## Exact-source evidence and deliberate repairs

`node scripts/orderbook_vectors.cjs --check` executes the unchanged complete
archive with the pinned BigNumber 9.3.1 implementation. Dependencies are
explicit: in-memory NeDB-style storage; a registry returning initial margin 2
and recording reserve calls; a caller-selected current-position margin adapter;
logging and UUID adapters; a tally trace; inert modules that reject unexpected
calls. No class method is replaced. This executes actual matching, actual
one-sided admission, actual queued processing and actual cancellation/failure
paths, not a handwritten arithmetic oracle.

The main TSV has **287 source cases** covering limit/post/market matching,
same-owner removal, stable priority, multi-fill/fractional and Number boundary
amounts, admission reserve calls, address comparisons and queued ordering.
Its fifth column records actual dependency calls/writes. Rust tests compare
all 287 cases. `orderbook_quirks.tsv` records **9 additional source cases**:
owner-only cancellation is a no-op due to nesting; foreign buy-ID removal;
sell-ID removal splicing the buy array; price-only no-op; price plus a missing
ID removing another owner's orders; corrupt JSON swallowed into an empty book;
the short refactor's unqualified `validateMatch`; missing buy-side criteria in
the contract-ID wrapper; and token cancellation's undefined `order`.

The Rust cancellation API intentionally fixes ownership, side selection,
missing-ID no-op behavior, reserve reporting and replay retention. Its separate
regressions test those repairs; they are not asserted equal to broken source
results. Complete economic atomicity, conservation, historical acceptance and
authenticated transaction/script validation remain the coordinator's separate
gates. External insurance and transaction-index/script work are unaffected.
