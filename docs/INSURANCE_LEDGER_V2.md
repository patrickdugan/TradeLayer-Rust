# Stage 25E2A: insured collateral is now real Ledger property

This document describes the Stage 25E2A sponsor-funded escrow and recovery
foundation. The later [Stage 25E2B closeout](INSURED_DISTRESSED_CLOSEOUT.md)
adds an atomic, fully funded synthetic whole-book termination on the same
sandbox Ledger. It does not execute market liquidation. It replaces the
insecure idea of trusting balances recorded in
the old JavaScript's separate `insurance.js` database. All development
and tests remain offline with synthetic actors; nothing is connected to
authenticated chain transactions or public markets.

## One ledger and one unspendable insurance reserve

Within each independent BTC/LTC/DOGE research network and specific
TradeLayer contract, `ClearingSandbox` still owns one `SpotExchange`,
which owns exactly **one token Ledger**.

The opt-in, immutable `InsurancePolicy { sponsor: AccountId }` is supplied
*before* any clearing event and validated against the reserved `~`
namespace. It does **not** authenticate a signature or a protocol mandate.

Stage 25E2A adds the deposit action:

```rust
deposit_insurance_unverified(external_id, sponsor, amount_atoms)
```

It requires the configured sponsor, a strictly positive amount, a
previously unused external event ID, and sufficient existing ordinary
available collateral. The insurance escrow account is deterministically
derived from:

```
"TradeLayer/insurance-reserve/stage25e2"
|| network.genesis
|| contract_id
|| collateral_property_id
|| sponsor_byte_length
|| sponsor_bytes
```

followed by SHA256d and the `~ins:` synthetic reserved-account prefix.
The sole Ledger records the deposit as an ordinary checked transfer from
the sponsor's available balance. Supply is preserved byte-for-byte,
and there is no secondary insurance balance table.

`SpotExchange::deposit_insurance_reserve_unverified` is a **crate-private,
one-way method**. It cannot debit the insurer, spend someone else's
`~clrm:` margin, draw from `~spot:` reserves or transfer to a client
account. Its validation does not weaken the existing `~clrm:` seam.
Normal token transfers and spot orders continue to reject the entire
reserved `~` address namespace.

There is no insurer withdrawal or standalone payout/claim API. The later
E2B closeout can debit actual insurance escrow only as part of its complete
distressed whole-book settlement. Market liquidation, AMM matching, bad-debt
allocation and ADL are outside this sandbox rule. The main Economic owner
has a separate pending-fee custody path; see the
[owner review](INSURANCE_OWNER_REVIEW.md).

Any downstream loss-budget projection still remains a read-only
observation. Multiple projections are *not* reservations on this fund;
simultaneous liabilities cannot count the same insurer twice.

## TLCL and TLCC v1 / v2 checkpoints

The previous version-1 formats preserve their exact bytes when the state
has no insurer policy:

- `TLCL\\x01`: complete spot/clearing state, **no insurance policy**.
- `TLCC\\x01`: complete mixed-block journal composed of v1 states.

A state with configured insurer policy uses:

- `TLCL\\x02`: adds the sponsor account to the versioned policy fields,
  prior to the persistent clearing-activity flag.
- `TLCC\\x02`: records the sponsor-bound TLCL v2 state at every retained
  rollback witness and the current tip.

Read APIs preserve the v1 signatures; the new
`from_snapshot_bytes_with_insurance`,
`from_checkpoint_bytes_with_insurance`, and
`load_checkpoint_with_insurance` demand the caller separately supply the
**expected insurance policy**. Neither format can infer trusted sponsorship
from its CRC. A v2 document can **never** be downgraded to v1 or loaded by
the old reader with a null insurance policy. A v1 document cannot conjure
insurance authority during recovery.

Even configuring an insurer with zero balance permanently sets the
existing clearing-activity fence. Old TLSP/TLSC/TLJP/TLJC snapshots
therefore cannot silently discard the configured insurer or a no-op
insurance history. New v2 readers independently check the sponsor,
genesis, contract, collateral property, permitted token list, frozen mark
and initial/maintenance policy, exact derived `~ins:` escrow identity,
other spot/margin reserves, total supply conservation, replay IDs and
canonical roundtrip encoding.

Both TLCL and TLCC retain a strict 64 MiB maximum and a CRC for detection
of accidental corruption. **A recomputed CRC is not a signature, and does
not establish ownership, consensus validity or economic solvency.**

## Block journal and fault model

The Stage25D `UnverifiedClearingBlock` now permits
`UnverifiedClearingEvent::InsuranceDeposit` in the synthetic transaction
sequence. It moves actual insurer property atoms and records one replay
marker only when the *entire* block succeeds. TLCC v2 includes both the
current reserve and all prior state witnesses.

A checkpoint file is written with immutable `create_new` and
`sync_all` before the in-memory mixed state can advance. A file collision,
missing directory, wrong sender, insufficient balance, invalid mark, or
late rejected transaction leaves all ordinary/spot/margin/insurer balances
and replay markers unchanged. On rollback, the insurer balance and
donation marker roll back together; replacement-fork replay can then use
the same synthetic ID in the new state.

This is tested with the existing deterministic fork-journal and by
v2-specific adversarial cases. Durable directory sync, hardware power-loss
assurance and real-chain authentication remain outside this prototype.

## Security exclusions and next milestone

The old JavaScript `insurance.js` stores a separate balance array and
applies withdrawals asynchronously. None of those external DB balances
are trusted by the Rust insurer. Historical payout and liquidation rules
remain unratified, including funding sign, mark notional, rounding,
market-fill priority, ADL allocation and maintenance treatment.

The later **Stage 25E2B** preflights every whole-book obligation, uses each
reserve atom once, credits winners only from actual losing-margin and
insurer accounts, closes equal/opposite positions and commits atomically
with the existing versioned sandbox checkpoint. Insufficient total reserve
rejects the entire closeout. It accepts neither a client-provided payout
amount nor a CRC as claim authority. Historical market-fill priority,
partial closeouts, fund-kind lifecycle, independent durable freshness and
authenticated settlement still require the main owner integration.

Type 3/23/31 settlement is still default-deny. No real funds, testnet,
mainnet write, node, wallet, or additional Rust crate is involved.

## Reproducible offline validation

```sh
python3 scripts/check_rust_supply_chain.py
cargo fmt --all -- --check
cargo clippy --all-targets --frozen -- -D warnings
cargo test --frozen --test insurance_ledger_v2
cargo test --frozen --test clearing_snapshot --test clearing_chain
cargo test --frozen
cargo build --all-targets --release --frozen
```
