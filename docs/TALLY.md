# Tally projections and funded fee metadata

`src/tally.rs` completes the metadata and calculation slice of pinned `src/tally.js`. It owns no `Ledger`, account balances, collateral, insurance fund, or vesting supply. `CustodyAtoms` is a transient projection of the economic owner's actual available, reserved, margin, and channel custody. Its owned total excludes the separate vesting entitlement. Every funded mutation belongs to the single economic owner and its existing ledger.

Reference: JS commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, complete LF-normalized `fixtures/legacy_tally_source.txt`, SHA256 `cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef`. The shared complete `legacy_orderbook_source.txt` has SHA256 `c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98`.

## Functional boundary

`aggregate_custody` and `CustodyAtoms::sufficient` calculate checked atom totals and shortfalls from supplied projections. They retain no account map. `LegacyBuckets`, `LegacyTally`, and `source_*` helpers are finite historical observations; an observed partial update never modifies actual custody. Source decimal calculations use the shared bounded `Decimal`, including explicit binary64 bridges where JS calls `.toNumber()` or uses numeric addition. NaN, Infinity, object coercions, malformed identifiers, and missing data remain source characterization rather than spendable Rust values.

`calculate_match_fees` implements the unchanged orderbook method: rate 0.0005 (5bps), divided by ten only for channel `true`; inverse `rate * notional / lastMark * quantity`, linear `rate * lastMark / notional * quantity`. Division rounds to 20 decimal places, HALF_UP, before quantity multiplication. The absolute result is multiplied by 1e8, floored, then an odd integer is increased by one. With neither maker, both sides debit half, using the buy quantity. Otherwise the strict `buyMaker === false` selects the buyer as taker; the other branch selects the seller. A maker receives half the taker's debit as a rebate. Missing flags and both-maker flags retain the actual source branches. The `MatchFeePlan` explicitly separates debits, credits, and retained atoms. Its conservation equation is total debits = total rebates + retained fee. Zero and invalid division results do not manufacture a balance.

`FeeBook` classifies already-funded fees. A `FeeKey` identifies `(property, effective contract)`; spot shares contract key 1 with native contract 1. `FeeRoute` preserves the routing distinction:

| Route | VALUE | STASH | Insurance request |
| --- | ---: | ---: | ---: |
| Spot property other than 1 | all | 0 | 0 |
| Native contract | all | 0 | 0 |
| Spot property 1 | 0 | amount − floor(amount/2) | floor(amount/2) |
| Oracle contract | 0 | amount − floor(amount/2) | floor(amount/2) |

`accrue_atoms` returns a plan with `collection_atoms = amount_atoms`; the caller debits actual payers and funds the fee/insurance destinations in the same private transaction before publishing the new metadata. `queue_repaired` requires the caller to fund the full queued amount once. `flush_repaired` uses `collection_atoms = 0`; it only reclassifies pending fees and requests the insurance leg. Its pre/post custody includes every other queued entry sharing the key, including other blocks and the Spot/Native(1) collision. `draw_source_order` requests a debit from VALUE, or from STASH only if VALUE is zero and STASH is allowed; it preserves the source's absence of a partial VALUE top-up.

For each key, `expected_custody_atoms` is VALUE + STASH + all funded pending amounts. Insurance amounts are excluded after accrual. `verify_custody` checks actual caller escrow projections, rejecting duplicates and unexplained nonzero escrows. `rows`, `queued`, and `journal` provide immutable iteration; `snapshot` and `from_snapshot` export/import metadata for the economic owner's combined codec. Import checks canonical row order, bounds, positive pending amounts, route/key identity, queue uniqueness, arithmetic overflow, safe block range, and internally consistent journal math. The journal is informational and does not prove that balances were funded. The owner must independently validate contract route policy and reconcile expected metadata custody against its restored ledger.

