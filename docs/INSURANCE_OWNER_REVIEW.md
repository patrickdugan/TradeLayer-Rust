# Insurance mechanisms: actual funded-owner review

This review independently read the complete pinned insurance.js and the actual
Rust owners. The externally reported49 Node observations and their IDs below
are **report labels only**: no three-file worker diff or49-case corpus was
available, imported, executed or counted here. No new insurance allocator,
payout policy, source oracle corpus or production change is supplied.

Pinned insurance.js: JS commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`,13,585 LF bytes, SHA256
`5abbe7934ca4c2f4180ed910989ae8f90d752bf9e17cb8944b8b49e33d03c39c`.
The exact blob is archived in `fixtures/insurance_owner_review/insurance.js`. This audit never edits
the six reserved insurance port files or the worker's additional paths.

The reviewed implementation baseline is `386a8ea` (Rust baseline `70dd911`).
The parent independently verified the pinned Git blob, all reviewed Rust
hashes and reran the 32 existing checks using the exact executables named
in the successful parent Cargo log. Reproducibility metadata and fresh logs
are in `fixtures/insurance_owner_review/manifest.json`.

## Outcome

No corresponding **new funded Rust owner defect** was reproduced. The actual
insured sandbox uses one fixed collateral property, real escrow debits,
atomic clone/check/swap, persistent replay IDs and independent restore policy.
Its supported synthetic full-book closeout is different from historical source
liquidation. The main EconomicState has only real pending-insurance fee custody;
it does not yet expose insurance payout, withdrawal, liquidate-to-admin or fund
snapshot import. Those missing capabilities cannot be counted as an integrated
insurance lifecycle merely because the source metadata port passes vectors.

There is no integrated `src/insurance.rs` in this checkout at review time. Its
absence is a real integration gate, not evidence that source insurance bugs
were automatically fixed by a same-named Rust port.

## Mechanism matrix

| External report label | Pinned source mechanism | Actual funded Rust protection | Classification / remaining gate |
| --- | --- | --- | --- |
| R06 duplicate payout | calcPayout mutates balances, inserts a payout key and saves; it has no prior-claim replay guard, input txid or lookup of an already-applied claim (`insurance.js:199–265`). NeDB's normal unique `_id` does not imply uniqueness of this separate `key`. | ClearingSandbox.guard_event checks its outer seen IDs, Ledger IDs and spot IDs; transaction publishes the ID only after all transfers and verify. Full-book closeout clears positions and advances mark. Same-ID retries reject before any leg; a fresh ID alone does not recreate closed positions. | Protected for supported synthetic closeout and restored replay history. Authentication/canonical derivation of event identity and full historical insurance claim identity remain external/unimplemented. A caller choosing another ID is not independently authenticated. |
| E31 committed snapshot + error / restart replay | Source event insertion, secondary event insertion, snapshot read/write and return happen separately; there is no single transaction or poisoned owner. A write can persist before an error/retry (`insurance.js:245–265`,105–120). | Sandbox transitions stage all balances/positions/replay together. DurableEconomicChain checks exact expected EconomicHead, commits a complete checkpoint into one Journal value, publishes cached memory only after commit, and denies cached reads after any uncertain write. Reopen checks independently expected sequence, commitment and tip. | Protected by actual DurableEconomicChain/Journal for its existing economic custody. The standalone ClearingChain TLCC writer is a narrower immutable-file experiment, not that durable owner; no insurance payout lifecycle is yet inside the durable main owner. |
| E18 cross-property payout | calcPayout totals every balance and debits them in insertion order, but takes no loss/collateral property argument (`insurance.js:203–231`). | Sandbox has one immutable collateral. Its insurance escrow binds collateral+network+contract+sponsor; payout planning reads only this collateral, and verify rejects any foreign-property balance in insurer/margin reserved accounts. Private closeout legs pass the owner's fixed collateral. | Protected for that one-collateral sandbox. Multiasset funds, explicit per-asset liabilities and cross-asset conversion are missing capabilities; no conversion authorization may be inferred from source aggregation. |
| E33 nominal total30 but tally credit510 | deposit stores decimal strings; liquidate reduces using JS `+`, then divides. Strings `'10'`/`'20'` can concatenate into`'01020'`, yielding510. It credits `contractId` as the token property, rather than iterating properties (`insurance.js:60–69,153–166`). | Rust escrow amounts are u128 atoms; payer/receiver totals use checked additions. Closeout requires losing-margin debit+insurer debit equal creditor credit and the actual mark receiver total. Every leg is a Ledger transfer, not an independent credit. | Protected for the admitted integer-atom transition. There is no liquidate-to-admin implementation and no arbitrary contractID→propertyID conversion. |
| E34 repeat credit without reserve debit | liquidate credits Tally but never decrements any snapshot balance. It then calls missing InsuranceFund.updateFeeCache and catches the error after the credit (`insurance.js:142–169`). | Sandbox transfers actual insurer atoms to real registered margin escrows. A later liability sees the reduced balance. Insufficient insurance rejects the whole staged transition. Normal transfers/orders cannot spend the reserved insurer account. | Protected for synthetic insured closeout. Admin liquidation, insurer redemption, fee-cache allocation and their independent authorization remain absent. |
| E35 oracle fund loads ordinary fund snapshot | loadFromSnapshot/saveSnapshot include `-oracle`; getSnapshot ignores it, and liquidate calls getSnapshot (`insurance.js:30–41,105–127,142–149`). | No source insurance snapshot importer exists. Economic expected config binds FeeRoute::Oracle/Native, contract and collateral, and reconstructs exact fees/insurance allocation. Its pending balance is not a loaded InsuranceFund. | Missing lifecycle capability, not a claim that oracle/native fund separation exists in Sandbox. Sandbox InsurancePolicy contains sponsor only and has no fund-kind field. A future fund must explicitly bind kind in its ID, escrow domain, receipts and independently expected restore config. |
| E49 precision loss | withdraw uses JS subtraction and calcPayout writes `.toNumber()` balances/payouts after decimal rounding (`insurance.js:94–101,222–265`); binary64 cannot represent all8dp atom balances above2^53 atoms. | Ledger balances/supply and insurer reserve are u128; closeout has checked integer/i128 PnL and exact integer transfers. Snapshot atoms encode fixed16-byte values. No f64 or Number conversion appears in these transfer/closeout/snapshot paths. Economic Decimal→atom conversion is checked; source-validity projections explicitly reject an unsupported binary64 domain. | No float-loss mechanism in those custody paths on source review. No new large-value regression was added solely to mirror the implementation; supported bounds/overflow still require checked errors. Economic/public display strings are distinct from custody. |

## Actual owner paths and invariants

ClearingSandbox retains the sole Ledger inside SpotExchange. The public
InsurancePolicy is immutable sponsor metadata, not signature authority.

- `with_insurance_policy_unverified`: can opt in only before clearing activity;
  even a zero-balance policy sets the permanent combined-history fence.
- `deposit_insurance_unverified(external, sponsor, amount_atoms)`: requires the
  configured normal sponsor and a positive amount. It transfers existing
  collateral from that sponsor to the derived`~ins:` account; there is no mint.
- `derived_insurance_escrow` hashes the domain, network genesis, contract,
  collateral, sponsor length and sponsor. `internal_txid` also binds network,
  contract, collateral, external ID, purpose and leg number.
- `plan_distressed_closeout`: recomputes mark and maintenance risk, rejects
  rounding/open-interest imbalance, totals actual losing margins, computes
  shortfalls, caps them against the current insurer reserve and creates
  deterministic positive transfer edges to creditors. It never accepts a
  precomputed caller payout amount as sufficient proof.
- `settle_distressed_closeout_unverified`: stages every losing-margin/insurer
  transfer, clears synthetic positions, advances mark, records the external ID
  and runs verify before publishing. Deliberate faults before either leg and
  after the final leg preserve the old owner. Creditor margin can later be
  withdrawn only through the existing flat-position checked path.
- `verify`: conserves Ledger supply, reconciles spot reserves, validates the
  unique expected insurer/margin escrows and fixed collateral, checks balanced
  signed positions, maintenance and retained history fences. There is no
  public mutable Ledger reference or generic Tally-credit callback.

Exact implementation locations: `src/clearing_sandbox.rs:433–469,580–690,
1080–1158`; `src/clearing_distress_impl.rs:62–278`;
`src/spot.rs:313–391`; `src/clearing_snapshot_impl.rs:103–346`.

This is an offline synthetic whole-book closeout. It is not the current
Economic liquidation/AMM-first/IOU owner, not a market-priority/liquidator
permission, and not an authorized implementation of source InsuranceFund.
Do not combine two live Ledgers by running both owners against the same funds.

## Main Economic owner and durable boundary

Economic ordinary/forced fills collect actual fee atoms, route exact configured
insurance allocations from the fee escrow to`~econ:...insurance-pending`, and
increase a checked counter (`src/economic.rs:449–457` and
`src/economic_liquidation_impl.rs:492–503`). Verify requires the entire economic
escrow map exactly equal metadata, including the fixed collateral and pending
insurance amount (`src/economic.rs:897–942`). It independently rebuilds FeeBook
and total insurance from retained ordinary/forced trade receipts using the
configured route (`src/economic.rs:971–999` and
`src/economic_liquidation_impl.rs:644–696`). An unexplained balance or fabricated
counter is not sufficient to restore a valid owner.

Loss collection and IOU payout in this owner use its real loss pool. They do
not draw pending-insurance custody. Unfunded loss remains recorded, not created
as spendable insurer assets. `service_view` labels the exposed amount
`economic_insurance_custody` and rejects insurance payout-history queries as
Unavailable (`src/service_view.rs:857–863,956–958`). Oracle liquidation requests
in the standalone Oracle module are metadata requests, not a private insurance
transfer capability.

DurableEconomicChain stores one complete economic-chain checkpoint as one
Journal mutation, with its real Ledger, current state, rollback witnesses and
replay history. The original owner holds the OS lock. Every mutation checks an
expected head; the durable publish point is after write+sync. Any uncertain
write poisons the Journal before I/O and blocks subsequent reads/writes. An
error after a synced frame therefore cannot expose old cached state as healthy;
recovery must reopen and reconcile the independently expected head
(`src/economic_chain.rs:383–473`; `src/storage.rs:391–479`).

By contrast, `ClearingChainState` TLCC snapshots use immutable create_new files
and independent network/anchor/policy expectations. They persist complete
states and replay IDs and support rollback, but are **not** a locked append-only
insurance journal with a freshness head. Their load API has no independent
expected current head, and they are not authenticated full event replays. CRC
and canonical encoding detect corruption, not a maliciously reauthored but
conserving economic state. A later production insurance handoff must use the
real durable owner and independent freshness/authentication, not silently treat
an arbitrary old TLCC file as the latest insurer state. Windows power-loss
directory-entry guarantees remain an explicit storage limitation.

There is no atomic transaction across an external insurance DB and the clearing
namespace. The current correct pending counter does not by itself establish
that such a handoff happened, nor can it authorize a second namespace to credit
another ledger after a retry.

## Executed evidence and limits

No new failing actual-owner regression was warranted: the reported source bugs
did not reproduce in the admitted owner operations. Rather than duplicate the
external source corpus or add implementation-mirroring tests, this audit reran
the existing relevant funded-owner targets, preserving their exact assertions:

| Executed evidence | Pass count | Retained log |
| --- | ---: | --- |
| tests/insurance_ledger_v2.rs | 9 | [log](../fixtures/insurance_owner_review/insurance_ledger_v2.log) |
| tests/insured_distressed_closeout.rs | 12 | [log](../fixtures/insurance_owner_review/insured_distressed_closeout.log) |
| Existing compiled economic_chain target | 9 | [log](../fixtures/insurance_owner_review/economic_chain.log) |
| Actual lost-ack poisoned-original-owner unit regression | 1 | [log](../fixtures/insurance_owner_review/poisoned_owner.log) |
| Actual Journal real-write-error poison/reopen unit regression | 1 | [log](../fixtures/insurance_owner_review/journal_poison.log) |

Total: **32 existing actual-owner test executions passed**, not 32 newly
added tests and not the external 49 cases. The parent reran all five targets
from the exact binaries recorded by the passing `70dd911` Cargo gate.
All reviewed Rust files remain byte-identical at the audit baseline.
No Cargo compilation or full-suite rerun was necessary for this documentation
and source-evidence change. The fresh logs and binary hashes are retained in
`fixtures/insurance_owner_review/`; this focused rerun does not increase the
1,200-execution full-suite baseline.

These checks cover replay, exact donor debit, sponsor/policy binding, version
downgrade denial, late-failure atomicity, insufficient reserve, no reserve reuse,
actual same-property creditor payment, snapshot restore, reorg replacement,
durable stale-head rejection, lock exclusion and poisoned-read denial. They do
not prove authenticated actor/chain provenance, the missing full insurance
lifecycle, external49 source parity, multiasset/oracle fund import or ADL.

## Concrete missing capabilities and review obligations

1. A single accepted owner must integrate the source fund's lifecycle with
   actual custody; source metadata alone cannot credit the Ledger. Define the
   ratified payout/liquidation policy and origin of every credited atom.
2. Bind network/genesis, contract, collateral, **native/oracle fund kind**,
   policy/sponsor authority, input/claim identity and independent expected
   durable head. Do not use contractId as a token property or merge asset pools.
3. Persist fund balances, claim/event replay, paid amounts, actual reserve debit,
   fee-cache allocation and all semantic receipts in the same atomic owner.
   A unique DB key or a returned error after save is not enough.
4. Specify the pending-fee→fund handoff, rollback and lost-ack recovery before
   exposing Economic pending custody as a live insurance fund. Avoid a second
   Ledger and avoid claiming cross-namespace atomicity that storage lacks.
5. Keep integer/decimal atom conversions exact and checked; do not convert
   stored reserves to Number for mutation. Retain scalar source observations
   separately from a conserving policy, including the source's failure order.
6. Read the historical sandbox stages together with
   [insured closeout](INSURED_DISTRESSED_CLOSEOUT.md). The earlier deposit
   documentation now identifies the later funded closeout explicitly. The
   old Stage 25C source header remains historical documentation debt; it is
   not a custody defect or permission to promote the sandbox to live liquidation.

No reserved production files were edited, no new economic policy was selected,
and the local evidence commit does not publish or promote the implementation.
