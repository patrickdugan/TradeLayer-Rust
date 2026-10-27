# Pinned channel financial execution

The source pin is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
These helpers extend the explicitly selected `SourceLifecycleV3` runtime. Its
actual Tally rows/cache remain the sole source financial authority; no Ledger,
channel escrow copy, conserving settlement plan or supplied financial verdict is
introduced. Existing V1/V2 admission and the original queue dispatch remain
separate.

`source_channel_financial_impl.rs` implements `Tally.updateChannelBalance`
(`tally.js:226–280`), the `channel === true` branch of
`ContractRegistry.moveCollateralToMargin` (`contractRegistry.js:795–824`), and
the token trade-history write (`orderbook.js:113–154`). It retains the source
load, cache mutation, delta, save and exception order. A channel debit can precede
a failed actor credit or failed per-column Channels debit. Zero Number changes
omit the delta; string `"0"` records one. Missing/Boolean/NaN/infinite values
follow BigNumber and source JSON behavior within the declared numeric bounds.
The common collateral reads and final Margin call remain in the original Core
adapter, including the seller's omitted arguments.

Logic's transfer starts two unawaited channel-balance updates. Separate transient
jobs capture each load, cache reference, delta continuation and save. Their
database/UUID order follows actual source microtasks around the Channels save
loop, including 2, 3, 4 and 6 cached channels, failures in its first two writes,
zero/negative-zero's skipped delta, string-zero's asymmetric debit/credit paths,
caught initial load failures and stale Tally caches. Successful reads with absent
rows or falsey data reset the cache; rejected factories/reads retain it. Source rejections remain
independent diagnostics while the other update and original Logic result
continue. Captured properties share mutations only for the same actual Map key
and cache generation; a subsequent load separates the old references. Jobs are
drained at the public stable boundary.

`token_settlement_source.rs` implements the actual
`Orderbook.processTokenMatches` / `processTokenChannelTrades`
(`orderbook.js:845–1056`). The detailed entry returns mutated match arguments,
including maker/taker labels written before a later exception. Fee collection,
four balance calls and trade history invoke actual runtime owners in order.
Token fee flooring handles negative fractional values explicitly.

## Source defects are observable

The pinned token methods use an undeclared lowercase `tallyMap`. The normal
main-subgraph realm throws `ReferenceError: tallyMap is not defined` at the first
financial identifier read. The port resolves that identifier through
`SourceGlobal.get` and preserves the failure. An explicit V3-only
`SourceGlobal.bindTally` configuration selects an alternate realm binding to the
same actual Tally namespace. It accepts no supplied balances or function return
values. This alternate profile exercises the otherwise unreachable source write
graph and is not evidence that the production import graph supplies that global.

Even under that explicit binding, Logic's token-channel handler writes `sender`
while the settlement helper reads `senderAddress`. It also passes ten arguments
to a nine-argument Tally method: the resulting `type`, `block` and `txid` are
`true`, `false` and the channel Boolean; the original transaction ID is ignored.
The helper moves ordinary reserves/available balances. It does not update channel
columns or charge channel token fees.

Ordinary raw type 20 is rejected earlier: validity reads `.hasSufficient` from
unawaited Promises (`validity.js:2339–2340`, `2369–2370`). Funded column A/B
examples retain that rejection. Raw type 19 uses the genuine contract settlement
with `channel=true`; its generated `sellSide`/`buySide` fields are not repaired
into the missing `sell` fields. Existing Margin/settlement behavior and retained
failures remain visible.

A direct, explicitly Tally-bound zero-amount token channel trade can pass the
wrong sender fields and reach volume recording. `source_channel_volume_impl.rs`
implements the actual `VolumeIndex.getTokenPriceInLTC` (`volumeIndex.js:101–113`)
and its unawaited Logic continuation. Its factory starts before the volume-save
factory; the actual price read resumes before the save update. Missing/falsey
prices return the source's 0.001 default. A rejected price factory/read remains
an unhandled rejection diagnostic while volume saving continues. The unused
Promise makes the saved LTC volume NaN; the BigNumber price retains the source
storage-clone shape. Save-factory/write failures preserve the source prefix,
and retry starts with no pending price job. Ordinary raw type 20 remains rejected
by its unchanged Promise sufficiency check.

