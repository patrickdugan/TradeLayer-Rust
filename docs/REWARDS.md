# Maker reward accounting (`src/rewards.rs`)

## Scope: accounting only

This module is an in-memory, integer-only calculator. It takes declared epoch
statistics and returns how a reward budget would be split among makers.

- It is **not** wired to the matcher, to the ledger, or to the registry.
- It mints, moves, and escrows **no token**. It produces **no payout transaction**.
- It is **not chain consensus**. Its inputs (`resting_notional_seconds`,
  `filled_notional_atoms`, `fees_paid_atoms`, `attested`, the budgets) are
  supplied by a trusted caller and are not verified here.
- **Legal caveat:** paying tokens as rewards for derivatives trading raises
  regulatory questions that depend on jurisdiction and product. This design
  must not go live without review by counsel.

## Inputs and outputs

`plan_epoch_rewards(params, fee_pool_atoms, emission_budget_atoms, makers)`

- `RewardParams`: `fee_share_bps` (<= 10_000), `max_account_share_bps` (1..=10_000),
  `fee_rebate_cap_bps` (1..=10_000), `require_attestation`. Degenerate values are
  rejected before any allocation.
- `MakerEpochStats` per maker: `resting_notional_seconds`, `filled_notional_atoms`,
  `fees_paid_atoms`, `attested`. These are declared inputs; the calculator does
  not verify that an entry represents maker activity. Only supplied eligible
  entries can be paid.
- Output `RewardPlan { payouts, retained_atoms }`. `payouts` lists nonzero totals
  sorted by `AccountId`. `sum(payouts) + retained_atoms` equals the budget exactly.
  Duplicate `AccountId`s are rejected.

## Rules

1. Budget: `fee_budget = floor(fee_pool_atoms * fee_share_bps / 10_000)`, and
   `budget = fee_budget + emission_budget_atoms` (checked).
2. Score: `score_i = resting_notional_seconds_i` if `filled_notional_atoms_i > 0`
   and (`attested_i` or not `require_attestation`), otherwise `0`.
3. Two independent pools, each split pro-rata to score:
   `raw_i = floor(pool * score_i / total_score)` (zero if `total_score == 0`),
   using exact 256-bit intermediate arithmetic.
4. Per-account cap: `payout_i <= floor(pool * max_account_share_bps / 10_000)`,
   applied to each pool. The excess is **retained**, not redistributed in the
   same pass.
5. Fee-funded pool only: `fee_payout_i <= floor(fees_paid_i * fee_rebate_cap_bps / 10_000)`.
   Because `fee_rebate_cap_bps <= 10_000`, this never exceeds the account's
   declared fee input. The calculator does not subtract maker rebates, fee
   refunds, transfers or other reimbursements from that input.
6. Floor remainders are **retained**. The function returns `retained_atoms` and
   never over-distributes.
7. Order independence: inputs are sorted by `AccountId` before any computation,
   and no `HashMap` iteration is used.
8. Overflow: all arithmetic is checked. Overflow returns an error and never panics.

## Anti-sybil analysis (what is and is not guaranteed)

The per-account cap is keyed on `AccountId`. It is therefore a limit on one
identity, not on one entity. These claims are argued below and tested in
`tests/rewards_accounting.rs`.

**(a) Splitting an entity across identities cannot raise its uncapped payout.**
With `max_account_share_bps = 10_000` and the fee rebate cap in force, splitting
score, fills, and fees across identities gives a total payout `<=` the combined
account's payout. This holds because
`floor(x) + floor(y) <= floor(x + y)`, `min(x_i, y_i) summed <= min(sum x_i, sum y_i)`,
and removing a zero-fill identity lowers the entity's share, since
`s / (O + s)` is increasing in `s`. Equality is **not** guaranteed; floors can
lose atoms. Test: `splitting_without_binding_cap_never_increases_payout_exact_example`
(5 becomes 4), plus 20,000 seeded random splits.

**(b) The per-account cap does not resist splitting.** Once the cap binds on the
combined account, splitting into identities can earn up to `k * cap`. Example:
pool 1000, cap 500, entity score 300 vs another account at 100. Combined, the
entity earns 500. Split 150/150, it earns 750. Test:
`per_account_cap_is_defeated_by_identity_splitting`. Attestation is a supplied
boolean/allowlist fact here; it does not establish one identity per economic
entity. Neither the cap nor this calculator prevents identity splitting.

