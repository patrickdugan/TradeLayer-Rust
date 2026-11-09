# In-protocol ZK verification: shared design

Status: design 2026-11-09; rollout step 1 is built for the Nova profile (`src/zk_nova/`,
[ZK_VERIFIER_NOVA.md](ZK_VERIFIER_NOVA.md)) and step 2 for phase 1 on the joint typed path
(section "Phase 1 as built" below). The STARK profile is not built. Before this work
TradeLayer-Rust had no in-protocol proof verifier: `src/zk_batch.rs` only inspects zkTL Cairo batch claims and prints
`stwo_proof=UNVERIFIED`, and the legacy proof-related types (30, 32-35) are ported only as far as
the pinned JavaScript goes (`docs/SOURCE_PROOF_LIFECYCLE.md`, `docs/ZK_CAIRO_INTEGRATION.md`).

Two proof systems are designed against this shared frame:

- [ZK_VERIFIER_STARK.md](ZK_VERIFIER_STARK.md): a transparent, hash-based STARK verifier.
- [ZK_VERIFIER_NOVA.md](ZK_VERIFIER_NOVA.md): a Nova folding SNARK over Pallas/Vesta with
  Spartan compression.

## Opcode

A new transaction type, **36 `ZK Verify`**. Types 0-35 are taken, and type 34 is already
ambiguous (tradelayer.js: Cross-Layer Bridge; zkTL: ZK batch move), so no existing number is
reused. Type 36 is inert until the Registry `Activate` action enables it at a height, like every
other type (`docs/ACTIVATION*.md`); before activation the source runner rejects it as unknown.

Payload (after the `tl` marker, in the wire codec):

| field | meaning |
|---|---|
| `system` | 1 = STARK profile, 2 = Nova profile (each doc defines one fixed profile) |
| `program` | 32-byte id of a registered statement program (below) |
| `public_inputs` | the statement's public inputs, length fixed by the program entry |
| `proof_ref` | either the proof inline, or a 32-byte hash of a proof carried in the same transaction's witness (see Data availability) |
| `expiry` | last Bitcoin height at which this transaction may be admitted |

## Program registry

A proof means nothing without a fixed statement. Proofs are accepted only for **registered
programs**:

- A new Registry action `RegisterZkProgram { system, program_id, verifying_key_hash,
  public_input_layout, effect, verifier_version }`. It is admitted only from the registry admin
  (the same authority as other Registry actions), and entries are immutable once recorded.
- `program_id` is the hash of the circuit/AIR description and its verifying key, so a changed
  circuit is a new program.
- `verifier_version` pins the exact verifier code path. A verifier bug fix is a new version
  activated at a height, never a silent change: old proofs keep their verdicts.

## Statement binding

Every registered program's public inputs **must** begin with this binding prefix, and the
verifier refuses a proof whose prefix does not equal the values the node computes itself:

1. Network genesis hash and the TradeLayer economic anchor (`EconomicConfig::anchor`).
2. The sender address (as authenticated by admission slice 1, `docs/ADMISSION.md`) and the
   transaction's `expiry`. This stops a third party from copying a proof out of the mempool
   and claiming its effect, and stops replay after expiry.
3. The program id.

Programs that read TradeLayer state also bind a **state root at a stated height**. Today no
root is shared between the Rust ledger and any circuit (zkTL's roots are a different namespace,
`docs/TLPT_ZKTL_ALIGNMENT.md`). A state-reading program therefore needs a canonical,
circuit-friendly commitment of the relevant namespace maintained by the economic owner, defined
per system (Poseidon2 for the STARK profile, Poseidon over Pallas for Nova). That commitment is a
consensus format change and is phase 2 below.

## Admission and determinism

- Verification is a pure function of (verifier version, program entry, public inputs, proof
  bytes). It must be bit-exact across implementations. It runs in the source runner's validity
  phase, like every other type check, and an invalid proof is a rejected transaction with a
  typed reason, never a panic.
- **Budget.** A block admits at most `MAX_ZK_VERIFY_PER_BLOCK` type-36 transactions and at most
  `MAX_ZK_PROOF_BYTES` of proof data. Each program entry records a measured worst-case
  verification time; a block's summed worst case stays under a per-block limit, so a block
  full of proofs cannot stall nodes.
- **Fee.** Each verification charges a protocol fee in the activation's fee property, debited
  before verification and kept on failure, so failing proofs cost the submitter.
- **Zero external crates.** `scripts/check_rust_supply_chain.py` admits no third-party crate in
  the consensus package. Both verifiers are written in this crate from their specifications,
  and checked against reference provers through test vectors (fixtures plus an independent
  script, as the other independent oracles in `scripts/` do). Admitting a verifier crate instead
  would be a supply-chain policy change for the owner to decide.

