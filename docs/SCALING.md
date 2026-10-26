# Scaling settlement lifecycle

`scaling.rs` provides an offline, bounded channel-settlement owner. It registers bilateral channel metadata and typed trades, advances pending-trade expiry, acknowledges keep-alives, records close-offset plans, consumes net references, and sweeps a range into a checked King custody plan. Every mutation stages the entire owner and publishes only after the transition and encoded-size checks pass.

The module reads the existing `Ledger` immutably. It creates no balance store and executes no collateral transfer. `process_king_settle_unverified` checks the Ledger's actual channel owners and available escrow, converts the absolute token amount to exact 10^-8 atoms, and returns the before/after transfer plan. The caller must connect that plan to one authenticated, atomic channel owner before it represents a paid settlement. A metadata `Swept` status records a planned sweep; it does not prove that the plan was executed. The same Ledger supplied repeatedly can yield repeated plans against unchanged escrow. There is no cross-owner atomicity or reservation in this module.

`ChannelDescriptor.root` is an asserted founding reference. It is deliberately separate from the existing Ledger's pending-claim settlement root. This module does not authorize wire types 23/31, verify signatures or multisig participation, prove a founding UTXO, rotate a UTXO, verify an oracle mark, or authenticate a trade's contract definition. The existing Joint authorization gates remain in force.

## Pinned source and evidence

