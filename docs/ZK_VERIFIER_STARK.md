# ZK Verify, STARK profile (system 1)

Status: design, 2026-11-09. Shared frame (opcode 36, program registry, statement binding,
admission budget, data availability, effects): [ZK_VERIFIER.md](ZK_VERIFIER.md).

## Why a STARK

- Transparent: no trusted setup, no ceremony.
- Hash-based: soundness rests on the hash and on FRI, not on discrete log, so it survives a
  quantum adversary that breaks secp256k1 and Pallas/Vesta.
- The verifier is field arithmetic, a hash and FRI. That is the smallest verifier to write and
  review inside a crate that admits no third-party code.

The costs are proof size and the work of fixing an AIR, below.

## The one-AIR rule

A STARK verifier evaluates the constraints of one specific AIR. Compiling a new AIR into
consensus for every program would make each new statement a consensus code change. The profile
therefore puts **exactly one AIR in consensus: a fixed final-layer recursion AIR**. Its public
inputs are `(inner_program_vk_hash, statement_public_inputs)`, and it attests that a proof of the
inner program with those public inputs verified.

Every statement program (credential membership, a Litecoin header chain, an oracle signature
check) is proved off-chain with any prover. That proof is then recursively compressed into a
proof of the final-layer AIR. `RegisterZkProgram` records the inner program's vk hash; consensus
code never changes per program.

## Profile parameters

| parameter | choice | reason |
|---|---|---|
| field | Mersenne-31 with the degree-4 secure extension QM31 (Circle STARK) | the field of Stwo, which already proves this project's Cairo work (zkTL, `chain_segment`, Raito) |
| commitment hash | SHA-256 for Merkle trees and the Fiat-Shamir channel | `tradelayer-core` already implements and tests SHA-256; no second hash enters consensus |
| low-degree test | circle FRI, fixed blowup, fixed query count | parameters frozen per `verifier_version` |
| security target | at least 100 bits under the standard FRI conjecture, with proof-of-work grinding counted separately and stated | the target the project's STARK projections already use (`zk-rusto-no-bitcoino` M25a) |
| final-layer AIR | one fixed recursion AIR with at most 2^18 rows and 128 columns | the `zk-rusto-no-bitcoino` M25a model (`final_layer_model.py`) puts a final proof under about 80 KB at 100 conjectured bits within this bound |

SHA-256 is slower to prove inside the recursion than Blake2s or Poseidon2. The final layer is
proved once per statement, off-chain, and the gain is a single hash function in consensus. If
prover cost proves prohibitive, the alternative is Blake2s, which is a second, simple,
fully-specified hash to implement and test; the choice is recorded per `verifier_version`.

## Proof size and data availability

- Target final proof: under 80 KB, the M25a model's projection; it is measured once the final-layer AIR exists. It is carried in a Taproot script-path witness
  envelope of the type-36 transaction, and the payload holds its SHA-256 hash.
- An uncompressed Stwo proof is far too large: `chain_segment` measured 15 MB per 8 headers
  without recursion (`btc-header-fold` comparison). Recursion to the final layer is mandatory.
  `MAX_ZK_PROOF_BYTES` for this profile is set at 128 KB, so standard relay holds with margin
  under the 400,000-weight standard transaction limit.

## Verifier, in this crate

1. M31 and QM31 arithmetic, and circle-group point arithmetic and domains.
2. Merkle path verification and the SHA-256 Fiat-Shamir channel.
3. Circle FRI verification: folding, queries, final polynomial.
4. Out-of-domain evaluation of the final-layer AIR's constraints and composition polynomial.
5. Proof decoding with every length bounded before allocation, and typed errors throughout.

Independent oracle: the reference prover (Stwo, pinned commit) produces proof vectors and a
script re-derives every Fiat-Shamir challenge. Tests cover valid proofs, every single-byte flip,
truncation, cross-program substitution and a wrong binding prefix.

## Effort and order

- Generic verifier (steps 1-3, 5): self-contained and comparable in size to the existing
  hand-written `secp256k1.rs` plus a FRI layer.
- Final-layer AIR (step 4): the heaviest piece. It must match the recursion circuit the prover
  side uses byte for byte; that circuit is chosen and frozen first.
- Prover-side pipeline (off-chain): inner program proof, then recursion, then final layer. It is
  outside consensus but needed for any end-to-end test.

## Risks

- Freezing the final-layer AIR ties consensus to one recursion design. A later prover ecosystem
  needs a new `verifier_version`, activated at a height.
- Conjectured, not proven, FRI soundness at 100 bits. The doc of each `verifier_version` states
  proven and conjectured bits separately.
- Large witness payloads raise transaction fees for submitters; the per-block byte budget keeps
  nodes safe, not cheap.
