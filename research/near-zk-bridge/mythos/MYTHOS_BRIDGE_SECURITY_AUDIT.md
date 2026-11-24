# Mythos security audit: ZTB / BRIDGE-SEC-01 -> BRIDGE-SEC-02

Date 2026-11-24. Adversarial review of the proposed TradeLayer <-> NEAR stablecoin bridge under
complete compromise of relayer, prover, RPC and administrative keys. Research only: no live
contract, chain, key or fund was touched. All adversarial runs use owned local fixtures.

**Reviewed at:**
- TradeLayer-Rust `research/ztb-bridge-sec-01-cyber` @ `03908f5` (PR 158; the ZTB research
  directory, type-36 verifier, registry, node path, `src/zk_deposit.rs`). Paths below are
  repo-relative to TradeLayer-Rust unless prefixed.
- zk-rusto-no-bitcoino main (local clone `rbzk-main`): `integrations/bitvm3-nova` m13/m14/m16/m18.
- BitVMArena `corpus/docs/BITVM3.md`; Spiral In-and-Out-Burger BitVM3 graph @ `76071a0` (`iob/`).
- UTXO-Ref `origin/main` @ `0dff29e` and `origin/security-review-fixes`.
- SystemicRiskBench PR 14 head `29ffd0a` (`content_cyber_bridge_2026`).
- bitcoin-rs clones (`wt/bitcoin-rs-ark`, `wt/bitcoin-rs-bip448`): Core-parity node code; no
  BitVM or Nova verifier inside. They are relevant only as a script/consensus oracle for
  presigned graphs (CSV, P2A, TRUC), so they are not reviewed further here.

**Deliverables in this directory:** [BRIDGE_SECURITY_THEOREM.md](BRIDGE_SECURITY_THEOREM.md),
[lean/BRIDGE_SECURITY.lean](lean/BRIDGE_SECURITY.lean), [NOVA_STATEMENT_V2.md](NOVA_STATEMENT_V2.md),
[BRIDGE_SEC_02_SPEC.md](BRIDGE_SEC_02_SPEC.md), [CODEX_WORK_ORDER.md](CODEX_WORK_ORDER.md), and the
executable regression [../bridge-sec-02.mjs](../bridge-sec-02.mjs) with frozen evidence
[../bridge_sec_02_evidence.json](../bridge_sec_02_evidence.json).

## Answer to the central question

The bridge **can** be built so that compromising every operational key affects availability
only. The design does not do that yet: no part of it currently authenticates NEAR consensus. The
reduction holds under five assumptions, each shown necessary by a checked counterexample
(theorem doc section 4):

- **A1 source authentication**: NEAR consensus plus Nova soundness plus statement binding.
- **A2 persistent nullifiers.**
- **A3 finality**: NEAR, plus Bitcoin K-depth for the reverse path.
- **A4 custody**: vault code, vault account keys, and token issuer.
- **A5 upgrade continuity.**

Two further conditions are easy to miss:

- **Liveness-to-safety coupling.** NEAR light-client updates and Bitcoin header submissions must
  be permissionless. A PoS light client left idle past the unbonding period, or a Bitcoin SPV
  view fed only by compromised relayers, turns an availability failure into a safety failure
  (H3, H4).
- **Out-of-band custody holders.** The NEAR vault account's access keys and the USDC issuer's
  upgrade and freeze powers sit outside any proof system (C4). Key-deletion policy can neutralize
  the first. The second can only be contained.

Three labels apply to what follows:
- **Verified**: run in this session. Covers the Lean build, the Node suites, and the Python oracles.
- **Source-read**: read in the code, not executed.
- **Design**: specified but unbuilt.

Throughout, accounting correctness, source consensus authentication, proof soundness, custody
authorization and production readiness are kept distinct.

## Findings (most severe first)

Each finding has a severity, a layer, the exact references and evidence, and a fix. Severities
are rated for a phase-2 deployment that enables `can_mint`. Today `can_mint=false` and
`ZkEffect` has only `StatementReceipt` (`src/zk_verify.rs:68-72`), so every Critical below is
latent. None is exploitable against live funds.

### C1 (Critical, source consensus): no component authenticates NEAR finality

