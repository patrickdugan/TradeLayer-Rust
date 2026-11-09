# In-protocol ZK verification: shared design

Status: design, 2026-11-09. Nothing here is implemented. TradeLayer-Rust has no in-protocol proof
verifier today: `src/zk_batch.rs` only inspects zkTL Cairo batch claims and prints
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
