# IOU local checkpoints

`IouStore::snapshot_bytes` / `from_snapshot_bytes` use the new **TLIO v1** format.
`FundedIouLedger::snapshot_bytes` / `from_snapshot_bytes` use the new **TLFI v1**
format. They are local prototype formats; neither reads the legacy JavaScript
database nor claims compatibility with historical TradeLayer state.

Restore the combined state with the independently configured network and atom
policy, for example:

```rust
let restored = FundedIouLedger::from_snapshot_bytes(
    &bytes,
    TradeLayerMainnet::Bitcoin,
    PayoutAtomPolicy::Exact,
)?;
```

The encoded network tag, genesis hash and payout policy must all agree with
those expected values. A checkpoint cannot silently reinterpret fractional
source allocations using a different atom policy or switch network identities.
The network assertion is still caller supplied; restoration does not establish
that an imported balance, transaction, loss or match was authorized on-chain.

## Complete state

TLIO contains every bucket's key, amount, last block, block-start amount,
delta, loss counter and profit counter; every claim map's key, block, and exact
claim sequence; and the total claim count. Canonical array-index claimant keys
are in ascending numeric order; ordinary strings retain source insertion order.
Signed and zero claims, negative bucket balances, zero bucket records, empty
claim maps, and block zero remain representable. Source bucket persistence is
limited to twenty decimal places; imported claims retain the complete supported
decimal precision.

TLFI embeds one **complete** `Ledger` checkpoint and one TLIO checkpoint, then
persists escrow ownership, all outer replay IDs and the last accepted height.
The existing TLL v1/v2/v3 ledger codec retains its complete token balances,
properties, channel columns, pending withdrawals, pending and swept net claims,
king-settlement receipts, and ledger replay IDs. It is not reconstructed from
available balances or restricted to ledgers without channel liabilities.

Successful source no-op payouts still consume their outer input ID and advance
the accepted height. These survive restart independently of ledger transfer
IDs. Source loss-pool counters survive even when a funded payout exhausts an
escrow. An owned escrow remains recorded after its available balance becomes
zero. Receipt results are returned by operations rather than stored in a
receipt journal; replay from a restored pre-operation checkpoint reproduces
their source amount, credited atom count and signed rounding delta.

## Canonical decoding and bounds

All integers use fixed-width big-endian encoding. Keys and replay IDs are
strictly ascending, with no duplicates. Decimal text is the exact canonical
plain `Decimal::to_string()` representation, length prefixed with `u32`; aliases
such as `01`, `-0`, exponents and extra trailing fractional zeros are rejected.
Accounts are length-prefixed with `u8` and obey the existing account domain.
The IOU reserved prototype names remain excluded from claimants.

The shared codec enforces a **64 MiB total checkpoint limit** and CRC32. Each
embedded checkpoint has its own CRC. Counts are checked before record parsing
and no claim vector capacity is reserved from an untrusted count. Decimal
lengths are checked before parsing or copying, followed by the independent
4096-digit/4096-scale decimal limits. IOU records, claims and funded replay IDs
retain their existing 4096, 16384 and 10000 limits. Writers check the output bound
before every append. Restore rejects trailing data, invalid versions, invalid
claim ordering, inconsistent claim counts, invalid heights, invalid escrow
ownership or foreign internal balances, and overlap between outer and ledger
replay domains. Full conservation and byte-for-byte canonical re-encoding are
required before the restored state is returned.

The standalone arithmetic store has a separate serialization bound: legal
combinations of 16,384 maximum-precision claims can exceed 64 MiB and return a
bounded serialization error. The funded owner instead requires successful
combined serialization on import and before every mutation commits. It rejects
uncheckpointable states, including a ledger operation that would cross an
embedded journal limit, while preserving the previous state and replay IDs.
`tests/iou_checkpoint_review.rs` reproduces both boundary cases.

CRC32 detects accidental corruption, **not authentication**. An attacker can
recompute it after forging otherwise internally consistent economic state.
Deployment must separately protect and authenticate the chosen checkpoint.

## Validation

`tests/iou_snapshot.rs` covers a funded accrue/collect/payout cycle with restarts,
outer and internal replay fences, no-op replay and height fences, fractional
payout metadata, exact parent checkpoint restoration and branch reapplication,
signed/zero source records and source claimant ordering, all supported network
and policy identities, and imported channel/pending/settled liabilities.
Adversarial cases include every truncated prefix, checksum damage, trailing
data with valid CRC, decimal aliases and oversized lengths, duplicate and
unordered claim keys and replay IDs, inconsistent counts and heights, forged
escrow ownership, removed ownership records, nested checksum damage and the
64 MiB bound. These checks establish local codec behavior; historical acceptance
and authenticated chain replay remain separate work.