## Data availability

The proof must be retrievable by every node that replays the chain.

- Small proofs (the Nova profile) go inline in the OP_RETURN payload. Bitcoin Core 31.1, pinned
  in this repository, relays large OP_RETURN outputs by default; a standard transaction stays
  under 400,000 weight units.
- Large proofs (the STARK profile) go in the spending input's witness (a Taproot script-path
  envelope), and the payload carries their hash. Witness bytes are in the block, so DA equals
  block availability and `admission.rs` already binds witnesses through the BIP141 commitment.
- Off-chain storage with an on-chain hash is not admitted: a node that cannot fetch the proof
  could not decide the transaction, which would make consensus depend on a server.

## Effects

Phase 1 (no custody movement): a verified proof records a **statement receipt**
`(program_id, public_inputs hash, sender, height, txid)` in the registry. Other types read
receipts through registered effects:

- ZK credential attestation: a proof of membership in an issuer's credential set
  (no identity revealed) records an attestation on a clearlist, which `tl_check_kyc` then
  reports like any other active attestation.
- Cross-chain facts: a proof of a Litecoin or other-chain header chain or payment
  (the Raito-Scrypt and `nova-header-chain` statements) records a checkpoint other types can
  reference, for example a Litecoin-side bridge leg.
- Oracle data proven from a signed source, recorded as an oracle publication.

Phase 2 (custody movement) needs the state-root commitment above and a reviewed circuit:

- Type 32-style batch moves: a proof that a batch of transfers takes the committed root S to S',
  with the economic owner checking S against its own root before applying S'.
- Bridge legs that release TradeLayer balances against a proven other-chain event.

Phase 2 is a consensus change to state commitments; it is not started until phase 1 is live.

## Rollout

1. Specify the profile (each doc), write the verifier and its independent oracle, and run
   adversarial vectors: truncated, bit-flipped and cross-program proofs, plus prefix mismatches.
2. Implement type 36 and `RegisterZkProgram` behind activation, phase 1 effects only.
3. Testnet4 staging with `tradelayer-node`: register a test program, submit valid and invalid
   proofs, and check the deterministic verdicts across two nodes.
4. Mainnet activation height set by the registry admin.

## Phase 1 as built (`ZkVerifyV1`)

Code: `src/zk_verify.rs` (types, frame codec, binding, budget, verifier trait, mock),
`src/registry_zk.rs` (registry state, type-36 transition, snapshot section), `src/joint.rs` and
`src/joint_chain.rs` (fee and block loop), `src/service_routes.rs` and `src/service_view.rs`
(`tl_getZkReceipts`). Tests: `tests/zk_verify.rs`, one case in `tests/service_view.rs`.

**Placement.** `ZkVerifyV1` is an opt-in policy of the registry, set before the first block
(`Registry::new_zk_verify_v1`, `JointChainState::enable_zk_verify_v1`) with the fee configuration.
Without it nothing changes: `Activate` still rejects type 36 as `InvalidType`, `MAX_ACTIVATION_TYPE`
stays 35, the 36-slot activation array and `TRANSACTION_CATALOG` are untouched, and registry
snapshots stay `TLRG\x01`. With it, `Activate` accepts type 36 (stored beside the array in
`zk_activation`), and `RegisterZkProgram` and type 36 are checked against that activation like any
other type (not active, or activated in the same block, is an invalid transaction).

**Built as designed.**

- `RegisterZkProgram { system, program_id, key_bytes, public_input_layout, effect,
  verifier_version, max_verify_ms }`: admin only, immutable, keyed by `program_id` =
  SHA-256(`TradeLayer/ZkProgram/v1/` || canonical encoding of the other fields). Duplicate
  refused, id mismatch refused, system 1 (STARK) refused, `key_bytes` 1..4096 bytes, layout
  1..64 elements, `max_verify_ms` 1..4000. `key_bytes` must decode as a Nova
  `RegisteredNovaKey` whose generator counts equal its `N` and whose arity is half the layout
  (`check_nova_key`); this needs no generator work.
- Type 36 payload `{ system, program_id, public_inputs, proof, expiry }`, binary frame
  `"tl" 0x24 | system | program_id | expiry u64 | n u32 | n*32 | proof_len u32 | proof`.
  The legacy ASCII parser cannot express type 36 (one base-36 selector character) and still
  rejects it.
- Binding: public input 0 must equal `binding_element(genesis, anchor, sender, expiry, program_id)`,
  derived by the node (genesis from the network profile, anchor from the chain's anchor tip,
  sender from the transaction actor). Mismatch rejects. The verifier receives the inputs with the
  binding element first.
