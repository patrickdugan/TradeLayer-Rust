# Literal contract placement over the source queue

`contract_placement_source` ports `Orderbook.addContractOrder`, `insertOrder`,
`matchContractOrders`, `isValidOrder`, `isValidOrderbook`,
`cancelExcessOrders`, and `sourceCrossContractReserve` from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c/src/orderbook.js`.
The complete archived class is `fixtures/legacy_orderbook_source.txt`, SHA-256
`c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98`.
No source expressions or reference flags are repaired.

The module owns no balances and no duplicate book collection. It loads, mutates,
and saves through the supplied `ContractQueueSourceOwner`. Financial effects
are actual dependency calls. A missing reserve, margin, or settlement capability
returns an unavailable-dependency error; a recorded source match or a callback's
returned book is never substituted for executing the local matching algorithm.
The source runtime's Tally owner is an alternative to the conserving economic
owner, and the two are not simultaneous spendable authorities.

## Public integration

```rust
pub fn add_contract_order<B: ContractPlacementBackend>(
    queue: &mut ContractQueueSourceOwner,
    arguments: &[SourceValue],
    backend: &mut B,
) -> Result<SourceOutcome, SourceRuleError>;
```

The arguments are the source's original thirteen arguments, including its last
`{orderbookInstance: id}` adapter handle. A missing or falsy last argument
constructs a fresh actual queue instance as the source does. Insurance remains
unused; `isLiq`, `reduce`, `post`, `stop`, original IDs, sender, original types,
and `initialReduce` retain their source values. This layer does not reinterpret
Logic's earlier shifted argument plumbing.

`ContractPlacementBackend: SourceBackend` supplies the overridable method
`invoke_in_queue(&mut self, &mut ContractQueueSourceOwner, &DependencyCall)`.
Its default forwards to `SourceBackend::invoke`; there is deliberately no
blanket trait implementation. **Every** external placement dependency uses
this hook. A live implementation can harvest preceding queue DB effects before
performing its own reads, and can execute settlement or cancellations against
the same queue. The ordinary dependency calls are:

| Target | Exact source arguments |
| --- | --- |
| `Margin.loadMarginMap` | `contractId` |
| `Margin.getPositionForAddress` | `sender, contractId` |
| `Contract.moveCollateralToReserve` | `sender, contractId, originalAmountOrFlipExcess, price, blockTime, txid` |
| `Contract.getInitialMargin` | `buyOrder.contractId, actualTradePrice` |
| `Orderbook.processContractMatches` | first the explicit receiver handle, then source `matches, blockTime, false` |

Only the settlement target has the extra first receiver adapter argument; the
source call itself has three arguments. Both resolved and rejected dependency
results preserve their source ordering. The match result is saved before
settlement is invoked, so later settlement rejection leaves that book saved.

The module also exports `insert_order`, `match_contract_orders`,
`is_valid_order`, and `is_valid_orderbook`. The pure matcher receives a mutable
`SourceValue` book and a `SourceBackend`. It computes actual matches and in-place
book mutations; the backend supplies initial margin, not a match answer.
`cancel_excess_orders` and `source_cross_contract_reserve` have the same
queue/arguments/backend shape as placement, with their original five arguments.

## Source behavior retained

Placement determines reduction from the existing signed contracts and **strict**
boolean `sell`. It reserves only the excess of a flip and skips an opening or
flip when the actual reserve response is falsy or nonpositive. Pure reductions
do not require a new reserve. An explicit failed flip reserve is not treated as
a successful pure reduction.

Insertion uses truthy `sell` to select the side and compares `o.time > order.time`;
it does not substitute `blockTime`. Invalid string-book JSON returns undefined.
Falsy input creates a local empty book. The generated order has `sell`, while
the contract shape checker requires `side`, so the source's own generated order
can fail that independent shape check. `isValidOrderbook` passes the array index
as the callback's `contract` argument via `Array.every`; index zero therefore
uses token shape rules and index one uses contract rules. That defect remains.

Matching sorts buys by descending BigNumber price and sells by ascending price,
then uses binary64 block-time subtraction. It retains the original zero-amount
removal, market-order price rules, maker flags, same-block post priority,
self-trade removal without a reserve refund, match transaction-ID selection,
full order spreads and all additional match fields. Amount subtraction uses
BigNumber semantics before the source Number conversion. Margin used is rounded
as BigNumber to eight places; remaining `initMargin` is the **string** produced
by the source binary64 product's `Number.toFixed(8)`. The two different rounding
boundaries are preserved, including cases where they produce different last
digits. Repeated original transaction IDs are not deduplicated.

Book JSON parsing reuses the queue's Number-preserving parser, retaining negative
zero, signed underflow, overflowing JSON numbers, duplicate-key replacement, and
integer-key ordering. Successful parsed-object loads return a source cache alias.
Insertion, sorting, maker updates, partial fills, and removals synchronize that
same cache. Missing-book fallbacks and separately parsed string books do not
acquire a false cache alias. Comparator exceptions do not publish V8's private
sort work array into the cache. Source throws retain earlier cache, book, and
external effects rather than rolling the whole operation back.

## Literal cancellation and cross-contract defects

Cancellation constructs a fresh instance using the original contract ID. It
sorts sell prices descending and buy prices ascending, then visits each array
from its end, buys before sells. It removes an owned order before calling
`Tally.updateBalance(address, collateralId, originalInitMargin,
-Number(originalInitMargin), 0, 0, "excessOrderCancellation", block)`.

When sufficient margin has been freed, the source returns immediately **without
saving the changed book**. Tally credits and cache removals persist, while the
original persisted order can be cancelled again on retry. Only the path that
exhausts cancellable orders without reaching the requested amount saves the
book. A Tally rejection also retains the preceding cache removal. The port does
not repair these persistence or reserve-refund defects.

Cross-contract sourcing calls `Contract.getAllContracts()` with no arguments;
it does not call or emulate a collateral-filtered query. It skips falsy IDs and
strictly equal `skipContractId` values. After each genuine cancellation, it
re-reads `Tally.getTally(address, propertyId)` and derives freed funds from the
actual increase in available balance. It never trusts a cancellation return
value as an accounting result. A rejected all-contract query is caught before
the initial tally read, as in the source.

The debit argument list refers to undeclared `block`, although the parameter is
`blockHeight`. The port calls `SourceGlobal.get(["block"])` at that precise
argument-evaluation boundary. An absent global produces the original
`ReferenceError: block is not defined` after cancellation credits but before
the debit. A real ambient value permits the source debit and subsequent
BigNumber updates; the port never substitutes `blockHeight`.

## Bounds, recovery, and checks

All state remains in the supplied queue and external source owners; this module
introduces no separate checkpoint. Tests restore complete queue checkpoints
after each action, then continue using the actual external capability state.
The shared SourceValue bounds and the queue's bounded JSON/codec domains apply.
Decimal work is bounded by the existing Decimal limits. A no-progress recursive
matching cycle is stopped by a local 2,048-round limit instead of exhausting the
Rust process stack; at most 1,024 matches are admitted.

Stable total price/time comparators support the full bounded array length. For
arrays of 64 or more entries, malformed, NaN-price, or nonfinite-time comparators
are explicitly unsupported because V8's large-run merge path becomes observable
for those non-transitive or throwing comparators. They return a local domain
error rather than inventing a different source ordering or exception prefix.
Small-array source comparator errors and special Number behavior are checked
against the actual runtime. Aliased arbitrary caller object graphs, functions,
and JavaScript prototype mutation remain outside SourceValue's owned-tree
domain; live orderbook identity is carried by the explicit handle.

Local capacity/domain failures restore this module's queue or mutable book
input. They cannot undo effects already performed by an external capability;
a caller requiring atomic local rejection must stage the queue and its sole
Tally/financial owner together. This is distinct from source exceptions, which
retain their actual prefixes.

`tests/contract_placement_source.rs` executes the unchanged complete class in
Node with the archived genuine BigNumber. Its 109 placement/insert/match/shape
and queue-drain actions compare returned/thrown values, complete documents and
instance caches, ordered DB and dependency calls, original arguments, and
external capability attempts, with checkpoint recovery after every action.
Nine additional cancellation/cross-contract actions execute full hash-pinned
`tally.js` and `contractRegistry.js` in JavaScript and use the actual Rust
`SourceContractRuntime` Tally owner. They compare persisted tally rows, mutable
Tally cache, complete delta records, book persistence, and all instance caches,
including insufficient reserve's partial mutation, repeated early cancellation,
the unfiltered contract scan, and missing/present ambient globals. No supplied
fixture matches or fabricated balance answers drive that test. Separate checks
cover missing capabilities and local capacity/domain rollback.
