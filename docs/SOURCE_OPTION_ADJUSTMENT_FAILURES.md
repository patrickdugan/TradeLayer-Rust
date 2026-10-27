# Literal V4 option adjustment failures

The pin is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The active
`Clearing.computeOptionAdjustments` is at `clearing.js:3330–3391` and is reached
by loss settlement at line 1865. The source's `Margin.loadMarginMap` catches
find/JSON failures and returns undefined (`marginMap.js:24–58`); the subsequent
`.margins` access must throw a source TypeError.

The `source_clearing_mark_settlement_impl.rs` include changes only this helper
under `OptionLifecycleV4`. Primary and alternate Margin receivers preserve that
TypeError. Main option quantities and alternate option merging use actual
property access: null legs throw instead of becoming zero quantities. A falsey
current leg still receives the source default object before merging. Object
spread/entries retain representable Array and ASCII string fields. Parsed NaN
strikes follow the source's `strike || 0` fallback.

Earlier policies keep the committed helper behavior, including its two known
defects. There is no new state field, caller financial verdict, or balance
authority. Unsupported values retain existing bounded local-error rollback;
source exceptions retain prior reads, live caches, Tally/IOU writes and contexts.

`tests/source_option_adjustment_failures.rs` loads the unchanged complete source
modules and runs actual Margin, Tally, Oracle, Orderbook, IOU and clearing calls.
Its declared storage/clock/UUID boundaries supply no financial method results.
Promise observers and factory fault injection return unaffected source Promises
unchanged. A constructor observer defines the source's identical own `seriesId`
data property and records the actual new receiver, including allocations before
a caught JSON error. It changes no method body or result. Prototype descriptor
inspection is outside this tested graph. Direct helper scenes and three-actor
composed loss scenes compare
outcomes, all stored collections, actual Margin instances, Tally, mark/context
caches, cross-cache reference identity and ordered database/UUID effects.
Later-actor failures follow an actual
earlier loss debit and IOU write. Retried commands continue from that prefix;
replay compares those same source effects and restores complete checkpoints
between observations. A separate test
retains the old V1/V2/V3 method outcomes and local rollback behavior.

The corpus has 73 observations: 22 reset states and 51 actual calls. All three
tests pass with genuine BigNumber 9.3.1 and lockfile 9.1.2; the reference outputs
are byte-identical. The existing 135-observation on-chain contract graph also
runs under both V2 and V4 with each BigNumber version. Integrated Rust 1.90
results are recorded in `fixtures/reference_fidelity/source-option-lifecycle.json`.

This is conditional source execution over explicit prestate and commands; it
supplies neither actor authentication nor a repaired economic policy.