- **Evidence (source-read and verified).**
  - BRIDGE-SEC-01's guard admits deposits from `trusted`, a hard-coded `Map`
    (`research/near-zk-bridge/bridge-sec-01.mjs:41-49`).
  - NEARPAY01-A's `proveOutcome` takes `headRoot` as "a pre-authenticated input ... never
    supplied authority" (`nearpay01/reference.mjs:38-39`).
  - NEARPAY01-B checks the block path against `policy.headRoot`, which is a frozen fixture pin
    (`nearpay01/receipt_membership.mjs:73-89`).
  - NEARHEAD01 is listed in `PROTOCOL.md` (Proof programs table) and is unbuilt.
  - `docs/ZK_DEPOSIT_PROOFS.md` covers only Bitcoin M16 attestations.
- **Consequence.** Every admission path that exists today reduces to "whoever supplies the head
  or the table". That is the CYBER failure class: a valid verifier signature over a message
  absent from source history. BRIDGE-SEC-01's 22 checks show the accounting arithmetic. They are
  not evidence of consensus verification, and the PR text says so correctly.
- **Fix.** Build NEARHEAD01 as specified in NOVA_STATEMENT_V2 section 3 mode H. BRIDGE-SEC-02
  replaces the trusted table with a fixture light client:
  - It uses real Ed25519 approvals, stake-weighted >2/3 finality and committed next-set hand-off.
  - It rejects headers signed by admin/relayer/RPC keys (A2), a 50% sub-quorum (A3), duplicated
    approvals (A3b), and retired producers (R3).
  - It accepts a forgery by 75% of epoch stake (X1). That acceptance is the consensus assumption,
    stated as a test.
  - The fixture is NOT NEAR's light-client block format (W1 in the work order).

### C2 (Critical, proof soundness): type-36 programs pin the key but not the initial state z0

- **Evidence (source-read).**
  - `ZkProgram` (`src/zk_verify.rs:86-96`) has these fields: system, program_id, key_bytes,
    layout, effect, verifier_version and max_verify_ms.
  - `register_zk_program` (`src/registry_zk.rs:62-104`) validates those fields.
  - `zk_outcome` (`src/registry_zk.rs:148-213`) checks only `public_inputs[0] == binding_element`
    (`src/zk_verify.rs:430-445`). Indices 1.. are prover-chosen.
  - The same gap exists one layer down. zk-rusto `m18 verify-window` checks the NHC1 magic, tip
    hash and height, not genesis z0 (`integrations/bitvm3-nova/m18/src/main.rs:755`). The M16
    guest echoes the bundle's first z0 (`m16/sp1/program/src/main.rs:277-295,325`). Only the
    In-and-Out-Burger desk policy pins z0, off-chain (`iob/policy.py`, `iob/statement.py`).
- **Consequence.**
  - Nova IVC proves `zn = F^n(z0)` for the z0 the prover chose.
  - For NEARHEAD01 that means a prover can start from a fake checkpoint whose "validator set" it
    controls, then fold honest-looking steps.
  - Every NEAR-side fact is then forgeable. No consensus key is needed: only the ability to
    submit a type-36 transaction.
- **Fix.** Add `z0_pins` to `ZkProgram` and include it in `program_id_of`. Reject any proof whose
  pinned indices differ (NOVA_STATEMENT_V2 section 6.1). Port the same assertion into the zk-rusto
  verifiers (m18, M16 guest) so the gap is not inherited by BitVM3 claims.
  - **Counterexample if the pin is removed:** BRIDGE-SEC-02 X1 (a forged quorum is accepted)
    generalizes. A forger who picks the initial set needs no stake at all.

### C3 (Critical, upgrade boundary): one `protocol_admin` key registers and activates programs, with no delay and no continuity constraint

- **Evidence (source-read).**
  - `RegistryAction::RegisterZkProgram` and `Activate` require only `actor == protocol_admin`
    (`src/registry.rs:375`, `src/registry.rs:601-613`).
  - Programs are immutable and append-only (`src/registry_zk.rs:94-103`). That part is good.
  - Nothing constrains which program a future custody effect trusts.
  - The ZTB statement ABI carries `program` and `key` fields (`codec.mjs:15`). The ABI says "the
    cryptographic receiver independently pins" them (`ZTB02_ABI.md`) without saying who sets or
    changes the pin.
