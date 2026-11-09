# ZK Nova: exact verifier specification (nova-snark 0.76.0, `CompressedSNARK`, Pallas/Vesta, ppsnark + IPA)

Status: specification, 2026-11-09. Companion to [ZK_VERIFIER_NOVA.md](ZK_VERIFIER_NOVA.md) (design) and
[ZK_VERIFIER.md](ZK_VERIFIER.md) (shared frame). This document is the byte-level and algorithm-level
contract for a zero-dependency verifier. It describes what the pinned prover library does; nothing here is
a design choice.

## 0. Scope, pins, citation form

Target type:

```
nova_snark::nova::CompressedSNARK<E1, E2, C, S1, S2>
  E1 = PallasEngine   E2 = VestaEngine
  S1 = spartan::ppsnark::RelaxedR1CSSNARK<E1, provider::ipa_pc::EvaluationEngine<E1>>
  S2 = spartan::ppsnark::RelaxedR1CSSNARK<E2, provider::ipa_pc::EvaluationEngine<E2>>
```

This is the preprocessing SNARK (`ppsnark`), not `spartan::snark`. `C` is a phantom type and has no bytes.

Pinned sources (crates.io registry, `index.crates.io-1949cf8c6b5b557f`):

| tag | crate | role |
|---|---|---|
| `N:` | `nova-snark-0.76.0/src` | protocol, transcript, Poseidon, serde derives |
| `H:` | `halo2curves-0.9.0/src` | Pallas/Vesta/Fp/Fq, point serde, hash-to-curve |
| `D:` | `halo2derive-0.2.0/src` | field macro (`from_uniform_bytes`, `is_odd`) |
| `S:` | `serde_arrays-0.2.0/src` | fixed-array serde |

Citations read `N:path:line` (or a line range). Build assumptions, all taken from `N:Cargo.toml`:
default features (`io` on), feature `evm` **off**. With `evm` on every byte order below reverses; that build is
out of scope. Serialization is `bincode` 2.0.1 `serde` API with `bincode::config::legacy()` (the
configuration nova itself uses in `N:digest.rs:27-31` and in its tests, for example `N:provider/hyperkzg.rs:1352`).
Nova does not fix a configuration for proof or vk files; any producer that wants these layouts must use `legacy()`.

Verification status of this document. A Rust probe (release build against the pinned crates) produced a real
CubicCircuit proof (3 steps, arity 1) and checked: the vk and proof layouts below byte for byte (every byte consumed
exactly), generator derivation (section 4.1) against `from_label`, the standard-form Poseidon permutation (section 5.2)
against `PoseidonRO` for 1 to 30 inputs on both fields, the Keccak transcript (section 5.3), the vk digest rule
(section 5.4) against `VerifierKey::digest()`, and a successful `CompressedSNARK::verify`. Items marked **[V]** are
covered by that probe.

A second, independent verifier was then written from sections 2 to 9 of this document alone (own transcript, own
Poseidon, own sum-check, own IPA equation in the single-MSM form of section 9, own byte parsing; only field and group
arithmetic, SHA-2/SHA-3 and point decompression came from libraries). It accepted the real proof and returned the correct
`zn`, and it rejected: a changed `num_steps`, a changed `z0`, a flipped byte in `r_U_secondary` (hash binding), a flipped
byte in `snark_primary` (outer sum-check), and a corrupted length prefix. Its assertions also confirmed the closed forms
of `s[i]` and `b_hat` in section 9 against the table definitions. That run covers the whole of sections 6 to 9 for one
circuit shape (`r_out = 14`, `k = 17`, arity 1). The probe and the second verifier are not part of this repository; the
numbers they produced are in section 12. Everything not marked **[V]** is transcribed from the cited source lines.

## 1. Fields, curves, the two sides

| name | value |
|---|---|
| `p` (Pallas base field, Vesta scalar field) = `Fp` | `0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001` |
| `q` (Pallas scalar field, Vesta base field) = `Fq` | `0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001` |
| curve (both) | `y^2 = x^3 + 5`, cofactor 1, `a = 0`, `b = 5` (`H:pasta/pallas.rs:24-37`, `vesta.rs:24-37`) |

`q > p`. Both are 255-bit primes (`NUM_BITS = 255`), 32-byte canonical representation.

`Engine` associations (`N:provider/mod.rs:126-148`, `N:provider/pasta.rs:24-31`):

| | E1 = Pallas | E2 = Vesta |
|---|---|---|
| `Base` | `Fp` | `Fq` |
| `Scalar` | `Fq` | `Fp` |
| `RO` (Poseidon) field | `Fp` | `Fq` |
| group `GE` coordinates | `Fp` | `Fq` |

Consequences used throughout:

- The step-circuit state `z0`, `zn`, `pp_digest`, `ri_primary` and everything "primary" (instance `X`, `u`, SNARK
  scalars, transcript scalars) live in **Fq**. Primary commitments are **Pallas** points with `Fp` coordinates.
- Everything "secondary" (`X`, `u`, `ri_secondary`, SNARK scalars) lives in **Fp**. Secondary commitments are
  **Vesta** points with `Fq` coordinates.
- The Poseidon RO that absorbs secondary-side data (`E2::RO`) works over `Fq`; the one for primary-side data
  (`E1::RO`) works over `Fp`.
- `field_switch` (`N:gadgets/utils.rs:82-103`): `scalar_as_base::<E>(x)` and `base_as_scalar::<E>(x)` take the
  canonical integer of `x` and reduce it into the other field: result = (integer of `x`) mod (other modulus).
  Because `p < q`, `Fp -> Fq` never reduces; `Fq -> Fp` reduces when the value is at least `p`
  (for example `q-1` maps to `q-1-p`). Probe: `q-1 as Fp` (**[V]**) is printed in section 12.

## 2. Encoding rules

### 2.1 bincode `legacy()` as used here

| Rust shape | bytes |
|---|---|
| `u8`, `bool` | 1 byte (`bool` is 0 or 1) |
| `u32` (enum variant index) | 4 bytes little endian |
| `u64`, `usize` | 8 bytes little endian (`usize` is always 8) |
| struct | fields in declaration order, no tag, no padding |
| `Vec<T>`, slice | `u64` length, then each element |
| tuple, `[T; N]` via `serde_arrays` | each element, **no length prefix** (`S:serializable.rs:37,80`) |
| enum | `u32` variant index (declaration order), then payload |
| `PhantomData`, `()` | 0 bytes |
| `Option<T>` | `u8` tag 0 or 1, then the payload if 1. **No `Option` occurs in any vk or proof type of this document.** |
| `#[serde(skip)]` field | absent |

### 2.2 Scalar field elements (`Fp`, `Fq`)

32 bytes, canonical little endian (`to_repr`). Written by `H:derive/field/common.rs:111-120`
(`serde_arrays::serialize` of `[u8; 32]`, non-human-readable). Read by `common.rs:94-108`: the 32 bytes must be a
value strictly below the modulus, otherwise deserialization fails. `serde_as(as = "EvmCompatSerde")` is a no-op
without feature `evm` (`N:traits/evm_serde.rs:9-39`, default trait methods), so every `#[serde_as]` scalar and
`Vec<scalar>` in nova is exactly this encoding.

### 2.3 Curve points (`Commitment`, generators, `DerandKey`)

A point is 32 bytes, **compressed** (`H:derive/curve.rs:104`, `188-229`; `SingleSpare` config `H:serde.rs:187`):

- bytes 0..31: `x` in canonical little endian, with the top byte (byte 31) carrying the sign flag in **bit 7**
  (`Flag::flag(0) = 1 << 7`, `H:serde.rs:205-207`, `pos_sign = 0` at `H:serde.rs:246-251`).
- sign flag = parity (low bit of the canonical representation, `to_repr()[0] & 1`) of `y`, forced to 0 for the identity
  (`H:derive/curve.rs:71-73`).
- identity: `x = 0`, sign 0, so 32 zero bytes. There is no separate identity flag for Pasta (`pos_idetity = None`,
  `H:serde.rs:265`).

Decode (`H:serde.rs:326-376`): clear bit 7 of byte 31 and remember it as `sign`; read `x` (canonical, `x < modulus`
else failure); if `x == 0` the result is the identity **whatever `sign` was** (the bit is ignored); otherwise
`y = sqrt(x^3 + 5)` must exist (else failure), and `y` is negated if `parity(y) != sign`. A decoded point is on the
curve and in the group (cofactor 1). Neither `Fp` nor `Fq` has a square root of 5 (**[V]**), so no real point has
`x = 0`; the identity has two accepted encodings, `00..00` and `00..0080` (**[V]**: both decode). A verifier that
rejects the second form diverges from nova; nova accepts it.

`Commitment<E>` is a struct with one field `comm: E::GE` serialized with `EvmCompatSerde`
(`N:provider/pedersen.rs:72-81`): exactly one point, 32 bytes. `CommitmentKey` stores affine points through the same
compressed serde (`N:provider/pedersen.rs:31-38`, no `serde_as`; `PallasAffine`/`VestaAffine` serde at
`H:derive/curve.rs:218-246`, same 32-byte form).

### 2.4 Coordinates for hashing (not for bytes on the wire)

`to_coordinates` (`N:provider/traits.rs:303-312`): for the identity, `(x, y, inf) = (0, 0, true)`; otherwise the
affine `(x, y, false)`. Used by both the RO absorb and the transcript encoding (sections 5.2 and 5.3).

