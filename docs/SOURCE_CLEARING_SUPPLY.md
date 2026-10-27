# Literal clearing supply checks

`source_clearing_supply_impl.rs` runs the pinned `clearingFunction`,
`verifyNetContracts`, `getTotalTokenBalances`, and their reached metadata/query
helpers against the actual shared source owners. Its sources are
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The composed test loads
the unchanged Clearing, Tally, Property, Insurance, IOU, Channels, Contract and
Vault modules; no expected result supplies a dependency answer.

The additional Vault archive is `fixtures/legacy_vaults_source.txt`, LF SHA256
`3dcf0554923864729bedcfc8244a06e4235d10a3241256eb3ba0a9d3d1534983`. The test adds
that complete hash-pinned module to its source inventory, replacing the older
oracle harness's explicit unowned Vault boundary. Existing source text and
fixtures are unchanged.

## Actual owner behavior

Clearing first removes empty channels through the real `SourceChannels` owner
and shared DB backend, then runs funding and settlement. Its optional contract
and Tally checks retain their source flags and call order. A successful Tally
delta sets the static flag, and a successful clearing supply check resets it.
An awaited failure preserves preceding writes, cache changes and flags.

The Property helper lazily loads its actual property index and preserves cached
metadata. A nonempty cache does not reload changed persisted metadata; an empty
cache retries loading. Tally totals retain separate available/reserved/margin/
channel and vesting aggregation, source NaN handling, and Number boundaries.
Static Tally addresses remain distinct from the instance's constructor field:
the first failed load can leave the static map undefined and produce the source
`.entries`, `.values`, or `.has` TypeError. Later failed loads retain a previously
initialized static map. This distinction is included in the V2 diagnostic state.

Vault initialization creates its two empty maps before the first DB read. Its
`findOneAsync` result is one document, while the source attempts to hydrate only
an array. Ordinary imported Vault documents therefore leave the maps empty.
Initialization failures retain the initialized maps, so a subsequent query need
not repeat the failed load. The synthetic-token lookup is intentionally
unawaited; its rejected promise is recorded separately from the awaited query
outcome. No balance or outstanding amount is invented from an unhydrated map.

Supply comparison sums separately rounded integer-satoshi balances from actual
Tally, fee cache, Insurance, numeric-ID Vaults, corresponding vesting and IOUs.
Synthetic IDs retain the source outstanding-supply diversion. IDs 2, 3 and 4 and
literal managed type 2 retain the source exemptions. Property 5 retains its
historical IOU profit/loss reconciliation rather than receiving an economic
conservation repair. Numeric-string IDs preserve the source's strict comparisons
and Number conversion where used. Missing, NaN and infinite expected supply keep
their source BigNumber behavior and error-message formatting.

## One-satoshi dust and asynchronous errors

A one-satoshi surplus is ignored by the source. A one-satoshi deficit starts an
unawaited Tally delta and then awaits the actual Insurance load and deposit.
The delta's own `getTally` is also unawaited; reading the promise's balance fields
produces a NaN recorded total. The original eleven arguments, channel change,
transaction fallback, UUID key, update/insert shape and flag timing are retained.

The composing runtime's two DB-boundary callbacks advance the actual pending
delta stages around Insurance loading. They preserve this observable order:
delta UUID/factory, Insurance factory, Tally factory/delta find, Insurance find,
Tally find/delta write, followed by the actual Insurance deposit/event/snapshot
graph. The staging job is drained before the public operation completes,
including after an Insurance exception. It is transient execution state, not a
second balance or database owner.

An unawaited delta rejection does not become an awaited clearing exception.
Actual `{name,message}` rejections are retained in bounded
`globalClearing.unhandledRejections` state and bound by complete replay. A direct
awaited `Tally.recordTallyMapDelta` call does reject on its own failed write.
Source exception prefixes remain distinct from the runtime's explicit rollback
for local unsupported-domain and capacity errors.

## Evidence and limits

`tests/source_clearing_supply.rs` compares 187 actions, including 120 calls, with
the complete unchanged CommonJS graph. Cases cover empty clearing in four module
orders, actual Channels deletion and its omitted-address DB defect, flags after
successful and failed checks, exact supply, mismatches, permitted IDs/types,
Property 5 reconciliation, both one-satoshi directions, cached properties,
unhydrated Vault documents, first versus later Tally load failures, nonfinite
inputs, primitive Map key identity, actual DB failures and source retries.

The test compares outcomes and messages, complete persisted collections, Tally
and Insurance caches, all loaded Margin instances, actual Channels state,
property/Vault/static-map state, rejected promises, and ordered storage/UUID/
clock effects. A second test checkpoints a successful supply call whose
background delta rejected after preceding Insurance progress, restores by actual
journal replay under an independent head, and continues the genuine retry.

These are the source's reached final checks. Empty Vault maps are evidence of
the source loader defect, not a claim that Vault creation, redemption or custody
is implemented. Full protocol completeness still requires the remaining actual
source graphs and their own composed execution evidence.