- **Consequence.** With a phase-2 custody effect selected per registered program, the admin key
  alone can register a permissive key and mint. That is CYBER's "additional message approved
  after key rotation", moved to the verifier layer.
- **Fix (constrained upgrade mechanism, section 5).**
  - The domain pins its program id at activation.
  - Replacement only after a delay, with commitments to the old consumed-set root and the
    light-client head.
  - The emergency power is pause-only.
  - BRIDGE-SEC-02 exercises it:
    - A4: an admin-registered permissive program is refused.
    - A5, A5b, A5c: reset nullifiers, swapped checkpoint and early activation are refused.
    - A benign upgrade with continuity is accepted.
    - R5, R6: replay under the new verifier and the retired program are refused.

### C4 (Critical, custody): vault and token code identity are outside every proof

- **Evidence (design).**
  - `PROTOCOL.md` says "No NEAR operator key may authorize an arbitrary mint or withdrawal". It
    does not specify the vault account's access-key policy.
  - A NEAR account holding any full-access key can deploy new code and transfer everything.
  - `vault_mock.mjs` has no admin transfer, but deployment is undefined.
  - USDC on NEAR is issuer-upgradable, with freeze and blacklist powers (PROTOCOL.md, Scope).
- **Consequence.** Assumption A4 fails without any cryptographic break. Lean
  `c4_no_custody_insolvent` and BRIDGE-SEC-02 X2 both leave 905 of 5,213,905.
- **Fix.**
  - Deploy the vault with zero full-access keys. The only function-call keys allowed are for
    non-value methods.
  - Code upgrade goes only through a self-upgrade method subject to the section 5 constraints.
  - Pin the vault code hash and the token code hash in the domain `policy` digest.
  - Treat the issuer as an explicit trusted party with containment: quarantine on unexplained
    balance change and haircut accounting.

### H1 (High, accounting/proof): type-36 receipts are not deduplicated by statement

- **Evidence (source-read).** `record_zk_verify` (`src/registry_zk.rs:118-146`) rejects only a
  repeated `txid`.
  - The binding element covers sender and expiry (`src/zk_verify.rs:430`). So one sender can
    submit the same payload in two transactions before expiry and get two `StatementReceipt`s
    with identical `public_inputs_hash`.
- **Consequence.** It is harmless for receipts. It is a double mint for any custody effect that
  credits per receipt.
- **Fix.** Value effects consume a statement nullifier (NOVA_STATEMENT_V2 section 6.2). In the V2
  relation, event nullifiers are inserted into an in-circuit accumulator with CAS on its root, so
  duplicates are refuted inside and across aggregates.

### H2 (High, replay trust): recorded verdicts replace proof math during state rebuilds

- **Evidence (source-read).**
  - `RecordedZkVerifier` maps the receipt reason string "zk:verified" to Accept
    (`src/zk_verify.rs:527-563`).
  - `src/consensus_source.rs:356-366` uses it for checkpoint verification and rollback.
- **Consequence.**
  - For a node replaying its own receipts this is a sound cache.
  - If receipts ever come from a peer, snapshot or checkpoint bundle, the verdict is trusted, not
    checked. That makes the authentication circular: state is authenticated by a record of its
    own acceptance.
- **Fix.** Value-moving effects must re-verify, or verify against a locally produced verdict log
  bound to the node's own first application. Document the boundary in `docs/ZK_VERIFIER.md`.

### H3 (High, consensus/liveness): a PoS light client left idle past the unbonding period loses safety

- **Evidence (design).**
  - NEARHEAD01 follows epochs from a pinned checkpoint.
  - Producers of an old epoch who have since withdrawn stake can sign an alternative history at
    no slashing risk (a long-range attack).
  - No mechanism in PROTOCOL.md or EXPERIMENTS.md forces the head forward.
- **Consequence.** If all relayers are compromised and NEAR-head updates are permissioned, the
  attacker can stall updates past the unbonding window. Once past it, a coalition of former
  validators with exited stake can forge. Operational compromise then degrades a safety
  assumption.
- **Fix.**
  - Permissionless head updates with a fee rebate.
  - A domain rule: if `now - lastHeadUpdate > unbonding/2`, quarantine admissions until a fresh
    update.
  - The NEAR unbonding duration must be read from the pinned nearcore version (not done here).