## 3. Data layouts

All integers are `u64` little endian unless stated. `n1`, `n2` are the generator counts of the primary and
secondary commitment keys. `a` is `F_arity`. Sizes are for the layouts exactly as bincode writes them.

### 3.1 `nova::VerifierKey` (`N:nova/mod.rs:700-720`)

| # | field | type | bytes |
|---|---|---|---|
| 1 | `F_arity` | `usize` | 8 |
| 2 | `ro_consts_primary` | `PoseidonConstantsCircuit<Fp>` (3.2) | 271800 |
| 3 | `ro_consts_secondary` | `PoseidonConstantsCircuit<Fq>` (3.2) | 271800 |
| 4 | `pp_digest` | `Fq` (`E1::Scalar`) | 32 |
| 5 | `vk_primary` | `ppsnark::VerifierKey<Pallas, IPA>` (3.3) | `360 + 32*n1` |
| 6 | `vk_secondary` | `ppsnark::VerifierKey<Vesta, IPA>` (3.3) | `360 + 32*n2` |
| 7 | `dk_primary` | `DerandKey<Pallas>`: one point | 32 |
| 8 | `dk_secondary` | `DerandKey<Vesta>`: one point | 32 |
| 9 | `_p` | `PhantomData<C>` | 0 |

Total `544424 + 32*(n1 + n2)` bytes **[V]**. Example (CubicCircuit, `n1 = n2 = 131072`): 8,933,032 bytes.
Measured offsets in that file: `ro_consts_primary` at 8, `ro_consts_secondary` at 271808, `pp_digest` at 543608,
`vk_primary` at 543640, `vk_secondary` at 4738304, `dk_primary` at 8932968, `dk_secondary` at 8933000.

### 3.2 `PoseidonConstantsCircuit<F>` (`N:provider/poseidon.rs:21-25`)

Struct `{ wide: PoseidonConstants<F, U24>, narrow: PoseidonConstants<F, U5> }`. Each `PoseidonConstants` has a hand
written `Serialize` (`N:frontend/gadgets/poseidon/serde_impl.rs:14-34`), fields in this order:

| field | type | note |
|---|---|---|
| `mds` | `MdsMatrices` = six `Matrix` in order `m, m_inv, m_hat, m_hat_inv, m_prime, m_double_prime` (`N:frontend/gadgets/poseidon/mds.rs:15-23`) | `Matrix = Vec<Vec<F>>`: `u64` rows, then per row `u64` cols + `32*cols` |
| `crc` | `Vec<F>` compressed round constants | length `R_F*t + R_P` |
| `psm` | `Matrix` pre-sparse matrix | `t x t` |
| `sm` | `Vec<SparseMatrix>`, each `{ w_hat: Vec<F>, v_rest: Vec<F> }` | `R_P` entries (`w_hat` has `t`, `v_rest` has `t-1`) |
| `s` | `Strength` enum, `Standard` is variant 0 | `u32` = 0 |
| `rf` | `usize` | 8 |
| `rp` | `usize` | 59 wide, 56 narrow |
| `ht` | `HashType` enum, `Sponge` is variant 6 (`N:frontend/gadgets/poseidon/hash_type.rs:20-28`) | `u32` = 6 |

Wide: `t = 25`, 240088 bytes. Narrow: `t = 6`, 31712 bytes **[V]**. The wide block is 118096 (`mds`) + 8296 (`crc`)
+ 20208 (`psm`) + 93464 (`sm`) + 24. Only the wide block is used by `Engine::RO::new`
(`N:provider/poseidon.rs:71-77`, `mode = Wide`). The stored form is Neptune's optimized form (compressed constants,
sparse matrices). Section 5.2 gives the standard form that computes the same function **[V]**, so a verifier does not
need to interpret these bytes, only skip them.

### 3.3 `ppsnark::VerifierKey<E, IPA>` (`N:spartan/ppsnark.rs:799-808`)

| field | type | bytes |
|---|---|---|
| `num_cons` | `usize` | 8 |
| `num_vars` | `usize` | 8 |
| `vk_ee` | `ipa_pc::VerifierKey` = `{ ck_v: CommitmentKey, ck_s: CommitmentKey }` (`N:provider/ipa_pc.rs:25-31`) | `(8+32n+32)` + `(8+32+32)` |
| `S_comm` | `R1CSShapeSparkCommitment` (`N:spartan/ppsnark.rs:76-97`) | 232 |
| `digest` | `#[serde(skip)]` | 0 |

`CommitmentKey` (`N:provider/pedersen.rs:31-38`) is `{ ck: Vec<point>, h: point }`: `u64 len`, `len` points, then one
point. `ck_v` is the full public-parameter key (`len = n`, so `8 + 32n + 32` bytes); `ck_s` has `len = 1`
(`8 + 32 + 32 = 72` bytes). `R1CSShapeSparkCommitment` is `{ N: usize, comm_row, comm_col, comm_val_A, comm_val_B,
comm_val_C, comm_ts_row, comm_ts_col }` = `8 + 7*32`. Total `360 + 32n` **[V]**.

Meaning: `num_cons` and `num_vars` are those of the **padded** shape (`S.pad()`, `N:spartan/ppsnark.rs:1038-1043`), both
equal to one power of two `m`. `N` is the spark vector length,
`N = next_pow2(max(nnz(A)+nnz(B)+nnz(C), 2*num_vars, num_cons))` (`N:spartan/ppsnark.rs:117-121`), a power of two with
`N >= 2*num_vars`. The seven spark commitments commit to the circuit's matrices and are the only circuit-specific data
besides the dimensions.

### 3.4 `CompressedSNARK` (`N:nova/mod.rs:722-757`)

| # | field | type | bytes (`|X| = 2`) |
|---|---|---|---|
| 1 | `r_U_secondary` | `RelaxedR1CSInstance<Vesta>` | 168 |
| 2 | `ri_secondary` | `Fp` | 32 |
| 3 | `l_u_secondary` | `R1CSInstance<Vesta>` | 104 |
| 4 | `nifs_Uf_secondary` | `NIFS<Vesta>` (one point) | 32 |
| 5 | `l_ur_secondary` | `RelaxedR1CSInstance<Vesta>` | 168 |
| 6 | `nifs_Un_secondary` | `NIFSRelaxed<Vesta>` (one point) | 32 |
| 7 | `r_U_primary` | `RelaxedR1CSInstance<Pallas>` | 168 |
| 8 | `ri_primary` | `Fq` | 32 |
| 9 | `l_ur_primary` | `RelaxedR1CSInstance<Pallas>` | 168 |
| 10 | `nifs_Un_primary` | `NIFSRelaxed<Pallas>` (one point) | 32 |
| 11 | `wit_blind_r_Wn_primary` | `Fq` | 32 |
| 12 | `err_blind_r_Wn_primary` | `Fq` | 32 |
| 13 | `wit_blind_r_Wn_secondary` | `Fp` | 32 |
| 14 | `err_blind_r_Wn_secondary` | `Fp` | 32 |
| 15 | `snark_primary` | `ppsnark::RelaxedR1CSSNARK<Pallas, IPA>` (3.6) | `864 + 104*r_out + 168*k` |
| 16 | `snark_secondary` | `ppsnark::RelaxedR1CSSNARK<Vesta, IPA>` (3.6) | same formula, own `r_out`, `k` |
| 17 | `zn` | `Vec<Fq>` | `8 + 32*a` |
| 18 | `_p` | `PhantomData` | 0 |

Fields 1 to 14 total 1064 bytes when every `X` has two entries **[V]**. Total proof length
`1064 + (8 + 32a) + len(snark_primary) + len(snark_secondary)`; the probe proof is 11456 bytes (`a = 1`,
`r_out = 14`, `k = 17`, each SNARK 5176 bytes) **[V]**.

### 3.5 R1CS instances and NIFS messages (`N:r1cs/mod.rs:72-79`, `91-101`)

`RelaxedR1CSInstance<E>`: `comm_W` (point, 32), `comm_E` (point, 32), `X: Vec<E::Scalar>` (`8 + 32*|X|`),
`u: E::Scalar` (32).
`R1CSInstance<E>`: `comm_W` (point, 32), `X: Vec<E::Scalar>`.
`NIFS<E>` and `NIFSRelaxed<E>` are `{ comm_T: Commitment<E> }` (`N:nova/nifs.rs:17-21`, `114-116`): 32 bytes.

### 3.6 `ppsnark::RelaxedR1CSSNARK<E, IPA>` (`N:spartan/ppsnark.rs:815-882`)

