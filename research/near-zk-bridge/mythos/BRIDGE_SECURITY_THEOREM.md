# Bridge security theorem (ZTB, BRIDGE-SEC-02)

Status 2026-11-24. Research. The accounting and authorization layer is machine-checked in
[lean/BRIDGE_SECURITY.lean](lean/BRIDGE_SECURITY.lean) (core Lean 4.35, no Mathlib, axioms
`propext` and `Quot.sound` only, no `sorry`). Everything below the accounting layer (NEAR
consensus, Nova soundness, hash functions, Bitcoin chainwork, contract code identity) enters as
named assumptions. Those assumptions are the remaining proof obligations, listed in section 6.

Four properties are kept separate throughout:

| Layer | Question | Where it is discharged |
|---|---|---|
| Accounting correctness | Given authentic events, do the books balance? | Lean Theorem 1, 2 (done) |
| Source consensus authentication | Did the event happen on the finalized source chain? | A1a, A3: NEARHEAD01, BTC anchor (unbuilt) |
| Proof soundness | Does an accepted proof imply the relation? | A1b: Nova key + circuit (unbuilt for NEAR) |
| Custody authorization | Can reserves leave by any path other than the payout relation? | A4: vault code + account keys + token issuer |
| Production readiness | All of the above plus liveness, cost, audit | not claimed |

## 1. Objects

Fix one asset domain `dom` (ZTB02_ABI: NEAR genesis, Bitcoin genesis, activation, policy, token,
vault, property). All quantities are integer atoms of the pinned NEP-141 token.

- **Finalized source history** `S_t`: the multiset of events `e` such that, at time `t`, the
  transfer -> receiver -> resolver receipt graph of `e` is in a NEAR block final under NEAR
  consensus, executed by the pinned token and vault code, with terminal resolver status success.
  `amt(e)` is the **economically retained** amount: `requested - unused`, where `unused` is the
  receiver's returned value and the resolver's `used` equals it (NEARPAY01-A resolver corner case).
- **Event identity** `id(e) = (H(dom, resolverReceiptId, 0), H(dom, candidateSeq))`. Both
  components are nullifiers. Identities are independent of relayer, signer, verifier, key and
  checkpoint epochs.
- **Consumed set** `C_t`: identities consumed by TL admission. Persistent across every upgrade.
- **Authenticated cumulative deposits** `D_t = sum_{e in C_t} amt(e)`.
- **Issued liabilities** `O_t`: TL atoms in balances, UTXO-bound outpoints, and every reserved
  column (orders, margin, channels, fees, insurance), counted once. `U_t` credited but unminted.
- **Pending withdrawals** `Q_t`: burned on TL, not yet paid (Pending or InFlight).
- **Completed payouts** `R_t`: payouts confirmed by the vault's own callback for the exact attempt.
- **Actual reserves** `V_t`: the vault's token balance in the token contract's state.
- **Gifts** `G_t`: tokens transferred to the vault outside the protocol (never credited).

## 2. Transitions (the model in Lean `exec`)

| Op | Guard (secure) | Effect |
|---|---|---|
| srcDeposit e | none (source chain, not adversary) | `S += e`, `V += amt e` |
| gift x | none | `G += x`, `V += x` |
| admit e | **A1** `e in S` and **A2** `e not in C` | `C += e`, `D += amt e`, `U += amt e` |
| mint a | `a <= U` | `U -= a`, `O += a` |
| burn a | `a <= O` | `O -= a`, `Q += a` |
| pay a | `a <= Q`, `a <= V` | `Q -= a`, `R += a`, `V -= a` |
| reorg e | refused under **A3** | `S -= e`, `V -= amt e` |
| adminWithdraw x | refused under **A4** | `V -= x` |
| migrate | under **A5** keeps `C` | else `C := []` |

TL-internal trading, clearing and UTXO import/export redistribute `O` without changing it. That
is the existing `TradeLayerFormal` conservation result (`every_reachable_state_conserves`),
imported here as the statement "internal transitions preserve O". It holds for the abstract
TL ledger, not yet for hybrid outpoints with non-protocol spends (audit finding M1).

## 3. Invariant and theorem

