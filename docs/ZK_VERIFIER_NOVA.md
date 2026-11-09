# ZK Verify, Nova profile (system 2)

Status: verifier implemented in `src/zk_nova/` (2026-11-09). Shared frame (opcode 36, program
registry, statement binding, admission budget, data availability, effects):
[ZK_VERIFIER.md](ZK_VERIFIER.md). Exact algorithm, byte layouts and source citations:
[ZK_NOVA_SPEC.md](ZK_NOVA_SPEC.md). Reference vectors: `fixtures/zk_nova/README.md`.

## Why Nova

- Small proofs that fit inline in the OP_RETURN payload: 12,784 bytes for the NHC1 header-chain
  circuit, independent of the number of folded steps.
- Folding fits long incremental statements: a header chain, a running balance, a sequence of
  oracle readings. Prover memory stays flat per step.
- One generic verifier for every circuit. Registering a program is registering its compact
  verifying key; no per-program code enters consensus.
- No trusted setup: Pallas/Vesta with IPA commitments.

The costs: discrete-log security (not post-quantum), and a two-curve verifier with an IPA and a
Spartan sum-check, written by hand in this crate.

## Profile parameters (`PROFILE_PPSNARK_IPA_PASTA_V1`)

| parameter | choice |
|---|---|
| reference prover | nova-snark 0.76.0 with halo2curves 0.9.0, pinned in the vector generator's `Cargo.lock` |
| proof type | `nova::CompressedSNARK<PallasEngine, VestaEngine, C, S1, S2>` |
| compression | `S1`, `S2` = `spartan::ppsnark::RelaxedR1CSSNARK<E, ipa_pc::EvaluationEngine<E>>` on each curve |
| curve cycle | Pallas (primary) / Vesta (secondary) |
| commitment | IPA over each curve; generators from `from_label(b"ck", n)` and `from_label(b"ipa", 2)` |
| random oracles | Poseidon over each base field for the folds; Keccak256 transcript for Spartan and the IPA |
| security | about 128 bits classical; not post-quantum |

### Why ppsnark and not `spartan::snark`

`spartan::snark`, the compression `nova-header-chain` uses today, puts the full R1CS shape into the
verifying key: about 250 MB for NHC1. A consensus registry cannot pin that. The preprocessing
SNARK (`ppsnark`) replaces the shape with seven commitments per curve (row, column, three value
vectors, two read-timestamp vectors), so the verifier needs only:

| field | bytes |
|---|---:|
| profile tag | 1 |
| `F_arity` | 8 |
| `pp_digest` | 32 |
| per curve: `num_cons`, `num_vars`, `N`, generator count | 2 × 32 |
| per curve: seven spark commitments, compressed | 2 × 224 |
| **`RegisteredNovaKey`** | **553** |

Everything else in nova's verifying key (8.9 MB for the cubic test circuit, 38.3 MB for NHC1) is
derived by the node: both generator tables (SHAKE256, then the halo2curves hash-to-curve; spec
section 4.1), the Poseidon round constants and MDS matrices, `dk = ck.h`, and the two ppsnark key
digests (SHA3-256 over the reconstructed key). The test
`registered_key_round_trips_and_expands_to_the_fixture_vk` rebuilds nova's full key from the 553
bytes and matches its SHA-256 and both digests.

A registry pins `SHA-256(RegisteredNovaKey::encode())`. Anyone can audit a registered key by
running nova-snark's setup on the published step circuit and comparing.

## Proof size

`1064 + 8 + 32·a + 2·(864 + 104·r_out + 168·k)` bytes, with `a` the arity, `r_out = log2(num_cons)`
and `k = log2(N)` per curve (spec section 3.4). Measured:

| circuit | arity | `N` primary / secondary | proof |
|---|---:|---|---:|
| cubic test circuit (`z' = z^3 + z + 5`) | 1 | 2^17 / 2^17 | 11,456 B |
| NHC1, one header per step | 17 | 2^20 / 2^17 | 12,784 B |

## Prover cost, NHC1

| step size | `N` primary | public-parameter setup | compress | peak RAM |
|---|---|---:|---:|---:|
| 1 header per step (10 steps) | 2^20 | 74 s | 199 s | 2.1 GiB |
| 10 headers per step (1 step) | 2^23 | | 1,490 s | 13.8 GiB |

The first registered program uses one header per step: same proof size, an 8× smaller verifier
MSM, and a prover that runs on a laptop.

## Verification cost

Measured in release on this project's laptop (`tests/zk_nova_verify.rs`, `timings_report`):

| step | cubic | NHC1 |
|---|---:|---:|
| derive `ck` generators, Pallas (once per process) | 1.1 s | 9.2 s |
| derive `ck` generators, Vesta (once per process) | 1.1 s | 1.1 s |
| expand the registered key, generators cached | 115 ms | 331 ms |
| `zk_nova::verify`, generators cached | 483 ms | 1.81 s |
| nova-snark's own `verify` of the same proof (vector generator, 8 threads) | 1.45 s | 4.52 s |

Verification is dominated by one multi-scalar multiplication of `N + 2k + 2` terms per curve
(spec section 10), so it is linear in `N`. `MAX_N` caps a key at 2^23; each program entry records
its measured worst case, and the shared per-block budget bounds a block's summed cost.

