# Literal source option expiry settlement

`source_clearing_expiry_impl.rs` executes the pinned Clearing methods
`settleOptionExpiries`, `resolveOptionExerciseTarget`,
`computeExerciseContracts` and `applyAssignedUnderlyingPosition` from
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The unchanged source is
`fixtures/legacy_clearing_source.txt`, LF SHA256
`612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528`.

The include runs inside the sole literal `SourceContractRuntime` owner. It uses
the actual shared Margin object arena and Tally collection/cache. It introduces
no conserving options ledger, separate position store, collateral balance, or
supplied settlement result. These methods belong to the versioned global
clearing runtime graph; the earlier onchain runtime policy retains its previous
unavailable-method behavior.

## Source calls and mutations

Expiry loads an actual Margin instance and contract definition. The source
Margin constructor's expiry/ticker/OI indexes are empty, and loading persisted
positions does not hydrate them. The expiry method therefore executes its
original fallback scan of each position's `options` object. It collects unique
expired ticker strings in source iteration order and uses the permissive pinned
Options ticker parser. Future and malformed tickers retain the source behavior.

In-the-money legs assign signed underlying exposure at strike and transfer the
held option margin into the core position. The source average-price calculation
is preserved for additions, reductions, flats and flips. The option's expiry
selects the earliest finite qualifying contract expiration, retaining original
candidate IDs and stable ties. Assignment records its first Margin delta before
deleting the option leg. The second delta records the option removal using the
source's exact argument positions and mode strings.

Out-of-the-money legs call the real `Tally.updateBalance` with zero available
change and negative option margin change. A zero-quantity leg is deleted without
a custody call or delta, even when its metadata contains held margin. Successful
processing calls the original expiry-index cleanup and saves the actual Margin
map once at the end. Missing definitions, no expired tickers, and missing
positions preserve the source's early returns and preceding loads.

The three exercise helpers are synchronous at the dependency boundary; expiry
settlement is asynchronous. BigNumber work uses the composing runtime's bounded
source arithmetic with twenty-place half-up division. Source Number conversion,
NaN, infinity, signed zero, truthiness defaults and source rounding are retained.
The parser retains its source `parseInt`/`parseFloat` prefixes and missing strike.
Executable/prototype behavior, cyclic values and unsupported resource domains
remain explicit local errors.

## Object identity and failure prefixes

Expiry iterates `MarginMapSourceOwner::position_object_entries`, which exposes
the actual ordered map keys and established object IDs. Mutation writes back to
the same arena object before the next awaited dependency. Every already linked
map key and clearing-context reference observes that object; equal values never
establish an alias. Source cache mutation is retained even if the final database
save has not occurred.

For a direct `Clearing.applyAssignedUnderlyingPosition` adapter invocation, the
first argument is `{sourcePositionObject:id}` referring to an existing zero-based
Margin arena ID. The remaining arguments retain their original source order.
This explicit object-reference protocol exposes caller-object mutation through
the actual shared owner. A copied by-value position is not silently interpreted
as an existing reference. Missing, malformed and forged IDs are rejected.

Source exceptions retain completed assignment, option deletion, Margin deltas,
Tally changes, consumed failure schedules and counters. There is no source-level
atomic repair. In particular, failure after assignment but before final save can
leave a mutated live Margin instance while persisted positions still contain
the original option. A later source retry reloads that persisted document and
can repeat the source's financial effects. The runtime's explicit local-error
rollback boundary remains distinct from these source exception prefixes.

The Tally-first CommonJS entry exposes a real circuit defect: Clearing captures
Tally's in-progress top-level export. Assignment can proceed, but its first OTM
release throws `TallyMap.updateBalance is not a function`. This retained partial
export is not replaced by a working dependency. Funding's local Tally import has
different source behavior.

## Verification and scope

`tests/source_clearing_expiry.rs` executes the complete unchanged CommonJS module
graph used by the composed source oracle, including the actual Options, Margin,
Contract and Tally modules. Its 94 actions include 77 calls across all four entry
orders, mixed ITM/OTM/future/zero option positions, string series IDs, all exercise
directions, candidate selection, assignment reductions/flips/flats, nonfinite
inputs, actual insufficient Tally margin, database failures and source retries.
Expected outcomes are assertions; no fixture answers a Rust dependency.

The differential test compares outcomes, complete persisted collections, the
actual Tally cache, every loaded Margin instance, actual clearing-context objects,
and ordered database/UUID/random/clock effects. A separate complete replay test
retains an assignment and option deletion followed by a failed expiry delta,
restores from the independent pinned journal head, and continues the genuine
retry. Restore re-executes the commands rather than accepting supplied current
state or business outcomes.

This is the reached expiry/exercise graph. It does not establish option issuance,
trading, dynamically populated option indexes, vault custody, or full global
clearing completeness. Those require their own actual source owners and composed
execution evidence.