`Inv(s)`:
1. `Fresh(C)`: no identity consumed twice.
2. `C subset S`: every consumed identity is a finalized retained source deposit.
3. `D = sum_{e in C} amt(e)`.
4. `U + O + Q + R = D` (liability accounting).
5. `V + R = sum_{e in S} amt(e) + G` (vault conservation: reserves move only by deposit, gift, pay).

**Lemma 1 (consumption bound, `consumed_sum_le`).** If `Fresh(C)` and `C subset S` then
`sum_C amt <= sum_S amt`. `S` may contain repeats; nothing about `S` is assumed.

**Lemma 2 (step preservation, `exec_inv`).** Under A1..A5 every enabled transition preserves `Inv`.

**Lemma 3 (`credited_le_source`).** `D <= sum_S amt`. **Lemma 4 (`paid_le_credited`).** `R <= D`.

**Theorem 1 (reserve conservation, `secure_solvent`).** Under A1..A5, every state reachable from
the empty state satisfies `U + O + Q <= V`.

**Corollary (`surplus_exact`).** `V + D = U + O + Q + sum_S amt + G`. The surplus
`V - (U+O+Q)` equals gifts plus finalized deposits never credited (unclaimed or censored).

**Theorem 2 (exactly-once payout, `at_most_once`).** For one exit under P1 (callback carries the
true promise result of the exact current attempt) and P2 (no retry of an in-flight attempt
without an authenticated failure), across any interleaving of attempts, failures, stale and
replayed callbacks, the number of external transfers executed is at most 1, and `Paid` implies
exactly 1 (`paid_implies_sent`).

**Theorem 3 (containment, `window_loss_bound`).** If each admission window credits at most `cap`,
then over `k` windows at most `k * cap` is credited, whatever the adversary controls. This bounds
the loss when A1 fails undetected; it prevents nothing.

### When reserve conservation follows inductively

Exactly when every value-affecting transition is one of the rows above, each guarded as stated.
Concretely the induction needs:
- admission enabled only by membership in `S` (A1) and freshness in `C` (A2);
- `S` monotone (A3);
- `V` decreased only by `pay`, and `pay` only against `Q` (A4);
- `C` monotone across upgrades (A5);
- TL-internal transitions conserve `O` (imported `TradeLayerFormal` result);
- `pay` debits `Q` exactly once per exit (Theorem 2, needs P1, P2).

Drop any one and the induction breaks at a specific step. The counterexamples are checked terms.

## 4. Minimality: one counterexample per assumption

All are `by decide` theorems in Lean section 4, calibrated to BRIDGE-SEC-01 where relevant.

| Dropped | Lean theorem | Trace | Result |
|---|---|---|---|
| A1 auth | `c1_no_auth_insolvent`, `c1_reserve_905` | honest 5,213,905; admit phantom 331 (5,213,000); mint; burn; pay | `V = 905`, `O = 5,213,905` |
| A2 nullify | `c2_no_nullifier_insolvent` | admit event 7 twice; mint 200 | `O = 200 > V = 100` |
| A3 finality | `c3_no_finality_insolvent` | admit 7; mint 100; reorg 7 | `O = 100 > V = 0` |
| A4 custody | `c4_no_custody_insolvent` | honest setup; admin withdraws 5,213,000 | `V = 905` |
| A5 continuity | `c5_no_continuity_insolvent` | admit 7; migrate (reset); admit 7; mint 200 | `O = 200 > V = 100` |
| P2 no timeout | `p2_timeout_double_pay` | begin; transfer ok; timeout retry; begin; transfer ok | `sent = 2` |
| P1 auth callback | `p1_forged_double_pay` | begin; transfer ok; forged failure; begin; transfer ok | `sent = 2` |

Each trace is also shown to be refused by the secure system (`*_secure_rejects`). So, at this
abstraction, {A1, A2, A3, A4, A5} is a minimal sufficient set: sufficient by Theorem 1, and each
member necessary by its counterexample. The executable benchmark BRIDGE-SEC-02 reproduces X1 (A1
at the consensus layer), X2 (A4), X3 (A3 at the Bitcoin anchor), X4 (A5) against concrete
cryptography.

## 5. The central question

> Can compromising every operational bridge key affect availability only?

**Yes, conditionally.** Define operational keys = relayers, provers, RPC providers, the TL
`protocol_admin`, bridge operators, and any vault administrator. They can be reduced to
availability-only exactly when none of them can affect A1..A5:

| Assumption | What makes operational keys irrelevant to it | Current status |
|---|---|---|
| A1 | Admission accepts only a proof of the V2 relation against a NEAR head the light client finalized from a pinned genesis/checkpoint by >2/3 stake; the program/key/z0 are pinned by the immutable domain | Not built. BRIDGE-SEC-01 source table is a trusted fixture; type 36 pins no z0 (finding C2); admin registers programs (C3) |
| A2 | Nullifiers consumed in consensus state, checked in-circuit for batches | Partly: model + deposit map; no statement-level nullifier in type 36 (H1) |
| A3 | NEAR finality from consensus signatures; Bitcoin anchor K-deep with honest header supply | NEAR: unbuilt. BTC: SPV only, relayer-fed on NEAR (H4) |
| A4 | Vault account has no full-access keys; code upgrade only via the constrained mechanism; no admin transfer method | vault_mock has none; deployed account key policy unspecified (C4) |
| A5 | Upgrades commit to the old consumed-set root and light-client head, after a delay; emergency power is pause-only | Not specified in TL registry (C3, H6) |

What remains outside reach of the key holders but is still assumed:
- **NEAR consensus**: at most 1/3 Byzantine stake in each epoch the light client traverses, AND
  the light client is advanced at least once per unbonding period (otherwise former validators
  with withdrawn stake can sign a long-range fork without slashing exposure). The second clause
  couples liveness to safety: if every relayer is compromised AND the update function is
  permissioned, safety degrades. It must therefore be permissionless (finding H3).
- **Bitcoin**: no reorg deeper than K after a payout, and at least one honest party feeds headers
  to the NEAR verifier within the challenge window (otherwise the attack cost falls to privately
  mining K+1 blocks; finding H4).
- **Cryptography**: Nova (Pallas/Vesta ppsnark IPA) knowledge soundness for the registered key; the
  key is the reproducible compilation of the reviewed circuit; SHA-256 collision resistance;
  Ed25519 unforgeability (NEAR approvals).
- **Token issuer**: the NEP-141 contract (for USDC, issuer-upgradable and with freeze powers) does
  not freeze, seize or rewrite the vault balance. This is a custody assumption no bridge design
  removes; it can only be contained (quarantine, haircut accounting).
- **TL consensus and TLSTATE01**: the reverse path needs a proof that the TL state transition is
  complete and valid. That proof does not exist; BRIDGE-SEC-02 uses a trusted fixture there.

Removing any of these yields a counterexample in section 4 or the benchmark.

## 6. Remaining proof obligations

| # | Obligation | Discharges | Notes |
|---|---|---|---|
| PO1 | NEARHEAD01: from pinned checkpoint, each accepted header carries approvals of >2/3 stake of its epoch's set, epoch hand-off via committed next-set hash; exact nearcore light-client block format | A1, A3 (NEAR) | BRIDGE-SEC-02 fixture LC is the native shape; nearcore format not modeled |
| PO2 | NEARPAY01 A+B composed: outcome + receipt-body + data-edge authentication under PO1 head | A1 | relation in NOVA_STATEMENT_V2 section 3 |
| PO3 | Circuit = relation: reviewed circuit; key derived reproducibly; z0 pinned to checkpoint state | A1 | type-36 z0 pin missing (C2) |
| PO4 | Nova knowledge soundness at the registered parameters | A1 | inherited assumption |
| PO5 | Nullifier persistence in TL consensus state, including in-batch uniqueness | A2 | statement-level nullifiers (H1) |
| PO6 | Vault code identity: no transfer path except payout relation; account access keys locked | A4 | NEAR deployment policy |
| PO7 | Upgrade continuity: new verifier state commits old consumed root and LC head | A5 | mechanism in audit section 5 |
| PO8 | TLSTATE01 completeness/validity for burns and exits; BTC anchor K-deep | reverse path | trusted fixture today |
| PO9 | Refinement: Rust/NEAR code refines the Lean `exec` (or Kani harnesses per transition) | all | not started |
| PO10 | Payout P1: callback result authentic (NEAR private callback + promise result parsing); recovery proof for paid-but-unrecorded | Theorem 2 | NEARPAY-OUT unbuilt |