The complete unchanged `src/scaling.js` from commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c` is archived as `fixtures/legacy_scaling_source.txt`, with LF SHA256 `ea2103b70dd051d39e44c9201a1eed2107b8110ac9803a0b825b92942799ca61`.

The full source executes in `scripts/scaling_vectors.cjs`. All 18 active methods are exercised. The 167 committed observations include 136 exact binary64 PnL cases, ordered validation reasons, future-ending King ranges, expiry-zero truthiness, unknown trade acceptance, global cross-channel lookups, metadata-only close/net behavior, source overdraft, and the no-op clear helper. PnL comparisons use exact resulting float bits, not rounded display values.

The oracle embeds the complete immutable MIT-licensed **BigNumber 9.1.2** implementation, compressed with its license retained. Its SHA256 is `20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`. The pinned scaling source imports this version but performs its PnL arithmetic with native JavaScript Number operations; it never calls BigNumber.

```text
node scripts/scaling_vectors.cjs --check
node scripts/scaling_vectors.cjs --check --source /path/to/pinned/src/scaling.js
node scripts/scaling_vectors.cjs --check --nedb /path/to/installed/nedb
```

The default oracle uses a narrowly described in-memory recording adapter for the DB queries/operators called by scaling. It is not a NeDB implementation or a backend parity claim. The optional genuine NeDB probe was also run during implementation: updating `trades.$.status` throws `Field names cannot begin with the $ character` and leaves the matching trade unchanged. The adapter exposes the same observed failure and the source call sequence. No RPC or filesystem-backed database is used.

## Source behavior and explicit lifecycle repairs

The source's PnL is `abs((mark - entry) * contracts)`. If A is the seller, a negative price difference produces `BtoA`; if A is the buyer, a positive difference produces `BtoA`. Every other case, including zero PnL, produces `AtoB`. The function ignores inverse-contract and notional metadata. Rust preserves that binary64 formula for finite inputs and rejects nonfinite intermediate results.

The source validators are separately callable as `validate_settle_channel_pnl` and `validate_king_settle`. Their `ValidationContext` is an explicit caller claim. Within the typed field domain, their ordered early exits and accumulated reasons follow the source. In particular, an unpublished trade can pass keep-alive validation, an expiry of zero is ignored, and King validation permits `blockEnd > currentBlock`. A pending-range query scans settlements, trades, then net settlements in insertion order; expired rows are included and duplicate transaction IDs across those categories remain separate rows.

The admitted lifecycle makes these named repairs:

| Concern | Admitted behavior |
| --- | --- |
| Source has no trade registration method | Explicit registration records a positive finite entry price, positive finite quantity, and expiry at or after registration. |
| Source has no expiry driver | A successful action at a later height expires only pending trades when `height > expiry`. Live trades remain live. Failed actions do not advance time or expire rows. |
| Source lookups and keep-alive-chain checks scan every channel | Query helpers retain global lookup behavior; actual mutations require references in the selected channel. |
| Source positional `$` writes fail on genuine NeDB | Typed transitions update concrete rows directly; no successful backend execution is inferred. |
| Unknown keep-alive can still record a source settlement | Admission requires a known pending/live trade and rejects expired, neutralized, or swept trades. |
| Close writes metadata but executes no offset or margin update | Rust returns a typed `OffsetPlan`, consumes the matching live trade/keep-alive, and records the close. It executes no trade or PnL payment. |
| Source neutralization touches only settlement rows | The low-level helper retains that scope. Close/net admission additionally consumes referenced trades to prevent reuse. |
| Source accepts replay and unchecked King amounts/ranges | Transaction IDs are globally unique across registered trades, settlements, and Kings. King admission requires a completed nonempty range, the declared founding reference, matching Ledger owners, exact atomic amount, and sufficient escrow. |
| Source assumes activation/signatures elsewhere | Admitted actions require independently configured activation at an earlier block. All entry points remain explicitly unverified. |

The source King mutates floating channel columns and can make them negative; it does not prove its root/count, bind a net amount to historical PnL, or rotate any UTXO. Rust checks affordability of the proposed amount but still does **not** prove that amount is the economically correct net. `neutralized_count` is the source query's row count, not a proof of distinct funded claims. Zero-amount plans are allowed if the range has pending rows. Negative finite net amounts use their absolute value, as the source does. Subatom amounts and values outside u128 atoms fail rather than round silently.

`clear_l2_state` retains all arrays, matching the source's logging-only helper. Explicit `neutralize_settlement_unverified` and `neutralize_settlements_in_range_unverified` expose the source helper operations without authorizing money movement. Standalone record helpers are private; only successful admitted transitions append public settlement/King records.

The typed surface accepts printable ASCII `AccountId`s, 32-byte transaction IDs, nonzero property IDs, finite Numbers, booleans, and integer heights/times at most 2^53-1. It does not reproduce JavaScript coercion of null, strings, arbitrary objects, malformed identifiers, or arbitrary extra fields spread from `trade.params`. `OffsetPlan` preserves the concrete entry/quantity/mark/PnL/reference fields used here. Validators return a result rather than modifying the caller's parameter object; source-only mutations such as defaulting a missing `totalContracts` to zero are represented by the caller's explicit typed field.

## Method coverage

| Active JavaScript method | Rust surface |
| --- | --- |
| `validateSettleChannelPNL`, `validateKingSettle` | Separate source-characterization validators |
| `settleChannelPNL` | `settle_channel_pnl_unverified` |
| `processKeepAlive`, `processClosePosition`, `processNetSettle` | Concrete private transitions reached by the settlement dispatcher |
| `processKingSettle` | `process_king_settle_unverified` with immutable Ledger witness |
| `calculatePnL` | `calculate_pnl` |
| `isSettlementNeutralized`, `neutralizeSettlement` | Query and explicit unverified neutralization helper |
| `neutralizeSettlementsInRange`, `getPendingSettlementsInRange` | Range mutation/query with the source category ordering |
| `getTradeStatus`, `getTradeByTxid`, `validateKeepAliveChain` | Global source query helpers |
| `recordSettlement`, `recordKingSettlement` | Private append operations after successful admission |
| `clearL2State` | `clear_l2_state`, retaining metadata |

## Checkpoint, bounds and recovery

`checkpoint_bytes` writes a canonical `TLSL` version-1 envelope containing the activation configuration, every typed action and historical custody witness, and the complete derived channel projection. The projection includes all trades, statuses, keep-alives, offsets, net rows, settlement timestamps, King requests/counts/plans, and the current height. Float fields preserve their finite binary64 bits, including signed-zero input metadata. A CRC detects accidental corruption; it does not authenticate any input or historical custody witness.

`from_checkpoint_bytes(bytes, &expected_config)` requires independently expected activations, bounds the envelope and action count before decoding, validates fixed-width fields and bounded account lengths, and replays the entire history through the actual lifecycle. The untrusted projection is compared as a borrowed byte slice with the replayed projection; it is never parsed into a second unverified state or used to allocate nested rows. The result must encode back to the exact original bytes. No caller can set replay bypass flags or install opaque imported channel state.

Limits are 128 channels, 4,096 actions, 16,384 composed records (actions, channels, and every row), and an 8 MiB checkpoint. Admission conservatively reserves five records for a new action before cloning. History is retained completely; capacity returns an error without eviction. Replay and per-action owner cloning favor auditability at this bounded prototype scale and are not a throughput claim. Restore does not create files or offer crash-safe disk writes. A containing chain owner can retain prior checkpoints to roll back and resume deterministically; this module does not bind itself to a network, tip, or durable journal.

Tests compare the source formulas/reasons, exercise the repaired lifecycle with real channel escrow, show the Ledger remains unchanged, and check failure atomicity, replay, exact atom limits, prior-block activation, finite/time/history bounds, repaired-CRC projection/action tampering, independent configuration mismatch, truncation, and rollback/resume.
