# Pinned clearing funding execution

`source_clearing_funding_impl.rs` implements the funding graph from pinned
commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c` inside `ContractCore`.
It uses the source runtime's existing Tally authority, Margin instances,
persisted collections and environment adapters. It adds no balance store,
funding policy, mark cache or caller-supplied financial result.

The inspected source boundaries are:

| Archived module | Methods and source lines |
| --- | --- |
| `legacy_clearing_source.txt` | `applyFundingRates` 553, `calculateFundingRate` 580, `clampFundingRate` 661, `applyFundingToPositions` 670, `processFundingPayments` 727, active `getIndexPrice` 770, `saveFundingEvent` 3450, `loadFundingEvents` 3470 |
| `legacy_contract_registry_source.txt` | `getAllPerpContracts` 228 |
| `legacy_oracle_source.txt` | `getTWAP` 494 |
| `legacy_volume_index_source.txt` | `getVWAP` 522 |

The later `getIndexPrice` class definition is active. The earlier definition at
line 643 is shadowed and is not an alternate execution policy.

## Wiring and dependency ownership

The include exposes `source_clearing_funding_call(method, arguments)` for the
eight Clearing methods above. `clampFundingRate` is synchronous and returns a
BigNumber value; the others return the source's asynchronous outcomes.
`Contract.getAllPerpContracts`, `Oracle.getTWAP` and `VolumeIndex.getVWAP` route
to `source_funding_perp_contracts`, `source_funding_oracle_twap` and
`source_funding_volume_vwap` respectively.

Nested reads and writes use the existing exact namespaces and positional
arguments through `Core::call`: `Contract.*`, `Oracle.*`, `VolumeIndex.*`,
`Clearing.*`, `Tally.updateBalance` and `Clock.nowISOString`.
`Margin.getInstance([contractId])` returns the existing explicit instance
handle, followed by `Margin.getAllPositions([handle, contractId])`. The latter
genuinely loads a separate map, as the source does. Funding does not mutate the
Margin map or call `updateMargin`.

The active native index getter reads the notional/collateral pair, orders rows
by their nested `value.blockHeight`, and selects the latest qualifying row.
The oracle getter retains the actual `Oracle.getOraclePrice` return type and
its default-price behavior. Oracle TWAP uses equal weight per returned oracle
entry. Native VWAP reads `value.volume`, weights both pair orientations, and
retains the original range query and eight-place rounding.

Funding events upsert `_id: funding-<contractId>-<block>` with the source clock
ISO timestamp. Loading uses the original contract ID and inclusive block
range; results retain database order. No additional sorting or event replay
guard is introduced.

## Preserved source behavior

The hourly loop uses the original Number remainder test `block % 24 !== 0`.
Perpetual selection uses the source's strict numeric `expiryPeriod === 0` or
case-sensitive ticker `includes("PERP")`, and returns `doc.data.id`.
Funding calculations use 192 trailing blocks, the five-basis-point dead band,
division by eight, four decimal places, and the signed 12.5 cap.

Several financial behaviors are intentional fidelity requirements here:

- Payments affect Tally **available** balances. Margin balances and positions
  remain unchanged.
- Negative funding accumulates a negative `shortFunding`. Its negative total
  is passed unchanged to payment distribution, so the named payers receive an
  available credit and the receivers receive a debit.
- Payers and receivers normalize their contract counts independently. Each
  amount rounds independently to eight places; the source does not reconcile
  the two rounded totals. The regression demonstrates three zero-rounded
  payer debits followed by a one-satoshi receiver credit.
- An empty receiver side is checked after payer debits. A later shortfall or
  write failure retains the earlier successful effects.
- The native VWAP reader expects `value.volume`. A row containing only the
  source writer's `rawVolume`/`ltcVolume` produces NaN VWAP and the funding
  calculator returns zero. The reader is not repaired to consume another field.

BigNumber operations preserve bounded exact decimals, 20-place HALF_UP
division, the original explicit rounding points, signed zero, NaN and
infinities. Number conversion occurs at source conversion points. Valid
hexadecimal/binary/octal constructor strings use the existing verified source
numeric helper. The local BigNumber input-text bound is 512 bytes; inherited
decimal digit/scale limits remain explicit local unsupported-domain errors.
These are implementation resource bounds, not source transaction validity.

Source catches are preserved: calculation failure returns zero; TWAP/VWAP or
index lookup failure returns null; perpetual-list failure returns an empty
list; event-save failure is swallowed; event-load failure returns an empty
list. Position-loading and payment failures propagate with their earlier
database/cache prefix. A local capacity or unsupported numeric-domain error
remains distinct and uses the enclosing runtime's rollback boundary.

## Verification

`tests/source_clearing_funding.rs` executes unchanged complete archived modules
through the existing full CommonJS environment. Only the already-declared
storage, activation, clock, UUID, random and external boundaries are adapted.
It exposes the storage adapter and persisted rows for observation in memory;
it supplies no validation, payment, margin or funding result.

The focused test compares **104 observations / 81 method invocations**:
complete returned/thrown values, all persisted collections, actual Tally cache,
every independently loaded Margin instance, and ordered database/UUID/random
effects. Cases cover all four existing entry profiles, full positive/negative
and native funding loops, missing and invalid price data, caps/clamps and
signed zero, selector behavior, no-position and no-receiver paths, per-side
dust, shortfall/write prefixes, and caught event/read/factory failures.

The same test passes in an isolated Rust harness against both the archived
BigNumber 9.3.1 constructor and the hash-verified pinned-lock 9.1.2 constructor.
The 104 source observations are byte-identical between those constructors.
The repository's `scripts/bignumber_lock_reference.cjs` supports an explicit
lock-reference run using `TL_BIGNUMBER_LOCK_ROOT` and Node's `--require` option;
the archived fixture provenance is unchanged.

This evidence covers the implemented funding graph over actual runtime owners.
It does not establish parity for the remainder of global clearing,
liquidation, insurance orchestration, channel settlement or external storage
recovery. The source funding behavior remains separate from ConservingV1.