| # | field | type | bytes |
|---|---|---|---|
| 1 | `comm_L_row` | point | 32 |
| 2 | `comm_L_col` | point | 32 |
| 3 | `comm_t_plus_r_inv_row` | point | 32 |
| 4 | `comm_w_plus_r_inv_row` | point | 32 |
| 5 | `comm_t_plus_r_inv_col` | point | 32 |
| 6 | `comm_w_plus_r_inv_col` | point | 32 |
| 7 | `sc_outer` | `SumcheckProof` | `8 + sum(8 + 32*len_i)` |
| 8 | `eval_Az_at_r_outer` | scalar | 32 |
| 9 | `eval_Bz_at_r_outer` | scalar | 32 |
| 10 | `eval_Cz_at_r_outer` | scalar | 32 |
| 11 | `eval_E_at_r_outer` | scalar | 32 |
| 12 | `sc_inner_batched` | `SumcheckProof` | as 7 |
| 13 | `eval_E` | scalar | 32 |
| 14 | `eval_L_row` | scalar | 32 |
| 15 | `eval_L_col` | scalar | 32 |
| 16 | `eval_val_A` | scalar | 32 |
| 17 | `eval_val_B` | scalar | 32 |
| 18 | `eval_val_C` | scalar | 32 |
| 19 | `eval_W` | scalar | 32 |
| 20 | `eval_t_plus_r_inv_row` | scalar | 32 |
| 21 | `eval_row` | scalar | 32 |
| 22 | `eval_w_plus_r_inv_row` | scalar | 32 |
| 23 | `eval_ts_row` | scalar | 32 |
| 24 | `eval_t_plus_r_inv_col` | scalar | 32 |
| 25 | `eval_col` | scalar | 32 |
| 26 | `eval_w_plus_r_inv_col` | scalar | 32 |
| 27 | `eval_ts_col` | scalar | 32 |
| 28 | `eval_arg` | `InnerProductArgument` | `(8 + 32k) + (8 + 32k) + 32` |

`SumcheckProof` (`N:spartan/sumcheck.rs:41-45`) is `{ compressed_polys: Vec<CompressedUniPoly> }`;
`CompressedUniPoly` (`N:spartan/polys/univariate.rs:25-33`) is `{ coeffs_except_linear_term: Vec<scalar> }`. A degree 3
round polynomial `c0 + c1 x + c2 x^2 + c3 x^3` is stored as `[c0, c2, c3]` (the linear term `c1` is dropped): 3 scalars,
104 bytes with its length prefix, and `8` bytes for the outer `Vec` header per proof. `InnerProductArgument`
(`N:provider/ipa_pc.rs:155-162`) is `{ L_vec: Vec<Commitment>, R_vec: Vec<Commitment>, a_hat: scalar }`. With
`r_out = log2(num_cons)` rounds in `sc_outer`, `r_in = log2(N) = k` rounds in `sc_inner_batched`, and
`|L_vec| = |R_vec| = k`, the SNARK is
`192 + (8 + 104 r_out) + 128 + (8 + 104 k) + 480 + 2(8 + 32k) + 32 = 864 + 104 r_out + 168 k` bytes **[V]**.

## 4. What in the verifier key is derivable

| vk part | status | how |
|---|---|---|
| `F_arity` | data | circuit arity, 8 bytes |
| `ro_consts_primary` / `_secondary` | **derivable**, no input | function of the field and the two widths only (4.2); 543,600 of the 8.9 MB |
| `pp_digest` | **data** | SHA3-256 of the full `PublicParams` (R1CS shapes, keys); cannot be rebuilt without the circuit |
| `num_cons`, `num_vars`, `S_comm.N` (per curve) | data | 8 bytes each |
| `S_comm` points (7 per curve) | data | commitments to the circuit's A/B/C matrices |
| `vk_ee.ck_v` (`n` points + `h`) | **derivable** given `n` | `from_label(b"ck", n + 1)`; `n` is a power of two and is data (4.1) |
| `vk_ee.ck_s` (1 point + `h`) | **derivable**, no input | `from_label(b"ipa", 2)`: `h = gens[0]`, `ck[0] = gens[1]` |
| `dk_primary`, `dk_secondary` | **derivable** | equals `ck_v.h`, the first point of the `ck` label of that curve |
| the two ppsnark vk digests | derivable from the bytes | section 5.4; or store the 32-byte results as data |

Irreducible data: `F_arity` (8) + `pp_digest` (32) + for each curve `num_cons`, `num_vars`, `N` (24) and the seven
`S_comm` points (224) = **536 bytes**, plus the two key lengths `n1`, `n2` (powers of two).

Size of the vk without the generator vectors (that is, with the `ck_v.ck` bodies removed and everything else kept):
**544,424 bytes** = 536 data + 543,600 Poseidon blocks + 288 fixed framing and fixed points (four `u64` length prefixes
= 32, the `h` of each `ck_v`, the `ck` and `h` of each `ck_s` = 192, the two `dk` = 64). The generator vectors add
`32*(n1+n2)`. With the derivable Poseidon blocks and framing also dropped, the vk is 536 bytes of data plus `n1`, `n2`.

### 4.1 Generators: `from_label(label, count)` **[V]**

Source: `N:provider/traits.rs:249-293` (macro body used for Pallas and Vesta, `N:provider/pasta.rs:33-47`) and the
Pedersen setup `N:provider/pedersen.rs:248-257`:

```
setup(label, n):  gens = from_label(label, n.next_power_of_two() + 1);  h = gens[0];  ck = gens[1..]
```

Labels in use: `b"ck"` for the public-parameter key of each curve (`N:r1cs/mod.rs:242`), with
`n = max(num_cons, num_vars, ck_floor)` of the **unpadded** augmented-circuit shape, `ck_floor = nnz(A)+nnz(B)+nnz(C)`
for ppsnark (`N:spartan/ppsnark.rs:1020-1025`, `N:r1cs/mod.rs:193-215`), so `ck_v.len = next_pow2(n)`; and `b"ipa"`
with `n = 1`, hence 2 points (`N:provider/ipa_pc.rs:53`).

`from_label(label, count)`:

1. `shake = SHAKE256(label)`. Read the XOF stream; point `i` (0-based) consumes bytes `32i .. 32i+32` as `u_i`
   (`N:provider/traits.rs:251-258`). The stream is read in order, so the first `c'` points of a longer sequence equal the
   `c'`-point sequence (prefix stable).
2. `P_i = hash_to_curve("from_uniform_bytes")(u_i)` for the curve (`traits.rs:259-265`), defined in
   `H:hash_to_curve.rs:33-159` with the Pasta suite `Suite<_, Sha256, 48>`: Pallas `b"pallas:SHA-256_SVDW_RO_"`,
   Vesta `b"vesta:SHA-256_SVDW_RO_"`, `Z = -13` in the base field (`H:pasta/pallas.rs:53-70`, `H:pasta/vesta.rs:55-72`),
   method SVDW.
3. `P_i` is normalized to affine (`traits.rs:267-292`; the batching has no effect on values).

**expand_message** (`H:hash_to_curve.rs:33-89`), `DST = "from_uniform_bytes" || suite string` (18 + 23 = 41 bytes),
`out_len = 96`, SHA-256 (block size 64, output 32):

```
b0 = SHA256( 0x00 * 64 || u_i || 0x00 0x60 0x00 || DST || len(DST) )          # len(DST) as one byte
b1 = SHA256( b0 || 0x01 || DST || len(DST) )
b2 = SHA256( (b0 xor b1) || 0x02 || DST || len(DST) )
b3 = SHA256( (b0 xor b2) || 0x03 || DST || len(DST) )
out = b1 || b2 || b3                                                             # 96 bytes
```

This is RFC 9380 `expand_message_xmd` with `DST' = DST || len(DST)`.

**hash_to_field** (`H:hash_to_curve.rs:135-153`): `u0 = int_BE(out[0..48]) mod B`, `u1 = int_BE(out[48..96]) mod B`,
`B` the base-field modulus of the curve (Pallas: `p`; Vesta: `q`). The code reverses each 48-byte chunk and calls
`from_uniform_bytes`, which computes `int_LE(bytes) mod B` (`D:field/mod.rs:575-607`).

**Map** (`H:hash_to_curve.rs:155-158`): `P = SVDW(u0) + SVDW(u1)` (group addition). Constants
(`H:hash_to_curve.rs:161-182`) with `A = 0`, `B = 5`, `Z = -13`:

```
c1 = Z^3 + 5                                    (= g(Z))
c2 = -Z / 2
c3 = sqrt(-c1 * 3 Z^2), the root with even parity (low bit of the canonical representation is 0)
c4 = -4 * c1 / (3 Z^2)
```

`SVDW(u)` (`H:hash_to_curve.rs:293-381`):

```
tv1 = u^2 * c1;  tv2 = 1 + tv1;  tv1 = 1 - tv1;  tv3 = inv0(tv1 * tv2)          # inv0(0) = 0
tv4 = u * tv1 * tv3 * c3
x1 = c2 - tv4;   gx1 = (x1^2 + A) * x1 + B;   e1 = is_square(gx1)
x2 = c2 + tv4;   gx2 = (x2^2 + A) * x2 + B;   e2 = is_square(gx2) and not e1
x3 = ((tv2^2 * tv3)^2) * c4 + Z
x  = x1 if e1, else x2 if e2, else x3
gx = (x^2 + A) * x + B;   y = sqrt(gx)                                           # any root
y  = y if parity(u) == parity(y) else -y                                         # parity = low bit of canonical repr
return (x, y)
```

`is_square(0)` is true. The root chosen by `sqrt` does not matter because `y` is then fixed by the parity rule.
The reference evaluates 8 exponentiation-class operations per generator (per `u`: one inversion, two
square tests, one square root; a variable-time implementation skips the second test when the first succeeds). For
`n = 2^17` that dominates the cost of re-deriving a key, so a deployed verifier pins the generator table (or its hash)
rather than regenerating it per proof.

### 4.2 Poseidon constants

`ROConstants::<E>::default()` (`N:provider/poseidon.rs:27-35`) builds both widths with `Strength::Standard` from the
field alone. The wide set (`t = 25`) is fully described by section 5.2: MDS, `R_F = 8`, `R_P = 59`, Grain round
constants. No vk or proof byte enters. The primary RO (`E1::RO`) uses the `Fp` instance; the secondary RO the `Fq`
instance.

