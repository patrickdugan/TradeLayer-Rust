# Nova statement V2 for NEAR -> TL deposits (NEARHEAD01 + NEARPAY01 composed)

Status 2026-11-24. Specification only. No circuit, key or proof exists for this relation. The
native evaluation of this relation (without consensus signatures) is `NEARPAY_V2` in
[../bridge-sec-02.mjs](../bridge-sec-02.mjs). BRIDGE-SEC-02 exercises it against the fixture
light client.

## 1. What V1 leaves open

| Gap | Evidence | Consequence |
|---|---|---|
| NEAR head root is an input, not a proved fact | `nearpay01/reference.mjs:38-39` ("HEAD ROOT is a pre-authenticated input"), `receipt_membership.mjs:73-89` (`policy.headRoot` fixture pin) | Whoever supplies the head controls admission (CYBER class) |
| Type-36 pins key and layout, not z0 | `src/registry_zk.rs:62-104` (ZkProgram has no initial-state field); `src/zk_verify.rs:430` binding covers only index 0 | An IVC proof from an attacker-chosen genesis validator set verifies |
| No consumed-set commitment in the statement | `codec.mjs:15-16` (TLSTMT01 fields) | Upgrades and migrations can reset replay protection silently (X4) |
| Receipts not deduplicated by statement | `src/registry_zk.rs:118-146` replay keyed only by txid | One proof, many receipts; a custody effect per receipt double-mints |
| Step count chosen by the prover | `src/zk_verify.rs` `split_proof_envelope` | Aggregates must carry their own counters; the verifier cannot infer coverage |

## 2. Public inputs (z0 || zn, Pallas scalars; 256-bit digests as two 128-bit limbs)

Every digest is split into big-endian u128 limbs `hi, lo` with an in-circuit range check and
exact byte reconstruction (ZTB02_ABI rule; never reduced modulo the field).

| Index | z0 (pinned) | zn (proved) |
|---|---|---|
| 0 | binding element (`zk_verify.rs:430`: TL genesis, anchor, sender, expiry, program id) | same value, copied every step |
| 1-2 | `dom` (domain digest) | same, copied |
| 3-4 | checkpoint: NEAR block hash the domain was activated at | final accepted NEAR head hash |
| 5-6 | checkpoint epoch id | head epoch id |
| 7-8 | `bpHash` of the checkpoint epoch's producer set | `bpHash` of the head epoch's set |
| 9 | checkpoint height | head height |
| 10-11 | `nullifierRoot_in` (TL consumed-set accumulator root, from TL state) | `nullifierRoot_out` |
| 12 | deposits processed = 0 | `k` deposits in this aggregate |
| 13 | retained sum = 0 | sum of `amt(e)` |
| 14-15 | deposit list accumulator = H(empty) | H-chain over (depositId, candidate, recipientScriptHash, amount) |
| 16 | statement version = 2 | same |

The registered program pins indices 1-9 and 16 of z0 (see section 6). The TL custody effect
checks: z0[10..11] equals TL's current nullifier root (compare-and-swap), and zn[14..15]
equals the hash chain over the decoded deposit list it is about to credit.

## 3. Step relation (one typed microstep per fold)

State tuple carried across steps: `(binding, dom, headHash, epoch, bpHash, height,
nulRoot, k, sum, listAcc, version, mode, cursor)`. `mode` selects one of:

**H: header advance (NEARHEAD01).** Witness: header bytes (exact nearcore LightClientBlockView
serialization for the pinned protocol version), approvals (signature, producer index), producer
set of `epoch` (and of `epoch+1` when crossing). Constraints:
1. `prev`-relationship per nearcore light-client spec: header height > current height; header
   epoch == current epoch or the next epoch whose set hash was committed by a header of the
   current epoch (`next_bp_hash`).
2. Producer set opens to `bpHash` (Merkle or full-list hash, fixed max size).
3. Approval signatures verify (Ed25519, non-native field arithmetic) over the exact nearcore
   approval message for the approved block; distinct producer indices; stake sum computed with
   range-checked u128 arithmetic; `3 * signed > 2 * total`.
4. Update `headHash, epoch, bpHash, height`. Store the header's outcome root and block-Merkle root
   in a bounded ring of authenticated heads (needed by P steps).

**P: deposit (NEARPAY01 A+B).** Witness: three outcomes, their Merkle paths, three receipt
bodies via ReceiptList openings (whole list streamed through SHA-256 in 64-byte blocks), the data
receipt, header paths to an authenticated head in the ring. Constraints:
1. Outcome membership for transfer, receiver, resolver; receipt-list membership for receiver,
   resolver and data receipt, against roots of headers already in the ring (NEARPAY01-B layout
   rules: shard index order, odd-node promotion, double hash of chunk root).
2. Graph: `transfer.status = SuccessReceiptId(resolver)`; both children generated; executors
   token/vault/token; predecessors; receiver output data id == resolver input data id; data
   receipt routed vault -> token with exactly the receiver's SuccessValue bytes.
