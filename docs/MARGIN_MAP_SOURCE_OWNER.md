# Actual source MarginMap metadata owner

`MarginMapSourceOwner` ports the contract-settlement slice of unchanged
`marginMap.js` from `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
The archive is `fixtures/legacy_margin_map_source.txt`, LF SHA256
`ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636`.
This owner stores complete source position objects and live instance identities.
It owns no spendable balance, Ledger, oracle, contract definition or duplicate
database. The composing backend owns actual shared database/Tally/Contract state.

## Methods and call boundary

The implemented methods are `getInstance`, `loadMarginMap`,
`getPositionForAddress`, `getAllPositions`, `setInitialMargin`,
`updateContractBalances`, `updateContractBalancesWithMatch`, `saveMarginMap`,
`recordMarginMapDelta`, `realizePnl`, `settlePNL`, `reduceMargin`,
`feeMarginReduce`, `updateAveragePrice`, `calculateLiquidationPrice`,
`calculateMarginRequirement`, `estimatePNL` and `writePositionToMap`.
Unimplemented MarginMap methods remain explicit UnknownMethod boundaries.

`evaluate(&SourceCall, &mut SourceBackend)` and `evaluate_instance(id, call,
backend)` return actual `SourceOutcome`. `invoke_source` /
`invoke_source_instance` expose async Returned/Thrown as Pending/PendingThrow.
Successful load/getInstance returns the adapter handle
`{sourceMarginInstance:id}`. Instance methods keep their original argument lists;
the instance ID is receiver context, not a new source argument.

For composition, `Margin.<method>` may carry the handle as its first adapter
argument; the dispatcher removes it before evaluating the original method.
`getPositionForAddress` always reloads its requested contract, independently of
the supplied receiver cache. Numeric and string series IDs retain their distinct
JSON document keys. A truthy load flag returns the existing document's raw
`value`. Missing documents create fresh instances. Source find/parse failures
are caught; database acquisition outside the source try block can reject.

The exact external dependency convention is:

| Target | Arguments | Timing |
| --- | --- | --- |
| `db.getDatabase` | `[collection]` | Awaited; returns the real opaque DB handle |
| `db.findOneAsync` | `[handle, query]` | Awaited |
| `db.updateAsync` | `[handle, query, update, options?]` | Awaited; omitted options stay omitted |
| `db.insertAsync` | `[handle, document]` | Awaited |
| `uuid.v4` | `[]` | Synchronous; before delta DB acquisition |
| `Contract.getContractInfo` | `[contractId]` | Awaited where source calls it |
| `Tally.getTally` | `[address, collateralId]` | Awaited during contract updates |
| `Contract.setModFlag` | `[true]` | Synchronous, after successful delta storage |

The explicit handle in DB method arguments represents the JavaScript receiver.
It is adapter context; original method arguments retain their order and count.
No selected method here independently calls Tally.updateBalance. Collateral
movement belongs to the actual surrounding contract/orderbook implementation.

## Position identity and mutation

`inspect_position(id, address)` inspects only that existing instance's cache.
It returns Undefined for an absent entry and never reloads or creates a row.
`set_cached_position` represents an actual source `this.margins.set` boundary.
`instance_value` projects the current complete map and empty option indexes.
The selected methods do not populate those option indexes.

Internally, map keys refer to position identities. Multiple keys can retain
the same object after source sets a position under its own `position.address`.
The checkpoint preserves those identities. Equal field values never establish
an alias.

`evaluate_detailed(instance, call, aliases, backend)` returns a SourceEvaluation
with actual dependency observations, mutated `arguments_after`, thrown/returned
outcome and any un-awaited dependency rejection. `PositionAlias {argument,
instance,address}` is explicit adapter knowledge that a position argument is an
already established cached reference. The cached object is used directly, even
if the caller's copied view is stale. The argument and named cached entry must
exist. Ordinary full-object arguments remain independent objects.

This distinction matters in the source:

- Initial getPositionForAddress results come from independent loads.
  They are not automatically aliases of the receiver used later for settlement.
- updateContractBalances mutates its supplied position before fetching contract
  or Tally data, stores that same object, then saves before recording its delta.
  WithMatch processes the buyer first and seller second; its bp/sp results are
  established aliases of those stored entries. Nested match positions appear
  in `arguments_after`, including mutations before a later source throw.
- settlePNL mutates the cached position while returning only a Number. Existing
  trade bp/sp references must therefore be observed through the established
  cached identity before the surrounding source persists or returns the trade.
- realizePnl mutates its position argument and records a delta, without setting
  the map or saving it. An explicit cached alias retains that mutation; an
  independent position argument changes only in `arguments_after`.

Arbitrary cyclic object graphs, aliased nested objects and reference-valued
Map keys are outside the SourceValue tree domain. This API does not infer or
manufacture those relationships from equal values.

## Preserved source behavior

An empty setInitialMargin receiver calls getAllPositions. That returns an Array
of position objects; the source's Map-like hydration path cannot turn a
nonempty Array of those objects into a Map, catches the failure, and can skip
the final save. A populated receiver instead saves with literal block `true`.
Supplied position fields, aliases, missing normalization and extra fields remain
observable; they are not projected into the strict typed Position schema.

Position lookup preserves exact matching and the source lowercase-prefix Bech32
fallback. A missing position is exactly `{contracts:0,margin:0,unrealizedPNL:0}`.
No address or persistent flat row is invented.

Contract update preserves the source average-price branch conditions, signed
contract change, liquidation fields, save-before-delta order and clearing tags.
Realized PnL retains the source isBuy sign and unrealized-PnL subtraction rules.
Linear settlePNL uses JavaScript `(price-lastMark)*contracts` without notional;
inverse settlement retains sequential BigNumber divisions and multiplication.
Its assignment is literally `oldUPNL - round(oldUPNL-pnl,8)` at Number boundaries.
It dereferences a missing position's avgPrice before its apparent missing-row
guard. Reductions retain unused arguments, negative fees/margin possibilities,
missing delta block/lastPrice fields and the original delta modes.

Arithmetic uses exact bounded Decimal operations for BigNumber work with the
default twenty-place half-up divisions and explicit source rounding and Number
conversions. NaN, infinity and signed zero remain representable source outputs.
Ordinary decimal and integer radix strings are supported. Decimal resource
limits and unsupported executable/prototype/named-array-property forms return
explicit local errors; they do not receive fabricated successful amounts.

Source Throws retain their actual cache, argument and backend-write prefixes.
There is no blanket transaction rollback. Unknown dependencies or local resource
errors likewise provide no automatic undo of already executed backend effects;
the composing runtime must handle its chosen local-error staging boundary.
This differs from the conserving economic runner's atomic funded/admission
rollback policy. Missing dependencies never become supplied financial outcomes.

## State, checkpoints and verification

`state_value`, `from_state_value` and `import_state` handle complete live
instance maps and position identities. Shared DB rows, UUID state and backend
failures are not duplicated in that owner state. A complete runtime checkpoint
must pin and restore those actual owners together. Process restart may instead
discard all Margin instances and reload the shared persisted documents.

The canonical `TLMP` version-1 binary checkpoint includes complete tagged values
and CRC32. Restore checks exact shape, unique primitive map keys, valid position
references, canonical Number/NaN bits, UTF-8, all bounds, end of input and exact
re-encoding. CRC is corruption detection, not authentication or proof of source
execution. Limits are 1,024 instances and position identities, the shared
SourceValue 8,192-node / depth-24 / 1 MiB text limits, and 2 MiB checkpoint bytes.
Counts/text/node budgets precede child allocation. Identity arenas are retained
within these bounds; they are not silently evicted.

`tests/margin_map_source_owner.rs` executes 76 actions against the complete hash-pinned module with
genuine archived BigNumber 9.3.1. Declared storage, UUID, Contract and Tally
boundaries are recorded; no method result, financial calculation or validity
verdict is supplied. It compares outcomes, external argument mutations,
dependency order/arguments, complete DB documents and instance caches, then
restores the complete identity checkpoint after every action and continues.
The corpus includes numeric/string IDs, empty hydration, updates and matches,
realized/settled PnL, aliases and independent objects, reduction/fees, all
linear/inverse liquidation branches, Number boundaries and early/late failures.
Separate tests cover checkpoint corruption/truncation, a valid-CRC oversized
count and unavailable reads. JSON.parse preserves signed zero and overflow to
infinity through the shared source JSON parser; save retains source JSON.stringify
normalization without altering the live cached object.

This owner is one actual source component. It does not by itself integrate
raw orderbook placement, sole Tally custody, mutable Clearing pools, fee flush,
history, volume/rewards, IOUs, global clearing or full source failure/retry
orchestration. The source method coverage must remain distinct from that
remaining composed financial graph.
