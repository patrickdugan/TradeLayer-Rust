# Margin positions and source parity

`src/margin_map.rs` implements a stateful series-scoped margin map with caller-supplied contract parameters and collateral context. It does not debit the sole ledger or authenticate an address. The caller must atomically apply collateral-return plans and the resulting positions to authorized ledger/storage state. An IOU remains an unfunded claim.

The reference is the complete unmodified `src/marginMap.js` at JS commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Its LF-normalized SHA256 is `ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636`. The full source is archived in `fixtures/legacy_margin_map_source.txt`. The constructor, two static methods, and **43 active instance methods** were inventoried from the executed class, rather than counting commented-out implementations.

Run the source oracle without npm or a source checkout:

```text
node scripts/margin_map_vectors.cjs
```

Use `--source PATH` to verify the same pinned external file, and `--write` only to regenerate fixtures. The runner executes the original class in a Node VM with the archived BigNumber 9.3.1 implementation and actual archived `options.js`. Dependencies are deliberately explicit: an in-memory NeDB-shaped adapter, deterministic UUIDs, contract notional 1/collateral 2/initial-margin 2, and a tally adapter with available 10 and no margin shortfall. No adapter supplies missing method implementations or repairs the class. Both numerical/state fixtures and source-failure characterizations are checked for drift.

There are **246 source-executed calculation, position, option, query, mint, and ADL vectors**, plus **10 source characterizations** (including the active prototype-method inventory). Rust tests consume all 246 numerical/state vectors. Number-bridge regression vectors include large-size weighted averages, decimal-string contract counts at half-up boundaries, decimal quantities in ADL, malformed option strikes, and liquidation order amounts. Additional tests cover atomic match rollback, expiry ordering/open-interest counts, flat/IOU retention, Bech32 lookup rules, explicit claim allocation, maintenance checks, nonfinite rollback, and deleveraging resets. This is deterministic source evidence, not historical-chain acceptance.

## Implemented behavior

Positions hold signed contracts, margin, average price, realized/unrealized PnL, liquidation/bankruptcy prices, IOU claims, available collateral, old/last marks, new-block position size, and per-option subpositions. Position insertion order is retained separately from the lookup map. Stable ADL ranking ties therefore retain source input order.