### H4 (High, Bitcoin anchor): the NEAR-side Bitcoin view is whatever submitters provide

- **Evidence (design and verified).**
  - PROTOCOL.md (Checkpoint ABI) says SPV "proves inclusion under assumptions, not global
    heaviest-chain knowledge".
  - `docs/ZK_DEPOSIT_PROOFS.md` ("What a light client still cannot know") says the same for M16.
  - BRIDGE-SEC-02 tests (`BtcLC`):
    - B1: a 1-confirmation anchor is refused at K=3.
    - B2, B3: unknown-block and re-anchored-predecessor substitutions are refused.
    - B4: CAS replay is refused.
    - X3: a heavier fork of K+2 blocks orphans an anchor after payout.
- **Consequence.**
  - With only compromised submitters, the attack cost is privately mining K+1 blocks at full
    difficulty.
  - With one honest submitter inside the challenge window, the cost is a majority-hash reorg
    deeper than K.
  - A post-payout deep reorg erases the burn from TL history while the reserve is gone.
- **Fix.**
  - Permissionless header submission with a challenge window.
  - An exposure cap: payout value per anchor window must be at most the cost of K blocks.
  - A post-payout reorg policy: halt, plus a recovery reserve (PROTOCOL.md already requires
    emergency accounting).

### H5 (High, reverse path): TLSTATE01 validity and completeness are unproven

- **Evidence.**
  - PROTOCOL.md TLSTATE01 requires full ordered replay with prevouts and witness roots, because
    "individually included transactions can omit losses".
  - No circuit exists.
  - BRIDGE-SEC-02 uses `TLSTATE_FIXTURE` (a trusted set of valid statement digests) and reports
    `tl_state_validity: "TRUSTED FIXTURE"`.
- **Consequence.** The reverse path has the same structure as C1. A forged TL statement with a
  fabricated burn is limited today only by the fixture.
- **Fix.** Work order W7. Until then, payouts need the TL statement CAS chain plus quarantine.

### H6 (High, upgrade/ABI): the statement ABI cannot express continuity

- **Evidence (source-read).**
  - TLSTMT01 (`codec.mjs:15-16`) has deposited, minted and burned, but no paid counter `R`.
  - It has no consumed-set root, no light-client epoch or producer-set hash, and no verifier
    epoch.
- **Consequence.** A verifier migration or state migration that snapshots balances can reset
  consumed identities without changing any committed field. BRIDGE-SEC-02 X4 shows this.
  - X4a: clearing only the bridge set is caught, because the ZTB-01 ledger's own deposit map is a
    second nullifier (`model.mjs:35`).
  - X4: a balance-only migration (fresh ledger plus a snapshot deposit) replays a deposit into
    insolvency.
- **Fix.** Add to the statement:
  - `paid`;
  - `nullifierRoot_in/out`;
  - `lcEpoch`, `lcBpHash`, `lcHeadHash`;
  - `verifierEpoch`;
  - `prevProgram`, for continuity across a re-key.
  - See NOVA_STATEMENT_V2 section 2.

### H7 (High, BitVM3): every BitVM3 safety property is currently operator-trusted

The survey covered zk-rusto `integrations/bitvm3-nova`, BitVMArena `corpus/docs/BITVM3.md` and
IOB `76071a0`. PROTOCOL.md (Existing proof paths) states that "BitVM3 enforcement is a future
Bitcoin custody path, not required to release NEAR escrow". These assumptions apply wherever
BitVM3 is used.

**Challenger availability:**
- The graph has no challenge transaction (BVA:240).
- The disprove leaf needs one watcher `W` (BVA:41, `single_watcher_disprove`).
- If no challenger appears, the payout leaf `<6> CSV` matures and **the false claim is paid from
  the vault** (BVA:90).

**Timelocks:**
- `WINDOW_BLOCKS = 6` and `RECLAIM_BLOCKS = 144` (`iob/bitvm3.py:36-38`).
- M14 evaluation takes 348-401 s, or 627-1,330 s when the machine swaps.
- Probability the 6-block window expires before evaluation finishes: 0.026 at 1,330 s. For a
  60-minute evaluation: 0.55 at a 6-block window, 0.15 at 9 blocks (BVA finding 2).