## Channel queue

`contract_channel_queue_source_impl.rs` is included inside the existing queue
owner. Its separate `evaluate_channel_with_capability` entry adds the two active
static methods at `orderbook.js:414–513` without changing the original nine
methods or checkpoint envelope. It reuses the same persisted rows, instances,
actual database calls, failures and mutable settlement capability.

The enqueue stores `match`; drain reads `entry.payload.match`. Its own ready
token/contract entries therefore throw before settlement. Imported source rows
with the expected `payload` shape can reach real settlement. A later malformed
entry retains earlier financial effects while leaving the queue uncleared.

The main block sequence has another observable defect: the on-chain drain scans
`channel-*` active pairs, writes an empty ordinary `queue-channel-*` row and
removes the pair before the channel drain runs. The original channel row remains
stranded. Direct channel drain and main-order drains are tested separately.

The new channel sorter preserves V8's natural-run/binary-insertion comparison
schedule for at most 63 ready rows. Larger ready batches return local capacity
failure with rollback; no TimSort equivalence is claimed. Existing queue/state,
SourceValue, ASCII comparison and numeric bounds apply. Source exceptions retain
their prefix; local unsupported-domain/capacity errors are not source exceptions.
BigNumber instances used as token-order timestamps are an explicit unsupported
domain; ordinary Number, string and array timestamp coercions are retained.

## Differential evidence

`tests/source_channel_financial.rs` executes the unchanged archived Types,
decoder, validity, Logic, Orderbook, ContractRegistry, MarginMap, Tally, Channels,
Clearing, fee and history modules in the frozen CommonJS environment. Declared
boundaries are the in-memory storage/query adapter, activation, clock, UUID,
randomness and the explicit alternate Tally global binding. Unowned imports remain
declared and are not replaced with successful financial results.

The four tests include 236 whole-source observations, with 126 actual method
invocations and four raw Types/Validity/Logic invocations, replayed through both
the mounted runtime and the detailed token argument-mutation interface. They
also check local rollback at the explicit 64-ready-row boundary.

The corpus checks original raw type 19/20 admission, direct financial methods,
bound/unbound token settlement, argument mutation, negative fee flooring,
channel/actor/column shortfalls, caught save failures, thrown delta/history
failures, enqueue failure prefixes and both channel queue routes. Comparisons
include actual returned/thrown outcomes, persisted collections, Tally cache,
channel registry, withdrawals, fee buffer, clearing trades, queue instance
caches, and ordered database/UUID/random/RPC effects. The same corpus runs with the
archived BigNumber 9.3.1 and the integrity-verified lockfile BigNumber 9.1.2.
All four tests pass under each dependency version; all 236 source observations
are byte-identical between them. These checks used isolated direct `rustc`
builds against the mounted runtime, without concurrent Cargo runs.

The deep transfer profile also explicitly binds original RPC transaction input
from imported `sourceRpcTransactions` rows. Its classifier and pubkey extraction
execute the full unchanged TxUtils module archived in
`fixtures/source_channel_txutils_reference.txt` (58,349 LF bytes, SHA256
`38fbc7b5c1dc4d187867238b5ea00bc165c855e9ccfbc20c48141cd4796ac8a3`).
No classifier/extractor outcome is supplied by the oracle. The normal unbound
RPC boundary remains unavailable. This profile does not authenticate the raw
transaction or channel actors.

This is bounded source execution, not signature authentication, production NeDB
equivalence or a conserving-policy claim. The separate economic Ledger and
typed scaling/custody lanes retain their own authority and admission policies.