## Verifier, in this crate

`src/zk_nova/`, zero external crates, `forbid(unsafe_code)`:

| module | content |
|---|---|
| `field`, `curve`, `msm` | Pallas/Vesta fields and points, compressed encodings, Pippenger MSM |
| `keccak`, `h2c` | Keccak256, SHA3-256, SHAKE256, SHA-256 `expand_message_xmd`, SvdW hash-to-curve, `from_label` |
| `transcript`, `poseidon` | nova's Keccak transcript and Poseidon random oracle, constants derived |
| `polys`, `sumcheck`, `ipa`, `ppsnark` | Spartan ppsnark verification and the IPA opening |
| `nova` | `CompressedSNARK::verify`: hash binding, three folds, derandomisation, two ppsnark checks |
| `codec`, `key`, `error` | bounded proof decoding, `RegisteredNovaKey`, `GeneratorCache`, typed errors |

API: `zk_nova::verify(&RegisteredNovaKey, &GeneratorCache, num_steps, z0, proof) -> Result<zn,
NovaVerifyError>`. Limits: `MAX_PROOF_BYTES` 262,144, `MAX_N` 2^23, `MAX_ARITY` 4096; every
length prefix is checked against the remaining input before allocating.

## Statement layout in type 36

A Nova program's public inputs are `z0 || zn`: `F_arity` canonical Pallas-scalar elements each,
32-byte little-endian, so `public_input_layout = 2 * F_arity`. `z0[0]` is the binding element of
the shared design. The node accepts only if the proof takes exactly that `z0` to exactly that `zn`
in the envelope's step count, so a receipt commits to the start state, the result and the sender.
The binding needs no support from the circuit: `z0` enters the hash that the proof's last
instance carries, so a proof for one sender's `z0` does not verify for another's. A circuit whose
`z0` is fully meaningful (NHC1 starts from a header-chain state) registers with one extra state
element for the binding.

`RegisterZkProgram` checks the key with `check_nova_key`: it decodes as a `RegisteredNovaKey`,
each curve's generator count equals its `N`, and the layout is twice the arity.

The verifier rejects, with a typed error, every input on which nova-snark would panic (spec
section 11), and every key nova cannot produce: shapes that are not powers of two or that underflow
nova's index arithmetic (`num_vars < 8`, `N < 2·num_vars`, `N < num_cons`), generator counts below
`N`, and a `pp_digest` of 2^250 or more (nova truncates the digest to 250 bits).

## Tests and independent oracle

The oracle is nova-snark 0.76.0 itself, through the vector generator (`fixtures/zk_nova/README.md`):

- Math layer (`tests/zk_nova_math.rs`): field, curve, encoding, hash-to-curve, generator and hash
  vectors.
- Parts (`tests/zk_nova_parts.rs`): the scripted transcript sessions that avoid `squeeze_bits`
  (the verifier never calls it), the scripted Poseidon sessions, and a replay of every Poseidon
  squeeze and transcript operation nova performs while verifying each fixture proof.
- End to end (`tests/zk_nova_verify.rs`): the three real proofs verify and return nova's `zn`; the
  corrupted proofs, wrong step counts, wrong `z0`, a wrong key, every truncation, inflated length
  prefixes and every spec section 11 shape are rejected without a panic.
- Type 36 (`tests/zk_verify.rs`): real proofs through `ZkNovaAdapter`, and a proof bound to one
  sender recorded through a block while two copies by another sender are rejected.

Where each runs: debug builds (`cargo test`, the CI `tests` job) skip the cases that derive large
generator tables or verify full proofs; the CI `release` job runs all four suites on its release
build. Three exhaustive checks are explicit `--ignored` runs
(`cargo test --release --test zk_nova_math --test zk_nova_verify -- --ignored --skip bench_`).
On commit 48ddd67 they took 31 minutes and passed:

| check | result |
|---|---|
| `from_label` at 2^23 + 1 points (`MAX_N`), both curves, first 16 points and digests of the full tables | equal to nova's |
| bit 0 flipped in each of the 11,456 bytes of the cubic proof | every proof rejected, no panic |
| bit 7 flipped in every byte outside the IPA vectors and in every IPA point's sign byte | every proof rejected, no panic |

## Fit with existing work

- NHC1 (`nova-header-chain`) is the first program: a Bitcoin or Litecoin header-chain checkpoint,
  for example a Litecoin checkpoint recorded on the Bitcoin-first TradeLayer chain as the basis of
  a Litecoin bridge leg. To register it moves from `spartan::snark` to `ppsnark` compression and
  adds the binding state element.
- Raito-Scrypt proves Litecoin chain and payment facts with SP1 (Groth16-wrapped); that is a
  different system and stays outside this profile.

## Risks

- Not post-quantum. A quantum adversary that breaks discrete log forges proofs; the STARK profile
  is the post-quantum route.
- The verifier mirrors one prover version's transcript and encoding. A nova-snark upgrade that
  changes either is a new profile tag and `verifier_version`, activated at a height.
- Verification time is linear in `N`. The registry rejects programs whose measured worst case
  exceeds the per-proof budget.