Mutations clone metadata and commit only after all calculations, event limits, and validation succeed. An entire flush fails without removing any pending entries if a later event cannot be recorded. Bounds are 4,096 fee rows, 4,096 queued route entries, and 16,384 retained journal events; block heights are at most Number.MAX_SAFE_INTEGER. `take_journal` supports draining events into the owner's block journal. These are explicit implementation resource limits, not newly inferred historical consensus rules.

The crate-private `FeeBook::from_accruals` rebuilds ordered receipt metadata on a fresh local book and returns its checked insurance-request total. It reuses the identical accrual transition and retains complete row/journal equality, zero behavior, overflow and capacity checks; an error drops the unpublished local value. Avoiding a growing journal clone per receipt changes this reconstruction from quadratic copy traffic to a linear pass. Four private tests compare full public-accrual results and late invalid/overflow/capacity failures. An exploratory direct-source Rust 1.90 `-O` benchmark used identical build/return timing boundaries, full equality/checksums outside timing, and eight counterbalanced trials at 1,000/4,000/8,000 receipts for native and oracle routing. Native upper-median times were 3.190/116.308/1,408.885 ms for repeated public accrual and 0.115/0.236/0.858 ms for the private rebuild. These busy-host wall-time measurements isolate fee metadata reconstruction; they establish no end-to-end economic-owner, storage, or peak-memory speedup, and repeated codec/leg verification costs remain.

## Source behavior and deliberate repairs

The source counts available + reserved + margin + channel as owned balance; vesting is excluded. `calculateTotal` rounds each component to 8dp before summing, then converts to Number. `getTotalForProperty` instead sums raw decimal columns, skips an entire entry containing a NaN counted component, and returns BigNumber. `getTotalTally` separately sums stored amount and all five columns, then rounds each aggregate to 8dp and Number. Instance `totalTokens` sums numeric available and reserved with binary64 addition and excludes margin and channels. No one total can replace the other three for historical comparisons.

`updateBalance` converts nonnumeric changes through Number, then checks and writes available, reserve, margin, and vesting in that order. Stored buckets have no uniform 8dp rounding. Failure in a later bucket leaves earlier in-memory writes while the stored `amount` and database can remain stale. `source_update_projection` exposes this partial observation. Channel mutation adds its input directly as BigNumber and converts the result to Number; its separate observation helper preserves that order. Live custody mutations must use the economic owner's atomic transaction rather than publishing a source partial update.

Source direct accrual truncates to whole sats before dust handling and discards positive sub-one-sat input entirely. With fractional dust and at least one whole sat, `_saveDust` calls LevelDB-style `put` on the repository's NeDB-shaped database and throws. `source_fee_conversion` reports whole atoms and fractional sats; the fractions are observations, not funded assets. This slice does not release dust into an insurance account.

The source queue rounds inputs HALF_UP to 8dp, converts a missing contract into string `"1"`, and later sends that string to direct accrual. This erases spot routing: a two-sat property-1 spot fee, direct, requests one insurance sat and one STASH sat; queued, it credits two VALUE sats when contract 1 is native. `queue_repaired` deliberately retains `FeeRoute::Spot`, and its name distinguishes this repair from the captured source behavior. It accepts already-established integer atoms and does not silently apply the source's potentially value-increasing queue rounding. Source flushing commits earlier entries before a later failure, keeps the whole buffer, and duplicates the earlier fees on retry. `flush_repaired` commits atomically.

Other observed defects: constructor instance maps differ from the static maps used by most methods; missing tally documents silently reset the map while read failures leave stale maps; `saveToDB` swallows failed writes; a missing `await` makes recorded delta totals NaN (JSON null); a null property lookup throws despite an earlier ticker guard; numeric string property IDs return an Error object while zero/negative integer IDs pass; async DB acquisition is unawaited in address discovery so normal vesting execution finds no holders; source negative fee adjustments are allowed. A spot insurance exception prevents its STASH save, whereas an oracle insurance exception still saves STASH. The old concatenated fee-cache key collides between `(1,23)` and `(12,3)` and is ignored by the new property-prefix loader.