### 4.3 Trust boundary of the stored constants

Nova's verifier uses the **deserialized** `ro_consts_*` bytes (`N:nova/mod.rs:948,960`; the deserializer recomputes only
`domain_tag` and `half_full_rounds`, `serde_impl.rs:107-120`). A vk produced by `CompressedSNARK::setup` always carries
the constants of 4.2, so a verifier that derives them is bit-identical to nova for every such vk. The program registry
pins the vk hash, so a tampered constant block fails the pin. A verifier must not accept an unpinned vk.

## 5. Primitives

### 5.1 Encodings that feed hashes

Everything absorbed into the Keccak transcript uses `TranscriptReprTrait`; everything absorbed into the Poseidon RO
uses `AbsorbInROTrait` (field elements, no bytes). They differ and must not be confused.

| object | transcript bytes (`N:`) | RO elements |
|---|---|---|
| scalar or base field element | 32 bytes canonical LE (`provider/traits.rs:322-333`) | the element |
| `Commitment` | `x(32) || y(32) || inf(1)`, `inf = 1` iff identity, from `to_coordinates` (`provider/pedersen.rs:103-118`) | three base elements `x, y, inf` with `inf` as 1 or 0 (`pedersen.rs:120-135`) |
| slice or `Vec<T>` | concatenation of the elements, **no length prefix** (`traits/mod.rs:237-244`) | n/a |
| `UniPoly` (sum-check round) | concatenation of `coeffs_except_linear_term` (`c0, c2, c3`), 96 bytes for a cubic (`spartan/polys/univariate.rs:177-205`) | n/a |
| `RelaxedR1CSInstance` | `comm_W || comm_E || u || X[0] || X[1] ...` (`r1cs/mod.rs:1278-1288`) | `comm_W` (3), `comm_E` (3), `u` (1, via `field_switch`), then each `X[i]` as 4 limbs (`r1cs/mod.rs:1290-1304`) |
| `R1CSInstance` | n/a | `comm_W` (3), then each `X[i]` as one element via `field_switch` (`r1cs/mod.rs:923-932`) |
| `InnerProductInstance` | `comm(65) || c(32)` (`provider/ipa_pc.rs:132-141`) | n/a |

The 4-limb form of a scalar `x` (`N:r1cs/mod.rs:1296-1302`, `gadgets/nonnative/bignat.rs:38-60`,
`gadgets/nonnative/util.rs:239-241`): take the canonical integer of `x`, split into four 64-bit limbs, least
significant first; each limb is a small integer absorbed as the base-field element with that value
(`BN_LIMB_WIDTH = 64`, `BN_N_LIMBS = 4`, `N:constants.rs:9-13`).

### 5.2 Poseidon random oracle (`E1::RO` over `Fp`, `E2::RO` over `Fq`)

Source: `N:provider/poseidon.rs:39-123` and the Neptune port under `N:frontend/gadgets/poseidon/`. **[V]** against
`PoseidonRO` for 1, 5, 20, 24, 25, 30 inputs on `Fp` and 2, 20, 26 inputs on `Fq`, with 250-bit and 128-bit outputs.

The RO is a one-shot hash. `absorb(e)` appends `e` to a list. `squeeze(num_bits, false)` hashes the **entire list** as
one sponge run with `n = list length`, then returns the low `num_bits` bits of the canonical integer of the output as a
field element (`poseidon.rs:93-121`, `start_with_one = false` everywhere in the verifier). The verifier squeezes each RO
exactly once (250 bits in step 6.4, 128 bits in each NIFS), so the post-squeeze state is irrelevant.

**Parameters** (field `F`; `Fp` or `Fq`, same numbers for both):

| item | value | source |
|---|---|---|
| width `t`, rate, capacity | 25, 24, 1 (arity `U24`) | `poseidon.rs:31`, `sponge/vanilla.rs:306-312` |
| S-box | `x^5` | `frontend/gadgets/poseidon/mod.rs:46,67-79` |
| full rounds `R_F`, partial rounds `R_P` | 8, 59 | `round_numbers.rs:23-71` (`calc_round_numbers(25, true)`) |
| MDS | `M[i][j] = (i + t + j)^-1` in `F`, `0 <= i,j < t` | `mds.rs:104-136` |
| round constants | Grain, below | `round_constants.rs:29-167` |

`(R_F, R_P)` come from a floating point search (`f32`, `N:.../round_numbers.rs:48-91`); hard-code (8, 59) (**[V]**; the
probe re-ran the search and gets (8, 59) for `t = 25` and (8, 56) for the unused `t = 6`).

**Round constants** (`generate_constants`, `round_constants.rs:29-82`): `(R_F + R_P)*t = 1675` field elements.

1. Build the 80-bit register `b[0..80)`, each field written most significant bit first and concatenated:
   field type `1` in 2 bits (`01`), S-box `1` in 4 bits (`0001`), `n = 255` (`F::NUM_BITS`) in 12 bits, `t = 25` in
   12 bits, `R_F = 8` in 10 bits, `R_P = 59` in 10 bits, then 30 one bits.
2. Step: `new = b[62] ^ b[51] ^ b[38] ^ b[23] ^ b[13] ^ b[0]`; drop `b[0]`, append `new` (`round_constants.rs:110-116`).
   Run 160 steps, discarding their outputs (`round_constants.rs:103-105`).
3. Output bit (self-shrinking, `round_constants.rs:155-166`): repeat { `b1 = step()`, `b2 = step()`; if `b1 == 1` return `b2` }.
4. One candidate is 255 output bits: the first 7 bits form byte 0 (most significant bit first), then 31 bytes of 8 bits
   each, most significant bit first (`round_constants.rs:135-152`, `255 mod 8 = 7`). Read the 32 bytes as a **big endian**
   integer; if it is `< |F|` accept it, otherwise discard the whole candidate and draw another (`round_constants.rs:59-75`).
   Constants are consumed in generation order: `rc[0], rc[1], ...`.

Round `r` (0-based) uses `rc[r*t .. (r+1)*t]`.