**(c) The fee-funded aggregate is bounded per entity, even across identities.**
For any set of identities controlled by one entity, the sum of fee-funded
payouts is `<= fee_rebate_cap_bps / 10_000 * (sum of fees those identities paid)`.
This follows from `sum floor(f_j * r) <= floor(f * r)`. Test:
`fee_funded_aggregate_is_bounded_by_fees_across_identities` and the seeded
random test. This is an arithmetic aggregate bound on the declared inputs.
It does not authenticate those amounts, identify common control, prove their
economic cost, or establish sybil resistance.

**(d) Gross fee bounds do not establish wash-trading unprofitability.** The
calculation bounds rewards by the supplied `fees_paid_atoms` value. If that is
a gross charged fee, maker rebates, fee refunds and any other reimbursements
can make actual unreimbursed cost smaller. For example, charges100, maker
rebates80 and a50 reward satisfy a50% gross-fee cap while reward minus actual
net cost is positive30. The public-API test
`gross_fee_cap_does_not_prove_a_loss_after_maker_rebates` retains this example.

A nonpositive fee-reward-minus-cost conclusion is conditional on independently
establishing the entity's net unreimbursed cost, including maker rebates and
all other fee compensation across every identity it controls. If the caller
chooses to supply that independently netted nonnegative cost consistently as
the fee input, the mathematical aggregate cap applies to it. The calculator
does not compute or verify that netting, attribution or completeness. The
companion public-API test supplies net cost20 and obtains reward10 under the
same unchanged50% rule. This clarifies a trust condition, not a new payout or
netting policy. Emissions remain outside the fee-input cap.

## Residual risks (not mitigated by this module)

- **Emission-funded payouts are not fee-capped.** A zero-fee account that
  satisfies the fill and attestation gates still earns its emission share
  (`residual_emission_pool_is_not_fee_capped`). Wash trading can therefore farm the
  emission pool. Mitigation requires a separate design, such as capping emission
  payouts by fees or by independently verified maker flow. This is **not** solved
  here.
- **The filled-notional gate is self-reported.** A same-operator wash trade
  satisfies `filled > 0`. This module trusts the caller's statistics.
- **Attestation is external.** `attested` must come from the registry's active
  attestation for the chosen list, and the module does not check that.
- **Identity limits are not enforced here.** See (b).
- **No token, no custody, no settlement.** Everything above is accounting.

## Local integration candidate and evidence

This candidate imports PR153 at exact head
`3a1cfc4eeffb7dcf73ece1b120a912f8848ff294`. Its standalone public API and
production arithmetic remain unchanged. The review corrected the randomized
test's incomplete low-half remainder predicate: `(2^127 * 2) / 2` with an
incorrect quotient0 passed that predicate. The replacement checks the complete
256-bit subtraction, including high-half borrow and a zero high remainder.

The1072 checked-in `fixtures/rewards_wide_vectors.tsv` cases are generated by
Python arbitrary-size multiplication and division, independently of Rust's
64-bit-limb multiply/restoring divider. They check full products, exact
quotients/remainders, zero-divisor refusals and quotient overflow. The original
thirty independent review vectors remain the prefix. Regenerate/check with
`python scripts/rewards_wide_vectors.py --check`.

The original PR reported7 unit and19 integration tests. Local standalone
Rust1.90 evidence and exact source/provider hashes accompany the integration
manifest. These checks use the actual immutable Core AccountId provider and
the real candidate reward module, not a replacement ledger. Parent still owns
current-Core/all-target checks and integration; no matcher, consensus rule,
settlement path, economic parameter choice or payout is enabled here.

## Current local integration

The current TradeLayer-Rust integration passes 9 unit and 21 public-API tests
on Rust 1.90. The independent Python oracle reproduces all 1,072 full-width
vectors. All-target Clippy with warnings denied, formatting and transaction
inventory checks pass. This is scoped validation; full-release promotion is
recorded separately. No payout or consensus authority is added by this module.