- The window is also the miner-censorship budget.
- A testnet4 reorg restarted a CSV (BVA:289-292).

**Operator solvency:**
- The desk fronts payouts.
- No bond is in the graph.
- A false claim costs only fees (BVA finding 4).

**Inclusion and fees:**
- Presigned at 2 sat/vB.
- No P2A anchor or TRUC transaction.
- Pinning was demonstrated with a 43,112 vB child (`disprove_anchor_pinned`).

**Presigned committee:**
- A committee of one that retains its keys can re-sign the matured payout to itself (BVA
  finding 9).
- The n-of-n key-deletion design (IOB README l.316-342) is unbuilt.

**Garbler honesty:**
- No cut-and-choose exists. A grindable version escaped in 3 trials (BVA:93).
- The garbler knows both output labels (`m14/claim/SECURITY.md:128-134`).

**Input labels:**
- M14 has 8,635,080 input bits, at about 23.5 vB per bit, so about 203M vB on chain.
- Only a 512-bit statement commitment (about 12k vB) is feasible.
- A withheld label means the false claim is paid.

**Statement replay across vaults:** the vault outpoint is not in the statement (BVA finding 7).

**Soundness history:**
- Garblings made before the Z1 fix (zk-rusto `d618707`/`43ff07c`, circuit now 792,546,673 gates)
  accept forged SP1 public values.
- Every recorded M14 run used the old 785,442,492-gate circuit (BVA finding 10).

**Fix.** Keep BitVM3 out of the bridge's custody path until all of the following exist:
- a bond;
- a permissionless presigned disprove with an anchor;
- committee key deletion;
- label authentication;
- a z0 pin;
- a vault-bound statement.

### H8 (High, settlement): UTXO-Ref PnL settlement is conserving but not authenticated

These items are source-read from the UTXO-Ref survey.

- `tradelayer_pnl_state_netting.js` recomputes PnL and rejects a graph that does not net to zero
  (L191, L314). It trusts a "state oracle" for positions and marks
  (`TRADELAYER_PNL_STATE_NETTING.md`).
- Multi-oracle DLC adaptor points are a plain sum with no nonce proof-of-possession
  (`dlc_signing_target.js:124-131`, `dlc_threshold_oracle.js:63-67`).
- That is U1 from the 2026-11-24 review. It is still unfixed on `origin/main` and on
  `origin/security-review-fixes`.
- UTXO-Ref has no Lean conservation or netting theorem. Its Lean proves signer and one-shot
  properties (OneShot, TxSet, Signer, BitVM modules).
- The withdrawal-queue referee has no check of queue total against reserves.

**Consequence for the bridge.** A conserving but misdirected settlement does not break Theorem 1,
because `O` is unchanged. It is user-level theft without reserve insolvency. A non-conserving
settlement (JS check bypassed) would break the imported "internal transitions preserve O"
premise.

**Fix.**
- Nonce PoP (or MuSig-style nonce commitment) for oracles.
- Prove netting-to-zero in Lean or Kani for the exact encoder.
- Feed positions from TLSTATE01, not from a state oracle.

### M1 (Medium, hybrid assets): non-protocol spends of token-bearing outpoints have no semantics

- **Evidence.** `PROTOCOL.md` (Transitions) says to "specify burn or recovery semantics before
  activation". `model.mjs` handles only protocol spends.
- **Consequence.** Either a liability leak or a resurrection risk.
- **Fix.** Treat a non-protocol spend as a burn without an exit. The liability decreases and the
  surplus goes to reserve, so Theorem 1 still holds. Never resurrect. Add the transition to the
  Lean model as "liability destroyed" (it only increases `V - (U+O+Q)`).

### M2 (Medium, payout liveness): paid-but-unrecorded has no recovery proof

- **Evidence.** `vault_mock.mjs:54-60` (`callback`) leaves the exit in-flight forever when the callback is
  lost. `ZTB02B_GATE.md` says recovery needs a real receipt proof.
- **Consequence.** Lean Theorem 2 holds (no double pay), but funds are locked in `reserved`. This
  is availability only. BRIDGE-SEC-02 D2 confirms there is no timeout resend.