**Permutation** (standard form; equal to nova's optimized `hash_optimized_static`, `poseidon_inner.rs:316-343`, **[V]**):

```
for r in 0 .. R_F + R_P:
    s[i] += rc[r*t + i]                      for i in 0..t
    if r < R_F/2 or r >= R_F/2 + R_P:  s[i] = s[i]^5  for all i      # full round
    else:                              s[0] = s[0]^5                 # partial round
    s = M * s                                                         # s'[i] = sum_j M[i][j] * s[j]
```

`M` is symmetric, so row versus column convention does not matter.

**Sponge run** over the list `e[0..n)` (`sponge/api.rs:198-264`, `sponge/vanilla.rs:357-417`, no padding because
the hash type is `Sponge`: `poseidon_inner.rs:284-303`):

```
tag = ( x*(n + 2^31) + x*x ) mod 2^128,   x = 2^128 - 159,   all in u128 wrapping arithmetic
s[0] = tag (as a field element, integer < 2^128);  s[1..25] = 0
pos = 0
for e in list:   if pos == 24 { permute; pos = 0 };  s[1+pos] += e;  pos += 1
permute
output = s[1]
```

The tag encodes the I/O pattern `[Absorb(n), Squeeze(1)]` with domain separator 0
(`sponge/api.rs:28-102`: `update(n + 2^31)`, `update(1)`, `update(0)` with running powers of `x`). Examples (**[V]**
by recomputation): `n = 1` gives `0xffffffffffffffffffffffb080006222`; `n = 3` gives `0xffffffffffffffffffffffb0800060e4`;
`n = 20` gives `0xffffffffffffffffffffffb080005655`. Permutation count: `1 <= n <= 24` costs one permutation
(the squeeze permutation); `25 <= n <= 48` costs two.

**Truncation**: `bits = canonical integer of output`; result = integer formed by its low `num_bits` bits
(`poseidon.rs:102-111`). The 250-bit digests zero the top 5 bits of a 255-bit value: for example output byte 31 is masked
with `0x03` and byte 30 is untouched in the 250-bit case (byte 31 holds bits 248..255; only bits 248, 249 survive).

### 5.3 Keccak256 transcript (`E::TE`)

Source `N:provider/keccak.rs:11-200`. Hash function: **Keccak-256 as in the `sha3` crate `Keccak256`** (original Keccak
padding byte `0x01`), not SHA3-256 (`keccak.rs:9`). **[V]** against `Keccak256Transcript`.

State: `round: u64`, `state: [u8; 64]`, `buf: Vec<u8>` (bytes absorbed since the last squeeze). Define
`K2(m) = Keccak256(m || 0x00) || Keccak256(m || 0x01)` (64 bytes, `keccak.rs:68-96`).

| operation | effect |
|---|---|
| `new(label)` | `state = K2("NoTR" || label)`, `round = 0`, `buf = empty` (`keccak.rs:135-147`) |
| `absorb(label, obj)` | `buf += label || obj.to_transcript_bytes()` (`keccak.rs:154-160`) |
| `dom_sep(b)` | `buf += "NoDS" || b` (`keccak.rs:162-167`) |
| `squeeze(label)` | `m = buf || "NoDS" || round_le_u64 || state || label`; `out = K2(m)`; `state = out`; `round += 1`; `buf = empty`; result = `int_LE(out[0..64]) mod |scalar field of this engine|` (`keccak.rs:103-131,149-152`) |

The scalar is reduced from all 64 bytes (`from_uniform_bytes` for 64 bytes: `int_LE(b[0..32]) + 2^256 * int_LE(b[32..64])`
mod the modulus, `D:field/mod.rs:575-607`, `N:provider/pasta.rs`/`traits.rs:315-320`). The modulus is `q` for transcripts
of the primary SNARK (`E1::Scalar = Fq`) and `p` for the secondary SNARK. The verifier never calls `squeeze_bits`.

### 5.4 Digests (`N:digest.rs`)

`DigestComputer::digest()` (`digest.rs:75-87`): bytes = `bincode::serde::encode_into_writer(value, legacy().with_little_endian().with_fixed_int_encoding())`
(`digest.rs:25-33`); `h = SHA3-256(bytes)` (FIPS 202, **not** Keccak-256: `digest.rs:12,43`); the scalar is the low 250 bits
of `h` read little endian (`map_to_field`, `digest.rs:47-64`): bit `i` is `(h[i/8] >> (i%8)) & 1` for `0 <= i < 250`. As a
32-byte LE scalar: `h[0..31]` unchanged, `h[31] & 0x03`. **[V]** (the probe checks both vks against
`VerifierKey::digest()`).

The only digest the verifier computes is the ppsnark `VerifierKey` digest (`N:spartan/ppsnark.rs:1002-1014`,
`SimpleDigestible` at `1010`). Its input is the serialization of section 3.3 with `digest` skipped, which is exactly the
byte slice of `vk_primary` (resp. `vk_secondary`) inside the `nova::VerifierKey` encoding when that encoding is produced
with `legacy()`. The digest covers every generator of `ck_v`, so computing it costs a SHA3-256 pass over `360 + 32n`
bytes per curve (4.2 MB each in the example), or one stored 32-byte value per curve if the registry records it.
`pp_digest` follows the same rule over `PublicParams`, but is read from the vk.

## 6. `CompressedSNARK::verify` (`N:nova/mod.rs:915-1037`)

Inputs: pinned `vk`, `proof`, `num_steps` (`usize`), `z0` (`a` values in `Fq`). Output: `zn` or an error.

**6.0 Decode.** Decode `vk` and `proof` with the rules of section 2 and the layouts of section 3. Every scalar must be
canonical, every point must decode. Nova's decoder does not forbid trailing bytes; a consensus verifier must reject them.

**6.1** (`mod.rs:921-926`) If `num_steps == 0` fail (`ProofVerifyError`).

**6.2** (`mod.rs:928-932`) If `len(z0) != F_arity` or `len(zn) != F_arity` fail.

**6.3** (`mod.rs:934-944`) Fail unless all five have exactly two public values:
`l_u_secondary.X`, `r_U_primary.X`, `r_U_secondary.X`, `l_ur_primary.X`, `l_ur_secondary.X`.

**6.4 Hash binding** (`mod.rs:946-980`). Two Poseidon ROs.

`H1` = `E2::RO` over **Fq** with `ro_consts_secondary`. Absorb, in order (`mod.rs:948-957`):

| # | element | count |
|---|---|---|
| 1 | `pp_digest` (`Fq`) | 1 |
| 2 | `num_steps` as `Fq` (`num_steps as u64`) | 1 |
| 3 | `z0[0..a]` | `a` |
| 4 | `zn[0..a]` | `a` |
| 5 | `r_U_secondary.absorb_in_ro` : `comm_W` (x, y, inf as `Fq`), `comm_E` (3), `u` (`Fp -> Fq`), `X[0]` as 4 limbs, `X[1]` as 4 limbs | 15 |
| 6 | `ri_primary` (`Fq`) | 1 |

Total `18 + 2a` elements. `hash_primary = H1.squeeze(250, false)` (an `Fq` value `< 2^250`).

`H2` = `E1::RO` over **Fp** with `ro_consts_primary`. Absorb, in order (`mod.rs:960-966`):

| # | element | count |
|---|---|---|
| 1 | `pp_digest` as `Fp` (`scalar_as_base::<E1>`; it is `< 2^250 < p`, so the same integer) | 1 |
| 2 | `num_steps` as `Fp` (`E2::Scalar::from(num_steps as u64)`) | 1 |
| 3 | `0` | 1 |
| 4 | `0` | 1 |
| 5 | `r_U_primary.absorb_in_ro`: `comm_W` (x, y, inf as `Fp`), `comm_E` (3), `u` (`Fq -> Fp`, reduces mod `p`), `X[0]`, `X[1]` as 4 limbs each | 15 |
| 6 | `ri_secondary` (`Fp`) | 1 |

Total 20 elements. `hash_secondary = H2.squeeze(250, false)` (an `Fp` value).

Accept only if (`mod.rs:974-980`):

```
hash_primary   == l_u_secondary.X[0]  interpreted as an Fq integer   (Fp -> Fq, no reduction since p < q)
hash_secondary == l_u_secondary.X[1]                                  (Fp, direct)
```

**6.5 Fold the secondary side** (`mod.rs:982-996`; section 7 gives the NIFS). `pd = pp_digest` as `Fq`.
`Uf = NIFS.verify(ro_consts_secondary, pd, r_U_secondary, l_u_secondary, nifs_Uf_secondary.comm_T)`;
`Un_sec = NIFSRelaxed.verify(ro_consts_secondary, pd, Uf, l_ur_secondary, nifs_Un_secondary.comm_T)`.
The pp-digest passes through `scalar_as_base::<E1>` then `scalar_as_base::<E2>` (`Fq -> Fp -> Fq`): the same integer.

**6.6 Fold the primary side** (`mod.rs:998-1004`):
`Un_pri = NIFSRelaxed.verify(ro_consts_primary, pp_digest, r_U_primary, l_ur_primary, nifs_Un_primary.comm_T)`;
the digest enters that RO as `Fp` (`scalar_as_base::<E1>`).

**6.7 Derandomize** (`mod.rs:1006-1016`, `N:provider/pedersen.rs:307-315`, `N:r1cs/mod.rs:1263-1275`), with
`h_pri = dk_primary`, `h_sec = dk_secondary`:

```
U_pri.comm_W = Un_pri.comm_W - wit_blind_r_Wn_primary * h_pri
U_pri.comm_E = Un_pri.comm_E - err_blind_r_Wn_primary * h_pri
U_sec.comm_W = Un_sec.comm_W - wit_blind_r_Wn_secondary * h_sec
U_sec.comm_E = Un_sec.comm_E - err_blind_r_Wn_secondary * h_sec
```

`X` and `u` are unchanged.

**6.8 Verify the two SNARKs** (`mod.rs:1018-1034`): `S1.verify(vk_primary, U_pri, snark_primary)` and
`S2.verify(vk_secondary, U_sec, snark_secondary)` (sections 8 and 9). Nova runs them in parallel and returns the
primary error first if both fail (`res_primary?; res_secondary?`); the boolean outcome does not depend on that.

**6.9** Return `zn` from the proof (`mod.rs:1036`). `zn` is bound only through step 6.4.

What the verifier does **not** check: that `ri_*`, the blinds, or `l_ur_*` have any structure; the number of steps beyond
`!= 0`; `u` of the random instances. Soundness comes from the checks above.

## 7. NIFS verification

Three folds share the challenge rule: `r = RO.squeeze(128, false)`, then `r` is reinterpreted as a scalar of the fold's
curve (the same integer, `< 2^128`). A new RO (fresh list) is made for each fold (`N:nova/nifs.rs:88,178`).

| fold | RO | absorb list (in order) | r |
|---|---|---|---|
| `NIFS<Vesta>` for `Uf` (`nifs.rs:80-107`) | `E2::RO` over `Fq`, `ro_consts_secondary` | `pd` (1), `l_u_secondary.absorb_in_ro` = `comm_W` (3) + `X[0]`, `X[1]` as one `Fq` each via `Fp -> Fq` (2), `comm_T` (3) | `r1` |
| `NIFSRelaxed<Vesta>` for `Un_sec` (`nifs.rs:170-201`) | `E2::RO`, `ro_consts_secondary` | `pd` (1), `Uf.absorb_in_ro` (15), `l_ur_secondary.absorb_in_ro` (15), `comm_T` (3) | `r2` |
| `NIFSRelaxed<Pallas>` for `Un_pri` (`nifs.rs:170-201`) | `E1::RO` over `Fp`, `ro_consts_primary` | `pd` as `Fp` (1), `r_U_primary.absorb_in_ro` (15), `l_ur_primary.absorb_in_ro` (15), `comm_T` (3) | `r3` |

Folding rules (`N:r1cs/mod.rs:1205-1260`), `U1` the running instance, `U2` the incoming one:

```
R1CS fold (U2 is an R1CSInstance):
    X_i   = U1.X_i + r * U2.X_i
    comm_W = U1.comm_W + r * U2.comm_W
    comm_E = U1.comm_E + r * comm_T
    u      = U1.u + r
Relaxed fold (U2 is a RelaxedR1CSInstance):
    X_i   = U1.X_i + r * U2.X_i
    comm_W = U1.comm_W + r * U2.comm_W
    comm_E = U1.comm_E + r * comm_T + r^2 * U2.comm_E
    u      = U1.u + r * U2.u
```

All scalar arithmetic is in the curve's scalar field. The folded instances `Uf` and `Un_sec` are absorbed into the next RO in
affine form (`to_coordinates`), so `Uf` must be normalized before the second fold.

## 8. ppsnark `RelaxedR1CSSNARK::verify` (`N:spartan/ppsnark.rs:1388-1655`)

Notation: `M = vk.num_cons`, `V = vk.num_vars` (both powers of two), `N = vk.S_comm.N`, `r_out = log2(M)`,
`k = log2(N)`, `m = log2(V)`. `U` is the derandomized instance (`U.comm_W, U.comm_E, U.X = (X0, X1), U.u`).
`D` is the vk digest (5.4). `eq(a, b) = prod_i (a_i b_i + (1-a_i)(1-b_i))`. Transcript `T` is the Keccak transcript (5.3)
over the SNARK's own engine.

### 8.1 Transcript order

| # | action | line |
|---|---|---|
| 1 | `T = new(b"RelaxedR1CSSNARK")` | 1389 |
| 2 | `absorb(b"vk", D)` ; `absorb(b"U", U)` (bytes in the note below) | 1392-1393 |
| 3 | `tau_i = squeeze(b"t")`, `i = 0..r_out` | 1398-1400 |
| 4 | outer sum-check (8.2, 8.4), yields `r_outer` | 1406-1409 |
| 5 | `absorb(b"e", [Az, Bz, Cz, E])` (the four `eval_*_at_r_outer`) | 1422-1431 |
| 6 | `r_pad_j = squeeze(b"p")`, `j = 0..k - r_out` | 1438-1440 |
| 7 | `absorb(b"e", [comm_L_row, comm_L_col])` | 1451 |
| 8 | `c = squeeze(b"c")` ; `gamma = squeeze(b"g")` ; `r = squeeze(b"r")` | 1454-1457 |
| 9 | `absorb(b"l", [comm_t_plus_r_inv_row, comm_w_plus_r_inv_row, comm_t_plus_r_inv_col, comm_w_plus_r_inv_col])` | 1459-1468 |
| 10 | `rho_j = squeeze(b"r")`, `j = 0..k` | 1470-1472 |
| 11 | `s = squeeze(b"r")` ; `coeffs[i] = s^i`, `i = 0..9` | 1476-1477 |
| 12 | inner sum-check, per 8.2, `r_in` | 1490-1493 |
| 13 | `absorb(b"e", eval_vec)` (15 scalars, order in 8.6) | 1639 |
| 14 | `c2 = squeeze(b"c")` | 1640 |
| 15 | IPA (section 9) | 1645-1652 |

`absorb(b"U", U)` writes `comm_W(65) || comm_E(65) || u(32) || X0(32) || X1(32)` (5.1).

### 8.2 Sum-check verification (`N:spartan/sumcheck.rs:87-125`)

`verify(claim, num_rounds, degree_bound = 3, T)`:

```
e = claim ; r = []
require len(compressed_polys) == num_rounds                           else InvalidSumcheckProof
for i in 0 .. num_rounds:
    [c0, c2, c3, ...] = compressed_polys[i]                          # at least one entry required (an empty list panics nova)
    c1   = e - 2*c0 - c2 - c3 - ...                                   # decompress with hint e (univariate.rs:161-175)
    poly = c0 + c1 x + c2 x^2 + c3 x^3 (+ ...)
    require degree(poly) <= 3                                         # i.e. at most 3 entries stored
    T.absorb(b"p", poly)                                              # bytes: stored entries c0, c2, c3 (not c1), 32 bytes each
    r_i = T.squeeze(b"c")
    e = poly(r_i)                                                     # Horner
return (e, r)
```

The check `poly(0) + poly(1) = e` holds by construction of `c1`. Challenges are indexed in order: `r_0` is the first
challenge; `r_0` is the most significant variable of every multilinear table below (the "top" variable bound first).

### 8.3 Polynomial evaluations the verifier needs

All are over the SNARK's scalar field. Point coordinates are indexed from the most significant variable.

- `eq(a, b)` as above (`N:spartan/polys/eq.rs:36-41`). The table `evals(a)` has entry `i` equal to `eq(a, bits(i))` with
  `a_0` pairing with the most significant bit of `i` (`eq.rs:54-73`).
- `Identity_k(x) = sum_{i=0}^{k-1} 2^{k-1-i} x_i` (`N:spartan/polys/identity.rs:22-33`): the value `i` whose bits are `x`.
- `MaskedEq(a, m)(x)` for `a, x` of length `k` (`N:spartan/polys/masked_eq.rs:34-52`): with `s = k - m`,
  `lo = [0, s)`, `hi = [s, k)`,
  `MaskedEq = ( prod_{lo} (a_j x_j + (1-a_j)(1-x_j))  -  prod_{lo} (1-a_j)(1-x_j) ) * prod_{hi} (a_j x_j + (1-a_j)(1-x_j))`.
- `SparseEval(Z = [Z0, Z1, Z2], y)` for `y` of length `m >= 3` (`N:spartan/polys/multilinear.rs:195-226`, with `Z.len() = 3`,
  `num_vars_z = 2`): `chi = eq-table(y[m-3 .. m])` (8 entries, index MSB first),
  `SparseEval = prod_{i < m-3} (1 - y_i) * (Z0*chi[0] + Z1*chi[1] + Z2*chi[2])`.
- `powers(s, n) = [1, s, s^2, ..., s^{n-1}]` (`N:spartan/mod.rs:40-48`).
- `spartan/polys/power.rs` (`PowPolynomial`) is used only by the `neutron` module, not by ppsnark verification. The only
  other polynomial helpers on the verifier path are `UniPoly`/`CompressedUniPoly` (8.2).

### 8.4 Outer sum-check (steps 3 to 5)

Claim: `0 = sum_{x in {0,1}^{r_out}} eq(tau, x) * ( Az(x) Bz(x) - u Cz(x) - E(x) )`, three-input cubic.

```
(claim_final, r_outer) = verify(0, r_out, 3, T)
require  eq(tau, r_outer) * ( Az*Bz - U.u*Cz - E ) == claim_final       # else InvalidSumcheckProof   (ppsnark.rs:1412-1419)
```

with `Az, Bz, Cz, E` = `eval_Az_at_r_outer ... eval_E_at_r_outer` from the proof. Then step 5 absorbs the four values in the
order `Az, Bz, Cz, E`.

### 8.5 Padding, claims, final check (steps 6 to 12)

```
require k >= r_out                                                   # else InvalidSumcheckProof
r_outer_full = r_pad (k - r_out values, in squeeze order) || r_outer  # padding challenges are the top variables
factor       = prod_j (1 - r_pad_j)
c, gamma, r, rho[0..k), s, coeffs[0..9)                               # transcript order in 8.1
claim0 = coeffs[6] * factor * ( Az + c*Bz + c^2*Cz )  +  coeffs[7] * factor * E_at_r_outer
(claim_final2, r_in) = verify(claim0, k, 3, T)                        # r_in has k entries
```

The nine batched claims are, in coefficient order: 0 and 1 (row/column `t - w` zero-sums), 2 to 5 (the four LogUp
product checks), 6 (the A + cB + c^2 C inner claim), 7 (the E claim), 8 (the witness bound claim). Claims 0 to 5 and 8
start at 0, so only 6 and 7 enter `claim0`.

Expected final value (`ppsnark.rs:1496-1598`):

```
rho_eq = eq(rho, r_in)
eq_ro  = eq(r_outer_full, r_in)
masked = MaskedEq(r_outer_full, m)(r_in)                              # m = log2(V)
id     = Identity_k(r_in)

t_row = id + gamma * eq_ro + r
w_row = eval_row + gamma * eval_L_row + r

l      = k - (m + 1)                                                  # = log2(N) - log2(2V)
fc     = prod_{j < l} (1 - r_in_j)
ru     = r_in[l .. k]                                                 # length m + 1
eval_X = SparseEval( [U.u, U.X0, U.X1], ru[1 .. m+1] )
z_col  = eval_W + fc * ru[0] * eval_X
t_col  = id + gamma * z_col + r
w_col  = eval_col + gamma * eval_L_col + r

mem = coeffs[0] * ( eval_t_plus_r_inv_row - eval_w_plus_r_inv_row )
    + coeffs[1] * ( eval_t_plus_r_inv_col - eval_w_plus_r_inv_col )
    + coeffs[2] * rho_eq * ( eval_t_plus_r_inv_row * t_row - eval_ts_row )
    + coeffs[3] * rho_eq * ( eval_w_plus_r_inv_row * w_row - 1 )
    + coeffs[4] * rho_eq * ( eval_t_plus_r_inv_col * t_col - eval_ts_col )
    + coeffs[5] * rho_eq * ( eval_w_plus_r_inv_col * w_col - 1 )
abc = coeffs[6] * eval_L_row * eval_L_col * ( eval_val_A + c*eval_val_B + c^2*eval_val_C )
ee  = coeffs[7] * eq_ro * eval_E
wb  = coeffs[8] * masked * eval_W
require mem + abc + ee + wb == claim_final2                           # else InvalidSumcheckProof
```

The `U.X` vector is `(X0, X1)` of the derandomized instance, and the public vector fed to the column memory is
`(u, X0, X1)` in that order (`ppsnark.rs:1540-1549`). Requirements for this to be defined: `V >= 8` (`m >= 3`) and
`N >= 2V` (`l >= 0`), both always true for a real Nova circuit; see section 11 for what nova does otherwise.

### 8.6 Opening claims and handoff to the IPA (steps 13 to 15)

`eval_vec` (15 scalars) and `comm_vec` (15 points), index by index (`ppsnark.rs:1605-1638`):

| i | eval | commitment |
|---|---|---|
| 0 | `eval_W` | `U.comm_W` |
| 1 | `eval_E` | `U.comm_E` |
| 2 | `eval_L_row` | `comm_L_row` |
| 3 | `eval_L_col` | `comm_L_col` |
| 4 | `eval_val_A` | `vk.S_comm.comm_val_A` |
| 5 | `eval_val_B` | `vk.S_comm.comm_val_B` |
| 6 | `eval_val_C` | `vk.S_comm.comm_val_C` |
| 7 | `eval_t_plus_r_inv_row` | `comm_t_plus_r_inv_row` |
| 8 | `eval_row` | `vk.S_comm.comm_row` |
| 9 | `eval_w_plus_r_inv_row` | `comm_w_plus_r_inv_row` |
| 10 | `eval_ts_row` | `vk.S_comm.comm_ts_row` |
| 11 | `eval_t_plus_r_inv_col` | `comm_t_plus_r_inv_col` |
| 12 | `eval_col` | `vk.S_comm.comm_col` |
| 13 | `eval_w_plus_r_inv_col` | `comm_w_plus_r_inv_col` |
| 14 | `eval_ts_col` | `vk.S_comm.comm_ts_col` |

The order of `eval_vec` is **not** the order of the proof fields: in the encoding (3.6) `eval_W` is field 19 and comes after
`eval_val_C`, while here it is index 0, and `eval_E` is field 13 but index 1. Map by name.

After `T.absorb(b"e", eval_vec)` and `c2 = T.squeeze(b"c")` (`PolyEvalInstance::batch`, `N:spartan/mod.rs:348-369`):

```
C_b = sum_{i=0}^{14} c2^i * comm_vec[i]          # a 15-term MSM, variable bases
e_b = sum_{i=0}^{14} c2^i * eval_vec[i]
```

and `EE::verify(vk.vk_ee, T, C_b, point = r_in, eval = e_b, proof.eval_arg)`.

## 9. IPA `EvaluationEngine::verify` (`N:provider/ipa_pc.rs:80-99`, `286-389`)

Inputs: `ck_v = vk_ee.ck_v`, `G_ipa = vk_ee.ck_s.ck[0]`, `C = C_b`, `x = r_in` (`k` entries), `e = e_b`,
argument `{ L[0..k), R[0..k), a_hat }`. `n = 2^k = N`.

**Length checks** (`ipa_pc.rs:297-303`): `|L| == |R|`, `n == 2^|L|`, `|L| < 32`; else `InvalidInputLength`. Also
`len(ck_v) >= n`, because nova slices `ck_v[0..n]` (`ipa_pc.rs:294`, `pedersen.rs:461-472`) and panics otherwise.

**Transcript** (`ipa_pc.rs:296-321`):

```
T.dom_sep(b"IPA")
T.absorb(b"U", C || e)                      # Commitment bytes (65) then the scalar (32), 5.1
gamma = T.squeeze(b"r")
for j in 0 .. k:
    T.absorb(b"L", L[j]); T.absorb(b"R", R[j])
    r_j = T.squeeze(b"r")
```

**Derived values** (`ipa_pc.rs:309-376`):

```
Gc    = gamma * G_ipa                         # ck_c.scale(gamma); ck_s.h is never used
P     = C + e * Gc
require every r_j != 0 (batch_invert fails otherwise); r_j^{-1} the inverse
s[i]  = prod_{j=0}^{k-1}  ( r_j if bit (k-1-j) of i is 1 else r_j^{-1} )         # i in 0 .. n; r_0 is the top bit
                                                                                   # (recurrence: s[0] = prod r_j^{-1};
                                                                                   #  s[i] = s[i - 2^p] * r_{k-1-p}^2, p = floor(log2 i))
b_vec = eq-table(x)                           # n entries
b_hat = sum_i b_vec[i] * s[i]
      = prod_{j=0}^{k-1} ( (1 - x_j) * r_j^{-1} + x_j * r_j )                     # closed form of the same sum
```

**Accept** iff (`ipa_pc.rs:358-388`)

```
sum_j r_j^2 * L[j]  +  sum_j r_j^{-2} * R[j]  +  P   ==   a_hat * ( sum_{i<n} s[i] * ck_v[i] )  +  (a_hat * b_hat) * Gc
```

Equality is of group elements (projective equality), not of encodings. `ck_v.h`, `ck_s.h` are multiplied by a zero blind and
play no role. Rearranged to one multi-scalar equation, the check is

```
sum_j r_j^2 L[j] + sum_j r_j^{-2} R[j] + C + ( gamma*(e - a_hat*b_hat) ) * G_ipa - a_hat * sum_i s[i] ck_v[i]  ==  identity
```

with `C = C_b` itself a 15-term sum (8.6).

## 10. Verifier-side multi-scalar work

Let `k_c = log2(N_c)` for curve `c` (Pallas uses the primary vk, Vesta the secondary; each has its own `N`).

| # | where | equation | terms | bases | scalar size |
|---|---|---|---|---|---|
| 1 | 6.5 `Uf.comm_W` | `U.W + r1*u.W` | 2 | variable | 128-bit |
| 2 | 6.5 `Uf.comm_E` | `U.E + r1*T1` | 2 | variable | 128-bit |
| 3 | 6.5 `Un_sec.comm_W` | `Uf.W + r2*lr.W` | 2 | variable | 128-bit |
| 4 | 6.5 `Un_sec.comm_E` | `Uf.E + r2*T2 + r2^2*lr.E` | 3 | variable | 128/255-bit |
| 5 | 6.6 `Un_pri.comm_W` | `U.W + r3*lr.W` | 2 | variable | 128-bit |
| 6 | 6.6 `Un_pri.comm_E` | `U.E + r3*T3 + r3^2*lr.E` | 3 | variable | 128/255-bit |
| 7 | 6.7 (4 derandomizations) | `Un.W - bw*h`, `Un.E - be*h`, both curves | 2 each | `h` fixed | 255-bit |
| 8 | 8.6, per curve | `C_b = sum c2^i C_i` | 15 | variable | 255-bit |
| 9 | 9, per curve | the IPA equation | `N_c + 2*k_c + 2` | `N_c` fixed (`ck_v[0..N]`), `2k_c + 2` variable | 255-bit |

Rows 1 to 7 are 24 variable-base terms in total and need affine normalization of `Uf` and of each final commitment
(they are absorbed into an RO or the transcript). Row 9 expands: if row 8 is merged in, the IPA equation has
`N_c + 2*k_c + 16` terms. Nova itself performs, per curve, a `commit` of size `N` (`ck_hat`), one of size `2k+1`
(`P_hat`), one of size 2, one of size 1 (`ck_c`) and `scale` of one base (`ipa_pc.rs:310-383`); the merged form above is
algebraically identical.

Dependence on the circuit: `N = next_pow2(max(nnz_total, 2*num_vars, num_cons))` with `num_vars = num_cons = V` after
padding, `N >= 2V`. The dominant MSM has `N` fixed bases per curve; over both curves the fixed-base term count is `N_pri + N_sec`.
Example (probe circuit, `V = 16384`, `nnz`-dominated): `N = 131072 = 2^17`, `k = 17` on both curves, so each curve
needs a `131072 + 34 + 16 = 131122`-term MSM, and the two generator tables hold `2 * 131072` points.

Field work outside the MSMs, per curve: the vector `s` (`n - 1` multiplications by the recurrence), or none if the
`ck_hat` MSM is computed from the product structure; `b_hat` is `O(k)` through the closed form, `O(n)` through the table.
Hash work: SHA3-256 over `360 + 32n` bytes per curve for the vk digest (unless stored), one Poseidon permutation count of
`ceil((18+2a)/24)` for `H1`, one for `H2`, and the NIFS ROs of 1, 2 and 2 permutations (9, 34 and 34 inputs).

## 11. Errors and hostile-input hazards

Nova returns `Err` for these (`N:errors.rs`) and the verifier must reject:

| condition | nova result |
|---|---|
| `num_steps == 0` | `ProofVerifyError` (`mod.rs:921`) |
| `len(z0)` or `len(zn)` `!= F_arity` | `ProofVerifyError` (`mod.rs:928`) |
| any of the five instances has `|X| != 2` | `ProofVerifyError` (`mod.rs:934`) |
| hash binding mismatch (6.4) | `ProofVerifyError` (`mod.rs:974`) |
| sum-check: wrong number of rounds, degree above 3, or a failed final claim | `InvalidSumcheckProof` |
| `k < r_out` | `InvalidSumcheckProof` (`ppsnark.rs:1435`) |
| IPA length checks | `InvalidInputLength` |
| a zero IPA challenge | `InternalError` from `batch_invert` (`spartan/mod.rs:130`) |
| IPA equation fails | `InvalidPCS` |

Conditions where nova **panics** on a malformed vk or proof (a consensus verifier must turn each into a rejection):

| condition | where |
|---|---|
| a round polynomial with an empty coefficient list | `univariate.rs:161-175` (`[0]` index) |
| `num_cons == 0` or `N == 0` | `Math::log_2` asserts non-zero (`spartan/math.rs:12-22`) |
| `num_vars` or `N` not a power of two | `log_2` rounds up; the later slicing and `l` arithmetic no longer match the prover |
| `m < 3`, i.e. `V < 8` | `SparsePolynomial::evaluate` slice underflow (`multilinear.rs:212`) |
| `N < 2V` (so `l < 0`) | `ppsnark.rs:1528` underflow |
| `m > k` | `masked_eq.rs:37` underflow |
| `len(ck_v) < N` | `pedersen.rs:461-472` slice |
| `|L| >= 32` | rejected by the check above, but `2^|L|` must not be computed before it |

Encoding hazards: a point with `x = 0` decodes to the identity for either sign bit (2.3); a scalar `>=` modulus fails to
decode; `usize` fields (`F_arity`, `num_cons`, `num_vars`, `N`) are 64-bit and need bounds before use as sizes. Vec length
prefixes in hostile input must be bounded by the remaining bytes before allocating.

## 12. Test vectors (from the probe, nova-snark 0.76.0)

All field elements and coordinates are 32-byte little endian hex, as `to_repr()`.

**Generators (4.1).** `gens[i]` of `from_label(label, count)`; for a commitment key `h = gens[0]`, `ck[0] = gens[1]`.

| curve, label | i | x | y |
|---|---|---|---|
| Pallas `ck` | 0 | `831ce30f60b003fb6310bb0915cbf344ad21e21fb4c9a8d14fd57b3cbd880a07` | `2ff95c0a8da8d804b9d1a24807271cf78f5f40d519aba0274ee07b327134a63a` |
| Pallas `ck` | 1 | `f025e536fd8fc9216f3d1c860a1b2fc99cc91e5b9722448d125e7ee0fbea3f19` | `4cbaf847e93fc40bd49a72f5211a3f2f6aca47d26c879a8d97450dd4aac4a138` |
| Pallas `ipa` | 0 | `f444c6bcff14f0070ceecd76806ae1a90fa96e7d1ab3d2ebdf9ba49c05db141a` | `cef4d78e8401787616940538cbd64390fd48e8692977a337e5938afffb7f311e` |
| Pallas `ipa` | 1 | `cceb5c1bec59e543a0cf17b14aff3edb2eabeda58f825b85a0c6121e6e64c303` | `c92e6547940cbee6d796ea0557ea358a364fcaf292459241ebbdd2ed41e3f92f` |
| Vesta `ck` | 0 | `3a374c57bd1aa8911930bda8645c6a497cf6715739962fc9da87c6947ca20015` | `bf7e10a16c2054a6d3135105a0c69f198440a631b9530c1a14d19178728b923f` |
| Vesta `ck` | 1 | `7354749f878c31b5a5ead1e5e287818109caaf82efee284df2e62e7e89b1330e` | `004c4aeb6e13ae9890910e6adb9ec00eebab5319f1d5d8f9c244612d20bbb325` |

Compressed (wire) form of Pallas `ipa` `gens[1]`: `cceb5c1bec59e543a0cf17b14aff3edb2eabeda58f825b85a0c6121e6e64c383`
(`y` is odd, so bit 7 of byte 31 is set: `0x03 | 0x80`). The probe computed the first 6 Pallas `ck`, 2 Pallas `ipa` and
4 Vesta `ck` generators with an independent implementation of 4.1 and they matched `from_label` exactly.

**Poseidon (5.2).** `Fp` instance, inputs `1, 2, ..., n` as field elements; "full" is the whole output, "250" and "128" the truncations:

| n | full | 250 bits | 128 bits |
|---|---|---|---|
| 1 | `8646c503de4e04ce9398c8abfddca7f4d3a8272b174701950d435e24ceeaef06` | `...ef02` (byte 31 `& 03`) | `8646c503de4e04ce9398c8abfddca7f4` then 16 zero bytes |
| 3 | `a388b83484bd7fadbb79744dccf39fa1c46fdf17d79dd0dfb84ffe8710bb9515` | `...9501` | `a388b83484bd7fadbb79744dccf39fa1` then 16 zero bytes |
| 24 | `3e0a758da0b85bbad7373be5ce503a5a641f18e83c93473521185ac3fbb2b024` | `...b000` | `3e0a758da0b85bbad7373be5ce503a5a` then 16 zero bytes |
| 25 | `a8c2d9efd259f137525861ce235a8e2441872974e6f8439be8ba1f191accdb17` | `...db03` | `a8c2d9efd259f137525861ce235a8e24` then 16 zero bytes |
| 30 | `3386677c67063105fc4063d912ff7b7eb4df7a33aea262e7a71f59f89accd106` | `...d102` | `3386677c67063105fc4063d912ff7b7e` then 16 zero bytes |

`Fq` instance, inputs `1..n`: `n = 1` full `f560d2df916de04e6f3d6308714f497a7f7925e2dbb75c51a89bd253c57b8801`
(250-bit identical, top byte already below `0x04`); `n = 3` full `a14307c30af52c5d8606f0afd98e73e06d477020cee9fd4463d6d8e2283a8413`,
250-bit `...8403`.

Round constants (little endian bytes of the field element): `Fp` `rc[0] = b1b6ce6d058bf5a049fd50f4efe22a7e91d72a3907b3e1a5ed86b40f04df212c`,
`rc[1] = 5b1ef852814565ff02b2c2fcaf022f71b75075c50743825f27651ec43009dd06`,
`rc[1674] = 75b85282fe0b5c602124b1dbd73e96529b69a12a15d65e4d026ed0ba4187a604`. `Fq` has the same `rc[0]` and `rc[1674]`.

**Field switch (1).** `q - 1` read as an `Fp` element is `ffffffff33ba19f3c1af47000000000000000000000000000000000000000000`.

**Keccak transcript (5.3), scalar field `Fq`.** `T = new(b"RelaxedR1CSSNARK")`; `absorb(b"vk", 123456789)`;
`squeeze(b"t")` returns `2330b36c9bdcfda1042e29d1267c331897ea8aa65d8fc852dc2a72a5e7dbef3e`; then `dom_sep(b"IPA")`;
`absorb(b"e", 5)`; `squeeze(b"c")` returns `543ab5dd9d065a4d9100272f2010381039f093850367aae0bcf79ea5103c0b35`.

**Probe proof (CubicCircuit `y = x^3 + x + 5`, arity 1, `z0 = 0`, 3 steps, `ck_floor` hints).** `zn = 2460515`
(`638b2500..00`). Verification with nova succeeds, and the vk/proof round-trip through `legacy()` bincode.

| quantity | value |
|---|---|
| vk bytes | 8,933,032 (`544424 + 32*(131072 + 131072)`) |
| proof bytes | 11,456 |
| `num_cons = num_vars` (both curves) | 16384 |
| `N`, `ck_v.len` (both curves) | 131072 (`k = 17`, `r_out = 14`) |
| `pp_digest` (LE bytes) | `842c97cd24db7848ca338b4191df6cf3134c9f48ac57784f0ee13c191bbfce03` |
| `vk_primary` digest (LE bytes) | `cda5f2516406f71c287f23cdc94b473e7a0c2f54ac339f926d3f0162444f3e02` (SHA3-256 `...3e5e`, masked) |
| `vk_secondary` digest (LE bytes) | `445f7ef18ca1376592ec0a4d5904663caba421ac782275e8a8b3c9ab6b118d01` (SHA3-256 `...8d7d`, masked) |

The vk is a deterministic function of the circuit, so the two vk digests and `pp_digest` are reproducible by anyone
holding the same circuit and nova version; the proof is randomized (blinds and random instances).

## 13. Findings that affect a zero-dependency implementation

1. **The vk is 8.9 MB for even a trivial step circuit.** The augmented Nova circuit is large (`num_cons = 2^14`), the
   commitment key covers `max(num_cons, nnz total)` = `2^17` generators per curve, and the ppsnark vk embeds the whole
   key. Only 536 bytes of it are circuit data; the rest is derivable (section 4). The ppsnark vk digest is over all
   `n` generators, so the digest is either computed over the full table or stored as 32 pinned bytes.
2. **One MSM of `N + 2k + 2` fixed-and-variable terms per curve** (`N = 2^17` here) dominates verification; the bases
   are the first `N` generators of the `ck` label. They are fixed per vk, which allows precomputed tables.
3. **Generator derivation is expensive** (8 exponentiation-class operations per point, 131072 points per curve) but is
   fully specified: SHAKE-256, SHA-256 `expand_message_xmd`, SVDW. It needs no external crate beyond SHA-256, SHA-3
   (SHAKE-256, Keccak-256, SHA3-256) and big-field arithmetic the project already writes.
4. **The Poseidon constants in the vk are a derived artifact** (543,600 bytes): skip them. The standard permutation of 5.2
   is equal to the stored optimized form.
5. **Three different hashes appear**: Keccak-256 (transcript), SHA3-256 (digests), SHA-256 and SHAKE-256 (generators).
6. **Parsing hazards** listed in section 11 are real: nova panics on several malformed shapes; a consensus verifier must
   return errors instead, and must bound every length prefix by the remaining input.
7. **Non-canonical identity encoding**: `x = 0` with the sign bit set decodes to the identity in nova (2.3).
8. **`field_switch` reduces** (`Fq -> Fp`) for `u` of the primary instance in `H2`; the value is an arbitrary 255-bit
   scalar, so the reduction `mod p` is part of the specification (6.4).