Source `feeCacheBuy` inserts an order, then pre-deducts all VALUE and STASH before matching. No-match restoration changes all former VALUE into STASH; a matching exception leaves the row deducted. This slice characterizes those orders and failures. Funded buyback must pass through the economic owner's actual orderbook, reserve/cancel rules, custody transfers, and joint transaction; there is no unfunded replacement buyback executor here.

## Complete source method map

The full archive contains 37 static methods, 8 instance methods, and the constructor. Every member was read. Delegation below is intentional: metadata support must not create another mutable ledger or a second startup/persistence owner.

| Source member | Rust disposition / concrete source behavior |
| --- | --- |
| constructor | Owner construction is explicit; source static/instance map split is characterized. |
| static getInstance | Caller owns lifetime; bootstrap/persistence restore the complete owner. Source reloads only static maps. |
| static setModFlag | Owner journal/transaction outcome supplies modification status; source sets an unawaited static flag. |
| instance verifyPropertyIds | Registry validation belongs to owner; source reads undefined `this.propertyIndex` despite loading a local variable. |
| static updateBalance | `source_update_projection` preserves finite arithmetic/partial-write observation; actual funds move through existing Ledger. |
| static updateChannelBalance | `source_update_channel_projection`; actual channel custody belongs to existing channel/ledger owner. |
| static getTotalForProperty | `source_total_for_property` for finite rows; `aggregate_custody` for actual atom projections. |
| static calculateTotal | `source_calculate_total`, per-component q8 then Number. |
| static setInitializationFlag | Bootstrap/persistence owner lifecycle; exact source DB flag roundtrip asserted. |
| static checkInitializationFlag | Same; absence yields false. |
| static getAddressBalances | Owner projects actual balances and registry data; source null-ticker throw characterized. |
| static getTotalTally | `source_total_tally`, separate stored amount/columns and final q8/Number. |
| static hasSufficientBalance | `CustodyAtoms::sufficient(Available)`; source rounds available shortfall to 8dp. |
| static hasSufficientReserve | `CustodyAtoms::sufficient(Reserved)`; source shortfall is Number without q8. |
| static hasSufficientMargin | `CustodyAtoms::sufficient(Margin)`; same source behavior as reserve. |
| static hasSufficientChannel | `CustodyAtoms::sufficient(Channel)`; source missing channel omits shortfall. |
| static saveToDB | Combined owner codec/durable transaction; source swallowed failure characterized. |
| static loadFromDB | Validated complete owner restore; source missing/reset and stale-on-error cases characterized. |
| static saveFeeCacheToDB | `FeeBook` metadata export, parent codec; old concatenated key collision characterized. |
| static loadFeeCacheFromDB | `FeeBook::from_snapshot`/`rows`; source parses floats, defaults falsey stash. |
| static loadFeeCacheForProperty | Immutable row iteration and checked totals; source new-prefix/legacy-key mismatch characterized. |
| static loadTallyBlobDirect | Persistence owner raw/validated checkpoint boundary; source absence returns null. |
| static resolveBlock | Explicit validated block argument; source TrackHeight || MaxProcessedHeight fallback and explicit zero captured. |
| static loadFeeRow | `FeeBook::row`; source absent row becomes zero. |
| static saveFeeRow | Checked unsigned metadata, snapshot export; source q8/Number and negative rows characterized. |
| static getFeeAccrualBuffer | `FeeBook::queued`; no mutable buffer escape. |
| static queueFeeAccrual | `queue_repaired`; source rounding/spot-identity loss captured separately. |
| static flushQueuedFeeAccruals | `flush_repaired`; source partial failure/retry duplication captured separately. |
| static flushAllQueuedFeeAccruals | Immutable ascending-block queue iteration, then explicit owner transactions; no implicit writes across blocks. |
| static updateFeeCache | Queue API; source returns immediately to queue, so the following old try body is unreachable. |
| static feeCacheBuy | Source no-match/failure/call order characterized; funded orderbook execution delegated to economic owner. |
| static accrueFee | `source_fee_conversion` + `FeeBook::accrue_atoms` + explicit insurance plan. |
| static adjustFeeCache | Owner applies funded legs and verified metadata plans; source arbitrary negative/value-creating adjustments are not exposed as live credits. |
| static drawOnFeeCache | `FeeBook::draw_source_order`; actual funds leave only through owner-applied plan. |
| instance applyDeltasSinceLastHeight | Persistence/joint replay replaces source undefined `currentBlockHeight`, missing DB get and missing applyDelta. |
| static recordTallyMapDelta | Explicit `FeeJournalEntry` and owner's balance journal; source unawaited tally produces null total. |
| static didReceiveClearingProfitThisBlock | Owner queries ordered economic events; source checks only `type === clearing`, without profit sign or amount. |
| instance applyDeltaToTallyMap | Owner transaction/replay; source supplies too few updateBalance arguments and ignores resulting rejected promise. |
| instance saveDeltaTodbInstance | Combined owner journal; source calls insert on an unawaited DB Promise and throws TypeError. |
| instance saveBlockDelta | Same unawaited Promise failure, replaced by owner durable block journal. |
| instance loadDeltasForBlock | Owner's actual block history; source method is empty and returns undefined. |
| instance totalTokens | `source_total_tokens` for finite numeric rows; actual owned custody uses all four atom buckets. |
| static getTally | Caller projects its actual owner ledger; source missing entry returns numeric zero. |
| instance getAddressBalances | Caller-owned projections; source reads the separate instance map. |
| static getAddressesWithBalanceForProperty | Owner filters actual projections; source missing await produces empty array on normal async DB API. |
| static applyVesting | Source missing-await no-op characterized; funded entitlement consumption belongs to vesting/economic owner, with explicit authorized supply. |

