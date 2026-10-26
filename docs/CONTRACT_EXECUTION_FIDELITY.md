# Contract execution fidelity audit

Review and bounded execution evidence for archived source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The literal `SourceContractRuntime`
now connects actual raw type18 validation, queue, placement and financial
settlement and compares all32 full-JavaScript scenes/135 observations, including
persisted collections, live caches and partial failure/retry behavior. It is an
explicit alternative to the separately named conserving EconomicState. See
[SOURCE_CONTRACT_RUNTIME.md](SOURCE_CONTRACT_RUNTIME.md) for the execution API,
source defects, evidence and remaining whole-module gates.

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

`EconomicSourceReadV2` supplies `Margin.loadMarginMap`,
`Margin.getPositionForAddress` and `Contract.getInitialMargin` from the actual
selected numeric contract, sole position owner and accepted contract catalog.
The load returns a Pending instance-handle projection; a missing position keeps
the exact three-field zero shape, while a missing definition produces
PendingThrow. Exact/lowercase-Bech32 lookup and lossless signed Number bounds
remain explicit. These reads create no second margin store and do not enable
unrelated Margin methods. See `SOURCE_CONTRACT_READS.md`.

`ConservingSequentialV2` enables those reads. V1 keeps precisely those targets
unavailable, preserving earlier DependencyUnavailable receipts and independently
expected checkpoint policy. Source-free typed owners also support the reads.
The actual source additionally performs this write graph after validation:

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

The composed harness now executes the source behaviors below; they must remain
distinct from the named conserving settlement choices:

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

Current `src/types_economic.rs` projects type 18 into typed `Order`, rejects
insurance/reduce/stop, requires positive quantity, and directly maps decoded post.
That direct post mapping differs from the actual Logic argument shift.
`src/economic_block_stage_impl.rs`, `src/economic.rs` and
`src/contract_orderbook.rs` implement the current coordinated economic path.
Atomic late-failure rollback, replay-fenced drain, reserve refunds, collected
funding before payouts, and integer conservation are deliberate ConservingV1
choices. They must remain distinguished from the inspected source write graph.

## Concrete raw-owner and composed-source evidence

`tests/economic_source_oracle_contracts.rs` feeds real raw13/14/16/18 through
the actual oracle/catalog owners and V2 validation. The non-admin actor first
fails the genuine country predicate, then a raw country self-certification in
the pending owner permits subsequent orders. Two opposing quantity-two orders
at 100 create positions +2/-2, margin 20 per actor, no remaining reserve and
one real matching receipt with original transaction IDs. The ordinary cash,
position and total-fee values match the independently executed source fixture's
`ordinary-matched-quantity-two/3` observation. That assertion deliberately does
not compare source fee buffering, source queue flags, full IDs, persistence or
failure prefixes as though they were the same policy.

An actual source publication at 104 then drives a Paid funding epoch at height
24, with one positive Ledger debit from the long and credit to the short.
Complete TLECv10 raw replay preserves the epoch and continues through a flip
to -3/+3 and a close to flat. This proves funded execution and recovery in
`ConservingFundingV1` / `ConservingV1`, not literal source funding parity.
Separate V1 regressions preserve its rejected/unavailable-read history and
reject a restore under the independently expected V2 policy.

`scripts/source_contract_execution_vectors.cjs --check` executes 25 complete,
unchanged hash-pinned source modules in one CommonJS realm. Its frozen TSV has
32 scenes / 135 observations, including initial state, exact raw decoding and
validation, real Logic enqueue, actual queue drain and financial methods. No
validation, match, reserve, PnL, fee, reward or insurance outcome is supplied.
The explicit async document store, activation, clock, deterministic identifier
and unowned-service boundaries are detailed in `SOURCE_CONTRACT_EXECUTION_REVIEW.md`.
Module import order and captured circular partial exports remain observable.

