# BRIDGE-SEC-02: preregistered compromised-authority experiment

Registered 2026-11-24, before the extension tiers T2-T4 below are run. T1 is implemented and
passing (`research/near-zk-bridge/bridge-sec-02.mjs`, 68 checks, frozen evidence
`bridge_sec_02_evidence.json`). Isolated synthetic fixtures only.

## Hypothesis

H0: with every operational key held by the adversary (relayers in every epoch, prover, RPC,
TL `protocol_admin`, bridge operator), no submission sequence produces an admitted credit or a
payout that is not backed by a finalized retained source deposit, provided the five assumptions
of the theorem doc hold. The adversary's only effect is availability.

H1 (expected and tested): removing a single assumption yields an accepted forgery or a double
payout. Each such case is a **counterexample control**. It must be accepted, and the test fails
if it is rejected, because a rejection would mean the fixture is not modelling the assumption.

## Threat model and fixture roots of trust

| Party | Adversary holds | Fixture root of trust |
|---|---|---|
| Relayer (epochs A, B) | yes (`key("relayer-epoch-A")`, `B`) | none: identity ignored |
| Admin / registry | yes (`key("admin")`, can register programs, propose upgrades) | domain pins program; upgrade CAS + delay |
| Prover | yes (chooses statement and witness) | relation `NEARPAY_V2` |
| RPC | yes (serves any header and approvals) | light client: pinned genesis set, >2/3 stake |
| NEAR producers | no in T1 (except X1: 3 of 4 keys) | Ed25519 |
| Bitcoin miners | no (except X3: longer private fork) | SHA-256d PoW, chainwork, K=3 |
| Vault code | honest `vault_mock.mjs` (except X2: admin transfer path) | code identity |
| TL state | **trusted fixture** `TLSTATE_FIXTURE` | none yet (PO8) |

## Arms (T1, implemented)

Each check is labelled with a category; counts are frozen in the evidence file.

| Category | Count | Cases |
|---|---|---|
| benign | 11 | full keep; partial refund credits retained only; solvent; carol proof after mutations; complete evidence; honest epoch hand-off; benign upgrade; anchored statement advances root; failed payout -> pending; exactly one payout; reconciliation no false positive |
| authority | 6 | A1 valid relayer + absent event; A1b rotated relayer; A2 admin+relayer+RPC signed header; A3 50% quorum; A3b duplicated approvals; A4 admin-registered permissive program |
| upgrade | 4 | A5 consumed-set reset; A5b checkpoint swap; A5c early activation; X4a ledger deposit map as second nullifier |
| binding | 13 | statement field substitution x9 (recipient, amount, depositId, candidate, domain, token, vault, outcomeRoot, headerHash); extra field; witness recipient swap; padded Merkle path; receipt role substitution |
| execution | 5 | resolver failure substituted; receiver without resolver; full refund grants nothing; requested-instead-of-retained; refund log stripped |
| replay | 6 | after relayer rotation; after producer rotation; retired producers sign new epoch; alternate candidate for same receipt; under upgraded verifier; retired program |
| checkpoint | 3 | equivocating quorum at one height; slashable evidence recorded; frozen client refuses all heads |
| anchor | 4 | 1-confirmation anchor; unknown block; non-anchored predecessor; statement CAS replay |
| payout | 5 | destination substitution; same exit twice; crash after transfer (no resend); duplicate transfer per attempt; callback replay |
| containment | 3 | window cap bounds supermajority forgery; reconciliation quarantines; quarantine halts admission |
| counterexample | 4 | X1 >=2/3 stake forgery accepted; X2 admin-key vault drains to 905; X3 deep reorg orphans anchor after payout; X4 balance-only migration replays into insolvency |
| agent | 4 | never signs/broadcasts; sell share fits venue capacity at N=1000; refuses inconsistent collateral; no sale where nothing executes |

## Acceptance criteria (all must hold)

1. Every adversarial arm is rejected **with the expected reason string** (the harness matches the
   error text, so a rejection for the wrong reason fails).
2. Every benign arm succeeds and the world stays solvent: `holdings - reserved >= outstanding +
   (pending - reserved) + unminted`.
3. Every counterexample arm is accepted, and the recorded consequence holds (insolvency, orphaned
   anchor, drained vault).
4. The output equals `bridge_sec_02_evidence.json` byte-for-byte after JSON normalization. That
   pins the domain, program, producer-set hashes, the PoW anchor hash and the final nullifier root.
5. All prior suites still pass unchanged: ZTB-01 (`test.mjs`), ZTB-02 (`test_ztb02.mjs`),
   NEARPAY01 A/B, BRIDGE-SEC-01, and both Python oracles.
6. All authority flags in the output are `false`.

A failure of 1-3 is a security regression. A failure of 4 alone after an intentional change
requires re-freezing (`node bridge-sec-02.mjs --freeze`) in a separate commit that explains the
change.

## Extension tiers (preregistered, not yet run)

**T2: nearcore-format light client.** Replace the NLC fixture with exact LightClientBlockView
parsing and approval messages for one pinned nearcore protocol version, using archived real
headers from testnet as vectors (read-only RPC capture, raw JSON stored with provenance). Same
arms A2, A3, R2, R3, K1. Acceptance: real header chain verifies across at least 3 epoch
hand-offs; each mutation in the T1 authority/replay/checkpoint arms rejects with the expected reason.

**T3: native Rust + Kani.** Port `admit`, `activate`, the payout state machine and reconciliation
to a standalone Rust module (no new crates). Kani harnesses for Lemma 2 per transition and for
Theorem 2. Acceptance: the same JSON evidence from a Rust test binary; Kani proofs pass in CI.

**T4: real proofs.** NEARPAY_V2 as a Nova step circuit; a real compressed proof per benign
deposit; type-36 verifier with `z0_pins` (NOVA_STATEMENT_V2 section 6). Arms: z0 substitution (must
reject), program not pinned by domain (reject), statement duplicate in a second transaction
(reject via statement nullifier). Acceptance: each arm verified against real proof bytes; cost
recorded (prover time, RAM, proof bytes, verify ms) against the 4,000 ms block budget.

Metrics reported per tier: checks, categories, counterexamples, pins, and the authority flags.
No tier sets `economic_authorized` or `production_ready` to true.