Top-level `toSatsDecimal`, `toSats`, `fromSats`, and `q8` map to shared Decimal arithmetic with explicit rounding; `getInsuranceModule` maps to an insurance transfer request in a plan, not an owned fund. `_loadDust`, `_saveDust`, and `_accumulateDust` retain the source DB mismatch in characterization. They have no unfunded Rust payout path.

## Reproducible evidence

`node scripts/tally_vectors.cjs --check` executes the complete unchanged tally class with deterministic async database, UUID, registry, property, insurance, and orderbook adapters. It extracts `calculateFee` verbatim from the complete hash-verified orderbook archive and executes it with the checked-in unmodified BigNumber 9.3.1 reference; there is no separately invented JavaScript fee formula. The database adapter preserves missing awaits and omits unsupported LevelDB get/put. Adapters journal dependency calls and inject I/O/insurance/match failures without contacting a node or spending funds.

`fixtures/tally_vectors.tsv` contains 248 actual-source numeric/state cases: per-component totals, raw property totals, aggregate tally totals, binary64 totalTokens, partial ordered balance writes, channel writes, 120 maker/inverse/channel fee cases, direct fee routing/dust cases, queued routing, and source VALUE-first draws. Rust tests consume every row. `tally_quirks.tsv` records separately asserted source ordering, partial failure, startup, query, and replay defects. The test suite also checks owned custody projections, odd fee conservation, coalesced/colliding queue routes, cross-block pending custody, no double collection, snapshot restore, malformed snapshot rejection, metadata bounds, and atomic rollback when an entire flush exhausts the journal. Parent validation supplies the complete combined runtime and persistence integration checks.

This module supplies functional metadata plans, immutable state export, and exact supported source arithmetic. Historical block reconciliation, authenticated authorization, durable combined ownership, insurance execution, actual buyback execution, and vesting issuance remain responsibilities of the existing/integrating owners. No claim of historical consensus equivalence follows from these finite fixtures alone.