The corpus executes ordinary and partial matches, each raw flag, zero/negative
quantity, oversubscribed reserves, self-cross, close/flip, same-block mutable
marks, buffered fees, real IOU paths and an explicitly imported reward alias.
Injected history/volume failures retain genuine prior effects and queues;
retry can execute the original orders again. A tiny contract-unit value reaches
an uninjected financial throw. Main-reachable, Tally-first and VolumeIndex-first
loads expose distinct fee/volume outcomes. These are concrete source results,
including defects; they are not instructions to normalize the source into the
conserving owner.

The standalone `ContractQueueSourceOwner` covers nine genuine queue/book
methods with 117 differential actions. It compares outcomes, complete documents,
instance caches, ordered DB/placement calls and retained exception prefixes;
every action continues through complete checkpoint restore. A separate test
matches 57 enqueue boundaries from the frozen composition. Placement now calls the actual literal settlement graph through an owner-aware
capability into the same source balance authority. This changes the separate
source runtime; the conserving economic runner remains a distinct policy.
See `CONTRACT_QUEUE_SOURCE_OWNER.md` for storage, capacity and durable-restart
boundaries.

## Numeric dependency provenance and CI

The committed oracle loader and fixtures retain their genuine archived
BigNumber **9.3.1** provenance, LF-normalized source SHA256
`58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16`.
The pinned JavaScript commit's package-lock instead resolves BigNumber
**9.1.2**. A separate work-only comparison downloaded that official npm tarball
and verified the lock's exact SHA512 integrity before executing it. Tarball
SHA256 is `819bd8cc7ee880c2bb5014f45c0c4593300810de7e1fe348ef3e6bed35be149c`;
raw CommonJS source SHA256 is
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.

That comparison replaced only the shared loader's exported `loadBigNumber`
with a fresh VM constructor from verified 9.1.2, retaining all source effects,
fixture bytes and original version/hash labels. On Node v24.19.0 all 31 invoked
generator/audit checks passed, including the composed 32 scenes / 135
observations, sends 17, oracle owner 311, margin 246, orderbook 287, clearing
117, decimal 1942, validity 564 and Types 1228. This establishes tested corpus
equality under 9.1.2, not whole-version equivalence or a change in fixture
provenance. The temporary preload/evidence stay outside the repository; CI's
normal source checks continue using the archived 9.3.1 loader.

The composed harness directly loads genuine `bitcoinjs-lib`, `tiny-secp256k1`
and `bip32`. CI explicitly installs the executed test-reference versions
6.1.8, 2.2.4 and 5.0.1, respectively, under its temporary reference prefix,
then passes that prefix with `--module-root`. It does not depend on a local JS
checkout or an accidental transitive installation of these direct imports.
These test versions are distinct from the pinned JS lock's 6.1.7, 2.2.3 and
5.0.0 entries. Existing genuine bitcore test references remain pinned separately.
The composed TSV uses LF with whitespace diagnostics disabled so significant
source observation separators are preserved.

## Remaining source graph

The literal `SourceContractRuntime` now executes the actual on-chain placement,
reserve/margin deltas, position fields, fee buffers/flush, mutable Clearing
mark/open-close/PnL pools, history, volume/rewards, IOU records and partial
failure/retry prefixes over one source Tally authority. Actual cross-contract
reserve cancellation and its source defects are composed through the same queue.
All135 frozen full-source observations compare complete persisted collections and
live caches, with additional component and adapter adversaries. Replay recovery
recomputes original inputs and complete continuation state.

This does not establish full global clearing, liquidation/deleveraging/insurance
payout orchestration, expiry/options, native-index/multicontract lifecycle,
channel queues, every enhanced reward path, all nonfinite/unbounded rounding
or comparator domains, or production NeDB/disk behavior. Source actor/state
provenance and portable per-transaction proof witnesses remain separate gates.
The literal owner is an explicit alternative to the conserving EconomicState;
its retained source failures remain distinct from atomic funded/admission
rollback. See `SOURCE_CONTRACT_RUNTIME.md` and `SOURCE_CONTRACT_REPLAY.md`.
