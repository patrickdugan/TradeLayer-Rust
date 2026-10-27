# Literal synthetic lifecycle

The explicit `OptionLifecycleV4` policy composes pinned `Logic.mintSynthetic` and
`Logic.redeemSynthetic`, their two Margin instance methods, and the reached
Property/Vault methods. All balances remain in the runtime's existing Tally
authority. Margin objects use the existing identity arena; Property and Vault
metadata use the same Core collections and singleton caches. Earlier policies
retain their existing method graph, numeric adapter, and checkpoint bytes.

The source commit is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The implementation
uses complete archived Logic, Validity, Types, Property, Margin, Contracts, Vault,
Tally, Volume, and Oracle modules as its differential reference. No validation,
collateral movement, synthetic issuance, or redemption verdict is supplied.

## Observable source behavior

- A successful raw type 24 uses the actual decoder and validator. Its positive
  contract count is subtracted from the user's contracts, including an existing
  short. The source's sign assumption is preserved.
- A cold Property singleton saves only its current Map. The first mint can
  replace a persisted index with the synthetic entry, removing collateral
  metadata. A subsequent mint then throws while reading the missing ticker.
- With a warmed property index, a subsequent mint updates property supply and
  mutates the live user and vault margin objects before the source references
  undeclared `amount`. The Margin database remains at the last saved state.
  Its nonexistent `updatedAvgPrice` method is also preserved.
- Raw type 25 uses actual existence/balance validation. The normal redemption
  path changes the live user's contracts, then references undeclared
  `notCovered`. It does not silently complete or refund this prefix.
- Non-native direct redemption passes an un-awaited Oracle Promise to initial
  margin, producing BigNumber NaN. Its Oracle database operations interleave
  with the initial-margin Contract read. Oracle rejection is independent of
  the redemption's source exception. The complete Oracle filter/latest-height
  reduction runs even though its returned price is unused.
- Vault `exists` compares an un-awaited database Promise with null and returns
  true even without a matching document. `createVault` omits its save value;
  registration omits `vaultId`. Restart loading uses `findOneAsync` and ignores
  its ordinary object result. Failed initialization retains initialized empty
  Maps, so retry skips the loader.
- Vault updates preserve JavaScript string concatenation, subtraction and
  failure prefixes. Property circulation updates preserve the un-awaited
  database factory's `base.updateAsync is not a function` failure.

The V4 initial-margin helper retains BigNumber nonfinite arithmetic, negative
zero, inverse ROUND_CEIL and linear ROUND_HALF_UP. It does not replace earlier
policies' finite adapter. Unsupported bounded numeric/resource inputs remain
local errors and roll back the runtime; actual source exceptions retain their
observable prefixes.

## Evidence and boundary

The focused corpus currently contains 78 observations: 19 reset states and 59
original calls. It compares returned/thrown values and synchronous/Promise
transport, every collection row, the full Margin instance/object arena, the
Property singleton Map, both Vault Maps, the sole Tally cache, independent
unhandled rejections, and ordered database/UUID/clock effects. Scenarios cover
successful raw issuance, raw validation failures, both normal redemption
failure paths, metadata loss, live aliases, loader and late-save failure/retry,
property Promise failure, unawaited Oracle price/height coercion errors, and
initial-margin factory/read failure. Replay tests restore whole checkpoints
between operations and continue from the reconstructed state.

The reference loader supplies a recording database and the already declared
external clock/UUID/dependency boundary. It adds the unchanged pinned Vault
module and gives the recording console the native undefined return. It does
not alter source function bodies. Factory failure injection returns every
unaffected Promise unchanged. Raw wire amounts use the decoder's `1e8`
scale; the original encoder is called with those explicit wire units.

The corpus is run with archived genuine BigNumber 9.3.1 and the SHA-verified
9.1.2 lock reference using
[the portable preload](BIGNUMBER_LOCK_REFERENCE.md). Equality concerns this
tested corpus; it is not a claim of blanket version equivalence. These are
conditional source-execution results over explicitly supplied prestate and
commands, not actor authentication, chain proof, or a repaired conservation
model. Later unreachable source branches remain literal and are not forced
with supplied financial verdicts.

Both focused Rust tests run with each dependency version; all 78 reference
observations are byte-identical. Integrated Rust 1.90 check results are recorded
in `fixtures/reference_fidelity/source-option-lifecycle.json`.

The unawaited Oracle continuation records only its real Core factory/find
operations. It passes those returned rows and the original Oracle ID to a
transient, read-only `OracleSourceOwner` projection to reuse complete price
filtering and JavaScript coercion/reduction semantics. Its private read log and
state are discarded; they add no storage effects or persistent authority. The
existing Oracle owner's bounded row domain applies to that projection.

## Runtime integration

The include lives inside `source_contract_runtime.rs`. V4 adds
`vault_cache: Vec<(SourceValue, SourceValue)>` and `synthetic_token_cache` of the
same type, initialized empty. It reuses `vault_initialized`, `property_cache`,
the existing Margin arena, Tally and all shared collections. Its diagnostic
state is `syntheticLifecycle: source_synthetic_state()`; the Maps are undefined
until source initialization, and empty initialized Maps survive loader errors.

V4 dispatch routes `Logic.mintSynthetic`/`redeemSynthetic` to
`source_synthetic_logic`, the two Margin instance methods to
`source_synthetic_margin` with the existing instance handle first, and reached
`Vaults.*` plus `Synthetic.exists` to `source_synthetic_vault`. Property
`getInstance`/`addProperty`/`updateTotalInCirculation` use
`source_synthetic_property`. Existing property data/index reads stay with their
current source implementation. Property `getInstance` is synchronous.

V4 `Contract.getInitialMargin` routes to `source_synthetic_initial_margin`.
Raw 24/25 admissions call `source_synthetic_raw_logic` only after the real
Types/Validity result is valid. The raw helper maps the unchanged Logic
positional arguments and uses existing runtime invocation/queue ownership.
All mounts, new state fields and the V4 replay domain are policy gated.