3. Amounts: `requested` from the exact NEP-141 transfer log; `unused` from the receiver's
   SuccessValue (u128 decimal JSON string, canonical); `used` from the resolver's SuccessValue;
   `used == requested - unused`; refund log present iff `unused > 0` with exact text; no extra
   logs; `used > 0`.
4. Candidate log `ZTB_DEPOSIT_V1:<dom>:<seq>:<sender>:<script>:<requested>:<unused>` parsed from
   constrained bytes; `dom` equals state `dom`.
5. Identities: `depositId = H(DEPOSIT-ID, dom, resolverId, 0)`, `candidate = H(CANDIDATE-ID, dom, seq)`.
6. Non-membership of both in `nulRoot` (sorted Merkle or indexed-Merkle accumulator), then insert
   both: `nulRoot := insert(insert(nulRoot, depositId), candidate)`. This is what rules out
   duplication inside an aggregate and across aggregates without TL having to see each id.
7. `k += 1`, `sum += used` (range-checked u64), `listAcc := H(listAcc, depositId, candidate, script, used)`.

**F: finish.** No-op padding step so step count is fixed per aggregate size class.

## 4. Witness schema (prover-supplied, never trusted)

| Item | Bound |
|---|---|
| Header bytes | exact pinned nearcore serialization; max 1 KiB |
| Producer set | max 128 entries (pinned constant; NEAR mainnet value to confirm) |
| Approvals | max = set size; Ed25519 (secp256k1 keys refused in V2) |
| Outcome | NEARPAY01-A narrow profile; 32 children, 32 logs, 4 KiB per log |
| Receipt list | NEARPAY01-B caps: 256 receipts, 256 KiB, 8 KiB args |
| Merkle paths | max 64 siblings, index-derived directions |
| Nullifier witness | low-leaf + path for indexed Merkle, depth 32 |

## 5. Attacks the relation must refute (and where)

| Attack | Refuted by |
|---|---|
| Omission (skip a deposit) | Not refuted: liveness only. Funds stay as `D - M` unminted surplus (Lean `surplus_exact`). Mitigate with permissionless proving and archival data availability |
| Omission of a debit (reverse path) | TLSTATE01 completeness (PROTOCOL.md section TLSTATE01), not this relation |
| Duplication within an aggregate | P6 non-membership + insert |
| Duplication across aggregates / epochs / verifier versions | z0[10..11] CAS against TL's root; nullifier root migrated unchanged (audit section 5) |
| Substitution of recipient, amount, token, vault, route | P2-P4: all bound to constrained bytes of authenticated receipts |
| Substitution of the head | H steps from pinned z0 checkpoint; z0 pinned by program (section 6) |
| Circular authentication | No input of this relation is derived from TL state except `nulRoot` (an exclusion set, cannot create credit) and the binding element. The NEAR head is never imported from a TL receipt or a NEAR-side contract's claim |
| Alternate receipt of the same graph | depositId from the terminal resolver AND candidate from the vault sequence, both consumed |
| Proof copied from mempool | binding element (sender, expiry) at z0[0] |

## 6. Required changes to TL type 36 before any custody effect

1. **Pinned initial state.** Add `z0_pins: Vec<(u32 index, [u8;32] value)>` to `ZkProgram`
   (included in `program_id_of`); verification rejects if any pinned index differs. Without
   this, IVC soundness says nothing about where the chain started.
2. **Statement nullifier.** Effects other than `StatementReceipt` must consume
   `public_inputs_hash` minus the binding element (or an explicit nullifier output) in a
   persistent set, so the same proof cannot produce two credits.
3. **Domain-pinned program.** The custody effect accepts only the program id recorded in the
   domain at activation; new programs require the upgrade procedure (audit section 5).
4. **No recorded verdicts for value effects.** `RecordedZkVerifier` (`zk_verify.rs:527-563`) may be
   used only for StatementReceipt replay from the node's own receipts.

## 7. Trust assumptions of this statement

Knowledge soundness of Nova (Pallas/Vesta ppsnark IPA) for the registered key; the key compiled
reproducibly from the reviewed circuit source; SHA-256 collision resistance; Ed25519 EUF-CMA;
NEAR consensus with <1/3 Byzantine stake per traversed epoch and LC advanced within the unbonding
period; exact nearcore serialization for the pinned protocol version (resharding and protocol
upgrades are new profile versions); pinned token and vault code (code hash in the domain policy).

## 8. Cost notes (estimates, unmeasured)

- In-circuit Ed25519 over Pallas is non-native. Order-of-magnitude estimates in the literature
  are 10^5 to 10^6 R1CS constraints per signature; an epoch crossing needs signatures worth >2/3
  stake. Measure before committing to H-step design. Alternative: verify approvals for one
  header per epoch only, and use block-Merkle-root ancestry for everything else (NEARPAY01-A
  already folds block paths).
- A 256 KiB receipt list is 4,097 SHA-256 compressions (NEARPAY01-B). Prefer small lists and the
  full-retain profile first.
- Existing type-36 budgets: 30,000-byte node frame, 4,000 ms per source block; NHC1 1-header step
  N=2^20 compresses in 199 s / 2.1 GiB. A deposit aggregate is larger; benchmark before activation.
