# ZK Verify, Nova profile (system 2)

Status: design, 2026-11-09. Shared frame (opcode 36, program registry, statement binding,
admission budget, data availability, effects): [ZK_VERIFIER.md](ZK_VERIFIER.md).

## Why Nova

- Small proofs. A Spartan-compressed Nova proof of 100 Bitcoin headers is about 13 KB
  (`btc-header-fold`; the `nova-header-chain` statement NHC1 is about 13.5 KB with its bundle),
  so proofs ride inline in the OP_RETURN payload with no witness envelope.
- Folding fits long incremental statements: a header chain, a running balance, a sequence of
  oracle readings. The prover's memory stays flat per step (1.3 GiB at 10 steps per fold
  measured in `btc-header-fold`), which made it the route that escaped the 28 GiB STARK memory
  floor on this project.
- One generic verifier: a compressed Nova proof verifies against a verifying key that carries the
  R1CS shape digest of the step circuit. Registering a program is registering its vk hash; no
  per-program AIR enters consensus.
- No trusted setup: Pallas/Vesta with IPA commitments.

The costs: discrete-log security (not post-quantum), and a verifier with two curves, an IPA and a
Spartan sum-check to write by hand.

## Profile parameters

| parameter | choice | reason |
|---|---|---|
| curve cycle | Pallas (primary) / Vesta (secondary) | the cycle `btc-header-fold` and `nova-header-chain` already use |
| commitment | IPA (Bulletproofs-style) over each curve | transparent; no powers-of-tau file |
| compression | Spartan with IPA, as the nova-snark `CompressedSNARK` | the measured 13 KB proofs above are this compression |
| random oracle | Poseidon over the curve scalar fields, with the exact constants of the pinned prover | must match the prover's transcript bit for bit |
| security | about 128 bits classical (discrete log in ~255-bit groups) | stated per `verifier_version`; not post-quantum |

The verifying key is pinned by digest at registration. The existing NHC1 vk at k=10 has digest
`e46bb069…1698` and was identical on Windows and Linux, which is the reproducibility a consensus
pin needs.

## Verification cost

Native verification is dominated by the IPA's multi-scalar multiplication, which is linear in the
commitment size (the number of generators). The circuits this project has measured use about
2^19 generators. Two numbers:

- `btc-header-fold`: verifying 100 compressed headers took 2.1 s on an 8-vCPU Haswell box
  (unoptimised path).
- The same MSM is what made a Bitcoin Script verifier infeasible (BitVM2 M23 projected about
  588 TiB of Script). That does not apply here: TradeLayer nodes verify natively, where a
  Pippenger MSM of this size is CPU-bound in fractions of a second to seconds.

Each program entry records its measured worst-case verification time. The per-block budget in the
shared design (`MAX_ZK_VERIFY_PER_BLOCK`, summed worst case) keeps a block of Nova proofs inside a
fixed CPU bound; at 2 s per proof that means a single-digit number of Nova proofs per block until
the verifier is optimised.

## Verifier, in this crate

1. Pallas and Vesta base and scalar field arithmetic, and point arithmetic (complete formulas,
   constant-shape decoding), alongside the existing hand-written `secp256k1.rs`.
2. Poseidon over both scalar fields, with the pinned constants and transcript layout.
3. Spartan sum-check verification, and IPA opening verification with a Pippenger MSM.
4. Nova's relaxed-R1CS instance checks for the folded and the last step's instances, plus the
   vk and R1CS-shape digest check.
5. Proof decoding with bounded lengths and typed errors; never a panic on hostile bytes.

Independent oracle: the pinned nova-snark prover produces vectors (valid proofs, the vk digest,
transcript challenges), and a separate script re-derives the transcript. Adversarial tests:
every single-byte flip, truncation, a proof under the wrong vk, a swapped primary/secondary
instance, and a wrong binding prefix.

## Fit with existing work

- `nova-header-chain` (NHC1) is a ready step circuit: a Bitcoin or Litecoin header chain
  checkpoint. It is a natural first registered program, for example a Litecoin header checkpoint
  recorded on the Bitcoin-first TradeLayer chain, the basis of a Litecoin bridge leg.
- Raito-Scrypt proves Litecoin chain and payment facts with SP1 (Groth16-wrapped); that is a
  different system and stays outside this profile.

## Effort and order

- Curve and field arithmetic (step 1): largest piece, with the most review surface.
- Poseidon and transcript (step 2): small, but bit-exact with the prover or nothing verifies.
- Spartan and IPA (step 3), then Nova instance checks (step 4).
- End-to-end test: register the NHC1 vk on testnet4 staging and verify a 100-header proof
  through `tradelayer-node`.

## Risks

- Not post-quantum. A quantum adversary that breaks discrete log forges proofs; the STARK profile
  is the post-quantum route.
- The verifier mirrors one prover implementation's transcript and encoding. A prover upgrade that
  changes either is a new `verifier_version`.
- Verification time is linear in circuit size. The program registry rejects programs whose
  measured worst case exceeds the per-proof budget.

## Recommendation between the profiles

Build the Nova profile first. Its proofs fit inline, its first program (NHC1) and vk pin already
exist, and it needs no consensus-side AIR. Then build the STARK profile, which needs the frozen
final-layer recursion AIR and a working recursion pipeline, as the post-quantum route.
