# Funded IOU settlement

`iou_ledger::FundedIouLedger` connects the existing source-compatible IOU store
to one owned `Ledger`. An unfunded match profit creates claim metadata; it
does not credit tokens. Collecting a loss transfers actual tokens from the
supplied payer into a network/contract/property-specific escrow and records the
same quantity in the source bucket. Paying claims transfers from that escrow
to recipients. Claims and buckets are accounting metadata, never a second
token balance. Every operation clones, executes, verifies and commits the
whole state, including replay markers and height, or changes nothing.

This supplies the funding that the standalone `iou` module deliberately lacks.
It is a caller-driven local adapter. It does not authenticate a match, actor,
loss, property definition, chain order or current mark. Actual trade/clearing
admission and margin-map integration remain outside it.

## Source operations and explicit adapter rules

The positive-PnL path follows pinned `orderbook.js`: sum buyer and seller PnL,
allocate the positive net imbalance using `addIouClaims`, then call `addProfit`.
The caller must establish that the trade qualifies for this path. Negative or
zero net PnL is rejected by this particular entry point; a real collected loss
uses `collect_loss_unverified` instead. Both source calculations and their
20-place persistence rounding remain in the existing IOU implementation.

`pay_unverified` calls the existing `payOutstandingIous` port. It retains its
current-block gate, source claimant ordering, proportional calculation,
claim reductions and unchanged block-loss pool. The source clearing routine
then credits allocations through `TallyMap.updateBalance` without passing a
funding account. This adapter instead requires matching escrow debits. Reusing
the unchanged source pool cannot mint tokens: an empty escrow rejects a second
payout and restores the tentative claim reductions. A genuinely new collection
can fund a later payout.

The source tally's `updateBalance` converts changes and balances through
binary64; that method has no universal eight-place quantization. The Rust
ledger represents integer atoms at eight decimal places. Consequently every
book requires an explicit, immutable local `PayoutAtomPolicy`:

| Policy | Decimal-to-atom rule |
| --- | --- |
| `Exact` | Reject a fractional atom and preserve the full state. |
| `Down` | Truncate each positive allocation toward zero; unused atoms stay in escrow. |
| `HalfUp` | Round each allocation to the nearest atom, ties upward. |

These policies are not claimed historical consensus rules. Every receipt
retains the source allocation, credited atoms and signed rounding difference.
Source claim reductions use the unrounded allocation, even when the selected
policy credits zero atoms. A receipt therefore exposes any claim/credit dust;
it must not be hidden as exact source tally parity. Adoption in consensus needs
the actual historical numeric/dust specification.

Total atom credits may exceed neither the caller's budget nor the source
current-block loss pool, and may not exceed actual escrow funds. An independent
review found that checking only the caller's budget would allow two half-atom
shares to spend two atoms against a one-atom current pool when older funds
remained. The additional pool check and regression reject that case atomically.
This is an explicit constraint on the adapter's rounding bridge.

## Replay and ownership

External event IDs and ledger-generated leg IDs are checked for replay. Even
an empty or zero-credit payout records its external event. Internal legs use
separate domains containing network genesis, outer event, contract, property,
purpose and ordinal. Escrow identities similarly bind genesis and the asset
pair. A Bitcoin book rejects a Litecoin-tagged operation before mutation.

All `~` account names are reserved for internal use. Constructor imports reject
existing reserved balances without their ownership metadata; payers and
claimants cannot masquerade as internal accounts. Full conservation is verified
after each staged operation, and all reserved balances must belong to the
book's derived escrows in the correct property. Public access to the ledger
and IOU state is immutable.

Construction and every staged commit must also serialize successfully through
the complete combined checkpoint. Independent per-record limits do not ensure
that all records fit its total byte cap; the embedded ledger also limits its
checkpoint journals. `UncheckpointableState` rejects such an import or rolls
back the entire operation before it can create a state that loses restart
recovery. This prototype serializes for that check on every mutation, in
addition to cloning, and makes no production mutation-throughput claim.

Heights cannot regress within an instance and must fit the source JS-safe
integer domain. There are at most 10,000 external inputs, in addition to the
existing IOU record/claim bounds. These are local resource limits, not verified
historical limits. Imported network and preexisting claims are caller
assertions. Import is distinct from restoring a replay-preserving combined
checkpoint; source database migration and canonical-chain selection remain open.

## Verification

```text
cargo test --test iou_ledger --frozen
node scripts/iou_vectors.cjs --check
```

Thirteen integration tests exercise a complete match-profit/real-loss/payout
cycle, partial budgets, source proportions and block gates, repeated pool use,
actual exhaustion, atom policies, rounding dust, source-pool bounds, failed
debits/accruals, reserved recipients, cross-network/key identities and replay.
They assert whole-state equality after failures and unchanged total supply.
The existing actual-JS oracle still establishes the standalone source
calculation behavior. Neither test set proves on-chain authorization or the
choice of a historical atom policy.