| Source method | Rust behavior |
| --- | --- |
| constructor | `MarginMap::new(ContractId)`; empty positions, deltas, and expiry/OI indices |
| `getInstance`, `loadMarginMap` | Storage loading belongs to the caller; `write_position`/`merge_positions` hydrate typed snapshots. Rust does not claim to execute NeDB or its swallowed read errors. |
| `getAllPositions` | `positions()` exposes all typed positions in insertion order. Unlike the legacy projected objects it retains IOU/options/mark fields. |
| `getPositionForAddress` | Exact match first; lowercase BTC/LTC Bech32 prefixes enable an ASCII case-insensitive fallback. An uppercase query prefix does not enable it. Missing query returns a fresh flat position. |
| `writePositionToMap`, `saveMarginMap` | Local `write_position`/`merge_positions`; durable writes and storage error handling remain caller responsibilities. |
| `setInitialMargin` | Add and round 8 places, initialize a missing account, preserve contracts/PnL/average, emit the original delta shape. |
| `mergePositions` | Atomic replacement; optional flattening drops zero-contract/zero-IOU positions even if margin/options remain, as in source. |
| `updateContractBalancesWithMatch` | Apply both position trades atomically; a second-side error rolls back both positions and their deltas. This improves source failure atomicity. |
| `updateContractBalances` | Open, extend, partial/full close, or flip; caller-supplied close/flip quantities select average-price branches. Signed contracts change by the complete trade amount. A flat close clears average and sets zero liquidation/bankruptcy prices. |
| `updateAveragePrice` | Absolute-size weighted numerator divided by absolute new size; 20-place division then 8-place half-up. A fully closed position returns no average. |
| `calculateLiquidationPrice` | Source linear/inverse formulas, collateral minus positive uPNL, 1.005/0.995 multipliers, 4-place absolute outputs, and null threshold branches. Zero-denominator/nonfinite cases return explicit errors. |
| `calculateMarginRequirement` | Source signed 10% notional helper, including inverse division. |
| `computeMaintenanceMarginRequirement`, `isBelowMaintenanceMargin` | Absolute contracts × supplied initial-per-contract × 0.5, rounded 8 places; strict margin-only comparison. uPNL is ignored. Missing position yields no comparison. |
| `reduceMargin` | Return only margin above the remaining position's requirement; optional fee can leave margin below requirement, matching the source. A preexisting deficiency is a no-op. |
| `feeMarginReduce` | Subtract the fee, round 8 places; signed results are permitted by source behavior. |
| `realizePnl` | Linear/inverse PnL with explicit notional and buyer-side sign inversion. Retain source `abs(uPNL)-abs(realized)` adjustment and overwrite `realizedPNL`. |
| `recordMarginMapDelta` | Typed ordered `deltas()`/`take_deltas()`; source total/margin/average quirks retained, with series scope plus optional option ticker. UUID generation, DB insertion, and registry modification flags are caller-side storage work. |
| `applyIouClaimDelta` | `apply_legacy_iou_claim_delta` is an explicit no-op on stored positions, matching the actual source. `allocate_iou_claims` is an explicitly named repair described below. |
| `settlePNL` | Linear settlement intentionally omits notional and retains binary64 subtraction/multiplication; inverse uses notional and 20-place reciprocal division. Preserve `uPNL -= round(uPNL-pnl)`, rather than silently replacing it with a different accounting rule. Missing position returns an error. |
| `estimatePNL`, `calcPriceDelta` | Source pure PnL helpers; linear estimate omits notional while `price_delta` includes it. Explicit typed notional zero corresponds to source truthy decimal string `"0"`; callers supply 1 for a missing/default notional. |
| `applyOptionTrade` | Source binary64 signed quantity/average/margin calculations, closed/flip quantities, and actual reduction-average behavior. Retains the broken automatic expiry-index call as described below. |
| `_touchExpiryIndex` | Correctly argued `touch_expiry_index` is separately callable. OI counts nonzero account positions, not contract quantity. Expiry groups and tickers retain insertion order. |
| `getExpiringTickersUpTo`, `cleanupExpiredTickersUpTo` | Inclusive height filter and removal of index/OI records; removal does not settle positions. |
| `calcLiquidationWithOptions` | Long puts raise the long-side liquidation floor; long calls lower the short-side ceiling. Actual code ignores option expiry, current height, spot, and coverage ratio, including expired/dust protection. |
| `updateMargin` | Add the supplied amount, initialize a missing position, and emit the resulting total margin in the delta (source calls it `newMargin` but adds it). |
| `clear` | Add mark PnL, update old/last marks, preserve position fields, and emit the mark delta. |
| `generateLiquidationOrder` | Positive requested amount, nonflat position, and positive margin+available are required. Returns a typed market-liquidation plan bounded at the supplied bankruptcy price. Flags are represented by this specific type rather than mutable generic booleans. |
| `simpleDeleverage` | Opposite-side counterparties, excluding the liquidator; positive mark-to-mark winners preferred. If none exist, retain all opposite positions. Sort by absolute movement PnL then size, stably. Inverse ranking intentionally omits notional. Match remaining sizes; return margin-release and rounded pool-assignment plans. No insurance, funded IOU payout, or invented collateral credit occurs. |
| `adjustDeleveraging` | Clamp reduction to absolute position size, return margin above the remaining initial requirement, subtract a caller-supplied ledger shortfall from the requested return, and retain duplicate source `deleverage` deltas. Flat reset clears bankruptcy/uPNL but leaves `avgPrice` because the source writes a different `averagePrice` field. |
| `executeDeleveraging` | Concrete signed contract update, resetting liquidation/bankruptcy only when flat. Caller must provide validated direction/size. |
| `moveMarginAndContractsForMint` | Executable first-vault branch implemented, including source excess-margin decisions and `s-property-contract` identity. Repeated-vault branch returns `UnsupportedLegacyPath`. |
| `moveMarginAndContractsForRedeem` | Source is broken; returns `UnsupportedLegacyPath`, with no invented redemption settlement. |

## Source failures and explicit repairs

The VM records these failures from the unmodified class:

- First mint succeeds. Repeated mint reaches undefined `amount` while evaluating the nonexistent `updatedAvgPrice` call. The source mutates its referenced positions before throwing. Rust detects the unsupported branch before committing any changes.
- Redemption reaches undefined `notCovered` (or can fail earlier in other branches). It also refers to missing `updatedAvgPrice`, inconsistent `position.contract`, and inconsistent vault keys. A complete economic redemption requires a specified repair and historical acceptance evidence.
- `applyOptionTrade` calls `_touchExpiryIndex(this, before, after, fullTicker, meta)` with five arguments for a four-argument method. The effective metadata is the ticker string, so the helper immediately returns. Rust preserves automatic OI/index non-updates; a caller may explicitly invoke the correctly argued helper.
- `applyIouClaimDelta` reads `getAllPositions`, which omits existing `iouClaim`, mutates those discarded projected objects, and never writes them back. The literal compatibility method is a no-op. The clearly named repaired `allocate_iou_claims` updates actual positions, retains prior claims, and allocates only the positive-PnL shares of an imbalance. Its allocation budget is the positive imbalance truncated to eight decimal places. Buyer then seller receive their nominal half-up-rounded shares capped at the remaining budget, so independent rounding cannot create claims above the imbalance. Missing winners' shares, residual rounding dust, and sub-atom imbalance remain unallocated; callers can calculate residual as imbalance minus returned shares. Claims use exact decimal addition in this repaired path, avoiding a binary64 increase larger than its intended increment. It never moves funded collateral. This repair is not claimed as source-identical consensus behavior.
- `dynamicDeleverage` references undefined `ContractRegistry`/`TallyMap`, calls methods with missing arguments, and uses unqualified helper names. `calculateNetExposure` likewise references an undefined registry. These orchestration paths remain unimplemented; the concrete simple-ADL path is available with explicit caller context.
- `getMarketPrice` references undefined `ContractsRegistry`, `Oracles`, and `VolumeIndex`. Price selection must be supplied by the independently validated oracle/volume/registry integration.
- `needsLiquidation` treats a Map as `this.margins[contract.id]`; normal instances throw. Rust exposes the working margin/maintenance comparison instead.
- `settlePNL` dereferences `pos.avgPrice` before checking a missing position. Rust returns `MissingPosition` atomically.

Other deliberate typed repairs: a new `updateMargin` position has its actual address (source stores it at an undefined key); first-mint source delta incorrectly uses the property id as the sender address, while typed records identify the position changed; newly created vault positions initialize absent PnL/IOU fields to zero; two-sided trades and multi-position ADL roll back entirely on errors. Those repairs require protocol review before promotion as consensus transitions.

## Remaining source adapters and acceptance gates

The source `saveLiquidationOrders` is a DB upsert keyed by contract/height and overwrites an earlier same-height record. `recordContractTrade`/`saveTrade` write generic trade data under UUID keys. They belong to typed reporting/storage integration, not a second competing ledger inside this module. `fetchLiquidationVolume` is an unported diagnostic: its liquidation lookup expects an array from `findOne`, uses `contractCount` and `size` inconsistently, counts side quantities by adding zero, sums fill prices without quantity weighting, and scans all contracts' liquidation fills. Empty inputs yield nonfinite values. These source diagnostics are **not** silently presented as correct economic VWAP accounting.

Durable snapshot encoding, recovery, replay/undo, authorization, actual ledger debits/credits, integration with clearing/order matching/options expiry, cross-contract collateral/exposure routing, and representative historical acceptance vectors remain open. This module's local atomicity does not establish those gates.

## Numeric and resource boundaries

BigNumber operations use the shared finite decimal implementation: division at 20 decimal places with half-up rounding, then the source's explicit 8/4-place quantization. Source `.toNumber()`, `Math.abs` coercions, ADL signed-size addition, and option/linear-settlement arithmetic cross an explicit binary64 bridge. In particular, the weighted-average denominator is coerced through Number before BigNumber division. Decimal results represent that finite number's shortest Rust spelling. Rust/JavaScript shortest formatting agreement is exercised by these vectors, not proved for every binary64 input on every runtime. Nonfinite source outputs are explicit errors, rather than NaN/Infinity balances.

Typed identifiers exclude empty/undefined keys; string/number equality/coercion distinctions are resolved by typed caller arguments. Source safe-block support is bounded at 2^53−1 where a block is recorded. Local limits are 65,536 positions, 4,096 options/indexed tickers, 262,144 pending deltas, 256 bytes per ticker, and the shared Decimal bounds. They are resource limits rather than historical consensus rules. Errors in multi-step operations leave the original positions, indices, and deltas unchanged.