- Verifier boundary `ZkVerifier::verify(system, key_bytes, num_steps, public_inputs, proof) ->
  Result<VerifiedStatement, ZkReject>`. `ZkNovaAdapter` is the Nova verifier: the public inputs
  are `z0 || zn` (`F_arity` canonical Pallas-scalar elements each, binding element as `z0[0]`),
  and it accepts only if `zk_nova::verify` takes exactly that `z0` to exactly that `zn` in
  `num_steps` steps. Generator tables are derived once per process. `MockZkVerifier` accepts one
  magic proof and serves the consensus tests that do not exercise proof math.
- Budgets per block: `MAX_ZK_VERIFY_PER_BLOCK` = 8, `MAX_ZK_PROOF_BYTES` = 256 KiB summed,
  `MAX_ZK_BLOCK_VERIFY_MS` = 4000 summed over `max_verify_ms`, plus `MAX_ZK_TX_PROOF_BYTES` =
  64 KiB per transaction. Charged in block order immediately before the verifier runs; a
  transaction over a cap is rejected and charges nothing.
- Fee: a ledger transfer of `ZkFeePolicy.amount_atoms` of `ZkFeePolicy.property` from the sender to
  `ZkFeePolicy.sink`, debited before verification and kept on every rejection. The joint owner has
  no fee cache, and its sole ledger is the existing mechanism that moves property balances, so the
  "activation's fee property" of the design is this configured property. The fee transfer's
  replay id is SHA-256(`TradeLayer/ZkFee/v1/` || txid). An unpayable fee makes the transaction
  invalid (the block is refused), like any other transfer that cannot be afforded.
- Outcomes: after activation and fee, every failure is a typed `ZkRejectReason` (expired,
  unknown program, system mismatch, oversize proof, public input count, the three budgets, binding
  mismatch, proof envelope, receipt table full, verifier verdict, statement-hash mismatch). The
  transaction is consumed (its txid is a replay marker) and the block continues. Rejections are
  returned in `ZkTxReport`s and are not persisted.
- Effect: a verified type 36 appends `StatementReceipt { program_id, public_inputs_hash, sender,
  height, txid }` to the registry. No balance moves. At most 100,000 receipts.
- Query: `POST /tl_getZkReceipts` with `programId` (64 hex) and/or `address`, target
  `registry.zk_receipts`, reading `JointProtocolState::registry()` through the economic read view.
- Snapshot: registries with the policy on write `TLRG\x02` (v1 body, then fee policy, type-36
  activation, programs, receipts); all others write the unchanged v1. A v2 body under the v1 magic
  is refused. Receipts roll back with the block (the registry is part of the cloned pre-block
  state) and travel in `TLJP`/`TLJC` checkpoints inside the embedded registry bytes.

**Differences from the design.**

- No `num_steps` field exists in the designed payload and the Nova verifier needs one. The first
  four bytes of `proof` carry it (big-endian, at least 1); the rest is the proof. Binding does not
  cover it; a wrong value fails verification.
- The binding prefix is one field element (a SHA-256 of the five values with the top three bits
  cleared), not five, because genesis and anchor are 256-bit values and do not fit one Pallas
  element.
- Type-36 items travel as `JointBlockItem::Zk` in `UnverifiedJointZkBlock`
  (`JointChainState::apply_unverified_zk_block`), not as a `JointOperation` variant, so no existing
  exhaustive match over operations and no economic history format changed. The one edit outside
  the registry and joint owners is an arm in `src/economic_snapshot_impl.rs`: the economic history
  writer refuses `RegisterZkProgram` (no tag; format unchanged).
- Only the joint typed path is integrated. The sequential source runner
  (`src/consensus_source.rs`) and the Types decoder execute pinned JavaScript and have no type 36;
  a `SourceConsensusPolicy` for it is not built. The economic owner (`EconomicState`) cannot enable
  the policy, so `tl_getZkReceipts` over an economic state returns an empty list today.
- `tradelayer-node` does not decode type-36 frames from Bitcoin transactions yet; the sender comes
  from the caller-supplied actor, as for every other typed joint transaction.

**Not built.** The STARK verifier, data availability through witnesses, state-root binding,
receipt-consuming effects (clearlist attestation, checkpoints, oracle data), and all of phase 2.

**End to end.** `tests/zk_verify.rs`
(`bound_real_proof_records_a_receipt_and_cannot_be_reused_by_another_sender`, release build)
registers the cubic test circuit, submits a real nova-snark proof whose `z0` is alice's binding
element, and records her receipt; in the same block bob's unchanged copy is rejected for its
binding and bob's copy with his own binding element is rejected by the verifier. All three pay the
fee.