- **Fix.** A NEARPAY-OUT relation proving the exact outgoing `ft_transfer` terminal outcome for
  `(exit, attempt)`, consumed once.

### M3 (Medium, aggregation): the step count is prover-chosen

- **Evidence.** `split_proof_envelope` (`src/zk_verify.rs:404-409`).
- **Fix.** Coverage counters `k` and `sum` plus a list accumulator in zn (NOVA_STATEMENT_V2
  section 2). Use fixed step classes.

### M4 (Medium, liveness): expiry-bound proofs must be regenerated per submission

- **Evidence.** The binding element includes `expiry` and `anchor` (`src/zk_verify.rs:430-445`).
  The NHC1 1-header step compresses in 199 s and 2.1 GiB (`docs/ZK_VERIFIER_NOVA.md`).
- **Consequence.** Fee spikes or a censoring relayer force re-proving. This is availability only.
- **Fix.** Long expiries for deposit aggregates. Sender binding stays, to prevent fee and credit
  front-running. Credits are bound to the recipient script, so front-running cannot redirect
  value.

### M5 (Medium, authority): BRIDGE-SEC-01's guard makes the relayer an authority, not a courier

- **Evidence.** `guard` refuses unless `relay === policy.relay` (`bridge-sec-01.mjs:43-44`).
- **Consequence.** The relay identity becomes both an availability point and part of
  authorization.
- **Fix.** Permissionless submission. BRIDGE-SEC-02 `admit` ignores relayer identity entirely,
  and R1 checks that a rotated relayer cannot replay.

### M6 (Medium, containment): reconciliation must use evidence independent of the admission path

- **Evidence (verified).** In BRIDGE-SEC-02:
  - C2 detects the X1 shortfall from the vault's physical holdings and quarantines.
  - C3 shows that quarantine stops admissions without bypassing proof checks.
  - The fixture reads holdings from the vault mock.
- **Consequence.** If holdings were read through the same compromised NEAR light client, a 2/3
  stake attacker forges both the deposit and the balance.
- **Fix.** Reconcile through an independent observer: a second consensus client implementation,
  an archival full node, or the issuer's attestation. The containment bound is otherwise
  Theorem 3 alone: `k * cap`.

### M7 (Medium, accounting): issuer actions are unmodelled reserve changes

- **Evidence.** PROTOCOL.md (Accounting) asks for a halt on "vault loss, token freeze, unexpected
  balance changes". No transition exists for them.
- **Fix.**
  - A `reserveShock(x)` transition that forces quarantine.
  - Haircut accounting of `V / (U+O+Q)`, proposed and never auto-executed.
  - The BitAgent policy below.

### L1 (Low, test hygiene): research suites were not in CI

- **Evidence.** `.github/workflows/ci.yml` has no `near-zk-bridge` step.
- **Fix (done here).** `.github/workflows/ztb-bridge-research.yml` runs:
  - all six Node suites;
  - both Python oracles;
  - the Lean package with `leanchecker` and an axiom audit.
- It triggers on `research/ztb-bridge-sec-**` pushes and on PRs.

### L2 (Low, model): ZTB-01's backing check is self-referential

- **Evidence.** `model.mjs:24`: `D-R >= outstanding+pending`, where `D` is the claimed deposit
  counter.
- BRIDGE-SEC-01 already demonstrates this. Lean invariant field 5 (`V + R = sum_S + G`) is the
  independent version.

## 5. Constrained upgrade mechanism

Requirements: every guarantee in Theorem 1 survives any upgrade, and no single key can apply an
upgrade.

1. **Domain immutability.** `dom` commits to:
   - the program id;
   - the z0 pins;
   - the vault and token code hashes;
   - the upgrade policy.

   Changing any of them creates a new domain with its own reserve. There is no shared reserve
   across domains.
2. **What an upgrade is.** A new program id `P'` for the same domain, proposed with a
   `MigrationCert`:
   - `nullifierRoot` and `consumedCount`, equal to the current values;
   - `lcHeadHash`, equal to the current light-client head (or a descendant proved by H steps);
   - `prevProgram`;
   - `statementVersion`;
   - the reproducible-build digest of the reviewed circuit.
3. **Delay.** At least the maximum of:
   - the withdrawal delay plus one unbonding period;
   - a governance review period.

   During the delay, exits are allowed under the old program, so users can leave.
4. **Activation check.** At activation, every commitment in the cert is rechecked against the
   current state (CAS). Any admission in between invalidates the cert.
5. **Monotone state.** Consumed identities, the deposit list, exit nullifiers and paid exits are
   never rewritten. The new verifier's z0 must equal the old verifier's zn on the continuity
   fields.
6. **Emergency power is pause-only.**
   - Pause stops admissions and payouts.
   - Pause cannot register programs, change pins, move funds, or skip proof checks.
   - Unpause requires reconciliation (`V >= U+O+Q`, or an explicit haircut decision recorded
     on chain).
7. **Key holders.** Proposal is a dual-control quorum (k-of-n, n >= 3). On TL this replaces the
   single `protocol_admin` for this domain. Revocation (deprecating a program) is allowed
   immediately, because it fails safe.
8. **Checkpoint replacement.** Only to a header the current light client finalized, or one proved
   to descend from it. Never to an admin-supplied root.

BRIDGE-SEC-02 tests items 2-5 (A5, A5b, A5c, the benign upgrade, R5, R6) and the cost of skipping
them (X4).

## 6. Economic containment and BitAgent

Formal pieces:
- Lean Theorem 3: window cap gives a loss of at most `k * cap` before detection.
- Theorem 1 corollary: the exact surplus.

BRIDGE-SEC-02 checks these pieces:
- **C1:** the cap bounds a supermajority forgery to one window. Of a 1,800,000 attempt, 900,000
  is admitted.
- **C2:** reconciliation quarantines with an exact shortfall of 900,000.
- **C3:** quarantine halts admission.
- **C4:** no false positive on the solvent world.

Withdrawals use the ZTB-02 per-exit reservation. Exits keep FIFO order. Quarantine freezes
`begin`, not registration. A haircut is a recorded proposal `V*10^6/(U+O+Q)` ppm. It is never
executed automatically.

**BitAgent** is the BitAgent repo's candidate-only deposit/strategy/withdraw agent; UTXO-Ref
`BITAGENT_REPO_REVIEW.md` describes its host boundary. It must respond to inconsistent collateral
evidence as a proposer, never as a signer.

The BRIDGE-SEC-02 agent checks encode four rules, matching SRB `content_cyber_bridge_2026`:
1. With the light client frozen or reconciliation failing, the bridged asset is unauthenticated
   collateral. Propose: no new exposure and no collateral admission.
2. A nominal cross-venue spread with transfers halted has zero executable matched volume. Propose
   no arbitrage.
3. Sell proposals are capped at `venue buy capacity / estimated agents`. At 10,000 capacity and
   N=1000 agents that is 10 per agent, which avoids the 90% unfilled synchronized dump in SRB
   stage 3.
4. `signs=false` and `broadcasts=false` always.

Under C1/C2/C3 conditions BitAgent must not:
- liquidate positions collateralized by the bridged asset into venues whose transfers are halted;
- bridge further deposits;
- treat a relay signature as evidence.

## 7. What was run

| Command | Result |
|---|---|
| `node research/near-zk-bridge/bridge-sec-02.mjs` | 68 checks, evidence equals frozen JSON (Windows, Node 24.19.0) |
| `node test.mjs`, `test_ztb02.mjs`, `bridge-sec-01.mjs`, `nearpay01/test.mjs`, `nearpay01/test_b.mjs` | all pass, unchanged |
| `python ztb02_oracle.py`, `python ztb02b_frontier.py` | PASS; 271/271 frontier checkpoints |
| `lean BRIDGE_SECURITY.lean` (v4.35.0, WSL) | 0 errors, 0 warnings; axioms `propext`, `Quot.sound` |
| `lake build` + `lake env leanchecker BRIDGE_SECURITY` | build OK, leanchecker exit 0 |

Every authority flag remains false: `native_finality_verified`, `nova_verified`,
`near_decider_verified`, `btc_anchor_verified`, `economic_authorized`, `production_ready`.
BRIDGE-SEC-02's fixture light client is cryptographically real but is not NEAR's format.
Passing it is evidence about the admission logic's shape, not that NEAR consensus verification
exists.
